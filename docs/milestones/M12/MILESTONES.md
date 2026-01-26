# Strata: Project Milestones

## Core Identity

**Strata is an embedded library.** The server is a deployment mode, not the product.

Like SQLite, the canonical form is a library linked into your application. The server exists for cases where multi-process access or language-agnostic clients are needed, but it adds no new semantics—it is a thin adapter over the core API.

This matters because:
- Agents want microsecond tool calls, not network round-trips
- Embedded means zero operational overhead
- Local-first enables offline execution and deterministic replay
- The library is the forcing function for correctness; you cannot hide complexity behind the network

If Strata becomes successful, people will build servers on top of it. But the value is being **the SQLite of agent memory**, not another data service.

---

## MVP Target: Single-Node, Embedded Library with Core Primitives + Server Access Mode

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

## Milestone 8: Vector Primitive ✅
**Goal**: Native vector primitive for semantic search and AI agent memory

**Deliverable**: VectorStore primitive with similarity search, integrated into transaction system and M6 retrieval surface

**Status**: Complete

**Philosophy**: Vector is not a standalone database feature. It's a **composite primitive** built on KV, enabling semantic search alongside keyword search. KV + JSON + Vector covers 99% of AI agent database needs.

**Success Criteria**:

### Gate 1: Core Semantics
- [x] VectorStore::insert(key, embedding, metadata) works
- [x] VectorStore::search(query_vector, k) returns top-k results
- [x] VectorStore::delete(key) works
- [x] VectorStore::get(key) retrieves embedding + metadata

### Gate 2: Similarity Search
- [x] Cosine similarity scoring
- [x] Metadata filtering (pre-filter or post-filter)
- [x] Configurable distance metrics (cosine, euclidean, dot product)

### Gate 3: Index Support
- [x] Brute-force search for small datasets
- [x] HNSW index for larger datasets (optional, can be deferred to M11)
- [x] Index persistence and recovery

### Gate 4: M6 Integration
- [x] `vector.search(&SearchRequest)` returns `SearchResponse`
- [x] Hybrid search: keyword (BM25) + semantic (vector) fusion
- [x] RRF fusion works across keyword and vector results

### Gate 5: Transaction Integration
- [x] Vector operations participate in transactions
- [x] Snapshot-consistent vector search
- [x] Cross-primitive atomicity (KV + Vector in same transaction)

**Risk**: HNSW complexity. Start with brute-force, add HNSW when needed. ✅ Mitigated

**Architecture Doc**: [M8_ARCHITECTURE.md](../architecture/M8_ARCHITECTURE.md)

---

## Milestone 9: API Stabilization & Universal Protocol ✅
**Goal**: Stabilize the external API before server implementation and client development

**Deliverable**: Frozen primitive contract, stable API shape, and documented product surfaces

**Status**: Complete

**Philosophy**: M9 answers the question: "What is the universal way a user interacts with anything in Strata?" Before building the server (M10) or clients (M12), the interface must be stable. This milestone separates invariants from conveniences and substrate from product.

**Success Criteria**:

### Gate 1: Primitive Contract (Constitutional)
- [x] Seven invariants documented and enforced:
  1. Everything is Addressable
  2. Everything is Versioned
  3. Everything is Transactional
  4. Everything Has a Lifecycle
  5. Everything Exists Within a Run
  6. Everything is Introspectable
  7. Reads and Writes Have Consistent Semantics
- [x] All 7 primitives conform to all 7 invariants
- [x] Conformance tests for each invariant

### Gate 2: Core API Shape
- [x] `EntityRef` type for universal addressing
- [x] `Versioned<T>` wrapper for all read operations
- [x] Unified `Transaction` trait with methods for all primitives
- [x] `RunHandle` pattern for scoped access
- [x] Consistent error types across primitives

### Gate 3: API Consistency Audit
- [x] All reads return `Versioned<T>`
- [x] All writes return version information
- [x] All primitives accessible through same patterns
- [x] No primitive-specific "special cases" in core API

### Gate 4: Documentation
- [x] PRIMITIVE_CONTRACT.md finalized (invariants)
- [x] CORE_API_SHAPE.md finalized (API patterns)
- [x] PRODUCT_SURFACES.md documented (features built on core)
- [x] Migration guide from current API

### Gate 5: Validation
- [x] Example code works with new API
- [x] Existing tests updated to use `Versioned<T>` returns
- [x] API review completed

**Risk**: Over-specification. Keep invariants minimal; leave room for API evolution. ✅ Mitigated

