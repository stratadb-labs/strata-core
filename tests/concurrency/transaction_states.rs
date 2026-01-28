//! Transaction State Machine Tests
//!
//! Tests the transaction state transitions:
//! - Active → Validating → Committed
//! - Active → Validating → Aborted
//! - Active → Aborted (explicit)

use strata_concurrency::transaction::{TransactionContext, TransactionStatus};
use strata_core::RunId;

// ============================================================================
// State Inspection
// ============================================================================

#[test]
fn new_transaction_is_active() {
    let run_id = RunId::new();
    let txn = TransactionContext::new(1, run_id, 100);

    assert!(txn.is_active());
    assert!(!txn.is_committed());
    assert!(!txn.is_aborted());
    assert!(matches!(txn.status.clone(), TransactionStatus::Active));
}

#[test]
fn transaction_status_active_variant() {
    let run_id = RunId::new();
    let txn = TransactionContext::new(1, run_id, 100);

    match txn.status.clone() {
        TransactionStatus::Active => {}
        _ => panic!("Expected Active status"),
    }
}

// ============================================================================
// Valid State Transitions
// ============================================================================

#[test]
fn active_to_validating_succeeds() {
    let run_id = RunId::new();
    let mut txn = TransactionContext::new(1, run_id, 100);

    assert!(txn.is_active());
    txn.mark_validating().unwrap();
    assert!(matches!(txn.status.clone(), TransactionStatus::Validating));
}

#[test]
fn validating_to_committed_succeeds() {
    let run_id = RunId::new();
    let mut txn = TransactionContext::new(1, run_id, 100);

    txn.mark_validating().unwrap();
    txn.mark_committed().unwrap();

    assert!(txn.is_committed());
    assert!(!txn.is_active());
    assert!(!txn.is_aborted());
    assert!(matches!(txn.status.clone(), TransactionStatus::Committed));
}

#[test]
fn validating_to_aborted_succeeds() {
    let run_id = RunId::new();
    let mut txn = TransactionContext::new(1, run_id, 100);

    txn.mark_validating().unwrap();
    txn.mark_aborted("validation failed".to_string()).unwrap();

    assert!(txn.is_aborted());
    assert!(!txn.is_active());
    assert!(!txn.is_committed());
}

#[test]
fn active_to_aborted_succeeds() {
    let run_id = RunId::new();
    let mut txn = TransactionContext::new(1, run_id, 100);

    txn.mark_aborted("explicit abort".to_string()).unwrap();

    assert!(txn.is_aborted());
    assert!(!txn.is_active());
}

#[test]
fn aborted_status_contains_reason() {
    let run_id = RunId::new();
    let mut txn = TransactionContext::new(1, run_id, 100);

    txn.mark_aborted("conflict detected".to_string()).unwrap();

    match txn.status.clone() {
        TransactionStatus::Aborted { reason } => {
            assert!(reason.contains("conflict"));
        }
        _ => panic!("Expected Aborted status"),
    }
}

#[test]
fn committed_status_is_committed() {
    let run_id = RunId::new();
    let mut txn = TransactionContext::new(1, run_id, 100);

    txn.mark_validating().unwrap();
    txn.mark_committed().unwrap();

    assert!(matches!(txn.status.clone(), TransactionStatus::Committed));
}

// ============================================================================
// Invalid State Transitions
// ============================================================================

#[test]
fn double_mark_validating_fails() {
    let run_id = RunId::new();
    let mut txn = TransactionContext::new(1, run_id, 100);

    txn.mark_validating().unwrap();
    let result = txn.mark_validating();
    assert!(result.is_err(), "Second mark_validating should fail");
}

#[test]
fn commit_while_active_fails() {
    let run_id = RunId::new();
    let mut txn = TransactionContext::new(1, run_id, 100);

    // Skip validating state
    let result = txn.mark_committed();
    assert!(result.is_err(), "Commit from Active should fail");
}

#[test]
fn commit_while_aborted_fails() {
    let run_id = RunId::new();
    let mut txn = TransactionContext::new(1, run_id, 100);

    txn.mark_aborted("aborted".to_string()).unwrap();
    let result = txn.mark_committed();
    assert!(result.is_err(), "Commit after abort should fail");
}

#[test]
fn commit_while_already_committed_fails() {
    let run_id = RunId::new();
    let mut txn = TransactionContext::new(1, run_id, 100);

    txn.mark_validating().unwrap();
    txn.mark_committed().unwrap();
    let result = txn.mark_committed();
    assert!(result.is_err(), "Double commit should fail");
}

// ============================================================================
// Transaction Properties
// ============================================================================

#[test]
fn transaction_preserves_ids() {
    let run_id = RunId::new();
    let txn = TransactionContext::new(42, run_id, 100);

    assert_eq!(txn.txn_id, 42);
    assert_eq!(txn.run_id, run_id);
    assert_eq!(txn.start_version, 100);
}

