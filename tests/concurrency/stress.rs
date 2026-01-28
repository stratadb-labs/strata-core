//! Stress Tests
//!
//! Heavy-workload tests for concurrency. All marked #[ignore] for opt-in execution.
//! Run with: cargo test --test concurrency stress -- --ignored

use strata_concurrency::manager::TransactionManager;
use strata_concurrency::transaction::TransactionContext;
use strata_concurrency::validation::validate_transaction;
use strata_core::traits::Storage;
use strata_core::types::{Key, Namespace};
use strata_core::value::Value;
use strata_core::RunId;
use strata_storage::sharded::ShardedStore;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Barrier};
use std::thread;
use std::time::{Duration, Instant};

fn create_test_key(run_id: RunId, name: &str) -> Key {
    let ns = Namespace::for_run(run_id);
    Key::new_kv(ns, name)
}

/// High concurrency read-write mix
#[test]
#[ignore]
fn stress_concurrent_read_write() {
    let store = Arc::new(ShardedStore::new());
    let manager = Arc::new(TransactionManager::new(1));
    let run_id = RunId::new();

    // Pre-populate
    for i in 0..100 {
        let key = create_test_key(run_id, &format!("key_{}", i));
        Storage::put(&*store, key, Value::Int(i), None).unwrap();
    }

    let barrier = Arc::new(Barrier::new(16));
    let commits = Arc::new(AtomicU64::new(0));
    let conflicts = Arc::new(AtomicU64::new(0));

    let handles: Vec<_> = (0..16)
        .map(|thread_id| {
            let store = Arc::clone(&store);
            let manager = Arc::clone(&manager);
            let barrier = Arc::clone(&barrier);
            let commits = Arc::clone(&commits);
            let conflicts = Arc::clone(&conflicts);

            thread::spawn(move || {
                barrier.wait();

                for iter in 0..100 {
                    // Pick a random key based on thread and iteration
                    let key_idx = (thread_id * 7 + iter * 11) % 100;
                    let key = create_test_key(run_id, &format!("key_{}", key_idx));

                    // Read-modify-write
                    let current = Storage::get(&*store, &key).unwrap().unwrap();
                    let version = current.version.as_u64();

                    let txn_id = manager.next_txn_id();
                    let mut txn = TransactionContext::new(txn_id, run_id, version);
                    txn.read_set.insert(key.clone(), version);
                    txn.write_set
                        .insert(key.clone(), Value::Int((thread_id * 1000 + iter) as i64));

                    let result = validate_transaction(&txn, &*store);
                    if result.is_valid() {
                        Storage::put(
                            &*store,
                            key,
                            Value::Int((thread_id * 1000 + iter) as i64),
                            None,
                        )
                        .unwrap();
                        commits.fetch_add(1, Ordering::Relaxed);
                    } else {
                        conflicts.fetch_add(1, Ordering::Relaxed);
                    }
                }
            })
        })
        .collect();

    for handle in handles {
        handle.join().unwrap();
    }

    let total_commits = commits.load(Ordering::Relaxed);
    let total_conflicts = conflicts.load(Ordering::Relaxed);

    println!(
        "Commits: {}, Conflicts: {}, Total: {}",
        total_commits,
        total_conflicts,
        total_commits + total_conflicts
    );

    assert_eq!(total_commits + total_conflicts, 16 * 100);
    assert!(total_commits > 0, "Some transactions should commit");
}

/// Rapid transaction throughput measurement
#[test]
#[ignore]
fn stress_transaction_throughput() {
    let store = Arc::new(ShardedStore::new());
    let manager = TransactionManager::new(1);
    let run_id = RunId::new();

    let key = create_test_key(run_id, "counter");
    Storage::put(&*store, key.clone(), Value::Int(0), None).unwrap();

    let duration = Duration::from_secs(5);
    let start = Instant::now();
    let mut commits = 0u64;
    let mut conflicts = 0u64;

    while start.elapsed() < duration {
        let current = Storage::get(&*store, &key).unwrap().unwrap();
        let version = current.version.as_u64();

        let txn_id = manager.next_txn_id();
        let mut txn = TransactionContext::new(txn_id, run_id, version);
        txn.read_set.insert(key.clone(), version);

        if let Value::Int(v) = current.value {
            txn.write_set.insert(key.clone(), Value::Int(v + 1));
        }

        let result = validate_transaction(&txn, &*store);
        if result.is_valid() {
            if let Value::Int(v) = current.value {
                Storage::put(&*store, key.clone(), Value::Int(v + 1), None).unwrap();
            }
            commits += 1;
        } else {
            conflicts += 1;
        }
    }

    let elapsed = start.elapsed();
    let tps = commits as f64 / elapsed.as_secs_f64();

    println!(
        "Commits: {}, Conflicts: {}, TPS: {:.0}",
        commits, conflicts, tps
    );

    // Single-threaded, conflicts should be minimal
    assert!(conflicts < 10, "Single-threaded should have minimal conflicts");
}

/// Large transaction with many operations
#[test]
#[ignore]
fn stress_large_transaction() {
    let store = Arc::new(ShardedStore::new());
    let run_id = RunId::new();

    // Create transaction with 10K operations
    let mut txn = TransactionContext::new(1, run_id, 1);

    let start = Instant::now();

    // Add 10K writes
    for i in 0..10_000 {
        let key = create_test_key(run_id, &format!("large_key_{}", i));
        txn.write_set.insert(key, Value::Int(i));
    }

    let build_time = start.elapsed();

    // Validate
    let validate_start = Instant::now();
    let result = validate_transaction(&txn, &*store);
    let validate_time = validate_start.elapsed();

    println!(
        "Build time: {:?}, Validate time: {:?}, Operations: {}",
        build_time,
        validate_time,
        txn.pending_operations().puts
    );

    assert!(result.is_valid());
    assert_eq!(txn.pending_operations().puts, 10_000);
}

