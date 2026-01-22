# M3 Comprehensive Test Suite Plan

**Version**: 1.2
**Status**: Planning
**Date**: 2026-01-14

---

## Overview

This document outlines the plan for adding M3 (Primitives) comprehensive tests to complement the existing `m1_m2_comprehensive` test suite. The M3 tests validate the five primitives (KVStore, EventLog, StateCell, TraceStore, RunIndex) and their integration patterns.

### Relationship to Existing Tests

| Test Location | Purpose |
|---------------|---------|
| `tests/m1_m2_comprehensive/` | ACID properties, WAL invariants, Snapshot Isolation (M1-M2) |
| `crates/primitives/tests/` | Per-primitive unit tests and basic integration tests |
| **NEW**: `tests/m3_comprehensive/` | M3 invariant tests, stress tests, multi-primitive scenarios |

### Layer Separation Principle

**CRITICAL**: M3 tests must NOT re-test M1/M2 invariants.

| Layer | Tests | M3 Test Stance |
|-------|-------|----------------|
| M1 | WAL serialization, storage ordering | **Assume correct** |
| M2 | Snapshot isolation, OCC, conflict detection | **Assume correct** |
| M3 | Primitive semantics, cross-primitive atomicity | **Test here** |

**Good M3 test**: "EventLog reads are snapshot-consistent with KV writes in the same transaction"
**Bad M3 test**: "Snapshot isolation is consistent across reads" (this is M2)

**Good M3 test**: "If a multi-primitive transaction commits, all primitives reflect it after recovery"
**Bad M3 test**: "WAL entries are serialized correctly" (this is M1)

---

## Glossary

| Term | Definition |
|------|------------|
| **Facade** | A primitive handle (e.g., `KVStore`) that holds no state except `Arc<Database>`. Multiple facades share underlying engine state. |
| **TypeTag** | Internal discriminator that isolates key namespaces across primitives. KV keys never collide with EventLog keys. |
| **Run** | A logical execution context identified by `RunId`. All primitive operations are scoped to a run. |
| **CAS** | Compare-And-Swap. StateCell operation that updates value only if current version matches expected version. |
| **Snapshot Isolation** | M2 guarantee that a transaction sees a consistent snapshot of data as of its start time. |
| **OCC** | Optimistic Concurrency Control. M2 conflict detection mechanism; transactions retry on conflict. |
| **Commit Serialization Order** | The order in which transactions successfully commit, which may differ from wall-clock order. |
| **Invariant** | A property that must always hold. Tier 1 tests verify invariants; failure indicates a bug. |
| **Substrate** | The M1/M2 layers that M3 builds upon. M3 assumes substrate correctness. |

---

## Non-Goals

This test suite explicitly does **NOT** test:

1. **WAL correctness** - Covered by M1 tests
2. **Snapshot isolation semantics** - Covered by M2 tests
3. **OCC conflict detection** - Covered by M2 tests
4. **Storage ordering guarantees** - Covered by M1 tests
5. **Real-time timestamp ordering** - Not guaranteed by design
6. **Single-writer enforcement** - EventLog allows concurrent appends; result is serialized
7. **Closure purity enforcement** - Cannot be enforced; only documented
8. **Cross-run transactions** - Each transaction operates on a single run
9. **Distributed consistency** - Single-node system only

If a test appears to be testing one of these, it should be removed or reframed.

---

## Test Tier Structure

Following the same tier pattern as `m1_m2_comprehensive`:

### Tier 1: Core Invariants (sacred, fast, must pass)

Run on every commit. Enforce M3-specific invariants.

| Test Module | Invariants Covered |
|-------------|-------------------|
| `primitive_invariant_tests.rs` | M3.1-M3.6 (facade identity, key isolation, no hidden writes) |
| `eventlog_chain_tests.rs` | M3.7-M3.11 (append-only, monotonic sequences, chain integrity) |
| `statecell_cas_tests.rs` | M3.12-M3.15 (version monotonicity, CAS correctness) |
| `runindex_lifecycle_tests.rs` | M3.16-M3.20 (status transitions, no resurrection) |
| `substrate_invariant_tests.rs` | M3.21-M3.24 (canonical source, ordering, replay contract, no coupling) |

### Tier 2: Behavioral Scenarios (medium, workflow tests)

Run on every commit. Test complete primitive workflows.

