# MVP Codebase Audit

Date: 2026-02-04
Status: **COMPLETE** — all 10 investigation areas audited

## Codebase Health Snapshot

| Metric | Status |
|--------|--------|
| Total LOC | 88,422 across 8 crates |
| Clippy warnings | 0 (enforced) |
| Formatting | Clean (`cargo fmt`) |
| Broken doc-tests | 0 |
| Unsafe blocks | 4 production |
| Test functions | 2,203+ |
| TODO/FIXME/HACK | 4 TODOs, 0 FIXME, 0 HACK |
| Version consistency | All crates at 0.5.1 |

---

## Investigation Areas

### 1. Panicking in Production Code — HIGH

33 `panic!()` + 16 `expect()` + 7 `unimplemented!()` in non-test code.
These are runtime bombs for an MVP database.

Key hotspots:

| File | Line(s) | Issue |
|------|---------|-------|
| `crates/core/src/contract/version.rs` | 147-155 | Version overflow panics instead of returning errors |
| `crates/engine/src/database/mod.rs` | 335 | `expect()` on thread spawn (can fail under resource pressure) |
| `crates/durability/src/format/writeset.rs` | 499-614 | 5 panics on mutation type matching (fragile enum handling) |
| `crates/executor/src/convert.rs` | 169-229 | 5 conversion panics that should be proper errors |
| `crates/executor/src/json.rs` | 223-300 | 4 JSON conversion panics |
| `crates/engine/src/transaction/context.rs` | 609-647 | 7 `unimplemented!()` stubs for Vector/Branch transaction ops |

**Goal**: Audit every `panic!`/`expect`/`unimplemented!` in non-test code. Convert
to `Result` propagation where appropriate. Document which ones are genuine invariant
violations that justify panicking.

---

### 2. Unsafe Code Audit — HIGH

Only 4 production unsafe blocks — small surface but needs verification.

| File | Line(s) | What |
|------|---------|------|
| `crates/core/src/primitives/json.rs` | 1028 | `unsafe { &*(current as *const ...) }` — raw pointer cast for JSON path traversal |
| `crates/core/src/primitives/json.rs` | 1088 | `unsafe { &mut *(current as *mut ...) }` — mutable cast for JSON path traversal |
| `crates/executor/src/executor.rs` | 712 | `unsafe impl Send for Executor {}` |
| `crates/executor/src/executor.rs` | 713 | `unsafe impl Sync for Executor {}` |

**Goal**: Add `// SAFETY:` comments per Rust convention. Verify soundness of each
block — particularly the pointer casts (aliasing rules) and the Send/Sync impls
(thread safety of Executor internals).

---

### 3. Large File Decomposition — MEDIUM

Files exceeding reasonable size for single-module comprehension:

| File | LOC | Issue |
|------|-----|-------|
| `storage/sharded.rs` | 3,312 | MVCC + version chains + tests all in one file |
| `core/primitives/json.rs` | 3,206 | Types + path traversal + patch ops + limits |
| `core/error.rs` | 2,545 | 20+ variants + constructors + classification + tests |
| `engine/primitives/vector/store.rs` | 2,395 | Vector store with multiple backends |
| `core/types.rs` | 1,762 | BranchId + Namespace + Key + TypeTag |
| `concurrency/transaction.rs` | 1,710 | Transaction state machine |

**Goal**: Evaluate whether splitting into sub-modules improves maintainability.
Priority candidate: `sharded.rs` — extract `VersionChain` into its own module,
separate tests into `tests/` sub-module.

---

### 4. Cloning Overhead in Hot Paths — MEDIUM

The storage layer performs extensive cloning:

- `key.clone()` ~50+ times in `sharded.rs` — `Key` contains `Vec<u8>` user_key
- `sv.versioned().clone()` ~30+ times — every scan clones full `VersionedValue`
- Every `scan_prefix()` and range query clones entire versioned values

`Key` structure (non-trivial to clone):
```rust
pub struct Key {
    pub namespace: Namespace,  // contains String fields
    pub type_tag: TypeTag,     // u8-backed enum, cheap
    pub user_key: Vec<u8>,     // heap allocation on clone
}
```

**Goal**: Profile whether `Key` should use `Arc<[u8]>` for user_key, and whether
scan operations should return references or `Cow<'_, VersionedValue>` for large
documents. Benchmark before and after.

---

### 5. Error Handling Consistency — MEDIUM

Each crate has its own error hierarchy:

| Crate | Error Types |
|-------|-------------|
| strata-core | `StrataError` (canonical, 10 wire codes) |
| strata-concurrency | `CommitError` |
| strata-durability | `BranchBundleError`, `DatabaseHandleError`, `SnapshotError`, `WalReaderError`, `CodecError`, `ManifestError` |
| strata-engine | Various per-primitive error types |
| strata-executor | `Error` (wraps engine errors) |

