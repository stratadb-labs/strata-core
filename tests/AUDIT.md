# Integration Test Audit Report

**Branch:** `cleanup/storage-crate-audit`
**Date:** 2026-01-30
**Scope:** All restored test files in `tests/` directory (~89 files across 8 subdirectories)

---

## Executive Summary

The restored `tests/` folder contains ~89 test files written against a pre-MVP API. The current codebase has been through aggressive simplification: ~60% of RunIndex methods removed, ~40% of EventLog methods removed, storage-layer format/compaction/retention modules moved to durability, search types relocated, and many Command enum variants deleted.

**The current 2,115 unit tests are implementation-focused** — they verify each method works in isolation. The restored integration tests add **architectural contract testing**: ACID guarantees across the full stack, crash recovery with corruption, OCC invariants like write-skew, cross-primitive atomicity, and concurrent correctness under contention. These are the tests that catch bugs that ship to production.

### Compilation Error Summary

| Test Suite | Errors | Root Cause |
|------------|--------|------------|
| intelligence | 489 | `strata_core::search_types` doesn't exist; missing `common` module |
| engine | 177 | References ~15 removed primitive methods |
| executor | 94 | References ~30 removed Command variants and Strata methods |
| integration | 52 | References removed methods via `common/` |
| storage | 34 | Imports modules moved to durability crate |
| concurrency | 22 | Minor import path issues |
| durability | 20 | `common/` module needs fixes |

---

## Tier 1: HIGH VALUE — Salvageable with Minor Fixes

These test **architectural invariants** that no unit test covers. They are close to compiling — errors are limited to import paths and 1-2 method signatures.

### Files

| File | Tests | What It Validates |
|------|-------|-------------------|
| `concurrency/transaction_states.rs` | 23 | Transaction FSM: valid/invalid state transitions (Active -> Validating -> Committed/Aborted) |
| `concurrency/occ_invariants.rs` | 11 | **First-committer-wins**, blind writes don't conflict, **write-skew is explicitly allowed**, read-only always commits |
| `concurrency/conflict_detection.rs` | 19 | Read-write conflict detection accuracy, CAS conflict detection, large read-set validation |
| `concurrency/cas_operations.rs` | 15 | CAS semantics: version matching, create-if-absent, stale detection, CAS not polluting read-set |
| `concurrency/version_counter.rs` | 15 | Monotonic versions, no gaps, concurrent uniqueness (8 threads x 1000 allocations) |
| `concurrency/transaction_lifecycle.rs` | 17 | Complete begin-commit/abort cycles, reset semantics, multi-key workflows |

**Total: 100 tests across 6 files**

### Why These Matter

Unit tests in `crates/concurrency/src/` test individual functions. These integration tests validate **combined behavior** — e.g., that a transaction with reads AND CAS operations validates both independently, or that write-skew is intentionally allowed (a design decision that could regress silently).

### Required Fixes

- Update import paths for `strata_concurrency::validation::` types
- Change 1 test that uses `ClonedSnapshotView::from_arc()` (now `pub(crate)`) to use `::new()` instead
- Verify `PendingOperations` struct field names (`.puts`, `.deletes`, `.cas`)
- Minor: `TransactionManager::with_txn_id()` constructor signature may have changed

### Imports Used (All Exist)

```
strata_concurrency::manager::TransactionManager
strata_concurrency::transaction::{TransactionContext, TransactionStatus, CASOperation}
strata_concurrency::validation::{validate_transaction, validate_read_set, validate_cas_set, ConflictType, ValidationResult}
strata_core::traits::Storage
strata_core::types::{Key, Namespace}
strata_core::value::Value
strata_storage::sharded::ShardedStore
```

---

## Tier 2: HIGH VALUE — Salvageable with Moderate Fixes

These test critical cross-layer contracts. They need `tests/common/mod.rs` fixed first, plus some removed method calls updated.

### Files

