//! Concurrent/Multi-threaded Tests for strata-concurrency
//!
//! These tests verify correct behavior under actual concurrent execution.
//! Unlike the sequential tests, these use multiple threads to exercise:
//!
//! 1. **TOCTOU Prevention** - The commit lock prevents race conditions
//! 2. **Concurrent Commits** - Multiple threads committing simultaneously
//! 3. **Version Monotonicity** - Versions always increase under load
//! 4. **First-Committer-Wins** - Conflict detection works with real races
//! 5. **Stress Testing** - High concurrency doesn't cause panics or corruption
//!
//! ## Running These Tests
//!
//! ```bash
//! cargo test --test concurrent_tests
//! cargo test --test concurrent_tests -- --nocapture --test-threads=1  # sequential for debugging
//! ```

use parking_lot::Mutex;
use std::collections::HashSet;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Barrier};
use std::thread;
use std::time::{Duration, Instant};

use strata_concurrency::{TransactionContext, TransactionManager, TransactionStatus};
use strata_core::traits::{SnapshotView, Storage};
use strata_core::types::{Key, Namespace, RunId};
use strata_core::value::Value;
use strata_durability::wal::{DurabilityMode, WAL};
use strata_storage::UnifiedStore;
use tempfile::TempDir;

// ============================================================================
// Test Helpers
// ============================================================================

fn create_namespace(run_id: RunId) -> Namespace {
    Namespace::new(
        "test_tenant".to_string(),
        "test_app".to_string(),
        "test_agent".to_string(),
        run_id,
    )
}

fn create_key(ns: &Namespace, name: &str) -> Key {
    Key::new_kv(ns.clone(), name)
}

/// Create a shared test environment for concurrent tests
/// NOTE: The TransactionManager is created AFTER the store, and should be
/// re-created after any initial data setup to ensure version synchronization.
fn create_shared_env() -> (Arc<UnifiedStore>, Arc<Mutex<WAL>>, TempDir) {
    let temp_dir = TempDir::new().unwrap();
    let wal_path = temp_dir.path().join("concurrent.wal");
    let wal = WAL::open(&wal_path, DurabilityMode::Strict).unwrap();
    let store = Arc::new(UnifiedStore::new());
    (store, Arc::new(Mutex::new(wal)), temp_dir)
}

/// Create a TransactionManager synchronized with the current store version
fn create_manager(store: &UnifiedStore) -> Arc<TransactionManager> {
    Arc::new(TransactionManager::new(store.current_version()))
}

// ============================================================================
// SECTION 1: TOCTOU Prevention Tests
// ============================================================================

mod toctou_prevention {
    use super::*;

    /// Test that the commit lock prevents the classic TOCTOU race:
    /// 1. T1 validates (passes)
    /// 2. T2 validates (passes) - same snapshot
    /// 3. T1 commits
    /// 4. T2 commits - should fail, not corrupt storage
    ///
    /// Without proper locking, both could commit and corrupt storage.
    #[test]
    fn test_commit_lock_prevents_toctou_race() {
        let (store, wal, _temp) = create_shared_env();
        let run_id = RunId::new();
        let ns = create_namespace(run_id);
        let key = create_key(&ns, "contested");

        // Setup initial value FIRST
        store.put(key.clone(), Value::Int(0), None).unwrap();

        // Create manager AFTER initial data to sync versions
        let manager = create_manager(&store);

        let barrier = Arc::new(Barrier::new(2));
        let success_count = Arc::new(AtomicUsize::new(0));
        let failure_count = Arc::new(AtomicUsize::new(0));

        let handles: Vec<_> = (0..2)
            .map(|i| {
                let manager = Arc::clone(&manager);
                let store = Arc::clone(&store);
                let wal = Arc::clone(&wal);
                let barrier = Arc::clone(&barrier);
                let success_count = Arc::clone(&success_count);
                let failure_count = Arc::clone(&failure_count);
                let key = key.clone();

                thread::spawn(move || {
                    // Create transaction - both see same snapshot
                    let snapshot = store.create_snapshot();
                    let mut txn = TransactionContext::with_snapshot(
                        manager.next_txn_id(),
                        run_id,
                        Box::new(snapshot),
                    );

                    // Both read the key (adds to read_set)
                    let _ = txn.get(&key).unwrap();
                    txn.put(key.clone(), Value::Int(i as i64 + 1)).unwrap();

                    // Synchronize - both transactions prepared at same time
                    barrier.wait();

                    // Both try to commit simultaneously
                    let mut wal_guard = wal.lock();
                    let result = manager.commit(&mut txn, store.as_ref(), &mut wal_guard);

                    if result.is_ok() {
                        success_count.fetch_add(1, Ordering::SeqCst);
                    } else {
                        failure_count.fetch_add(1, Ordering::SeqCst);
                    }
                })
            })
            .collect();

        for handle in handles {
            handle.join().unwrap();
        }

        let final_success = success_count.load(Ordering::SeqCst);
        let final_failure = failure_count.load(Ordering::SeqCst);

        // Exactly one should succeed, one should fail (first-committer-wins)
        assert_eq!(final_success, 1, "Exactly one commit should succeed");
        assert_eq!(final_failure, 1, "Exactly one commit should fail");

        // Storage should have exactly one committed value (not corrupted)
        let final_value = store.get(&key).unwrap().unwrap();
        assert!(
            final_value.value == Value::Int(1) || final_value.value == Value::Int(2),
            "Value should be from one of the transactions"
        );
    }

    /// Test that validation + apply is atomic
    /// Multiple threads trying to commit transactions that read the same key
    #[test]
    fn test_validation_apply_atomicity() {
        let (store, wal, _temp) = create_shared_env();
        let run_id = RunId::new();
        let ns = create_namespace(run_id);
        let key = create_key(&ns, "atomic_test");

        // Setup initial data FIRST
        store.put(key.clone(), Value::Int(0), None).unwrap();

        // Create manager AFTER initial data
        let manager = create_manager(&store);

        let num_threads = 10;
        let barrier = Arc::new(Barrier::new(num_threads));
        let success_count = Arc::new(AtomicUsize::new(0));

        let handles: Vec<_> = (0..num_threads)
            .map(|i| {
                let manager = Arc::clone(&manager);
                let store = Arc::clone(&store);
                let wal = Arc::clone(&wal);
                let barrier = Arc::clone(&barrier);
                let success_count = Arc::clone(&success_count);
                let key = key.clone();

                thread::spawn(move || {
                    let snapshot = store.create_snapshot();
                    let mut txn = TransactionContext::with_snapshot(
                        manager.next_txn_id(),
                        run_id,
                        Box::new(snapshot),
                    );

                    // Read then write
                    let _ = txn.get(&key).unwrap();
                    txn.put(key.clone(), Value::Int(i as i64)).unwrap();

                    barrier.wait();

                    let mut wal_guard = wal.lock();
                    if manager.commit(&mut txn, store.as_ref(), &mut wal_guard).is_ok() {
                        success_count.fetch_add(1, Ordering::SeqCst);
                    }
                })
            })
            .collect();

        for handle in handles {
            handle.join().unwrap();
        }

        // Only ONE transaction should have won (first-committer-wins)
        assert_eq!(
            success_count.load(Ordering::SeqCst),
            1,
            "Only one transaction should succeed when all read+write same key"
        );
    }
}

// ============================================================================
// SECTION 2: Concurrent Commit Tests
// ============================================================================

mod concurrent_commits {
    use super::*;

    /// Multiple threads committing to different keys - all should succeed
    #[test]
    fn test_concurrent_commits_different_keys() {
        let (store, wal, _temp) = create_shared_env();
        let manager = create_manager(&store);
        let run_id = RunId::new();
        let ns = create_namespace(run_id);

        let num_threads = 20;
        let barrier = Arc::new(Barrier::new(num_threads));
        let success_count = Arc::new(AtomicUsize::new(0));

        let handles: Vec<_> = (0..num_threads)
            .map(|i| {
                let manager = Arc::clone(&manager);
                let store = Arc::clone(&store);
                let wal = Arc::clone(&wal);
                let barrier = Arc::clone(&barrier);
                let success_count = Arc::clone(&success_count);
                let ns = ns.clone();

                thread::spawn(move || {
                    let key = create_key(&ns, &format!("key_{}", i));
                    let mut txn = TransactionContext::new(
                        manager.next_txn_id(),
                        run_id,
                        store.current_version(),
                    );
                    txn.put(key, Value::Int(i as i64)).unwrap();

                    barrier.wait();

                    let mut wal_guard = wal.lock();
                    if manager.commit(&mut txn, store.as_ref(), &mut wal_guard).is_ok() {
                        success_count.fetch_add(1, Ordering::SeqCst);
                    }
                })
            })
            .collect();

        for handle in handles {
            handle.join().unwrap();
        }

        // All should succeed (no conflicts - different keys)
        assert_eq!(
            success_count.load(Ordering::SeqCst),
            num_threads,
            "All commits to different keys should succeed"
        );

        // Verify all keys exist
        for i in 0..num_threads {
            let key = create_key(&ns, &format!("key_{}", i));
            let value = store.get(&key).unwrap();
            assert!(value.is_some(), "Key {} should exist", i);
            assert_eq!(value.unwrap().value, Value::Int(i as i64));
        }
    }

