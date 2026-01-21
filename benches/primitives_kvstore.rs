//! KVStore Primitive Benchmarks
//!
//! Comprehensive benchmarks for the KVStore primitive covering:
//! - Basic operations (get, put, delete)
//! - Access patterns (hot_key, uniform, working_set, miss)
//! - Value size scaling
//! - Key count scaling
//! - Fast path vs transaction path
//!
//! ## Running
//!
//! ```bash
//! # Full KVStore benchmarks
//! cargo bench --bench primitives_kvstore
//!
//! # Specific categories
//! cargo bench --bench primitives_kvstore -- "kvstore/get"
//! cargo bench --bench primitives_kvstore -- "kvstore/put"
//! cargo bench --bench primitives_kvstore -- "value_size"
//! cargo bench --bench primitives_kvstore -- "key_scaling"
//! ```

mod bench_env;

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use strata_core::traits::Storage;
use strata_core::types::{Key, Namespace, RunId};
use strata_core::value::Value;
use strata_durability::wal::DurabilityMode;
use strata_engine::Database;
use strata_primitives::KVStore;
use strata_storage::UnifiedStore;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tempfile::TempDir;

// =============================================================================
// Constants and Configuration
// =============================================================================

/// Fixed seed for deterministic "random" key selection.
const BENCH_SEED: u64 = 0xDEADBEEF_CAFEBABE;

/// Value sizes for scaling benchmarks.
const VALUE_SIZES: &[usize] = &[64, 256, 1024, 4096, 65536];

// =============================================================================
// Helper Functions
// =============================================================================

/// Get durability mode from environment variable.
fn get_durability_mode() -> DurabilityMode {
    std::env::var("INMEM_DURABILITY_MODE")
        .ok()
        .and_then(|s| match s.to_lowercase().as_str() {
            "inmemory" | "in_memory" | "in-memory" => Some(DurabilityMode::InMemory),
            "batched" | "buffered" => Some(DurabilityMode::buffered_default()),
            "strict" => Some(DurabilityMode::Strict),
            _ => None,
        })
        .unwrap_or(DurabilityMode::Strict)
}

/// Get durability mode suffix for benchmark naming.
fn durability_suffix() -> &'static str {
    match get_durability_mode() {
        DurabilityMode::InMemory => "inmemory",
        DurabilityMode::Batched { .. } => "batched",
        DurabilityMode::Async { .. } => "async",
        DurabilityMode::Strict => "strict",
    }
}

/// Create a test database with the configured durability mode.
fn create_db() -> (Arc<Database>, TempDir) {
    let temp_dir = TempDir::new().unwrap();
    let mode = get_durability_mode();
    let db = Arc::new(
        Database::builder()
            .durability(mode)
            .path(temp_dir.path().join("db"))
            .open()
            .unwrap(),
    );
    (db, temp_dir)
}

/// Create a database with a specific durability mode.
fn create_db_with_mode(mode: DurabilityMode) -> (Arc<Database>, TempDir) {
    let temp_dir = TempDir::new().unwrap();
    let db = Arc::new(
        Database::builder()
            .durability(mode)
            .path(temp_dir.path().join("db"))
            .open()
            .unwrap(),
    );
    (db, temp_dir)
}

/// Create a standalone UnifiedStore for Tier A0 benchmarks.
fn create_store() -> UnifiedStore {
    UnifiedStore::new()
}

/// Create a test namespace with the given run ID.
fn test_namespace(run_id: RunId) -> Namespace {
    Namespace::new(
        "tenant".to_string(),
        "app".to_string(),
        "agent".to_string(),
        run_id,
    )
}

/// Create a Key for KV operations.
fn make_kv_key(ns: &Namespace, name: &str) -> Key {
    Key::new_kv(ns.clone(), name)
}

/// Simple LCG for deterministic "random" key selection.
#[inline]
fn lcg_next(state: &mut u64) -> u64 {
    *state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
    *state
}

/// Get a random index in range [0, max) using LCG.
#[inline]
fn lcg_index(state: &mut u64, max: usize) -> usize {
    (lcg_next(state) % max as u64) as usize
}

