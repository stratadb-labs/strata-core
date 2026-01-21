//! Primitive Recovery Tests (Story #199)
//!
//! Tests verifying that ALL primitives survive crash + WAL replay.
//! The recovery contract ensures:
//! - Sequence numbers: Preserved
//! - Secondary indices: Replayed, not rebuilt
//! - Derived keys (hashes): Stored, not recomputed

use strata_core::types::RunId;
use strata_core::value::Value;
use strata_engine::Database;
use strata_primitives::{EventLog, KVStore, RunIndex, RunStatus, StateCell, TraceStore, TraceType};
use std::path::PathBuf;
use std::sync::Arc;
use tempfile::TempDir;

fn setup() -> (Arc<Database>, TempDir, RunId) {
    let temp_dir = TempDir::new().unwrap();
    let db = Arc::new(Database::open(temp_dir.path()).unwrap());
    let run_id = RunId::new();
    (db, temp_dir, run_id)
}

/// Helper to get the path from TempDir
fn get_path(temp_dir: &TempDir) -> PathBuf {
    temp_dir.path().to_path_buf()
}

/// Test KV data survives recovery
#[test]
fn test_kv_survives_recovery() {
    let (db, temp_dir, run_id) = setup();
    let path = get_path(&temp_dir);

    // Write KV data
    let kv = KVStore::new(db.clone());
    kv.put(&run_id, "key1", Value::String("value1".into()))
        .unwrap();
    kv.put(&run_id, "key2", Value::I64(42)).unwrap();
    kv.put(&run_id, "nested/path/key", Value::Bool(true))
        .unwrap();

    // Verify before crash
    assert_eq!(
        kv.get(&run_id, "key1").unwrap().map(|v| v.value),
        Some(Value::String("value1".into()))
    );

    // Simulate crash
    drop(kv);
    drop(db);

    // Recovery
    let db = Arc::new(Database::open(&path).unwrap());
    let kv = KVStore::new(db.clone());

    // Data survived
    assert_eq!(
        kv.get(&run_id, "key1").unwrap().map(|v| v.value),
        Some(Value::String("value1".into()))
    );
    assert_eq!(kv.get(&run_id, "key2").unwrap().map(|v| v.value), Some(Value::I64(42)));
    assert_eq!(
        kv.get(&run_id, "nested/path/key").unwrap().map(|v| v.value),
        Some(Value::Bool(true))
    );

    // Can still write after recovery
    kv.put(&run_id, "key3", Value::String("after_recovery".into()))
        .unwrap();
    assert_eq!(
        kv.get(&run_id, "key3").unwrap().map(|v| v.value),
        Some(Value::String("after_recovery".into()))
    );
}

/// Test KV list survives recovery
#[test]
fn test_kv_list_survives_recovery() {
    let (db, temp_dir, run_id) = setup();
    let path = get_path(&temp_dir);

    let kv = KVStore::new(db.clone());

    // Create multiple keys with prefix
    kv.put(&run_id, "config/a", Value::I64(1)).unwrap();
    kv.put(&run_id, "config/b", Value::I64(2)).unwrap();
    kv.put(&run_id, "config/c", Value::I64(3)).unwrap();
    kv.put(&run_id, "other/x", Value::I64(99)).unwrap();

    // Verify list before crash
    let config_keys = kv.list(&run_id, Some("config/")).unwrap();
    assert_eq!(config_keys.len(), 3);

    // Simulate crash
    drop(kv);
    drop(db);

    // Recovery
    let db = Arc::new(Database::open(&path).unwrap());
    let kv = KVStore::new(db.clone());

    // List still works
    let config_keys = kv.list(&run_id, Some("config/")).unwrap();
    assert_eq!(config_keys.len(), 3);
    assert!(config_keys.contains(&"config/a".to_string()));
    assert!(config_keys.contains(&"config/b".to_string()));
    assert!(config_keys.contains(&"config/c".to_string()));
}

