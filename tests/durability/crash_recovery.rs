//! Crash Recovery Tests
//!
//! Simulates crash scenarios by corrupting WAL/snapshot files,
//! truncating mid-write, and verifying graceful recovery.

use crate::common::*;

#[test]
fn truncated_wal_recovers_prefix() {
    let mut test_db = TestDb::new_strict();
    let run_id = test_db.run_id;

    let kv = test_db.kv();
    for i in 0..100 {
        kv.put(&run_id, &format!("k{}", i), Value::Int(i)).unwrap();
    }

    // Truncate the WAL to simulate crash mid-write
    let wal_path = test_db.wal_path();
    let wal_size = file_size(&wal_path);
    if wal_size > 200 {
        truncate_file(&wal_path, wal_size * 3 / 4);
    }

    // Recovery should not panic, and should recover a prefix of the data
    test_db.reopen();

    let kv = test_db.kv();
    // Early keys are more likely to survive truncation
    let recovered = (0..100)
        .filter(|i| {
            kv.get(&run_id, &format!("k{}", i))
                .unwrap()
                .is_some()
        })
        .count();

    // At minimum, some prefix should survive
    assert!(
        recovered > 0,
        "At least some data should survive WAL truncation"
    );
    // Later keys may be lost due to truncation — that's acceptable
}

#[test]
fn corrupted_wal_tail_recovers_valid_prefix() {
    let mut test_db = TestDb::new_strict();
    let run_id = test_db.run_id;

    let kv = test_db.kv();
    for i in 0..50 {
        kv.put(&run_id, &format!("k{}", i), Value::Int(i)).unwrap();
    }

    // Corrupt the tail of the WAL (simulates incomplete write)
    let wal_path = test_db.wal_path();
    let wal_size = file_size(&wal_path);
    if wal_size > 100 {
        corrupt_file_at_offset(&wal_path, wal_size - 20, &[0xFF; 20]);
    }

    // Delete snapshots to force WAL-only recovery
    delete_snapshots(&test_db.snapshot_dir());

    test_db.reopen();

    // Database should be functional after recovery
    assert_db_healthy(&test_db.db, &run_id);
}

#[test]
fn completely_corrupted_wal_still_boots() {
    let mut test_db = TestDb::new_strict();
    let run_id = test_db.run_id;

    let kv = test_db.kv();
    kv.put(&run_id, "before_corruption", Value::Int(1)).unwrap();

    // Completely trash the WAL file
    let wal_path = test_db.wal_path();
    if wal_path.exists() {
        std::fs::write(&wal_path, vec![0xFF; 1000]).unwrap();
    }

    // Delete snapshots too
    delete_snapshots(&test_db.snapshot_dir());

    // Database should still open (empty state is acceptable)
    test_db.reopen();

    // Should be functional for new writes
    let kv = test_db.kv();
    kv.put(&run_id, "after_corruption", Value::Int(2)).unwrap();
    let val = kv.get(&run_id, "after_corruption").unwrap();
    assert!(val.is_some(), "New writes should work after corrupted WAL recovery");
}

#[test]
fn missing_wal_file_starts_fresh() {
    let mut test_db = TestDb::new_strict();
    let run_id = test_db.run_id;

    let kv = test_db.kv();
    kv.put(&run_id, "will_be_lost", Value::Int(1)).unwrap();

    // Delete WAL and snapshots
    let wal_path = test_db.wal_path();
    if wal_path.exists() {
        std::fs::remove_file(&wal_path).unwrap();
    }
    delete_snapshots(&test_db.snapshot_dir());

    test_db.reopen();

    // Database should start fresh — old data is gone
    let kv = test_db.kv();
    assert_db_healthy(&test_db.db, &run_id);
}

#[test]
fn reopen_after_no_writes_succeeds() {
    let mut test_db = TestDb::new_strict();

    // No writes at all
    test_db.reopen();

    // Should be healthy
    assert_db_healthy(&test_db.db, &test_db.run_id);
}

#[test]
fn rapid_reopen_cycles_are_stable() {
    let mut test_db = TestDb::new_strict();
    let run_id = test_db.run_id;

    // Write → reopen cycle 5 times
    for cycle in 0..5 {
        let kv = test_db.kv();
        kv.put(
            &run_id,
            &format!("cycle_{}", cycle),
            Value::Int(cycle as i64),
        )
        .unwrap();
        test_db.reopen();
    }

    // All cycle keys should exist
    let kv = test_db.kv();
    for cycle in 0..5 {
        let val = kv
            .get(&run_id, &format!("cycle_{}", cycle))
            .unwrap();
        assert!(
            val.is_some(),
            "Key from cycle {} should survive rapid reopen",
            cycle
        );
    }
}

#[test]
fn recovery_after_high_churn_on_same_keys() {
    let mut test_db = TestDb::new_strict();
    let run_id = test_db.run_id;

    let kv = test_db.kv();
    // Overwrite same 10 keys 100 times each
    for round in 0..100 {
        for key_idx in 0..10 {
            kv.put(
                &run_id,
                &format!("churn_{}", key_idx),
                Value::Int(round * 10 + key_idx),
            )
            .unwrap();
        }
    }

    test_db.reopen();

    // Each key should have the last-written value
    let kv = test_db.kv();
    for key_idx in 0..10i64 {
        let val = kv
            .get(&run_id, &format!("churn_{}", key_idx))
            .unwrap()
            .unwrap();
        assert_eq!(
            val.value,
            Value::Int(99 * 10 + key_idx),
            "Key churn_{} should have last-written value",
            key_idx
        );
    }
}
