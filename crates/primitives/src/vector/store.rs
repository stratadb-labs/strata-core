//! VectorStore: Vector storage and search primitive
//!
//! ## Design
//!
//! VectorStore is a **stateless facade** over the Database engine for collection
//! management. Following the same pattern as KVStore, JsonStore, and other primitives:
//!
//! - VectorStore holds only `Arc<Database>` (no private state)
//! - All persistent state lives in the Database (via the extension mechanism)
//! - Multiple VectorStore instances for the same Database share state
//!
//! This ensures that concurrent access from multiple threads or instances
//! sees consistent state, avoiding the data loss bug where each VectorStore::new()
//! created a private, empty backends map.
//!
//! ## Run Isolation
//!
//! All operations are scoped to a `RunId`. Different runs cannot see
//! each other's collections or vectors.
//!
//! ## Thread Safety
//!
//! VectorStore is `Send + Sync` and can be safely shared across threads.
//! All VectorStore instances for the same Database share backend state
//! through `Database::extension::<VectorBackendState>()`.

use crate::extensions::VectorStoreExt;
use crate::vector::collection::{validate_collection_name, validate_vector_key};
use crate::vector::{
    CollectionId, CollectionInfo, CollectionRecord, IndexBackendFactory, MetadataFilter,
    VectorConfig, VectorEntry, VectorError, VectorId, VectorIndexBackend, VectorMatch,
    VectorRecord, VectorResult,
};
use strata_concurrency::TransactionContext;
use strata_core::contract::{Timestamp, Version, Versioned};
use strata_core::search_types::{DocRef, SearchBudget, SearchHit, SearchResponse, SearchStats};
use strata_core::types::{Key, Namespace, RunId};
use strata_core::value::Value;
use strata_durability::wal::WALEntry;
use strata_engine::Database;
use serde_json::Value as JsonValue;
use std::collections::BTreeMap;
use std::sync::{Arc, RwLock};

/// Statistics from vector recovery
#[derive(Debug, Default, Clone)]
pub struct RecoveryStats {
    /// Number of collections created during recovery
    pub collections_created: usize,
    /// Number of collections deleted during recovery
    pub collections_deleted: usize,
    /// Number of vectors upserted during recovery
    pub vectors_upserted: usize,
    /// Number of vectors deleted during recovery
    pub vectors_deleted: usize,
}

/// Shared backend state for VectorStore
///
/// This struct is stored in the Database via the extension mechanism,
/// ensuring all VectorStore instances for the same Database share the same
/// backend state. This is critical for correct concurrent operation.
///
/// # Thread Safety
///
/// Protected by RwLock for concurrent read access and exclusive write access.
/// Uses BTreeMap for deterministic iteration order (Invariant R3).
pub struct VectorBackendState {
    /// In-memory index backends per collection
    /// CRITICAL: BTreeMap for deterministic iteration (Invariant R3)
    pub backends: RwLock<BTreeMap<CollectionId, Box<dyn VectorIndexBackend>>>,
}

impl Default for VectorBackendState {
    fn default() -> Self {
        Self {
            backends: RwLock::new(BTreeMap::new()),
        }
    }
}

/// Vector storage and search primitive
///
/// Manages collections of vectors with similarity search capabilities.
/// This is a **stateless facade** - it holds only a reference to the Database.
/// All backend state is stored in the Database via `extension::<VectorBackendState>()`.
///
/// # Example
///
/// ```ignore
/// use strata_primitives::VectorStore;
/// use strata_engine::Database;
/// use strata_core::types::RunId;
///
/// let db = Arc::new(Database::open("/path/to/data")?);
/// let store = VectorStore::new(db.clone());
/// let run_id = RunId::new();
///
/// // Create collection
/// let config = VectorConfig::for_minilm();
/// store.create_collection(run_id, "embeddings", config)?;
///
/// // Multiple stores share the same backend state
/// let store2 = VectorStore::new(db.clone());
/// // store2 sees the same collections as store
/// ```
#[derive(Clone)]
pub struct VectorStore {
    db: Arc<Database>,
}

impl VectorStore {
    /// Create a new VectorStore
    ///
    /// This is a stateless facade - all backend state is stored in the Database
    /// via the extension mechanism. Multiple VectorStore instances for the same
    /// Database share the same backend state.
    ///
    /// NOTE: Recovery is NOT performed automatically. Recovery is orchestrated
    /// by the Database during startup, which calls `recover()` after all
    /// primitives are registered.
    pub fn new(db: Arc<Database>) -> Self {
        Self { db }
    }

    /// Get the underlying database reference
    pub fn database(&self) -> &Arc<Database> {
        &self.db
    }

    /// Get access to the shared backend state
    ///
    /// This returns the shared `VectorBackendState` stored in the Database.
    /// All VectorStore instances for the same Database share this state.
    fn state(&self) -> Arc<VectorBackendState> {
        self.db.extension::<VectorBackendState>()
    }

    /// Get the backend factory (hardcoded for M8, configurable in M9)
    fn backend_factory(&self) -> IndexBackendFactory {
        // M8: Hardcoded to BruteForce. M9 will make this configurable.
        IndexBackendFactory::default()
    }

    // ========================================================================
    // WAL Writing (Epic 55 - M8)
    // ========================================================================

    /// Check if WAL writing is required (not InMemory mode)
    fn requires_wal(&self) -> bool {
        self.db.durability_mode().requires_wal()
    }

