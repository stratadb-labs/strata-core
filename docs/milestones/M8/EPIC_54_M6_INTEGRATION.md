# Epic 54: M6 Search Integration

**Goal**: Integrate vector search with M6 retrieval surfaces

**Dependencies**: Epic 52 (Index Backend), Epic 53 (Collection Management)

---

## Scope

- VectorStore facade with insert/get/delete/search
- search() with metadata filtering
- search_request() for SearchRequest/SearchResponse compatibility
- DocRef::Vector variant for hybrid search
- RRF fusion with keyword + vector results
- Vector Searchable implementation

---

## User Stories

| Story | Description | Priority |
|-------|-------------|----------|
| #351 | VectorStore Facade Implementation | CRITICAL |
| #352 | search() Method with Metadata Filtering | CRITICAL |
| #353 | search_request() for SearchRequest/SearchResponse | CRITICAL |
| #354 | DocRef::Vector Variant | HIGH |
| #355 | RRF Hybrid Search Fusion | CRITICAL |
| #356 | Vector Searchable Implementation | HIGH |

---

## Story #351: VectorStore Facade Implementation

**File**: `crates/primitives/src/vector/store.rs` (NEW)

**Deliverable**: Stateless facade over Database

### Implementation

```rust
use std::sync::{Arc, RwLock};
use std::collections::HashMap;

use crate::core::{Database, RunId, Namespace, Key, TypeTag};
use crate::vector::{
    VectorConfig, VectorEntry, VectorMatch, VectorId, VectorError,
    VectorRecord, CollectionId, CollectionInfo, MetadataFilter,
    VectorIndexBackend, BruteForceBackend, IndexBackendFactory,
    validate_collection_name, validate_vector_key,
};

/// Stateless facade for vector operations
///
/// IMPORTANT: This struct follows Rule 1 (Stateless Facade Pattern).
/// All persistent state lives in Database. The backends map is an
/// in-memory cache that can be reconstructed from Database state.
///
/// Multiple VectorStore instances pointing to the same Database are safe.
pub struct VectorStore {
    /// Reference to the database
    db: Arc<Database>,

    /// In-memory index backends (cache, reconstructible from DB)
    /// Key: CollectionId, Value: Box<dyn VectorIndexBackend>
    backends: RwLock<HashMap<CollectionId, Box<dyn VectorIndexBackend>>>,

    /// Factory for creating index backends
    backend_factory: IndexBackendFactory,
}

impl VectorStore {
    /// Create a new VectorStore facade
    pub fn new(db: Arc<Database>) -> Self {
        VectorStore {
            db,
            backends: RwLock::new(HashMap::new()),
            backend_factory: IndexBackendFactory::default(),
        }
    }

    /// Create with a specific backend factory
    pub fn with_factory(db: Arc<Database>, factory: IndexBackendFactory) -> Self {
        VectorStore {
            db,
            backends: RwLock::new(HashMap::new()),
            backend_factory: factory,
        }
    }

    /// Insert a vector (upsert semantics)
    ///
    /// If a vector with this key already exists, it is overwritten.
    /// This follows Rule 3 (Upsert Semantics).
    pub fn insert(
        &self,
        run_id: RunId,
        collection: &str,
        key: &str,
        embedding: &[f32],
        metadata: Option<serde_json::Value>,
    ) -> Result<(), VectorError> {
        // Validate inputs
        validate_vector_key(key)?;

        // Ensure collection is loaded
        self.ensure_collection_loaded(run_id, collection)?;

        let collection_id = CollectionId::new(run_id, collection);

        // Get or allocate VectorId
        let (vector_id, is_update) = self.get_or_allocate_vector_id(
            &collection_id,
            key,
        )?;

        // Validate dimension
        let config = self.get_collection_config_required(run_id, collection)?;
        if embedding.len() != config.dimension {
            return Err(VectorError::DimensionMismatch {
                expected: config.dimension,
                got: embedding.len(),
            });
        }

        // Update backend
        {
            let mut backends = self.backends.write().unwrap();
            let backend = backends.get_mut(&collection_id)
                .ok_or_else(|| VectorError::CollectionNotFound {
                    name: collection.to_string(),
                })?;
            backend.insert(vector_id, embedding)?;
        }

        // Update KV metadata
        let record = if is_update {
            let mut record = self.get_vector_record(run_id, collection, key)?
                .ok_or_else(|| VectorError::Internal("Record missing".to_string()))?;
            record.update(metadata);
            record
        } else {
            VectorRecord::new(vector_id, metadata)
        };

        let kv_key = Key::new_vector(Namespace::from_run_id(run_id), collection, key);
        self.db.kv_put(&kv_key, &record.to_bytes()?)?;

        // Write WAL
        self.write_wal_upsert(run_id, collection, key, vector_id, embedding, &record.metadata)?;

        Ok(())
    }

    /// Get a vector by key
    pub fn get(
        &self,
        run_id: RunId,
        collection: &str,
        key: &str,
    ) -> Result<Option<VectorEntry>, VectorError> {
        self.ensure_collection_loaded(run_id, collection)?;

        let collection_id = CollectionId::new(run_id, collection);

        // Get record from KV
        let Some(record) = self.get_vector_record(run_id, collection, key)? else {
            return Ok(None);
        };

        let vector_id = record.vector_id();

        // Get embedding from backend
        let backends = self.backends.read().unwrap();
        let backend = backends.get(&collection_id)
            .ok_or_else(|| VectorError::CollectionNotFound {
                name: collection.to_string(),
            })?;

        let embedding = backend.get(vector_id)
            .ok_or_else(|| VectorError::Internal(
                "Embedding missing from backend".to_string()
            ))?;

        Ok(Some(VectorEntry {
            key: key.to_string(),
            embedding: embedding.to_vec(),
            metadata: record.metadata,
            vector_id,
            version: record.version,
        }))
    }

    /// Delete a vector by key
    pub fn delete(
        &self,
        run_id: RunId,
        collection: &str,
        key: &str,
    ) -> Result<bool, VectorError> {
        self.ensure_collection_loaded(run_id, collection)?;

        let collection_id = CollectionId::new(run_id, collection);

        // Get existing record
        let Some(record) = self.get_vector_record(run_id, collection, key)? else {
            return Ok(false);
        };

        let vector_id = record.vector_id();

        // Delete from backend
        {
            let mut backends = self.backends.write().unwrap();
            let backend = backends.get_mut(&collection_id)
                .ok_or_else(|| VectorError::CollectionNotFound {
                    name: collection.to_string(),
                })?;
            backend.delete(vector_id)?;
        }

        // Delete from KV
        let kv_key = Key::new_vector(Namespace::from_run_id(run_id), collection, key);
        self.db.kv_delete(&kv_key)?;

        // Write WAL
        self.write_wal_delete(run_id, collection, key, vector_id)?;

        Ok(true)
    }

    /// Get count of vectors in a collection
    pub fn count(
        &self,
        run_id: RunId,
        collection: &str,
    ) -> Result<usize, VectorError> {
        self.ensure_collection_loaded(run_id, collection)?;

        let collection_id = CollectionId::new(run_id, collection);
        let backends = self.backends.read().unwrap();

        backends.get(&collection_id)
            .map(|b| b.len())
            .ok_or_else(|| VectorError::CollectionNotFound {
                name: collection.to_string(),
            })
    }
}

// Clone is safe because VectorStore is stateless (points to shared Database)
impl Clone for VectorStore {
    fn clone(&self) -> Self {
        VectorStore {
            db: self.db.clone(),
            backends: RwLock::new(HashMap::new()), // Each clone gets fresh cache
            backend_factory: self.backend_factory.clone(),
        }
    }
}
```

