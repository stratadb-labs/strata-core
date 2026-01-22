# M8 Architecture Diagrams: Vector Primitive

This document contains visual representations of the M8 architecture focused on native vector storage, similarity search, and integration with existing primitives.

**Architecture Spec Version**: 1.0

---

## Semantic Invariants (Reference)

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                         M8 SEMANTIC INVARIANTS                               │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│  STORAGE INVARIANTS (S1-S9)                                                 │
│  ─────────────────────────                                                  │
│  S1. DIMENSION IMMUTABLE    Collection dimension cannot change after create │
│  S2. METRIC IMMUTABLE       Distance metric cannot change after creation    │
│  S3. VECTORID STABLE        IDs do not change within collection lifetime    │
│  S4. VECTORID NEVER REUSED  Once assigned, ID is never recycled (even del) │
│  S5. HEAP-KV CONSISTENCY    Vector heap and KV metadata always in sync      │
│  S6. RUN ISOLATION          Collections scoped to RunId                     │
│  S7. BTREEMAP SOLE SOURCE   id_to_offset is ONLY source of active vectors  │
│  S8. SNAPSHOT-WAL EQUIV     Snapshot + WAL = pure WAL replay (same state)  │
│  S9. RECONSTRUCTIBILITY     Heap and KV can be rebuilt from snapshot + WAL │
│                                                                             │
│  SEARCH INVARIANTS (R1-R10)                                                 │
│  ─────────────────────────                                                  │
│  R1. DIMENSION MATCH        Query dimension must match collection dimension │
│  R2. SCORE NORMALIZATION    All metrics return "higher is better" scores    │
│  R3. DETERMINISTIC ORDER    Same query = same result order (always)         │
│  R4. BACKEND TIE-BREAK      Backend sorts by (score desc, VectorId asc)    │
│  R5. FACADE TIE-BREAK       Facade sorts by (score desc, key asc)          │
│  R6. SNAPSHOT CONSISTENCY   Search sees consistent point-in-time view       │
│  R7. COARSE BUDGET          Budget checked at boundaries; may overshoot     │
│  R8. SINGLE-THREADED        Similarity computation is single-threaded       │
│  R9. NO IMPLICIT NORMALIZE  Embeddings stored as-is, no silent normalization│
│  R10. SEARCH READ-ONLY      Search must not write anything: no side effects │
│                                                                             │
│  TRANSACTION INVARIANTS (T1-T4)                                             │
│  ─────────────────────────────                                              │
│  T1. ATOMIC VISIBILITY      Insert/delete atomic with other primitives      │
│  T2. CONFLICT DETECTION     Concurrent writes to same key conflict          │
│  T3. ROLLBACK SAFETY        Failed transactions leave no partial state      │
│  T4. ID MONOTONICITY        After crash, new VectorIds > all previous IDs   │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

---

## 1. System Architecture Overview (M8)

```
+-------------------------------------------------------------------------+
|                           Application Layer                              |
|                      (Agent Applications using DB)                       |
+-----------------------------------+-------------------------------------+
                                    |
                                    | High-level typed APIs
                                    v
+-------------------------------------------------------------------------+
|                          Primitives Layer (M3-M8)                        |
|                          (Stateless Facades)                             |
|                                                                          |
|  +-------------+  +-------------+  +--------------+  +-------------+    |
|  |  KV Store   |  |  Event Log  |  |  StateCell   |  |Trace Store  |    |
|  +------+------+  +------+------+  +------+-------+  +------+------+    |
|         |                |                |                |            |
|         +----------------+-------+--------+----------------+            |
|                                  |                                      |
|  +---------------------------+   |   +-----------------------------+   |
|  |        Run Index          |   |   |      JSON Store (M5)        |   |
|  +-------------+-------------+   |   +-------------+---------------+   |
|                |                 |                 |                    |
|  +─────────────────────────────────────────────────────────────────+   |
|  │                     M8 NEW: Vector Store                         │   |
|  │                     (Stateless Facade)                           │   |
|  │                                                                   │   |
|  │   - create_collection()     - search()                           │   |
|  │   - delete_collection()     - search_request()                   │   |
|  │   - insert() (upsert)       - list_collections()                 │   |
|  │   - get()                   - get_collection()                   │   |
|  │   - delete()                                                     │   |
|  └─────────────────────────────────────────────────────────────────+   |
+----------------+-----------------+-----------------+--------------------+
                                   |
                                   | Database transaction API
                                   v
+-------------------------------------------------------------------------+
|                         Engine Layer (M1-M8)                             |
|                   (Orchestration & Coordination)                         |
|                                                                          |
|  +-------------------------------------------------------------------+  |
|  |                          Database                                  |  |
|  |                                                                    |  |
|  |  M8 NEW: Vector Storage System                                    |  |
|  |  +-------------------------------------------------------------+  |  |
|  |  |                   VectorStorage                              |  |  |
|  |  |  - Vector Heap (contiguous f32[] per collection)            |  |  |
|  |  |  - KV Metadata (VectorRecord per key)                       |  |  |
|  |  |  - VectorIndexBackend (trait for search algorithms)         |  |  |
|  |  |  - BruteForceBackend (M8 implementation)                    |  |  |
|  |  +-------------------------------------------------------------+  |  |
|  |                                                                    |  |
|  |  M6/M8: Hybrid Search Integration                                 |  |
|  |  +-------------------------------------------------------------+  |  |
|  |  |                   HybridSearch                               |  |  |
|  |  |  - Keyword search across KV, JSON, Event, etc.              |  |  |
|  |  |  - M8: Vector search via search_by_embedding()              |  |  |
|  |  |  - RRF fusion of results                                    |  |  |
|  |  +-------------------------------------------------------------+  |  |
|  |                                                                    |  |
|  +-------------------------------------------------------------------+  |
|                               |                                          |
+----------+-------------------+-------------------+-----------------------+
           |                   |                   |
           v                   v                   v
+------------------+  +-------------------+  +------------------------+
|  Storage (M4+M7) |  | Durability (M4+M7)|  | Concurrency (M4)       |
|                  |  |                   |  |                        |
| M8 NEW:          |  | M8 NEW:           |  | M8: Cross-primitive    |
| - Vector heap    |  | - WAL entries     |  |   transaction support  |
|   storage        |  |   0x70-0x73       |  | - VectorStoreExt trait |
| - TypeTag::Vector|  | - Snapshot blob   |  |                        |
| - TypeTag::      |  |   serialization   |  |                        |
|   VectorConfig   |  |                   |  |                        |
+------------------+  +-------------------+  +------------------------+
           |                   |                   |
           +-------------------+-------------------+
                               |
                               v
+-------------------------------------------------------------------------+
|                         Core Types Layer (M1 + M8)                       |
|                       (Foundation Definitions)                           |
|                                                                          |
|  M8 NEW Types:                                                           |
|  - VectorConfig      (dimension, metric, storage_dtype)                 |
|  - VectorEntry       (key, embedding, metadata, vector_id)              |
|  - VectorMatch       (key, score, metadata)                             |
|  - VectorId          (u64, monotonically increasing, never reused)      |
|  - DistanceMetric    (Cosine, Euclidean, DotProduct)                    |
|  - StorageDtype      (F32 only in M8; F16, Int8 in M9)                  |
|  - MetadataFilter    (equality filtering only in M8)                    |
|  - CollectionInfo    (name, config, count, created_at)                  |
|  - DocRef::Vector    (collection, key)                                  |
|  - PrimitiveKind::Vector                                                |
+-------------------------------------------------------------------------+
```

---

## 2. Hybrid Storage Architecture