**Architecture Docs**:
- [PRIMITIVE_CONTRACT.md](../architecture/PRIMITIVE_CONTRACT.md) - The invariants
- [CORE_API_SHAPE.md](../architecture/CORE_API_SHAPE.md) - The API patterns
- [PRODUCT_SURFACES.md](../architecture/PRODUCT_SURFACES.md) - Features on top of core

---

## Milestone 10: Storage Backend, Retention & Compaction ✅
**Goal**: Make Strata durable and portable without changing substrate semantics

**Deliverable**: Disk-backed storage backend with WAL + snapshots, user-configurable retention policies, and deterministic compaction

**Status**: Complete

**Philosophy**: M10 delivers production-ready persistence. Storage is authoritative, memory is a cache. Database growth beyond RAM is supported. Portable artifacts enable offline transfer (SQLite-like portability by copy). A codec seam enables future encryption-at-rest without redesign.

**Success Criteria**:

### Gate 1: Storage Artifact Format
- [x] Portable directory structure (`strata.db/` with MANIFEST, WAL/, SNAPSHOTS/, DATA/)
- [x] MANIFEST with format version, database UUID, WAL segment id, snapshot watermark
- [x] Portability: copying `strata.db/` produces a valid clone when closed cleanly
- [x] `checkpoint()` creates stable boundary for safe copying

### Gate 2: WAL Contract
- [x] WAL is append-only and segmented (`wal-N.seg`)
- [x] WAL record: format_version, txn_id, run_id, commit_timestamp, writeset, checksum
- [x] Writeset representation: Put, Delete, Append mutations with EntityRef
- [x] WAL replay is deterministic and idempotent
- [x] Durability modes (InMemory, Buffered, Strict) fully integrated

### Gate 3: Snapshot & Checkpoint
- [x] Snapshot materializes state at watermark W (all txn_id <= W included)
- [x] `checkpoint()` → CheckpointInfo { watermark_txn, snapshot_id, timestamp }
- [x] Crash-safe snapshot creation (temp write, fsync, atomic rename, manifest update)
- [x] Recovery: load snapshot + replay WAL entries > watermark

### Gate 4: Retention Policy
- [x] Global defaults + per-run + per-primitive overrides
- [x] Policy types: KeepAll, KeepLast(N), KeepFor(Duration)
- [x] Safety: version ordering preserved, deleted versions explicitly unavailable
- [x] Observability: VersionNotFound/HistoryTrimmed errors with metadata

### Gate 5: Compaction
- [x] `compact(mode)` → CompactInfo { reclaimed_bytes, wal_segments_removed, versions_removed }
- [x] Modes: WALOnly (remove WAL <= snapshot watermark), Full (retention + WAL cleanup)
- [x] Compaction is logically invisible (retained reads unchanged)
- [x] Tombstones handled correctly

### Gate 6: Public API
- [x] `open(path, options)` with durability and retention defaults
- [x] `close()` with clean shutdown
- [x] `export(path)` / `import(path)` for offline portability (or documented copy behavior)

### Gate 7: Testing & Validation
- [x] Crash recovery tests (commit, crash, reopen, verify)
- [x] Checkpoint correctness (checkpoint at W, replay reproduces exact state)
- [x] Retention enforcement (apply policy, compact, verify errors for trimmed versions)
- [x] Compaction invariance (before/after reads match)
- [x] Portability (checkpoint, copy, open copy, verify identical behavior)

**Risk**: Data loss bugs. Must test recovery thoroughly. Retention semantics must be precise. ✅ Mitigated

**Non-Goals for M10**:
- Encryption implementation (codec seam only)
- Background compaction tuning / adaptive heuristics
- Incremental snapshots
- Online defragmentation
- Multi-node replication
- Sharding across processes
- Tiered storage (S3, object stores)

---

## Milestone 11a: Core Contract & API
**Goal**: Freeze the core contract (value model, wire encoding, error model) and implement the two-layer API (Facade + Substrate)

**Deliverable**: Frozen core contract with working Facade and Substrate APIs, validated by core contract tests

**Status**: In Progress

**Work Completed**:
- Comprehensive API audit comparing against best-in-class databases (Redis, DynamoDB, MongoDB, Pinecone)
- 47 issues identified across naming, patterns, and missing features
- Capabilities audit documenting all user-facing operations
- Unified API design proposal created
- API encapsulation strategy defined

**Architecture Docs Created**:
- `UNIFIED_API_DESIGN.md` - Target API surface
- `API_AUDIT_REPORT.md` - Gap analysis and issues
- `CAPABILITIES_AUDIT.md` - All user capabilities
- `API_ENCAPSULATION.md` - Visibility strategy
- `EXPOSED_API_SURFACE.md` - Current API analysis

