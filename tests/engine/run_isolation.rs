//! Run Isolation Tests
//!
//! Tests that verify data isolation between different runs.

use crate::common::*;
use strata_core::primitives::json::JsonPath;
use std::collections::HashMap;

/// Helper to create an event payload object
fn event_payload(data: Value) -> Value {
    Value::Object(HashMap::from([
        ("data".to_string(), data),
    ]))
}

// ============================================================================
// KVStore Isolation
// ============================================================================

#[test]
fn kv_runs_are_isolated() {
    let test_db = TestDb::new();
    let kv = test_db.kv();

    let branch_a = BranchId::new();
    let branch_b = BranchId::new();

    // Same key, different runs
    kv.put(&branch_a, "key", Value::Int(1)).unwrap();
    kv.put(&branch_b, "key", Value::Int(2)).unwrap();

    // Each run sees its own value
    assert_eq!(kv.get(&branch_a, "key").unwrap().unwrap(), Value::Int(1));
    assert_eq!(kv.get(&branch_b, "key").unwrap().unwrap(), Value::Int(2));
}

#[test]
fn kv_delete_doesnt_affect_other_run() {
    let test_db = TestDb::new();
    let kv = test_db.kv();

    let branch_a = BranchId::new();
    let branch_b = BranchId::new();

    kv.put(&branch_a, "key", Value::Int(1)).unwrap();
    kv.put(&branch_b, "key", Value::Int(2)).unwrap();

    kv.delete(&branch_a, "key").unwrap();

    // Run A's key is gone
    assert!(kv.get(&branch_a, "key").unwrap().is_none());

    // Run B's key still exists
    assert_eq!(kv.get(&branch_b, "key").unwrap().unwrap(), Value::Int(2));
}

#[test]
fn kv_list_only_shows_run_keys() {
    let test_db = TestDb::new();
    let kv = test_db.kv();

    let branch_a = BranchId::new();
    let branch_b = BranchId::new();

    kv.put(&branch_a, "a1", Value::Int(1)).unwrap();
    kv.put(&branch_a, "a2", Value::Int(2)).unwrap();
    kv.put(&branch_b, "b1", Value::Int(3)).unwrap();

    let keys_a = kv.list(&branch_a, None).unwrap();
    let keys_b = kv.list(&branch_b, None).unwrap();

    assert_eq!(keys_a.len(), 2);
    assert_eq!(keys_b.len(), 1);

    assert!(keys_a.contains(&"a1".to_string()));
    assert!(keys_a.contains(&"a2".to_string()));
    assert!(keys_b.contains(&"b1".to_string()));
}

// ============================================================================
// EventLog Isolation
// ============================================================================

#[test]
fn eventlog_runs_are_isolated() {
    let test_db = TestDb::new();
    let event = test_db.event();

    let branch_a = BranchId::new();
    let branch_b = BranchId::new();

    event.append(&branch_a, "type", event_payload(Value::String("branch_a".into()))).unwrap();
    event.append(&branch_a, "type", event_payload(Value::String("run_a_2".into()))).unwrap();
    event.append(&branch_b, "type", event_payload(Value::String("branch_b".into()))).unwrap();

    assert_eq!(event.len(&branch_a).unwrap(), 2);
    assert_eq!(event.len(&branch_b).unwrap(), 1);
}

#[test]
fn eventlog_sequence_numbers_per_run() {
    let test_db = TestDb::new();
    let event = test_db.event();

    let branch_a = BranchId::new();
    let branch_b = BranchId::new();

    // Both runs start sequence at 0
    let seq_a = event.append(&branch_a, "type", event_payload(Value::Int(1))).unwrap();
    let seq_b = event.append(&branch_b, "type", event_payload(Value::Int(1))).unwrap();

    assert_eq!(seq_a.as_u64(), 0);
    assert_eq!(seq_b.as_u64(), 0);
}

