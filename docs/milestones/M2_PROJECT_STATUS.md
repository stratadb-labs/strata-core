# M2 Project Status: Transactions

**Last Updated**: 2026-01-11

## Current Phase: M2 Planning Complete ✅

**M2 Transactions milestone planning is complete!** Architecture specification and diagrams have been created based on M1 foundation.

### M2 Planning Achievements
- ✅ **M2_ARCHITECTURE.md** complete (850 lines) - Full OCC specification
- ✅ **m2-architecture.md** complete (650 lines) - 10 detailed diagrams
- ✅ **Backwards compatibility** designed - M1 API still works
- ✅ **Trade-offs documented** - ClonedSnapshotView vs future optimizations
- ✅ **Performance targets** set - 10K ops/sec with conflicts, 500K reads/sec
- ✅ **Testing strategy** defined - Multi-threaded isolation tests

**Next**: Break down M2 into epics and user stories

---

## M2 Overview

**Goal**: Add Optimistic Concurrency Control (OCC) with snapshot isolation to enable multi-operation atomic transactions.

**Why M2 Matters**: M1 provides single-operation implicit transactions. M2 adds multi-key atomic operations, which are essential for:
- Agent coordination (CAS on state machines)
- Multi-step tool call sequences (all-or-nothing semantics)
- Safe concurrent access from multiple agent instances

**What Changes from M1**:
- New `crates/concurrency` - OCC transaction layer
- TransactionContext with read/write/CAS tracking
- Snapshot isolation via ClonedSnapshotView
- Three-phase commit: BEGIN → VALIDATE → COMMIT/ABORT
- M1 single-operation API remains unchanged (backwards compatible)

---

## ✅ Completed (M2 Planning Phase)

### 1. Architecture & Design

- ✅ **[M2_ARCHITECTURE.md](../architecture/M2_ARCHITECTURE.md)** - Complete M2 specification (13 sections)
  - Executive summary with goals/non-goals
  - OCC transaction model (three-phase commit)
  - Component architecture (new concurrency crate)
  - Snapshot isolation design
  - Conflict detection algorithm
  - API design with TransactionContext
  - WAL integration for transactions
  - Performance characteristics
  - Testing strategy
  - Migration from M1 (backwards compatibility)
  - Known limitations and trade-offs

- ✅ **[docs/diagrams/m2-architecture.md](../diagrams/m2-architecture.md)** - 10 detailed diagrams
  1. System Architecture Overview (M2)
  2. OCC Transaction Flow (multi-step)
  3. Snapshot Isolation Mechanism (timeline)
  4. Conflict Detection Examples (4 scenarios)
  5. Snapshot Creation and Management
  6. Transaction State Machine
  7. Concurrency Comparison: M1 vs M2
  8. Read-Your-Writes Guarantee
  9. Layer Dependencies (M2 Updated)
  10. API Evolution: M1 → M2

---

## 📋 M2 High-Level Epics

Based on the M2 architecture and following the M1 development approach, M2 will consist of **4 epics**:

### Epic 6: Concurrency Crate Foundation
**Goal**: Create the concurrency crate with core transaction types and snapshot infrastructure

**Why This Epic**:
- Establishes new crate structure for OCC
- Defines TransactionContext and state machine
- Creates snapshot abstraction (SnapshotView trait + ClonedSnapshotView)
- Blocks all other M2 work

**Estimated Stories**: 5-6 stories
**Estimated Duration**: 2-3 days with 3 Claudes in parallel (after initial story)
**Dependencies**: M1 complete (✅)

**Story Breakdown**:
1. **Story #33**: Create concurrency crate structure
   - Add `crates/concurrency` to workspace
   - Define module structure (transaction, snapshot, validation, cas)
   - Establish dependencies (core, storage)
   - **BLOCKS all other Epic 6 stories**

2. **Story #34**: TransactionContext type and state machine
   - Define TransactionContext struct
   - Implement TransactionStatus enum
   - Add read_set, write_set, delete_set, cas_set tracking
   - Unit tests for context creation and state transitions

3. **Story #35**: SnapshotView trait and ClonedSnapshotView
   - Define SnapshotView trait (abstraction for future)
   - Implement ClonedSnapshotView (M2 implementation)
   - Snapshot creation from UnifiedStore
   - Unit tests for snapshot isolation

4. **Story #36**: CASOperation type and validation
   - Define CASOperation struct
   - Implement CAS validation logic
   - Unit tests for CAS semantics

