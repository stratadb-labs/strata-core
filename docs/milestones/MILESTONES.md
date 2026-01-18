# Project Milestones: In-Memory Agent Database

## MVP Target: Single-Node, Embedded Library with Core Primitives + Performance + Replay

---

## Milestone 1: Foundation ✅
**Goal**: Basic storage and WAL without transactions

**Deliverable**: Can store/retrieve KV pairs and append to WAL, recover from WAL on restart

**Status**: Complete

**Success Criteria**:
- [x] Cargo workspace builds
- [x] Core types defined (RunId, Key, Value, TypeTag)
- [x] UnifiedStore stores and retrieves values
- [x] WAL appends entries and can be read back
- [x] Basic recovery: restart process, replay WAL, restore state
- [x] Unit tests pass

**Risk**: Foundation bugs will cascade. Must get this right.

---

## Milestone 2: Transactions ✅
**Goal**: OCC with snapshot isolation and conflict detection

**Deliverable**: Concurrent transactions with proper isolation and rollback

**Status**: Complete

**Success Criteria**:
- [x] TransactionContext with read/write sets
- [x] Snapshot isolation (ClonedSnapshotView)
- [x] Conflict detection at commit
- [x] CAS operations work
- [x] Multi-threaded tests show proper isolation
- [x] Conflict resolution (retry/abort) works

**Risk**: Concurrency bugs are subtle. Need thorough testing.

---

## Milestone 3: Primitives ✅
**Goal**: All 5 MVP primitives working (KV, Event Log, StateCell, Trace, Run Index)

**Deliverable**: Agent can use all primitive APIs

**Status**: Complete

**Success Criteria**:
- [x] KV store: get, put, delete, list
- [x] Event log: append, read, simple chaining (non-crypto hash)
- [x] StateCell: read, init, cas, set, transition
- [x] Trace store: record tool calls, decisions, queries
- [x] Run Index: create_run, get_run, update_status, query_runs
- [x] All primitives are stateless facades over engine
- [x] Integration tests cover primitive interactions

**Risk**: Layer boundaries. Primitives must not leak into each other.

---

## Milestone 4: Performance ✅
**Goal**: Remove architectural blockers to Redis-class latency

**Deliverable**: Database achieves 250K ops/sec in InMemory mode with <10µs read latency

**Status**: Complete

**Philosophy**: M4 does not aim to be fast. M4 aims to be *fastable*. M4 removes blockers; M5+ achieves parity.

**Critical Invariants** (validated via codebase analysis):
- **Atomicity Scope**: Transactions atomic within single RunId only; cross-run atomicity not guaranteed
- **Snapshot Semantics**: Fast-path reads must be observationally equivalent to snapshot-based transactions
- **Dependencies**: Use `rustc-hash` (not `fxhash`), `dashmap`, `parking_lot`

**Success Criteria**:

### Gate 1: Durability Modes
- [x] Three modes implemented: InMemory, Buffered, Strict
- [x] InMemory mode: `engine/put_direct` < 3µs
- [x] InMemory mode: 250K ops/sec (1-thread)
- [x] Buffered mode: `kvstore/put` < 30µs
- [x] Buffered mode: 50K ops/sec throughput
- [x] Buffered mode: Thread lifecycle managed (shutdown flag + join)
- [x] Strict mode: Same behavior as M3 (backwards compatible)

### Gate 2: Hot Path Optimization
- [x] Transaction pooling: Zero allocations in A1 hot path
- [x] Snapshot acquisition: < 500ns, allocation-free
- [x] Read optimization: `kvstore/get` < 10µs

### Gate 3: Scaling
- [x] Lock sharding: DashMap + HashMap replaces RwLock + BTreeMap
- [x] Disjoint scaling ≥ 1.8× at 2 threads
- [x] Disjoint scaling ≥ 3.2× at 4 threads
- [x] 4-thread disjoint throughput: ≥ 800K ops/sec

### Gate 4: Facade Tax
- [x] A1/A0 < 10× (InMemory mode)
- [x] B/A1 < 5×
- [x] B/A0 < 30×

