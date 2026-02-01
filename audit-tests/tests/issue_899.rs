//! Audit test for issue #899: Panic during commit leaves transaction in indeterminate state
//! Verdict: CONFIRMED BUG
//!
//! The commit flow in TransactionManager sets the transaction status to Committed
//! (via txn.commit()) BEFORE the WAL write. If a panic occurs between these steps,
//! the transaction is marked Committed in-memory but has no WAL record for durability.
//! Since parking_lot::Mutex does not poison on panic, subsequent operations proceed
//! without detecting the inconsistency.

use std::sync::Arc;
use strata_concurrency::TransactionContext;
use strata_core::types::{BranchId, Key, Namespace};
use strata_core::value::Value;
use strata_storage::ShardedStore;

fn create_test_namespace(branch_id: BranchId) -> Namespace {
    Namespace::new(
        "tenant".to_string(),
        "app".to_string(),
        "agent".to_string(),
        branch_id,
    )
}

fn create_test_key(ns: &Namespace, name: &str) -> Key {
    Key::new_kv(ns.clone(), name)
}

#[test]
fn issue_899_commit_sets_status_before_wal_write() {
    // Demonstrate that txn.commit(store) sets the status to Committed
    // BEFORE any WAL write happens. This is the root cause of the bug.
    let store = Arc::new(ShardedStore::new());
    let branch_id = BranchId::new();
    let ns = create_test_namespace(branch_id);
    let key = create_test_key(&ns, "test-key");

    let snapshot = store.snapshot();
    let mut txn = TransactionContext::with_snapshot(1, branch_id, Box::new(snapshot));
    txn.put(key, Value::Int(42)).unwrap();

    // This call validates and sets status to Committed
    txn.commit(store.as_ref()).unwrap();

    // BUG: The transaction is now marked as Committed, but no WAL record exists.
    // In TransactionManager::commit(), this happens at line 194, BEFORE the WAL
    // write at lines 204-225. If a panic occurred between these two points,
    // the transaction would be Committed in-memory but not durable.
    assert!(
        txn.is_committed(),
        "Transaction is marked Committed before WAL write occurs"
    );

    // The transaction can no longer be aborted (terminal state)
    assert!(
        txn.mark_aborted("trying to abort".to_string()).is_err(),
        "Cannot abort a Committed transaction - the status is irreversible"
    );
}

#[test]
fn issue_899_version_consumed_before_wal_write() {
    // Demonstrate that allocate_version() is called after commit() but
    // before WAL write, consuming a version number that may never be durable.
    let manager = strata_concurrency::TransactionManager::new(0);

    // Allocate a version (simulating what happens after txn.commit() succeeds)
    let v1 = manager.allocate_version();
    assert_eq!(v1, 1);

    // If a panic happened here (before WAL write), version 1 is consumed
    // but no WAL record exists for it. The next transaction would get version 2.
    let v2 = manager.allocate_version();
    assert_eq!(v2, 2);

    // The gap at version 1 (no WAL record) is the symptom of this bug
    assert_eq!(
        manager.current_version(),
        2,
        "Version counter advanced past the lost version"
    );
}

#[test]
fn issue_899_parking_lot_mutex_does_not_poison() {
    // Demonstrate that parking_lot::Mutex does not poison on panic,
    // which means subsequent lock acquisitions succeed silently after a panic.
    use parking_lot::Mutex;

    let mutex = Arc::new(Mutex::new(42));
    let mutex_clone = Arc::clone(&mutex);

    // Spawn a thread that panics while holding the lock
    let handle = std::thread::spawn(move || {
        let _guard = mutex_clone.lock();
        // guard is dropped during unwind, releasing the lock
        panic!("simulated panic while holding lock");
    });

    // Wait for the thread to finish (it panicked)
    let result = handle.join();
    assert!(result.is_err(), "Thread should have panicked");

    // BUG: The mutex is NOT poisoned - we can still acquire it
    // With std::sync::Mutex, this would return Err(PoisonError)
    let guard = mutex.lock();
    assert_eq!(
        *guard, 42,
        "Lock acquired successfully after panic - no poisoning"
    );

    // This means if a commit panics after acquiring the per-branch lock,
    // subsequent commits on that branch will proceed without knowing
    // that a previous commit was interrupted mid-way.
}