| Test Module | Scenarios Covered |
|-------------|------------------|
| `primitive_api_tests.rs` | KVStore, EventLog, StateCell, TraceStore, RunIndex APIs |
| `cross_primitive_transaction_tests.rs` | Multi-primitive atomic operations |
| `run_isolation_comprehensive_tests.rs` | N-run isolation verification |
| `recovery_comprehensive_tests.rs` | All primitives survive crash+recovery |
| `index_consistency_tests.rs` | TraceStore and RunIndex secondary indices |
| `edge_case_tests.rs` | Boundary conditions for all primitives |

### Tier 3: Stress/Chaos (opt-in, slow)

NOT run on every commit. Find rare bugs.

| Test Module | Focus |
|-------------|-------|
| `concurrent_primitive_stress_tests.rs` | Multi-threaded primitive operations |
| `run_lifecycle_stress_tests.rs` | Many runs, many status transitions |
| `eventlog_chain_stress_tests.rs` | Long chains, high-throughput appends |

---

## Detailed Test Specifications

### 1. Primitive Invariant Tests (`primitive_invariant_tests.rs`)

**M3.1: TypeTag Isolation**
- Keys with different TypeTags are completely isolated
- KV keys never visible to EventLog, StateCell, etc.
- Cross-primitive key collision is impossible

*What breaks if this fails?* Cross-primitive data corruption. A KV `get("foo")` could return an EventLog entry. Complete data integrity failure.

```rust
#[test]
fn test_typetag_isolation() {
    // Write to KV with key "data"
    // Write to Event with "data" (as event_type)
    // Write to State with "data" (as cell name)
    // Verify each primitive sees only its own data
}
```

**M3.2: Run Namespace Isolation**
- Same key in different runs are independent
- List operations scoped to single run
- No cross-run data leakage

*What breaks if this fails?* Multi-tenant data leak. Agent run A could see data from Agent run B. Security and correctness failure.

**M3.3: Facade Identity Invariant**
- Primitives are facades over shared engine state, not stateful caches
- Creating, dropping, recreating a primitive handle must not affect visibility
- No in-memory cache tied to primitive instance lifetime

*What breaks if this fails?* Memory leaks or data loss. If primitives cache state, dropping a handle could lose uncommitted data or leak memory.

```rust
#[test]
fn test_facade_identity() {
    let kv1 = KVStore::new(db.clone());
    kv1.put(&run_id, "key", Value::I64(42)).unwrap();
    drop(kv1);  // Drop the primitive handle

    let kv2 = KVStore::new(db.clone());  // New handle
    assert_eq!(kv2.get(&run_id, "key").unwrap(), Some(Value::I64(42)));
    // Data visible through new handle - proves no instance-local state
}
```

**M3.4: Value Type Safety**
- Values stored and retrieved maintain type fidelity
- I64, String, Bool, Null, Array, Object all round-trip

*What breaks if this fails?* Silent data corruption. Store an I64, get back a String. Application logic fails unpredictably.

**M3.5: Deterministic Key Ordering**
- Primitives return keys in deterministic order (lexicographic by byte)
- This tests that M3 layer preserves M1 ordering—not that ordering works
- Range scans return results in consistent, reproducible order
- Test via multiple reads returning same order, not via comparison to expected order

*What breaks if this fails?* Non-deterministic iteration. Same query returns different order on different calls. Pagination breaks; debugging becomes impossible.

**M3.6: No Hidden Writes Invariant**
- Primitives must not write outside of transaction boundaries
- Aborted transactions leave no trace

*What breaks if this fails?* Atomicity violation. Failed transactions leave partial state. Rollback is incomplete. Data corruption.

```rust
#[test]
fn test_no_hidden_writes() {
    // Start transaction
    // Call EventLog.append via extension trait
    // Abort transaction (return Err)
    // Assert: no event exists
    // Assert: sequence number not consumed
}
```

---

### 2. EventLog Chain Tests (`eventlog_chain_tests.rs`)

**M3.7: Append-Only Invariant**
- `update()` and `delete()` return error (if exposed)
- Events cannot be modified after append
- Sequence numbers are immutable

*What breaks if this fails?* Audit log tampering. Events can be silently modified. Chain integrity is meaningless.

**M3.8: Monotonic Sequence Numbers**
- Sequences are contiguous (0, 1, 2, ...)
- No gaps after transaction failure
- Sequence numbers never reused

