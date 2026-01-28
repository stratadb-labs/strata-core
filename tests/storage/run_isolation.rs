//! Run Isolation Tests
//!
//! Tests that different runs are properly isolated in the storage layer.

use strata_core::traits::Storage;
use strata_core::types::{Key, Namespace};
use strata_core::value::Value;
use strata_core::RunId;
use strata_storage::sharded::ShardedStore;
use std::sync::Arc;
use std::thread;

fn create_test_key(run_id: RunId, name: &str) -> Key {
    let ns = Namespace::for_run(run_id);
    Key::new_kv(ns, name)
}

// ============================================================================
// Basic Isolation
// ============================================================================

#[test]
fn different_runs_have_separate_namespaces() {
    let store = ShardedStore::new();
    let run1 = RunId::new();
    let run2 = RunId::new();

    // Same key name, different runs
    let key1 = create_test_key(run1, "shared_name");
    let key2 = create_test_key(run2, "shared_name");

    Storage::put(&store, key1.clone(), Value::Int(100), None).unwrap();
    Storage::put(&store, key2.clone(), Value::Int(200), None).unwrap();

    let val1 = Storage::get(&store, &key1).unwrap().unwrap().value;
    let val2 = Storage::get(&store, &key2).unwrap().unwrap().value;

    assert_eq!(val1, Value::Int(100));
    assert_eq!(val2, Value::Int(200));
}

#[test]
fn clear_run_only_affects_target_run() {
    let store = ShardedStore::new();
    let run1 = RunId::new();
    let run2 = RunId::new();

    // Put keys in both runs
    for i in 0..5 {
        let key1 = create_test_key(run1, &format!("key_{}", i));
        let key2 = create_test_key(run2, &format!("key_{}", i));
        Storage::put(&store, key1, Value::Int(i), None).unwrap();
        Storage::put(&store, key2, Value::Int(i + 100), None).unwrap();
    }

    // Clear run1
    store.clear_run(&run1);

    // Run1 should be empty
    for i in 0..5 {
        let key1 = create_test_key(run1, &format!("key_{}", i));
        assert!(Storage::get(&store, &key1).unwrap().is_none());
    }

    // Run2 should still have data
    for i in 0..5 {
        let key2 = create_test_key(run2, &format!("key_{}", i));
        let val = Storage::get(&store, &key2).unwrap();
        assert!(val.is_some());
        assert_eq!(val.unwrap().value, Value::Int(i + 100));
    }
}

#[test]
fn delete_in_one_run_doesnt_affect_other() {
    let store = ShardedStore::new();
    let run1 = RunId::new();
    let run2 = RunId::new();

    let key1 = create_test_key(run1, "shared");
    let key2 = create_test_key(run2, "shared");

    Storage::put(&store, key1.clone(), Value::Int(1), None).unwrap();
    Storage::put(&store, key2.clone(), Value::Int(2), None).unwrap();

    // Delete in run1
    Storage::delete(&store, &key1).unwrap();

    // Run1 deleted
    assert!(Storage::get(&store, &key1).unwrap().is_none());

    // Run2 unaffected
    assert_eq!(
        Storage::get(&store, &key2).unwrap().unwrap().value,
        Value::Int(2)
    );
}

// ============================================================================
// Concurrent Access Across Runs
// ============================================================================

#[test]
fn concurrent_writes_to_different_runs() {
    let store = Arc::new(ShardedStore::new());
    let num_runs = 8;
    let keys_per_run = 100;

    let handles: Vec<_> = (0..num_runs)
        .map(|_| {
            let store = Arc::clone(&store);
            thread::spawn(move || {
                let run_id = RunId::new();
                for i in 0..keys_per_run {
                    let key = create_test_key(run_id, &format!("key_{}", i));
                    Storage::put(&*store, key, Value::Int(i), None).unwrap();
                }
                run_id
            })
        })
        .collect();

    let run_ids: Vec<RunId> = handles.into_iter().map(|h| h.join().unwrap()).collect();

    // Verify all runs have their data
    for run_id in run_ids {
        for i in 0..keys_per_run {
            let key = create_test_key(run_id, &format!("key_{}", i));
            let val = Storage::get(&*store, &key).unwrap();
            assert!(val.is_some(), "Run {:?} key {} missing", run_id, i);
            assert_eq!(val.unwrap().value, Value::Int(i));
        }
    }
}