5. **Story #37**: ConflictInfo and error types
   - Define ConflictInfo enum (ReadConflict, WriteConflict, CASConflict)
   - Extend Error types for concurrency
   - Unit tests for conflict detection

6. **Story #38**: Concurrency crate integration tests
   - Multi-threaded snapshot creation
   - Concurrent context initialization
   - Conflict detection scenarios

**Parallelization**: After #33, stories #34-37 can run in parallel (3-4 Claudes)

---

### Epic 7: Transaction Lifecycle
**Goal**: Implement the three-phase OCC transaction lifecycle (BEGIN → VALIDATE → COMMIT/ABORT)

**Why This Epic**:
- Core OCC implementation
- Transaction begin/commit/abort logic
- Validation algorithm (conflict detection)
- Integration with storage and WAL

**Estimated Stories**: 6-7 stories
**Estimated Duration**: 3-4 days with 2-3 Claudes in parallel
**Dependencies**: Epic 6 complete

**Story Breakdown**:
1. **Story #39**: Transaction begin (snapshot creation)
   - Allocate transaction ID
   - Capture start_version from storage
   - Create snapshot (ClonedSnapshotView)
   - Initialize tracking sets (read/write/delete/cas)
   - Unit tests for transaction initialization

2. **Story #40**: Transaction read operations
   - Implement txn.get() with read-your-writes
   - Track reads in read_set
   - Read from snapshot with fallback to write_set
   - Unit tests for read isolation

3. **Story #41**: Transaction write operations
   - Implement txn.put() (buffer to write_set)
   - Implement txn.delete() (buffer to delete_set)
   - Implement txn.cas() (buffer to cas_set)
   - Unit tests for write buffering

4. **Story #42**: Validation algorithm
   - Implement validate_transaction()
   - Check read_set for version changes
   - Check write_set for conflicts
   - Validate CAS operations
   - Return ConflictInfo on conflicts
   - Unit tests for all conflict types

5. **Story #43**: Transaction commit
   - Apply writes to UnifiedStore
   - Update global version
   - Append to WAL (BeginTxn, Writes, CommitTxn)
   - Release resources
   - Unit tests for commit success

6. **Story #44**: Transaction abort and rollback
   - Implement abort logic
   - Discard buffered writes
   - Append AbortTxn to WAL
   - Unit tests for abort scenarios

7. **Story #45**: Transaction lifecycle integration tests
   - End-to-end transaction tests
   - Multi-threaded conflict scenarios
   - Crash during transaction tests
   - Performance tests (conflict rates)

**Parallelization**: After #39, some parallelization possible:
- #40-41 can run in parallel (2 Claudes)
- #42-44 sequential (depend on #40-41)

---

### Epic 8: Database Engine Integration
**Goal**: Integrate OCC transactions into the Database engine with explicit transaction API

**Why This Epic**:
- Expose transaction API to users
- Maintain backwards compatibility with M1 API
- Coordinate transactions with WAL and recovery
- Enable multi-primitive transactions

**Estimated Stories**: 5-6 stories
**Estimated Duration**: 2-3 days with 2 Claudes in parallel
**Dependencies**: Epic 7 complete

**Story Breakdown**:
1. **Story #46**: Database transaction API
   - Add `Database::transaction()` method
   - Accept closure with TransactionContext
   - Handle commit/abort/retry logic
   - Backwards compatibility tests (M1 API still works)

2. **Story #47**: Transaction coordinator in engine
   - Manage transaction ID allocation
   - Coordinate snapshot creation
   - Handle transaction lifecycle
   - Track active transactions
   - Unit tests for coordinator

3. **Story #48**: WAL integration for transactions
   - Extend WAL writer for transaction entries
   - BeginTxn, CommitTxn, AbortTxn entries
   - Batch writes within transaction
   - Unit tests for WAL transaction boundaries

4. **Story #49**: Recovery integration for transactions
   - Extend recovery to handle transaction entries
   - Reconstruct TransactionContext from WAL
   - Apply only committed transactions
   - Discard incomplete transactions
   - Unit tests for transaction recovery

5. **Story #50**: Multi-primitive transaction support
   - Enable KV + Event + State Machine in one transaction
   - Atomic multi-primitive operations
   - Integration tests for cross-primitive transactions

6. **Story #51**: Database transaction integration tests
   - End-to-end transaction API tests
   - Crash recovery with transactions
   - Performance benchmarks
   - Backwards compatibility validation

**Parallelization**: Limited due to dependencies:
- #46 blocks #47
- #48-49 can run in parallel after #47