*What breaks if this fails?* Lost events or duplicate events. Sequence gaps indicate missing data. Sequence reuse causes event collision.

```rust
#[test]
fn test_sequence_contiguity_after_failure() {
    // Append event 0
    // Start transaction, append event 1, FAIL transaction
    // Append event 1 again - should get sequence 1 (not 2)
    // Verify: 0, 1 - no gap
}
```

**M3.9: Hash Chain Integrity**
- Each event's prev_hash matches previous event's hash
- Chain verification passes for valid chains
- Chain verification detects corruption

*What breaks if this fails?* Undetected corruption. Events can be inserted, deleted, or reordered without detection. Audit trail is unreliable.

**M3.10: Total Order Under Concurrency**
- Concurrent appends serialize to a total order
- Final sequence order matches serialization order
- No parallel appends to same run's log (by design)

Note: This is NOT "single writer enforcement" - multiple threads CAN attempt appends. The invariant is that the result is totally ordered.

**Important**: Order reflects commit serialization order, not real-time or thread scheduling order. If thread A calls append before thread B in wall-clock time, but B commits first, B gets the lower sequence number.

*What breaks if this fails?* Non-deterministic sequence assignment. Same concurrent scenario produces different orderings. Replay is non-deterministic.

**M3.11: Metadata Consistency**
- `len()` matches actual event count
- `head()` returns most recent event
- Metadata survives recovery

*What breaks if this fails?* Off-by-one errors everywhere. `len()` disagrees with actual count. `head()` returns wrong event. Iteration bounds are wrong.

---

### 3. StateCell CAS Tests (`statecell_cas_tests.rs`)

**M3.12: Version Monotonicity**
- Versions always increase (1, 2, 3, ...)
- CAS cannot set lower version
- set() increments version atomically

*What breaks if this fails?* ABA problem. Old version can mask intervening writes. CAS succeeds when it should fail.

**M3.13: CAS Atomicity**
- Only one concurrent CAS succeeds per version
- Losing CAS sees correct version for retry
- No lost updates

*What breaks if this fails?* Lost updates. Two concurrent increments both succeed, but counter only goes up by 1. Race condition.

```rust
#[test]
fn test_cas_lost_update_prevention() {
    // N threads try to increment counter via CAS
    // Final value == N (no lost updates)
}
```

**M3.14: Init Uniqueness**
- `init()` fails if cell already exists
- Second `init()` returns error, not overwrite
- Use CAS for updates after init

*What breaks if this fails?* Silent overwrites. `init()` clobbers existing state. Data loss without error.

**M3.15: Transition Speculative Execution**
- `transition()` closure may be re-executed on OCC conflict
- The system does NOT guarantee single invocation
- Closure must be treated as pure (side effects will be multiplied)

```rust
#[test]
fn test_transition_reexecution() {
    let call_count = Arc::new(AtomicU64::new(0));

    // Create contention to force retries
    // ...

    sc.transition(run_id, "cell", |state| {
        call_count.fetch_add(1, Ordering::Relaxed);
        // Pure computation only
        Ok((new_value, result))
    });

    // call_count may be > 1 due to retries
    // Final result is still correct
}
```

Note: We cannot "enforce" purity. We document and test the consequences of impurity.

*What breaks if this fails?* Side effects multiply. If closure sends HTTP request, request sent N times. If closure writes file, file has garbage. Non-idempotent operations corrupt external state.

---

### 4. RunIndex Lifecycle Tests (`runindex_lifecycle_tests.rs`)

**M3.16: Valid Status Transitions**
- Active -> Completed/Failed/Cancelled/Paused/Archived (valid)
- Paused -> Active/Cancelled/Archived (valid)
- Terminal -> Archived only (valid)

*What breaks if this fails?* Invalid state machine. Completed run transitions to Active. Run lifecycle is meaningless.

**M3.17: No Resurrection**
- Completed -> Active (error)
- Failed -> Active (error)
- Cancelled -> Active (error)

*What breaks if this fails?* Zombie runs. Completed runs restart. Audit trails are invalid. Billing/accounting is wrong.

**M3.18: Archived is Terminal**
- Archived -> any other status (error)
- Data still accessible after archive
- Archive is soft delete

*What breaks if this fails?* Archived runs revive. "Deleted" data comes back. Storage guarantees are broken.