    /// Write a WAL entry (if not InMemory mode)
    fn write_wal_entry(&self, entry: WALEntry) -> VectorResult<()> {
        if !self.requires_wal() {
            return Ok(());
        }

        let wal = self.db.wal();
        let mut wal_guard = wal.lock().unwrap();
        wal_guard
            .append(&entry)
            .map_err(|e| VectorError::Storage(format!("WAL write failed: {}", e)))?;
        wal_guard
            .flush()
            .map_err(|e| VectorError::Storage(format!("WAL flush failed: {}", e)))?;
        Ok(())
    }

    // ========================================================================
    // WAL Recovery (Epic 55 - M8 Story #424)
    // ========================================================================

    /// Recover vector state from WAL
    ///
    /// This method reads the WAL and replays all committed Vector entries.
    /// It should be called after VectorStore is created to restore durability.
    ///
    /// CRITICAL: This is part of Story #424 (Vector Recovery Implementation)
    /// The recovery process:
    /// 1. Read all WAL entries
    /// 2. For transactional entries: group by transaction, only replay committed
    /// 3. For standalone Vector entries (not in a transaction): replay directly
    ///    (these are flushed immediately so they're considered durable)
    pub fn recover(&self) -> VectorResult<RecoveryStats> {
        use std::collections::{HashMap, HashSet};

        if !self.requires_wal() {
            return Ok(RecoveryStats::default());
        }

        let wal = self.db.wal();
        let wal_guard = wal.lock().unwrap();

        // Read all WAL entries
        let entries = wal_guard
            .read_all()
            .map_err(|e| VectorError::Storage(format!("WAL read failed: {}", e)))?;

        drop(wal_guard);

        let mut stats = RecoveryStats::default();

        // Track transactions: txn_id -> (run_id, entries, committed)
        struct TxnState {
            entries: Vec<WALEntry>,
            committed: bool,
        }
        let mut transactions: HashMap<u64, TxnState> = HashMap::new();
        let mut active_txn: HashMap<RunId, u64> = HashMap::new();
        let mut entries_in_txn: HashSet<usize> = HashSet::new(); // Track indices of entries that are in transactions

        // First pass: group transactional entries by transaction
        for (idx, entry) in entries.iter().enumerate() {
            match entry {
                WALEntry::BeginTxn { txn_id, run_id, .. } => {
                    transactions.insert(*txn_id, TxnState {
                        entries: Vec::new(),
                        committed: false,
                    });
                    active_txn.insert(*run_id, *txn_id);
                    entries_in_txn.insert(idx);
                }
                WALEntry::CommitTxn { txn_id, .. } => {
                    if let Some(txn) = transactions.get_mut(txn_id) {
                        txn.committed = true;
                    }
                    entries_in_txn.insert(idx);
                }
                WALEntry::AbortTxn { txn_id, run_id } => {
                    // Remove aborted transaction - its entries won't be replayed
                    transactions.remove(txn_id);
                    if active_txn.get(run_id) == Some(txn_id) {
                        active_txn.remove(run_id);
                    }
                    entries_in_txn.insert(idx);
                }
                // Vector entries - check if in an active transaction
                WALEntry::VectorCollectionCreate { run_id, .. }
                | WALEntry::VectorCollectionDelete { run_id, .. }
                | WALEntry::VectorUpsert { run_id, .. }
                | WALEntry::VectorDelete { run_id, .. } => {
                    if let Some(&txn_id) = active_txn.get(run_id) {
                        if let Some(txn) = transactions.get_mut(&txn_id) {
                            txn.entries.push(entry.clone());
                            entries_in_txn.insert(idx);
                        }
                    }
                    // If not in a transaction, it will be processed as a standalone entry
                }
                _ => {} // Ignore KV and JSON entries
            }
        }

        // Second pass: replay committed transactional Vector entries
        let mut committed_txns: Vec<_> = transactions
            .into_iter()
            .filter(|(_, txn)| txn.committed)
            .collect();
        committed_txns.sort_by_key(|(txn_id, _)| *txn_id);

        for (_txn_id, txn) in committed_txns {
            for entry in txn.entries {
                self.replay_vector_entry(&entry, &mut stats)?;
            }
        }

        // Third pass: replay standalone Vector entries (not in any transaction)
        // These are considered durable because they were flushed immediately
        for (idx, entry) in entries.iter().enumerate() {
            if entries_in_txn.contains(&idx) {
                continue; // Skip entries that were part of a transaction
            }

            match entry {
                WALEntry::VectorCollectionCreate { .. }
                | WALEntry::VectorCollectionDelete { .. }
                | WALEntry::VectorUpsert { .. }
                | WALEntry::VectorDelete { .. } => {
                    self.replay_vector_entry(entry, &mut stats)?;
                }
                _ => {} // Ignore non-Vector entries
            }
        }

        Ok(stats)
    }

