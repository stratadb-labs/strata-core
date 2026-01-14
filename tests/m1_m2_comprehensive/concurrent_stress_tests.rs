//! Concurrent Stress Tests
//!
//! High-volume and high-concurrency tests for validating OCC
//! behavior under load.

use super::test_utils::*;
use in_mem_core::error::Error;
use in_mem_core::types::RunId;
use in_mem_core::value::Value;
use in_mem_engine::{Database, RetryConfig};
use std::collections::HashSet;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Barrier};
use std::thread;
use std::time::{Duration, Instant};
use tempfile::TempDir;

// ============================================================================
// Configuration
// ============================================================================

const LIGHT_THREADS: usize = 4;
const MEDIUM_THREADS: usize = 8;
const HEAVY_THREADS: usize = 16;
const TXN_PER_THREAD_LIGHT: usize = 50;
const TXN_PER_THREAD_MEDIUM: usize = 100;
const TXN_PER_THREAD_HEAVY: usize = 200;

// ============================================================================
// High Concurrency Tests
// ============================================================================

mod high_concurrency {
    use super::*;

    #[test]
    fn test_many_threads_disjoint_keys() {
        let temp_dir = TempDir::new().unwrap();
        let db = Arc::new(Database::open(temp_dir.path().join("db")).unwrap());

        let (run_id, ns) = create_namespace();

        let success_count = Arc::new(AtomicU64::new(0));

        let handles: Vec<_> = (0..MEDIUM_THREADS)
            .map(|thread_id| {
                let db = Arc::clone(&db);
                let ns = ns.clone();
                let success_count = Arc::clone(&success_count);

                thread::spawn(move || {
                    for i in 0..TXN_PER_THREAD_MEDIUM {
                        // Each thread writes to its own keys (disjoint)
                        let key = kv_key(&ns, &format!("t{}_k{}", thread_id, i));

                        let result = db.transaction(run_id, |txn| {
                            txn.put(key.clone(), values::int((thread_id * 1000 + i) as i64))?;
                            Ok(())
                        });

                        if result.is_ok() {
                            success_count.fetch_add(1, Ordering::Relaxed);
                        }
                    }
                })
            })
            .collect();

        for h in handles {
            h.join().unwrap();
        }

        // All transactions should succeed (no conflicts on disjoint keys)
        let expected = (MEDIUM_THREADS * TXN_PER_THREAD_MEDIUM) as u64;
        assert_eq!(success_count.load(Ordering::Relaxed), expected);
    }

    #[test]
    fn test_many_threads_same_key_with_retry() {
        let temp_dir = TempDir::new().unwrap();
        let db = Arc::new(Database::open(temp_dir.path().join("db")).unwrap());

        let (run_id, ns) = create_namespace();
        let key = kv_key(&ns, "hot_key");

        // Initialize counter
        db.put(run_id, key.clone(), values::int(0)).unwrap();

        let success_count = Arc::new(AtomicU64::new(0));

        let handles: Vec<_> = (0..LIGHT_THREADS)
            .map(|_| {
                let db = Arc::clone(&db);
                let key = key.clone();
                let success_count = Arc::clone(&success_count);

                thread::spawn(move || {
                    for _ in 0..TXN_PER_THREAD_LIGHT {
                        let result = db.transaction_with_retry(
                            run_id,
                            RetryConfig::new().with_max_retries(50),
                            |txn| {
                                let current = match txn.get(&key)? {
                                    Some(Value::I64(n)) => n,
                                    _ => 0,
                                };
                                txn.put(key.clone(), values::int(current + 1))?;
                                Ok(())
                            },
                        );

                        if result.is_ok() {
                            success_count.fetch_add(1, Ordering::Relaxed);
                        }
                    }
                })
            })
            .collect();

        for h in handles {
            h.join().unwrap();
        }

        // All increments should eventually succeed
        let expected = (LIGHT_THREADS * TXN_PER_THREAD_LIGHT) as u64;
        assert_eq!(success_count.load(Ordering::Relaxed), expected);

        // Final value should equal total increments
        let final_val = db.get(&key).unwrap().unwrap().value;
        assert_eq!(final_val, values::int(expected as i64));
    }

