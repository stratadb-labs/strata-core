//! Cross-Primitive Transaction Tests (Story #197)
//!
//! Tests verifying that multiple primitives can participate in atomic transactions.

use in_mem_core::types::RunId;
use in_mem_core::value::Value;
use in_mem_engine::Database;
use in_mem_primitives::{
    EventLog, EventLogExt, KVStore, KVStoreExt, StateCell, StateCellExt, TraceStore, TraceStoreExt,
};
use std::sync::Arc;
use tempfile::TempDir;

fn setup() -> (Arc<Database>, TempDir, RunId) {
    let temp_dir = TempDir::new().unwrap();
    let db = Arc::new(Database::open(temp_dir.path()).unwrap());
    let run_id = RunId::new();
    (db, temp_dir, run_id)
}

/// Test that KV, Event, State, and Trace operations work atomically in a single transaction
#[test]
fn test_kv_event_state_trace_atomic() {
    let (db, _temp, run_id) = setup();

    // Initialize state cell first (needed for CAS)
    let state_cell = StateCell::new(db.clone());
    state_cell.init(&run_id, "workflow", Value::I64(0)).unwrap();

    // Perform atomic transaction with all 4 primitives
    let result = db.transaction(run_id, |txn| {
        // KV operation
        txn.kv_put("task/status", Value::String("running".into()))?;

        // Event operation (sequences start at 0)
        let seq = txn.event_append("task_started", Value::String("payload".into()))?;
        assert_eq!(seq, 0);

        // State operation (CAS from version 1 after init)
        let new_version = txn.state_cas("workflow", 1, Value::String("step1".into()))?;
        assert_eq!(new_version, 2);

        // Trace operation
        let trace_id =
            txn.trace_record("Thought", Value::String("Starting task processing".into()))?;
        assert!(!trace_id.is_empty());

        Ok(())
    });

    assert!(result.is_ok());

    // Verify all operations succeeded
    let kv = KVStore::new(db.clone());
    let event_log = EventLog::new(db.clone());
    let trace_store = TraceStore::new(db.clone());

    assert_eq!(
        kv.get(&run_id, "task/status").unwrap(),
        Some(Value::String("running".into()))
    );
    assert_eq!(event_log.len(&run_id).unwrap(), 1);

    let state = state_cell.read(&run_id, "workflow").unwrap().unwrap();
    assert_eq!(state.value, Value::String("step1".into()));
    assert_eq!(state.version, 2);

    assert_eq!(trace_store.count(&run_id).unwrap(), 1);
}

/// Test that a failed operation causes full rollback of all primitives
#[test]
fn test_cross_primitive_rollback() {
    let (db, _temp, run_id) = setup();

    // Initialize state cell with version 1
    let state_cell = StateCell::new(db.clone());
    state_cell.init(&run_id, "cell", Value::I64(100)).unwrap();

    // Attempt transaction with wrong CAS version - should fail and rollback
    let result = db.transaction(run_id, |txn| {
        // KV put (should succeed alone)
        txn.kv_put("key_to_rollback", Value::I64(42))?;

        // Event append (should succeed alone)
        txn.event_append("event_to_rollback", Value::Null)?;

        // StateCell CAS with WRONG version (should fail)
        // State is at version 1, but we try version 999
        txn.state_cas("cell", 999, Value::I64(200))?;

        Ok(())
    });

    // Transaction should have failed
    assert!(result.is_err());

    // Verify KV was NOT written (rollback affected all)
    let kv = KVStore::new(db.clone());
    assert!(kv.get(&run_id, "key_to_rollback").unwrap().is_none());

    // Verify Event was NOT written
    let event_log = EventLog::new(db.clone());
    assert_eq!(event_log.len(&run_id).unwrap(), 0);

    // Verify StateCell unchanged
    let state = state_cell.read(&run_id, "cell").unwrap().unwrap();
    assert_eq!(state.value, Value::I64(100));
    assert_eq!(state.version, 1);
}

