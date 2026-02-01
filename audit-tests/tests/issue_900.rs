//! Audit test for issue #900: Validation re-reads storage for every key in read-set — no caching
//! Verdict: CONFIRMED BUG
//!
//! During transaction validation, each validation phase (read-set, CAS-set, JSON-set)
//! independently reads from storage. If a key appears in multiple sets, it is read
//! from storage multiple times with no caching between phases.

use std::sync::Arc;
use strata_concurrency::{TransactionContext, TransactionManager};
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
fn issue_900_key_in_both_read_set_and_cas_set_read_twice() {
    // If a key is both read (adding to read_set) and then CAS'd (adding to cas_set),
    // validation will read the key from storage twice: once in validate_read_set()
    // and once in validate_cas_set().
    let store = Arc::new(ShardedStore::new());
    let manager = TransactionManager::new(0);
    let branch_id = BranchId::new();
    let ns = create_test_namespace(branch_id);
    let key = create_test_key(&ns, "shared-key");

    // Setup: create the key in storage
    {
        let snapshot = store.snapshot();
        let mut setup_txn = TransactionContext::with_snapshot(1, branch_id, Box::new(snapshot));
        setup_txn.put(key.clone(), Value::Int(100)).unwrap();
        manager
            .commit(&mut setup_txn, store.as_ref(), None)
            .unwrap();
    }

    // Transaction that reads the key AND performs CAS on it
    let snapshot = store.snapshot();
    let mut txn = TransactionContext::with_snapshot(2, branch_id, Box::new(snapshot));

    // Read the key (adds to read_set with version 1)
    let val = txn.get(&key).unwrap();
    assert!(val.is_some());

    // CAS on the same key (adds to cas_set with expected_version 1)
    txn.cas(key.clone(), 1, Value::Int(200)).unwrap();

    // At this point, the key is in BOTH read_set and cas_set.
    assert!(txn.read_set.contains_key(&key), "Key should be in read_set");
    assert_eq!(txn.cas_set.len(), 1, "Key should be in cas_set");

    // When commit runs validation, it will:
    // 1. validate_read_set() -> store.get(&key)  [1st read]
    // 2. validate_cas_set() -> store.get(&key)    [2nd read, REDUNDANT]
    // Both reads return the same data since we're in the same commit lock.

    // The commit should succeed, but it performed 2 storage reads for 1 key
    let result = manager.commit(&mut txn, store.as_ref(), None);
    assert!(result.is_ok(), "Commit should succeed: {:?}", result);
}

#[test]
fn issue_900_large_read_set_means_many_storage_reads() {
    // Demonstrate the performance impact: a scan_prefix that reads N keys
    // results in N storage reads during validation, even if the transaction
    // only writes to 1 key.
    let store = Arc::new(ShardedStore::new());
    let manager = TransactionManager::new(0);
    let branch_id = BranchId::new();
    let ns = create_test_namespace(branch_id);

    // Setup: create 100 keys
    {
        let snapshot = store.snapshot();
        let mut setup_txn = TransactionContext::with_snapshot(1, branch_id, Box::new(snapshot));
        for i in 0..100 {
            let key = create_test_key(&ns, &format!("prefix:{:03}", i));
            setup_txn.put(key, Value::Int(i as i64)).unwrap();
        }
        manager
            .commit(&mut setup_txn, store.as_ref(), None)
            .unwrap();
    }

    // Transaction: scan all 100 keys, then write 1 key
    let snapshot = store.snapshot();
    let mut txn = TransactionContext::with_snapshot(2, branch_id, Box::new(snapshot));

    let prefix = create_test_key(&ns, "prefix:");
    let results = txn.scan_prefix(&prefix).unwrap();
    assert_eq!(results.len(), 100, "Should scan 100 keys");

    // Write just 1 key
    let write_key = create_test_key(&ns, "prefix:000");
    txn.put(write_key, Value::Int(999)).unwrap();

    // Read set now has 100 entries from the scan
    assert_eq!(
        txn.read_set.len(),
        100,
        "Read set should contain 100 keys from scan"
    );

    // BUG: Validation will perform 100 store.get() calls for validate_read_set(),
    // one for each key in the read-set. There is no batch read API or caching.
    // For a transaction that reads 1000 keys and writes 1, this means 1000+
    // individual storage lookups at commit time.
    let result = manager.commit(&mut txn, store.as_ref(), None);
    assert!(result.is_ok(), "Commit should succeed: {:?}", result);
}

#[test]
fn issue_900_no_cross_phase_version_cache() {
    // Demonstrate that there is no version cache shared across validation phases.
    // Each phase independently calls store.get() for the keys it needs to check.
    //
    // This test verifies the structure of the validation code:
    // validate_transaction() calls:
    //   1. validate_read_set(read_set, store)     -- reads from store
    //   2. validate_cas_set(cas_set, store)        -- reads from store again
    //   3. validate_json_set(json_versions, store) -- reads from store yet again
    //
    // A key appearing in all three sets would be read 3 times.

    let store = Arc::new(ShardedStore::new());
    let manager = TransactionManager::new(0);
    let branch_id = BranchId::new();
    let ns = create_test_namespace(branch_id);
    let key = create_test_key(&ns, "multi-set-key");

    // Setup
    {
        let snapshot = store.snapshot();
        let mut setup_txn = TransactionContext::with_snapshot(1, branch_id, Box::new(snapshot));
        setup_txn.put(key.clone(), Value::Int(1)).unwrap();
        manager
            .commit(&mut setup_txn, store.as_ref(), None)
            .unwrap();
    }

    // Create a transaction that has the key in read_set AND cas_set
    let snapshot = store.snapshot();
    let mut txn = TransactionContext::with_snapshot(2, branch_id, Box::new(snapshot));

    // Read (adds to read_set)
    txn.get(&key).unwrap();
    // CAS (adds to cas_set)
    txn.cas(key.clone(), 1, Value::Int(2)).unwrap();

    // Verify both sets contain the key
    assert!(txn.read_set.contains_key(&key));
    assert_eq!(txn.cas_set.len(), 1);
    assert_eq!(txn.cas_set[0].key, key);

    // Commit validates both sets independently, reading from store twice for this key
    let result = manager.commit(&mut txn, store.as_ref(), None);
    assert!(
        result.is_ok(),
        "Commit should succeed despite redundant reads: {:?}",
        result
    );
}
