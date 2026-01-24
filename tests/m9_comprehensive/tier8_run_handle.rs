//! RunHandle Pattern Tests (Story #478)
//!
//! This module verifies the RunHandle pattern implementation:
//!
//! - RunHandle provides scoped access to primitives
//! - Each primitive handle (kv(), events(), state(), json(), vectors()) is accessible
//! - transaction() provides atomic cross-primitive operations
//! - Thread safety: Clone, Send, Sync
//! - Run isolation: operations scoped to the bound run
//!
//! # Story #478: RunHandle Pattern Implementation

use strata_core::types::RunId;
use strata_primitives::run_handle::RunHandle;
use strata_engine::Database;
use std::sync::Arc;

/// Create a test database and RunHandle
fn setup() -> (Arc<Database>, RunHandle) {
    let db = Arc::new(Database::builder().in_memory().open_temp().unwrap());
    let run_id = RunId::new();
    let handle = RunHandle::new(db.clone(), run_id);
    (db, handle)
}

// ============================================================================
// RunHandle Construction
// ============================================================================

#[test]
fn run_handle_construction() {
    let db = Arc::new(Database::builder().in_memory().open_temp().unwrap());
    let run_id = RunId::new();

    let handle = RunHandle::new(db.clone(), run_id);

    assert_eq!(handle.run_id(), &run_id);
}

#[test]
fn run_handle_database_accessible() {
    let (db, handle) = setup();

    // Can access underlying database
    let db_ref = handle.database();
    assert!(Arc::ptr_eq(&db, db_ref));
}

#[test]
fn run_handle_is_clone() {
    let (_, handle) = setup();

    let handle2 = handle.clone();

    // Both handles point to same run
    assert_eq!(handle.run_id(), handle2.run_id());
}

// ============================================================================
// Primitive Handles
// ============================================================================

#[test]
fn run_handle_provides_kv_handle() {
    let (_, handle) = setup();

    // Can access KV handle
    let _kv = handle.kv();
}

#[test]
fn run_handle_provides_events_handle() {
    let (_, handle) = setup();

    // Can access Events handle
    let _events = handle.events();
}

#[test]
fn run_handle_provides_state_handle() {
    let (_, handle) = setup();

    // Can access State handle
    let _state = handle.state();
}

#[test]
fn run_handle_provides_json_handle() {
    let (_, handle) = setup();

    // Can access JSON handle
    let _json = handle.json();
}

#[test]
fn run_handle_provides_vectors_handle() {
    let (_, handle) = setup();

    // Can access Vectors handle
    let _vectors = handle.vectors();
}

// ============================================================================
// KvHandle Operations
// ============================================================================

#[test]
fn kv_handle_get_nonexistent_returns_none() {
    let (_, handle) = setup();
    let kv = handle.kv();

    let result = kv.get("nonexistent").unwrap();
    assert!(result.is_none());
}

#[test]
fn kv_handle_put_and_get() {
    let (_, handle) = setup();
    let kv = handle.kv();

    kv.put("key", strata_core::value::Value::String("value".into())).unwrap();
    let result = kv.get("key").unwrap();

    assert!(result.is_some());
}

#[test]
fn kv_handle_exists() {
    let (_, handle) = setup();
    let kv = handle.kv();

    assert!(!kv.exists("key").unwrap());
    kv.put("key", strata_core::value::Value::Int(42)).unwrap();
    assert!(kv.exists("key").unwrap());
}

#[test]
fn kv_handle_delete() {
    let (_, handle) = setup();
    let kv = handle.kv();

    kv.put("key", strata_core::value::Value::Int(42)).unwrap();
    assert!(kv.exists("key").unwrap());

    kv.delete("key").unwrap();
    assert!(!kv.exists("key").unwrap());
}

// ============================================================================
// EventHandle Operations
// ============================================================================

#[test]
fn event_handle_append_returns_sequence() {
    let (_, handle) = setup();
    let events = handle.events();

    let seq1 = events.append("test-event", strata_core::value::Value::Null).unwrap();
    let seq2 = events.append("test-event", strata_core::value::Value::Null).unwrap();

    // Sequences should be monotonically increasing
    assert!(seq2 > seq1);
}

#[test]
fn event_handle_read_by_sequence() {
    let (_, handle) = setup();
    let events = handle.events();

    let seq = events.append("my-event", strata_core::value::Value::Int(42)).unwrap();
    let result = events.read(seq).unwrap();

    assert!(result.is_some());
}

// ============================================================================
// StateHandle Operations
// ============================================================================

#[test]
fn state_handle_read_nonexistent_returns_none() {
    let (_, handle) = setup();
    let state = handle.state();

    let result = state.read("nonexistent").unwrap();
    assert!(result.is_none());
}

#[test]
fn state_handle_set_and_read() {
    let (_, handle) = setup();
    let state = handle.state();

    state.set("counter", strata_core::value::Value::Int(0)).unwrap();
    let result = state.read("counter").unwrap();

    assert!(result.is_some());
}

// ============================================================================
// Transaction Support
// ============================================================================

#[test]
fn run_handle_transaction_basic() {
    let (_, handle) = setup();

    // Transaction should complete successfully
    let result = handle.transaction(|_txn| {
        Ok(42)
    });

    assert_eq!(result.unwrap(), 42);
}

