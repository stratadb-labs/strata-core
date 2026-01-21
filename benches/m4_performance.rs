//! M4 Performance Benchmarks
//!
//! Run with: cargo bench --bench m4_performance
//! Compare to baseline: checkout m3_baseline_perf tag
//!
//! These benchmarks measure progress toward M4 performance goals:
//! - InMemory mode: <3µs put, 250K ops/sec
//! - Buffered mode: <30µs put, 50K ops/sec
//! - Strict mode: ~2ms put (baseline)
//! - Snapshot acquisition: <500ns (red flag: >2µs)

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use strata_core::types::RunId;
use strata_core::value::{Value, VersionedValue};
use strata_engine::Database;
use strata_primitives::KVStore;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

/// Latency benchmarks for each durability mode
fn latency_benchmarks(c: &mut Criterion) {
    let mut group = c.benchmark_group("latency");
    group.measurement_time(Duration::from_secs(10));

    // InMemory mode
    {
        let db = Database::builder().in_memory().open_temp().unwrap();
        let kv = KVStore::new(Arc::new(db));
        let run_id = RunId::new();

        // Warmup
        for i in 0..100 {
            kv.put(&run_id, &format!("warmup{}", i), Value::I64(i as i64))
                .unwrap();
        }

        group.bench_function(BenchmarkId::new("kvstore/put", "inmemory"), |b| {
            let mut i = 0;
            b.iter(|| {
                i += 1;
                kv.put(&run_id, &format!("key{}", i), Value::I64(i as i64))
                    .unwrap();
            });
        });

        group.bench_function(BenchmarkId::new("kvstore/get", "inmemory"), |b| {
            b.iter(|| {
                kv.get(&run_id, "warmup50").unwrap();
            });
        });
    }

    // Buffered mode
    {
        let db = Database::builder().buffered().open_temp().unwrap();
        let kv = KVStore::new(Arc::new(db));
        let run_id = RunId::new();

        // Warmup
        for i in 0..100 {
            kv.put(&run_id, &format!("warmup{}", i), Value::I64(i as i64))
                .unwrap();
        }

        group.bench_function(BenchmarkId::new("kvstore/put", "buffered"), |b| {
            let mut i = 0;
            b.iter(|| {
                i += 1;
                kv.put(&run_id, &format!("key{}", i), Value::I64(i as i64))
                    .unwrap();
            });
        });

        group.bench_function(BenchmarkId::new("kvstore/get", "buffered"), |b| {
            b.iter(|| {
                kv.get(&run_id, "warmup50").unwrap();
            });
        });
    }

    // Strict mode
    {
        let db = Database::builder().strict().open_temp().unwrap();
        let kv = KVStore::new(Arc::new(db));
        let run_id = RunId::new();

        // Warmup (less iterations due to slow fsync)
        for i in 0..10 {
            kv.put(&run_id, &format!("warmup{}", i), Value::I64(i as i64))
                .unwrap();
        }

        group.bench_function(BenchmarkId::new("kvstore/put", "strict"), |b| {
            let mut i = 0;
            b.iter(|| {
                i += 1;
                kv.put(&run_id, &format!("key{}", i), Value::I64(i as i64))
                    .unwrap();
            });
        });

        group.bench_function(BenchmarkId::new("kvstore/get", "strict"), |b| {
            b.iter(|| {
                kv.get(&run_id, "warmup5").unwrap();
            });
        });
    }

    group.finish();
}

/// Throughput benchmarks
fn throughput_benchmarks(c: &mut Criterion) {
    let mut group = c.benchmark_group("throughput");
    group.measurement_time(Duration::from_secs(15));
    group.throughput(Throughput::Elements(1000));

    let db = Database::builder().in_memory().open_temp().unwrap();
    let kv = KVStore::new(Arc::new(db));
    let run_id = RunId::new();

    group.bench_function("inmemory/1000_puts", |b| {
        let mut batch = 0;
        b.iter(|| {
            batch += 1;
            for i in 0..1000 {
                kv.put(
                    &run_id,
                    &format!("batch{}key{}", batch, i),
                    Value::I64(i as i64),
                )
                .unwrap();
            }
        });
    });

    // Warmup for gets
    for i in 0..1000 {
        kv.put(&run_id, &format!("getkey{}", i), Value::I64(i as i64))
            .unwrap();
    }

    group.bench_function("inmemory/1000_gets", |b| {
        b.iter(|| {
            for i in 0..1000 {
                kv.get(&run_id, &format!("getkey{}", i)).unwrap();
            }
        });
    });

    group.finish();
}