    /// Blind writes to same key - all should succeed (no read-set conflicts)
    #[test]
    fn test_concurrent_blind_writes_same_key() {
        let (store, wal, _temp) = create_shared_env();
        let manager = create_manager(&store);
        let run_id = RunId::new();
        let ns = create_namespace(run_id);
        let key = create_key(&ns, "blind_write_key");

        let num_threads = 10;
        let barrier = Arc::new(Barrier::new(num_threads));
        let success_count = Arc::new(AtomicUsize::new(0));
        let committed_values = Arc::new(Mutex::new(Vec::new()));

        let handles: Vec<_> = (0..num_threads)
            .map(|i| {
                let manager = Arc::clone(&manager);
                let store = Arc::clone(&store);
                let wal = Arc::clone(&wal);
                let barrier = Arc::clone(&barrier);
                let success_count = Arc::clone(&success_count);
                let committed_values = Arc::clone(&committed_values);
                let key = key.clone();

                thread::spawn(move || {
                    // Blind write - no read first
                    let mut txn = TransactionContext::new(
                        manager.next_txn_id(),
                        run_id,
                        store.current_version(),
                    );
                    txn.put(key.clone(), Value::Int(i as i64)).unwrap();

                    barrier.wait();

                    let mut wal_guard = wal.lock();
                    if let Ok(version) = manager.commit(&mut txn, store.as_ref(), &mut wal_guard) {
                        success_count.fetch_add(1, Ordering::SeqCst);
                        committed_values.lock().push((i, version));
                    }
                })
            })
            .collect();

        for handle in handles {
            handle.join().unwrap();
        }

        // All should succeed (blind writes don't conflict)
        assert_eq!(
            success_count.load(Ordering::SeqCst),
            num_threads,
            "All blind writes should succeed"
        );

        // Final value should be from the last committer (highest version)
        let values = committed_values.lock();
        let last_commit = values.iter().max_by_key(|(_, v)| v).unwrap();
        let final_value = store.get(&key).unwrap().unwrap();
        assert_eq!(final_value.value, Value::Int(last_commit.0 as i64));
    }

    /// Test read-only transactions always succeed even under concurrent writes
    #[test]
    fn test_concurrent_read_only_always_succeeds() {
        let (store, wal, _temp) = create_shared_env();
        let run_id = RunId::new();
        let ns = create_namespace(run_id);
        let key = create_key(&ns, "read_only_test");

        // Setup initial data FIRST
        store.put(key.clone(), Value::Int(0), None).unwrap();

        // Create manager AFTER initial data
        let manager = create_manager(&store);

        let num_readers = 10;
        let num_writers = 5;
        let total = num_readers + num_writers;
        let barrier = Arc::new(Barrier::new(total));
        let read_success = Arc::new(AtomicUsize::new(0));
        let write_success = Arc::new(AtomicUsize::new(0));

        let mut handles = Vec::new();

        // Spawn readers
        for _ in 0..num_readers {
            let manager = Arc::clone(&manager);
            let store = Arc::clone(&store);
            let wal = Arc::clone(&wal);
            let barrier = Arc::clone(&barrier);
            let read_success = Arc::clone(&read_success);
            let key = key.clone();

            handles.push(thread::spawn(move || {
                let snapshot = store.create_snapshot();
                let mut txn = TransactionContext::with_snapshot(
                    manager.next_txn_id(),
                    run_id,
                    Box::new(snapshot),
                );

                // Only read, no write
                let _ = txn.get(&key).unwrap();

                barrier.wait();

                let mut wal_guard = wal.lock();
                if manager.commit(&mut txn, store.as_ref(), &mut wal_guard).is_ok() {
                    read_success.fetch_add(1, Ordering::SeqCst);
                }
            }));
        }

        // Spawn writers
        for i in 0..num_writers {
            let manager = Arc::clone(&manager);
            let store = Arc::clone(&store);
            let wal = Arc::clone(&wal);
            let barrier = Arc::clone(&barrier);
            let write_success = Arc::clone(&write_success);
            let key = key.clone();

            handles.push(thread::spawn(move || {
                let snapshot = store.create_snapshot();
                let mut txn = TransactionContext::with_snapshot(
                    manager.next_txn_id(),
                    run_id,
                    Box::new(snapshot),
                );

                // Read then write
                let _ = txn.get(&key).unwrap();
                txn.put(key.clone(), Value::Int(i as i64 + 100)).unwrap();

                barrier.wait();

                let mut wal_guard = wal.lock();
                if manager.commit(&mut txn, store.as_ref(), &mut wal_guard).is_ok() {
                    write_success.fetch_add(1, Ordering::SeqCst);
                }
            }));
        }

        for handle in handles {
            handle.join().unwrap();
        }

        // All read-only transactions should succeed
        assert_eq!(
            read_success.load(Ordering::SeqCst),
            num_readers,
            "All read-only transactions should succeed"
        );

        // Only one writer should succeed (first-committer-wins)
        assert_eq!(
            write_success.load(Ordering::SeqCst),
            1,
            "Only one writer should succeed"
        );
    }
}

// ============================================================================
// SECTION 3: Version Monotonicity Tests
// ============================================================================

mod version_monotonicity {
    use super::*;

    /// Verify versions are always monotonically increasing under concurrent load
    #[test]
    fn test_version_monotonicity_under_load() {
        let (store, wal, _temp) = create_shared_env();
        let manager = create_manager(&store);
        let run_id = RunId::new();
        let ns = create_namespace(run_id);

        let num_threads = 20;
        let commits_per_thread = 10;
        let barrier = Arc::new(Barrier::new(num_threads));
        let all_versions = Arc::new(Mutex::new(Vec::new()));

        let handles: Vec<_> = (0..num_threads)
            .map(|t| {
                let manager = Arc::clone(&manager);
                let store = Arc::clone(&store);
                let wal = Arc::clone(&wal);
                let barrier = Arc::clone(&barrier);
                let all_versions = Arc::clone(&all_versions);
                let ns = ns.clone();

                thread::spawn(move || {
                    let mut thread_versions = Vec::new();

                    barrier.wait();

                    for i in 0..commits_per_thread {
                        let key = create_key(&ns, &format!("t{}_k{}", t, i));
                        let mut txn = TransactionContext::new(
                            manager.next_txn_id(),
                            run_id,
                            store.current_version(),
                        );
                        txn.put(key, Value::Int((t * commits_per_thread + i) as i64))
                            .unwrap();

                        let mut wal_guard = wal.lock();
                        if let Ok(version) =
                            manager.commit(&mut txn, store.as_ref(), &mut wal_guard)
                        {
                            thread_versions.push(version);
                        }
                    }

                    all_versions.lock().extend(thread_versions);
                })
            })
            .collect();

        for handle in handles {
            handle.join().unwrap();
        }

        // Verify all versions are unique
        let versions = all_versions.lock();
        let unique_versions: HashSet<_> = versions.iter().cloned().collect();
        assert_eq!(
            versions.len(),
            unique_versions.len(),
            "All commit versions should be unique"
        );

        // Verify versions are in increasing order when sorted
        let mut sorted_versions: Vec<_> = versions.iter().cloned().collect();
        sorted_versions.sort();
        for i in 1..sorted_versions.len() {
            assert!(
                sorted_versions[i] > sorted_versions[i - 1],
                "Versions should be strictly increasing"
            );
        }
    }

    /// Verify transaction IDs are unique across threads
    #[test]
    fn test_txn_id_uniqueness_concurrent() {
        let manager = Arc::new(TransactionManager::new(0));
        let num_threads = 10;
        let ids_per_thread = 100;
        let barrier = Arc::new(Barrier::new(num_threads));
        let all_ids = Arc::new(Mutex::new(Vec::new()));

        let handles: Vec<_> = (0..num_threads)
            .map(|_| {
                let manager = Arc::clone(&manager);
                let barrier = Arc::clone(&barrier);
                let all_ids = Arc::clone(&all_ids);

                thread::spawn(move || {
                    let mut thread_ids = Vec::new();

                    barrier.wait();

                    for _ in 0..ids_per_thread {
                        thread_ids.push(manager.next_txn_id());
                    }

                    all_ids.lock().extend(thread_ids);
                })
            })
            .collect();

        for handle in handles {
            handle.join().unwrap();
        }

        let ids = all_ids.lock();
        let unique_ids: HashSet<_> = ids.iter().cloned().collect();
        assert_eq!(
            ids.len(),
            unique_ids.len(),
            "All transaction IDs should be unique"
        );
        assert_eq!(
            ids.len(),
            num_threads * ids_per_thread,
            "Should have all expected IDs"
        );
    }
}

// ============================================================================
// SECTION 4: Stress Tests
// ============================================================================

mod stress_tests {
    use super::*;

