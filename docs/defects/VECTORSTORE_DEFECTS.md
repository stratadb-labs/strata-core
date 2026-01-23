# VectorStore Defects and Gaps

> Consolidated from architecture review, primitive vs substrate analysis, and vector database best practices.
> Source: `crates/api/src/substrate/vector.rs`, `crates/api/src/facade/vector.rs`, and `crates/primitives/src/vector/`

## Summary

| Category | Count | Priority |
|----------|-------|----------|
| Critical Implementation Bugs | 2 | P0 |
| Hidden Primitive Features | 4 | P1 |
| Missing Table Stakes APIs | 5 | P1 |
| API Design Issues | 3 | P1 |
| Performance Limitations | 2 | P1-P2 |
| World-Class Features | 4 | P2 |
| **Total Issues** | **20** | |

**Critical Finding:** Substrate accepts `SearchFilter` with Range/Or/And/Not but **completely ignores it**, passing `None` to primitive. This is a silent data loss bug.

---

## What is VectorStore?

VectorStore is a **dense vector embedding storage and similarity search primitive** for semantic search and RAG applications.

**Purpose:**
- Store high-dimensional embeddings with metadata
- K-nearest neighbor (KNN) similarity search
- Metadata filtering during search
- Multiple collections with independent configurations
- Run-isolated operations

**Distance Metrics:**
| Metric | Formula | Range | Use Case |
|--------|---------|-------|----------|
| Cosine (default) | `dot(a,b) / (‖a‖·‖b‖)` | [-1, 1] | Text embeddings |
| Euclidean | `1 / (1 + L2)` | (0, 1] | Image embeddings |
| DotProduct | Raw dot product | unbounded | Pre-normalized vectors |

All scores normalized to "higher = more similar" at interface.

---

## Current API Surface

### Primitive (~15 methods)
```rust
// Collections
create_collection(run_id, name, config) -> Versioned<CollectionInfo>
delete_collection(run_id, name) -> ()
list_collections(run_id) -> Vec<CollectionInfo>
get_collection(run_id, name) -> Option<Versioned<CollectionInfo>>
collection_exists(run_id, name) -> bool

// Vectors
insert(run_id, collection, key, embedding, metadata) -> Version
get(run_id, collection, key) -> Option<Versioned<VectorEntry>>
delete(run_id, collection, key) -> bool
count(run_id, collection) -> usize

// Search
search(run_id, collection, query, k, filter) -> Vec<VectorMatch>
search_with_budget(run_id, collection, query, k, filter, budget) -> (Vec<VectorMatch>, truncated)

// Recovery
recover() -> RecoveryStats
replay_* methods for WAL replay
```

### Substrate (7 methods)
```rust
vector_upsert(run, collection, key, vector, metadata) -> Version
vector_get(run, collection, key) -> Option<Versioned<VectorData>>
vector_delete(run, collection, key) -> bool
vector_search(run, collection, query, k, filter?, metric?) -> Vec<VectorMatch>
vector_collection_info(run, collection) -> Option<(dimension, count, metric)>
vector_create_collection(run, collection, dimension, metric) -> Version
vector_drop_collection(run, collection) -> bool
```

### Facade (7 methods)
```rust
vadd(collection, key, vector, metadata) -> ()
vget(collection, key) -> Option<(Vec<f32>, Value)>
vdel(collection, key) -> bool
vsim(collection, query, k) -> Vec<VectorResult>
vsim_with_options(collection, query, k, options) -> Vec<VectorResult>
vcollection_info(collection) -> Option<(dimension, count)>
vcollection_drop(collection) -> bool
```

---

## Part 1: Critical Implementation Bugs (P0)

### Bug 1: SearchFilter Silently Ignored

**Priority:** P0 - Silent data loss

**The Problem:**

Substrate defines a rich filter type:
```rust
enum SearchFilter {
    Equals { field: String, value: Value },
    Prefix { field: String, prefix: String },
    Range { field: String, min: Value, max: Value },
    And(Vec<SearchFilter>),
    Or(Vec<SearchFilter>),
    Not(Box<SearchFilter>),
}
```

But the implementation **completely ignores it**:
```rust
// crates/api/src/substrate/vector.rs lines 326-338
fn vector_search(..., filter: Option<SearchFilter>, ...) {
    let results = self.vector().search(
        run_id,
        collection,
        query,
        k as usize,
        None  // <-- Filter ALWAYS None! SearchFilter is discarded!
    )?;
}
```

**Impact:**
- Users pass filters expecting filtered results
- They get unfiltered results with no error
- Silent incorrect behavior - the worst kind of bug

**Fix:** Either:
1. Implement SearchFilter → MetadataFilter conversion
2. Return error for unsupported filter types
3. Remove SearchFilter from API until implemented

---

### Bug 2: Vector Data Always Empty in Search Results

