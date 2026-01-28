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

    let run_a = RunId::new();
    let run_b = RunId::new();

    // Write to run A
    kv.put(&run_a, "key", Value::String("value_a".into())).unwrap();

    // Write to run B
    kv.put(&run_b, "key", Value::String("value_b".into())).unwrap();

    // Each run sees only its own data
    let val_a = kv.get(&run_a, "key").unwrap().unwrap();
    let val_b = kv.get(&run_b, "key").unwrap().unwrap();

    assert_eq!(val_a.value, Value::String("value_a".into()));
    assert_eq!(val_b.value, Value::String("value_b".into()));
}

#[test]
fn delete_in_one_run_doesnt_affect_other() {
    let test_db = TestDb::new();
    let kv = test_db.kv();

    let run_a = RunId::new();
    let run_b = RunId::new();

    // Write same key to both runs
    kv.put(&run_a, "shared_key", Value::Int(1)).unwrap();
    kv.put(&run_b, "shared_key", Value::Int(2)).unwrap();

    // Delete from run A
    kv.delete(&run_a, "shared_key").unwrap();

    // Run A should be empty, run B should have data
    assert!(kv.get(&run_a, "shared_key").unwrap().is_none());
    assert!(kv.get(&run_b, "shared_key").unwrap().is_some());
}

#[test]
fn all_primitives_isolated_between_runs() {
    let test_db = TestDb::new();
    let p = test_db.all_primitives();

    let run_a = RunId::new();
    let run_b = RunId::new();

    // Write to run A
    p.kv.put(&run_a, "k", Value::Int(1)).unwrap();
    p.state.init(&run_a, "s", Value::Int(1)).unwrap();
    p.event.append(&run_a, "e", int_payload(1)).unwrap();
    p.json.create(&run_a, "j", json_value(serde_json::json!({"a": 1}))).unwrap();
    p.vector.create_collection(run_a, "v", config_small()).unwrap();
    p.vector.insert(run_a, "v", "vec", &[1.0, 0.0, 0.0], None).unwrap();

    // Write different values to run B
    p.kv.put(&run_b, "k", Value::Int(2)).unwrap();
    p.state.init(&run_b, "s", Value::Int(2)).unwrap();
    p.event.append(&run_b, "e", int_payload(2)).unwrap();
    p.json.create(&run_b, "j", json_value(serde_json::json!({"b": 2}))).unwrap();
    p.vector.create_collection(run_b, "v", config_small()).unwrap();
    p.vector.insert(run_b, "v", "vec", &[0.0, 1.0, 0.0], None).unwrap();

    // Verify isolation
    assert_eq!(p.kv.get(&run_a, "k").unwrap().unwrap().value, Value::Int(1));
    assert_eq!(p.kv.get(&run_b, "k").unwrap().unwrap().value, Value::Int(2));

    assert_eq!(p.state.read(&run_a, "s").unwrap().unwrap().value.value, Value::Int(1));
    assert_eq!(p.state.read(&run_b, "s").unwrap().unwrap().value.value, Value::Int(2));

    let events_a = p.event.read_by_type(&run_a, "e").unwrap();
    let events_b = p.event.read_by_type(&run_b, "e").unwrap();
    assert_eq!(events_a.len(), 1);
    assert_eq!(events_b.len(), 1);

    let json_a = p.json.get(&run_a, "j", &root()).unwrap().unwrap();
    let json_b = p.json.get(&run_b, "j", &root()).unwrap().unwrap();
    assert!(json_a.value.as_inner().get("a").is_some());
    assert!(json_b.value.as_inner().get("b").is_some());

    let vec_a = p.vector.get(run_a, "v", "vec").unwrap().unwrap();
    let vec_b = p.vector.get(run_b, "v", "vec").unwrap().unwrap();
    assert_eq!(vec_a.value.embedding[0], 1.0);
    assert_eq!(vec_b.value.embedding[1], 1.0);
}