/// Test that all 4 extension traits compose correctly in single transaction
#[test]
fn test_all_extension_traits_compose() {
    let (db, _temp, run_id) = setup();

    // Pre-initialize state cell
    let state_cell = StateCell::new(db.clone());
    state_cell.init(&run_id, "counter", Value::I64(0)).unwrap();

    // Use all 4 extension traits in single transaction
    let result = db.transaction(run_id, |txn| {
        // KVStoreExt::kv_put()
        txn.kv_put("config", Value::String("enabled".into()))?;

        // EventLogExt::event_append() - sequences start at 0
        let seq = txn.event_append("config_changed", Value::Null)?;
        assert_eq!(seq, 0);

        // StateCellExt::state_set() (unconditional) - version 2 after init
        let version = txn.state_set("counter", Value::I64(1))?;
        assert_eq!(version, 2);

        // TraceStoreExt::trace_record()
        let trace_id = txn.trace_record("Decision", Value::String("config update".into()))?;
        assert!(!trace_id.is_empty());

        Ok(())
    });

    assert!(result.is_ok());

    // Verify all succeeded
    let kv = KVStore::new(db.clone());
    let event_log = EventLog::new(db.clone());
    let trace_store = TraceStore::new(db.clone());

    assert!(kv.get(&run_id, "config").unwrap().is_some());
    assert_eq!(event_log.len(&run_id).unwrap(), 1);
    assert!(state_cell.read(&run_id, "counter").unwrap().is_some());
    assert_eq!(trace_store.count(&run_id).unwrap(), 1);
}

/// Test that partial failure in any primitive causes full rollback
#[test]
fn test_partial_failure_full_rollback() {
    let (db, _temp, run_id) = setup();

    // Initialize state cell
    let state_cell = StateCell::new(db.clone());
    state_cell.init(&run_id, "state", Value::I64(0)).unwrap();

    // Write successfully to 3 primitives, then fail on 4th
    let result = db.transaction(run_id, |txn| {
        // 1. KV - success
        txn.kv_put("partial_key", Value::I64(1))?;

        // 2. Event - success
        txn.event_append("partial_event", Value::Null)?;

        // 3. Trace - success
        txn.trace_record("Thought", Value::String("partial".into()))?;

        // 4. State CAS with wrong version - FAILURE
        txn.state_cas("state", 999, Value::I64(100))?;

        Ok(())
    });

    // Should fail
    assert!(result.is_err());

    // Verify ALL 4 operations rolled back
    let kv = KVStore::new(db.clone());
    let event_log = EventLog::new(db.clone());
    let trace_store = TraceStore::new(db.clone());

    assert!(kv.get(&run_id, "partial_key").unwrap().is_none());
    assert_eq!(event_log.len(&run_id).unwrap(), 0);
    assert_eq!(trace_store.count(&run_id).unwrap(), 0);

    let state = state_cell.read(&run_id, "state").unwrap().unwrap();
    assert_eq!(state.version, 1); // Unchanged
}

/// Test nested/chained primitive operations within single transaction
#[test]
fn test_nested_primitive_operations() {
    let (db, _temp, run_id) = setup();

    // Pre-populate some KV data
    let kv = KVStore::new(db.clone());
    kv.put(&run_id, "initial_value", Value::I64(42)).unwrap();

    // Initialize state
    let state_cell = StateCell::new(db.clone());
    state_cell
        .init(&run_id, "sequence_tracker", Value::I64(0))
        .unwrap();

    // Chain operations: read KV -> use in Event -> update State -> record Trace
    let result = db.transaction(run_id, |txn| {
        // Read KV -> use value in Event payload
        let kv_value = txn.kv_get("initial_value")?;
        let payload = kv_value.unwrap_or(Value::Null);

        // Append Event with payload from KV (sequence starts at 0)
        let seq = txn.event_append("chained_event", payload)?;

        // Update State with sequence number
        let _version = txn.state_set("sequence_tracker", Value::I64(seq as i64))?;

        // Record trace documenting the chain
        txn.trace_record(
            "ToolCall",
            Value::String(format!("Processed sequence {}", seq)),
        )?;

        Ok(seq)
    });

    assert!(result.is_ok());
    let seq = result.unwrap();
    assert_eq!(seq, 0); // Sequences start at 0

    // Verify causal chain worked
    let event_log = EventLog::new(db.clone());
    let event = event_log.read(&run_id, 0).unwrap().unwrap();
    assert_eq!(event.payload, Value::I64(42)); // From KV

    let state = state_cell
        .read(&run_id, "sequence_tracker")
        .unwrap()
        .unwrap();
    assert_eq!(state.value, Value::I64(0)); // Sequence number (starts at 0)

    let trace_store = TraceStore::new(db.clone());
    assert_eq!(trace_store.count(&run_id).unwrap(), 1);
}

