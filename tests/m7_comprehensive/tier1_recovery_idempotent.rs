//! Tier 1.2: R2 - Idempotent Recovery Tests
//!
//! **Invariant R2**: replay(replay(S, WAL), WAL) = replay(S, WAL)
//!
//! These tests verify that recovery is idempotent:
//! - Multiple recoveries produce the same result
//! - Double replay doesn't change state
//! - Recovery is safe to run multiple times

use crate::test_utils::*;
use strata_core::types::RunId;
use strata_core::value::Value;
use strata_primitives::KVStore;
use std::sync::Arc;

/// R2: Double recovery produces same state
#[test]
fn test_r2_replay_idempotent_basic() {
    let mut test_db = TestDb::new();
    let run_id = test_db.run_id;

    // Write data
    let kv = test_db.kv();
    kv.put(&run_id, "key1", Value::String("value1".into()))
        .unwrap();
    kv.put(&run_id, "key2", Value::String("value2".into()))
        .unwrap();

    // First recovery
    test_db.reopen();
    let state1 = CapturedState::capture(&test_db.db, &run_id);

    // Second recovery (recovery of recovered state)
    test_db.reopen();
    let state2 = CapturedState::capture(&test_db.db, &run_id);

    assert_eq!(
        state1.hash, state2.hash,
        "R2 VIOLATED: Double replay changed state"
    );
}

/// R2: Multiple recovery cycles produce identical state
#[test]
fn test_r2_multiple_recovery_cycles() {
    let mut test_db = TestDb::new();
    let run_id = test_db.run_id;

    // Populate test data
    let kv = test_db.kv();
    for i in 0..50 {
        kv.put(
            &run_id,
            &format!("key_{}", i),
            Value::String(format!("value_{}", i)),
        )
        .unwrap();
    }

    let original_state = CapturedState::capture(&test_db.db, &run_id);

    // Simulate 10 crash/recovery cycles
    for cycle in 0..10 {
        test_db.reopen();
        let recovered_state = CapturedState::capture(&test_db.db, &run_id);

        assert_eq!(
            original_state.hash, recovered_state.hash,
            "R2 VIOLATED: Recovery cycle {} changed state",
            cycle
        );
    }
}

/// R2: Idempotent after writes and deletes
#[test]
fn test_r2_idempotent_with_mixed_operations() {
    let mut test_db = TestDb::new();
    let run_id = test_db.run_id;

    let kv = test_db.kv();

    // Mixed operations
    kv.put(&run_id, "a", Value::String("1".into())).unwrap();
    kv.put(&run_id, "b", Value::String("2".into())).unwrap();
    kv.delete(&run_id, "a").unwrap();
    kv.put(&run_id, "c", Value::String("3".into())).unwrap();

    let original_state = CapturedState::capture(&test_db.db, &run_id);

    // Multiple recoveries
    for _ in 0..5 {
        test_db.reopen();
    }

    let final_state = CapturedState::capture(&test_db.db, &run_id);
    assert_states_equal(&original_state, &final_state, "R2 VIOLATED: Mixed ops");
}

/// R2: Idempotent with overwrites
#[test]
fn test_r2_idempotent_with_overwrites() {
    let mut test_db = TestDb::new();
    let run_id = test_db.run_id;

    let kv = test_db.kv();

    // Many overwrites to same key
    for i in 0..100 {
        kv.put(&run_id, "counter", Value::I64(i)).unwrap();
    }

    let original_state = CapturedState::capture(&test_db.db, &run_id);

    // Multiple recoveries
    for _ in 0..5 {
        test_db.reopen();
    }

    let final_state = CapturedState::capture(&test_db.db, &run_id);
    assert_states_equal(&original_state, &final_state, "R2 VIOLATED: Overwrites");

    // Verify final value
    let kv = test_db.kv();
    assert_eq!(kv.get(&run_id, "counter").unwrap().map(|v| v.value), Some(Value::I64(99)));
}

/// R2: State invariant - recovery never adds extra data
#[test]
fn test_r2_recovery_never_accumulates_state() {
    let mut test_db = TestDb::new();
    let run_id = test_db.run_id;

    let kv = test_db.kv();
    kv.put(&run_id, "only_key", Value::String("only_value".into()))
        .unwrap();

    // Count keys before
    let state_before = CapturedState::capture(&test_db.db, &run_id);
    let count_before = state_before.kv_entries.len();

    // Multiple recoveries
    for _ in 0..10 {
        test_db.reopen();
    }

    // Count keys after
    let state_after = CapturedState::capture(&test_db.db, &run_id);
    let count_after = state_after.kv_entries.len();

    assert_eq!(
        count_before, count_after,
        "R2 VIOLATED: Recovery accumulated extra state ({} -> {} entries)",
        count_before, count_after
    );
}

/// R2: Idempotent recovery of empty database
#[test]
fn test_r2_idempotent_empty_database() {
    let mut test_db = TestDb::new();
    let run_id = test_db.run_id;

    let state_before = CapturedState::capture(&test_db.db, &run_id);

    // Multiple recoveries of empty database
    for _ in 0..5 {
        test_db.reopen();
    }

    let state_after = CapturedState::capture(&test_db.db, &run_id);

    // Empty database should remain empty
    assert!(
        state_after.kv_entries.is_empty(),
        "R2: Empty db should stay empty"
    );
    assert_eq!(state_before.hash, state_after.hash);
}

/// R2: Idempotent with large dataset
#[test]
fn test_r2_idempotent_large_dataset() {
    let mut test_db = TestDb::new();
    let run_id = test_db.run_id;

    // Create large dataset
    let kv = test_db.kv();
    for i in 0..1000 {
        kv.put(
            &run_id,
            &format!("key_{:05}", i),
            Value::String(format!("value_{:05}", i)),
        )
        .unwrap();
    }

    let original_state = CapturedState::capture(&test_db.db, &run_id);

    // Multiple recovery cycles
    for cycle in 0..3 {
        test_db.reopen();
        let recovered_state = CapturedState::capture(&test_db.db, &run_id);

        assert_eq!(
            original_state.kv_entries.len(),
            recovered_state.kv_entries.len(),
            "R2 VIOLATED: Entry count changed on cycle {}",
            cycle
        );
        assert_eq!(
            original_state.hash, recovered_state.hash,
            "R2 VIOLATED: Hash changed on cycle {}",
            cycle
        );
    }
}

/// R2: Recovery is stable - no drift over time
#[test]
fn test_r2_no_drift_over_many_cycles() {
    let mut test_db = TestDb::new();
    let run_id = test_db.run_id;

    let kv = test_db.kv();
    kv.put(&run_id, "stable_key", Value::String("stable_value".into()))
        .unwrap();

    // Record hashes over many cycles
    let mut hashes = Vec::new();
    for _ in 0..20 {
        test_db.reopen();
        let state = CapturedState::capture(&test_db.db, &run_id);
        hashes.push(state.hash);
    }

    // All hashes should be identical (no drift)
    let first_hash = hashes[0];
    for (i, hash) in hashes.iter().enumerate() {
        assert_eq!(
            first_hash, *hash,
            "R2 VIOLATED: State drifted at cycle {}",
            i
        );
    }
}
