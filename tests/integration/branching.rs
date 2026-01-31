//! Branching and Run Isolation Tests
//!
//! Tests run isolation guarantees and run management operations.
//! Note: Run forking (copying parent data) is not yet implemented - see issue #780.

use crate::common::*;

// ============================================================================
// Run Isolation
// ============================================================================

#[test]
fn data_isolated_between_runs() {
    let test_db = TestDb::new();
    let kv = test_db.kv();

    let branch_a = BranchId::new();
    let branch_b = BranchId::new();

    // Write to run A
    kv.put(&branch_a, "key", Value::String("value_a".into())).unwrap();

    // Write to run B
    kv.put(&branch_b, "key", Value::String("value_b".into())).unwrap();

    // Each run sees only its own data
    let val_a = kv.get(&branch_a, "key").unwrap().unwrap();
    let val_b = kv.get(&branch_b, "key").unwrap().unwrap();

    assert_eq!(val_a, Value::String("value_a".into()));
    assert_eq!(val_b, Value::String("value_b".into()));
}

#[test]
fn delete_in_one_run_doesnt_affect_other() {
    let test_db = TestDb::new();
    let kv = test_db.kv();

    let branch_a = BranchId::new();
    let branch_b = BranchId::new();

    // Write same key to both runs
    kv.put(&branch_a, "shared_key", Value::Int(1)).unwrap();
    kv.put(&branch_b, "shared_key", Value::Int(2)).unwrap();

    // Delete from run A
    kv.delete(&branch_a, "shared_key").unwrap();

    // Run A should be empty, run B should have data
    assert!(kv.get(&branch_a, "shared_key").unwrap().is_none());
    assert_eq!(kv.get(&branch_b, "shared_key").unwrap(), Some(Value::Int(2)));
}

#[test]
fn all_primitives_isolated_between_runs() {
    let test_db = TestDb::new();
    let p = test_db.all_primitives();

    let branch_a = BranchId::new();
    let branch_b = BranchId::new();

    // Write to run A
    p.kv.put(&branch_a, "k", Value::Int(1)).unwrap();
    p.state.init(&branch_a, "s", Value::Int(1)).unwrap();
    p.event.append(&branch_a, "e", int_payload(1)).unwrap();
    p.json.create(&branch_a, "j", json_value(serde_json::json!({"a": 1}))).unwrap();
    p.vector.create_collection(branch_a, "v", config_small()).unwrap();
    p.vector.insert(branch_a, "v", "vec", &[1.0, 0.0, 0.0], None).unwrap();

    // Write different values to run B
    p.kv.put(&branch_b, "k", Value::Int(2)).unwrap();
    p.state.init(&branch_b, "s", Value::Int(2)).unwrap();
    p.event.append(&branch_b, "e", int_payload(2)).unwrap();
    p.json.create(&branch_b, "j", json_value(serde_json::json!({"b": 2}))).unwrap();
    p.vector.create_collection(branch_b, "v", config_small()).unwrap();
    p.vector.insert(branch_b, "v", "vec", &[0.0, 1.0, 0.0], None).unwrap();

    // Verify isolation
    assert_eq!(p.kv.get(&branch_a, "k").unwrap().unwrap(), Value::Int(1));
    assert_eq!(p.kv.get(&branch_b, "k").unwrap().unwrap(), Value::Int(2));

    assert_eq!(p.state.read(&branch_a, "s").unwrap().unwrap(), Value::Int(1));
    assert_eq!(p.state.read(&branch_b, "s").unwrap().unwrap(), Value::Int(2));

    let events_a = p.event.read_by_type(&branch_a, "e").unwrap();
    let events_b = p.event.read_by_type(&branch_b, "e").unwrap();
    assert_eq!(events_a.len(), 1);
    assert_eq!(events_b.len(), 1);

    let json_a = p.json.get(&branch_a, "j", &root()).unwrap().unwrap();
    let json_b = p.json.get(&branch_b, "j", &root()).unwrap().unwrap();
    assert_eq!(json_a.as_inner().get("a"), Some(&serde_json::json!(1)));
    assert_eq!(json_b.as_inner().get("b"), Some(&serde_json::json!(2)));

    let vec_a = p.vector.get(branch_a, "v", "vec").unwrap().unwrap();
    let vec_b = p.vector.get(branch_b, "v", "vec").unwrap().unwrap();
    assert_eq!(vec_a.value.embedding[0], 1.0);
    assert_eq!(vec_b.value.embedding[1], 1.0);
}

#[test]
fn many_concurrent_runs() {
    let test_db = TestDb::new();
    let kv = test_db.kv();

    // Create 100 runs with data
    let branch_ids: Vec<BranchId> = (0..100).map(|_| BranchId::new()).collect();

    for (i, branch_id) in branch_ids.iter().enumerate() {
        kv.put(branch_id, "index", Value::Int(i as i64)).unwrap();
    }

    // Verify each run has correct isolated data
    for (i, branch_id) in branch_ids.iter().enumerate() {
        let val = kv.get(branch_id, "index").unwrap().unwrap();
        assert_eq!(val, Value::Int(i as i64));
    }
}