### Gate 5: Infrastructure
- [x] Baseline tagged: `m3_baseline_perf`
- [x] Per-layer instrumentation working
- [x] Backwards compatibility: M3 code unchanged

### Red Flag Check (hard stops)
- [x] Snapshot acquisition ≤ 2µs
- [x] A1/A0 ≤ 20×
- [x] B/A1 ≤ 8×
- [x] Disjoint scaling (4 threads) ≥ 2.5×
- [x] p99 ≤ 20× mean
- [x] Zero hot-path allocations

**Risk**: Performance work can be unbounded. M4 is scoped to *de-blocking*, not *optimization*. Red flags define hard stops. ✅ Mitigated

**Architecture Doc**: [M4_ARCHITECTURE.md](../architecture/M4_ARCHITECTURE.md)
**Diagrams**: [m4-architecture.md](../diagrams/m4-architecture.md)

---

## Milestone 5: JSON Primitive ✅
**Goal**: Native JSON primitive with path-level mutation semantics

**Deliverable**: JsonStore primitive with region-based conflict detection, integrated into transaction system

**Status**: Complete

**Philosophy**: JSON is not a value type. It defines **mutation semantics**. M5 freezes the semantic model. M6+ optimizes the implementation.

**Success Criteria**:

### Gate 1: Core Semantics
- [x] JsonStore::create() works
- [x] JsonStore::get(path) works
- [x] JsonStore::set(path) works
- [x] JsonStore::delete(path) works
- [x] JsonStore::cas() works with document version
- [x] JsonStore::patch() applies multiple operations atomically

### Gate 2: Conflict Detection
- [x] Sibling paths do not conflict
- [x] Ancestor/descendant paths conflict
- [x] Same path conflicts
- [x] Different documents do not conflict
- [x] Root path conflicts with all paths

### Gate 3: WAL Integration
- [x] JSON WAL entries written correctly (0x20-0x23)
- [x] WAL replay is deterministic
- [x] WAL replay is idempotent
- [x] Recovery works after simulated crash

### Gate 4: Transaction Integration
- [x] JSON participates in transactions
- [x] Read-your-writes works
- [x] Cross-primitive atomicity works
- [x] Conflict detection fails transaction correctly

### Gate 5: Non-Regression
- [x] KV performance unchanged
- [x] Event performance unchanged
- [x] State performance unchanged
- [x] Trace performance unchanged
- [x] Non-JSON transactions have zero overhead

**Risk**: Semantic complexity. Must lock in semantics before optimization. ✅ Mitigated

**Architecture Doc**: [M5_ARCHITECTURE.md](../architecture/M5_ARCHITECTURE.md)

---

## Milestone 6: Retrieval Surfaces ✅
**Goal**: Add retrieval surface for fast experimentation with search and ranking across all primitives

**Deliverable**: Primitive-native search hooks + composite search planner + minimal keyword search algorithm

**Status**: Complete

**Philosophy**: M6 is the "retrieval substrate milestone". It does not ship a world-class search engine. It ships the **surface** that enables algorithm swaps without engine rewrites.

**Success Criteria**:

### Gate 1: Primitive Search APIs
- [x] `kv.search(&SearchRequest)` returns `SearchResponse`
- [x] `json.search(&SearchRequest)` returns `SearchResponse`
- [x] `event.search(&SearchRequest)` returns `SearchResponse`
- [x] `state.search(&SearchRequest)` returns `SearchResponse`
- [x] `trace.search(&SearchRequest)` returns `SearchResponse`
- [x] `run_index.search(&SearchRequest)` returns `SearchResponse`

### Gate 2: Composite Search
- [x] `db.hybrid.search(&SearchRequest)` orchestrates across primitives
- [x] RRF (Reciprocal Rank Fusion) with k_rrf=60 implemented
- [x] Primitive filters honored
- [x] Time range filters work
- [x] Budget enforcement (time and candidate caps)

### Gate 3: Core Types
- [x] `SearchDoc` ephemeral view with DocRef back-pointer
- [x] `DocRef` variants for all primitives (Kv, Json, Event, State, Trace, Run)
- [x] `SearchRequest` with query, k, budget, mode, filters
- [x] `SearchResponse` with hits, truncated flag, stats