    /// High-concurrency stress test - many threads, many operations
    #[test]
    fn test_high_concurrency_stress() {
        let (store, wal, _temp) = create_shared_env();
        let manager = create_manager(&store);
        let run_id = RunId::new();
        let ns = create_namespace(run_id);

        let num_threads = 50;
        let ops_per_thread = 20;
        let barrier = Arc::new(Barrier::new(num_threads));
        let total_commits = Arc::new(AtomicUsize::new(0));
        let total_failures = Arc::new(AtomicUsize::new(0));

        let handles: Vec<_> = (0..num_threads)
            .map(|t| {
                let manager = Arc::clone(&manager);
                let store = Arc::clone(&store);
                let wal = Arc::clone(&wal);
                let barrier = Arc::clone(&barrier);
                let total_commits = Arc::clone(&total_commits);
                let total_failures = Arc::clone(&total_failures);
                let ns = ns.clone();

                thread::spawn(move || {
                    barrier.wait();

                    for i in 0..ops_per_thread {
                        // Mix of operations
                        let key = create_key(&ns, &format!("stress_t{}_k{}", t, i % 10));
                        let snapshot = store.create_snapshot();
                        let mut txn = TransactionContext::with_snapshot(
                            manager.next_txn_id(),
                            run_id,
                            Box::new(snapshot),
                        );

                        // Sometimes read first (causes conflicts), sometimes blind write
                        if i % 3 == 0 {
                            let _ = txn.get(&key);
                        }
                        txn.put(key, Value::Int((t * ops_per_thread + i) as i64))
                            .unwrap();

                        let mut wal_guard = wal.lock();
                        if manager.commit(&mut txn, store.as_ref(), &mut wal_guard).is_ok() {
                            total_commits.fetch_add(1, Ordering::SeqCst);
                        } else {
                            total_failures.fetch_add(1, Ordering::SeqCst);
                        }
                    }
                })
            })
            .collect();

        for handle in handles {
            handle.join().unwrap();
        }

        let commits = total_commits.load(Ordering::SeqCst);
        let failures = total_failures.load(Ordering::SeqCst);

        // We should have processed all operations (no panics, no hangs)
        assert_eq!(
            commits + failures,
            num_threads * ops_per_thread,
            "All operations should complete (commit or fail)"
        );

        // Some commits should succeed
        assert!(commits > 0, "Some commits should succeed");

        // Some failures are expected due to conflicts
        // (though not required - blind writes don't fail)
    }

    /// Long-running concurrent test to detect memory leaks or corruption
    #[test]
    fn test_sustained_concurrent_load() {
        let (store, wal, _temp) = create_shared_env();
        let manager = create_manager(&store);
        let run_id = RunId::new();
        let ns = create_namespace(run_id);

        let num_threads = 10;
        let duration = Duration::from_millis(500); // Run for 500ms
        let barrier = Arc::new(Barrier::new(num_threads));
        let total_commits = Arc::new(AtomicUsize::new(0));
        let running = Arc::new(std::sync::atomic::AtomicBool::new(true));

        let handles: Vec<_> = (0..num_threads)
            .map(|t| {
                let manager = Arc::clone(&manager);
                let store = Arc::clone(&store);
                let wal = Arc::clone(&wal);
                let barrier = Arc::clone(&barrier);
                let total_commits = Arc::clone(&total_commits);
                let running = Arc::clone(&running);
                let ns = ns.clone();

                thread::spawn(move || {
                    barrier.wait();
                    let mut op_count = 0u64;

                    while running.load(Ordering::SeqCst) {
                        let key = create_key(&ns, &format!("sustained_t{}_k{}", t, op_count % 5));
                        let mut txn = TransactionContext::new(
                            manager.next_txn_id(),
                            run_id,
                            store.current_version(),
                        );
                        txn.put(key, Value::Int(op_count as i64)).unwrap();

                        let mut wal_guard = wal.lock();
                        if manager.commit(&mut txn, store.as_ref(), &mut wal_guard).is_ok() {
                            total_commits.fetch_add(1, Ordering::SeqCst);
                        }
                        op_count += 1;
                    }
                })
            })
            .collect();

        // Let it run for the specified duration
        thread::sleep(duration);
        running.store(false, Ordering::SeqCst);

        for handle in handles {
            handle.join().unwrap();
        }

        let commits = total_commits.load(Ordering::SeqCst);
        // Under high contention with WAL locking, commit rate may be limited
        // We just need to verify the system works without deadlock or panic
        assert!(
            commits > 10,
            "Should have some commits in sustained test, got {}",
            commits
        );
    }

    /// Test no deadlocks occur under high contention
    #[test]
    fn test_no_deadlock_high_contention() {
        let (store, wal, _temp) = create_shared_env();
        let manager = create_manager(&store);
        let run_id = RunId::new();
        let ns = create_namespace(run_id);

        // Use few keys to maximize contention
        let num_keys = 3;
        let num_threads = 20;
        let ops_per_thread = 50;
        let barrier = Arc::new(Barrier::new(num_threads));
        let completed = Arc::new(AtomicUsize::new(0));

        let start = Instant::now();
        // High contention can be slow - allow generous timeout
        // The key is that it completes at all (no deadlock)
        let timeout = Duration::from_secs(30);

        let handles: Vec<_> = (0..num_threads)
            .map(|t| {
                let manager = Arc::clone(&manager);
                let store = Arc::clone(&store);
                let wal = Arc::clone(&wal);
                let barrier = Arc::clone(&barrier);
                let completed = Arc::clone(&completed);
                let ns = ns.clone();

                thread::spawn(move || {
                    barrier.wait();

                    for i in 0..ops_per_thread {
                        let key = create_key(&ns, &format!("contended_{}", i % num_keys));
                        let snapshot = store.create_snapshot();
                        let mut txn = TransactionContext::with_snapshot(
                            manager.next_txn_id(),
                            run_id,
                            Box::new(snapshot),
                        );

                        let _ = txn.get(&key); // Read to create contention
                        txn.put(key, Value::Int(t as i64)).unwrap();

                        let mut wal_guard = wal.lock();
                        let _ = manager.commit(&mut txn, store.as_ref(), &mut wal_guard);
                    }

                    completed.fetch_add(1, Ordering::SeqCst);
                })
            })
            .collect();

        // Wait for completion or timeout
        for handle in handles {
            handle.join().unwrap();
        }

        let elapsed = start.elapsed();
        assert!(
            elapsed < timeout,
            "Test should complete without deadlock. Took {:?}",
            elapsed
        );
        assert_eq!(
            completed.load(Ordering::SeqCst),
            num_threads,
            "All threads should complete"
        );
    }
}

// ============================================================================
// SECTION 5: CAS Concurrent Tests
// ============================================================================

mod concurrent_cas {
    use super::*;

    /// Test CAS operations under concurrent load
    /// Multiple threads trying to increment a counter with CAS
    #[test]
    fn test_concurrent_cas_counter() {
        let (store, wal, _temp) = create_shared_env();
        let run_id = RunId::new();
        let ns = create_namespace(run_id);
        let key = create_key(&ns, "counter");

        // Initialize counter FIRST
        store.put(key.clone(), Value::Int(0), None).unwrap();

        // Create manager AFTER initial data
        let manager = create_manager(&store);

        let num_threads = 10;
        let increments_per_thread = 5;
        let successful_increments = Arc::new(AtomicUsize::new(0));

        let handles: Vec<_> = (0..num_threads)
            .map(|_| {
                let manager = Arc::clone(&manager);
                let store = Arc::clone(&store);
                let wal = Arc::clone(&wal);
                let successful_increments = Arc::clone(&successful_increments);
                let key = key.clone();

                thread::spawn(move || {
                    for _ in 0..increments_per_thread {
                        // CAS retry loop
                        loop {
                            let snapshot = store.create_snapshot();
                            let current = snapshot.get(&key).unwrap().unwrap();
                            let current_version = current.version.as_u64();
                            let current_value = match current.value {
                                Value::Int(v) => v,
                                _ => panic!("Expected Int"),
                            };

                            let mut txn = TransactionContext::with_snapshot(
                                manager.next_txn_id(),
                                run_id,
                                Box::new(snapshot),
                            );
                            txn.cas(key.clone(), current_version, Value::Int(current_value + 1))
                                .unwrap();

                            let mut wal_guard = wal.lock();
                            if manager.commit(&mut txn, store.as_ref(), &mut wal_guard).is_ok() {
                                successful_increments.fetch_add(1, Ordering::SeqCst);
                                break;
                            }
                            // Retry on conflict
                        }
                    }
                })
            })
            .collect();

        for handle in handles {
            handle.join().unwrap();
        }

        // All increments should eventually succeed
        let total_increments = successful_increments.load(Ordering::SeqCst);
        assert_eq!(
            total_increments,
            num_threads * increments_per_thread,
            "All CAS increments should eventually succeed"
        );

        // Final value should match total increments
        let final_value = store.get(&key).unwrap().unwrap();
        match final_value.value {
            Value::Int(v) => {
                assert_eq!(
                    v as usize,
                    num_threads * increments_per_thread,
                    "Counter should equal total increments"
                );
            }
            _ => panic!("Expected Int"),
        }
    }