/// Test EventLog chain survives recovery and sequences continue correctly
#[test]
fn test_event_log_chain_survives_recovery() {
    let (db, temp_dir, run_id) = setup();
    let path = get_path(&temp_dir);

    use strata_core::contract::Version;

    let event_log = EventLog::new(db.clone());

    // Append multiple events (sequences are 0-based)
    event_log
        .append(&run_id, "event1", Value::String("payload1".into()))
        .unwrap();
    event_log
        .append(&run_id, "event2", Value::String("payload2".into()))
        .unwrap();
    event_log
        .append(&run_id, "event3", Value::String("payload3".into()))
        .unwrap();

    // Read to get hashes before crash
    let pre_event0 = event_log.read(&run_id, 0).unwrap().unwrap();
    let pre_event1 = event_log.read(&run_id, 1).unwrap().unwrap();
    let pre_event2 = event_log.read(&run_id, 2).unwrap().unwrap();

    let hash0 = pre_event0.value.hash;
    let hash1 = pre_event1.value.hash;
    let hash2 = pre_event2.value.hash;

    // Verify chain before crash
    let verification = event_log.verify_chain(&run_id).unwrap();
    assert!(verification.is_valid);
    assert_eq!(event_log.len(&run_id).unwrap(), 3);

    // Simulate crash
    drop(event_log);
    drop(db);

    // Recovery
    let db = Arc::new(Database::open(&path).unwrap());
    let event_log = EventLog::new(db.clone());

    // Chain is intact
    let verification = event_log.verify_chain(&run_id).unwrap();
    assert!(verification.is_valid);
    assert_eq!(event_log.len(&run_id).unwrap(), 3);

    // Events readable with correct hashes
    let event0 = event_log.read(&run_id, 0).unwrap().unwrap();
    assert_eq!(event0.value.event_type, "event1");
    assert_eq!(event0.value.payload, Value::String("payload1".into()));
    assert_eq!(event0.value.hash, hash0);

    let event2 = event_log.read(&run_id, 2).unwrap().unwrap();
    assert_eq!(event2.value.hash, hash2);

    // Hash chaining preserved - event1 prev_hash points to event0's hash
    let event1 = event_log.read(&run_id, 1).unwrap().unwrap();
    assert_eq!(event1.value.prev_hash, hash0);
    assert_eq!(event1.value.hash, hash1);

    // Sequence continues correctly (not restarted)
    let v3 = event_log
        .append(&run_id, "event4", Value::String("payload4".into()))
        .unwrap();
    assert!(matches!(v3, Version::Sequence(3))); // Not 0 (would be restart)

    // Chain still valid after new append
    let verification = event_log.verify_chain(&run_id).unwrap();
    assert!(verification.is_valid);
    assert_eq!(event_log.len(&run_id).unwrap(), 4);
}

/// Test EventLog range queries survive recovery
#[test]
fn test_event_log_range_survives_recovery() {
    let (db, temp_dir, run_id) = setup();
    let path = get_path(&temp_dir);

    let event_log = EventLog::new(db.clone());

    // Append 5 events
    for i in 0..5 {
        event_log
            .append(&run_id, "numbered", Value::I64(i))
            .unwrap();
    }

    // Simulate crash
    drop(event_log);
    drop(db);

    // Recovery
    let db = Arc::new(Database::open(&path).unwrap());
    let event_log = EventLog::new(db.clone());

    // Range query works
    let events = event_log.read_range(&run_id, 1, 4).unwrap();
    assert_eq!(events.len(), 3);
    assert_eq!(events[0].value.sequence, 1);
    assert_eq!(events[1].value.sequence, 2);
    assert_eq!(events[2].value.sequence, 3);
}

/// Test StateCell version survives recovery
#[test]
fn test_state_cell_version_survives_recovery() {
    let (db, temp_dir, run_id) = setup();
    let path = get_path(&temp_dir);

    let state_cell = StateCell::new(db.clone());

    // Init creates version 1
    state_cell.init(&run_id, "counter", Value::I64(0)).unwrap();

    // CAS increments version
    state_cell
        .cas(&run_id, "counter", 1, Value::I64(10))
        .unwrap(); // -> v2
    state_cell
        .cas(&run_id, "counter", 2, Value::I64(20))
        .unwrap(); // -> v3
    state_cell
        .cas(&run_id, "counter", 3, Value::I64(30))
        .unwrap(); // -> v4

    // Verify before crash
    let state = state_cell.read(&run_id, "counter").unwrap().unwrap();
    assert_eq!(state.value.version, 4);
    assert_eq!(state.value.value, Value::I64(30));

    // Simulate crash
    drop(state_cell);
    drop(db);

    // Recovery
    let db = Arc::new(Database::open(&path).unwrap());
    let state_cell = StateCell::new(db.clone());

    // Version is correct (4, not 1)
    let state = state_cell.read(&run_id, "counter").unwrap().unwrap();
    assert_eq!(state.value.version, 4);
    assert_eq!(state.value.value, Value::I64(30));

    // CAS works with correct version
    let new_versioned = state_cell
        .cas(&run_id, "counter", 4, Value::I64(40))
        .unwrap();
    assert_eq!(new_versioned.value, 5);

    // CAS with old version fails
    let result = state_cell.cas(&run_id, "counter", 4, Value::I64(999));
    assert!(result.is_err());
}

