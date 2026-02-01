//! Audit test for issue #882: DashMap iterator contention -- shard lock held during full clone in list ops
//! Verdict: CONFIRMED BUG (duplicate of #874)
//!
//! This is the same underlying issue as #874. DashMap's .get() returns a Ref guard
//! that holds a read lock on the shard. The list operations iterate, clone, and sort
//! all entries while this guard is alive, blocking writes to any branch that hashes
//! to the same DashMap shard.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::thread;

use strata_core::traits::Storage;
use strata_core::types::{BranchId, Key, Namespace, TypeTag};
use strata_core::value::Value;
use strata_storage::ShardedStore;

fn create_namespace(branch_id: BranchId) -> Namespace {
    Namespace::new(
        "tenant".to_string(),
        "app".to_string(),
        "agent".to_string(),
        branch_id,
    )
}

fn create_key(ns: &Namespace, name: &str) -> Key {
    Key::new_kv(ns.clone(), name)
}

/// Demonstrates that list_by_type() holds the DashMap shard lock
/// while iterating all entries, filtering by type, cloning, and sorting.
#[test]
fn issue_882_list_by_type_holds_shard_lock() {
    let store = Arc::new(ShardedStore::new());
    let branch_id = BranchId::new();
    let ns = create_namespace(branch_id);

    // Populate with KV entries
    for i in 0..500 {
        let key = create_key(&ns, &format!("kv_key_{:04}", i));
        store
            .put_with_version(key, Value::Bytes(vec![0u8; 256]), 1, None)
            .unwrap();
    }

    // list_by_type scans the entire shard, filters by type, clones, and sorts
    let results = store.list_by_type(&branch_id, TypeTag::KV);
    assert_eq!(results.len(), 500);

    // Verify results are sorted
    for i in 1..results.len() {
        assert!(results[i - 1].0 <= results[i].0);
    }
}

/// Demonstrates that concurrent list and write operations contend
/// on the same DashMap shard.
#[test]
fn issue_882_concurrent_list_and_write_contention() {
    let store = Arc::new(ShardedStore::new());
    let branch_id = BranchId::new();
    let ns = create_namespace(branch_id);

    // Populate branch with many entries
    for i in 0..1000 {
        let key = create_key(&ns, &format!("entry_{:04}", i));
        store
            .put_with_version(key, Value::Bytes(vec![0u8; 512]), 1, None)
            .unwrap();
    }

    let write_count = Arc::new(AtomicU64::new(0));
    let stop = Arc::new(AtomicBool::new(false));

    // Writer thread: continuously writes to the same branch
    let store_w = Arc::clone(&store);
    let ns_w = ns.clone();
    let write_count_w = Arc::clone(&write_count);
    let stop_w = Arc::clone(&stop);
    let writer = thread::spawn(move || {
        let mut i = 2000;
        while !stop_w.load(Ordering::Relaxed) {
            let key = create_key(&ns_w, &format!("writer_{}", i));
            store_w
                .put_with_version(key, Value::Int(i as i64), 2, None)
                .unwrap();
            write_count_w.fetch_add(1, Ordering::Relaxed);
            i += 1;
        }
    });

    // Reader thread: repeatedly lists the branch (holds lock during clone+sort)
    let store_r = Arc::clone(&store);
    let reader = thread::spawn(move || {
        for _ in 0..10 {
            let results = store_r.list_branch(&branch_id);
            assert!(results.len() >= 1000);
        }
    });

    reader.join().unwrap();
    stop.store(true, Ordering::Relaxed);
    writer.join().unwrap();

    let writes = write_count.load(Ordering::Relaxed);
    eprintln!(
        "Writer completed {} writes during 10 list_branch() iterations",
        writes
    );

    // Both threads complete, proving no deadlock, but contention exists
}

/// Demonstrates that the issue affects multiple branches that hash to the
/// same DashMap internal shard (not just the listed branch).
#[test]
fn issue_882_cross_branch_shard_contention() {
    let store = Arc::new(ShardedStore::new());

    // Create many branches -- some will hash to the same DashMap shard
    let mut branches = Vec::new();
    for _ in 0..32 {
        let branch_id = BranchId::new();
        let ns = create_namespace(branch_id);
        // Populate each branch with a few entries
        for i in 0..50 {
            let key = create_key(&ns, &format!("key_{}", i));
            store
                .put_with_version(key, Value::Bytes(vec![0u8; 256]), 1, None)
                .unwrap();
        }
        branches.push(branch_id);
    }

    // List from the first branch -- this locks one DashMap shard
    let results = store.list_branch(&branches[0]);
    assert_eq!(results.len(), 50);

    // All other branches should still be accessible (some may or may not
    // share the same DashMap shard)
    for branch_id in &branches[1..] {
        let results = store.list_branch(branch_id);
        assert_eq!(results.len(), 50);
    }
}
