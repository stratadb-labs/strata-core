//! Tier 1.1: R1 - Deterministic Recovery Tests
//!
//! **Invariant R1**: Same WAL produces same state every replay.
//!
//! These tests verify that recovery is deterministic:
//! - Given identical WAL, recovery always produces identical state
//! - Order of operations is preserved during replay
//! - State is identical across restarts

use crate::test_utils::*;
use strata_core::types::RunId;
use strata_core::value::Value;
use strata_primitives::KVStore;
use std::sync::Arc;

/// R1: Same WAL produces same state - basic test
#[test]
fn test_r1_same_wal_same_state_basic() {
    let mut test_db = TestDb::new();
    let run_id = test_db.run_id;

    // Write some data
    let kv = test_db.kv();
    kv.put(&run_id, "key1", Value::String("value1".into()))
        .unwrap();
    kv.put(&run_id, "key2", Value::String("value2".into()))
        .unwrap();
    kv.put(&run_id, "key3", Value::String("value3".into()))
        .unwrap();

    // Capture state
    let state_before = CapturedState::capture(&test_db.db, &run_id);

    // Simulate restart
    test_db.reopen();

    // Capture state after recovery
    let state_after = CapturedState::capture(&test_db.db, &run_id);

    assert_states_equal(&state_before, &state_after, "R1 VIOLATED");
}

/// R1: Multiple restarts produce identical state
#[test]
fn test_r1_deterministic_across_multiple_restarts() {
    let mut test_db = TestDb::new();
    let run_id = test_db.run_id;

    // Write initial data
    let kv = test_db.kv();
    for i in 0..10 {
        kv.put(
            &run_id,
            &format!("key_{}", i),
            Value::String(format!("value_{}", i)),
        )
        .unwrap();
    }

    // Capture original state
    let original_state = CapturedState::capture(&test_db.db, &run_id);

    // Perform multiple restarts
    for restart_num in 0..5 {
        test_db.reopen();
        let recovered_state = CapturedState::capture(&test_db.db, &run_id);

        assert_eq!(
            original_state.hash, recovered_state.hash,
            "R1 VIOLATED: State differs after restart #{}",
            restart_num
        );
    }
}

/// R1: Replay preserves operation ordering
#[test]
fn test_r1_ordering_preserved() {
    let mut test_db = TestDb::new();
    let run_id = test_db.run_id;

    // Write sequence of overwrites
    let kv = test_db.kv();
    kv.put(&run_id, "key", Value::String("v1".into())).unwrap();
    kv.put(&run_id, "key", Value::String("v2".into())).unwrap();
    kv.put(&run_id, "key", Value::String("v3".into())).unwrap();

    // Simulate restart
    test_db.reopen();

    // Final value must be "v3" (last write wins)
    let kv = test_db.kv();
    let value = kv.get(&run_id, "key").unwrap().map(|v| v.value);
    assert_eq!(
        value,
        Some(Value::String("v3".into())),
        "R1 VIOLATED: Operation order not preserved"
    );
}

/// R1: Deterministic with interleaved operations
#[test]
fn test_r1_deterministic_interleaved_operations() {
    let mut test_db = TestDb::new();
    let run_id = test_db.run_id;

    // Interleaved puts and deletes
    let kv = test_db.kv();
    kv.put(&run_id, "a", Value::String("1".into())).unwrap();
    kv.put(&run_id, "b", Value::String("2".into())).unwrap();
    kv.delete(&run_id, "a").unwrap();
    kv.put(&run_id, "c", Value::String("3".into())).unwrap();
    kv.put(&run_id, "a", Value::String("4".into())).unwrap();

    let state_before = CapturedState::capture(&test_db.db, &run_id);

    // Restart
    test_db.reopen();

    let state_after = CapturedState::capture(&test_db.db, &run_id);
    assert_states_equal(&state_before, &state_after, "R1 VIOLATED: Interleaved ops");

    // Verify specific values
    let kv = test_db.kv();
    assert_eq!(
        kv.get(&run_id, "a").unwrap().map(|v| v.value),
        Some(Value::String("4".into()))
    );
    assert_eq!(
        kv.get(&run_id, "b").unwrap().map(|v| v.value),
        Some(Value::String("2".into()))
    );
    assert_eq!(
        kv.get(&run_id, "c").unwrap().map(|v| v.value),
        Some(Value::String("3".into()))
    );
}

