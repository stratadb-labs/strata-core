//! M8 Vector Benchmarks
//!
//! Run with: cargo bench --bench m8_vector
//!
//! These benchmarks test VectorStore performance across all operations:
//! - Collection management (create, delete, list)
//! - Vector operations (insert, get, delete, count)
//! - Search operations (various k values, filtering)
//! - Scaling tests (dimension, collection size)
//!
//! ## Benchmark Categories
//!
//! ### Tier B: VectorStore Facade Operations
//! - vector_create_collection: Collection creation across dimensions
//! - vector_insert: Single and batch inserts
//! - vector_get: Point lookups
//! - vector_delete: Vector deletion
//! - vector_search: K-NN search with various k values
//! - vector_count: Collection counting
//!
//! ### Tier C: Scaling Tests
//! - vector_dimension_scaling: Impact of dimension on operations
//! - vector_collection_scaling: Impact of collection size on search
//! - vector_metric_comparison: Distance metric performance
//!
//! ## Performance Targets
//!
//! - vector_create_collection: < 100µs
//! - vector_insert/single: < 50µs
//! - vector_get: < 20µs
//! - vector_search/k=10/n=1000: < 1ms
//! - vector_search/k=100/n=10000: < 10ms

use criterion::{
    black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput,
};
use in_mem_core::types::RunId;
use in_mem_engine::Database;
use in_mem_primitives::vector::{DistanceMetric, VectorConfig, VectorStore};
use std::sync::Arc;
use std::time::Duration;

// ============================================================================
// Constants and Utilities
// ============================================================================

/// Fixed seed for reproducible benchmarks
const BENCH_SEED: u64 = 0xDEADBEEF_CAFEBABE;

/// Simple LCG for deterministic pseudo-random number generation
fn lcg_next(state: &mut u64) -> u64 {
    *state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
    *state
}

/// Generate a deterministic random vector of given dimension
fn random_vector(dimension: usize, seed: u64) -> Vec<f32> {
    let mut state = seed;
    (0..dimension)
        .map(|_| {
            let bits = lcg_next(&mut state);
            // Map to [-1, 1] range
            (bits as f32 / u64::MAX as f32) * 2.0 - 1.0
        })
        .collect()
}

/// Generate normalized random vector (for cosine similarity)
fn random_normalized_vector(dimension: usize, seed: u64) -> Vec<f32> {
    let vec = random_vector(dimension, seed);
    let norm: f32 = vec.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm > 0.0 {
        vec.iter().map(|x| x / norm).collect()
    } else {
        vec
    }
}

/// Pre-generate keys for deterministic benchmarks
fn pregenerate_keys(count: usize) -> Vec<String> {
    (0..count).map(|i| format!("key_{:08}", i)).collect()
}

/// Get durability mode label from environment
fn durability_label() -> &'static str {
    match std::env::var("INMEM_DURABILITY_MODE").ok().as_deref() {
        Some("inmemory") => "dur_inmemory",
        Some("batched") => "dur_batched",
        _ => "dur_strict",
    }
}

/// Create a VectorStore backed by in-memory database
fn create_vector_store() -> (VectorStore, Arc<Database>) {
    let db = Database::builder().in_memory().open_temp().unwrap();
    let db = Arc::new(db);
    let store = VectorStore::new(Arc::clone(&db));
    (store, db)
}

/// Common dimensions to test
const DIMENSIONS: [usize; 4] = [128, 384, 768, 1536];

/// Common collection sizes
const COLLECTION_SIZES: [usize; 4] = [100, 1_000, 10_000, 50_000];

/// Common k values for search
const K_VALUES: [usize; 3] = [1, 10, 100];

// ============================================================================
// Tier B: vector_create_collection
// ============================================================================

fn vector_create_collection(c: &mut Criterion) {
    let mut group = c.benchmark_group("vector_create_collection");
    group.measurement_time(Duration::from_secs(5));
    let dur = durability_label();

    for dimension in &[128, 384, 768, 1536] {
        group.bench_with_input(
            BenchmarkId::new(format!("dim_{}/{}", dimension, dur), dimension),
            dimension,
            |b, &dimension| {
                b.iter_custom(|iters| {
                    let mut total = Duration::ZERO;
                    for i in 0..iters {
                        let (store, _db) = create_vector_store();
                        let run_id = RunId::new();
                        let config = VectorConfig {
                            dimension,
                            metric: DistanceMetric::Cosine,
                            storage_dtype: in_mem_primitives::vector::StorageDtype::F32,
                        };
                        let name = format!("collection_{}", i);

                        let start = std::time::Instant::now();
                        store
                            .create_collection(run_id, &name, config)
                            .expect("create collection");
                        total += start.elapsed();
                    }
                    total
                });
            },
        );
    }

    group.finish();
}

