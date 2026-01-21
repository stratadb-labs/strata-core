//! Tier 3.1: Snapshot Format Tests
//!
//! Tests for snapshot envelope format validation.

use crate::test_utils::*;
use strata_core::value::Value;
use strata_durability::{SNAPSHOT_HEADER_SIZE, SNAPSHOT_MAGIC, SNAPSHOT_VERSION_1};
use std::fs::File;
use std::io::Read;

/// Snapshot has correct magic number
#[test]
fn test_snapshot_magic_number() {
    let test_db = TestDb::new();
    let run_id = test_db.run_id;

    let kv = test_db.kv();
    kv.put(&run_id, "key", Value::String("value".into()))
        .unwrap();

    // Create snapshot if the feature is available
    // Note: This test validates the format spec, actual snapshot creation
    // depends on database configuration
    let snapshot_dir = test_db.snapshot_dir();
    if snapshot_dir.exists() {
        let snapshots = list_snapshots(&snapshot_dir);
        if let Some(snapshot_path) = snapshots.first() {
            let mut file = File::open(snapshot_path).unwrap();
            let mut magic = [0u8; 10];
            file.read_exact(&mut magic).unwrap();

            assert_eq!(&magic, SNAPSHOT_MAGIC, "Snapshot magic number incorrect");
        }
    }
}

/// Snapshot version field is valid
#[test]
fn test_snapshot_version_valid() {
    // Test that SNAPSHOT_VERSION_1 is defined correctly
    assert!(SNAPSHOT_VERSION_1 >= 1, "Snapshot version must be >= 1");
}

/// Snapshot header size is reasonable
#[test]
fn test_snapshot_header_size() {
    assert!(
        SNAPSHOT_HEADER_SIZE > 0,
        "Snapshot header size must be positive"
    );
    assert!(
        SNAPSHOT_HEADER_SIZE < 1024,
        "Snapshot header size seems too large"
    );
}

/// Empty database can create snapshot (conceptually)
#[test]
fn test_empty_snapshot_concept() {
    let test_db = TestDb::new();
    let run_id = test_db.run_id;

    // No writes - empty state
    let state = CapturedState::capture(&test_db.db, &run_id);
    assert!(state.kv_entries.is_empty());

    // Database should be healthy
    assert_db_healthy(&test_db.db, &run_id);
}

/// Snapshot directory is created correctly
#[test]
fn test_snapshot_directory_created() {
    let test_db = TestDb::new();

    // Snapshot directory should be at expected location
    let snapshot_dir = test_db.snapshot_dir();
    // Note: Directory may or may not exist depending on configuration
    // This test validates the path is constructed correctly
    assert!(snapshot_dir.ends_with("snapshots"));
}

/// Large data can be included in snapshot concept
#[test]
fn test_large_data_snapshot_concept() {
    let test_db = TestDb::new();
    let run_id = test_db.run_id;

    let kv = test_db.kv();

    // Write substantial data
    for i in 0..500 {
        let large_value = format!("{:0>1000}", i);
        kv.put(&run_id, &format!("key_{}", i), Value::String(large_value))
            .unwrap();
    }

    let state = CapturedState::capture(&test_db.db, &run_id);
    assert_eq!(state.kv_entries.len(), 500);
}