### Acceptance Criteria

- [ ] VectorStore is stateless (Rule 1)
- [ ] insert() with upsert semantics (Rule 3)
- [ ] get() returns VectorEntry with embedding and metadata
- [ ] delete() returns bool indicating if vector existed
- [ ] count() returns number of vectors
- [ ] All operations validate collection exists
- [ ] All operations write to WAL
- [ ] Clone is safe (multiple instances OK)

---

## Story #352: search() Method with Metadata Filtering

**File**: `crates/primitives/src/vector/store.rs`

**Deliverable**: Search with post-filtering

### Implementation

```rust
impl VectorStore {
    /// Search for similar vectors
    ///
    /// Returns top-k vectors most similar to the query.
    /// Metadata filtering is applied as post-filter (not pre-filter).
    ///
    /// INVARIANTS SATISFIED:
    /// - R1: Dimension validated against collection config
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
    ) -> Result<Vec<VectorMatch>, VectorError> {
        // Validate k
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
            let backend = backends.get(&collection_id)
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
            let (key, metadata) = self.get_key_and_metadata(
                run_id,
                collection,
                vector_id,
            )?;

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
            b.score.partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.key.cmp(&b.key))
        });

        // Ensure we don't exceed k after sorting
        matches.truncate(k);

        Ok(matches)
    }

    /// Get key and metadata for a VectorId
    fn get_key_and_metadata(
        &self,
        run_id: RunId,
        collection: &str,
        vector_id: VectorId,
    ) -> Result<(String, Option<serde_json::Value>), VectorError> {
        // We need to find the key for this VectorId
        // This is inefficient in M8 (scan), but works correctly
        // M9 can add a reverse index if needed

        let namespace = Namespace::from_run_id(run_id);
        let prefix = Key::vector_collection_prefix(namespace, collection);

        let entries: Vec<(Key, Vec<u8>)> = self.db.scan_with_prefix(&prefix)?;

        for (key, value) in entries {
            let record = VectorRecord::from_bytes(&value)?;
            if record.vector_id() == vector_id {
                let user_key = key.user_key();
                // Extract key from "collection/key" format
                let vector_key = user_key.strip_prefix(&format!("{}/", collection))
                    .unwrap_or(user_key)
                    .to_string();
                return Ok((vector_key, record.metadata));
            }
        }

        Err(VectorError::Internal(format!(
            "VectorId {:?} not found in KV",
            vector_id
        )))
    }

    /// Convenience method: search without filter
    pub fn search_simple(
        &self,
        run_id: RunId,
        collection: &str,
        query: &[f32],
        k: usize,
    ) -> Result<Vec<VectorMatch>, VectorError> {
        self.search(run_id, collection, query, k, None)
    }
}
```

