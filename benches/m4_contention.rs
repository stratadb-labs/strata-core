//! M4 Contention Scaling Benchmarks
//!
//! Measures scaling behavior under various contention patterns:
//! - Disjoint runs: Each thread uses different RunId (no contention)
//! - Shared run: All threads use same RunId (maximum contention)
//! - Mixed pattern: Realistic mix of disjoint and shared access
//!
//! Run with: cargo bench --bench m4_contention
//!
//! Expected results (M4 targets):
//! - Disjoint scaling: ≥1.8× at 2T, ≥3.2× at 4T
//! - 4-thread disjoint throughput: ≥800K ops/sec

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use in_mem_core::types::RunId;
use in_mem_core::value::Value;
use in_mem_engine::Database;
use in_mem_primitives::KVStore;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

const ITERATIONS_PER_THREAD: usize = 1000;

/// Disjoint run pattern - each thread operates on different RunId
fn bench_disjoint_scaling(c: &mut Criterion) {
    let mut group = c.benchmark_group("contention/disjoint");
    group.measurement_time(Duration::from_secs(15));
    group.throughput(Throughput::Elements(ITERATIONS_PER_THREAD as u64));

    for threads in [1, 2, 4, 8] {
        group.bench_function(BenchmarkId::new("puts", threads), |b| {
            b.iter(|| {
                let db = Arc::new(Database::builder().in_memory().open_temp().unwrap());

                let handles: Vec<_> = (0..threads)
                    .map(|_| {
                        let db = Arc::clone(&db);
                        std::thread::spawn(move || {
                            let kv = KVStore::new(db);
                            let run_id = RunId::new(); // Different run per thread
                            for i in 0..ITERATIONS_PER_THREAD {
                                kv.put(&run_id, &format!("key{}", i), Value::I64(i as i64))
                                    .unwrap();
                            }
                        })
                    })
                    .collect();

                for h in handles {
                    h.join().unwrap();
                }
            });
        });
    }

    group.finish();
}

/// Shared run pattern - all threads operate on same RunId
fn bench_shared_scaling(c: &mut Criterion) {
    let mut group = c.benchmark_group("contention/shared");
    group.measurement_time(Duration::from_secs(15));
    group.throughput(Throughput::Elements(ITERATIONS_PER_THREAD as u64));

    for threads in [1, 2, 4, 8] {
        group.bench_function(BenchmarkId::new("puts", threads), |b| {
            b.iter(|| {
                let db = Arc::new(Database::builder().in_memory().open_temp().unwrap());
                let run_id = RunId::new(); // Same run for all threads

                let counter = Arc::new(AtomicUsize::new(0));

                let handles: Vec<_> = (0..threads)
                    .map(|_| {
                        let db = Arc::clone(&db);
                        let counter = Arc::clone(&counter);
                        std::thread::spawn(move || {
                            let kv = KVStore::new(db);
                            for _ in 0..ITERATIONS_PER_THREAD {
                                let i = counter.fetch_add(1, Ordering::Relaxed);
                                kv.put(&run_id, &format!("key{}", i), Value::I64(i as i64))
                                    .unwrap();
                            }
                        })
                    })
                    .collect();

                for h in handles {
                    h.join().unwrap();
                }
            });
        });
    }

    group.finish();
}

/// Mixed read-write pattern
fn bench_mixed_read_write(c: &mut Criterion) {
    let mut group = c.benchmark_group("contention/mixed");
    group.measurement_time(Duration::from_secs(15));

    for threads in [1, 2, 4] {
        group.bench_function(BenchmarkId::new("80read_20write", threads), |b| {
            let db = Arc::new(Database::builder().in_memory().open_temp().unwrap());
            let kv = KVStore::new(db.clone());
            let run_id = RunId::new();

            // Pre-populate with data
            for i in 0..1000 {
                kv.put(&run_id, &format!("read_key{}", i), Value::I64(i as i64))
                    .unwrap();
            }

            b.iter(|| {
                let handles: Vec<_> = (0..threads)
                    .map(|t| {
                        let db = Arc::clone(&db);
                        std::thread::spawn(move || {
                            let kv = KVStore::new(db);
                            for i in 0..ITERATIONS_PER_THREAD {
                                if i % 5 == 0 {
                                    // 20% writes
                                    kv.put(
                                        &run_id,
                                        &format!("write_t{}_{}", t, i),
                                        Value::I64(i as i64),
                                    )
                                    .unwrap();
                                } else {
                                    // 80% reads
                                    let _ = kv.get(&run_id, &format!("read_key{}", i % 1000));
                                }
                            }
                        })
                    })
                    .collect();

                for h in handles {
                    h.join().unwrap();
                }
            });
        });
    }

    group.finish();
}