/// Test multiple sequential transactions with all primitives
#[test]
fn test_multiple_transactions_consistency() {
    let (db, _temp, run_id) = setup();

    // Initialize state
    let state_cell = StateCell::new(db.clone());
    state_cell.init(&run_id, "counter", Value::I64(0)).unwrap();

    // Run 10 sequential transactions
    for i in 1..=10 {
        let result = db.transaction(run_id, |txn| {
            txn.kv_put(&format!("key_{}", i), Value::I64(i))?;
            txn.event_append("iteration", Value::I64(i))?;
            txn.state_set("counter", Value::I64(i))?;
            txn.trace_record("Thought", Value::String(format!("Iteration {}", i)))?;
            Ok(())
        });
        assert!(result.is_ok());
    }

    // Verify final state
    let kv = KVStore::new(db.clone());
    let event_log = EventLog::new(db.clone());
    let trace_store = TraceStore::new(db.clone());

    // All 10 KV entries exist
    for i in 1..=10 {
        assert_eq!(
            kv.get(&run_id, &format!("key_{}", i)).unwrap(),
            Some(Value::I64(i))
        );
    }

    // 10 events
    assert_eq!(event_log.len(&run_id).unwrap(), 10);

    // Counter at 10
    let state = state_cell.read(&run_id, "counter").unwrap().unwrap();
    assert_eq!(state.value, Value::I64(10));

    // 10 traces
    assert_eq!(trace_store.count(&run_id).unwrap(), 10);
}

/// Test read operations within transaction see uncommitted writes
#[test]
fn test_read_your_writes_in_transaction() {
    let (db, _temp, run_id) = setup();

    let result = db.transaction(run_id, |txn| {
        // Write KV
        txn.kv_put("test_key", Value::String("test_value".into()))?;

        // Read back within same transaction (read-your-writes)
        let value = txn.kv_get("test_key")?;
        assert_eq!(value, Some(Value::String("test_value".into())));

        // Append event (sequences start at 0)
        let seq1 = txn.event_append("event1", Value::Null)?;
        assert_eq!(seq1, 0);

        // Append another - sequence should continue
        let seq2 = txn.event_append("event2", Value::Null)?;
        assert_eq!(seq2, 1);

        Ok(())
    });

    assert!(result.is_ok());
}

/// Test transaction with only reads doesn't modify anything
#[test]
fn test_read_only_transaction() {
    let (db, _temp, run_id) = setup();

    // Pre-populate data
    let kv = KVStore::new(db.clone());
    kv.put(&run_id, "existing", Value::I64(100)).unwrap();

    let state_cell = StateCell::new(db.clone());
    state_cell.init(&run_id, "cell", Value::I64(50)).unwrap();

    // Read-only transaction
    let result = db.transaction(run_id, |txn| {
        let kv_val = txn.kv_get("existing")?;
        assert_eq!(kv_val, Some(Value::I64(100)));

        let state_val = txn.state_read("cell")?;
        assert!(state_val.is_some());

        Ok(())
    });

    assert!(result.is_ok());

    // Data unchanged
    assert_eq!(kv.get(&run_id, "existing").unwrap(), Some(Value::I64(100)));
    let state = state_cell.read(&run_id, "cell").unwrap().unwrap();
    assert_eq!(state.version, 1);
}
