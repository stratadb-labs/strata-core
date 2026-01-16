# M4 Primitive Performance Test Plan

## Purpose

Sanity testing to verify that M4 performance improvements (MVCC version chains, per-run commit locks, lazy snapshots) did not accidentally regress performance or semantics of M3 primitives.

This is **not** benchmarking for optimization. This is regression detection.

**Two categories of risk:**
1. **Performance regression** - Operations became slower
2. **Semantic drift** - Observable behavior changed subtly

## Scope

All M3 primitives:
- KVStore
- EventLog
- StateCell
- TraceStore
- RunIndex

---

## Part 1: Semantic Equivalence Tests

### Why This Matters

M4 changed fundamental mechanisms:

| Change | Risk |
|--------|------|
| **MVCC Version Chains** | Snapshot reads might return different values under concurrent writes |
| **Lazy Snapshots** | Reads happen against live storage, not frozen copy |
| **Per-Run Sharding** | Cross-run operations might behave differently |
| **Per-Run Commit Locks** | Commit ordering within vs across runs changed |
| **WAL Skip (InMemory)** | Recovery behavior differs by mode |

These changes could introduce subtle semantic differences that performance tests won't catch.

### Methodology

For each primitive, run identical workloads under all three durability modes:
- `DurabilityMode::Strict`
- `DurabilityMode::Buffered`
- `DurabilityMode::InMemory`

**Assert**: Observable behavior is identical across all modes.

### 1.1 StateCell Semantic Invariants

StateCell is highest risk due to CAS semantics interacting with MVCC.

```
Test: statecell_cas_semantics_across_modes
Workload:
  1. Init cell with value A
  2. Read version V1
  3. CAS(expected=V1, new=B) → should succeed
  4. CAS(expected=V1, new=C) → should fail (stale version)
  5. Read → should return B
Assert: Identical behavior in Strict, Buffered, InMemory

Test: statecell_concurrent_cas_no_lost_updates
Workload:
  1. Init cell with value 0, version V0
  2. N threads each try CAS(read_version, current+1) in loop until success
  3. Each thread records: (success_count, versions_won)
  4. Final value should equal sum of all success_count
Assert:
  - Final value == total successful CAS across all threads
  - No two threads report success for the same version
  - All won versions are unique and monotonically increasing
  - No phantom success (thread claims success but value doesn't reflect it)
NOTE: Do NOT assert winner identity - thread scheduling varies across modes/OS/CPUs

Test: statecell_snapshot_isolation_cas
Workload:
  1. Init cell with value A, version V1
  2. Transaction T1: read cell (sees A, V1), pause
  3. Transaction T2: CAS(V1, B) → succeeds, commits
  4. Transaction T1: CAS(V1, C) → must fail (V1 is stale)
Assert: T1 CAS fails in all modes (snapshot isolation preserved)

Test: statecell_transition_atomicity
Workload:
  1. Init cell with value 0
  2. 4 threads: transition(|v| v + 1) × 100 each
  3. Final value should be 400
Assert: Value is exactly 400 in all modes (no lost updates)
```

### 1.2 EventLog Semantic Invariants

EventLog has hash chain integrity that must be preserved.

```
Test: eventlog_sequence_monotonicity_across_modes
Workload:
  1. Append 100 events
  2. Read all events
Assert: Sequence numbers 0-99, monotonic, no gaps, all modes

Test: eventlog_hash_chain_integrity_across_modes
Workload:
  1. Append 100 events
  2. Verify each event.prev_hash == hash(previous_event)
Assert: Chain valid in all modes

Test: eventlog_concurrent_append_ordering
Workload:
  1. 4 threads each append 25 events to same log
  2. Read all 100 events
Assert:
  - Exactly 100 events
  - Sequence numbers 0-99
  - Hash chain valid
  - Same in all modes

Test: eventlog_snapshot_sees_committed_only
Workload:
  1. Append events 0-9, commit
  2. Start transaction T1, create snapshot
  3. T1 reads events → should see 0-9
  4. Append events 10-19 in T2, commit
  5. T1 reads events → should still see only 0-9
Assert: Snapshot isolation in all modes
```

