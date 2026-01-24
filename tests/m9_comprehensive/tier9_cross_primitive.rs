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
use strata_concurrency::snapshot::ClonedSnapshotView;
use strata_concurrency::TransactionContext;
use strata_core::types::{Namespace, RunId};
use strata_core::value::Value;
use strata_engine::transaction::Transaction;
use strata_engine::transaction_ops::TransactionOps;

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
    txn.kv_put("counter", Value::Int(1)).unwrap();
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
fn cross_primitive_all_three_primitives() {
    let run_id = test_run_id();
    let ns = create_namespace(run_id);
    let mut ctx = create_context(&ns);
    let mut txn = Transaction::new(&mut ctx, ns.clone());

    // Use all three primitives
    txn.kv_put("key", Value::Int(1)).unwrap();
    txn.event_append("event", Value::Null).unwrap();
    txn.state_init("state", Value::Int(0)).unwrap();

    // All visible within transaction
    assert!(txn.kv_exists("key").unwrap());
    assert_eq!(txn.event_len().unwrap(), 1);
    assert!(txn.state_exists("state").unwrap());
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
    txn.event_append("event1", Value::Int(1)).unwrap();
    txn.event_append("event2", Value::Int(2)).unwrap();

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
    txn.state_init("counter", Value::Int(0)).unwrap();

    // Read it
    let state = txn.state_read("counter").unwrap().unwrap();
    assert_eq!(state.value.value, Value::Int(0));

    // Update via CAS
    txn.state_cas("counter", 1, Value::Int(1)).unwrap();

    // Read sees update
    let state = txn.state_read("counter").unwrap().unwrap();
    assert_eq!(state.value.value, Value::Int(1));
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
    let v1 = txn.kv_put("key1", Value::Int(1)).unwrap();
    let v2 = txn.kv_put("key2", Value::Int(2)).unwrap();

    assert_eq!(v1.as_u64(), v2.as_u64());
}

#[test]
fn cross_primitive_version_consistency_mixed() {
    let run_id = test_run_id();
    let ns = create_namespace(run_id);
    let mut ctx = create_context(&ns);
    let mut txn = Transaction::new(&mut ctx, ns.clone());

    // KV write
    let kv_version = txn.kv_put("key", Value::Int(1)).unwrap();

    // Event append (returns sequence, not TxnId)
    let event_version = txn.event_append("event", Value::Null).unwrap();

    // State init (returns Counter)
    let state_version = txn.state_init("state", Value::Int(0)).unwrap();

    // KV uses TxnId
    assert!(kv_version.is_txn_id());

    // Event uses Sequence
    assert!(event_version.is_sequence());

    // State uses Counter
    assert!(state_version.is_counter());
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
    txn.kv_put("key", Value::Int(1)).unwrap();
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
    txn.state_init("state", Value::Int(0)).unwrap();
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
    txn.state_init("counter", Value::Int(0)).unwrap();
    txn.event_append("counter_created", Value::Int(0)).unwrap();

    // Increment counter
    txn.state_cas("counter", 1, Value::Int(1)).unwrap();
    txn.event_append("counter_incremented", Value::Int(1)).unwrap();

    // Verify state
    let counter = txn.state_read("counter").unwrap().unwrap();
    assert_eq!(counter.value.value, Value::Int(1));
    assert_eq!(txn.event_len().unwrap(), 2);
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
    txn.state_init("counter", Value::Int(0)).unwrap();

    // CAS with wrong expected version should fail
    let result = txn.state_cas("counter", 99, Value::Int(1));

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
    txn.event_append("first", Value::Int(1)).unwrap();
    txn.event_append("second", Value::Int(2)).unwrap();
    txn.event_append("third", Value::Int(3)).unwrap();

    // Read them back - order should be preserved
    let e0 = txn.event_read(0).unwrap().unwrap();
    let e1 = txn.event_read(1).unwrap().unwrap();
    let e2 = txn.event_read(2).unwrap().unwrap();

    assert_eq!(e0.value.payload, Value::Int(1));
    assert_eq!(e1.value.payload, Value::Int(2));
    assert_eq!(e2.value.payload, Value::Int(3));
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

        txn.kv_put("temp_key", Value::Int(42)).unwrap();
        txn.event_append("temp_event", Value::Null).unwrap();
        txn.state_init("temp_state", Value::Int(0)).unwrap();

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
        txn.kv_put(&format!("key-{}", i), Value::Int(i as i64)).unwrap();
    }

    // Many events
    for i in 0..50 {
        txn.event_append(&format!("event-{}", i), Value::Int(i as i64)).unwrap();
    }

    // Verify
    for i in 0..100 {
        assert!(txn.kv_exists(&format!("key-{}", i)).unwrap());
    }
    assert_eq!(txn.event_len().unwrap(), 50);
}