    /// Helper to replay a single Vector WAL entry
    fn replay_vector_entry(&self, entry: &WALEntry, stats: &mut RecoveryStats) -> VectorResult<()> {
        match entry {
            WALEntry::VectorCollectionCreate {
                run_id,
                collection,
                dimension,
                metric,
                ..
            } => {
                let config = VectorConfig {
                    dimension: *dimension,
                    metric: crate::vector::DistanceMetric::from_byte(*metric)
                        .ok_or_else(|| {
                            VectorError::Serialization(format!("Invalid metric: {}", metric))
                        })?,
                    storage_dtype: crate::vector::StorageDtype::F32,
                };
                // Ignore errors if collection already exists (idempotent replay)
                let _ = self.replay_create_collection(*run_id, collection, config);
                stats.collections_created += 1;
            }
            WALEntry::VectorCollectionDelete {
                run_id,
                collection,
                ..
            } => {
                let _ = self.replay_delete_collection(*run_id, collection);
                stats.collections_deleted += 1;
            }
            WALEntry::VectorUpsert {
                run_id,
                collection,
                key,
                vector_id,
                embedding,
                metadata,
                ..
            } => {
                // If collection doesn't exist yet, try to load it from KV
                // (the KV recovery may have restored the collection config)
                let collection_id = CollectionId::new(*run_id, collection);
                let state = self.state();
                if !state.backends.read().unwrap().contains_key(&collection_id) {
                    // Try to load from KV - collection config should have been restored
                    if let Ok(Some(config)) = self.load_collection_config(*run_id, collection) {
                        self.init_backend(&collection_id, &config);
                    }
                }

                let meta: Option<JsonValue> = metadata
                    .as_ref()
                    .map(|m| serde_json::from_slice(m))
                    .transpose()
                    .map_err(|e| VectorError::Serialization(e.to_string()))?;

                // Ignore errors - collection may not exist yet if VectorCollectionCreate
                // WAL entry wasn't written (possible in some edge cases)
                // NOTE: replay_upsert calls insert_with_id which updates the per-collection
                // next_id counter to maintain VectorId monotonicity (Invariant T4)
                let _ = self.replay_upsert(
                    *run_id,
                    collection,
                    key,
                    VectorId::new(*vector_id),
                    embedding,
                    meta,
                );
                stats.vectors_upserted += 1;
            }
            WALEntry::VectorDelete {
                run_id,
                collection,
                key,
                vector_id,
                ..
            } => {
                let _ = self.replay_delete(*run_id, collection, key, VectorId::new(*vector_id));
                stats.vectors_deleted += 1;
            }
            _ => {} // Ignore other entries
        }
        Ok(())
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
    ) -> VectorResult<Versioned<CollectionInfo>> {
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

        // Write WAL entry for durability (M8 Epic 55)
        self.write_wal_entry(WALEntry::VectorCollectionCreate {
            run_id,
            collection: name.to_string(),
            dimension: config.dimension,
            metric: config.metric.to_byte(),
            version: 1, // TODO: Get proper version from coordinator
        })?;

        let info = CollectionInfo {
            name: name.to_string(),
            config,
            count: 0,
            created_at: now,
        };

        Ok(Versioned::with_timestamp(
            info,
            Version::counter(1),
            Timestamp::from_micros(now),
        ))
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
        {
            let state = self.state();
            state.backends.write().unwrap().remove(&collection_id);
        }

        // Write WAL entry for durability (M8 Epic 55)
        self.write_wal_entry(WALEntry::VectorCollectionDelete {
            run_id,
            collection: name.to_string(),
            version: 1, // TODO: Get proper version from coordinator
        })?;

        Ok(())
    }

    /// List all collections for a run
    ///
    /// Returns CollectionInfo for each collection, including current vector count.
    /// Results are sorted by name for determinism (Invariant R4).
    pub fn list_collections(&self, run_id: RunId) -> VectorResult<Vec<CollectionInfo>> {
        use strata_core::traits::SnapshotView;

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
    ) -> VectorResult<Option<Versioned<CollectionInfo>>> {
        let config_key = Key::new_vector_config(Namespace::for_run(run_id), name);

        // Read from snapshot
        use strata_core::traits::SnapshotView;
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

        let info = CollectionInfo {
            name: name.to_string(),
            config,
            count,
            created_at: record.created_at,
        };

        Ok(Some(Versioned::with_timestamp(
            info,
            versioned_value.version,
            versioned_value.timestamp,
        )))
    }

    /// Check if a collection exists
    pub fn collection_exists(&self, run_id: RunId, name: &str) -> VectorResult<bool> {
        use strata_core::traits::SnapshotView;

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
    ) -> VectorResult<Version> {
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

        // Serialize metadata to bytes for WAL storage (before it's consumed)
        let metadata_bytes = metadata
            .as_ref()
            .map(|m| serde_json::to_vec(m))
            .transpose()
            .map_err(|e| VectorError::Serialization(e.to_string()))?;

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
            // New vector: allocate VectorId from backend's per-collection counter
            let state = self.state();
            let mut backends = state.backends.write().unwrap();
            let backend = backends.get_mut(&collection_id).ok_or_else(|| {
                VectorError::CollectionNotFound {
                    name: collection.to_string(),
                }
            })?;

            // Allocate new ID from backend's per-collection counter (deterministic)
            let vector_id = backend.allocate_id();
            let record = VectorRecord::new(vector_id, metadata);

            // Insert into backend
            backend.insert(vector_id, embedding)?;

            drop(backends);
            (vector_id, record)
        };

        // For updates, update the backend
        if is_update {
            let state = self.state();
            let mut backends = state.backends.write().unwrap();
            if let Some(backend) = backends.get_mut(&collection_id) {
                backend.insert(vector_id, embedding)?;
            }
        }

        // Store record in KV
        let record_version = record.version;
        let record_bytes = record.to_bytes()?;
        self.db
            .transaction(run_id, |txn| {
                txn.put(kv_key.clone(), Value::Bytes(record_bytes.clone()))
            })
            .map_err(|e| VectorError::Storage(e.to_string()))?;

        // Write WAL entry for durability (M8 Epic 55)
        self.write_wal_entry(WALEntry::VectorUpsert {
            run_id,
            collection: collection.to_string(),
            key: key.to_string(),
            vector_id: vector_id.as_u64(),
            embedding: embedding.to_vec(),
            metadata: metadata_bytes,
            version: 1, // TODO: Get proper version from coordinator
        })?;