**M3.19: Cascading Delete**
- `delete_run()` removes all primitive data
- KV, Events, States, Traces all deleted
- Other runs unaffected

*What breaks if this fails?* Orphaned data. Deleted run leaves behind KV entries, events, traces. Storage leak. Potential data resurrection.

```rust
#[test]
fn test_cascading_delete_removes_all_data() {
    // Create run, write to all 5 primitives
    // Delete run via RunIndex
    // Verify: all KV, Events, States, Traces GONE
    // Verify: metadata GONE
}
```

**M3.20: Status Updates Are Transactional**
- RunIndex status updates are WAL-backed
- Status changes are atomic with other operations in same transaction
- Recovery preserves last committed status

*What breaks if this fails?* Status/data inconsistency. Run shows "Completed" but data is partial. Recovery resurrects wrong status.

```rust
#[test]
fn test_status_update_transactional() {
    // Create run (Active)
    // In transaction: update status to Completed, write KV
    // Crash before commit
    // Recover: status is Active, KV not present (both rolled back)
}
```

---

### 5. Primitive API Tests (`primitive_api_tests.rs`)

Comprehensive API coverage for each primitive:

**KVStore API**
- `get()`/`put()`/`delete()` - basic CRUD
- `put_with_ttl()` - TTL expiration (if implemented)
- `list()` - all keys
- `list()` with prefix - filtered keys
- `list_with_values()` - key-value pairs

**EventLog API**
- `append()` - returns (sequence, hash)
- `read()` - single event by sequence
- `read_range()` - range of events
- `head()` - most recent event
- `len()` - event count
- `iter()` - iterate all events
- `verify_chain()` - chain integrity check
- `read_by_type()` - filter by event_type

**StateCell API**
- `init()` - create new cell
- `read()` - get current state
- `cas()` - compare-and-swap
- `set()` - unconditional set
- `delete()` - remove cell
- `list()` - all cell names
- `exists()` - check existence
- `transition()` - closure-based update

**TraceStore API**
- `record()` - create trace
- `record_child()` - nested trace
- `record_with_options()` - custom ID, tags, metadata
- `get()` - get trace by ID
- `query_by_type()` - filter by trace type
- `query_by_tag()` - filter by tag
- `query_by_time()` - time range query
- `get_children()` - direct children
- `get_tree()` - recursive tree
- `list()` - all trace IDs
- `count()` - trace count

**RunIndex API**
- `create_run()` - new run
- `create_run_with_options()` - with parent, tags, metadata
- `get_run()` - get run metadata
- `update_status()` - status transition
- `fail_run()` - fail with error
- `complete_run()` - mark complete
- `add_tags()` - add tags
- `update_metadata()` - update metadata
- `query_runs()` - query with filters
- `list_runs()` - all run IDs
- `get_child_runs()` - forked runs
- `delete_run()` - cascading delete
- `archive_run()` - soft delete
- `get_stats()` - run statistics

---

### 5.5. Substrate Invariant Tests (`substrate_invariant_tests.rs`)

These tests cement the M3 architectural philosophy.

**M3.21: Primitives Are Projections Over KV (Canonical Source)**

All M3 primitives ultimately store data as key-value pairs. This test proves reconstructability.

*What breaks if this fails?* Non-reconstructable state. If primitives have hidden state, recovery is impossible. Backup/restore breaks.

```rust
#[test]
fn test_primitives_rebuildable_from_storage() {
    // Write using EventLog, StateCell, TraceStore
    // Get raw keys from storage (direct Database access)
    // Rebuild secondary indexes from primary data
    // Verify: derived state matches original primitive queries
}
```

**M3.22: Cross-Primitive Ordering Consistency**

Operations within a single transaction form a consistent snapshot. No real-time timestamp ordering is guaranteed across transactions.

```rust
#[test]
fn test_cross_primitive_ordering() {
    db.transaction(run_id, |txn| {
        txn.kv_put("step", Value::I64(1))?;           // Op 1
        txn.event_append("started", Value::Null)?;    // Op 2
        txn.state_set("phase", Value::I64(1))?;       // Op 3
        txn.trace_record("Thought", Value::Null)?;    // Op 4
        Ok(())
    })?;

    // Read back and assert snapshot consistency is preserved
    // All operations visible atomically (all-or-nothing)
    // No partial visibility across primitives
}
```