**Philosophy**: M11a establishes the **foundation contract** that cannot change. Value model, wire encoding, error codes, and the API layers (Facade + Substrate) are frozen here. This must be rock-solid before building consumer surfaces (CLI, SDK) on top.

**Key Architectural Invariant**: Every facade call **desugars to exactly one substrate call**. No magic, no hidden semantics.

**Success Criteria**:

### Gate 1: Value Model Frozen
- [ ] Canonical types: Null, Bool, Int(i64), Float(f64), String, Bytes, Array, Object
- [ ] Float edge cases: NaN, ±Inf, -0.0 handled with `$f64` wire wrapper
- [ ] Bytes encoding: `$bytes` base64 wrapper
- [ ] Size limits enforced: max_key_bytes, max_string_bytes, max_nesting_depth, etc.
- [ ] Value comparison: structural equality only, no ordering
- [ ] No implicit type coercions (`Int(1) != Float(1.0)`)

### Gate 2: Wire Encoding Frozen
- [ ] JSON wire encoding mandatory
- [ ] Request/response envelope structure frozen
- [ ] Operation names frozen (e.g., `kv.set`, `json.get`, `substrate.kv.put`)
- [ ] Special encodings: `$bytes`, `$f64`, `$absent`
- [ ] Versioned value encoding frozen
- [ ] Round-trip preserves exact types

### Gate 3: Error Model Frozen
- [ ] Canonical error codes: NotFound, WrongType, InvalidKey, InvalidPath, HistoryTrimmed, ConstraintViolation, Conflict, etc.
- [ ] Structured error payloads with `code`, `message`, `details`
- [ ] `ConstraintViolation` reason codes defined

### Gate 4: Two-Layer API Model
- [ ] Facade API implemented (Redis-like: `set`, `get`, `delete`, `exists`, `mget`, `mset`, `incr`, `json_*`, `xadd`, `xrange`, `vset`, `vget`, `vdel`, `cas_set`, `cas_get`)
- [ ] Substrate API implemented (explicit run/version/primitive: `kv_put`, `kv_get`, `json_set`, etc.)
- [ ] Facade→Substrate desugaring documented and tested
- [ ] Escape hatches: `getv()` for versioned reads, `use_run()` for run scoping
- [ ] Default run named `"default"` (literal string, always exists)

### Gate 5: Versioned<T> Contract
- [ ] All reads can return `Versioned<T>` with version and timestamp
- [ ] Version tagged union: `txn`, `sequence`, `counter`
- [ ] Timestamps in microseconds since Unix epoch
- [ ] History access with pagination (`limit`, `before`)

### Gate 6: Core Validation
- [ ] Value round-trip tests (all 8 types)
- [ ] Wire encoding conformance tests
- [ ] Facade-Substrate parity tests
- [ ] Error model validation tests
- [ ] Determinism tests

**Risk**: Over-specification. Keep invariants minimal; leave room for API evolution.

**Contract Doc**: [M11_CONTRACT.md](M11/M11_CONTRACT.md)

---

## Milestone 11b: Consumer Surfaces
**Goal**: Build CLI, SDK foundation, and complete contract validation on top of frozen M11a contract

**Deliverable**: CLI with all facade operations, Rust SDK, Python/JS mapping definitions, comprehensive contract validation suite

**Status**: Not Started (blocked on M11a completion)

**Philosophy**: M11b builds **user-facing surfaces** on top of the frozen contract from M11a. CLI and SDK are thin layers that expose the Facade API. The validation suite ensures no regressions.

**Success Criteria**:

### Gate 1: CLI Contract
- [ ] Redis-like command interface (`strata set x 123`)
- [ ] All facade operations exposed (KV, JSON, Event, Vector, State, History)
- [ ] Output conventions frozen ((nil), (integer), JSON)
- [ ] Argument parsing rules frozen
- [ ] `--run` flag for run scoping
- [ ] CLI is facade-only

### Gate 2: SDK Foundation
- [ ] Rust SDK implemented with same API shape as embedded
- [ ] Python value mapping defined (None, bool, int, float, str, bytes, list, dict)
- [ ] JavaScript value mapping defined (null, boolean, number/BigInt, string, Uint8Array)
- [ ] SDK requirements documented (preserve widths, Bytes vs String, Versioned shape)