    #[test]
    fn test_reader_writer_concurrency() {
        let temp_dir = TempDir::new().unwrap();
        let db = Arc::new(Database::open(temp_dir.path().join("db")).unwrap());

        let (run_id, ns) = create_namespace();

        // Initialize multiple keys
        for i in 0..10 {
            let key = kv_key(&ns, &format!("rw_{}", i));
            db.put(run_id, key, values::int(0)).unwrap();
        }

        let reads = Arc::new(AtomicU64::new(0));
        let writes = Arc::new(AtomicU64::new(0));

        let barrier = Arc::new(Barrier::new(MEDIUM_THREADS));

        let handles: Vec<_> = (0..MEDIUM_THREADS)
            .map(|thread_id| {
                let db = Arc::clone(&db);
                let ns = ns.clone();
                let reads = Arc::clone(&reads);
                let writes = Arc::clone(&writes);
                let barrier = Arc::clone(&barrier);

                thread::spawn(move || {
                    barrier.wait();

                    for i in 0..TXN_PER_THREAD_LIGHT {
                        let key_idx = (thread_id + i) % 10;
                        let key = kv_key(&ns, &format!("rw_{}", key_idx));

                        if thread_id % 2 == 0 {
                            // Reader
                            let _ = db.transaction(run_id, |txn| {
                                let _ = txn.get(&key)?;
                                Ok(())
                            });
                            reads.fetch_add(1, Ordering::Relaxed);
                        } else {
                            // Writer
                            let _ = db.transaction_with_retry(
                                run_id,
                                RetryConfig::new().with_max_retries(20),
                                |txn| {
                                    let current = match txn.get(&key)? {
                                        Some(Value::I64(n)) => n,
                                        _ => 0,
                                    };
                                    txn.put(key.clone(), values::int(current + 1))?;
                                    Ok(())
                                },
                            );
                            writes.fetch_add(1, Ordering::Relaxed);
                        }
                    }
                })
            })
            .collect();

        for h in handles {
            h.join().unwrap();
        }

        // Both reads and writes should have happened
        assert!(reads.load(Ordering::Relaxed) > 0);
        assert!(writes.load(Ordering::Relaxed) > 0);
    }

    #[test]
    fn test_thundering_herd() {
        let temp_dir = TempDir::new().unwrap();
        let db = Arc::new(Database::open(temp_dir.path().join("db")).unwrap());

        let (run_id, ns) = create_namespace();
        let key = kv_key(&ns, "herd_key");

        db.put(run_id, key.clone(), values::int(0)).unwrap();

        let barrier = Arc::new(Barrier::new(HEAVY_THREADS));
        let success = Arc::new(AtomicU64::new(0));
        let conflicts = Arc::new(AtomicU64::new(0));

        // All threads try to update the same key at exactly the same time
        let handles: Vec<_> = (0..HEAVY_THREADS)
            .map(|thread_id| {
                let db = Arc::clone(&db);
                let key = key.clone();
                let barrier = Arc::clone(&barrier);
                let success = Arc::clone(&success);
                let conflicts = Arc::clone(&conflicts);

                thread::spawn(move || {
                    barrier.wait();

                    let result: Result<(), Error> = db.transaction(run_id, |txn| {
                        let _ = txn.get(&key)?;
                        // Small delay to increase conflict window
                        thread::sleep(Duration::from_micros(100));
                        txn.put(key.clone(), values::int(thread_id as i64))?;
                        Ok(())
                    });

                    match result {
                        Ok(_) => success.fetch_add(1, Ordering::Relaxed),
                        Err(e) if e.is_conflict() => conflicts.fetch_add(1, Ordering::Relaxed),
                        _ => 0,
                    };
                })
            })
            .collect();

        for h in handles {
            h.join().unwrap();
        }

        // At least one should succeed
        assert!(success.load(Ordering::Relaxed) >= 1);

        // Most should conflict (thundering herd effect)
        // Note: Actual number depends on timing
    }
}