### Gate 4: Indexing (Optional)
- [x] Inverted index per primitive (opt-in)
- [x] BM25-lite scoring over extracted text
- [x] Index updates on commit (synchronous)
- [x] Snapshot-consistent search results

### Gate 5: Non-Regression
- [x] Zero overhead when search APIs not used
- [x] No extra allocations per transaction when search disabled
- [x] No background indexing unless opted in

**Risk**: Scope creep into full search engine. M6 validates the surface only. ✅ Mitigated

**Architecture Doc**: [M6_ARCHITECTURE.md](../architecture/M6_ARCHITECTURE.md)

---

## Milestone 7: Durability, Snapshots, Replay & Storage Stabilization ✅
**Goal**: Production-ready persistence with snapshots, replay, and stabilized storage engine

**Deliverable**: Database survives crashes, restarts correctly, replays runs deterministically, and has a stable storage foundation

**Status**: Complete

**Philosophy**: M7 consolidates all durability concerns into one milestone. Snapshots enable efficient recovery, replay enables debugging and time-travel, and storage stabilization ensures a solid foundation for future primitives (Vector in M8).

**Success Criteria**:

### Gate 1: Snapshot System
- [x] Periodic snapshots (time-based and size-based)
- [x] Snapshot metadata includes version and WAL offset
- [x] WAL truncation after snapshot
- [x] Full recovery: load snapshot + replay WAL

### Gate 2: Crash Recovery
- [x] Crash simulation tests pass
- [x] Durability modes from M4 integrate with snapshot system
- [x] Bounded recovery time (proportional to WAL size since last snapshot)

### Gate 3: JSON & Cross-Primitive Recovery
- [x] JSON documents recovered correctly from WAL
- [x] JSON patches replayed in order
- [x] Cross-primitive transactions recover atomically

### Gate 4: Deterministic Replay
- [x] replay_run(run_id) reconstructs database state
- [x] Run Index enables O(run size) replay (not O(WAL size))
- [x] diff_runs(run_a, run_b) compares two runs
- [x] Run lifecycle (begin_run, end_run) fully working

### Gate 5: Storage Stabilization
- [x] Storage engine API frozen for M8+ primitives
- [x] Clear extension points for new primitive types
- [x] Performance benchmarks documented as baseline

**Risk**: Data loss bugs. Must test recovery thoroughly. Replay determinism is subtle. ✅ Mitigated

**Architecture Doc**: [M7_ARCHITECTURE.md](../architecture/M7_ARCHITECTURE.md)

---

## Milestone 8: Vector Primitive
**Goal**: Native vector primitive for semantic search and AI agent memory

**Deliverable**: VectorStore primitive with similarity search, integrated into transaction system and M6 retrieval surface

**Philosophy**: Vector is not a standalone database feature. It's a **composite primitive** built on KV, enabling semantic search alongside keyword search. KV + JSON + Vector covers 99% of AI agent database needs.

**Success Criteria**:

### Gate 1: Core Semantics
- [ ] VectorStore::insert(key, embedding, metadata) works
- [ ] VectorStore::search(query_vector, k) returns top-k results
- [ ] VectorStore::delete(key) works
- [ ] VectorStore::get(key) retrieves embedding + metadata

### Gate 2: Similarity Search
- [ ] Cosine similarity scoring
- [ ] Metadata filtering (pre-filter or post-filter)
- [ ] Configurable distance metrics (cosine, euclidean, dot product)

### Gate 3: Index Support
- [ ] Brute-force search for small datasets
- [ ] HNSW index for larger datasets (optional, can be deferred to M9)
- [ ] Index persistence and recovery

### Gate 4: M6 Integration
- [ ] `vector.search(&SearchRequest)` returns `SearchResponse`
- [ ] Hybrid search: keyword (BM25) + semantic (vector) fusion
- [ ] RRF fusion works across keyword and vector results

### Gate 5: Transaction Integration
- [ ] Vector operations participate in transactions
- [ ] Snapshot-consistent vector search
- [ ] Cross-primitive atomicity (KV + Vector in same transaction)