/// Test StateCell set operation survives recovery
#[test]
fn test_state_cell_set_survives_recovery() {
    let (db, temp_dir, run_id) = setup();
    let path = get_path(&temp_dir);

    let state_cell = StateCell::new(db.clone());

    // Init and set
    state_cell
        .init(&run_id, "status", Value::String("initial".into()))
        .unwrap();
    state_cell
        .set(&run_id, "status", Value::String("updated".into()))
        .unwrap();

    // Simulate crash
    drop(state_cell);
    drop(db);

    // Recovery
    let db = Arc::new(Database::open(&path).unwrap());
    let state_cell = StateCell::new(db.clone());

    // Value preserved
    let state = state_cell.read(&run_id, "status").unwrap().unwrap();
    assert_eq!(state.value.value, Value::String("updated".into()));
    assert_eq!(state.value.version, 2); // init = 1, set = 2
}

/// Test TraceStore data survives recovery
#[test]
fn test_trace_store_survives_recovery() {
    let (db, temp_dir, run_id) = setup();
    let path = get_path(&temp_dir);

    let trace_store = TraceStore::new(db.clone());

    // Record traces
    let id1 = trace_store
        .record(
            &run_id,
            TraceType::Thought {
                content: "first thought".into(),
                confidence: None,
            },
            vec![],
            Value::Null,
        )
        .unwrap()
        .value; // Extract trace_id from Versioned
    let id2 = trace_store
        .record(
            &run_id,
            TraceType::ToolCall {
                tool_name: "test_tool".into(),
                arguments: Value::Null,
                result: None,
                duration_ms: None,
            },
            vec![],
            Value::Null,
        )
        .unwrap()
        .value; // Extract trace_id from Versioned
    let id3 = trace_store
        .record(
            &run_id,
            TraceType::Decision {
                question: "what to do?".into(),
                options: vec!["a".into(), "b".into()],
                chosen: "a".into(),
                reasoning: None,
            },
            vec![],
            Value::Null,
        )
        .unwrap()
        .value; // Extract trace_id from Versioned

    // Verify before crash
    assert_eq!(trace_store.count(&run_id).unwrap(), 3);

    // Simulate crash
    drop(trace_store);
    drop(db);

    // Recovery
    let db = Arc::new(Database::open(&path).unwrap());
    let trace_store = TraceStore::new(db.clone());

    // Primary data accessible
    assert_eq!(trace_store.count(&run_id).unwrap(), 3);

    let trace1 = trace_store.get(&run_id, &id1).unwrap().unwrap();
    assert!(matches!(trace1.value.trace_type, TraceType::Thought { .. }));

    let trace2 = trace_store.get(&run_id, &id2).unwrap().unwrap();
    assert!(matches!(trace2.value.trace_type, TraceType::ToolCall { .. }));

    let trace3 = trace_store.get(&run_id, &id3).unwrap().unwrap();
    assert!(matches!(trace3.value.trace_type, TraceType::Decision { .. }));
}

/// Test TraceStore type indices survive recovery
#[test]
fn test_trace_type_index_survives_recovery() {
    let (db, temp_dir, run_id) = setup();
    let path = get_path(&temp_dir);

    let trace_store = TraceStore::new(db.clone());

    // Record multiple traces of different types
    trace_store
        .record(
            &run_id,
            TraceType::Thought {
                content: "thought1".into(),
                confidence: None,
            },
            vec![],
            Value::Null,
        )
        .unwrap();
    trace_store
        .record(
            &run_id,
            TraceType::Thought {
                content: "thought2".into(),
                confidence: None,
            },
            vec![],
            Value::Null,
        )
        .unwrap();
    trace_store
        .record(
            &run_id,
            TraceType::ToolCall {
                tool_name: "tool1".into(),
                arguments: Value::Null,
                result: None,
                duration_ms: None,
            },
            vec![],
            Value::Null,
        )
        .unwrap();
    trace_store
        .record(
            &run_id,
            TraceType::Decision {
                question: "q".into(),
                options: vec!["a".into()],
                chosen: "a".into(),
                reasoning: None,
            },
            vec![],
            Value::Null,
        )
        .unwrap();

    // Verify type query before crash
    let thoughts = trace_store.query_by_type(&run_id, "Thought").unwrap();
    assert_eq!(thoughts.len(), 2);

    // Simulate crash
    drop(trace_store);
    drop(db);

    // Recovery
    let db = Arc::new(Database::open(&path).unwrap());
    let trace_store = TraceStore::new(db.clone());

    // Type index works after recovery
    let thoughts = trace_store.query_by_type(&run_id, "Thought").unwrap();
    assert_eq!(thoughts.len(), 2);

    let tool_calls = trace_store.query_by_type(&run_id, "ToolCall").unwrap();
    assert_eq!(tool_calls.len(), 1);

    let decisions = trace_store.query_by_type(&run_id, "Decision").unwrap();
    assert_eq!(decisions.len(), 1);
}

