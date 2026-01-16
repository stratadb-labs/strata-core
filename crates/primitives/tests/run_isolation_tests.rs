//! Run Isolation Integration Tests (Story #198)
//!
//! Tests verifying that different runs are completely isolated from each other.
//! Each run has its own namespace and cannot see or affect other runs' data.

use in_mem_core::types::RunId;
use in_mem_core::value::Value;
use in_mem_engine::Database;
use in_mem_primitives::{EventLog, KVStore, RunIndex, StateCell, TraceStore, TraceType};
use std::sync::Arc;
use tempfile::TempDir;

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
    kv.put(&run1, "key", Value::I64(1)).unwrap();
    kv.put(&run2, "key", Value::I64(2)).unwrap();

    // Each run sees ONLY its own data
    assert_eq!(kv.get(&run1, "key").unwrap(), Some(Value::I64(1)));
    assert_eq!(kv.get(&run2, "key").unwrap(), Some(Value::I64(2)));

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

    let run1 = RunId::new();
    let run2 = RunId::new();

    // Both runs start at sequence 0
    let (seq1, _) = event_log
        .append(&run1, "event", Value::String("run1".into()))
        .unwrap();
    let (seq2, _) = event_log
        .append(&run2, "event", Value::String("run2".into()))
        .unwrap();

    // Independent sequences - both start at 0
    assert_eq!(seq1, 0);
    assert_eq!(seq2, 0);

    // Each has length 1
    assert_eq!(event_log.len(&run1).unwrap(), 1);
    assert_eq!(event_log.len(&run2).unwrap(), 1);

    // Chain verification is per-run
    assert!(event_log.verify_chain(&run1).unwrap().is_valid);
    assert!(event_log.verify_chain(&run2).unwrap().is_valid);

    // Appending more to run1 doesn't affect run2
    event_log
        .append(&run1, "event2", Value::String("run1-2".into()))
        .unwrap();
    event_log
        .append(&run1, "event3", Value::String("run1-3".into()))
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
    state_cell.init(&run1, "counter", Value::I64(0)).unwrap();
    state_cell.init(&run2, "counter", Value::I64(100)).unwrap();

    // Each run sees its own value
    let state1 = state_cell.read(&run1, "counter").unwrap().unwrap();
    let state2 = state_cell.read(&run2, "counter").unwrap().unwrap();

    assert_eq!(state1.value, Value::I64(0));
    assert_eq!(state2.value, Value::I64(100));

    // Both start at version 1
    assert_eq!(state1.version, 1);
    assert_eq!(state2.version, 1);

    // CAS on run1 doesn't affect run2
    state_cell.cas(&run1, "counter", 1, Value::I64(10)).unwrap();

    let state1 = state_cell.read(&run1, "counter").unwrap().unwrap();
    let state2 = state_cell.read(&run2, "counter").unwrap().unwrap();

    assert_eq!(state1.value, Value::I64(10));
    assert_eq!(state1.version, 2);
    assert_eq!(state2.value, Value::I64(100)); // Unchanged
    assert_eq!(state2.version, 1); // Unchanged
}