### 1.3 TraceStore Semantic Invariants

TraceStore has parent-child relationships that must be consistent.

```
Test: tracestore_parent_child_consistency_across_modes
Workload:
  1. Create root trace R
  2. Create child traces C1, C2, C3 with parent=R
  3. get_children(R) → should return {C1, C2, C3}
Assert: Same children in all modes

Test: tracestore_trace_type_filtering_across_modes
Workload:
  1. Record 10 LLM traces, 10 Tool traces, 10 Agent traces
  2. get_by_type(LLM) → should return exactly 10
Assert: Same results in all modes

Test: tracestore_concurrent_tree_building
Workload:
  1. Thread 1: Create root R, children C1-C5
  2. Thread 2: Create children C1.1-C1.5 under C1
  3. Thread 3: Create children C2.1-C2.5 under C2
  4. Verify complete tree structure
Assert: Tree structure identical in all modes
```

### 1.4 KVStore Semantic Invariants

KVStore is lower risk but still needs verification.

```
Test: kv_read_your_writes_across_modes
Workload:
  1. In transaction: put(k, v1), get(k) → should return v1
  2. Commit
  3. get(k) → should return v1
Assert: Same in all modes

Test: kv_snapshot_isolation_across_modes
Workload:
  1. put(k, v1), commit
  2. Start T1, read k → v1
  3. In T2: put(k, v2), commit
  4. T1 read k → should still be v1
  5. T1 commit
  6. New read k → should be v2
Assert: Snapshot isolation in all modes

Test: kv_delete_visibility_across_modes
Workload:
  1. put(k, v), commit
  2. Start T1, read k → v
  3. In T2: delete(k), commit
  4. T1 read k → should still be v (snapshot)
  5. T1 commit
  6. New read k → should be None
Assert: Same in all modes

Test: kv_ttl_expiration_across_modes
Workload:
  1. put_with_ttl(k, v, 100ms)
  2. Immediate read → should return v
  3. Wait 150ms
  4. Read → should return None
Assert: TTL behavior same in all modes
```

### 1.5 RunIndex Semantic Invariants

```
Test: runindex_status_transitions_across_modes
Workload:
  1. Create run (status=Pending)
  2. Transition: Pending → Active → Completed
  3. Verify each transition succeeds
  4. Verify invalid transition (Completed → Active) fails
Assert: Same transition rules in all modes

Test: runindex_list_by_status_across_modes
Workload:
  1. Create 10 runs: 3 Active, 4 Completed, 3 Failed
  2. list_by_status(Active) → exactly 3
  3. list_by_status(Completed) → exactly 4
Assert: Same filtering in all modes
```

### 1.6 Cross-Primitive Semantic Invariants

These test interactions between primitives under M4 changes.

```
Test: cross_primitive_transaction_atomicity
Workload:
  In single transaction:
    1. KV: put(k, v)
    2. EventLog: append(event)
    3. StateCell: set(cell, state)
    4. Commit
  Verify: All three visible after commit, none before
Assert: Atomicity in all modes

Test: cross_primitive_rollback_consistency
Workload:
  In single transaction:
    1. KV: put(k, v)
    2. EventLog: append(event)
    3. Force abort (e.g., validation conflict)
  Verify: Neither KV nor EventLog change visible
Assert: Rollback complete in all modes

Test: cross_run_isolation
Workload:
  1. Run A: put(k, v1)
  2. Run B: put(k, v2) (same key name, different namespace)
  3. Run A read k → v1
  4. Run B read k → v2
Assert: Complete isolation in all modes
```

### 1.7 Snapshot Monotonicity Invariants

**Critical for version chain correctness**: Once a snapshot sees version X, it must never later see something older than X. This catches bugs in version chain traversal.