```
+-------------------------------------------------------------------------+
|                    Hybrid Storage Architecture (M8)                      |
+-------------------------------------------------------------------------+

Design Rationale:
=================

    ┌─────────────────────────────────────────────────────────────────────┐
    │                                                                     │
    │  WHY HYBRID STORAGE?                                                │
    │  ─────────────────────                                              │
    │                                                                     │
    │  Pure KV-backed:                                                    │
    │  ✗ Requires deserializing every vector on every search              │
    │  ✗ Not cache-friendly for dense numeric scanning                    │
    │  ✗ Becomes bottleneck immediately                                   │
    │                                                                     │
    │  Pure vector heap:                                                  │
    │  ✗ Can't participate in transactions                                │
    │  ✗ No WAL integration for durability                                │
    │  ✗ No flexible metadata schema                                      │
    │                                                                     │
    │  HYBRID APPROACH (M8):                                              │
    │  ✓ Vector heap: contiguous Vec<f32> for fast similarity scan        │
    │  ✓ KV metadata: standard storage for metadata, key mapping          │
    │  ✓ Best of both worlds                                              │
    │                                                                     │
    └─────────────────────────────────────────────────────────────────────┘


Storage Layout:
===============

    ┌─────────────────────────────────────────────────────────────────────┐
    │                         VectorStore                                  │
    ├─────────────────────────────────────────────────────────────────────┤
    │                                                                     │
    │  ┌─────────────────────────┐    ┌─────────────────────────────┐   │
    │  │     Vector Heap         │    │      KV Metadata            │   │
    │  │     (per collection)    │    │      (ShardedStore)         │   │
    │  │                         │    │                             │   │
    │  │  ┌───────────────────┐  │    │  ┌───────────────────────┐  │   │
    │  │  │ VectorId → offset │  │    │  │ Key → VectorRecord    │  │   │
    │  │  │ (BTreeMap)        │  │    │  │                       │  │   │
    │  │  └───────────────────┘  │    │  │  - vector_id: u64     │  │   │
    │  │                         │    │  │  - metadata: Option   │  │   │
    │  │  ┌───────────────────┐  │    │  │  - version: u64       │  │   │
    │  │  │ Contiguous f32[]  │  │    │  │  - created_at: u64    │  │   │
    │  │  │                   │  │    │  │  - updated_at: u64    │  │   │
    │  │  │ [v0_dim0, v0_dim1,│  │    │  └───────────────────────┘  │   │
    │  │  │  ..., v0_dimN,    │  │    │                             │   │
    │  │  │  v1_dim0, ...]    │  │    │  Fast metadata lookup       │   │
    │  │  └───────────────────┘  │    │  Flexible JSON schema       │   │
    │  │                         │    │  Transaction support        │   │
    │  │  Cache-friendly scan    │    │                             │   │
    │  │  Fast brute-force       │    │                             │   │
    │  └─────────────────────────┘    └─────────────────────────────┘   │
    │                                                                     │
    │  ┌─────────────────────────────────────────────────────────────┐   │
    │  │              VectorIndexBackend (trait)                      │   │
    │  │                                                               │   │
    │  │  fn insert(&mut self, id: VectorId, embedding: &[f32])       │   │
    │  │  fn delete(&mut self, id: VectorId) -> bool                  │   │
    │  │  fn search(&self, query: &[f32], k: usize) -> Vec<(Id,f32)>  │   │
    │  │                                                               │   │
    │  │  ┌────────────────────┐    ┌────────────────────┐           │   │
    │  │  │ BruteForceBackend  │    │   HnswBackend      │           │   │
    │  │  │ (M8: O(n) scan)    │    │   (M9: O(log n))   │           │   │
    │  │  └────────────────────┘    └────────────────────┘           │   │
    │  └─────────────────────────────────────────────────────────────┘   │
    │                                                                     │
    └─────────────────────────────────────────────────────────────────────┘


VectorHeap Detail:
==================

    ┌─────────────────────────────────────────────────────────────────────┐
    │                         VectorHeap Structure                         │
    ├─────────────────────────────────────────────────────────────────────┤
    │                                                                     │
    │  struct VectorHeap {                                                │
    │      config: VectorConfig,                                          │
    │      data: Vec<f32>,              // Contiguous embedding storage   │
    │      id_to_offset: BTreeMap<VectorId, usize>,  // SOLE source      │
    │      free_slots: Vec<usize>,      // Reusable storage slots        │
    │      next_id: AtomicU64,          // Monotonically increasing      │
    │      version: AtomicU64,          // For snapshot consistency      │
    │  }                                                                  │
    │                                                                     │
    │  CRITICAL INVARIANTS:                                               │
    │  ─────────────────────                                              │
    │  1. BTreeMap is SOLE source of truth for active vectors             │
    │  2. Storage slots may be reused, but VectorId values NEVER are     │
    │  3. next_id and free_slots MUST be persisted in snapshots          │
    │  4. BTreeMap iteration order is deterministic (sorted by VectorId) │
    │                                                                     │
    │  Memory Layout Example (dimension=4):                               │
    │  ─────────────────────────────────────                              │
    │                                                                     │
    │  data: [v0_d0, v0_d1, v0_d2, v0_d3, v1_d0, v1_d1, v1_d2, v1_d3, ...]│
    │         ├──── VectorId=0 ────┤     ├──── VectorId=1 ────┤          │
    │         offset=0                    offset=4                        │
    │                                                                     │
    │  id_to_offset: { VectorId(0) → 0, VectorId(1) → 4 }                │
    │                                                                     │
    └─────────────────────────────────────────────────────────────────────┘
```

---

## 3. The Six Architectural Rules

