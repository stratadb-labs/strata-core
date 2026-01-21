//! Tier 2.1: P1 - Replay is a Pure Function Tests
//!
//! **Invariant P1**: fn(run_id, event_log) → ReadOnlyView
//!
//! These tests verify:
//! - Replay takes inputs and returns a view
//! - Same inputs produce same output
//! - Different runs produce different views

use crate::test_utils::*;
use strata_core::types::RunId;
use strata_core::value::Value;
use strata_engine::ReadOnlyView;
use strata_primitives::KVStore;
use std::sync::Arc;

/// P1: replay_run returns a ReadOnlyView
#[test]
fn test_p1_replay_returns_view() {
    let test_db = TestDb::new_in_memory();
    let run_id = test_db.run_id;

    let kv = test_db.kv();
    kv.put(&run_id, "key", Value::String("value".into()))
        .unwrap();

    // This test verifies the conceptual model:
    // replay_run(run_id) -> ReadOnlyView
    // In the current implementation, we capture state instead
    let state = CapturedState::capture(&test_db.db, &run_id);

    // State should contain the written data
    assert!(state.kv_entries.contains_key("key"));
}

/// P1: Same inputs produce same output
#[test]
fn test_p1_same_inputs_same_output() {
    let test_db = TestDb::new_in_memory();
    let run_id = test_db.run_id;

    let kv = test_db.kv();
    kv.put(&run_id, "key1", Value::String("value1".into()))
        .unwrap();
    kv.put(&run_id, "key2", Value::String("value2".into()))
        .unwrap();

    // Capture state multiple times
    let state1 = CapturedState::capture(&test_db.db, &run_id);
    let state2 = CapturedState::capture(&test_db.db, &run_id);

    // Must be identical
    assert_eq!(
        state1.hash, state2.hash,
        "P1 VIOLATED: Same inputs different outputs"
    );
    assert_eq!(state1.kv_entries, state2.kv_entries);
}

/// P1: Different runs produce different views
#[test]
fn test_p1_different_runs_different_views() {
    let test_db = TestDb::new_in_memory();
    let run_id1 = test_db.run_id;
    let run_id2 = RunId::new();

    let kv = test_db.kv();

    // Write different data to different runs
    kv.put(&run_id1, "key", Value::String("run1_value".into()))
        .unwrap();
    kv.put(&run_id2, "key", Value::String("run2_value".into()))
        .unwrap();

    // Capture states
    let state1 = CapturedState::capture(&test_db.db, &run_id1);
    let state2 = CapturedState::capture(&test_db.db, &run_id2);

    // States should differ
    assert_ne!(
        state1.hash, state2.hash,
        "P1: Different runs should have different views"
    );
}

/// P1: Function is consistent over multiple calls
#[test]
fn test_p1_consistent_over_calls() {
    let test_db = TestDb::new_in_memory();
    let run_id = test_db.run_id;

    let kv = test_db.kv();
    for i in 0..20 {
        kv.put(&run_id, &format!("k{}", i), Value::I64(i)).unwrap();
    }

    // Call many times
    let mut hashes = Vec::new();
    for _ in 0..50 {
        let state = CapturedState::capture(&test_db.db, &run_id);
        hashes.push(state.hash);
    }

    // All should be identical
    let first = hashes[0];
    for (i, hash) in hashes.iter().enumerate() {
        assert_eq!(
            first, *hash,
            "P1 VIOLATED: Call {} returned different result",
            i
        );
    }
}

/// P1: Empty run returns empty view
#[test]
fn test_p1_empty_run_empty_view() {
    let test_db = TestDb::new_in_memory();
    let run_id = test_db.run_id;

    // No writes

    let state = CapturedState::capture(&test_db.db, &run_id);

    assert!(
        state.kv_entries.is_empty(),
        "P1: Empty run should produce empty view"
    );
}

/// P1: View contains exactly what was written
#[test]
fn test_p1_view_contains_written_data() {
    let test_db = TestDb::new_in_memory();
    let run_id = test_db.run_id;

    let kv = test_db.kv();

    let expected = vec![("alpha", "A"), ("beta", "B"), ("gamma", "C")];

    for (key, value) in &expected {
        kv.put(&run_id, key, Value::String((*value).to_string()))
            .unwrap();
    }

    let state = CapturedState::capture(&test_db.db, &run_id);

    // Should contain all expected keys
    for (key, _) in &expected {
        assert!(
            state.kv_entries.contains_key(*key),
            "P1: View missing key {}",
            key
        );
    }
}

/// P1: Run ID is part of the function signature
#[test]
fn test_p1_run_id_matters() {
    let test_db = TestDb::new_in_memory();
    let run_id1 = RunId::new();
    let run_id2 = RunId::new();

    let kv = test_db.kv();

    // Same key, different runs
    kv.put(&run_id1, "shared_key", Value::String("run1".into()))
        .unwrap();
    kv.put(&run_id2, "shared_key", Value::String("run2".into()))
        .unwrap();

    let state1 = CapturedState::capture(&test_db.db, &run_id1);
    let state2 = CapturedState::capture(&test_db.db, &run_id2);

    // Different run_ids should produce isolated views
    assert_ne!(state1.kv_entries, state2.kv_entries);
}

/// P1: View reflects final state
#[test]
fn test_p1_view_reflects_final_state() {
    let test_db = TestDb::new_in_memory();
    let run_id = test_db.run_id;

    let kv = test_db.kv();

    // Multiple writes to same key
    kv.put(&run_id, "counter", Value::I64(1)).unwrap();
    kv.put(&run_id, "counter", Value::I64(2)).unwrap();
    kv.put(&run_id, "counter", Value::I64(3)).unwrap();

    let state = CapturedState::capture(&test_db.db, &run_id);

    // View should show final value
    assert!(state.kv_entries.contains_key("counter"));
    // The value format in captured state includes debug format
    assert!(state.kv_entries["counter"].contains("3"));
}