/// Pre-populate a store (Tier A0) with entries.
fn prepopulate_store(store: &UnifiedStore, ns: &Namespace, count: usize) -> Vec<Key> {
    let keys: Vec<Key> = (0..count)
        .map(|i| make_kv_key(ns, &format!("key_{}", i)))
        .collect();

    for (i, key) in keys.iter().enumerate() {
        store.put(key.clone(), Value::I64(i as i64), None).unwrap();
    }

    keys
}

// =============================================================================
// Tier A0: Core Storage Benchmarks (No Transaction Machinery)
// =============================================================================

fn tier_a0_get(c: &mut Criterion) {
    let mut group = c.benchmark_group("core_storage/get");
    group.throughput(Throughput::Elements(1));

    // Hot key - single key repeated access
    {
        let store = create_store();
        let run_id = RunId::new();
        let ns = test_namespace(run_id);
        let key = make_kv_key(&ns, "hot_key");
        store.put(key.clone(), Value::I64(42), None).unwrap();

        group.bench_function("hot_key", |b| {
            b.iter(|| {
                let result = store.get(black_box(&key));
                black_box(result.unwrap())
            });
        });
    }

    // Uniform - random keys from full keyspace
    {
        let store = create_store();
        let run_id = RunId::new();
        let ns = test_namespace(run_id);
        let keys = prepopulate_store(&store, &ns, 10_000);
        let mut rng_state = BENCH_SEED;

        group.bench_function("uniform", |b| {
            b.iter(|| {
                let idx = lcg_index(&mut rng_state, keys.len());
                let result = store.get(black_box(&keys[idx]));
                black_box(result.unwrap())
            });
        });
    }

    // Miss - key not found
    {
        let store = create_store();
        let run_id = RunId::new();
        let ns = test_namespace(run_id);
        let missing_key = make_kv_key(&ns, "nonexistent");

        group.bench_function("miss", |b| {
            b.iter(|| {
                let result = store.get(black_box(&missing_key));
                black_box(result)
            });
        });
    }

    // Working set - small hot subset
    for ws_size in &[100usize, 1000] {
        let store = create_store();
        let run_id = RunId::new();
        let ns = test_namespace(run_id);
        let keys = prepopulate_store(&store, &ns, 10_000);
        let mut rng_state = BENCH_SEED;

        group.bench_function(BenchmarkId::new("working_set", ws_size), |b| {
            b.iter(|| {
                let idx = lcg_index(&mut rng_state, *ws_size);
                let result = store.get(black_box(&keys[idx]));
                black_box(result.unwrap())
            });
        });
    }

    group.finish();
}

fn tier_a0_put(c: &mut Criterion) {
    let mut group = c.benchmark_group("core_storage/put");
    group.throughput(Throughput::Elements(1));

    // Insert - new unique keys
    {
        let store = create_store();
        let run_id = RunId::new();
        let ns = test_namespace(run_id);
        let counter = AtomicU64::new(0);

        group.bench_function("insert", |b| {
            b.iter(|| {
                let i = counter.fetch_add(1, Ordering::Relaxed);
                let key = make_kv_key(&ns, &format!("insert_{}", i));
                let result = store.put(key, Value::I64(i as i64), None);
                black_box(result.unwrap())
            });
        });
    }

    // Overwrite - same key updates
    {
        let store = create_store();
        let run_id = RunId::new();
        let ns = test_namespace(run_id);
        let key = make_kv_key(&ns, "hot_key");
        store.put(key.clone(), Value::I64(0), None).unwrap();
        let counter = AtomicU64::new(0);

        group.bench_function("overwrite_hot", |b| {
            b.iter(|| {
                let i = counter.fetch_add(1, Ordering::Relaxed);
                let result = store.put(key.clone(), Value::I64(i as i64), None);
                black_box(result.unwrap())
            });
        });
    }

    // Overwrite uniform - random key updates
    {
        let store = create_store();
        let run_id = RunId::new();
        let ns = test_namespace(run_id);
        let keys = prepopulate_store(&store, &ns, 10_000);
        let mut rng_state = BENCH_SEED;
        let counter = AtomicU64::new(0);

        group.bench_function("overwrite_uniform", |b| {
            b.iter(|| {
                let idx = lcg_index(&mut rng_state, keys.len());
                let i = counter.fetch_add(1, Ordering::Relaxed);
                let result = store.put(keys[idx].clone(), Value::I64(i as i64), None);
                black_box(result.unwrap())
            });
        });
    }

    group.finish();
}