```
+-------------------------------------------------------------------------+
|                   The Six Architectural Rules (M8)                       |
|                        (NON-NEGOTIABLE)                                  |
+-------------------------------------------------------------------------+

Rule 1: STATELESS FACADE PATTERN
================================

    ┌─────────────────────────────────────────────────────────────────────┐
    │                                                                     │
    │  VectorStore is a STATELESS facade. All state lives in Database.   │
    │                                                                     │
    │  CORRECT:                                  WRONG:                   │
    │  ─────────                                 ──────                   │
    │  struct VectorStore {                      struct VectorStore {     │
    │      db: Arc<Database>,                        db: Arc<Database>,   │
    │  }                                             local_cache: HashMap │
    │                                            }                        │
    │  impl Clone for VectorStore {                                       │
    │      fn clone(&self) -> Self {             // NEVER cache state    │
    │          VectorStore {                     // locally              │
    │              db: self.db.clone()                                    │
    │          }                                                          │
    │      }                                                              │
    │  }                                                                  │
    │                                                                     │
    │  WHY: Multiple VectorStore instances on same DB must be safe.       │
    │                                                                     │
    └─────────────────────────────────────────────────────────────────────┘


Rule 2: COLLECTIONS PER RUNID
=============================

    ┌─────────────────────────────────────────────────────────────────────┐
    │                                                                     │
    │  Collections are scoped to RunId. Different runs cannot see         │
    │  each other's collections.                                          │
    │                                                                     │
    │  CORRECT:                                  WRONG:                   │
    │  ─────────                                 ──────                   │
    │  fn insert(&self,                          fn insert(&self,         │
    │      run_id: RunId,   ← REQUIRED               collection: &str,   │
    │      collection: &str,                         key: &str,           │
    │      key: &str,                            ) // NO run_id!          │
    │      ...                                                            │
    │  )                                                                  │
    │                                                                     │
    │  WHY: Run isolation is a core invariant across all primitives.      │
    │                                                                     │
    └─────────────────────────────────────────────────────────────────────┘


Rule 3: UPSERT SEMANTICS
========================

    ┌─────────────────────────────────────────────────────────────────────┐
    │                                                                     │
    │  Insert OVERWRITES if key exists. No separate update method.        │
    │                                                                     │
    │  CORRECT:                                  WRONG:                   │
    │  ─────────                                 ──────                   │
    │  fn insert(...) {                          fn insert(...) {         │
    │      // If key exists: overwrite               // fails if exists   │
    │      // If key doesn't exist: create       }                        │
    │  }                                         fn update(...) {         │
    │                                                // fails if !exists  │
    │                                            }                        │
    │                                                                     │
    │  WHY: Agents want "set this vector" semantics. Upsert is simpler.   │
    │                                                                     │
    └─────────────────────────────────────────────────────────────────────┘


Rule 4: DIMENSION VALIDATION
============================

    ┌─────────────────────────────────────────────────────────────────────┐
    │                                                                     │
    │  All vectors in a collection MUST have the same dimension.          │
    │  Enforce on insert AND query.                                       │
    │                                                                     │
    │  fn insert(&self, ..., embedding: &[f32], ...) -> Result<()> {     │
    │      let config = self.get_collection_config(run_id, collection)?; │
    │      if embedding.len() != config.dimension {                       │
    │          return Err(VectorError::DimensionMismatch {                │
    │              expected: config.dimension,                            │
    │              got: embedding.len(),                                  │
    │          });                                                        │
    │      }                                                              │
    │      // ...                                                         │
    │  }                                                                  │
    │                                                                     │
    │  WHY: Distance calculations require matching dimensions.            │
    │       Mixed dimensions would produce garbage results.               │
    │                                                                     │
    └─────────────────────────────────────────────────────────────────────┘


Rule 5: DETERMINISTIC ORDERING AT EVERY LAYER
=============================================

    ┌─────────────────────────────────────────────────────────────────────┐
    │                                                                     │
    │  Determinism MUST be enforced at the backend level, not just facade.│
    │                                                                     │
    │  BACKEND LEVEL:                                                     │
    │  ──────────────                                                     │
    │  results.sort_by(|(id_a, score_a), (id_b, score_b)| {              │
    │      score_b.partial_cmp(score_a)                                   │
    │          .unwrap_or(Ordering::Equal)                                │
    │          .then_with(|| id_a.cmp(id_b))  ← VectorId tie-break       │
    │  });                                                                │
    │                                                                     │
    │  FACADE LEVEL:                                                      │
    │  ─────────────                                                      │
    │  matches.sort_by(|a, b| {                                          │
    │      b.score.partial_cmp(&a.score)                                  │
    │          .unwrap_or(Ordering::Equal)                                │
    │          .then_with(|| a.key.cmp(&b.key))  ← key tie-break         │
    │  });                                                                │
    │                                                                     │
    │  DETERMINISM CHAIN:                                                 │
    │  ───────────────────                                                │
    │  1. Backend: sort by (score desc, VectorId asc)                    │
    │  2. Facade: map VectorId → key, sort by (score desc, key asc)      │
    │  3. Both layers enforce determinism independently                   │
    │                                                                     │
    │  WHY: HashMap iteration is nondeterministic. Float ties are common. │
    │                                                                     │
    └─────────────────────────────────────────────────────────────────────┘


Rule 6: VECTORID IS NEVER REUSED
================================

    ┌─────────────────────────────────────────────────────────────────────┐
    │                                                                     │
    │  Once a VectorId is assigned, it is NEVER recycled (even after del)│
    │                                                                     │
    │  CORRECT:                                  WRONG:                   │
    │  ─────────                                 ──────                   │
    │  fn allocate_id(&self) -> VectorId {       fn allocate_id(&self) {  │
    │      VectorId(self.next_id                     if let Some(id) =    │
    │          .fetch_add(1, Ordering::Relaxed))         self.free_ids.pop│
    │  }                                             {                    │
    │                                                    return id; // NO!│
    │  fn delete(&mut self, id: VectorId) {          }                    │
    │      if let Some(offset) =                     VectorId(self.next_id│
    │          self.id_to_offset.remove(&id) {           .fetch_add(1))   │
    │          self.free_slots.push(offset);     }                        │
    │          // Reuse STORAGE slot, but                                 │
    │          // NEVER reuse VectorId value                              │
    │      }                                                              │
    │  }                                                                  │
    │                                                                     │
    │  WHY: VectorId reuse creates subtle replay bugs.                    │
    │       insert → delete → insert must produce identical replay state. │
    │                                                                     │
    └─────────────────────────────────────────────────────────────────────┘


Rule 7: NO BACKEND-SPECIFIC FIELDS IN VECTORCONFIG
==================================================

    ┌─────────────────────────────────────────────────────────────────────┐
    │                                                                     │
    │  VectorConfig contains ONLY primitive-level configuration.          │
    │  Backend-specific tuning must NOT pollute this type.                │
    │                                                                     │
    │  CORRECT:                                  WRONG:                   │
    │  ─────────                                 ──────                   │
    │  struct VectorConfig {                     struct VectorConfig {    │
    │      dimension: usize,                         dimension: usize,    │
    │      metric: DistanceMetric,                   metric: ...,         │
    │      storage_dtype: StorageDtype,              storage_dtype: ...,  │
    │  }                                             ef_construction: usize│
    │                                                M: usize,  // HNSW   │
    │  // Backend config is SEPARATE:            }                        │
    │  struct HnswConfig {                                                │
    │      ef_construction: usize,                                        │
    │      M: usize,                                                      │
    │      ef_search: usize,                                              │
    │  }                                                                  │
    │                                                                     │
    │  WHY: Prevents HNSW from polluting Vector API when added in M9.     │
    │                                                                     │
    └─────────────────────────────────────────────────────────────────────┘
```

---

## 4. Distance Metrics

```
+-------------------------------------------------------------------------+
|                       Distance Metrics (M8)                              |
+-------------------------------------------------------------------------+

Score Normalization:
====================

    ┌─────────────────────────────────────────────────────────────────────┐
    │                                                                     │
    │  ALL METRICS return scores where HIGHER = MORE SIMILAR.             │
    │                                                                     │
    │  ┌────────────────┬───────────────────────────┬──────────────────┐ │
    │  │    Metric      │       Formula             │      Range       │ │
    │  ├────────────────┼───────────────────────────┼──────────────────┤ │
    │  │ Cosine         │ dot(a,b) / (‖a‖ × ‖b‖)   │ [-1, 1]          │ │
    │  │                │                           │ 1 = identical    │ │
    │  ├────────────────┼───────────────────────────┼──────────────────┤ │
    │  │ Euclidean      │ 1 / (1 + √Σ(aᵢ-bᵢ)²)     │ (0, 1]           │ │
    │  │                │                           │ 1 = identical    │ │
    │  ├────────────────┼───────────────────────────┼──────────────────┤ │
    │  │ DotProduct     │ Σ(aᵢ × bᵢ)               │ [-∞, +∞]         │ │
    │  │                │ (assumes normalized)      │ Higher = better  │ │
    │  └────────────────┴───────────────────────────┴──────────────────┘ │
    │                                                                     │
    └─────────────────────────────────────────────────────────────────────┘


Similarity Computation:
=======================

    ┌─────────────────────────────────────────────────────────────────────┐
    │                                                                     │
    │  fn compute_similarity(&self, a: &[f32], b: &[f32]) -> f32 {       │
    │      match self.metric {                                            │
    │          DistanceMetric::Cosine => {                                │
    │              let dot = dot_product(a, b);                           │
    │              let norm_a = l2_norm(a);                               │
    │              let norm_b = l2_norm(b);                               │
    │              if norm_a == 0.0 || norm_b == 0.0 {                    │
    │                  0.0                                                │
    │              } else {                                               │
    │                  dot / (norm_a * norm_b)                            │
    │              }                                                      │
    │          }                                                          │
    │          DistanceMetric::Euclidean => {                             │
    │              let dist = euclidean_distance(a, b);                   │
    │              1.0 / (1.0 + dist)                                     │
    │          }                                                          │
    │          DistanceMetric::DotProduct => {                            │
    │              dot_product(a, b)                                      │
    │          }                                                          │
    │      }                                                              │
    │  }                                                                  │
    │                                                                     │
    └─────────────────────────────────────────────────────────────────────┘


Visualization:
==============

                           Cosine Similarity

                        score = 1.0 (identical)
                              ↑
                              │  A
                              │ /
                              │/
          score = 0  ─────────+─────────→  score = 0
           (orthogonal)       │            (orthogonal)
                              │
                              │
                              ↓
                        score = -1.0 (opposite)



                         Euclidean Similarity

                        score = 1.0 (same point)
                              ●A=B

                        score → 0.5 (distance = 1)
                          A ●───● B

                        score → 0.33 (distance = 2)
                       A ●───────● B

                        score → 0 (as distance → ∞)
              A ●────────────────────────────────● B
```

---

## 5. Search Flow