/// Test TraceStore parent-child relationships survive recovery
#[test]
fn test_trace_parent_child_survives_recovery() {
    let (db, temp_dir, run_id) = setup();
    let path = get_path(&temp_dir);

    let trace_store = TraceStore::new(db.clone());

    // Create parent trace
    let parent_id = trace_store
        .record(
            &run_id,
            TraceType::ToolCall {
                tool_name: "parent_tool".into(),
                arguments: Value::Null,
                result: None,
                duration_ms: None,
            },
            vec![],
            Value::Null,
        )
        .unwrap()
        .value; // Extract trace_id from Versioned

    // Create child traces
    let child1_id = trace_store
        .record_child(
            &run_id,
            &parent_id,
            TraceType::Thought {
                content: "child thought 1".into(),
                confidence: None,
            },
            vec![],
            Value::Null,
        )
        .unwrap()
        .value; // Extract trace_id from Versioned
    let child2_id = trace_store
        .record_child(
            &run_id,
            &parent_id,
            TraceType::Thought {
                content: "child thought 2".into(),
                confidence: None,
            },
            vec![],
            Value::Null,
        )
        .unwrap()
        .value; // Extract trace_id from Versioned

    // Verify children before crash
    let children = trace_store.get_children(&run_id, &parent_id).unwrap();
    assert_eq!(children.len(), 2);

    // Simulate crash
    drop(trace_store);
    drop(db);

    // Recovery
    let db = Arc::new(Database::open(&path).unwrap());
    let trace_store = TraceStore::new(db.clone());

    // Parent-child index works after recovery
    let children = trace_store.get_children(&run_id, &parent_id).unwrap();
    assert_eq!(children.len(), 2);

    // Children have correct parent_id
    let child1 = trace_store.get(&run_id, &child1_id).unwrap().unwrap();
    assert_eq!(child1.value.parent_id, Some(parent_id.clone()));

    let child2 = trace_store.get(&run_id, &child2_id).unwrap().unwrap();
    assert_eq!(child2.value.parent_id, Some(parent_id.clone()));
}

/// Test RunIndex status survives recovery
#[test]
fn test_run_index_status_survives_recovery() {
    let temp_dir = TempDir::new().unwrap();
    let path = get_path(&temp_dir);
    let db = Arc::new(Database::open(&path).unwrap());

    let run_index = RunIndex::new(db.clone());

    // Create run with metadata
    let run_meta = run_index.create_run("test-run").unwrap();
    let run_name = run_meta.value.name.clone();

    // Update status (use Paused instead of default Active)
    run_index
        .update_status(&run_name, RunStatus::Paused)
        .unwrap();

    // Add tags
    run_index
        .add_tags(
            &run_name,
            vec!["important".to_string(), "batch-1".to_string()],
        )
        .unwrap();

    // Verify before crash
    let run = run_index.get_run(&run_name).unwrap().unwrap();
    assert_eq!(run.value.status, RunStatus::Paused);
    assert!(run.value.tags.contains(&"important".to_string()));
    assert!(run.value.tags.contains(&"batch-1".to_string()));

    // Simulate crash
    drop(run_index);
    drop(db);

    // Recovery
    let db = Arc::new(Database::open(&path).unwrap());
    let run_index = RunIndex::new(db.clone());

    // Status preserved
    let recovered = run_index.get_run(&run_name).unwrap().unwrap();
    assert_eq!(recovered.value.status, RunStatus::Paused);
    assert!(recovered.value.tags.contains(&"important".to_string()));
    assert!(recovered.value.tags.contains(&"batch-1".to_string()));
    assert_eq!(recovered.value.name, "test-run");
}

