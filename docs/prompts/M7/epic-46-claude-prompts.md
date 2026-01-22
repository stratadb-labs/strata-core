# Epic 46: Validation & Benchmarks - Implementation Prompts

**Epic Goal**: Ensure correctness and document performance baselines

**GitHub Issue**: [#344](https://github.com/anibjoshi/in-mem/issues/344)
**Status**: Ready to begin (after all other epics complete)
**Dependencies**: Epics 40, 41, 42, 43, 44, 45

---

## AUTHORITATIVE SPECIFICATIONS - READ THESE FIRST

**`docs/architecture/M7_ARCHITECTURE.md` is THE AUTHORITATIVE SPEC.**

Before starting ANY story in this epic, read:
1. **Architecture Spec (AUTHORITATIVE)**: `docs/architecture/M7_ARCHITECTURE.md`
2. **Epic Spec**: `docs/milestones/M7/EPIC_46_VALIDATION.md`
3. **Prompt Header**: `docs/prompts/M7/M7_PROMPT_HEADER.md` for the 5 architectural rules

---

## Epic 46 Overview

### Scope
- Crash simulation test suite
- Recovery invariant tests (R1-R6)
- Replay determinism tests (P1-P6)
- Performance baseline documentation

### Key Goal

M7 is a **correctness milestone**. Epic 46 validates that all correctness guarantees are met:

1. **Recovery Invariants (R1-R6)**
   - R1: Deterministic
   - R2: Idempotent
   - R3: Prefix-consistent
   - R4: Never invents data
   - R5: Never drops committed
   - R6: May drop uncommitted

2. **Replay Invariants (P1-P6)**
   - P1: Pure function
   - P2: Side-effect free
   - P3: Derived view
   - P4: Does not persist
   - P5: Deterministic
   - P6: Idempotent

### Success Criteria
- [ ] Crash simulation tests for all scenarios
- [ ] All recovery invariants (R1-R6) validated
- [ ] All replay invariants (P1-P6) validated
- [ ] Performance baselines documented

### Component Breakdown
- **Story #326 (GitHub #381)**: Crash Simulation Test Suite - CRITICAL
- **Story #327 (GitHub #382)**: Recovery Invariant Tests - CRITICAL
- **Story #328 (GitHub #383)**: Replay Determinism Tests - CRITICAL
- **Story #329 (GitHub #384)**: Performance Baseline Documentation - HIGH

---

## Dependency Graph

```
Story #381 (Crash Sim) ──> Story #382 (Recovery Tests)
                                    │
                                    v
                          Story #383 (Replay Tests)
                                    │
                                    v
                          Story #384 (Performance)
```

---

## Story #381: Crash Simulation Test Suite

**GitHub Issue**: [#381](https://github.com/anibjoshi/in-mem/issues/381)
**Estimated Time**: 4 hours
**Dependencies**: All implementation epics complete

### Start Story

```bash
gh issue view 381
./scripts/start-story.sh 46 381 crash-simulation
```

### Implementation

Create `crates/durability/tests/crash_simulation.rs`:

```rust
//! Crash simulation tests
//!
//! These tests simulate various crash scenarios to ensure
//! correct recovery behavior.

use tempfile::TempDir;

mod crash_simulation {
    use super::*;

    /// Test: Crash during normal operation
    #[test]
    fn test_crash_during_normal_operation() {
        let temp_dir = TempDir::new().unwrap();

        // Write some data
        {
            let db = create_test_db(temp_dir.path());

            // Commit several transactions
            for i in 0..100 {
                let mut tx = Transaction::new();
                tx.kv_put(format!("key_{}", i).into(), format!("value_{}", i).into());
                db.commit(tx).unwrap();
            }

            // "Crash" - no graceful shutdown
        }

        // Recover
        let (recovered, result) = RecoveryEngine::recover(
            temp_dir.path(),
            RecoveryOptions::default(),
        ).unwrap();

        // All 100 transactions should be recovered
        assert_eq!(result.transactions_recovered, 100);
        for i in 0..100 {
            assert!(recovered.kv.get(&format!("key_{}", i)).is_some());
        }
    }

    /// Test: Crash during snapshot write
    #[test]
    fn test_crash_during_snapshot_write() {
        let temp_dir = TempDir::new().unwrap();

        {
            let db = create_test_db(temp_dir.path());

            // Write some data
            for i in 0..50 {
                let mut tx = Transaction::new();
                tx.kv_put(format!("key_{}", i).into(), format!("value_{}", i).into());
                db.commit(tx).unwrap();
            }

            // Start snapshot but don't complete it
            let snapshot_dir = temp_dir.path().join("snapshots");
            std::fs::create_dir_all(&snapshot_dir).unwrap();
            let partial_snapshot = snapshot_dir.join("snapshot_partial.dat");
            std::fs::write(&partial_snapshot, b"INMEM_SNAP").unwrap(); // Just magic
        }

        // Recover - should use WAL only
        let (recovered, result) = RecoveryEngine::recover(
            temp_dir.path(),
            RecoveryOptions::default(),
        ).unwrap();

        // Data should still be intact
        assert_eq!(result.transactions_recovered, 50);
        assert!(result.snapshot_used.is_none()); // Partial snapshot skipped
    }

    /// Test: Crash during WAL truncation
    #[test]
    fn test_crash_during_wal_truncation() {
        let temp_dir = TempDir::new().unwrap();

        {
            let db = create_test_db(temp_dir.path());

            // Write data
            for i in 0..50 {
                let mut tx = Transaction::new();
                tx.kv_put(format!("key_{}", i).into(), format!("value_{}", i).into());
                db.commit(tx).unwrap();
            }

            // Create valid snapshot
            db.snapshot().unwrap();

            // Write more data after snapshot
            for i in 50..100 {
                let mut tx = Transaction::new();
                tx.kv_put(format!("key_{}", i).into(), format!("value_{}", i).into());
                db.commit(tx).unwrap();
            }

            // Simulate partial WAL truncation
            // (leave both old and new WAL content)
        }

        // Recover
        let (recovered, _) = RecoveryEngine::recover(
            temp_dir.path(),
            RecoveryOptions::default(),
        ).unwrap();

        // All data should be present
        for i in 0..100 {
            assert!(recovered.kv.get(&format!("key_{}", i)).is_some());
        }
    }

    /// Test: Crash with corrupt WAL entries
    #[test]
    fn test_crash_with_corrupt_wal() {
        let temp_dir = TempDir::new().unwrap();

        {
            let db = create_test_db(temp_dir.path());

            // Write data
            for i in 0..20 {
                let mut tx = Transaction::new();
                tx.kv_put(format!("key_{}", i).into(), format!("value_{}", i).into());
                db.commit(tx).unwrap();
            }
        }

        // Corrupt some WAL entries
        let wal_path = temp_dir.path().join("wal.dat");
        let mut data = std::fs::read(&wal_path).unwrap();
        // Corrupt bytes in the middle
        if data.len() > 100 {
            data[100] ^= 0xFF;
        }
        std::fs::write(&wal_path, &data).unwrap();

        // Recover with permissive options
        let (recovered, result) = RecoveryEngine::recover(
            temp_dir.path(),
            RecoveryOptions::permissive(),
        ).unwrap();

        // Should have recovered most transactions
        assert!(result.corrupt_entries_skipped > 0);
        assert!(result.transactions_recovered > 0);
    }

    /// Test: Crash with corrupt snapshot
    #[test]
    fn test_crash_with_corrupt_snapshot() {
        let temp_dir = TempDir::new().unwrap();

        {
            let db = create_test_db(temp_dir.path());

            // Write data
            for i in 0..50 {
                let mut tx = Transaction::new();
                tx.kv_put(format!("key_{}", i).into(), format!("value_{}", i).into());
                db.commit(tx).unwrap();
            }

            // Create first snapshot
            db.snapshot().unwrap();

            // Write more data
            for i in 50..100 {
                let mut tx = Transaction::new();
                tx.kv_put(format!("key_{}", i).into(), format!("value_{}", i).into());
                db.commit(tx).unwrap();
            }

            // Create second snapshot
            std::thread::sleep(std::time::Duration::from_millis(10));
            db.snapshot().unwrap();
        }

        // Corrupt newest snapshot
        let snapshot_dir = temp_dir.path().join("snapshots");
        let mut snapshots: Vec<_> = std::fs::read_dir(&snapshot_dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .collect();
        snapshots.sort();
        let newest = snapshots.last().unwrap();
        let mut data = std::fs::read(newest).unwrap();
        data[50] ^= 0xFF;
        std::fs::write(newest, &data).unwrap();

        // Recover - should use older snapshot
        let (recovered, result) = RecoveryEngine::recover(
            temp_dir.path(),
            RecoveryOptions::default(),
        ).unwrap();

        // Should have used older snapshot + WAL replay
        assert!(result.snapshot_used.is_some());
        for i in 0..100 {
            assert!(recovered.kv.get(&format!("key_{}", i)).is_some());
        }
    }

    /// Test: Crash mid-transaction
    #[test]
    fn test_crash_mid_transaction() {
        let temp_dir = TempDir::new().unwrap();

        {
            let db = create_test_db(temp_dir.path());

            // Commit a transaction
            let mut tx1 = Transaction::new();
            tx1.kv_put("committed".into(), "yes".into());
            db.commit(tx1).unwrap();

            // Start a transaction but don't commit
            let tx2_id = TxId::new_v4();
            for i in 0..10 {
                let entry = WalEntry {
                    entry_type: WalEntryType::KvPut,
                    version: 1,
                    tx_id: Some(tx2_id),
                    payload: serialize_kv_put(&format!("pending_{}", i), "value"),
                };
                db.wal.write_entry(&entry).unwrap();
            }
            // "Crash" before commit marker
        }

        // Recover
        let (recovered, result) = RecoveryEngine::recover(
            temp_dir.path(),
            RecoveryOptions::default(),
        ).unwrap();

        // Committed transaction visible
        assert!(recovered.kv.get("committed").is_some());

        // Uncommitted transaction NOT visible
        for i in 0..10 {
            assert!(recovered.kv.get(&format!("pending_{}", i)).is_none());
        }

        assert_eq!(result.transactions_recovered, 1);
        assert_eq!(result.orphaned_transactions, 1);
    }
}
```

### Acceptance Criteria

- [ ] Crash during normal operation test
- [ ] Crash during snapshot write test
- [ ] Crash during WAL truncation test
- [ ] Crash with corrupt WAL test
- [ ] Crash with corrupt snapshot test
- [ ] Crash mid-transaction test

### Complete Story

```bash
./scripts/complete-story.sh 381
```

---

## Story #382: Recovery Invariant Tests

**GitHub Issue**: [#382](https://github.com/anibjoshi/in-mem/issues/382)
**Estimated Time**: 4 hours
**Dependencies**: Story #381

### Start Story

```bash
gh issue view 382
./scripts/start-story.sh 46 382 recovery-invariants
```

### Implementation

Create `crates/durability/tests/recovery_invariants.rs`:

```rust
//! Recovery invariant tests
//!
//! R1: Deterministic - Same WAL + Snapshot = Same state
//! R2: Idempotent - Replaying recovery produces identical state
//! R3: Prefix-consistent - No partial transactions visible
//! R4: Never invents data - Only committed data appears
//! R5: Never drops committed - All durable commits survive
//! R6: May drop uncommitted - Depending on durability mode

mod recovery_invariants {
    use super::*;

    /// R1: Recovery is deterministic
    #[test]
    fn test_r1_deterministic() {
        let temp_dir = TempDir::new().unwrap();

        // Create database with data
        {
            let db = create_test_db(temp_dir.path());
            for i in 0..100 {
                let mut tx = Transaction::new();
                tx.kv_put(format!("key_{}", i).into(), format!("value_{}", i).into());
                db.commit(tx).unwrap();
            }
            db.snapshot().unwrap();
        }

        // Recover twice
        let (db1, _) = RecoveryEngine::recover(
            temp_dir.path(),
            RecoveryOptions::default(),
        ).unwrap();

        let (db2, _) = RecoveryEngine::recover(
            temp_dir.path(),
            RecoveryOptions::default(),
        ).unwrap();

        // Must be identical
        let keys1: Vec<_> = db1.kv.list_all().unwrap();
        let keys2: Vec<_> = db2.kv.list_all().unwrap();
        assert_eq!(keys1, keys2);
    }

    /// R2: Recovery is idempotent
    #[test]
    fn test_r2_idempotent() {
        let temp_dir = TempDir::new().unwrap();

        {
            let db = create_test_db(temp_dir.path());
            for i in 0..50 {
                let mut tx = Transaction::new();
                tx.kv_put(format!("key_{}", i).into(), format!("value_{}", i).into());
                db.commit(tx).unwrap();
            }
        }

        // Recover
        let (db1, result1) = RecoveryEngine::recover(
            temp_dir.path(),
            RecoveryOptions::default(),
        ).unwrap();

        // Save state
        let state1: Vec<_> = db1.kv.list_all().unwrap();

        // Recover again (simulating restart)
        let (db2, result2) = RecoveryEngine::recover(
            temp_dir.path(),
            RecoveryOptions::default(),
        ).unwrap();

        // Must be identical
        let state2: Vec<_> = db2.kv.list_all().unwrap();
        assert_eq!(state1, state2);
        assert_eq!(result1.transactions_recovered, result2.transactions_recovered);
    }

    /// R3: Recovery is prefix-consistent
    #[test]
    fn test_r3_prefix_consistent() {
        let temp_dir = TempDir::new().unwrap();

        {
            let db = create_test_db(temp_dir.path());

            // Transaction 1 - commit
            let mut tx1 = Transaction::new();
            tx1.kv_put("tx1_kv".into(), "value".into())
               .json_set("tx1_json".into(), json!({}));
            db.commit(tx1).unwrap();

            // Transaction 2 - partial (no commit)
            let tx2_id = TxId::new_v4();
            db.wal.write_entry(&WalEntry {
                entry_type: WalEntryType::KvPut,
                version: 1,
                tx_id: Some(tx2_id),
                payload: serialize_kv_put("tx2_kv", "value"),
            }).unwrap();
            // No commit marker
        }

        let (recovered, result) = RecoveryEngine::recover(
            temp_dir.path(),
            RecoveryOptions::default(),
        ).unwrap();

        // TX1 fully visible (all effects)
        assert!(recovered.kv.get("tx1_kv").is_some());
        assert!(recovered.json.get("tx1_json").is_some());

        // TX2 not visible at all
        assert!(recovered.kv.get("tx2_kv").is_none());

        assert_eq!(result.transactions_recovered, 1);
        assert_eq!(result.orphaned_transactions, 1);
    }

    /// R4: Recovery never invents data
    #[test]
    fn test_r4_never_invents_data() {
        let temp_dir = TempDir::new().unwrap();

        let known_keys: Vec<String> = (0..50)
            .map(|i| format!("key_{}", i))
            .collect();

        {
            let db = create_test_db(temp_dir.path());
            for key in &known_keys {
                let mut tx = Transaction::new();
                tx.kv_put(key.clone().into(), "value".into());
                db.commit(tx).unwrap();
            }
        }

        let (recovered, _) = RecoveryEngine::recover(
            temp_dir.path(),
            RecoveryOptions::default(),
        ).unwrap();

        // Only known keys should exist
        let all_keys: Vec<_> = recovered.kv.list_keys().unwrap();
        for key in &all_keys {
            assert!(
                known_keys.contains(key),
                "Invented key: {}",
                key
            );
        }
    }

    /// R5: Recovery never drops committed data
    #[test]
    fn test_r5_never_drops_committed() {
        let temp_dir = TempDir::new().unwrap();

        {
            let db = create_test_db(temp_dir.path());

            // Commit 100 transactions
            for i in 0..100 {
                let mut tx = Transaction::new();
                tx.kv_put(format!("key_{}", i).into(), format!("value_{}", i).into());
                db.commit(tx).unwrap();
            }

            // Snapshot
            db.snapshot().unwrap();

            // Commit 50 more
            for i in 100..150 {
                let mut tx = Transaction::new();
                tx.kv_put(format!("key_{}", i).into(), format!("value_{}", i).into());
                db.commit(tx).unwrap();
            }
        }

        let (recovered, result) = RecoveryEngine::recover(
            temp_dir.path(),
            RecoveryOptions::default(),
        ).unwrap();

        // All 150 must be present
        assert_eq!(result.transactions_recovered, 150);
        for i in 0..150 {
            let value = recovered.kv.get(&format!("key_{}", i)).unwrap();
            assert!(value.is_some(), "Missing key_{}", i);
        }
    }

    /// R6: Recovery may drop uncommitted data
    #[test]
    fn test_r6_may_drop_uncommitted() {
        let temp_dir = TempDir::new().unwrap();

        {
            let db = create_test_db(temp_dir.path());

            // Commit some transactions
            for i in 0..10 {
                let mut tx = Transaction::new();
                tx.kv_put(format!("committed_{}", i).into(), "value".into());
                db.commit(tx).unwrap();
            }

            // Write uncommitted data
            let tx_id = TxId::new_v4();
            for i in 0..10 {
                db.wal.write_entry(&WalEntry {
                    entry_type: WalEntryType::KvPut,
                    version: 1,
                    tx_id: Some(tx_id),
                    payload: serialize_kv_put(&format!("uncommitted_{}", i), "value"),
                }).unwrap();
            }
            // No commit marker
        }

        let (recovered, result) = RecoveryEngine::recover(
            temp_dir.path(),
            RecoveryOptions::default(),
        ).unwrap();

        // Committed data present
        for i in 0..10 {
            assert!(recovered.kv.get(&format!("committed_{}", i)).is_some());
        }

        // Uncommitted data NOT present
        for i in 0..10 {
            assert!(recovered.kv.get(&format!("uncommitted_{}", i)).is_none());
        }

        assert_eq!(result.orphaned_transactions, 1);
    }
}
```

### Acceptance Criteria

- [ ] R1 test passes (deterministic)
- [ ] R2 test passes (idempotent)
- [ ] R3 test passes (prefix-consistent)
- [ ] R4 test passes (never invents)
- [ ] R5 test passes (never drops committed)
- [ ] R6 test passes (may drop uncommitted)

### Complete Story

```bash
./scripts/complete-story.sh 382
```

---

## Story #383: Replay Determinism Tests

**GitHub Issue**: [#383](https://github.com/anibjoshi/in-mem/issues/383)
**Estimated Time**: 3 hours
**Dependencies**: Story #382

### Start Story

```bash
gh issue view 383
./scripts/start-story.sh 46 383 replay-determinism
```

### Implementation

Create `crates/engine/tests/replay_invariants.rs`:

```rust
//! Replay invariant tests
//!
//! P1: Pure function - Over (Snapshot, WAL, EventLog)
//! P2: Side-effect free - Does not mutate canonical store
//! P3: Derived view - Not a new source of truth
//! P4: Does not persist - Unless explicitly materialized
//! P5: Deterministic - Same inputs = Same view
//! P6: Idempotent - Running twice produces identical view

mod replay_invariants {
    use super::*;

    /// P1: Replay is a pure function
    #[test]
    fn test_p1_pure_function() {
        let db = create_test_db_in_memory();

        let run_id = RunId::new();
        db.begin_run(run_id).unwrap();
        db.kv.put(run_id, "key1", "value1").unwrap();
        db.kv.put(run_id, "key2", "value2").unwrap();
        db.end_run(run_id).unwrap();

        // Replay returns same view given same inputs
        let view1 = db.replay_run(run_id).unwrap();
        let view2 = db.replay_run(run_id).unwrap();

        // Views are identical
        assert_eq!(view1.kv_keys().count(), view2.kv_keys().count());
        assert_eq!(view1.get_kv(&"key1".into()), view2.get_kv(&"key1".into()));
        assert_eq!(view1.get_kv(&"key2".into()), view2.get_kv(&"key2".into()));
    }

    /// P2: Replay is side-effect free
    #[test]
    fn test_p2_side_effect_free() {
        let db = create_test_db_in_memory();

        // Setup: Create run with data
        let run_id = RunId::new();
        db.begin_run(run_id).unwrap();
        db.kv.put(run_id, "run_key", "run_value").unwrap();
        db.end_run(run_id).unwrap();

        // Add more data outside run
        db.kv.put_global("global_key", "global_value").unwrap();

        // Capture canonical state before replay
        let before: Vec<_> = db.kv.list_all().unwrap();

        // Replay
        let _view = db.replay_run(run_id).unwrap();

        // Canonical state unchanged
        let after: Vec<_> = db.kv.list_all().unwrap();
        assert_eq!(before, after);
    }

    /// P3: Replay produces a derived view
    #[test]
    fn test_p3_derived_view() {
        let db = create_test_db_in_memory();

        let run_id = RunId::new();
        db.begin_run(run_id).unwrap();
        db.kv.put(run_id, "key", "value").unwrap();
        db.end_run(run_id).unwrap();

        let view = db.replay_run(run_id).unwrap();

        // View is read-only (no mutation methods)
        // This is enforced by the type system - ReadOnlyView has no mut methods
        assert!(view.get_kv(&"key".into()).is_some());
    }

    /// P4: Replay does not persist state
    #[test]
    fn test_p4_does_not_persist() {
        let temp_dir = TempDir::new().unwrap();

        {
            let db = create_test_db(temp_dir.path());

            let run_id = RunId::new();
            db.begin_run(run_id).unwrap();
            db.kv.put(run_id, "key", "value").unwrap();
            db.end_run(run_id).unwrap();

            // Replay
            let _view = db.replay_run(run_id).unwrap();
        }

        // Recover
        let (recovered, _) = RecoveryEngine::recover(
            temp_dir.path(),
            RecoveryOptions::default(),
        ).unwrap();

        // No additional data from replay (only original run data)
        let keys: Vec<_> = recovered.kv.list_keys().unwrap();
        assert_eq!(keys.len(), 1);
    }

    /// P5: Replay is deterministic
    #[test]
    fn test_p5_deterministic() {
        let db = create_test_db_in_memory();

        let run_id = RunId::new();
        db.begin_run(run_id).unwrap();

        // Add various operations
        db.kv.put(run_id, "kv1", "v1").unwrap();
        db.kv.put(run_id, "kv2", "v2").unwrap();
        db.json.set(run_id, "json1", json!({"a": 1})).unwrap();
        db.state.set(run_id, "state1", "running").unwrap();

        db.end_run(run_id).unwrap();

        // Replay 10 times
        let views: Vec<_> = (0..10)
            .map(|_| db.replay_run(run_id).unwrap())
            .collect();

        // All must be identical
        let reference = &views[0];
        for view in &views[1..] {
            assert_eq!(
                reference.kv_keys().count(),
                view.kv_keys().count()
            );
            assert_eq!(
                reference.json_keys().count(),
                view.json_keys().count()
            );
        }
    }

    /// P6: Replay is idempotent
    #[test]
    fn test_p6_idempotent() {
        let db = create_test_db_in_memory();

        let run_id = RunId::new();
        db.begin_run(run_id).unwrap();
        db.kv.put(run_id, "key", "value").unwrap();
        db.end_run(run_id).unwrap();

        // First replay
        let view1 = db.replay_run(run_id).unwrap();
        let state1 = view1.get_kv(&"key".into()).cloned();

        // Second replay
        let view2 = db.replay_run(run_id).unwrap();
        let state2 = view2.get_kv(&"key".into()).cloned();

        // Identical results
        assert_eq!(state1, state2);
    }
}
```

### Acceptance Criteria

- [ ] P1 test passes (pure function)
- [ ] P2 test passes (side-effect free)
- [ ] P3 test passes (derived view)
- [ ] P4 test passes (does not persist)
- [ ] P5 test passes (deterministic)
- [ ] P6 test passes (idempotent)

### Complete Story

```bash
./scripts/complete-story.sh 383
```

---

## Story #384: Performance Baseline Documentation

**GitHub Issue**: [#384](https://github.com/anibjoshi/in-mem/issues/384)
**Estimated Time**: 3 hours
**Dependencies**: Story #383

### Start Story

```bash
gh issue view 384
./scripts/start-story.sh 46 384 performance-baseline
```

### Implementation

Create `crates/durability/benches/m7_performance.rs`:

```rust
//! M7 Performance Benchmarks
//!
//! These benchmarks establish performance baselines for M7 operations.

use criterion::{black_box, criterion_group, criterion_main, Criterion};

fn snapshot_benchmarks(c: &mut Criterion) {
    let mut group = c.benchmark_group("snapshot");

    // Snapshot write (small DB)
    group.bench_function("write_1mb", |b| {
        let temp_dir = TempDir::new().unwrap();
        let db = create_db_with_size(temp_dir.path(), 1_000_000); // 1 MB

        b.iter(|| {
            black_box(db.snapshot().unwrap());
        });
    });

    // Snapshot write (medium DB)
    group.bench_function("write_10mb", |b| {
        let temp_dir = TempDir::new().unwrap();
        let db = create_db_with_size(temp_dir.path(), 10_000_000); // 10 MB

        b.iter(|| {
            black_box(db.snapshot().unwrap());
        });
    });

    // Snapshot write (large DB)
    group.bench_function("write_100mb", |b| {
        let temp_dir = TempDir::new().unwrap();
        let db = create_db_with_size(temp_dir.path(), 100_000_000); // 100 MB

        b.iter(|| {
            black_box(db.snapshot().unwrap());
        });
    });

    group.finish();
}

fn recovery_benchmarks(c: &mut Criterion) {
    let mut group = c.benchmark_group("recovery");

    // Recovery from snapshot only
    group.bench_function("from_snapshot_10mb", |b| {
        let temp_dir = TempDir::new().unwrap();
        {
            let db = create_db_with_size(temp_dir.path(), 10_000_000);
            db.snapshot().unwrap();
        }

        b.iter(|| {
            let (_db, _result) = black_box(RecoveryEngine::recover(
                temp_dir.path(),
                RecoveryOptions::default(),
            ).unwrap());
        });
    });

    // Recovery from snapshot + WAL
    group.bench_function("from_snapshot_plus_wal", |b| {
        let temp_dir = TempDir::new().unwrap();
        {
            let db = create_db_with_size(temp_dir.path(), 10_000_000);
            db.snapshot().unwrap();
            // Add 10K more entries after snapshot
            for i in 0..10_000 {
                let mut tx = Transaction::new();
                tx.kv_put(format!("post_{}", i).into(), "value".into());
                db.commit(tx).unwrap();
            }
        }

        b.iter(|| {
            let (_db, _result) = black_box(RecoveryEngine::recover(
                temp_dir.path(),
                RecoveryOptions::default(),
            ).unwrap());
        });
    });

    // WAL replay only (no snapshot)
    group.bench_function("wal_only_10k_entries", |b| {
        let temp_dir = TempDir::new().unwrap();
        {
            let db = create_test_db(temp_dir.path());
            for i in 0..10_000 {
                let mut tx = Transaction::new();
                tx.kv_put(format!("key_{}", i).into(), "value".into());
                db.commit(tx).unwrap();
            }
        }

        b.iter(|| {
            let (_db, _result) = black_box(RecoveryEngine::recover(
                temp_dir.path(),
                RecoveryOptions::default(),
            ).unwrap());
        });
    });

    group.finish();
}

fn replay_benchmarks(c: &mut Criterion) {
    let mut group = c.benchmark_group("replay");

    // Replay small run
    group.bench_function("run_100_events", |b| {
        let db = create_test_db_in_memory();
        let run_id = setup_run_with_events(&db, 100);

        b.iter(|| {
            black_box(db.replay_run(run_id).unwrap());
        });
    });

    // Replay medium run
    group.bench_function("run_1k_events", |b| {
        let db = create_test_db_in_memory();
        let run_id = setup_run_with_events(&db, 1_000);

        b.iter(|| {
            black_box(db.replay_run(run_id).unwrap());
        });
    });

    // Diff two runs
    group.bench_function("diff_1k_keys", |b| {
        let db = create_test_db_in_memory();
        let run_a = setup_run_with_keys(&db, 1_000);
        let run_b = setup_run_with_keys(&db, 1_000);

        b.iter(|| {
            black_box(db.diff_runs(run_a, run_b).unwrap());
        });
    });

    group.finish();
}

criterion_group!(
    benches,
    snapshot_benchmarks,
    recovery_benchmarks,
    replay_benchmarks
);
criterion_main!(benches);
```

Create `docs/benchmarks/M7_PERFORMANCE_BASELINE.md`:

```markdown
# M7 Performance Baseline

## Overview

This document records the M7 performance baselines established during validation.

## Test Environment

- Hardware: [Document your test machine]
- Rust version: [rustc --version]
- Date: [Date of measurement]

## Baseline Results

### Snapshot Operations

| Operation | Data Size | Target | Measured | Status |
|-----------|-----------|--------|----------|--------|
| Snapshot write | 1 MB | < 500ms | | |
| Snapshot write | 10 MB | < 1s | | |
| Snapshot write | 100 MB | < 5s | | |
| Snapshot load | 1 MB | < 200ms | | |
| Snapshot load | 10 MB | < 500ms | | |
| Snapshot load | 100 MB | < 3s | | |

### Recovery Operations

| Operation | Data Size | Target | Measured | Status |
|-----------|-----------|--------|----------|--------|
| Full recovery (snap + 10K WAL) | 10 MB + 10K | < 5s | | |
| WAL-only recovery | 10K entries | < 1s | | |
| Index rebuild | 10K docs | < 2s | | |

### Replay Operations

| Operation | Run Size | Target | Measured | Status |
|-----------|----------|--------|----------|--------|
| Replay run | 100 events | < 10ms | | |
| Replay run | 1K events | < 100ms | | |
| Diff runs | 1K keys each | < 200ms | | |

## Non-Regression (M4/M5/M6)

| Metric | Previous | Current | Delta | Status |
|--------|----------|---------|-------|--------|
| KV put | < 3µs | | | |
| KV get | < 5µs | | | |
| JSON create | < 1ms | | | |
| Search (indexed) | < 10ms | | | |

## Notes

[Add any notes about the measurements, anomalies, or areas for future optimization]
```

### Acceptance Criteria

- [ ] Benchmark suite created
- [ ] Snapshot benchmarks
- [ ] Recovery benchmarks
- [ ] Replay benchmarks
- [ ] Performance baseline document

### Complete Story

```bash
./scripts/complete-story.sh 384
```

---

## Epic 46 Completion Checklist

### 1. Final Validation

```bash
~/.cargo/bin/cargo test --workspace -- crash_simulation
~/.cargo/bin/cargo test --workspace -- recovery_invariants
~/.cargo/bin/cargo test --workspace -- replay_invariants
~/.cargo/bin/cargo bench --bench m7_performance
```

### 2. Verify All Invariants

- [ ] All 6 crash simulation tests pass
- [ ] All 6 recovery invariants (R1-R6) verified
- [ ] All 6 replay invariants (P1-P6) verified
- [ ] Performance baselines documented

### 3. Merge to Develop

```bash
git checkout develop
git merge --no-ff epic-46-validation -m "Epic 46: Validation & Benchmarks complete

Delivered:
- Crash simulation test suite (6 scenarios)
- Recovery invariant tests (R1-R6)
- Replay determinism tests (P1-P6)
- Performance baseline documentation

All M7 correctness guarantees verified.

Stories: #381, #382, #383, #384
"
git push origin develop
gh issue close 344 --comment "Epic 46: Validation & Benchmarks - COMPLETE"
```

---

## M7 Milestone Completion

After Epic 46 is complete, M7 is ready for final merge:

```bash
# Final validation
~/.cargo/bin/cargo test --workspace
~/.cargo/bin/cargo clippy --workspace -- -D warnings
~/.cargo/bin/cargo fmt --check
~/.cargo/bin/cargo bench

# Merge develop to main
git checkout main
git merge --no-ff develop -m "M7: Durability, Snapshots, Replay & Storage Stabilization - COMPLETE

M7 delivers:
- Periodic snapshots for bounded recovery time
- Crash recovery that is deterministic, idempotent, and prefix-consistent
- Deterministic replay for agent run reconstruction
- Storage APIs frozen for future primitives (Vector in M8)

All invariants verified:
- Recovery invariants (R1-R6): PASS
- Replay invariants (P1-P6): PASS
- Crash simulation tests: PASS

Performance baselines documented.

Epics: #338-#344
Stories: #347-#384
"
git push origin main
```

---

## Summary

Epic 46 validates that M7 meets all correctness requirements. After these tests pass, the database survives crashes correctly, recovers efficiently, and enables deterministic replay of agent runs.