```
+-------------------------------------------------------------------------+
|                         Search Flow (M8)                                 |
+-------------------------------------------------------------------------+

Search Pipeline:
================

    SearchRequest
         │
         ▼
    ┌─────────────────┐
    │  Validate query │
    │  - dimension    │     DimensionMismatch if query.len() != config.dim
    │  - collection   │     CollectionNotFound if doesn't exist
    └────────┬────────┘
             │
             ▼
    ┌─────────────────┐
    │  Get snapshot   │     Point-in-time consistency
    │  (consistency)  │     Search sees stable view
    └────────┬────────┘
             │
             ▼
    ┌─────────────────────────────┐
    │  Index backend search       │
    │                             │
    │  BruteForce (M8):          │
    │  for (id, emb) in heap {   │     O(n) scan
    │      score = similarity()  │     Single-threaded
    │      results.push((id,sc)) │     Deterministic iteration (BTreeMap)
    │  }                         │
    │  sort by (score↓, id↑)     │     Backend tie-break
    │  truncate to k*factor      │
    │                             │
    │  HNSW (M9):                │
    │  graph.search(query, k)    │     O(log n) traversal
    └────────────┬────────────────┘
                 │
                 ▼
    ┌─────────────────────────────┐
    │  Load metadata for matches  │     KV lookup per result
    │  (from KV store)            │     VectorId → key mapping
    └────────────┬────────────────┘
                 │
                 ▼
    ┌─────────────────────────────┐
    │  Apply metadata filters     │     Post-filter in M8
    │  (post-filter in M8)        │     Equality only: field == value
    └────────────┬────────────────┘
                 │
                 ▼
    ┌─────────────────────────────┐
    │  Apply facade tie-breaking  │     sort by (score↓, key↑)
    │  (score desc, key asc)      │     Final deterministic ordering
    └────────────┬────────────────┘
                 │
                 ▼
           Vec<VectorMatch>


Post-Filter Detail:
===================

    ┌─────────────────────────────────────────────────────────────────────┐
    │                                                                     │
    │  M8: POST-FILTERING ONLY                                            │
    │  ────────────────────────                                           │
    │                                                                     │
    │  1. Search index backend (k * filter_factor candidates)            │
    │  2. Load metadata for each candidate                                │
    │  3. Filter out non-matching metadata                                │
    │  4. Return first k matching results                                 │
    │                                                                     │
    │                                                                     │
    │  ┌───────────────────────────────────────────────────────────────┐ │
    │  │ Index: Returns top 30 candidates (k=10, factor=3)             │ │
    │  │                                                                │ │
    │  │  [█ █ █ █ █ █ █ █ █ █ █ █ █ █ █ █ █ █ █ █ █ █ █ █ █ █ █ █ █ █]│ │
    │  │   ↓ Post-filter: keep only category="doc"                     │ │
    │  │  [█ - █ - - █ █ - █ - - - █ - █ - - - █ - - █ - - - - - - - -]│ │
    │  │   ↓ Take first k=10                                           │ │
    │  │  [█   █     █ █   █       █   █       █     █                ]│ │
    │  │                                                                │ │
    │  └───────────────────────────────────────────────────────────────┘ │
    │                                                                     │
    │  M9 adds PRE-FILTERING: filter candidates BEFORE similarity scan   │
    │                                                                     │
    └─────────────────────────────────────────────────────────────────────┘


Budget Enforcement:
===================

    ┌─────────────────────────────────────────────────────────────────────┐
    │                                                                     │
    │  COARSE-GRAINED BUDGET (R7)                                         │
    │  ───────────────────────────                                        │
    │                                                                     │
    │  Budget is checked at PHASE BOUNDARIES, not mid-computation.        │
    │  Brute-force cannot be interrupted mid-loop.                        │
    │                                                                     │
    │  fn search(..., budget: &SearchBudget) {                           │
    │      // CHECK 1: Before starting                                   │
    │      if budget_exceeded() { return early; }                        │
    │                                                                     │
    │      // BRUTE-FORCE: Runs to completion (cannot interrupt)         │
    │      let results = self.compute_all_similarities(query);           │
    │                                                                     │
    │      // CHECK 2: After scan                                        │
    │      if budget_exceeded() { truncated = true; }                    │
    │                                                                     │
    │      // Sorting, filtering - runs to completion                    │
    │      ...                                                            │
    │  }                                                                  │
    │                                                                     │
    │  IMPLICATION: For 50K+ vectors, actual time may exceed budget      │
    │  significantly before check. M9's HNSW can check during traversal. │
    │                                                                     │
    └─────────────────────────────────────────────────────────────────────┘
```

---

## 6. M6 Search Integration

```
+-------------------------------------------------------------------------+
|                      M6 Search Integration (M8)                          |
+-------------------------------------------------------------------------+

Design Decision:
================

    ┌─────────────────────────────────────────────────────────────────────┐
    │                                                                     │
    │  VECTOR DOES NOT SUPPORT KEYWORD SEARCH NATIVELY                    │
    │  ────────────────────────────────────────────────                   │
    │                                                                     │
    │  Vector search is fundamentally different from keyword search:      │
    │                                                                     │
    │  Keyword Search:                                                    │
    │  - Input: text query "hello world"                                  │
    │  - Process: tokenize → BM25 scoring over text                       │
    │  - Output: documents matching terms                                 │
    │                                                                     │
    │  Vector Search:                                                     │
    │  - Input: embedding [0.1, 0.3, -0.2, ...]                          │
    │  - Process: similarity computation over vectors                     │
    │  - Output: nearest neighbors by distance                            │
    │                                                                     │
    │  Vector participates in hybrid search via EXPLICIT EMBEDDING        │
    │  queries, not by reimplementing text search on metadata.            │
    │                                                                     │
    └─────────────────────────────────────────────────────────────────────┘


Searchable Implementation:
==========================

    ┌─────────────────────────────────────────────────────────────────────┐
    │                                                                     │
    │  impl Searchable for VectorStore {                                  │
    │      fn search(&self, req: &SearchRequest) -> Result<SearchResponse>│
    │      {                                                              │
    │          match req.mode {                                           │
    │              SearchMode::Keyword => {                               │
    │                  // Vector does NOT do keyword search               │
    │                  // Return empty - orchestrator handles this        │
    │                  return Ok(SearchResponse::empty());                │
    │              }                                                      │
    │              SearchMode::Vector | SearchMode::Hybrid => {           │
    │                  // Requires query embedding                        │
    │                  // Orchestrator should call search_by_embedding()  │
    │                  return Ok(SearchResponse::empty());                │
    │              }                                                      │
    │          }                                                          │
    │      }                                                              │
    │  }                                                                  │
    │                                                                     │
    └─────────────────────────────────────────────────────────────────────┘


Hybrid Search Flow:
===================

    ┌─────────────────────────────────────────────────────────────────────┐
    │                                                                     │
    │  HybridSearch Orchestrator is responsible for:                      │
    │  1. Embedding the text query (external model, not M8 scope)        │
    │  2. Calling vector.search_by_embedding() with the embedding        │
    │  3. Fusing results via RRF                                         │
    │                                                                     │
    │                                                                     │
    │  User Query: "similar documents to this concept"                   │
    │       │                                                             │
    │       ▼                                                             │
    │  ┌───────────────────────────────────────────────────────────────┐ │
    │  │                    HybridSearch                                │ │
    │  └───────────────────────────────────────────────────────────────┘ │
    │       │                                                             │
    │       ├──────────────────────────────────────────┐                 │
    │       │                                          │                  │
    │       ▼                                          ▼                  │
    │  ┌────────────┐                          ┌────────────────┐        │
    │  │ Keyword    │                          │ External       │        │
    │  │ Primitives │                          │ Embedding Model│        │
    │  │            │                          │ (not M8 scope) │        │
    │  │ - KV       │                          └───────┬────────┘        │
    │  │ - JSON     │                                  │                  │
    │  │ - Event    │                                  ▼                  │
    │  │ - State    │                          ┌────────────────┐        │
    │  │ - Trace    │                          │ query_embedding│        │
    │  │ - Run      │                          │ [0.1, 0.3, ...]│        │
    │  └─────┬──────┘                          └───────┬────────┘        │
    │        │                                         │                  │
    │        │                                         ▼                  │
    │        │                                 ┌────────────────┐        │
    │        │                                 │ VectorStore    │        │
    │        │                                 │ search_by_     │        │
    │        │                                 │ embedding()    │        │
    │        │                                 └───────┬────────┘        │
    │        │                                         │                  │
    │        ▼                                         ▼                  │
    │  ┌───────────┐                           ┌───────────┐             │
    │  │ Keyword   │                           │ Vector    │             │
    │  │ Results   │                           │ Results   │             │
    │  └─────┬─────┘                           └─────┬─────┘             │
    │        │                                       │                    │
    │        └───────────────┬───────────────────────┘                   │
    │                        │                                            │
    │                        ▼                                            │
    │               ┌────────────────┐                                   │
    │               │  RRF Fusion    │                                   │
    │               │                │                                   │
    │               │  score = Σ     │                                   │
    │               │  1/(k + rank)  │                                   │
    │               └────────┬───────┘                                   │
    │                        │                                            │
    │                        ▼                                            │
    │               ┌────────────────┐                                   │
    │               │ SearchResponse │                                   │
    │               │ (fused results)│                                   │
    │               └────────────────┘                                   │
    │                                                                     │
    └─────────────────────────────────────────────────────────────────────┘


DocRef::Vector:
===============

    ┌─────────────────────────────────────────────────────────────────────┐
    │                                                                     │
    │  enum DocRef {                                                      │
    │      Kv { key: Key },                                               │
    │      Json { key: Key, doc_id: JsonDocId },                         │
    │      Event { log_key: Key, seq: u64 },                             │
    │      State { key: Key },                                           │
    │      Trace { key: Key, span_id: u64 },                             │
    │      Run { run_id: RunId },                                        │
    │                                                                     │
    │      // M8 addition                                                 │
    │      Vector {                                                       │
    │          collection: String,                                        │
    │          key: String,                                               │
    │      },                                                             │
    │  }                                                                  │
    │                                                                     │
    │  enum PrimitiveKind {                                               │
    │      Kv,                                                            │
    │      Json,                                                          │
    │      Event,                                                         │
    │      State,                                                         │
    │      Trace,                                                         │
    │      Run,                                                           │
    │      Vector,  // M8 addition                                        │
    │  }                                                                  │
    │                                                                     │
    └─────────────────────────────────────────────────────────────────────┘
```

