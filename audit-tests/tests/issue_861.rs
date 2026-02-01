//! Audit test for issue #861: TTLIndex is disconnected — entries never expire
//! Verdict: CONFIRMED BUG
//!
//! TTLIndex provides insert/find_expired/remove_expired methods but is never
//! instantiated or used by ShardedStore or the engine. Expired keys are only
//! filtered at read time; memory is never reclaimed.

use std::sync::Arc;
use std::time::Duration;
use strata_core::contract::Timestamp;
use strata_core::traits::Storage;
use strata_core::types::{BranchId, Key, Namespace};
use strata_core::value::Value;
use strata_storage::{ShardedStore, TTLIndex};

fn test_ns() -> Namespace {
    let branch_id = BranchId::new();
    Namespace::for_branch(branch_id)
}

fn test_key(ns: &Namespace, name: &str) -> Key {
    Key::new_kv(ns.clone(), name)
}

#[test]
fn issue_861_ttl_index_is_functional_but_unused() {
    // Verify TTLIndex works as a data structure
    let mut index = TTLIndex::new();
    let ns = test_ns();
    let key = test_key(&ns, "expiring_key");

    // Insert a key that "expires" at timestamp 1000
    index.insert(Timestamp::from_micros(1000), key.clone());

    // Find expired at timestamp 2000
    let expired = index.find_expired(Timestamp::from_micros(2000));
    assert_eq!(expired.len(), 1);
    assert_eq!(expired[0], key);

    // The TTLIndex works perfectly as a standalone data structure.
    // The bug is that it's never connected to ShardedStore.
}

#[test]
fn issue_861_expired_keys_not_cleaned_from_memory() {
    let store = Arc::new(ShardedStore::new());
    let ns = test_ns();
    let branch_id = ns.branch_id;

    // Store a value with a very short TTL (1ms)
    let key = test_key(&ns, "short_lived");
    Storage::put(
        &*store,
        key.clone(),
        Value::String("ephemeral".to_string()),
        Some(Duration::from_millis(1)),
    )
    .unwrap();

    // Wait for TTL to expire
    std::thread::sleep(Duration::from_millis(50));

    // get() correctly filters the expired value
    let get_result = Storage::get(&*store, &key).unwrap();
    assert!(
        get_result.is_none(),
        "get() should return None for expired key"
    );

    // But the entry is still in memory (total_entries counts it)
    let total = store.total_entries();
    assert!(
        total > 0,
        "BUG CONFIRMED: Expired entry still occupies memory (total_entries = {})",
        total
    );

    // The entry count for this branch still includes the expired key
    let branch_count = store.branch_entry_count(&branch_id);
    assert!(
        branch_count > 0,
        "BUG CONFIRMED: Expired entry still in branch shard (count = {})",
        branch_count
    );

    // There is no mechanism to clean up expired entries.
    // TTLIndex::find_expired() exists but is never called by the storage layer.
}

#[test]
fn issue_861_no_proactive_cleanup_mechanism() {
    let store = Arc::new(ShardedStore::new());
    let ns = test_ns();

    // Insert multiple keys with short TTL
    for i in 0..10 {
        let key = test_key(&ns, &format!("key_{}", i));
        Storage::put(&*store, key, Value::Int(i), Some(Duration::from_millis(1))).unwrap();
    }

    // Wait for all TTLs to expire
    std::thread::sleep(Duration::from_millis(50));

    // All 10 keys should be expired (not returned by get)
    for i in 0..10 {
        let key = test_key(&ns, &format!("key_{}", i));
        assert!(
            Storage::get(&*store, &key).unwrap().is_none(),
            "Key {} should be expired",
            i
        );
    }

    // But all 10 are still in memory
    assert_eq!(
        store.total_entries(),
        10,
        "BUG CONFIRMED: All 10 expired entries still in memory, no cleanup mechanism exists"
    );
}
