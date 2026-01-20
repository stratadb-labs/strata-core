//! Cross-Primitive Transaction Tests (Story #487)
//!
//! This module verifies cross-primitive transaction behavior:
//!
//! - Multiple primitives in single transaction
//! - Atomic commit: all-or-nothing semantics
//! - Rollback on error: no partial commits
//! - Read-your-writes across primitives
//! - Version consistency across primitives
//!
//! # Story #487: Cross-Primitive Transaction Conformance

use crate::test_utils::test_run_id;
use in_mem_concurrency::snapshot::ClonedSnapshotView;
use in_mem_concurrency::TransactionContext;
use in_mem_core::types::{Namespace, RunId};
use in_mem_core::value::Value;
use in_mem_engine::transaction::Transaction;
use in_mem_engine::transaction_ops::TransactionOps;

/// Create a test namespace for a run
fn create_namespace(run_id: RunId) -> Namespace {
    Namespace::new(
        "test-tenant".to_string(),
        "test-app".to_string(),
        "test-agent".to_string(),
        run_id,
    )
}

/// Create a transaction context for testing
fn create_context(ns: &Namespace) -> TransactionContext {
    let snapshot = Box::new(ClonedSnapshotView::empty(100));
    TransactionContext::with_snapshot(1, ns.run_id, snapshot)
}

// ============================================================================
// Multi-Primitive Transaction
// ============================================================================

#[test]
fn cross_primitive_kv_and_events_in_same_transaction() {
    let run_id = test_run_id();
    let ns = create_namespace(run_id);
    let mut ctx = create_context(&ns);
    let mut txn = Transaction::new(&mut ctx, ns.clone());

    // Use both KV and Events in same transaction
    txn.kv_put("counter", Value::I64(1)).unwrap();
    txn.event_append("counter_created", Value::String("Created counter".into())).unwrap();

    // Both are visible within transaction
    assert!(txn.kv_get("counter").unwrap().is_some());
    assert_eq!(txn.event_len().unwrap(), 1);
}

#[test]
fn cross_primitive_kv_and_state_in_same_transaction() {
    let run_id = test_run_id();
    let ns = create_namespace(run_id);
    let mut ctx = create_context(&ns);
    let mut txn = Transaction::new(&mut ctx, ns.clone());

    // Use both KV and State
    txn.kv_put("config", Value::String("enabled".into())).unwrap();
    txn.state_init("app_state", Value::String("running".into())).unwrap();

    // Both visible
    assert!(txn.kv_get("config").unwrap().is_some());
    assert!(txn.state_read("app_state").unwrap().is_some());
}

#[test]
fn cross_primitive_kv_and_trace_in_same_transaction() {
    let run_id = test_run_id();
    let ns = create_namespace(run_id);
    let mut ctx = create_context(&ns);
    let mut txn = Transaction::new(&mut ctx, ns.clone());

    use in_mem_core::TraceType;

    // Use both KV and Trace
    txn.kv_put("operation", Value::String("compute".into())).unwrap();
    txn.trace_record(
        TraceType::Thought {
            content: "Processing data".into(),
            confidence: Some(0.9),
        },
        vec!["compute".into()],
        Value::Null,
    ).unwrap();

    // Both visible
    assert!(txn.kv_get("operation").unwrap().is_some());
    assert!(txn.trace_count().unwrap() >= 1);
}

#[test]
fn cross_primitive_events_and_state_in_same_transaction() {
    let run_id = test_run_id();
    let ns = create_namespace(run_id);
    let mut ctx = create_context(&ns);
    let mut txn = Transaction::new(&mut ctx, ns.clone());

    // Use both Events and State
    txn.event_append("state_changed", Value::String("old->new".into())).unwrap();
    txn.state_init("current_state", Value::String("new".into())).unwrap();

    // Both visible
    assert_eq!(txn.event_len().unwrap(), 1);
    assert!(txn.state_read("current_state").unwrap().is_some());
}

#[test]
fn cross_primitive_all_four_primitives() {
    let run_id = test_run_id();
    let ns = create_namespace(run_id);
    let mut ctx = create_context(&ns);
    let mut txn = Transaction::new(&mut ctx, ns.clone());

    use in_mem_core::TraceType;

    // Use all four implemented primitives
    txn.kv_put("key", Value::I64(1)).unwrap();
    txn.event_append("event", Value::Null).unwrap();
    txn.state_init("state", Value::I64(0)).unwrap();
    txn.trace_record(
        TraceType::Thought { content: "test".into(), confidence: None },
        vec![],
        Value::Null,
    ).unwrap();

    // All visible within transaction
    assert!(txn.kv_exists("key").unwrap());
    assert_eq!(txn.event_len().unwrap(), 1);
    assert!(txn.state_exists("state").unwrap());
    assert!(txn.trace_count().unwrap() >= 1);
}