// ============================================================================
// Tier B: vector_insert - Single Vector Insertion
// ============================================================================

fn vector_insert_single(c: &mut Criterion) {
    let mut group = c.benchmark_group("vector_insert");
    group.measurement_time(Duration::from_secs(5));
    let dur = durability_label();

    for dimension in &DIMENSIONS {
        group.throughput(Throughput::Elements(1));
        group.bench_with_input(
            BenchmarkId::new(format!("single/dim_{}/{}", dimension, dur), dimension),
            dimension,
            |b, &dimension| {
                let (store, _db) = create_vector_store();
                let run_id = RunId::new();
                let config = VectorConfig {
                    dimension,
                    metric: DistanceMetric::Cosine,
                    storage_dtype: in_mem_primitives::vector::StorageDtype::F32,
                };
                store
                    .create_collection(run_id, "bench", config)
                    .expect("create collection");

                let mut seed = BENCH_SEED;
                let mut key_counter = 0u64;

                b.iter(|| {
                    let key = format!("key_{}", key_counter);
                    key_counter += 1;
                    let vector = random_normalized_vector(dimension, seed);
                    seed = lcg_next(&mut seed);

                    store
                        .insert(run_id, "bench", &key, &vector, None)
                        .expect("insert");
                });
            },
        );
    }

    group.finish();
}

/// Batch insert benchmarks with different batch sizes
fn vector_insert_batch(c: &mut Criterion) {
    let mut group = c.benchmark_group("vector_insert");
    group.measurement_time(Duration::from_secs(10));
    let dur = durability_label();
    let dimension = 384; // MiniLM dimension

    for batch_size in &[10, 100, 1000] {
        group.throughput(Throughput::Elements(*batch_size as u64));
        group.bench_with_input(
            BenchmarkId::new(format!("batch_{}/{}", batch_size, dur), batch_size),
            batch_size,
            |b, &batch_size| {
                b.iter_custom(|iters| {
                    let mut total = Duration::ZERO;

                    for iter in 0..iters {
                        let (store, _db) = create_vector_store();
                        let run_id = RunId::new();
                        let config = VectorConfig {
                            dimension,
                            metric: DistanceMetric::Cosine,
                            storage_dtype: in_mem_primitives::vector::StorageDtype::F32,
                        };
                        store
                            .create_collection(run_id, "bench", config)
                            .expect("create collection");

                        // Pre-generate vectors
                        let vectors: Vec<_> = (0..batch_size)
                            .map(|i| {
                                let seed = BENCH_SEED.wrapping_add(iter * 10000 + i as u64);
                                random_normalized_vector(dimension, seed)
                            })
                            .collect();
                        let keys: Vec<_> = (0..batch_size)
                            .map(|i| format!("key_{}_{}", iter, i))
                            .collect();

                        let start = std::time::Instant::now();
                        for (key, vec) in keys.iter().zip(vectors.iter()) {
                            store
                                .insert(run_id, "bench", key, vec, None)
                                .expect("insert");
                        }
                        total += start.elapsed();
                    }

                    total
                });
            },
        );
    }

    group.finish();
}

// ============================================================================
// Tier B: vector_get - Point Lookup
// ============================================================================

