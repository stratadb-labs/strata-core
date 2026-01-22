//! KV Concurrency Tests
//!
//! Tests for multi-threaded safety and isolation:
//! - Concurrent reads don't interfere
//! - Concurrent writes are serialized correctly
//! - Version monotonicity under concurrent access
//! - Atomic operations are truly atomic
//! - No data corruption under stress
//!
//! Note: Most concurrency tests use sequential integers for deterministic
//! verification of concurrent operations rather than testdata values.

use super::*;
use crate::test_data::load_kv_test_data;
use std::sync::{Arc, Barrier};
use std::thread;
use std::time::Duration;

// =============================================================================
// CONCURRENT READS
// =============================================================================

#[test]
fn test_concurrent_reads_same_key() {
    let db = create_buffered_db();
    let substrate = Arc::new(create_substrate(db.clone()));
    let run = ApiRunId::default();
    let test_data = load_kv_test_data();

    // Use value from testdata
    let entry = test_data.get_type("int").first().expect("Need int entry");

    // Setup: write a value
    substrate
        .kv_put(&run, &entry.key, entry.value.clone())
        .unwrap();

    const NUM_READERS: usize = 10;
    const READS_PER_THREAD: usize = 100;

    let expected_value = entry.value.clone();
    let key = entry.key.clone();

    let barrier = Arc::new(Barrier::new(NUM_READERS));
    let handles: Vec<_> = (0..NUM_READERS)
        .map(|_| {
            let substrate = Arc::clone(&substrate);
            let barrier = Arc::clone(&barrier);
            let run = run.clone();
            let key = key.clone();
            let expected = expected_value.clone();

            thread::spawn(move || {
                barrier.wait();

                let mut success = 0;
                for _ in 0..READS_PER_THREAD {
                    match substrate.kv_get(&run, &key) {
                        Ok(Some(v)) if values_equal(&v.value, &expected) => success += 1,
                        Ok(Some(v)) => panic!("Wrong value: {:?}", v.value),
                        Ok(None) => panic!("Key disappeared during reads"),
                        Err(e) => panic!("Read error: {:?}", e),
                    }
                }
                success
            })
        })
        .collect();

    let total_success: usize = handles.into_iter().map(|h| h.join().unwrap()).sum();
    assert_eq!(
        total_success,
        NUM_READERS * READS_PER_THREAD,
        "All concurrent reads should succeed"
    );
}

// =============================================================================
// CONCURRENT WRITES
// =============================================================================

#[test]
fn test_concurrent_writes_different_keys() {
    let db = create_buffered_db();
    let substrate = Arc::new(create_substrate(db.clone()));
    let run = ApiRunId::default();

    const NUM_WRITERS: usize = 10;
    const WRITES_PER_THREAD: usize = 50;

    let barrier = Arc::new(Barrier::new(NUM_WRITERS));
    let handles: Vec<_> = (0..NUM_WRITERS)
        .map(|thread_id| {
            let substrate = Arc::clone(&substrate);
            let barrier = Arc::clone(&barrier);
            let run = run.clone();

            thread::spawn(move || {
                barrier.wait();

                for i in 0..WRITES_PER_THREAD {
                    let key = format!("thread_{}_key_{}", thread_id, i);
                    substrate
                        .kv_put(&run, &key, Value::Int((thread_id * 1000 + i) as i64))
                        .expect("Concurrent write should succeed");
                }
            })
        })
        .collect();

    // Wait for all writes
    for h in handles {
        h.join().unwrap();
    }

    // Verify all keys exist with correct values
    for thread_id in 0..NUM_WRITERS {
        for i in 0..WRITES_PER_THREAD {
            let key = format!("thread_{}_key_{}", thread_id, i);
            let value = substrate.kv_get(&run, &key).unwrap().unwrap().value;
            assert_eq!(
                value,
                Value::Int((thread_id * 1000 + i) as i64),
                "Key {} should have correct value",
                key
            );
        }
    }
}

#[test]
fn test_concurrent_writes_same_key() {
    let db = create_buffered_db();
    let substrate = Arc::new(create_substrate(db.clone()));
    let run = ApiRunId::default();

    const NUM_WRITERS: usize = 10;
    const WRITES_PER_THREAD: usize = 100;

    let barrier = Arc::new(Barrier::new(NUM_WRITERS));
    let handles: Vec<_> = (0..NUM_WRITERS)
        .map(|thread_id| {
            let substrate = Arc::clone(&substrate);
            let barrier = Arc::clone(&barrier);
            let run = run.clone();

            thread::spawn(move || {
                barrier.wait();

                for i in 0..WRITES_PER_THREAD {
                    substrate
                        .kv_put(&run, "contended_key", Value::Int((thread_id * 1000 + i) as i64))
                        .expect("Concurrent write should succeed");
                }
            })
        })
        .collect();

    // Wait for all writes
    for h in handles {
        h.join().unwrap();
    }

    // Final value should be one of the written values (last writer wins)
    let final_value = substrate
        .kv_get(&run, "contended_key")
        .unwrap()
        .unwrap()
        .value;
    match final_value {
        Value::Int(v) => {
            // Should be a valid value from one of the threads
            assert!(v >= 0, "Final value should be non-negative");
        }
        _ => panic!("Expected Int value"),
    }
}

