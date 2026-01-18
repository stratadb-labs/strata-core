# M8 Implementation Plan: Vector Primitive

## Overview

This document provides the high-level implementation plan for M8 (Vector Primitive).

**Total Scope**: 6 Epics, 32 Stories

**References**:
- [M8 Architecture Specification](../../architecture/M8_ARCHITECTURE.md) - Authoritative spec
- [M8 Scope](../M8_SCOPE.md) - Sealed design decisions

**Critical Framing**:
> M8 is an **API validation milestone**, not a performance milestone. It validates the vector interface with brute-force search. M9 optimizes for scale with HNSW.
>
> The interfaces matter more than search speed. We can swap backends; we cannot easily change APIs.

**Epic Details**:
- [Epic 50: Core Types & Configuration](./EPIC_50_CORE_TYPES.md)
- [Epic 51: Vector Heap & Storage](./EPIC_51_VECTOR_HEAP.md)
- [Epic 52: Index Backend Abstraction](./EPIC_52_INDEX_BACKEND.md)
- [Epic 53: Collection Management](./EPIC_53_COLLECTION_MANAGEMENT.md)
- [Epic 54: M6 Search Integration](./EPIC_54_M6_INTEGRATION.md)
- [Epic 55: Transaction & Durability](./EPIC_55_TRANSACTION_DURABILITY.md)

---

## Architectural Integration Rules (NON-NEGOTIABLE)

These rules ensure M8 integrates properly with the M1-M7 architecture.

### Rule 1: Stateless Facade Pattern

VectorStore is a stateless facade. All state lives in Database. Multiple VectorStore instances on the same Database must be safe.

**FORBIDDEN**: Any local state in VectorStore (caches, indexes, buffers).

### Rule 2: Collections Per RunId

Collections are scoped to RunId. Different runs cannot see each other's collections.

**FORBIDDEN**: Global collections without run_id parameter.

### Rule 3: Upsert Semantics

Insert overwrites if key exists. No separate insert vs update methods.

**FORBIDDEN**: Separate insert/update that fail based on key existence.

### Rule 4: Dimension Validation

All vectors in a collection MUST have the same dimension. Enforce on insert AND query.

**FORBIDDEN**: Mixed dimensions in a collection.

### Rule 5: Deterministic Ordering at Every Layer

Backend sorts by (score desc, VectorId asc). Facade sorts by (score desc, key asc). Both layers enforce determinism independently.

**FORBIDDEN**: HashMap iteration order, arbitrary tie-breaking.

### Rule 6: VectorId Is Never Reused

Once a VectorId is assigned, it is never recycled. Storage slots may be reused, but IDs never are.

**FORBIDDEN**: Recycling VectorId values after deletion.

### Rule 7: No Backend-Specific Fields in VectorConfig

VectorConfig contains only primitive-level configuration. Backend-specific tuning (HNSW parameters, etc.) must NOT pollute this type.

**FORBIDDEN**: Adding `ef_construction`, `M`, or any HNSW-specific fields to VectorConfig.

---

## Core Invariants

### Storage Invariants

| # | Invariant | Test Strategy |
|---|-----------|---------------|
| S1 | Dimension immutable | Attempt dimension change, verify error |
| S2 | Metric immutable | Attempt metric change, verify error |
| S3 | VectorId stable | Track IDs across operations, verify no change |
| S4 | VectorId never reused | Insert → delete → insert, verify new ID |
| S5 | Heap + KV consistency | Concurrent operations, verify sync |
| S6 | Run isolation | Cross-run access, verify isolation |
| S7 | BTreeMap sole source | No secondary data structures for active vectors |
| S8 | Snapshot-WAL equivalence | Snapshot + WAL replay = pure WAL replay (catches serialization bugs) |
| S9 | Heap-KV reconstructibility | VectorHeap and KV metadata can be fully reconstructed from snapshot + WAL |

### Search Invariants

| # | Invariant | Test Strategy |
|---|-----------|---------------|
| R1 | Dimension match | Query with wrong dimension, verify error |
| R2 | Score normalization | All metrics return "higher is better" |
| R3 | Deterministic order | Same query = same results, property test |
| R4 | Backend tie-break | Score ties use VectorId asc |
| R5 | Facade tie-break | Score ties use key asc |
| R6 | Snapshot consistency | Concurrent writes during search |
| R7 | Coarse-grained budget | Budget at phase boundaries |
| R8 | Single-threaded | No parallel similarity computation |
| R9 | No implicit normalization | Embeddings stored exactly as provided |
| R10 | Search is read-only | No writes, no counters, no caches during search |

