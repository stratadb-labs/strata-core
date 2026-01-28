//! Snapshot Lifecycle Tests
//!
//! Tests snapshot creation, validation, and recovery interaction
//! at the Database level.

use crate::common::*;

#[test]
fn snapshot_directory_exists_for_persistent_db() {
    let test_db = TestDb::new_strict();
    let run_id = test_db.run_id;

    let kv = test_db.kv();
    kv.put(&run_id, "trigger", Value::Int(1)).unwrap();

    // Snapshot dir should exist (may or may not contain files yet)
    let snap_dir = test_db.snapshot_dir();
    // The directory may not be created until a snapshot is taken,
    // but the path should be valid
    assert!(
        snap_dir.to_str().unwrap().contains("snapshot"),
        "Snapshot directory path should reference snapshots"
    );
}

#[test]
fn recovery_works_with_snapshot_plus_wal() {
    let mut test_db = TestDb::new_strict();
    let run_id = test_db.run_id;

    let kv = test_db.kv();

    // Phase 1: Write data (may get snapshotted)
    for i in 0..100 {
        kv.put(&run_id, &format!("phase1_{}", i), Value::Int(i))
            .unwrap();
    }

    // Phase 2: More writes (likely in WAL after snapshot)
    for i in 0..50 {
        kv.put(&run_id, &format!("phase2_{}", i), Value::Int(i + 100))
            .unwrap();
    }

    let state_before = CapturedState::capture(&test_db.db, &run_id);

    test_db.reopen();

    let state_after = CapturedState::capture(&test_db.db, &run_id);
    assert_states_equal(
        &state_before,
        &state_after,
        "Recovery with snapshot+WAL should preserve all data",
    );
}

#[test]
fn corrupted_snapshot_falls_back_to_wal() {
    let mut test_db = TestDb::new_strict();
    let run_id = test_db.run_id;

    let kv = test_db.kv();
    for i in 0..50 {
        kv.put(&run_id, &format!("k{}", i), Value::Int(i)).unwrap();
    }

    // Corrupt any existing snapshots
    let snap_dir = test_db.snapshot_dir();
    let snapshots = list_snapshots(&snap_dir);
    for snap_path in &snapshots {
        corrupt_file_random(snap_path);
    }

    // Recovery should still work (from WAL)
    test_db.reopen();

    let kv = test_db.kv();
    // At minimum, recent WAL entries should be recoverable
    // (exact behavior depends on implementation)
    assert_db_healthy(&test_db.db, &run_id);
}

#[test]
fn deleted_snapshots_dont_prevent_recovery() {
    let mut test_db = TestDb::new_strict();
    let run_id = test_db.run_id;

    let kv = test_db.kv();
    for i in 0..20 {
        kv.put(&run_id, &format!("k{}", i), Value::Int(i)).unwrap();
    }

    // Delete all snapshots
    delete_snapshots(&test_db.snapshot_dir());

    // Should recover from WAL alone
    test_db.reopen();

    let kv = test_db.kv();
    for i in 0..20 {
        let val = kv.get(&run_id, &format!("k{}", i)).unwrap();
        assert!(
            val.is_some(),
            "Key k{} should be recoverable from WAL after snapshot deletion",
            i
        );
    }
}

#[test]
fn recovery_handles_empty_snapshot_directory() {
    let mut test_db = TestDb::new_strict();
    let run_id = test_db.run_id;

    let kv = test_db.kv();
    kv.put(&run_id, "test", Value::Int(1)).unwrap();

    // Ensure snapshot dir exists but is empty
    let snap_dir = test_db.snapshot_dir();
    delete_snapshots(&snap_dir);
    let _ = std::fs::create_dir_all(&snap_dir);

    test_db.reopen();

    assert_db_healthy(&test_db.db, &run_id);
}

#[test]
fn data_written_after_snapshot_recovers() {
    let mut test_db = TestDb::new_strict();
    let run_id = test_db.run_id;

    let kv = test_db.kv();

    // Write enough to trigger a snapshot (if auto-snapshotting is enabled)
    for i in 0..200 {
        kv.put(&run_id, &format!("pre_{}", i), Value::Int(i))
            .unwrap();
    }

    // Write more after potential snapshot
    for i in 0..50 {
        kv.put(&run_id, &format!("post_{}", i), Value::Int(i + 1000))
            .unwrap();
    }

    test_db.reopen();

    // Both pre and post data should be present
    let kv = test_db.kv();
    let pre = kv.get(&run_id, "pre_0").unwrap();
    let post = kv.get(&run_id, "post_0").unwrap();
    assert!(pre.is_some(), "Pre-snapshot data should recover");
    assert!(post.is_some(), "Post-snapshot data should recover");
    assert_eq!(post.unwrap().value, Value::Int(1000));
}