### Acceptance Criteria

- [ ] Validates query dimension against config
- [ ] Calls backend.search() for similarity computation
- [ ] Over-fetches when filter is present
- [ ] Applies MetadataFilter as post-filter
- [ ] Sorts by (score desc, key asc) at facade level
- [ ] Returns up to k results
- [ ] Search is read-only (Invariant R10)

---

## Story #353: search_request() for SearchRequest/SearchResponse

**File**: `crates/primitives/src/vector/store.rs`

**Deliverable**: M6-compatible search interface

### Implementation

```rust
use crate::search::{SearchRequest, SearchResponse, SearchResult, SearchMode};

impl VectorStore {
    /// Execute a SearchRequest and return SearchResponse
    ///
    /// This provides M6 compatibility, allowing vector search to
    /// participate in composite queries.
    pub fn search_request(
        &self,
        run_id: RunId,
        request: &SearchRequest,
    ) -> Result<SearchResponse, VectorError> {
        // Vector only handles Semantic mode
        // For Keyword or Hybrid, return empty (keyword handled elsewhere)
        if request.mode == SearchMode::Keyword {
            return Ok(SearchResponse::empty());
        }

        // Extract vector query
        let Some(ref query_embedding) = request.query_embedding else {
            return Err(VectorError::EmptyEmbedding);
        };

        // Get collection from request
        let collection = request.collection.as_ref()
            .ok_or_else(|| VectorError::InvalidCollectionName {
                name: "".to_string(),
                reason: "Collection required for vector search".to_string(),
            })?;

        // Convert request filter to MetadataFilter
        let filter = request.filter.as_ref().map(|f| {
            self.convert_search_filter(f)
        }).transpose()?;

        // Execute search
        let matches = self.search(
            run_id,
            collection,
            query_embedding,
            request.limit.unwrap_or(10),
            filter,
        )?;

        // Convert to SearchResponse
        let results: Vec<SearchResult> = matches
            .into_iter()
            .enumerate()
            .map(|(rank, m)| SearchResult {
                doc_ref: DocRef::Vector {
                    collection: collection.clone(),
                    key: m.key,
                },
                score: m.score,
                rank: rank + 1,
                metadata: m.metadata,
            })
            .collect();

        Ok(SearchResponse {
            results,
            total_count: None, // Not computed for vector search
            truncated: false,
        })
    }

    /// Convert M6 filter to MetadataFilter
    fn convert_search_filter(
        &self,
        filter: &SearchFilter,
    ) -> Result<MetadataFilter, VectorError> {
        let mut meta_filter = MetadataFilter::new();

        for (key, value) in &filter.equals {
            let scalar = match value {
                serde_json::Value::Null => JsonScalar::Null,
                serde_json::Value::Bool(b) => JsonScalar::Bool(*b),
                serde_json::Value::Number(n) => {
                    JsonScalar::Number(n.as_f64().unwrap_or(0.0))
                }
                serde_json::Value::String(s) => JsonScalar::String(s.clone()),
                _ => {
                    return Err(VectorError::InvalidKey {
                        key: key.clone(),
                        reason: "Filter value must be scalar".to_string(),
                    });
                }
            };
            meta_filter = meta_filter.eq(key.clone(), scalar);
        }

        Ok(meta_filter)
    }
}
```

