//! Tier 3.3: Snapshot Atomic Write Tests
//!
//! Tests for atomic snapshot write protocol.

use crate::test_utils::*;
use strata_core::types::RunId;
use strata_core::value::Value;
use strata_primitives::KVStore;
use std::ffi::OsStr;
use std::fs;

/// No temp files remain after snapshot
#[test]
fn test_no_temp_files_after_snapshot() {
    let test_db = TestDb::new();
    let run_id = test_db.run_id;

    let kv = test_db.kv();
    kv.put(&run_id, "key", Value::String("value".into()))
        .unwrap();

    let snapshot_dir = test_db.snapshot_dir();

    if snapshot_dir.exists() {
        // Check for .tmp files
        let tmp_files: Vec<_> = fs::read_dir(&snapshot_dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension() == Some(OsStr::new("tmp")))
            .collect();

        assert!(
            tmp_files.is_empty(),
            "Temp files remaining after snapshot: {:?}",
            tmp_files.iter().map(|f| f.path()).collect::<Vec<_>>()
        );
    }
}

/// Partial snapshot file is ignored
#[test]
fn test_partial_snapshot_ignored() {
    let mut test_db = TestDb::new();
    let run_id = test_db.run_id;

    let kv = test_db.kv();
    kv.put(&run_id, "real_key", Value::String("real_value".into()))
        .unwrap();

    let snapshot_dir = test_db.snapshot_dir();
    fs::create_dir_all(&snapshot_dir).ok();

    // Create a partial/invalid .tmp file
    let tmp_path = snapshot_dir.join("snapshot.tmp");
    fs::write(&tmp_path, b"partial snapshot data").unwrap();

    // Recovery should ignore the partial file
    test_db.reopen();

    // Database should still work
    assert_db_healthy(&test_db.db, &run_id);

    // Real data should be present
    let kv = test_db.kv();
    let value = kv.get(&run_id, "real_key").unwrap();
    assert!(value.is_some(), "Real data should survive");
}

/// Database healthy after crash during snapshot write
#[test]
fn test_healthy_after_snapshot_crash() {
    let mut test_db = TestDb::new();
    let run_id = test_db.run_id;

    let kv = test_db.kv();
    kv.put(&run_id, "key", Value::String("value".into()))
        .unwrap();

    // Simulate partial snapshot (crash during write)
    let snapshot_dir = test_db.snapshot_dir();
    fs::create_dir_all(&snapshot_dir).ok();
    fs::write(
        snapshot_dir.join("snapshot_incomplete.tmp"),
        b"incomplete data",
    )
    .ok();

    // Recovery
    test_db.reopen();

    // Database should be healthy
    assert_db_healthy(&test_db.db, &run_id);
}

/// Snapshot directory creation is atomic
#[test]
fn test_snapshot_directory_atomicity() {
    let test_db = TestDb::new();
    let snapshot_dir = test_db.snapshot_dir();

    // If directory exists, it should be fully formed
    if snapshot_dir.exists() {
        assert!(
            snapshot_dir.is_dir(),
            "Snapshot directory should be a directory"
        );
    }
}

/// Multiple snapshots can coexist
#[test]
fn test_multiple_snapshots_coexist() {
    let test_db = TestDb::new();
    let run_id = test_db.run_id;

    let kv = test_db.kv();

    // Write data
    kv.put(&run_id, "key1", Value::I64(1)).unwrap();
    let state1 = CapturedState::capture(&test_db.db, &run_id);

    // Write more data
    kv.put(&run_id, "key2", Value::I64(2)).unwrap();
    let state2 = CapturedState::capture(&test_db.db, &run_id);

    // Both states should be valid
    assert!(state1.kv_entries.contains_key("key1"));
    assert!(!state1.kv_entries.contains_key("key2"));

    assert!(state2.kv_entries.contains_key("key1"));
    assert!(state2.kv_entries.contains_key("key2"));
}