// ============================================================================
// Run Lifecycle (via BranchIndex)
// ============================================================================

#[test]
fn create_and_list_runs() {
    let test_db = TestDb::new();
    let run_index = test_db.run_index();

    // Create some runs
    run_index.create_branch("run_1").unwrap();
    run_index.create_branch("run_2").unwrap();
    run_index.create_branch("run_3").unwrap();

    // List all runs
    let runs = run_index.list_branches().unwrap();
    assert!(runs.len() >= 3);

    // Verify our runs exist
    assert!(runs.contains(&"run_1".to_string()));
    assert!(runs.contains(&"run_2".to_string()));
    assert!(runs.contains(&"run_3".to_string()));
}

#[test]
fn run_with_metadata() {
    let test_db = TestDb::new();
    let run_index = test_db.run_index();

    // create_branch creates a run with default metadata;
    // verify we can retrieve the run and it has the expected fields.
    run_index.create_branch("with_metadata").unwrap();

    let run = run_index.get_branch("with_metadata").unwrap().unwrap();
    assert_eq!(run.value.name, "with_metadata");
}

// ============================================================================
// Run Isolation with Data Operations
// ============================================================================

#[test]
fn vector_collections_isolated_per_run() {
    let test_db = TestDb::new();
    let vector = test_db.vector();

    let branch_a = BranchId::new();
    let branch_b = BranchId::new();

    // Same collection name, different runs
    vector.create_collection(branch_a, "embeddings", config_small()).unwrap();
    vector.create_collection(branch_b, "embeddings", config_small()).unwrap();

    vector.insert(branch_a, "embeddings", "vec", &[1.0, 0.0, 0.0], None).unwrap();
    vector.insert(branch_b, "embeddings", "vec", &[0.0, 1.0, 0.0], None).unwrap();

    // Verify isolation
    let vec_a = vector.get(branch_a, "embeddings", "vec").unwrap().unwrap();
    let vec_b = vector.get(branch_b, "embeddings", "vec").unwrap().unwrap();

    assert_eq!(vec_a.value.embedding[0], 1.0);
    assert_eq!(vec_b.value.embedding[1], 1.0);
}

#[test]
fn event_streams_isolated_per_run() {
    let test_db = TestDb::new();
    let event = test_db.event();

    let branch_a = BranchId::new();
    let branch_b = BranchId::new();

    // Same stream name, different runs
    event.append(&branch_a, "audit", int_payload(100)).unwrap();
    event.append(&branch_a, "audit", int_payload(101)).unwrap();
    event.append(&branch_b, "audit", int_payload(200)).unwrap();

    assert_eq!(event.read_by_type(&branch_a, "audit").unwrap().len(), 2);
    assert_eq!(event.read_by_type(&branch_b, "audit").unwrap().len(), 1);
}

#[test]
fn json_documents_isolated_per_run() {
    let test_db = TestDb::new();
    let json = test_db.json();

    let branch_a = BranchId::new();
    let branch_b = BranchId::new();

    // Same doc ID, different runs
    json.create(&branch_a, "config", json_value(serde_json::json!({"version": 1}))).unwrap();
    json.create(&branch_b, "config", json_value(serde_json::json!({"version": 2}))).unwrap();

    let doc_a = json.get(&branch_a, "config", &path(".version")).unwrap().unwrap();
    let doc_b = json.get(&branch_b, "config", &path(".version")).unwrap().unwrap();

    assert_eq!(doc_a.as_inner(), &serde_json::json!(1));
    assert_eq!(doc_b.as_inner(), &serde_json::json!(2));
}

// ============================================================================
// Run Forking Tests (Document Current Behavior)
// ============================================================================

/// Note: Run forking (parent_run option) currently does NOT copy parent data.
/// This is a known issue (#780). This test documents current behavior.
#[test]
fn child_run_does_not_inherit_parent_data_currently() {
    let test_db = TestDb::new();
    let run_index = test_db.run_index();
    let kv = test_db.kv();

    // Create parent run and get its branch_id
    let parent_meta = run_index.create_branch("parent").unwrap();
    let parent_run_id = BranchId::from_string(&parent_meta.value.branch_id).unwrap();

    kv.put(&parent_run_id, "parent_key", Value::String("parent_value".into()))
        .unwrap();

    // Create child run (parent reference is not supported in current API)
    let child_meta = run_index.create_branch("child").unwrap();
    let child_run_id = BranchId::from_string(&child_meta.value.branch_id).unwrap();

    // Currently: child does NOT inherit parent's data (this is a bug/missing feature)
    let child_value = kv.get(&child_run_id, "parent_key").unwrap();

    // Document current behavior: child doesn't see parent's data
    assert!(
        child_value.is_none(),
        "CURRENT BEHAVIOR: Child runs do not inherit parent data. See issue #780."
    );

    // Parent data should still exist
    let parent_value = kv.get(&parent_run_id, "parent_key").unwrap();
    assert_eq!(parent_value, Some(Value::String("parent_value".into())), "Parent data should remain");
}