**Priority:** P0 - Incomplete results

**The Problem:**

Search results should optionally include vector data:
```rust
struct VectorMatch {
    key: String,
    score: f32,
    metadata: Option<Value>,
    vector: Vec<f32>,  // <-- Always empty!
}
```

But implementation always returns empty:
```rust
// In substrate implementation
Ok(VectorMatch {
    key: m.key.clone(),
    score: m.score,
    metadata: m.metadata.clone(),
    vector: vec![],  // <-- Hardcoded empty!
})
```

**Impact:**
- Cannot retrieve vectors from search results
- Must do separate `vector_get` calls for each result
- Extra round trips, worse performance

**Fix:** Fetch and include vector data in results (optionally via parameter)

---

## Part 2: Hidden Primitive Features (P1)

### Gap 1: `vector_list_collections` - Collection Enumeration

**Priority:** P1

**What Primitive Has:**
```rust
fn list_collections(&self, run_id: &RunId) -> Result<Vec<CollectionInfo>>;
```

**What Substrate/Facade Expose:** Nothing

**Why Important:**
- Cannot discover what collections exist
- Same problem as KV's missing `kv_keys`

**Proposed API:**
```rust
fn vector_list_collections(&self, run: &ApiRunId) -> StrataResult<Vec<CollectionInfo>>;
```

---

### Gap 2: `vector_search_with_budget` - Hybrid Search

**Priority:** P1

**What Primitive Has:**
```rust
fn search_with_budget(run_id, collection, query, k, filter, budget)
    -> Result<(Vec<VectorMatch>, bool)>;  // Returns (results, was_truncated)
```

**What Substrate/Facade Expose:** Nothing

**Why Important:**
- Cannot limit search compute time
- Important for real-time applications
- Part of M6 retrieval infrastructure

**Proposed API:**
```rust
fn vector_search_with_budget(&self, run: &ApiRunId, collection: &str,
    query: &[f32], k: u64, filter: Option<SearchFilter>, budget_ms: u64)
    -> StrataResult<(Vec<VectorMatch>, bool)>;
```

---

### Gap 3: `vector_count` - Collection Size

**Priority:** P1

**What Primitive Has:**
```rust
fn count(&self, run_id: &RunId, collection: &str) -> Result<usize>;
```

**What Substrate Has:** Only via `vector_collection_info` (returns tuple)

**Why Important:**
- Dedicated count method is clearer
- `collection_info` returns more than needed for just count

**Proposed API:**
```rust
fn vector_count(&self, run: &ApiRunId, collection: &str) -> StrataResult<u64>;
```

---

### Gap 4: `vector_exists` - Existence Check

**Priority:** P1

**What Primitive Has:**
```rust
fn collection_exists(&self, run_id: &RunId, name: &str) -> Result<bool>;
// Also can check vector existence via get() returning None
```

**What Substrate/Facade Expose:** Nothing direct

**Why Important:**
- Efficient existence check without reading data

**Proposed API:**
```rust
fn vector_collection_exists(&self, run: &ApiRunId, collection: &str) -> StrataResult<bool>;
fn vector_exists(&self, run: &ApiRunId, collection: &str, key: &str) -> StrataResult<bool>;
```

---

## Part 3: Missing Table Stakes APIs (P1)

### Gap 5: Batch Operations

**Priority:** P1

**Current:** No batch operations at any layer

**Why Critical:**
- Inserting 1000 vectors requires 1000 calls
- Cannot atomically insert related vectors
- Performance bottleneck for bulk operations

**Proposed API:**
```rust
fn vector_batch_upsert(&self, run: &ApiRunId, collection: &str,
    vectors: Vec<(String, Vec<f32>, Option<Value>)>) -> StrataResult<Vec<Version>>;

fn vector_batch_delete(&self, run: &ApiRunId, collection: &str,
    keys: &[&str]) -> StrataResult<u64>;

fn vector_batch_get(&self, run: &ApiRunId, collection: &str,
    keys: &[&str]) -> StrataResult<Vec<Option<VectorData>>>;
```

---

### Gap 6: Vector Update (Metadata Only)

**Priority:** P1

**Current:** Must upsert full vector to update metadata

**Why Important:**
- Vectors are large (e.g., 1536 floats = 6KB)
- Often need to update only metadata
- Wastes bandwidth and storage

**Proposed API:**
```rust
fn vector_update_metadata(&self, run: &ApiRunId, collection: &str, key: &str,
    metadata: Value) -> StrataResult<Version>;
```

---

### Gap 7: Collection Configuration Updates

**Priority:** P1

**Current:** Config is immutable after creation

**Why Important:**
- Cannot change metric without recreating collection
- Cannot add config options later