        Ok(Version::counter(record_version))
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
    ) -> VectorResult<Option<Versioned<VectorEntry>>> {
        // Ensure collection is loaded
        self.ensure_collection_loaded(run_id, collection)?;

        let collection_id = CollectionId::new(run_id, collection);
        let kv_key = Key::new_vector(Namespace::for_run(run_id), collection, key);

        // Get record from KV with version info
        use strata_core::traits::SnapshotView;
        let snapshot = self.db.storage().create_snapshot();
        let Some(versioned_value) = snapshot
            .get(&kv_key)
            .map_err(|e| VectorError::Storage(e.to_string()))?
        else {
            return Ok(None);
        };

        let bytes = match &versioned_value.value {
            Value::Bytes(b) => b,
            _ => {
                return Err(VectorError::Serialization(
                    "Expected Bytes value for vector record".to_string(),
                ))
            }
        };

        let record = VectorRecord::from_bytes(bytes)?;
        let vector_id = VectorId(record.vector_id);

        // Get embedding from backend
        let state = self.state();
        let backends = state.backends.read().unwrap();
        let backend =
            backends
                .get(&collection_id)
                .ok_or_else(|| VectorError::CollectionNotFound {
                    name: collection.to_string(),
                })?;

        let embedding = backend
            .get(vector_id)
            .ok_or_else(|| VectorError::Internal("Embedding missing from backend".to_string()))?;

        let entry = VectorEntry {
            key: key.to_string(),
            embedding: embedding.to_vec(),
            metadata: record.metadata,
            vector_id,
            version: record.version,
        };

        Ok(Some(Versioned::with_timestamp(
            entry,
            versioned_value.version,
            versioned_value.timestamp,
        )))
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
            let state = self.state();
            let mut backends = state.backends.write().unwrap();
            if let Some(backend) = backends.get_mut(&collection_id) {
                backend.delete(vector_id)?;
            }
        }

        // Delete from KV
        self.db
            .transaction(run_id, |txn| txn.delete(kv_key.clone()))
            .map_err(|e| VectorError::Storage(e.to_string()))?;

        // Write WAL entry for durability (M8 Epic 55)
        self.write_wal_entry(WALEntry::VectorDelete {
            run_id,
            collection: collection.to_string(),
            key: key.to_string(),
            vector_id: vector_id.as_u64(),
            version: 1, // TODO: Get proper version from coordinator
        })?;

        Ok(true)
    }

    /// Get count of vectors in a collection
    pub fn count(&self, run_id: RunId, collection: &str) -> VectorResult<usize> {
        // Ensure collection is loaded
        self.ensure_collection_loaded(run_id, collection)?;

        let collection_id = CollectionId::new(run_id, collection);
        let state = self.state();
        let backends = state.backends.read().unwrap();

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

        // Search backend with adaptive over-fetch for filtering (Issue #453)
        //
        // When a metadata filter is active, we over-fetch from the backend to account
        // for filtered-out results. If the initial fetch doesn't yield enough results,
        // we retry with a higher multiplier up to a max limit.
        //
        // Multiplier strategy: 3x -> 6x -> 12x -> all (capped at collection size)
        let mut matches = Vec::with_capacity(k);

        if filter.is_none() {
            // No filter - simple case, fetch exactly k
            let candidates = {
                let state = self.state();
                let backends = state.backends.read().unwrap();
                let backend =
                    backends
                        .get(&collection_id)
                        .ok_or_else(|| VectorError::CollectionNotFound {
                            name: collection.to_string(),
                        })?;
                backend.search(query, k)
            };

            for (vector_id, score) in candidates {
                let (key, metadata) = self.get_key_and_metadata(run_id, collection, vector_id)?;
                matches.push(VectorMatch { key, score, metadata });
            }
        } else {
            // Filter active - use adaptive over-fetch
            let multipliers = [3, 6, 12];
            let collection_size = {
                let state = self.state();
                let backends = state.backends.read().unwrap();
                backends
                    .get(&collection_id)
                    .map(|b| b.len())
                    .unwrap_or(0)
            };

            for &mult in &multipliers {
                let fetch_k = (k * mult).min(collection_size);
                if fetch_k == 0 {
                    break;
                }

                let candidates = {
                    let state = self.state();
                    let backends = state.backends.read().unwrap();
                    let backend =
                        backends
                            .get(&collection_id)
                            .ok_or_else(|| VectorError::CollectionNotFound {
                                name: collection.to_string(),
                            })?;
                    backend.search(query, fetch_k)
                };

                matches.clear();
                for (vector_id, score) in candidates {
                    let (key, metadata) = self.get_key_and_metadata(run_id, collection, vector_id)?;

                    // Apply filter
                    if let Some(ref f) = filter {
                        if !f.matches(&metadata) {
                            continue;
                        }
                    }

                    matches.push(VectorMatch { key, score, metadata });
                    if matches.len() >= k {
                        break;
                    }
                }

                // If we have enough results or searched all vectors, stop
                if matches.len() >= k || fetch_k >= collection_size {
                    break;
                }
            }
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

    /// Budget-aware search (Issue #451)
    ///
    /// Respects the M6 SearchBudget time and candidate limits.
    /// Returns (results, truncated) where truncated is true if budget was exhausted.
    ///
    /// Budget checks are performed:
    /// 1. Before starting search (early exit if already over time)
    /// 2. After similarity computation
    /// 3. After filtering (if any)
    pub fn search_with_budget(
        &self,
        run_id: RunId,
        collection: &str,
        query: &[f32],
        k: usize,
        filter: Option<MetadataFilter>,
        budget: &SearchBudget,
    ) -> VectorResult<(Vec<VectorMatch>, bool)> {
        let start = std::time::Instant::now();

        // Early exit if budget already exhausted
        if start.elapsed().as_micros() as u64 >= budget.max_wall_time_micros {
            return Ok((Vec::new(), true));
        }

        // k=0 returns empty
        if k == 0 {
            return Ok((Vec::new(), false));
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

        // Check time budget before backend search
        if start.elapsed().as_micros() as u64 >= budget.max_wall_time_micros {
            return Ok((Vec::new(), true));
        }

        // Cap fetch at budget candidate limit
        let max_candidates = budget.max_candidates_per_primitive.min(budget.max_candidates);
        let fetch_k = if filter.is_some() {
            (k * 3).min(max_candidates)
        } else {
            k.min(max_candidates)
        };

        // Search backend
        let candidates = {
            let state = self.state();
            let backends = state.backends.read().unwrap();
            let backend = backends
                .get(&collection_id)
                .ok_or_else(|| VectorError::CollectionNotFound {
                    name: collection.to_string(),
                })?;
            backend.search(query, fetch_k)
        };

        // Check time budget after search
        let truncated = start.elapsed().as_micros() as u64 >= budget.max_wall_time_micros;
        if truncated {
            return Ok((Vec::new(), true));
        }

        // Load metadata and apply filter
        let mut matches = Vec::with_capacity(k);

        for (vector_id, score) in candidates {
            // Check time budget periodically
            if matches.len() % 100 == 0 {
                if start.elapsed().as_micros() as u64 >= budget.max_wall_time_micros {
                    return Ok((matches, true));
                }
            }

            if matches.len() >= k {
                break;
            }

            let (key, metadata) = self.get_key_and_metadata(run_id, collection, vector_id)?;

            // Apply filter (post-filter)
            if let Some(ref f) = filter {
                if !f.matches(&metadata) {
                    continue;
                }
            }

            matches.push(VectorMatch { key, score, metadata });
        }

        // Apply facade-level tie-breaking
        matches.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.key.cmp(&b.key))
        });

        matches.truncate(k);

        let truncated = start.elapsed().as_micros() as u64 >= budget.max_wall_time_micros;
        Ok((matches, truncated))
    }

    // ========================================================================
    // Internal Helpers
    // ========================================================================

    /// Initialize the index backend for a collection
    fn init_backend(&self, id: &CollectionId, config: &VectorConfig) {
        let backend = self.backend_factory().create(config);
        let state = self.state();
        state.backends.write().unwrap().insert(id.clone(), backend);
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
        use strata_core::traits::SnapshotView;

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

    /// Get key and metadata for a VectorId by scanning KV
    ///
    /// This is O(n) in M8. M9 can add a reverse index for O(1) lookup.
    pub fn get_key_and_metadata(
        &self,
        run_id: RunId,
        collection: &str,
        target_id: VectorId,
    ) -> VectorResult<(String, Option<JsonValue>)> {
        use strata_core::traits::SnapshotView;

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
        let state = self.state();
        let backends = state.backends.read().unwrap();
        if let Some(backend) = backends.get(id) {
            return Ok(backend.len());
        }
        drop(backends);

        // Backend not loaded - count from KV
        use strata_core::traits::SnapshotView;
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
        use strata_core::traits::SnapshotView;

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
        use strata_core::traits::SnapshotView;

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
        {
            let state = self.state();
            if state.backends.read().unwrap().contains_key(&collection_id) {
                return Ok(());
            }
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

    // ========================================================================
    // WAL Replay Methods (Epic 55 Story #358)
    // ========================================================================

    /// Replay collection creation from WAL (no WAL write)
    ///
    /// IMPORTANT: This method is for WAL replay during recovery.
    /// It does NOT write to WAL - that would cause infinite loops during replay.
    ///
    /// Called by the global WAL replayer for committed VectorCollectionCreate entries.
    ///
    /// # Config Validation (Issue #452)
    ///
    /// If collection already exists, validates that the config matches.
    /// This catches WAL corruption or conflicting create entries.
    pub fn replay_create_collection(
        &self,
        run_id: RunId,
        name: &str,
        config: VectorConfig,
    ) -> VectorResult<()> {
        let collection_id = CollectionId::new(run_id, name);

        // Check if collection already exists in backend
        {
            let state = self.state();
            let backends = state.backends.read().unwrap();
            if let Some(existing_backend) = backends.get(&collection_id) {
                // Validate config matches (Issue #452)
                let existing_config = existing_backend.config();
                if existing_config.dimension != config.dimension {
                    tracing::warn!(
                        collection = name,
                        existing_dim = existing_config.dimension,
                        wal_dim = config.dimension,
                        "Config mismatch during WAL replay: dimension differs"
                    );
                    return Err(VectorError::DimensionMismatch {
                        expected: existing_config.dimension,
                        got: config.dimension,
                    });
                }
                if existing_config.metric != config.metric {
                    tracing::warn!(
                        collection = name,
                        existing_metric = ?existing_config.metric,
                        wal_metric = ?config.metric,
                        "Config mismatch during WAL replay: metric differs"
                    );
                    return Err(VectorError::ConfigMismatch {
                        collection: name.to_string(),
                        field: "metric".to_string(),
                    });
                }
                // Collection already exists with matching config - idempotent replay
                return Ok(());
            }
        }

        // Initialize backend (no KV write - KV is replayed separately)
        let backend = self.backend_factory().create(&config);
        let state = self.state();
        state.backends
            .write()
            .unwrap()
            .insert(collection_id, backend);

        Ok(())
    }

    /// Replay collection deletion from WAL (no WAL write)
    ///
    /// IMPORTANT: This method is for WAL replay during recovery.
    /// It does NOT write to WAL.
    pub fn replay_delete_collection(&self, run_id: RunId, name: &str) -> VectorResult<()> {
        let collection_id = CollectionId::new(run_id, name);

        // Remove in-memory backend
        let state = self.state();
        state.backends.write().unwrap().remove(&collection_id);

        Ok(())
    }

    /// Replay vector upsert from WAL (no WAL write)
    ///
    /// IMPORTANT: This method is for WAL replay during recovery.
    /// It does NOT write to WAL.
    ///
    /// Uses insert_with_id to maintain VectorId monotonicity (Invariant T4).
    pub fn replay_upsert(
        &self,
        run_id: RunId,
        collection: &str,
        _key: &str,
        vector_id: VectorId,
        embedding: &[f32],
        _metadata: Option<serde_json::Value>,
    ) -> VectorResult<()> {
        let collection_id = CollectionId::new(run_id, collection);

        let state = self.state();
        let mut backends = state.backends.write().unwrap();
        let backend =
            backends
                .get_mut(&collection_id)
                .ok_or_else(|| VectorError::CollectionNotFound {
                    name: collection.to_string(),
                })?;

        // Use insert_with_id to maintain VectorId monotonicity
        backend.insert_with_id(vector_id, embedding)?;

        Ok(())
    }

    /// Replay vector deletion from WAL (no WAL write)
    ///
    /// IMPORTANT: This method is for WAL replay during recovery.
    /// It does NOT write to WAL.
    pub fn replay_delete(
        &self,
        run_id: RunId,
        collection: &str,
        _key: &str,
        vector_id: VectorId,
    ) -> VectorResult<()> {
        let collection_id = CollectionId::new(run_id, collection);

        let state = self.state();
        let mut backends = state.backends.write().unwrap();
        if let Some(backend) = backends.get_mut(&collection_id) {
            backend.delete(vector_id)?;
        }
        // Note: If collection doesn't exist, that's OK - it may have been deleted

        Ok(())
    }

    /// Get access to the shared backend state (for recovery/snapshot)
    ///
    /// Returns the shared `VectorBackendState` stored in the Database.
    /// Use `state.backends.read()` or `state.backends.write()` to access backends.
    pub fn backends(&self) -> Arc<VectorBackendState> {
        self.state()
    }

    /// Get access to the database (for snapshot operations)
    pub fn db(&self) -> &Database {
        &self.db
    }

    /// Internal helper to create vector KV key
    pub(crate) fn vector_key_internal(&self, run_id: RunId, collection: &str, key: &str) -> Key {
        Key::new_vector(Namespace::for_run(run_id), collection, key)
    }
}

// ========== Searchable Trait Implementation (M6 Integration, Issue #436) ==========

impl crate::searchable::Searchable for VectorStore {
    /// Vector search via M6 interface
    ///
    /// NOTE: Per M8_ARCHITECTURE.md Section 12.3:
    /// - For SearchMode::Keyword, Vector returns empty results
    /// - Vector does not attempt to do keyword matching on metadata
    /// - For SearchMode::Vector or SearchMode::Hybrid, the caller must
    ///   provide the query embedding via VectorSearchRequest extension
    ///
    /// The hybrid search orchestrator is responsible for:
    /// 1. Embedding the text query (using an external model)
    /// 2. Calling `vector.search_by_embedding()` with the embedding
    /// 3. Fusing results via RRF
    fn search(
        &self,
        req: &strata_core::SearchRequest,
    ) -> strata_core::error::Result<strata_core::SearchResponse> {
        use strata_core::search_types::{SearchMode, SearchResponse, SearchStats};
        use std::time::Instant;

        let start = Instant::now();

        // Vector primitive only responds to Vector or Hybrid mode
        // with an explicit query embedding provided externally.
        //
        // For Keyword mode, return empty - hybrid orchestrator handles this.
        // For Vector/Hybrid mode without embedding, return empty -
        // the hybrid orchestrator should call search_by_embedding() directly.
        match req.mode {
            SearchMode::Keyword => {
                // Vector does NOT do keyword search on metadata
                Ok(SearchResponse::new(
                    vec![],
                    false,
                    SearchStats::new(start.elapsed().as_micros() as u64, 0),
                ))
            }
            SearchMode::Vector | SearchMode::Hybrid => {
                // Requires query embedding - the orchestrator must call
                // search_by_embedding() or search_response() directly
                // with the actual embedding vector.
                Ok(SearchResponse::new(
                    vec![],
                    false,
                    SearchStats::new(start.elapsed().as_micros() as u64, 0),
                ))
            }
        }
    }

    fn primitive_kind(&self) -> strata_core::PrimitiveType {
        strata_core::PrimitiveType::Vector
    }
}

// ========== PrimitiveStorageExt Trait Implementation (Issue #438) ==========

impl strata_storage::PrimitiveStorageExt for VectorStore {
    /// Vector primitive type ID is 7
    fn primitive_type_id(&self) -> u8 {
        strata_storage::primitive_type_ids::VECTOR
    }

    /// Vector WAL entry types: 0x70-0x73
    fn wal_entry_types(&self) -> &'static [u8] {
        &[0x70, 0x71, 0x72, 0x73]
    }

    /// Serialize vector state for snapshot
    ///
    /// Wraps the existing snapshot_serialize method with a Vec<u8> buffer.
    fn snapshot_serialize(&self) -> Result<Vec<u8>, strata_storage::PrimitiveExtError> {
        let mut buffer = Vec::new();
        self.snapshot_serialize(&mut buffer)
            .map_err(|e| strata_storage::PrimitiveExtError::Serialization(e.to_string()))?;
        Ok(buffer)
    }

    /// Deserialize vector state from snapshot
    ///
    /// Wraps the existing snapshot_deserialize method.
    /// Note: Uses interior mutability via the RwLock in VectorBackendState.
    fn snapshot_deserialize(&mut self, data: &[u8]) -> Result<(), strata_storage::PrimitiveExtError> {
        use std::io::Cursor;
        let mut cursor = Cursor::new(data);
        // Note: snapshot_deserialize takes &self and uses interior mutability
        VectorStore::snapshot_deserialize(self, &mut cursor)
            .map_err(|e| strata_storage::PrimitiveExtError::Deserialization(e.to_string()))
    }

    /// Apply a WAL entry during recovery
    ///
    /// Delegates to VectorWalReplayer for actual entry processing.
    /// Note: Uses interior mutability via the RwLock in VectorBackendState.
    fn apply_wal_entry(
        &mut self,
        entry_type: u8,
        payload: &[u8],
    ) -> Result<(), strata_storage::PrimitiveExtError> {
        use crate::vector::wal::VectorWalReplayer;
        use strata_durability::WalEntryType;
        use std::convert::TryFrom;

        let wal_entry_type = WalEntryType::try_from(entry_type).map_err(|_| {
            strata_storage::PrimitiveExtError::UnknownEntryType(entry_type)
        })?;

        let replayer = VectorWalReplayer::new(self);
        replayer
            .apply(wal_entry_type, payload)
            .map_err(|e| strata_storage::PrimitiveExtError::InvalidOperation(e.to_string()))
    }

    /// Primitive name for logging/debugging
    fn primitive_name(&self) -> &'static str {
        "vector"
    }

    /// Rebuild indexes after recovery
    ///
    /// For M8 BruteForce backend, no indexes need rebuilding.
    /// M9 HNSW may need to rebuild graph structure here.
    fn rebuild_indexes(&mut self) -> Result<(), strata_storage::PrimitiveExtError> {
        // BruteForce backend has no derived indexes to rebuild.
        // HNSW (M9) would rebuild the graph here.
        Ok(())
    }
}