**Note**: We do NOT guarantee real-time ordering across transactions. If T1 commits before T2 in wall-clock time, we do not guarantee T1's timestamp < T2's timestamp. The guarantee is snapshot consistency within transactions, not real-time causality.

*What breaks if this fails?* Partial visibility. Some primitive operations visible, others not. Inconsistent cross-primitive reads within one transaction.

**M3.23: Replay Metadata Contract (M5 Forward Compatibility)**

Even though replay is M5, lock in the schema now.

```rust
#[test]
fn test_replay_metadata_present() {
    // Write to all primitives
    // Assert: EventLog stores enough metadata for replay
    //   - sequence numbers
    //   - timestamps
    //   - event_type
    //   - prev_hash (for chain verification)
    // Assert: TraceStore stores enough metadata for debugging
    //   - trace_type
    //   - parent_id
    //   - timestamp
    // This is a SCHEMA test, not a BEHAVIOR test
}
```

*What breaks if this fails?* M5 replay impossible. Events lack sequence numbers or timestamps. Traces lack parent IDs. Replay cannot reconstruct execution order.

**M3.24: No Implicit Coupling Between Primitives**

Primitives operate independently—no primitive operation implicitly triggers another primitive's operation.

```rust
#[test]
fn test_no_implicit_coupling() {
    // EventLog append does NOT auto-record a trace
    // StateCell transition does NOT auto-append an event
    // KV put does NOT auto-update any index outside KV
    // Each primitive operation affects ONLY that primitive's storage

    let event_count_before = event_log.len(&run_id)?;
    let trace_count_before = trace_store.count(&run_id)?;

    kv.put(&run_id, "key", Value::I64(42))?;

    // No side effects on other primitives
    assert_eq!(event_log.len(&run_id)?, event_count_before);
    assert_eq!(trace_store.count(&run_id)?, trace_count_before);
}
```

*What breaks if this fails?* Hidden dependencies. KV write triggers unexpected event. Debugging becomes impossible because operations have non-local effects.

---

### 6. Cross-Primitive Transaction Tests (`cross_primitive_transaction_tests.rs`)

**Atomic Multi-Primitive Operations**
```rust
#[test]
fn test_atomic_kv_event_state_trace() {
    // Single transaction:
    // - KV put
    // - Event append
    // - State CAS
    // - Trace record
    // All succeed or all fail
}
```

**Cross-Primitive Rollback**
```rust
#[test]
fn test_cross_primitive_rollback() {
    // KV put (success)
    // Event append (success)
    // State CAS with wrong version (FAIL)
    // Verify: KV and Event also rolled back
}
```

**Extension Trait Composition**
```rust
#[test]
fn test_extension_traits_compose() {
    // Use KVStoreExt, EventLogExt, StateCellExt, TraceStoreExt
    // in single transaction via db.transaction()
}
```

**Read-Your-Writes in Transaction**
```rust
#[test]
fn test_read_your_writes_cross_primitive() {
    // KV put -> KV get (same transaction)
    // Event append -> Event read (same transaction)
    // State set -> State read (same transaction)
}
```

**Cross-Primitive Snapshot Consistency**

Note: This tests that primitives USE snapshot isolation correctly (M3), not that snapshot isolation WORKS (M2).

```rust
#[test]
fn test_cross_primitive_snapshot_consistency() {
    // T1: Read KV, Event, State in one transaction
    // T2 (concurrent): Modify all three
    // T1: Read again - must see SAME values (snapshot)
    // This tests primitive integration with M2, not M2 itself
}
```

---

### 7. Run Isolation Comprehensive Tests (`run_isolation_comprehensive_tests.rs`)

**N-Run Isolation**
```rust
#[test]
fn test_100_run_isolation() {
    // Create 100 runs
    // Each run writes to all 5 primitives
    // Verify each run sees only its own data
    // No cross-run leakage
}
```

**Concurrent Run Operations**
```rust
#[test]
fn test_concurrent_run_operations() {
    // N threads, each operating on different run
    // All operations succeed
    // No interference between runs
}
```

**Run Delete Isolation**
```rust
#[test]
fn test_run_delete_isolation() {
    // Create run A and B
    // Write to both
    // Delete run A
    // Verify: run B data untouched
}
```

---

### 8. Recovery Comprehensive Tests (`recovery_comprehensive_tests.rs`)

