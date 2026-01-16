# Epic End Validation Plan

**Run this validation at the end of every epic before merging to develop.**

---

## Overview

Epic-end validation ensures:
1. All stories in the epic are complete and correct
2. Code quality meets standards
3. Implementation matches architecture spec (M3 or M4)
4. Tests are comprehensive and passing
5. No regressions introduced

**Note**: This document covers validation for both M3 (Epics 13-19) and M4 (Epics 20-25).
For M4-specific validation, see [Phase 6b: M4 Performance Validation](#phase-6b-m4-performance-validation-m4-only).

---

## Phase 1: Automated Checks (5 minutes)

### 1.1 Build & Test Suite

```bash
# Full workspace build
~/.cargo/bin/cargo build --workspace

# Full test suite
~/.cargo/bin/cargo test --workspace

# Release mode tests (catches optimization-related issues)
~/.cargo/bin/cargo test --workspace --release
```

### 1.2 Code Quality

```bash
# Clippy with strict warnings
~/.cargo/bin/cargo clippy --workspace -- -D warnings

# Format check
~/.cargo/bin/cargo fmt --check

# Documentation builds without warnings
~/.cargo/bin/cargo doc --workspace --no-deps
```

### 1.3 Automated Check Summary

| Check | Command | Pass Criteria |
|-------|---------|---------------|
| Build | `cargo build --workspace` | Zero errors |
| Tests | `cargo test --workspace` | All pass |
| Release Tests | `cargo test --workspace --release` | All pass |
| Clippy | `cargo clippy --workspace -- -D warnings` | Zero warnings |
| Format | `cargo fmt --check` | No changes needed |
| Docs | `cargo doc --workspace --no-deps` | Builds without warnings |

---

## Phase 2: Story Completion Verification (10 minutes)

### 2.1 Story Checklist

For EACH story in the epic, verify:

| Story | Files Created/Modified | Tests Added | Acceptance Criteria Met |
|-------|------------------------|-------------|-------------------------|
| #XXX | [ ] | [ ] | [ ] |
| #XXX | [ ] | [ ] | [ ] |
| ... | | | |

### 2.2 Verify Story Deliverables

```bash
# Check that expected files exist
ls -la crates/primitives/src/<expected_files>

# Check test count for the epic's module
~/.cargo/bin/cargo test --package primitives <module_name> -- --list 2>/dev/null | grep -c "test"
```

### 2.3 PR Status Check

```bash
# Verify all story PRs are merged to epic branch
/opt/homebrew/bin/gh pr list --state merged --base epic-<N>-<name> --json number,title

# Should match the number of stories in the epic
```

---

## Phase 3: Spec Compliance Review (15 minutes)

### 3.1 Architecture Spec Compliance

**For M3 (Epics 13-19)**: Open `docs/architecture/M3_ARCHITECTURE.md`
**For M4 (Epics 20-25)**: Open `docs/architecture/M4_ARCHITECTURE.md`

Verify implementation matches:

#### M3 Spec Compliance (Epics 13-19)

| Section | Requirement | Implemented Correctly |
|---------|-------------|----------------------|
| Section 3 | Stateless facades | [ ] Primitives hold only `Arc<Database>` |
| Section 4-8 | Primitive APIs | [ ] All methods match spec signatures |
| Section 9 | Key design | [ ] TypeTags and key formats correct |
| Section 10 | Transaction integration | [ ] Extension traits work |
| Section 12 | Invariant enforcement | [ ] Primitives enforce their invariants |

#### M4 Spec Compliance (Epics 20-25)

| Section | Requirement | Implemented Correctly |
|---------|-------------|----------------------|
| Section 2 | Key Design Decisions | [ ] DurabilityMode enum, DashMap + HashMap, pooling |
| Section 3 | Durability Modes | [ ] InMemory, Buffered, Strict implementations |
| Section 4 | Sharded Storage | [ ] ShardedStore, per-RunId sharding |
| Section 5 | Transaction Pooling | [ ] Thread-local pools, reset() method |
| Section 6 | Read Path Optimization | [ ] Fast path reads, snapshot-based |
| Section 7 | Red Flag Thresholds | [ ] All hard stops respected |

### 3.2 Spec Deviation Check

Search for potential deviations:

```bash
# Look for TODOs or FIXMEs that might indicate spec deviations
grep -r "TODO\|FIXME\|HACK\|XXX" crates/primitives/src/

# Look for unwrap/expect that might indicate incomplete error handling
grep -r "\.unwrap()\|\.expect(" crates/primitives/src/ | wc -l
```

**Rule**: If ANY deviation from spec is found:
1. Document why
2. Get explicit approval
3. Create follow-up issue if needed

---

## Phase 4: Code Review Checklist (20 minutes)

### 4.1 Structural Review

| Item | Check | Status |
|------|-------|--------|
| **Module organization** | Files in correct locations | [ ] |
| **Public API** | Only intended items are `pub` | [ ] |
| **Dependencies** | No unnecessary dependencies added | [ ] |
| **Re-exports** | lib.rs exports what users need | [ ] |

### 4.2 Code Quality Review

| Item | Check | Status |
|------|-------|--------|
| **Error handling** | Uses `Result<T, Error>`, no panics in library code | [ ] |
| **Naming** | Follows Rust conventions (snake_case, CamelCase) | [ ] |
| **Documentation** | Public items have doc comments | [ ] |
| **No dead code** | No unused functions, structs, or imports | [ ] |
| **No debug code** | No `println!`, `dbg!`, or debug logging left in | [ ] |

### 4.3 Safety Review

| Item | Check | Status |
|------|-------|--------|
| **No unsafe** | No `unsafe` blocks (unless justified and documented) | [ ] |
| **No unwrap on user input** | All user-provided data validated | [ ] |
| **No panic paths** | Library code returns errors, doesn't panic | [ ] |
| **Thread safety** | Primitives are `Send + Sync` | [ ] |

### 4.4 Test Quality Review

| Item | Check | Status |
|------|-------|--------|
| **Happy path tested** | Normal operations work | [ ] |
| **Error cases tested** | Invalid inputs return appropriate errors | [ ] |
| **Edge cases tested** | Empty inputs, max values, boundaries | [ ] |
| **Concurrent access** | Thread safety verified where applicable | [ ] |
| **Integration tests** | Cross-component interactions tested | [ ] |

---

## Phase 5: Best Practices Verification (10 minutes)

### 5.1 Rust Best Practices

| Practice | Verified |
|----------|----------|
| Use `&str` for input, `String` for owned data | [ ] |
| Prefer iterators over manual loops | [ ] |
| Use `?` operator for error propagation | [ ] |
| Derive traits where appropriate (`Debug`, `Clone`, etc.) | [ ] |
| Use `#[must_use]` for functions with important return values | [ ] |

### 5.2 Project-Specific Best Practices

| Practice | Verified |
|----------|----------|
| Primitives are stateless (only hold `Arc<Database>`) | [ ] |
| All operations scoped to `RunId` | [ ] |
| Keys use correct `TypeTag` | [ ] |
| Extension traits delegate to primitive internals | [ ] |
| Tests follow TDD - written before implementation | [ ] |

### 5.3 Performance Considerations

| Item | Verified |
|------|----------|
| No unnecessary allocations in hot paths | [ ] |
| No holding locks across await points | [ ] |
| Efficient key construction | [ ] |
| Batch operations where appropriate | [ ] |

---

## Phase 6: Epic-Specific Validation

### For Each M3 Epic:

#### Epic 13: Foundation
```bash
# Verify Key helpers work correctly
~/.cargo/bin/cargo test --package primitives key_

# Verify TypeTags are correct
grep -r "TypeTag::" crates/primitives/src/
```

#### Epic 14: KVStore
```bash
# Verify CRUD operations
~/.cargo/bin/cargo test --package primitives kv_

# Verify list with prefix
~/.cargo/bin/cargo test --package primitives test_kv_list
```

#### Epic 15: EventLog
```bash
# Verify chain integrity
~/.cargo/bin/cargo test --package primitives test_event_chain

# Verify append-only (no update/delete)
grep -r "fn update\|fn delete" crates/primitives/src/event_log.rs
# Should show methods that return InvalidOperation error
```

#### Epic 16: StateCell
```bash
# Verify CAS semantics
~/.cargo/bin/cargo test --package primitives test_state_cas

# Verify transition purity documented
grep -r "purity\|pure\|Purity" crates/primitives/src/state_cell.rs
```

#### Epic 17: TraceStore
```bash
# Verify indices work
~/.cargo/bin/cargo test --package primitives test_trace_query

# Verify parent-child relationships
~/.cargo/bin/cargo test --package primitives test_trace_tree
```

#### Epic 18: RunIndex
```bash
# Verify status transitions
~/.cargo/bin/cargo test --package primitives test_status_transition

# Verify cascading delete
~/.cargo/bin/cargo test --package primitives test_delete_run
```

#### Epic 19: Integration
```bash
# Run all integration tests
~/.cargo/bin/cargo test --package primitives --test cross_primitive_tests
~/.cargo/bin/cargo test --package primitives --test run_isolation_tests
~/.cargo/bin/cargo test --package primitives --test recovery_tests

# Run benchmarks
~/.cargo/bin/cargo bench --package primitives
```

---

## Phase 6b: M4 Performance Validation (M4 Only)

**This phase is REQUIRED for all M4 epics (20-25).**

### 6b.1 Performance Benchmarks

```bash
# Run M4 performance benchmarks
~/.cargo/bin/cargo bench --bench m4_performance

# Run facade tax benchmarks
~/.cargo/bin/cargo bench --bench m4_facade_tax

# Expected results (InMemory mode):
# - engine/put_direct: < 3µs (red flag: > 10µs)
# - kvstore/put: < 8µs (red flag: > 20µs)
# - kvstore/get: < 5µs (red flag: > 10µs)
```

### 6b.2 Red Flag Tests

```bash
# Run red flag verification tests
~/.cargo/bin/cargo test --test m4_red_flags

# All red flag thresholds must pass:
# - Snapshot acquisition: < 2µs
# - A1/A0 ratio: < 20×
# - B/A1 ratio: < 8×
# - Disjoint scaling (4T): > 2.5×
# - p99 latency: < 20× mean
# - Hot-path allocations: 0
```

**CRITICAL**: If ANY red flag test fails, STOP and REDESIGN before proceeding.

### 6b.3 Facade Tax Verification

| Layer | Description | Target | Red Flag |
|-------|-------------|--------|----------|
| A0 | Raw HashMap | baseline | - |
| A1 | Engine/Storage | < 10× A0 | > 20× A0 |
| B | Primitive facade | < 5× A1 | > 8× A1 |

```bash
# Verify facade tax is within limits
~/.cargo/bin/cargo bench --bench m4_facade_tax -- --save-baseline m4-facade

# Expected output should show:
# A1/A0: < 10× (target), definitely < 20× (red flag)
# B/A1: < 5× (target), definitely < 8× (red flag)
```

### 6b.4 Scaling Verification

```bash
# Run scaling tests
~/.cargo/bin/cargo test --test m4_scaling

# Verify disjoint workload scaling:
# - 1 thread: baseline
# - 2 threads: > 1.8× improvement
# - 4 threads: > 2.5× improvement (red flag: < 2.5×)
```

### 6b.5 Durability Mode Verification (Epic 21+)

```bash
# Test each durability mode
~/.cargo/bin/cargo test --test m4_durability_modes

# Verify latency targets:
# - InMemory: < 3µs (no persistence)
# - Buffered: < 30µs (async fsync)
# - Strict: ~2ms (sync fsync, matches M3)
```

### 6b.6 M4 Epic-Specific Validation

#### Epic 20: Performance Foundation
```bash
# Verify baseline tag exists
git tag -l | grep "m3_baseline_perf"

# Verify DurabilityMode enum (in durability crate, not core)
grep -r "enum DurabilityMode" crates/durability/src/

# Verify instrumentation (in engine crate, not core)
~/.cargo/bin/cargo test --package in-mem-engine --features perf-trace perf
```

#### Epic 21: Durability Modes
```bash
# Verify all three modes work (durability crate)
~/.cargo/bin/cargo test --package in-mem-durability durability

# Verify graceful shutdown (engine crate)
~/.cargo/bin/cargo test --package in-mem-engine test_shutdown

# Verify per-operation override (engine crate)
~/.cargo/bin/cargo test --package in-mem-engine test_override
```

#### Epic 22: Sharded Storage
```bash
# Verify ShardedStore implementation (storage crate)
~/.cargo/bin/cargo test --package in-mem-storage sharded

# Verify snapshot acquisition time < 2µs
~/.cargo/bin/cargo bench --bench m4_performance -- snapshot

# Verify migration from old storage (storage crate)
~/.cargo/bin/cargo test --package in-mem-storage test_migration
```

#### Epic 23: Transaction Pooling
```bash
# Verify pool implementation (concurrency crate)
~/.cargo/bin/cargo test --package in-mem-concurrency transaction_pool

# Verify reset() preserves capacity (concurrency crate)
~/.cargo/bin/cargo test --package in-mem-concurrency test_reset_capacity

# Verify no pool exhaustion under load (concurrency crate)
~/.cargo/bin/cargo test --package in-mem-concurrency test_pool_stress
```

#### Epic 24: Read Path Optimization
```bash
# Verify fast path reads
~/.cargo/bin/cargo test --package primitives fast_path

# Verify observational equivalence
~/.cargo/bin/cargo test --test m4_fast_path_equivalence

# Verify batch operations use single snapshot
~/.cargo/bin/cargo test --package primitives test_get_many
```

#### Epic 25: Validation & Red Flags
```bash
# Run complete validation suite
~/.cargo/bin/cargo bench --bench m4_performance
~/.cargo/bin/cargo test --test m4_red_flags
~/.cargo/bin/cargo test --test m4_contention

# Generate performance report
~/.cargo/bin/cargo bench --bench m4_performance -- --save-baseline m4-final
```

### 6b.7 M4 Performance Checklist

| Metric | Target | Red Flag | Actual | Status |
|--------|--------|----------|--------|--------|
| `engine/put_direct` (InMemory) | < 3µs | > 10µs | ___ | [ ] |
| `kvstore/put` (InMemory) | < 8µs | > 20µs | ___ | [ ] |
| `kvstore/get` | < 5µs | > 10µs | ___ | [ ] |
| Throughput (1T InMemory) | 250K ops/sec | < 100K | ___ | [ ] |
| Throughput (4T disjoint) | 800K ops/sec | < 400K | ___ | [ ] |
| Snapshot acquisition | < 500ns | > 2µs | ___ | [ ] |
| A1/A0 ratio | < 10× | > 20× | ___ | [ ] |
| B/A1 ratio | < 5× | > 8× | ___ | [ ] |
| Disjoint scaling (4T) | > 3× | < 2.5× | ___ | [ ] |
| p99/mean latency | < 10× | > 20× | ___ | [ ] |

---

## Phase 7: Final Sign-Off

### 7.1 Completion Checklist

| Item | Status |
|------|--------|
| All automated checks pass (Phase 1) | [ ] |
| All stories verified complete (Phase 2) | [ ] |
| Spec compliance confirmed (Phase 3) | [ ] |
| Code review complete (Phase 4) | [ ] |
| Best practices verified (Phase 5) | [ ] |
| Epic-specific validation done (Phase 6) | [ ] |
| **M4 Only**: Performance validation (Phase 6b) | [ ] |
| **M4 Only**: All red flags pass | [ ] |

### 7.2 Sign-Off

```
Epic: ___
Reviewer: ___
Date: ___

[ ] This epic is APPROVED for merge to develop

Notes:
_______________________
```

---

## Post-Validation: Merge to Develop

After all phases pass:

```bash
# 1. Ensure epic branch is up to date
git checkout epic-<N>-<name>
git pull origin epic-<N>-<name>

# 2. Final test run
~/.cargo/bin/cargo test --workspace

# 3. Merge to develop
git checkout develop
git pull origin develop
git merge --no-ff epic-<N>-<name> -m "Epic <N>: <Epic Name> complete

Delivered:
- Story #XXX: Description
- Story #XXX: Description
...

All validation phases passed."

# 4. Push
git push origin develop

# 5. Close epic issue
/opt/homebrew/bin/gh issue close <epic-issue-number> --comment "Epic complete. All stories delivered and validated."
```

---

## Validation Prompt Template

### M3 Epic Validation (Epics 13-19)

Use this prompt to run epic-end validation:

```
## Task: Epic End Validation

Run the complete epic-end validation for Epic <N>: <Epic Name>.

**Steps**:
1. Run Phase 1 automated checks
2. Verify all <X> stories are complete (Phase 2)
3. Verify spec compliance against M3_ARCHITECTURE.md (Phase 3)
4. Perform code review checklist (Phase 4)
5. Verify best practices (Phase 5)
6. Run epic-specific validation (Phase 6)
7. Provide final sign-off summary (Phase 7)

**Expected Output**:
- Pass/fail status for each phase
- Any issues found with recommendations
- Final sign-off or list of blockers

**Reference**:
- Epic prompt: docs/prompts/epic-<N>-claude-prompts.md
- M3 Architecture: docs/architecture/M3_ARCHITECTURE.md
- Stories: #XXX - #XXX
```

### M4 Epic Validation (Epics 20-25)

Use this prompt for M4 performance milestone validation:

```
## Task: M4 Epic End Validation

Run the complete epic-end validation for Epic <N>: <Epic Name>.

**Steps**:
1. Run Phase 1 automated checks
2. Verify all <X> stories are complete (Phase 2)
3. Verify spec compliance against M4_ARCHITECTURE.md (Phase 3)
4. Perform code review checklist (Phase 4)
5. Verify best practices (Phase 5)
6. Run epic-specific validation (Phase 6)
7. Run M4 performance validation (Phase 6b) - CRITICAL
8. Verify ALL red flag thresholds pass
9. Provide final sign-off summary (Phase 7)

**Expected Output**:
- Pass/fail status for each phase
- Performance benchmark results vs targets
- Red flag test results (ALL must pass)
- Any issues found with recommendations
- Final sign-off or list of blockers

**Reference**:
- Epic prompt: docs/prompts/epic-<N>-claude-prompts.md
- M4 Architecture: docs/architecture/M4_ARCHITECTURE.md
- M4 Prompt Header: docs/prompts/M4_PROMPT_HEADER.md
- Stories: #XXX - #XXX

**CRITICAL**: If ANY red flag fails, STOP and REDESIGN. Do NOT proceed.
```

---

## Quick Reference: Validation Commands

### M3 Quick Validation

```bash
# One-liner for Phase 1
~/.cargo/bin/cargo build --workspace && \
~/.cargo/bin/cargo test --workspace && \
~/.cargo/bin/cargo clippy --workspace -- -D warnings && \
~/.cargo/bin/cargo fmt --check && \
echo "Phase 1: PASS"

# Count tests in primitives
~/.cargo/bin/cargo test --package primitives -- --list 2>/dev/null | grep -c "test"

# Check for spec deviations
grep -r "TODO\|FIXME\|HACK" crates/primitives/src/
```

### M4 Quick Validation

```bash
# One-liner for Phase 1 + M4 performance
~/.cargo/bin/cargo build --workspace && \
~/.cargo/bin/cargo test --workspace && \
~/.cargo/bin/cargo clippy --workspace -- -D warnings && \
~/.cargo/bin/cargo fmt --check && \
~/.cargo/bin/cargo bench --bench m4_performance && \
~/.cargo/bin/cargo test --test m4_red_flags && \
echo "Phase 1 + M4 Performance: PASS"

# Run all M4 benchmarks
~/.cargo/bin/cargo bench --bench m4_performance
~/.cargo/bin/cargo bench --bench m4_facade_tax

# Run all M4 tests
~/.cargo/bin/cargo test --test m4_red_flags
~/.cargo/bin/cargo test --test m4_fast_path_equivalence
~/.cargo/bin/cargo test --test m4_scaling
~/.cargo/bin/cargo test --test m4_contention

# Check red flag compliance
~/.cargo/bin/cargo test --test m4_red_flags -- --nocapture
```

### M4 Red Flag Quick Check

```bash
# Quick red flag verification (run after any M4 change)
~/.cargo/bin/cargo test --test m4_red_flags -- \
  test_snapshot_acquisition_time \
  test_facade_tax_a1_a0 \
  test_facade_tax_b_a1 \
  test_disjoint_scaling \
  test_p99_latency \
  test_zero_allocations
```

---

## M4 Milestone Completion Checklist

**Use this checklist when completing ALL M4 epics (Epic 25 final validation):**

| Category | Requirement | Status |
|----------|-------------|--------|
| **Durability** | | |
| | InMemory mode: < 3µs | [ ] |
| | Buffered mode: < 30µs | [ ] |
| | Strict mode: ~2ms (M3 compatible) | [ ] |
| | Per-operation override works | [ ] |
| | Graceful shutdown flushes all | [ ] |
| **Sharding** | | |
| | ShardedStore with DashMap | [ ] |
| | Snapshot acquisition < 2µs | [ ] |
| | Zero-allocation snapshots | [ ] |
| **Pooling** | | |
| | Thread-local transaction pools | [ ] |
| | reset() preserves capacity | [ ] |
| | No pool exhaustion | [ ] |
| **Read Path** | | |
| | Fast path get() < 5µs | [ ] |
| | Batch get uses single snapshot | [ ] |
| | Observational equivalence | [ ] |
| **Red Flags** | | |
| | Snapshot: < 2µs | [ ] |
| | A1/A0: < 20× | [ ] |
| | B/A1: < 8× | [ ] |
| | Scaling (4T): > 2.5× | [ ] |
| | p99: < 20× mean | [ ] |
| | Hot-path allocations: 0 | [ ] |
| **Documentation** | | |
| | M4_ARCHITECTURE.md complete | [ ] |
| | All story PRs merged | [ ] |
| | Benchmark results documented | [ ] |

---

*End of Epic End Validation Plan*