#[test]
fn transaction_tracks_elapsed_time() {
    let run_id = RunId::new();
    let txn = TransactionContext::new(1, run_id, 100);

    let elapsed = txn.elapsed();
    assert!(elapsed.as_secs() < 1, "Elapsed should be very small");
}

#[test]
fn transaction_expiration_check() {
    use std::time::Duration;

    let run_id = RunId::new();
    let txn = TransactionContext::new(1, run_id, 100);

    // Should not be expired with 1 hour timeout
    assert!(!txn.is_expired(Duration::from_secs(3600)));

    // Should be "expired" with 0 timeout (always expired)
    assert!(txn.is_expired(Duration::from_secs(0)));
}

// ============================================================================
// Read-Only Detection
// ============================================================================

#[test]
fn empty_transaction_is_read_only() {
    let run_id = RunId::new();
    let txn = TransactionContext::new(1, run_id, 100);

    assert!(txn.is_read_only());
}

#[test]
fn transaction_with_write_is_not_read_only() {
    use strata_core::types::{Key, Namespace};
    use strata_core::value::Value;

    let run_id = RunId::new();
    let mut txn = TransactionContext::new(1, run_id, 100);

    let key = Key::new_kv(Namespace::for_run(run_id), "test");
    txn.write_set.insert(key, Value::Int(42));

    assert!(!txn.is_read_only());
}

#[test]
fn transaction_with_delete_is_not_read_only() {
    use strata_core::types::{Key, Namespace};

    let run_id = RunId::new();
    let mut txn = TransactionContext::new(1, run_id, 100);

    let key = Key::new_kv(Namespace::for_run(run_id), "test");
    txn.delete_set.insert(key);

    assert!(!txn.is_read_only());
}

#[test]
fn transaction_with_cas_is_not_read_only() {
    use strata_concurrency::transaction::CASOperation;
    use strata_core::types::{Key, Namespace};
    use strata_core::value::Value;

    let run_id = RunId::new();
    let mut txn = TransactionContext::new(1, run_id, 100);

    let key = Key::new_kv(Namespace::for_run(run_id), "test");
    txn.cas_set.push(CASOperation {
        key,
        expected_version: 1,
        new_value: Value::Int(42),
    });

    assert!(!txn.is_read_only());
}

#[test]
fn transaction_with_only_reads_is_read_only() {
    use strata_core::types::{Key, Namespace};

    let run_id = RunId::new();
    let mut txn = TransactionContext::new(1, run_id, 100);

    let key = Key::new_kv(Namespace::for_run(run_id), "test");
    txn.read_set.insert(key, 1);

    assert!(txn.is_read_only());
}

// ============================================================================
// Operation Counts
// ============================================================================

#[test]
fn pending_operations_count() {
    use strata_core::types::{Key, Namespace};
    use strata_core::value::Value;

    let run_id = RunId::new();
    let mut txn = TransactionContext::new(1, run_id, 100);

    let ns = Namespace::for_run(run_id);

    // Add some operations
    txn.write_set.insert(Key::new_kv(ns.clone(), "w1"), Value::Int(1));
    txn.write_set.insert(Key::new_kv(ns.clone(), "w2"), Value::Int(2));
    txn.delete_set.insert(Key::new_kv(ns.clone(), "d1"));

    let pending = txn.pending_operations();
    assert_eq!(pending.puts, 2);
    assert_eq!(pending.deletes, 1);
    assert_eq!(pending.cas, 0);
}

#[test]
fn read_count_tracks_reads() {
    use strata_core::types::{Key, Namespace};

    let run_id = RunId::new();
    let mut txn = TransactionContext::new(1, run_id, 100);

    let ns = Namespace::for_run(run_id);

    txn.read_set.insert(Key::new_kv(ns.clone(), "r1"), 1);
    txn.read_set.insert(Key::new_kv(ns.clone(), "r2"), 2);
    txn.read_set.insert(Key::new_kv(ns.clone(), "r3"), 3);

    assert_eq!(txn.read_count(), 3);
}

#[test]
fn write_count_tracks_only_writes() {
    use strata_core::types::{Key, Namespace};
    use strata_core::value::Value;

    let run_id = RunId::new();
    let mut txn = TransactionContext::new(1, run_id, 100);

    let ns = Namespace::for_run(run_id);

    txn.write_set.insert(Key::new_kv(ns.clone(), "w1"), Value::Int(1));
    txn.write_set.insert(Key::new_kv(ns.clone(), "w2"), Value::Int(2));
    txn.delete_set.insert(Key::new_kv(ns.clone(), "d1"));
    txn.delete_set.insert(Key::new_kv(ns.clone(), "d2"));

    assert_eq!(txn.write_count(), 2); // Only writes, not deletes
    assert_eq!(txn.delete_set.len(), 2); // Deletes tracked separately
}