// ============================================================================
// Throughput Tests
// ============================================================================

mod throughput {
    use super::*;

    #[test]
    fn test_single_thread_throughput() {
        let temp_dir = TempDir::new().unwrap();
        let db = Database::open(temp_dir.path().join("db")).unwrap();

        let (run_id, ns) = create_namespace();

        let num_transactions = 1000;
        let start = Instant::now();

        for i in 0..num_transactions {
            let key = kv_key(&ns, &format!("throughput_{}", i));
            db.transaction(run_id, |txn| {
                txn.put(key.clone(), values::int(i))?;
                Ok(())
            })
            .unwrap();
        }

        let elapsed = start.elapsed();
        let tps = num_transactions as f64 / elapsed.as_secs_f64();

        println!(
            "Single-thread throughput: {} transactions in {:?} ({:.0} TPS)",
            num_transactions, elapsed, tps
        );

        // Should be reasonably fast (at least 100 TPS)
        assert!(tps > 100.0);
    }

    #[test]
    fn test_multi_thread_throughput_disjoint() {
        let temp_dir = TempDir::new().unwrap();
        let db = Arc::new(Database::open(temp_dir.path().join("db")).unwrap());

        let (run_id, ns) = create_namespace();
        let total_transactions = Arc::new(AtomicU64::new(0));

        let start = Instant::now();

        let handles: Vec<_> = (0..LIGHT_THREADS)
            .map(|thread_id| {
                let db = Arc::clone(&db);
                let ns = ns.clone();
                let total = Arc::clone(&total_transactions);

                thread::spawn(move || {
                    for i in 0..TXN_PER_THREAD_HEAVY {
                        let key = kv_key(&ns, &format!("tp_t{}_k{}", thread_id, i));
                        db.transaction(run_id, |txn| {
                            txn.put(key.clone(), values::int(i as i64))?;
                            Ok(())
                        })
                        .unwrap();
                        total.fetch_add(1, Ordering::Relaxed);
                    }
                })
            })
            .collect();

        for h in handles {
            h.join().unwrap();
        }

        let elapsed = start.elapsed();
        let total = total_transactions.load(Ordering::Relaxed);
        let tps = total as f64 / elapsed.as_secs_f64();

        println!(
            "Multi-thread throughput (disjoint): {} transactions in {:?} ({:.0} TPS)",
            total, elapsed, tps
        );
    }

    #[test]
    fn test_read_throughput() {
        let temp_dir = TempDir::new().unwrap();
        let db = Arc::new(Database::open(temp_dir.path().join("db")).unwrap());

        let (run_id, ns) = create_namespace();

        // Pre-populate data
        for i in 0..1000 {
            let key = kv_key(&ns, &format!("read_tp_{}", i));
            db.put(run_id, key, values::int(i)).unwrap();
        }

        let reads = Arc::new(AtomicU64::new(0));
        let start = Instant::now();

        let handles: Vec<_> = (0..LIGHT_THREADS)
            .map(|_| {
                let db = Arc::clone(&db);
                let ns = ns.clone();
                let reads = Arc::clone(&reads);

                thread::spawn(move || {
                    for _ in 0..TXN_PER_THREAD_HEAVY {
                        let key_idx = rand_idx(1000);
                        let key = kv_key(&ns, &format!("read_tp_{}", key_idx));
                        let _ = db.get(&key);
                        reads.fetch_add(1, Ordering::Relaxed);
                    }
                })
            })
            .collect();

        for h in handles {
            h.join().unwrap();
        }

        let elapsed = start.elapsed();
        let total = reads.load(Ordering::Relaxed);
        let rps = total as f64 / elapsed.as_secs_f64();

        println!(
            "Read throughput: {} reads in {:?} ({:.0} RPS)",
            total, elapsed, rps
        );
    }

    fn rand_idx(max: usize) -> usize {
        use std::collections::hash_map::RandomState;
        use std::hash::{BuildHasher, Hasher};

        let s = RandomState::new();
        let mut h = s.build_hasher();
        h.write_usize(std::time::Instant::now().elapsed().as_nanos() as usize);
        h.finish() as usize % max
    }
}

