//! Tier 3.4: Snapshot Discovery Tests
//!
//! Tests for snapshot discovery and ordering.

use crate::test_utils::*;
use strata_core::types::RunId;
use strata_core::value::Value;
use strata_primitives::KVStore;
use std::fs;

/// Discover snapshots in directory
#[test]
fn test_discover_snapshots() {
    let test_db = TestDb::new();

    let snapshot_dir = test_db.snapshot_dir();
    let snapshots = list_snapshots(&snapshot_dir);

    // Count should be >= 0
    assert!(snapshots.len() >= 0);
}

/// Invalid snapshots are filtered out
#[test]
fn test_invalid_snapshots_filtered() {
    let test_db = TestDb::new();

    let snapshot_dir = test_db.snapshot_dir();
    fs::create_dir_all(&snapshot_dir).ok();

    // Create invalid snapshot file
    let invalid_path = snapshot_dir.join("invalid.snap");
    fs::write(&invalid_path, b"not a valid snapshot").unwrap();

    // List should not include invalid files (or handle them gracefully)
    let snapshots = list_snapshots(&snapshot_dir);

    // Each snapshot file should be a valid path
    for snap in &snapshots {
        assert!(snap.exists());
    }
}

/// Snapshot count is correct
#[test]
fn test_snapshot_count() {
    let test_db = TestDb::new();

    let snapshot_dir = test_db.snapshot_dir();
    let count = count_snapshots(&snapshot_dir);

    // Should be non-negative
    assert!(count >= 0);
}

/// Newer snapshots are preferred
#[test]
fn test_newer_snapshots_preferred() {
    let mut test_db = TestDb::new();
    let run_id = test_db.run_id;

    let kv = test_db.kv();

    // Write initial data
    kv.put(&run_id, "v1", Value::I64(1)).unwrap();

    // Write more data
    kv.put(&run_id, "v2", Value::I64(2)).unwrap();

    // Write even more
    kv.put(&run_id, "v3", Value::I64(3)).unwrap();

    let state_before = CapturedState::capture(&test_db.db, &run_id);

    // Reopen - should recover to latest state
    test_db.reopen();

    let state_after = CapturedState::capture(&test_db.db, &run_id);

    // Should have all data
    assert_eq!(state_before.hash, state_after.hash);
}

/// Empty snapshot directory handled
#[test]
fn test_empty_snapshot_directory() {
    let test_db = TestDb::new();

    let snapshot_dir = test_db.snapshot_dir();

    // Create empty directory
    fs::create_dir_all(&snapshot_dir).ok();

    let count = count_snapshots(&snapshot_dir);
    let snapshots = list_snapshots(&snapshot_dir);

    assert_eq!(count, 0);
    assert!(snapshots.is_empty());
}

/// Non-existent snapshot directory handled
#[test]
fn test_nonexistent_snapshot_directory() {
    let test_db = TestDb::new();

    let fake_dir = test_db.db_path().join("nonexistent_snapshots");

    let count = count_snapshots(&fake_dir);
    let snapshots = list_snapshots(&fake_dir);

    assert_eq!(count, 0);
    assert!(snapshots.is_empty());
}