/// Snapshot acquisition benchmarks - CRITICAL for M4
fn snapshot_benchmarks(c: &mut Criterion) {
    let mut group = c.benchmark_group("snapshot");
    group.measurement_time(Duration::from_secs(5));

    let db = Arc::new(Database::builder().in_memory().open_temp().unwrap());

    // Populate with some data
    let kv = KVStore::new(db.clone());
    let run_id = RunId::new();
    for i in 0..1000 {
        kv.put(&run_id, &format!("key{}", i), Value::I64(i as i64))
            .unwrap();
    }

    group.bench_function("acquire", |b| {
        b.iter(|| {
            let _snapshot = db.storage().create_snapshot();
        });
    });

    group.finish();
}

/// Transaction pooling benchmarks
fn transaction_pool_benchmarks(c: &mut Criterion) {
    use strata_engine::TransactionPool;

    let mut group = c.benchmark_group("transaction_pool");
    group.measurement_time(Duration::from_secs(5));

    // Warmup pool
    TransactionPool::warmup(8);

    group.bench_function("acquire_release", |b| {
        let run_id = RunId::new();
        b.iter(|| {
            let ctx = TransactionPool::acquire(1, run_id, None);
            TransactionPool::release(ctx);
        });
    });

    group.finish();
}

/// Read path optimization benchmarks
fn read_path_benchmarks(c: &mut Criterion) {
    let mut group = c.benchmark_group("read_path");
    group.measurement_time(Duration::from_secs(5));

    let db = Database::builder().in_memory().open_temp().unwrap();
    let kv = KVStore::new(Arc::new(db));
    let run_id = RunId::new();

    // Populate
    for i in 0..100 {
        kv.put(&run_id, &format!("key{}", i), Value::I64(i as i64))
            .unwrap();
    }

    group.bench_function("fast_get", |b| {
        b.iter(|| {
            kv.get(&run_id, "key50").unwrap();
        });
    });

    group.bench_function("transaction_get", |b| {
        b.iter(|| {
            kv.get_in_transaction(&run_id, "key50").unwrap();
        });
    });

    group.bench_function("batch_get_10", |b| {
        let keys: Vec<&str> = (0..10)
            .map(|i| {
                [
                    "key0", "key1", "key2", "key3", "key4", "key5", "key6", "key7", "key8", "key9",
                ][i]
            })
            .collect();
        b.iter(|| {
            kv.get_many(&run_id, &keys).unwrap();
        });
    });

    group.finish();
}

/// Facade tax benchmarks - measures overhead at each layer
fn facade_tax_benchmarks(c: &mut Criterion) {
    use strata_core::types::{Key, Namespace, TypeTag};

    let mut group = c.benchmark_group("facade_tax");
    group.measurement_time(Duration::from_secs(5));

    // A0: Raw HashMap (baseline)
    let mut map: HashMap<String, i64> = HashMap::new();
    group.bench_function("A0/hashmap_insert", |b| {
        let mut i = 0;
        b.iter(|| {
            i += 1;
            map.insert(format!("key{}", i), i as i64);
        });
    });

    // A1: Engine storage layer direct (via Storage trait)
    let db = Database::builder().in_memory().open_temp().unwrap();
    let run_id = RunId::new();
    let ns = Namespace::for_run(run_id);

    group.bench_function("A1/storage_put", |b| {
        let mut i = 0;
        b.iter(|| {
            i += 1;
            let key = Key::new(ns.clone(), TypeTag::KV, format!("key{}", i).into_bytes());
            db.storage().put(key, VersionedValue::new(Value::I64(i as i64), i as u64, None));
        });
    });

    // B: Facade layer (KVStore)
    let kv = KVStore::new(Arc::new(db));
    group.bench_function("B/kvstore_put", |b| {
        let mut i = 0;
        b.iter(|| {
            i += 1;
            kv.put(&run_id, &format!("kvkey{}", i), Value::I64(i as i64))
                .unwrap();
        });
    });

    group.finish();
}

/// Contention scaling benchmarks
fn contention_benchmarks(c: &mut Criterion) {
    let mut group = c.benchmark_group("contention");
    group.measurement_time(Duration::from_secs(10));

    for threads in [1, 2, 4] {
        group.bench_function(BenchmarkId::new("disjoint_runs", threads), |b| {
            b.iter(|| {
                let db = Arc::new(Database::builder().in_memory().open_temp().unwrap());

                let handles: Vec<_> = (0..threads)
                    .map(|_| {
                        let db = Arc::clone(&db);
                        std::thread::spawn(move || {
                            let kv = KVStore::new(db);
                            let run_id = RunId::new(); // Different run per thread
                            for i in 0..1000 {
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

criterion_group!(
    name = m4_benchmarks;
    config = Criterion::default().sample_size(50);
    targets =
        latency_benchmarks,
        throughput_benchmarks,
        snapshot_benchmarks,
        transaction_pool_benchmarks,
        read_path_benchmarks,
        facade_tax_benchmarks,
        contention_benchmarks
);

criterion_main!(m4_benchmarks);