**Goal**: Verify all `From` impls are lossless (no information dropped during error
conversion). Ensure all error paths are tested. Confirm wire encoding covers all
variants that can reach the API boundary.

---

### 6. API Consistency Audit — MEDIUM

The primitives (KV, JSON, Event, State, Vector) follow uniform patterns. Verify:

- Do all primitives support space-scoped operations consistently?
- Are branch operations (fork/diff/merge) consistent across all primitive types?
- Do all versioned-read APIs return `VersionedHistory<T>` uniformly?
- Is the `Strata` public API surface minimal and well-documented for MVP?
- Are naming conventions consistent (`kv_get` vs `json_get` vs `event_read`)?

Cross-primitive pattern matrix to validate:

| Operation | KV | JSON | Event | State | Vector |
|-----------|----|----|-------|-------|--------|
| Versioned read | `kv_get` | `json_get` | `event_read` | `state_read` | `vector_get` |
| Write → Version | `kv_put` | `json_set` | `event_append` | `state_cas` | `vector_insert` |
| Exists check | `kv_exists` | `json_exists` | ? | ? | `vector_exists` |
| Delete → bool | `kv_delete` | `json_delete` | ? | ? | `vector_delete` |

**Goal**: Create the complete matrix, identify gaps, ensure consistency.

---

### 7. Concurrency Safety Review — MEDIUM

48 files use concurrent data structures:

| Pattern | Count | Primary Usage |
|---------|-------|---------------|
| `Arc` | ~80+ | Shared ownership across threads |
| `DashMap` | ~40+ | Lock-free concurrent HashMap (ShardedStore) |
| `RwLock` | ~20+ | Reader-writer synchronization |
| `Mutex` | ~15+ | parking_lot Mutex for critical sections |

Key review areas:

- **ShardedStore + DashMap**: Verify no iterator invalidation under concurrent modification
- **TransactionManager + RwLock**: Check for deadlock potential (lock ordering)
- **Executor Send/Sync**: Verify thread safety of internal state (relates to #2)
- **No channels/atomics**: Confirm synchronous design is intentional

**Goal**: Review lock ordering conventions. Verify no potential deadlocks. Confirm
DashMap usage patterns are safe under concurrent iteration + mutation.

---

### 8. Dependency Audit — LOW

27 workspace dependencies.

Checks to perform:

- [ ] `cargo audit` — any known CVEs in dependency tree?
- [ ] `cargo outdated` — any stale dependencies?
- [ ] `proptest` is listed as a dependency but 0 property-based tests found — use or remove
- [ ] Are optional deps (`redb`, `heed`, `rusqlite`, `usearch`) properly feature-gated?
- [ ] Is `anyhow` used anywhere? (It's a workspace dep but `thiserror` is the primary error crate)

**Goal**: Clean dependency tree. Remove unused deps. Update stale ones.

---

### 9. Documentation Completeness — LOW

6 of 8 crates enforce `#![warn(missing_docs)]`.

| Crate | Enforces missing_docs |
|-------|-----------------------|
| strata-core | Yes |
| strata-storage | Yes |
| strata-concurrency | Yes |
| strata-durability | Yes |
| strata-engine | Yes |
| strata-executor | Yes |
| strata-security | **No** |
| strata-intelligence | **No** |

Additional documentation concerns:

- 4 TODO comments reference unimplemented substrate integration (`engine/database/mod.rs:590,603`, `executor/executor.rs:127,131`)
- `Value::Bytes` ↔ JSON roundtrip is lossy (bytes become base64 strings) — is this documented at the API level?
- Two `DiffEntry` types exist (`recovery::DiffEntry` vs `branch_ops::DiffEntry`) — resolved by comments, could be clearer
- `TypeTag` lacks `From<u8>` / `Into<u8>` traits — uses manual `as_byte()` / `from_byte()` instead

**Goal**: Enforce `missing_docs` on all crates. Document known limitations.
Add API-level docs for lossy conversions.

---

### 10. Test Coverage Gaps — LOW

2,203+ tests exist across unit, integration, and doc-test categories.

| Category | Count |
|----------|-------|
| Unit test files | 133 |
| Integration test files | 21 |
| Doc-tests | ~150 |
| Property-based tests | 0 |
| Benchmark suites | 2 (criterion) |
| `#[ignore]` tests | 46 |

Gaps to investigate:

- `proptest` dependency exists but has 0 property-based tests — add for serialization roundtrips or remove dep
- Are error recovery paths tested (WAL corruption, disk full, partial writes)?
- Do the 7 `unimplemented!()` vector transaction stubs have corresponding "returns error" tests?
- Are the 46 `#[ignore]` tests documented with reasons for being ignored?
- Is there coverage for concurrent fork/diff/merge operations?

**Goal**: Identify untested critical paths. Add property-based tests for
Value/Key serialization roundtrips. Document reasons for ignored tests.

---

## Execution Order

| Phase | Areas | Rationale |
|-------|-------|-----------|
| **Phase 1** | #1 (panics), #2 (unsafe) | Correctness — these can crash production |
| **Phase 2** | #5 (errors), #6 (API consistency) | Completeness — MVP API surface must be solid |
| **Phase 3** | #7 (concurrency), #4 (cloning) | Performance — behavior under concurrent load |
| **Phase 4** | #3 (large files), #8 (deps), #9 (docs), #10 (tests) | Maintainability — long-term codebase health |

---

## Reference: Crate Architecture

```
strata-executor   (public API layer)
    └── strata-engine   (database orchestration, primitives)
            ├── strata-storage       (in-memory MVCC, ShardedStore)
            ├── strata-concurrency   (OCC transactions, snapshots)
            ├── strata-durability    (WAL, snapshots, recovery, bundles)
            └── strata-intelligence  (search, BM25, hybrid scoring)
    └── strata-security   (access control, read-only mode)
    └── strata-core       (foundational types, traits, errors)
```

## Reference: Code Size by Crate

| Crate | LOC | Share |
|-------|-----|-------|
| strata-engine | 23,878 | 27% |
| strata-durability | 17,071 | 19% |
| strata-core | 15,996 | 18% |
| strata-executor | 10,710 | 12% |
| strata-concurrency | 4,810 | 5% |
| strata-storage | 4,579 | 5% |
| strata-intelligence | 1,064 | 1% |
| strata-security | 47 | <1% |
| **Total** | **88,422** | |

---

## Audit Results Summary

All 10 investigation areas have been audited. Detailed findings are in separate documents.

### Findings Overview

| # | Area | Verdict | Key Finding | Report |
|---|------|---------|-------------|--------|
| 1 | Panics | **PASS** | 11 instances: 9 justified, 2 should convert to Result | [audit-phase1-panics.md](audit-phase1-panics.md) |
| 2 | Unsafe | **PASS** | 4 instances: all sound, all LOW risk | [audit-phase1-unsafe.md](audit-phase1-unsafe.md) |
| 3 | Large Files | **PASS** | 16 files >1K LOC; `database/mod.rs` needs split | [audit-phase4-maintenance.md](audit-phase4-maintenance.md) |
| 4 | Cloning | **PASS** | 397 clones; scan ops most impacted (5.5ms/10k results) | [audit-phase3-cloning.md](audit-phase3-cloning.md) |
| 5 | Errors | **CONDITIONAL** | 12+ durability errors lack From→StrataError conversion | [audit-phase2-errors.md](audit-phase2-errors.md) |
| 6 | API | **CONDITIONAL** | Naming inconsistency (`get` vs `read`), State return type mismatch | [audit-phase2-api.md](audit-phase2-api.md) |
| 7 | Concurrency | **PASS** | Strong design; DashMap + parking_lot; no deadlocks found | [audit-phase3-concurrency.md](audit-phase3-concurrency.md) |
| 8 | Dependencies | **PASS** | Remove unused `proptest` and `anyhow` | [audit-phase4-maintenance.md](audit-phase4-maintenance.md) |
| 9 | Documentation | **PASS** | 2 crates missing `missing_docs`; DiffEntry name clash | [audit-phase4-maintenance.md](audit-phase4-maintenance.md) |
| 10 | Tests | **PASS** | 2,203+ tests; 7 stubs lack panic tests; 0 property tests | [audit-phase4-maintenance.md](audit-phase4-maintenance.md) |

### MVP Blocking Items

1. **Error conversion gaps** — 12+ durability error types have no `From` impl to `StrataError`. If they surface at the API boundary, wire encoding fails.
2. **API naming inconsistency** — Event/State use `read()` while KV/JSON/Vector use `get()`. State returns `Versioned<Version>` while others return `Version`.

### Recommended Pre-MVP Fixes (Priority Order)

1. Add `From` impls for durability errors → `StrataError`
2. Fix VectorError placeholder BranchId in conversions
3. Standardize read operation naming (`get` across all primitives)
4. Fix State write return type (bare `Version`, not `Versioned<Version>`)
5. Remove unused `proptest` and `anyhow` dependencies
6. Add `#![warn(missing_docs)]` to executor and security crates
7. Rename duplicate `DiffEntry` types
8. Write `#[should_panic]` tests for 7 unimplemented stubs
9. Document `Value::Bytes` JSON roundtrip limitation at API level

### Post-MVP Optimizations

1. Hash-based transaction read_set (eliminate per-read key clones)
2. Iterator-based scan returns (10x improvement for large result sets)
3. Complete transaction extension traits (JSON delete, Event by_type, State init)
4. Split `database/mod.rs` and `core/primitives/json.rs` into sub-modules
5. Implement property-based tests for serialization roundtrips