**Risk**: HNSW complexity. Start with brute-force, add HNSW when needed.

---

## Milestone 9: Performance & Indexing
**Goal**: Optimize hot paths and add secondary indexing capabilities

**Deliverable**: Faster queries, better scaling, and indexing infrastructure for all primitives

**Philosophy**: M9 is the "make it fast" milestone. By now we have real workloads from M7/M8. Optimize based on data, not speculation. HNSW refinement belongs here if not completed in M8.

**Success Criteria**:

### Gate 1: Query Optimization
- [ ] Profile and optimize hot paths identified in M7/M8
- [ ] Index-accelerated lookups where beneficial
- [ ] Query planning for complex searches

### Gate 2: Secondary Indexes
- [ ] Secondary index infrastructure (B-tree or similar)
- [ ] Index on JSON paths
- [ ] Index on metadata fields

### Gate 3: HNSW Refinement (if deferred from M8)
- [ ] HNSW parameter tuning
- [ ] Incremental index updates
- [ ] Index compaction

### Gate 4: Scaling
- [ ] Multi-threaded index builds
- [ ] Parallel query execution
- [ ] Memory usage optimization

**Risk**: Premature optimization. Only optimize what benchmarks show matters.

---

## Milestone 10: Python Client
**Goal**: First-class Python client for AI agent developers

**Deliverable**: Python SDK with ergonomic API, async support, and comprehensive documentation

**Philosophy**: Python dominates AI/ML tooling. A clean Python client unlocks the majority of agent developers. This is the MVP client library.

**Success Criteria**:

### Gate 1: Core Client
- [ ] Python package installable via pip
- [ ] Connection management and configuration
- [ ] All primitive operations exposed (KV, JSON, Event, State, Trace, Run, Vector)

### Gate 2: Ergonomic API
- [ ] Pythonic API design (context managers, iterators, type hints)
- [ ] Async support (asyncio)
- [ ] Error handling with clear exception hierarchy

### Gate 3: Search Integration
- [ ] Search API with query builders
- [ ] Hybrid search (keyword + vector) support
- [ ] Pagination and streaming results

### Gate 4: Documentation & Examples
- [ ] API documentation
- [ ] Example agent applications
- [ ] Jupyter notebook tutorials

**Risk**: API design can bike-shed. Ship MVP, iterate based on feedback.

---

## Milestone 11: Security & Multi-Tenancy
**Goal**: Production security features and tenant isolation

**Deliverable**: Authentication, authorization, and multi-tenant support

**Philosophy**: Security is table stakes for production. Multi-tenancy enables SaaS deployment. These features shouldn't slow core development but are essential before production.

**Success Criteria**:

### Gate 1: Authentication
- [ ] API key authentication
- [ ] Token-based auth (JWT or similar)
- [ ] Connection-level authentication

### Gate 2: Authorization
- [ ] Role-based access control (RBAC)
- [ ] Primitive-level permissions
- [ ] Run-level access control

### Gate 3: Multi-Tenancy
- [ ] Tenant isolation (data separation)
- [ ] Per-tenant resource limits
- [ ] Tenant-aware routing

### Gate 4: Security Hardening
- [ ] Encryption at rest (optional)
- [ ] Audit logging
- [ ] Security review and penetration testing

**Risk**: Security features can expand infinitely. Scope to essential production needs.

---

## Milestone 12: Production Readiness
**Goal**: Operational excellence for production deployment

**Deliverable**: Observable, maintainable, deployable system

**Philosophy**: M12 is the capstone milestone. Everything needed to run in production with confidence: monitoring, deployment, documentation.

**Success Criteria**:

### Gate 1: Observability
- [ ] Metrics export (Prometheus/OpenTelemetry)
- [ ] Structured logging
- [ ] Distributed tracing integration
- [ ] Health checks and readiness probes

### Gate 2: Operations
- [ ] Graceful shutdown and startup
- [ ] Configuration management
- [ ] Backup and restore procedures
- [ ] Upgrade/migration tooling

### Gate 3: Deployment
- [ ] Docker image
- [ ] Kubernetes manifests / Helm chart
- [ ] Cloud deployment guides (AWS, GCP, Azure)