```
Test: kv_snapshot_version_monotonicity
Workload:
  1. put(k, v1) at version 1
  2. put(k, v2) at version 2
  3. put(k, v3) at version 3
  4. Create snapshot at version 2
  5. Read k → should see v2
  6. Read k again → must still see v2 (not v1)
  7. Concurrent write: put(k, v4) at version 4
  8. Snapshot read k → must still see v2
Assert: Snapshot never regresses to older version

Test: statecell_snapshot_version_monotonicity
Workload:
  1. Init cell, CAS to v1 (version 1)
  2. CAS to v2 (version 2)
  3. CAS to v3 (version 3)
  4. Create snapshot at version 2
  5. Read cell → should see v2
  6. Concurrent CAS to v4 (version 4)
  7. Snapshot read cell → must still see v2
  8. Snapshot CAS with version 2 → must fail (even though v2 matches snapshot)
Assert: Version never goes backward within snapshot lifetime

Test: eventlog_snapshot_sequence_monotonicity
Workload:
  1. Append events 0-9
  2. Create snapshot
  3. Snapshot reads events → sees 0-9
  4. Append events 10-19 (concurrent)
  5. Snapshot reads events → must still see exactly 0-9
  6. Snapshot must not see partial 10-19
Assert: Event sequence boundary stable within snapshot

Test: repeated_read_stability
Workload:
  For each primitive (KV, StateCell, EventLog):
    1. Write initial value
    2. Create snapshot
    3. Read value 100 times in loop
    4. Concurrent writes happening in background
    5. All 100 reads must return identical value
Assert: Snapshot reads are stable under concurrent mutation
```

### 1.8 ABA Detection Tests

**Critical for versioned CAS correctness**: The ABA problem occurs when a value changes A→B→A. A naive implementation might allow stale CAS to succeed because the value "looks the same." Version-based systems must reject this.

```
Test: statecell_aba_version_guard
Workload:
  1. Init cell with value "A", version V1
  2. Transaction T1: read cell → sees ("A", V1), pause
  3. Transaction T2: CAS(V1, "B") → succeeds, now ("B", V2)
  4. Transaction T3: CAS(V2, "A") → succeeds, now ("A", V3)
  5. Transaction T1: CAS(V1, "C") → MUST FAIL
     (value is "A" again, but version is V3, not V1)
Assert: CAS fails despite value matching original

Test: statecell_aba_rapid_cycle
Workload:
  1. Init cell with value 0, version V0
  2. Snapshot S1 reads → sees (0, V0)
  3. Rapid cycle: 0 → 1 → 2 → 1 → 0 (back to original value)
  4. Current state: value=0, version=V4
  5. S1 attempts CAS(V0, 99) → MUST FAIL
Assert: Version V0 ≠ V4 even though values equal

Test: statecell_aba_concurrent_stress
Workload:
  1. Init cell with value 0
  2. Thread A: loop { read; pause; CAS(read_version, value+1) }
  3. Thread B: loop { increment; decrement } (creates ABA cycles)
  4. Thread C: loop { increment; decrement } (more ABA cycles)
  5. Run for 1000 iterations
Assert:
  - Thread A never succeeds with stale version
  - Final value == Thread A's successful CAS count
  - No lost updates despite ABA cycles

Test: kv_aba_delete_recreate
Workload:
  1. put(k, "A") at version V1
  2. Snapshot S1 reads k → sees "A"
  3. delete(k)
  4. put(k, "A") again at version V3
  5. S1 reads k → should see original "A" (V1), not new "A" (V3)
  6. Any version-based operation from S1 must use V1, not V3
Assert: Delete+recreate doesn't confuse snapshot

Test: eventlog_aba_not_applicable
Description:
  EventLog is append-only, so ABA doesn't apply in the traditional sense.
  However, verify that sequence numbers are never reused:
Workload:
  1. Append events 0-9
  2. "Delete" log (if supported) or create new log
  3. Append events to new/cleared log
  4. New events must start at seq 0, not continue from 9
  5. Old snapshots must not see new log's events
Assert: Sequence namespaces don't collide
```

