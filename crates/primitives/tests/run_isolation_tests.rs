//! Run Isolation Integration Tests (Story #198)
//!
//! Tests verifying that different runs are completely isolated from each other.
//! Each run has its own namespace and cannot see or affect other runs' data.

use strata_core::types::RunId;
use strata_core::value::Value;
use strata_engine::Database;
use strata_primitives::{EventLog, KVStore, RunIndex, StateCell};
use std::collections::HashMap;
use std::sync::Arc;
use tempfile::TempDir;

/// Helper to create an empty object payload for EventLog
fn empty_payload() -> Value {
    Value::Object(HashMap::new())
}

/// Helper to create an object payload with an integer value
fn int_payload(v: i64) -> Value {
    Value::Object(HashMap::from([("value".to_string(), Value::Int(v))]))
}

/// Helper to create an object payload with a string value
fn string_payload(s: &str) -> Value {
    Value::Object(HashMap::from([("data".to_string(), Value::String(s.into()))]))
}

fn setup() -> (Arc<Database>, TempDir) {
    let temp_dir = TempDir::new().unwrap();
    let db = Arc::new(Database::open(temp_dir.path()).unwrap());
    (db, temp_dir)
}

/// Test KV isolation - same key in different runs are independent
#[test]
fn test_kv_isolation() {
    let (db, _temp) = setup();
    let kv = KVStore::new(db.clone());

    let run1 = RunId::new();
    let run2 = RunId::new();

    // Same key, different runs
    kv.put(&run1, "key", Value::Int(1)).unwrap();
    kv.put(&run2, "key", Value::Int(2)).unwrap();

    // Each run sees ONLY its own data
    assert_eq!(kv.get(&run1, "key").unwrap().map(|v| v.value), Some(Value::Int(1)));
    assert_eq!(kv.get(&run2, "key").unwrap().map(|v| v.value), Some(Value::Int(2)));

    // List shows only own keys
    let run1_keys = kv.list(&run1, None).unwrap();
    let run2_keys = kv.list(&run2, None).unwrap();

    assert_eq!(run1_keys.len(), 1);
    assert_eq!(run2_keys.len(), 1);
    assert!(run1_keys.contains(&"key".to_string()));
    assert!(run2_keys.contains(&"key".to_string()));
}

/// Test EventLog isolation - each run has independent sequence numbers
#[test]
fn test_event_log_isolation() {
    let (db, _temp) = setup();
    let event_log = EventLog::new(db.clone());

    use strata_core::contract::Version;

    let run1 = RunId::new();
    let run2 = RunId::new();

    // Both runs start at sequence 0
    let v1 = event_log
        .append(&run1, "event", string_payload("run1"))
        .unwrap();
    let v2 = event_log
        .append(&run2, "event", string_payload("run2"))
        .unwrap();

    // Independent sequences - both start at 0
    assert!(matches!(v1, Version::Sequence(0)));
    assert!(matches!(v2, Version::Sequence(0)));

    // Each has length 1
    assert_eq!(event_log.len(&run1).unwrap(), 1);
    assert_eq!(event_log.len(&run2).unwrap(), 1);

    // Chain verification is per-run
    assert!(event_log.verify_chain(&run1).unwrap().is_valid);
    assert!(event_log.verify_chain(&run2).unwrap().is_valid);

    // Appending more to run1 doesn't affect run2
    event_log
        .append(&run1, "event2", string_payload("run1-2"))
        .unwrap();
    event_log
        .append(&run1, "event3", string_payload("run1-3"))
        .unwrap();

    assert_eq!(event_log.len(&run1).unwrap(), 3);
    assert_eq!(event_log.len(&run2).unwrap(), 1); // Still 1
}