### Acceptance Criteria

- [ ] Accepts SearchRequest, returns SearchResponse
- [ ] Returns empty for SearchMode::Keyword
- [ ] Extracts query_embedding from request
- [ ] Converts SearchFilter to MetadataFilter
- [ ] Returns SearchResult with DocRef::Vector
- [ ] Includes rank in results

---

## Story #354: DocRef::Vector Variant

**File**: `crates/search/src/doc_ref.rs`

**Deliverable**: DocRef variant for vector results

### Implementation

```rust
/// Reference to a document in search results
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum DocRef {
    /// KV store entry
    Kv { key: String },

    /// JSON document
    Json { key: String },

    /// Event log entry
    Event { run_id: RunId, sequence: u64 },

    /// State cell
    State { key: String },

    /// Vector entry (M8 addition)
    Vector {
        /// Collection name
        collection: String,
        /// Vector key within collection
        key: String,
    },
}

impl DocRef {
    /// Create a vector document reference
    pub fn vector(collection: impl Into<String>, key: impl Into<String>) -> Self {
        DocRef::Vector {
            collection: collection.into(),
            key: key.into(),
        }
    }

    /// Get the primitive type for this reference
    pub fn primitive_type(&self) -> &'static str {
        match self {
            DocRef::Kv { .. } => "kv",
            DocRef::Json { .. } => "json",
            DocRef::Event { .. } => "event",
            DocRef::State { .. } => "state",
            DocRef::Vector { .. } => "vector",
        }
    }

    /// Check if this is a vector reference
    pub fn is_vector(&self) -> bool {
        matches!(self, DocRef::Vector { .. })
    }

    /// Get the key (for comparison and deduplication)
    pub fn canonical_key(&self) -> String {
        match self {
            DocRef::Kv { key } => format!("kv:{}", key),
            DocRef::Json { key } => format!("json:{}", key),
            DocRef::Event { run_id, sequence } => format!("event:{}:{}", run_id, sequence),
            DocRef::State { key } => format!("state:{}", key),
            DocRef::Vector { collection, key } => format!("vector:{}:{}", collection, key),
        }
    }
}
```