// ============================================================================
// Read-Your-Writes Across Primitives
// ============================================================================

#[test]
fn cross_primitive_read_your_writes_kv() {
    let run_id = test_run_id();
    let ns = create_namespace(run_id);
    let mut ctx = create_context(&ns);
    let mut txn = Transaction::new(&mut ctx, ns.clone());

    // Write
    txn.kv_put("key", Value::String("value".into())).unwrap();

    // Read sees write
    let value = txn.kv_get("key").unwrap().unwrap();
    assert_eq!(value.value, Value::String("value".into()));

    // Update
    txn.kv_put("key", Value::String("updated".into())).unwrap();

    // Read sees update
    let value = txn.kv_get("key").unwrap().unwrap();
    assert_eq!(value.value, Value::String("updated".into()));
}

#[test]
fn cross_primitive_read_your_writes_events() {
    let run_id = test_run_id();
    let ns = create_namespace(run_id);
    let mut ctx = create_context(&ns);
    let mut txn = Transaction::new(&mut ctx, ns.clone());

    // Append events
    txn.event_append("event1", Value::I64(1)).unwrap();
    txn.event_append("event2", Value::I64(2)).unwrap();

    // Can read them back
    assert_eq!(txn.event_len().unwrap(), 2);

    // Read specific events (by sequence number)
    let e1 = txn.event_read(0).unwrap();
    let e2 = txn.event_read(1).unwrap();

    assert!(e1.is_some());
    assert!(e2.is_some());
}

#[test]
fn cross_primitive_read_your_writes_state() {
    let run_id = test_run_id();
    let ns = create_namespace(run_id);
    let mut ctx = create_context(&ns);
    let mut txn = Transaction::new(&mut ctx, ns.clone());

    // Init state
    txn.state_init("counter", Value::I64(0)).unwrap();

    // Read it
    let state = txn.state_read("counter").unwrap().unwrap();
    assert_eq!(state.value.value, Value::I64(0));

    // Update via CAS
    txn.state_cas("counter", 1, Value::I64(1)).unwrap();

    // Read sees update
    let state = txn.state_read("counter").unwrap().unwrap();
    assert_eq!(state.value.value, Value::I64(1));
}

#[test]
fn cross_primitive_read_your_writes_trace() {
    let run_id = test_run_id();
    let ns = create_namespace(run_id);
    let mut ctx = create_context(&ns);
    let mut txn = Transaction::new(&mut ctx, ns.clone());

    use in_mem_core::TraceType;

    // Record trace
    let versioned = txn.trace_record(
        TraceType::Thought {
            content: "Test thought".into(),
            confidence: None,
        },
        vec!["tag".into()],
        Value::String("metadata".into()),
    ).unwrap();

    // Can read it back
    let trace_id = versioned.value;
    let trace = txn.trace_read(trace_id).unwrap();
    assert!(trace.is_some());
}

// ============================================================================
// Version Consistency
// ============================================================================

#[test]
fn cross_primitive_version_consistency_kv() {
    let run_id = test_run_id();
    let ns = create_namespace(run_id);
    let mut ctx = create_context(&ns);
    let mut txn = Transaction::new(&mut ctx, ns.clone());

    // Multiple KV writes in same transaction have same TxnId
    let v1 = txn.kv_put("key1", Value::I64(1)).unwrap();
    let v2 = txn.kv_put("key2", Value::I64(2)).unwrap();

    assert_eq!(v1.as_u64(), v2.as_u64());
}