// ============================================================================
// Stress Tests
// ============================================================================

mod stress {
    use super::*;

    #[test]
    fn test_sustained_load() {
        let temp_dir = TempDir::new().unwrap();
        let db = Arc::new(Database::open(temp_dir.path().join("db")).unwrap());

        let (run_id, ns) = create_namespace();

        let running = Arc::new(AtomicU64::new(1));
        let transactions = Arc::new(AtomicU64::new(0));
        let errors = Arc::new(AtomicU64::new(0));

        let handles: Vec<_> = (0..LIGHT_THREADS)
            .map(|thread_id| {
                let db = Arc::clone(&db);
                let ns = ns.clone();
                let running = Arc::clone(&running);
                let transactions = Arc::clone(&transactions);
                let errors = Arc::clone(&errors);

                thread::spawn(move || {
                    let mut i = 0;
                    while running.load(Ordering::Relaxed) == 1 {
                        let key = kv_key(&ns, &format!("sustained_t{}_k{}", thread_id, i % 100));

                        let result: Result<(), Error> = db.transaction(run_id, |txn| {
                            let _ = txn.get(&key);
                            txn.put(key.clone(), values::int(i))?;
                            Ok(())
                        });

                        match result {
                            Ok(_) => transactions.fetch_add(1, Ordering::Relaxed),
                            Err(_) => errors.fetch_add(1, Ordering::Relaxed),
                        };

                        i += 1;
                    }
                })
            })
            .collect();

        // Run for 2 seconds
        thread::sleep(Duration::from_secs(2));
        running.store(0, Ordering::Relaxed);

        for h in handles {
            h.join().unwrap();
        }

        let total_txns = transactions.load(Ordering::Relaxed);
        let total_errors = errors.load(Ordering::Relaxed);

        println!(
            "Sustained load test: {} transactions, {} errors",
            total_txns, total_errors
        );

        // Should have processed many transactions
        assert!(total_txns > 100);
    }

    #[test]
    fn test_burst_load() {
        let temp_dir = TempDir::new().unwrap();
        let db = Arc::new(Database::open(temp_dir.path().join("db")).unwrap());

        let (run_id, ns) = create_namespace();

        // Multiple bursts
        for burst in 0..3 {
            let barrier = Arc::new(Barrier::new(HEAVY_THREADS));
            let success = Arc::new(AtomicU64::new(0));

            let handles: Vec<_> = (0..HEAVY_THREADS)
                .map(|thread_id| {
                    let db = Arc::clone(&db);
                    let ns = ns.clone();
                    let barrier = Arc::clone(&barrier);
                    let success = Arc::clone(&success);

                    thread::spawn(move || {
                        barrier.wait();

                        // Burst of transactions
                        for i in 0..10 {
                            let key =
                                kv_key(&ns, &format!("burst_{}_t{}_k{}", burst, thread_id, i));

                            if db
                                .transaction(run_id, |txn| {
                                    txn.put(key.clone(), values::int(i as i64))?;
                                    Ok(())
                                })
                                .is_ok()
                            {
                                success.fetch_add(1, Ordering::Relaxed);
                            }
                        }
                    })
                })
                .collect();

            for h in handles {
                h.join().unwrap();
            }

            println!(
                "Burst {}: {} successful transactions",
                burst,
                success.load(Ordering::Relaxed)
            );

            // Brief pause between bursts
            thread::sleep(Duration::from_millis(100));
        }
    }

    #[test]
    fn test_large_transactions() {
        let temp_dir = TempDir::new().unwrap();
        let db = Database::open(temp_dir.path().join("db")).unwrap();

        let (run_id, ns) = create_namespace();

        // Transaction with many keys
        let num_keys = 500;

        let start = Instant::now();
        db.transaction(run_id, |txn| {
            for i in 0..num_keys {
                let key = kv_key(&ns, &format!("large_txn_{}", i));
                txn.put(key, values::int(i))?;
            }
            Ok(())
        })
        .unwrap();
        let elapsed = start.elapsed();

        println!(
            "Large transaction ({} keys) committed in {:?}",
            num_keys, elapsed
        );

        // Verify all committed
        for i in 0..num_keys {
            let key = kv_key(&ns, &format!("large_txn_{}", i));
            assert!(db.get(&key).unwrap().is_some());
        }
    }