Building on existing `crates/primitives/tests/recovery_tests.rs`.

**Atomicity Domain**: Recovery atomicity is the *transaction*. M3 tests verify that multi-primitive transactions recover atomically (all primitives or none), relying on M1/M2 WAL recovery. We do NOT re-test WAL correctness—we test that M3 layer correctly participates in recovery.

**Multi-Primitive Recovery**
```rust
#[test]
fn test_all_primitives_recover_atomically() {
    // Write to all 5 primitives in single transaction
    // Crash
    // Recover
    // Verify: all data present OR none present (atomic)
    // NOTE: Atomicity domain is the transaction, not individual primitives
}
```

**Sequence Continuity After Recovery**
```rust
#[test]
fn test_eventlog_sequence_continues_after_recovery() {
    // Append events 0, 1, 2
    // Crash
    // Recover
    // Append event - should get sequence 3 (not 0)
}
```

**CAS Version Continuity After Recovery**
```rust
#[test]
fn test_statecell_version_continues_after_recovery() {
    // Init cell (version 1)
    // CAS multiple times (version 2, 3, 4)
    // Crash
    // Recover
    // CAS with version 4 succeeds, version 3 fails
}
```

**Index Recovery**
```rust
#[test]
fn test_tracestore_indices_survive_recovery() {
    // Record traces with tags
    // Crash
    // Recover
    // Query by tag returns correct results
}
```

**Multiple Recovery Cycles**
```rust
#[test]
fn test_multiple_recovery_cycles() {
    // Cycle 1: Write data
    // Cycle 2: Recover, write more, verify all
    // Cycle 3: Recover, write more, verify all
    // ...N cycles
}
```

---

### 9. Index Consistency Tests (`index_consistency_tests.rs`)

**TraceStore Index Consistency**
```rust
#[test]
fn test_trace_type_index_consistency() {
    // Record 100 traces of mixed types
    // Query by type
    // Verify: count matches direct list count
}

#[test]
fn test_trace_parent_child_index_consistency() {
    // Create tree of traces
    // get_children() matches direct parent_id checks
}
```

**RunIndex Index Consistency**
```rust
#[test]
fn test_run_status_index_consistency() {
    // Create runs with various statuses
    // Update statuses
    // query_by_status() returns correct runs
}

#[test]
fn test_run_tag_index_consistency() {
    // Create runs with tags
    // Add more tags
    // Query by tag returns correct runs
}
```

---

### 10. Edge Case Tests (`edge_case_tests.rs`)

**Empty State Tests**
- Get from non-existent KV key
- Read non-existent event sequence
- Read non-existent StateCell
- Get non-existent trace
- Get non-existent run

**Boundary Values**
- Max key length
- Max value size
- Max event sequence number
- Max trace tree depth
- Zero-length values

**Unicode and Special Characters**
- Unicode keys and values
- Binary data in values
- Empty strings
- Null bytes in strings

**Concurrent Edge Cases**
- Many transactions on same key
- High-contention CAS
- Rapid status transitions

---

### 11. Concurrent Primitive Stress Tests (`concurrent_primitive_stress_tests.rs`)

**KVStore Stress**
```rust
#[test]
#[ignore]
fn stress_kv_concurrent_writes() {
    // 100 threads, 1000 writes each
    // Verify: all writes committed or properly conflicted
}
```

**EventLog Stress**
```rust
#[test]
#[ignore]
fn stress_eventlog_concurrent_appends() {
    // 50 threads, 100 appends each
    // Verify: all sequences contiguous
    // Verify: chain integrity
}
```

**StateCell Stress**
```rust
#[test]
#[ignore]
fn stress_statecell_concurrent_cas() {
    // 100 threads, increment counter via CAS
    // Verify: final value == number of successful CAS
    // No lost updates
}
```

**Cross-Primitive Stress**
```rust
#[test]
#[ignore]
fn stress_cross_primitive_transactions() {
    // 50 threads, each doing multi-primitive transactions
    // Verify: all transactions atomic
}
```

---

### 12. Run Lifecycle Stress Tests (`run_lifecycle_stress_tests.rs`)

**Many Runs**
```rust
#[test]
#[ignore]
fn stress_1000_runs() {
    // Create 1000 runs
    // Write data to each
    // Query operations still fast
    // Memory usage reasonable
}
```