### Transaction Invariants

| # | Invariant | Test Strategy |
|---|-----------|---------------|
| T1 | Atomic visibility | Cross-primitive transaction tests |
| T2 | Conflict detection | Concurrent writes to same key |
| T3 | Rollback safety | Failed transaction cleanup |
| T4 | VectorId monotonicity across crashes | Crash → recover → insert: new ID > all previous IDs |

---

## Epic Overview

| Epic | Name | Stories | Dependencies |
|------|------|---------|--------------|
| 50 | Core Types & Configuration | 5 | M7 complete |
| 51 | Vector Heap & Storage | 6 | Epic 50 |
| 52 | Index Backend Abstraction | 5 | Epic 51 |
| 53 | Collection Management | 5 | Epic 51 |
| 54 | M6 Search Integration | 6 | Epic 52, 53 |
| 55 | Transaction & Durability | 5 | Epic 51, 52 |

---

## Epic 50: Core Types & Configuration

**Goal**: Define all type definitions for the Vector primitive

| Story | Description | Priority |
|-------|-------------|----------|
| #330 | VectorConfig Type Definition | FOUNDATION |
| #331 | DistanceMetric Enum | FOUNDATION |
| #332 | VectorEntry and VectorMatch Types | FOUNDATION |
| #333 | MetadataFilter and JsonScalar Types | HIGH |
| #334 | VectorError Enum | FOUNDATION |

**Acceptance Criteria**:
- [ ] `VectorConfig` with dimension, metric, storage_dtype fields
- [ ] `DistanceMetric` enum: Cosine, Euclidean, DotProduct
- [ ] `StorageDtype` enum with F32 (reserved for F16, Int8 in M9)
- [ ] `VectorEntry` with key, embedding, metadata, vector_id, version
- [ ] `VectorMatch` with key, score, metadata
- [ ] `MetadataFilter` with equals HashMap for equality filtering
- [ ] `JsonScalar` enum: Null, Bool, Number, String
- [ ] `VectorError` with all error variants
- [ ] Helper constructors: `VectorConfig::for_openai_ada()`, `for_minilm()`
- [ ] All types implement Debug, Clone; Config types implement PartialEq, Eq

---

## Epic 51: Vector Heap & Storage

**Goal**: Implement hybrid storage model (vector heap + KV metadata)

| Story | Description | Priority |
|-------|-------------|----------|
| #335 | VectorHeap Data Structure | CRITICAL |
| #336 | VectorHeap Insert/Upsert | CRITICAL |
| #337 | VectorHeap Delete with Slot Reuse | CRITICAL |
| #338 | VectorHeap Get and Iteration | CRITICAL |
| #339 | VectorRecord KV Metadata | HIGH |
| #340 | TypeTag Extensions (Vector, VectorConfig) | FOUNDATION |

**Acceptance Criteria**:
- [ ] `VectorHeap` with contiguous `Vec<f32>` storage
- [ ] `id_to_offset` using **BTreeMap** (not HashMap) for deterministic iteration
- [ ] `free_slots` Vec for storage slot reuse
- [ ] `next_id` AtomicU64 (monotonically increasing, never recycled)
- [ ] `upsert()` updates in-place or allocates new slot
- [ ] `upsert()` copies embedding into reused slots correctly
- [ ] `delete()` adds offset to free_slots, zeros data
- [ ] `get()` returns &[f32] slice by VectorId
- [ ] `iter()` returns vectors in VectorId order (BTreeMap iteration)
- [ ] `VectorRecord` with vector_id, metadata, version, timestamps
- [ ] TypeTag::Vector = 0x70, TypeTag::VectorConfig = 0x71

---

## Epic 52: Index Backend Abstraction

**Goal**: Implement VectorIndexBackend trait and BruteForceBackend