### Gate 3: Full Contract Validation
- [ ] CLI conformance tests (parsing, output, all commands)
- [ ] SDK conformance test harness
- [ ] End-to-end regression tests
- [ ] Golden file tests for contract stability
- [ ] Redis mental model validation: `set`/`get`/`delete` feel familiar

### Gate 4: Documentation
- [ ] CLI usage documentation
- [ ] SDK usage documentation
- [ ] Contract stability guarantees documented
- [ ] Capability discovery operation (`capabilities()`) documented

**Risk**: CLI/SDK API bike-shedding. Stick to contract defined in M11a.

**Non-Goals for M11a/M11b**:
- Diff semantics
- Search ranking algorithms
- TTL/EXPIRE semantics
- Consumer groups for events
- Vector search query DSL
- JSONPath filters/wildcards
- Python SDK implementation (M15)
- MessagePack wire (optional, not required)
- Run deletion/garbage collection

**Contract Doc**: [M11_CONTRACT.md](M11/M11_CONTRACT.md)

---

## Milestone 12: Unified API Surface
**Goal**: Transform Strata's API from a two-layer system (Facade + Substrate) into a single, unified, intuitive API surface

**Deliverable**: Clean public API via `strata` crate with single entry point, consistent naming, and progressive disclosure pattern

**Status**: Not Started (blocked on M11b completion)

**Philosophy**: M12 is the "API unification milestone". It consolidates the confusing Facade/Substrate split into a single clean API that matches best-in-class databases like Redis and DynamoDB. The API should feel familiar to developers coming from any background.

**Success Criteria**:

### Gate 1: Create `strata` Crate
- [ ] `strata` crate at workspace root as sole public entry point
- [ ] `use strata::prelude::*` provides all needed imports
- [ ] Internal crates (`strata-core`, `strata-api`, etc.) marked `publish = false`
- [ ] `#[doc(hidden)]` on internal-but-public items
- [ ] `cargo doc` shows only public API

### Gate 2: Unified Primitive Access
- [ ] `db.kv` for key-value operations
- [ ] `db.json` for JSON document operations
- [ ] `db.events` for event stream operations
- [ ] `db.state` for state cell operations
- [ ] `db.vectors` for vector similarity search
- [ ] `db.runs` for run lifecycle management

### Gate 3: Clean Method Naming
- [ ] No redundant prefixes (`set` not `kv_set`)
- [ ] No Store/Log suffixes in external names (KV not KVStore)
- [ ] Consistent verb patterns across primitives (set/get/delete)
- [ ] Progressive disclosure: simple → run-scoped → full control

### Gate 4: Missing Capabilities
- [ ] `json.get_at(version)` for versioned reads
- [ ] `state.get_at(version)` for versioned reads
- [ ] `storage_stats()` for storage usage information
- [ ] `runs.size(&run)` for run size estimation
- [ ] Checkpoint API design (named persistent snapshots)

### Gate 5: Visibility & Encapsulation
- [ ] `publish = false` on all internal crates
- [ ] Sealed traits for extension points
- [ ] Clear separation: public types vs internal implementation
- [ ] No internal errors leak to users

### Gate 6: Deprecation & Migration
- [ ] Old API marked `#[deprecated(since = "0.12.0")]`
- [ ] Old tests updated to new API
- [ ] Examples updated to new API

### Gate 7: Documentation & Validation
- [ ] README updated with new API
- [ ] Example files (quickstart, transactions, versioning, vectors)
- [ ] All doc examples compile
- [ ] 100% test coverage on new API

**Risk**: Breaking changes. Mitigated by deprecation path (old API works with warnings).

**Implementation Plan**: [M12_UNIFIED_API_PLAN.md](M12_UNIFIED_API_PLAN.md)

**Reference Documents**:
- [UNIFIED_API_DESIGN.md](../architecture/UNIFIED_API_DESIGN.md) - Target API surface
- [API_AUDIT_REPORT.md](../architecture/API_AUDIT_REPORT.md) - 47 issues to fix
- [CAPABILITIES_AUDIT.md](../architecture/CAPABILITIES_AUDIT.md) - All user capabilities
- [API_ENCAPSULATION.md](../architecture/API_ENCAPSULATION.md) - Visibility strategy

---

## Milestone 13: Command Execution Layer (strata-executor)
**Goal**: Create a standardized command execution layer between all external APIs and the core database engine

**Deliverable**: `strata-executor` crate with `Command` enum, `Output` enum, `Error` enum, and `Executor` struct

**Status**: Not Started (blocked on M12 completion)