/// Test StateCell isolation - same cell name in different runs are independent
#[test]
fn test_state_cell_isolation() {
    let (db, _temp) = setup();
    let state_cell = StateCell::new(db.clone());

    let run1 = RunId::new();
    let run2 = RunId::new();

    // Same cell name, different runs
    state_cell.init(&run1, "counter", Value::Int(0)).unwrap();
    state_cell.init(&run2, "counter", Value::Int(100)).unwrap();

    // Each run sees its own value
    let state1 = state_cell.read(&run1, "counter").unwrap().unwrap();
    let state2 = state_cell.read(&run2, "counter").unwrap().unwrap();

    assert_eq!(state1.value.value, Value::Int(0));
    assert_eq!(state2.value.value, Value::Int(100));

    // Both start at version 1
    assert_eq!(state1.value.version, 1);
    assert_eq!(state2.value.version, 1);

    // CAS on run1 doesn't affect run2
    state_cell.cas(&run1, "counter", 1, Value::Int(10)).unwrap();

    let state1 = state_cell.read(&run1, "counter").unwrap().unwrap();
    let state2 = state_cell.read(&run2, "counter").unwrap().unwrap();

    assert_eq!(state1.value.value, Value::Int(10));
    assert_eq!(state1.value.version, 2);
    assert_eq!(state2.value.value, Value::Int(100)); // Unchanged
    assert_eq!(state2.value.version, 1); // Unchanged
}

/// Test that queries in one run context NEVER return data from another run
#[test]
fn test_cross_run_query_isolation() {
    let (db, _temp) = setup();

    let run1 = RunId::new();
    let run2 = RunId::new();

    // Create extensive data in both runs
    let kv = KVStore::new(db.clone());
    let event_log = EventLog::new(db.clone());
    let state_cell = StateCell::new(db.clone());

    // Run1 data
    for i in 0..10 {
        kv.put(&run1, &format!("key{}", i), Value::Int(i)).unwrap();
        event_log.append(&run1, "event", int_payload(i)).unwrap();
    }
    state_cell
        .init(&run1, "state", Value::String("run1".into()))
        .unwrap();

    // Run2 data
    for i in 0..5 {
        kv.put(&run2, &format!("key{}", i), Value::Int(i + 100))
            .unwrap();
        event_log
            .append(&run2, "event", int_payload(i + 100))
            .unwrap();
    }
    state_cell
        .init(&run2, "state", Value::String("run2".into()))
        .unwrap();

    // Verify counts are isolated
    assert_eq!(kv.list(&run1, None).unwrap().len(), 10);
    assert_eq!(kv.list(&run2, None).unwrap().len(), 5);

    assert_eq!(event_log.len(&run1).unwrap(), 10);
    assert_eq!(event_log.len(&run2).unwrap(), 5);

    // Verify values are isolated
    assert_eq!(kv.get(&run1, "key0").unwrap().map(|v| v.value), Some(Value::Int(0)));
    assert_eq!(kv.get(&run2, "key0").unwrap().map(|v| v.value), Some(Value::Int(100)));

    // Run1 cannot see run2's keys that don't exist in run1
    // (run2 only has key0-key4, run1 has key0-key9)
    // Actually both have overlapping key names, but different values
    assert_eq!(
        state_cell.read(&run1, "state").unwrap().unwrap().value.value,
        Value::String("run1".into())
    );
    assert_eq!(
        state_cell.read(&run2, "state").unwrap().unwrap().value.value,
        Value::String("run2".into())
    );
}

/// Test that deleting a run only affects that run's data
#[test]
fn test_run_delete_isolation() {
    let (db, _temp) = setup();

    let run_index = RunIndex::new(db.clone());
    let kv = KVStore::new(db.clone());
    let event_log = EventLog::new(db.clone());
    let state_cell = StateCell::new(db.clone());

    // Create two runs via RunIndex
    let meta1 = run_index.create_run("run1").unwrap();
    let meta2 = run_index.create_run("run2").unwrap();

    let run1 = RunId::from_string(&meta1.value.run_id).unwrap();
    let run2 = RunId::from_string(&meta2.value.run_id).unwrap();

    // Write data to both runs
    kv.put(&run1, "key", Value::Int(1)).unwrap();
    kv.put(&run2, "key", Value::Int(2)).unwrap();

    event_log.append(&run1, "event", empty_payload()).unwrap();
    event_log.append(&run2, "event", empty_payload()).unwrap();

    state_cell.init(&run1, "cell", Value::Int(10)).unwrap();
    state_cell.init(&run2, "cell", Value::Int(20)).unwrap();

    // Verify both runs have data
    assert!(kv.get(&run1, "key").unwrap().is_some());
    assert!(kv.get(&run2, "key").unwrap().is_some());

    // Delete run1 (cascading delete)
    run_index.delete_run("run1").unwrap();

    // run1 data is GONE
    assert!(kv.get(&run1, "key").unwrap().is_none());
    assert_eq!(event_log.len(&run1).unwrap(), 0);
    assert!(!state_cell.exists(&run1, "cell").unwrap());

    // run2 data is UNTOUCHED
    assert_eq!(kv.get(&run2, "key").unwrap().map(|v| v.value), Some(Value::Int(2)));
    assert_eq!(event_log.len(&run2).unwrap(), 1);
    assert!(state_cell.exists(&run2, "cell").unwrap());
}