#[test]
fn eventlog_independent_per_run() {
    let test_db = TestDb::new();
    let event = test_db.event();

    let branch_a = BranchId::new();
    let branch_b = BranchId::new();

    for i in 0..5 {
        event.append(&branch_a, "type", event_payload(Value::Int(i))).unwrap();
        event.append(&branch_b, "type", event_payload(Value::Int(i * 10))).unwrap();
    }

    // Both runs should have 5 events independently
    assert_eq!(event.len(&branch_a).unwrap(), 5);
    assert_eq!(event.len(&branch_b).unwrap(), 5);

    // Events should be readable independently
    let a_event = event.read(&branch_a, 0).unwrap();
    let b_event = event.read(&branch_b, 0).unwrap();
    assert_eq!(a_event.as_ref().unwrap().value.event_type, "type");
    assert_eq!(b_event.as_ref().unwrap().value.event_type, "type");
}

// ============================================================================
// StateCell Isolation
// ============================================================================

#[test]
fn statecell_runs_are_isolated() {
    let test_db = TestDb::new();
    let state = test_db.state();

    let branch_a = BranchId::new();
    let branch_b = BranchId::new();

    // Same cell name, different runs
    state.init(&branch_a, "cell", Value::Int(1)).unwrap();
    state.init(&branch_b, "cell", Value::Int(2)).unwrap();

    assert_eq!(state.read(&branch_a, "cell").unwrap().unwrap(), Value::Int(1));
    assert_eq!(state.read(&branch_b, "cell").unwrap().unwrap(), Value::Int(2));
}

#[test]
fn statecell_cas_isolated() {
    let test_db = TestDb::new();
    let state = test_db.state();

    let branch_a = BranchId::new();
    let branch_b = BranchId::new();

    state.init(&branch_a, "cell", Value::Int(0)).unwrap();
    state.init(&branch_b, "cell", Value::Int(0)).unwrap();

    let version_a = state.readv(&branch_a, "cell").unwrap().unwrap().version();
    let version_b = state.readv(&branch_b, "cell").unwrap().unwrap().version();

    // CAS on run A
    state.cas(&branch_a, "cell", version_a, Value::Int(100)).unwrap();

    // Run B unchanged
    assert_eq!(state.read(&branch_b, "cell").unwrap().unwrap(), Value::Int(0));

    // CAS on run B still works with its original version
    state.cas(&branch_b, "cell", version_b, Value::Int(200)).unwrap();

    // Both have their own values
    assert_eq!(state.read(&branch_a, "cell").unwrap().unwrap(), Value::Int(100));
    assert_eq!(state.read(&branch_b, "cell").unwrap().unwrap(), Value::Int(200));
}

// ============================================================================
// JsonStore Isolation
// ============================================================================

#[test]
fn jsonstore_runs_are_isolated() {
    let test_db = TestDb::new();
    let json = test_db.json();

    let branch_a = BranchId::new();
    let branch_b = BranchId::new();

    json.create(&branch_a, "doc", serde_json::json!({"run": "a"}).into()).unwrap();
    json.create(&branch_b, "doc", serde_json::json!({"run": "b"}).into()).unwrap();

    let a_doc = json.get(&branch_a, "doc", &JsonPath::root()).unwrap().unwrap();
    let b_doc = json.get(&branch_b, "doc", &JsonPath::root()).unwrap().unwrap();

    assert_eq!(a_doc["run"], "a");
    assert_eq!(b_doc["run"], "b");
}

#[test]
fn jsonstore_count_per_run() {
    let test_db = TestDb::new();
    let json = test_db.json();

    let branch_a = BranchId::new();
    let branch_b = BranchId::new();

    json.create(&branch_a, "doc1", serde_json::json!({}).into()).unwrap();
    json.create(&branch_a, "doc2", serde_json::json!({}).into()).unwrap();
    json.create(&branch_b, "doc1", serde_json::json!({}).into()).unwrap();

    assert_eq!(json.list(&branch_a, None, None, 1000).unwrap().doc_ids.len(), 2);
    assert_eq!(json.list(&branch_b, None, None, 1000).unwrap().doc_ids.len(), 1);
}