### Why ABA Matters for M4

M4 introduced version chains with lazy snapshot reads. The risk:

```
Version Chain: [V3: "A"] → [V2: "B"] → [V1: "A"]

Snapshot at V1 reads "A"
Current value is also "A" (at V3)

Naive implementation might:
  - See current value = "A"
  - Think "matches snapshot"
  - Allow CAS

Correct implementation:
  - Check version, not just value
  - V1 ≠ V3, reject CAS
```

### Semantic Test Structure

```
tests/
  m4_semantic_equivalence/
    mod.rs                    # Multi-mode test harness
    statecell_semantics.rs    # StateCell invariants
    eventlog_semantics.rs     # EventLog invariants
    tracestore_semantics.rs   # TraceStore invariants
    kv_semantics.rs           # KVStore invariants
    runindex_semantics.rs     # RunIndex invariants
    cross_primitive.rs        # Cross-primitive invariants
    snapshot_monotonicity.rs  # Version monotonicity under snapshot (1.7)
    aba_detection.rs          # ABA problem detection (1.8)
```

### Multi-Mode Test Harness

```rust
/// Run a test workload across all durability modes
fn test_across_modes<F, T>(name: &str, workload: F)
where
    F: Fn(Arc<Database>) -> T,
    T: PartialEq + Debug,
{
    let modes = [
        DurabilityMode::Strict,
        DurabilityMode::Buffered,
        DurabilityMode::InMemory,
    ];

    let results: Vec<(DurabilityMode, T)> = modes
        .iter()
        .map(|mode| {
            let db = create_test_db(*mode);
            let result = workload(db);
            (*mode, result)
        })
        .collect();

    // Assert all results identical
    let first = &results[0].1;
    for (mode, result) in &results[1..] {
        assert_eq!(
            first, result,
            "Semantic drift: {:?} differs from Strict mode in test {}",
            mode, name
        );
    }
}
```

---

## Part 2: Performance Tests

## Methodology

### Test Structure

Each primitive gets tests in three categories:

1. **Single-Operation Latency**: Measure individual operation cost
2. **Throughput Under Load**: Measure sustained operation rate
3. **Concurrent Access**: Verify scaling with multiple threads

### Measurement Approach

- Use `std::time::Instant` for timing (not criterion)
- Warmup phase before measurement (100-1000 ops)
- Measure over sufficient iterations (1000-10000 ops)
- Report: mean, p50, p95, p99
- Run in release mode only

### Pass/Fail Criteria

Tests fail if:
- Mean latency > 10× expected baseline
- p99 > 50× mean (tail latency explosion)
- Throughput < 50% of expected baseline
- Concurrent scaling < 1.5× at 4 threads for disjoint workloads

These are intentionally loose thresholds - we're catching regressions, not optimizing.

---

## Test Specifications

### 1. KVStore Performance Tests

#### 1.1 KV Single-Operation Latency

```
Test: kv_put_latency
Setup: Create KVStore, single RunId
Measure: 10000 puts of small values (i64)
Expected: < 5µs mean

Test: kv_get_latency
Setup: Pre-populate 1000 keys
Measure: 10000 gets (mix of hits and misses)
Expected: < 2µs mean

Test: kv_delete_latency
Setup: Pre-populate 1000 keys
Measure: 1000 deletes
Expected: < 5µs mean

Test: kv_put_with_ttl_latency
Setup: Create KVStore, single RunId
Measure: 10000 puts with TTL
Expected: < 6µs mean (slight overhead for TTL)
```

#### 1.2 KV Throughput

```
Test: kv_sustained_write_throughput
Setup: Single thread, single RunId
Measure: Ops/sec over 100K puts
Expected: > 100K ops/sec

Test: kv_sustained_read_throughput
Setup: Pre-populate 10K keys
Measure: Ops/sec over 100K gets
Expected: > 200K ops/sec

Test: kv_mixed_workload_throughput
Setup: Pre-populate 5K keys
Measure: 50% reads, 50% writes over 50K ops
Expected: > 80K ops/sec
```