/// Get current time in microseconds since Unix epoch
///
/// Returns 0 if system clock is before Unix epoch (clock went backwards).
fn now_micros() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_micros() as u64)
        .unwrap_or(0)
}

// =============================================================================
// VectorStoreExt Implementation (Story #477)
// =============================================================================
//
// Extension trait implementation for cross-primitive transactions.
//
// LIMITATION: VectorStore operations in transactions have limited support
// because embeddings are stored in in-memory backends (VectorHeap/HNSW)
// which are not accessible through TransactionContext. Full vector operations
// require access to the VectorBackendState which is Database-scoped.
//
// Future enhancement: Could add pending_vector_ops to TransactionContext
// and apply them at commit time, but this requires significant infrastructure.

impl VectorStoreExt for TransactionContext {
    fn vector_get(&mut self, collection: &str, key: &str) -> strata_core::Result<Option<Vec<f32>>> {
        // VectorStore embeddings are stored in VectorHeap (in-memory backend),
        // which is not accessible from TransactionContext.
        //
        // The VectorRecord in KV storage only contains vector_id (index into heap),
        // not the actual embedding data.
        //
        // To properly support this, TransactionContext would need access to
        // Database::extension::<VectorBackendState>().
        let _ = (collection, key); // Mark as intentionally unused
        Err(strata_core::error::Error::InvalidOperation(
            "VectorStore get operations are not supported in cross-primitive transactions. \
             Embeddings are stored in in-memory backends not accessible from TransactionContext. \
             Use VectorStore::get() directly outside of transactions."
                .to_string(),
        ))
    }

