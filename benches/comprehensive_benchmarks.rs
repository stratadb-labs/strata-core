//! Comprehensive Benchmark Suite for in-mem
//!
//! This benchmark suite answers three critical questions:
//! 1. Is it fast enough for its intended workloads?
//! 2. Is it meaningfully better than alternatives for those workloads?
//! 3. Does its semantics justify any performance tradeoffs?
//!
//! ## Benchmark Categories
//!
//! - **Tier 1: Microbenchmarks** - Per-primitive, per-operation, pure compute+memory
//! - **Tier 2: Concurrency** - Transaction throughput under contention
//! - **Tier 3: Recovery** - WAL replay and snapshot performance
//! - **Tier 4: Durability** - fsync overhead and batching tradeoffs
//! - **Tier 5: Memory** - Heap usage and overhead
//! - **Tier 6: Scenarios** - Agent-like workloads
//!
//! ## Target Performance (MVP)
//!
//! | Operation           | Goal           | Acceptable     |
//! |---------------------|----------------|----------------|
//! | KV get              | 50-200+ Kops/s | 20+ Kops/s     |
//! | KV put              | 10-50+ Kops/s  | 5+ Kops/s      |
//! | KV CAS              | 5-20+ Kops/s   | 2+ Kops/s      |
//! | Event append        | 50-200+ Kops/s | 20+ Kops/s     |
//! | Event scan          | 10-50+ Kops/s  | 5+ Kops/s      |
//! | Snapshot creation   | <100ms/1M keys | <500ms/1M keys |
//! | WAL replay          | <300ms/1M ops  | <1s/1M ops     |
//!
//! ## Running Benchmarks
//!
//! ```bash
//! # Run all benchmarks
//! cargo bench --bench comprehensive_benchmarks
//!
//! # Run specific category
//! cargo bench --bench comprehensive_benchmarks -- kv_microbenchmarks
//! cargo bench --bench comprehensive_benchmarks -- concurrency
//! cargo bench --bench comprehensive_benchmarks -- recovery
//! cargo bench --bench comprehensive_benchmarks -- durability
//! cargo bench --bench comprehensive_benchmarks -- memory
//! cargo bench --bench comprehensive_benchmarks -- scenario
//!
//! # Run with specific data sizes
//! cargo bench --bench comprehensive_benchmarks -- "1000 keys"
//! ```

use criterion::{
    black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput,
};
use in_mem_core::traits::Storage;
use in_mem_core::types::{Key, Namespace, RunId, TypeTag};
use in_mem_core::value::Value;
use in_mem_durability::wal::DurabilityMode;
use in_mem_engine::Database;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Barrier};
use std::thread;
use std::time::{Duration, Instant};
use tempfile::TempDir;

// =============================================================================
// Test Utilities
// =============================================================================

fn create_namespace(run_id: RunId) -> Namespace {
    Namespace::new(
        "tenant".to_string(),
        "app".to_string(),
        "agent".to_string(),
        run_id,
    )
}

fn make_key(ns: &Namespace, name: &str) -> Key {
    Key::new_kv(ns.clone(), name)
}

fn make_event_key(ns: &Namespace, seq: u64) -> Key {
    Key::new_event(ns.clone(), seq)
}

// =============================================================================
// Tier 1: KV Microbenchmarks
// =============================================================================