    /// Test CAS version 0 (insert if not exists) concurrent race
    #[test]
    fn test_concurrent_cas_insert_if_not_exists() {
        let (store, wal, _temp) = create_shared_env();
        let manager = create_manager(&store);
        let run_id = RunId::new();
        let ns = create_namespace(run_id);
        let key = create_key(&ns, "cas_insert");

        let num_threads = 10;
        let barrier = Arc::new(Barrier::new(num_threads));
        let success_count = Arc::new(AtomicUsize::new(0));
        let winner_value = Arc::new(AtomicU64::new(0));

        let handles: Vec<_> = (0..num_threads)
            .map(|i| {
                let manager = Arc::clone(&manager);
                let store = Arc::clone(&store);
                let wal = Arc::clone(&wal);
                let barrier = Arc::clone(&barrier);
                let success_count = Arc::clone(&success_count);
                let winner_value = Arc::clone(&winner_value);
                let key = key.clone();

                thread::spawn(move || {
                    let mut txn =
                        TransactionContext::new(manager.next_txn_id(), run_id, store.current_version());
                    // CAS with version 0 = insert if not exists
                    txn.cas(key.clone(), 0, Value::Int(i as i64)).unwrap();

                    barrier.wait();

                    let mut wal_guard = wal.lock();
                    if manager.commit(&mut txn, store.as_ref(), &mut wal_guard).is_ok() {
                        success_count.fetch_add(1, Ordering::SeqCst);
                        winner_value.store(i as u64, Ordering::SeqCst);
                    }
                })
            })
            .collect();

        for handle in handles {
            handle.join().unwrap();
        }

        // Exactly one should win
        assert_eq!(
            success_count.load(Ordering::SeqCst),
            1,
            "Only one CAS insert should succeed"
        );

        // Value should be from the winner
        let final_value = store.get(&key).unwrap().unwrap();
        assert_eq!(
            final_value.value,
            Value::Int(winner_value.load(Ordering::SeqCst) as i64)
        );
    }
}

// ============================================================================
// SECTION 6: Abort and Rollback Tests
// ============================================================================

mod concurrent_abort {
    use super::*;

    /// Test that aborted transactions don't affect storage
    #[test]
    fn test_abort_leaves_storage_unchanged() {
        let (store, _wal, _temp) = create_shared_env();
        let run_id = RunId::new();
        let ns = create_namespace(run_id);
        let key = create_key(&ns, "abort_test");

        // Initial value FIRST
        store.put(key.clone(), Value::Int(100), None).unwrap();

        // Create manager AFTER initial data
        let manager = create_manager(&store);
        let initial_version = store.current_version();

        let num_threads = 10;
        let barrier = Arc::new(Barrier::new(num_threads));

        let handles: Vec<_> = (0..num_threads)
            .map(|i| {
                let manager = Arc::clone(&manager);
                let store = Arc::clone(&store);
                let barrier = Arc::clone(&barrier);
                let key = key.clone();

                thread::spawn(move || {
                    let snapshot = store.create_snapshot();
                    let mut txn = TransactionContext::with_snapshot(
                        manager.next_txn_id(),
                        run_id,
                        Box::new(snapshot),
                    );

                    txn.put(key.clone(), Value::Int(i as i64)).unwrap();

                    barrier.wait();

                    // Abort instead of commit
                    manager.abort(&mut txn, format!("Thread {} aborted", i)).unwrap();

                    assert!(matches!(txn.status, TransactionStatus::Aborted { .. }));
                })
            })
            .collect();

        for handle in handles {
            handle.join().unwrap();
        }

        // Storage should be completely unchanged
        let final_value = store.get(&key).unwrap().unwrap();
        assert_eq!(final_value.value, Value::Int(100), "Value should be unchanged");
        assert_eq!(
            store.current_version(),
            initial_version,
            "Version should be unchanged"
        );
    }

    /// Test mixed commit and abort under concurrent load
    #[test]
    fn test_mixed_commit_abort_concurrent() {
        let (store, wal, _temp) = create_shared_env();
        let manager = create_manager(&store);
        let run_id = RunId::new();
        let ns = create_namespace(run_id);

        let num_threads = 20;
        let barrier = Arc::new(Barrier::new(num_threads));
        let commit_count = Arc::new(AtomicUsize::new(0));
        let abort_count = Arc::new(AtomicUsize::new(0));

        let handles: Vec<_> = (0..num_threads)
            .map(|i| {
                let manager = Arc::clone(&manager);
                let store = Arc::clone(&store);
                let wal = Arc::clone(&wal);
                let barrier = Arc::clone(&barrier);
                let commit_count = Arc::clone(&commit_count);
                let abort_count = Arc::clone(&abort_count);
                let ns = ns.clone();

                thread::spawn(move || {
                    let key = create_key(&ns, &format!("mixed_{}", i));
                    let mut txn =
                        TransactionContext::new(manager.next_txn_id(), run_id, store.current_version());
                    txn.put(key.clone(), Value::Int(i as i64)).unwrap();

                    barrier.wait();

                    // Half commit, half abort
                    if i % 2 == 0 {
                        let mut wal_guard = wal.lock();
                        if manager.commit(&mut txn, store.as_ref(), &mut wal_guard).is_ok() {
                            commit_count.fetch_add(1, Ordering::SeqCst);
                        }
                    } else {
                        manager.abort(&mut txn, "intentional abort".to_string()).unwrap();
                        abort_count.fetch_add(1, Ordering::SeqCst);
                    }
                })
            })
            .collect();

        for handle in handles {
            handle.join().unwrap();
        }

        // Verify counts
        assert_eq!(commit_count.load(Ordering::SeqCst), num_threads / 2);
        assert_eq!(abort_count.load(Ordering::SeqCst), num_threads / 2);

        // Verify only committed keys exist
        for i in 0..num_threads {
            let key = create_key(&ns, &format!("mixed_{}", i));
            let exists = store.get(&key).unwrap().is_some();
            if i % 2 == 0 {
                assert!(exists, "Committed key {} should exist", i);
            } else {
                assert!(!exists, "Aborted key {} should not exist", i);
            }
        }
    }
}

// ============================================================================
// SECTION 7: Snapshot Isolation Under Concurrent Load
// ============================================================================

mod concurrent_snapshot_isolation {
    use super::*;

    /// Verify snapshot provides consistent view while concurrent commits happen
    #[test]
    fn test_snapshot_consistency_during_commits() {
        let (store, wal, _temp) = create_shared_env();
        let run_id = RunId::new();
        let ns = create_namespace(run_id);

        // Setup initial data FIRST
        for i in 0..10 {
            let key = create_key(&ns, &format!("snap_key_{}", i));
            store.put(key, Value::Int(i), None).unwrap();
        }

        // Create manager AFTER initial data
        let manager = create_manager(&store);

        let snapshot_version = store.current_version();
        let reader_snapshot = store.create_snapshot();

        // Now spawn writers that will modify all keys
        let num_writers = 5;
        let barrier = Arc::new(Barrier::new(num_writers + 1)); // +1 for reader

        let mut handles = Vec::new();

        // Spawn writers
        for w in 0..num_writers {
            let manager = Arc::clone(&manager);
            let store = Arc::clone(&store);
            let wal = Arc::clone(&wal);
            let barrier = Arc::clone(&barrier);
            let ns = ns.clone();

            handles.push(thread::spawn(move || {
                barrier.wait();

                for i in 0..10 {
                    let key = create_key(&ns, &format!("snap_key_{}", i));
                    let mut txn = TransactionContext::new(
                        manager.next_txn_id(),
                        run_id,
                        store.current_version(),
                    );
                    txn.put(key, Value::Int(100 + w as i64)).unwrap();

                    let mut wal_guard = wal.lock();
                    let _ = manager.commit(&mut txn, store.as_ref(), &mut wal_guard);
                }
            }));
        }

        // Reader verifies snapshot consistency
        let barrier_reader = Arc::clone(&barrier);
        let reader_ns = ns.clone();
        let reader_handle = thread::spawn(move || {
            barrier_reader.wait();

            // Read all keys multiple times - should always see original values
            for _ in 0..100 {
                for i in 0..10 {
                    let key = create_key(&reader_ns, &format!("snap_key_{}", i));
                    let value = reader_snapshot.get(&key).unwrap().unwrap();
                    assert_eq!(
                        value.value,
                        Value::Int(i),
                        "Snapshot should see original value"
                    );
                    assert_eq!(
                        value.version.as_u64(),
                        snapshot_version - 9 + i as u64,
                        "Snapshot version should be consistent"
                    );
                }
            }
        });

        for handle in handles {
            handle.join().unwrap();
        }
        reader_handle.join().unwrap();
    }