**Proposed API:**
```rust
fn vector_collection_update(&self, run: &ApiRunId, collection: &str,
    config_updates: CollectionConfigUpdate) -> StrataResult<Version>;
```

---

### Gap 8: Multi-Vector Search

**Priority:** P1

**Current:** Search with single query vector only

**Why Important:**
- Cannot search with multiple query vectors
- Common for "find similar to these 3 examples"

**Proposed API:**
```rust
fn vector_search_multi(&self, run: &ApiRunId, collection: &str,
    queries: &[&[f32]], k: u64, aggregate: AggregateMethod)
    -> StrataResult<Vec<VectorMatch>>;

enum AggregateMethod {
    Max,      // Max similarity across queries
    Average,  // Average similarity
    Min,      // Min similarity (must be similar to ALL)
}
```

---

### Gap 9: Vector List/Scan

**Priority:** P1

**Current:** No way to enumerate vectors in collection

**Why Important:**
- Cannot iterate over all vectors
- Cannot export collection
- Same problem as KV scan

**Proposed API:**
```rust
fn vector_list(&self, run: &ApiRunId, collection: &str,
    limit: Option<u64>, cursor: Option<&str>) -> StrataResult<VectorScanResult>;
```

---

## Part 4: API Design Issues (P1)

### Design Issue 1: Collection Info Returns Tuple

**Current:**
```rust
fn vector_collection_info(...) -> Option<(usize, u64, DistanceMetric)>;
// What is usize? What is u64? Order matters!
```

**Should Be:**
```rust
struct CollectionInfo {
    name: String,
    dimension: usize,
    count: u64,
    metric: DistanceMetric,
    created_at: u64,
}

fn vector_collection_info(...) -> Option<CollectionInfo>;
```

---

### Design Issue 2: Metric Override Parameter Ignored

**Current:**
```rust
fn vector_search(..., metric: Option<DistanceMetric>) -> ...;
// metric parameter is accepted but ignored!
```

**Fix:** Either implement or remove the parameter

---

### Design Issue 3: Inconsistent Naming

| Primitive | Substrate | Facade |
|-----------|-----------|--------|
| `insert` | `vector_upsert` | `vadd` |
| `delete` | `vector_delete` | `vdel` |
| `search` | `vector_search` | `vsim` |

`upsert` vs `insert` vs `add` - pick one naming convention.

---

## Part 5: Performance Limitations (P1-P2)

### Limitation 1: Brute Force Only (O(n))

**Priority:** P1 at scale

**Current:** O(n) exhaustive search via `BruteForceBackend`

**Impact:**
- Fine for < 10K vectors
- P95 > 100ms at 50K vectors
- Unusable at 100K+ vectors

**Roadmap:** HNSW backend reserved for M9

**Proposed API (future):**
```rust
struct CollectionConfig {
    dimension: usize,
    metric: DistanceMetric,
    index_type: IndexType,  // BruteForce, HNSW, IVF
    index_params: Option<IndexParams>,
}
```

---

### Limitation 2: Single-Threaded Search

**Priority:** P2

**Current:** No parallel computation in search

**Impact:**
- Doesn't utilize multi-core CPUs
- Linear scaling only

**Fix:** Parallelize distance computations with rayon

---

## Part 6: World-Class Features (P2)

### Gap 10: Hybrid Search (Vector + Full-Text)

**Priority:** P2

**Problem:** Cannot combine vector similarity with keyword search

**Use Case:** "Find documents similar to X that also mention 'python'"

**Proposed API:**
```rust
fn vector_hybrid_search(&self, run: &ApiRunId, collection: &str,
    query: &[f32], text_query: &str, k: u64, weights: (f32, f32))
    -> StrataResult<Vec<VectorMatch>>;
```

---

### Gap 11: Quantization

**Priority:** P2

**Current:** F32 only (4 bytes per dimension)

**Missing:**
- F16 (2 bytes) - 50% memory savings
- Int8 (1 byte) - 75% memory savings

**Primitive has placeholder:**
```rust
enum StorageDtype {
    F32,
    F16,    // Reserved
    Int8,   // Reserved
}
```

---

### Gap 12: Namespaces/Partitions

**Priority:** P2

**Current:** Flat collection structure

**Missing:** Hierarchical organization for multi-tenant

**Proposed API:**
```rust
fn vector_create_collection_with_namespace(&self, run: &ApiRunId,
    namespace: &str, collection: &str, config: CollectionConfig) -> ...;
```

---

### Gap 13: Index Statistics

**Priority:** P2

**Problem:** No visibility into index health/performance

**Proposed API:**
```rust
struct IndexStats {
    vector_count: u64,
    dimension: usize,
    memory_bytes: u64,
    avg_search_ms: f64,
    index_type: String,
}

fn vector_index_stats(&self, run: &ApiRunId, collection: &str)
    -> StrataResult<IndexStats>;
```

---

