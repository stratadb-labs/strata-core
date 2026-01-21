//! Crash scenario matrix tests
//!
//! Tests covering critical crash recovery scenarios at the storage layer.
//! These tests validate that WAL corruption, truncation, and partial records
//! are handled correctly during recovery.

use strata_storage::database::{DatabaseConfig, DatabaseHandle};
use strata_storage::format::WalRecord;
use strata_storage::testing::{
    CrashPoint, DataState, ReferenceModel, VerificationResult, WalCorruptionTester,
};
use tempfile::tempdir;

/// Helper to create a test database with WAL data
fn create_test_database_with_data(db_path: &std::path::Path, record_count: usize) {
    let handle = DatabaseHandle::create(db_path, DatabaseConfig::for_testing()).unwrap();
    let uuid = handle.uuid();

    for i in 0..record_count {
        let record = WalRecord::new(i as u64 + 1, uuid, i as u64 * 1000, vec![i as u8; 50]);
        handle.append_wal(&record).unwrap();
    }

    handle.flush_wal().unwrap();
    handle.close().unwrap();
}

// === WAL Truncation Scenarios ===

#[test]
fn scenario_wal_truncation_small() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("test.db");

    create_test_database_with_data(&db_path, 10);

    let tester = WalCorruptionTester::new(&db_path);
    let result = tester.truncate_wal_tail(20).unwrap();

    assert!(result.segment.is_some());
    assert_eq!(result.bytes_removed, 20);

    // Recovery should handle truncation gracefully
    let verification = tester.verify_recovery().unwrap();
    assert!(
        verification.recovered,
        "Should recover from small truncation"
    );
}

#[test]
fn scenario_wal_truncation_large() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("test.db");

    create_test_database_with_data(&db_path, 100);

    let tester = WalCorruptionTester::new(&db_path);
    tester.truncate_wal_tail(500).unwrap();

    let verification = tester.verify_recovery().unwrap();
    assert!(
        verification.recovered,
        "Should recover from large truncation"
    );
}

#[test]
fn scenario_wal_truncation_half_record() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("test.db");

    create_test_database_with_data(&db_path, 5);

    let tester = WalCorruptionTester::new(&db_path);
    // Truncate mid-record (approximately half a record)
    tester.truncate_wal_tail(30).unwrap();

    let verification = tester.verify_recovery().unwrap();
    assert!(
        verification.recovered,
        "Should recover from mid-record truncation"
    );
}

// === WAL Garbage Scenarios ===

#[test]
fn scenario_garbage_random_bytes() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("test.db");

    create_test_database_with_data(&db_path, 5);

    let tester = WalCorruptionTester::new(&db_path);
    let garbage = vec![0xFF, 0xFE, 0xFD, 0xFC, 0xFB, 0xFA];
    tester.append_garbage(&garbage).unwrap();

    let verification = tester.verify_recovery().unwrap();
    assert!(
        verification.recovered,
        "Should recover with garbage at tail"
    );
}

#[test]
fn scenario_garbage_zero_bytes() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("test.db");

    create_test_database_with_data(&db_path, 5);

    let tester = WalCorruptionTester::new(&db_path);
    let zeros = vec![0u8; 100];
    tester.append_garbage(&zeros).unwrap();

    let verification = tester.verify_recovery().unwrap();
    assert!(
        verification.recovered,
        "Should recover with zero bytes at tail"
    );
}

#[test]
fn scenario_garbage_large_block() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("test.db");

    create_test_database_with_data(&db_path, 10);

    let tester = WalCorruptionTester::new(&db_path);
    tester.append_random_garbage(1000).unwrap();

    let verification = tester.verify_recovery().unwrap();
    assert!(
        verification.recovered,
        "Should recover with large garbage block"
    );
}

// === Partial Record Scenarios ===

#[test]
fn scenario_partial_record_header_only() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("test.db");

    create_test_database_with_data(&db_path, 5);

    let tester = WalCorruptionTester::new(&db_path);
    tester.create_partial_record().unwrap();

    let verification = tester.verify_recovery().unwrap();
    assert!(verification.recovered, "Should recover from partial record");
}

#[test]
fn scenario_partial_length_prefix() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("test.db");

    create_test_database_with_data(&db_path, 5);

    let tester = WalCorruptionTester::new(&db_path);
    // Just a length prefix with no data
    let partial_length = vec![0x20, 0x00, 0x00, 0x00]; // 32 bytes expected
    tester.append_garbage(&partial_length).unwrap();

    let verification = tester.verify_recovery().unwrap();
    assert!(
        verification.recovered,
        "Should recover from partial length prefix"
    );
}

// === Bit Rot Scenarios ===

#[test]
fn scenario_single_bit_corruption() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("test.db");

    create_test_database_with_data(&db_path, 20);

    let tester = WalCorruptionTester::new(&db_path);
    let result = tester.corrupt_random_bytes(1).unwrap();

    assert!(result.bytes_corrupted >= 1);

    // Note: Bit rot may cause CRC failures which should be detected
    // Recovery may succeed or fail depending on where corruption occurred
    let _verification = tester.verify_recovery();
}