/// Many concurrent transactions on different runs
#[test]
#[ignore]
fn stress_many_runs() {
    let store = Arc::new(ShardedStore::new());
    let manager = Arc::new(TransactionManager::new(1));
    let barrier = Arc::new(Barrier::new(100));
    let commits = Arc::new(AtomicU64::new(0));

    let handles: Vec<_> = (0..100)
        .map(|_| {
            let store = Arc::clone(&store);
            let manager = Arc::clone(&manager);
            let barrier = Arc::clone(&barrier);
            let commits = Arc::clone(&commits);

            thread::spawn(move || {
                let run_id = RunId::new(); // Each thread gets unique run
                let key = create_test_key(run_id, "data");

                barrier.wait();

                for i in 0..100 {
                    let txn_id = manager.next_txn_id();
                    let mut txn = TransactionContext::new(txn_id, run_id, 1);
                    txn.write_set.insert(key.clone(), Value::Int(i));

                    let result = validate_transaction(&txn, &*store);
                    if result.is_valid() {
                        Storage::put(&*store, key.clone(), Value::Int(i), None).unwrap();
                        commits.fetch_add(1, Ordering::Relaxed);
                    }
                }
            })
        })
        .collect();

    for handle in handles {
        handle.join().unwrap();
    }

    let total = commits.load(Ordering::Relaxed);
    println!("Total commits across 100 runs: {}", total);

    // All should commit (no cross-run contention)
    assert_eq!(total, 100 * 100);
}

/// Long-running transaction with concurrent modifications
#[test]
#[ignore]
fn stress_long_running_transaction() {
    let store = Arc::new(ShardedStore::new());
    let run_id = RunId::new();
    let key = create_test_key(run_id, "contested");

    // Initial value
    Storage::put(&*store, key.clone(), Value::Int(0), None).unwrap();
    let initial_version = Storage::get(&*store, &key).unwrap().unwrap().version.as_u64();

    // Start a long-running transaction
    let mut long_txn = TransactionContext::new(1, run_id, initial_version);
    long_txn.read_set.insert(key.clone(), initial_version);

    // Spawn concurrent writers
    let store_clone = Arc::clone(&store);
    let key_clone = key.clone();
    let writer = thread::spawn(move || {
        for i in 1..=100 {
            Storage::put(&*store_clone, key_clone.clone(), Value::Int(i), None).unwrap();
            thread::sleep(Duration::from_millis(1));
        }
    });

    // Simulate long work
    thread::sleep(Duration::from_millis(50));

    // Long transaction tries to commit
    long_txn.write_set.insert(key.clone(), Value::Int(999));
    let result = validate_transaction(&long_txn, &*store);

    writer.join().unwrap();

    // Should conflict due to concurrent modifications
    assert!(!result.is_valid(), "Long-running transaction should conflict");
}

/// Sustained mixed workload
#[test]
#[ignore]
fn stress_sustained_workload() {
    let store = Arc::new(ShardedStore::new());
    let manager = Arc::new(TransactionManager::new(1));
    let run_id = RunId::new();

    // Pre-populate
    for i in 0..50 {
        let key = create_test_key(run_id, &format!("key_{}", i));
        Storage::put(&*store, key, Value::Int(i), None).unwrap();
    }

    let duration = Duration::from_secs(10);
    let start = Instant::now();
    let barrier = Arc::new(Barrier::new(8));
    let ops = Arc::new(AtomicU64::new(0));

    let handles: Vec<_> = (0..8)
        .map(|thread_id| {
            let store = Arc::clone(&store);
            let manager = Arc::clone(&manager);
            let barrier = Arc::clone(&barrier);
            let ops = Arc::clone(&ops);

            thread::spawn(move || {
                barrier.wait();
                let local_start = Instant::now();

                while local_start.elapsed() < duration {
                    // Mix of reads and writes
                    let key_idx = (thread_id * 13 + ops.load(Ordering::Relaxed) as usize * 7) % 50;
                    let key = create_test_key(run_id, &format!("key_{}", key_idx));

                    if ops.load(Ordering::Relaxed) % 3 == 0 {
                        // Write
                        let current = Storage::get(&*store, &key).unwrap().unwrap();
                        let version = current.version.as_u64();

                        let txn_id = manager.next_txn_id();
                        let mut txn = TransactionContext::new(txn_id, run_id, version);
                        txn.read_set.insert(key.clone(), version);
                        txn.write_set.insert(key.clone(), Value::Int(thread_id as i64));

                        let result = validate_transaction(&txn, &*store);
                        if result.is_valid() {
                            Storage::put(&*store, key, Value::Int(thread_id as i64), None).unwrap();
                        }
                    } else {
                        // Read
                        let _ = Storage::get(&*store, &key);
                    }

                    ops.fetch_add(1, Ordering::Relaxed);
                }
            })
        })
        .collect();

    for handle in handles {
        handle.join().unwrap();
    }

    let elapsed = start.elapsed();
    let total_ops = ops.load(Ordering::Relaxed);
    let ops_per_sec = total_ops as f64 / elapsed.as_secs_f64();

    println!(
        "Sustained workload: {} ops in {:?} ({:.0} ops/sec)",
        total_ops, elapsed, ops_per_sec
    );
}