## Priority Matrix

| ID | Issue | Priority | Effort | Category |
|----|-------|----------|--------|----------|
| Bug 1 | SearchFilter ignored | P0 | Medium | Critical Bug |
| Bug 2 | Vector data empty | P0 | Low | Critical Bug |
| Gap 1 | List collections | P1 | Low | Hidden |
| Gap 2 | Search with budget | P1 | Low | Hidden |
| Gap 3 | Count | P1 | Low | Hidden |
| Gap 4 | Exists | P1 | Low | Hidden |
| Gap 5 | Batch operations | P1 | Medium | Missing API |
| Gap 6 | Metadata-only update | P1 | Low | Missing API |
| Gap 7 | Collection updates | P1 | Medium | Missing API |
| Gap 8 | Multi-vector search | P1 | Medium | Missing API |
| Gap 9 | Vector list/scan | P1 | Medium | Missing API |
| Design 1 | Tuple return | P1 | Low | Design |
| Design 2 | Metric param ignored | P1 | Low | Design |
| Design 3 | Naming inconsistency | P1 | Low | Design |
| Limit 1 | Brute force only | P1 | High | Performance |
| Limit 2 | Single-threaded | P2 | Medium | Performance |
| Gap 10 | Hybrid search | P2 | High | World-Class |
| Gap 11 | Quantization | P2 | High | World-Class |
| Gap 12 | Namespaces | P2 | Medium | World-Class |
| Gap 13 | Index stats | P2 | Low | World-Class |

---

## Recommended Fix Order

### Phase 0: Critical Bugs (Immediate)
1. **Fix SearchFilter** - Either implement or error, not silent ignore
2. **Fix vector data** - Return actual vectors in search results

### Phase 1: Quick Wins (Low Effort)
3. Expose `list_collections` (Gap 1)
4. Expose `search_with_budget` (Gap 2)
5. Add `vector_count` (Gap 3)
6. Add `vector_exists` (Gap 4)
7. Fix collection_info return type (Design 1)
8. Fix or remove metric param (Design 2)

### Phase 2: Core Features (Medium Effort)
9. Add batch operations (Gap 5)
10. Add metadata-only update (Gap 6)
11. Add vector list/scan (Gap 9)
12. Standardize naming (Design 3)

### Phase 3: Advanced (High Effort)
13. HNSW indexing (Limit 1)
14. Multi-vector search (Gap 8)
15. Hybrid search (Gap 10)
16. Quantization (Gap 11)

---

## Comparison with Industry Standards

| Feature | Strata VectorStore | Pinecone | Weaviate | Qdrant |
|---------|-------------------|----------|----------|--------|
| Basic CRUD | ✅ | ✅ | ✅ | ✅ |
| Similarity search | ✅ | ✅ | ✅ | ✅ |
| **Metadata filtering** | ❌ (broken!) | ✅ | ✅ | ✅ |
| Batch operations | ❌ | ✅ | ✅ | ✅ |
| List/scan | ❌ | ✅ | ✅ | ✅ |
| Multi-vector search | ❌ | ✅ | ✅ | ✅ |
| **ANN indexing** | ❌ (brute force) | ✅ | ✅ | ✅ |
| Hybrid search | ❌ | ✅ | ✅ | ✅ |
| Quantization | ❌ | ✅ | ❌ | ✅ |
| Namespaces | ❌ | ✅ | ✅ | ✅ |
| **Embedded library** | ✅ | ❌ | ❌ | ❌ |
| **Run isolation** | ✅ | ❌ | ❌ | ❌ |

**Strata's Unique Strengths:**
- Embedded (no network, no deployment)
- Run isolation for multi-agent
- WAL-based durability

**Strata's Critical Gaps:**
- SearchFilter is broken (silent no-op)
- No ANN indexing (O(n) only)
- No batch operations
- No collection enumeration

---

## Key Findings

### Critical: SearchFilter is a Lie

The substrate API promises rich filtering:
```rust
SearchFilter::Range { field: "price", min: 10, max: 100 }
SearchFilter::Or(vec![...])
SearchFilter::And(vec![...])
```

But **all filters are silently discarded**. Users get unfiltered results with no error. This is worse than "not implemented" - it's "silently wrong."

### Good: Primitive is Solid

The primitive layer has:
- Adaptive over-fetch for filtering
- Deterministic ordering guarantees
- WAL integration
- Recovery/replay support
- Budget-aware search

The problem is substrate doesn't use it.

### Reality Check

For a vector database to be useful for RAG:
1. ❌ Metadata filtering (broken)
2. ❌ Batch insert (missing)
3. ❌ Scalable indexing (brute force only)
4. ✅ Basic similarity search (works)
5. ✅ Embedded operation (works)

**VectorStore is a proof-of-concept, not production-ready for RAG workloads.**
