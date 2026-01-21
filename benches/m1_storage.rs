//! M1 Storage Benchmarks - Semantic Regression Harness
//!
//! ## Benchmark Path Types (Layer Labels)
//!
//! - `engine_*`: End-to-end API path through Database (includes WAL, locks, full path)
//! - `wal_*`: Recovery and WAL operations (no engine runtime path)
//!
//! The prefix indicates the **primary semantic being exercised**, not just which
//! module owns the code path.
//!
//! ## Durability Modes
//!
//! All write benchmarks explicitly label their durability mode:
//! - `dur_strict`: fsync on every write (current M1 default)
//!
//! This prevents baseline comparisons from being invalidated by durability changes.
//!
//! ## Key Access Patterns
//!
//! - `hot_key`: Single key, repeated access (best case, cache-friendly)
//! - `uniform`: Random keys from full keyspace (realistic agent pattern)
//! - `working_set_N`: Small subset of N keys (80/20 skewed access)
//! - `miss`: Key not found (error/existence check path)
//!
//! ## Deterministic Randomness
//!
//! All "random" access patterns use a fixed seed (BENCH_SEED) for reproducibility.
//! This ensures baseline comparisons are not affected by run-to-run variance.
//!
//! ## What These Benchmarks Prove
//!
//! | Benchmark | Semantic Guarantee | Regression Detection |
//! |-----------|-------------------|----------------------|
//! | engine_get/* | Returns latest committed version for a key | BTreeMap/lock overhead |
//! | engine_put/* | Write persisted to WAL before returning | fsync/serialization cost |
//! | engine_delete/* | Delete makes key unreadable (tombstone) | Delete vs insert parity |
//! | wal_recovery/* | WAL replay reconstructs same final state | Replay scaling |
//! | engine_key_scaling/* | O(log n) lookup guarantee holds | BTreeMap degradation |
//!
//! ## Running
//!
//! ```bash
//! cargo bench --bench m1_storage
//! cargo bench --bench m1_storage -- "engine_get"  # specific group
//! ```

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use strata_core::types::{Key, Namespace, RunId};
use strata_core::value::Value;
use strata_engine::Database;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};
use tempfile::TempDir;

// =============================================================================
// Constants and Configuration
// =============================================================================

/// Fixed seed for deterministic "random" key selection.
/// Change this seed and baselines become invalid - that's intentional.
const BENCH_SEED: u64 = 0xDEADBEEF_CAFEBABE;

// =============================================================================
// Test Utilities - All allocation happens here, outside timed loops
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

/// Pre-generate keys to avoid allocation in timed loops
fn pregenerate_keys(ns: &Namespace, prefix: &str, count: usize) -> Vec<Key> {
    (0..count)
        .map(|i| make_key(ns, &format!("{}_{:06}", prefix, i)))
        .collect()
}

/// Simple LCG for deterministic "random" key selection without allocation.
/// Uses fixed multiplier from Knuth's MMIX.
fn lcg_next(state: &mut u64) -> u64 {
    *state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
    *state
}

// =============================================================================
// Engine Layer: KV Get Benchmarks
// =============================================================================
// Semantic: Returns the latest committed version for a key
// Regression: Lock contention changes, map implementation changes
// Agent pattern: Frequent state lookups during execution

fn engine_get_benchmarks(c: &mut Criterion) {
    let mut group = c.benchmark_group("engine_get");
    group.throughput(Throughput::Elements(1));

    // --- Setup (outside all timed loops) ---
    let temp_dir = TempDir::new().unwrap();
    let db = Database::open(temp_dir.path().join("db")).unwrap();
    let run_id = RunId::new();
    let ns = create_namespace(run_id);

    const NUM_KEYS: usize = 10_000;
    let keys = pregenerate_keys(&ns, "key", NUM_KEYS);

    // Populate database
    for (i, key) in keys.iter().enumerate() {
        db.put(run_id, key.clone(), Value::I64(i as i64)).unwrap();
    }

    let hot_key = keys[NUM_KEYS / 2].clone();
    let miss_key = make_key(&ns, "nonexistent_key");
    let working_set: Vec<_> = keys[0..100].to_vec(); // 1% of keys

    // --- Benchmark: hot_key (single key, best case) ---
    // Semantic: Read returns latest committed value for existing key
    // Real pattern: Agent reading same config key repeatedly
    group.bench_function("hot_key", |b| {
        b.iter(|| black_box(db.get(&hot_key).unwrap()));
    });

    // --- Benchmark: miss (key not found) ---
    // Semantic: Read returns None for non-existent key
    // Regression: Miss path should not be slower than hit path
    group.bench_function("miss", |b| {
        b.iter(|| black_box(db.get(&miss_key).unwrap()));
    });

    // --- Benchmark: uniform (random from full keyspace) ---
    // Semantic: Read returns correct value regardless of access pattern
    // Real pattern: Agent accessing arbitrary state keys
    // Note: Uses BENCH_SEED for reproducibility
    group.bench_function("uniform", |b| {
        let mut rng_state = BENCH_SEED;
        b.iter(|| {
            let idx = (lcg_next(&mut rng_state) as usize) % NUM_KEYS;
            black_box(db.get(&keys[idx]).unwrap())
        });
    });

    // --- Benchmark: working_set (small hot subset) ---
    // Semantic: Read returns correct value from frequently accessed subset
    // Real pattern: Agent with frequently accessed state subset
    group.bench_function("working_set_100", |b| {
        let mut rng_state = BENCH_SEED ^ 0x12345;
        b.iter(|| {
            let idx = (lcg_next(&mut rng_state) as usize) % working_set.len();
            black_box(db.get(&working_set[idx]).unwrap())
        });
    });

    group.finish();
}