/// R1: Large dataset determinism
#[test]
fn test_r1_deterministic_large_dataset() {
    let mut test_db = TestDb::new();
    let run_id = test_db.run_id;

    // Write many entries
    let kv = test_db.kv();
    for i in 0..1000 {
        kv.put(
            &run_id,
            &format!("key_{}", i),
            Value::String(format!("value_{}", i)),
        )
        .unwrap();
    }

    let state_before = CapturedState::capture(&test_db.db, &run_id);

    // Multiple restarts
    for _ in 0..3 {
        test_db.reopen();
        let state_after = CapturedState::capture(&test_db.db, &run_id);
        assert_states_equal(&state_before, &state_after, "R1 VIOLATED: Large dataset");
    }
}

/// R1: Deterministic with various value types
#[test]
fn test_r1_deterministic_value_types() {
    let mut test_db = TestDb::new();
    let run_id = test_db.run_id;

    let kv = test_db.kv();

    // Various value types
    kv.put(&run_id, "string", Value::String("hello".into()))
        .unwrap();
    kv.put(&run_id, "int", Value::I64(42)).unwrap();
    kv.put(&run_id, "float", Value::F64(3.14)).unwrap();
    kv.put(&run_id, "bool", Value::Bool(true)).unwrap();
    kv.put(&run_id, "null", Value::Null).unwrap();

    let state_before = CapturedState::capture(&test_db.db, &run_id);

    test_db.reopen();

    let state_after = CapturedState::capture(&test_db.db, &run_id);
    assert_states_equal(&state_before, &state_after, "R1 VIOLATED: Value types");
}

/// R1: Parallel replay produces identical results
#[test]
fn test_r1_replay_100_times_identical() {
    let mut test_db = TestDb::new();
    let run_id = test_db.run_id;

    // Write data
    let kv = test_db.kv();
    for i in 0..50 {
        kv.put(
            &run_id,
            &format!("k{}", i),
            Value::String(format!("v{}", i)),
        )
        .unwrap();
    }

    // Collect hashes from multiple replays
    let mut hashes = Vec::new();
    for _ in 0..100 {
        test_db.reopen();
        let state = CapturedState::capture(&test_db.db, &run_id);
        hashes.push(state.hash);
    }

    // ALL hashes must be identical
    assert!(
        hashes.windows(2).all(|w| w[0] == w[1]),
        "R1 VIOLATED: Same WAL produced different states across 100 replays"
    );
}

/// R1: Deterministic after delete operations
#[test]
fn test_r1_deterministic_after_deletes() {
    let mut test_db = TestDb::new();
    let run_id = test_db.run_id;

    let kv = test_db.kv();

    // Create, then delete some keys
    for i in 0..20 {
        kv.put(
            &run_id,
            &format!("key_{}", i),
            Value::String(format!("value_{}", i)),
        )
        .unwrap();
    }

    // Delete every other key
    for i in (0..20).step_by(2) {
        kv.delete(&run_id, &format!("key_{}", i)).unwrap();
    }

    let state_before = CapturedState::capture(&test_db.db, &run_id);

    test_db.reopen();

    let state_after = CapturedState::capture(&test_db.db, &run_id);
    assert_states_equal(&state_before, &state_after, "R1 VIOLATED: After deletes");

    // Verify correct keys exist
    let kv = test_db.kv();
    for i in 0..20 {
        let value = kv.get(&run_id, &format!("key_{}", i)).unwrap();
        if i % 2 == 0 {
            assert!(value.is_none(), "Deleted key_{} should not exist", i);
        } else {
            assert!(value.is_some(), "key_{} should exist", i);
        }
    }
}