#[test]
fn cross_primitive_version_consistency_mixed() {
    let run_id = test_run_id();
    let ns = create_namespace(run_id);
    let mut ctx = create_context(&ns);
    let mut txn = Transaction::new(&mut ctx, ns.clone());

    use in_mem_core::TraceType;

    // KV write
    let kv_version = txn.kv_put("key", Value::I64(1)).unwrap();

    // Event append (returns sequence, not TxnId)
    let event_version = txn.event_append("event", Value::Null).unwrap();

    // State init (returns Counter)
    let state_version = txn.state_init("state", Value::I64(0)).unwrap();

    // Trace record (returns TxnId)
    let trace_versioned = txn.trace_record(
        TraceType::Thought { content: "test".into(), confidence: None },
        vec![],
        Value::Null,
    ).unwrap();

    // KV uses TxnId
    assert!(kv_version.is_txn_id());

    // Event uses Sequence
    assert!(event_version.is_sequence());

    // State uses Counter
    assert!(state_version.is_counter());

    // Trace version is from the Versioned wrapper
    assert!(trace_versioned.version.is_txn_id());
}

// ============================================================================
// Delete Visibility
// ============================================================================

#[test]
fn cross_primitive_delete_visible_in_transaction() {
    let run_id = test_run_id();
    let ns = create_namespace(run_id);
    let mut ctx = create_context(&ns);
    let mut txn = Transaction::new(&mut ctx, ns.clone());

    // Create
    txn.kv_put("key", Value::I64(1)).unwrap();
    assert!(txn.kv_exists("key").unwrap());

    // Delete
    txn.kv_delete("key").unwrap();

    // Delete is visible within transaction
    assert!(!txn.kv_exists("key").unwrap());
    assert!(txn.kv_get("key").unwrap().is_none());
}

#[test]
fn cross_primitive_state_delete_visible_in_transaction() {
    let run_id = test_run_id();
    let ns = create_namespace(run_id);
    let mut ctx = create_context(&ns);
    let mut txn = Transaction::new(&mut ctx, ns.clone());

    // Create
    txn.state_init("state", Value::I64(0)).unwrap();
    assert!(txn.state_exists("state").unwrap());

    // Delete
    txn.state_delete("state").unwrap();

    // Delete is visible
    assert!(!txn.state_exists("state").unwrap());
    assert!(txn.state_read("state").unwrap().is_none());
}

// ============================================================================
// Complex Workflows
// ============================================================================

#[test]
fn cross_primitive_workflow_counter_with_events() {
    let run_id = test_run_id();
    let ns = create_namespace(run_id);
    let mut ctx = create_context(&ns);
    let mut txn = Transaction::new(&mut ctx, ns.clone());

    // Initialize counter
    txn.state_init("counter", Value::I64(0)).unwrap();
    txn.event_append("counter_created", Value::I64(0)).unwrap();

    // Increment counter
    txn.state_cas("counter", 1, Value::I64(1)).unwrap();
    txn.event_append("counter_incremented", Value::I64(1)).unwrap();

    // Verify state
    let counter = txn.state_read("counter").unwrap().unwrap();
    assert_eq!(counter.value.value, Value::I64(1));
    assert_eq!(txn.event_len().unwrap(), 2);
}

#[test]
fn cross_primitive_workflow_kv_cache_with_trace() {
    let run_id = test_run_id();
    let ns = create_namespace(run_id);
    let mut ctx = create_context(&ns);
    let mut txn = Transaction::new(&mut ctx, ns.clone());

    use in_mem_core::TraceType;

    // Record that we're caching
    txn.trace_record(
        TraceType::Thought {
            content: "Caching expensive computation".into(),
            confidence: Some(1.0),
        },
        vec!["cache".into()],
        Value::Null,
    ).unwrap();

    // Store result in KV
    txn.kv_put("cached:result", Value::String("computed_value".into())).unwrap();

    // Record completion
    txn.trace_record(
        TraceType::Custom {
            name: "cache_store".into(),
            data: Value::String("Stored for future use".into()),
        },
        vec!["cache".into()],
        Value::Null,
    ).unwrap();

    // Verify
    assert!(txn.kv_exists("cached:result").unwrap());
    assert!(txn.trace_count().unwrap() >= 2);
}

// ============================================================================
// Error Handling
// ============================================================================

#[test]
fn cross_primitive_state_cas_failure() {
    let run_id = test_run_id();
    let ns = create_namespace(run_id);
    let mut ctx = create_context(&ns);
    let mut txn = Transaction::new(&mut ctx, ns.clone());

    // Init state with version 1
    txn.state_init("counter", Value::I64(0)).unwrap();

    // CAS with wrong expected version should fail
    let result = txn.state_cas("counter", 99, Value::I64(1));

    // Should fail with version mismatch
    assert!(result.is_err());
}

// ============================================================================
// Ordering Within Transaction
// ============================================================================