fn kv_microbenchmarks(c: &mut Criterion) {
    let mut group = c.benchmark_group("kv_microbenchmarks");

    // ---------------------------------------------------------------------------
    // KV Get Throughput (single-threaded)
    // Target: 50-200+ Kops/sec
    // ---------------------------------------------------------------------------
    {
        let temp_dir = TempDir::new().unwrap();
        let db = Database::open(temp_dir.path().join("db")).unwrap();
        let run_id = RunId::new();
        let ns = create_namespace(run_id);

        // Pre-populate with 10K keys
        for i in 0..10_000 {
            let key = make_key(&ns, &format!("key_{:05}", i));
            db.put(run_id, key, Value::I64(i)).unwrap();
        }

        group.throughput(Throughput::Elements(1));
        group.bench_function("get_existing_key", |b| {
            let key = make_key(&ns, "key_05000");
            b.iter(|| {
                let result = db.get(&key);
                black_box(result.unwrap());
            });
        });

        group.bench_function("get_nonexistent_key", |b| {
            let key = make_key(&ns, "nonexistent_key");
            b.iter(|| {
                let result = db.get(&key);
                black_box(result.unwrap());
            });
        });
    }

    // ---------------------------------------------------------------------------
    // KV Put Throughput (single-threaded)
    // Target: 10-50+ Kops/sec
    // ---------------------------------------------------------------------------
    for data_size in [10_000, 100_000] {
        let temp_dir = TempDir::new().unwrap();
        let db = Database::open(temp_dir.path().join("db")).unwrap();
        let run_id = RunId::new();
        let ns = create_namespace(run_id);

        group.throughput(Throughput::Elements(1));
        group.bench_with_input(
            BenchmarkId::new("put_unique_keys", data_size),
            &data_size,
            |b, _| {
                let counter = AtomicU64::new(0);
                b.iter(|| {
                    let i = counter.fetch_add(1, Ordering::Relaxed);
                    let key = make_key(&ns, &format!("unique_{}", i));
                    let result = db.put(run_id, key, Value::I64(i as i64));
                    black_box(result.unwrap());
                });
            },
        );
    }

    // ---------------------------------------------------------------------------
    // KV Put Overwrite (single key, measures version overhead)
    // ---------------------------------------------------------------------------
    {
        let temp_dir = TempDir::new().unwrap();
        let db = Database::open(temp_dir.path().join("db")).unwrap();
        let run_id = RunId::new();
        let ns = create_namespace(run_id);
        let key = make_key(&ns, "overwrite_key");

        // Initial put
        db.put(run_id, key.clone(), Value::I64(0)).unwrap();

        group.throughput(Throughput::Elements(1));
        group.bench_function("put_overwrite_same_key", |b| {
            let mut counter = 0i64;
            b.iter(|| {
                counter += 1;
                let result = db.put(run_id, key.clone(), Value::I64(counter));
                black_box(result.unwrap());
            });
        });
    }

    // ---------------------------------------------------------------------------
    // KV CAS Throughput
    // Target: 5-20+ Kops/sec
    // ---------------------------------------------------------------------------
    {
        let temp_dir = TempDir::new().unwrap();
        let db = Database::open(temp_dir.path().join("db")).unwrap();
        let run_id = RunId::new();
        let ns = create_namespace(run_id);
        let key = make_key(&ns, "cas_key");

        db.put(run_id, key.clone(), Value::I64(0)).unwrap();

        group.throughput(Throughput::Elements(1));
        group.bench_function("cas_success", |b| {
            b.iter(|| {
                // Get current version
                let current = db.get(&key).unwrap().unwrap();
                let new_val = match current.value {
                    Value::I64(n) => n + 1,
                    _ => 1,
                };
                // CAS with correct version
                let result = db.cas(run_id, key.clone(), current.version, Value::I64(new_val));
                black_box(result.unwrap());
            });
        });
    }

    // ---------------------------------------------------------------------------
    // KV Delete Throughput
    // ---------------------------------------------------------------------------
    {
        let temp_dir = TempDir::new().unwrap();
        let db = Database::open(temp_dir.path().join("db")).unwrap();
        let run_id = RunId::new();
        let ns = create_namespace(run_id);

        group.throughput(Throughput::Elements(1));
        group.bench_function("delete", |b| {
            let counter = AtomicU64::new(0);
            b.iter_custom(|iters| {
                // Setup: create keys to delete
                let start_idx = counter.fetch_add(iters, Ordering::Relaxed);
                for i in start_idx..(start_idx + iters) {
                    let key = make_key(&ns, &format!("delete_{}", i));
                    db.put(run_id, key, Value::I64(i as i64)).unwrap();
                }

                // Benchmark: delete all
                let start = Instant::now();
                for i in start_idx..(start_idx + iters) {
                    let key = make_key(&ns, &format!("delete_{}", i));
                    db.delete(run_id, key).unwrap();
                }
                start.elapsed()
            });
        });
    }

    // ---------------------------------------------------------------------------
    // Value Size Impact
    // ---------------------------------------------------------------------------
    for value_size in [64, 256, 1024, 4096] {
        let temp_dir = TempDir::new().unwrap();
        let db = Database::open(temp_dir.path().join("db")).unwrap();
        let run_id = RunId::new();
        let ns = create_namespace(run_id);

        let data = vec![0u8; value_size];

        group.throughput(Throughput::Bytes(value_size as u64));
        group.bench_with_input(
            BenchmarkId::new("put_value_size_bytes", value_size),
            &value_size,
            |b, _| {
                let counter = AtomicU64::new(0);
                b.iter(|| {
                    let i = counter.fetch_add(1, Ordering::Relaxed);
                    let key = make_key(&ns, &format!("sized_{}", i));
                    let result = db.put(run_id, key, Value::Bytes(data.clone()));
                    black_box(result.unwrap());
                });
            },
        );
    }

    group.finish();
}

// =============================================================================
// Tier 1b: Event Microbenchmarks
// =============================================================================