---

## 7. Transaction Integration

```
+-------------------------------------------------------------------------+
|                    Transaction Integration (M8)                          |
+-------------------------------------------------------------------------+

Cross-Primitive Atomicity:
==========================

    ┌─────────────────────────────────────────────────────────────────────┐
    │                                                                     │
    │  A single transaction may write to MULTIPLE primitives.             │
    │  All or nothing: Either ALL writes commit or NONE do.               │
    │                                                                     │
    │  ┌──────────┐                                                      │
    │  │ TxBegin  │  TxId = (run_123, seq_456)                          │
    │  └────┬─────┘                                                      │
    │       │                                                             │
    │       ▼                                                             │
    │  ┌──────────┐  ┌──────────┐  ┌──────────┐  ┌──────────┐           │
    │  │  KvPut   │  │ JsonSet  │  │VectorIns │  │ EventApp │           │
    │  │ doc:1=..│  │ meta={}  │  │ emb=[...]│  │ log event│           │
    │  └────┬─────┘  └────┬─────┘  └────┬─────┘  └────┬─────┘           │
    │       │              │              │              │                │
    │       └──────────────┴──────────────┴──────────────┘               │
    │                              │                                      │
    │                              ▼                                      │
    │                        ┌──────────┐                                │
    │                        │ TxCommit │                                │
    │                        └──────────┘                                │
    │                                                                     │
    └─────────────────────────────────────────────────────────────────────┘


VectorStoreExt Trait:
=====================

    ┌─────────────────────────────────────────────────────────────────────┐
    │                                                                     │
    │  pub trait VectorStoreExt {                                         │
    │      fn vector_insert(                                              │
    │          &mut self,                                                 │
    │          collection: &str,                                          │
    │          key: &str,                                                 │
    │          embedding: &[f32],                                         │
    │          metadata: Option<JsonValue>,                               │
    │      ) -> Result<()>;                                               │
    │                                                                     │
    │      fn vector_delete(                                              │
    │          &mut self,                                                 │
    │          collection: &str,                                          │
    │          key: &str,                                                 │
    │      ) -> Result<bool>;                                             │
    │                                                                     │
    │      fn vector_get(                                                 │
    │          &mut self,                                                 │
    │          collection: &str,                                          │
    │          key: &str,                                                 │
    │      ) -> Result<Option<VectorEntry>>;                              │
    │  }                                                                  │
    │                                                                     │
    │  impl VectorStoreExt for TransactionContext { ... }                │
    │                                                                     │
    └─────────────────────────────────────────────────────────────────────┘


Usage Example:
==============

    ┌─────────────────────────────────────────────────────────────────────┐
    │                                                                     │
    │  // Atomic KV + Vector operation                                   │
    │  db.transaction(run_id, |txn| {                                    │
    │      // Store document in KV                                       │
    │      txn.kv_put("doc:123", json!({                                 │
    │          "title": "Example Document",                              │
    │          "content": "This is the document content...",             │
    │      }))?;                                                          │
    │                                                                     │
    │      // Store embedding in Vector                                  │
    │      let embedding = embed("This is the document content...");     │
    │      txn.vector_insert(                                            │
    │          "documents",                                               │
    │          "doc:123",                                                 │
    │          &embedding,                                                │
    │          Some(json!({ "type": "document" })),                      │
    │      )?;                                                            │
    │                                                                     │
    │      Ok(())                                                         │
    │  })?;                                                               │
    │                                                                     │
    │  // On failure, NEITHER KV nor Vector is modified                  │
    │                                                                     │
    └─────────────────────────────────────────────────────────────────────┘


Transaction Failure Semantics:
==============================

    ┌─────────────────────────────────────────────────────────────────────┐
    │                                                                     │
    │  SUCCESS:                          FAILURE:                        │
    │  ────────                          ────────                        │
    │  TxBegin                           TxBegin                         │
    │    │                                 │                              │
    │    ▼                                 ▼                              │
    │  KvPut(doc:1)  ✓                   KvPut(doc:2)  ✓                 │
    │    │                                 │                              │
    │    ▼                                 ▼                              │
    │  VectorInsert(doc:1) ✓             VectorInsert(doc:2) ✓          │
    │    │                                 │                              │
    │    ▼                                 ▼                              │
    │  TxCommit ✓                        Error! (before commit)          │
    │    │                                 │                              │
    │    ▼                                 ▼                              │
    │  BOTH visible                      Rollback                        │
    │                                       │                             │
    │                                       ▼                             │
    │                                    NEITHER visible                 │
    │                                    (T3: Rollback Safety)           │
    │                                                                     │
    └─────────────────────────────────────────────────────────────────────┘
```

---

## 8. WAL Integration

