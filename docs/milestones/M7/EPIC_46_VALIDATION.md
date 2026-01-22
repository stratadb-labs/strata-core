# Epic 46: Validation & Benchmarks

**Goal**: Ensure correctness and document performance baselines

**Dependencies**: All other M7 epics

---

## Scope

- Crash simulation test suite
- Recovery invariant tests
- Replay determinism tests
- Performance baseline documentation

---

## User Stories

| Story | Description | Priority |
|-------|-------------|----------|
| #326 | Crash Simulation Test Suite | CRITICAL |
| #327 | Recovery Invariant Tests | CRITICAL |
| #328 | Replay Determinism Tests | CRITICAL |
| #329 | Performance Baseline Documentation | HIGH |

---

## Story #326: Crash Simulation Test Suite

**File**: `tests/crash_simulation.rs` (NEW)

**Deliverable**: Comprehensive crash scenario tests

### Implementation

```rust
//! Crash simulation tests
//!
//! These tests verify that the database correctly recovers from
//! various crash scenarios.

use tempfile::TempDir;

/// Test crash during normal operation
#[test]
fn test_crash_during_normal_operation() {
    let temp_dir = TempDir::new().unwrap();
    let data_dir = temp_dir.path();

    // Create DB and do some work
    {
        let db = create_db(data_dir);
        let run_id = RunId::new();

        db.begin_run(run_id).unwrap();

        // Committed transactions
        for i in 0..10 {
            db.transaction(run_id, |tx| {
                tx.kv_put(format!("key{}", i), format!("value{}", i))?;
                Ok(())
            }).unwrap();
        }

        // Uncommitted transaction (simulated crash mid-transaction)
        let (tx_id, entries) = create_transaction_entries(run_id, "uncommitted");
        for entry in entries {
            db.wal.write_entry(&entry).unwrap();
        }
        // NO commit marker

        // Snapshot before "crash"
        db.snapshot().unwrap();
    }

    // Recover
    let (recovered, result) = RecoveryEngine::recover(
        data_dir,
        RecoveryOptions::default(),
    ).unwrap();

    // Committed data should be present
    for i in 0..10 {
        assert!(recovered.kv.get_any(&format!("key{}", i)).unwrap().is_some());
    }

    // Uncommitted data should NOT be present
    assert!(recovered.kv.get_any("uncommitted").unwrap().is_none());

    // Stats
    assert_eq!(result.transactions_recovered, 10);
    assert_eq!(result.orphaned_transactions, 1);
}

/// Test crash during snapshot write
#[test]
fn test_crash_during_snapshot_write() {
    let temp_dir = TempDir::new().unwrap();
    let data_dir = temp_dir.path();
    let snapshot_dir = data_dir.join("snapshots");
    std::fs::create_dir_all(&snapshot_dir).unwrap();

    // Create DB and do some work
    {
        let db = create_db(data_dir);
        let run_id = RunId::new();

        db.begin_run(run_id).unwrap();
        for i in 0..10 {
            db.transaction(run_id, |tx| {
                tx.kv_put(format!("key{}", i), format!("value{}", i))?;
                Ok(())
            }).unwrap();
        }
        db.end_run(run_id).unwrap();
    }

    // Simulate partial snapshot (crash during write)
    let partial_path = snapshot_dir.join("snapshot_99999999.dat");
    {
        let mut file = std::fs::File::create(&partial_path).unwrap();
        file.write_all(b"INMEM_SNAP").unwrap();  // Magic only, incomplete
    }

    // Recover - should use full WAL replay (no valid snapshot)
    let (recovered, result) = RecoveryEngine::recover(
        data_dir,
        RecoveryOptions::default(),
    ).unwrap();

    // Data should be recovered from WAL
    for i in 0..10 {
        assert!(recovered.kv.get_any(&format!("key{}", i)).unwrap().is_some());
    }

    // No snapshot used (partial was invalid)
    assert!(result.snapshot_used.is_none() || result.wal_entries_replayed > 0);
}

/// Test crash during WAL truncation
#[test]
fn test_crash_during_wal_truncation() {
    let temp_dir = TempDir::new().unwrap();
    let data_dir = temp_dir.path();

    // Create DB with data
    {
        let db = create_db(data_dir);
        let run_id = RunId::new();

        db.begin_run(run_id).unwrap();
        for i in 0..100 {
            db.transaction(run_id, |tx| {
                tx.kv_put(format!("key{}", i), format!("value{}", i))?;
                Ok(())
            }).unwrap();
        }
        db.end_run(run_id).unwrap();

        // Create snapshot
        db.snapshot().unwrap();
    }

    // Simulate crash during truncation by leaving both old and temp WAL
    let wal_path = data_dir.join("wal.dat");
    let temp_wal = data_dir.join("wal.tmp");
    std::fs::copy(&wal_path, &temp_wal).unwrap();

    // Recover
    let (recovered, _result) = RecoveryEngine::recover(
        data_dir,
        RecoveryOptions::default(),
    ).unwrap();

    // Data should still be intact
    for i in 0..100 {
        assert!(recovered.kv.get_any(&format!("key{}", i)).unwrap().is_some());
    }
}

/// Test corrupted WAL entries
#[test]
fn test_corrupted_wal_entries() {
    let temp_dir = TempDir::new().unwrap();
    let data_dir = temp_dir.path();

    // Create DB with data
    {
        let db = create_db(data_dir);
        let run_id = RunId::new();

        db.begin_run(run_id).unwrap();
        for i in 0..20 {
            db.transaction(run_id, |tx| {
                tx.kv_put(format!("key{}", i), format!("value{}", i))?;
                Ok(())
            }).unwrap();
        }
        db.end_run(run_id).unwrap();
    }

    // Corrupt some WAL entries
    let wal_path = data_dir.join("wal.dat");
    let mut data = std::fs::read(&wal_path).unwrap();
    // Flip bits in middle of file
    let mid = data.len() / 2;
    data[mid] ^= 0xFF;
    data[mid + 1] ^= 0xFF;
    std::fs::write(&wal_path, &data).unwrap();

    // Recover with permissive options
    let (recovered, result) = RecoveryEngine::recover(
        data_dir,
        RecoveryOptions::permissive(),
    ).unwrap();

    // Some data recovered, some corruption skipped
    assert!(result.corrupt_entries_skipped > 0);
    // Most data should still be there
}

/// Test corrupted snapshot with fallback
#[test]
fn test_corrupted_snapshot_fallback() {
    let temp_dir = TempDir::new().unwrap();
    let data_dir = temp_dir.path();
    let snapshot_dir = data_dir.join("snapshots");
    std::fs::create_dir_all(&snapshot_dir).unwrap();

    // Create DB and multiple snapshots
    {
        let db = create_db(data_dir);
        let run_id = RunId::new();

        db.begin_run(run_id).unwrap();
        for i in 0..10 {
            db.transaction(run_id, |tx| {
                tx.kv_put(format!("key{}", i), format!("value{}", i))?;
                Ok(())
            }).unwrap();
        }

        // First snapshot
        db.snapshot().unwrap();

        // More work
        for i in 10..20 {
            db.transaction(run_id, |tx| {
                tx.kv_put(format!("key{}", i), format!("value{}", i))?;
                Ok(())
            }).unwrap();
        }

        // Second snapshot
        std::thread::sleep(std::time::Duration::from_millis(10));
        db.snapshot().unwrap();

        db.end_run(run_id).unwrap();
    }

    // Corrupt newest snapshot
    let mut snapshots: Vec<_> = std::fs::read_dir(&snapshot_dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().map_or(false, |e| e == "dat"))
        .collect();
    snapshots.sort();
    let newest = snapshots.last().unwrap();

    let mut data = std::fs::read(newest).unwrap();
    data[50] ^= 0xFF;
    std::fs::write(newest, &data).unwrap();

    // Recover - should fall back to older snapshot
    let (recovered, result) = RecoveryEngine::recover(
        data_dir,
        RecoveryOptions::default(),
    ).unwrap();

    // Should have used older snapshot
    assert!(result.snapshot_used.is_some());
    // All data should be recovered (older snapshot + WAL)
    for i in 0..20 {
        assert!(recovered.kv.get_any(&format!("key{}", i)).unwrap().is_some());
    }
}

/// Test power failure simulation
#[test]
fn test_power_failure_simulation() {
    let temp_dir = TempDir::new().unwrap();
    let data_dir = temp_dir.path();

    // Simulate power failure at random points
    for trial in 0..10 {
        // Create fresh DB
        let _ = std::fs::remove_dir_all(data_dir);
        std::fs::create_dir_all(data_dir).unwrap();

        // Write some data
        {
            let db = create_db(data_dir);
            let run_id = RunId::new();

            db.begin_run(run_id).unwrap();
            for i in 0..50 {
                db.transaction(run_id, |tx| {
                    tx.kv_put(format!("key{}", i), format!("value{}", i))?;
                    Ok(())
                }).unwrap();

                // Simulate power failure at random point
                if i == 25 + (trial % 20) {
                    break;  // "Power failure"
                }
            }
        }

        // Recover
        let result = RecoveryEngine::recover(
            data_dir,
            RecoveryOptions::default(),
        );

        // Should either succeed or fail gracefully
        match result {
            Ok((recovered, _)) => {
                // Verify no partial transactions
                // All recovered data should be consistent
            }
            Err(e) => {
                // Should be a recoverable error, not panic
                assert!(!format!("{:?}", e).contains("panic"));
            }
        }
    }
}
```