    #[test]
    fn test_large_values() {
        let temp_dir = TempDir::new().unwrap();
        let db = Database::open(temp_dir.path().join("db")).unwrap();

        let (run_id, ns) = create_namespace();

        // 100KB values
        let large_value = values::large_bytes(100);

        let start = Instant::now();
        for i in 0..50 {
            let key = kv_key(&ns, &format!("large_val_{}", i));
            db.transaction(run_id, |txn| {
                txn.put(key.clone(), large_value.clone())?;
                Ok(())
            })
            .unwrap();
        }
        let elapsed = start.elapsed();

        println!("50 transactions with 100KB values in {:?}", elapsed);
    }
}

// ============================================================================
// Contention Pattern Tests
// ============================================================================

mod contention_patterns {
    use super::*;

    #[test]
    fn test_hot_spot_single_key() {
        let temp_dir = TempDir::new().unwrap();
        let db = Arc::new(Database::open(temp_dir.path().join("db")).unwrap());

        let (run_id, ns) = create_namespace();
        let hot_key = kv_key(&ns, "hot_spot");

        db.put(run_id, hot_key.clone(), values::int(0)).unwrap();

        let success = Arc::new(AtomicU64::new(0));
        let conflicts = Arc::new(AtomicU64::new(0));

        let handles: Vec<_> = (0..MEDIUM_THREADS)
            .map(|_| {
                let db = Arc::clone(&db);
                let hot_key = hot_key.clone();
                let success = Arc::clone(&success);
                let conflicts = Arc::clone(&conflicts);

                thread::spawn(move || {
                    for _ in 0..TXN_PER_THREAD_LIGHT {
                        let result: Result<(), Error> = db.transaction(run_id, |txn| {
                            let v = txn.get(&hot_key)?.unwrap_or(Value::I64(0));
                            if let Value::I64(n) = v {
                                txn.put(hot_key.clone(), values::int(n + 1))?;
                            }
                            Ok(())
                        });

                        match result {
                            Ok(_) => { success.fetch_add(1, Ordering::Relaxed); },
                            Err(e) if e.is_conflict() => { conflicts.fetch_add(1, Ordering::Relaxed); },
                            _ => {}
                        }
                    }
                })
            })
            .collect();

        for h in handles {
            h.join().unwrap();
        }

        let s = success.load(Ordering::Relaxed);
        let c = conflicts.load(Ordering::Relaxed);
        println!("Hot spot test: {} successes, {} conflicts", s, c);

        // Final value should equal success count
        let final_val = match db.get(&hot_key).unwrap().unwrap().value {
            Value::I64(n) => n,
            _ => panic!("Expected I64"),
        };
        assert_eq!(final_val, s as i64);
    }

    #[test]
    fn test_hot_spot_few_keys() {
        let temp_dir = TempDir::new().unwrap();
        let db = Arc::new(Database::open(temp_dir.path().join("db")).unwrap());

        let (run_id, ns) = create_namespace();

        // 5 hot keys
        let hot_keys: Vec<_> = (0..5)
            .map(|i| {
                let key = kv_key(&ns, &format!("hot_{}", i));
                db.put(run_id, key.clone(), values::int(0)).unwrap();
                key
            })
            .collect();

        let success = Arc::new(AtomicU64::new(0));

        let handles: Vec<_> = (0..MEDIUM_THREADS)
            .map(|thread_id| {
                let db = Arc::clone(&db);
                let hot_keys = hot_keys.clone();
                let success = Arc::clone(&success);

                thread::spawn(move || {
                    for i in 0..TXN_PER_THREAD_LIGHT {
                        let key_idx = (thread_id + i) % 5;
                        let key = &hot_keys[key_idx];

                        let result = db.transaction_with_retry(
                            run_id,
                            RetryConfig::new().with_max_retries(30),
                            |txn| {
                                let v = txn.get(key)?.unwrap_or(Value::I64(0));
                                if let Value::I64(n) = v {
                                    txn.put(key.clone(), values::int(n + 1))?;
                                }
                                Ok(())
                            },
                        );

                        if result.is_ok() {
                            success.fetch_add(1, Ordering::Relaxed);
                        }
                    }
                })
            })
            .collect();

        for h in handles {
            h.join().unwrap();
        }

        // All should succeed with retry
        let expected = (MEDIUM_THREADS * TXN_PER_THREAD_LIGHT) as u64;
        assert_eq!(success.load(Ordering::Relaxed), expected);

        // Sum of all hot keys should equal success count
        let sum: i64 = hot_keys
            .iter()
            .map(|key| match db.get(key).unwrap().unwrap().value {
                Value::I64(n) => n,
                _ => 0,
            })
            .sum();

        assert_eq!(sum, expected as i64);
    }

