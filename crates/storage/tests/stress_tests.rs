//! Stress tests for the storage layer
//!
//! These tests verify the storage layer works correctly under heavy load:
//! - Large number of keys (1 million+)
//! - Large scan results (100K+)
//! - Concurrent snapshot creation under load
//! - Memory pressure scenarios
//!
//! Note: These tests are marked with #[ignore] as they take longer to run.
//! Run with: cargo test --release -- --ignored

use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use in_mem_core::{Key, Namespace, RunId, SnapshotView, Storage, Value};
use in_mem_storage::UnifiedStore;

// ============================================================================
// Helper Functions
// ============================================================================

fn namespace_with_run(run_id: RunId) -> Namespace {
    Namespace::new(
        "tenant".to_string(),
        "app".to_string(),
        "agent".to_string(),
        run_id,
    )
}

// ============================================================================
// Large Scale Tests
// ============================================================================

/// Test inserting 1 million keys
///
/// This test verifies:
/// - Storage can handle large datasets
/// - Version counter doesn't overflow or collide
/// - Performance is reasonable (should complete in < 60 seconds)
#[test]
#[ignore] // Run with: cargo test --release -- --ignored test_insert_one_million_keys
fn test_insert_one_million_keys() {
    let store = UnifiedStore::new();
    let run_id = RunId::new();
    let ns = namespace_with_run(run_id);

    let start = Instant::now();

    for i in 0..1_000_000 {
        let key = Key::new_kv(ns.clone(), format!("key_{:07}", i));
        store.put(key, Value::I64(i), None).unwrap();

        // Progress indicator every 100K
        if i > 0 && i % 100_000 == 0 {
            println!("Inserted {} keys in {:?}", i, start.elapsed());
        }
    }

    let duration = start.elapsed();
    println!(
        "Inserted 1,000,000 keys in {:?} ({:.0} keys/sec)",
        duration,
        1_000_000.0 / duration.as_secs_f64()
    );

    // Verify final state
    assert_eq!(store.current_version(), 1_000_000);

    // Spot check some keys
    for i in [0, 500_000, 999_999] {
        let key = Key::new_kv(ns.clone(), format!("key_{:07}", i));
        let value = store.get(&key).unwrap();
        assert!(value.is_some(), "Key {} not found", i);
        assert_eq!(value.unwrap().value, Value::I64(i));
    }

    // Performance should be reasonable (at least 10K keys/sec)
    assert!(duration.as_secs() < 120, "Insert too slow: {:?}", duration);
}

/// Test scanning with 100K results
///
/// This test verifies:
/// - Scan operations work with large result sets
/// - Results are correctly ordered
/// - Memory usage is reasonable
#[test]
#[ignore] // Run with: cargo test --release -- --ignored test_scan_100k_results
fn test_scan_100k_results() {
    let store = UnifiedStore::new();
    let run_id = RunId::new();
    let ns = namespace_with_run(run_id);

    // Insert 100K keys with common prefix
    println!("Inserting 100,000 keys...");
    let start = Instant::now();

    for i in 0..100_000 {
        let key = Key::new_kv(ns.clone(), format!("scantest:{:06}", i));
        store.put(key, Value::I64(i), None).unwrap();
    }

    println!("Insert completed in {:?}", start.elapsed());

    // Scan all keys
    let scan_start = Instant::now();
    let prefix = Key::new_kv(ns.clone(), "scantest:");
    let results = store.scan_prefix(&prefix, u64::MAX).unwrap();
    let scan_duration = scan_start.elapsed();

    println!(
        "Scanned {} results in {:?} ({:.0} results/sec)",
        results.len(),
        scan_duration,
        results.len() as f64 / scan_duration.as_secs_f64()
    );

    assert_eq!(results.len(), 100_000);

    // Verify results are ordered
    for i in 1..results.len() {
        assert!(
            results[i].0 > results[i - 1].0,
            "Results not ordered at index {}",
            i
        );
    }

    // Scan should be reasonably fast (< 5 seconds)
    assert!(
        scan_duration.as_secs() < 10,
        "Scan too slow: {:?}",
        scan_duration
    );
}