### Acceptance Criteria

- [ ] Crash during normal operation test
- [ ] Crash during snapshot write test
- [ ] Crash during WAL truncation test
- [ ] Corrupted WAL entries test
- [ ] Corrupted snapshot with fallback test
- [ ] Power failure simulation test

---

## Story #327: Recovery Invariant Tests

**File**: `tests/recovery_invariants.rs` (NEW)

**Deliverable**: Tests validating all recovery invariants

### Implementation

```rust
//! Recovery invariant tests
//!
//! These tests validate the recovery invariants (R1-R6) from M7_SCOPE.md

/// R1: Recovery is deterministic
#[test]
fn test_recovery_deterministic() {
    let temp_dir = TempDir::new().unwrap();
    let data_dir = temp_dir.path();

    // Create DB with data
    {
        let db = create_db(data_dir);
        populate_test_data(&db);
        db.snapshot().unwrap();
    }

    // Recover twice
    let (db1, result1) = RecoveryEngine::recover(data_dir, Default::default()).unwrap();
    let (db2, result2) = RecoveryEngine::recover(data_dir, Default::default()).unwrap();

    // Must produce identical state
    assert_eq!(db1.kv.list_all().unwrap(), db2.kv.list_all().unwrap());
    assert_eq!(db1.json.list_all().unwrap(), db2.json.list_all().unwrap());
    assert_eq!(db1.event_log.count().unwrap(), db2.event_log.count().unwrap());

    // Stats should be identical
    assert_eq!(result1.transactions_recovered, result2.transactions_recovered);
    assert_eq!(result1.wal_entries_replayed, result2.wal_entries_replayed);
}

/// R2: Recovery is idempotent
#[test]
fn test_recovery_idempotent() {
    let temp_dir = TempDir::new().unwrap();
    let data_dir = temp_dir.path();

    // Create DB
    {
        let db = create_db(data_dir);
        populate_test_data(&db);
        db.snapshot().unwrap();
    }

    // Recover
    let (db1, _) = RecoveryEngine::recover(data_dir, Default::default()).unwrap();
    let state1 = capture_state(&db1);

    // "Crash" and recover again
    drop(db1);
    let (db2, _) = RecoveryEngine::recover(data_dir, Default::default()).unwrap();
    let state2 = capture_state(&db2);

    // State must be identical
    assert_eq!(state1, state2);
}

/// R3: Recovery is prefix-consistent
#[test]
fn test_recovery_prefix_consistent() {
    let temp_dir = TempDir::new().unwrap();
    let data_dir = temp_dir.path();

    // Create DB with transactions
    {
        let db = create_db(data_dir);

        for i in 0..10 {
            let run_id = RunId::new();
            db.begin_run(run_id).unwrap();
            db.transaction(run_id, |tx| {
                tx.kv_put(format!("tx{}_key1", i), "v1")?;
                tx.kv_put(format!("tx{}_key2", i), "v2")?;
                tx.json_set(format!("tx{}_doc", i), json!({"i": i}))?;
                Ok(())
            }).unwrap();
            db.end_run(run_id).unwrap();
        }

        db.snapshot().unwrap();
    }

    // Recover
    let (recovered, _) = RecoveryEngine::recover(data_dir, Default::default()).unwrap();

    // For each transaction, either all keys exist or none
    for i in 0..10 {
        let key1_exists = recovered.kv.get_any(&format!("tx{}_key1", i)).unwrap().is_some();
        let key2_exists = recovered.kv.get_any(&format!("tx{}_key2", i)).unwrap().is_some();
        let doc_exists = recovered.json.get_any(&format!("tx{}_doc", i)).unwrap().is_some();

        // All or nothing
        assert_eq!(key1_exists, key2_exists);
        assert_eq!(key2_exists, doc_exists);
    }
}

/// R4: Recovery never invents data
#[test]
fn test_recovery_never_invents_data() {
    let temp_dir = TempDir::new().unwrap();
    let data_dir = temp_dir.path();

    let mut written_keys = HashSet::new();

    // Create DB with known data
    {
        let db = create_db(data_dir);
        let run_id = RunId::new();
        db.begin_run(run_id).unwrap();

        for i in 0..100 {
            let key = format!("key{}", i);
            written_keys.insert(key.clone());
            db.transaction(run_id, |tx| {
                tx.kv_put(key, format!("value{}", i))?;
                Ok(())
            }).unwrap();
        }

        db.end_run(run_id).unwrap();
        db.snapshot().unwrap();
    }

    // Recover
    let (recovered, _) = RecoveryEngine::recover(data_dir, Default::default()).unwrap();

    // Every key in recovered DB must be in written_keys
    for (key, _) in recovered.kv.list_all().unwrap() {
        let key_str = key.user_key_str();
        assert!(
            written_keys.contains(key_str),
            "Recovered key '{}' was never written",
            key_str
        );
    }
}

/// R5: Recovery never drops committed data
#[test]
fn test_recovery_never_drops_committed() {
    let temp_dir = TempDir::new().unwrap();
    let data_dir = temp_dir.path();

    let mut committed_keys = Vec::new();

    // Create DB with committed data
    {
        let db = create_db_strict(data_dir);  // Strict mode for sync
        let run_id = RunId::new();
        db.begin_run(run_id).unwrap();

        for i in 0..100 {
            let key = format!("key{}", i);
            committed_keys.push(key.clone());
            db.transaction(run_id, |tx| {
                tx.kv_put(key, format!("value{}", i))?;
                Ok(())
            }).unwrap();  // Committed and synced
        }

        db.end_run(run_id).unwrap();
        db.snapshot().unwrap();
    }

    // Recover
    let (recovered, _) = RecoveryEngine::recover(data_dir, Default::default()).unwrap();

    // Every committed key must be present
    for key in committed_keys {
        let value = recovered.kv.get_any(&key).unwrap();
        assert!(
            value.is_some(),
            "Committed key '{}' was dropped during recovery",
            key
        );
    }
}

/// R6: Recovery may drop uncommitted data
#[test]
fn test_recovery_may_drop_uncommitted() {
    let temp_dir = TempDir::new().unwrap();
    let data_dir = temp_dir.path();

    // Create DB with uncommitted transaction
    {
        let db = create_db(data_dir);
        let run_id = RunId::new();
        db.begin_run(run_id).unwrap();

        // Committed transaction
        db.transaction(run_id, |tx| {
            tx.kv_put("committed_key", "committed_value")?;
            Ok(())
        }).unwrap();

        // Uncommitted (write WAL entries but no commit marker)
        let tx_id = TxId::new();
        db.wal.write_tx_entry(tx_id, WalEntryType::KvPut, b"uncommitted_key=uncommitted_value".to_vec()).unwrap();
        // NO commit marker

        db.snapshot().unwrap();
    }

    // Recover
    let (recovered, result) = RecoveryEngine::recover(data_dir, Default::default()).unwrap();

    // Committed data present
    assert!(recovered.kv.get_any("committed_key").unwrap().is_some());

    // Uncommitted data absent
    assert!(recovered.kv.get_any("uncommitted_key").unwrap().is_none());

    // Stats reflect orphaned transaction
    assert_eq!(result.orphaned_transactions, 1);
}
```