| File | Tests | What It Validates | Key Issues |
|------|-------|-------------------|------------|
| `concurrency/concurrent_transactions.rs` | 11 | Parallel commits, run isolation, high contention (8 threads on 1 key), version uniqueness | `validate_transaction` import path |
| `concurrency/snapshot_isolation.rs` | 17 | **Snapshot immutability**, repeatable reads, read-your-writes, concurrent snapshot reads (4 threads) | `from_arc()` is now private — use `new()` |
| `durability/recovery_invariants.rs` | 11 | **5 fundamental guarantees**: no data lost, no data invented, idempotent recovery, deterministic recovery, last-write-wins | `common/` fixes; VectorStore `insert()` signature |
| `durability/crash_recovery.rs` | 7 | Truncated WAL, corrupted WAL tail, completely corrupted WAL, missing WAL, rapid reopen cycles | `common/` fixes for file manipulation helpers |
| `durability/cross_primitive_recovery.rs` | 5 | **All 6 primitives recover atomically**, interleaved writes, multi-run independence | `common/` fixes |
| `durability/wal_lifecycle.rs` | 6 | WAL growth monotonicity, large value recovery, many small writes recovery | `common/` fixes |
| `durability/mode_equivalence.rs` | 8 | **Semantic equivalence across None/Batched/Strict modes** — prevents behavioral drift | `common/` fixes |
| `durability/snapshot_lifecycle.rs` | 6 | Snapshot+WAL interaction, corrupted snapshot fallback | `common/` fixes |
| `engine/acid_properties.rs` | 11 | **ACID properties**: atomicity (all-or-nothing), consistency (CAS prevents invalid state), isolation (concurrent counters), durability (survives restart) | Uses `KVStoreExt`, `EventLogExt`, `StateCellExt` — all exist |
| `engine/cross_primitive.rs` | ~5 | Cross-primitive transaction atomicity | `common/` fixes |

**Total: ~87 tests across 10 files**

### Why These Matter

`acid_properties.rs` is the single most important test file in the codebase. It validates that the transaction system actually provides ACID guarantees across the full stack (engine -> concurrency -> storage -> durability). No unit test does this.

The recovery invariant tests catch bugs like "recovery invents phantom keys" or "recovery is non-deterministic" — bugs that only manifest after crash+restart.

### Required Fixes

