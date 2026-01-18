# Epic 54: Search Integration - Implementation Prompts

**Epic Goal**: Integrate vector search with M6 retrieval surfaces

**GitHub Issue**: [#392](https://github.com/anibjoshi/in-mem/issues/392)
**Status**: Ready after Epic 52 and Epic 53
**Dependencies**: Epic 52 (Index Backend), Epic 53 (Collection Management)

---

## AUTHORITATIVE SPECIFICATIONS - READ THESE FIRST

**`docs/architecture/M8_ARCHITECTURE.md` is THE AUTHORITATIVE SPEC.**

### IMPORTANT: Naming Convention

**Do NOT use "M8" or "m8" in the codebase or comments.** M8 is an internal milestone indicator only. In code, use "Vector" prefix instead:
- Module names: `vector`, `store`, `searchable`, `fusion`
- Type names: `VectorStore`, `VectorMatch`, `DocRef::Vector`
- Test names: `test_vector_*`, `test_search_*`, not `test_m8_*`
- Comments: "Vector search" not "M8 search"

Before starting ANY story in this epic, read:
1. **Architecture Spec (AUTHORITATIVE)**: `docs/architecture/M8_ARCHITECTURE.md`
2. **Epic Spec**: `docs/milestones/M8/EPIC_54_M6_INTEGRATION.md`
3. **Prompt Header**: `docs/prompts/M8/M8_PROMPT_HEADER.md` for the 7 architectural rules

---

## Epic 54 Overview

### Scope
- VectorStore facade with insert/get/delete/search
- search() with metadata filtering
- search_request() for SearchRequest/SearchResponse compatibility
- DocRef::Vector variant for hybrid search
- RRF fusion with keyword + vector results
- Vector Searchable implementation

### Critical Invariants

| Invariant | Description |
|-----------|-------------|
| **R5** | Facade tie-break: score desc, key asc |
| **R10** | Search is READ-ONLY (no WAL writes, no counters, no caches) |
| **Rule 1** | Stateless Facade Pattern |

### Component Breakdown
- **Story #415**: VectorStore Facade Implementation - CRITICAL
- **Story #416**: search() Method with Metadata Filtering - CRITICAL
- **Story #417**: search_request() for SearchRequest/SearchResponse - CRITICAL
- **Story #418**: DocRef::Vector Variant - HIGH
- **Story #419**: RRF Hybrid Search Fusion - CRITICAL
- **Story #420**: Vector Searchable Implementation - HIGH

---

## Story #415: VectorStore Facade Implementation

**GitHub Issue**: [#415](https://github.com/anibjoshi/in-mem/issues/415)
**Estimated Time**: 3 hours
**Dependencies**: Epic 52, Epic 53
**Blocks**: #416, #417

### Start Story

```bash
gh issue view 415
./scripts/start-story.sh 54 415 vector-store-facade
```

### Implementation

Extend `crates/primitives/src/vector/store.rs`:

```rust
use serde_json::Value as JsonValue;

impl VectorStore {
    /// Insert a vector (upsert semantics per Rule 3)
    ///
    /// If a vector with this key already exists, it is overwritten.
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

        let collection_id = CollectionId::new(run_id.clone(), collection);

        // Get collection config to validate dimension
        let config = self.get_collection_config(&collection_id)?
            .ok_or_else(|| VectorError::CollectionNotFound {
                name: collection.to_string(),
            })?;

        // Validate dimension
        if embedding.len() != config.dimension {
            return Err(VectorError::DimensionMismatch {
                expected: config.dimension,
                got: embedding.len(),
            });
        }

        // Check if key already exists
        let existing_record = self.get_vector_record(&collection_id, key)?;

        let vector_id = if let Some(record) = existing_record {
            // Upsert: reuse existing VectorId
            record.vector_id
        } else {
            // New insert: allocate VectorId from backend
            self.allocate_vector_id(&collection_id)?
        };

        // Update backend
        {
            let mut backends = self.backends.write().unwrap();
            let backend = backends.get_mut(&collection_id)
                .ok_or_else(|| VectorError::CollectionNotFound {
                    name: collection.to_string(),
                })?;
            backend.insert(vector_id, embedding)?;
        }

        // Create/update record in KV
        let record = VectorRecord::new(key.to_string(), vector_id, metadata);
        let record_key = format!("{}:{}", collection_id.to_key_string(), key);
        self.db.kv_put_with_tag(
            TypeTag::VectorRecord,
            &record_key,
            &record.to_bytes()?,
        )?;

        // Log WAL entry: UPSERT
        // self.db.log_wal_entry(WalEntryType::VectorUpsert, ...)?;

        Ok(())
    }

    /// Get a vector by key
    pub fn get(
        &self,
        run_id: RunId,
        collection: &str,
        key: &str,
    ) -> VectorResult<Option<VectorEntry>> {
        let collection_id = CollectionId::new(run_id, collection);

        // Get record from KV
        let record = self.get_vector_record(&collection_id, key)?;
        let Some(record) = record else {
            return Ok(None);
        };

        // Get embedding from backend
        let embedding = {
            let backends = self.backends.read().unwrap();
            let backend = backends.get(&collection_id)
                .ok_or_else(|| VectorError::CollectionNotFound {
                    name: collection.to_string(),
                })?;

            backend.get(record.vector_id)
                .map(|e| e.to_vec())
                .ok_or_else(|| VectorError::Internal(
                    "Record exists but embedding missing".into()
                ))?
        };

        Ok(Some(VectorEntry {
            key: record.key,
            embedding,
            metadata: record.metadata,
            vector_id: record.vector_id,
            version: record.version,
        }))
    }

    /// Delete a vector by key
    pub fn delete(
        &self,
        run_id: RunId,
        collection: &str,
        key: &str,
    ) -> VectorResult<bool> {
        let collection_id = CollectionId::new(run_id, collection);

        // Get record to find VectorId
        let record = self.get_vector_record(&collection_id, key)?;
        let Some(record) = record else {
            return Ok(false);
        };

        // Delete from backend
        {
            let mut backends = self.backends.write().unwrap();
            if let Some(backend) = backends.get_mut(&collection_id) {
                backend.delete(record.vector_id)?;
            }
        }

        // Delete record from KV
        let record_key = format!("{}:{}", collection_id.to_key_string(), key);
        self.db.kv_delete_with_tag(TypeTag::VectorRecord, &record_key)?;

        // Log WAL entry: DELETE
        // self.db.log_wal_entry(WalEntryType::VectorDelete, ...)?;

        Ok(true)
    }

    /// Get vector record from KV
    fn get_vector_record(
        &self,
        collection_id: &CollectionId,
        key: &str,
    ) -> VectorResult<Option<VectorRecord>> {
        let record_key = format!("{}:{}", collection_id.to_key_string(), key);
        let value = self.db.kv_get_with_tag(TypeTag::VectorRecord, &record_key)?;

        match value {
            Some(bytes) => Ok(Some(VectorRecord::from_bytes(&bytes)?)),
            None => Ok(None),
        }
    }

    /// Get collection config
    fn get_collection_config(
        &self,
        collection_id: &CollectionId,
    ) -> VectorResult<Option<VectorConfig>> {
        let key = collection_id.to_key_string();
        let value = self.db.kv_get_with_tag(TypeTag::VectorCollection, &key)?;

        match value {
            Some(bytes) => {
                let stored = StoredCollectionConfig::from_bytes(&bytes)?;
                Ok(Some(stored.config))
            }
            None => Ok(None),
        }
    }

    /// Allocate a new VectorId from the backend
    fn allocate_vector_id(&self, collection_id: &CollectionId) -> VectorResult<VectorId> {
        let backends = self.backends.read().unwrap();
        let backend = backends.get(collection_id)
            .ok_or_else(|| VectorError::CollectionNotFound {
                name: collection_id.name.clone(),
            })?;

        // The backend's heap tracks next_id
        // For now, we need a way to get it - this might need adjustment
        // based on actual backend implementation
        todo!("Get next VectorId from backend heap")
    }
}
```

### Acceptance Criteria

- [ ] Stateless design (cache is reconstructible)
- [ ] Thread-safe with RwLock for backends
- [ ] insert() with upsert semantics
- [ ] get() returns Option<VectorEntry>
- [ ] delete() returns bool
- [ ] All ops validate collection exists

### Complete Story

```bash
~/.cargo/bin/cargo test --workspace
./scripts/complete-story.sh 415
```

---

## Story #416: search() Method with Metadata Filtering

**GitHub Issue**: [#416](https://github.com/anibjoshi/in-mem/issues/416)
**Estimated Time**: 2.5 hours
**Dependencies**: #415
**Blocks**: #417

### Start Story

```bash
gh issue view 416
./scripts/start-story.sh 54 416 search-method
```

### Implementation

```rust
impl VectorStore {
    /// Search for similar vectors
    ///
    /// IMPORTANT: Search is READ-ONLY (Invariant R10).
    /// This method MUST NOT:
    /// - Write WAL entries
    /// - Update counters
    /// - Modify caches
    /// - Have any side effects
    ///
    /// Results are filtered by metadata, then sorted by (score desc, key asc).
    pub fn search(
        &self,
        run_id: RunId,
        collection: &str,
        query: &[f32],
        limit: usize,
        filter: Option<MetadataFilter>,
    ) -> VectorResult<Vec<VectorMatch>> {
        let collection_id = CollectionId::new(run_id, collection);

        // Validate collection exists and get config
        let config = self.get_collection_config(&collection_id)?
            .ok_or_else(|| VectorError::CollectionNotFound {
                name: collection.to_string(),
            })?;

        // Validate query dimension
        if query.len() != config.dimension {
            return Err(VectorError::DimensionMismatch {
                expected: config.dimension,
                got: query.len(),
            });
        }

        // Validate limit
        const MAX_LIMIT: usize = 1000;
        if limit > MAX_LIMIT {
            return Err(VectorError::SearchLimitExceeded {
                requested: limit,
                max: MAX_LIMIT,
            });
        }

        // Search backend (returns VectorId, score pairs)
        let candidates = {
            let backends = self.backends.read().unwrap();
            let backend = backends.get(&collection_id)
                .ok_or_else(|| VectorError::CollectionNotFound {
                    name: collection.to_string(),
                })?;

            // Over-fetch if filtering to ensure we get enough after filtering
            let fetch_limit = if filter.is_some() { limit * 3 } else { limit };
            backend.search(query, fetch_limit)
        };

        // Filter and enrich with metadata
        let matches = self.filter_and_enrich(
            &collection_id,
            candidates,
            filter.as_ref(),
            limit,
        )?;

        Ok(matches)
    }

    /// Filter candidates by metadata and enrich with full data
    fn filter_and_enrich(
        &self,
        collection_id: &CollectionId,
        candidates: Vec<(VectorId, f32)>,
        filter: Option<&MetadataFilter>,
        limit: usize,
    ) -> VectorResult<Vec<VectorMatch>> {
        let mut matches = Vec::with_capacity(limit.min(candidates.len()));

        // Build VectorId -> key mapping for this collection
        let records = self.get_all_records(collection_id)?;
        let id_to_record: std::collections::HashMap<_, _> = records
            .into_iter()
            .map(|r| (r.vector_id, r))
            .collect();

        for (vector_id, score) in candidates {
            if matches.len() >= limit {
                break;
            }

            let Some(record) = id_to_record.get(&vector_id) else {
                continue; // Record deleted between search and enrichment
            };

            // Apply metadata filter
            if let Some(f) = filter {
                if !f.matches(&record.metadata) {
                    continue;
                }
            }

            matches.push(VectorMatch {
                key: record.key.clone(),
                score,
                metadata: record.metadata.clone(),
            });
        }

        // Sort by (score desc, key asc) for facade ordering (Invariant R5)
        matches.sort_by(|a, b| {
            let score_cmp = b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal);
            if score_cmp == std::cmp::Ordering::Equal {
                a.key.cmp(&b.key)
            } else {
                score_cmp
            }
        });

        Ok(matches)
    }

    /// Get all records for a collection (for enrichment)
    fn get_all_records(&self, collection_id: &CollectionId) -> VectorResult<Vec<VectorRecord>> {
        let prefix = format!("{}:", collection_id.to_key_string());
        let entries = self.db.kv_scan_with_tag(TypeTag::VectorRecord, &prefix)?;

        let mut records = Vec::new();
        for (_key, value) in entries {
            records.push(VectorRecord::from_bytes(&value)?);
        }

        Ok(records)
    }
}

#[cfg(test)]
mod search_tests {
    use super::*;

    #[test]
    fn test_facade_ordering() {
        // Verify (score desc, key asc) ordering
        let mut matches = vec![
            VectorMatch::new("b".to_string(), 0.9, None),
            VectorMatch::new("a".to_string(), 0.9, None), // Same score, should come first
            VectorMatch::new("c".to_string(), 0.8, None),
        ];

        matches.sort_by(|a, b| {
            let score_cmp = b.score.partial_cmp(&a.score).unwrap();
            if score_cmp == std::cmp::Ordering::Equal {
                a.key.cmp(&b.key)
            } else {
                score_cmp
            }
        });

        assert_eq!(matches[0].key, "a"); // Same score, key asc
        assert_eq!(matches[1].key, "b");
        assert_eq!(matches[2].key, "c"); // Lower score
    }
}
```

### Acceptance Criteria

- [ ] Validates collection exists
- [ ] Validates query dimension matches config
- [ ] Applies MetadataFilter (AND semantics)
- [ ] Returns VectorMatch with key, score, metadata
- [ ] Final ordering: (score desc, key asc) - Invariant R5
- [ ] NO WAL writes (read-only invariant R10)

### Complete Story

```bash
~/.cargo/bin/cargo test --workspace
./scripts/complete-story.sh 416
```

---

## Story #417: search_request() for SearchRequest/SearchResponse

**GitHub Issue**: [#417](https://github.com/anibjoshi/in-mem/issues/417)
**Estimated Time**: 2 hours
**Dependencies**: #416
**Blocks**: #419

### Start Story

```bash
gh issue view 417
./scripts/start-story.sh 54 417 search-request
```

### Implementation

```rust
use crate::search::{SearchRequest, SearchResponse, SearchResult, DocRef};

impl VectorStore {
    /// Search using M6 SearchRequest/SearchResponse format
    ///
    /// Bridges VectorStore to M6 retrieval surface.
    pub fn search_request(
        &self,
        run_id: RunId,
        collection: &str,
        request: &SearchRequest,
    ) -> VectorResult<SearchResponse> {
        // Extract embedding from request
        let embedding = request.embedding
            .as_ref()
            .ok_or(VectorError::EmptyEmbedding)?;

        // Extract filter from request options
        let filter = request.options
            .as_ref()
            .and_then(|o| o.metadata_filter.clone());

        // Extract limit with default
        let limit = request.limit.unwrap_or(10);

        // Execute search
        let matches = self.search(
            run_id,
            collection,
            embedding,
            limit,
            filter,
        )?;

        // Convert to SearchResponse
        Ok(SearchResponse {
            results: matches
                .into_iter()
                .map(|m| SearchResult {
                    doc_ref: DocRef::Vector {
                        collection: collection.to_string(),
                        key: m.key,
                    },
                    score: m.score,
                    metadata: m.metadata,
                })
                .collect(),
        })
    }
}
```

### Acceptance Criteria

- [ ] Accepts SearchRequest from M6
- [ ] Extracts embedding, limit, filter from request
- [ ] Returns SearchResponse with DocRef::Vector
- [ ] Compatible with hybrid search fusion

### Complete Story

```bash
~/.cargo/bin/cargo test --workspace
./scripts/complete-story.sh 417
```

---

## Story #418: DocRef::Vector Variant

**GitHub Issue**: [#418](https://github.com/anibjoshi/in-mem/issues/418)
**Estimated Time**: 1 hour
**Dependencies**: None
**Blocks**: #417, #419

### Start Story

```bash
gh issue view 418
./scripts/start-story.sh 54 418 doc-ref-vector
```

### Implementation

Modify `crates/primitives/src/search/types.rs`:

```rust
/// Document reference for search results
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum DocRef {
    /// Reference to a KV entry
    Kv { namespace: String, key: String },

    /// Reference to a JSON document
    Json { collection: String, id: String },

    /// Reference to an event
    Event { topic: String, id: u64 },

    /// Reference to a vector entry (M8)
    Vector { collection: String, key: String },
}

impl DocRef {
    /// Check if this is a vector reference
    pub fn is_vector(&self) -> bool {
        matches!(self, DocRef::Vector { .. })
    }

    /// Get vector collection name if applicable
    pub fn vector_collection(&self) -> Option<&str> {
        match self {
            DocRef::Vector { collection, .. } => Some(collection),
            _ => None,
        }
    }

    /// Get vector key if applicable
    pub fn vector_key(&self) -> Option<&str> {
        match self {
            DocRef::Vector { key, .. } => Some(key),
            _ => None,
        }
    }

    /// Create a unique string key for deduplication
    pub fn to_dedup_key(&self) -> String {
        match self {
            DocRef::Kv { namespace, key } => format!("kv:{}:{}", namespace, key),
            DocRef::Json { collection, id } => format!("json:{}:{}", collection, id),
            DocRef::Event { topic, id } => format!("event:{}:{}", topic, id),
            DocRef::Vector { collection, key } => format!("vector:{}:{}", collection, key),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_doc_ref_vector() {
        let doc_ref = DocRef::Vector {
            collection: "embeddings".to_string(),
            key: "doc_123".to_string(),
        };

        assert!(doc_ref.is_vector());
        assert_eq!(doc_ref.vector_collection(), Some("embeddings"));
        assert_eq!(doc_ref.vector_key(), Some("doc_123"));
    }

    #[test]
    fn test_doc_ref_dedup_key() {
        let doc_ref = DocRef::Vector {
            collection: "test".to_string(),
            key: "key1".to_string(),
        };

        assert_eq!(doc_ref.to_dedup_key(), "vector:test:key1");
    }

    #[test]
    fn test_is_vector_false_for_others() {
        let kv = DocRef::Kv {
            namespace: "ns".to_string(),
            key: "k".to_string(),
        };
        assert!(!kv.is_vector());
    }
}
```

### Acceptance Criteria

- [ ] DocRef::Vector with collection and key
- [ ] `is_vector()` helper method
- [ ] `vector_collection()` accessor
- [ ] `vector_key()` accessor
- [ ] `to_dedup_key()` for fusion deduplication
- [ ] Compatible with existing DocRef variants

### Complete Story

```bash
~/.cargo/bin/cargo test --workspace
./scripts/complete-story.sh 418
```

---

## Story #419: RRF Hybrid Search Fusion

**GitHub Issue**: [#419](https://github.com/anibjoshi/in-mem/issues/419)
**Estimated Time**: 2.5 hours
**Dependencies**: #418
**Blocks**: None

### Start Story

```bash
gh issue view 419
./scripts/start-story.sh 54 419 rrf-fusion
```

### Implementation

Create or extend `crates/primitives/src/search/fusion.rs`:

```rust
//! Hybrid search fusion using Reciprocal Rank Fusion (RRF)

use std::collections::HashMap;
use crate::search::{SearchResult, DocRef};

/// Default RRF k parameter
/// k=60 is a common choice that balances between keyword and semantic matches
pub const DEFAULT_RRF_K: f32 = 60.0;

/// Reciprocal Rank Fusion for combining keyword + vector results
///
/// RRF score = sum of 1/(k + rank_i) for each result list containing the doc
///
/// This is a well-established fusion method that:
/// - Doesn't require score normalization across different retrieval methods
/// - Gives higher weight to top-ranked documents
/// - Is robust to different score distributions
pub fn rrf_fusion(
    keyword_results: Vec<SearchResult>,
    vector_results: Vec<SearchResult>,
    k: f32,
    limit: usize,
) -> Vec<SearchResult> {
    let mut scores: HashMap<String, f32> = HashMap::new();
    let mut docs: HashMap<String, SearchResult> = HashMap::new();

    // Score keyword results
    for (rank, result) in keyword_results.into_iter().enumerate() {
        let key = result.doc_ref.to_dedup_key();
        let rrf_score = 1.0 / (k + rank as f32 + 1.0);
        *scores.entry(key.clone()).or_insert(0.0) += rrf_score;
        docs.entry(key).or_insert(result);
    }

    // Score vector results
    for (rank, result) in vector_results.into_iter().enumerate() {
        let key = result.doc_ref.to_dedup_key();
        let rrf_score = 1.0 / (k + rank as f32 + 1.0);
        *scores.entry(key.clone()).or_insert(0.0) += rrf_score;
        docs.entry(key).or_insert(result);
    }

    // Collect and sort by RRF score
    let mut results: Vec<_> = scores
        .into_iter()
        .map(|(key, score)| {
            let mut result = docs.remove(&key).unwrap();
            result.score = score;
            result
        })
        .collect();

    // Sort by score descending, then by doc_ref for determinism
    results.sort_by(|a, b| {
        let score_cmp = b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal);
        if score_cmp == std::cmp::Ordering::Equal {
            a.doc_ref.to_dedup_key().cmp(&b.doc_ref.to_dedup_key())
        } else {
            score_cmp
        }
    });

    results.truncate(limit);
    results
}

/// Convenience function with default k
pub fn rrf_fusion_default(
    keyword_results: Vec<SearchResult>,
    vector_results: Vec<SearchResult>,
    limit: usize,
) -> Vec<SearchResult> {
    rrf_fusion(keyword_results, vector_results, DEFAULT_RRF_K, limit)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn make_result(doc_ref: DocRef, score: f32) -> SearchResult {
        SearchResult {
            doc_ref,
            score,
            metadata: None,
        }
    }

    #[test]
    fn test_rrf_basic() {
        let keyword = vec![
            make_result(DocRef::Kv { namespace: "ns".into(), key: "a".into() }, 0.9),
            make_result(DocRef::Kv { namespace: "ns".into(), key: "b".into() }, 0.8),
            make_result(DocRef::Kv { namespace: "ns".into(), key: "c".into() }, 0.7),
        ];

        let vector = vec![
            make_result(DocRef::Vector { collection: "emb".into(), key: "b".into() }, 0.95),
            make_result(DocRef::Vector { collection: "emb".into(), key: "d".into() }, 0.85),
            make_result(DocRef::Kv { namespace: "ns".into(), key: "a".into() }, 0.75), // Same as keyword[0]
        ];

        let results = rrf_fusion(keyword, vector, 60.0, 10);

        // "a" appears in both lists (rank 0 in keyword, rank 2 in vector)
        // "b" appears in both but with different DocRefs (won't merge)
        assert!(!results.is_empty());
    }

    #[test]
    fn test_rrf_document_in_both_lists_scores_higher() {
        // Same document in both lists should score higher than one in only one list
        let keyword = vec![
            make_result(DocRef::Kv { namespace: "ns".into(), key: "shared".into() }, 0.9),
        ];

        let vector = vec![
            make_result(DocRef::Kv { namespace: "ns".into(), key: "shared".into() }, 0.9),
            make_result(DocRef::Kv { namespace: "ns".into(), key: "vector_only".into() }, 0.95),
        ];

        let results = rrf_fusion(keyword, vector, 60.0, 10);

        // "shared" should be first because it's in both lists
        assert_eq!(results[0].doc_ref.to_dedup_key(), "kv:ns:shared");
    }

    #[test]
    fn test_rrf_deterministic() {
        let keyword = vec![
            make_result(DocRef::Kv { namespace: "ns".into(), key: "a".into() }, 0.9),
            make_result(DocRef::Kv { namespace: "ns".into(), key: "b".into() }, 0.9),
        ];

        let vector = vec![];

        let results1 = rrf_fusion(keyword.clone(), vector.clone(), 60.0, 10);
        let results2 = rrf_fusion(keyword, vector, 60.0, 10);

        assert_eq!(results1.len(), results2.len());
        for (r1, r2) in results1.iter().zip(results2.iter()) {
            assert_eq!(r1.doc_ref.to_dedup_key(), r2.doc_ref.to_dedup_key());
        }
    }

    #[test]
    fn test_rrf_limit() {
        let keyword: Vec<SearchResult> = (0..100)
            .map(|i| make_result(
                DocRef::Kv { namespace: "ns".into(), key: format!("k{}", i) },
                0.9 - i as f32 * 0.01,
            ))
            .collect();

        let results = rrf_fusion(keyword, vec![], 60.0, 10);
        assert_eq!(results.len(), 10);
    }
}
```

### Acceptance Criteria

- [ ] RRF formula: 1/(k + rank)
- [ ] Default k=60
- [ ] Handles documents appearing in both lists
- [ ] Final score is sum of RRF contributions
- [ ] Deterministic ordering for tie-breaks
- [ ] Unit tests with known rankings

### Complete Story

```bash
~/.cargo/bin/cargo test --workspace
./scripts/complete-story.sh 419
```

---

## Story #420: Vector Searchable Implementation

**GitHub Issue**: [#420](https://github.com/anibjoshi/in-mem/issues/420)
**Estimated Time**: 1.5 hours
**Dependencies**: #417
**Blocks**: None

### Start Story

```bash
gh issue view 420
./scripts/start-story.sh 54 420 vector-searchable
```

### Implementation

Create `crates/primitives/src/vector/searchable.rs`:

```rust
//! Searchable trait implementation for VectorStore

use crate::search::{Searchable, SearchRequest, SearchResponse, SearchError};
use crate::vector::VectorStore;

impl Searchable for VectorStore {
    fn search(&self, request: &SearchRequest) -> Result<SearchResponse, SearchError> {
        // Extract run_id from request context
        let run_id = request.context
            .as_ref()
            .and_then(|c| c.run_id.clone())
            .ok_or(SearchError::MissingContext("run_id".to_string()))?;

        // Extract collection from request context
        let collection = request.context
            .as_ref()
            .and_then(|c| c.collection.as_ref())
            .ok_or(SearchError::MissingContext("collection".to_string()))?;

        // Delegate to VectorStore::search_request
        self.search_request(run_id, collection, request)
            .map_err(|e| SearchError::Vector(e.to_string()))
    }

    fn supports_embedding(&self) -> bool {
        true
    }

    fn supports_keyword(&self) -> bool {
        false // Vector does NOT do keyword search
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_supports_embedding() {
        // VectorStore should support embedding search
        // Actual test requires mock Database
    }

    #[test]
    fn test_does_not_support_keyword() {
        // VectorStore should NOT support keyword search
        // Keyword search is handled by other primitives (KV, JSON)
    }
}
```

### Acceptance Criteria

- [ ] Implements Searchable trait
- [ ] `supports_embedding()` returns true
- [ ] `supports_keyword()` returns false (Vector doesn't do keyword search)
- [ ] Extracts run_id and collection from request context
- [ ] Proper error mapping to SearchError

### Complete Story

```bash
~/.cargo/bin/cargo test --workspace
./scripts/complete-story.sh 420
```

---

## Epic 54 Completion Checklist

### Validation

```bash
# Full test suite
~/.cargo/bin/cargo test --workspace

# Search-specific tests
~/.cargo/bin/cargo test vector::store::search
~/.cargo/bin/cargo test search::fusion

# Clippy and format
~/.cargo/bin/cargo clippy --workspace -- -D warnings
~/.cargo/bin/cargo fmt --check
```

### Read-Only Invariant Test (R10)

```rust
#[test]
fn test_search_is_read_only() {
    // Search operations must not modify any state
    let store = setup_test_store();

    // Record state before search
    let wal_offset_before = store.db.wal_offset();

    // Perform search
    let _ = store.search(run_id, "collection", &query, 10, None);

    // WAL offset should not have changed
    let wal_offset_after = store.db.wal_offset();
    assert_eq!(wal_offset_before, wal_offset_after, "Search wrote to WAL!");
}
```

### Epic Merge

```bash
git checkout develop
git merge --no-ff epic-54-search-integration -m "Epic 54: Search Integration complete"
git push origin develop

gh issue close 392 --comment "Epic 54 complete. All 6 stories merged and validated."
```

---

*End of Epic 54 Prompts*