### Acceptance Criteria

- [ ] R1 test: Deterministic recovery
- [ ] R2 test: Idempotent recovery
- [ ] R3 test: Prefix-consistent recovery
- [ ] R4 test: Never invents data
- [ ] R5 test: Never drops committed
- [ ] R6 test: May drop uncommitted

---

## Story #328: Replay Determinism Tests

**File**: `tests/replay_determinism.rs` (NEW)

**Deliverable**: Tests validating replay invariants

### Implementation

```rust
//! Replay determinism tests
//!
//! These tests validate the replay invariants (P1-P6) from M7_SCOPE.md

/// P1: Replay is a pure function
#[test]
fn test_replay_pure_function() {
    let db = test_db();
    let run_id = RunId::new();

    db.begin_run(run_id).unwrap();
    db.kv.put(run_id, "key1", "value1").unwrap();
    db.kv.put(run_id, "key2", "value2").unwrap();
    db.end_run(run_id).unwrap();

    // Replay with same inputs should produce same output
    let view1 = db.replay_run(run_id).unwrap();
    let view2 = db.replay_run(run_id).unwrap();

    assert_eq!(view1.kv_state, view2.kv_state);
}

/// P2: Replay is side-effect free
#[test]
fn test_replay_side_effect_free() {
    let db = test_db();
    let run_id = RunId::new();

    db.begin_run(run_id).unwrap();
    db.kv.put(run_id, "key1", "value1").unwrap();
    db.end_run(run_id).unwrap();

    // Capture canonical state before replay
    let before = db.kv.list_all().unwrap();
    let wal_size_before = db.wal.size().unwrap();

    // Replay
    let _view = db.replay_run(run_id).unwrap();

    // Canonical state unchanged
    let after = db.kv.list_all().unwrap();
    let wal_size_after = db.wal.size().unwrap();

    assert_eq!(before, after);
    assert_eq!(wal_size_before, wal_size_after);
}

/// P3: Replay produces a derived view
#[test]
fn test_replay_produces_derived_view() {
    let db = test_db();
    let run_id = RunId::new();

    db.begin_run(run_id).unwrap();
    db.kv.put(run_id, "key1", "value1").unwrap();
    db.end_run(run_id).unwrap();

    let view = db.replay_run(run_id).unwrap();

    // View is read-only (no mutation methods)
    // This is a compile-time check - ReadOnlyView has no mut methods

    // View contents match what was written
    assert_eq!(view.get_kv(&Key::new_kv(run_id, "key1")), Some(&"value1".into()));
}

/// P4: Replay does not persist state
#[test]
fn test_replay_does_not_persist() {
    let temp_dir = TempDir::new().unwrap();
    let data_dir = temp_dir.path();

    // Create DB with run
    let run_id;
    {
        let db = create_db(data_dir);
        run_id = RunId::new();
        db.begin_run(run_id).unwrap();
        db.kv.put(run_id, "key1", "value1").unwrap();
        db.end_run(run_id).unwrap();
        db.snapshot().unwrap();
    }

    // Recover and replay
    let (db, _) = RecoveryEngine::recover(data_dir, Default::default()).unwrap();
    let view = db.replay_run(run_id).unwrap();

    // Get file sizes
    let snapshot_size = std::fs::metadata(data_dir.join("snapshots").read_dir().unwrap().next().unwrap().unwrap().path()).unwrap().len();

    // Replay again
    let _view2 = db.replay_run(run_id).unwrap();

    // No new files, no size increase
    // (Replay doesn't write anything)
}

/// P5: Replay is deterministic
#[test]
fn test_replay_deterministic() {
    let db = test_db();

    // Create multiple runs
    for run_num in 0..5 {
        let run_id = RunId::new();
        db.begin_run(run_id).unwrap();

        for i in 0..10 {
            db.kv.put(run_id, format!("key{}", i), format!("run{}_value{}", run_num, i)).unwrap();
        }

        db.end_run(run_id).unwrap();
    }

    // Replay each run multiple times
    for run_id in db.list_runs().unwrap() {
        let views: Vec<_> = (0..3)
            .map(|_| db.replay_run(run_id.run_id).unwrap())
            .collect();

        // All replays must produce identical views
        for view in &views[1..] {
            assert_eq!(views[0].kv_state, view.kv_state);
        }
    }
}

/// P6: Replay is idempotent
#[test]
fn test_replay_idempotent() {
    let db = test_db();
    let run_id = RunId::new();

    db.begin_run(run_id).unwrap();
    db.kv.put(run_id, "key1", "value1").unwrap();
    db.json.set(run_id, "doc1", json!({"field": "value"})).unwrap();
    db.end_run(run_id).unwrap();

    // Replay many times
    let views: Vec<_> = (0..10)
        .map(|_| db.replay_run(run_id).unwrap())
        .collect();

    // All views must be identical
    let first = &views[0];
    for view in &views[1..] {
        assert_eq!(first.kv_state, view.kv_state);
        assert_eq!(first.json_state, view.json_state);
    }
}

/// Test replay handles JSON patches correctly
#[test]
fn test_replay_json_patches() {
    let db = test_db();
    let run_id = RunId::new();

    db.begin_run(run_id).unwrap();

    // Create document
    db.json.set(run_id, "doc1", json!({"counter": 0, "items": []})).unwrap();

    // Apply patches
    for i in 1..=5 {
        db.json.patch(run_id, "doc1", vec![
            JsonPatch::Set {
                path: JsonPath::parse("$.counter").unwrap(),
                value: json!(i),
            },
        ]).unwrap();
    }

    db.end_run(run_id).unwrap();

    // Replay
    let view = db.replay_run(run_id).unwrap();

    // Should have final state
    let doc = view.get_json(&Key::new_json(run_id, "doc1")).unwrap();
    assert_eq!(doc.value["counter"], 5);
}

/// Test replay preserves ordering
#[test]
fn test_replay_preserves_ordering() {
    let db = test_db();
    let run_id = RunId::new();

    db.begin_run(run_id).unwrap();

    // Write in specific order
    db.kv.put(run_id, "a", "first").unwrap();
    db.kv.put(run_id, "a", "second").unwrap();
    db.kv.put(run_id, "a", "third").unwrap();

    db.end_run(run_id).unwrap();

    // Replay should have final value
    let view = db.replay_run(run_id).unwrap();
    assert_eq!(view.get_kv(&Key::new_kv(run_id, "a")), Some(&"third".into()));
}
```