/// Test TraceStore isolation - queries respect run boundaries
#[test]
fn test_trace_store_isolation() {
    let (db, _temp) = setup();
    let trace_store = TraceStore::new(db.clone());

    let run1 = RunId::new();
    let run2 = RunId::new();

    // Record traces in both runs
    trace_store
        .record(
            &run1,
            TraceType::Thought {
                content: "run1 thought".into(),
                confidence: None,
            },
            vec!["tag1".into()],
            Value::Null,
        )
        .unwrap();

    trace_store
        .record(
            &run2,
            TraceType::Thought {
                content: "run2 thought".into(),
                confidence: None,
            },
            vec!["tag2".into()],
            Value::Null,
        )
        .unwrap();

    // Each run has exactly 1 trace
    assert_eq!(trace_store.count(&run1).unwrap(), 1);
    assert_eq!(trace_store.count(&run2).unwrap(), 1);

    // Queries are isolated
    let run1_traces = trace_store.query_by_type(&run1, "Thought").unwrap();
    let run2_traces = trace_store.query_by_type(&run2, "Thought").unwrap();

    assert_eq!(run1_traces.len(), 1);
    assert_eq!(run2_traces.len(), 1);

    // Tag queries are isolated
    let tag1_traces = trace_store.query_by_tag(&run1, "tag1").unwrap();
    let tag2_traces = trace_store.query_by_tag(&run2, "tag2").unwrap();

    assert_eq!(tag1_traces.len(), 1);
    assert_eq!(tag2_traces.len(), 1);

    // Cross-query returns nothing
    let empty1 = trace_store.query_by_tag(&run1, "tag2").unwrap();
    let empty2 = trace_store.query_by_tag(&run2, "tag1").unwrap();

    assert_eq!(empty1.len(), 0);
    assert_eq!(empty2.len(), 0);
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
    let trace_store = TraceStore::new(db.clone());

    // Run1 data
    for i in 0..10 {
        kv.put(&run1, &format!("key{}", i), Value::I64(i)).unwrap();
        event_log.append(&run1, "event", Value::I64(i)).unwrap();
        trace_store
            .record(
                &run1,
                TraceType::Thought {
                    content: format!("run1 trace {}", i),
                    confidence: None,
                },
                vec![],
                Value::Null,
            )
            .unwrap();
    }
    state_cell
        .init(&run1, "state", Value::String("run1".into()))
        .unwrap();

    // Run2 data
    for i in 0..5 {
        kv.put(&run2, &format!("key{}", i), Value::I64(i + 100))
            .unwrap();
        event_log
            .append(&run2, "event", Value::I64(i + 100))
            .unwrap();
        trace_store
            .record(
                &run2,
                TraceType::Thought {
                    content: format!("run2 trace {}", i),
                    confidence: None,
                },
                vec![],
                Value::Null,
            )
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

    assert_eq!(trace_store.count(&run1).unwrap(), 10);
    assert_eq!(trace_store.count(&run2).unwrap(), 5);

    // Verify values are isolated
    assert_eq!(kv.get(&run1, "key0").unwrap(), Some(Value::I64(0)));
    assert_eq!(kv.get(&run2, "key0").unwrap(), Some(Value::I64(100)));

    // Run1 cannot see run2's keys that don't exist in run1
    // (run2 only has key0-key4, run1 has key0-key9)
    // Actually both have overlapping key names, but different values
    assert_eq!(
        state_cell.read(&run1, "state").unwrap().unwrap().value,
        Value::String("run1".into())
    );
    assert_eq!(
        state_cell.read(&run2, "state").unwrap().unwrap().value,
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
    let trace_store = TraceStore::new(db.clone());

    // Create two runs via RunIndex
    let meta1 = run_index.create_run("run1").unwrap();
    let meta2 = run_index.create_run("run2").unwrap();

    let run1 = RunId::from_string(&meta1.run_id).unwrap();
    let run2 = RunId::from_string(&meta2.run_id).unwrap();

    // Write data to both runs
    kv.put(&run1, "key", Value::I64(1)).unwrap();
    kv.put(&run2, "key", Value::I64(2)).unwrap();

    event_log.append(&run1, "event", Value::Null).unwrap();
    event_log.append(&run2, "event", Value::Null).unwrap();

    state_cell.init(&run1, "cell", Value::I64(10)).unwrap();
    state_cell.init(&run2, "cell", Value::I64(20)).unwrap();

    trace_store
        .record(
            &run1,
            TraceType::Thought {
                content: "run1".into(),
                confidence: None,
            },
            vec![],
            Value::Null,
        )
        .unwrap();
    trace_store
        .record(
            &run2,
            TraceType::Thought {
                content: "run2".into(),
                confidence: None,
            },
            vec![],
            Value::Null,
        )
        .unwrap();

    // Verify both runs have data
    assert!(kv.get(&run1, "key").unwrap().is_some());
    assert!(kv.get(&run2, "key").unwrap().is_some());

    // Delete run1 (cascading delete)
    run_index.delete_run("run1").unwrap();

    // run1 data is GONE
    assert!(kv.get(&run1, "key").unwrap().is_none());
    assert_eq!(event_log.len(&run1).unwrap(), 0);
    assert!(!state_cell.exists(&run1, "cell").unwrap());
    assert_eq!(trace_store.count(&run1).unwrap(), 0);

    // run2 data is UNTOUCHED
    assert_eq!(kv.get(&run2, "key").unwrap(), Some(Value::I64(2)));
    assert_eq!(event_log.len(&run2).unwrap(), 1);
    assert!(state_cell.exists(&run2, "cell").unwrap());
    assert_eq!(trace_store.count(&run2).unwrap(), 1);
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
        kv.put(run_id, "value", Value::I64(i as i64)).unwrap();
        kv.put(run_id, "run_index", Value::I64(i as i64)).unwrap();
    }

    // Verify each run sees only its data
    for (i, run_id) in runs.iter().enumerate() {
        let value = kv.get(run_id, "value").unwrap().unwrap();
        assert_eq!(value, Value::I64(i as i64));

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
    state_cell.init(&run1, "cell", Value::I64(0)).unwrap();
    state_cell.init(&run2, "cell", Value::I64(0)).unwrap();

    // CAS on run1 with version 1
    state_cell.cas(&run1, "cell", 1, Value::I64(10)).unwrap();

    // CAS on run2 with version 1 should ALSO succeed (independent versions)
    let result = state_cell.cas(&run2, "cell", 1, Value::I64(20));
    assert!(result.is_ok());

    // Both have been updated
    let s1 = state_cell.read(&run1, "cell").unwrap().unwrap();
    let s2 = state_cell.read(&run2, "cell").unwrap().unwrap();

    assert_eq!(s1.value, Value::I64(10));
    assert_eq!(s1.version, 2);
    assert_eq!(s2.value, Value::I64(20));
    assert_eq!(s2.version, 2);
}

/// Test EventLog chain isolation - chains are independent per run
#[test]
fn test_event_log_chain_isolation() {
    let (db, _temp) = setup();
    let event_log = EventLog::new(db.clone());

    let run1 = RunId::new();
    let run2 = RunId::new();

    // Build chain in run1
    let (_, hash1_0) = event_log.append(&run1, "e1", Value::I64(0)).unwrap();
    let (_, _hash1_1) = event_log.append(&run1, "e2", Value::I64(1)).unwrap();
    let (_, _hash1_2) = event_log.append(&run1, "e3", Value::I64(2)).unwrap();

    // Build different chain in run2
    let (_, hash2_0) = event_log.append(&run2, "x1", Value::I64(100)).unwrap();
    let (_, _hash2_1) = event_log.append(&run2, "x2", Value::I64(101)).unwrap();

    // Chains have different hashes (different content)
    assert_ne!(hash1_0, hash2_0);

    // Read event from run1 - prev_hash links within run1 only
    let event1_1 = event_log.read(&run1, 1).unwrap().unwrap();
    assert_eq!(event1_1.prev_hash, hash1_0);

    // Verify chains independently
    assert!(event_log.verify_chain(&run1).unwrap().is_valid);
    assert!(event_log.verify_chain(&run2).unwrap().is_valid);

    // Corrupting run1 chain doesn't affect run2 verification
    // (We can't easily corrupt without direct DB access, so just verify both remain valid)
    assert_eq!(event_log.verify_chain(&run1).unwrap().length, 3);
    assert_eq!(event_log.verify_chain(&run2).unwrap().length, 2);
}

/// Test TraceStore parent-child relationships are isolated per run
#[test]
fn test_trace_parent_child_isolation() {
    let (db, _temp) = setup();
    let trace_store = TraceStore::new(db.clone());

    let run1 = RunId::new();
    let run2 = RunId::new();

    // Create parent-child in run1
    let parent1 = trace_store
        .record(
            &run1,
            TraceType::ToolCall {
                tool_name: "search".into(),
                arguments: Value::Null,
                result: None,
                duration_ms: None,
            },
            vec![],
            Value::Null,
        )
        .unwrap();

    let _child1 = trace_store
        .record_child(
            &run1,
            &parent1,
            TraceType::Thought {
                content: "child of run1".into(),
                confidence: None,
            },
            vec![],
            Value::Null,
        )
        .unwrap();

    // Create parent-child in run2 with same parent ID format
    let parent2 = trace_store
        .record(
            &run2,
            TraceType::ToolCall {
                tool_name: "search".into(),
                arguments: Value::Null,
                result: None,
                duration_ms: None,
            },
            vec![],
            Value::Null,
        )
        .unwrap();

    let _child2 = trace_store
        .record_child(
            &run2,
            &parent2,
            TraceType::Thought {
                content: "child of run2".into(),
                confidence: None,
            },
            vec![],
            Value::Null,
        )
        .unwrap();

    // Get children in run1 - should only see run1's child
    let children1 = trace_store.get_children(&run1, &parent1).unwrap();
    assert_eq!(children1.len(), 1);

    // Get children in run2 - should only see run2's child
    let children2 = trace_store.get_children(&run2, &parent2).unwrap();
    assert_eq!(children2.len(), 1);

    // Cross-run parent lookup returns nothing
    let cross1 = trace_store.get_children(&run1, &parent2).unwrap();
    let cross2 = trace_store.get_children(&run2, &parent1).unwrap();
    assert_eq!(cross1.len(), 0);
    assert_eq!(cross2.len(), 0);
}
