//! VectorStore: Vector storage and search primitive
//!
//! ## Design
//!
//! VectorStore is a stateless facade over the Database engine for collection
//! management. It holds:
//! - `Arc<Database>` for storage operations
//! - `RwLock<BTreeMap<CollectionId, Box<dyn VectorIndexBackend>>>` for in-memory index
//!
//! ## Run Isolation
//!
//! All operations are scoped to a `RunId`. Different runs cannot see
//! each other's collections or vectors.
//!
//! ## Thread Safety
//!
//! VectorStore is `Send + Sync` and can be safely shared across threads.

use crate::vector::collection::{validate_collection_name, validate_vector_key};
use crate::vector::{
    CollectionId, CollectionInfo, CollectionRecord, IndexBackendFactory, MetadataFilter,
    VectorConfig, VectorEntry, VectorError, VectorId, VectorIndexBackend, VectorMatch,
    VectorRecord, VectorResult,
};
use in_mem_core::search_types::{DocRef, SearchHit, SearchResponse, SearchStats};
use in_mem_core::types::{Key, Namespace, RunId};
use in_mem_core::value::Value;
use in_mem_engine::Database;
use serde_json::Value as JsonValue;
use std::collections::BTreeMap;
use std::sync::{Arc, RwLock};

/// Vector storage and search primitive
///
/// Manages collections of vectors with similarity search capabilities.
/// Uses BTreeMap for deterministic iteration order (Invariant R3).
///
/// # Example
///
/// ```ignore
/// use in_mem_primitives::VectorStore;
/// use in_mem_engine::Database;
/// use in_mem_core::types::RunId;
///
/// let db = Arc::new(Database::open("/path/to/data")?);
/// let store = VectorStore::new(db);
/// let run_id = RunId::new();
///
/// // Create collection
/// let config = VectorConfig::for_minilm();
/// store.create_collection(run_id, "embeddings", config)?;
///
/// // List collections
/// let collections = store.list_collections(run_id)?;
/// ```
#[derive(Clone)]
pub struct VectorStore {
    db: Arc<Database>,
    /// In-memory index backends per collection
    /// CRITICAL: BTreeMap for deterministic iteration (Invariant R3)
    backends: Arc<RwLock<BTreeMap<CollectionId, Box<dyn VectorIndexBackend>>>>,
    /// Factory for creating index backends
    backend_factory: IndexBackendFactory,
}

impl VectorStore {
    /// Create a new VectorStore
    pub fn new(db: Arc<Database>) -> Self {
        Self {
            db,
            backends: Arc::new(RwLock::new(BTreeMap::new())),
            backend_factory: IndexBackendFactory::default(),
        }
    }

    /// Create a new VectorStore with custom backend factory
    pub fn with_backend_factory(db: Arc<Database>, factory: IndexBackendFactory) -> Self {
        Self {
            db,
            backends: Arc::new(RwLock::new(BTreeMap::new())),
            backend_factory: factory,
        }
    }

    /// Get the underlying database reference
    pub fn database(&self) -> &Arc<Database> {
        &self.db
    }

    // ========================================================================
    // Collection Management (Epic 53)
    // ========================================================================

    /// Create a new collection
    ///
    /// Creates a collection with the specified configuration.
    /// The configuration (dimension, metric, dtype) is immutable after creation.
    ///
    /// # Errors
    /// - `CollectionAlreadyExists` if a collection with this name exists
    /// - `InvalidCollectionName` if name is invalid
    /// - `InvalidDimension` if dimension is 0
    pub fn create_collection(
        &self,
        run_id: RunId,
        name: &str,
        config: VectorConfig,
    ) -> VectorResult<CollectionInfo> {
        // Validate name
        validate_collection_name(name)?;

        // Validate config (dimension must be > 0)
        if config.dimension == 0 {
            return Err(VectorError::InvalidDimension {
                dimension: config.dimension,
            });
        }

        let collection_id = CollectionId::new(run_id, name);

        // Check if collection already exists
        if self.collection_exists(run_id, name)? {
            return Err(VectorError::CollectionAlreadyExists {
                name: name.to_string(),
            });
        }

        let now = now_micros();

        // Create collection record
        let record = CollectionRecord::new(&config);

        // Store config in KV
        let config_key = Key::new_vector_config(Namespace::for_run(run_id), name);
        let config_bytes = record.to_bytes()?;

        // Use transaction for atomic storage
        self.db
            .transaction(run_id, |txn| {
                txn.put(config_key.clone(), Value::Bytes(config_bytes.clone()))
            })
            .map_err(|e| VectorError::Storage(e.to_string()))?;

        // Initialize in-memory backend
        self.init_backend(&collection_id, &config);

        Ok(CollectionInfo {
            name: name.to_string(),
            config,
            count: 0,
            created_at: now,
        })
    }

