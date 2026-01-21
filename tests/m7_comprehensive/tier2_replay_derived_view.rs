//! Tier 2.3: P3 - Replay Returns Derived View Tests
//!
//! **Invariant P3**: Computes view, does NOT reconstruct state.
//!
//! These tests verify:
//! - View is computed, not stored
//! - View reflects events, not mutations
//! - View is read-only

use crate::test_utils::*;
use strata_core::types::RunId;
use strata_core::value::Value;
use strata_primitives::KVStore;
use std::sync::Arc;

/// P3: View is computed on demand
#[test]
fn test_p3_view_computed_on_demand() {
    let test_db = TestDb::new_in_memory();
    let run_id = test_db.run_id;

    let kv = test_db.kv();
    kv.put(&run_id, "key1", Value::String("v1".into())).unwrap();

    // First capture
    let state1 = CapturedState::capture(&test_db.db, &run_id);

    // Add more data
    kv.put(&run_id, "key2", Value::String("v2".into())).unwrap();

    // Second capture
    let state2 = CapturedState::capture(&test_db.db, &run_id);

    // Views should be different (computed fresh each time)
    assert_ne!(
        state1.hash, state2.hash,
        "P3: Views should differ after new write"
    );
    assert!(!state1.kv_entries.contains_key("key2"));
    assert!(state2.kv_entries.contains_key("key2"));
}

/// P3: View reflects final state from events
#[test]
fn test_p3_view_reflects_events() {
    let test_db = TestDb::new_in_memory();
    let run_id = test_db.run_id;

    let kv = test_db.kv();

    // Sequence of events
    kv.put(&run_id, "counter", Value::I64(1)).unwrap();
    kv.put(&run_id, "counter", Value::I64(2)).unwrap();
    kv.put(&run_id, "counter", Value::I64(3)).unwrap();

    // View should show computed final state
    let state = CapturedState::capture(&test_db.db, &run_id);

    // Should contain final value
    assert!(state.kv_entries.contains_key("counter"));
    // Value should be 3 (last write)
    let value_str = &state.kv_entries["counter"];
    assert!(
        value_str.contains("3"),
        "P3: View should show final value, got {}",
        value_str
    );
}

/// P3: View is independent of other views
#[test]
fn test_p3_views_independent() {
    let test_db = TestDb::new_in_memory();
    let run_id = test_db.run_id;

    let kv = test_db.kv();
    kv.put(&run_id, "key", Value::String("initial".into()))
        .unwrap();

    // Capture first view
    let state1 = CapturedState::capture(&test_db.db, &run_id);

    // Modify data
    kv.put(&run_id, "key", Value::String("modified".into()))
        .unwrap();

    // Capture second view
    let state2 = CapturedState::capture(&test_db.db, &run_id);

    // Views are independent - first shouldn't affect second
    assert!(state1.kv_entries["key"].contains("initial"));
    assert!(state2.kv_entries["key"].contains("modified"));
}

/// P3: Delete events reflected in view
#[test]
fn test_p3_delete_events_reflected() {
    let test_db = TestDb::new_in_memory();
    let run_id = test_db.run_id;

    let kv = test_db.kv();

    kv.put(
        &run_id,
        "to_delete",
        Value::String("will be deleted".into()),
    )
    .unwrap();
    let state_before = CapturedState::capture(&test_db.db, &run_id);
    assert!(state_before.kv_entries.contains_key("to_delete"));

    kv.delete(&run_id, "to_delete").unwrap();
    let state_after = CapturedState::capture(&test_db.db, &run_id);

    // Delete should be reflected
    assert!(!state_after.kv_entries.contains_key("to_delete"));
}

/// P3: View derived from all relevant events
#[test]
fn test_p3_view_from_all_events() {
    let test_db = TestDb::new_in_memory();
    let run_id = test_db.run_id;

    let kv = test_db.kv();

    // Many events
    for i in 0..100 {
        kv.put(&run_id, &format!("key_{}", i), Value::I64(i))
            .unwrap();
    }

    // Delete some
    for i in (0..100).step_by(2) {
        kv.delete(&run_id, &format!("key_{}", i)).unwrap();
    }

    let state = CapturedState::capture(&test_db.db, &run_id);

    // Only odd-numbered keys should remain
    for i in 0..100 {
        let key = format!("key_{}", i);
        if i % 2 == 0 {
            assert!(
                !state.kv_entries.contains_key(&key),
                "P3: Deleted key {} present",
                i
            );
        } else {
            assert!(
                state.kv_entries.contains_key(&key),
                "P3: Expected key {} missing",
                i
            );
        }
    }
}

/// P3: Multiple runs have isolated views
#[test]
fn test_p3_run_isolation() {
    let test_db = TestDb::new_in_memory();
    let run_id1 = RunId::new();
    let run_id2 = RunId::new();

    let kv = test_db.kv();

    kv.put(&run_id1, "shared", Value::String("run1".into()))
        .unwrap();
    kv.put(&run_id2, "shared", Value::String("run2".into()))
        .unwrap();

    let state1 = CapturedState::capture(&test_db.db, &run_id1);
    let state2 = CapturedState::capture(&test_db.db, &run_id2);

    // Each view derived from its own run's events
    assert!(state1.kv_entries["shared"].contains("run1"));
    assert!(state2.kv_entries["shared"].contains("run2"));
}

/// P3: View computation handles overwrites correctly
#[test]
fn test_p3_overwrite_computation() {
    let test_db = TestDb::new_in_memory();
    let run_id = test_db.run_id;

    let kv = test_db.kv();

    // Many overwrites
    for i in 0..50 {
        kv.put(&run_id, "overwritten", Value::I64(i)).unwrap();
    }

    let state = CapturedState::capture(&test_db.db, &run_id);

    // View should show final computed value
    assert!(state.kv_entries.contains_key("overwritten"));
    // Final value should be 49
    assert!(state.kv_entries["overwritten"].contains("49"));
}

/// P3: Empty events produce empty view
#[test]
fn test_p3_empty_events_empty_view() {
    let test_db = TestDb::new_in_memory();
    let run_id = test_db.run_id;

    // No events

    let state = CapturedState::capture(&test_db.db, &run_id);

    assert!(
        state.kv_entries.is_empty(),
        "P3: Empty events should produce empty view"
    );
}

/// P3: View reflects current state, not historical
#[test]
fn test_p3_current_not_historical() {
    let test_db = TestDb::new_in_memory();
    let run_id = test_db.run_id;

    let kv = test_db.kv();

    // Create-delete-recreate cycle
    kv.put(&run_id, "key", Value::String("v1".into())).unwrap();
    kv.delete(&run_id, "key").unwrap();
    kv.put(&run_id, "key", Value::String("v2".into())).unwrap();

    let state = CapturedState::capture(&test_db.db, &run_id);

    // View should show current state (v2), not historical (v1)
    assert!(state.kv_entries["key"].contains("v2"));
    assert!(!state.kv_entries["key"].contains("v1"));
}