```
+-------------------------------------------------------------------------+
|                        WAL Integration (M8)                              |
+-------------------------------------------------------------------------+

Entry Type Registry:
====================

    ┌─────────────────────────────────────────────────────────────────────┐
    │                     WAL ENTRY TYPE REGISTRY                          │
    ├─────────────────────────────────────────────────────────────────────┤
    │                                                                     │
    │  Range         Primitive        Types                              │
    │  ─────────────────────────────────────────────────────────────     │
    │  0x00-0x0F     Core             TxBegin, TxCommit, TxAbort,        │
    │                                 Checkpoint, Noop                    │
    │                                                                     │
    │  0x10-0x1F     KV Store         KvPut, KvDelete, KvClear           │
    │                                                                     │
    │  0x20-0x2F     JSON Store       JsonCreate, JsonSet, JsonDelete,   │
    │                                 JsonPatch                           │
    │                                                                     │
    │  0x30-0x3F     Event Log        EventAppend, EventTruncate         │
    │                                                                     │
    │  0x40-0x4F     StateCell        StateInit, StateSet, StateCas      │
    │                                                                     │
    │  0x50-0x5F     Trace Store      TraceRecord, TraceEndSpan          │
    │                                                                     │
    │  0x60-0x6F     Run Index        RunBegin, RunEnd, RunUpdate        │
    │                                                                     │
    │  0x70-0x7F     Vector (M8)      ← NEW IN M8                        │
    │                                 0x70: VectorCollectionCreate       │
    │                                 0x71: VectorCollectionDelete       │
    │                                 0x72: VectorUpsert                 │
    │                                 0x73: VectorDelete                 │
    │                                                                     │
    │  0x80-0xFF     Future           RESERVED for new primitives        │
    │                                                                     │
    └─────────────────────────────────────────────────────────────────────┘


WAL Entry Payloads:
===================

    ┌─────────────────────────────────────────────────────────────────────┐
    │                                                                     │
    │  VectorCollectionCreate (0x70)                                     │
    │  ─────────────────────────────                                     │
    │  {                                                                  │
    │      run_id: RunId,                                                │
    │      collection: String,                                           │
    │      config: VectorConfig,                                         │
    │      timestamp: u64,                                               │
    │  }                                                                  │
    │                                                                     │
    │  VectorCollectionDelete (0x71)                                     │
    │  ─────────────────────────────                                     │
    │  {                                                                  │
    │      run_id: RunId,                                                │
    │      collection: String,                                           │
    │      timestamp: u64,                                               │
    │  }                                                                  │
    │                                                                     │
    │  VectorUpsert (0x72)                                               │
    │  ───────────────────                                               │
    │  {                                                                  │
    │      run_id: RunId,                                                │
    │      collection: String,                                           │
    │      key: String,                                                  │
    │      vector_id: u64,                                               │
    │      embedding: Vec<f32>,  ← FULL EMBEDDING (temporary M8 format) │
    │      metadata: Option<JsonValue>,                                  │
    │      timestamp: u64,                                               │
    │  }                                                                  │
    │                                                                     │
    │  VectorDelete (0x73)                                               │
    │  ───────────────────                                               │
    │  {                                                                  │
    │      run_id: RunId,                                                │
    │      collection: String,                                           │
    │      key: String,                                                  │
    │      vector_id: u64,                                               │
    │      timestamp: u64,                                               │
    │  }                                                                  │
    │                                                                     │
    │  WARNING: VectorUpsert contains FULL embedding in M8.              │
    │  This bloats WAL (~3KB per 768-dim vector).                        │
    │  M9 may optimize (delta encoding, separate segment).               │
    │                                                                     │
    └─────────────────────────────────────────────────────────────────────┘


WAL Replay:
===========

    ┌─────────────────────────────────────────────────────────────────────┐
    │                                                                     │
    │  Global WAL replay mechanism (not Vector-specific):                │
    │                                                                     │
    │  1. Reads WAL entries in order                                     │
    │  2. Groups entries by transaction ID                               │
    │  3. Only applies COMMITTED transactions                            │
    │  4. Calls apply_wal_entry() for each entry in commit order         │
    │                                                                     │
    │  This ensures:                                                      │
    │  - Transaction atomicity (partial transactions not applied)        │
    │  - Order preservation (entries applied in WAL order)               │
    │  - Cross-primitive atomicity (KV + Vector in same transaction)     │
    │                                                                     │
    │  fn apply_wal_entry(&mut self, entry: &WalEntry) -> Result<()> {   │
    │      match entry.entry_type {                                      │
    │          VectorCollectionCreate => self.create_collection(...),    │
    │          VectorCollectionDelete => self.delete_collection(...),    │
    │          VectorUpsert => self.insert_raw(...),                     │
    │          VectorDelete => self.delete_raw(...),                     │
    │          _ => {}                                                    │
    │      }                                                              │
    │  }                                                                  │
    │                                                                     │
    └─────────────────────────────────────────────────────────────────────┘
```

---

## 9. Snapshot Format

```
+-------------------------------------------------------------------------+
|                       Snapshot Format (M8)                               |
+-------------------------------------------------------------------------+

Vector Snapshot Section:
========================

    ┌─────────────────────────────────────────────────────────────────────┐
    │                  VECTOR SNAPSHOT SECTION LAYOUT                      │
    ├─────────────────────────────────────────────────────────────────────┤
    │                                                                     │
    │  ┌────────────────────────────────────────────────────────────┐    │
    │  │ Section Header (fixed)                                     │    │
    │  │  - Primitive ID: u8         │  0x07 (Vector)               │    │
    │  │  - Format Version: u8       │  0x01 (M8 format)            │    │
    │  │  - Section Length: u64 LE   │  Total bytes following       │    │
    │  ├────────────────────────────────────────────────────────────┤    │
    │  │ Collection Count: u32 LE                                   │    │
    │  ├────────────────────────────────────────────────────────────┤    │
    │  │ Collection 1                                               │    │
    │  │  ┌──────────────────────────────────────────────────────┐  │    │
    │  │  │ Header Length: u32 LE                                │  │    │
    │  │  ├──────────────────────────────────────────────────────┤  │    │
    │  │  │ Header (MessagePack)                                 │  │    │
    │  │  │  - run_id: u64                                       │  │    │
    │  │  │  - name: String                                      │  │    │
    │  │  │  - dimension: u32                                    │  │    │
    │  │  │  - metric: u8                                        │  │    │
    │  │  │  - storage_dtype: u8                                 │  │    │
    │  │  │  - next_id: u64           ← CRITICAL for ID recovery │  │    │
    │  │  │  - free_slots: Vec<usize> ← CRITICAL for slot reuse  │  │    │
    │  │  │  - count: u32                                        │  │    │
    │  │  ├──────────────────────────────────────────────────────┤  │    │
    │  │  │ Vectors (raw bytes)                                  │  │    │
    │  │  │  For each vector (count times):                      │  │    │
    │  │  │   - vector_id: u64 LE                                │  │    │
    │  │  │   - embedding: [f32 LE] (dimension * 4 bytes)        │  │    │
    │  │  └──────────────────────────────────────────────────────┘  │    │
    │  ├────────────────────────────────────────────────────────────┤    │
    │  │ Collection 2                                               │    │
    │  ├────────────────────────────────────────────────────────────┤    │
    │  │ ...                                                        │    │
    │  └────────────────────────────────────────────────────────────┘    │
    │                                                                     │
    └─────────────────────────────────────────────────────────────────────┘


Critical Fields for Recovery:
=============================

    ┌─────────────────────────────────────────────────────────────────────┐
    │                                                                     │
    │  next_id: u64                                                       │
    │  ─────────────                                                      │
    │  Without this, recovery would start allocating from 0,              │
    │  reusing VectorIds and breaking replay determinism.                 │
    │                                                                     │
    │  Example:                                                           │
    │  - Before crash: next_id = 100, vectors [0, 1, 2, ..., 99]         │
    │  - Without snapshot: recovery starts at 0                          │
    │  - New insert gets VectorId(0) → COLLISION!                        │
    │  - With snapshot: recovery restores next_id = 100                  │
    │  - New insert gets VectorId(100) → correct                         │
    │                                                                     │
    │                                                                     │
    │  free_slots: Vec<usize>                                            │
    │  ───────────────────────                                           │
    │  Without this, deleted slot tracking would be lost,                │
    │  potentially causing data corruption on recovery.                  │
    │                                                                     │
    │  Example:                                                           │
    │  - Before crash: slots 5, 12 are free (deleted vectors)            │
    │  - Without snapshot: free_slots = [] after recovery                │
    │  - New insert appends to end instead of reusing slot 5             │
    │  - Heap grows unnecessarily, slot 5 contains stale data            │
    │  - With snapshot: free_slots = [5, 12] restored                    │
    │  - New insert reuses slot 5 → correct                              │
    │                                                                     │
    └─────────────────────────────────────────────────────────────────────┘


Version Compatibility:
======================

    ┌─────────────────────────────────────────────────────────────────────┐
    │                                                                     │
    │  fn snapshot_deserialize(&mut self, data: &[u8]) -> Result<()> {   │
    │      let version = data[1];                                        │
    │      match version {                                                │
    │          0x01 => self.deserialize_v1(data),  // M8 format          │
    │          v => Err(SnapshotError::UnsupportedVersion { version: v })│
    │      }                                                              │
    │  }                                                                  │
    │                                                                     │
    │  - Format version 0x01: M8 initial format                          │
    │  - Future versions must support reading 0x01 format                │
    │  - Unknown versions: fail loudly, do not guess                     │
    │                                                                     │
    │  M9 may change format (e.g., for HNSW graph storage).              │
    │  Version byte ensures clean migration.                             │
    │                                                                     │
    └─────────────────────────────────────────────────────────────────────┘
```

---

## 10. Performance Characteristics