**Philosophy**: The Command Execution Layer is the **semantic instruction set** of Strata. Every operation becomes a `Command` that is executed by a single `Executor` to produce a `Result`. This is NOT a wire protocol—it is a deterministic, in-process execution boundary that enforces all invariants exactly once. All API surfaces (Rust, Python, Node, CLI, MCP) compile user intent into Commands.

**Why This Matters**:
- **Single Source of Truth**: Invariants enforced once, not reimplemented per SDK
- **Determinism**: Same commands + same initial state = same results (enables replay)
- **Black-Box Testing**: Test the engine by feeding command sequences
- **Thin SDKs**: Python/Node/CLI become trivial wrappers that build Commands
- **RunBundle Integration**: Commands provide the semantic log for export/import

**Success Criteria**:

### Gate 1: Core Types
- [ ] `Command` enum with all 48+ variants (KV, JSON, Events, State, Vectors, Runs, Database)
- [ ] `Output` enum with all result variants (Value, Versioned, Values, Version, Bool, Int, etc.)
- [ ] `Error` enum with all error cases (KeyNotFound, WrongType, VersionConflict, etc.)
- [ ] All types implement `Serialize`/`Deserialize`
- [ ] Round-trip serialization tests pass

### Gate 2: Executor Implementation
- [ ] `Executor::new(engine)` creates executor wrapping database
- [ ] `Executor::execute(cmd)` dispatches to appropriate primitive
- [ ] All KV commands implemented (12 variants)
- [ ] All JSON commands implemented (6 variants)
- [ ] All Event commands implemented (7 variants)
- [ ] All State commands implemented (5 variants)
- [ ] All Vector commands implemented (7 variants)
- [ ] All Run commands implemented (7 variants)
- [ ] All Database commands implemented (4 variants)

### Gate 3: Error Mapping
- [ ] Internal errors converted to `Error` enum
- [ ] Error messages are user-friendly
- [ ] Error details preserved where useful
- [ ] No internal types leak through errors

### Gate 4: JSON Utilities
- [ ] `Value` → JSON encoding for CLI/MCP output
- [ ] Special wrappers (`$bytes`, `$f64`) for non-JSON types
- [ ] `Output` → JSON for human-readable output
- [ ] `Error` → JSON for error reporting

### Gate 5: Integration
- [ ] `strata-executor` added to workspace
- [ ] Executor can be used standalone
- [ ] Integration tests for full command flows
- [ ] Every command variant has execution tests

**Risk**: Large enum can become unwieldy. Mitigated by grouping by primitive and comprehensive docs.

**Non-Goals for M13**:
- Transaction batching (`execute_atomic`) - future enhancement
- Command middleware - future enhancement
- Async execution - future enhancement

**Architecture Doc**: [M13_EXECUTOR.md](M13_EXECUTOR.md)

---

## Milestone 14: Server & Network Protocol
**Goal**: Add server deployment mode for multi-process and multi-language access

**Deliverable**: `strata-server` binary that exposes the Command Execution Layer over the network, plus a Rust client library

**Status**: Not Started (blocked on M13 completion)

**Philosophy**: M14 adds a **deployment mode**, not a new product. Strata remains an embedded library; the server is a thin adapter for cases requiring multi-process sharing or language-agnostic clients. The server adds no new semantics—`Command → Executor → Output`. If the server is adding logic beyond transport, something is wrong. This is the SQLite/rqlite pattern, not the Redis pattern.

**Success Criteria**:

### Gate 1: Server Binary
- [ ] `strata-server` binary builds and runs
- [ ] Configurable listen address (default: `127.0.0.1:6380`)
- [ ] Configurable data directory
- [ ] Graceful shutdown (SIGTERM/SIGINT)
- [ ] Startup banner with version and config

### Gate 2: Network Protocol
- [ ] JSON serialization of Commands from M13
- [ ] Request/response framing with length prefix
- [ ] All Commands supported through network
- [ ] Error responses with structured error codes

### Gate 3: Connection Management
- [ ] Accept multiple concurrent connections
- [ ] Per-connection state (current run, etc.)
- [ ] Connection timeout handling
- [ ] Clean disconnect handling

### Gate 4: Rust Client Library
- [ ] `strata-client` crate with same API shape as embedded
- [ ] Connect to server via TCP
- [ ] All primitive operations work through client
- [ ] Connection pooling (basic)

### Gate 5: Validation
- [ ] Embedded and client APIs are interchangeable (same trait)
- [ ] All existing tests pass through client (not just embedded)
- [ ] Round-trip latency < 1ms for simple operations (localhost)
- [ ] Basic load test: 10K ops/sec through network

**Risk**: Protocol design can over-engineer. Start minimal (no auth, no TLS, no multiplexing). Add features in later milestones.