/// Test that many concurrent runs remain isolated
#[test]
fn test_many_runs_isolation() {
    let (db, _temp) = setup();
    let kv = KVStore::new(db.clone());

    // Create 100 runs
    let runs: Vec<RunId> = (0..100).map(|_| RunId::new()).collect();

    // Each run writes its own data
    for (i, run_id) in runs.iter().enumerate() {
        kv.put(run_id, "value", Value::Int(i as i64)).unwrap();
        kv.put(run_id, "run_index", Value::Int(i as i64)).unwrap();
    }

    // Verify each run sees only its data
    for (i, run_id) in runs.iter().enumerate() {
        let versioned = kv.get(run_id, "value").unwrap().unwrap();
        assert_eq!(versioned.value, Value::Int(i as i64));

        let keys = kv.list(run_id, None).unwrap();
        assert_eq!(keys.len(), 2);
    }
}

/// Test StateCell CAS isolation - version conflicts don't cross runs
#[test]
fn test_state_cell_cas_isolation() {
    let (db, _temp) = setup();
    let state_cell = StateCell::new(db.clone());

    let run1 = RunId::new();
    let run2 = RunId::new();

    // Both init same cell name
    state_cell.init(&run1, "cell", Value::Int(0)).unwrap();
    state_cell.init(&run2, "cell", Value::Int(0)).unwrap();

    // CAS on run1 with version 1
    state_cell.cas(&run1, "cell", 1, Value::Int(10)).unwrap();

    // CAS on run2 with version 1 should ALSO succeed (independent versions)
    let result = state_cell.cas(&run2, "cell", 1, Value::Int(20));
    assert!(result.is_ok());

    // Both have been updated
    let s1 = state_cell.read(&run1, "cell").unwrap().unwrap();
    let s2 = state_cell.read(&run2, "cell").unwrap().unwrap();

    assert_eq!(s1.value.value, Value::Int(10));
    assert_eq!(s1.value.version, 2);
    assert_eq!(s2.value.value, Value::Int(20));
    assert_eq!(s2.value.version, 2);
}

/// Test EventLog chain isolation - chains are independent per run
#[test]
fn test_event_log_chain_isolation() {
    let (db, _temp) = setup();
    let event_log = EventLog::new(db.clone());

    let run1 = RunId::new();
    let run2 = RunId::new();

    // Build chain in run1
    event_log.append(&run1, "e1", int_payload(0)).unwrap();
    event_log.append(&run1, "e2", int_payload(1)).unwrap();
    event_log.append(&run1, "e3", int_payload(2)).unwrap();

    // Build different chain in run2
    event_log.append(&run2, "x1", int_payload(100)).unwrap();
    event_log.append(&run2, "x2", int_payload(101)).unwrap();

    // Read events to get hashes
    let event1_0 = event_log.read(&run1, 0).unwrap().unwrap();
    let event2_0 = event_log.read(&run2, 0).unwrap().unwrap();

    // Chains have different hashes (different content)
    assert_ne!(event1_0.value.hash, event2_0.value.hash);

    // Read event from run1 - prev_hash links within run1 only
    let event1_1 = event_log.read(&run1, 1).unwrap().unwrap();
    assert_eq!(event1_1.value.prev_hash, event1_0.value.hash);

    // Verify chains independently
    assert!(event_log.verify_chain(&run1).unwrap().is_valid);
    assert!(event_log.verify_chain(&run2).unwrap().is_valid);

    // Corrupting run1 chain doesn't affect run2 verification
    // (We can't easily corrupt without direct DB access, so just verify both remain valid)
    assert_eq!(event_log.verify_chain(&run1).unwrap().length, 3);
    assert_eq!(event_log.verify_chain(&run2).unwrap().length, 2);
}