| Story | Description | Priority |
|-------|-------------|----------|
| #341 | VectorIndexBackend Trait Definition | CRITICAL |
| #342 | BruteForceBackend Implementation | CRITICAL |
| #343 | Distance Metric Calculations | CRITICAL |
| #344 | Deterministic Search Ordering | CRITICAL |
| #345 | Score Normalization | HIGH |

**Acceptance Criteria**:
- [ ] `VectorIndexBackend` trait with insert, delete, search, len, dimension, metric
- [ ] Trait is `Send + Sync` for future concurrency
- [ ] `BruteForceBackend` wrapping VectorHeap
- [ ] `search()` returns `Vec<(VectorId, f32)>` sorted by (score desc, VectorId asc)
- [ ] Cosine similarity: `dot(a,b) / (||a|| * ||b||)`
- [ ] Euclidean similarity: `1 / (1 + l2_distance)`
- [ ] Dot product: raw value (assumes normalized vectors)
- [ ] All scores "higher is better"
- [ ] Zero-norm handling (return 0.0 for cosine)
- [ ] Helper functions: `dot_product()`, `l2_norm()`, `euclidean_distance()`

---

## Epic 53: Collection Management

**Goal**: Implement collection CRUD operations

| Story | Description | Priority |
|-------|-------------|----------|
| #346 | CollectionInfo and CollectionId Types | FOUNDATION |
| #347 | create_collection() Implementation | CRITICAL |
| #348 | delete_collection() Implementation | CRITICAL |
| #349 | list_collections() and get_collection() | HIGH |
| #350 | Collection Config Persistence | HIGH |

**Acceptance Criteria**:
- [ ] `CollectionInfo` with name, config, count, created_at
- [ ] `CollectionId` = (RunId, name)
- [ ] `create_collection()` validates dimension > 0, stores config in KV
- [ ] `create_collection()` returns error if collection already exists
- [ ] `delete_collection()` removes all vectors and config
- [ ] `list_collections()` returns all collections for run_id
- [ ] `get_collection()` returns Option<CollectionInfo>
- [ ] Collection config persisted via VectorConfig WAL entry
- [ ] Collection names validated (non-empty, no "/" character)

---

## Epic 54: M6 Search Integration

**Goal**: Integrate vector search with M6 retrieval surfaces

| Story | Description | Priority |
|-------|-------------|----------|
| #351 | VectorStore Facade Implementation | CRITICAL |
| #352 | search() Method with Metadata Filtering | CRITICAL |
| #353 | search_request() for SearchRequest/SearchResponse | CRITICAL |
| #354 | DocRef::Vector Variant | HIGH |
| #355 | RRF Hybrid Search Fusion | CRITICAL |
| #356 | Vector Searchable Implementation | HIGH |

**Acceptance Criteria**:
- [ ] `VectorStore` stateless facade over Database
- [ ] `insert()`, `get()`, `delete()`, `count()` operations
- [ ] `search()` with query, k, optional MetadataFilter
- [ ] Post-filtering: over-fetch candidates, filter, return top-k
- [ ] `search_request(&SearchRequest) -> SearchResponse`
- [ ] `DocRef::Vector { collection, key }` variant
- [ ] RRF fusion: `1 / (k + rank)` with k=60
- [ ] Hybrid search combines keyword (BM25) + vector results
- [ ] Vector `Searchable` impl returns empty for `SearchMode::Keyword`
- [ ] Deterministic tie-breaking at facade level (score desc, key asc)

---

## Epic 55: Transaction & Durability

**Goal**: Integrate with transaction system and M7 durability

| Story | Description | Priority |
|-------|-------------|----------|
| #357 | Vector WAL Entry Types | CRITICAL |
| #358 | Vector WAL Write and Replay | CRITICAL |
| #359 | Vector Snapshot Serialization | CRITICAL |
| #360 | Vector Recovery Implementation | CRITICAL |
| #361 | Cross-Primitive Transaction Tests | HIGH |

**Acceptance Criteria**:
- [ ] WAL entry types: 0x70 COLLECTION_CREATE, 0x71 COLLECTION_DELETE, 0x72 UPSERT, 0x73 DELETE
- [ ] WAL payloads include full embedding (marked as temporary for M9 optimization)
- [ ] WAL replay is transaction-aware (uses global replayer, respects commit markers)
- [ ] Snapshot format: version byte (0x01), MessagePack headers, raw f32 embeddings
- [ ] Snapshot includes `next_id` and `free_slots` for each collection
- [ ] Recovery loads snapshot, replays WAL from offset
- [ ] Vector operations participate in cross-primitive transactions
- [ ] KV + JSON + Vector in same transaction recovers atomically

