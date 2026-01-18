# M8 Architecture Specification: Vector Primitive

**Version**: 1.0
**Status**: Implementation Ready
**Last Updated**: 2026-01-17

---

## Executive Summary

This document specifies the architecture for **Milestone 8 (M8): Vector Primitive** of the in-memory agent database. M8 introduces a native vector primitive for semantic search and AI agent memory, enabling similarity-based retrieval alongside the keyword search from M6.

**THIS DOCUMENT IS AUTHORITATIVE.** All M8 implementation must conform to this specification.

**Related Documents**:
- [M8 Scope](../milestones/M8_SCOPE.md) - Finalized scope and locked decisions
- [M7 Architecture](./M7_ARCHITECTURE.md) - Durability and storage stabilization
- [M6 Architecture](./M6_ARCHITECTURE.md) - Retrieval surfaces
- [MILESTONES.md](../milestones/MILESTONES.md) - Project milestone tracking

**M8 Philosophy**:
> Vector is not a standalone database feature. It's a **composite primitive** that enables semantic search alongside keyword search. KV + JSON + Vector covers 99% of AI agent database needs.
>
> M8 validates the API and integration. M9 optimizes for scale.

**M8 Goals**:
- Native vector storage with configurable dimensions and distance metrics
- Brute-force similarity search (validates API, sufficient for small datasets)
- Full integration with M6 retrieval surfaces (hybrid search)
- Full integration with transaction system (atomic cross-primitive operations)
- VectorIndexBackend trait for M9 HNSW integration

**M8 Non-Goals** (Deferred to M9):
- HNSW index
- Quantization (F16, Int8)
- Complex metadata filtering (ranges, nested paths)
- Pre-filtering
- Batch insert optimization

**Critical Constraint**:
> M8 is an API validation milestone, not a performance milestone. Brute-force search is O(n) and will become slow at scale. That is acceptable. The interfaces matter more than search speed. We can add HNSW in M9.

**Built on M1-M7**:
- M1 provides: Storage (UnifiedStore), WAL, Recovery
- M2 provides: OCC transactions, Snapshot isolation, Conflict detection
- M3 provides: Five primitives (KVStore, EventLog, StateCell, TraceStore, RunIndex)
- M4 provides: Durability modes, performance optimizations, ShardedStore
- M5 provides: JsonStore primitive with path-level mutations
- M6 provides: Retrieval surface with primitive-native search and composite hybrid search
- M7 provides: Snapshots, crash recovery, deterministic replay, storage stabilization
- M8 adds: Vector primitive with similarity search and hybrid retrieval

---

## Table of Contents

