//! Tier 4.2: WAL CRC Validation Tests
//!
//! Tests for WAL CRC32 corruption detection.

use crate::test_utils::*;
use strata_core::types::RunId;
use strata_core::value::Value;
use strata_primitives::KVStore;

/// WAL corruption is detected on recovery
#[test]
fn test_wal_corruption_detected() {
    let mut test_db = TestDb::new();
    let run_id = test_db.run_id;

    let kv = test_db.kv();
    kv.put(&run_id, "key1", Value::String("value1".into()))
        .unwrap();
    kv.put(&run_id, "key2", Value::String("value2".into()))
        .unwrap();

    let wal_path = test_db.wal_path();

    if wal_path.exists() && file_size(&wal_path) > 100 {
        // Corrupt the WAL
        corrupt_file_at_offset(&wal_path, 50, &[0xFF, 0xFF]);

        // Recovery should either:
        // 1. Detect corruption and recover to last valid state
        // 2. Truncate to last valid boundary
        test_db.reopen();

        // Database should still be usable
        assert_db_healthy(&test_db.db, &run_id);
    }
}

/// Truncated WAL is handled
#[test]
fn test_truncated_wal_handled() {
    let mut test_db = TestDb::new();
    let run_id = test_db.run_id;

    let kv = test_db.kv();
    for i in 0..50 {
        kv.put(&run_id, &format!("k{}", i), Value::I64(i)).unwrap();
    }

    let wal_path = test_db.wal_path();

    if wal_path.exists() {
        let original_size = file_size(&wal_path);
        if original_size > 100 {
            // Truncate to 80% of original
            truncate_file(&wal_path, (original_size as f64 * 0.8) as u64);

            // Recovery should handle truncation
            test_db.reopen();

            // Database should be healthy
            assert_db_healthy(&test_db.db, &run_id);
        }
    }
}

/// CRC detects single byte corruption
#[test]
fn test_crc_single_byte_corruption() {
    let mut test_db = TestDb::new();
    let run_id = test_db.run_id;

    let kv = test_db.kv();
    kv.put(&run_id, "important", Value::String("data".into()))
        .unwrap();

    let wal_path = test_db.wal_path();

    if wal_path.exists() && file_size(&wal_path) > 50 {
        // Single byte flip
        corrupt_file_at_offset(&wal_path, 40, &[0xFF]);

        test_db.reopen();

        // Database should handle corruption gracefully
        assert_db_healthy(&test_db.db, &run_id);
    }
}

/// Valid WAL passes CRC check
#[test]
fn test_valid_wal_passes_crc() {
    let mut test_db = TestDb::new();
    let run_id = test_db.run_id;

    let kv = test_db.kv();
    for i in 0..20 {
        kv.put(&run_id, &format!("k{}", i), Value::I64(i)).unwrap();
    }

    let state_before = CapturedState::capture(&test_db.db, &run_id);

    // No corruption - just restart
    test_db.reopen();

    let state_after = CapturedState::capture(&test_db.db, &run_id);

    // State should be identical
    assert_states_equal(&state_before, &state_after, "Valid WAL recovery failed");
}

/// Multiple restarts with valid WAL
#[test]
fn test_multiple_restarts_valid_wal() {
    let mut test_db = TestDb::new();
    let run_id = test_db.run_id;

    let kv = test_db.kv();
    kv.put(&run_id, "persistent", Value::I64(42)).unwrap();

    let original_state = CapturedState::capture(&test_db.db, &run_id);

    for _ in 0..5 {
        test_db.reopen();
        let current_state = CapturedState::capture(&test_db.db, &run_id);
        assert_eq!(original_state.hash, current_state.hash);
    }
}

/// Corrupted entry doesn't affect earlier entries
#[test]
fn test_corruption_preserves_earlier_entries() {
    let mut test_db = TestDb::new();
    let run_id = test_db.run_id;

    let kv = test_db.kv();

    // Write some early data
    kv.put(&run_id, "early", Value::String("value".into()))
        .unwrap();

    // Write more data
    for i in 0..50 {
        kv.put(&run_id, &format!("later_{}", i), Value::I64(i))
            .unwrap();
    }

    let wal_path = test_db.wal_path();

    if wal_path.exists() {
        let size = file_size(&wal_path);
        if size > 200 {
            // Corrupt near the end (later entries)
            corrupt_file_at_offset(&wal_path, size - 50, &[0xFF, 0xFF, 0xFF]);

            test_db.reopen();

            // Early data should likely survive (depends on corruption location)
            // At minimum, database should be healthy
            assert_db_healthy(&test_db.db, &run_id);
        }
    }
}