    /// Test that reads within a transaction are consistent
    #[test]
    fn test_transaction_read_consistency() {
        let (store, wal, _temp) = create_shared_env();
        let run_id = RunId::new();
        let ns = create_namespace(run_id);
        let key = create_key(&ns, "consistency_test");

        // Setup initial data FIRST
        store.put(key.clone(), Value::Int(1), None).unwrap();

        // Create manager AFTER initial data
        let manager = create_manager(&store);

        let num_threads = 10;
        let barrier = Arc::new(Barrier::new(num_threads));
        let inconsistencies = Arc::new(AtomicUsize::new(0));

        let handles: Vec<_> = (0..num_threads)
            .map(|i| {
                let manager = Arc::clone(&manager);
                let store = Arc::clone(&store);
                let wal = Arc::clone(&wal);
                let barrier = Arc::clone(&barrier);
                let inconsistencies = Arc::clone(&inconsistencies);
                let key = key.clone();

                thread::spawn(move || {
                    let snapshot = store.create_snapshot();
                    let mut txn = TransactionContext::with_snapshot(
                        manager.next_txn_id(),
                        run_id,
                        Box::new(snapshot),
                    );

                    // First read
                    let first_read = txn.get(&key).unwrap();

                    barrier.wait();

                    // Some threads write (changing the underlying storage)
                    if i % 3 == 0 {
                        let mut write_txn = TransactionContext::new(
                            manager.next_txn_id(),
                            run_id,
                            store.current_version(),
                        );
                        write_txn.put(key.clone(), Value::Int(i as i64 + 100)).unwrap();
                        let mut wal_guard = wal.lock();
                        let _ = manager.commit(&mut write_txn, store.as_ref(), &mut wal_guard);
                    }

                    // Second read - should be same as first (snapshot isolation)
                    let second_read = txn.get(&key).unwrap();

                    if first_read != second_read {
                        inconsistencies.fetch_add(1, Ordering::SeqCst);
                    }
                })
            })
            .collect();

        for handle in handles {
            handle.join().unwrap();
        }

        assert_eq!(
            inconsistencies.load(Ordering::SeqCst),
            0,
            "Should be no read inconsistencies within a transaction"
        );
    }
}

// ============================================================================
// SECTION 8: Recovery Concurrent Tests
// ============================================================================

mod concurrent_recovery {
    use super::*;
    use strata_concurrency::RecoveryCoordinator;

    /// Test that recovery produces deterministic results
    #[test]
    fn test_recovery_determinism_after_concurrent_commits() {
        let temp_dir = TempDir::new().unwrap();
        let wal_path = temp_dir.path().join("recovery.wal");

        let run_id = RunId::new();
        let ns = create_namespace(run_id);

        // Phase 1: Concurrent commits
        {
            let wal = Arc::new(Mutex::new(
                WAL::open(&wal_path, DurabilityMode::Strict).unwrap(),
            ));
            let store = Arc::new(UnifiedStore::new());
            let manager = Arc::new(TransactionManager::new(store.current_version()));

            let num_threads = 10;
            let barrier = Arc::new(Barrier::new(num_threads));

            let handles: Vec<_> = (0..num_threads)
                .map(|t| {
                    let manager = Arc::clone(&manager);
                    let store = Arc::clone(&store);
                    let wal = Arc::clone(&wal);
                    let barrier = Arc::clone(&barrier);
                    let ns = ns.clone();

                    thread::spawn(move || {
                        let key = create_key(&ns, &format!("recovery_key_{}", t));
                        let mut txn = TransactionContext::new(
                            manager.next_txn_id(),
                            run_id,
                            store.current_version(),
                        );
                        txn.put(key, Value::Int(t as i64)).unwrap();

                        barrier.wait();

                        let mut wal_guard = wal.lock();
                        let _ = manager.commit(&mut txn, store.as_ref(), &mut wal_guard);
                    })
                })
                .collect();

            for handle in handles {
                handle.join().unwrap();
            }
        }

        // Phase 2: Recovery - run twice, verify same result
        let mut results = Vec::new();
        for _ in 0..2 {
            let coordinator = RecoveryCoordinator::new(wal_path.clone());
            let result = coordinator.recover().unwrap();

            let mut state: Vec<_> = (0..10)
                .filter_map(|t| {
                    let key = create_key(&ns, &format!("recovery_key_{}", t));
                    result.storage.get(&key).unwrap().map(|v| (t, v.value.clone()))
                })
                .collect();
            state.sort_by_key(|(t, _)| *t);

            results.push((result.stats.final_version, state));
        }

        assert_eq!(results[0], results[1], "Recovery should be deterministic");
    }
}

// ============================================================================
// SECTION 9: Concurrent Delete Tests
// ============================================================================

mod concurrent_delete {
    use super::*;

    /// Test concurrent deletes of the same key - only one should succeed
    #[test]
    fn test_concurrent_delete_same_key() {
        let (store, wal, _temp) = create_shared_env();
        let run_id = RunId::new();
        let ns = create_namespace(run_id);
        let key = create_key(&ns, "to_delete");

        // Setup initial data FIRST
        store.put(key.clone(), Value::Int(100), None).unwrap();

        // Create manager AFTER initial data
        let manager = create_manager(&store);

        let num_threads = 5;
        let barrier = Arc::new(Barrier::new(num_threads));
        let success_count = Arc::new(AtomicUsize::new(0));

        let handles: Vec<_> = (0..num_threads)
            .map(|_| {
                let manager = Arc::clone(&manager);
                let store = Arc::clone(&store);
                let wal = Arc::clone(&wal);
                let barrier = Arc::clone(&barrier);
                let success_count = Arc::clone(&success_count);
                let key = key.clone();

                thread::spawn(move || {
                    let snapshot = store.create_snapshot();
                    let mut txn = TransactionContext::with_snapshot(
                        manager.next_txn_id(),
                        run_id,
                        Box::new(snapshot),
                    );

                    // Read then delete (creates conflict)
                    let _ = txn.get(&key).unwrap();
                    txn.delete(key.clone()).unwrap();

                    barrier.wait();

                    let mut wal_guard = wal.lock();
                    if manager.commit(&mut txn, store.as_ref(), &mut wal_guard).is_ok() {
                        success_count.fetch_add(1, Ordering::SeqCst);
                    }
                })
            })
            .collect();

        for handle in handles {
            handle.join().unwrap();
        }

        // Only one delete should succeed (first-committer-wins)
        assert_eq!(
            success_count.load(Ordering::SeqCst),
            1,
            "Only one delete should succeed"
        );

        // Key should be deleted
        assert!(store.get(&key).unwrap().is_none());
    }

    /// Test concurrent delete and write to same key
    #[test]
    fn test_concurrent_delete_vs_write() {
        let (store, wal, _temp) = create_shared_env();
        let run_id = RunId::new();
        let ns = create_namespace(run_id);
        let key = create_key(&ns, "contested_key");

        // Setup initial data FIRST
        store.put(key.clone(), Value::Int(0), None).unwrap();

        // Create manager AFTER initial data
        let manager = create_manager(&store);

        let barrier = Arc::new(Barrier::new(2));
        let delete_succeeded = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let write_succeeded = Arc::new(std::sync::atomic::AtomicBool::new(false));

        // Thread 1: Delete
        let h1 = {
            let manager = Arc::clone(&manager);
            let store = Arc::clone(&store);
            let wal = Arc::clone(&wal);
            let barrier = Arc::clone(&barrier);
            let delete_succeeded = Arc::clone(&delete_succeeded);
            let key = key.clone();

            thread::spawn(move || {
                let snapshot = store.create_snapshot();
                let mut txn = TransactionContext::with_snapshot(
                    manager.next_txn_id(),
                    run_id,
                    Box::new(snapshot),
                );
                let _ = txn.get(&key).unwrap();
                txn.delete(key.clone()).unwrap();

                barrier.wait();

                let mut wal_guard = wal.lock();
                if manager.commit(&mut txn, store.as_ref(), &mut wal_guard).is_ok() {
                    delete_succeeded.store(true, Ordering::SeqCst);
                }
            })
        };

        // Thread 2: Write
        let h2 = {
            let manager = Arc::clone(&manager);
            let store = Arc::clone(&store);
            let wal = Arc::clone(&wal);
            let barrier = Arc::clone(&barrier);
            let write_succeeded = Arc::clone(&write_succeeded);
            let key = key.clone();

            thread::spawn(move || {
                let snapshot = store.create_snapshot();
                let mut txn = TransactionContext::with_snapshot(
                    manager.next_txn_id(),
                    run_id,
                    Box::new(snapshot),
                );
                let _ = txn.get(&key).unwrap();
                txn.put(key.clone(), Value::Int(999)).unwrap();

                barrier.wait();

                let mut wal_guard = wal.lock();
                if manager.commit(&mut txn, store.as_ref(), &mut wal_guard).is_ok() {
                    write_succeeded.store(true, Ordering::SeqCst);
                }
            })
        };

        h1.join().unwrap();
        h2.join().unwrap();

        // Exactly one should succeed
        let del = delete_succeeded.load(Ordering::SeqCst);
        let wrt = write_succeeded.load(Ordering::SeqCst);
        assert!(
            (del && !wrt) || (!del && wrt),
            "Exactly one of delete or write should succeed"
        );

        // Check final state matches the winner
        let final_state = store.get(&key).unwrap();
        if del {
            assert!(final_state.is_none(), "Key should be deleted");
        } else {
            assert_eq!(final_state.unwrap().value, Value::Int(999));
        }
    }

