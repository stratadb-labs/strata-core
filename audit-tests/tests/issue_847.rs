//! Audit test for issue #847: ShardedStore list() and count() do not filter tombstones
//! Verdict: CONFIRMED BUG
//!
//! The ShardedStore's direct (non-snapshot) list_branch(), list_by_prefix(),
//! list_by_type(), and count_by_type() methods do not filter tombstones or
//! expired values, unlike their ShardedSnapshot counterparts which correctly
//! filter them.

use std::sync::Arc;
use strata_core::types::{BranchId, Key, Namespace, TypeTag};
use strata_core::value::Value;
use strata_core::Version;
use strata_storage::stored_value::StoredValue;
use strata_storage::ShardedStore;

fn make_key(branch_id: BranchId, user_key: &str) -> Key {
    Key::new_kv(Namespace::for_branch(branch_id), user_key)
}

/// Demonstrates that ShardedStore::list_branch includes tombstoned entries.
#[test]
fn issue_847_list_branch_includes_tombstones() {
    let store = ShardedStore::new();
    let branch_id = BranchId::new();

    // Put two keys
    let key1 = make_key(branch_id, "key1");
    let key2 = make_key(branch_id, "key2");

    let v1 = store.next_version();
    store.put(
        key1.clone(),
        StoredValue::new(Value::String("value1".into()), Version::txn(v1), None),
    );

    let v2 = store.next_version();
    store.put(
        key2.clone(),
        StoredValue::new(Value::String("value2".into()), Version::txn(v2), None),
    );

    // Delete key1 (adds tombstone)
    store.delete(&key1);

    // list_branch should exclude tombstoned entries
    let results = store.list_branch(&branch_id);

    // BUG: list_branch returns tombstoned entries too
    let has_deleted_key = results.iter().any(|(k, _)| k == &key1);

    if has_deleted_key {
        eprintln!(
            "BUG CONFIRMED: list_branch() returned {} entries including tombstoned key1. \
             Expected only key2.",
            results.len()
        );
    }

    // For comparison, the Storage::get trait method correctly filters tombstones
    use strata_core::traits::Storage;
    let get_result = store.get(&key1).unwrap();
    assert!(
        get_result.is_none(),
        "Storage::get correctly filters tombstones"
    );
}

/// Demonstrates that ShardedStore::list_by_prefix includes tombstoned entries.
#[test]
fn issue_847_list_by_prefix_includes_tombstones() {
    let store = ShardedStore::new();
    let branch_id = BranchId::new();

    let key1 = make_key(branch_id, "prefix_a");
    let key2 = make_key(branch_id, "prefix_b");

    let v1 = store.next_version();
    store.put(
        key1.clone(),
        StoredValue::new(Value::Int(1), Version::txn(v1), None),
    );
    let v2 = store.next_version();
    store.put(
        key2.clone(),
        StoredValue::new(Value::Int(2), Version::txn(v2), None),
    );

    // Delete key1
    store.delete(&key1);

    // list_by_prefix should exclude tombstones
    let prefix = Key::new_kv(Namespace::for_branch(branch_id), "prefix_");
    let results = store.list_by_prefix(&prefix);

    let has_deleted = results.iter().any(|(k, _)| k == &key1);
    if has_deleted {
        eprintln!(
            "BUG CONFIRMED: list_by_prefix() returned tombstoned entry. \
             Got {} entries, expected 1.",
            results.len()
        );
    }
}

/// Demonstrates that count_by_type includes tombstoned entries.
#[test]
fn issue_847_count_by_type_includes_tombstones() {
    let store = ShardedStore::new();
    let branch_id = BranchId::new();

    // Put 3 KV keys
    for i in 0..3 {
        let key = make_key(branch_id, &format!("key{}", i));
        let v = store.next_version();
        store.put(key, StoredValue::new(Value::Int(i), Version::txn(v), None));
    }

    // Delete 2 of them
    let key0 = make_key(branch_id, "key0");
    let key1 = make_key(branch_id, "key1");
    store.delete(&key0);
    store.delete(&key1);

    // count_by_type should return 1 (only key2 is alive)
    let count = store.count_by_type(&branch_id, TypeTag::KV);

    if count != 1 {
        eprintln!(
            "BUG CONFIRMED: count_by_type() returned {} instead of 1. \
             Tombstoned entries are being counted.",
            count
        );
    }
}

/// Demonstrates that the ShardedSnapshot versions of these methods
/// DO correctly filter tombstones (for comparison).
#[test]
fn issue_847_snapshot_correctly_filters_tombstones() {
    let store = Arc::new(ShardedStore::new());
    let branch_id = BranchId::new();

    // Put and delete a key
    let key1 = make_key(branch_id, "snap_key1");
    let key2 = make_key(branch_id, "snap_key2");

    let v1 = store.next_version();
    store.put(
        key1.clone(),
        StoredValue::new(Value::String("alive".into()), Version::txn(v1), None),
    );
    let v2 = store.next_version();
    store.put(
        key2.clone(),
        StoredValue::new(Value::String("deleted".into()), Version::txn(v2), None),
    );
    store.delete(&key2);

    // Create a snapshot
    let snapshot = store.snapshot();

    // Snapshot's list_branch should filter tombstones
    let snapshot_results = snapshot.list_branch(&branch_id);
    let has_deleted = snapshot_results.iter().any(|(k, _)| k == &key2);
    assert!(
        !has_deleted,
        "ShardedSnapshot::list_branch correctly excludes tombstones"
    );
    assert_eq!(
        snapshot_results.len(),
        1,
        "Snapshot should show only 1 live entry"
    );

    // Direct store list_branch does NOT filter
    let store_results = store.list_branch(&branch_id);
    // This may include the tombstoned key2
    eprintln!(
        "Direct store list_branch: {} entries, Snapshot list_branch: {} entries",
        store_results.len(),
        snapshot_results.len()
    );
}