fn event_microbenchmarks(c: &mut Criterion) {
    let mut group = c.benchmark_group("event_microbenchmarks");

    // ---------------------------------------------------------------------------
    // Event Append Throughput
    // Target: 50-200+ Kops/sec
    // ---------------------------------------------------------------------------
    {
        let temp_dir = TempDir::new().unwrap();
        let db = Database::open(temp_dir.path().join("db")).unwrap();
        let run_id = RunId::new();
        let ns = create_namespace(run_id);

        group.throughput(Throughput::Elements(1));
        group.bench_function("event_append", |b| {
            let seq = AtomicU64::new(0);
            b.iter(|| {
                let s = seq.fetch_add(1, Ordering::Relaxed);
                let key = make_event_key(&ns, s);
                let event_data = Value::Bytes(format!("event_payload_{}", s).into_bytes());
                let result = db.put(run_id, key, event_data);
                black_box(result.unwrap());
            });
        });
    }

    // ---------------------------------------------------------------------------
    // Event Scan Throughput
    // Target: 10-50+ Kops/sec (per scan result)
    // ---------------------------------------------------------------------------
    for num_events in [100, 1000, 10000] {
        let temp_dir = TempDir::new().unwrap();
        let db = Database::open(temp_dir.path().join("db")).unwrap();
        let run_id = RunId::new();
        let ns = create_namespace(run_id);

        // Pre-populate events
        for seq in 0..num_events {
            let key = make_event_key(&ns, seq);
            db.put(
                run_id,
                key,
                Value::Bytes(format!("event_{}", seq).into_bytes()),
            )
            .unwrap();
        }

        group.throughput(Throughput::Elements(num_events as u64));
        group.bench_with_input(
            BenchmarkId::new("event_scan", num_events),
            &num_events,
            |b, _| {
                let prefix = Key::new(ns.clone(), TypeTag::Event, vec![]);
                b.iter(|| {
                    let results = db.storage().scan_prefix(&prefix, u64::MAX);
                    black_box(results.unwrap());
                });
            },
        );
    }

    group.finish();
}

// =============================================================================
// Tier 2: Concurrency Benchmarks
// =============================================================================