// =============================================================================
// Engine Layer: KV Put Benchmarks (includes WAL)
// =============================================================================
// Semantic: Write persisted to WAL before function returns
// Note: WAL is always enabled in Database API. Cannot isolate storage-only cost.
// Regression: fsync overhead, serialization changes, lock contention
//
// DURABILITY: All benchmarks use dur_strict (fsync per write) which is M1 default.

fn engine_put_benchmarks(c: &mut Criterion) {
    let mut group = c.benchmark_group("engine_put");
    group.throughput(Throughput::Elements(1));

    // --- Benchmark: insert/dur_strict/uniform (unique keys, append pattern) ---
    // Semantic: New key is persisted and readable after put returns
    // Real pattern: Agent creating new state entries
    {
        let temp_dir = TempDir::new().unwrap();
        let db = Database::open(temp_dir.path().join("db")).unwrap();
        let run_id = RunId::new();
        let ns = create_namespace(run_id);

        let counter = AtomicU64::new(0);

        group.bench_function("insert/dur_strict/uniform", |b| {
            b.iter(|| {
                let i = counter.fetch_add(1, Ordering::Relaxed);
                // Generate key in real-time - includes key creation cost which is realistic
                let key = make_key(&ns, &format!("insert_{}", i));
                black_box(db.put(run_id, key, Value::I64(i as i64)).unwrap())
            });
        });
    }

    // --- Benchmark: overwrite/dur_strict/hot_key (same key, update pattern) ---
    // Semantic: Update to existing key increments version, old value replaced
    // Real pattern: Agent updating counter or status
    {
        let temp_dir = TempDir::new().unwrap();
        let db = Database::open(temp_dir.path().join("db")).unwrap();
        let run_id = RunId::new();
        let ns = create_namespace(run_id);
        let key = make_key(&ns, "hot_overwrite");
        db.put(run_id, key.clone(), Value::I64(0)).unwrap();

        let counter = AtomicU64::new(0);

        group.bench_function("overwrite/dur_strict/hot_key", |b| {
            b.iter(|| {
                let i = counter.fetch_add(1, Ordering::Relaxed);
                black_box(db.put(run_id, key.clone(), Value::I64(i as i64)).unwrap())
            });
        });
    }

    // --- Benchmark: overwrite/dur_strict/uniform (random key updates) ---
    // Semantic: Updates to random existing keys are all persisted
    // Real pattern: Agent updating various state entries
    {
        let temp_dir = TempDir::new().unwrap();
        let db = Database::open(temp_dir.path().join("db")).unwrap();
        let run_id = RunId::new();
        let ns = create_namespace(run_id);

        const NUM_KEYS: usize = 1_000;
        let keys = pregenerate_keys(&ns, "uniform", NUM_KEYS);

        // Pre-populate
        for (i, key) in keys.iter().enumerate() {
            db.put(run_id, key.clone(), Value::I64(i as i64)).unwrap();
        }

        let counter = AtomicU64::new(0);

        group.bench_function("overwrite/dur_strict/uniform", |b| {
            let mut rng_state = BENCH_SEED ^ 0x11111;
            b.iter(|| {
                let idx = (lcg_next(&mut rng_state) as usize) % NUM_KEYS;
                let i = counter.fetch_add(1, Ordering::Relaxed);
                black_box(
                    db.put(run_id, keys[idx].clone(), Value::I64(i as i64))
                        .unwrap(),
                )
            });
        });
    }

    group.finish();
}