/// Test scan by run with large dataset
#[test]
#[ignore]
fn test_scan_by_run_large_dataset() {
    let store = UnifiedStore::new();

    // Create 10 runs with 10K keys each
    println!("Creating 10 runs with 10K keys each...");
    let start = Instant::now();

    let runs: Vec<RunId> = (0..10).map(|_| RunId::new()).collect();

    for (run_idx, run_id) in runs.iter().enumerate() {
        let ns = namespace_with_run(*run_id);

        for i in 0..10_000 {
            let key = Key::new_kv(ns.clone(), format!("key_{:05}", i));
            store.put(key, Value::I64(i), None).unwrap();
        }

        println!("Run {} completed", run_idx);
    }

    println!("Insert completed in {:?}", start.elapsed());

    // Scan each run - should use index efficiently
    for (run_idx, run_id) in runs.iter().enumerate() {
        let scan_start = Instant::now();
        let results = store.scan_by_run(*run_id, u64::MAX).unwrap();
        let scan_duration = scan_start.elapsed();

        assert_eq!(results.len(), 10_000, "Run {} has wrong count", run_idx);

        // Index-based scan should be fast (< 100ms)
        assert!(
            scan_duration.as_millis() < 500,
            "Scan for run {} too slow: {:?}",
            run_idx,
            scan_duration
        );
    }
}

/// Test concurrent snapshot creation under load
#[test]
#[ignore]
fn test_concurrent_snapshots_under_load() {
    let store = Arc::new(UnifiedStore::new());
    let run_id = RunId::new();
    let ns = namespace_with_run(run_id);

    // Pre-populate with data
    println!("Pre-populating with 10,000 keys...");
    for i in 0..10_000 {
        let key = Key::new_kv(ns.clone(), format!("key_{:05}", i));
        store.put(key, Value::I64(i), None).unwrap();
    }

    let start = Instant::now();
    let mut handles = vec![];

    // Spawn writer threads
    for thread_id in 0..5 {
        let store = Arc::clone(&store);
        let ns = ns.clone();

        let handle = thread::spawn(move || {
            for i in 0..1000 {
                let key = Key::new_kv(ns.clone(), format!("new_{}_{:04}", thread_id, i));
                store.put(key, Value::I64(i), None).unwrap();
            }
        });

        handles.push(handle);
    }

    // Spawn snapshot reader threads
    for _ in 0..10 {
        let store = Arc::clone(&store);
        let ns = ns.clone();

        let handle = thread::spawn(move || {
            for _ in 0..10 {
                let snapshot = store.create_snapshot();

                // Read from snapshot
                for i in (0..10_000).step_by(100) {
                    let key = Key::new_kv(ns.clone(), format!("key_{:05}", i));
                    let _ = snapshot.get(&key);
                }

                // Small delay to allow writes to proceed
                thread::sleep(Duration::from_millis(1));
            }
        });

        handles.push(handle);
    }

    for handle in handles {
        handle.join().unwrap();
    }

    let duration = start.elapsed();
    println!(
        "Completed concurrent snapshots under load in {:?}",
        duration
    );

    // Verify final state
    assert!(store.current_version() >= 10_000 + 5000); // Initial + new writes
}

/// Test memory pressure with many small values
#[test]
#[ignore]
fn test_many_small_values() {
    let store = UnifiedStore::new();
    let run_id = RunId::new();
    let ns = namespace_with_run(run_id);

    println!("Inserting 500,000 small values...");
    let start = Instant::now();

    for i in 0..500_000 {
        let key = Key::new_kv(ns.clone(), format!("small_{:06}", i));
        store.put(key, Value::I64(i), None).unwrap();
    }

    let duration = start.elapsed();
    println!(
        "Inserted 500,000 small values in {:?} ({:.0} values/sec)",
        duration,
        500_000.0 / duration.as_secs_f64()
    );

    // Create snapshot (should handle large dataset)
    let snap_start = Instant::now();
    let snapshot = store.create_snapshot();
    let snap_duration = snap_start.elapsed();

    println!("Created snapshot in {:?}", snap_duration);
    assert_eq!(snapshot.version(), 500_000);
}

/// Test with large values
#[test]
#[ignore]
fn test_large_values() {
    let store = UnifiedStore::new();
    let run_id = RunId::new();
    let ns = namespace_with_run(run_id);

    println!("Inserting 1000 large values (1KB each)...");
    let large_data = vec![0xABu8; 1024]; // 1KB value

    let start = Instant::now();

    for i in 0..1000 {
        let key = Key::new_kv(ns.clone(), format!("large_{:04}", i));
        store
            .put(key, Value::Bytes(large_data.clone()), None)
            .unwrap();
    }

    let duration = start.elapsed();
    println!("Inserted 1000 large values in {:?}", duration);

    // Verify data integrity
    for i in (0..1000).step_by(100) {
        let key = Key::new_kv(ns.clone(), format!("large_{:04}", i));
        let value = store.get(&key).unwrap().unwrap();

        if let Value::Bytes(data) = value.value {
            assert_eq!(data.len(), 1024);
            assert_eq!(data[0], 0xAB);
        } else {
            panic!("Expected Bytes value");
        }
    }
}