**Non-Goals for M14**:
- Authentication/authorization (M17)
- TLS encryption (M17)
- Connection multiplexing
- Clustering/replication
- Admin commands beyond basic health

---

## Milestone 15: Performance & Indexing
**Goal**: Optimize hot paths and add secondary indexing capabilities

**Deliverable**: Faster queries, better scaling, and indexing infrastructure for all primitives

**Status**: Not Started

**Philosophy**: M15 is the "make it fast" milestone. By now we have real workloads from M7/M8, a stable API from M9/M12, storage from M10, executor from M13, and a server from M14. Optimize based on data, not speculation. HNSW refinement belongs here if not completed in M8.

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

## Milestone 16: Python Client
**Goal**: First-class Python client for AI agent developers

**Deliverable**: Python SDK with ergonomic API, async support, and comprehensive documentation

**Status**: Not Started

**Philosophy**: Python dominates AI/ML tooling. A clean Python client unlocks the majority of agent developers. This is the MVP client library. Requires unified API from M12, executor from M13, and server from M14.

**Success Criteria**:

### Gate 1: Core Client
- [ ] Python package installable via pip
- [ ] Connection to strata-server
- [ ] All primitive operations exposed (KV, JSON, Event, State, Trace, Run, Vector)

### Gate 2: Ergonomic API
- [ ] Pythonic API design (context managers, iterators, type hints)
- [ ] Async support (asyncio)
- [ ] Error handling with clear exception hierarchy (per M12 unified API)

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

## Milestone 17: Security & Multi-Tenancy
**Goal**: Production security features and tenant isolation

**Deliverable**: Authentication, authorization, and multi-tenant support

**Status**: Not Started

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
- [ ] TLS encryption for network protocol
- [ ] Encryption at rest (optional)
- [ ] Audit logging
- [ ] Security review and penetration testing

**Risk**: Security features can expand infinitely. Scope to essential production needs.

---

## Milestone 18: Production Readiness
**Goal**: Operational excellence for production deployment

**Deliverable**: Observable, maintainable, deployable system

**Status**: Not Started

**Philosophy**: M18 is the capstone milestone. Everything needed to run in production with confidence: monitoring, deployment, documentation.

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

> **See also**: [MAGIC_APIS.md](../architecture/MAGIC_APIS.md) - The five APIs that make Strata unique

### The Five Magic APIs

These APIs transform Strata from "agent storage" into "a substrate for reasoning about agent behavior over time."

#### 1. replay(): Time Travel
- `replay(run_id)` → Reconstruct full world state
- `replay_until(run_id, timestamp)` → State at any point in time
- `replay_range(run_id, t1, t2)` → Event sequence between points
- Cross-primitive: KV, JSON, StateCell, Vector, Trace, Event all replayed
- **Makes Strata a debugger, simulator, and learning loop engine**

#### 2. diff(): Change Intelligence
- `diff_runs(run_a, run_b)` → Compare two runs
- `diff_states(view_a, view_b)` → Compare two snapshots
- `diff_range(run_id, t1, t2)` → Changes over time
- Cross-primitive: KV diffs, JSON path diffs, vector changes, state transitions
- **No major database has a native diff engine**

#### 3. branch(): Counterfactuals
- `branch_from(run_id, timestamp)` → Fork from any point in time
- `fork(run_id)` → Fork current state
- All primitives snapshotted into new branch (copy-on-write)
- **Turns Strata into a multiverse engine for what-if simulations**

#### 4. explain(): Causal Reasoning
- `explain(entity_ref)` → Why is this state what it is?
- `explain_transition(entity_ref, timestamp)` → Why did this change?
- Output: prior states, events, operations, trace steps, causal chain
- **System explainability, not LLM explainability**

#### 5. search(): Semantic Memory Over Time
- `search_states(query)` → Search historical states
- `search_events(query)` → Search event history
- `search_traces(query)` → Search execution traces
- `search_runs(query)` → Search across runs
- Combines: keyword, vector similarity, structural filters, temporal constraints
- **Time-aware semantic memory**

### JSON Optimization (Structural Storage)
- Per-node versioning / subtree MVCC
- Structural sharing for efficient snapshots
- Array insert/remove with stable identities

### MCP Integration
- MCP server implementation
- Tool definitions for agent access
- IDE integration demos

### Network Layer Enhancements
- gRPC server (alternative to binary protocol)
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
- Incremental snapshots
- Distributed mode (far future)

---

## MVP Definition

**MVP = Milestones 1-18 Complete**

