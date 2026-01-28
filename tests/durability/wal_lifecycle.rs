//! WAL Lifecycle Tests
//!
//! Tests WAL growth, file presence, and interaction with snapshots
//! at the Database level.

use crate::common::*;

#[test]
fn wal_file_exists_after_write_in_strict_mode() {
    let test_db = TestDb::new_strict();
    let run_id = test_db.run_id;

    let kv = test_db.kv();
    kv.put(&run_id, "trigger", Value::Int(1)).unwrap();

    let wal_dir = test_db.wal_dir();
    assert!(wal_dir.exists(), "WAL directory should exist after write");
}

#[test]
fn wal_grows_monotonically_during_writes() {
    let test_db = TestDb::new_strict();
    let run_id = test_db.run_id;

    let kv = test_db.kv();
    let wal_path = test_db.wal_path();

    let mut prev_size = 0u64;
    for i in 0..10 {
        kv.put(&run_id, &format!("k{}", i), Value::Int(i)).unwrap();
        let size = file_size(&wal_path);
        assert!(
            size >= prev_size,
            "WAL should not shrink: {} < {} at iteration {}",
            size,
            prev_size,
            i
        );
        prev_size = size;
    }
}

#[test]
fn wal_contains_data_after_bulk_writes() {
    let test_db = TestDb::new_strict();
    let run_id = test_db.run_id;

    let kv = test_db.kv();
    for i in 0..500 {
        kv.put(&run_id, &format!("bulk_{}", i), Value::Int(i))
            .unwrap();
    }

    let wal_path = test_db.wal_path();
    let size = file_size(&wal_path);
    assert!(size > 0, "WAL should contain data after bulk writes");
    // 500 entries should produce a non-trivial WAL
    assert!(
        size > 1000,
        "WAL for 500 entries should be > 1KB, got {} bytes",
        size
    );
}

#[test]
fn data_written_to_wal_is_recoverable() {
    let mut test_db = TestDb::new_strict();
    let run_id = test_db.run_id;

    let kv = test_db.kv();
    kv.put(&run_id, "wal_key", Value::String("wal_value".into()))
        .unwrap();

    // Delete snapshots to force WAL-only recovery
    delete_snapshots(&test_db.snapshot_dir());

    test_db.reopen();

    let kv = test_db.kv();
    let val = kv.get(&run_id, "wal_key").unwrap();
    assert!(val.is_some(), "Data should be recoverable from WAL alone");
    assert_eq!(val.unwrap().value, Value::String("wal_value".into()));
}

#[test]
fn large_values_in_wal_survive_recovery() {
    let mut test_db = TestDb::new_strict();
    let run_id = test_db.run_id;

    let kv = test_db.kv();
    let large_value = Value::String("x".repeat(100_000)); // 100KB
    kv.put(&run_id, "large", large_value.clone()).unwrap();

    test_db.reopen();

    let kv = test_db.kv();
    let val = kv.get(&run_id, "large").unwrap().unwrap();
    assert_eq!(val.value, large_value, "Large value should survive recovery");
}

#[test]
fn wal_handles_many_small_writes() {
    let mut test_db = TestDb::new_strict();
    let run_id = test_db.run_id;

    let kv = test_db.kv();
    for i in 0..2000 {
        kv.put(&run_id, &format!("small_{}", i), Value::Int(i))
            .unwrap();
    }

    test_db.reopen();

    let kv = test_db.kv();
    // Sample check — don't need to check all 2000
    for i in (0..2000).step_by(100) {
        let val = kv.get(&run_id, &format!("small_{}", i)).unwrap();
        assert!(
            val.is_some(),
            "Key small_{} should survive recovery from large WAL",
            i
        );
    }
}