// =============================================================================
// Tier B: KVStore Facade Benchmarks (Full Transaction Path)
// =============================================================================

fn kvstore_get(c: &mut Criterion) {
    let dur_suffix = durability_suffix();
    let mut group = c.benchmark_group(format!("kvstore/get/{}", dur_suffix));
    group.throughput(Throughput::Elements(1));

    // Hot key
    {
        let (db, _temp) = create_db();
        let run_id = RunId::new();
        let kv = KVStore::new(db.clone());

        kv.put(&run_id, "hot_key", Value::I64(42)).unwrap();

        group.bench_function("hot_key", |b| {
            b.iter(|| {
                let result = kv.get(&run_id, black_box("hot_key"));
                black_box(result.unwrap())
            });
        });
    }

    // Uniform access
    {
        let (db, _temp) = create_db();
        let run_id = RunId::new();
        let kv = KVStore::new(db.clone());

        for i in 0..1000 {
            kv.put(&run_id, &format!("key_{}", i), Value::I64(i as i64))
                .unwrap();
        }

        let mut rng_state = BENCH_SEED;

        group.bench_function("uniform", |b| {
            b.iter(|| {
                let idx = lcg_index(&mut rng_state, 1000);
                let result = kv.get(&run_id, black_box(&format!("key_{}", idx)));
                black_box(result.unwrap())
            });
        });
    }

    // Miss
    {
        let (db, _temp) = create_db();
        let run_id = RunId::new();
        let kv = KVStore::new(db.clone());

        group.bench_function("miss", |b| {
            b.iter(|| {
                let result = kv.get(&run_id, black_box("nonexistent"));
                black_box(result)
            });
        });
    }

    // Working set sizes
    for ws_size in &[100usize, 1000] {
        let (db, _temp) = create_db();
        let run_id = RunId::new();
        let kv = KVStore::new(db.clone());

        for i in 0..10_000 {
            kv.put(&run_id, &format!("key_{}", i), Value::I64(i as i64))
                .unwrap();
        }

        let mut rng_state = BENCH_SEED;

        group.bench_function(BenchmarkId::new("working_set", ws_size), |b| {
            b.iter(|| {
                let idx = lcg_index(&mut rng_state, *ws_size);
                let result = kv.get(&run_id, black_box(&format!("key_{}", idx)));
                black_box(result.unwrap())
            });
        });
    }

    group.finish();
}

fn kvstore_put(c: &mut Criterion) {
    let dur_suffix = durability_suffix();
    let mut group = c.benchmark_group(format!("kvstore/put/{}", dur_suffix));
    group.throughput(Throughput::Elements(1));

    // Insert - new unique keys
    {
        let (db, _temp) = create_db();
        let run_id = RunId::new();
        let kv = KVStore::new(db.clone());
        let counter = AtomicU64::new(0);

        group.bench_function("insert", |b| {
            b.iter(|| {
                let i = counter.fetch_add(1, Ordering::Relaxed);
                let result = kv.put(&run_id, &format!("insert_{}", i), Value::I64(i as i64));
                black_box(result.unwrap())
            });
        });
    }

    // Overwrite hot - same key updates
    {
        let (db, _temp) = create_db();
        let run_id = RunId::new();
        let kv = KVStore::new(db.clone());
        kv.put(&run_id, "hot_key", Value::I64(0)).unwrap();
        let counter = AtomicU64::new(0);

        group.bench_function("overwrite_hot", |b| {
            b.iter(|| {
                let i = counter.fetch_add(1, Ordering::Relaxed);
                let result = kv.put(&run_id, "hot_key", Value::I64(i as i64));
                black_box(result.unwrap())
            });
        });
    }

    // Overwrite uniform - random key updates
    {
        let (db, _temp) = create_db();
        let run_id = RunId::new();
        let kv = KVStore::new(db.clone());

        for i in 0..1000 {
            kv.put(&run_id, &format!("key_{}", i), Value::I64(i as i64))
                .unwrap();
        }

        let mut rng_state = BENCH_SEED;
        let counter = AtomicU64::new(0);

        group.bench_function("overwrite_uniform", |b| {
            b.iter(|| {
                let idx = lcg_index(&mut rng_state, 1000);
                let i = counter.fetch_add(1, Ordering::Relaxed);
                let result = kv.put(&run_id, &format!("key_{}", idx), Value::I64(i as i64));
                black_box(result.unwrap())
            });
        });
    }

    group.finish();
}