---

### Epic 9: OCC Testing & Validation
**Goal**: Comprehensive testing of OCC implementation with multi-threaded workloads and edge cases

**Why This Epic**:
- OCC bugs are subtle and race-dependent
- Need extensive multi-threaded validation
- Property-based testing for correctness
- Performance benchmarking under contention

**Estimated Stories**: 4-5 stories
**Estimated Duration**: 2-3 days with 2-3 Claudes in parallel
**Dependencies**: Epic 8 complete

**Story Breakdown**:
1. **Story #52**: Multi-threaded conflict tests
   - Intentional conflict scenarios
   - High-contention workloads
   - Verify first-committer-wins
   - Track abort/retry rates
   - Validate isolation guarantees

2. **Story #53**: Property-based transaction tests
   - Use proptest for random operation sequences
   - Verify serializability
   - Validate snapshot isolation
   - Check invariants (no lost updates, etc.)

3. **Story #54**: Performance benchmarks
   - Single-threaded transaction throughput
   - Multi-threaded transaction throughput
   - Conflict rate vs parallelism
   - Snapshot creation overhead
   - Compare M1 vs M2 performance

4. **Story #55**: Edge case and stress tests
   - Long-running transactions
   - Large write sets
   - Snapshot memory usage
   - Transaction timeout handling
   - Concurrent snapshot creation

5. **Story #56**: OCC integration and regression tests
   - Full M2 end-to-end scenarios
   - Agent coordination patterns
   - Multi-agent workloads
   - Backwards compatibility regression tests
   - Documentation and examples

**Parallelization**: All stories can run in parallel (3 Claudes)

---

## 📊 M2 Project Metrics