/// Test RunIndex query survives recovery
#[test]
fn test_run_index_query_survives_recovery() {
    let temp_dir = TempDir::new().unwrap();
    let path = get_path(&temp_dir);
    let db = Arc::new(Database::open(&path).unwrap());

    let run_index = RunIndex::new(db.clone());

    // Create multiple runs with different statuses
    let run1 = run_index.create_run("run1").unwrap();
    let run2 = run_index.create_run("run2").unwrap();
    let _run3 = run_index.create_run("run3").unwrap();

    run_index
        .update_status(&run1.value.name, RunStatus::Completed)
        .unwrap();
    run_index
        .update_status(&run2.value.name, RunStatus::Failed)
        .unwrap();
    // run3 stays Active (default)

    // Simulate crash
    drop(run_index);
    drop(db);

    // Recovery
    let db = Arc::new(Database::open(&path).unwrap());
    let run_index = RunIndex::new(db.clone());

    // Query by status works
    let completed = run_index.query_by_status(RunStatus::Completed).unwrap();
    assert!(completed.iter().any(|r| r.name == "run1"));

    let failed = run_index.query_by_status(RunStatus::Failed).unwrap();
    assert!(failed.iter().any(|r| r.name == "run2"));

    let active = run_index.query_by_status(RunStatus::Active).unwrap();
    assert!(active.iter().any(|r| r.name == "run3"));
}

/// Test RunIndex cascading delete survives recovery
#[test]
fn test_run_delete_survives_recovery() {
    let temp_dir = TempDir::new().unwrap();
    let path = get_path(&temp_dir);
    let db = Arc::new(Database::open(&path).unwrap());

    let run_index = RunIndex::new(db.clone());
    let kv = KVStore::new(db.clone());

    // Create two runs
    let meta1 = run_index.create_run("run1").unwrap();
    let meta2 = run_index.create_run("run2").unwrap();
    let run1 = RunId::from_string(&meta1.value.run_id).unwrap();
    let run2 = RunId::from_string(&meta2.value.run_id).unwrap();

    // Write data to both
    kv.put(&run1, "key", Value::I64(1)).unwrap();
    kv.put(&run2, "key", Value::I64(2)).unwrap();

    // Delete run1
    run_index.delete_run("run1").unwrap();

    // Simulate crash
    drop(run_index);
    drop(kv);
    drop(db);

    // Recovery
    let db = Arc::new(Database::open(&path).unwrap());
    let run_index = RunIndex::new(db.clone());
    let kv = KVStore::new(db.clone());

    // run1 is still deleted
    assert!(run_index.get_run("run1").unwrap().is_none());
    assert!(kv.get(&run1, "key").unwrap().is_none());

    // run2 data preserved
    assert!(run_index.get_run("run2").unwrap().is_some());
    assert_eq!(kv.get(&run2, "key").unwrap().map(|v| v.value), Some(Value::I64(2)));
}

/// Test cross-primitive transaction survives recovery
#[test]
fn test_cross_primitive_transaction_survives_recovery() {
    use strata_primitives::{EventLogExt, KVStoreExt, StateCellExt, TraceStoreExt};

    let (db, temp_dir, run_id) = setup();
    let path = get_path(&temp_dir);

    // Initialize state cell
    let state_cell = StateCell::new(db.clone());
    state_cell
        .init(&run_id, "txn_state", Value::I64(0))
        .unwrap();

    // Perform atomic transaction
    let result = db.transaction(run_id, |txn| {
        txn.kv_put("txn_key", Value::String("txn_value".into()))?;
        txn.event_append("txn_event", Value::I64(100))?;
        txn.state_set("txn_state", Value::I64(42))?;
        txn.trace_record("Thought", Value::String("txn thought".into()))?;
        Ok(())
    });
    assert!(result.is_ok());

    // Simulate crash
    drop(state_cell);
    drop(db);

    // Recovery
    let db = Arc::new(Database::open(&path).unwrap());
    let kv = KVStore::new(db.clone());
    let event_log = EventLog::new(db.clone());
    let state_cell = StateCell::new(db.clone());
    let trace_store = TraceStore::new(db.clone());

    // All operations survived
    assert_eq!(
        kv.get(&run_id, "txn_key").unwrap().map(|v| v.value),
        Some(Value::String("txn_value".into()))
    );
    assert_eq!(event_log.len(&run_id).unwrap(), 1);
    let state = state_cell.read(&run_id, "txn_state").unwrap().unwrap();
    assert_eq!(state.value.value, Value::I64(42));
    assert_eq!(trace_store.count(&run_id).unwrap(), 1);
}