### Acceptance Criteria

- [ ] DocRef::Vector with collection and key fields
- [ ] vector() constructor method
- [ ] primitive_type() returns "vector"
- [ ] is_vector() helper
- [ ] canonical_key() includes collection and key

---

## Story #355: RRF Hybrid Search Fusion

**File**: `crates/search/src/hybrid.rs`

**Deliverable**: Reciprocal Rank Fusion with vector results

### Implementation

```rust
use std::collections::HashMap;

/// Reciprocal Rank Fusion constant
/// Higher k reduces the impact of high rankings
const RRF_K: f32 = 60.0;

/// Fuse results from multiple search modalities using RRF
///
/// RRF formula: score = sum(1 / (k + rank)) for each list containing the doc
///
/// This handles:
/// - Keyword (BM25) results
/// - Vector (semantic) results
/// - Multiple vector collections
pub fn rrf_fusion(
    result_lists: Vec<Vec<SearchResult>>,
) -> Vec<SearchResult> {
    // Accumulate RRF scores by DocRef
    let mut scores: HashMap<String, (DocRef, f32, Option<serde_json::Value>)> = HashMap::new();

    for results in result_lists {
        for result in results {
            let key = result.doc_ref.canonical_key();
            let rrf_score = 1.0 / (RRF_K + result.rank as f32);

            scores.entry(key.clone())
                .and_modify(|(_, score, _)| *score += rrf_score)
                .or_insert((result.doc_ref.clone(), rrf_score, result.metadata.clone()));
        }
    }

    // Sort by fused score (descending)
    let mut fused: Vec<_> = scores.into_values().collect();
    fused.sort_by(|a, b| {
        b.1.partial_cmp(&a.1)
            .unwrap_or(std::cmp::Ordering::Equal)
            // Tie-break by canonical key for determinism
            .then_with(|| a.0.canonical_key().cmp(&b.0.canonical_key()))
    });

    // Convert to SearchResult with new ranks
    fused
        .into_iter()
        .enumerate()
        .map(|(i, (doc_ref, score, metadata))| SearchResult {
            doc_ref,
            score,
            rank: i + 1,
            metadata,
        })
        .collect()
}

/// Execute hybrid search (keyword + vector)
pub fn hybrid_search(
    db: &Database,
    run_id: RunId,
    request: &SearchRequest,
) -> Result<SearchResponse, SearchError> {
    let mut result_lists = Vec::new();

    // Keyword search (if text query provided)
    if let Some(ref text_query) = request.query_text {
        if request.mode != SearchMode::Semantic {
            let keyword_results = keyword_search(db, run_id, text_query, request)?;
            result_lists.push(keyword_results.results);
        }
    }

    // Vector search (if embedding provided)
    if let Some(ref embedding) = request.query_embedding {
        if request.mode != SearchMode::Keyword {
            let vector_store = db.vector_store();
            let vector_response = vector_store.search_request(run_id, request)?;
            result_lists.push(vector_response.results);
        }
    }

    // Fuse results
    let fused = rrf_fusion(result_lists);

    // Apply limit
    let limit = request.limit.unwrap_or(10);
    let truncated = fused.len() > limit;
    let results: Vec<_> = fused.into_iter().take(limit).collect();

    Ok(SearchResponse {
        results,
        total_count: None,
        truncated,
    })
}
```

### Acceptance Criteria