**Rapid Status Transitions**
```rust
#[test]
#[ignore]
fn stress_rapid_status_transitions() {
    // Create runs
    // Rapidly transition: Active -> Paused -> Active -> Completed
    // All transitions valid
}
```

**Cascading Delete Performance**
```rust
#[test]
#[ignore]
fn stress_cascading_delete_large_run() {
    // Create run with 10000 keys, 1000 events, 500 traces
    // Delete run
    // Verify: all data gone
    // Measure time
}
```

---

### 13. EventLog Chain Stress Tests (`eventlog_chain_stress_tests.rs`)

**Long Chain**
```rust
#[test]
#[ignore]
fn stress_long_event_chain() {
    // Append 10000 events
    // Verify chain integrity
    // Measure: verification time
}
```

**Chain Verification Performance**
```rust
#[test]
#[ignore]
fn stress_chain_verification_performance() {
    // Various chain lengths: 100, 1000, 10000
    // Verify: time scales linearly
}
```

**Recovery After Long Chain**
```rust
#[test]
#[ignore]
fn stress_recovery_long_chain() {
    // Append 5000 events
    // Crash
    // Recover
    // Append continues correctly
    // Chain still valid
}
```

---

## Test Invariant Summary

### M3 Invariants

| ID | Invariant | Primitive | Test Module |
|----|-----------|-----------|-------------|
| M3.1 | TypeTag Isolation | All | `primitive_invariant_tests.rs` |
| M3.2 | Run Namespace Isolation | All | `primitive_invariant_tests.rs` |
| M3.3 | Facade Identity | All | `primitive_invariant_tests.rs` |
| M3.4 | Value Type Safety | All | `primitive_invariant_tests.rs` |
| M3.5 | Deterministic Key Ordering | All | `primitive_invariant_tests.rs` |
| M3.6 | No Hidden Writes | All | `primitive_invariant_tests.rs` |
| M3.7 | Append-Only | EventLog | `eventlog_chain_tests.rs` |
| M3.8 | Monotonic Sequences | EventLog | `eventlog_chain_tests.rs` |
| M3.9 | Hash Chain Integrity | EventLog | `eventlog_chain_tests.rs` |
| M3.10 | Total Order Under Concurrency | EventLog | `eventlog_chain_tests.rs` |
| M3.11 | Metadata Consistency | EventLog | `eventlog_chain_tests.rs` |
| M3.12 | Version Monotonicity | StateCell | `statecell_cas_tests.rs` |
| M3.13 | CAS Atomicity | StateCell | `statecell_cas_tests.rs` |
| M3.14 | Init Uniqueness | StateCell | `statecell_cas_tests.rs` |
| M3.15 | Transition Speculative Execution | StateCell | `statecell_cas_tests.rs` |
| M3.16 | Valid Status Transitions | RunIndex | `runindex_lifecycle_tests.rs` |
| M3.17 | No Resurrection | RunIndex | `runindex_lifecycle_tests.rs` |
| M3.18 | Archived is Terminal | RunIndex | `runindex_lifecycle_tests.rs` |
| M3.19 | Cascading Delete | RunIndex | `runindex_lifecycle_tests.rs` |
| M3.20 | Status Updates Transactional | RunIndex | `runindex_lifecycle_tests.rs` |
| M3.21 | Primitives Are Projections (Canonical Source) | Substrate | `substrate_invariant_tests.rs` |
| M3.22 | Cross-Primitive Ordering Consistency | Substrate | `substrate_invariant_tests.rs` |
| M3.23 | Replay Metadata Contract | Substrate | `substrate_invariant_tests.rs` |
| M3.24 | No Implicit Coupling Between Primitives | Substrate | `substrate_invariant_tests.rs` |

---

## File Structure

```
tests/
  m3_comprehensive/
    main.rs                              # Module declarations
    test_utils.rs                        # Shared test utilities

    # Tier 1: Core Invariants
    primitive_invariant_tests.rs         # M3.1-M3.6
    eventlog_chain_tests.rs              # M3.7-M3.11
    statecell_cas_tests.rs               # M3.12-M3.15
    runindex_lifecycle_tests.rs          # M3.16-M3.20
    substrate_invariant_tests.rs         # M3.21-M3.24

    # Tier 2: Behavioral Scenarios
    primitive_api_tests.rs               # API coverage
    cross_primitive_transaction_tests.rs # Multi-primitive atomic ops
    run_isolation_comprehensive_tests.rs # N-run isolation
    recovery_comprehensive_tests.rs      # Crash recovery
    index_consistency_tests.rs           # Secondary index tests
    edge_case_tests.rs                   # Boundary conditions

    # Tier 3: Stress/Chaos
    concurrent_primitive_stress_tests.rs # Multi-threaded stress
    run_lifecycle_stress_tests.rs        # Many runs stress
    eventlog_chain_stress_tests.rs       # Long chain stress
```