// ============================================================================
// VectorStore Isolation
// ============================================================================

#[test]
fn vectorstore_collections_per_run() {
    let test_db = TestDb::new();
    let vector = test_db.vector();

    let branch_a = BranchId::new();
    let branch_b = BranchId::new();

    let config = config_small();

    // Same collection name, different runs
    vector.create_collection(branch_a, "coll", config.clone()).unwrap();
    vector.create_collection(branch_b, "coll", config.clone()).unwrap();

    // Both exist independently
    let a_colls = vector.list_collections(branch_a).unwrap();
    let b_colls = vector.list_collections(branch_b).unwrap();
    assert!(a_colls.iter().any(|c| c.name == "coll"));
    assert!(b_colls.iter().any(|c| c.name == "coll"));

    // Delete from run A doesn't affect run B
    vector.delete_collection(branch_a, "coll").unwrap();

    let a_colls = vector.list_collections(branch_a).unwrap();
    let b_colls = vector.list_collections(branch_b).unwrap();
    assert!(!a_colls.iter().any(|c| c.name == "coll"));
    assert!(b_colls.iter().any(|c| c.name == "coll"));
}

#[test]
fn vectorstore_vectors_per_run() {
    let test_db = TestDb::new();
    let vector = test_db.vector();

    let branch_a = BranchId::new();
    let branch_b = BranchId::new();

    let config = config_small();
    vector.create_collection(branch_a, "coll", config.clone()).unwrap();
    vector.create_collection(branch_b, "coll", config.clone()).unwrap();

    // Insert different vectors with same key
    vector.insert(branch_a, "coll", "vec", &[1.0f32, 0.0, 0.0], None).unwrap();
    vector.insert(branch_b, "coll", "vec", &[0.0f32, 1.0, 0.0], None).unwrap();

    let a_vec = vector.get(branch_a, "coll", "vec").unwrap().unwrap();
    let b_vec = vector.get(branch_b, "coll", "vec").unwrap().unwrap();

    assert_eq!(a_vec.value.embedding, vec![1.0f32, 0.0, 0.0]);
    assert_eq!(b_vec.value.embedding, vec![0.0f32, 1.0, 0.0]);
}

// ============================================================================
// Cross-Primitive Run Isolation
// ============================================================================

#[test]
fn all_primitives_isolated_by_run() {
    let test_db = TestDb::new();
    let prims = test_db.all_primitives();

    let branch_a = BranchId::new();
    let branch_b = BranchId::new();

    // Write to all primitives in run A
    prims.kv.put(&branch_a, "key", Value::Int(1)).unwrap();
    prims.event.append(&branch_a, "type", event_payload(Value::Int(1))).unwrap();
    prims.state.init(&branch_a, "cell", Value::Int(1)).unwrap();
    prims.json.create(&branch_a, "doc", serde_json::json!({"n": 1}).into()).unwrap();

    // Run B should see nothing
    assert!(prims.kv.get(&branch_b, "key").unwrap().is_none());
    assert_eq!(prims.event.len(&branch_b).unwrap(), 0);
    assert!(prims.state.read(&branch_b, "cell").unwrap().is_none());
    assert!(!prims.json.exists(&branch_b, "doc").unwrap());
}

#[test]
fn many_runs_no_interference() {
    let test_db = TestDb::new();
    let kv = test_db.kv();

    // Create 10 runs, each with its own data
    let runs: Vec<BranchId> = (0..10).map(|_| BranchId::new()).collect();

    for (i, branch_id) in runs.iter().enumerate() {
        kv.put(branch_id, "data", Value::Int(i as i64)).unwrap();
    }

    // Each run sees only its own data
    for (i, branch_id) in runs.iter().enumerate() {
        let val = kv.get(branch_id, "data").unwrap().unwrap();
        assert_eq!(val, Value::Int(i as i64));
    }
}