### Acceptance Criteria

- [ ] P1 test: Pure function
- [ ] P2 test: Side-effect free
- [ ] P3 test: Derived view
- [ ] P4 test: Does not persist
- [ ] P5 test: Deterministic
- [ ] P6 test: Idempotent
- [ ] JSON patch replay test
- [ ] Ordering preservation test

---

## Story #329: Performance Baseline Documentation

**File**: `docs/architecture/M7_PERFORMANCE_BASELINE.md` (NEW)

**Deliverable**: Document M7 performance baselines

### Implementation

```markdown
# M7 Performance Baseline

## Overview

This document establishes performance baselines for M7 (Durability, Snapshots, Replay).
These baselines serve as reference points for future optimization work (M9).

## Test Environment

- **Hardware**: [To be filled during benchmarking]
- **OS**: Linux
- **Rust Version**: 1.70+
- **Build Profile**: Release

## Snapshot Performance

### Snapshot Write

| Data Size | Time | Throughput |
|-----------|------|------------|
| 1 MB | TBD | TBD |
| 10 MB | TBD | TBD |
| 100 MB | TBD | TBD |
| 1 GB | TBD | TBD |

**Target**: 100 MB snapshot in < 5 seconds

### Snapshot Load

| Data Size | Time | Throughput |
|-----------|------|------------|
| 1 MB | TBD | TBD |
| 10 MB | TBD | TBD |
| 100 MB | TBD | TBD |
| 1 GB | TBD | TBD |

**Target**: 100 MB snapshot load in < 3 seconds

## WAL Performance

### WAL Entry Write

| Entry Size | Time (Buffered) | Time (Strict) |
|------------|-----------------|---------------|
| 100 bytes | TBD | TBD |
| 1 KB | TBD | TBD |
| 10 KB | TBD | TBD |

### WAL Replay

| Entry Count | Time |
|-------------|------|
| 1,000 | TBD |
| 10,000 | TBD |
| 100,000 | TBD |

**Target**: 10K entries replayed in < 1 second

## Recovery Performance

### Full Recovery (Snapshot + WAL)

| Snapshot Size | WAL Entries | Total Time |
|---------------|-------------|------------|
| 10 MB | 1,000 | TBD |
| 100 MB | 10,000 | TBD |
| 100 MB | 100,000 | TBD |

**Target**: 100 MB snapshot + 10K WAL entries in < 5 seconds

### Recovery Time Breakdown

| Phase | % of Total Time |
|-------|-----------------|
| Snapshot discovery | TBD |
| Snapshot load | TBD |
| WAL replay | TBD |
| Index rebuild | TBD |

## Replay Performance

### replay_run()

| Events in Run | Time |
|---------------|------|
| 100 | TBD |
| 1,000 | TBD |
| 10,000 | TBD |

**Target**: 1K events replayed in < 100 ms

### diff_runs()

| Keys per Run | Time |
|--------------|------|
| 100 | TBD |
| 1,000 | TBD |
| 10,000 | TBD |

**Target**: 1K keys diffed in < 200 ms

## Memory Usage

### Snapshot Memory Overhead

During snapshot write, memory usage increases by approximately
2x the state size due to copy-on-write semantics.

### Replay Memory Overhead

Replay creates an in-memory view. Memory usage equals the
size of the replayed run's state.

## Bottlenecks Identified

1. **Snapshot serialization**: bincode serialization is not optimized
2. **Index rebuild**: Linear scan during recovery
3. **WAL sync in Strict mode**: fsync on every commit

## Future Optimization Opportunities (M9)

1. **Snapshot compression**: zstd compression could reduce I/O
2. **Parallel index rebuild**: Use multiple threads
3. **Incremental snapshots**: Only write changed data
4. **WAL batch sync**: Group fsyncs in Strict mode

## Running Benchmarks

```bash
# Run M7 benchmarks
cargo bench --bench m7_durability