---

## Running Tests

```bash
# Run Tier 1 + Tier 2 (default, every commit)
cargo test --test m3_comprehensive

# Run only core invariants (Tier 1)
cargo test --test m3_comprehensive invariant

# Run only EventLog tests
cargo test --test m3_comprehensive eventlog

# Run only StateCell tests
cargo test --test m3_comprehensive statecell

# Run only RunIndex tests
cargo test --test m3_comprehensive runindex

# Run stress tests (Tier 3, opt-in)
cargo test --test m3_comprehensive stress -- --ignored

# Run all comprehensive tests (M1+M2+M3)
cargo test --test m1_m2_comprehensive && cargo test --test m3_comprehensive
```

---

## Implementation Notes

1. **Reuse test_utils from m1_m2_comprehensive** where applicable
2. **Add M3-specific helpers** for primitive setup
3. **Each test maps to one invariant** - no noise tests
4. **Tier 3 tests use `#[ignore]`** - not run by default
5. **Coverage goal**: 80%+ for primitives crate

---

## Success Criteria

- [ ] All Tier 1 invariant tests passing
- [ ] All Tier 2 scenario tests passing
- [ ] Tier 3 stress tests pass when run (opt-in)
- [ ] No test flakiness
- [ ] Tests complete in < 60 seconds (excluding Tier 3)
- [ ] Coverage > 80% for `crates/primitives/`

---

## Revision History

### v1.2 (2026-01-14)
Final refinements for semantic precision and completeness:

**New Sections**
- Added **Glossary** with definitions for Facade, TypeTag, Run, CAS, Snapshot Isolation, OCC, Commit Serialization Order, Invariant, Substrate
- Added **Non-Goals** section explicitly listing what this test suite does NOT test

**Invariant Clarifications**
- M3.5: Renamed "Key Ordering" → "Deterministic Key Ordering" to avoid M1 leakage; tests ordering preservation, not ordering correctness
- M3.10: Added note that order reflects commit serialization order, not real-time or thread scheduling order
- M3.22: Clarified that we do NOT guarantee real-time ordering across transactions; only snapshot consistency within transactions

**New Invariant**
- M3.24: "No Implicit Coupling Between Primitives" - primitive operations do not implicitly trigger other primitives

**Risk Mitigation Notes**
- Added "What breaks if this fails?" notes to all 24 Tier 1 invariants
- Added atomicity domain clarification to recovery tests section

### v1.1 (2026-01-14)
Incorporated feedback to tighten semantics and avoid M1/M2 scope creep:

**Layer Separation**
- Added explicit "Layer Separation Principle" section
- M3 tests MUST NOT re-test M1/M2 invariants
- Added good/bad test examples

**Renamed/Clarified Invariants**
- M3.3: "Primitive Statelessness" → "Facade Identity" (more precise)
- M3.10: "Single-Writer-Ordered" → "Total Order Under Concurrency" (correct semantics)
- M3.15: "Transition Purity Enforcement" → "Transition Speculative Execution" (we cannot enforce purity)
- M3.20: Added "Status Updates Are Transactional" (was implicit)

**New Invariants Added**
- M3.6: "No Hidden Writes" - primitives cannot write outside transactions
- M3.21: "Primitives Are Projections" (canonical source of truth)
- M3.22: "Cross-Primitive Ordering Consistency" (causal ordering)
- M3.23: "Replay Metadata Contract" (M5 forward compatibility)

**New Test Module**
- Added `substrate_invariant_tests.rs` for architectural invariants

**Cross-Primitive Clarification**
- Added "Cross-Primitive Snapshot Consistency" test with explicit note that this tests primitive integration with M2, not M2 itself

---

**Document Version**: 1.2
**Status**: Planning
**Date**: 2026-01-14
**Invariant Count**: 24 (M3.1-M3.24)