// =============================================================================
// Engine Layer: Delete Benchmarks
// =============================================================================
// Semantic: Delete creates tombstone, key becomes unreadable
// Regression: Delete should have similar cost to insert (both WAL writes)

fn engine_delete_benchmarks(c: &mut Criterion) {
    let mut group = c.benchmark_group("engine_delete");
    group.throughput(Throughput::Elements(1));

    // --- Benchmark: existing/dur_strict (tombstone creation) ---
    // Semantic: After delete, get returns None for that key
    // Real pattern: Agent cleanup of temporary state
    group.bench_function("existing/dur_strict", |b| {
        let counter = AtomicU64::new(0);

        b.iter_custom(|iters| {
            // Setup for this iteration batch (outside timing)
            let temp_dir = TempDir::new().unwrap();
            let db = Database::open(temp_dir.path().join("db")).unwrap();
            let run_id = RunId::new();
            let ns = create_namespace(run_id);

            let start_idx = counter.fetch_add(iters, Ordering::Relaxed);
            let keys: Vec<_> = (0..iters)
                .map(|i| make_key(&ns, &format!("del_{}", start_idx + i)))
                .collect();

            // Create keys to delete
            for (i, key) in keys.iter().enumerate() {
                db.put(run_id, key.clone(), Value::I64(i as i64)).unwrap();
            }

            // Timed: delete only
            let start = Instant::now();
            for key in &keys {
                db.delete(run_id, key.clone()).unwrap();
            }
            start.elapsed()
        });
    });

    // --- Benchmark: nonexistent (no-op path) ---
    // Semantic: Delete of missing key is a no-op (idempotent)
    {
        let temp_dir = TempDir::new().unwrap();
        let db = Database::open(temp_dir.path().join("db")).unwrap();
        let run_id = RunId::new();
        let ns = create_namespace(run_id);
        let key = make_key(&ns, "never_existed");

        group.bench_function("nonexistent", |b| {
            b.iter(|| black_box(db.delete(run_id, key.clone())));
        });
    }

    group.finish();
}

// =============================================================================
// Value Size Scaling
// =============================================================================
// Semantic: Write persisted regardless of value size
// Regression: Large values should not cause disproportionate slowdown

fn value_size_benchmarks(c: &mut Criterion) {
    let mut group = c.benchmark_group("engine_value_size");

    for value_size in [64, 256, 1024, 4096, 65536] {
        let temp_dir = TempDir::new().unwrap();
        let db = Database::open(temp_dir.path().join("db")).unwrap();
        let run_id = RunId::new();
        let ns = create_namespace(run_id);

        // Pre-allocate value (outside timed loop)
        let value_data = vec![0xABu8; value_size];
        let value = Value::Bytes(value_data);

        let counter = AtomicU64::new(0);

        group.throughput(Throughput::Bytes(value_size as u64));
        group.bench_with_input(
            BenchmarkId::new("put_bytes/dur_strict", value_size),
            &value_size,
            |b, _| {
                b.iter(|| {
                    let i = counter.fetch_add(1, Ordering::Relaxed);
                    // Generate key in real-time
                    let key = make_key(&ns, &format!("size_{}_{}", value_size, i));
                    black_box(db.put(run_id, key, value.clone()).unwrap())
                });
            },
        );
    }

    group.finish();
}

// =============================================================================
// Key Count Scaling (Cache Boundary Tests)
// =============================================================================
// Semantic: O(log n) BTreeMap lookup must hold as key count grows
// Regression: Lookup time should grow logarithmically, not linearly
//
// Scales chosen to cross typical cache boundaries:
// - 10K keys: ~fits in L2/L3
// - 100K keys: exceeds L2, may fit L3
// - 1M keys: exceeds L3 on most systems

fn key_scaling_benchmarks(c: &mut Criterion) {
    let mut group = c.benchmark_group("engine_key_scaling");
    group.sample_size(20);
    group.throughput(Throughput::Elements(1));

    for num_keys in [10_000, 100_000, 1_000_000] {
        let temp_dir = TempDir::new().unwrap();
        let db = Database::open(temp_dir.path().join("db")).unwrap();
        let run_id = RunId::new();
        let ns = create_namespace(run_id);

        // Populate (outside timing)
        let keys = pregenerate_keys(&ns, "scale", num_keys);
        for (i, key) in keys.iter().enumerate() {
            db.put(run_id, key.clone(), Value::I64(i as i64)).unwrap();
        }

        // Rotating sequence of reads - NOT hot key
        // This ensures we measure memory hierarchy effects
        group.bench_with_input(
            BenchmarkId::new("get_rotating", num_keys),
            &num_keys,
            |b, &num_keys| {
                let mut rng_state = BENCH_SEED ^ (num_keys as u64);
                b.iter(|| {
                    let idx = (lcg_next(&mut rng_state) as usize) % num_keys;
                    black_box(db.get(&keys[idx]).unwrap())
                });
            },
        );
    }

    group.finish();
}