### Gate 4: Documentation
- [ ] Operations runbook
- [ ] Architecture documentation
- [ ] API reference (complete)
- [ ] Performance tuning guide

### Gate 5: Quality
- [ ] Integration test coverage >90%
- [ ] Load testing and benchmarks
- [ ] Chaos engineering tests
- [ ] Example agent application works end-to-end

**Risk**: Scope creep. Define "production ready" clearly and stick to it.

---

## Post-MVP Enhancements (Future)

### JSON Optimization (Structural Storage)
- Per-node versioning / subtree MVCC
- Structural sharing for efficient snapshots
- Array insert/remove with stable identities
- Diff operations

### Advanced Search
- Enhanced hybrid search algorithms
- Learning-to-rank integration
- Query expansion and synonyms

### MCP Integration
- MCP server implementation
- Tool definitions for agent access
- IDE integration demos

### Network Layer Enhancements
- gRPC server
- Additional client libraries (TypeScript, Go)
- Connection pooling and load balancing

### Performance Phase 2 (Redis Parity)
- Arena allocators and memory management
- Cache-line alignment and SoA transforms
- Lock-free reads (epoch-based/RCU)
- Prefetching and branch optimization
- Target: Millions ops/sec (Redis internal loop parity)

### Advanced Features
- Query DSL for complex filters
- Run forking and lineage tracking
- Incremental snapshots
- Distributed mode (far future)

---

## MVP Definition

**MVP = Milestones 1-12 Complete**

At MVP completion, the system should:
1. Store agent state in 7 primitives (KV, Events, StateCell, Trace, RunIndex, JSON, **Vector**)
2. Support concurrent transactions with OCC
3. **Achieve Redis-competitive performance in InMemory mode (250K+ ops/sec)**
4. Persist data with WAL and snapshots
5. Survive crashes and recover correctly
6. Replay runs deterministically
7. Run as embedded library (single-node)
8. Scale near-linearly for disjoint keys (multi-thread)
9. Have >90% test coverage
10. **JSON primitive with path-level mutations and region-based conflict detection**
11. **Retrieval surface with primitive-native search and composite hybrid search**
12. **Vector primitive with semantic search and hybrid retrieval (keyword + vector)**
13. **Python client library for AI agent developers**
14. **Security: authentication, authorization, multi-tenancy**
15. **Production-ready: observability, deployment, documentation**

**Not in MVP**:
- JSON structural optimization (post-MVP enhancement)
- Redis internal loop parity (post-MVP enhancement)
- MCP server (post-MVP enhancement)
- Additional client libraries beyond Python (post-MVP enhancement)
- Distributed mode (far future)

---

## Timeline

```
Completed:
- M1 (Foundation)         ✅
- M2 (Transactions)       ✅
- M3 (Primitives)         ✅
- M4 (Performance)        ✅
- M5 (JSON Primitive)     ✅
- M6 (Retrieval Surfaces) ✅
- M7 (Durability, Snapshots, Replay & Storage) ✅

Current:
- M8 (Vector Primitive) ← YOU ARE HERE

Remaining:
- M9 (Performance & Indexing)
- M10 (Python Client)
- M11 (Security & Multi-Tenancy)
- M12 (Production Readiness)
```

---

## Critical Path

```
M1 (Foundation) ✅
  ↓
M2 (Transactions) ✅
  ↓
M3 (Primitives) ✅
  ↓
M4 (Performance) ✅
  ↓
M5 (JSON Primitive) ✅
  ↓
M6 (Retrieval Surfaces) ✅
  ↓
M7 (Durability, Snapshots, Replay) ✅
  ↓
M8 (Vector Primitive) ← Current
  ↓
M9 (Performance & Indexing)
  ↓
M10 (Python Client)
  ↓
M11 (Security & Multi-Tenancy)
  ↓
M12 (Production Readiness)
```