    /// Delete a collection and all its vectors
    ///
    /// This is a destructive operation that:
    /// 1. Deletes all vectors in the collection
    /// 2. Deletes the collection configuration
    /// 3. Removes the in-memory backend
    ///
    /// # Errors
    /// - `CollectionNotFound` if collection doesn't exist
    pub fn delete_collection(&self, run_id: RunId, name: &str) -> VectorResult<()> {
        let collection_id = CollectionId::new(run_id, name);

        // Check if collection exists
        if !self.collection_exists(run_id, name)? {
            return Err(VectorError::CollectionNotFound {
                name: name.to_string(),
            });
        }

        // Delete all vectors in the collection
        self.delete_all_vectors(run_id, name)?;

        // Delete config from KV
        let config_key = Key::new_vector_config(Namespace::for_run(run_id), name);
        self.db
            .transaction(run_id, |txn| txn.delete(config_key.clone()))
            .map_err(|e| VectorError::Storage(e.to_string()))?;

        // Remove in-memory backend
        self.backends.write().unwrap().remove(&collection_id);

        Ok(())
    }

    /// List all collections for a run
    ///
    /// Returns CollectionInfo for each collection, including current vector count.
    /// Results are sorted by name for determinism (Invariant R4).
    pub fn list_collections(&self, run_id: RunId) -> VectorResult<Vec<CollectionInfo>> {
        use in_mem_core::traits::SnapshotView;

        let namespace = Namespace::for_run(run_id);
        let prefix = Key::new_vector_config_prefix(namespace);

        // Read from snapshot for consistency
        let snapshot = self.db.storage().create_snapshot();
        let entries = snapshot
            .scan_prefix(&prefix)
            .map_err(|e| VectorError::Storage(e.to_string()))?;

        let mut collections = Vec::new();

        for (key, versioned_value) in entries {
            // Extract collection name from key
            let name = String::from_utf8(key.user_key.clone())
                .map_err(|e| VectorError::Serialization(e.to_string()))?;

            // Deserialize the record from the stored bytes
            let bytes = match &versioned_value.value {
                Value::Bytes(b) => b.clone(),
                _ => {
                    return Err(VectorError::Serialization(
                        "Expected Bytes value for collection record".to_string(),
                    ))
                }
            };
            let record = CollectionRecord::from_bytes(&bytes)?;
            let config = VectorConfig::try_from(record.config)?;

            // Get current count from backend
            let collection_id = CollectionId::new(run_id, &name);
            let count = self.get_collection_count(&collection_id, run_id, &name)?;

            collections.push(CollectionInfo {
                name,
                config,
                count,
                created_at: record.created_at,
            });
        }

        // Sort by name for determinism
        collections.sort_by(|a, b| a.name.cmp(&b.name));

        Ok(collections)
    }

    /// Get a single collection's info
    ///
    /// Returns None if collection doesn't exist.
    pub fn get_collection(
        &self,
        run_id: RunId,
        name: &str,
    ) -> VectorResult<Option<CollectionInfo>> {
        let config_key = Key::new_vector_config(Namespace::for_run(run_id), name);

        // Read from snapshot
        use in_mem_core::traits::SnapshotView;
        let snapshot = self.db.storage().create_snapshot();

        let Some(versioned_value) = snapshot
            .get(&config_key)
            .map_err(|e| VectorError::Storage(e.to_string()))?
        else {
            return Ok(None);
        };

        // Deserialize the record
        let bytes = match &versioned_value.value {
            Value::Bytes(b) => b.clone(),
            _ => {
                return Err(VectorError::Serialization(
                    "Expected Bytes value for collection record".to_string(),
                ))
            }
        };
        let record = CollectionRecord::from_bytes(&bytes)?;
        let config = VectorConfig::try_from(record.config)?;

        let collection_id = CollectionId::new(run_id, name);
        let count = self.get_collection_count(&collection_id, run_id, name)?;

        Ok(Some(CollectionInfo {
            name: name.to_string(),
            config,
            count,
            created_at: record.created_at,
        }))
    }

    /// Check if a collection exists
    pub fn collection_exists(&self, run_id: RunId, name: &str) -> VectorResult<bool> {
        use in_mem_core::traits::SnapshotView;

        let config_key = Key::new_vector_config(Namespace::for_run(run_id), name);
        let snapshot = self.db.storage().create_snapshot();

        Ok(snapshot
            .get(&config_key)
            .map_err(|e| VectorError::Storage(e.to_string()))?
            .is_some())
    }

    // ========================================================================
    // Vector Operations (Epic 54)
    // ========================================================================