fn concurrency_benchmarks(c: &mut Criterion) {
    let mut group = c.benchmark_group("concurrency");
    group.sample_size(50); // Fewer samples for concurrent benchmarks

    // ---------------------------------------------------------------------------
    // Transaction Throughput - No Contention (Different Keys)
    // Target: 80-90% of single-threaded throughput
    // ---------------------------------------------------------------------------
    for num_threads in [2, 4, 8, 16] {
        group.throughput(Throughput::Elements(num_threads as u64));
        group.bench_with_input(
            BenchmarkId::new("txn_no_contention_threads", num_threads),
            &num_threads,
            |b, &num_threads| {
                b.iter_custom(|iters| {
                    let temp_dir = TempDir::new().unwrap();
                    let db = Arc::new(Database::open(temp_dir.path().join("db")).unwrap());
                    let run_id = RunId::new();

                    let barrier = Arc::new(Barrier::new(num_threads + 1));
                    let ops_per_thread = iters / num_threads as u64;

                    let handles: Vec<_> = (0..num_threads)
                        .map(|thread_id| {
                            let db = Arc::clone(&db);
                            let barrier = Arc::clone(&barrier);
                            let ns = create_namespace(run_id);

                            thread::spawn(move || {
                                barrier.wait();
                                for i in 0..ops_per_thread {
                                    let key = make_key(&ns, &format!("t{}_{}", thread_id, i));
                                    db.transaction(run_id, |txn| {
                                        txn.put(key.clone(), Value::I64(i as i64))?;
                                        Ok(())
                                    })
                                    .unwrap();
                                }
                            })
                        })
                        .collect();

                    let start = Instant::now();
                    barrier.wait(); // Release all threads

                    for h in handles {
                        h.join().unwrap();
                    }

                    start.elapsed()
                });
            },
        );
    }

    // ---------------------------------------------------------------------------
    // Transaction Throughput - High Contention (Same Key)
    // Measures conflict detection and retry overhead
    // ---------------------------------------------------------------------------
    for num_threads in [2, 4, 8] {
        group.throughput(Throughput::Elements(num_threads as u64));
        group.bench_with_input(
            BenchmarkId::new("txn_high_contention_threads", num_threads),
            &num_threads,
            |b, &num_threads| {
                b.iter_custom(|iters| {
                    let temp_dir = TempDir::new().unwrap();
                    let db = Arc::new(Database::open(temp_dir.path().join("db")).unwrap());
                    let run_id = RunId::new();
                    let ns = create_namespace(run_id);
                    let contested_key = make_key(&ns, "contested");

                    // Initialize
                    db.put(run_id, contested_key.clone(), Value::I64(0)).unwrap();

                    let barrier = Arc::new(Barrier::new(num_threads + 1));
                    let ops_per_thread = iters / num_threads as u64;

                    let handles: Vec<_> = (0..num_threads)
                        .map(|_| {
                            let db = Arc::clone(&db);
                            let barrier = Arc::clone(&barrier);
                            let key = contested_key.clone();

                            thread::spawn(move || {
                                barrier.wait();
                                for _ in 0..ops_per_thread {
                                    // Retry on conflict
                                    loop {
                                        let result = db.transaction(run_id, |txn| {
                                            let val = txn.get(&key)?;
                                            let n = match val {
                                                Some(Value::I64(n)) => n,
                                                _ => 0,
                                            };
                                            txn.put(key.clone(), Value::I64(n + 1))?;
                                            Ok(())
                                        });
                                        if result.is_ok() {
                                            break;
                                        }
                                        // Small backoff
                                        thread::sleep(Duration::from_micros(10));
                                    }
                                }
                            })
                        })
                        .collect();

                    let start = Instant::now();
                    barrier.wait();

                    for h in handles {
                        h.join().unwrap();
                    }

                    start.elapsed()
                });
            },
        );
    }

    // ---------------------------------------------------------------------------
    // Conflict Rate Measurement
    // ---------------------------------------------------------------------------
    group.bench_function("conflict_rate_measurement", |b| {
        b.iter_custom(|iters| {
            let temp_dir = TempDir::new().unwrap();
            let db = Arc::new(Database::open(temp_dir.path().join("db")).unwrap());
            let run_id = RunId::new();
            let ns = create_namespace(run_id);
            let key = make_key(&ns, "conflict_key");

            db.put(run_id, key.clone(), Value::I64(0)).unwrap();

            let num_threads: usize = 4;
            let ops_per_thread = iters / num_threads as u64;
            let conflict_count = Arc::new(AtomicU64::new(0));
            let barrier = Arc::new(Barrier::new(num_threads + 1));

            let handles: Vec<_> = (0..num_threads)
                .map(|_| {
                    let db = Arc::clone(&db);
                    let barrier = Arc::clone(&barrier);
                    let conflict_count = Arc::clone(&conflict_count);
                    let key = key.clone();

                    thread::spawn(move || {
                        barrier.wait();
                        for _ in 0..ops_per_thread {
                            let result = db.transaction(run_id, |txn| {
                                let val = txn.get(&key)?;
                                let n = match val {
                                    Some(Value::I64(n)) => n,
                                    _ => 0,
                                };
                                thread::sleep(Duration::from_micros(1)); // Increase conflict window
                                txn.put(key.clone(), Value::I64(n + 1))?;
                                Ok(())
                            });
                            if result.is_err() {
                                conflict_count.fetch_add(1, Ordering::Relaxed);
                            }
                        }
                    })
                })
                .collect();

            let start = Instant::now();
            barrier.wait();

            for h in handles {
                h.join().unwrap();
            }

            start.elapsed()
        });
    });

    // ---------------------------------------------------------------------------
    // CAS Under Contention (Exactly One Winner)
    // ---------------------------------------------------------------------------
    group.bench_function("cas_contention_one_winner", |b| {
        b.iter_custom(|iters| {
            let mut total_elapsed = Duration::ZERO;

            for _ in 0..iters {
                let temp_dir = TempDir::new().unwrap();
                let db = Arc::new(Database::open(temp_dir.path().join("db")).unwrap());
                let run_id = RunId::new();
                let ns = create_namespace(run_id);
                let key = make_key(&ns, "cas_contention");

                db.put(run_id, key.clone(), Value::I64(0)).unwrap();
                let initial_version = db.get(&key).unwrap().unwrap().version;

                let num_threads: usize = 4;
                let barrier = Arc::new(Barrier::new(num_threads + 1));
                let winners = Arc::new(AtomicU64::new(0));

                let handles: Vec<_> = (0..num_threads)
                    .map(|id| {
                        let db = Arc::clone(&db);
                        let barrier = Arc::clone(&barrier);
                        let winners = Arc::clone(&winners);
                        let key = key.clone();

                        thread::spawn(move || {
                            barrier.wait();
                            let result = db.cas(run_id, key, initial_version, Value::I64(id as i64));
                            if result.is_ok() {
                                winners.fetch_add(1, Ordering::Relaxed);
                            }
                        })
                    })
                    .collect();

                let start = Instant::now();
                barrier.wait();

                for h in handles {
                    h.join().unwrap();
                }

                total_elapsed += start.elapsed();

                // Verify exactly one winner
                assert_eq!(winners.load(Ordering::Relaxed), 1);
            }

            total_elapsed
        });
    });

    group.finish();
}