    /// Test blind delete (delete without read) doesn't conflict
    #[test]
    fn test_concurrent_blind_deletes() {
        let (store, wal, _temp) = create_shared_env();
        let run_id = RunId::new();
        let ns = create_namespace(run_id);
        let key = create_key(&ns, "blind_delete");

        // Setup initial data
        store.put(key.clone(), Value::Int(100), None).unwrap();

        let manager = create_manager(&store);

        let num_threads = 5;
        let barrier = Arc::new(Barrier::new(num_threads));
        let success_count = Arc::new(AtomicUsize::new(0));

        let handles: Vec<_> = (0..num_threads)
            .map(|_| {
                let manager = Arc::clone(&manager);
                let store = Arc::clone(&store);
                let wal = Arc::clone(&wal);
                let barrier = Arc::clone(&barrier);
                let success_count = Arc::clone(&success_count);
                let key = key.clone();

                thread::spawn(move || {
                    // Blind delete - no read first
                    let mut txn = TransactionContext::new(
                        manager.next_txn_id(),
                        run_id,
                        store.current_version(),
                    );
                    txn.delete(key.clone()).unwrap();

                    barrier.wait();

                    let mut wal_guard = wal.lock();
                    if manager.commit(&mut txn, store.as_ref(), &mut wal_guard).is_ok() {
                        success_count.fetch_add(1, Ordering::SeqCst);
                    }
                })
            })
            .collect();

        for handle in handles {
            handle.join().unwrap();
        }

        // All blind deletes should succeed (no read-set conflicts)
        assert_eq!(
            success_count.load(Ordering::SeqCst),
            num_threads,
            "All blind deletes should succeed"
        );

        // Key should be deleted
        assert!(store.get(&key).unwrap().is_none());
    }
}

// ============================================================================
// SECTION 10: Disjoint Read-Set Transactions (Concurrent Updates to Independent Data)
// ============================================================================

mod disjoint_transactions {
    use super::*;

    /// Transactions with completely disjoint read/write sets should all succeed
    /// T1: Read/Write A only
    /// T2: Read/Write B only
    /// No overlap = no conflict
    #[test]
    fn test_disjoint_transactions_both_succeed() {
        let (store, wal, _temp) = create_shared_env();
        let run_id = RunId::new();
        let ns = create_namespace(run_id);
        let key_a = create_key(&ns, "key_a");
        let key_b = create_key(&ns, "key_b");

        // Setup initial data FIRST
        store.put(key_a.clone(), Value::Int(100), None).unwrap();
        store.put(key_b.clone(), Value::Int(200), None).unwrap();

        // Create manager AFTER initial data
        let manager = create_manager(&store);

        let barrier = Arc::new(Barrier::new(2));
        let t1_success = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let t2_success = Arc::new(std::sync::atomic::AtomicBool::new(false));

        // T1: Read A, Write A (completely isolated from B)
        let h1 = {
            let manager = Arc::clone(&manager);
            let store = Arc::clone(&store);
            let wal = Arc::clone(&wal);
            let barrier = Arc::clone(&barrier);
            let t1_success = Arc::clone(&t1_success);
            let key_a = key_a.clone();

            thread::spawn(move || {
                let snapshot = store.create_snapshot();
                let mut txn = TransactionContext::with_snapshot(
                    manager.next_txn_id(),
                    run_id,
                    Box::new(snapshot),
                );

                // Read A, Write A
                let _ = txn.get(&key_a).unwrap();
                txn.put(key_a.clone(), Value::Int(999)).unwrap();

                barrier.wait();

                let mut wal_guard = wal.lock();
                if manager.commit(&mut txn, store.as_ref(), &mut wal_guard).is_ok() {
                    t1_success.store(true, Ordering::SeqCst);
                }
            })
        };

        // T2: Read B, Write B (completely isolated from A)
        let h2 = {
            let manager = Arc::clone(&manager);
            let store = Arc::clone(&store);
            let wal = Arc::clone(&wal);
            let barrier = Arc::clone(&barrier);
            let t2_success = Arc::clone(&t2_success);
            let key_b = key_b.clone();

            thread::spawn(move || {
                let snapshot = store.create_snapshot();
                let mut txn = TransactionContext::with_snapshot(
                    manager.next_txn_id(),
                    run_id,
                    Box::new(snapshot),
                );

                // Read B, Write B
                let _ = txn.get(&key_b).unwrap();
                txn.put(key_b.clone(), Value::Int(888)).unwrap();

                barrier.wait();

                let mut wal_guard = wal.lock();
                if manager.commit(&mut txn, store.as_ref(), &mut wal_guard).is_ok() {
                    t2_success.store(true, Ordering::SeqCst);
                }
            })
        };

        h1.join().unwrap();
        h2.join().unwrap();

        // Both should succeed - completely disjoint read/write sets
        assert!(
            t1_success.load(Ordering::SeqCst),
            "T1 should succeed (disjoint from T2)"
        );
        assert!(
            t2_success.load(Ordering::SeqCst),
            "T2 should succeed (disjoint from T1)"
        );

        // Both writes should be visible
        assert_eq!(store.get(&key_a).unwrap().unwrap().value, Value::Int(999));
        assert_eq!(store.get(&key_b).unwrap().unwrap().value, Value::Int(888));
    }

    /// Cross-read conflict: T1 reads A, writes B; T2 reads B, writes A
    /// T1 writes to what T2 reads -> T2 fails validation (first-committer-wins)
    /// This tests the read-set validation correctly detects the conflict
    #[test]
    fn test_cross_read_conflict_detection() {
        let (store, wal, _temp) = create_shared_env();
        let run_id = RunId::new();
        let ns = create_namespace(run_id);
        let key_a = create_key(&ns, "key_a");
        let key_b = create_key(&ns, "key_b");

        // Setup initial data FIRST
        store.put(key_a.clone(), Value::Int(100), None).unwrap();
        store.put(key_b.clone(), Value::Int(200), None).unwrap();

        // Create manager AFTER initial data
        let manager = create_manager(&store);

        let barrier = Arc::new(Barrier::new(2));
        let success_count = Arc::new(AtomicUsize::new(0));

        // T1: Read A, Write B
        let h1 = {
            let manager = Arc::clone(&manager);
            let store = Arc::clone(&store);
            let wal = Arc::clone(&wal);
            let barrier = Arc::clone(&barrier);
            let success_count = Arc::clone(&success_count);
            let key_a = key_a.clone();
            let key_b = key_b.clone();

            thread::spawn(move || {
                let snapshot = store.create_snapshot();
                let mut txn = TransactionContext::with_snapshot(
                    manager.next_txn_id(),
                    run_id,
                    Box::new(snapshot),
                );

                let _ = txn.get(&key_a).unwrap(); // Read A
                txn.put(key_b.clone(), Value::Int(999)).unwrap(); // Write B

                barrier.wait();

                let mut wal_guard = wal.lock();
                if manager.commit(&mut txn, store.as_ref(), &mut wal_guard).is_ok() {
                    success_count.fetch_add(1, Ordering::SeqCst);
                }
            })
        };

        // T2: Read B, Write A
        let h2 = {
            let manager = Arc::clone(&manager);
            let store = Arc::clone(&store);
            let wal = Arc::clone(&wal);
            let barrier = Arc::clone(&barrier);
            let success_count = Arc::clone(&success_count);
            let key_a = key_a.clone();
            let key_b = key_b.clone();

            thread::spawn(move || {
                let snapshot = store.create_snapshot();
                let mut txn = TransactionContext::with_snapshot(
                    manager.next_txn_id(),
                    run_id,
                    Box::new(snapshot),
                );

                let _ = txn.get(&key_b).unwrap(); // Read B
                txn.put(key_a.clone(), Value::Int(888)).unwrap(); // Write A

                barrier.wait();

                let mut wal_guard = wal.lock();
                if manager.commit(&mut txn, store.as_ref(), &mut wal_guard).is_ok() {
                    success_count.fetch_add(1, Ordering::SeqCst);
                }
            })
        };

        h1.join().unwrap();
        h2.join().unwrap();

        // Exactly one should succeed (first-committer-wins)
        // T1 writes B, T2 reads B -> whoever commits second fails
        assert_eq!(
            success_count.load(Ordering::SeqCst),
            1,
            "Exactly one transaction should succeed due to cross-read conflict"
        );
    }