    fn vector_insert(
        &mut self,
        collection: &str,
        key: &str,
        embedding: &[f32],
    ) -> strata_core::Result<Version> {
        // VectorStore inserts require:
        // 1. Adding embedding to VectorHeap (in-memory)
        // 2. Getting a VectorId from the backend's allocator
        // 3. Creating/updating VectorRecord in KV storage
        // 4. Updating the search index
        //
        // Steps 1, 2, and 4 require access to VectorBackendState which is
        // not available from TransactionContext.
        let _ = (collection, key, embedding); // Mark as intentionally unused
        Err(strata_core::error::Error::InvalidOperation(
            "VectorStore insert operations are not supported in cross-primitive transactions. \
             Vector operations require access to in-memory backends not accessible from \
             TransactionContext. Use VectorStore::insert() directly outside of transactions."
                .to_string(),
        ))
    }
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
        let versioned = store
            .create_collection(run_id, "test", config.clone())
            .unwrap();
        let info = versioned.value;

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

        let info = store.get_collection(run_id, "embeddings").unwrap().unwrap().value;
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
        let info = store.get_collection(run_id, "test").unwrap().unwrap().value;
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

            let info = store.get_collection(run_id, "persistent").unwrap().unwrap().value;
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
        let entry = store.get(run_id, "test", "doc1").unwrap().unwrap().value;
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