/// When forking is properly implemented, child should inherit all parent data.
/// This test is ignored until #780 is fixed.
#[test]
#[ignore = "Waiting for issue #780 - RunCreateChild should copy parent data"]
fn child_run_should_inherit_parent_data() {
    let test_db = TestDb::new();
    let run_index = test_db.run_index();
    let p = test_db.all_primitives();

    // Create parent run
    let parent_meta = run_index.create_branch("fork_parent").unwrap();
    let parent_id = BranchId::from_string(&parent_meta.value.branch_id).unwrap();

    // Add data to parent
    p.kv.put(&parent_id, "config", Value::String("inherited".into()))
        .unwrap();
    p.state
        .init(&parent_id, "status", Value::String("active".into()))
        .unwrap();
    p.event
        .append(&parent_id, "history", int_payload(1))
        .unwrap();
    p.json
        .create(&parent_id, "context", json_value(serde_json::json!({"fork": true})))
        .unwrap();
    p.vector
        .create_collection(parent_id, "memory", config_small())
        .unwrap();
    p.vector
        .insert(parent_id, "memory", "m1", &[1.0, 0.0, 0.0], None)
        .unwrap();

    // Fork (create child with parent) - using create_branch since create_run_with_options doesn't exist
    let child_meta = run_index.create_branch("fork_child").unwrap();
    let child_id = BranchId::from_string(&child_meta.value.branch_id).unwrap();

    // Child SHOULD have all parent's data (when #780 is fixed)
    assert_eq!(p.kv.get(&child_id, "config").unwrap(), Some(Value::String("inherited".into())));
    assert_eq!(p.state.read(&child_id, "status").unwrap().unwrap(), Value::String("active".into()));
    assert!(!p.event.read_by_type(&child_id, "history").unwrap().is_empty());
    assert_eq!(p.json.get(&child_id, "context", &root()).unwrap().unwrap().as_inner(), &serde_json::json!({"fork": true}));
    assert_eq!(p.vector.get(child_id, "memory", "m1").unwrap().unwrap().value.embedding, vec![1.0f32, 0.0, 0.0]);

    // Modifications to child should not affect parent
    p.kv.put(&child_id, "config", Value::String("modified".into()))
        .unwrap();

    let parent_config = p.kv.get(&parent_id, "config").unwrap().unwrap();
    assert_eq!(parent_config, Value::String("inherited".into()));
}

// ============================================================================
// Run Isolation Stress Test
// ============================================================================

#[test]
fn concurrent_operations_across_runs() {
    use std::sync::{Arc, Barrier};
    use std::thread;

    let test_db = TestDb::new();
    let db = test_db.db.clone();

    let num_runs = 10;
    let ops_per_run = 100;
    let barrier = Arc::new(Barrier::new(num_runs));

    let handles: Vec<_> = (0..num_runs)
        .map(|r| {
            let db = db.clone();
            let barrier = barrier.clone();
            thread::spawn(move || {
                let branch_id = BranchId::new();
                let kv = KVStore::new(db.clone());
                let event = EventLog::new(db);

                barrier.wait();

                for i in 0..ops_per_run {
                    kv.put(&branch_id, &format!("key_{}", i), Value::Int((r * 1000 + i) as i64))
                        .unwrap();
                    event
                        .append(&branch_id, "ops", int_payload((r * 1000 + i) as i64))
                        .unwrap();
                }

                // Verify own data
                for i in 0..ops_per_run {
                    let val = kv.get(&branch_id, &format!("key_{}", i)).unwrap().unwrap();
                    assert_eq!(val, Value::Int((r * 1000 + i) as i64));
                }

                branch_id
            })
        })
        .collect();

    let branch_ids: Vec<BranchId> = handles.into_iter().map(|h| h.join().unwrap()).collect();

    // Verify all runs have correct isolated data
    let kv = KVStore::new(test_db.db.clone());
    for (r, branch_id) in branch_ids.iter().enumerate() {
        let keys = kv.list(branch_id, Some("key_")).unwrap();
        assert_eq!(keys.len(), ops_per_run);

        for i in 0..ops_per_run {
            let val = kv.get(branch_id, &format!("key_{}", i)).unwrap().unwrap();
            assert_eq!(val, Value::Int((r * 1000 + i) as i64));
        }
    }
}