---

## WAL Entry Types

```rust
// Vector WAL entries: 0x70-0x7F range (reserved in M7)
pub const WAL_VECTOR_COLLECTION_CREATE: u8 = 0x70;
pub const WAL_VECTOR_COLLECTION_DELETE: u8 = 0x71;
pub const WAL_VECTOR_UPSERT: u8 = 0x72;
pub const WAL_VECTOR_DELETE: u8 = 0x73;
```

**Naming rationale**: `COLLECTION_CREATE`/`DELETE` are prefixed to avoid confusion with vector-level operations. `UPSERT` (not `INSERT`) because our semantic is always upsert.

---

## Snapshot Format

```
Vector Snapshot Section:
┌─────────────────────────────────────────────┐
│ Version byte: 0x01                          │  (1 byte)
├─────────────────────────────────────────────┤
│ Collection count (u32 LE)                   │  (4 bytes)
├─────────────────────────────────────────────┤
│ For each collection:                        │
│  ┌─────────────────────────────────────────┐│
│  │ Header (MessagePack):                   ││
│  │  - run_id                               ││
│  │  - name                                 ││
│  │  - dimension                            ││
│  │  - metric                               ││
│  │  - storage_dtype                        ││
│  │  - next_id (CRITICAL)                   ││
│  │  - free_slots (CRITICAL)               ││
│  │  - count                                ││
│  ├─────────────────────────────────────────┤│
│  │ Vector data (raw f32 LE):               ││
│  │  For each vector in VectorId order:     ││
│  │   - VectorId (u64 LE)                   ││
│  │   - Key length (u32 LE)                 ││
│  │   - Key (UTF-8 bytes)                   ││
│  │   - Embedding (dimension * f32 LE)      ││
│  │   - Has metadata flag (u8)              ││
│  │   - Metadata (MessagePack, if present)  ││
│  └─────────────────────────────────────────┘│
└─────────────────────────────────────────────┘
```

---

## Files to Modify/Create

| File | Action | Description |
|------|--------|-------------|
| `crates/primitives/src/vector/mod.rs` | CREATE | Vector module entry point |
| `crates/primitives/src/vector/types.rs` | CREATE | VectorConfig, VectorEntry, VectorMatch, etc. |
| `crates/primitives/src/vector/error.rs` | CREATE | VectorError enum |
| `crates/primitives/src/vector/heap.rs` | CREATE | VectorHeap implementation |
| `crates/primitives/src/vector/backend.rs` | CREATE | VectorIndexBackend trait |
| `crates/primitives/src/vector/brute_force.rs` | CREATE | BruteForceBackend implementation |
| `crates/primitives/src/vector/store.rs` | CREATE | VectorStore facade |
| `crates/primitives/src/vector/filter.rs` | CREATE | MetadataFilter, JsonScalar |
| `crates/primitives/src/vector/search.rs` | CREATE | Search implementation |
| `crates/primitives/src/vector/collection.rs` | CREATE | Collection management |
| `crates/primitives/src/lib.rs` | MODIFY | Export vector module |
| `crates/core/src/key.rs` | MODIFY | Add Key::new_vector(), Key::new_vector_config() |
| `crates/core/src/type_tag.rs` | MODIFY | Add TypeTag::Vector, TypeTag::VectorConfig |
| `crates/durability/src/wal_types.rs` | MODIFY | Add vector WAL entry types |
| `crates/durability/src/snapshot.rs` | MODIFY | Add vector snapshot serialization |
| `crates/durability/src/recovery.rs` | MODIFY | Add vector recovery logic |
| `crates/engine/src/database.rs` | MODIFY | Wire VectorStore, add vector_store() method |
| `crates/search/src/hybrid.rs` | MODIFY | Add vector to hybrid search fusion |
| `crates/search/src/doc_ref.rs` | MODIFY | Add DocRef::Vector variant |
| `crates/search/src/searchable.rs` | MODIFY | Add Vector Searchable implementation |

---

## Dependency Order