        let entry = store.get(run_id, "test", "doc1").unwrap().unwrap().value;
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

        let entry = store.get(run_id, "test", "doc1").unwrap().unwrap().value;
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

    // ========================================
    // WAL Replay Tests (Epic 55 Story #358)
    // ========================================

    #[test]
    fn test_replay_create_collection() {
        let (_temp, _db, store) = setup();
        let run_id = RunId::new();

        let config = VectorConfig::new(3, DistanceMetric::Cosine).unwrap();

        // Replay collection creation
        store
            .replay_create_collection(run_id, "test", config)
            .unwrap();

        // Backend should be created
        let collection_id = CollectionId::new(run_id, "test");
        assert!(store.backends().backends.read().unwrap().contains_key(&collection_id));
    }

    #[test]
    fn test_replay_delete_collection() {
        let (_temp, _db, store) = setup();
        let run_id = RunId::new();

        let config = VectorConfig::new(3, DistanceMetric::Cosine).unwrap();
        store
            .replay_create_collection(run_id, "test", config)
            .unwrap();

        let collection_id = CollectionId::new(run_id, "test");
        assert!(store.backends().backends.read().unwrap().contains_key(&collection_id));

        // Replay deletion
        store.replay_delete_collection(run_id, "test").unwrap();

        assert!(!store.backends().backends.read().unwrap().contains_key(&collection_id));
    }