#[test]
fn many_concurrent_runs() {
    let test_db = TestDb::new();
    let kv = test_db.kv();

    // Create 100 runs with data
    let run_ids: Vec<RunId> = (0..100).map(|_| RunId::new()).collect();

    for (i, run_id) in run_ids.iter().enumerate() {
        kv.put(run_id, "index", Value::Int(i as i64)).unwrap();
    }

    // Verify each run has correct isolated data
    for (i, run_id) in run_ids.iter().enumerate() {
        let val = kv.get(run_id, "index").unwrap().unwrap();
        assert_eq!(val.value, Value::Int(i as i64));
    }
}

// ============================================================================
// Run Lifecycle (via RunIndex)
// ============================================================================

#[test]
fn create_and_list_runs() {
    let test_db = TestDb::new();
    let run_index = test_db.run_index();

    // Create some runs
    run_index.create_run("run_1").unwrap();
    run_index.create_run("run_2").unwrap();
    run_index.create_run("run_3").unwrap();

    // List all runs
    let runs = run_index.list_runs().unwrap();
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

    let metadata = Value::Object(
        [("purpose".to_string(), Value::String("test".into()))]
            .into_iter()
            .collect(),
    );

    run_index
        .create_run_with_options("with_metadata", None, vec![], metadata.clone())
        .unwrap();

    let run = run_index.get_run("with_metadata").unwrap().unwrap();
    assert_eq!(run.value.metadata, metadata);
}

#[test]
fn run_tags() {
    let test_db = TestDb::new();
    let run_index = test_db.run_index();

    run_index.create_run("tagged_run").unwrap();
    run_index
        .add_tags("tagged_run", vec!["test".to_string(), "integration".to_string()])
        .unwrap();

    let run = run_index.get_run("tagged_run").unwrap().unwrap();
    assert!(run.value.tags.contains(&"test".to_string()));
    assert!(run.value.tags.contains(&"integration".to_string()));
}

// ============================================================================
// Run Isolation with Data Operations
// ============================================================================

#[test]
fn vector_collections_isolated_per_run() {
    let test_db = TestDb::new();
    let vector = test_db.vector();

    let run_a = RunId::new();
    let run_b = RunId::new();

    // Same collection name, different runs
    vector.create_collection(run_a, "embeddings", config_small()).unwrap();
    vector.create_collection(run_b, "embeddings", config_small()).unwrap();

    vector.insert(run_a, "embeddings", "vec", &[1.0, 0.0, 0.0], None).unwrap();
    vector.insert(run_b, "embeddings", "vec", &[0.0, 1.0, 0.0], None).unwrap();

    // Verify isolation
    let vec_a = vector.get(run_a, "embeddings", "vec").unwrap().unwrap();
    let vec_b = vector.get(run_b, "embeddings", "vec").unwrap().unwrap();

    assert_eq!(vec_a.value.embedding[0], 1.0);
    assert_eq!(vec_b.value.embedding[1], 1.0);
}

#[test]
fn event_streams_isolated_per_run() {
    let test_db = TestDb::new();
    let event = test_db.event();

    let run_a = RunId::new();
    let run_b = RunId::new();

    // Same stream name, different runs
    event.append(&run_a, "audit", int_payload(100)).unwrap();
    event.append(&run_a, "audit", int_payload(101)).unwrap();
    event.append(&run_b, "audit", int_payload(200)).unwrap();

    assert_eq!(event.len_by_type(&run_a, "audit").unwrap(), 2);
    assert_eq!(event.len_by_type(&run_b, "audit").unwrap(), 1);
}