// =============================================================================
// Tier 3: Recovery Benchmarks
// =============================================================================

fn recovery_benchmarks(c: &mut Criterion) {
    let mut group = c.benchmark_group("recovery");
    group.sample_size(20); // Fewer samples, these are slow

    // ---------------------------------------------------------------------------
    // WAL Replay Time
    // Target: <300ms per 1M ops, acceptable <1s
    // ---------------------------------------------------------------------------
    for num_ops in [1_000, 10_000, 100_000] {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("db");

        // Setup: populate database
        {
            let db = Database::open(&db_path).unwrap();
            let run_id = RunId::new();
            let ns = create_namespace(run_id);

            for i in 0..num_ops {
                let key = make_key(&ns, &format!("key_{}", i));
                db.put(run_id, key, Value::I64(i as i64)).unwrap();
            }
            // Ensure flushed
            db.flush().unwrap();
        }

        group.throughput(Throughput::Elements(num_ops as u64));
        group.bench_with_input(
            BenchmarkId::new("wal_replay", num_ops),
            &num_ops,
            |b, _| {
                b.iter(|| {
                    // Re-open database (triggers recovery)
                    let db = Database::open(&db_path).unwrap();
                    black_box(db.storage().current_version());
                });
            },
        );
    }

    // ---------------------------------------------------------------------------
    // Mixed Workload Recovery (puts, deletes, transactions)
    // ---------------------------------------------------------------------------
    for num_txns in [100, 1000, 5000] {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("db");

        // Setup: create mixed workload
        {
            let db = Database::open(&db_path).unwrap();
            let run_id = RunId::new();
            let ns = create_namespace(run_id);

            for i in 0..num_txns {
                // Multi-key transaction
                db.transaction(run_id, |txn| {
                    for j in 0..5 {
                        let key = make_key(&ns, &format!("txn_{}_{}", i, j));
                        txn.put(key, Value::I64((i * 5 + j) as i64))?;
                    }
                    Ok(())
                })
                .unwrap();

                // Some deletes
                if i % 10 == 0 && i > 0 {
                    let key_to_delete = make_key(&ns, &format!("txn_{}_0", i - 5));
                    db.delete(run_id, key_to_delete).unwrap();
                }
            }
            db.flush().unwrap();
        }

        group.throughput(Throughput::Elements(num_txns as u64));
        group.bench_with_input(
            BenchmarkId::new("mixed_workload_recovery", num_txns),
            &num_txns,
            |b, _| {
                b.iter(|| {
                    let db = Database::open(&db_path).unwrap();
                    black_box(db.storage().current_version());
                });
            },
        );
    }

    group.finish();
}

// =============================================================================
// Tier 4: Durability Benchmarks
// =============================================================================