fn vector_get(c: &mut Criterion) {
    let mut group = c.benchmark_group("vector_get");
    group.measurement_time(Duration::from_secs(5));
    let dur = durability_label();
    let dimension = 384;

    // Test with different collection sizes
    for size in &[100, 1000, 10000] {
        group.bench_with_input(
            BenchmarkId::new(format!("existing/n_{}/{}", size, dur), size),
            size,
            |b, &size| {
                let (store, _db) = create_vector_store();
                let run_id = RunId::new();
                let config = VectorConfig {
                    dimension,
                    metric: DistanceMetric::Cosine,
                    storage_dtype: in_mem_primitives::vector::StorageDtype::F32,
                };
                store
                    .create_collection(run_id, "bench", config)
                    .expect("create collection");

                let keys = pregenerate_keys(size);
                for (i, key) in keys.iter().enumerate() {
                    let vec = random_normalized_vector(dimension, BENCH_SEED + i as u64);
                    store
                        .insert(run_id, "bench", key, &vec, None)
                        .expect("insert");
                }

                let mut seed = BENCH_SEED;
                b.iter(|| {
                    let idx = (lcg_next(&mut seed) as usize) % size;
                    let key = &keys[idx];
                    black_box(store.get(run_id, "bench", key).expect("get"))
                });
            },
        );
    }

    // Test miss case
    group.bench_function(format!("missing/{}", dur), |b| {
        let (store, _db) = create_vector_store();
        let run_id = RunId::new();
        let config = VectorConfig {
            dimension,
            metric: DistanceMetric::Cosine,
            storage_dtype: in_mem_primitives::vector::StorageDtype::F32,
        };
        store
            .create_collection(run_id, "bench", config)
            .expect("create collection");

        // Insert some vectors
        for i in 0..1000 {
            let key = format!("key_{}", i);
            let vec = random_normalized_vector(dimension, BENCH_SEED + i);
            store
                .insert(run_id, "bench", &key, &vec, None)
                .expect("insert");
        }

        b.iter(|| {
            black_box(
                store
                    .get(run_id, "bench", "nonexistent_key_12345")
                    .expect("get"),
            )
        });
    });

    group.finish();
}

// ============================================================================
// Tier B: vector_delete
// ============================================================================

fn vector_delete(c: &mut Criterion) {
    let mut group = c.benchmark_group("vector_delete");
    group.measurement_time(Duration::from_secs(5));
    let dur = durability_label();
    let dimension = 384;

    group.bench_function(format!("existing/{}", dur), |b| {
        b.iter_custom(|iters| {
            let (store, _db) = create_vector_store();
            let run_id = RunId::new();
            let config = VectorConfig {
                dimension,
                metric: DistanceMetric::Cosine,
                storage_dtype: in_mem_primitives::vector::StorageDtype::F32,
            };
            store
                .create_collection(run_id, "bench", config)
                .expect("create collection");

            // Insert enough vectors for all iterations
            let count = iters as usize + 1000;
            for i in 0..count {
                let key = format!("key_{}", i);
                let vec = random_normalized_vector(dimension, BENCH_SEED + i as u64);
                store
                    .insert(run_id, "bench", &key, &vec, None)
                    .expect("insert");
            }

            let mut total = Duration::ZERO;
            for i in 0..iters {
                let key = format!("key_{}", i);

                let start = std::time::Instant::now();
                store.delete(run_id, "bench", &key).expect("delete");
                total += start.elapsed();
            }
            total
        });
    });

    group.bench_function(format!("missing/{}", dur), |b| {
        let (store, _db) = create_vector_store();
        let run_id = RunId::new();
        let config = VectorConfig {
            dimension,
            metric: DistanceMetric::Cosine,
            storage_dtype: in_mem_primitives::vector::StorageDtype::F32,
        };
        store
            .create_collection(run_id, "bench", config)
            .expect("create collection");

        // Insert some vectors
        for i in 0..1000 {
            let key = format!("key_{}", i);
            let vec = random_normalized_vector(dimension, BENCH_SEED + i);
            store
                .insert(run_id, "bench", &key, &vec, None)
                .expect("insert");
        }

        let mut counter = 0u64;
        b.iter(|| {
            let key = format!("nonexistent_{}", counter);
            counter += 1;
            black_box(store.delete(run_id, "bench", &key).expect("delete"))
        });
    });

    group.finish();
}

// ============================================================================
// Tier B: vector_count
// ============================================================================

fn vector_count(c: &mut Criterion) {
    let mut group = c.benchmark_group("vector_count");
    group.measurement_time(Duration::from_secs(5));
    let dur = durability_label();
    let dimension = 384;

    for size in &[100, 1000, 10000] {
        group.bench_with_input(
            BenchmarkId::new(format!("n_{}/{}", size, dur), size),
            size,
            |b, &size| {
                let (store, _db) = create_vector_store();
                let run_id = RunId::new();
                let config = VectorConfig {
                    dimension,
                    metric: DistanceMetric::Cosine,
                    storage_dtype: in_mem_primitives::vector::StorageDtype::F32,
                };
                store
                    .create_collection(run_id, "bench", config)
                    .expect("create collection");

                for i in 0..size {
                    let key = format!("key_{}", i);
                    let vec = random_normalized_vector(dimension, BENCH_SEED + i as u64);
                    store
                        .insert(run_id, "bench", &key, &vec, None)
                        .expect("insert");
                }

                b.iter(|| black_box(store.count(run_id, "bench").expect("count")));
            },
        );
    }

    group.finish();
}