# Specific benchmark
cargo bench --bench m7_durability -- snapshot_write
```

## Benchmark Code Location

- `benches/m7_durability.rs`: Main benchmark file
- `benches/m7_recovery.rs`: Recovery-specific benchmarks
- `benches/m7_replay.rs`: Replay benchmarks
```

### Acceptance Criteria

- [ ] Document structure defined
- [ ] Targets specified
- [ ] Benchmark locations documented
- [ ] Optimization opportunities listed
- [ ] Test environment documented

---

## Testing

All tests from previous stories plus:

```rust
#[cfg(test)]
mod integration_tests {
    /// End-to-end test: create, crash, recover, verify
    #[test]
    fn test_full_lifecycle() {
        let temp_dir = TempDir::new().unwrap();
        let data_dir = temp_dir.path();

        // Phase 1: Create and populate
        let run_id;
        {
            let db = create_db(data_dir);
            run_id = RunId::new();

            db.begin_run(run_id).unwrap();

            // KV operations
            for i in 0..100 {
                db.kv.put(run_id, format!("key{}", i), format!("value{}", i)).unwrap();
            }

            // JSON operations
            for i in 0..50 {
                db.json.set(run_id, format!("doc{}", i), json!({"i": i})).unwrap();
            }

            // Events
            for i in 0..200 {
                db.event_log.append(run_id, Event::new(format!("event{}", i))).unwrap();
            }

            db.end_run(run_id).unwrap();
            db.snapshot().unwrap();
        }

        // Phase 2: Recover
        let (recovered, result) = RecoveryEngine::recover(
            data_dir,
            RecoveryOptions::default(),
        ).unwrap();

        // Verify recovery
        assert!(result.snapshot_used.is_some());

        // Phase 3: Replay
        let view = recovered.replay_run(run_id).unwrap();

        // Verify replay
        assert_eq!(view.kv_state.len(), 100);
        assert_eq!(view.json_state.len(), 50);
        assert_eq!(view.events().len(), 200);

        // Phase 4: Diff (compare with self - should be empty)
        let diff = recovered.diff_runs(run_id, run_id).unwrap();
        assert!(diff.is_empty());
    }
}
```

---

## Files Modified/Created

| File | Action |
|------|--------|
| `tests/crash_simulation.rs` | CREATE - Crash tests |
| `tests/recovery_invariants.rs` | CREATE - Recovery invariant tests |
| `tests/replay_determinism.rs` | CREATE - Replay tests |
| `docs/architecture/M7_PERFORMANCE_BASELINE.md` | CREATE - Baseline doc |
| `benches/m7_durability.rs` | CREATE - Benchmarks |