- [ ] RRF formula: 1/(k + rank) with k=60
- [ ] Handles multiple result lists
- [ ] Deduplicates by DocRef.canonical_key()
- [ ] Sorts by fused score descending
- [ ] Deterministic tie-breaking by canonical key
- [ ] hybrid_search() combines keyword and vector
- [ ] Respects SearchMode (Keyword/Semantic/Hybrid)

---

## Story #356: Vector Searchable Implementation

**File**: `crates/search/src/searchable.rs`

**Deliverable**: Searchable trait impl for Vector

### Implementation

```rust
use crate::search::{Searchable, SearchRequest, SearchResponse, SearchMode};
use crate::vector::VectorStore;

/// Searchable implementation for Vector primitive
///
/// IMPORTANT: Vector does NOT do keyword search natively.
/// For SearchMode::Keyword, returns empty results.
/// Keyword search on vector metadata would require a separate index.
impl Searchable for VectorStore {
    fn search(
        &self,
        run_id: RunId,
        request: &SearchRequest,
    ) -> Result<SearchResponse, SearchError> {
        // Vector only handles Semantic mode
        // Keyword mode returns empty (handled by other primitives)
        match request.mode {
            SearchMode::Keyword => {
                // Vector does NOT participate in keyword search
                // Return empty - keyword search is handled by KV/JSON
                Ok(SearchResponse::empty())
            }
            SearchMode::Semantic | SearchMode::Hybrid => {
                // For Hybrid, we return semantic results
                // RRF fusion happens at a higher level
                self.search_request(run_id, request)
                    .map_err(SearchError::from)
            }
        }
    }

    fn primitive_type(&self) -> &'static str {
        "vector"
    }

    fn supports_mode(&self, mode: SearchMode) -> bool {
        match mode {
            SearchMode::Keyword => false,  // Vector doesn't do keyword search
            SearchMode::Semantic => true,
            SearchMode::Hybrid => true,    // Contributes semantic part
        }
    }
}

impl From<VectorError> for SearchError {
    fn from(e: VectorError) -> Self {
        SearchError::Primitive {
            primitive: "vector".to_string(),
            message: e.to_string(),
        }
    }
}
```

### Acceptance Criteria

- [ ] Implements Searchable trait
- [ ] Returns empty for SearchMode::Keyword
- [ ] Returns results for SearchMode::Semantic
- [ ] Returns results for SearchMode::Hybrid
- [ ] supports_mode() returns false for Keyword
- [ ] VectorError converts to SearchError

---