#[test]
fn json_documents_isolated_per_run() {
    let test_db = TestDb::new();
    let json = test_db.json();

    let run_a = RunId::new();
    let run_b = RunId::new();

    // Same doc ID, different runs
    json.create(&run_a, "config", json_value(serde_json::json!({"version": 1}))).unwrap();
    json.create(&run_b, "config", json_value(serde_json::json!({"version": 2}))).unwrap();

    let doc_a = json.get(&run_a, "config", &path(".version")).unwrap().unwrap();
    let doc_b = json.get(&run_b, "config", &path(".version")).unwrap().unwrap();

    assert_eq!(doc_a.value.as_inner(), &serde_json::json!(1));
    assert_eq!(doc_b.value.as_inner(), &serde_json::json!(2));
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

    // Create parent run and get its run_id
    let parent_meta = run_index.create_run("parent").unwrap();
    let parent_run_id = RunId::from_string(&parent_meta.value.run_id).unwrap();

    kv.put(&parent_run_id, "parent_key", Value::String("parent_value".into()))
        .unwrap();

    // Create child run with parent reference
    let child_meta = run_index
        .create_run_with_options("child", Some("parent".to_string()), vec![], Value::Null)
        .unwrap();
    let child_run_id = RunId::from_string(&child_meta.value.run_id).unwrap();

    // Currently: child does NOT inherit parent's data (this is a bug/missing feature)
    let child_value = kv.get(&child_run_id, "parent_key").unwrap();

    // Document current behavior: child doesn't see parent's data
    assert!(
        child_value.is_none(),
        "CURRENT BEHAVIOR: Child runs do not inherit parent data. See issue #780."
    );

    // Parent data should still exist
    let parent_value = kv.get(&parent_run_id, "parent_key").unwrap();
    assert!(parent_value.is_some(), "Parent data should remain");
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
    let parent_meta = run_index.create_run("fork_parent").unwrap();
    let parent_id = RunId::from_string(&parent_meta.value.run_id).unwrap();

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

    // Fork (create child with parent)
    let child_meta = run_index
        .create_run_with_options("fork_child", Some("fork_parent".to_string()), vec![], Value::Null)
        .unwrap();
    let child_id = RunId::from_string(&child_meta.value.run_id).unwrap();

    // Child SHOULD have all parent's data (when #780 is fixed)
    assert!(p.kv.get(&child_id, "config").unwrap().is_some());
    assert!(p.state.read(&child_id, "status").unwrap().is_some());
    assert!(p.event.len_by_type(&child_id, "history").unwrap() > 0);
    assert!(p.json.get(&child_id, "context", &root()).unwrap().is_some());
    assert!(p.vector.get(child_id, "memory", "m1").unwrap().is_some());

    // Modifications to child should not affect parent
    p.kv.put(&child_id, "config", Value::String("modified".into()))
        .unwrap();

    let parent_config = p.kv.get(&parent_id, "config").unwrap().unwrap();
    assert_eq!(parent_config.value, Value::String("inherited".into()));
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
                let run_id = RunId::new();
                let kv = KVStore::new(db.clone());
                let event = EventLog::new(db);

                barrier.wait();

                for i in 0..ops_per_run {
                    kv.put(&run_id, &format!("key_{}", i), Value::Int((r * 1000 + i) as i64))
                        .unwrap();
                    event
                        .append(&run_id, "ops", int_payload((r * 1000 + i) as i64))
                        .unwrap();
                }

                // Verify own data
                for i in 0..ops_per_run {
                    let val = kv.get(&run_id, &format!("key_{}", i)).unwrap().unwrap();
                    assert_eq!(val.value, Value::Int((r * 1000 + i) as i64));
                }

                run_id
            })
        })
        .collect();

    let run_ids: Vec<RunId> = handles.into_iter().map(|h| h.join().unwrap()).collect();

    // Verify all runs have correct isolated data
    let kv = KVStore::new(test_db.db.clone());
    for (r, run_id) in run_ids.iter().enumerate() {
        let keys = kv.list(run_id, Some("key_")).unwrap();
        assert_eq!(keys.len(), ops_per_run);

        for i in 0..ops_per_run {
            let val = kv.get(run_id, &format!("key_{}", i)).unwrap().unwrap();
            assert_eq!(val.value, Value::Int((r * 1000 + i) as i64));
        }
    }
}