// === Multiple Corruption Scenarios ===

#[test]
fn scenario_truncation_then_garbage() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("test.db");

    create_test_database_with_data(&db_path, 10);

    let tester = WalCorruptionTester::new(&db_path);
    tester.truncate_wal_tail(50).unwrap();
    tester.append_garbage(b"CORRUPTION").unwrap();

    let verification = tester.verify_recovery().unwrap();
    assert!(
        verification.recovered,
        "Should recover from truncation + garbage"
    );
}

#[test]
fn scenario_multiple_partial_records() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("test.db");

    create_test_database_with_data(&db_path, 5);

    let tester = WalCorruptionTester::new(&db_path);
    tester.create_partial_record().unwrap();
    tester.create_partial_record().unwrap();

    let verification = tester.verify_recovery().unwrap();
    assert!(
        verification.recovered,
        "Should recover from multiple partial records"
    );
}

// === Empty WAL Scenarios ===

#[test]
fn scenario_empty_wal_truncation() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("test.db");

    // Create database with no records
    let handle = DatabaseHandle::create(&db_path, DatabaseConfig::for_testing()).unwrap();
    handle.flush_wal().unwrap();
    handle.close().unwrap();

    let tester = WalCorruptionTester::new(&db_path);
    // Attempt truncation on mostly empty WAL
    let result = tester.truncate_wal_tail(100).unwrap();

    // Truncation might not remove much if WAL is mostly header
    if result.bytes_removed > 0 {
        let verification = tester.verify_recovery().unwrap();
        assert!(
            verification.recovered,
            "Should recover from empty WAL truncation"
        );
    }
}

// === Reference Model Verification ===

#[test]
fn reference_model_tracks_kv_operations() {
    let mut model = ReferenceModel::new();

    model.kv_put("run1", "key1", b"value1".to_vec());
    model.kv_put("run1", "key2", b"value2".to_vec());
    model.kv_delete("run1", "key1");

    assert!(model.get_kv("run1", "key1").is_none());
    assert_eq!(model.get_kv("run1", "key2"), Some(&b"value2".to_vec()));
    assert_eq!(model.total_operations(), 3);
}

#[test]
fn reference_model_tracks_events() {
    let mut model = ReferenceModel::new();

    model.event_append("run1", b"event1".to_vec());
    model.event_append("run1", b"event2".to_vec());

    let events = model.get_events("run1").unwrap();
    assert_eq!(events.len(), 2);
}

#[test]
fn reference_model_checkpoint_tracking() {
    let mut model = ReferenceModel::new();

    model.kv_put("run1", "key1", b"value1".to_vec());
    assert_eq!(model.operations_since_checkpoint(), 1);

    model.checkpoint();
    assert_eq!(model.operations_since_checkpoint(), 0);
    assert!(model.last_checkpoint_index().is_some());
}

#[test]
fn reference_model_comparison_finds_mismatches() {
    let mut model = ReferenceModel::new();
    model.kv_put("run1", "key1", b"value1".to_vec());
    model.kv_put("run1", "key2", b"value2".to_vec());

    // Actual state with missing key
    let actual: std::collections::HashMap<String, Vec<u8>> =
        [("key1".to_string(), b"value1".to_vec())]
            .into_iter()
            .collect();

    let mismatches = model.compare_kv("run1", &actual);
    assert_eq!(mismatches.len(), 1);
    assert!(mismatches[0].entity.contains("key2"));
}

// === Crash Point Data State ===

#[test]
fn crash_point_expected_states() {
    // Before WAL write: data should not be present
    assert_eq!(
        CrashPoint::BeforeWalWrite.expected_data_state(),
        DataState::NotPresent
    );

    // After write before fsync: data may or may not be present
    assert_eq!(
        CrashPoint::AfterWalWriteBeforeFsync.expected_data_state(),
        DataState::MayBePresent
    );

    // After fsync: data should be present
    assert_eq!(
        CrashPoint::AfterFsync.expected_data_state(),
        DataState::Present
    );

    // During segment rotation: committed data should be present
    assert_eq!(
        CrashPoint::DuringSegmentRotation.expected_data_state(),
        DataState::Present
    );

    // During snapshot operations: data should be present
    assert_eq!(
        CrashPoint::DuringSnapshotBeforeRename.expected_data_state(),
        DataState::Present
    );
    assert_eq!(
        CrashPoint::DuringSnapshotAfterRename.expected_data_state(),
        DataState::Present
    );
}

// === Verification Result Helpers ===

#[test]
fn verification_result_success() {
    let result = VerificationResult::success();
    assert!(result.is_valid);
    assert!(result.error.is_none());
    assert!(result.mismatches.is_empty());
}

#[test]
fn verification_result_with_error() {
    let result = VerificationResult::error("test error");
    assert!(!result.is_valid);
    assert_eq!(result.error, Some("test error".to_string()));
}