#### 1.3 KV Concurrent Access

```
Test: kv_disjoint_run_scaling
Setup: 4 threads, each with unique RunId
Measure: Scaling factor vs single thread
Expected: > 2.5× scaling

Test: kv_same_run_contention
Setup: 4 threads, same RunId, different keys
Measure: Throughput vs single thread
Expected: > 1.5× throughput (some contention expected)

Test: kv_hot_key_contention
Setup: 4 threads, same RunId, same key
Measure: All operations complete without deadlock
Expected: Operations complete, throughput may be low
```

---

### 2. EventLog Performance Tests

#### 2.1 EventLog Single-Operation Latency

```
Test: eventlog_append_latency
Setup: Create EventLog, single RunId
Measure: 10000 appends with small payloads
Expected: < 10µs mean (hash chain computation)

Test: eventlog_get_latency
Setup: Pre-populate 1000 events
Measure: 10000 gets by sequence number
Expected: < 3µs mean

Test: eventlog_range_latency
Setup: Pre-populate 1000 events
Measure: 1000 range queries (10 events each)
Expected: < 20µs mean
```

#### 2.2 EventLog Throughput

```
Test: eventlog_sustained_append_throughput
Setup: Single thread, single RunId
Measure: Ops/sec over 50K appends
Expected: > 50K ops/sec

Test: eventlog_hash_chain_overhead
Setup: Compare append with/without hash verification
Measure: Overhead percentage
Expected: < 30% overhead from hash chain
```

#### 2.3 EventLog Concurrent Access

```
Test: eventlog_disjoint_run_scaling
Setup: 4 threads, each with unique RunId
Measure: Scaling factor vs single thread
Expected: > 2.5× scaling

Test: eventlog_same_run_append_serialization
Setup: 4 threads, same RunId, appending
Measure: Events maintain correct sequence
Expected: Sequence numbers monotonic, no gaps
```

---

### 3. StateCell Performance Tests

#### 3.1 StateCell Single-Operation Latency

```
Test: statecell_init_latency
Setup: Create StateCell, single RunId
Measure: 1000 inits (different cell names)
Expected: < 5µs mean

Test: statecell_get_latency
Setup: Pre-populate 100 cells
Measure: 10000 gets
Expected: < 2µs mean

Test: statecell_set_latency
Setup: Pre-populate 100 cells
Measure: 10000 sets
Expected: < 5µs mean

Test: statecell_cas_latency
Setup: Pre-populate 100 cells
Measure: 10000 CAS operations (no contention)
Expected: < 8µs mean

Test: statecell_transition_latency
Setup: Pre-populate 100 cells
Measure: 1000 transitions with simple closure
Expected: < 15µs mean
```

#### 3.2 StateCell CAS Contention

```
Test: statecell_cas_retry_behavior
Setup: 4 threads, same cell, concurrent CAS
Measure: Retry count distribution
Expected: Mean retries < 3, max < 10

Test: statecell_cas_fairness
Setup: 4 threads, same cell, 1000 CAS each
Measure: Success distribution across threads
Expected: Each thread wins 20-30% of time

Test: statecell_transition_retry_overhead
Setup: 4 threads, same cell, concurrent transitions
Measure: Total time vs (threads × single-thread time)
Expected: < 5× overhead
```

---

### 4. TraceStore Performance Tests

#### 4.1 TraceStore Single-Operation Latency

```
Test: tracestore_record_latency
Setup: Create TraceStore, single RunId
Measure: 10000 records (various trace types)
Expected: < 8µs mean

Test: tracestore_get_latency
Setup: Pre-populate 1000 traces
Measure: 10000 gets by trace_id
Expected: < 3µs mean

Test: tracestore_get_children_latency
Setup: Pre-populate tree (depth 3, branching 5)
Measure: 1000 get_children queries
Expected: < 30µs mean

Test: tracestore_get_by_type_latency
Setup: Pre-populate 1000 traces (mixed types)
Measure: 100 get_by_type queries
Expected: < 100µs mean (scan operation)
```

