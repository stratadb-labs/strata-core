# Phase 4: Maintenance Audit (Dependencies, Docs, Tests, Large Files)

Date: 2026-02-04
Status: Complete

## Summary

The codebase is well-maintained with clean dependencies, comprehensive error recovery testing, and good documentation coverage. Key issues: 2 unused dependencies, 2 crates missing `missing_docs` lint, 7 unimplemented stubs without panic tests, and 1 large file needing decomposition.

**MVP Readiness: PASS** — no blocking issues. Several cleanup items recommended.

---

## A. Dependency Audit

### Unused Dependencies

| Dependency | Version | Status | Action |
|-----------|---------|--------|--------|
| `proptest` | 1.4 | Listed in 5 crate dev-deps, **zero usage** | Remove or implement property tests |
| `anyhow` | 1.0 | Listed in workspace deps, **zero usage** | Remove (codebase uses `thiserror` exclusively) |

### Optional Dependencies — Properly Feature-Gated

| Dependency | Feature Gate | Purpose |
|-----------|-------------|---------|
| `redb` 2.0 | `comparison-benchmarks` | Benchmark comparison |
| `heed` 0.20 | `comparison-benchmarks` | Benchmark comparison |
| `rusqlite` 0.32 | `comparison-benchmarks` | Benchmark comparison |
| `usearch` 2.0 | `usearch-enabled` | Vector index backend |

All optional deps are isolated behind feature flags. No production code depends on them without the feature enabled.

### Dependency Versions — All Current

No deprecated, yanked, or known-problematic versions found. Key dependencies:
- `uuid` 1.6, `serde` 1.0, `tokio` 1.35, `chrono` 0.4, `parking_lot` 0.12, `dashmap` 5 — all stable and current.

---

## B. Documentation Completeness

### `missing_docs` Lint Enforcement

| Crate | Enforces `warn(missing_docs)` |
|-------|------------------------------|
| strata-core | Yes |
| strata-storage | Yes |
| strata-concurrency | Yes |
| strata-durability | Yes |
| strata-engine | Yes |
| strata-intelligence | Yes |
| strata-executor | **No** |
| strata-security | **No** |

**Issue**: `strata-executor` is the public API crate — it should enforce `missing_docs`.

### TODO Comments (4 total)

| File | Line | Content |
|------|------|---------|
| `engine/database/mod.rs` | 590 | TODO: Wire to `DatabaseHandle::checkpoint()` |
| `engine/database/mod.rs` | 603 | TODO: Wire to `DatabaseHandle::compact()` |
| `executor/executor.rs` | 127 | TODO: Call substrate flush |
| `executor/executor.rs` | 131 | TODO: Call substrate compact |

All 4 are legitimate Phase 4/5 work items. No stale or forgotten TODOs.

### Duplicate Type Names

**Two `DiffEntry` types exist**:
1. `crates/engine/src/recovery/replay.rs` — replay context
2. `crates/engine/src/branch_ops.rs` — branch diff context

The engine `lib.rs` has a comment noting the clash:
```rust
// Note: DiffEntry is not re-exported here to avoid clash with recovery::DiffEntry.
// Use strata_engine::branch_ops::DiffEntry for branch diff entries.
```

**Recommendation**: Rename to `BranchDiffEntry` and `ReplayDiffEntry` for clarity.

### Value::Bytes ↔ JSON Roundtrip

`Value::Bytes` is base64-encoded when serialized to JSON. This lossy conversion is **not documented at the API level**. Consumers must know that binary data round-tripped through JSON will change representation.

---

## C. Test Coverage

### Test Statistics

| Category | Count |
|----------|-------|
| Unit tests (`#[test]`) | 2,203+ |
| Integration test files | 25 |
| Doc-tests | ~150 |
| Benchmark suites | 2 (criterion) |
| `#[ignore]` tests | 0 |
| Property-based tests | 0 |

### Ignored Tests

**None found.** All tests are expected to run. This is good — no hidden skipped tests.

### Unimplemented Stubs Without Panic Tests

7 `unimplemented!()` stubs in `crates/engine/src/transaction/context.rs`:

| Line | Stub | Phase |
|------|------|-------|
| 609 | `vector_insert()` | Phase 4 |
| 617 | `vector_get()` | Phase 4 |
| 621 | `vector_delete()` | Phase 4 |
| 631 | `vector_search()` | Phase 4 |
| 635 | `vector_exists()` | Phase 4 |
| 643 | `branch_metadata()` | Phase 5 |
| 647 | `branch_update_status()` | Phase 5 |