```
+-------------------------------------------------------------------------+
|                   Performance Characteristics (M8)                       |
+-------------------------------------------------------------------------+

M8 Expectations:
================

    ┌─────────────────────────────────────────────────────────────────────┐
    │                                                                     │
    │  M8 PRIORITIZES CORRECTNESS OVER SPEED.                             │
    │                                                                     │
    │  +─────────────────────────+──────────────+─────────────────────+  │
    │  │       Operation         │    Target    │        Notes        │  │
    │  +─────────────────────────+──────────────+─────────────────────+  │
    │  │ Insert (768 dims)       │   < 50 µs    │ Plus WAL write      │  │
    │  │ Get                     │   < 10 µs    │ Fast path           │  │
    │  │ Delete                  │   < 30 µs    │ Plus WAL write      │  │
    │  +─────────────────────────+──────────────+─────────────────────+  │
    │  │ Search 1K vectors       │   < 5 ms     │ Brute-force O(n)   │  │
    │  │ Search 10K vectors      │   < 50 ms    │ Brute-force O(n)   │  │
    │  │ Search 50K vectors      │   < 200 ms   │ Acceptable         │  │
    │  │ Search 100K vectors     │   > 500 ms   │ Forces M9 priority │  │
    │  +─────────────────────────+──────────────+─────────────────────+  │
    │                                                                     │
    └─────────────────────────────────────────────────────────────────────┘


Memory Overhead:
================

    ┌─────────────────────────────────────────────────────────────────────┐
    │                                                                     │
    │  +───────────────────────────────+────────────────────────────────+ │
    │  │          Component            │            Size                │ │
    │  +───────────────────────────────+────────────────────────────────+ │
    │  │ Per vector (768 dims)         │ 3 KB (768 × 4 bytes)          │ │
    │  │ Metadata per vector           │ ~100 bytes (typical)          │ │
    │  │ Collection overhead           │ ~1 KB                          │ │
    │  │ Index overhead                │ ~8 bytes per vector (ID map)  │ │
    │  +───────────────────────────────+────────────────────────────────+ │
    │                                                                     │
    └─────────────────────────────────────────────────────────────────────┘


Scaling Characteristics:
========================

    ┌─────────────────────────────────────────────────────────────────────┐
    │                                                                     │
    │  +────────────────+──────────────────+───────────────────────────+ │
    │  │ Dataset Size   │ Memory (768 dim) │     Search Latency        │ │
    │  +────────────────+──────────────────+───────────────────────────+ │
    │  │    1K vectors  │      ~3 MB       │        < 5 ms             │ │
    │  │   10K vectors  │     ~30 MB       │       < 50 ms             │ │
    │  │  100K vectors  │    ~300 MB       │      < 500 ms             │ │
    │  │    1M vectors  │     ~3 GB        │ > 5 seconds (needs HNSW)  │ │
    │  +────────────────+──────────────────+───────────────────────────+ │
    │                                                                     │
    │                                                                     │
    │  Visual: Search Latency vs Dataset Size                            │
    │  ──────────────────────────────────────                            │
    │                                                                     │
    │  Latency                                                            │
    │     │                                                               │
    │  5s │                                              ╱                │
    │     │                                            ╱                  │
    │  1s │                                          ╱  M9 HNSW needed   │
    │     │                                        ╱                      │
    │500ms│                                 ● ──╱                         │
    │     │                              ╱                                │
    │200ms│                        ● ──╱                                  │
    │     │                     ╱                                         │
    │ 50ms│              ● ──╱       M8 acceptable                       │
    │     │           ╱                                                   │
    │  5ms│    ● ──╱                                                      │
    │     │                                                               │
    │     └────────────────────────────────────────────────── Vectors    │
    │          1K    10K    50K   100K              1M                   │
    │                                                                     │
    └─────────────────────────────────────────────────────────────────────┘


Switch Threshold:
=================

    ┌─────────────────────────────────────────────────────────────────────┐
    │                                                                     │
    │  WHEN TO PRIORITIZE M9 (HNSW):                                      │
    │  ───────────────────────────────                                    │
    │                                                                     │
    │  ✓ P95 search latency > 100 ms                                      │
    │  ✓ Dataset exceeds 50K vectors                                      │
    │  ✓ Search QPS requirements > 10/sec at 10K+ vectors                │
    │                                                                     │
    │  M8's brute-force is ACCEPTABLE until these thresholds.             │
    │  After threshold, M9's HNSW becomes priority.                       │
    │                                                                     │
    └─────────────────────────────────────────────────────────────────────┘
```

---

## 11. Known Limitations

```
+-------------------------------------------------------------------------+
|                      Known Limitations (M8)                              |
+-------------------------------------------------------------------------+

Intentional M8 Limitations:
===========================

    ┌─────────────────────────────────────────────────────────────────────┐
    │                                                                     │
    │  +─────────────────────────+────────────────────+─────────────────+ │
    │  │      Limitation         │      Impact        │   Mitigation    │ │
    │  +─────────────────────────+────────────────────+─────────────────+ │
    │  │ Brute-force only        │ O(n) search        │ Add HNSW in M9  │ │
    │  │ No quantization         │ Higher memory      │ Add in M9       │ │
    │  │ Post-filtering only     │ May scan more      │ Pre-filter M9   │ │
    │  │ No batch optimization   │ insert_many = loop │ Optimize in M9  │ │
    │  │ Simple metadata filter  │ Equality only      │ Ranges in M9    │ │
    │  │ Single-threaded search  │ No parallelism     │ M9 parallel     │ │
    │  │ Non-interruptible scan  │ Budget at boundary │ HNSW interrupt  │ │
    │  │ Full embeddings in WAL  │ Large WAL, slow    │ M9 optimize     │ │
    │  +─────────────────────────+────────────────────+─────────────────+ │
    │                                                                     │
    └─────────────────────────────────────────────────────────────────────┘


Critical Implementation Constraints:
====================================

    ┌─────────────────────────────────────────────────────────────────────┐
    │                                                                     │
    │  A. SINGLE-THREADED, DETERMINISTIC COMPUTATION                      │
    │  ─────────────────────────────────────────────                      │
    │                                                                     │
    │  M8 MUST remain single-threaded for similarity computation.         │
    │                                                                     │
    │  Floating point is sensitive to:                                    │
    │  - Order of operations (associativity)                              │
    │  - SIMD instruction selection                                       │
    │  - Thread scheduling with parallel iterators                        │
    │                                                                     │
    │  CORRECT:                              WRONG:                       │
    │  ─────────                             ──────                       │
    │  self.heap.iter()                      self.heap.par_iter()         │
    │      .map(|(id, emb)| ...)                 .map(...)                │
    │      .collect()                            .collect()               │
    │                                        // Breaks replay determinism │
    │                                                                     │
    │  NO rayon. NO parallel iterators. NO SIMD that changes results.    │
    │                                                                     │
    │                                                                     │
    │  B. NO IMPLICIT NORMALIZATION                                       │
    │  ────────────────────────────                                       │
    │                                                                     │
    │  Vector primitive does NOT silently normalize embeddings.           │
    │  Embeddings are stored as-is.                                       │
    │                                                                     │
    │  Options for callers:                                               │
    │  1. Use Cosine metric (handles normalization internally)            │
    │  2. Pre-normalize vectors before insert (for DotProduct)           │
    │  3. Accept unnormalized DotProduct behavior                         │
    │                                                                     │
    └─────────────────────────────────────────────────────────────────────┘


What M8 Explicitly Does NOT Provide:
====================================

    ┌─────────────────────────────────────────────────────────────────────┐
    │                                                                     │
    │  ✗ HNSW or other ANN indexes                                        │
    │  ✗ Quantization (F16, Int8, PQ)                                     │
    │  ✗ Pre-filtering                                                    │
    │  ✗ Batch insert optimization                                        │
    │  ✗ Range metadata filters                                           │
    │  ✗ Nested path metadata filters                                     │
    │  ✗ GPU acceleration                                                 │
    │  ✗ Distributed vector search                                        │
    │                                                                     │
    │  These are all INTENTIONALLY DEFERRED, not forgotten.               │
    │                                                                     │
    └─────────────────────────────────────────────────────────────────────┘
```

---

## 12. Future Extension Points (M9+)