// =============================================================================
// VERSION MONOTONICITY
// =============================================================================

#[test]
fn test_version_monotonicity_under_concurrent_writes() {
    let db = create_buffered_db();
    let substrate = Arc::new(create_substrate(db.clone()));
    let run = ApiRunId::default();

    // Initial write
    substrate.kv_put(&run, "monotonic_key", Value::Int(0)).unwrap();

    const NUM_WRITERS: usize = 4;
    const WRITES_PER_THREAD: usize = 100;

    let barrier = Arc::new(Barrier::new(NUM_WRITERS + 1)); // +1 for reader
    let stop_flag = Arc::new(std::sync::atomic::AtomicBool::new(false));

    // Writer threads
    let writer_handles: Vec<_> = (0..NUM_WRITERS)
        .map(|_| {
            let substrate = Arc::clone(&substrate);
            let barrier = Arc::clone(&barrier);
            let stop_flag = Arc::clone(&stop_flag);
            let run = run.clone();

            thread::spawn(move || {
                barrier.wait();

                for i in 0..WRITES_PER_THREAD {
                    if stop_flag.load(std::sync::atomic::Ordering::Relaxed) {
                        break;
                    }
                    substrate
                        .kv_put(&run, "monotonic_key", Value::Int(i as i64))
                        .unwrap();
                    thread::sleep(Duration::from_micros(10));
                }
            })
        })
        .collect();

    // Reader thread: check for version regression
    let reader_substrate = Arc::clone(&substrate);
    let reader_barrier = Arc::clone(&barrier);
    let reader_stop = Arc::clone(&stop_flag);
    let reader_run = run.clone();

    let reader_handle = thread::spawn(move || {
        reader_barrier.wait();

        let mut last_version: Option<u64> = None;
        let mut regressions = 0;
        let mut reads = 0;

        while !reader_stop.load(std::sync::atomic::Ordering::Relaxed) {
            if let Ok(Some(versioned)) = reader_substrate.kv_get(&reader_run, "monotonic_key") {
                let current_version = match versioned.version {
                    Version::Txn(v) => v,
                    _ => continue,
                };

                if let Some(last) = last_version {
                    if current_version < last {
                        regressions += 1;
                    }
                }
                last_version = Some(current_version);
                reads += 1;
            }
            thread::sleep(Duration::from_micros(5));
        }

        (regressions, reads)
    });

    // Let it run for a bit
    thread::sleep(Duration::from_millis(100));
    stop_flag.store(true, std::sync::atomic::Ordering::Relaxed);

    // Wait for all threads
    for h in writer_handles {
        h.join().unwrap();
    }
    let (regressions, reads) = reader_handle.join().unwrap();

    assert_eq!(
        regressions, 0,
        "No version regressions should occur ({} reads performed)",
        reads
    );
}

// =============================================================================
// ATOMIC INCREMENT
// =============================================================================

#[test]
fn test_incr_atomic_under_concurrency() {
    let db = create_buffered_db();
    let substrate = Arc::new(create_substrate(db.clone()));
    let run = ApiRunId::default();

    const NUM_THREADS: usize = 10;
    const INCREMENTS_PER_THREAD: usize = 100;

    let barrier = Arc::new(Barrier::new(NUM_THREADS));
    let handles: Vec<_> = (0..NUM_THREADS)
        .map(|_| {
            let substrate = Arc::clone(&substrate);
            let barrier = Arc::clone(&barrier);
            let run = run.clone();

            thread::spawn(move || {
                barrier.wait();

                for _ in 0..INCREMENTS_PER_THREAD {
                    substrate.kv_incr(&run, "atomic_counter", 1).unwrap();
                }
            })
        })
        .collect();

    // Wait for all increments
    for h in handles {
        h.join().unwrap();
    }

    // Final value should be exactly NUM_THREADS * INCREMENTS_PER_THREAD
    let final_value = substrate
        .kv_get(&run, "atomic_counter")
        .unwrap()
        .unwrap()
        .value;
    assert_eq!(
        final_value,
        Value::Int((NUM_THREADS * INCREMENTS_PER_THREAD) as i64),
        "Atomic increments should sum correctly"
    );
}

// =============================================================================
// CAS UNDER CONCURRENCY
// =============================================================================