#[test]
fn concurrent_reads_and_writes_different_runs() {
    let store = Arc::new(ShardedStore::new());
    let read_run = RunId::new();
    let write_run = RunId::new();

    // Pre-populate read run
    for i in 0..100 {
        let key = create_test_key(read_run, &format!("key_{}", i));
        Storage::put(&*store, key, Value::Int(i), None).unwrap();
    }

    let store_read = Arc::clone(&store);
    let store_write = Arc::clone(&store);

    // Reader thread
    let reader = thread::spawn(move || {
        let mut reads = 0u64;
        for _ in 0..1000 {
            for i in 0..100 {
                let key = create_test_key(read_run, &format!("key_{}", i));
                let val = Storage::get(&*store_read, &key).unwrap();
                assert!(val.is_some());
                assert_eq!(val.unwrap().value, Value::Int(i));
                reads += 1;
            }
        }
        reads
    });

    // Writer thread (different run)
    let writer = thread::spawn(move || {
        for i in 0..1000 {
            for j in 0..10 {
                let key = create_test_key(write_run, &format!("key_{}", j));
                Storage::put(&*store_write, key, Value::Int(i), None).unwrap();
            }
        }
    });

    let reads = reader.join().unwrap();
    writer.join().unwrap();

    assert_eq!(reads, 1000 * 100);
}

// ============================================================================
// Run Listing
// ============================================================================

#[test]
fn run_ids_lists_all_active_runs() {
    let store = ShardedStore::new();
    let run1 = RunId::new();
    let run2 = RunId::new();
    let run3 = RunId::new();

    // Put one key in each run
    let key1 = create_test_key(run1, "k");
    let key2 = create_test_key(run2, "k");
    let key3 = create_test_key(run3, "k");

    Storage::put(&store, key1, Value::Int(1), None).unwrap();
    Storage::put(&store, key2, Value::Int(2), None).unwrap();
    Storage::put(&store, key3, Value::Int(3), None).unwrap();

    let runs = store.run_ids();
    assert_eq!(runs.len(), 3);
    assert!(runs.contains(&run1));
    assert!(runs.contains(&run2));
    assert!(runs.contains(&run3));
}

#[test]
fn run_entry_count() {
    let store = ShardedStore::new();
    let run_id = RunId::new();

    // Put 10 keys
    for i in 0..10 {
        let key = create_test_key(run_id, &format!("key_{}", i));
        Storage::put(&store, key, Value::Int(i), None).unwrap();
    }

    let count = store.run_entry_count(&run_id);
    assert_eq!(count, 10);
}

#[test]
fn list_run_keys() {
    let store = ShardedStore::new();
    let run_id = RunId::new();
    let ns = Namespace::for_run(run_id);

    // Put 5 keys
    for i in 0..5 {
        let key = Key::new_kv(ns.clone(), &format!("key_{}", i));
        Storage::put(&store, key, Value::Int(i), None).unwrap();
    }

    let keys = store.list_run(&run_id);
    assert_eq!(keys.len(), 5);
}

// ============================================================================
// Empty Run Handling
// ============================================================================

#[test]
fn get_from_nonexistent_run_returns_none() {
    let store = ShardedStore::new();
    let run_id = RunId::new();
    let key = create_test_key(run_id, "never_written");

    let result = Storage::get(&store, &key).unwrap();
    assert!(result.is_none());
}

#[test]
fn clear_nonexistent_run_succeeds() {
    let store = ShardedStore::new();
    let run_id = RunId::new();

    // Should not panic
    store.clear_run(&run_id);
}

#[test]
fn run_entry_count_for_empty_run() {
    let store = ShardedStore::new();
    let run_id = RunId::new();

    let count = store.run_entry_count(&run_id);
    assert_eq!(count, 0);
}