    /// Insert a vector (upsert semantics)
    ///
    /// If a vector with this key already exists, it is overwritten.
    /// This follows Rule 3 (Upsert Semantics).
    ///
    /// # Errors
    /// - `CollectionNotFound` if collection doesn't exist
    /// - `InvalidKey` if key is invalid
    /// - `DimensionMismatch` if embedding dimension doesn't match config
    pub fn insert(
        &self,
        run_id: RunId,
        collection: &str,
        key: &str,
        embedding: &[f32],
        metadata: Option<JsonValue>,
    ) -> VectorResult<()> {
        // Validate key
        validate_vector_key(key)?;

        // Ensure collection is loaded
        self.ensure_collection_loaded(run_id, collection)?;

        let collection_id = CollectionId::new(run_id, collection);

        // Validate dimension
        let config = self.get_collection_config_required(run_id, collection)?;
        if embedding.len() != config.dimension {
            return Err(VectorError::DimensionMismatch {
                expected: config.dimension,
                got: embedding.len(),
            });
        }

        // Check if vector already exists
        let kv_key = Key::new_vector(Namespace::for_run(run_id), collection, key);
        let existing = self.get_vector_record_by_key(&kv_key)?;
        let is_update = existing.is_some();

        let (vector_id, record) = if let Some(existing_record) = existing {
            // Update existing: keep the same VectorId
            let mut updated = existing_record;
            updated.update(metadata);
            (VectorId(updated.vector_id), updated)
        } else {
            // New vector: allocate VectorId from backend
            let mut backends = self.backends.write().unwrap();
            let backend = backends.get_mut(&collection_id).ok_or_else(|| {
                VectorError::CollectionNotFound {
                    name: collection.to_string(),
                }
            })?;

            // Allocate new ID (monotonic, never reused)
            let vector_id = self.allocate_vector_id(&collection_id);
            let record = VectorRecord::new(vector_id, metadata);

            // Insert into backend
            backend.insert(vector_id, embedding)?;

            drop(backends);
            (vector_id, record)
        };

        // For updates, update the backend
        if is_update {
            let mut backends = self.backends.write().unwrap();
            if let Some(backend) = backends.get_mut(&collection_id) {
                backend.insert(vector_id, embedding)?;
            }
        }

        // Store record in KV
        let record_bytes = record.to_bytes()?;
        self.db
            .transaction(run_id, |txn| {
                txn.put(kv_key.clone(), Value::Bytes(record_bytes.clone()))
            })
            .map_err(|e| VectorError::Storage(e.to_string()))?;

        Ok(())
    }

    /// Get a vector by key
    ///
    /// Returns the vector entry including embedding and metadata.
    /// Returns None if vector doesn't exist.
    pub fn get(
        &self,
        run_id: RunId,
        collection: &str,
        key: &str,
    ) -> VectorResult<Option<VectorEntry>> {
        // Ensure collection is loaded
        self.ensure_collection_loaded(run_id, collection)?;

        let collection_id = CollectionId::new(run_id, collection);
        let kv_key = Key::new_vector(Namespace::for_run(run_id), collection, key);

        // Get record from KV
        let Some(record) = self.get_vector_record_by_key(&kv_key)? else {
            return Ok(None);
        };

        let vector_id = VectorId(record.vector_id);

        // Get embedding from backend
        let backends = self.backends.read().unwrap();
        let backend =
            backends
                .get(&collection_id)
                .ok_or_else(|| VectorError::CollectionNotFound {
                    name: collection.to_string(),
                })?;

        let embedding = backend
            .get(vector_id)
            .ok_or_else(|| VectorError::Internal("Embedding missing from backend".to_string()))?;

        Ok(Some(VectorEntry {
            key: key.to_string(),
            embedding: embedding.to_vec(),
            metadata: record.metadata,
            vector_id,
            version: record.version,
        }))
    }

    /// Delete a vector by key
    ///
    /// Returns true if the vector existed and was deleted.
    pub fn delete(&self, run_id: RunId, collection: &str, key: &str) -> VectorResult<bool> {
        // Ensure collection is loaded
        self.ensure_collection_loaded(run_id, collection)?;

        let collection_id = CollectionId::new(run_id, collection);
        let kv_key = Key::new_vector(Namespace::for_run(run_id), collection, key);

        // Get existing record
        let Some(record) = self.get_vector_record_by_key(&kv_key)? else {
            return Ok(false);
        };

        let vector_id = VectorId(record.vector_id);

        // Delete from backend
        {
            let mut backends = self.backends.write().unwrap();
            if let Some(backend) = backends.get_mut(&collection_id) {
                backend.delete(vector_id)?;
            }
        }

        // Delete from KV
        self.db
            .transaction(run_id, |txn| txn.delete(kv_key.clone()))
            .map_err(|e| VectorError::Storage(e.to_string()))?;

        Ok(true)
    }

    /// Get count of vectors in a collection
    pub fn count(&self, run_id: RunId, collection: &str) -> VectorResult<usize> {
        // Ensure collection is loaded
        self.ensure_collection_loaded(run_id, collection)?;

        let collection_id = CollectionId::new(run_id, collection);
        let backends = self.backends.read().unwrap();

        backends
            .get(&collection_id)
            .map(|b| b.len())
            .ok_or_else(|| VectorError::CollectionNotFound {
                name: collection.to_string(),
            })
    }