#[test]
fn run_handle_transaction_with_kv_ops() {
    let (_, handle) = setup();

    handle.transaction(|txn| {
        use strata_primitives::extensions::KVStoreExt;
        txn.kv_put("key", strata_core::value::Value::String("value".into()))?;
        Ok(())
    }).unwrap();

    // Value should persist after transaction
    let value = handle.kv().get("key").unwrap();
    assert!(value.is_some());
}

#[test]
fn run_handle_transaction_returns_value() {
    let (_, handle) = setup();

    let result = handle.transaction(|txn| {
        use strata_primitives::extensions::KVStoreExt;
        txn.kv_put("key", strata_core::value::Value::Int(42))?;
        Ok("success")
    });

    assert_eq!(result.unwrap(), "success");
}

// ============================================================================
// Run Isolation
// ============================================================================

#[test]
fn run_handle_isolates_between_runs() {
    let db = Arc::new(Database::builder().in_memory().open_temp().unwrap());
    let run1 = RunId::new();
    let run2 = RunId::new();

    let handle1 = RunHandle::new(db.clone(), run1);
    let handle2 = RunHandle::new(db.clone(), run2);

    // Write to run1
    handle1.kv().put("key", strata_core::value::Value::String("run1".into())).unwrap();

    // Write same key to run2
    handle2.kv().put("key", strata_core::value::Value::String("run2".into())).unwrap();

    // Each run has its own value
    let v1 = handle1.kv().get("key").unwrap().unwrap();
    let v2 = handle2.kv().get("key").unwrap().unwrap();

    // Values are different (isolated)
    assert_ne!(v1.value, v2.value);
}

#[test]
fn run_handle_events_isolated() {
    let db = Arc::new(Database::builder().in_memory().open_temp().unwrap());
    let run1 = RunId::new();
    let run2 = RunId::new();

    let handle1 = RunHandle::new(db.clone(), run1);
    let handle2 = RunHandle::new(db.clone(), run2);

    // Append events to run1
    let seq1 = handle1.events().append("e1", strata_core::value::Value::Null).unwrap();
    let _seq2 = handle1.events().append("e2", strata_core::value::Value::Null).unwrap();

    // run1 can read the event
    let event_result = handle1.events().read(seq1).unwrap();
    assert!(event_result.is_some());

    // run2 cannot read run1's event (events are isolated by run)
    let run2_result = handle2.events().read(seq1).unwrap();
    assert!(run2_result.is_none());
}

// ============================================================================
// Thread Safety
// ============================================================================

#[test]
fn run_handle_is_send() {
    fn assert_send<T: Send>() {}
    assert_send::<RunHandle>();
}

#[test]
fn run_handle_is_sync() {
    fn assert_sync<T: Sync>() {}
    assert_sync::<RunHandle>();
}

#[test]
fn run_handle_concurrent_access() {
    use std::thread;

    let db = Arc::new(Database::builder().in_memory().open_temp().unwrap());
    let run_id = RunId::new();
    let handle = RunHandle::new(db, run_id);

    let handle1 = handle.clone();
    let handle2 = handle.clone();

    // Spawn two threads using the same handle
    let t1 = thread::spawn(move || {
        for i in 0..10 {
            handle1.kv().put(&format!("t1-key-{}", i), strata_core::value::Value::Int(i as i64)).unwrap();
        }
    });

    let t2 = thread::spawn(move || {
        for i in 0..10 {
            handle2.kv().put(&format!("t2-key-{}", i), strata_core::value::Value::Int(i as i64)).unwrap();
        }
    });

    t1.join().unwrap();
    t2.join().unwrap();

    // All keys should exist
    for i in 0..10 {
        assert!(handle.kv().exists(&format!("t1-key-{}", i)).unwrap());
        assert!(handle.kv().exists(&format!("t2-key-{}", i)).unwrap());
    }
}

// ============================================================================
// Primitive Handle Cloning
// ============================================================================

#[test]
fn kv_handle_is_clone() {
    let (_, handle) = setup();
    let kv = handle.kv();
    let _kv2 = kv.clone();
}

#[test]
fn event_handle_is_clone() {
    let (_, handle) = setup();
    let events = handle.events();
    let _events2 = events.clone();
}

#[test]
fn state_handle_is_clone() {
    let (_, handle) = setup();
    let state = handle.state();
    let _state2 = state.clone();
}

#[test]
fn json_handle_is_clone() {
    let (_, handle) = setup();
    let json = handle.json();
    let _json2 = json.clone();
}

#[test]
fn vector_handle_is_clone() {
    let (_, handle) = setup();
    let vectors = handle.vectors();
    let _vectors2 = vectors.clone();
}

// ============================================================================
// Multiple Operations
// ============================================================================

#[test]
fn run_handle_multiple_primitive_operations() {
    let (_, handle) = setup();

    // Use multiple primitives through the same handle
    handle.kv().put("key", strata_core::value::Value::Int(1)).unwrap();
    let event_seq = handle.events().append("event", strata_core::value::Value::Null).unwrap();
    handle.state().set("cell", strata_core::value::Value::Int(0)).unwrap();

    // All should exist
    assert!(handle.kv().exists("key").unwrap());
    assert!(handle.events().read(event_seq).unwrap().is_some());
    assert!(handle.state().read("cell").unwrap().is_some());
}