1. **Fix `tests/common/mod.rs` first** — everything depends on this
   - Remove `CapturedVectorState` VectorStore iterator usage (VectorStore is not an iterator)
   - Update vector helper signatures to match current `VectorStore::insert()` API
   - Remove/update search helper imports (`strata_intelligence::SearchRequest` etc.)
   - Verify `TestDb` wrapper methods match current `Database`/`DatabaseBuilder` API
   - Remove references to `DatabaseBuilder::open_temp()` (doesn't exist — use `Database::cache()`)
2. Fix `acid_properties.rs`:
   - `Database::shutdown()` is `pub(crate)` — either make public or use `drop()`
   - `KVStoreExt`, `EventLogExt`, `StateCellExt` all exist and match

### Blocking Dependency

```
tests/common/mod.rs  <-- MUST fix first
    |
    +-- durability/recovery_invariants.rs
    +-- durability/crash_recovery.rs
    +-- durability/cross_primitive_recovery.rs
    +-- durability/wal_lifecycle.rs
    +-- durability/mode_equivalence.rs
    +-- durability/snapshot_lifecycle.rs
    +-- engine/acid_properties.rs
    +-- engine/cross_primitive.rs
```

---

## Tier 3: MODERATE VALUE — Salvageable with Significant Adaptation

The test **intent** is valid, but 30-50% of individual test functions reference removed API surface. Strategy: delete tests for removed methods, keep tests for surviving methods.

### Files

| File | What's Valid | What's Broken |
|------|-------------|---------------|
| `integration/primitives.rs` | CRUD for all 6 primitives, cross-primitive isolation, agent memory workflow | References `len_by_type()`, `search()` on KVStore |
| `integration/branching.rs` | Run isolation (same key in different runs), concurrent runs (100 parallel) | References `create_run_with_options()`, `add_tags()`, `fork_run()` — all removed |
| `engine/primitives/kv.rs` | Basic CRUD, list with prefix, value types | May reference `exists()`, `get_many()`, `list_with_values()`, `search()` |
| `engine/primitives/eventlog.rs` | append, read, read_by_type, len | References `head()`, `is_empty()`, `len_by_type()`, `event_types()`, `read_range()`, `append_batch()`, `verify_chain()` |
| `engine/primitives/statecell.rs` | init, read, cas, set | References `exists()`, `list()`, `delete()`, `transition()`, `transition_or_init()` |
| `engine/primitives/jsonstore.rs` | create, get, set, delete, list | References `cas()`, `merge()`, `increment()`, `array_push()`, `array_pop()`, iterator |
| `engine/primitives/vectorstore.rs` | create_collection, insert, search, delete | References `search_simple()`, `collection_exists()` (private), iterator |
| `executor/strata_api.rs` | KV/Event/State/JSON/Vector basic ops | `Strata::new()` doesn't exist; missing: `kv_exists`, `kv_incr`, `kv_keys`, `event_range`, `state_exists`, `state_delete`, `run_complete`, `run_fail`, tag methods |

### Current vs Removed API Surface

#### KVStore
| Method | Status |
|--------|--------|
| `get()`, `put()`, `delete()`, `list()` | EXISTS |
| `exists()`, `get_many()`, `list_with_values()`, `search()` | REMOVED |

#### EventLog
| Method | Status |
|--------|--------|
| `append()`, `read()`, `len()`, `read_by_type()` | EXISTS |
| `head()`, `is_empty()`, `len_by_type()`, `event_types()`, `read_range()`, `append_batch()`, `verify_chain()` | REMOVED |

#### StateCell
| Method | Status |
|--------|--------|
| `init()`, `read()`, `cas()`, `set()` | EXISTS |
| `exists()`, `list()`, `delete()`, `transition()`, `transition_or_init()` | REMOVED |

#### JsonStore
| Method | Status |
|--------|--------|
| `create()`, `get()`, `exists()`, `set()`, `delete_at_path()`, `destroy()`, `list()` | EXISTS |
| `cas()`, `merge()`, `increment()`, `array_push()`, `array_pop()`, iterator | REMOVED |

#### VectorStore
| Method | Status |
|--------|--------|
| `create_collection()`, `insert()`, `get()`, `delete()`, `search()`, `list_collections()`, `delete_collection()` | EXISTS |
| `search_simple()`, `collection_exists()` (private), `get_collection()` (private), iterator | REMOVED |

#### RunIndex
| Method | Status |
|--------|--------|
| `create_run()`, `get_run()`, `exists()`, `list_runs()`, `delete_run()` | EXISTS |
| `create_run_with_options()`, `add_tags()`, `remove_tags()`, `complete_run()`, `fail_run()`, `pause_run()`, `resume_run()`, `cancel_run()`, `archive_run()`, `update_status()`, `update_metadata()`, `query_by_status()`, `query_by_tag()`, `search()`, `fork_run()` | REMOVED |

#### Command Enum (Executor)
| Category | Exists | Removed |
|----------|--------|---------|
| KV | KvPut, KvGet, KvDelete, KvList | KvExists, KvIncr, KvKeys, KvMget, KvMput, KvGetAt |
| Event | EventAppend, EventRead, EventReadByType, EventLen | EventRange |
| State | StateSet, StateRead, StateCas, StateInit | StateDelete, StateExists |
| Run | RunCreate, RunGet, RunList, RunExists, RunDelete | RunComplete, RunFail, RunAddTags, RunArchive, RunCancel, RunPause, RunResume, RunSearch, RunQueryByStatus, RunQueryByTag, RunUpdateMetadata, RunCreateChild, RunGetChildren, RunGetParent, RunCount, RunGetRetention, RunSetRetention, RunRemoveTags |
| Vector | All 7 variants exist | VectorCollectionExists |

---

## Tier 4: ARCHITECTURALLY VALUABLE — Needs Full Rewrite

The **concepts** are essential, but the code is too far from current API to patch.

### `tests/intelligence/` (32 files, 489 errors)

Every file references `strata_core::search_types` which doesn't exist — search types are now in `strata_engine::search`. Missing `tests/intelligence/common.rs` module entirely. Helper functions (`populate_test_data`, `verify_deterministic`, etc.) undefined in scope.

**What's architecturally valuable:**
- Budget enforcement (search doesn't exceed allocated budget)
- Score normalization consistency
- RRF fusion correctness
- Hybrid search orchestration (keyword + vector combined)
- Deterministic ordering (same query -> same order)
- Snapshot consistency during search (reads don't see concurrent writes)
- Regression tests (e.g., `issue_018_search_overfetch`)

**Module list:**
```
architectural_invariants    budget_semantics         explainability
fusion                      hybrid                   identity
indexing                    issue_018_search_overfetch
m6_budget_propagation       m6_hybrid_search         m6_rrf_fusion
m6_search_request           m6_search_response       scoring
search_all_primitives       search_backend_tiebreak
search_budget_enforcement   search_budget_enforcement_cross
search_correctness          search_deterministic_order
search_dimension_match      search_facade_tiebreak
search_hybrid_orchestration search_no_normalization
search_readonly             search_score_normalization
search_single_threaded      search_snapshot_consistency
stress
```

### `tests/engine/primitives/runindex.rs`

RunIndex was stripped from ~20 methods to 5. All lifecycle management (complete, fail, pause, resume, cancel, archive), tag management, status queries, and forking are gone. The remaining 5 methods (`create_run`, `get_run`, `exists`, `list_runs`, `delete_run`) are trivial enough that unit tests suffice.

**Note:** If you plan to re-add run lifecycle management, keep this file as a specification of what the API should look like.

### `tests/executor/command_dispatch.rs`

Tests 106 command variants; the current Command enum has ~40. The ~66 removed variants make this file more wrong than right.

---

## Tier 5: DEAD — Tests for Removed Functionality

| File | Why Dead |
|------|----------|
| `storage/compaction.rs` | Imports `strata_storage::compaction`, `format::wal_record` — moved to durability crate. Storage is now pure in-memory. |
| `storage/format_validation.rs` | Imports `strata_storage::codec`, `format::snapshot`, `disk_snapshot` — all moved to durability. |
| `storage/retention_policy.rs` | Imports `strata_storage::retention` — moved to durability. |
| `executor/serialization.rs` | Tests serialization of removed Command variants. |

**Note:** The compaction, format validation, and retention tests may have value if re-targeted at the **durability** crate, where these modules now live.

---

## Architectural Properties Only Integration Tests Cover

These are the properties the current 2,115 unit tests do NOT validate:

### 1. ACID Across the Full Stack
`acid_properties.rs` tests that a failed transaction in the engine layer doesn't leave partial writes in storage. No unit test crosses crate boundaries like this.

### 2. Crash Recovery Correctness
`crash_recovery.rs` physically truncates and corrupts WAL files on disk, then reopens the DB. Unit tests mock this.

### 3. Recovery Idempotency
`recovery_invariants.rs` verifies that opening the DB 3 times in a row produces identical state. Catches bugs where replay applies operations twice.

### 4. Recovery Determinism
Two separate DB instances with identical operations must recover to identical state. Catches ordering bugs in WAL replay.

### 5. OCC Write-Skew Allowance
`occ_invariants.rs` explicitly tests that write-skew IS allowed (two transactions read overlapping data, write disjoint keys, both commit). This is a **design decision** that could silently regress.

### 6. Cross-Primitive Atomicity
`cross_primitive_recovery.rs` verifies that KV + EventLog + StateCell + JsonStore + VectorStore all recover together or not at all after crash.

### 7. Mode Equivalence
`mode_equivalence.rs` ensures None, Batched, and Strict durability modes produce identical observable behavior. Without this, behavioral drift between modes is invisible.

### 8. High-Contention Correctness
`concurrent_transactions.rs` runs 8 threads competing on a single key and verifies the commit/conflict rates are correct. Unit tests use single-threaded mocks.

---

## Recommended Priority Order

### Phase 1: Fix `tests/common/mod.rs`
Everything depends on this. Remove references to removed methods, fix VectorStore iterator usage, update search helper imports.

### Phase 2: Fix Tier 1 (6 concurrency files)
These need the least work and validate the most critical invariants (OCC correctness, CAS semantics, transaction FSM). **~100 tests recovered.**

### Phase 3: Fix Tier 2 (10 files)
Especially `acid_properties.rs`, `recovery_invariants.rs`, and `crash_recovery.rs`. These prevent shipping data-loss bugs. **~87 tests recovered.**

### Phase 4: Triage Tier 3 (8 files)
Delete tests for removed methods, keep tests for surviving API. Worth doing because they exercise the full stack. **~50-80 tests recovered (after pruning).**

### Phase 5: Defer Tier 4 (intelligence)
Needs a full rewrite to target the current search API in `strata_engine::search`. High value but high effort. **~200+ tests when rewritten.**

### Phase 6: Delete Tier 5 (4 files)
Or move them to target the durability crate if you want format/compaction/retention integration tests.

---

## Stress Tests (All Tiers)

The following files contain `#[ignore]`-gated stress tests that are opt-in. They should be preserved alongside their parent tier:

| File | Tests | What It Stresses |
|------|-------|------------------|
| `concurrency/stress.rs` | 7 | 16 threads read-write, TPS measurement, 10K ops in single txn, 100 concurrent runs |
| `durability/stress.rs` | 9 | 10K key recovery, 8 threads x 1000 writes, 100K small writes, 1MB values, 20 reopen cycles |
| `engine/stress.rs` | ~5 | Engine-level load tests |
| `storage/stress.rs` | ~5 | High-volume storage operations |
| `intelligence/stress.rs` | ~5 | Search under load |

---

## Test Infrastructure (`tests/common/mod.rs`)

The common module is 1,300+ lines of test infrastructure. Key components:

### Database Wrappers
- `TestDb` — wraps Database with reopen support, primitive accessors, path helpers
- `AllPrimitives` — container for all 6 primitive instances
- `create_test_db()` / `create_persistent_db()` / `test_across_modes()`

### State Capture & Comparison
- `CapturedState` — captures all KV state with hash for comparison
- `CapturedVectorState` — captures vector collection state (BROKEN: uses VectorStore as iterator)
- `assert_states_equal()` / `assert_vector_states_equal()`

### File Manipulation (for crash tests)
- `corrupt_file_at_offset()` / `corrupt_file_random()` / `truncate_file()`
- `file_size()` / `create_partial_wal_entry()`
- `count_snapshots()` / `list_snapshots()` / `delete_snapshots()`

### Concurrency Helpers
- `run_concurrent(num_threads, fn)` — barrier-synchronized thread spawning
- `run_with_shared(num_threads, shared_state, fn)` — with shared Arc state

### Vector Helpers
- `config_small()` (3D cosine) / `config_standard()` (384D) / `config_euclidean()`
- `seeded_vector(dim, seed)` — deterministic random vectors
- `populate_vector_collection()` — bulk insert helper

### Search Helpers (BROKEN: wrong import paths)
- `assert_all_from_primitive()` / `verify_deterministic()`
- `verify_scores_decreasing()` / `verify_ranks_sequential()`
