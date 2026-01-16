//! Facade Tax Benchmarks
//!
//! Measures overhead at each layer:
//! - A0: Core data structure (FxHashMap baseline)
//! - A1: Engine layer (storage.put direct)
//! - B:  Facade layer (KVStore.put with transaction)
//!
//! Run with: cargo bench --bench m4_facade_tax
//!
//! Expected ratios (M4 targets):
//! - A1/A0 < 10× (storage overhead vs hashmap)
//! - B/A1 < 5× (KVStore overhead vs storage)
//! - B/A0 < 30× (total facade tax)

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use in_mem_core::types::{Key, Namespace, RunId, TypeTag};
use in_mem_core::value::Value;
use in_mem_core::Storage;
use in_mem_engine::Database;
use in_mem_primitives::KVStore;
use rustc_hash::FxHashMap;
use std::sync::Arc;
use std::time::Duration;

/// A0: Raw HashMap baseline - the theoretical minimum
fn bench_a0_hashmap(c: &mut Criterion) {
    let mut group = c.benchmark_group("facade_tax/A0");
    group.measurement_time(Duration::from_secs(5));

    let mut map: FxHashMap<String, i64> = FxHashMap::default();

    group.bench_function("hashmap_insert", |b| {
        let mut i = 0;
        b.iter(|| {
            i += 1;
            map.insert(format!("key{}", i), i as i64);
        });
    });

    // Pre-populate for reads
    for i in 0..10000 {
        map.insert(format!("read_key{}", i), i as i64);
    }

    group.bench_function("hashmap_get", |b| {
        let mut i = 0;
        b.iter(|| {
            i = (i + 1) % 10000;
            map.get(&format!("read_key{}", i))
        });
    });

    group.finish();
}

/// A1: Engine storage layer - direct storage access without transactions
fn bench_a1_storage(c: &mut Criterion) {
    let mut group = c.benchmark_group("facade_tax/A1");
    group.measurement_time(Duration::from_secs(5));

    let db = Arc::new(Database::builder().in_memory().open_temp().unwrap());
    let run_id = RunId::new();
    let ns = Namespace::for_run(run_id);

    // Warmup
    for i in 0..100 {
        let key = Key::new(ns.clone(), TypeTag::KV, format!("warmup{}", i).into_bytes());
        let _ = db.storage().put(key, Value::I64(i as i64), None);
    }

    group.bench_function("storage_put", |b| {
        let mut i = 0;
        b.iter(|| {
            i += 1;
            let key = Key::new(ns.clone(), TypeTag::KV, format!("key{}", i).into_bytes());
            let _ = db.storage().put(key, Value::I64(i as i64), None);
        });
    });

    // Pre-populate for reads
    for i in 0..10000 {
        let key = Key::new(
            ns.clone(),
            TypeTag::KV,
            format!("read_key{}", i).into_bytes(),
        );
        let _ = db.storage().put(key, Value::I64(i as i64), None);
    }

    group.bench_function("storage_get", |b| {
        let mut i = 0;
        b.iter(|| {
            i = (i + 1) % 10000;
            let key = Key::new(
                ns.clone(),
                TypeTag::KV,
                format!("read_key{}", i).into_bytes(),
            );
            let _ = db.storage().get(&key);
        });
    });

    group.finish();
}

/// B: Facade layer (KVStore) - full primitive API with transactions
fn bench_b_kvstore(c: &mut Criterion) {
    let mut group = c.benchmark_group("facade_tax/B");
    group.measurement_time(Duration::from_secs(5));

    let db = Arc::new(Database::builder().in_memory().open_temp().unwrap());
    let kv = KVStore::new(db.clone());
    let run_id = RunId::new();

    // Warmup
    for i in 0..100 {
        kv.put(&run_id, &format!("warmup{}", i), Value::I64(i as i64))
            .unwrap();
    }

    group.bench_function("kvstore_put", |b| {
        let mut i = 0;
        b.iter(|| {
            i += 1;
            kv.put(&run_id, &format!("key{}", i), Value::I64(i as i64))
                .unwrap();
        });
    });

    // Pre-populate for reads
    for i in 0..10000 {
        kv.put(&run_id, &format!("read_key{}", i), Value::I64(i as i64))
            .unwrap();
    }

    group.bench_function("kvstore_get", |b| {
        let mut i = 0;
        b.iter(|| {
            i = (i + 1) % 10000;
            kv.get(&run_id, &format!("read_key{}", i)).unwrap()
        });
    });

    // Fast path vs transaction path
    group.bench_function("kvstore_get_fast_path", |b| {
        b.iter(|| kv.get(&run_id, "read_key5000").unwrap());
    });

    group.bench_function("kvstore_get_in_transaction", |b| {
        b.iter(|| kv.get_in_transaction(&run_id, "read_key5000").unwrap());
    });

    group.finish();
}

/// Direct comparison at each layer
fn bench_layer_comparison(c: &mut Criterion) {
    let mut group = c.benchmark_group("facade_tax/comparison");
    group.measurement_time(Duration::from_secs(10));

    // A0: HashMap
    let mut map: FxHashMap<String, i64> = FxHashMap::default();
    group.bench_function(BenchmarkId::new("put", "A0_hashmap"), |b| {
        let mut i = 0;
        b.iter(|| {
            i += 1;
            map.insert(format!("key{}", i), i as i64);
        });
    });

    // A1: Storage
    let db = Arc::new(Database::builder().in_memory().open_temp().unwrap());
    let run_id = RunId::new();
    let ns = Namespace::for_run(run_id);

    group.bench_function(BenchmarkId::new("put", "A1_storage"), |b| {
        let mut i = 0;
        b.iter(|| {
            i += 1;
            let key = Key::new(ns.clone(), TypeTag::KV, format!("key{}", i).into_bytes());
            let _ = db.storage().put(key, Value::I64(i as i64), None);
        });
    });

    // B: KVStore
    let kv = KVStore::new(db.clone());
    group.bench_function(BenchmarkId::new("put", "B_kvstore"), |b| {
        let mut i = 0;
        b.iter(|| {
            i += 1;
            kv.put(&run_id, &format!("kvkey{}", i), Value::I64(i as i64))
                .unwrap();
        });
    });

    group.finish();
}

/// Value size impact on facade tax
fn bench_value_size_impact(c: &mut Criterion) {
    let mut group = c.benchmark_group("facade_tax/value_size");
    group.measurement_time(Duration::from_secs(5));

    let db = Arc::new(Database::builder().in_memory().open_temp().unwrap());
    let kv = KVStore::new(db.clone());
    let run_id = RunId::new();

    for size in [16, 256, 1024, 4096] {
        let value = Value::Bytes(vec![0u8; size]);

        group.bench_function(BenchmarkId::new("kvstore_put", format!("{}B", size)), |b| {
            let mut i = 0;
            b.iter(|| {
                i += 1;
                kv.put(&run_id, &format!("sized_key{}", i), value.clone())
                    .unwrap();
            });
        });
    }

    group.finish();
}

criterion_group!(
    name = facade_tax;
    config = Criterion::default().sample_size(100);
    targets =
        bench_a0_hashmap,
        bench_a1_storage,
        bench_b_kvstore,
        bench_layer_comparison,
        bench_value_size_impact
);

criterion_main!(facade_tax);