## Testing

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vector_search_basic() {
        let db = test_db();
        let store = VectorStore::new(db);
        let run_id = RunId::new();

        // Create collection and insert vectors
        let config = VectorConfig::new(3, DistanceMetric::Cosine).unwrap();
        store.create_collection(run_id, "test", config).unwrap();

        store.insert(run_id, "test", "a", &[1.0, 0.0, 0.0], None).unwrap();
        store.insert(run_id, "test", "b", &[0.0, 1.0, 0.0], None).unwrap();
        store.insert(run_id, "test", "c", &[0.9, 0.1, 0.0], None).unwrap();

        // Search
        let query = [1.0, 0.0, 0.0];
        let results = store.search(run_id, "test", &query, 2, None).unwrap();

        assert_eq!(results.len(), 2);
        assert_eq!(results[0].key, "a"); // Most similar
        assert_eq!(results[1].key, "c"); // Second most similar
    }

    #[test]
    fn test_metadata_filtering() {
        let db = test_db();
        let store = VectorStore::new(db);
        let run_id = RunId::new();

        let config = VectorConfig::new(3, DistanceMetric::Cosine).unwrap();
        store.create_collection(run_id, "test", config).unwrap();

        store.insert(run_id, "test", "a", &[1.0, 0.0, 0.0],
            Some(json!({"type": "document"}))).unwrap();
        store.insert(run_id, "test", "b", &[0.9, 0.1, 0.0],
            Some(json!({"type": "image"}))).unwrap();
        store.insert(run_id, "test", "c", &[0.8, 0.2, 0.0],
            Some(json!({"type": "document"}))).unwrap();

        // Filter by type
        let filter = MetadataFilter::new().eq("type", "document");
        let results = store.search(run_id, "test", &[1.0, 0.0, 0.0], 10, Some(filter)).unwrap();

        assert_eq!(results.len(), 2);
        assert!(results.iter().all(|r| {
            r.metadata.as_ref()
                .and_then(|m| m.get("type"))
                .and_then(|v| v.as_str())
                == Some("document")
        }));
    }

    #[test]
    fn test_rrf_fusion() {
        // Keyword results
        let keyword = vec![
            SearchResult { doc_ref: DocRef::Kv { key: "a".into() }, score: 0.9, rank: 1, metadata: None },
            SearchResult { doc_ref: DocRef::Kv { key: "b".into() }, score: 0.8, rank: 2, metadata: None },
            SearchResult { doc_ref: DocRef::Kv { key: "c".into() }, score: 0.7, rank: 3, metadata: None },
        ];

        // Vector results (different order)
        let vector = vec![
            SearchResult { doc_ref: DocRef::Kv { key: "b".into() }, score: 0.95, rank: 1, metadata: None },
            SearchResult { doc_ref: DocRef::Kv { key: "a".into() }, score: 0.85, rank: 2, metadata: None },
            SearchResult { doc_ref: DocRef::Kv { key: "d".into() }, score: 0.75, rank: 3, metadata: None },
        ];

        let fused = rrf_fusion(vec![keyword, vector]);

        // "a" and "b" appear in both lists, should have higher fused scores
        // "a": 1/(60+1) + 1/(60+2) ≈ 0.0164 + 0.0161 = 0.0325
        // "b": 1/(60+2) + 1/(60+1) ≈ 0.0161 + 0.0164 = 0.0325
        // "c": 1/(60+3) ≈ 0.0159
        // "d": 1/(60+3) ≈ 0.0159

        assert!(fused.len() == 4);
        // Top 2 should be "a" and "b" (in some order, tied)
        let top_keys: Vec<_> = fused.iter().take(2).map(|r| r.doc_ref.canonical_key()).collect();
        assert!(top_keys.contains(&"kv:a".to_string()));
        assert!(top_keys.contains(&"kv:b".to_string()));
    }

    #[test]
    fn test_search_mode_handling() {
        let db = test_db();
        let store = VectorStore::new(db);
        let run_id = RunId::new();

        let config = VectorConfig::new(3, DistanceMetric::Cosine).unwrap();
        store.create_collection(run_id, "test", config).unwrap();
        store.insert(run_id, "test", "a", &[1.0, 0.0, 0.0], None).unwrap();

        // Keyword mode returns empty
        let request = SearchRequest {
            mode: SearchMode::Keyword,
            query_text: Some("test".into()),
            query_embedding: Some(vec![1.0, 0.0, 0.0]),
            collection: Some("test".into()),
            limit: Some(10),
            filter: None,
        };

        let response = store.search_request(run_id, &request).unwrap();
        assert!(response.results.is_empty());

        // Semantic mode returns results
        let request = SearchRequest {
            mode: SearchMode::Semantic,
            ..request
        };

        let response = store.search_request(run_id, &request).unwrap();
        assert_eq!(response.results.len(), 1);
    }
}
```

---

## Files Modified/Created

| File | Action |
|------|--------|
| `crates/primitives/src/vector/store.rs` | CREATE - VectorStore facade |
| `crates/primitives/src/vector/search.rs` | CREATE - Search implementation |
| `crates/search/src/doc_ref.rs` | MODIFY - Add DocRef::Vector |
| `crates/search/src/hybrid.rs` | MODIFY - Add RRF fusion with vectors |
| `crates/search/src/searchable.rs` | MODIFY - Add Vector Searchable impl |
| `crates/engine/src/database.rs` | MODIFY - Add vector_store() method |