#### 4.2 TraceStore Hierarchy

```
Test: tracestore_deep_hierarchy_performance
Setup: Create trace tree depth 10
Measure: Record at each level
Expected: Depth does not affect record latency

Test: tracestore_wide_hierarchy_performance
Setup: Create trace tree width 100 at level 1
Measure: get_children at root
Expected: < 500µs for 100 children
```

---

### 5. RunIndex Performance Tests

#### 5.1 RunIndex Single-Operation Latency

```
Test: runindex_create_run_latency
Setup: Create RunIndex
Measure: 1000 run creations
Expected: < 10µs mean

Test: runindex_get_run_latency
Setup: Pre-populate 100 runs
Measure: 10000 gets
Expected: < 2µs mean

Test: runindex_update_status_latency
Setup: Pre-populate 100 runs
Measure: 1000 status updates
Expected: < 8µs mean

Test: runindex_list_by_status_latency
Setup: Pre-populate 1000 runs (mixed status)
Measure: 100 list_by_status queries
Expected: < 200µs mean (scan operation)
```

#### 5.2 RunIndex Scale

```
Test: runindex_many_runs_performance
Setup: Create 10000 runs
Measure: Create, get, update latencies
Expected: No degradation vs 100 runs

Test: runindex_tag_query_performance
Setup: 1000 runs with 1-5 tags each
Measure: Query by tag
Expected: < 500µs mean
```

---

## Test File Structure

```
tests/
  m4_semantic_equivalence/
    mod.rs                    # Multi-mode test harness
    statecell_semantics.rs    # StateCell invariants (highest priority)
    eventlog_semantics.rs     # EventLog invariants
    tracestore_semantics.rs   # TraceStore invariants
    kv_semantics.rs           # KVStore invariants
    runindex_semantics.rs     # RunIndex invariants
    cross_primitive.rs        # Cross-primitive invariants

  m4_primitive_performance/
    mod.rs                    # Performance test harness
    kv_performance.rs
    eventlog_performance.rs
    statecell_performance.rs
    tracestore_performance.rs
    runindex_performance.rs
```

## Utilities Needed

```rust
// === Semantic Testing ===

/// Run a test workload across all durability modes
fn test_across_modes<F, T>(name: &str, workload: F)
where
    F: Fn(Arc<Database>) -> T,
    T: PartialEq + Debug;

/// Create test database with specific mode
fn create_test_db(mode: DurabilityMode) -> Arc<Database>;

/// Assert semantic equivalence
fn assert_same_across_modes<T: PartialEq + Debug>(
    results: &[(DurabilityMode, T)],
    test_name: &str,
);

// === Performance Testing ===

/// Performance test result
struct PerfResult {
    operation: String,
    iterations: usize,
    mean_ns: u64,
    p50_ns: u64,
    p95_ns: u64,
    p99_ns: u64,
    ops_per_sec: f64,
}

/// Measure operation latency
fn measure_latency<F>(name: &str, iterations: usize, warmup: usize, f: F) -> PerfResult

/// Assert performance within threshold
fn assert_latency_under(result: &PerfResult, threshold_us: u64)
fn assert_throughput_above(result: &PerfResult, min_ops_per_sec: f64)
fn assert_tail_latency_ratio(result: &PerfResult, max_p99_to_mean: f64)
```

## Execution Notes

### Semantic Tests
1. Run with `cargo test --release --test m4_semantic_equivalence`
2. **Run first** - semantic failures are more critical than performance
3. No warmup needed - correctness doesn't vary with cache state
4. Failures are deterministic - investigate immediately

### Performance Tests
1. Run with `cargo test --release --test m4_primitive_performance -- --nocapture`
2. Run on quiet system (no heavy background processes)
3. Run multiple times to check for variance
4. If flaky, increase iteration counts
5. Document any environment-specific failures

## Success Criteria