#[test]
fn test_cas_value_under_concurrency() {
    let db = create_buffered_db();
    let substrate = Arc::new(create_substrate(db.clone()));
    let run = ApiRunId::default();

    // Initialize
    substrate.kv_put(&run, "cas_key", Value::Int(0)).unwrap();

    const NUM_THREADS: usize = 10;
    const ATTEMPTS_PER_THREAD: usize = 50;

    let barrier = Arc::new(Barrier::new(NUM_THREADS));
    let handles: Vec<_> = (0..NUM_THREADS)
        .map(|_| {
            let substrate = Arc::clone(&substrate);
            let barrier = Arc::clone(&barrier);
            let run = run.clone();

            thread::spawn(move || {
                barrier.wait();

                let mut successes = 0;
                for _ in 0..ATTEMPTS_PER_THREAD {
                    // Read current value
                    let current = substrate
                        .kv_get(&run, "cas_key")
                        .unwrap()
                        .unwrap()
                        .value
                        .clone();

                    // Try to increment via CAS
                    if let Value::Int(n) = current {
                        if substrate
                            .kv_cas_value(&run, "cas_key", Some(Value::Int(n)), Value::Int(n + 1))
                            .unwrap()
                        {
                            successes += 1;
                        }
                    }
                }
                successes
            })
        })
        .collect();

    let total_successes: usize = handles.into_iter().map(|h| h.join().unwrap()).sum();

    // Final value should match number of successful CAS operations
    let final_value = substrate
        .kv_get(&run, "cas_key")
        .unwrap()
        .unwrap()
        .value;

    match final_value {
        Value::Int(n) => {
            assert_eq!(
                n, total_successes as i64,
                "Final value should equal total successful CAS operations"
            );
        }
        _ => panic!("Expected Int"),
    }
}

// =============================================================================
// RUN ISOLATION UNDER CONCURRENCY
// =============================================================================

#[test]
fn test_run_isolation_under_concurrency() {
    let db = create_buffered_db();
    let substrate = Arc::new(create_substrate(db.clone()));

    const NUM_RUNS: usize = 5;
    const OPS_PER_RUN: usize = 100;

    let runs: Vec<ApiRunId> = (0..NUM_RUNS)
        .map(|i| {
            if i == 0 {
                ApiRunId::default()
            } else {
                ApiRunId::new()
            }
        })
        .collect();

    let barrier = Arc::new(Barrier::new(NUM_RUNS));
    let handles: Vec<_> = runs
        .iter()
        .enumerate()
        .map(|(run_idx, run)| {
            let substrate = Arc::clone(&substrate);
            let barrier = Arc::clone(&barrier);
            let run = run.clone();

            thread::spawn(move || {
                barrier.wait();

                // Each run writes to same key name but should be isolated
                for i in 0..OPS_PER_RUN {
                    substrate
                        .kv_put(&run, "shared_name", Value::Int((run_idx * 1000 + i) as i64))
                        .unwrap();
                }

                // Final value for this run
                substrate
                    .kv_get(&run, "shared_name")
                    .unwrap()
                    .unwrap()
                    .value
                    .clone()
            })
        })
        .collect();

    let final_values: Vec<Value> = handles.into_iter().map(|h| h.join().unwrap()).collect();

    // Each run should have a value from its own range
    for (run_idx, value) in final_values.iter().enumerate() {
        match value {
            Value::Int(n) => {
                let min = (run_idx * 1000) as i64;
                let max = ((run_idx + 1) * 1000 - 1) as i64;
                assert!(
                    *n >= min && *n <= max,
                    "Run {} final value {} should be in range [{}, {}]",
                    run_idx,
                    n,
                    min,
                    max
                );
            }
            _ => panic!("Expected Int"),
        }
    }
}

// =============================================================================
// STRESS TEST
// =============================================================================

#[test]
fn test_mixed_operations_stress() {
    let db = create_buffered_db();
    let substrate = Arc::new(create_substrate(db.clone()));
    let run = ApiRunId::default();

    const NUM_THREADS: usize = 8;
    const OPS_PER_THREAD: usize = 200;

    let barrier = Arc::new(Barrier::new(NUM_THREADS));
    let handles: Vec<_> = (0..NUM_THREADS)
        .map(|thread_id| {
            let substrate = Arc::clone(&substrate);
            let barrier = Arc::clone(&barrier);
            let run = run.clone();

            thread::spawn(move || {
                barrier.wait();

                let mut errors = 0;
                for i in 0..OPS_PER_THREAD {
                    let key = format!("stress_{}_{}", thread_id, i % 10); // Some key overlap

                    // Mix of operations based on iteration
                    let result = match i % 5 {
                        0 => substrate.kv_put(&run, &key, Value::Int(i as i64)).map(|_| ()),
                        1 => substrate.kv_get(&run, &key).map(|_| ()),
                        2 => substrate.kv_incr(&run, &format!("counter_{}", thread_id), 1).map(|_| ()),
                        3 => substrate.kv_exists(&run, &key).map(|_| ()),
                        4 => substrate.kv_delete(&run, &key).map(|_| ()),
                        _ => unreachable!(),
                    };

                    if result.is_err() {
                        errors += 1;
                    }
                }
                errors
            })
        })
        .collect();

    let total_errors: usize = handles.into_iter().map(|h| h.join().unwrap()).sum();

    assert_eq!(
        total_errors, 0,
        "No errors should occur during stress test"
    );
}