**No `#[should_panic]` tests exist for these stubs.** If any code path accidentally calls them, they'll panic without a test catching the regression.

### Error Recovery Testing — Excellent

| Test File | LOC | Coverage |
|-----------|-----|---------|
| `crash_simulation_test.rs` | 935 | WAL corruption scenarios |
| `recovery_tests.rs` | 697 | Crash recovery, partial records |
| `adversarial_tests.rs` | 1,143 | Adversarial scenarios |
| `critical_audit_tests.rs` | — | Audit-specific paths |

WAL corruption, partial writes, crash-before-commit, crash-after-commit, and recovery idempotence are all tested.

### Property-Based Testing

`proptest` dependency exists but has zero usage. Candidates for property testing:
- Key/Value serialization roundtrips
- JSON path parsing (fuzzing)
- Version comparison logic
- Codec compression/decompression

---

## D. Large File Decomposition

### Files Over 1,000 LOC

| File | LOC | Cohesion | Split Priority |
|------|-----|----------|---------------|
| `storage/sharded.rs` | 3,312 | GOOD | LOW — cohesive MVCC impl |
| `core/primitives/json.rs` | 3,206 | FAIR | MEDIUM — extract path/patch |
| `core/error.rs` | 2,461 | GOOD | LOW — cohesive error hierarchy |
| `engine/primitives/vector/store.rs` | 2,395 | FAIR | MEDIUM — extract indexing |
| `engine/primitives/json.rs` | 1,795 | FAIR | LOW |
| `core/types.rs` | 1,762 | FAIR | LOW |
| `concurrency/transaction.rs` | 1,661 | FAIR | LOW |
| `engine/database/mod.rs` | 1,551 | **POOR** | **HIGH** |
| `core/primitives/vector.rs` | 1,335 | FAIR | LOW |
| `engine/primitives/vector/hnsw.rs` | 1,241 | GOOD | LOW — cohesive algorithm |
| `engine/primitives/event.rs` | 1,160 | FAIR | LOW |
| `engine/transaction/context.rs` | 1,154 | FAIR | LOW |
| `engine/branch_ops.rs` | 1,133 | FAIR | LOW |
| `engine/recovery/replay.rs` | 1,127 | FAIR | LOW |
| `durability/snapshot.rs` | 1,059 | FAIR | LOW |
| `engine/coordinator.rs` | 1,008 | FAIR | LOW |

### Decomposition Recommendations

#### HIGH: `engine/database/mod.rs` (1,551 LOC)

Combines database lifecycle, config, registry, and transaction APIs. Already partially split (`config.rs`, `registry.rs`, `transactions.rs` exist as sub-modules), but the main `mod.rs` still has too many responsibilities.

**Suggested split**:
1. `database/checkpoint.rs` — checkpoint/compact stubs (~100 LOC)
2. Keep remaining core in `database/mod.rs` (~1,450 LOC)

#### MEDIUM: `core/primitives/json.rs` (3,206 LOC)

**Suggested split**:
1. `json/path.rs` — JsonPath, PathSegment, parsing (~500 LOC)
2. `json/patch.rs` — Patch operations (~800 LOC)
3. `json/mod.rs` — JsonValue + utilities (remaining)

#### MEDIUM: `engine/primitives/vector/store.rs` (2,395 LOC)

**Suggested split**:
1. `vector/indexing.rs` — Collection indexing logic (~800 LOC)
2. `vector/store.rs` — Core storage (remaining)

---

## E. Recommendations

### Pre-MVP

1. **Remove unused `proptest` and `anyhow` dependencies** — clean dependency tree
2. **Add `#![warn(missing_docs)]` to executor and security crates**
3. **Rename one `DiffEntry` type** to eliminate ambiguity
4. **Write `#[should_panic]` tests** for the 7 unimplemented stubs
5. **Document `Value::Bytes` JSON roundtrip** at the API level

### Post-MVP

6. Split `database/mod.rs` into sub-modules
7. Extract `json.rs` path/patch operations
8. Implement property-based tests for serialization roundtrips
9. Extract `vector/store.rs` indexing logic

---

## Methodology

Read root `Cargo.toml` and all crate `Cargo.toml` files. Searched for `proptest::`, `anyhow::`, `#![warn(missing_docs)]`, `TODO`, `FIXME`, `HACK`, `#[ignore]`, `unimplemented!()`, `#[should_panic]`. Counted test functions. Measured file line counts. Assessed cohesion by examining struct/impl distribution within each large file.