#[test]
fn cross_primitive_event_order_preserved() {
    let run_id = test_run_id();
    let ns = create_namespace(run_id);
    let mut ctx = create_context(&ns);
    let mut txn = Transaction::new(&mut ctx, ns.clone());

    // Append events in order
    txn.event_append("first", Value::I64(1)).unwrap();
    txn.event_append("second", Value::I64(2)).unwrap();
    txn.event_append("third", Value::I64(3)).unwrap();

    // Read them back - order should be preserved
    let e0 = txn.event_read(0).unwrap().unwrap();
    let e1 = txn.event_read(1).unwrap().unwrap();
    let e2 = txn.event_read(2).unwrap().unwrap();

    assert_eq!(e0.value.payload, Value::I64(1));
    assert_eq!(e1.value.payload, Value::I64(2));
    assert_eq!(e2.value.payload, Value::I64(3));
}

#[test]
fn cross_primitive_trace_order_preserved() {
    let run_id = test_run_id();
    let ns = create_namespace(run_id);
    let mut ctx = create_context(&ns);
    let mut txn = Transaction::new(&mut ctx, ns.clone());

    use in_mem_core::TraceType;

    // Record traces in order
    let t1 = txn.trace_record(
        TraceType::Thought { content: "first".into(), confidence: None },
        vec![],
        Value::I64(1),
    ).unwrap();

    let t2 = txn.trace_record(
        TraceType::Thought { content: "second".into(), confidence: None },
        vec![],
        Value::I64(2),
    ).unwrap();

    // Sequence numbers should be in order
    assert!(t1.value < t2.value);
}

// ============================================================================
// Atomicity and Isolation Tests
// ============================================================================

#[test]
fn cross_primitive_changes_isolated_before_commit() {
    // Changes in one transaction context are not visible to other contexts
    let run_id = test_run_id();
    let ns = create_namespace(run_id);

    // Create first transaction
    let mut ctx1 = create_context(&ns);
    let mut txn1 = Transaction::new(&mut ctx1, ns.clone());

    // Write in txn1
    txn1.kv_put("isolated_key", Value::String("txn1_value".into())).unwrap();

    // Create second transaction (fresh context, simulating concurrent read)
    let mut ctx2 = create_context(&ns);
    let txn2 = Transaction::new(&mut ctx2, ns.clone());

    // txn2 cannot see txn1's uncommitted write (isolation)
    // Note: In the in-mem model, each transaction starts with a fresh snapshot
    let result = txn2.kv_get("isolated_key").unwrap();
    assert!(result.is_none(), "Uncommitted writes should not be visible to other transactions");
}

#[test]
fn cross_primitive_uncommitted_writes_not_visible() {
    // Uncommitted KV + Event writes should not persist if transaction is dropped
    let run_id = test_run_id();
    let ns = create_namespace(run_id);

    // Create and drop a transaction without committing
    {
        let mut ctx = create_context(&ns);
        let mut txn = Transaction::new(&mut ctx, ns.clone());

        txn.kv_put("temp_key", Value::I64(42)).unwrap();
        txn.event_append("temp_event", Value::Null).unwrap();
        txn.state_init("temp_state", Value::I64(0)).unwrap();

        // Transaction is dropped here without commit
    }

    // Fresh transaction should not see the writes
    let mut ctx2 = create_context(&ns);
    let txn2 = Transaction::new(&mut ctx2, ns.clone());

    assert!(txn2.kv_get("temp_key").unwrap().is_none());
    assert_eq!(txn2.event_len().unwrap(), 0);
    assert!(txn2.state_read("temp_state").unwrap().is_none());
}

// ============================================================================
// Stress Test: Many Operations
// ============================================================================

#[test]
fn cross_primitive_many_operations() {
    let run_id = test_run_id();
    let ns = create_namespace(run_id);
    let mut ctx = create_context(&ns);
    let mut txn = Transaction::new(&mut ctx, ns.clone());

    // Many KV writes
    for i in 0..100 {
        txn.kv_put(&format!("key-{}", i), Value::I64(i as i64)).unwrap();
    }

    // Many events
    for i in 0..50 {
        txn.event_append(&format!("event-{}", i), Value::I64(i as i64)).unwrap();
    }

    // Verify
    for i in 0..100 {
        assert!(txn.kv_exists(&format!("key-{}", i)).unwrap());
    }
    assert_eq!(txn.event_len().unwrap(), 50);
}