1. [Scope Boundaries](#1-scope-boundaries)
2. [THE SIX ARCHITECTURAL RULES](#2-the-six-architectural-rules-non-negotiable)
3. [Core Invariants](#3-core-invariants)
4. [Architecture Principles](#4-architecture-principles)
5. [Interface Invariants](#5-interface-invariants)
6. [Core Types](#6-core-types)
7. [Storage Model](#7-storage-model)
8. [Index Backend Abstraction](#8-index-backend-abstraction)
9. [Similarity Search](#9-similarity-search)
10. [Collection Management](#10-collection-management)
11. [Transaction Integration](#11-transaction-integration)
12. [M6 Search Integration](#12-m6-search-integration)
13. [WAL Integration](#13-wal-integration)
14. [Snapshot & Recovery](#14-snapshot--recovery)
15. [API Design](#15-api-design)
16. [Performance Characteristics](#16-performance-characteristics)
17. [Testing Strategy](#17-testing-strategy)
18. [Known Limitations](#18-known-limitations)
19. [Future Extension Points](#19-future-extension-points)
20. [Appendix](#20-appendix)

---

## 1. Scope Boundaries

### 1.1 What M8 IS

M8 is an **API validation milestone**. It defines:

| Aspect | M8 Commits To |
|--------|---------------|
| **Core API** | insert (upsert), get, delete, search |
| **Storage** | Vector heap + KV-backed metadata |
| **Search** | Brute-force with cosine/euclidean/dot product |
| **Collections** | Named collections per RunId, immutable config |
| **M6 Integration** | SearchRequest/SearchResponse, RRF fusion |
| **Transactions** | Full participation in cross-primitive transactions |
| **Index Abstraction** | VectorIndexBackend trait for M9 HNSW |

### 1.2 What M8 is NOT

M8 is **not** a performance milestone. These are explicitly deferred:

| Deferred Item | Why Deferred | Target Milestone |
|---------------|--------------|------------------|
| HNSW index | Complexity | M9 |
| Quantization (F16, Int8) | Optimization | M9 |
| Complex metadata filtering | Feature scope | M9 |
| Pre-filtering | Complexity | M9 |
| Batch insert optimization | Optimization | M9 |
| GPU acceleration | Far future | Post-MVP |
| Distributed vector search | Far future | Post-MVP |

### 1.3 Performance Expectations

**M8 accepts O(n) brute-force search.**

| Dataset Size | Expected Latency | Acceptable? |
|--------------|------------------|-------------|
| 1K vectors | < 5 ms | Yes |
| 10K vectors | < 50 ms | Yes |
| 50K vectors | < 200 ms | Borderline |
| 100K vectors | > 500 ms | Forces M9 priority |

**Switch threshold**: P95 > 100ms at 50K vectors triggers M9/HNSW priority.

### 1.4 The Risk We Are Avoiding

Vector search implementations can become complex:
- HNSW graph construction and maintenance
- Quantization with accuracy tradeoffs
- Index persistence and incremental updates
- Memory management for large embeddings

**We don't need this complexity yet.** M8 validates the API with brute-force. Once the API is validated, M9 adds HNSW behind the same interface.

**Rule**: If a feature requires HNSW, it is out of scope for M8.

### 1.5 Evolution Warnings

**These are explicit warnings about M8 design decisions that must not ossify:**

#### A. VectorIndexBackend Must Enable HNSW

The `VectorIndexBackend` trait exists precisely to swap brute-force for HNSW. The trait interface must not assume brute-force semantics.

```rust
// CORRECT: Trait that works for both brute-force and HNSW
pub trait VectorIndexBackend: Send + Sync {
    fn insert(&mut self, id: VectorId, embedding: &[f32]) -> Result<()>;
    fn delete(&mut self, id: VectorId) -> Result<bool>;
    fn search(&self, query: &[f32], k: usize) -> Vec<(VectorId, f32)>;
}

// WRONG: Trait that assumes brute-force
pub trait VectorIndexBackend {
    fn get_all_vectors(&self) -> &[f32];  // HNSW doesn't work this way
}
```

#### B. Vector Heap Layout Must Support Deletion

Brute-force can use a simple `Vec<f32>` but HNSW needs stable IDs. M8's storage must not assume append-only layout.

#### C. Metadata Is Separate From Embedding

Embeddings are stored in the vector heap for cache-friendly scanning. Metadata is stored in KV for flexible querying. Do not merge them.

#### D. Collection Config Is Immutable

Dimension, metric, and storage dtype are set at collection creation. Changing them would invalidate all vectors. This is intentional.

---

## 2. THE SIX ARCHITECTURAL RULES (NON-NEGOTIABLE)

**These rules MUST be followed in ALL M8 implementation. Violating any of these is a blocking issue.**

### Rule 1: Stateless Facade Pattern

> **VectorStore is a stateless facade. All state lives in Database.**

```rust
// CORRECT: Stateless facade
pub struct VectorStore {
    db: Arc<Database>,
}

impl Clone for VectorStore {
    fn clone(&self) -> Self {
        VectorStore { db: self.db.clone() }
    }
}

// WRONG: Stateful primitive
pub struct VectorStore {
    db: Arc<Database>,
    local_cache: HashMap<VectorId, Vec<f32>>,  // NEVER DO THIS
}
```

**Why**: Multiple VectorStore instances on the same Database must be safe. State must flow through Database for consistency.

### Rule 2: Collections Per RunId

> **Collections are scoped to RunId. Different runs cannot see each other's collections.**

```rust
// CORRECT: Run-scoped operations
pub fn insert(&self, run_id: RunId, collection: &str, key: &str, ...) -> Result<()>;
pub fn search(&self, run_id: RunId, collection: &str, ...) -> Result<Vec<VectorMatch>>;

// WRONG: Global collections
pub fn insert(&self, collection: &str, key: &str, ...) -> Result<()>;  // No run_id
```

**Why**: Run isolation is a core invariant. Vectors follow the same pattern as all other primitives.

### Rule 3: Upsert Semantics

> **Insert overwrites if key exists. No separate update method.**

```rust
// CORRECT: Upsert
pub fn insert(&self, run_id: RunId, collection: &str, key: &str,
              embedding: &[f32], metadata: Option<JsonValue>) -> Result<()> {
    // If key exists, overwrite
    // If key doesn't exist, create
}

// WRONG: Separate insert and update
pub fn insert(...) -> Result<()> { /* fails if exists */ }
pub fn update(...) -> Result<()> { /* fails if not exists */ }
```

**Why**: Agents typically want "set this vector" semantics. Upsert is simpler and matches user expectations.

### Rule 4: Dimension Validation

> **All vectors in a collection MUST have the same dimension. Enforce on insert and query.**

```rust
// CORRECT: Validate dimensions
pub fn insert(&self, run_id: RunId, collection: &str, key: &str,
              embedding: &[f32], ...) -> Result<()> {
    let config = self.get_collection_config(run_id, collection)?;
    if embedding.len() != config.dimension {
        return Err(VectorError::DimensionMismatch {
            expected: config.dimension,
            got: embedding.len(),
        });
    }
    // ...
}

// WRONG: Allow mixed dimensions
pub fn insert(..., embedding: &[f32], ...) -> Result<()> {
    // No dimension check - will break search
}
```

**Why**: Distance calculations require matching dimensions. Mixed dimensions would produce garbage results.

### Rule 5: Deterministic Ordering at Every Layer

> **Determinism MUST be enforced at the backend level, not just the facade.**

```rust
// CORRECT: Backend returns deterministically ordered results
impl VectorIndexBackend for BruteForceBackend {
    fn search(&self, query: &[f32], k: usize) -> Vec<(VectorId, f32)> {
        let mut results = self.compute_all_similarities(query);
        // Sort by (score desc, VectorId asc) at backend level
        results.sort_by(|(id_a, score_a), (id_b, score_b)| {
            score_b.partial_cmp(score_a)
                .unwrap_or(Ordering::Equal)
                .then_with(|| id_a.cmp(id_b))  // VectorId tie-break
        });
        results.truncate(k);
        results
    }
}

// WRONG: Backend returns nondeterministic order, facade "fixes" it
impl VectorIndexBackend for BruteForceBackend {
    fn search(&self, query: &[f32], k: usize) -> Vec<(VectorId, f32)> {
        let mut results = self.compute_all_similarities(query);
        results.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());  // Ties arbitrary!
        results
    }
}
```

**Determinism chain**:
1. Backend: sort by `(score desc, VectorId asc)`
2. Facade: map VectorId → key, then sort by `(score desc, key asc)`
3. Both layers enforce determinism independently

**Why**: HashMap iteration is nondeterministic. Floating point ties are common. If backend returns arbitrary order, facade sorting cannot recover determinism reliably.

### Rule 6: VectorId Is Never Reused

> **Once a VectorId is assigned, it is never recycled, even after deletion.**

```rust
// CORRECT: Monotonically increasing IDs
impl VectorHeap {
    fn allocate_id(&self) -> VectorId {
        VectorId(self.next_id.fetch_add(1, Ordering::Relaxed))
    }

    fn delete(&mut self, id: VectorId) -> bool {
        if let Some(offset) = self.id_to_offset.remove(&id) {
            self.free_slots.push(offset);  // Reuse storage slot
            // But NEVER reuse the VectorId value
            true
        } else {
            false
        }
    }
}

// WRONG: Recycling VectorId values
impl VectorHeap {
    fn allocate_id(&self) -> VectorId {
        if let Some(recycled) = self.free_ids.pop() {
            return recycled;  // NEVER DO THIS
        }
        VectorId(self.next_id.fetch_add(1, Ordering::Relaxed))
    }
}
```

**Why**: VectorId reuse creates subtle replay bugs. If you insert → delete → insert the same key, replay must produce identical state. Reusing IDs makes this fragile.

**Invariant**: `VectorId` values are monotonically increasing within a collection lifetime. Storage slots may be reused, but IDs never are.

### Rule 7: No Backend-Specific Fields in VectorConfig

> **VectorConfig contains only primitive-level configuration. Backend-specific tuning must NOT pollute this type.**

```rust
// CORRECT: Primitive-level config only
pub struct VectorConfig {
    pub dimension: usize,
    pub metric: DistanceMetric,
    pub storage_dtype: StorageDtype,
}

// WRONG: Backend-specific fields in primitive config
pub struct VectorConfig {
    pub dimension: usize,
    pub metric: DistanceMetric,
    pub storage_dtype: StorageDtype,
    pub ef_construction: usize,  // NEVER DO THIS - HNSW-specific
    pub M: usize,                // NEVER DO THIS - HNSW-specific
}
```

**Why**: Backend-specific config belongs in backend initialization, not in the primitive config. This prevents HNSW from polluting the Vector API when added in M9.

**Future pattern**: If M9 needs HNSW tuning, use a separate `HnswConfig` passed to backend construction, not to `VectorConfig`.

---

## 3. Core Invariants

### 3.1 Storage Invariants

| # | Invariant | Meaning |
|---|-----------|---------|
| S1 | Dimension immutable | Collection dimension cannot change after creation |
| S2 | Metric immutable | Distance metric cannot change after creation |
| S3 | VectorId stable | IDs do not change within collection lifetime |
| S4 | VectorId never reused | Once assigned, a VectorId is never recycled (even after deletion) |
| S5 | Heap + KV consistency | Vector heap and KV metadata always in sync |
| S6 | Run isolation | Collections scoped to RunId |
| S7 | BTreeMap sole source | id_to_offset (BTreeMap) is the ONLY source of truth for active vectors |
| S8 | Snapshot-WAL equivalence | Snapshot + WAL replay must produce identical state to pure WAL replay |
| S9 | Heap-KV reconstructibility | VectorHeap and KV metadata can both be fully reconstructed from snapshot + WAL |

### 3.2 Search Invariants

| # | Invariant | Meaning |
|---|-----------|---------|
| R1 | Dimension match | Query dimension must match collection dimension |
| R2 | Score normalization | All metrics return "higher is better" scores |
| R3 | Deterministic order | Same query = same result order (enforced at backend level) |
| R4 | Backend tie-break | Backend sorts by (score desc, VectorId asc) |
| R5 | Facade tie-break | Facade sorts by (score desc, key asc) |
| R6 | Snapshot consistency | Search sees consistent point-in-time view |
| R7 | Coarse-grained budget | Budget checked at phase boundaries; brute-force may overshoot |
| R8 | Single-threaded | Similarity computation is single-threaded for determinism |
| R9 | No implicit normalization | Embeddings stored as-is, no silent normalization |
| R10 | Search is read-only | Search must not write anything: no counters, no caches, no side effects |

**Budget Enforcement Note (R7)**: Brute-force search cannot be interrupted mid-loop. Budget is checked:
1. Before starting search
2. After completing similarity computation
3. After sorting/filtering

For large datasets (50K+ vectors), actual time may significantly exceed budget before check. This is acceptable for M8. M9's HNSW can check budget during graph traversal.

### 3.3 Transaction Invariants

| # | Invariant | Meaning |
|---|-----------|---------|
| T1 | Atomic visibility | Insert/delete atomic with other primitives |
| T2 | Conflict detection | Concurrent writes to same key conflict |
| T3 | Rollback safety | Failed transactions leave no partial state |
| T4 | VectorId monotonicity across crashes | After crash recovery, new VectorIds must be > all previous IDs |

---

## 4. Architecture Principles

### 4.1 M8-Specific Principles

1. **API Over Speed**
   - M8 may produce slow search results. That is acceptable.
   - Interface correctness matters more than latency.
   - Performance improvements happen by swapping backends, not changing APIs.

2. **Simplicity First**
   - Brute-force is simple and correct.
   - HNSW adds complexity (graph construction, persistence, tuning).
   - Validate API with simple implementation, optimize later.

3. **Composability**
   - Vectors participate in cross-primitive transactions.
   - Vectors integrate with M6 hybrid search.
   - Vectors use existing WAL, snapshot, recovery infrastructure.

4. **Explicit Configuration**
   - Collection config is explicit (dimension, metric, dtype).
   - No magic defaults that change behavior.
   - Immutable config prevents silent data corruption.

5. **Budget-Bounded Execution**
   - Search operations respect M6 budget model.
   - Graceful degradation over timeout errors.
   - Truncated results clearly marked.

### 4.2 What Vector Is NOT

| Misconception | Reality |
|---------------|---------|
| "Vector replaces KV" | Vector is for similarity; KV for exact access |
| "Vector needs GPU" | M8 uses CPU brute-force |
| "Vector is always fast" | Brute-force is O(n) |
| "One metric fits all" | Different embeddings need different metrics |
| "HNSW is required" | Brute-force is sufficient for < 10K vectors |

---

## 5. Interface Invariants (Never Change)

This section defines interface invariants that **MUST hold for all future milestones**.

### 5.1 VectorConfig Is Immutable After Creation

```rust
pub struct VectorConfig {
    pub dimension: usize,
    pub metric: DistanceMetric,
    pub storage_dtype: StorageDtype,
}
```

**This config must not change after collection creation.**

### 5.2 Distance Metrics Are Normalized

All metrics return scores where **higher = more similar**.

```rust
pub enum DistanceMetric {
    Cosine,      // 1 - cosine_distance
    Euclidean,   // 1 / (1 + l2_distance)
    DotProduct,  // raw dot product (assumes normalized vectors)
}
```

**This normalization must not change.**

### 5.3 VectorMatch Contains Key and Score

```rust
pub struct VectorMatch {
    pub key: String,
    pub score: f32,  // Always "higher is better"
    pub metadata: Option<JsonValue>,
}
```

**This structure must not change.**

### 5.4 Search Returns Same Type as M6

Vector search returns `SearchResponse` compatible with M6 hybrid search.

```rust
impl VectorStore {
    pub fn search_request(&self, run_id: RunId, request: &SearchRequest)
        -> Result<SearchResponse>;
}
```

**This compatibility must not change.**

---

## 6. Core Types

### 6.1 Configuration Types

```rust
/// Collection configuration - immutable after creation
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VectorConfig {
    /// Embedding dimension (e.g., 384, 768, 1536)
    pub dimension: usize,

    /// Distance metric for similarity
    pub metric: DistanceMetric,

    /// Storage data type (only F32 in M8)
    pub storage_dtype: StorageDtype,
}

impl VectorConfig {
    /// Create config for common embedding models
    pub fn for_openai_ada() -> Self {
        VectorConfig {
            dimension: 1536,
            metric: DistanceMetric::Cosine,
            storage_dtype: StorageDtype::F32,
        }
    }

    pub fn for_minilm() -> Self {
        VectorConfig {
            dimension: 384,
            metric: DistanceMetric::Cosine,
            storage_dtype: StorageDtype::F32,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DistanceMetric {
    /// Cosine similarity: 1 - cosine_distance
    /// Higher = more similar. Range: [-1, 1] mapped to [0, 2]
    Cosine,

    /// Euclidean distance: 1 / (1 + l2_distance)
    /// Higher = more similar. Range: (0, 1]
    Euclidean,

    /// Dot product (assumes normalized vectors)
    /// Higher = more similar. Range: [-1, 1] for normalized
    DotProduct,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum StorageDtype {
    #[default]
    F32,
    // F16,     // M9: Half precision
    // Int8,    // M9: Scalar quantization
}
```

### 6.2 Collection Types

```rust
/// Collection metadata
#[derive(Debug, Clone)]
pub struct CollectionInfo {
    /// Collection name
    pub name: String,

    /// Immutable configuration
    pub config: VectorConfig,

    /// Current vector count
    pub count: usize,

    /// Creation timestamp (microseconds)
    pub created_at: u64,
}

/// Unique identifier for a collection within a run
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CollectionId {
    pub run_id: RunId,
    pub name: String,
}
```

### 6.3 Vector Types

```rust
/// Internal vector identifier (stable within collection)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct VectorId(u64);

impl VectorId {
    pub fn new(id: u64) -> Self {
        VectorId(id)
    }
}

/// Vector entry with embedding and metadata
#[derive(Debug, Clone)]
pub struct VectorEntry {
    /// User-provided key
    pub key: String,

    /// Embedding vector
    pub embedding: Vec<f32>,

    /// Optional JSON metadata
    pub metadata: Option<JsonValue>,

    /// Internal ID (for index backend)
    pub(crate) vector_id: VectorId,

    /// Version for CAS (optional, for future use)
    pub(crate) version: u64,
}

/// Search result
#[derive(Debug, Clone)]
pub struct VectorMatch {
    /// User-provided key
    pub key: String,

    /// Similarity score (higher = more similar)
    pub score: f32,

    /// Optional metadata (if requested)
    pub metadata: Option<JsonValue>,
}
```

### 6.4 Filter Types

```rust
/// Metadata filter for search (M8: equality only)
#[derive(Debug, Clone, Default)]
pub struct MetadataFilter {
    /// Top-level field equality (scalar values only)
    pub equals: HashMap<String, JsonScalar>,
}

/// JSON scalar value for filtering
#[derive(Debug, Clone, PartialEq)]
pub enum JsonScalar {
    Null,
    Bool(bool),
    Number(f64),
    String(String),
}

impl MetadataFilter {
    /// Check if metadata matches filter
    pub fn matches(&self, metadata: &Option<JsonValue>) -> bool {
        if self.equals.is_empty() {
            return true;
        }

        let Some(meta) = metadata else {
            return false;
        };

        let Some(obj) = meta.as_object() else {
            return false;
        };

        for (key, expected) in &self.equals {
            let Some(actual) = obj.get(key) else {
                return false;
            };
            if !scalar_matches(actual, expected) {
                return false;
            }
        }

        true
    }
}
```

### 6.5 Error Types

```rust
#[derive(Debug, thiserror::Error)]
pub enum VectorError {
    #[error("Collection not found: {name}")]
    CollectionNotFound { name: String },

    #[error("Collection already exists: {name}")]
    CollectionAlreadyExists { name: String },

    #[error("Dimension mismatch: expected {expected}, got {got}")]
    DimensionMismatch { expected: usize, got: usize },

    #[error("Invalid dimension: {dimension}")]
    InvalidDimension { dimension: usize },

    #[error("Vector not found: {key}")]
    VectorNotFound { key: String },

    #[error("Empty embedding")]
    EmptyEmbedding,

    #[error("Invalid collection name: {name}")]
    InvalidCollectionName { name: String },

    #[error("Storage error: {0}")]
    Storage(#[from] StorageError),

    #[error("Transaction error: {0}")]
    Transaction(#[from] TransactionError),
}
```

---

## 7. Storage Model

### 7.1 Hybrid Storage Architecture

M8 uses a **hybrid storage model**: vector heap for embeddings, KV for metadata.

```
┌─────────────────────────────────────────────────────────┐
│                    VectorStore                          │
├─────────────────────────────────────────────────────────┤
│                                                         │
│  ┌─────────────────────┐    ┌─────────────────────────┐ │
│  │   Vector Heap       │    │    KV Metadata          │ │
│  │   (per collection)  │    │    (ShardedStore)       │ │
│  │                     │    │                         │ │
│  │  VectorId → offset  │    │  Key → VectorRecord     │ │
│  │  Contiguous f32[]   │    │    - vector_id          │ │
│  │  Cache-friendly     │    │    - metadata           │ │
│  │                     │    │    - version            │ │
│  │  Fast brute-force   │    │    - timestamp          │ │
│  │  scan               │    │                         │ │
│  └─────────────────────┘    └─────────────────────────┘ │
│                                                         │
│  ┌─────────────────────────────────────────────────────┐│
│  │            VectorIndexBackend (trait)               ││
│  │  - BruteForceBackend (M8)                          ││
│  │  - HnswBackend (M9)                                ││
│  └─────────────────────────────────────────────────────┘│
└─────────────────────────────────────────────────────────┘
```

### 7.2 Why Hybrid Storage?

**Why not pure KV-backed?**
- KV bytes require deserializing every vector on every search
- Not cache-friendly for dense numeric scanning
- Becomes bottleneck immediately

**Why not pure vector heap?**
- Need to participate in transactions
- Need WAL integration for durability
- Need metadata with flexible schema

**Hybrid approach**:
- Vector heap: contiguous `Vec<f32>` for fast similarity computation
- KV metadata: standard storage for metadata, key mapping, version tracking
- Best of both worlds

### 7.3 Vector Heap Structure

```rust
/// Per-collection vector heap
pub(crate) struct VectorHeap {
    /// Collection configuration
    config: VectorConfig,

    /// Contiguous embedding storage
    /// Layout: [v0_dim0, v0_dim1, ..., v0_dimN, v1_dim0, ...]
    data: Vec<f32>,

    /// VectorId -> offset in data (in floats, not bytes)
    /// IMPORTANT: Use BTreeMap for deterministic iteration order.
    /// This is the SOLE source of truth for active vectors.
    /// No secondary data structures (like active_ids Vec) - single source of truth.
    id_to_offset: BTreeMap<VectorId, usize>,

    /// Free list for deleted storage slots (enables slot reuse)
    /// NOTE: Storage slots are reused, but VectorId values are NEVER reused.
    /// MUST be persisted in snapshots for correct recovery.
    free_slots: Vec<usize>,

    /// Next VectorId to allocate (monotonically increasing, never recycled)
    /// MUST be persisted in snapshots to maintain ID uniqueness across restarts.
    /// Without this, recovery would reuse IDs and break replay determinism.
    next_id: AtomicU64,

    /// Version for snapshot consistency
    version: AtomicU64,
}

impl VectorHeap {
    /// Insert or update a vector
    pub fn upsert(&mut self, id: VectorId, embedding: &[f32]) -> Result<()> {
        assert_eq!(embedding.len(), self.config.dimension);

        if let Some(&offset) = self.id_to_offset.get(&id) {
            // Update in place
            let start = offset;
            let end = offset + self.config.dimension;
            self.data[start..end].copy_from_slice(embedding);
        } else {
            // Insert new
            let offset = if let Some(slot) = self.free_slots.pop() {
                // Reuse deleted slot - MUST copy embedding into the reused slot
                let start = slot;
                let end = slot + self.config.dimension;
                self.data[start..end].copy_from_slice(embedding);
                slot
            } else {
                // Append to end
                let offset = self.data.len();
                self.data.extend_from_slice(embedding);
                offset
            };
            self.id_to_offset.insert(id, offset);
        }

        self.version.fetch_add(1, Ordering::Release);
        Ok(())
    }

    /// Delete a vector (marks slot as free)
    pub fn delete(&mut self, id: VectorId) -> bool {
        if let Some(offset) = self.id_to_offset.remove(&id) {
            // Mark slot as free for reuse
            self.free_slots.push(offset);
            // Zero out data (optional, for security)
            let start = offset;
            let end = offset + self.config.dimension;
            self.data[start..end].fill(0.0);
            self.version.fetch_add(1, Ordering::Release);
            true
        } else {
            false
        }
    }

    /// Get embedding by ID
    pub fn get(&self, id: VectorId) -> Option<&[f32]> {
        let offset = *self.id_to_offset.get(&id)?;
        let start = offset;
        let end = offset + self.config.dimension;
        Some(&self.data[start..end])
    }

    /// Iterate all vectors in deterministic order (sorted by VectorId)
    ///
    /// IMPORTANT: This uses BTreeMap which guarantees sorted iteration.
    /// This is critical for deterministic brute-force search.
    /// HashMap iteration would be nondeterministic.
    pub fn iter(&self) -> impl Iterator<Item = (VectorId, &[f32])> {
        // BTreeMap iterates in key order (VectorId ascending)
        self.id_to_offset.iter().map(|(&id, &offset)| {
            let start = offset;
            let end = offset + self.config.dimension;
            (id, &self.data[start..end])
        })
    }

    /// Count of active vectors
    pub fn len(&self) -> usize {
        self.id_to_offset.len()
    }
}
```

### 7.4 KV Metadata Structure

```rust
/// Metadata stored in KV (MessagePack serialized)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct VectorRecord {
    /// Internal vector ID (maps to heap)
    pub vector_id: u64,

    /// User-provided metadata
    pub metadata: Option<JsonValue>,

    /// Version for optimistic concurrency
    pub version: u64,

    /// Timestamp (microseconds)
    pub created_at: u64,
    pub updated_at: u64,
}

/// Key construction for vector metadata
impl Key {
    /// Create key for vector metadata
    /// Format: namespace + TypeTag::Vector + collection_name + "/" + vector_key
    pub fn new_vector(namespace: Namespace, collection: &str, key: &str) -> Self {
        let user_key = format!("{}/{}", collection, key);
        Key::new(namespace, TypeTag::Vector, user_key)
    }

    /// Create key for collection config
    /// Format: namespace + TypeTag::VectorConfig + collection_name
    pub fn new_vector_config(namespace: Namespace, collection: &str) -> Self {
        Key::new(namespace, TypeTag::VectorConfig, collection.to_string())
    }
}
```

### 7.5 TypeTag Extensions

```rust
/// Add to existing TypeTag enum
pub enum TypeTag {
    // Existing...
    Kv = 0x10,
    Json = 0x20,
    Event = 0x30,
    State = 0x40,
    Trace = 0x50,
    Run = 0x60,

    // M8 additions
    Vector = 0x70,
    VectorConfig = 0x71,
}
```

---

## 8. Index Backend Abstraction

### 8.1 VectorIndexBackend Trait

```rust
/// Trait for swappable vector index implementations
///
/// M8: BruteForceBackend
/// M9: HnswBackend
pub trait VectorIndexBackend: Send + Sync {
    /// Insert a vector (upsert semantics)
    fn insert(&mut self, id: VectorId, embedding: &[f32]) -> Result<()>;

    /// Delete a vector
    fn delete(&mut self, id: VectorId) -> Result<bool>;

    /// Search for k nearest neighbors
    /// Returns (VectorId, score) pairs, sorted by score descending
    fn search(&self, query: &[f32], k: usize) -> Vec<(VectorId, f32)>;

    /// Get number of indexed vectors
    fn len(&self) -> usize;

    /// Check if empty
    fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Get dimension
    fn dimension(&self) -> usize;

    /// Get metric
    fn metric(&self) -> DistanceMetric;
}
```

### 8.2 BruteForceBackend (M8 Implementation)

```rust
/// Brute-force vector search backend
///
/// Simple O(n) implementation for M8.
/// Sufficient for datasets < 10K vectors.
pub struct BruteForceBackend {
    /// Vector heap (contiguous storage)
    heap: VectorHeap,

    /// Distance metric
    metric: DistanceMetric,
}

impl BruteForceBackend {
    pub fn new(config: &VectorConfig) -> Self {
        BruteForceBackend {
            heap: VectorHeap::new(config.dimension),
            metric: config.metric,
        }
    }
}

impl VectorIndexBackend for BruteForceBackend {
    fn insert(&mut self, id: VectorId, embedding: &[f32]) -> Result<()> {
        self.heap.upsert(id, embedding)
    }

    fn delete(&mut self, id: VectorId) -> Result<bool> {
        Ok(self.heap.delete(id))
    }

    fn search(&self, query: &[f32], k: usize) -> Vec<(VectorId, f32)> {
        // IMPORTANT: heap.iter() returns vectors in VectorId order (BTreeMap)
        // This ensures deterministic iteration before scoring
        let mut results: Vec<(VectorId, f32)> = self.heap
            .iter()
            .map(|(id, embedding)| {
                let score = self.compute_similarity(query, embedding);
                (id, score)
            })
            .collect();

        // Sort by (score desc, VectorId asc) for determinism
        // CRITICAL: VectorId tie-break ensures identical results across runs
        results.sort_by(|(id_a, score_a), (id_b, score_b)| {
            score_b.partial_cmp(score_a)
                .unwrap_or(Ordering::Equal)
                .then_with(|| id_a.cmp(id_b))  // Deterministic tie-break
        });

        results.truncate(k);
        results
    }

    fn len(&self) -> usize {
        self.heap.len()
    }

    fn dimension(&self) -> usize {
        self.heap.config.dimension
    }

    fn metric(&self) -> DistanceMetric {
        self.metric
    }
}

impl BruteForceBackend {
    /// Compute similarity score (higher = more similar)
    fn compute_similarity(&self, a: &[f32], b: &[f32]) -> f32 {
        match self.metric {
            DistanceMetric::Cosine => {
                let dot = dot_product(a, b);
                let norm_a = l2_norm(a);
                let norm_b = l2_norm(b);
                if norm_a == 0.0 || norm_b == 0.0 {
                    0.0
                } else {
                    // Cosine similarity: [-1, 1], higher is more similar
                    dot / (norm_a * norm_b)
                }
            }
            DistanceMetric::Euclidean => {
                let dist = euclidean_distance(a, b);
                // Transform to similarity: 1 / (1 + dist)
                // Range: (0, 1], higher is more similar
                1.0 / (1.0 + dist)
            }
            DistanceMetric::DotProduct => {
                // Raw dot product (assumes normalized vectors)
                dot_product(a, b)
            }
        }
    }
}

// Helper functions
fn dot_product(a: &[f32], b: &[f32]) -> f32 {
    a.iter().zip(b.iter()).map(|(x, y)| x * y).sum()
}

fn l2_norm(v: &[f32]) -> f32 {
    v.iter().map(|x| x * x).sum::<f32>().sqrt()
}

fn euclidean_distance(a: &[f32], b: &[f32]) -> f32 {
    a.iter()
        .zip(b.iter())
        .map(|(x, y)| (x - y).powi(2))
        .sum::<f32>()
        .sqrt()
}
```

### 8.3 Future: HnswBackend (M9 Placeholder)

```rust
/// HNSW vector search backend (M9)
///
/// NOT IMPLEMENTED IN M8.
/// Placeholder to show trait compatibility.
pub struct HnswBackend {
    // HNSW graph structure
    // Configurable parameters (M, ef_construction, ef_search)
    // Incremental update support
}

impl VectorIndexBackend for HnswBackend {
    // Same trait, different implementation
    // O(log n) search instead of O(n)
}
```

---

## 9. Similarity Search

### 9.1 Search Flow

```
SearchRequest
     │
     ▼
┌─────────────────┐
│  Validate query │
│  - dimension    │
│  - collection   │
└────────┬────────┘
         │
         ▼
┌─────────────────┐
│  Get snapshot   │
│  (consistency)  │
└────────┬────────┘
         │
         ▼
┌─────────────────────────────┐
│  Index backend search       │
│  - BruteForce: O(n) scan   │
│  - HNSW (M9): O(log n)     │
└────────────┬────────────────┘
             │
             ▼
┌─────────────────────────────┐
│  Load metadata for matches  │
│  (from KV store)            │
└────────────┬────────────────┘
             │
             ▼
┌─────────────────────────────┐
│  Apply metadata filters     │
│  (post-filter in M8)        │
└────────────┬────────────────┘
             │
             ▼
┌─────────────────────────────┐
│  Apply tie-breaking         │
│  (score desc, key asc)      │
└────────────┬────────────────┘
             │
             ▼
       Vec<VectorMatch>
```

### 9.2 Search Implementation

```rust
impl VectorStore {
    /// Search for similar vectors
    pub fn search(
        &self,
        run_id: RunId,
        collection: &str,
        query: &[f32],
        k: usize,
        filter: Option<MetadataFilter>,
    ) -> Result<Vec<VectorMatch>> {
        // Get collection config
        let config = self.get_collection_config(run_id, collection)?
            .ok_or_else(|| VectorError::CollectionNotFound {
                name: collection.to_string()
            })?;

        // Validate query dimension
        if query.len() != config.config.dimension {
            return Err(VectorError::DimensionMismatch {
                expected: config.config.dimension,
                got: query.len(),
            });
        }

        // Get snapshot for consistency
        let snapshot = self.db.snapshot();

        // Get index backend
        let backend = self.get_index_backend(run_id, collection)?;

        // Search (may return more than k if filtering)
        let filter_factor = if filter.is_some() { 3 } else { 1 };
        let candidates = backend.search(query, k * filter_factor);

        // Load metadata and filter
        let mut matches = Vec::with_capacity(k);
        for (vector_id, score) in candidates {
            if matches.len() >= k {
                break;
            }

            // Load metadata from KV
            let key = self.id_to_key(run_id, collection, vector_id)?;
            let record = self.get_record(&snapshot, run_id, collection, &key)?;

            // Apply filter
            if let Some(ref f) = filter {
                if !f.matches(&record.metadata) {
                    continue;
                }
            }

            matches.push(VectorMatch {
                key,
                score,
                metadata: record.metadata,
            });
        }

        // Apply deterministic tie-breaking
        matches.sort_by(|a, b| {
            b.score.partial_cmp(&a.score)
                .unwrap_or(Ordering::Equal)
                .then_with(|| a.key.cmp(&b.key))
        });

        Ok(matches)
    }
}
```

### 9.3 Budget Enforcement

```rust
impl VectorStore {
    /// Search with M6 budget constraints
    pub fn search_with_budget(
        &self,
        run_id: RunId,
        collection: &str,
        query: &[f32],
        k: usize,
        filter: Option<MetadataFilter>,
        budget: &SearchBudget,
    ) -> Result<(Vec<VectorMatch>, bool)> {
        let start = Instant::now();

        // Early budget check
        if start.elapsed().as_micros() as u64 >= budget.max_wall_time_micros {
            return Ok((vec![], true));
        }

        // Search (internally checks budget)
        let matches = self.search(run_id, collection, query, k, filter)?;

        let truncated = start.elapsed().as_micros() as u64 >= budget.max_wall_time_micros;

        Ok((matches, truncated))
    }
}
```

---

## 10. Collection Management

### 10.1 Collection Lifecycle

```rust
impl VectorStore {
    /// Create a new collection
    pub fn create_collection(
        &self,
        run_id: RunId,
        name: &str,
        config: VectorConfig,
    ) -> Result<()> {
        // Validate name
        if name.is_empty() || name.len() > 255 || name.contains('/') {
            return Err(VectorError::InvalidCollectionName {
                name: name.to_string()
            });
        }

        // Validate dimension
        if config.dimension == 0 || config.dimension > 65536 {
            return Err(VectorError::InvalidDimension {
                dimension: config.dimension
            });
        }

        // Check if already exists
        if self.collection_exists(run_id, name)? {
            return Err(VectorError::CollectionAlreadyExists {
                name: name.to_string()
            });
        }

        // Store config in KV
        let config_key = Key::new_vector_config(
            Namespace::for_run(run_id),
            name,
        );

        let config_value = CollectionConfigRecord {
            config: config.clone(),
            created_at: now_micros(),
            count: 0,
        };

        self.db.put_raw(config_key, serialize(&config_value)?)?;

        // Initialize index backend
        self.init_index_backend(run_id, name, &config)?;

        // Write WAL entry
        self.write_wal_create_collection(run_id, name, &config)?;

        Ok(())
    }

    /// Delete a collection and all its vectors
    pub fn delete_collection(&self, run_id: RunId, name: &str) -> Result<()> {
        // Check exists
        if !self.collection_exists(run_id, name)? {
            return Err(VectorError::CollectionNotFound {
                name: name.to_string()
            });
        }

        // Delete all vectors (scan and delete)
        let prefix = Key::new_vector_prefix(Namespace::for_run(run_id), name);
        for key in self.db.scan_prefix(&prefix)? {
            self.db.delete_raw(key)?;
        }

        // Delete config
        let config_key = Key::new_vector_config(
            Namespace::for_run(run_id),
            name,
        );
        self.db.delete_raw(config_key)?;

        // Cleanup index backend
        self.cleanup_index_backend(run_id, name)?;

        // Write WAL entry
        self.write_wal_delete_collection(run_id, name)?;

        Ok(())
    }

    /// List all collections in a run
    pub fn list_collections(&self, run_id: RunId) -> Result<Vec<CollectionInfo>> {
        let prefix = Key::new_vector_config_prefix(Namespace::for_run(run_id));

        let mut collections = Vec::new();
        for (key, value) in self.db.scan_prefix(&prefix)? {
            let record: CollectionConfigRecord = deserialize(&value)?;
            let name = key.user_key_str().to_string();

            collections.push(CollectionInfo {
                name,
                config: record.config,
                count: record.count,
                created_at: record.created_at,
            });
        }

        // Sort by name for determinism
        collections.sort_by(|a, b| a.name.cmp(&b.name));

        Ok(collections)
    }

    /// Get collection info
    pub fn get_collection(
        &self,
        run_id: RunId,
        name: &str,
    ) -> Result<Option<CollectionInfo>> {
        let config_key = Key::new_vector_config(
            Namespace::for_run(run_id),
            name,
        );

        let Some(value) = self.db.get_raw(&config_key)? else {
            return Ok(None);
        };

        let record: CollectionConfigRecord = deserialize(&value)?;

        Ok(Some(CollectionInfo {
            name: name.to_string(),
            config: record.config,
            count: record.count,
            created_at: record.created_at,
        }))
    }
}
```

---

## 11. Transaction Integration

### 11.1 VectorStoreExt Trait

```rust
/// Extension trait for vector operations in transactions
pub trait VectorStoreExt {
    /// Insert a vector (upsert semantics)
    fn vector_insert(
        &mut self,
        collection: &str,
        key: &str,
        embedding: &[f32],
        metadata: Option<JsonValue>,
    ) -> Result<()>;

    /// Delete a vector
    fn vector_delete(&mut self, collection: &str, key: &str) -> Result<bool>;

    /// Get a vector
    fn vector_get(
        &mut self,
        collection: &str,
        key: &str,
    ) -> Result<Option<VectorEntry>>;
}

impl VectorStoreExt for TransactionContext {
    fn vector_insert(
        &mut self,
        collection: &str,
        key: &str,
        embedding: &[f32],
        metadata: Option<JsonValue>,
    ) -> Result<()> {
        // Validate collection exists and dimension matches
        let config = self.get_collection_config(collection)?
            .ok_or_else(|| VectorError::CollectionNotFound {
                name: collection.to_string()
            })?;

        if embedding.len() != config.dimension {
            return Err(VectorError::DimensionMismatch {
                expected: config.dimension,
                got: embedding.len(),
            }.into());
        }

        // Create or update record
        let vector_key = Key::new_vector(self.namespace(), collection, key);

        let existing = self.get(&vector_key)?;
        let vector_id = if let Some(existing_value) = existing {
            let record: VectorRecord = deserialize(&existing_value)?;
            VectorId(record.vector_id)
        } else {
            self.allocate_vector_id(collection)?
        };

        let record = VectorRecord {
            vector_id: vector_id.0,
            metadata,
            version: 1,
            created_at: now_micros(),
            updated_at: now_micros(),
        };

        // Store metadata
        self.put(vector_key, serialize(&record)?)?;

        // Update index backend (deferred to commit)
        self.pending_vector_ops.push(VectorOp::Insert {
            collection: collection.to_string(),
            vector_id,
            embedding: embedding.to_vec(),
        });

        Ok(())
    }

    fn vector_delete(&mut self, collection: &str, key: &str) -> Result<bool> {
        let vector_key = Key::new_vector(self.namespace(), collection, key);

        let Some(existing) = self.get(&vector_key)? else {
            return Ok(false);
        };

        let record: VectorRecord = deserialize(&existing)?;

        // Delete metadata
        self.delete(vector_key)?;

        // Queue index deletion (deferred to commit)
        self.pending_vector_ops.push(VectorOp::Delete {
            collection: collection.to_string(),
            vector_id: VectorId(record.vector_id),
        });

        Ok(true)
    }

    fn vector_get(
        &mut self,
        collection: &str,
        key: &str,
    ) -> Result<Option<VectorEntry>> {
        let vector_key = Key::new_vector(self.namespace(), collection, key);

        let Some(value) = self.get(&vector_key)? else {
            return Ok(None);
        };

        let record: VectorRecord = deserialize(&value)?;

        // Get embedding from index backend
        let embedding = self.get_embedding(collection, VectorId(record.vector_id))?;

        Ok(Some(VectorEntry {
            key: key.to_string(),
            embedding,
            metadata: record.metadata,
            vector_id: VectorId(record.vector_id),
            version: record.version,
        }))
    }
}
```

### 11.2 Cross-Primitive Transactions

```rust
// Example: Atomic KV + Vector operation
db.transaction(run_id, |txn| {
    // Store document in KV
    txn.kv_put("doc:123", json!({
        "title": "Example Document",
        "content": "This is the document content...",
    }))?;

    // Store embedding in Vector
    let embedding = embed("This is the document content...");
    txn.vector_insert(
        "documents",
        "doc:123",
        &embedding,
        Some(json!({ "type": "document" })),
    )?;

    Ok(())
})?;
```

---

## 12. M6 Search Integration

### 12.1 DocRef::Vector Variant

```rust
/// Add Vector variant to DocRef
pub enum DocRef {
    Kv { key: Key },
    Json { key: Key, doc_id: JsonDocId },
    Event { log_key: Key, seq: u64 },
    State { key: Key },
    Trace { key: Key, span_id: u64 },
    Run { run_id: RunId },

    // M8 addition
    Vector {
        collection: String,
        key: String,
    },
}

impl DocRef {
    pub fn primitive_kind(&self) -> PrimitiveKind {
        match self {
            // ...existing...
            DocRef::Vector { .. } => PrimitiveKind::Vector,
        }
    }
}
```

### 12.2 PrimitiveKind::Vector

```rust
/// Add Vector to PrimitiveKind
pub enum PrimitiveKind {
    Kv,
    Json,
    Event,
    State,
    Trace,
    Run,
    Vector,  // M8 addition
}
```

### 12.3 Searchable Implementation

**IMPORTANT DESIGN DECISION**: Vector primitive does NOT support keyword search natively.

Vector search is fundamentally different from keyword search:
- Keyword search: text query → BM25 scoring over text
- Vector search: embedding query → similarity scoring over vectors

Vector primitive participates in hybrid search via **explicit embedding queries**, not by reimplementing text search on metadata. The M6 hybrid search orchestrator is responsible for:
1. Deciding whether to invoke vector search
2. Providing the query embedding (from an external embedding model)
3. Fusing vector results with keyword results

```rust
impl Searchable for VectorStore {
    /// Vector search via M6 interface
    ///
    /// NOTE: For SearchMode::Keyword, Vector returns empty results.
    /// Vector does not attempt to do keyword matching on metadata.
    /// Keyword search on vector metadata is the responsibility of
    /// the hybrid search layer, not the vector primitive.
    ///
    /// For SearchMode::Vector or SearchMode::Hybrid, the caller must
    /// provide the query embedding via VectorSearchRequest extension.
    fn search(&self, req: &SearchRequest) -> Result<SearchResponse> {
        let start = Instant::now();

        // Vector primitive only responds to Vector or Hybrid mode
        // with an explicit query embedding
        match req.mode {
            SearchMode::Keyword => {
                // Vector does NOT do keyword search
                // Return empty - hybrid orchestrator handles this
                return Ok(SearchResponse {
                    hits: vec![],
                    truncated: false,
                    stats: SearchStats {
                        elapsed_micros: start.elapsed().as_micros() as u64,
                        candidates_considered: 0,
                        candidates_by_primitive: HashMap::new(),
                        index_used: false,
                    },
                });
            }
            SearchMode::Vector | SearchMode::Hybrid => {
                // Requires query embedding - see search_by_embedding()
                // If no embedding provided, return empty
                // The hybrid orchestrator should call search_by_embedding() directly
                return Ok(SearchResponse {
                    hits: vec![],
                    truncated: false,
                    stats: SearchStats {
                        elapsed_micros: start.elapsed().as_micros() as u64,
                        candidates_considered: 0,
                        candidates_by_primitive: HashMap::new(),
                        index_used: false,
                    },
                });
            }
        }
    }

    fn primitive_kind(&self) -> PrimitiveKind {
        PrimitiveKind::Vector
    }
}
```

**How Hybrid Search Works With Vectors**:

The M6 `HybridSearch` orchestrator is responsible for:
1. Embedding the text query (using an external model, not M8 scope)
2. Calling `vector.search_by_embedding()` with the embedding
3. Fusing results via RRF

```rust
impl HybridSearch {
    pub fn search(&self, req: &SearchRequest) -> Result<SearchResponse> {
        let mut primitive_results = Vec::new();

        // Keyword primitives (KV, JSON, Event, etc.)
        for primitive in [Kv, Json, Event, State, Trace, Run] {
            if req.mode == SearchMode::Keyword || req.mode == SearchMode::Hybrid {
                let result = self.search_primitive(primitive, req)?;
                primitive_results.push((primitive, result));
            }
        }

        // Vector primitive (requires embedding)
        if req.mode == SearchMode::Vector || req.mode == SearchMode::Hybrid {
            if let Some(query_embedding) = self.get_query_embedding(req) {
                // Embedding provided by caller or external model
                let vector_req = VectorSearchRequest {
                    collection: req.vector_collection.clone().unwrap_or_default(),
                    query_embedding,
                    k: req.k,
                    filter: None,
                };
                let result = self.db.vector().search_by_embedding(req.run_id, &vector_req)?;
                primitive_results.push((PrimitiveKind::Vector, result));
            }
            // If no embedding available, vector is skipped (not an error)
        }

        self.fuse_results(primitive_results, req)
    }
}
```

### 12.4 Vector-Specific Search API

```rust
impl VectorStore {
    /// Search with embedding query (for hybrid search)
    pub fn search_by_embedding(
        &self,
        run_id: RunId,
        request: &VectorSearchRequest,
    ) -> Result<SearchResponse> {
        let start = Instant::now();

        let matches = self.search(
            run_id,
            &request.collection,
            &request.query_embedding,
            request.k,
            request.filter.clone(),
        )?;

        let hits: Vec<SearchHit> = matches
            .into_iter()
            .enumerate()
            .map(|(i, m)| SearchHit {
                ref_: DocRef::Vector {
                    collection: request.collection.clone(),
                    key: m.key,
                },
                score: m.score,
                rank: (i + 1) as u32,
                snippet: None,
                debug: None,
            })
            .collect();

        Ok(SearchResponse {
            hits,
            truncated: false,
            stats: SearchStats {
                elapsed_micros: start.elapsed().as_micros() as u64,
                candidates_considered: matches.len(),
                candidates_by_primitive: [(PrimitiveKind::Vector, matches.len())]
                    .into_iter()
                    .collect(),
                index_used: true,
            },
        })
    }
}

/// Vector-specific search request
pub struct VectorSearchRequest {
    pub collection: String,
    pub query_embedding: Vec<f32>,
    pub k: usize,
    pub filter: Option<MetadataFilter>,
}
```

### 12.5 Hybrid Search Integration

```rust
impl HybridSearch {
    pub fn search(&self, req: &SearchRequest) -> Result<SearchResponse> {
        // ... existing primitive searches ...

        // Add vector search
        if self.should_search_primitive(PrimitiveKind::Vector, req) {
            let vector_results = self.db.vector().search(req)?;
            primitive_results.push((PrimitiveKind::Vector, vector_results));
        }

        // Fuse all results with RRF
        self.fuse_results(primitive_results, req)
    }
}
```

---

## 13. WAL Integration

### 13.1 WAL Entry Types

```rust
/// Vector WAL entry types (0x70-0x7F range)
///
/// Naming convention:
/// - COLLECTION_CREATE/DELETE: prefixed to distinguish from vector-level ops
/// - UPSERT (not INSERT): matches our semantic (insert overwrites if exists)
pub const WAL_VECTOR_COLLECTION_CREATE: u8 = 0x70;
pub const WAL_VECTOR_COLLECTION_DELETE: u8 = 0x71;
pub const WAL_VECTOR_UPSERT: u8 = 0x72;
pub const WAL_VECTOR_DELETE: u8 = 0x73;

impl WalEntryType {
    pub fn from_byte(byte: u8) -> Option<Self> {
        match byte {
            // ... existing ...
            0x70 => Some(WalEntryType::VectorCollectionCreate),
            0x71 => Some(WalEntryType::VectorCollectionDelete),
            0x72 => Some(WalEntryType::VectorUpsert),
            0x73 => Some(WalEntryType::VectorDelete),
            _ => None,
        }
    }

    pub fn primitive_kind(&self) -> Option<PrimitiveKind> {
        match self {
            // ... existing ...
            WalEntryType::VectorCollectionCreate |
            WalEntryType::VectorCollectionDelete |
            WalEntryType::VectorUpsert |
            WalEntryType::VectorDelete => Some(PrimitiveKind::Vector),
            _ => None,
        }
    }
}
```

### 13.2 WAL Entry Payloads

```rust
/// WAL payload for collection creation
#[derive(Serialize, Deserialize)]
pub struct WalVectorCollectionCreate {
    pub run_id: RunId,
    pub collection: String,
    pub config: VectorConfig,
    pub timestamp: u64,
}

/// WAL payload for collection deletion
#[derive(Serialize, Deserialize)]
pub struct WalVectorCollectionDelete {
    pub run_id: RunId,
    pub collection: String,
    pub timestamp: u64,
}

/// WAL payload for vector upsert
///
/// WARNING: TEMPORARY M8 FORMAT
/// This payload contains the full embedding, which:
/// - Bloats WAL size significantly (3KB per 768-dim vector)
/// - Slows down recovery proportionally
///
/// This is acceptable for M8 (correctness over performance).
///
/// M9 MAY change this to:
/// - Store embeddings in separate segment
/// - Use delta encoding for updates
/// - Reference external embedding storage
///
/// Any such change MUST be versioned and backward compatible.
#[derive(Serialize, Deserialize)]
pub struct WalVectorUpsert {
    pub run_id: RunId,
    pub collection: String,
    pub key: String,
    pub vector_id: u64,
    pub embedding: Vec<f32>,  // TEMPORARY: Full embedding in WAL
    pub metadata: Option<JsonValue>,
    pub timestamp: u64,
}

/// WAL payload for vector delete
#[derive(Serialize, Deserialize)]
pub struct WalVectorDelete {
    pub run_id: RunId,
    pub collection: String,
    pub key: String,
    pub vector_id: u64,
    pub timestamp: u64,
}
```

### 13.3 WAL Entry Serialization

```rust
impl VectorStore {
    fn write_wal_insert(
        &self,
        run_id: RunId,
        collection: &str,
        key: &str,
        vector_id: VectorId,
        embedding: &[f32],
        metadata: Option<JsonValue>,
    ) -> Result<()> {
        let payload = WalVectorUpsert {
            run_id,
            collection: collection.to_string(),
            key: key.to_string(),
            vector_id: vector_id.0,
            embedding: embedding.to_vec(),
            metadata,
            timestamp: now_micros(),
        };

        let entry = WalEntry {
            entry_type: WalEntryType::VectorUpsert,
            version: 1,
            tx_id: self.current_tx_id(),
            payload: serialize(&payload)?,
        };

        self.db.wal().write_entry(&entry)
    }
}
```

---

## 14. Snapshot & Recovery

### 14.1 PrimitiveStorageExt Implementation

```rust
impl PrimitiveStorageExt for VectorStorage {
    fn wal_entry_types(&self) -> &'static [u8] {
        &[0x70, 0x71, 0x72, 0x73]
    }

    fn primitive_type_id(&self) -> u8 {
        7  // After existing 6 primitives
    }

    fn snapshot_serialize(&self) -> Result<Vec<u8>> {
        let mut data = Vec::new();

        // Write section header (fixed format)
        data.push(0x07);  // Primitive ID: Vector
        data.push(0x01);  // Format Version: M8

        // Placeholder for section length (will fill in at end)
        let length_offset = data.len();
        data.extend_from_slice(&[0u8; 8]);

        // Collection count
        let collection_count = self.backends.len() as u32;
        data.extend_from_slice(&collection_count.to_le_bytes());

        // Serialize each collection
        for (collection_id, backend) in &self.backends {
            // Collection header (MessagePack)
            let header = CollectionSnapshotHeader {
                run_id: collection_id.run_id,
                name: collection_id.name.clone(),
                config: backend.config().clone(),
                next_id: backend.heap.next_id.load(Ordering::Relaxed),  // CRITICAL
                free_slots: backend.heap.free_slots.clone(),  // CRITICAL
                count: backend.len() as u32,
            };
            let header_bytes = serialize(&header)?;
            // Length-prefix the header
            data.extend_from_slice(&(header_bytes.len() as u32).to_le_bytes());
            data.extend(header_bytes);

            // Serialize all vectors (deterministic order via BTreeMap)
            for (id, embedding) in backend.iter() {
                // vector_id: u64 LE
                data.extend_from_slice(&id.0.to_le_bytes());
                // embedding: [f32] as raw bytes
                for &val in embedding {
                    data.extend_from_slice(&val.to_le_bytes());
                }
            }
        }

        // Fill in section length
        let section_length = (data.len() - length_offset - 8) as u64;
        data[length_offset..length_offset + 8].copy_from_slice(&section_length.to_le_bytes());

        Ok(data)
    }

    fn snapshot_deserialize(&mut self, data: &[u8]) -> Result<()> {
        let mut pos = 0;

        // Read section header
        let primitive_id = data[pos];
        pos += 1;
        if primitive_id != 0x07 {
            return Err(SnapshotError::InvalidPrimitiveId { expected: 0x07, got: primitive_id });
        }

        let format_version = data[pos];
        pos += 1;
        if format_version != 0x01 {
            return Err(SnapshotError::UnsupportedVersion { version: format_version });
        }

        let section_length = u64::from_le_bytes(data[pos..pos+8].try_into()?);
        pos += 8;

        // Collection count
        let collection_count = u32::from_le_bytes(data[pos..pos+4].try_into()?) as usize;
        pos += 4;

        // Read each collection
        for _ in 0..collection_count {
            // Header length
            let header_len = u32::from_le_bytes(data[pos..pos+4].try_into()?) as usize;
            pos += 4;

            // Header (MessagePack)
            let header: CollectionSnapshotHeader = deserialize(&data[pos..pos+header_len])?;
            pos += header_len;

            // Create backend with restored state
            let mut backend = BruteForceBackend::new(&header.config);
            backend.heap.next_id = AtomicU64::new(header.next_id);  // CRITICAL
            backend.heap.free_slots = header.free_slots.clone();  // CRITICAL

            // Read vectors
            for _ in 0..header.count {
                let vector_id = VectorId(u64::from_le_bytes(data[pos..pos+8].try_into()?));
                pos += 8;

                let dim = header.config.dimension;
                let mut embedding = Vec::with_capacity(dim);
                for _ in 0..dim {
                    embedding.push(f32::from_le_bytes(data[pos..pos+4].try_into()?));
                    pos += 4;
                }

                backend.insert(vector_id, &embedding)?;
            }

            // Store backend
            let collection_id = CollectionId {
                run_id: header.run_id,
                name: header.name,
            };
            self.backends.insert(collection_id, backend);
        }

        Ok(())
    }

    /// Apply a single WAL entry during replay
    ///
    /// IMPORTANT: This method is called by the global WAL replay mechanism.
    /// Vector WAL replay does NOT happen independently. The global replayer:
    /// 1. Reads WAL entries in order
    /// 2. Groups entries by transaction ID
    /// 3. Only applies committed transactions
    /// 4. Calls apply_wal_entry() for each entry in commit order
    ///
    /// This ensures:
    /// - Transaction atomicity (partial transactions are not applied)
    /// - Order preservation (entries applied in WAL order)
    /// - Cross-primitive atomicity (KV + Vector in same transaction)
    fn apply_wal_entry(&mut self, entry: &WalEntry) -> Result<()> {
        match entry.entry_type {
            WalEntryType::VectorCollectionCreate => {
                let payload: WalVectorCollectionCreate = deserialize(&entry.payload)?;
                self.create_collection(
                    payload.run_id,
                    &payload.collection,
                    payload.config,
                )?;
            }
            WalEntryType::VectorCollectionDelete => {
                let payload: WalVectorCollectionDelete = deserialize(&entry.payload)?;
                self.delete_collection(payload.run_id, &payload.collection)?;
            }
            WalEntryType::VectorUpsert => {
                let payload: WalVectorUpsert = deserialize(&entry.payload)?;
                self.insert_raw(
                    payload.run_id,
                    &payload.collection,
                    &payload.key,
                    VectorId(payload.vector_id),
                    &payload.embedding,
                    payload.metadata,
                )?;
            }
            WalEntryType::VectorDelete => {
                let payload: WalVectorDelete = deserialize(&entry.payload)?;
                self.delete_raw(
                    payload.run_id,
                    &payload.collection,
                    &payload.key,
                    VectorId(payload.vector_id),
                )?;
            }
            _ => {}
        }
        Ok(())
    }
}
```

### 14.2 Snapshot Format

**Encoding**: MessagePack (consistent with other primitives)
**Endianness**: Little-endian for all numeric values
**Version**: Snapshot section includes version byte for forward compatibility

```
Vector Snapshot Section:
+-------------------------------+
| Section Header (fixed)        |
|  - Primitive ID: u8           |  0x07 (Vector)
|  - Format Version: u8         |  0x01 (M8 format)
|  - Section Length: u64 LE     |  Total bytes following
+-------------------------------+
| Collection Count: u32 LE      |
+-------------------------------+
| Collection 1                  |
|  +---------------------------+|
|  | Header Length: u32 LE     ||
|  +---------------------------+|
|  | Header (MessagePack)      ||
|  |  - run_id: u64            ||
|  |  - name: String           ||
|  |  - dimension: u32         ||
|  |  - metric: u8             ||
|  |  - storage_dtype: u8      ||
|  |  - next_id: u64           ||  CRITICAL: For ID uniqueness
|  |  - free_slots: Vec<usize> ||  CRITICAL: For slot reuse
|  |  - count: u32             ||
|  +---------------------------+|
|  | Vectors (raw bytes)       ||
|  |  For each vector:         ||
|  |   - vector_id: u64 LE     ||
|  |   - embedding: [f32 LE]   ||  dimension * 4 bytes
|  +---------------------------+|
+-------------------------------+
| Collection 2                  |
+-------------------------------+
| ...                           |
+-------------------------------+
```

**Critical Fields for Recovery**:
- `next_id`: Without this, recovery would start allocating from 0, reusing IDs
- `free_slots`: Without this, deleted slot tracking would be lost, causing data corruption

**Version Compatibility**:
- Format version 0x01: M8 initial format
- Future versions must support reading 0x01 format
- Unknown versions: fail loudly, do not guess

**Forward Compatibility Contract**:
```rust
fn snapshot_deserialize(&mut self, data: &[u8]) -> Result<()> {
    let version = data[1];
    match version {
        0x01 => self.deserialize_v1(data),
        v => Err(SnapshotError::UnsupportedVersion { version: v }),
    }
}
```

**IMPORTANT**: M9 may change format (e.g., for HNSW graph storage). Version byte ensures clean migration.

---

## 15. API Design

### 15.1 VectorStore Public API

```rust
/// Stateless facade for vector operations
#[derive(Clone)]
pub struct VectorStore {
    db: Arc<Database>,
}

impl VectorStore {
    /// Get VectorStore from Database
    pub fn new(db: Arc<Database>) -> Self {
        VectorStore { db }
    }

    // ========== Collection Management ==========

    /// Create a new collection
    pub fn create_collection(
        &self,
        run_id: RunId,
        name: &str,
        config: VectorConfig,
    ) -> Result<()>;

    /// Delete a collection and all its vectors
    pub fn delete_collection(
        &self,
        run_id: RunId,
        name: &str,
    ) -> Result<()>;

    /// List all collections in a run
    pub fn list_collections(&self, run_id: RunId) -> Result<Vec<CollectionInfo>>;

    /// Get collection info
    pub fn get_collection(
        &self,
        run_id: RunId,
        name: &str,
    ) -> Result<Option<CollectionInfo>>;

    /// Check if collection exists
    pub fn collection_exists(&self, run_id: RunId, name: &str) -> Result<bool>;

    // ========== Vector Operations ==========

    /// Insert or update a vector (upsert semantics)
    pub fn insert(
        &self,
        run_id: RunId,
        collection: &str,
        key: &str,
        embedding: &[f32],
        metadata: Option<JsonValue>,
    ) -> Result<()>;

    /// Get a vector by key
    pub fn get(
        &self,
        run_id: RunId,
        collection: &str,
        key: &str,
    ) -> Result<Option<VectorEntry>>;

    /// Delete a vector by key
    pub fn delete(
        &self,
        run_id: RunId,
        collection: &str,
        key: &str,
    ) -> Result<bool>;

    /// Check if vector exists
    pub fn exists(
        &self,
        run_id: RunId,
        collection: &str,
        key: &str,
    ) -> Result<bool>;

    /// Count vectors in collection
    pub fn count(&self, run_id: RunId, collection: &str) -> Result<usize>;

    // ========== Search ==========

    /// Search for similar vectors
    pub fn search(
        &self,
        run_id: RunId,
        collection: &str,
        query: &[f32],
        k: usize,
        filter: Option<MetadataFilter>,
    ) -> Result<Vec<VectorMatch>>;

    // ========== M6 Integration ==========

    /// Search with M6 SearchRequest
    pub fn search_request(
        &self,
        run_id: RunId,
        request: &SearchRequest,
    ) -> Result<SearchResponse>;

    // ========== Convenience ==========

    /// Insert multiple vectors (not optimized in M8)
    pub fn insert_many(
        &self,
        run_id: RunId,
        collection: &str,
        entries: &[(String, Vec<f32>, Option<JsonValue>)],
    ) -> Result<()> {
        for (key, embedding, metadata) in entries {
            self.insert(run_id, collection, key, embedding, metadata.clone())?;
        }
        Ok(())
    }
}
```

### 15.2 Database Extension

```rust
impl Database {
    /// Get vector store facade
    pub fn vector(&self) -> VectorStore {
        VectorStore::new(Arc::new(self.clone()))
    }
}
```

### 15.3 Usage Examples

```rust
// Create a collection
let db = Database::open("./data")?;
let run_id = RunId::new();

db.vector().create_collection(
    run_id,
    "documents",
    VectorConfig {
        dimension: 768,
        metric: DistanceMetric::Cosine,
        storage_dtype: StorageDtype::F32,
    },
)?;

// Insert vectors
let embedding = embed("Hello, world!");  // Your embedding function
db.vector().insert(
    run_id,
    "documents",
    "doc:1",
    &embedding,
    Some(json!({ "title": "Greeting", "category": "example" })),
)?;

// Search
let query = embed("greeting message");
let results = db.vector().search(
    run_id,
    "documents",
    &query,
    10,
    Some(MetadataFilter {
        equals: [("category".to_string(), JsonScalar::String("example".to_string()))]
            .into_iter()
            .collect(),
    }),
)?;

for match_ in results {
    println!("{}: {:.4}", match_.key, match_.score);
}

// Cross-primitive transaction
db.transaction(run_id, |txn| {
    txn.kv_put("doc:1:content", "Hello, world!")?;
    txn.vector_insert("documents", "doc:1", &embedding, None)?;
    Ok(())
})?;

// Hybrid search (M6)
let response = db.hybrid().search(&SearchRequest {
    run_id,
    query: "greeting".to_string(),
    k: 10,
    budget: SearchBudget::default(),
    mode: SearchMode::Keyword,
    primitive_filter: None,
    time_range: None,
    tags_any: vec![],
})?;
```

---

## 16. Performance Characteristics

### 16.1 M8 Performance Expectations

**M8 prioritizes correctness over speed.**

| Operation | Target | Notes |
|-----------|--------|-------|
| Insert (768 dims) | < 50 µs | Plus WAL write |
| Get | < 10 µs | Fast path |
| Delete | < 30 µs | Plus WAL write |
| Search 1K vectors | < 5 ms | Brute-force |
| Search 10K vectors | < 50 ms | Brute-force |
| Search 50K vectors | < 200 ms | Acceptable |
| Search 100K vectors | > 500 ms | Forces M9 |

### 16.2 Memory Overhead

| Component | Size |
|-----------|------|
| Per vector (768 dims) | 3 KB (768 × 4 bytes) |
| Metadata per vector | ~100 bytes (typical) |
| Collection overhead | ~1 KB |
| Index overhead | ~8 bytes per vector (ID mapping) |

### 16.3 Scaling Characteristics

| Dataset Size | Memory (768 dims) | Search Latency |
|--------------|-------------------|----------------|
| 1K vectors | ~3 MB | < 5 ms |
| 10K vectors | ~30 MB | < 50 ms |
| 100K vectors | ~300 MB | < 500 ms |
| 1M vectors | ~3 GB | > 5 seconds (needs HNSW) |

### 16.4 Switch Threshold

**When to prioritize M9 (HNSW)**:
- P95 search latency > 100 ms
- Dataset exceeds 50K vectors
- Search QPS requirements > 10/sec at 10K+ vectors

---

## 17. Testing Strategy

### 17.1 Core Invariant Tests

```rust
#[test]
fn test_dimension_immutable() {
    let db = test_db();
    let run_id = RunId::new();

    db.vector().create_collection(run_id, "test", VectorConfig {
        dimension: 128,
        metric: DistanceMetric::Cosine,
        storage_dtype: StorageDtype::F32,
    })?;

    // Wrong dimension should fail
    let wrong_dim = vec![0.0f32; 256];
    let result = db.vector().insert(run_id, "test", "key1", &wrong_dim, None);
    assert!(matches!(result, Err(VectorError::DimensionMismatch { .. })));
}

#[test]
fn test_upsert_semantics() {
    let db = test_db();
    let run_id = RunId::new();

    db.vector().create_collection(run_id, "test", VectorConfig::for_minilm())?;

    let v1 = vec![1.0f32; 384];
    let v2 = vec![2.0f32; 384];

    // First insert
    db.vector().insert(run_id, "test", "key1", &v1, None)?;

    // Upsert (overwrite)
    db.vector().insert(run_id, "test", "key1", &v2, None)?;

    // Should have new value
    let entry = db.vector().get(run_id, "test", "key1")?.unwrap();
    assert_eq!(entry.embedding, v2);

    // Count should be 1 (not 2)
    assert_eq!(db.vector().count(run_id, "test")?, 1);
}

#[test]
fn test_deterministic_ordering() {
    let db = test_db();
    let run_id = RunId::new();

    db.vector().create_collection(run_id, "test", VectorConfig::for_minilm())?;

    // Insert vectors with similar scores
    for i in 0..100 {
        let v = vec![(i as f32) / 100.0; 384];
        db.vector().insert(run_id, "test", &format!("key{}", i), &v, None)?;
    }

    let query = vec![0.5f32; 384];

    // Search twice
    let r1 = db.vector().search(run_id, "test", &query, 10, None)?;
    let r2 = db.vector().search(run_id, "test", &query, 10, None)?;

    // Must be identical
    for (a, b) in r1.iter().zip(r2.iter()) {
        assert_eq!(a.key, b.key);
        assert!((a.score - b.score).abs() < 0.0001);
    }
}
```

### 17.2 Transaction Tests

```rust
#[test]
fn test_cross_primitive_atomicity() {
    let db = test_db();
    let run_id = RunId::new();

    db.vector().create_collection(run_id, "test", VectorConfig::for_minilm())?;

    // Atomic insert
    db.transaction(run_id, |txn| {
        txn.kv_put("doc:1", "content")?;
        txn.vector_insert("test", "doc:1", &vec![1.0f32; 384], None)?;
        Ok(())
    })?;

    // Both should exist
    assert!(db.kv().get(run_id, "doc:1")?.is_some());
    assert!(db.vector().exists(run_id, "test", "doc:1")?);

    // Atomic failure
    let result = db.transaction(run_id, |txn| {
        txn.kv_put("doc:2", "content")?;
        txn.vector_insert("test", "doc:2", &vec![1.0f32; 384], None)?;
        Err::<(), _>(anyhow::anyhow!("rollback"))
    });

    assert!(result.is_err());

    // Neither should exist
    assert!(db.kv().get(run_id, "doc:2")?.is_none());
    assert!(!db.vector().exists(run_id, "test", "doc:2")?);
}
```

### 17.3 Search Tests

```rust
#[test]
fn test_cosine_similarity() {
    let db = test_db();
    let run_id = RunId::new();

    db.vector().create_collection(run_id, "test", VectorConfig {
        dimension: 3,
        metric: DistanceMetric::Cosine,
        storage_dtype: StorageDtype::F32,
    })?;

    // Insert orthogonal vectors
    db.vector().insert(run_id, "test", "x", &[1.0, 0.0, 0.0], None)?;
    db.vector().insert(run_id, "test", "y", &[0.0, 1.0, 0.0], None)?;
    db.vector().insert(run_id, "test", "z", &[0.0, 0.0, 1.0], None)?;

    // Search for x-axis
    let results = db.vector().search(run_id, "test", &[1.0, 0.0, 0.0], 3, None)?;

    // "x" should be first (score = 1.0)
    assert_eq!(results[0].key, "x");
    assert!((results[0].score - 1.0).abs() < 0.0001);

    // Others should have score = 0 (orthogonal)
    assert!(results[1].score.abs() < 0.0001);
    assert!(results[2].score.abs() < 0.0001);
}

#[test]
fn test_metadata_filter() {
    let db = test_db();
    let run_id = RunId::new();

    db.vector().create_collection(run_id, "test", VectorConfig::for_minilm())?;

    // Insert with different categories
    for i in 0..10 {
        let category = if i % 2 == 0 { "even" } else { "odd" };
        db.vector().insert(
            run_id, "test", &format!("key{}", i),
            &vec![i as f32 / 10.0; 384],
            Some(json!({ "category": category })),
        )?;
    }

    // Search with filter
    let results = db.vector().search(
        run_id, "test",
        &vec![0.5f32; 384],
        10,
        Some(MetadataFilter {
            equals: [("category".to_string(), JsonScalar::String("even".to_string()))]
                .into_iter()
                .collect(),
        }),
    )?;

    // All results should be "even"
    for m in &results {
        let meta = m.metadata.as_ref().unwrap();
        assert_eq!(meta["category"], "even");
    }
}
```

### 17.4 Recovery Tests

```rust
#[test]
fn test_vector_recovery() {
    let dir = tempdir()?;

    // Create and populate
    {
        let db = Database::open(&dir)?;
        let run_id = RunId::new();

        db.vector().create_collection(run_id, "test", VectorConfig::for_minilm())?;

        for i in 0..100 {
            db.vector().insert(
                run_id, "test", &format!("key{}", i),
                &vec![i as f32; 384],
                Some(json!({ "index": i })),
            )?;
        }

        db.snapshot()?;
    }

    // Recover
    {
        let (db, result) = Database::recover(&dir, Default::default())?;

        // Verify all vectors recovered
        assert_eq!(db.vector().count(run_id, "test")?, 100);

        // Verify data integrity
        for i in 0..100 {
            let entry = db.vector().get(run_id, "test", &format!("key{}", i))?.unwrap();
            assert_eq!(entry.embedding[0], i as f32);
            assert_eq!(entry.metadata.unwrap()["index"], i);
        }
    }
}
```

### 17.5 M6 Integration Tests

```rust
#[test]
fn test_hybrid_search_includes_vectors() {
    let db = test_db();
    let run_id = RunId::new();

    // Setup KV and Vector data
    db.kv().put(run_id, "doc1", json!({ "text": "hello world" }))?;

    db.vector().create_collection(run_id, "docs", VectorConfig::for_minilm())?;
    db.vector().insert(
        run_id, "docs", "doc1",
        &embed("hello world"),
        Some(json!({ "text": "hello world" })),
    )?;

    // Hybrid search
    let response = db.hybrid().search(&SearchRequest {
        run_id,
        query: "hello".to_string(),
        k: 10,
        budget: SearchBudget::default(),
        mode: SearchMode::Keyword,
        primitive_filter: None,
        time_range: None,
        tags_any: vec![],
    })?;

    // Should have results from both KV and Vector
    let primitives: HashSet<_> = response.hits
        .iter()
        .map(|h| h.ref_.primitive_kind())
        .collect();

    assert!(primitives.contains(&PrimitiveKind::Kv));
    assert!(primitives.contains(&PrimitiveKind::Vector));
}
```

---

## 18. Known Limitations

### 18.1 M8 Limitations (Intentional)

| Limitation | Impact | Mitigation |
|------------|--------|------------|
| **Brute-force only** | O(n) search | Add HNSW in M9 |
| **No quantization** | Higher memory usage | Add in M9 |
| **Post-filtering only** | May scan more than needed | Add pre-filtering in M9 |
| **No batch optimization** | insert_many is loop | Optimize in M9 |
| **Simple metadata filter** | Equality only | Add ranges in M9 |
| **Single-threaded search** | No parallelism | M9 may add parallel scan |
| **Non-interruptible brute-force** | Budget enforced at boundaries only | HNSW can be interruptible |
| **Full embeddings in WAL** | Large WAL, slow recovery | M9 may optimize |

### 18.2 Critical Implementation Constraints

#### A. Single-Threaded, Deterministic Computation

**M8 MUST remain single-threaded for similarity computation.**

Floating point arithmetic is sensitive to:
- Order of operations (associativity)
- SIMD instruction selection
- Thread scheduling with parallel iterators

```rust
// CORRECT: Single-threaded, deterministic
fn compute_all_similarities(&self, query: &[f32]) -> Vec<(VectorId, f32)> {
    self.heap.iter()  // Sequential iteration
        .map(|(id, emb)| (id, self.compute_similarity(query, emb)))
        .collect()
}

// WRONG: Parallel, nondeterministic
fn compute_all_similarities(&self, query: &[f32]) -> Vec<(VectorId, f32)> {
    self.heap.par_iter()  // NEVER in M8 - breaks replay determinism
        .map(|(id, emb)| (id, self.compute_similarity(query, emb)))
        .collect()
}
```

**No rayon. No parallel iterators. No SIMD optimizations that change results.**

M9 may introduce parallelism with explicit determinism contracts.

#### B. Budget Enforcement Is Coarse-Grained

Brute-force search cannot be interrupted mid-computation:

```rust
// Budget only checked at phase boundaries
fn search(&self, query: &[f32], k: usize, budget: &SearchBudget) -> Result<...> {
    // Check 1: Before starting
    if budget_exceeded(start, budget) { return early; }

    // Brute-force scan - CANNOT be interrupted
    let results = self.compute_all_similarities(query);  // Runs to completion

    // Check 2: After scan
    if budget_exceeded(start, budget) { truncated = true; }

    // Sorting, filtering - also runs to completion
    ...
}
```

**Implication**: For large datasets, budget may be significantly exceeded before check.

M9's HNSW can check budget during graph traversal (interruptible).

#### C. No Implicit Normalization

**Vector primitive does NOT silently normalize embeddings.**

```rust
// CORRECT: Require explicit normalization or document behavior
pub fn insert(&self, ..., embedding: &[f32], ...) -> Result<()> {
    // Store as-is, no normalization
    self.heap.upsert(id, embedding)?;
}

// If DotProduct metric is used with unnormalized vectors,
// results may be unexpected. This is the caller's responsibility.

// WRONG: Silent normalization
pub fn insert(&self, ..., embedding: &[f32], ...) -> Result<()> {
    let normalized = normalize(embedding);  // NEVER silently normalize
    self.heap.upsert(id, &normalized)?;
}
```

**Options for callers**:
1. Use Cosine metric (handles normalization internally)
2. Pre-normalize vectors before insert (for DotProduct)
3. Accept unnormalized DotProduct behavior

Future: May add `normalized: bool` flag to VectorConfig.

### 18.3 What M8 Explicitly Does NOT Provide

- HNSW or other ANN indexes
- Quantization (F16, Int8, PQ)
- Pre-filtering
- Batch insert optimization
- Range metadata filters
- Nested path metadata filters
- GPU acceleration
- Distributed vector search

These are all **intentionally deferred**, not forgotten.

---

## 19. Future Extension Points

### 19.1 M9: HNSW Backend

```rust
/// M9 adds HNSW backend
pub struct HnswBackend {
    graph: HnswGraph,
    config: HnswConfig,
}

pub struct HnswConfig {
    /// Max connections per layer
    pub m: usize,
    /// Construction ef
    pub ef_construction: usize,
    /// Search ef
    pub ef_search: usize,
}

impl VectorIndexBackend for HnswBackend {
    // O(log n) search instead of O(n)
}
```

### 19.2 M9: Quantization

```rust
/// M9 adds quantization
pub enum StorageDtype {
    F32,
    F16,      // Half precision
    Int8,     // Scalar quantization
    // PQ,    // Product quantization (future)
}

impl VectorHeap {
    fn store_quantized(&mut self, embedding: &[f32], dtype: StorageDtype) {
        match dtype {
            StorageDtype::F16 => { /* convert to half */ }
            StorageDtype::Int8 => { /* scalar quantize */ }
            _ => { /* full precision */ }
        }
    }
}
```

### 19.3 M9: Complex Filtering

```rust
/// M9 adds richer filters
pub struct MetadataFilter {
    pub equals: HashMap<String, JsonScalar>,
    pub range: Option<RangeFilter>,  // M9
    pub contains: Option<Vec<JsonScalar>>,  // M9
    pub path: Option<JsonPath>,  // M9
}

pub struct RangeFilter {
    pub field: String,
    pub min: Option<f64>,
    pub max: Option<f64>,
}
```

### 19.4 Extension Hooks

M8 code is designed for extension:

```rust
// Index backend is swappable
impl VectorStore {
    pub fn with_backend(&self, backend: Box<dyn VectorIndexBackend>) -> Self;
}

// Metric is configurable at creation
pub fn create_collection(&self, config: VectorConfig) -> Result<()> {
    // config.metric determines similarity function
}

// Storage dtype is reserved for future
pub struct VectorConfig {
    pub storage_dtype: StorageDtype,  // Only F32 in M8
}
```

---

## 20. Appendix

### 20.1 Crate Structure

```
in-mem/
├── crates/
│   ├── core/
│   │   └── src/
│   │       ├── types.rs          # +DocRef::Vector, PrimitiveKind::Vector
│   │       └── vector/           # NEW
│   │           ├── mod.rs
│   │           ├── types.rs      # VectorConfig, VectorEntry, etc.
│   │           ├── error.rs      # VectorError
│   │           └── filter.rs     # MetadataFilter, JsonScalar
│   ├── primitives/
│   │   └── src/
│   │       ├── vector/           # NEW
│   │       │   ├── mod.rs        # VectorStore
│   │       │   ├── backend.rs    # VectorIndexBackend trait
│   │       │   ├── brute_force.rs # BruteForceBackend
│   │       │   ├── heap.rs       # VectorHeap
│   │       │   └── search.rs     # Searchable impl
│   │       └── extensions.rs     # +VectorStoreExt
│   ├── durability/
│   │   └── src/
│   │       └── wal_entry_types.rs # +Vector entries 0x70-0x73
│   └── search/
│       └── src/
│           └── hybrid.rs         # +Vector integration
```

### 20.2 Success Criteria Checklist

**Gate 1: Core Semantics**
- [ ] `VectorStore::insert()` works (upsert semantics)
- [ ] `VectorStore::get()` retrieves embedding + metadata
- [ ] `VectorStore::delete()` works
- [ ] `VectorStore::search()` returns top-k results
- [ ] Dimension validation on insert and query

**Gate 2: Similarity Search**
- [ ] Cosine similarity scoring
- [ ] Euclidean distance scoring
- [ ] Dot product scoring
- [ ] Score normalization ("higher is better")
- [ ] Deterministic tie-breaking
- [ ] Metadata filtering (post-filter, equality only)

**Gate 3: Index Support**
- [ ] `VectorIndexBackend` trait defined
- [ ] `BruteForceBackend` implemented
- [ ] Vector heap storage
- [ ] Index persistence in snapshots
- [ ] Index recovery from WAL

**Gate 4: M6 Integration**
- [ ] `DocRef::Vector` variant
- [ ] `PrimitiveKind::Vector`
- [ ] `Searchable` trait implementation
- [ ] `search_request(&SearchRequest) -> SearchResponse`
- [ ] Hybrid search includes vectors
- [ ] RRF fusion works with vector results

**Gate 5: Transaction Integration**
- [ ] `VectorStoreExt` trait for transactions
- [ ] Cross-primitive atomicity (KV + Vector)
- [ ] Conflict detection on same key
- [ ] Rollback safety

**Gate 6: Durability**
- [ ] WAL entry types (0x70-0x73)
- [ ] `PrimitiveStorageExt` implementation
- [ ] Snapshot serialization
- [ ] WAL replay
- [ ] Crash recovery

---

## Conclusion

M8 is an **API validation milestone**.

It defines:
- Vector storage with configurable dimensions and metrics
- Brute-force similarity search
- Full M6 search integration
- Full transaction integration
- VectorIndexBackend trait for M9 HNSW

It does NOT attempt to optimize for scale. That is intentional.

**M8 builds the API. M9 builds the speed.**

After M8, agents can store and search embeddings. The API is validated, the integration is complete, and the path to HNSW is clear. This enables semantic search alongside keyword search for AI agent memory.

---

**Document Version**: 1.0
**Status**: Implementation Ready
**Date**: 2026-01-17