At MVP completion, the system should:
1. Store agent state in 7 primitives (KV, Events, StateCell, Trace, RunIndex, JSON, **Vector**)
2. Support concurrent transactions with OCC
3. **Achieve Redis-competitive performance in InMemory mode (250K+ ops/sec)**
4. Persist data with WAL and snapshots
5. Survive crashes and recover correctly
6. Replay runs deterministically
7. **Disk-backed storage with retention policies and compaction**
8. **Portable database artifacts (SQLite-like copy portability)**
9. **Frozen public API contract with two-layer model (Facade + Substrate)**
10. **Unified API surface with single entry point and clean naming**
11. **Command Execution Layer as single source of truth for all operations**
12. **Run as standalone server with network protocol access**
13. Scale near-linearly for disjoint keys (multi-thread)
14. Have >90% test coverage
15. **JSON primitive with path-level mutations and region-based conflict detection**
16. **Retrieval surface with primitive-native search and composite hybrid search**
17. **Vector primitive with semantic search and hybrid retrieval (keyword + vector)**
18. **Stable, universal API with documented invariants and consistent patterns**
19. **Python client library for AI agent developers**
20. **Security: authentication, authorization, multi-tenancy**
21. **Production-ready: observability, deployment, documentation**

**Not in MVP**:
- JSON structural optimization (post-MVP enhancement)
- Redis internal loop parity (post-MVP enhancement)
- MCP server (post-MVP enhancement)
- Additional client libraries beyond Python (post-MVP enhancement)
- Encryption at rest (codec seam ready, implementation post-MVP)
- Incremental snapshots (post-MVP enhancement)
- Distributed mode (far future)

---

## Timeline

```
Completed:
- M1  (Foundation)         ✅
- M2  (Transactions)       ✅
- M3  (Primitives)         ✅
- M4  (Performance)        ✅
- M5  (JSON Primitive)     ✅
- M6  (Retrieval Surfaces) ✅
- M7  (Durability, Snapshots, Replay & Storage) ✅
- M8  (Vector Primitive)   ✅
- M9  (API Stabilization & Universal Protocol) ✅
- M10 (Storage Backend, Retention & Compaction) ✅

Current:
- M11a (Core Contract & API) ← YOU ARE HERE

Remaining:
- M11b (Consumer Surfaces: CLI, SDK)
- M12 (Unified API Surface)
- M13 (Command Execution Layer)
- M14 (Server & Network Protocol)
- M15 (Performance & Indexing)
- M16 (Python Client)
- M17 (Security & Multi-Tenancy)
- M18 (Production Readiness)
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
M8 (Vector Primitive) ✅
  ↓
M9 (API Stabilization) ✅
  ↓
M10 (Storage Backend, Retention & Compaction) ✅
  ↓
M11a (Core Contract & API) ← Current
  ↓
M11b (Consumer Surfaces: CLI, SDK)
  ↓
M12 (Unified API Surface)
  ↓
M13 (Command Execution Layer)
  ↓
M14 (Server & Network Protocol)
  ↓
M15 (Performance & Indexing)
  ↓
M16 (Python Client)
  ↓
M17 (Security & Multi-Tenancy)
  ↓
M18 (Production Readiness)
```

**Notes**:
- M4 introduced durability *modes* (InMemory/Buffered/Strict). M7 adds durability *infrastructure* (snapshots, replay, WAL rotation).
- M5 locked in JSON mutation semantics. JSON optimization (structural storage, per-node versioning) is post-MVP.
- M6 adds retrieval surface that M8 (Vector Primitive) will plug into for hybrid search.
- M7 consolidates all durability concerns: snapshots, replay, storage stabilization.
- M8 Vector is a composite primitive on KV - enables semantic search alongside keyword search.
- **M9 stabilizes the internal API. Answers: "What is the universal way to interact with Strata?"**
- **M10 adds production storage: disk-backed backend, retention policies, compaction, portable artifacts.**
- **M11a freezes the core contract: value model, wire encoding, error model, and two-layer API (Facade + Substrate).**
- **M11b builds consumer surfaces: CLI, Rust SDK, Python/JS mappings, and complete validation suite.**
- **M12 unifies the API: transforms Facade + Substrate into single clean `strata` crate with progressive disclosure.**
- **M13 creates the Command Execution Layer: single source of truth for all operations, enables thin SDKs and deterministic replay.**
- **M14 makes Strata a server. External clients can connect over the network using Commands.**
- M15 optimizes based on real workloads with stable API, storage, executor, and server. HNSW refinement if needed.
- M16 Python client connects to strata-server - requires M12 unified API, M13 executor, and M14 server.

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
4. **Vector complexity (M8)**: HNSW can be complex ✅ Mitigated
   - Mitigation: Start with brute-force, add HNSW when needed; defer refinement to M15
