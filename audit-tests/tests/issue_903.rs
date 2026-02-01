//! Audit test for issue #903: Snapshot version not protected from GC reclaim
//! Verdict: CONFIRMED BUG
//!
//! ShardedSnapshot holds a version number but there is no mechanism to prevent
//! VersionChain::gc() from reclaiming versions that active snapshots depend on.
//! If GC runs with min_version > snapshot.version, the snapshot will return
//! incorrect results.
//!
//! The gc() method is public on VersionChain. Any internal compaction or GC
//! process that calls VersionChain::gc() has no way to know about active
//! snapshots, because ShardedStore has no snapshot registry or version pinning.
//!
//! This test demonstrates the bug directly via VersionChain::gc().

use std::sync::Arc;
use strata_core::types::{BranchId, Key, Namespace, TypeTag};
use strata_core::value::Value;
use strata_core::Version;
use strata_storage::sharded::{ShardedStore, VersionChain};
use strata_storage::stored_value::StoredValue;

fn make_key(branch_id: BranchId, name: &[u8]) -> Key {
    let ns = Namespace::for_branch(branch_id);
    Key::new(ns, TypeTag::KV, name.to_vec())
}

/// Demonstrates that ShardedSnapshot holds a version but nothing prevents
/// the version chain from being truncated beneath it.
///
/// Uses apply_batch to properly advance the store version.
#[test]
fn issue_903_snapshot_has_no_version_pinning() {
    let store = Arc::new(ShardedStore::new());
    let branch_id = BranchId::new();
    let key = make_key(branch_id, b"test_key");

    // Step 1: Insert value at version 1 using apply_batch (advances store version)
    store
        .apply_batch(
            &[(key.clone(), Value::String("original".to_string()))],
            &[],
            1,
        )
        .unwrap();

    // Step 2: Take snapshot -- captures current version (1)
    let snapshot = store.snapshot();
    let snap_version = snapshot.version();
    assert_eq!(snap_version, 1, "Snapshot should capture version 1");

    // Verify snapshot sees the value
    {
        use strata_core::traits::SnapshotView;
        let result = snapshot.get(&key).unwrap();
        assert!(
            result.is_some(),
            "Snapshot at version {} should see the key",
            snap_version
        );
        assert_eq!(result.unwrap().value, Value::String("original".to_string()));
    }

    // Step 3: Write many new versions to create a deep version chain
    for v in 2..=10u64 {
        store
            .apply_batch(&[(key.clone(), Value::String(format!("v{}", v)))], &[], v)
            .unwrap();
    }

    // Step 4: Snapshot should still see version 1 data (no GC yet)
    {
        use strata_core::traits::SnapshotView;
        let result = snapshot.get(&key).unwrap();
        assert!(
            result.is_some(),
            "Snapshot should still see its version after new writes (no GC yet)"
        );
        assert_eq!(
            result.unwrap().value,
            Value::String("original".to_string()),
            "Snapshot at version 1 should see 'original', not a newer value"
        );
    }

    // Step 5: The snapshot just holds a version number and an Arc<ShardedStore>.
    // There is no Drop impl that unregisters anything from a snapshot registry.
    // If gc() were called on the version chains, the snapshot would break.
    let snapshot2 = store.snapshot();
    drop(snapshot2);
    // Dropping does nothing to any registry -- there is no registry to update.
}

/// Directly demonstrates that VersionChain::gc() removes versions needed by
/// snapshots, confirming the bug.
///
/// VersionChain::gc() is a public method. Any code that calls it (e.g., a
/// background compaction thread) has no way to know about active snapshots
/// because ShardedStore maintains no snapshot registry.
#[test]
fn issue_903_version_chain_gc_removes_old_versions() {
    let mut chain = VersionChain::new(StoredValue::new(
        Value::String("v1".to_string()),
        Version::txn(1),
        None,
    ));

    // Add more versions
    chain.push(StoredValue::new(
        Value::String("v2".to_string()),
        Version::txn(2),
        None,
    ));
    chain.push(StoredValue::new(
        Value::String("v3".to_string()),
        Version::txn(3),
        None,
    ));

    // Verify version 1 is accessible (simulates a snapshot at version 1)
    let v1 = chain.get_at_version(1);
    assert!(v1.is_some(), "Version 1 should be accessible before GC");
    assert_eq!(*v1.unwrap().value(), Value::String("v1".to_string()));

    // Run GC with min_version=3 -- this removes all versions < 3
    chain.gc(3);

    // Version 1 is now gone -- any snapshot at version 1 would get wrong results
    let v1_after = chain.get_at_version(1);
    assert!(
        v1_after.is_none(),
        "After gc(3), version 1 should be removed from the chain. \
         Any snapshot depending on version 1 would now return incorrect results. \
         This confirms the bug: gc() has no awareness of active snapshots."
    );

    // Version 2 should also be gone
    let v2_after = chain.get_at_version(2);
    assert!(
        v2_after.is_none(),
        "After gc(3), version 2 should also be removed."
    );

    // Version 3 should still be accessible
    let v3 = chain.get_at_version(3);
    assert!(v3.is_some());
    assert_eq!(*v3.unwrap().value(), Value::String("v3".to_string()));
}

/// Demonstrates that a snapshot at version N, after GC(N+1), returns
/// wrong data -- it gets a newer version instead of None.
#[test]
fn issue_903_gc_causes_snapshot_to_see_wrong_version() {
    let mut chain = VersionChain::new(StoredValue::new(
        Value::String("old_data".to_string()),
        Version::txn(5),
        None,
    ));
    chain.push(StoredValue::new(
        Value::String("new_data".to_string()),
        Version::txn(10),
        None,
    ));

    // A snapshot at version 7 should see version 5's data
    let before_gc = chain.get_at_version(7);
    assert!(before_gc.is_some());
    assert_eq!(
        *before_gc.unwrap().value(),
        Value::String("old_data".to_string()),
        "Before GC, snapshot at version 7 correctly sees version 5's data"
    );

    // GC removes version 5 (< min_version 10)
    chain.gc(10);

    // Now a snapshot at version 7 gets None -- the data it depended on is gone
    let after_gc = chain.get_at_version(7);
    assert!(
        after_gc.is_none(),
        "After gc(10), snapshot at version 7 returns None because version 5 was removed. \
         This is the bug: the snapshot's data was reclaimed without its knowledge."
    );
}
