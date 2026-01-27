//! Primitive Recovery Tests
//!
//! Tests verifying that ALL primitives survive crash + WAL replay.
//! The recovery contract ensures:
//! - Sequence numbers: Preserved
//! - Secondary indices: Replayed, not rebuilt
//! - Derived keys (hashes): Stored, not recomputed

use strata_core::contract::Version;
use strata_core::types::RunId;
use strata_core::value::Value;
use strata_engine::Database;
use strata_engine::{EventLog, KVStore, RunIndex, RunStatus, StateCell};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tempfile::TempDir;

/// Helper to create an object payload with a string value
fn string_payload(s: &str) -> Value {
    Value::Object(HashMap::from([("data".to_string(), Value::String(s.into()))]))
}

/// Helper to create an object payload with an integer value
fn int_payload(v: i64) -> Value {
    Value::Object(HashMap::from([("value".to_string(), Value::Int(v))]))
}

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
    kv.put(&run_id, "key2", Value::Int(42)).unwrap();
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
    assert_eq!(kv.get(&run_id, "key2").unwrap().map(|v| v.value), Some(Value::Int(42)));
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
    kv.put(&run_id, "config/a", Value::Int(1)).unwrap();
    kv.put(&run_id, "config/b", Value::Int(2)).unwrap();
    kv.put(&run_id, "config/c", Value::Int(3)).unwrap();
    kv.put(&run_id, "other/x", Value::Int(99)).unwrap();

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
        .append(&run_id, "event1", string_payload("payload1"))
        .unwrap();
    event_log
        .append(&run_id, "event2", string_payload("payload2"))
        .unwrap();
    event_log
        .append(&run_id, "event3", string_payload("payload3"))
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
    assert_eq!(event0.value.payload, string_payload("payload1"));
    assert_eq!(event0.value.hash, hash0);

    let event2 = event_log.read(&run_id, 2).unwrap().unwrap();
    assert_eq!(event2.value.hash, hash2);

    // Hash chaining preserved - event1 prev_hash points to event0's hash
    let event1 = event_log.read(&run_id, 1).unwrap().unwrap();
    assert_eq!(event1.value.prev_hash, hash0);
    assert_eq!(event1.value.hash, hash1);

    // Sequence continues correctly (not restarted)
    let v3 = event_log
        .append(&run_id, "event4", string_payload("payload4"))
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
            .append(&run_id, "numbered", int_payload(i))
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
    state_cell.init(&run_id, "counter", Value::Int(0)).unwrap();

    // CAS increments version
    state_cell
        .cas(&run_id, "counter", Version::counter(1), Value::Int(10))
        .unwrap(); // -> v2
    state_cell
        .cas(&run_id, "counter", Version::counter(2), Value::Int(20))
        .unwrap(); // -> v3
    state_cell
        .cas(&run_id, "counter", Version::counter(3), Value::Int(30))
        .unwrap(); // -> v4

    // Verify before crash
    let state = state_cell.read(&run_id, "counter").unwrap().unwrap();
    assert_eq!(state.value.version, Version::counter(4));
    assert_eq!(state.value.value, Value::Int(30));

    // Simulate crash
    drop(state_cell);
    drop(db);

    // Recovery
    let db = Arc::new(Database::open(&path).unwrap());
    let state_cell = StateCell::new(db.clone());

    // Version is correct (4, not 1)
    let state = state_cell.read(&run_id, "counter").unwrap().unwrap();
    assert_eq!(state.value.version, Version::counter(4));
    assert_eq!(state.value.value, Value::Int(30));

    // CAS works with correct version
    let new_versioned = state_cell
        .cas(&run_id, "counter", Version::counter(4), Value::Int(40))
        .unwrap();
    assert_eq!(new_versioned.value, Version::counter(5));

    // CAS with old version fails
    let result = state_cell.cas(&run_id, "counter", Version::counter(4), Value::Int(999));
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
    assert_eq!(state.value.version, Version::counter(2)); // init = 1, set = 2
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
    kv.put(&run1, "key", Value::Int(1)).unwrap();
    kv.put(&run2, "key", Value::Int(2)).unwrap();

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
    assert_eq!(kv.get(&run2, "key").unwrap().map(|v| v.value), Some(Value::Int(2)));
}

