//! Audit test for issue #859: contains() ignores tombstones
//! Verdict: CONFIRMED BUG
//!
//! ShardedStore::contains() only checks HashMap key existence,
//! without filtering tombstones or checking TTL expiration.
//! After deleting a key, contains() still returns true.

use std::sync::Arc;
use strata_core::traits::Storage;
use strata_core::types::{BranchId, Key, Namespace};
use strata_core::value::Value;
use strata_storage::ShardedStore;

fn test_ns() -> Namespace {
    let branch_id = BranchId::new();
    Namespace::new(
        "test".to_string(),
        "app".to_string(),
        "agent".to_string(),
        branch_id,
    )
}

fn test_key(ns: &Namespace, name: &str) -> Key {
    Key::new_kv(ns.clone(), name)
}

#[test]
fn issue_859_contains_returns_true_after_delete() {
    let store = Arc::new(ShardedStore::new());
    let ns = test_ns();
    let key = test_key(&ns, "mykey");

    // Put a value
    Storage::put(&*store, key.clone(), Value::Int(42), None).unwrap();

    // Verify contains returns true
    assert!(
        store.contains(&key),
        "contains() should return true for existing key"
    );

    // Verify get returns the value
    assert!(
        Storage::get(&*store, &key).unwrap().is_some(),
        "get() should return value for existing key"
    );

    // Delete the key (adds a tombstone)
    Storage::delete(&*store, &key).unwrap();

    // get() correctly returns None (filters tombstones)
    assert!(
        Storage::get(&*store, &key).unwrap().is_none(),
        "get() should return None after delete"
    );

    // BUG: contains() still returns true because it doesn't check tombstones
    assert!(
        store.contains(&key),
        "BUG CONFIRMED: contains() returns true for deleted key (tombstone not filtered)"
    );
}

#[test]
fn issue_859_contains_should_be_consistent_with_get() {
    let store = Arc::new(ShardedStore::new());
    let ns = test_ns();
    let key = test_key(&ns, "consistency_test");

    // For a non-existent key, both should agree
    assert!(
        !store.contains(&key),
        "contains() should be false for non-existent key"
    );
    assert!(
        Storage::get(&*store, &key).unwrap().is_none(),
        "get() should be None for non-existent key"
    );

    // After put, both should agree
    Storage::put(
        &*store,
        key.clone(),
        Value::String("hello".to_string()),
        None,
    )
    .unwrap();
    assert!(store.contains(&key), "contains() should be true after put");
    assert!(
        Storage::get(&*store, &key).unwrap().is_some(),
        "get() should return Some after put"
    );

    // After delete, they should still agree -- but they don't due to the bug
    Storage::delete(&*store, &key).unwrap();
    let get_result = Storage::get(&*store, &key).unwrap();
    let contains_result = store.contains(&key);

    // BUG: get() returns None but contains() returns true
    assert!(
        get_result.is_none(),
        "get() correctly returns None after delete"
    );
    assert!(
        contains_result,
        "BUG CONFIRMED: contains() returns true after delete (inconsistent with get)"
    );
}