### Semantic Tests (Hard Requirements)
- **All semantic tests must pass** - no exceptions
- Any semantic failure indicates a bug, not a tuning issue
- Semantic drift in StateCell CAS or EventLog hash chain is a ship-blocker

### Performance Tests (Soft Requirements)
All tests should pass for M4 to be considered complete. Any failure indicates:
1. A regression from M4 changes, OR
2. A pre-existing issue now exposed, OR
3. Test threshold too aggressive (adjust with justification)

## Priority Order

0. **Global Regression Sentinel** - Kill switch test (run first, always)
1. **ABA Detection tests** - Version chain + CAS correctness (highest risk from M4)
2. **Snapshot Monotonicity tests** - Version chain traversal bugs
3. **StateCell semantic tests** - CAS + MVCC interaction
4. **EventLog semantic tests** - Hash chain integrity must be preserved
5. **Cross-primitive atomicity** - Transaction boundaries must hold
6. **All other semantic tests**
7. **Performance tests** - Only after semantics verified

---

## Global Regression Sentinel (Kill Switch Test)

**Purpose**: Single test that exercises all primitives under stress. If this fails, something is catastrophically wrong. Run this first, always.

### Test: `global_regression_sentinel`

```rust
/// Kill switch test - catches catastrophic regressions
///
/// If this test fails, DO NOT proceed with other tests.
/// Investigate immediately.
#[test]
fn global_regression_sentinel() {
    // === Configuration ===
    const OPS_PER_PRIMITIVE: usize = 1000;
    const TIMEOUT_SECS: u64 = 30;

    // === Phase 1: Single Thread Baseline ===
    let (single_thread_time, single_thread_ops) = run_mixed_workload(
        num_threads: 1,
        ops_per_primitive: OPS_PER_PRIMITIVE,
        disjoint_runs: false,
    );

    // === Phase 2: 4 Threads, Same Run (Contention) ===
    let (contention_time, contention_ops) = run_mixed_workload(
        num_threads: 4,
        ops_per_primitive: OPS_PER_PRIMITIVE,
        disjoint_runs: false,
    );

    // === Phase 3: 4 Threads, Disjoint Runs (Scaling) ===
    let (disjoint_time, disjoint_ops) = run_mixed_workload(
        num_threads: 4,
        ops_per_primitive: OPS_PER_PRIMITIVE,
        disjoint_runs: true,
    );

    // === Assertions ===
    // These are intentionally loose - we're catching catastrophic failures

    // 1. No timeout (deadlock detection)
    assert!(single_thread_time < Duration::from_secs(TIMEOUT_SECS),
        "KILL SWITCH: Single thread timed out - possible deadlock");
    assert!(contention_time < Duration::from_secs(TIMEOUT_SECS),
        "KILL SWITCH: Contention test timed out - possible deadlock");
    assert!(disjoint_time < Duration::from_secs(TIMEOUT_SECS),
        "KILL SWITCH: Disjoint test timed out - possible deadlock");

    // 2. No throughput collapse (> 10% of single thread)
    let contention_throughput = contention_ops as f64 / contention_time.as_secs_f64();
    let single_throughput = single_thread_ops as f64 / single_thread_time.as_secs_f64();
    assert!(contention_throughput > single_throughput * 0.1,
        "KILL SWITCH: Throughput collapsed under contention");

    // 3. Disjoint should scale (not regress)
    let disjoint_throughput = disjoint_ops as f64 / disjoint_time.as_secs_f64();
    assert!(disjoint_throughput > single_throughput * 0.5,
        "KILL SWITCH: Disjoint runs slower than single thread");

    // 4. p99 sanity (checked inside run_mixed_workload)
    // Fails internally if p99 > 100× mean for any primitive
}

fn run_mixed_workload(
    num_threads: usize,
    ops_per_primitive: usize,
    disjoint_runs: bool,
) -> (Duration, usize) {
    let db = Database::builder().in_memory().open_temp().unwrap();
    let db = Arc::new(db);

    let start = Instant::now();
    let total_ops = AtomicUsize::new(0);
    let mut latencies: Mutex<Vec<u128>> = Mutex::new(Vec::new());

    let handles: Vec<_> = (0..num_threads).map(|thread_id| {
        let db = Arc::clone(&db);
        let latencies = &latencies;
        let total_ops = &total_ops;

        thread::spawn(move || {
            let run_id = if disjoint_runs {
                RunId::new()  // Each thread gets unique run
            } else {
                RunId::from_u128(1)  // All threads share run
            };

            let kv = KVStore::new(db.clone());
            let events = EventLog::new(db.clone());
            let state = StateCell::new(db.clone());
            let traces = TraceStore::new(db.clone());
            let runs = RunIndex::new(db.clone());

            // Initialize StateCell
            state.init(&run_id, "counter", Value::I64(0)).ok();

            for i in 0..ops_per_primitive {
                let op_start = Instant::now();

                // Round-robin across primitives
                match i % 5 {
                    0 => {
                        // KVStore: put + get
                        kv.put(&run_id, &format!("key_{}", i), Value::I64(i as i64)).unwrap();
                        let _ = kv.get(&run_id, &format!("key_{}", i));
                    }
                    1 => {
                        // EventLog: append
                        events.append(&run_id, "test_event", Value::I64(i as i64)).unwrap();
                    }
                    2 => {
                        // StateCell: transition (increment)
                        let _ = state.transition(&run_id, "counter", |v| {
                            if let Value::I64(n) = v {
                                Ok(Value::I64(n + 1))
                            } else {
                                Ok(v)
                            }
                        });
                    }
                    3 => {
                        // TraceStore: record
                        traces.record(&run_id, TraceType::Tool, None, Value::I64(i as i64)).unwrap();
                    }
                    4 => {
                        // RunIndex: create + update (less frequent)
                        if i % 100 == 4 {
                            let meta = runs.create_run(&format!("run_{}_{}", thread_id, i)).unwrap();
                            runs.update_status(&meta.run_id, RunStatus::Active).ok();
                        }
                    }
                    _ => unreachable!(),
                }

                let elapsed = op_start.elapsed().as_nanos();
                latencies.lock().unwrap().push(elapsed);
                total_ops.fetch_add(1, Ordering::Relaxed);
            }
        })
    }).collect();

    // Wait for all threads (with timeout)
    for h in handles {
        h.join().expect("KILL SWITCH: Thread panicked");
    }

    let elapsed = start.elapsed();
    let ops = total_ops.load(Ordering::Relaxed);

    // Check p99/mean ratio
    let mut lats = latencies.into_inner().unwrap();
    if !lats.is_empty() {
        lats.sort();
        let mean = lats.iter().sum::<u128>() / lats.len() as u128;
        let p99 = lats[lats.len() * 99 / 100];
        let ratio = p99 as f64 / mean.max(1) as f64;

        assert!(ratio < 100.0,
            "KILL SWITCH: p99/mean = {:.1}× > 100× threshold", ratio);
    }

    (elapsed, ops)
}
```

### What This Test Catches

| Failure Mode | Symptom |
|--------------|---------|
| Deadlock | Timeout (30s) |
| Panic | Thread join fails |
| Livelock | Throughput collapse |
| Lock contention explosion | p99 > 100× mean |
| Broken scaling | Disjoint slower than single |
| MVCC corruption | Panic in primitive ops |
| Memory corruption | Panic or undefined behavior |

### When to Run

- **Before every commit** (part of CI)
- **First test in any manual run**
- **After any M4 infrastructure change**

### If This Test Fails

1. **Stop** - Do not run other tests
2. **Check for panics** - Look at thread join failures
3. **Check for deadlocks** - Which phase timed out?
4. **Check throughput** - Which phase collapsed?
5. **Bisect** - Find the commit that broke it

## Non-Goals

- Micro-optimization based on these results
- Comparison with other databases
- Production workload simulation
- Memory usage profiling (separate concern)
- Testing recovery semantics (covered by existing M2 tests)