/// Test multiple sequential recoveries
#[test]
fn test_multiple_recovery_cycles() {
    let temp_dir = TempDir::new().unwrap();
    let path = get_path(&temp_dir);
    let run_id = RunId::new();

    // Cycle 1: Create and populate
    {
        let db = Arc::new(Database::open(&path).unwrap());
        let kv = KVStore::new(db.clone());
        kv.put(&run_id, "cycle1", Value::I64(1)).unwrap();
    }

    // Cycle 2: Add more data
    {
        let db = Arc::new(Database::open(&path).unwrap());
        let kv = KVStore::new(db.clone());

        // Verify cycle 1 data
        assert_eq!(kv.get(&run_id, "cycle1").unwrap().map(|v| v.value), Some(Value::I64(1)));

        // Add cycle 2 data
        kv.put(&run_id, "cycle2", Value::I64(2)).unwrap();
    }

    // Cycle 3: Add more data
    {
        let db = Arc::new(Database::open(&path).unwrap());
        let kv = KVStore::new(db.clone());

        // Verify all previous data
        assert_eq!(kv.get(&run_id, "cycle1").unwrap().map(|v| v.value), Some(Value::I64(1)));
        assert_eq!(kv.get(&run_id, "cycle2").unwrap().map(|v| v.value), Some(Value::I64(2)));

        // Add cycle 3 data
        kv.put(&run_id, "cycle3", Value::I64(3)).unwrap();
    }

    // Final verification
    {
        let db = Arc::new(Database::open(&path).unwrap());
        let kv = KVStore::new(db.clone());

        assert_eq!(kv.get(&run_id, "cycle1").unwrap().map(|v| v.value), Some(Value::I64(1)));
        assert_eq!(kv.get(&run_id, "cycle2").unwrap().map(|v| v.value), Some(Value::I64(2)));
        assert_eq!(kv.get(&run_id, "cycle3").unwrap().map(|v| v.value), Some(Value::I64(3)));
    }
}

/// Test that all primitives recover together correctly
#[test]
fn test_all_primitives_recover_together() {
    let temp_dir = TempDir::new().unwrap();
    let path = get_path(&temp_dir);

    // Phase 1: Create data for all primitives
    let run_id: RunId;
    {
        let db = Arc::new(Database::open(&path).unwrap());
        let run_index = RunIndex::new(db.clone());
        let kv = KVStore::new(db.clone());
        let event_log = EventLog::new(db.clone());
        let state_cell = StateCell::new(db.clone());
        let trace_store = TraceStore::new(db.clone());

        // Create run
        let run_meta = run_index.create_run("full-test").unwrap();
        run_id = RunId::from_string(&run_meta.value.run_id).unwrap();

        // Populate all primitives
        kv.put(&run_id, "full_key", Value::String("full_value".into()))
            .unwrap();

        event_log
            .append(&run_id, "full_event", Value::I64(999))
            .unwrap();

        state_cell
            .init(&run_id, "full_state", Value::I64(0))
            .unwrap();
        state_cell
            .cas(&run_id, "full_state", 1, Value::I64(100))
            .unwrap();

        trace_store
            .record(
                &run_id,
                TraceType::Thought {
                    content: "full thought".into(),
                    confidence: None,
                },
                vec![],
                Value::Null,
            )
            .unwrap();
    }

    // Phase 2: Verify all recovered
    {
        let db = Arc::new(Database::open(&path).unwrap());
        let run_index = RunIndex::new(db.clone());
        let kv = KVStore::new(db.clone());
        let event_log = EventLog::new(db.clone());
        let state_cell = StateCell::new(db.clone());
        let trace_store = TraceStore::new(db.clone());

        // RunIndex
        let run = run_index.get_run("full-test").unwrap().unwrap();
        assert_eq!(run.value.name, "full-test");

        // KV
        assert_eq!(
            kv.get(&run_id, "full_key").unwrap().map(|v| v.value),
            Some(Value::String("full_value".into()))
        );

        // EventLog
        assert_eq!(event_log.len(&run_id).unwrap(), 1);
        let event = event_log.read(&run_id, 0).unwrap().unwrap();
        assert_eq!(event.value.payload, Value::I64(999));

        // StateCell
        let state = state_cell.read(&run_id, "full_state").unwrap().unwrap();
        assert_eq!(state.value.value, Value::I64(100));
        assert_eq!(state.value.version, 2);

        // TraceStore
        assert_eq!(trace_store.count(&run_id).unwrap(), 1);
    }
}