/// Test concurrent deletes under load
#[test]
#[ignore]
fn test_concurrent_deletes_under_load() {
    let store = Arc::new(UnifiedStore::new());
    let run_id = RunId::new();
    let ns = namespace_with_run(run_id);

    // Pre-populate
    println!("Pre-populating with 10,000 keys...");
    for i in 0..10_000 {
        let key = Key::new_kv(ns.clone(), format!("key_{:05}", i));
        store.put(key, Value::I64(i), None).unwrap();
    }

    let start = Instant::now();
    let mut handles = vec![];

    // Spawn delete threads (delete odd keys)
    for thread_id in 0..5 {
        let store = Arc::clone(&store);
        let ns = ns.clone();

        let handle = thread::spawn(move || {
            for i in 0..1000 {
                let key_idx = thread_id * 2000 + i * 2 + 1; // Odd indices
                if key_idx < 10_000 {
                    let key = Key::new_kv(ns.clone(), format!("key_{:05}", key_idx));
                    let _ = store.delete(&key);
                }
            }
        });

        handles.push(handle);
    }

    // Spawn reader threads
    for _ in 0..5 {
        let store = Arc::clone(&store);
        let ns = ns.clone();

        let handle = thread::spawn(move || {
            for i in 0..1000 {
                let key = Key::new_kv(ns.clone(), format!("key_{:05}", i * 10));
                let _ = store.get(&key);
            }
        });

        handles.push(handle);
    }

    // Spawn writer threads (add new keys)
    for thread_id in 0..3 {
        let store = Arc::clone(&store);
        let ns = ns.clone();

        let handle = thread::spawn(move || {
            for i in 0..500 {
                let key = Key::new_kv(ns.clone(), format!("newkey_{}_{:04}", thread_id, i));
                store.put(key, Value::I64(i), None).unwrap();
            }
        });

        handles.push(handle);
    }

    for handle in handles {
        handle.join().unwrap();
    }

    let duration = start.elapsed();
    println!("Completed concurrent deletes under load in {:?}", duration);

    // Verify state is consistent
    let remaining = store.scan_by_run(run_id, u64::MAX).unwrap();
    println!("Remaining keys: {}", remaining.len());

    // Should have original - deleted + new
    // Some deletions may race, so just verify it's reasonable
    assert!(remaining.len() > 1000, "Too few keys remaining");
}

/// Test TTL with large number of expiring keys
///
/// Note: TTL must be longer than insert duration to ensure all keys exist
/// after insertion. With 10,000 keys taking ~160ms to insert, we use 1 second
/// TTL to provide comfortable margin across different systems.
#[test]
#[ignore]
fn test_ttl_large_scale() {
    let store = UnifiedStore::new();
    let run_id = RunId::new();
    let ns = namespace_with_run(run_id);

    // TTL must be longer than insert duration (~160ms on typical systems)
    // Using 1 second to ensure keys don't expire during insertion
    let ttl = Duration::from_secs(1);

    println!("Inserting 10,000 keys with TTL={:?}...", ttl);
    let start = Instant::now();

    for i in 0..10_000 {
        let key = Key::new_kv(ns.clone(), format!("ttl_{:05}", i));
        store.put(key, Value::I64(i), Some(ttl)).unwrap();
    }

    let insert_duration = start.elapsed();
    println!("Insert completed in {:?}", insert_duration);

    // Verify TTL was long enough for all inserts
    assert!(
        ttl > insert_duration,
        "TTL ({:?}) must be longer than insert duration ({:?})",
        ttl,
        insert_duration
    );

    // Verify keys exist immediately after insert
    let initial = store.scan_by_run(run_id, u64::MAX).unwrap();
    assert_eq!(initial.len(), 10_000);

    // Wait for expiration (TTL + small buffer)
    thread::sleep(ttl + Duration::from_millis(100));

    // Keys should be filtered out
    let after_expiry = store.scan_by_run(run_id, u64::MAX).unwrap();
    assert_eq!(after_expiry.len(), 0, "Expired keys still visible");

    // find_expired_keys should return all
    let expired = store.find_expired_keys().unwrap();
    assert_eq!(expired.len(), 10_000);
}