// =============================================================================
// WAL Recovery Benchmarks
// =============================================================================
// Semantic: WAL replay reconstructs the same final state as before crash
// Regression: Recovery time should scale linearly with WAL size

fn wal_recovery_benchmarks(c: &mut Criterion) {
    let mut group = c.benchmark_group("wal_recovery");
    group.sample_size(10);

    // --- Recovery: insert-only workload ---
    // Semantic: All inserted keys readable after recovery
    for num_ops in [1_000, 10_000, 50_000] {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("db");

        // Setup: create WAL with insert-only ops
        {
            let db = Database::open(&db_path).unwrap();
            let run_id = RunId::new();
            let ns = create_namespace(run_id);
            let keys = pregenerate_keys(&ns, "insert", num_ops);

            for (i, key) in keys.iter().enumerate() {
                db.put(run_id, key.clone(), Value::I64(i as i64)).unwrap();
            }
            db.flush().unwrap();
        }

        group.throughput(Throughput::Elements(num_ops as u64));
        group.bench_with_input(
            BenchmarkId::new("insert_only", num_ops),
            &num_ops,
            |b, _| {
                b.iter(|| black_box(Database::open(&db_path).unwrap()));
            },
        );
    }

    // --- Recovery: overwrite-heavy workload ---
    // Semantic: Only latest version of each key visible after recovery
    {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("db");
        const NUM_KEYS: usize = 100;
        const VERSIONS_PER_KEY: usize = 100;

        {
            let db = Database::open(&db_path).unwrap();
            let run_id = RunId::new();
            let ns = create_namespace(run_id);
            let keys = pregenerate_keys(&ns, "overwrite", NUM_KEYS);

            for v in 0..VERSIONS_PER_KEY {
                for (i, key) in keys.iter().enumerate() {
                    db.put(run_id, key.clone(), Value::I64((v * NUM_KEYS + i) as i64))
                        .unwrap();
                }
            }
            db.flush().unwrap();
        }

        let total_ops = NUM_KEYS * VERSIONS_PER_KEY;
        group.throughput(Throughput::Elements(total_ops as u64));
        group.bench_function("overwrite_heavy", |b| {
            b.iter(|| black_box(Database::open(&db_path).unwrap()));
        });
    }

    // --- Recovery: delete-heavy workload ---
    // Semantic: Deleted keys return None after recovery
    {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("db");
        const NUM_KEYS: usize = 10_000;

        {
            let db = Database::open(&db_path).unwrap();
            let run_id = RunId::new();
            let ns = create_namespace(run_id);
            let keys = pregenerate_keys(&ns, "deletes", NUM_KEYS);

            // Insert all
            for (i, key) in keys.iter().enumerate() {
                db.put(run_id, key.clone(), Value::I64(i as i64)).unwrap();
            }
            // Delete 80%
            for key in keys.iter().take(NUM_KEYS * 8 / 10) {
                db.delete(run_id, key.clone()).unwrap();
            }
            db.flush().unwrap();
        }

        let total_ops = NUM_KEYS + (NUM_KEYS * 8 / 10);
        group.throughput(Throughput::Elements(total_ops as u64));
        group.bench_function("delete_heavy", |b| {
            b.iter(|| black_box(Database::open(&db_path).unwrap()));
        });
    }

    group.finish();
}

// =============================================================================
// Benchmark Groups
// =============================================================================

criterion_group!(
    name = engine_kv;
    config = Criterion::default().measurement_time(Duration::from_secs(10));
    targets = engine_get_benchmarks, engine_put_benchmarks, engine_delete_benchmarks
);

criterion_group!(
    name = engine_scaling;
    config = Criterion::default()
        .measurement_time(Duration::from_secs(10))
        .sample_size(20);
    targets = value_size_benchmarks, key_scaling_benchmarks
);

criterion_group!(
    name = wal;
    config = Criterion::default()
        .measurement_time(Duration::from_secs(15))
        .sample_size(10);
    targets = wal_recovery_benchmarks
);

criterion_main!(engine_kv, engine_scaling, wal);