// ============================================================================
// Tier B: vector_search - K-NN Search
// ============================================================================

fn vector_search(c: &mut Criterion) {
    let mut group = c.benchmark_group("vector_search");
    group.measurement_time(Duration::from_secs(10));
    let dur = durability_label();
    let dimension = 384;

    // Test different collection sizes and k values
    for &collection_size in &[100, 1_000, 10_000] {
        let (store, _db) = create_vector_store();
        let run_id = RunId::new();
        let config = VectorConfig {
            dimension,
            metric: DistanceMetric::Cosine,
            storage_dtype: in_mem_primitives::vector::StorageDtype::F32,
        };
        store
            .create_collection(run_id, "bench", config)
            .expect("create collection");

        // Insert vectors
        for i in 0..collection_size {
            let key = format!("key_{:08}", i);
            let vec = random_normalized_vector(dimension, BENCH_SEED + i as u64);
            store
                .insert(run_id, "bench", &key, &vec, None)
                .expect("insert");
        }

        for k in &K_VALUES {
            if *k > collection_size {
                continue;
            }

            group.throughput(Throughput::Elements(*k as u64));
            group.bench_with_input(
                BenchmarkId::new(format!("k_{}/n_{}/{}", k, collection_size, dur), k),
                k,
                |b, &k| {
                    let mut seed = BENCH_SEED;
                    b.iter(|| {
                        let query = random_normalized_vector(dimension, seed);
                        seed = lcg_next(&mut seed);
                        black_box(
                            store
                                .search(run_id, "bench", &query, k, None)
                                .expect("search"),
                        )
                    });
                },
            );
        }
    }

    group.finish();
}

// ============================================================================
// Tier C: Dimension Scaling
// ============================================================================

fn vector_dimension_scaling(c: &mut Criterion) {
    let mut group = c.benchmark_group("vector_dimension_scaling");
    group.measurement_time(Duration::from_secs(5));
    let dur = durability_label();
    let collection_size = 1000;
    let k = 10;

    for dimension in &DIMENSIONS {
        // Setup collection
        let (store, _db) = create_vector_store();
        let run_id = RunId::new();
        let config = VectorConfig {
            dimension: *dimension,
            metric: DistanceMetric::Cosine,
            storage_dtype: in_mem_primitives::vector::StorageDtype::F32,
        };
        store
            .create_collection(run_id, "bench", config)
            .expect("create collection");

        for i in 0..collection_size {
            let key = format!("key_{:08}", i);
            let vec = random_normalized_vector(*dimension, BENCH_SEED + i as u64);
            store
                .insert(run_id, "bench", &key, &vec, None)
                .expect("insert");
        }

        // Benchmark insert
        group.bench_function(format!("insert/dim_{}/{}", dimension, dur), |b| {
            let mut key_counter = collection_size as u64;
            b.iter(|| {
                let key = format!("key_{}", key_counter);
                key_counter += 1;
                let vec = random_normalized_vector(*dimension, key_counter);
                store
                    .insert(run_id, "bench", &key, &vec, None)
                    .expect("insert");
            });
        });

        // Benchmark search
        group.bench_function(format!("search/dim_{}/{}", dimension, dur), |b| {
            let mut seed = BENCH_SEED;
            b.iter(|| {
                let query = random_normalized_vector(*dimension, seed);
                seed = lcg_next(&mut seed);
                black_box(
                    store
                        .search(run_id, "bench", &query, k, None)
                        .expect("search"),
                )
            });
        });
    }

    group.finish();
}

// ============================================================================
// Tier C: Distance Metric Comparison
// ============================================================================