5. **API over-specification (M9)**: Risk of freezing too much too early ✅ Mitigated
   - Mitigation: Separate invariants (constitutional) from API shape (stable) from product surfaces (evolving)
6. **Storage data loss (M10)**: Retention and compaction bugs can lose data ✅ Mitigated
   - Mitigation: Thorough crash recovery tests, retention enforcement tests, compaction invariance tests
7. **Contract over-specification (M11)**: Risk of freezing too much too early
   - Mitigation: Explicit "What M11 Does NOT Freeze" section; focus on essential semantics
8. **API unification (M12)**: Breaking changes can disrupt users
   - Mitigation: Deprecation path (old API works with warnings); comprehensive migration guide
9. **Executor scope creep (M13)**: Command enum can grow unbounded
   - Mitigation: Group by primitive; comprehensive docs; start with core operations only
10. **Protocol over-engineering (M14)**: Network protocol can grow unbounded
   - Mitigation: Start minimal (no auth, no TLS); add features in M17
11. **Security scope creep (M17)**: Security features can expand infinitely
   - Mitigation: Scope to essential production needs; iterate post-MVP

### Low-Risk Areas
1. **Foundation (M1)**: Well-understood patterns ✅ Complete
2. **API design (M3)**: Can iterate post-MVP ✅ Complete
3. **JSON API (M5)**: Follows established primitive patterns ✅ Complete
4. **Public contract (M11)**: Builds on M9 stabilization; main risk is scope creep
5. **API unification (M12)**: Clear design with deprecation path
6. **Executor (M13)**: Well-defined scope (48 commands); main risk is scope creep
7. **Server implementation (M14)**: Well-understood; main risk is scope creep
8. **Python client (M16)**: Well-understood; main risk is API bike-shedding; mitigated by unified API from M12
9. **Production readiness (M18)**: Standard practices; just needs execution

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
- Post-MVP target: Millions ops/sec (Redis parity)

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
| 8.0 | 2026-01-19 | Inserted M9 (API Stabilization & Universal Protocol); renumbered M9→M10, M10→M11, M11→M12, M12→M13. MVP now 13 milestones (M1-M13). |
| 9.0 | 2026-01-19 | Inserted M10 (Server & Wire Protocol); renumbered M10→M11, M11→M12, M12→M13, M13→M14. MVP now 14 milestones (M1-M14). |
| 10.0 | 2026-01-20 | M8 Vector Primitive complete; comprehensive test suite with 14 tiers covering core semantics, similarity search, index support, M6 integration, transaction integration, crash recovery, determinism, and stress tests. |
| 11.0 | 2026-01-20 | M9 API Stabilization complete; `Versioned<T>` wrapper for all read operations, unified error types, 2,105+ tests passing. All primitives conform to 7 invariants. |
| 12.0 | 2026-01-20 | Inserted M10 (Storage Backend, Retention & Compaction); renumbered M10→M11, M11→M12, M12→M13, M13→M14, M14→M15. MVP now 15 milestones (M1-M15). |
| 13.0 | 2026-01-21 | M10 Storage Backend complete. Inserted M11 (Public API & SDK Contract) based on M11_CONTRACT.md; renumbered M11→M12, M12→M13, M13→M14, M14→M15, M15→M16. MVP now 16 milestones (M1-M16). |
| 14.0 | 2026-01-21 | Split M11 into M11a (Core Contract & API) and M11b (Consumer Surfaces). M11a: Value Model, Wire Encoding, Error Model, Facade API, Substrate API, Core Validation. M11b: CLI, SDK Foundation, Full Validation. |
| 15.0 | 2026-01-24 | Inserted M12 (Unified API Surface) based on comprehensive API audit. Created architecture docs: UNIFIED_API_DESIGN.md, API_AUDIT_REPORT.md, CAPABILITIES_AUDIT.md, API_ENCAPSULATION.md. Renumbered M12→M13, M13→M14, M14→M15, M15→M16, M16→M17. MVP now 17 milestones (M1-M17). |
| 16.0 | 2026-01-25 | Inserted M13 (Command Execution Layer / strata-executor). Removed strata-wire crate. Renumbered M13→M14, M14→M15, M15→M16, M16→M17, M17→M18. MVP now 18 milestones (M1-M18). Created M13_EXECUTOR.md architecture doc. |