    /// Search for similar vectors
    ///
    /// Returns top-k vectors most similar to the query.
    /// Metadata filtering is applied as post-filter.
    ///
    /// # Invariants Satisfied
    /// - R1: Dimension validated against collection config
    /// - R2: Scores normalized to "higher = more similar"
    /// - R3: Deterministic order (backend + facade tie-breaking)
    /// - R5: Facade tie-break (score desc, key asc)
    /// - R10: Search is read-only (no mutations)
    pub fn search(
        &self,
        run_id: RunId,
        collection: &str,
        query: &[f32],
        k: usize,
        filter: Option<MetadataFilter>,
    ) -> VectorResult<Vec<VectorMatch>> {
        // k=0 returns empty
        if k == 0 {
            return Ok(Vec::new());
        }

        // Ensure collection is loaded
        self.ensure_collection_loaded(run_id, collection)?;

        let collection_id = CollectionId::new(run_id, collection);

        // Validate query dimension
        let config = self.get_collection_config_required(run_id, collection)?;
        if query.len() != config.dimension {
            return Err(VectorError::DimensionMismatch {
                expected: config.dimension,
                got: query.len(),
            });
        }

        // Search backend (returns VectorId, score pairs)
        let candidates = {
            let backends = self.backends.read().unwrap();
            let backend =
                backends
                    .get(&collection_id)
                    .ok_or_else(|| VectorError::CollectionNotFound {
                        name: collection.to_string(),
                    })?;

            // Over-fetch if filtering to account for filtered-out results
            let fetch_k = if filter.is_some() { k * 3 } else { k };
            backend.search(query, fetch_k)
        };

        // Load metadata and apply filter
        let mut matches = Vec::with_capacity(k);

        for (vector_id, score) in candidates {
            if matches.len() >= k {
                break;
            }

            // Get key and metadata from KV
            let (key, metadata) = self.get_key_and_metadata(run_id, collection, vector_id)?;

            // Apply filter (post-filter)
            if let Some(ref f) = filter {
                if !f.matches(&metadata) {
                    continue;
                }
            }

            matches.push(VectorMatch {
                key,
                score,
                metadata,
            });
        }

        // Apply facade-level tie-breaking (score desc, key asc)
        // This satisfies Invariant R5
        matches.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.key.cmp(&b.key))
        });

        // Ensure we don't exceed k after sorting
        matches.truncate(k);

        Ok(matches)
    }

    /// Search without filter (convenience method)
    pub fn search_simple(
        &self,
        run_id: RunId,
        collection: &str,
        query: &[f32],
        k: usize,
    ) -> VectorResult<Vec<VectorMatch>> {
        self.search(run_id, collection, query, k, None)
    }

    /// Search returning M6-compatible SearchResponse
    ///
    /// Converts vector results to SearchResponse for hybrid search integration.
    pub fn search_response(
        &self,
        run_id: RunId,
        collection: &str,
        query: &[f32],
        k: usize,
        filter: Option<MetadataFilter>,
    ) -> VectorResult<SearchResponse> {
        let start = std::time::Instant::now();
        let matches = self.search(run_id, collection, query, k, filter)?;

        let hits: Vec<SearchHit> = matches
            .into_iter()
            .enumerate()
            .map(|(rank, m)| {
                let doc_ref = DocRef::vector(run_id, collection, &m.key);
                SearchHit::new(doc_ref, m.score, (rank + 1) as u32)
            })
            .collect();

        let stats = SearchStats::new(start.elapsed().as_micros() as u64, hits.len());

        Ok(SearchResponse::new(hits, false, stats))
    }

    // ========================================================================
    // Internal Helpers
    // ========================================================================

    /// Initialize the index backend for a collection
    fn init_backend(&self, id: &CollectionId, config: &VectorConfig) {
        let backend = self.backend_factory.create(config);
        self.backends.write().unwrap().insert(id.clone(), backend);
    }

    /// Get collection config (required version that errors if not found)
    fn get_collection_config_required(
        &self,
        run_id: RunId,
        name: &str,
    ) -> VectorResult<VectorConfig> {
        self.load_collection_config(run_id, name)?
            .ok_or_else(|| VectorError::CollectionNotFound {
                name: name.to_string(),
            })
    }

    /// Get a vector record by KV key
    fn get_vector_record_by_key(&self, key: &Key) -> VectorResult<Option<VectorRecord>> {
        use in_mem_core::traits::SnapshotView;

        let snapshot = self.db.storage().create_snapshot();
        let Some(versioned) = snapshot
            .get(key)
            .map_err(|e| VectorError::Storage(e.to_string()))?
        else {
            return Ok(None);
        };

        let bytes = match &versioned.value {
            Value::Bytes(b) => b,
            _ => {
                return Err(VectorError::Serialization(
                    "Expected Bytes value for vector record".to_string(),
                ))
            }
        };

        let record = VectorRecord::from_bytes(bytes)?;
        Ok(Some(record))
    }

    /// Allocate a new VectorId (monotonic, never reused)
    fn allocate_vector_id(&self, _collection_id: &CollectionId) -> VectorId {
        // For now, use the next available ID based on backend size + 1
        // This is a simplification - in production, we'd use atomic counters
        // that persist across restarts
        static NEXT_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
        VectorId(NEXT_ID.fetch_add(1, std::sync::atomic::Ordering::SeqCst))
    }

    /// Get key and metadata for a VectorId by scanning KV
    ///
    /// This is O(n) in M8. M9 can add a reverse index for O(1) lookup.
    fn get_key_and_metadata(
        &self,
        run_id: RunId,
        collection: &str,
        target_id: VectorId,
    ) -> VectorResult<(String, Option<JsonValue>)> {
        use in_mem_core::traits::SnapshotView;

        let namespace = Namespace::for_run(run_id);
        let prefix = Key::vector_collection_prefix(namespace, collection);

        let snapshot = self.db.storage().create_snapshot();
        let entries = snapshot
            .scan_prefix(&prefix)
            .map_err(|e| VectorError::Storage(e.to_string()))?;

        for (key, versioned) in entries {
            let bytes = match &versioned.value {
                Value::Bytes(b) => b,
                _ => continue,
            };

            let record = match VectorRecord::from_bytes(bytes) {
                Ok(r) => r,
                Err(_) => continue,
            };

            if record.vector_id == target_id.0 {
                // Extract vector key from the full key
                // Key format: collection/key
                let user_key = String::from_utf8(key.user_key.clone())
                    .map_err(|e| VectorError::Serialization(e.to_string()))?;

                // Remove collection prefix
                let vector_key = user_key
                    .strip_prefix(&format!("{}/", collection))
                    .unwrap_or(&user_key)
                    .to_string();

                return Ok((vector_key, record.metadata));
            }
        }

        Err(VectorError::Internal(format!(
            "VectorId {:?} not found in KV",
            target_id
        )))
    }

    /// Get the current vector count for a collection
    fn get_collection_count(
        &self,
        id: &CollectionId,
        run_id: RunId,
        name: &str,
    ) -> VectorResult<usize> {
        // Check in-memory backend first
        let backends = self.backends.read().unwrap();
        if let Some(backend) = backends.get(id) {
            return Ok(backend.len());
        }
        drop(backends);

        // Backend not loaded - count from KV
        use in_mem_core::traits::SnapshotView;
        let namespace = Namespace::for_run(run_id);
        let prefix = Key::vector_collection_prefix(namespace, name);

        let snapshot = self.db.storage().create_snapshot();
        let entries = snapshot
            .scan_prefix(&prefix)
            .map_err(|e| VectorError::Storage(e.to_string()))?;

        Ok(entries.len())
    }

    /// Delete all vectors in a collection
    fn delete_all_vectors(&self, run_id: RunId, name: &str) -> VectorResult<()> {
        use in_mem_core::traits::SnapshotView;

        let namespace = Namespace::for_run(run_id);
        let prefix = Key::vector_collection_prefix(namespace, name);

        // Scan all vector keys in this collection
        let snapshot = self.db.storage().create_snapshot();
        let entries = snapshot
            .scan_prefix(&prefix)
            .map_err(|e| VectorError::Storage(e.to_string()))?;

        let keys: Vec<Key> = entries.into_iter().map(|(key, _)| key).collect();

        // Delete each vector in a transaction
        if !keys.is_empty() {
            self.db
                .transaction(run_id, |txn| {
                    for key in &keys {
                        let k: Key = key.clone();
                        txn.delete(k)?;
                    }
                    Ok(())
                })
                .map_err(|e| VectorError::Storage(e.to_string()))?;
        }

        Ok(())
    }

    /// Load collection config from KV
    fn load_collection_config(
        &self,
        run_id: RunId,
        name: &str,
    ) -> VectorResult<Option<VectorConfig>> {
        use in_mem_core::traits::SnapshotView;

        let config_key = Key::new_vector_config(Namespace::for_run(run_id), name);
        let snapshot = self.db.storage().create_snapshot();

        let Some(versioned_value) = snapshot
            .get(&config_key)
            .map_err(|e| VectorError::Storage(e.to_string()))?
        else {
            return Ok(None);
        };

        let bytes = match &versioned_value.value {
            Value::Bytes(b) => b.clone(),
            _ => {
                return Err(VectorError::Serialization(
                    "Expected Bytes value for collection record".to_string(),
                ))
            }
        };

        let record = CollectionRecord::from_bytes(&bytes)?;
        let config = VectorConfig::try_from(record.config)?;
        Ok(Some(config))
    }

    /// Ensure collection is loaded into memory
    ///
    /// If the collection exists in KV but not in memory (after recovery),
    /// this loads it and initializes the backend.
    pub fn ensure_collection_loaded(&self, run_id: RunId, name: &str) -> VectorResult<()> {
        let collection_id = CollectionId::new(run_id, name);

        // Already loaded?
        if self.backends.read().unwrap().contains_key(&collection_id) {
            return Ok(());
        }

        // Load from KV
        let config = self.load_collection_config(run_id, name)?.ok_or_else(|| {
            VectorError::CollectionNotFound {
                name: name.to_string(),
            }
        })?;

        // Initialize backend
        self.init_backend(&collection_id, &config);

        // Note: Loading vectors into backend happens in Epic 55 (recovery)

        Ok(())
    }
}