/// Test cross-primitive transaction survives recovery
#[test]
fn test_cross_primitive_transaction_survives_recovery() {
    use strata_engine::{EventLogExt, KVStoreExt, StateCellExt};

    let (db, temp_dir, run_id) = setup();
    let path = get_path(&temp_dir);

    // Initialize state cell
    let state_cell = StateCell::new(db.clone());
    state_cell
        .init(&run_id, "txn_state", Value::Int(0))
        .unwrap();

    // Perform atomic transaction
    let result = db.transaction(run_id, |txn| {
        txn.kv_put("txn_key", Value::String("txn_value".into()))?;
        txn.event_append("txn_event", int_payload(100))?;
        txn.state_set("txn_state", Value::Int(42))?;
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

    // All operations survived
    assert_eq!(
        kv.get(&run_id, "txn_key").unwrap().map(|v| v.value),
        Some(Value::String("txn_value".into()))
    );
    assert_eq!(event_log.len(&run_id).unwrap(), 1);
    let state = state_cell.read(&run_id, "txn_state").unwrap().unwrap();
    assert_eq!(state.value.value, Value::Int(42));
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
        kv.put(&run_id, "cycle1", Value::Int(1)).unwrap();
    }

    // Cycle 2: Add more data
    {
        let db = Arc::new(Database::open(&path).unwrap());
        let kv = KVStore::new(db.clone());

        // Verify cycle 1 data
        assert_eq!(kv.get(&run_id, "cycle1").unwrap().map(|v| v.value), Some(Value::Int(1)));

        // Add cycle 2 data
        kv.put(&run_id, "cycle2", Value::Int(2)).unwrap();
    }

    // Cycle 3: Add more data
    {
        let db = Arc::new(Database::open(&path).unwrap());
        let kv = KVStore::new(db.clone());

        // Verify all previous data
        assert_eq!(kv.get(&run_id, "cycle1").unwrap().map(|v| v.value), Some(Value::Int(1)));
        assert_eq!(kv.get(&run_id, "cycle2").unwrap().map(|v| v.value), Some(Value::Int(2)));

        // Add cycle 3 data
        kv.put(&run_id, "cycle3", Value::Int(3)).unwrap();
    }

    // Final verification
    {
        let db = Arc::new(Database::open(&path).unwrap());
        let kv = KVStore::new(db.clone());

        assert_eq!(kv.get(&run_id, "cycle1").unwrap().map(|v| v.value), Some(Value::Int(1)));
        assert_eq!(kv.get(&run_id, "cycle2").unwrap().map(|v| v.value), Some(Value::Int(2)));
        assert_eq!(kv.get(&run_id, "cycle3").unwrap().map(|v| v.value), Some(Value::Int(3)));
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

        // Create run
        let run_meta = run_index.create_run("full-test").unwrap();
        run_id = RunId::from_string(&run_meta.value.run_id).unwrap();

        // Populate all primitives
        kv.put(&run_id, "full_key", Value::String("full_value".into()))
            .unwrap();

        event_log
            .append(&run_id, "full_event", int_payload(999))
            .unwrap();

        state_cell
            .init(&run_id, "full_state", Value::Int(0))
            .unwrap();
        state_cell
            .cas(&run_id, "full_state", Version::counter(1), Value::Int(100))
            .unwrap();
    }

    // Phase 2: Verify all recovered
    {
        let db = Arc::new(Database::open(&path).unwrap());
        let run_index = RunIndex::new(db.clone());
        let kv = KVStore::new(db.clone());
        let event_log = EventLog::new(db.clone());
        let state_cell = StateCell::new(db.clone());

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
        assert_eq!(event.value.payload, int_payload(999));

        // StateCell
        let state = state_cell.read(&run_id, "full_state").unwrap().unwrap();
        assert_eq!(state.value.value, Value::Int(100));
        assert_eq!(state.value.version, Version::counter(2));
    }
}