    /// Shared read scenario: Both transactions read the same key, write different keys
    /// T1: Read C, Write A
    /// T2: Read C, Write B
    /// Both should succeed (no conflict - reading same key is fine if neither writes it)
    #[test]
    fn test_shared_read_no_write_conflict() {
        let (store, wal, _temp) = create_shared_env();
        let run_id = RunId::new();
        let ns = create_namespace(run_id);
        let key_a = create_key(&ns, "key_a");
        let key_b = create_key(&ns, "key_b");
        let key_c = create_key(&ns, "key_c"); // Shared read key

        // Setup initial data
        store.put(key_a.clone(), Value::Int(100), None).unwrap();
        store.put(key_b.clone(), Value::Int(200), None).unwrap();
        store.put(key_c.clone(), Value::Int(300), None).unwrap();

        let manager = create_manager(&store);

        let barrier = Arc::new(Barrier::new(2));
        let t1_success = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let t2_success = Arc::new(std::sync::atomic::AtomicBool::new(false));

        // T1: Read C, Write A
        let h1 = {
            let manager = Arc::clone(&manager);
            let store = Arc::clone(&store);
            let wal = Arc::clone(&wal);
            let barrier = Arc::clone(&barrier);
            let t1_success = Arc::clone(&t1_success);
            let key_a = key_a.clone();
            let key_c = key_c.clone();

            thread::spawn(move || {
                let snapshot = store.create_snapshot();
                let mut txn = TransactionContext::with_snapshot(
                    manager.next_txn_id(),
                    run_id,
                    Box::new(snapshot),
                );

                let _ = txn.get(&key_c).unwrap(); // Read shared key
                txn.put(key_a.clone(), Value::Int(999)).unwrap(); // Write A

                barrier.wait();

                let mut wal_guard = wal.lock();
                if manager.commit(&mut txn, store.as_ref(), &mut wal_guard).is_ok() {
                    t1_success.store(true, Ordering::SeqCst);
                }
            })
        };

        // T2: Read C, Write B
        let h2 = {
            let manager = Arc::clone(&manager);
            let store = Arc::clone(&store);
            let wal = Arc::clone(&wal);
            let barrier = Arc::clone(&barrier);
            let t2_success = Arc::clone(&t2_success);
            let key_b = key_b.clone();
            let key_c = key_c.clone();

            thread::spawn(move || {
                let snapshot = store.create_snapshot();
                let mut txn = TransactionContext::with_snapshot(
                    manager.next_txn_id(),
                    run_id,
                    Box::new(snapshot),
                );

                let _ = txn.get(&key_c).unwrap(); // Read shared key
                txn.put(key_b.clone(), Value::Int(888)).unwrap(); // Write B

                barrier.wait();

                let mut wal_guard = wal.lock();
                if manager.commit(&mut txn, store.as_ref(), &mut wal_guard).is_ok() {
                    t2_success.store(true, Ordering::SeqCst);
                }
            })
        };

        h1.join().unwrap();
        h2.join().unwrap();

        // Both should succeed - reading same key C is fine, writes are disjoint
        assert!(
            t1_success.load(Ordering::SeqCst),
            "T1 should succeed"
        );
        assert!(
            t2_success.load(Ordering::SeqCst),
            "T2 should succeed"
        );

        // All writes visible, C unchanged
        assert_eq!(store.get(&key_a).unwrap().unwrap().value, Value::Int(999));
        assert_eq!(store.get(&key_b).unwrap().unwrap().value, Value::Int(888));
        assert_eq!(store.get(&key_c).unwrap().unwrap().value, Value::Int(300));
    }
}

// ============================================================================
// SECTION 11: Advanced Concurrent State Tests
// ============================================================================

mod concurrent_state {
    use super::*;

    /// Test that transaction IDs are globally unique across high concurrency
    #[test]
    fn test_txn_id_globally_unique_under_pressure() {
        let manager = Arc::new(TransactionManager::new(0));
        let num_threads = 50;
        let ids_per_thread = 100;
        let barrier = Arc::new(Barrier::new(num_threads));
        let all_ids = Arc::new(Mutex::new(Vec::new()));

        let handles: Vec<_> = (0..num_threads)
            .map(|_| {
                let manager = Arc::clone(&manager);
                let barrier = Arc::clone(&barrier);
                let all_ids = Arc::clone(&all_ids);

                thread::spawn(move || {
                    let mut thread_ids = Vec::new();

                    barrier.wait();

                    for _ in 0..ids_per_thread {
                        thread_ids.push(manager.next_txn_id());
                    }

                    all_ids.lock().extend(thread_ids);
                })
            })
            .collect();

        for handle in handles {
            handle.join().unwrap();
        }

        let ids = all_ids.lock();
        let unique_ids: HashSet<_> = ids.iter().cloned().collect();

        // All IDs should be unique
        assert_eq!(
            ids.len(),
            unique_ids.len(),
            "All {} transaction IDs should be unique",
            ids.len()
        );

        // Should have exactly num_threads * ids_per_thread IDs
        assert_eq!(
            ids.len(),
            num_threads * ids_per_thread,
            "Should have all expected IDs"
        );
    }

    /// Test version allocation is strictly monotonic even under high load
    #[test]
    fn test_version_allocation_strictly_monotonic() {
        let manager = Arc::new(TransactionManager::new(0));
        let num_threads = 20;
        let allocations_per_thread = 50;
        let barrier = Arc::new(Barrier::new(num_threads));
        let all_versions = Arc::new(Mutex::new(Vec::new()));

        let handles: Vec<_> = (0..num_threads)
            .map(|_| {
                let manager = Arc::clone(&manager);
                let barrier = Arc::clone(&barrier);
                let all_versions = Arc::clone(&all_versions);

                thread::spawn(move || {
                    let mut thread_versions = Vec::new();

                    barrier.wait();

                    for _ in 0..allocations_per_thread {
                        thread_versions.push(manager.allocate_version());
                    }

                    all_versions.lock().extend(thread_versions);
                })
            })
            .collect();

        for handle in handles {
            handle.join().unwrap();
        }

        let versions = all_versions.lock();
        let unique_versions: HashSet<_> = versions.iter().cloned().collect();

        // All versions should be unique
        assert_eq!(
            versions.len(),
            unique_versions.len(),
            "All versions should be unique"
        );

        // Versions should be sequential (1 through N)
        let min = *versions.iter().min().unwrap();
        let max = *versions.iter().max().unwrap();
        assert_eq!(min, 1, "Minimum version should be 1");
        assert_eq!(
            max,
            (num_threads * allocations_per_thread) as u64,
            "Maximum version should equal total allocations"
        );
    }

    /// Test that read-only transactions never block writers
    #[test]
    fn test_readonly_never_blocks_writers() {
        let (store, wal, _temp) = create_shared_env();
        let run_id = RunId::new();
        let ns = create_namespace(run_id);
        let key = create_key(&ns, "readonly_test");

        // Setup initial data
        store.put(key.clone(), Value::Int(0), None).unwrap();
        let manager = create_manager(&store);

        let num_readers = 20;
        let num_writers = 5;
        let total = num_readers + num_writers;
        let barrier = Arc::new(Barrier::new(total));
        let reader_commits = Arc::new(AtomicUsize::new(0));
        let writer_commits = Arc::new(AtomicUsize::new(0));

        let mut handles = Vec::new();

        // Spawn many readers that hold snapshots for a while
        for _ in 0..num_readers {
            let manager = Arc::clone(&manager);
            let store = Arc::clone(&store);
            let wal = Arc::clone(&wal);
            let barrier = Arc::clone(&barrier);
            let reader_commits = Arc::clone(&reader_commits);
            let key = key.clone();

            handles.push(thread::spawn(move || {
                let snapshot = store.create_snapshot();
                let mut txn = TransactionContext::with_snapshot(
                    manager.next_txn_id(),
                    run_id,
                    Box::new(snapshot),
                );

                // Read key multiple times
                for _ in 0..10 {
                    let _ = txn.get(&key);
                    thread::yield_now(); // Allow other threads to run
                }

                barrier.wait();

                // Commit read-only transaction
                let mut wal_guard = wal.lock();
                if manager.commit(&mut txn, store.as_ref(), &mut wal_guard).is_ok() {
                    reader_commits.fetch_add(1, Ordering::SeqCst);
                }
            }));
        }

        // Spawn writers
        for i in 0..num_writers {
            let manager = Arc::clone(&manager);
            let store = Arc::clone(&store);
            let wal = Arc::clone(&wal);
            let barrier = Arc::clone(&barrier);
            let writer_commits = Arc::clone(&writer_commits);
            let ns = ns.clone();

            handles.push(thread::spawn(move || {
                // Write to different key (no conflict)
                let writer_key = create_key(&ns, &format!("writer_{}", i));
                let mut txn = TransactionContext::new(
                    manager.next_txn_id(),
                    run_id,
                    store.current_version(),
                );
                txn.put(writer_key, Value::Int(i as i64)).unwrap();

                barrier.wait();

                let mut wal_guard = wal.lock();
                if manager.commit(&mut txn, store.as_ref(), &mut wal_guard).is_ok() {
                    writer_commits.fetch_add(1, Ordering::SeqCst);
                }
            }));
        }

        for handle in handles {
            handle.join().unwrap();
        }

        // All readers should succeed (read-only always commits per spec)
        assert_eq!(
            reader_commits.load(Ordering::SeqCst),
            num_readers,
            "All read-only transactions should succeed"
        );

        // All writers should succeed (writing to different keys)
        assert_eq!(
            writer_commits.load(Ordering::SeqCst),
            num_writers,
            "All writers should succeed (different keys)"
        );
    }
}

// ============================================================================
// SECTION 12: Concurrent Error Path Tests
// ============================================================================

mod concurrent_error_paths {
    use super::*;