fn durability_benchmarks(c: &mut Criterion) {
    let mut group = c.benchmark_group("durability");
    group.sample_size(30);

    // ---------------------------------------------------------------------------
    // Strict Mode (fsync per commit)
    // ---------------------------------------------------------------------------
    {
        let temp_dir = TempDir::new().unwrap();
        let db = Database::open_with_mode(
            temp_dir.path().join("db"),
            DurabilityMode::Strict,
        )
        .unwrap();
        let run_id = RunId::new();
        let ns = create_namespace(run_id);

        group.throughput(Throughput::Elements(1));
        group.bench_function("strict_mode_put", |b| {
            let counter = AtomicU64::new(0);
            b.iter(|| {
                let i = counter.fetch_add(1, Ordering::Relaxed);
                let key = make_key(&ns, &format!("strict_{}", i));
                let result = db.put(run_id, key, Value::I64(i as i64));
                black_box(result.unwrap());
            });
        });
    }

    // ---------------------------------------------------------------------------
    // Batched Mode (fsync every N ms or N ops)
    // Target: <2-10ms median commit latency
    // ---------------------------------------------------------------------------
    for batch_size in [100, 500, 1000] {
        let temp_dir = TempDir::new().unwrap();
        let db = Database::open_with_mode(
            temp_dir.path().join("db"),
            DurabilityMode::Batched {
                interval_ms: 100,
                batch_size,
            },
        )
        .unwrap();
        let run_id = RunId::new();
        let ns = create_namespace(run_id);

        group.throughput(Throughput::Elements(1));
        group.bench_with_input(
            BenchmarkId::new("batched_mode_put", batch_size),
            &batch_size,
            |b, _| {
                let counter = AtomicU64::new(0);
                b.iter(|| {
                    let i = counter.fetch_add(1, Ordering::Relaxed);
                    let key = make_key(&ns, &format!("batched_{}", i));
                    let result = db.put(run_id, key, Value::I64(i as i64));
                    black_box(result.unwrap());
                });
            },
        );
    }

    // ---------------------------------------------------------------------------
    // Async Mode (background fsync)
    // ---------------------------------------------------------------------------
    {
        let temp_dir = TempDir::new().unwrap();
        let db = Database::open_with_mode(
            temp_dir.path().join("db"),
            DurabilityMode::Async { interval_ms: 50 },
        )
        .unwrap();
        let run_id = RunId::new();
        let ns = create_namespace(run_id);

        group.throughput(Throughput::Elements(1));
        group.bench_function("async_mode_put", |b| {
            let counter = AtomicU64::new(0);
            b.iter(|| {
                let i = counter.fetch_add(1, Ordering::Relaxed);
                let key = make_key(&ns, &format!("async_{}", i));
                let result = db.put(run_id, key, Value::I64(i as i64));
                black_box(result.unwrap());
            });
        });
    }

    // ---------------------------------------------------------------------------
    // Transaction Durability Comparison
    // ---------------------------------------------------------------------------
    for mode_name in ["strict", "batched", "async"] {
        let temp_dir = TempDir::new().unwrap();
        let mode = match mode_name {
            "strict" => DurabilityMode::Strict,
            "batched" => DurabilityMode::Batched {
                interval_ms: 100,
                batch_size: 500,
            },
            "async" => DurabilityMode::Async { interval_ms: 50 },
            _ => unreachable!(),
        };

        let db = Database::open_with_mode(temp_dir.path().join("db"), mode).unwrap();
        let run_id = RunId::new();
        let ns = create_namespace(run_id);

        group.throughput(Throughput::Elements(1));
        group.bench_with_input(
            BenchmarkId::new("transaction_durability", mode_name),
            &mode_name,
            |b, _| {
                let counter = AtomicU64::new(0);
                b.iter(|| {
                    let i = counter.fetch_add(1, Ordering::Relaxed);
                    let result = db.transaction(run_id, |txn| {
                        for j in 0..3 {
                            let key = make_key(&ns, &format!("txn_{}_{}", i, j));
                            txn.put(key, Value::I64((i * 3 + j) as i64))?;
                        }
                        Ok(())
                    });
                    black_box(result.unwrap());
                });
            },
        );
    }

    group.finish();
}

// =============================================================================
// Tier 5: Memory Benchmarks
// =============================================================================

fn memory_benchmarks(c: &mut Criterion) {
    let mut group = c.benchmark_group("memory");
    group.sample_size(20);

    // ---------------------------------------------------------------------------
    // Snapshot Creation Time (scales with data size)
    // Target: <100ms per 1M keys
    // ---------------------------------------------------------------------------
    for num_keys in [1_000, 10_000, 100_000] {
        let temp_dir = TempDir::new().unwrap();
        let db = Database::open(temp_dir.path().join("db")).unwrap();
        let run_id = RunId::new();
        let ns = create_namespace(run_id);

        // Pre-populate
        for i in 0..num_keys {
            let key = make_key(&ns, &format!("key_{}", i));
            db.put(run_id, key, Value::I64(i as i64)).unwrap();
        }

        group.throughput(Throughput::Elements(1));
        group.bench_with_input(
            BenchmarkId::new("snapshot_creation", num_keys),
            &num_keys,
            |b, _| {
                b.iter(|| {
                    // begin_transaction creates a snapshot
                    let txn = db.begin_transaction(run_id);
                    black_box(txn);
                });
            },
        );
    }

    // ---------------------------------------------------------------------------
    // Prefix Scan Performance (scales with result size)
    // ---------------------------------------------------------------------------
    for num_keys in [100, 1000, 10000] {
        let temp_dir = TempDir::new().unwrap();
        let db = Database::open(temp_dir.path().join("db")).unwrap();
        let run_id = RunId::new();
        let ns = create_namespace(run_id);

        // Create keys with common prefix
        for i in 0..num_keys {
            let key = make_key(&ns, &format!("prefix_scan_{:05}", i));
            db.put(run_id, key, Value::I64(i as i64)).unwrap();
        }

        // Also create keys with different prefix (noise)
        for i in 0..1000 {
            let key = make_key(&ns, &format!("other_{}", i));
            db.put(run_id, key, Value::I64(i as i64)).unwrap();
        }

        group.throughput(Throughput::Elements(num_keys as u64));
        group.bench_with_input(
            BenchmarkId::new("prefix_scan", num_keys),
            &num_keys,
            |b, _| {
                let prefix = Key::new_kv(ns.clone(), "prefix_scan_");
                b.iter(|| {
                    let results = db.storage().scan_prefix(&prefix, u64::MAX);
                    black_box(results.unwrap());
                });
            },
        );
    }

    group.finish();
}