```
+-------------------------------------------------------------------------+
|                    Future Extension Points (M9+)                         |
+-------------------------------------------------------------------------+

M9: HNSW Backend:
=================

    ┌─────────────────────────────────────────────────────────────────────┐
    │                                                                     │
    │  pub struct HnswBackend {                                           │
    │      graph: HnswGraph,                                              │
    │      config: HnswConfig,                                            │
    │  }                                                                  │
    │                                                                     │
    │  pub struct HnswConfig {                                            │
    │      pub m: usize,              // Max connections per layer        │
    │      pub ef_construction: usize,// Construction ef                  │
    │      pub ef_search: usize,      // Search ef                        │
    │  }                                                                  │
    │                                                                     │
    │  impl VectorIndexBackend for HnswBackend {                          │
    │      // O(log n) search instead of O(n)                            │
    │      // Interruptible during graph traversal                        │
    │      // Better budget enforcement                                   │
    │  }                                                                  │
    │                                                                     │
    └─────────────────────────────────────────────────────────────────────┘


M9: Quantization:
=================

    ┌─────────────────────────────────────────────────────────────────────┐
    │                                                                     │
    │  pub enum StorageDtype {                                            │
    │      F32,           // Full precision (M8)                          │
    │      F16,           // Half precision (M9)                          │
    │      Int8,          // Scalar quantization (M9)                     │
    │      // PQ,         // Product quantization (future)                │
    │  }                                                                  │
    │                                                                     │
    │  Memory savings:                                                    │
    │  +────────────+─────────────+───────────────+                      │
    │  │   Dtype    │ Bytes/float │ 768-dim vec   │                      │
    │  +────────────+─────────────+───────────────+                      │
    │  │    F32     │      4      │    3072 bytes │                      │
    │  │    F16     │      2      │    1536 bytes │  50% reduction       │
    │  │   Int8     │      1      │     768 bytes │  75% reduction       │
    │  +────────────+─────────────+───────────────+                      │
    │                                                                     │
    └─────────────────────────────────────────────────────────────────────┘


M9: Complex Filtering:
======================

    ┌─────────────────────────────────────────────────────────────────────┐
    │                                                                     │
    │  pub struct MetadataFilter {                                        │
    │      pub equals: HashMap<String, JsonScalar>,  // M8               │
    │      pub range: Option<RangeFilter>,           // M9               │
    │      pub contains: Option<Vec<JsonScalar>>,    // M9               │
    │      pub path: Option<JsonPath>,               // M9               │
    │  }                                                                  │
    │                                                                     │
    │  pub struct RangeFilter {                                           │
    │      pub field: String,                                             │
    │      pub min: Option<f64>,                                          │
    │      pub max: Option<f64>,                                          │
    │  }                                                                  │
    │                                                                     │
    │  Example:                                                           │
    │  - M8: filter by { "category": "doc" }  (equality only)            │
    │  - M9: filter by { "price": 10..100 }   (range)                    │
    │  - M9: filter by { "tags": contains("important") }                 │
    │                                                                     │
    └─────────────────────────────────────────────────────────────────────┘


Extension Hooks in M8:
======================

    ┌─────────────────────────────────────────────────────────────────────┐
    │                                                                     │
    │  M8 code is designed for extension:                                 │
    │                                                                     │
    │  // Index backend is swappable                                      │
    │  impl VectorStore {                                                 │
    │      pub fn with_backend(                                           │
    │          &self,                                                     │
    │          backend: Box<dyn VectorIndexBackend>                       │
    │      ) -> Self;                                                     │
    │  }                                                                  │
    │                                                                     │
    │  // Metric is configurable at creation                              │
    │  VectorConfig {                                                     │
    │      metric: DistanceMetric::Cosine,  // or Euclidean, DotProduct  │
    │  }                                                                  │
    │                                                                     │
    │  // Storage dtype reserved for future                               │
    │  VectorConfig {                                                     │
    │      storage_dtype: StorageDtype::F32,  // Only F32 in M8          │
    │  }                                                                  │
    │                                                                     │
    └─────────────────────────────────────────────────────────────────────┘
```

---

## 13. M8 Philosophy

```
+-------------------------------------------------------------------------+
|                           M8 Philosophy                                  |
+-------------------------------------------------------------------------+

    ┌─────────────────────────────────────────────────────────────────────┐
    │                                                                     │
    │     M8 builds the API. M9 builds the speed.                         │
    │                                                                     │
    │     Vector is not a standalone database feature. It's a             │
    │     COMPOSITE PRIMITIVE that enables semantic search alongside      │
    │     keyword search. KV + JSON + Vector covers 99% of AI agent       │
    │     database needs.                                                 │
    │                                                                     │
    │     M8 validates the API and integration.                           │
    │     M9 optimizes for scale.                                         │
    │                                                                     │
    └─────────────────────────────────────────────────────────────────────┘


What M8 IS:
===========

    ┌─────────────────────────────────────────────────────────────────────┐
    │                                                                     │
    │  M8 is an API VALIDATION milestone, not a performance milestone.    │
    │                                                                     │
    │  ✓ Native vector storage with configurable dimensions/metrics       │
    │  ✓ Brute-force similarity search (validates API)                   │
    │  ✓ Full integration with M6 retrieval surfaces (hybrid search)     │
    │  ✓ Full integration with transaction system                         │
    │  ✓ VectorIndexBackend trait for M9 HNSW integration                │
    │                                                                     │
    │  The interfaces matter more than search speed.                      │
    │  We can add HNSW in M9.                                            │
    │                                                                     │
    └─────────────────────────────────────────────────────────────────────┘


What M8 is NOT:
===============

    ┌─────────────────────────────────────────────────────────────────────┐
    │                                                                     │
    │  M8 is NOT a performance milestone.                                 │
    │                                                                     │
    │  Brute-force search is O(n) and WILL become slow at scale.          │
    │  That is ACCEPTABLE.                                                │
    │                                                                     │
    │  If a feature requires HNSW, it is out of scope for M8.            │
    │                                                                     │
    └─────────────────────────────────────────────────────────────────────┘


Success Criteria:
=================

    ┌─────────────────────────────────────────────────────────────────────┐
    │                                                                     │
    │  Gate 1: Core Semantics                                            │
    │  ✓ VectorStore::insert() works (upsert semantics)                  │
    │  ✓ VectorStore::get() retrieves embedding + metadata               │
    │  ✓ VectorStore::delete() works                                     │
    │  ✓ VectorStore::search() returns top-k results                     │
    │  ✓ Dimension validation on insert and query                        │
    │                                                                     │
    │  Gate 2: Similarity Search                                         │
    │  ✓ Cosine, Euclidean, DotProduct scoring                           │
    │  ✓ Score normalization ("higher is better")                        │
    │  ✓ Deterministic tie-breaking                                      │
    │  ✓ Metadata filtering (post-filter, equality only)                 │
    │                                                                     │
    │  Gate 3: Index Support                                             │
    │  ✓ VectorIndexBackend trait defined                                │
    │  ✓ BruteForceBackend implemented                                   │
    │  ✓ Index persistence in snapshots                                  │
    │  ✓ Index recovery from WAL                                         │
    │                                                                     │
    │  Gate 4: M6 Integration                                            │
    │  ✓ DocRef::Vector, PrimitiveKind::Vector                           │
    │  ✓ Searchable trait implementation                                 │
    │  ✓ Hybrid search includes vectors                                  │
    │  ✓ RRF fusion works with vector results                            │
    │                                                                     │
    │  Gate 5: Transaction Integration                                   │
    │  ✓ VectorStoreExt trait for transactions                           │
    │  ✓ Cross-primitive atomicity (KV + Vector)                         │
    │  ✓ Conflict detection, rollback safety                             │
    │                                                                     │
    │  Gate 6: Durability                                                │
    │  ✓ WAL entry types (0x70-0x73)                                     │
    │  ✓ PrimitiveStorageExt implementation                              │
    │  ✓ Snapshot serialization                                          │
    │  ✓ Crash recovery                                                   │
    │                                                                     │
    └─────────────────────────────────────────────────────────────────────┘
```

---

These diagrams illustrate the key architectural components and flows for M8's Vector Primitive milestone. M8 builds upon M7's durability and storage stabilization while adding semantic search capabilities.

**Key Design Points Reflected in These Diagrams**:
- Vector is a composite primitive enabling semantic search alongside keyword search
- Hybrid storage: vector heap for fast scanning, KV for metadata and transactions
- VectorIndexBackend trait enables swapping brute-force for HNSW in M9
- Six non-negotiable architectural rules ensure correctness
- Full integration with M6 search, M2 transactions, M7 durability
- Determinism enforced at every layer (backend and facade)
- VectorId never reused, storage slots may be reused
- Score normalization: "higher is better" for all metrics

**M8 Philosophy**: M8 builds the API, M9 builds the speed. Brute-force O(n) search is acceptable for API validation. The interfaces matter more than search latency. Once validated, HNSW can be added behind the same interface in M9.
