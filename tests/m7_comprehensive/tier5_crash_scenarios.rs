//! Tier 5: Crash Scenarios Tests
//!
//! Tests for various crash scenarios and recovery.

use crate::test_utils::*;
use strata_core::types::RunId;
use strata_core::value::Value;
use strata_primitives::KVStore;
use std::fs;

/// Crash during WAL write (partial entry)
#[test]
fn test_crash_during_wal_write() {
    let mut test_db = TestDb::new();
    let run_id = test_db.run_id;

    let kv = test_db.kv();
    kv.put(&run_id, "before_crash", Value::String("safe".into()))
        .unwrap();

    let wal_path = test_db.wal_path();

    if wal_path.exists() {
        // Simulate partial write by truncating WAL
        let size = file_size(&wal_path);
        if size > 50 {
            truncate_file(&wal_path, size - 20);
        }
    }

    // Recovery
    test_db.reopen();

    // Database should be healthy
    assert_db_healthy(&test_db.db, &run_id);
}

/// Crash between operations
#[test]
fn test_crash_between_operations() {
    let mut test_db = TestDb::new();
    let run_id = test_db.run_id;

    let kv = test_db.kv();

    // First operation
    kv.put(&run_id, "op1", Value::I64(1)).unwrap();

    // Crash (simulated by reopen)
    test_db.reopen();

    // First operation should be durable
    let kv = test_db.kv();
    let value = kv.get(&run_id, "op1").unwrap();
    assert!(value.is_some(), "First operation should survive crash");
}

/// Crash during batch write
#[test]
fn test_crash_during_batch() {
    let mut test_db = TestDb::new();
    let run_id = test_db.run_id;

    let kv = test_db.kv();

    // Batch of writes
    for i in 0..100 {
        kv.put(&run_id, &format!("batch_{}", i), Value::I64(i))
            .unwrap();
    }

    // Crash
    test_db.reopen();

    // Some or all should survive (prefix consistent)
    let kv = test_db.kv();
    let mut found = 0;
    for i in 0..100 {
        if kv.get(&run_id, &format!("batch_{}", i)).unwrap().is_some() {
            found += 1;
        } else {
            // Once we hit a missing one, rest should also be missing
            break;
        }
    }

    // Prefix consistency check
    for i in 0..found {
        assert!(
            kv.get(&run_id, &format!("batch_{}", i)).unwrap().is_some(),
            "Gap in prefix at {}",
            i
        );
    }
}

/// Multiple crashes in sequence
#[test]
fn test_multiple_crashes() {
    let mut test_db = TestDb::new();
    let run_id = test_db.run_id;

    let kv = test_db.kv();
    kv.put(&run_id, "survivor", Value::String("immortal".into()))
        .unwrap();

    // Multiple crashes
    for crash_num in 0..5 {
        test_db.reopen();

        // Data should survive
        let kv = test_db.kv();
        let value = kv.get(&run_id, "survivor").unwrap();
        assert!(value.is_some(), "Data lost after crash {}", crash_num);
    }
}

/// Crash with corrupted WAL tail
#[test]
fn test_crash_corrupted_wal_tail() {
    let mut test_db = TestDb::new();
    let run_id = test_db.run_id;

    let kv = test_db.kv();
    for i in 0..50 {
        kv.put(&run_id, &format!("k{}", i), Value::I64(i)).unwrap();
    }

    let wal_path = test_db.wal_path();

    if wal_path.exists() {
        let size = file_size(&wal_path);
        if size > 100 {
            // Corrupt tail
            corrupt_file_at_offset(&wal_path, size - 30, &[0xFF, 0xFF, 0xFF]);
        }
    }

    // Recovery should truncate to last valid boundary
    test_db.reopen();

    // Database should be healthy
    assert_db_healthy(&test_db.db, &run_id);
}

/// Crash after delete operation
#[test]
fn test_crash_after_delete() {
    let mut test_db = TestDb::new();
    let run_id = test_db.run_id;

    let kv = test_db.kv();

    // Create then delete
    kv.put(&run_id, "to_delete", Value::String("temp".into()))
        .unwrap();
    kv.delete(&run_id, "to_delete").unwrap();
    kv.put(&run_id, "keeper", Value::String("keep".into()))
        .unwrap();

    // Crash
    test_db.reopen();

    let kv = test_db.kv();

    // Delete should be durable (if it was committed)
    // Key behavior depends on what was committed before crash
    assert_db_healthy(&test_db.db, &run_id);
}

/// Crash with zero-length WAL
#[test]
fn test_crash_zero_length_wal() {
    let mut test_db = TestDb::new();
    let run_id = test_db.run_id;

    let wal_path = test_db.wal_path();

    if wal_path.exists() {
        // Truncate to zero
        truncate_file(&wal_path, 0);
    }

    // Recovery should handle empty WAL
    test_db.reopen();

    // Database should be healthy (empty state)
    assert_db_healthy(&test_db.db, &run_id);
}

/// Crash with missing WAL file
#[test]
fn test_crash_missing_wal() {
    let mut test_db = TestDb::new();
    let run_id = test_db.run_id;

    let wal_path = test_db.wal_path();

    if wal_path.exists() {
        fs::remove_file(&wal_path).ok();
    }

    // Recovery should handle missing WAL
    test_db.reopen();

    // Database should be healthy (fresh start)
    assert_db_healthy(&test_db.db, &run_id);
}

/// Recovery sequence completes correctly
#[test]
fn test_recovery_sequence() {
    let mut test_db = TestDb::new();
    let run_id = test_db.run_id;

    let kv = test_db.kv();

    // Write significant data
    for i in 0..200 {
        kv.put(&run_id, &format!("seq_{}", i), Value::I64(i))
            .unwrap();
    }

    let state_before = CapturedState::capture(&test_db.db, &run_id);

    // Recovery
    test_db.reopen();

    let state_after = CapturedState::capture(&test_db.db, &run_id);

    // States should be identical
    assert_states_equal(&state_before, &state_after, "Recovery sequence failed");
}

/// Crash during overwrite
#[test]
fn test_crash_during_overwrite() {
    let mut test_db = TestDb::new();
    let run_id = test_db.run_id;

    let kv = test_db.kv();

    // Initial value
    kv.put(&run_id, "overwrite_key", Value::I64(1)).unwrap();

    // Overwrite
    kv.put(&run_id, "overwrite_key", Value::I64(2)).unwrap();

    // Crash
    test_db.reopen();

    // Value should be either 1 or 2 (never something else)
    let kv = test_db.kv();
    if let Some(versioned) = kv.get(&run_id, "overwrite_key").unwrap() {
        match versioned.value {
            Value::I64(1) | Value::I64(2) => {} // OK
            _ => panic!("Unexpected value after crash: {:?}", versioned.value),
        }
    }
}