### Estimated Scope
- **Total Epics**: 4 (Epics 6-9)
- **Total User Stories**: ~24 stories (#33-56)
- **Estimated Sequential Time**: ~60-80 hours
- **Estimated Parallel Time**: ~20-30 hours (with 3 Claudes)
- **Speedup**: ~2.5-3x with parallelization

### Test Coverage Goals
- **M2 Target**: >90% test coverage (maintain M1 bar)
- **Concurrency Crate**: 95%+ (critical correctness)
- **Transaction Lifecycle**: 95%+ (OCC correctness)
- **Engine Integration**: 85%+ (orchestration)
- **OCC Testing**: 100% (validation suite)

### Performance Targets (from M2_ARCHITECTURE.md)
- **Single-threaded transactions**: ~10K ops/sec (may be lower due to validation overhead)
- **Multi-threaded reads**: ~500K ops/sec (non-blocking with OCC)
- **Conflict rate**: <5% under normal agent workloads
- **Snapshot creation**: <10ms for 100K keys
- **Validation time**: <1ms for typical read/write sets

### Success Criteria
- ✅ All 24 user stories complete
- ✅ >90% test coverage across M2 code
- ✅ Multi-threaded tests pass with proper isolation
- ✅ Conflict detection and retry logic works
- ✅ Backwards compatibility with M1 API maintained
- ✅ Recovery handles transaction boundaries correctly
- ✅ Performance targets met (10K ops/sec, low conflict rate)
- ✅ Documentation updated (API reference, examples)

---

## 🎯 M2 Critical Design Decisions

### 1. ClonedSnapshotView for M2 (Deep Copy)

**Decision**: M2 uses ClonedSnapshotView (deep copy of BTreeMap at transaction start)

**Rationale**:
- Simple and correct for MVP
- Avoids complex version-bounded reads
- Memory overhead acceptable for embedded use
- Snapshot trait allows future optimization (LazySnapshotView)

**Trade-offs**:
- ❌ Memory overhead (entire map cloned)
- ❌ Snapshot creation time proportional to store size
- ✅ Simple implementation
- ✅ No contention with ongoing writes
- ✅ Clear upgrade path (trait abstraction)

**Future**: M3+ can add LazySnapshotView (version-bounded reads from live store)

### 2. First-Committer-Wins Conflict Resolution

**Decision**: When two transactions conflict, first to commit wins, second aborts and retries

**Rationale**:
- Agent workloads have low contention (different keys)
- Retry logic simple to implement
- No deadlocks (no waiting)
- Optimistic assumption fits agent patterns

**Trade-offs**:
- ❌ Starvation possible under high contention
- ❌ Wasted work on aborts
- ✅ No blocking (non-blocking reads/writes)
- ✅ Simple conflict resolution
- ✅ Matches agent usage patterns

**Mitigation**: Exponential backoff on retry, conflict rate monitoring

### 3. Backwards Compatibility with M1 API

**Decision**: M1 single-operation API (db.put(), db.get()) still works in M2

**Rationale**:
- Smooth migration path
- Existing code doesn't break
- Single-operation calls wrapped in implicit transaction
- No forced rewrites

**Implementation**:
```rust
// M1 API (still works in M2)
db.put(run_id, key, value)?;  // Implicit transaction

// M2 API (explicit transaction)
db.transaction(run_id, |txn| {
    txn.put(key1, value1)?;
    txn.put(key2, value2)?;
    Ok(())
})?;  // Atomic commit
```

**Trade-offs**:
- ❌ Two API styles (implicit vs explicit)
- ✅ No breaking changes
- ✅ Gradual adoption of transactions
- ✅ Easy migration path

### 4. Three-Phase Commit (BEGIN → VALIDATE → COMMIT)

**Decision**: OCC uses three distinct phases

**Rationale**:
- Clear separation of concerns
- Validation is distinct operation (can be retried)
- Matches OCC literature and best practices
- Enables conflict analysis and monitoring

**Phases**:
1. **BEGIN**: Create snapshot, allocate txn_id
2. **EXECUTE**: Buffer reads/writes, no storage mutation
3. **VALIDATE**: Check for conflicts (read versions unchanged)
4. **COMMIT** (if valid) or **ABORT** (if conflicts)

**Trade-offs**:
- ❌ Three-step coordination overhead
- ✅ Clear correctness guarantees
- ✅ Testable phases
- ✅ Conflict visibility (can log/monitor)

---

## 📋 M2 Epic Dependencies

```
M1 Foundation ✅
    ↓
Epic 6: Concurrency Crate Foundation
    ├─ Story #33 (workspace) [BLOCKS Epic 6]
    └─ Stories #34-38 (can parallelize: 3-4 Claudes)
    ↓
Epic 7: Transaction Lifecycle
    ├─ Story #39 (BEGIN)
    ├─ Stories #40-41 (READ/WRITE, parallel: 2 Claudes)
    ├─ Story #42 (VALIDATE)
    ├─ Stories #43-44 (COMMIT/ABORT, parallel: 2 Claudes)
    └─ Story #45 (integration tests)
    ↓
Epic 8: Database Engine Integration
    ├─ Story #46 (transaction API)
    ├─ Story #47 (coordinator)
    ├─ Stories #48-49 (WAL/Recovery, parallel: 2 Claudes)
    ├─ Story #50 (multi-primitive)
    └─ Story #51 (integration tests)
    ↓
Epic 9: OCC Testing & Validation
    └─ Stories #52-56 (all parallel: 3 Claudes)
```

**Critical Path**: Stories #33 → #39 → #42 → #46 → #47 are sequential and block other work

**Parallelization Opportunities**:
- Epic 6: 3-4 Claudes after #33
- Epic 7: 2 Claudes for #40-41, #43-44
- Epic 8: 2 Claudes for #48-49
- Epic 9: 3 Claudes for all stories

**Overall M2 Speedup**: ~2.5-3x with 3 parallel Claudes

---

## 🎓 M2 Testing Strategy

Following the M1 TDD methodology adapted for M2:

### Epic 6: Concurrency Crate (TDD - Test First)

**Approach**: Pure TDD (Red-Green-Refactor)

**Why**: Concurrency is complex with subtle bugs. Tests define correct behavior.

**Example Tests**:
```rust
#[test]
fn test_transaction_context_creation() {
    let ctx = TransactionContext::new(txn_id, run_id, snapshot);
    assert_eq!(ctx.status, TransactionStatus::Active);
    assert!(ctx.read_set.is_empty());
}

#[test]
fn test_snapshot_isolation() {
    let store = UnifiedStore::new();
    store.put(key, value1, None)?;

    let snapshot = ClonedSnapshotView::create(&store, version)?;

    // Concurrent write after snapshot
    store.put(key, value2, None)?;

    // Snapshot sees old value
    assert_eq!(snapshot.get(&key)?.value, value1);
}

#[test]
fn test_cas_operation_validation() {
    let cas = CASOperation { key, expected: 42, new_value };
    let valid = validate_cas(&cas, &storage)?;
    assert!(valid);
}
```

### Epic 7: Transaction Lifecycle (TDD + Multi-threaded Tests)

**Approach**: TDD for logic, multi-threaded tests for concurrency

**Why**: OCC correctness requires both unit tests and concurrency validation.

**Example Tests**:
```rust
#[test]
fn test_read_your_writes() {
    let txn = begin_transaction(&store)?;
    txn.put(key, value1)?;

    // Should see buffered write
    assert_eq!(txn.get(key)?, Some(value1));
}

#[test]
fn test_conflict_detection_write_write() {
    let txn1 = begin_transaction(&store)?;
    let txn2 = begin_transaction(&store)?;

    txn1.put(key, value1)?;
    txn2.put(key, value2)?;

    // First commits wins
    assert!(txn1.commit().is_ok());
    assert!(txn2.commit().is_err()); // Conflict!
}

#[test]
fn test_concurrent_non_conflicting_transactions() {
    let store = Arc::new(UnifiedStore::new());

    let handles: Vec<_> = (0..10).map(|i| {
        let store = Arc::clone(&store);
        thread::spawn(move || {
            let txn = begin_transaction(&store).unwrap();
            txn.put(format!("key{}", i), format!("value{}", i)).unwrap();
            txn.commit().unwrap();
        })
    }).collect();

    for h in handles { h.join().unwrap(); }

    // All transactions should succeed (no conflicts)
}
```

### Epic 8: Engine Integration (Integration Tests)

**Approach**: End-to-end integration tests

**Why**: Engine orchestrates multiple layers. Integration tests prove it works.

**Example Tests**:
```rust
#[test]
fn test_database_transaction_api() {
    let db = Database::open("test.db")?;
    let run_id = db.begin_run();

    db.transaction(run_id, |txn| {
        txn.put("key1", "value1")?;
        txn.put("key2", "value2")?;
        Ok(())
    })?;

    // Both writes committed atomically
    assert_eq!(db.get(run_id, "key1")?, Some("value1"));
    assert_eq!(db.get(run_id, "key2")?, Some("value2"));
}

#[test]
fn test_transaction_recovery_after_crash() {
    let db = Database::open("test.db")?;
    let run_id = db.begin_run();

    db.transaction(run_id, |txn| {
        txn.put("key", "value")?;
        Ok(())
    })?;

    // Simulate crash
    drop(db);

    // Recover
    let db2 = Database::open("test.db")?;
    let run_id2 = db2.begin_run();

    // Transaction should be recovered
    assert_eq!(db2.get(run_id2, "key")?, Some("value"));
}
```

### Epic 9: OCC Validation (Property-Based + Stress Tests)

**Approach**: Property-based testing with proptest

**Why**: OCC must work for ALL operation sequences, not just examples.

**Example Tests**:
```rust
use proptest::prelude::*;

proptest! {
    #[test]
    fn test_serializability(
        ops in prop::collection::vec(transaction_op_strategy(), 10..100)
    ) {
        let db = Database::open_in_memory()?;

        // Execute operations concurrently
        let handles = ops.chunks(10).map(|chunk| {
            let db = db.clone();
            thread::spawn(move || {
                for op in chunk {
                    execute_op(&db, op).unwrap();
                }
            })
        }).collect::<Vec<_>>();

        for h in handles { h.join().unwrap(); }

        // Verify invariants hold
        verify_consistency(&db)?;
    }
}

#[test]
fn test_high_contention_stress() {
    let db = Database::open_in_memory()?;
    let key = "hotspot";

    let handles: Vec<_> = (0..100).map(|i| {
        let db = db.clone();
        thread::spawn(move || {
            for _ in 0..100 {
                db.transaction(RunId::new(), |txn| {
                    let val: u64 = txn.get(key)?.unwrap_or(0);
                    txn.put(key, val + 1)?;
                    Ok(())
                }).unwrap();
            }
        })
    }).collect();

    for h in handles { h.join().unwrap(); }

    // Should see 10,000 increments (no lost updates)
    let final_val: u64 = db.get(RunId::new(), key)?.unwrap();
    assert_eq!(final_val, 10_000);
}
```

---

## 🚀 Next Steps

### Immediate (Today/This Week)

1. **Create M2 GitHub Milestone**
   ```bash
   gh api repos/anibjoshi/in-mem/milestones -X POST \
     -f title="M2: Transactions" \
     -f description="Optimistic Concurrency Control with snapshot isolation" \
     -f due_on="2026-01-24T00:00:00Z"
   ```

2. **Create Epic Issues (#6-9)**
   - Epic #6: Concurrency Crate Foundation
   - Epic #7: Transaction Lifecycle
   - Epic #8: Database Engine Integration
   - Epic #9: OCC Testing & Validation

3. **Create User Story Issues (#33-56)**
   - Each with: user story format, context, acceptance criteria
   - Implementation guidance from M2_ARCHITECTURE.md
   - Testing requirements from TDD_METHODOLOGY.md
   - Effort estimates (2-8 hours per story)
   - Labels: milestone-2, epic-N, priority, risk

4. **Update CLAUDE_COORDINATION.md for M2**
   - Parallelization strategy for each epic
   - File ownership to minimize conflicts
   - Communication protocols

### Week 3 (M2 Implementation)

- ⏳ Epic 6: Concurrency Crate Foundation (2-3 days with 3 Claudes)
- ⏳ Epic 7: Transaction Lifecycle (3-4 days with 2-3 Claudes)
- ⏳ Epic 8: Database Engine Integration (2-3 days with 2 Claudes)
- ⏳ Epic 9: OCC Testing & Validation (2-3 days with 3 Claudes)

**Target**: M2 complete by end of Week 3 (2026-01-18)

---

## 📈 Progress Tracking

### Epic 6: Concurrency Crate Foundation ⏳ Not Started
- [ ] Story #33: Create concurrency crate structure
- [ ] Story #34: TransactionContext type and state machine
- [ ] Story #35: SnapshotView trait and ClonedSnapshotView
- [ ] Story #36: CASOperation type and validation
- [ ] Story #37: ConflictInfo and error types
- [ ] Story #38: Concurrency crate integration tests

**Status**: ⏳ **READY TO START**

### Epic 7: Transaction Lifecycle ⏳ Not Started
- [ ] Story #39: Transaction begin (snapshot creation)
- [ ] Story #40: Transaction read operations
- [ ] Story #41: Transaction write operations
- [ ] Story #42: Validation algorithm
- [ ] Story #43: Transaction commit
- [ ] Story #44: Transaction abort and rollback
- [ ] Story #45: Transaction lifecycle integration tests

**Status**: ⏳ **BLOCKED by Epic 6**

### Epic 8: Database Engine Integration ⏳ Not Started
- [ ] Story #46: Database transaction API
- [ ] Story #47: Transaction coordinator in engine
- [ ] Story #48: WAL integration for transactions
- [ ] Story #49: Recovery integration for transactions
- [ ] Story #50: Multi-primitive transaction support
- [ ] Story #51: Database transaction integration tests

**Status**: ⏳ **BLOCKED by Epic 7**

### Epic 9: OCC Testing & Validation ⏳ Not Started
- [ ] Story #52: Multi-threaded conflict tests
- [ ] Story #53: Property-based transaction tests
- [ ] Story #54: Performance benchmarks
- [ ] Story #55: Edge case and stress tests
- [ ] Story #56: OCC integration and regression tests

**Status**: ⏳ **BLOCKED by Epic 8**

---

## 🎉 Summary

**M2 Planning Phase: COMPLETE ✅**

We have:
- ✅ Complete M2 architecture specification (M2_ARCHITECTURE.md)
- ✅ Visual M2 architecture diagrams (10 diagrams)
- ✅ 4 high-level epics defined (Epics 6-9)
- ✅ ~24 user stories identified (#33-56)
- ✅ Testing strategy adapted from M1 TDD methodology
- ✅ Performance targets and success criteria defined
- ✅ Parallelization strategy planned (~2.5-3x speedup)

**Next: Create GitHub Issues and Begin Implementation** 🚀

```bash
# Start with Epic 6 planning
# Then begin Story #33 (concurrency crate structure)
```

---

## 📞 Communication

### For M2 Questions
- Read [M2_ARCHITECTURE.md](../architecture/M2_ARCHITECTURE.md) for technical details
- Read [m2-architecture.md](../diagrams/m2-architecture.md) for visual diagrams
- Read [TDD_METHODOLOGY.md](../development/TDD_METHODOLOGY.md) for testing approach
- Read [DEVELOPMENT_WORKFLOW.md](../development/DEVELOPMENT_WORKFLOW.md) for Git workflow
- Check GitHub issues for context

### For Coordination
- Update CLAUDE_COORDINATION.md with M2 assignments
- Comment on GitHub issues when starting work
- Comment when blocked on dependencies
- Comment when complete with PR link

---

**Current Status**: ✅ **M2 PLANNING COMPLETE - READY TO CREATE ISSUES AND BEGIN IMPLEMENTATION**