fn vector_metric_comparison(c: &mut Criterion) {
    let mut group = c.benchmark_group("vector_metric_comparison");
    group.measurement_time(Duration::from_secs(5));
    let dur = durability_label();
    let dimension = 384;
    let collection_size = 1000;
    let k = 10;

    let metrics = [
        (DistanceMetric::Cosine, "cosine"),
        (DistanceMetric::Euclidean, "euclidean"),
        (DistanceMetric::DotProduct, "dot_product"),
    ];

    for (metric, metric_name) in metrics {
        let (store, _db) = create_vector_store();
        let run_id = RunId::new();
        let config = VectorConfig {
            dimension,
            metric,
            storage_dtype: in_mem_primitives::vector::StorageDtype::F32,
        };
        store
            .create_collection(run_id, "bench", config)
            .expect("create collection");

        for i in 0..collection_size {
            let key = format!("key_{:08}", i);
            let vec = random_normalized_vector(dimension, BENCH_SEED + i as u64);
            store
                .insert(run_id, "bench", &key, &vec, None)
                .expect("insert");
        }

        group.bench_function(format!("search/{}/{}", metric_name, dur), |b| {
            let mut seed = BENCH_SEED;
            b.iter(|| {
                let query = random_normalized_vector(dimension, seed);
                seed = lcg_next(&mut seed);
                black_box(
                    store
                        .search(run_id, "bench", &query, k, None)
                        .expect("search"),
                )
            });
        });
    }

    group.finish();
}

// ============================================================================
// Tier C: Collection Size Scaling (Search Performance)
// ============================================================================

fn vector_collection_scaling(c: &mut Criterion) {
    let mut group = c.benchmark_group("vector_collection_scaling");
    group.measurement_time(Duration::from_secs(10));
    let dur = durability_label();
    let dimension = 384;
    let k = 10;

    for collection_size in &COLLECTION_SIZES {
        let (store, _db) = create_vector_store();
        let run_id = RunId::new();
        let config = VectorConfig {
            dimension,
            metric: DistanceMetric::Cosine,
            storage_dtype: in_mem_primitives::vector::StorageDtype::F32,
        };
        store
            .create_collection(run_id, "bench", config)
            .expect("create collection");

        // Insert vectors
        for i in 0..*collection_size {
            let key = format!("key_{:08}", i);
            let vec = random_normalized_vector(dimension, BENCH_SEED + i as u64);
            store
                .insert(run_id, "bench", &key, &vec, None)
                .expect("insert");
        }

        group.bench_function(format!("search/n_{}/{}", collection_size, dur), |b| {
            let mut seed = BENCH_SEED;
            b.iter(|| {
                let query = random_normalized_vector(dimension, seed);
                seed = lcg_next(&mut seed);
                black_box(
                    store
                        .search(run_id, "bench", &query, k, None)
                        .expect("search"),
                )
            });
        });
    }

    group.finish();
}

// ============================================================================
// Tier B: list_collections
// ============================================================================

fn vector_list_collections(c: &mut Criterion) {
    let mut group = c.benchmark_group("vector_list_collections");
    group.measurement_time(Duration::from_secs(5));
    let dur = durability_label();

    for count in &[1, 10, 50] {
        group.bench_with_input(
            BenchmarkId::new(format!("n_{}/{}", count, dur), count),
            count,
            |b, &count| {
                let (store, _db) = create_vector_store();
                let run_id = RunId::new();

                for i in 0..count {
                    let name = format!("collection_{:03}", i);
                    let config = VectorConfig {
                        dimension: 384,
                        metric: DistanceMetric::Cosine,
                        storage_dtype: in_mem_primitives::vector::StorageDtype::F32,
                    };
                    store
                        .create_collection(run_id, &name, config)
                        .expect("create collection");
                }

                b.iter(|| black_box(store.list_collections(run_id).expect("list")));
            },
        );
    }

    group.finish();
}

// ============================================================================
// Criterion Configuration
// ============================================================================

criterion_group!(
    name = vector_collection_benches;
    config = Criterion::default().sample_size(50);
    targets =
        vector_create_collection,
        vector_list_collections,
);

criterion_group!(
    name = vector_operation_benches;
    config = Criterion::default().sample_size(100);
    targets =
        vector_insert_single,
        vector_insert_batch,
        vector_get,
        vector_delete,
        vector_count,
);

criterion_group!(
    name = vector_search_benches;
    config = Criterion::default().sample_size(50);
    targets =
        vector_search,
);

criterion_group!(
    name = vector_scaling_benches;
    config = Criterion::default().sample_size(30);
    targets =
        vector_dimension_scaling,
        vector_metric_comparison,
        vector_collection_scaling,
);

criterion_main!(
    vector_collection_benches,
    vector_operation_benches,
    vector_search_benches,
    vector_scaling_benches,
);