fn kvstore_delete(c: &mut Criterion) {
    let dur_suffix = durability_suffix();
    let mut group = c.benchmark_group(format!("kvstore/delete/{}", dur_suffix));
    group.throughput(Throughput::Elements(1));

    // Delete existing keys
    {
        let (db, _temp) = create_db();
        let run_id = RunId::new();
        let kv = KVStore::new(db.clone());
        let counter = AtomicU64::new(0);

        // Pre-populate
        for i in 0..100_000 {
            kv.put(&run_id, &format!("del_{}", i), Value::I64(i as i64))
                .unwrap();
        }

        group.bench_function("existing", |b| {
            b.iter(|| {
                let i = counter.fetch_add(1, Ordering::Relaxed);
                let result = kv.delete(&run_id, &format!("del_{}", i));
                black_box(result.unwrap())
            });
        });
    }

    // Delete nonexistent keys
    {
        let (db, _temp) = create_db();
        let run_id = RunId::new();
        let kv = KVStore::new(db.clone());

        group.bench_function("nonexistent", |b| {
            b.iter(|| {
                let result = kv.delete(&run_id, black_box("nonexistent"));
                black_box(result)
            });
        });
    }

    group.finish();
}

// =============================================================================
// Value Size Scaling Benchmarks
// =============================================================================

fn kvstore_value_size(c: &mut Criterion) {
    let dur_suffix = durability_suffix();
    let mut group = c.benchmark_group(format!("kvstore/value_size/{}", dur_suffix));
    group.throughput(Throughput::Elements(1));

    for size in VALUE_SIZES {
        let (db, _temp) = create_db();
        let run_id = RunId::new();
        let kv = KVStore::new(db.clone());

        let value = Value::Bytes(vec![0x42u8; *size]);
        let counter = AtomicU64::new(0);

        group.bench_function(BenchmarkId::new("put", format!("{}B", size)), |b| {
            b.iter(|| {
                let i = counter.fetch_add(1, Ordering::Relaxed);
                let result = kv.put(&run_id, &format!("key_{}", i), value.clone());
                black_box(result.unwrap())
            });
        });
    }

    // GET with different value sizes
    for size in VALUE_SIZES {
        let (db, _temp) = create_db();
        let run_id = RunId::new();
        let kv = KVStore::new(db.clone());

        let value = Value::Bytes(vec![0x42u8; *size]);
        kv.put(&run_id, "sized_key", value).unwrap();

        group.bench_function(BenchmarkId::new("get", format!("{}B", size)), |b| {
            b.iter(|| {
                let result = kv.get(&run_id, black_box("sized_key"));
                black_box(result.unwrap())
            });
        });
    }

    group.finish();
}

// =============================================================================
// Key Count Scaling Benchmarks
// =============================================================================