**Notes**:
- M4 introduced durability *modes* (InMemory/Buffered/Strict). M7 adds durability *infrastructure* (snapshots, replay, WAL rotation).
- M5 locked in JSON mutation semantics. JSON optimization (structural storage, per-node versioning) is post-MVP.
- M6 adds retrieval surface that M8 (Vector Primitive) will plug into for hybrid search.
- M7 consolidates all durability concerns: snapshots, replay, storage stabilization.
- M8 Vector is a composite primitive on KV - enables semantic search alongside keyword search.
- M9 optimizes based on real workloads from M7/M8. HNSW refinement if needed.
- M10 Python client is the MVP client library - TypeScript/Go are post-MVP.

---

## Risk Mitigation

### High-Risk Areas
1. **Concurrency (M2)**: OCC bugs are subtle ✅ Mitigated
   - Mitigation: Extensive multi-threaded tests completed
2. **Recovery & Replay (M7)**: Data loss is unacceptable, determinism is hard ✅ Mitigated
   - Mitigation: 182 comprehensive tests covering recovery invariants (R1-R6), replay invariants (P1-P6), crash scenarios, and cross-primitive atomicity
3. **Layer boundaries (M3)**: Primitives leaking into each other ✅ Mitigated
   - Mitigation: Mock tests, strict dependency rules enforced
4. **Performance unbounded (M4)**: Optimization work can expand infinitely ✅ Mitigated
   - Mitigation: Red flag thresholds defined hard stops; M4 completed within scope

### Medium-Risk Areas
1. **Performance targets (M4)**: May not hit 250K ops/sec ✅ Mitigated
   - Mitigation: DashMap + HashMap architecture delivered; benchmarks validated
2. **JSON semantic complexity (M5)**: Mutation semantics can drift ✅ Mitigated
   - Mitigation: Six architectural rules enforced; semantics frozen before optimization
3. **Retrieval scope creep (M6)**: Risk of building full search engine ✅ Mitigated
   - Mitigation: Six architectural rules; M6 validates surface only, not relevance
4. **Vector complexity (M8)**: HNSW can be complex
   - Mitigation: Start with brute-force, add HNSW when needed; defer refinement to M9
5. **Security scope creep (M11)**: Security features can expand infinitely
   - Mitigation: Scope to essential production needs; iterate post-MVP

### Low-Risk Areas
1. **Foundation (M1)**: Well-understood patterns ✅ Complete
2. **API design (M3)**: Can iterate post-MVP ✅ Complete
3. **JSON API (M5)**: Follows established primitive patterns ✅ Complete
4. **Python client (M10)**: Well-understood; main risk is API bike-shedding
5. **Production readiness (M12)**: Standard practices; just needs execution

---

## Performance Targets Summary

| Mode | Latency Target | Throughput Target |
|------|----------------|-------------------|
| **InMemory** | <8µs put, <5µs get | 250K ops/sec |
| **Buffered** | <30µs put, <5µs get | 50K ops/sec |
| **Strict** | ~2ms put, <5µs get | ~500 ops/sec |

**Comparison**:
- Redis over TCP: ~100K-200K ops/sec
- Redis internal loop: Millions ops/sec
- M4 target: 250K ops/sec (removes blockers)
- M14 target: Millions ops/sec (Redis parity)

---

## Document History

| Version | Date | Changes |
|---------|------|---------|
| 1.0 | Initial | Original 5-milestone plan |
| 2.0 | 2026-01-15 | Inserted M4 Performance; MVP now 6 milestones |
| 3.0 | 2026-01-16 | M4 complete; M5 JSON Primitive complete; MVP now 7 milestones (M1-M7) |
| 4.0 | 2026-01-16 | Inserted M6 Retrieval Surfaces; Durability→M7, Replay→M8; MVP now 8 milestones (M1-M8) |
| 5.0 | 2026-01-17 | M6 Retrieval Surfaces complete; 125 tests passing (6 stress tests ignored) |
| 6.0 | 2026-01-17 | Major roadmap restructure: M7 consolidates durability+snapshots+replay; M8=Vector; M9=Performance; M10=Python; M11=Security; M12=Production. MVP now 12 milestones (M1-M12). Post-MVP becomes enhancements. |
| 7.0 | 2026-01-17 | M7 Durability complete; 182 comprehensive tests passing; snapshot system, crash recovery, deterministic replay, run lifecycle, storage stabilization all complete. |