    /// Test that failed commits don't corrupt state
    #[test]
    fn test_failed_commits_leave_state_clean() {
        let (store, wal, _temp) = create_shared_env();
        let run_id = RunId::new();
        let ns = create_namespace(run_id);
        let key = create_key(&ns, "error_test");

        // Setup initial data
        store.put(key.clone(), Value::Int(42), None).unwrap();
        let manager = create_manager(&store);
        let initial_version = store.current_version();

        let num_threads = 20;
        let barrier = Arc::new(Barrier::new(num_threads));
        let success_count = Arc::new(AtomicUsize::new(0));

        let handles: Vec<_> = (0..num_threads)
            .map(|i| {
                let manager = Arc::clone(&manager);
                let store = Arc::clone(&store);
                let wal = Arc::clone(&wal);
                let barrier = Arc::clone(&barrier);
                let success_count = Arc::clone(&success_count);
                let key = key.clone();

                thread::spawn(move || {
                    let snapshot = store.create_snapshot();
                    let mut txn = TransactionContext::with_snapshot(
                        manager.next_txn_id(),
                        run_id,
                        Box::new(snapshot),
                    );

                    // Read and write (creates conflict)
                    let _ = txn.get(&key).unwrap();
                    txn.put(key.clone(), Value::Int(i as i64 * 100)).unwrap();

                    barrier.wait();

                    let mut wal_guard = wal.lock();
                    if manager.commit(&mut txn, store.as_ref(), &mut wal_guard).is_ok() {
                        success_count.fetch_add(1, Ordering::SeqCst);
                    }
                })
            })
            .collect();

        for handle in handles {
            handle.join().unwrap();
        }

        // Only one should succeed
        assert_eq!(
            success_count.load(Ordering::SeqCst),
            1,
            "Only one commit should succeed"
        );

        // State should be clean - exactly one modification
        let final_version = store.current_version();
        assert_eq!(
            final_version,
            initial_version + 1,
            "Version should increment exactly once"
        );

        // Value should be from the successful transaction (not corrupted)
        let final_value = store.get(&key).unwrap().unwrap();
        assert!(
            matches!(final_value.value, Value::Int(_)),
            "Value should be intact"
        );
    }

    /// Test concurrent transactions with mixed success/failure
    #[test]
    fn test_mixed_success_failure_concurrent() {
        let (store, wal, _temp) = create_shared_env();
        let run_id = RunId::new();
        let ns = create_namespace(run_id);

        // Create 10 keys
        let keys: Vec<_> = (0..10)
            .map(|i| {
                let key = create_key(&ns, &format!("key_{}", i));
                store.put(key.clone(), Value::Int(i), None).unwrap();
                key
            })
            .collect();

        let manager = create_manager(&store);

        let num_threads = 30;
        let barrier = Arc::new(Barrier::new(num_threads));
        let success_count = Arc::new(AtomicUsize::new(0));
        let failure_count = Arc::new(AtomicUsize::new(0));

        let handles: Vec<_> = (0..num_threads)
            .map(|i| {
                let manager = Arc::clone(&manager);
                let store = Arc::clone(&store);
                let wal = Arc::clone(&wal);
                let barrier = Arc::clone(&barrier);
                let success_count = Arc::clone(&success_count);
                let failure_count = Arc::clone(&failure_count);
                let keys = keys.clone();

                thread::spawn(move || {
                    let snapshot = store.create_snapshot();
                    let mut txn = TransactionContext::with_snapshot(
                        manager.next_txn_id(),
                        run_id,
                        Box::new(snapshot),
                    );

                    // Read and write to a subset of keys (some will conflict)
                    let key_idx = i % 10;
                    let _ = txn.get(&keys[key_idx]).unwrap();
                    txn.put(keys[key_idx].clone(), Value::Int(i as i64 * 100))
                        .unwrap();

                    barrier.wait();

                    let mut wal_guard = wal.lock();
                    if manager.commit(&mut txn, store.as_ref(), &mut wal_guard).is_ok() {
                        success_count.fetch_add(1, Ordering::SeqCst);
                    } else {
                        failure_count.fetch_add(1, Ordering::SeqCst);
                    }
                })
            })
            .collect();

        for handle in handles {
            handle.join().unwrap();
        }

        let successes = success_count.load(Ordering::SeqCst);
        let failures = failure_count.load(Ordering::SeqCst);

        // All transactions should complete (success or failure)
        assert_eq!(
            successes + failures,
            num_threads,
            "All transactions should complete"
        );

        // At least one success per key group (10 keys, 3 threads per key)
        // Exactly 10 should succeed (one per key, first-committer-wins)
        assert_eq!(
            successes, 10,
            "Exactly one transaction per key should succeed"
        );
    }
}

// ============================================================================
// SECTION 13: Concurrent Ordering Guarantees
// ============================================================================

mod concurrent_ordering {
    use super::*;

    /// Test that commit order is deterministic for conflicting transactions
    #[test]
    fn test_commit_order_serialized() {
        let (store, wal, _temp) = create_shared_env();
        let run_id = RunId::new();
        let ns = create_namespace(run_id);
        let key = create_key(&ns, "serial_key");

        // Setup initial data
        store.put(key.clone(), Value::Int(0), None).unwrap();
        let manager = create_manager(&store);

        let num_successful_commits = Arc::new(AtomicUsize::new(0));
        let committed_versions = Arc::new(Mutex::new(Vec::new()));

        // Run multiple rounds to verify deterministic behavior
        for _ in 0..5 {
            num_successful_commits.store(0, Ordering::SeqCst);
            committed_versions.lock().clear();

            let num_threads = 10;
            let barrier = Arc::new(Barrier::new(num_threads));

            let handles: Vec<_> = (0..num_threads)
                .map(|i| {
                    let manager = Arc::clone(&manager);
                    let store = Arc::clone(&store);
                    let wal = Arc::clone(&wal);
                    let barrier = Arc::clone(&barrier);
                    let num_successful_commits = Arc::clone(&num_successful_commits);
                    let committed_versions = Arc::clone(&committed_versions);
                    let key = key.clone();

                    thread::spawn(move || {
                        let snapshot = store.create_snapshot();
                        let mut txn = TransactionContext::with_snapshot(
                            manager.next_txn_id(),
                            run_id,
                            Box::new(snapshot),
                        );

                        let _ = txn.get(&key).unwrap();
                        txn.put(key.clone(), Value::Int(i as i64)).unwrap();

                        barrier.wait();

                        let mut wal_guard = wal.lock();
                        if let Ok(version) = manager.commit(&mut txn, store.as_ref(), &mut wal_guard)
                        {
                            num_successful_commits.fetch_add(1, Ordering::SeqCst);
                            committed_versions.lock().push(version);
                        }
                    })
                })
                .collect();

            for handle in handles {
                handle.join().unwrap();
            }

            // Exactly one should succeed per round
            assert_eq!(
                num_successful_commits.load(Ordering::SeqCst),
                1,
                "Exactly one commit should succeed per round"
            );

            // Version should have incremented
            let versions = committed_versions.lock();
            assert_eq!(versions.len(), 1, "Should have exactly one committed version");
        }
    }

    /// Test that the commit lock provides proper serialization
    #[test]
    fn test_commit_lock_serialization() {
        let (store, wal, _temp) = create_shared_env();
        let run_id = RunId::new();
        let ns = create_namespace(run_id);

        // Setup multiple keys
        for i in 0..5 {
            let key = create_key(&ns, &format!("serial_{}", i));
            store.put(key, Value::Int(i), None).unwrap();
        }

        let manager = create_manager(&store);

        let num_threads = 20;
        let barrier = Arc::new(Barrier::new(num_threads));
        let commit_sequence = Arc::new(Mutex::new(Vec::new()));

        let handles: Vec<_> = (0..num_threads)
            .map(|i| {
                let manager = Arc::clone(&manager);
                let store = Arc::clone(&store);
                let wal = Arc::clone(&wal);
                let barrier = Arc::clone(&barrier);
                let commit_sequence = Arc::clone(&commit_sequence);
                let ns = ns.clone();

                thread::spawn(move || {
                    // Each thread writes to its own unique key (no conflicts)
                    let key = create_key(&ns, &format!("thread_{}", i));
                    let mut txn = TransactionContext::new(
                        manager.next_txn_id(),
                        run_id,
                        store.current_version(),
                    );
                    txn.put(key, Value::Int(i as i64)).unwrap();

                    barrier.wait();

                    let mut wal_guard = wal.lock();
                    if let Ok(version) = manager.commit(&mut txn, store.as_ref(), &mut wal_guard) {
                        commit_sequence.lock().push((i, version));
                    }
                })
            })
            .collect();

        for handle in handles {
            handle.join().unwrap();
        }

        let sequence = commit_sequence.lock();

        // All should have committed (no conflicts)
        assert_eq!(sequence.len(), num_threads, "All commits should succeed");

        // Versions should be unique and sequential
        let mut versions: Vec<_> = sequence.iter().map(|(_, v)| *v).collect();
        versions.sort();

        for i in 1..versions.len() {
            assert_eq!(
                versions[i],
                versions[i - 1] + 1,
                "Versions should be sequential"
            );
        }
    }
}