fn kvstore_key_scaling(c: &mut Criterion) {
    let mut group = c.benchmark_group("kvstore/key_scaling");
    group.throughput(Throughput::Elements(1));

    // Only test with InMemory mode to avoid slow disk I/O
    for key_count in &[10_000usize, 100_000] {
        let (db, _temp) = create_db_with_mode(DurabilityMode::InMemory);
        let run_id = RunId::new();
        let kv = KVStore::new(db.clone());

        // Pre-populate
        for i in 0..*key_count {
            kv.put(&run_id, &format!("key_{}", i), Value::I64(i as i64))
                .unwrap();
        }

        let mut rng_state = BENCH_SEED;

        group.bench_function(BenchmarkId::new("get_rotating", key_count), |b| {
            b.iter(|| {
                let idx = lcg_index(&mut rng_state, *key_count);
                let result = kv.get(&run_id, black_box(&format!("key_{}", idx)));
                black_box(result.unwrap())
            });
        });
    }

    group.finish();
}

// =============================================================================
// Fast Path vs Transaction Path
// =============================================================================

fn kvstore_fast_path(c: &mut Criterion) {
    let mut group = c.benchmark_group("kvstore/fast_path");
    group.throughput(Throughput::Elements(1));

    // Fast path get (outside transaction)
    {
        let (db, _temp) = create_db_with_mode(DurabilityMode::InMemory);
        let run_id = RunId::new();
        let kv = KVStore::new(db.clone());

        kv.put(&run_id, "key", Value::I64(42)).unwrap();

        group.bench_function("get", |b| {
            b.iter(|| {
                let result = kv.get(&run_id, black_box("key"));
                black_box(result.unwrap())
            });
        });
    }

    // Transaction path get (using get_in_transaction)
    {
        let (db, _temp) = create_db_with_mode(DurabilityMode::InMemory);
        let run_id = RunId::new();
        let kv = KVStore::new(db.clone());

        kv.put(&run_id, "key", Value::I64(42)).unwrap();

        group.bench_function("get_in_transaction", |b| {
            b.iter(|| {
                let result = kv.get_in_transaction(&run_id, black_box("key"));
                black_box(result.unwrap())
            });
        });
    }

    group.finish();
}

// =============================================================================
// Durability Mode Comparison
// =============================================================================

fn kvstore_durability_comparison(c: &mut Criterion) {
    let mut group = c.benchmark_group("kvstore/durability_comparison");
    group.throughput(Throughput::Elements(1));

    let modes = [
        ("inmemory", DurabilityMode::InMemory),
        ("buffered", DurabilityMode::buffered_default()),
        ("strict", DurabilityMode::Strict),
    ];

    for (name, mode) in modes {
        let (db, _temp) = create_db_with_mode(mode);
        let run_id = RunId::new();
        let kv = KVStore::new(db.clone());
        let counter = AtomicU64::new(0);

        group.bench_function(BenchmarkId::new("put", name), |b| {
            b.iter(|| {
                let i = counter.fetch_add(1, Ordering::Relaxed);
                let result = kv.put(&run_id, &format!("key_{}", i), Value::I64(i as i64));
                black_box(result.unwrap())
            });
        });
    }

    for (name, mode) in modes {
        let (db, _temp) = create_db_with_mode(mode);
        let run_id = RunId::new();
        let kv = KVStore::new(db.clone());

        kv.put(&run_id, "key", Value::I64(42)).unwrap();

        group.bench_function(BenchmarkId::new("get", name), |b| {
            b.iter(|| {
                let result = kv.get(&run_id, black_box("key"));
                black_box(result.unwrap())
            });
        });
    }

    group.finish();
}

// =============================================================================
// Benchmark Groups
// =============================================================================

criterion_group! {
    name = core_storage;
    config = Criterion::default();
    targets = tier_a0_get, tier_a0_put
}

criterion_group! {
    name = kvstore_operations;
    config = Criterion::default();
    targets = kvstore_get, kvstore_put, kvstore_delete
}

criterion_group! {
    name = kvstore_scaling;
    config = Criterion::default();
    targets = kvstore_value_size, kvstore_key_scaling
}

criterion_group! {
    name = kvstore_paths;
    config = Criterion::default();
    targets = kvstore_fast_path, kvstore_durability_comparison
}

criterion_main!(core_storage, kvstore_operations, kvstore_scaling, kvstore_paths);