/// Throughput measurement at different thread counts
fn bench_throughput_scaling(c: &mut Criterion) {
    let mut group = c.benchmark_group("contention/throughput");
    group.measurement_time(Duration::from_secs(20));
    group.throughput(Throughput::Elements(
        (ITERATIONS_PER_THREAD * 4) as u64, // 4 threads
    ));

    // Measure absolute throughput with disjoint runs
    group.bench_function("disjoint_4thread_4000ops", |b| {
        b.iter(|| {
            let db = Arc::new(Database::builder().in_memory().open_temp().unwrap());

            let handles: Vec<_> = (0..4)
                .map(|_| {
                    let db = Arc::clone(&db);
                    std::thread::spawn(move || {
                        let kv = KVStore::new(db);
                        let run_id = RunId::new();
                        for i in 0..ITERATIONS_PER_THREAD {
                            kv.put(&run_id, &format!("key{}", i), Value::I64(i as i64))
                                .unwrap();
                        }
                    })
                })
                .collect();

            for h in handles {
                h.join().unwrap();
            }
        });
    });

    group.finish();
}

/// Lock contention measurement - tracks time spent waiting
fn bench_lock_contention(c: &mut Criterion) {
    let mut group = c.benchmark_group("contention/lock_overhead");
    group.measurement_time(Duration::from_secs(10));

    // Single thread (no contention)
    group.bench_function(BenchmarkId::new("single_thread", "baseline"), |b| {
        let db = Arc::new(Database::builder().in_memory().open_temp().unwrap());
        let kv = KVStore::new(db);
        let run_id = RunId::new();

        b.iter(|| {
            for i in 0..100 {
                kv.put(&run_id, &format!("key{}", i), Value::I64(i as i64))
                    .unwrap();
            }
        });
    });

    // Multi-thread same run (high contention)
    group.bench_function(BenchmarkId::new("4_threads_same_run", "contended"), |b| {
        b.iter(|| {
            let db = Arc::new(Database::builder().in_memory().open_temp().unwrap());
            let run_id = RunId::new();

            let handles: Vec<_> = (0..4)
                .map(|t| {
                    let db = Arc::clone(&db);
                    std::thread::spawn(move || {
                        let kv = KVStore::new(db);
                        for i in 0..25 {
                            kv.put(&run_id, &format!("t{}_key{}", t, i), Value::I64(i as i64))
                                .unwrap();
                        }
                    })
                })
                .collect();

            for h in handles {
                h.join().unwrap();
            }
        });
    });

    // Multi-thread different runs (low contention)
    group.bench_function(
        BenchmarkId::new("4_threads_diff_runs", "uncontended"),
        |b| {
            b.iter(|| {
                let db = Arc::new(Database::builder().in_memory().open_temp().unwrap());

                let handles: Vec<_> = (0..4)
                    .map(|_| {
                        let db = Arc::clone(&db);
                        std::thread::spawn(move || {
                            let kv = KVStore::new(db);
                            let run_id = RunId::new(); // Different run
                            for i in 0..25 {
                                kv.put(&run_id, &format!("key{}", i), Value::I64(i as i64))
                                    .unwrap();
                            }
                        })
                    })
                    .collect();

                for h in handles {
                    h.join().unwrap();
                }
            });
        },
    );

    group.finish();
}

criterion_group!(
    name = contention;
    config = Criterion::default().sample_size(50);
    targets =
        bench_disjoint_scaling,
        bench_shared_scaling,
        bench_mixed_read_write,
        bench_throughput_scaling,
        bench_lock_contention
);

criterion_main!(contention);