    #[test]
    fn test_zipf_like_distribution() {
        let temp_dir = TempDir::new().unwrap();
        let db = Arc::new(Database::open(temp_dir.path().join("db")).unwrap());

        let (run_id, ns) = create_namespace();

        // 100 keys, but access follows zipf-like pattern
        let num_keys = 100;
        for i in 0..num_keys {
            let key = kv_key(&ns, &format!("zipf_{}", i));
            db.put(run_id, key, values::int(0)).unwrap();
        }

        let handles: Vec<_> = (0..LIGHT_THREADS)
            .map(|_| {
                let db = Arc::clone(&db);
                let ns = ns.clone();

                thread::spawn(move || {
                    for i in 0..TXN_PER_THREAD_MEDIUM {
                        // Zipf-like: lower indices accessed more frequently
                        let key_idx = zipf_sample(num_keys, i);
                        let key = kv_key(&ns, &format!("zipf_{}", key_idx));

                        let _ = db.transaction_with_retry(
                            run_id,
                            RetryConfig::new().with_max_retries(20),
                            |txn| {
                                let v = txn.get(&key)?.unwrap_or(Value::I64(0));
                                if let Value::I64(n) = v {
                                    txn.put(key.clone(), values::int(n + 1))?;
                                }
                                Ok(())
                            },
                        );
                    }
                })
            })
            .collect();

        for h in handles {
            h.join().unwrap();
        }

        // Key 0 should have more updates than key 99
        let key_0 = kv_key(&ns, "zipf_0");
        let key_99 = kv_key(&ns, "zipf_99");

        let val_0 = match db.get(&key_0).unwrap().unwrap().value {
            Value::I64(n) => n,
            _ => 0,
        };
        let val_99 = match db.get(&key_99).unwrap().unwrap().value {
            Value::I64(n) => n,
            _ => 0,
        };

        println!("Zipf test: key_0={}, key_99={}", val_0, val_99);
        assert!(val_0 > val_99);
    }

    fn zipf_sample(max: usize, seed: usize) -> usize {
        // Simple approximation of Zipf distribution
        let x = (seed % 100) as f64 / 100.0;
        let idx = (x * x * max as f64) as usize;
        idx.min(max - 1)
    }
}

// ============================================================================
// Data Integrity Under Load Tests
// ============================================================================

mod data_integrity {
    use super::*;