    #[test]
    fn test_replay_upsert() {
        let (_temp, _db, store) = setup();
        let run_id = RunId::new();

        let config = VectorConfig::new(3, DistanceMetric::Cosine).unwrap();
        store
            .replay_create_collection(run_id, "test", config)
            .unwrap();

        // Replay upsert with specific VectorId
        let vector_id = VectorId::new(42);
        store
            .replay_upsert(run_id, "test", "doc1", vector_id, &[1.0, 0.0, 0.0], None)
            .unwrap();

        // Verify vector exists in backend
        let collection_id = CollectionId::new(run_id, "test");
        let state = store.backends();
        let backends = state.backends.read().unwrap();
        let backend = backends.get(&collection_id).unwrap();
        assert!(backend.contains(vector_id));
        assert_eq!(backend.len(), 1);
    }

    #[test]
    fn test_replay_upsert_maintains_id_monotonicity() {
        let (_temp, _db, store) = setup();
        let run_id = RunId::new();

        let config = VectorConfig::new(3, DistanceMetric::Cosine).unwrap();
        store
            .replay_create_collection(run_id, "test", config)
            .unwrap();

        // Replay upsert with high VectorId
        let high_id = VectorId::new(1000);
        store
            .replay_upsert(run_id, "test", "doc", high_id, &[1.0, 0.0, 0.0], None)
            .unwrap();

        // Verify the vector exists
        let collection_id = CollectionId::new(run_id, "test");
        let state = store.backends();
        let backends = state.backends.read().unwrap();
        let backend = backends.get(&collection_id).unwrap();
        assert!(backend.contains(high_id));
    }

    #[test]
    fn test_replay_delete() {
        let (_temp, _db, store) = setup();
        let run_id = RunId::new();

        let config = VectorConfig::new(3, DistanceMetric::Cosine).unwrap();
        store
            .replay_create_collection(run_id, "test", config)
            .unwrap();

        // Replay upsert
        let vector_id = VectorId::new(1);
        store
            .replay_upsert(run_id, "test", "doc", vector_id, &[1.0, 0.0, 0.0], None)
            .unwrap();

        let collection_id = CollectionId::new(run_id, "test");
        {
            let state = store.backends();
            let backends = state.backends.read().unwrap();
            assert!(backends.get(&collection_id).unwrap().contains(vector_id));
        }

        // Replay deletion
        store
            .replay_delete(run_id, "test", "doc", vector_id)
            .unwrap();

        {
            let state = store.backends();
            let backends = state.backends.read().unwrap();
            assert!(!backends.get(&collection_id).unwrap().contains(vector_id));
        }
    }

    #[test]
    fn test_replay_delete_missing_collection() {
        let (_temp, _db, store) = setup();
        let run_id = RunId::new();

        // Replay delete on non-existent collection should succeed (idempotent)
        let result = store.replay_delete(run_id, "nonexistent", "doc", VectorId::new(1));
        assert!(result.is_ok());
    }

    #[test]
    fn test_replay_sequence() {
        let (_temp, _db, store) = setup();
        let run_id = RunId::new();

        let config = VectorConfig::new(3, DistanceMetric::Cosine).unwrap();

        // Replay a sequence of operations
        store
            .replay_create_collection(run_id, "col1", config.clone())
            .unwrap();

        store
            .replay_upsert(
                run_id,
                "col1",
                "v1",
                VectorId::new(1),
                &[1.0, 0.0, 0.0],
                None,
            )
            .unwrap();

        store
            .replay_upsert(
                run_id,
                "col1",
                "v2",
                VectorId::new(2),
                &[0.0, 1.0, 0.0],
                None,
            )
            .unwrap();

        store
            .replay_delete(run_id, "col1", "v1", VectorId::new(1))
            .unwrap();

        // Verify final state
        let collection_id = CollectionId::new(run_id, "col1");
        let state = store.backends();
        let backends = state.backends.read().unwrap();
        let backend = backends.get(&collection_id).unwrap();

        assert!(!backend.contains(VectorId::new(1)));
        assert!(backend.contains(VectorId::new(2)));
        assert_eq!(backend.len(), 1);
    }

    #[test]
    fn test_backends_accessor() {
        let (_temp, _db, store) = setup();
        let run_id = RunId::new();

        let config = VectorConfig::new(3, DistanceMetric::Cosine).unwrap();
        store.create_collection(run_id, "test", config).unwrap();

        // Use backends accessor
        let state = store.backends();
        let guard = state.backends.read().unwrap();
        assert_eq!(guard.len(), 1);
    }
}