/// Get current time in microseconds since Unix epoch
fn now_micros() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("Time went backwards")
        .as_micros() as u64
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vector::{DistanceMetric, VectorConfig};
    use tempfile::TempDir;

    fn setup() -> (TempDir, Arc<Database>, VectorStore) {
        let temp_dir = TempDir::new().unwrap();
        let db = Arc::new(Database::open(temp_dir.path()).unwrap());
        let store = VectorStore::new(db.clone());
        (temp_dir, db, store)
    }

    // ========================================
    // Collection Lifecycle Tests (#347, #348)
    // ========================================

    #[test]
    fn test_create_collection() {
        let (_temp, _db, store) = setup();
        let run_id = RunId::new();

        let config = VectorConfig::for_minilm();
        let info = store
            .create_collection(run_id, "test", config.clone())
            .unwrap();

        assert_eq!(info.name, "test");
        assert_eq!(info.count, 0);
        assert_eq!(info.config.dimension, 384);
        assert_eq!(info.config.metric, DistanceMetric::Cosine);
    }

    #[test]
    fn test_collection_already_exists() {
        let (_temp, _db, store) = setup();
        let run_id = RunId::new();

        let config = VectorConfig::for_minilm();
        store
            .create_collection(run_id, "test", config.clone())
            .unwrap();

        // Second create should fail
        let result = store.create_collection(run_id, "test", config);
        assert!(matches!(
            result,
            Err(VectorError::CollectionAlreadyExists { .. })
        ));
    }

    #[test]
    fn test_delete_collection() {
        let (_temp, _db, store) = setup();
        let run_id = RunId::new();

        let config = VectorConfig::for_minilm();
        store
            .create_collection(run_id, "test", config.clone())
            .unwrap();

        // Delete should succeed
        store.delete_collection(run_id, "test").unwrap();

        // Collection should no longer exist
        assert!(!store.collection_exists(run_id, "test").unwrap());
    }

    #[test]
    fn test_delete_collection_not_found() {
        let (_temp, _db, store) = setup();
        let run_id = RunId::new();

        let result = store.delete_collection(run_id, "nonexistent");
        assert!(matches!(
            result,
            Err(VectorError::CollectionNotFound { .. })
        ));
    }

    // ========================================
    // Collection Discovery Tests (#349)
    // ========================================

    #[test]
    fn test_list_collections() {
        let (_temp, _db, store) = setup();
        let run_id = RunId::new();

        // Create multiple collections
        store
            .create_collection(run_id, "zeta", VectorConfig::for_minilm())
            .unwrap();
        store
            .create_collection(run_id, "alpha", VectorConfig::for_mpnet())
            .unwrap();
        store
            .create_collection(run_id, "beta", VectorConfig::for_openai_ada())
            .unwrap();

        let collections = store.list_collections(run_id).unwrap();

        // Should be sorted by name
        assert_eq!(collections.len(), 3);
        assert_eq!(collections[0].name, "alpha");
        assert_eq!(collections[1].name, "beta");
        assert_eq!(collections[2].name, "zeta");
    }

    #[test]
    fn test_list_collections_empty() {
        let (_temp, _db, store) = setup();
        let run_id = RunId::new();

        let collections = store.list_collections(run_id).unwrap();
        assert!(collections.is_empty());
    }

    #[test]
    fn test_get_collection() {
        let (_temp, _db, store) = setup();
        let run_id = RunId::new();

        let config = VectorConfig::new(768, DistanceMetric::Euclidean).unwrap();
        store
            .create_collection(run_id, "embeddings", config)
            .unwrap();

        let info = store.get_collection(run_id, "embeddings").unwrap().unwrap();
        assert_eq!(info.name, "embeddings");
        assert_eq!(info.config.dimension, 768);
        assert_eq!(info.config.metric, DistanceMetric::Euclidean);
    }

    #[test]
    fn test_get_collection_not_found() {
        let (_temp, _db, store) = setup();
        let run_id = RunId::new();

        let info = store.get_collection(run_id, "nonexistent").unwrap();
        assert!(info.is_none());
    }

    // ========================================
    // Run Isolation Tests (Rule #2)
    // ========================================

    #[test]
    fn test_run_isolation() {
        let (_temp, _db, store) = setup();
        let run1 = RunId::new();
        let run2 = RunId::new();

        let config = VectorConfig::for_minilm();

        // Create same-named collection in different runs
        store
            .create_collection(run1, "shared_name", config.clone())
            .unwrap();
        store
            .create_collection(run2, "shared_name", config)
            .unwrap();

        // Each run sees only its own collection
        let list1 = store.list_collections(run1).unwrap();
        let list2 = store.list_collections(run2).unwrap();

        assert_eq!(list1.len(), 1);
        assert_eq!(list2.len(), 1);

        // Deleting from one run doesn't affect the other
        store.delete_collection(run1, "shared_name").unwrap();
        assert!(store.get_collection(run2, "shared_name").unwrap().is_some());
    }

    // ========================================
    // Config Persistence Tests (#350)
    // ========================================

    #[test]
    fn test_collection_config_roundtrip() {
        let (_temp, _db, store) = setup();
        let run_id = RunId::new();

        let config = VectorConfig::new(768, DistanceMetric::Euclidean).unwrap();
        store
            .create_collection(run_id, "test", config.clone())
            .unwrap();

        // Get collection and verify config
        let info = store.get_collection(run_id, "test").unwrap().unwrap();
        assert_eq!(info.config.dimension, config.dimension);
        assert_eq!(info.config.metric, config.metric);
    }

    #[test]
    fn test_collection_survives_reload() {
        let temp_dir = TempDir::new().unwrap();
        let run_id = RunId::new();

        // Create collection
        {
            let db = Arc::new(Database::open(temp_dir.path()).unwrap());
            let store = VectorStore::new(db);

            let config = VectorConfig::new(512, DistanceMetric::DotProduct).unwrap();
            store
                .create_collection(run_id, "persistent", config)
                .unwrap();
        }

        // Reopen database and verify collection exists
        {
            let db = Arc::new(Database::open(temp_dir.path()).unwrap());
            let store = VectorStore::new(db);

            let info = store.get_collection(run_id, "persistent").unwrap().unwrap();
            assert_eq!(info.config.dimension, 512);
            assert_eq!(info.config.metric, DistanceMetric::DotProduct);
        }
    }

    // ========================================
    // Validation Tests
    // ========================================

    #[test]
    fn test_invalid_collection_name() {
        let (_temp, _db, store) = setup();
        let run_id = RunId::new();

        let config = VectorConfig::for_minilm();

        // Empty name
        let result = store.create_collection(run_id, "", config.clone());
        assert!(matches!(
            result,
            Err(VectorError::InvalidCollectionName { .. })
        ));

        // Reserved name
        let result = store.create_collection(run_id, "_reserved", config.clone());
        assert!(matches!(
            result,
            Err(VectorError::InvalidCollectionName { .. })
        ));

        // Contains slash
        let result = store.create_collection(run_id, "has/slash", config);
        assert!(matches!(
            result,
            Err(VectorError::InvalidCollectionName { .. })
        ));
    }

    #[test]
    fn test_invalid_dimension() {
        let (_temp, _db, store) = setup();
        let run_id = RunId::new();

        // Dimension 0 should fail
        let config = VectorConfig {
            dimension: 0,
            metric: DistanceMetric::Cosine,
            storage_dtype: crate::vector::StorageDtype::F32,
        };

        let result = store.create_collection(run_id, "test", config);
        assert!(matches!(
            result,
            Err(VectorError::InvalidDimension { dimension: 0 })
        ));
    }

    // ========================================
    // Thread Safety Tests
    // ========================================

    #[test]
    fn test_vector_store_is_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<VectorStore>();
    }

    #[test]
    fn test_vector_store_clone() {
        let (_temp, _db, store1) = setup();
        let store2 = store1.clone();

        // Both point to same database
        assert!(Arc::ptr_eq(store1.database(), store2.database()));
    }

    // ========================================
    // Vector Insert/Get/Delete Tests (Epic 54)
    // ========================================

    #[test]
    fn test_insert_and_get_vector() {
        let (_temp, _db, store) = setup();
        let run_id = RunId::new();

        // Create collection
        let config = VectorConfig::new(3, DistanceMetric::Cosine).unwrap();
        store.create_collection(run_id, "test", config).unwrap();

        // Insert vector
        let embedding = vec![1.0, 0.0, 0.0];
        store
            .insert(run_id, "test", "doc1", &embedding, None)
            .unwrap();

        // Get vector
        let entry = store.get(run_id, "test", "doc1").unwrap().unwrap();
        assert_eq!(entry.key, "doc1");
        assert_eq!(entry.embedding, embedding);
        assert!(entry.metadata.is_none());
    }

    #[test]
    fn test_insert_with_metadata() {
        let (_temp, _db, store) = setup();
        let run_id = RunId::new();

        let config = VectorConfig::new(3, DistanceMetric::Cosine).unwrap();
        store.create_collection(run_id, "test", config).unwrap();

        let metadata = serde_json::json!({"type": "document", "author": "test"});
        store
            .insert(
                run_id,
                "test",
                "doc1",
                &[1.0, 0.0, 0.0],
                Some(metadata.clone()),
            )
            .unwrap();

        let entry = store.get(run_id, "test", "doc1").unwrap().unwrap();
        assert_eq!(entry.metadata, Some(metadata));
    }

    #[test]
    fn test_upsert_overwrites() {
        let (_temp, _db, store) = setup();
        let run_id = RunId::new();

        let config = VectorConfig::new(3, DistanceMetric::Cosine).unwrap();
        store.create_collection(run_id, "test", config).unwrap();

        // Insert original
        store
            .insert(run_id, "test", "doc1", &[1.0, 0.0, 0.0], None)
            .unwrap();

        // Upsert with new embedding
        store
            .insert(run_id, "test", "doc1", &[0.0, 1.0, 0.0], None)
            .unwrap();

        let entry = store.get(run_id, "test", "doc1").unwrap().unwrap();
        assert_eq!(entry.embedding, vec![0.0, 1.0, 0.0]);
    }

    #[test]
    fn test_delete_vector() {
        let (_temp, _db, store) = setup();
        let run_id = RunId::new();

        let config = VectorConfig::new(3, DistanceMetric::Cosine).unwrap();
        store.create_collection(run_id, "test", config).unwrap();

        store
            .insert(run_id, "test", "doc1", &[1.0, 0.0, 0.0], None)
            .unwrap();

        // Delete
        let deleted = store.delete(run_id, "test", "doc1").unwrap();
        assert!(deleted);

        // Should not exist
        let entry = store.get(run_id, "test", "doc1").unwrap();
        assert!(entry.is_none());

        // Delete again returns false
        let deleted = store.delete(run_id, "test", "doc1").unwrap();
        assert!(!deleted);
    }

    #[test]
    fn test_count() {
        let (_temp, _db, store) = setup();
        let run_id = RunId::new();

        let config = VectorConfig::new(3, DistanceMetric::Cosine).unwrap();
        store.create_collection(run_id, "test", config).unwrap();

        assert_eq!(store.count(run_id, "test").unwrap(), 0);

        store
            .insert(run_id, "test", "a", &[1.0, 0.0, 0.0], None)
            .unwrap();
        store
            .insert(run_id, "test", "b", &[0.0, 1.0, 0.0], None)
            .unwrap();

        assert_eq!(store.count(run_id, "test").unwrap(), 2);
    }

    #[test]
    fn test_dimension_mismatch() {
        let (_temp, _db, store) = setup();
        let run_id = RunId::new();

        let config = VectorConfig::new(3, DistanceMetric::Cosine).unwrap();
        store.create_collection(run_id, "test", config).unwrap();

        // Wrong dimension
        let result = store.insert(run_id, "test", "doc1", &[1.0, 0.0], None);
        assert!(matches!(
            result,
            Err(VectorError::DimensionMismatch {
                expected: 3,
                got: 2
            })
        ));
    }

    // ========================================
    // Vector Search Tests (Epic 54)
    // ========================================

    #[test]
    fn test_search_basic() {
        let (_temp, _db, store) = setup();
        let run_id = RunId::new();

        let config = VectorConfig::new(3, DistanceMetric::Cosine).unwrap();
        store.create_collection(run_id, "test", config).unwrap();

        // Insert vectors
        store
            .insert(run_id, "test", "a", &[1.0, 0.0, 0.0], None)
            .unwrap();
        store
            .insert(run_id, "test", "b", &[0.0, 1.0, 0.0], None)
            .unwrap();
        store
            .insert(run_id, "test", "c", &[0.9, 0.1, 0.0], None)
            .unwrap();

        // Search
        let query = [1.0, 0.0, 0.0];
        let results = store.search(run_id, "test", &query, 2, None).unwrap();

        assert_eq!(results.len(), 2);
        assert_eq!(results[0].key, "a"); // Most similar
        assert_eq!(results[1].key, "c"); // Second most similar
    }

    #[test]
    fn test_search_k_zero() {
        let (_temp, _db, store) = setup();
        let run_id = RunId::new();

        let config = VectorConfig::new(3, DistanceMetric::Cosine).unwrap();
        store.create_collection(run_id, "test", config).unwrap();

        store
            .insert(run_id, "test", "a", &[1.0, 0.0, 0.0], None)
            .unwrap();

        let results = store
            .search(run_id, "test", &[1.0, 0.0, 0.0], 0, None)
            .unwrap();
        assert!(results.is_empty());
    }

    #[test]
    fn test_search_with_filter() {
        let (_temp, _db, store) = setup();
        let run_id = RunId::new();

        let config = VectorConfig::new(3, DistanceMetric::Cosine).unwrap();
        store.create_collection(run_id, "test", config).unwrap();

        store
            .insert(
                run_id,
                "test",
                "a",
                &[1.0, 0.0, 0.0],
                Some(serde_json::json!({"type": "document"})),
            )
            .unwrap();
        store
            .insert(
                run_id,
                "test",
                "b",
                &[0.9, 0.1, 0.0],
                Some(serde_json::json!({"type": "image"})),
            )
            .unwrap();
        store
            .insert(
                run_id,
                "test",
                "c",
                &[0.8, 0.2, 0.0],
                Some(serde_json::json!({"type": "document"})),
            )
            .unwrap();

        // Filter by type
        let filter = MetadataFilter::new().eq("type", "document");
        let results = store
            .search(run_id, "test", &[1.0, 0.0, 0.0], 10, Some(filter))
            .unwrap();

        assert_eq!(results.len(), 2);
        for result in &results {
            let meta = result.metadata.as_ref().unwrap();
            assert_eq!(meta.get("type").unwrap().as_str().unwrap(), "document");
        }
    }

    #[test]
    fn test_search_response() {
        let (_temp, _db, store) = setup();
        let run_id = RunId::new();

        let config = VectorConfig::new(3, DistanceMetric::Cosine).unwrap();
        store.create_collection(run_id, "test", config).unwrap();

        store
            .insert(run_id, "test", "doc1", &[1.0, 0.0, 0.0], None)
            .unwrap();

        let response = store
            .search_response(run_id, "test", &[1.0, 0.0, 0.0], 10, None)
            .unwrap();

        assert_eq!(response.hits.len(), 1);
        assert!(response.hits[0].doc_ref.is_vector());
        assert_eq!(response.hits[0].rank, 1);
    }

    #[test]
    fn test_search_deterministic_order() {
        let (_temp, _db, store) = setup();
        let run_id = RunId::new();

        let config = VectorConfig::new(3, DistanceMetric::Cosine).unwrap();
        store.create_collection(run_id, "test", config).unwrap();

        // Insert vectors with same similarity
        store
            .insert(run_id, "test", "b", &[1.0, 0.0, 0.0], None)
            .unwrap();
        store
            .insert(run_id, "test", "a", &[1.0, 0.0, 0.0], None)
            .unwrap();
        store
            .insert(run_id, "test", "c", &[1.0, 0.0, 0.0], None)
            .unwrap();

        // Search multiple times - order should be consistent
        for _ in 0..5 {
            let results = store
                .search(run_id, "test", &[1.0, 0.0, 0.0], 3, None)
                .unwrap();

            // Should be sorted by key (tie-breaker) since scores are equal
            assert_eq!(results[0].key, "a");
            assert_eq!(results[1].key, "b");
            assert_eq!(results[2].key, "c");
        }
    }
}