// =============================================================================
// Tier 6: Scenario Benchmarks (Agent Workloads)
// =============================================================================

fn scenario_benchmarks(c: &mut Criterion) {
    let mut group = c.benchmark_group("scenario");
    group.sample_size(20);

    // ---------------------------------------------------------------------------
    // Agent Run Simulation
    // Typical pattern: append events, checkpoint state, read history
    // ---------------------------------------------------------------------------
    group.bench_function("agent_run_simulation", |b| {
        b.iter_custom(|iters| {
            let temp_dir = TempDir::new().unwrap();
            let db = Database::open(temp_dir.path().join("db")).unwrap();
            let run_id = RunId::new();
            let ns = create_namespace(run_id);

            let start = Instant::now();

            for run in 0..iters {
                // 1. Append events (like tool calls, LLM responses)
                for event_idx in 0..10 {
                    let key = make_event_key(&ns, run * 10 + event_idx);
                    db.put(
                        run_id,
                        key,
                        Value::Bytes(format!("event_data_{}", event_idx).into_bytes()),
                    )
                    .unwrap();
                }

                // 2. Update state (like conversation context)
                db.transaction(run_id, |txn| {
                    let state_key = make_key(&ns, "agent_state");
                    txn.put(
                        state_key,
                        Value::Bytes(format!("state_at_step_{}", run).into_bytes()),
                    )?;

                    let counter_key = make_key(&ns, "step_counter");
                    txn.put(counter_key, Value::I64(run as i64))?;
                    Ok(())
                })
                .unwrap();

                // 3. Occasional history read (every 5 steps)
                if run % 5 == 0 {
                    let prefix = Key::new(ns.clone(), TypeTag::Event, vec![]);
                    let _ = db.storage().scan_prefix(&prefix, u64::MAX);
                }
            }

            start.elapsed()
        });
    });

    // ---------------------------------------------------------------------------
    // Multi-Agent Coordination
    // Multiple agents sharing state with transactions
    // ---------------------------------------------------------------------------
    for num_agents in [2, 4, 8] {
        group.bench_with_input(
            BenchmarkId::new("multi_agent_coordination", num_agents),
            &num_agents,
            |b, &num_agents| {
                b.iter_custom(|iters| {
                    let temp_dir = TempDir::new().unwrap();
                    let db = Arc::new(Database::open(temp_dir.path().join("db")).unwrap());
                    let run_id = RunId::new();
                    let ns = create_namespace(run_id);

                    // Shared state
                    let shared_key = make_key(&ns, "shared_resource");
                    db.put(run_id, shared_key.clone(), Value::I64(0)).unwrap();

                    let barrier = Arc::new(Barrier::new(num_agents + 1));
                    let ops_per_agent = iters / num_agents as u64;

                    let handles: Vec<_> = (0..num_agents)
                        .map(|agent_id| {
                            let db = Arc::clone(&db);
                            let barrier = Arc::clone(&barrier);
                            let ns = ns.clone();
                            let shared_key = shared_key.clone();

                            thread::spawn(move || {
                                barrier.wait();

                                for step in 0..ops_per_agent {
                                    // Agent-local state update
                                    let local_key = make_key(&ns, &format!("agent_{}_state", agent_id));
                                    db.put(run_id, local_key, Value::I64(step as i64)).unwrap();

                                    // Occasional shared state update
                                    if step % 3 == 0 {
                                        let _ = db.transaction(run_id, |txn| {
                                            let val = txn.get(&shared_key)?;
                                            let n = match val {
                                                Some(Value::I64(n)) => n,
                                                _ => 0,
                                            };
                                            txn.put(shared_key.clone(), Value::I64(n + 1))?;
                                            Ok(())
                                        });
                                    }
                                }
                            })
                        })
                        .collect();

                    let start = Instant::now();
                    barrier.wait();

                    for h in handles {
                        h.join().unwrap();
                    }

                    start.elapsed()
                });
            },
        );
    }

    // ---------------------------------------------------------------------------
    // Replay Simulation
    // Read all data for a run after completion
    // ---------------------------------------------------------------------------
    for num_events in [100, 1000, 5000] {
        let temp_dir = TempDir::new().unwrap();
        let db = Database::open(temp_dir.path().join("db")).unwrap();
        let run_id = RunId::new();
        let ns = create_namespace(run_id);

        // Populate run data
        for i in 0..num_events {
            let event_key = make_event_key(&ns, i);
            db.put(
                run_id,
                event_key,
                Value::Bytes(format!("event_payload_{}", i).into_bytes()),
            )
            .unwrap();
        }

        // State checkpoints
        for i in 0..10 {
            let state_key = make_key(&ns, &format!("checkpoint_{}", i));
            db.put(run_id, state_key, Value::I64(i)).unwrap();
        }

        group.throughput(Throughput::Elements(num_events as u64));
        group.bench_with_input(
            BenchmarkId::new("replay_simulation", num_events),
            &num_events,
            |b, _| {
                b.iter(|| {
                    // Read all events
                    let event_prefix = Key::new(ns.clone(), TypeTag::Event, vec![]);
                    let events = db.storage().scan_prefix(&event_prefix, u64::MAX);
                    black_box(events.unwrap());

                    // Read all KV state
                    let kv_prefix = Key::new(ns.clone(), TypeTag::KV, vec![]);
                    let state = db.storage().scan_prefix(&kv_prefix, u64::MAX);
                    black_box(state.unwrap());
                });
            },
        );
    }

    group.finish();
}