    #[test]
    fn test_no_lost_updates_under_contention() {
        let temp_dir = TempDir::new().unwrap();
        let db = Arc::new(Database::open(temp_dir.path().join("db")).unwrap());

        let (run_id, ns) = create_namespace();
        let key = kv_key(&ns, "counter");

        db.put(run_id, key.clone(), values::int(0)).unwrap();

        let increments_attempted = Arc::new(AtomicU64::new(0));
        let increments_succeeded = Arc::new(AtomicU64::new(0));

        let handles: Vec<_> = (0..LIGHT_THREADS)
            .map(|_| {
                let db = Arc::clone(&db);
                let key = key.clone();
                let attempted = Arc::clone(&increments_attempted);
                let succeeded = Arc::clone(&increments_succeeded);

                thread::spawn(move || {
                    for _ in 0..TXN_PER_THREAD_LIGHT {
                        attempted.fetch_add(1, Ordering::Relaxed);

                        let result = db.transaction_with_retry(
                            run_id,
                            RetryConfig::new().with_max_retries(100),
                            |txn| {
                                let current = match txn.get(&key)? {
                                    Some(Value::I64(n)) => n,
                                    _ => 0,
                                };
                                txn.put(key.clone(), values::int(current + 1))?;
                                Ok(())
                            },
                        );

                        if result.is_ok() {
                            succeeded.fetch_add(1, Ordering::Relaxed);
                        }
                    }
                })
            })
            .collect();

        for h in handles {
            h.join().unwrap();
        }

        let total_succeeded = increments_succeeded.load(Ordering::Relaxed);
        let final_value = match db.get(&key).unwrap().unwrap().value {
            Value::I64(n) => n as u64,
            _ => panic!("Expected I64"),
        };

        // Critical check: final value must equal successful increments
        assert_eq!(
            final_value, total_succeeded,
            "Lost updates detected! Final value {} != succeeded {}",
            final_value, total_succeeded
        );
    }

    #[test]
    fn test_atomic_multi_key_updates() {
        let temp_dir = TempDir::new().unwrap();
        let db = Arc::new(Database::open(temp_dir.path().join("db")).unwrap());

        let (run_id, ns) = create_namespace();

        // 3 keys that should always sum to 300
        let keys: Vec<_> = (0..3)
            .map(|i| {
                let key = kv_key(&ns, &format!("sum_{}", i));
                db.put(run_id, key.clone(), values::int(100)).unwrap();
                key
            })
            .collect();

        let handles: Vec<_> = (0..LIGHT_THREADS)
            .map(|thread_id| {
                let db = Arc::clone(&db);
                let keys = keys.clone();

                thread::spawn(move || {
                    for i in 0..TXN_PER_THREAD_LIGHT {
                        let from_idx = (thread_id + i) % 3;
                        let to_idx = (from_idx + 1) % 3;

                        let _ = db.transaction_with_retry(
                            run_id,
                            RetryConfig::new().with_max_retries(50),
                            |txn| {
                                let from_val = match txn.get(&keys[from_idx])? {
                                    Some(Value::I64(n)) => n,
                                    _ => 0,
                                };
                                let to_val = match txn.get(&keys[to_idx])? {
                                    Some(Value::I64(n)) => n,
                                    _ => 0,
                                };

                                // Transfer 10 from one key to another
                                if from_val >= 10 {
                                    txn.put(keys[from_idx].clone(), values::int(from_val - 10))?;
                                    txn.put(keys[to_idx].clone(), values::int(to_val + 10))?;
                                }
                                Ok(())
                            },
                        );
                    }
                })
            })
            .collect();

        for h in handles {
            h.join().unwrap();
        }

        // Critical check: sum should still be 300
        let sum: i64 = keys
            .iter()
            .map(|key| match db.get(key).unwrap().unwrap().value {
                Value::I64(n) => n,
                _ => 0,
            })
            .sum();

        assert_eq!(sum, 300, "Atomicity violated! Sum is {} instead of 300", sum);
    }

    #[test]
    fn test_no_duplicate_versions() {
        let temp_dir = TempDir::new().unwrap();
        let db = Arc::new(Database::open(temp_dir.path().join("db")).unwrap());

        let (run_id, ns) = create_namespace();

        let versions = Arc::new(std::sync::Mutex::new(Vec::new()));

        let handles: Vec<_> = (0..MEDIUM_THREADS)
            .map(|thread_id| {
                let db = Arc::clone(&db);
                let ns = ns.clone();
                let versions = Arc::clone(&versions);

                thread::spawn(move || {
                    for i in 0..TXN_PER_THREAD_LIGHT {
                        let key = kv_key(&ns, &format!("ver_t{}_k{}", thread_id, i));

                        db.transaction(run_id, |txn| {
                            txn.put(key.clone(), values::int(i as i64))?;
                            Ok(())
                        })
                        .unwrap();

                        let version = db.get(&key).unwrap().unwrap().version;
                        versions.lock().unwrap().push(version);
                    }
                })
            })
            .collect();

        for h in handles {
            h.join().unwrap();
        }

        let versions = versions.lock().unwrap();
        let unique: HashSet<_> = versions.iter().collect();

        // All versions should be unique
        assert_eq!(
            unique.len(),
            versions.len(),
            "Duplicate versions detected!"
        );
    }
}