```
Epic 50 (Core Types)
    ↓
Epic 51 (Vector Heap) ←── Epic 50
    ↓
Epic 52 (Index Backend) ←── Epic 51
    ↓
Epic 53 (Collection Management) ←── Epic 51
    ↓
Epic 54 (M6 Integration) ←── Epic 52, 53
    ↓
Epic 55 (Transaction & Durability) ←── Epic 51, 52
```

**Recommended Implementation Order**:
1. Epic 50: Core Types & Configuration
2. Epic 51: Vector Heap & Storage
3. Epic 52: Index Backend Abstraction
4. Epic 53: Collection Management
5. Epic 55: Transaction & Durability
6. Epic 54: M6 Search Integration

---

## Success Metrics

**Functional**: All 32 stories passing, 100% acceptance criteria met

**Correctness**:
- All storage invariants (S1-S9) validated
- All search invariants (R1-R10) validated
- All transaction invariants (T1-T4) validated
- Determinism verified via property tests
- Recovery verified via crash simulation
- Snapshot-WAL equivalence verified (S8)
- VectorId monotonicity across crashes verified (T4)

**Performance** (M8 baselines, not targets):
- Insert (384-1536 dims): < 10ms
- Search 1K vectors: < 5 ms
- Search 10K vectors: < 50 ms
- Search 50K vectors: < 200 ms (borderline, triggers M9)
- Hybrid search: within M6 budget constraints

**Quality**: Test coverage > 90% for new code

---

## Risk Mitigation

| Risk | Likelihood | Impact | Mitigation |
|------|------------|--------|------------|
| Brute-force too slow | Medium | Medium | VectorIndexBackend trait, add HNSW in M9 |
| Vector heap complexity | Low | Medium | Simple design, test recovery thoroughly |
| API churn before HNSW | Low | Low | Trait abstraction isolates changes |
| Memory usage | Medium | Medium | Monitor, add quantization in M9 |
| Nondeterminism bugs | Medium | High | BTreeMap, explicit tie-breaking, property tests |
| Snapshot format bugs | Medium | High | Version byte, comprehensive recovery tests |

---

## Not In Scope (Explicitly Deferred)

1. **HNSW index** - M9
2. **Quantization (F16, Int8)** - M9
3. **Complex metadata filtering** - M9 (ranges, nested paths, arrays)
4. **Pre-filtering** - M9
5. **Batch insert optimization** - M9
6. **GPU acceleration** - Post-MVP
7. **Distributed vector search** - Post-MVP
8. **Shared collections across runs** - Post-MVP (ScopeId)
9. **Automatic embedding generation** - Never (user provides embeddings)

---

## Post-M8 Expectations

After M8 completion:
1. Vector primitive API is validated and stable
2. Brute-force search works correctly for < 50K vectors
3. Vector integrates with M6 hybrid search (RRF fusion)
4. Vector participates in cross-primitive transactions
5. Vector recovery uses M7 infrastructure (WAL + snapshots)
6. `VectorIndexBackend` trait ready for M9 HNSW implementation
7. Performance baselines documented for M9 comparison

---

## Testing Strategy

### Unit Tests
- VectorHeap operations (insert, delete, get, iter)
- Distance metric calculations
- Score normalization
- MetadataFilter matching
- Key construction

### Integration Tests
- Collection CRUD lifecycle
- Search with various dimensions and metrics
- Metadata filtering
- Transaction atomicity
- Cross-primitive transactions

### Recovery Tests
- Snapshot write and load
- WAL replay
- Crash during insert
- Crash during delete
- Recovery with incomplete transactions
- **Snapshot-WAL equivalence** (S8): Compare state from snapshot+WAL vs pure WAL replay
- **VectorId monotonicity across crashes** (T4): Crash → recover → insert, verify ID > max previous

### Determinism Tests
- Same query = same results (property test)
- Tie-breaking verification
- BTreeMap iteration order
- **Search read-only verification** (R10): Search must not mutate any state

### Invariant Tests
- **Heap-KV reconstructibility** (S9): Verify both representations can be rebuilt from WAL
- No hidden state in heap (all state derivable from WAL entries)
- No hidden state in KV metadata (all state derivable from WAL entries)

### Performance Tests
- Search latency at 1K, 10K, 50K vectors
- Insert throughput
- Memory usage per vector