// =============================================================================
// Latency Distribution Benchmarks
// =============================================================================

fn latency_distribution(c: &mut Criterion) {
    let mut group = c.benchmark_group("latency_distribution");
    group.sample_size(1000); // Many samples for distribution

    // ---------------------------------------------------------------------------
    // Put Latency Distribution
    // Target: P50 <1ms, P95 <10ms, P99 <50ms
    // ---------------------------------------------------------------------------
    {
        let temp_dir = TempDir::new().unwrap();
        let db = Database::open(temp_dir.path().join("db")).unwrap();
        let run_id = RunId::new();
        let ns = create_namespace(run_id);

        group.bench_function("put_latency", |b| {
            let counter = AtomicU64::new(0);
            b.iter(|| {
                let i = counter.fetch_add(1, Ordering::Relaxed);
                let key = make_key(&ns, &format!("lat_{}", i));
                let result = db.put(run_id, key, Value::I64(i as i64));
                black_box(result.unwrap());
            });
        });
    }

    // ---------------------------------------------------------------------------
    // Get Latency Distribution
    // ---------------------------------------------------------------------------
    {
        let temp_dir = TempDir::new().unwrap();
        let db = Database::open(temp_dir.path().join("db")).unwrap();
        let run_id = RunId::new();
        let ns = create_namespace(run_id);

        // Pre-populate
        for i in 0..10_000 {
            let key = make_key(&ns, &format!("lat_read_{}", i));
            db.put(run_id, key, Value::I64(i)).unwrap();
        }

        group.bench_function("get_latency", |b| {
            let counter = AtomicU64::new(0);
            b.iter(|| {
                let i = (counter.fetch_add(1, Ordering::Relaxed) % 10_000) as i64;
                let key = make_key(&ns, &format!("lat_read_{}", i));
                let result = db.get(&key);
                black_box(result.unwrap());
            });
        });
    }

    // ---------------------------------------------------------------------------
    // Transaction Latency Distribution
    // ---------------------------------------------------------------------------
    {
        let temp_dir = TempDir::new().unwrap();
        let db = Database::open(temp_dir.path().join("db")).unwrap();
        let run_id = RunId::new();
        let ns = create_namespace(run_id);

        group.bench_function("transaction_latency", |b| {
            let counter = AtomicU64::new(0);
            b.iter(|| {
                let i = counter.fetch_add(1, Ordering::Relaxed);
                let result = db.transaction(run_id, |txn| {
                    let key = make_key(&ns, &format!("txn_lat_{}", i));
                    txn.put(key, Value::I64(i as i64))?;
                    Ok(())
                });
                black_box(result.unwrap());
            });
        });
    }

    group.finish();
}

// =============================================================================
// Benchmark Groups
// =============================================================================

criterion_group!(
    name = microbenchmarks;
    config = Criterion::default().measurement_time(Duration::from_secs(10));
    targets = kv_microbenchmarks, event_microbenchmarks
);

criterion_group!(
    name = concurrent;
    config = Criterion::default()
        .measurement_time(Duration::from_secs(15))
        .sample_size(50);
    targets = concurrency_benchmarks
);

criterion_group!(
    name = durability_recovery;
    config = Criterion::default()
        .measurement_time(Duration::from_secs(20))
        .sample_size(30);
    targets = recovery_benchmarks, durability_benchmarks
);

criterion_group!(
    name = memory_scenarios;
    config = Criterion::default()
        .measurement_time(Duration::from_secs(15))
        .sample_size(30);
    targets = memory_benchmarks, scenario_benchmarks
);

criterion_group!(
    name = latency;
    config = Criterion::default()
        .measurement_time(Duration::from_secs(10))
        .sample_size(1000);
    targets = latency_distribution
);

criterion_main!(
    microbenchmarks,
    concurrent,
    durability_recovery,
    memory_scenarios,
    latency
);