// ============================================================================
// Resource Usage Tests
// ============================================================================

mod resource_usage {
    use super::*;

    #[test]
    fn test_many_aborted_transactions_no_leak() {
        let temp_dir = TempDir::new().unwrap();
        let db = Database::open(temp_dir.path().join("db")).unwrap();

        let (run_id, ns) = create_namespace();

        // Abort many transactions
        for i in 0..1000 {
            let key = kv_key(&ns, &format!("abort_{}", i));
            let _: Result<(), Error> = db.transaction(run_id, |txn| {
                txn.put(key, values::int(i))?;
                Err(Error::InvalidState("intentional".to_string()))
            });
        }

        // Should still be able to work normally
        let test_key = kv_key(&ns, "after_aborts");
        db.transaction(run_id, |txn| {
            txn.put(test_key.clone(), values::int(42))?;
            Ok(())
        })
        .unwrap();

        assert!(db.get(&test_key).unwrap().is_some());
    }

    #[test]
    fn test_many_retries_no_leak() {
        let temp_dir = TempDir::new().unwrap();
        let db = Database::open(temp_dir.path().join("db")).unwrap();

        let (run_id, ns) = create_namespace();
        let key = kv_key(&ns, "retry_leak");

        db.put(run_id, key.clone(), values::int(0)).unwrap();

        let attempts = AtomicU64::new(0);

        // Transaction that fails many times before succeeding
        db.transaction_with_retry(
            run_id,
            RetryConfig::new().with_max_retries(100),
            |txn| {
                let count = attempts.fetch_add(1, Ordering::Relaxed);
                if count < 50 {
                    return Err(Error::TransactionConflict("simulated".to_string()));
                }

                let v = txn.get(&key)?.unwrap_or(Value::I64(0));
                if let Value::I64(n) = v {
                    txn.put(key.clone(), values::int(n + 1))?;
                }
                Ok(())
            },
        )
        .unwrap();

        // Should work normally after many retries
        let test_key = kv_key(&ns, "after_retries");
        db.transaction(run_id, |txn| {
            txn.put(test_key.clone(), values::int(99))?;
            Ok(())
        })
        .unwrap();

        assert!(db.get(&test_key).unwrap().is_some());
    }

    #[test]
    fn test_concurrent_open_transactions() {
        let temp_dir = TempDir::new().unwrap();
        let db = Arc::new(Database::open(temp_dir.path().join("db")).unwrap());

        let (run_id, ns) = create_namespace();

        // Open many transactions simultaneously (without committing immediately)
        let barrier = Arc::new(Barrier::new(HEAVY_THREADS));

        let handles: Vec<_> = (0..HEAVY_THREADS)
            .map(|thread_id| {
                let db = Arc::clone(&db);
                let ns = ns.clone();
                let barrier = Arc::clone(&barrier);

                thread::spawn(move || {
                    let key = kv_key(&ns, &format!("concurrent_{}", thread_id));

                    barrier.wait();

                    // All threads start transactions at the same time
                    db.transaction(run_id, |txn| {
                        txn.put(key.clone(), values::int(thread_id as i64))?;
                        // Hold the transaction open briefly
                        thread::sleep(Duration::from_millis(10));
                        Ok(())
                    })
                    .unwrap();
                })
            })
            .collect();

        for h in handles {
            h.join().unwrap();
        }

        // All should have committed
        for i in 0..HEAVY_THREADS {
            let key = kv_key(&ns, &format!("concurrent_{}", i));
            assert!(db.get(&key).unwrap().is_some());
        }
    }
}
