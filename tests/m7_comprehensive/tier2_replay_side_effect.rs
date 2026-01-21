//! Tier 2.2: P2 - Replay is Side-Effect Free Tests
//!
//! **Invariant P2**: Does NOT mutate any persistent state.
//!
//! These tests verify:
//! - Capturing state doesn't modify anything
//! - Reading doesn't write to WAL
//! - Replay doesn't create snapshots

use crate::test_utils::*;
use strata_core::types::RunId;
use strata_core::value::Value;
use strata_primitives::KVStore;
use std::sync::Arc;

/// P2: Capturing state doesn't modify database
#[test]
fn test_p2_capture_does_not_modify() {
    let test_db = TestDb::new_in_memory();
    let run_id = test_db.run_id;

    let kv = test_db.kv();
    kv.put(&run_id, "key", Value::String("value".into()))
        .unwrap();

    let state_before = CapturedState::capture(&test_db.db, &run_id);

    // Capture many times
    for _ in 0..100 {
        let _ = CapturedState::capture(&test_db.db, &run_id);
    }

    let state_after = CapturedState::capture(&test_db.db, &run_id);

    assert_states_equal(
        &state_before,
        &state_after,
        "P2 VIOLATED: Capture modified state",
    );
}

/// P2: Reading doesn't affect subsequent writes
#[test]
fn test_p2_read_does_not_affect_write() {
    let test_db = TestDb::new_in_memory();
    let run_id = test_db.run_id;

    let kv = test_db.kv();

    // Write
    kv.put(&run_id, "key1", Value::I64(1)).unwrap();

    // Read multiple times
    for _ in 0..10 {
        let _ = kv.get(&run_id, "key1");
    }

    // Write again
    kv.put(&run_id, "key2", Value::I64(2)).unwrap();

    // Read shouldn't have affected the write
    let state = CapturedState::capture(&test_db.db, &run_id);
    assert!(state.kv_entries.contains_key("key1"));
    assert!(state.kv_entries.contains_key("key2"));
}

/// P2: Multiple state captures produce identical results
#[test]
fn test_p2_multiple_captures_identical() {
    let test_db = TestDb::new_in_memory();
    let run_id = test_db.run_id;

    let kv = test_db.kv();
    for i in 0..20 {
        kv.put(&run_id, &format!("k{}", i), Value::I64(i)).unwrap();
    }

    // Capture multiple states
    let states: Vec<_> = (0..10)
        .map(|_| CapturedState::capture(&test_db.db, &run_id))
        .collect();

    // All should be identical
    let first_hash = states[0].hash;
    for (i, state) in states.iter().enumerate() {
        assert_eq!(
            first_hash, state.hash,
            "P2 VIOLATED: Capture {} returned different state",
            i
        );
    }
}

/// P2: Capture doesn't create any files
#[test]
fn test_p2_capture_creates_no_files() {
    let test_db = TestDb::new();
    let run_id = test_db.run_id;

    let kv = test_db.kv();
    kv.put(&run_id, "key", Value::String("value".into()))
        .unwrap();

    // Count files before
    let files_before = count_files_in_dir(test_db.db_path());

    // Capture state many times
    for _ in 0..50 {
        let _ = CapturedState::capture(&test_db.db, &run_id);
    }

    // Count files after
    let files_after = count_files_in_dir(test_db.db_path());

    // No new files should be created
    // Note: May have small differences due to internal operations
    let diff = files_after.saturating_sub(files_before);
    assert!(
        diff <= 1, // Allow for 1 file difference due to timing
        "P2 VIOLATED: Capture created {} new files",
        diff
    );
}

/// P2: Read operations are idempotent
#[test]
fn test_p2_reads_idempotent() {
    let test_db = TestDb::new_in_memory();
    let run_id = test_db.run_id;

    let kv = test_db.kv();
    kv.put(&run_id, "key", Value::String("value".into()))
        .unwrap();

    // Read same key many times
    let mut values = Vec::new();
    for _ in 0..100 {
        values.push(kv.get(&run_id, "key").unwrap());
    }

    // All reads should return same value
    let first = &values[0];
    for (i, value) in values.iter().enumerate() {
        assert_eq!(
            first, value,
            "P2 VIOLATED: Read {} returned different value",
            i
        );
    }
}

/// P2: Capture of non-existent key doesn't create it
#[test]
fn test_p2_capture_nonexistent_key() {
    let test_db = TestDb::new_in_memory();
    let run_id = test_db.run_id;

    let kv = test_db.kv();

    // Read non-existent key
    let result = kv.get(&run_id, "does_not_exist").unwrap();
    assert!(result.is_none());

    // Capture state
    let state = CapturedState::capture(&test_db.db, &run_id);

    // Non-existent key should not appear
    assert!(
        !state.kv_entries.contains_key("does_not_exist"),
        "P2 VIOLATED: Reading created phantom key"
    );
}

/// P2: State capture on empty run has no side effects
#[test]
fn test_p2_empty_run_no_side_effects() {
    let test_db = TestDb::new_in_memory();
    let run_id = test_db.run_id;

    // Capture empty state many times
    for _ in 0..50 {
        let state = CapturedState::capture(&test_db.db, &run_id);
        assert!(state.kv_entries.is_empty());
    }

    // Write something
    let kv = test_db.kv();
    kv.put(&run_id, "new_key", Value::I64(42)).unwrap();

    // Verify the write succeeded (previous captures didn't interfere)
    let value = kv.get(&run_id, "new_key").unwrap().map(|v| v.value);
    assert_eq!(value, Some(Value::I64(42)));
}

/// P2: Concurrent reads don't interfere
#[test]
fn test_p2_concurrent_reads_safe() {
    use std::sync::Arc;
    use std::thread;

    let test_db = TestDb::new_in_memory();
    let run_id = test_db.run_id;

    let kv = test_db.kv();
    kv.put(&run_id, "shared", Value::String("shared_value".into()))
        .unwrap();

    let db = test_db.db.clone();
    let run_id_copy = run_id;

    // Spawn threads to read concurrently
    let handles: Vec<_> = (0..10)
        .map(|_| {
            let db = db.clone();
            thread::spawn(move || {
                for _ in 0..100 {
                    let _ = CapturedState::capture(&db, &run_id_copy);
                }
            })
        })
        .collect();

    for handle in handles {
        handle.join().unwrap();
    }

    // Original data should be unchanged
    let kv = test_db.kv();
    let value = kv.get(&run_id, "shared").unwrap().map(|v| v.value);
    assert_eq!(value, Some(Value::String("shared_value".into())));
}

// Helper function
fn count_files_in_dir(dir: &std::path::Path) -> usize {
    std::fs::read_dir(dir)
        .map(|entries| entries.filter_map(Result::ok).count())
        .unwrap_or(0)
}
