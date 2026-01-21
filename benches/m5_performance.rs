//! M5 Performance Benchmarks - JSON Primitive
//!
//! Run with: cargo bench --bench m5_performance
//!
//! These benchmarks follow the M1/M2 taxonomy with explicit labels for:
//! - Layer (json_*)
//! - Access pattern (hot_doc, uniform, working_set, miss)
//! - Durability mode (dur_strict, dur_batched, dur_inmemory)
//! - Document complexity (size, depth, key count)
//!
//! JSON Performance Targets:
//! - json_create/small: < 50µs
//! - json_get/hot_doc: < 10µs
//! - json_set/hot_path: < 50µs
//! - json_delete_at_path: < 30µs
//!
//! Non-Regression Targets (M4):
//! - KV put InMemory: < 3µs
//! - KV get fast path: < 5µs
//! - Event append: < 10µs
//! - State read: < 5µs

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use strata_core::json::{JsonPath, JsonValue};
use strata_core::types::{JsonDocId, RunId};
use strata_core::value::Value;
use strata_engine::Database;
use strata_primitives::{EventLog, JsonStore, KVStore, StateCell, TraceStore, TraceType};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

// ============================================================================
// Constants and Utilities
// ============================================================================

/// Fixed seed for reproducible benchmarks
const BENCH_SEED: u64 = 0xDEADBEEF_CAFEBABE;

/// Simple LCG for deterministic pseudo-random access patterns
fn lcg_next(state: &mut u64) -> u64 {
    *state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
    *state
}

/// Pre-generate document IDs for benchmarks
fn pregenerate_doc_ids(count: usize) -> Vec<JsonDocId> {
    (0..count).map(|_| JsonDocId::new()).collect()
}

/// Create a JSON document of approximately the given size
fn create_sized_document(size: usize) -> JsonValue {
    // Approximate size by creating object with string values
    let value_size = size.max(10) - 10; // Account for JSON overhead
    JsonValue::from("x".repeat(value_size))
}

/// Create a nested JSON object of given depth
fn create_nested_document(depth: usize) -> (JsonValue, JsonPath) {
    let mut value: serde_json::Value = serde_json::json!(42);
    for _ in 0..depth {
        let mut obj = serde_json::Map::new();
        obj.insert("nested".to_string(), value);
        value = serde_json::Value::Object(obj);
    }
    let path_str = (0..depth).map(|_| "nested").collect::<Vec<_>>().join(".");
    let path: JsonPath = if path_str.is_empty() {
        JsonPath::root()
    } else {
        path_str.parse().unwrap()
    };
    (JsonValue::from_value(value), path)
}

/// Create a JSON object with N keys
fn create_wide_document(key_count: usize) -> JsonValue {
    let mut obj = serde_json::Map::new();
    for i in 0..key_count {
        obj.insert(format!("key_{}", i), serde_json::json!(i));
    }
    JsonValue::from_value(serde_json::Value::Object(obj))
}

/// Get durability mode label from environment
fn durability_label() -> &'static str {
    match std::env::var("INMEM_DURABILITY_MODE").ok().as_deref() {
        Some("inmemory") => "dur_inmemory",
        Some("batched") => "dur_batched",
        _ => "dur_strict",
    }
}

// ============================================================================
// json_create - Document Creation Benchmarks
// ============================================================================

fn json_create_by_size(c: &mut Criterion) {
    let mut group = c.benchmark_group("json_create");
    group.measurement_time(Duration::from_secs(5));
    let dur = durability_label();

    // --- Benchmark: json_create/small ---
    // Semantic: Small document (100 bytes) can be created
    // Real pattern: Simple state storage
    for size in [100, 1_000, 10_000] {
        let label = match size {
            100 => "small",
            1_000 => "medium",
            10_000 => "large",
            _ => "custom",
        };

        group.throughput(Throughput::Bytes(size as u64));
        group.bench_with_input(
            BenchmarkId::new(format!("{}/{}", label, dur), size),
            &size,
            |b, &size| {
                let db = Database::builder().in_memory().open_temp().unwrap();
                let json = JsonStore::new(Arc::new(db));
                let run_id = RunId::new();
                let value = create_sized_document(size);

                b.iter(|| {
                    let doc_id = JsonDocId::new();
                    json.create(&run_id, &doc_id, value.clone()).unwrap()
                });
            },
        );
    }

    group.finish();
}

fn json_create_by_complexity(c: &mut Criterion) {
    let mut group = c.benchmark_group("json_create");
    group.measurement_time(Duration::from_secs(5));
    let dur = durability_label();

    // --- Benchmark: json_create/depth_* ---
    // Semantic: Nested documents can be created
    // Real pattern: Hierarchical state
    for depth in [1, 5, 10, 20] {
        group.bench_with_input(
            BenchmarkId::new(format!("depth_{}/{}", depth, dur), depth),
            &depth,
            |b, &depth| {
                let db = Database::builder().in_memory().open_temp().unwrap();
                let json = JsonStore::new(Arc::new(db));
                let run_id = RunId::new();
                let (value, _) = create_nested_document(depth);

                b.iter(|| {
                    let doc_id = JsonDocId::new();
                    json.create(&run_id, &doc_id, value.clone()).unwrap()
                });
            },
        );
    }

    // --- Benchmark: json_create/keys_* ---
    // Semantic: Wide objects can be created
    // Real pattern: Flat state with many fields
    for key_count in [10, 100, 1000] {
        group.bench_with_input(
            BenchmarkId::new(format!("keys_{}/{}", key_count, dur), key_count),
            &key_count,
            |b, &key_count| {
                let db = Database::builder().in_memory().open_temp().unwrap();
                let json = JsonStore::new(Arc::new(db));
                let run_id = RunId::new();
                let value = create_wide_document(key_count);

                b.iter(|| {
                    let doc_id = JsonDocId::new();
                    json.create(&run_id, &doc_id, value.clone()).unwrap()
                });
            },
        );
    }

    group.finish();
}

// ============================================================================
// json_get - Read Benchmarks by Access Pattern
// ============================================================================

fn json_get_by_access_pattern(c: &mut Criterion) {
    let mut group = c.benchmark_group("json_get");
    group.measurement_time(Duration::from_secs(5));

    // --- Benchmark: json_get/hot_doc ---
    // Semantic: Same document, repeated access
    // Real pattern: Config reads, counters
    group.bench_function("hot_doc", |b| {
        let db = Database::builder().in_memory().open_temp().unwrap();
        let json = JsonStore::new(Arc::new(db));
        let run_id = RunId::new();
        let doc_id = JsonDocId::new();

        json.create(
            &run_id,
            &doc_id,
            serde_json::json!({"data": "test", "count": 42}).into(),
        )
        .unwrap();

        let path: JsonPath = "data".parse().unwrap();

        b.iter(|| json.get(&run_id, &doc_id, &path).unwrap());
    });

    // --- Benchmark: json_get/uniform ---
    // Semantic: Random documents from keyspace
    // Real pattern: Arbitrary state access
    {
        let doc_count = 1000;

        group.bench_function("uniform", |b| {
            let db = Database::builder().in_memory().open_temp().unwrap();
            let json = JsonStore::new(Arc::new(db));
            let run_id = RunId::new();
            let doc_ids = pregenerate_doc_ids(doc_count);

            // Pre-populate
            for (i, doc_id) in doc_ids.iter().enumerate() {
                json.create(&run_id, doc_id, JsonValue::from(i as i64))
                    .unwrap();
            }

            let path = JsonPath::root();
            let mut rng_state = BENCH_SEED;

            b.iter(|| {
                let idx = (lcg_next(&mut rng_state) as usize) % doc_count;
                json.get(&run_id, &doc_ids[idx], &path).unwrap()
            });
        });
    }

    // --- Benchmark: json_get/working_set_100 ---
    // Semantic: Small subset of documents
    // Real pattern: Frequently accessed state
    {
        let total_docs = 10000;
        let working_set = 100;

        group.bench_function("working_set_100", |b| {
            let db = Database::builder().in_memory().open_temp().unwrap();
            let json = JsonStore::new(Arc::new(db));
            let run_id = RunId::new();
            let doc_ids = pregenerate_doc_ids(total_docs);

            // Pre-populate all
            for (i, doc_id) in doc_ids.iter().enumerate() {
                json.create(&run_id, doc_id, JsonValue::from(i as i64))
                    .unwrap();
            }

            let path = JsonPath::root();
            let mut rng_state = BENCH_SEED;

            b.iter(|| {
                // Access only first 100 docs
                let idx = (lcg_next(&mut rng_state) as usize) % working_set;
                json.get(&run_id, &doc_ids[idx], &path).unwrap()
            });
        });
    }

    // --- Benchmark: json_get/miss ---
    // Semantic: Non-existent document
    // Real pattern: Existence checks
    group.bench_function("miss", |b| {
        let db = Database::builder().in_memory().open_temp().unwrap();
        let json = JsonStore::new(Arc::new(db));
        let run_id = RunId::new();
        let doc_id = JsonDocId::new(); // Never created

        let path = JsonPath::root();

        b.iter(|| json.get(&run_id, &doc_id, &path).unwrap());
    });

    group.finish();
}

fn json_get_by_path_depth(c: &mut Criterion) {
    let mut group = c.benchmark_group("json_get");
    group.measurement_time(Duration::from_secs(5));

    // --- Benchmark: json_get/depth_* ---
    // Semantic: Path traversal at various depths
    // Real pattern: Nested state access
    for depth in [1, 5, 10, 20] {
        group.bench_with_input(BenchmarkId::new("depth", depth), &depth, |b, &depth| {
            let db = Database::builder().in_memory().open_temp().unwrap();
            let json = JsonStore::new(Arc::new(db));
            let run_id = RunId::new();
            let doc_id = JsonDocId::new();

            let (value, path) = create_nested_document(depth);
            json.create(&run_id, &doc_id, value).unwrap();

            b.iter(|| json.get(&run_id, &doc_id, &path).unwrap());
        });
    }

    group.finish();
}

// ============================================================================
// json_set - Write Benchmarks
// ============================================================================

fn json_set_by_access_pattern(c: &mut Criterion) {
    let mut group = c.benchmark_group("json_set");
    group.measurement_time(Duration::from_secs(5));
    let dur = durability_label();

    // --- Benchmark: json_set/hot_path ---
    // Semantic: Same path, repeated updates
    // Real pattern: Counter updates
    group.bench_function(format!("hot_path/{}", dur), |b| {
        let db = Database::builder().in_memory().open_temp().unwrap();
        let json = JsonStore::new(Arc::new(db));
        let run_id = RunId::new();
        let doc_id = JsonDocId::new();

        json.create(&run_id, &doc_id, serde_json::json!({"counter": 0}).into())
            .unwrap();

        let path: JsonPath = "counter".parse().unwrap();
        let mut counter = 0i64;

        b.iter(|| {
            counter += 1;
            json.set(&run_id, &doc_id, &path, JsonValue::from(counter))
                .unwrap()
        });
    });

    // --- Benchmark: json_set/uniform_docs ---
    // Semantic: Updates across many documents
    // Real pattern: Distributed state updates
    {
        let doc_count = 100;

        group.bench_function(format!("uniform_docs/{}", dur), |b| {
            let db = Database::builder().in_memory().open_temp().unwrap();
            let json = JsonStore::new(Arc::new(db));
            let run_id = RunId::new();
            let doc_ids = pregenerate_doc_ids(doc_count);

            // Pre-populate
            for doc_id in &doc_ids {
                json.create(&run_id, doc_id, serde_json::json!({"value": 0}).into())
                    .unwrap();
            }

            let path: JsonPath = "value".parse().unwrap();
            let mut rng_state = BENCH_SEED;
            let mut counter = 0i64;

            b.iter(|| {
                counter += 1;
                let idx = (lcg_next(&mut rng_state) as usize) % doc_count;
                json.set(&run_id, &doc_ids[idx], &path, JsonValue::from(counter))
                    .unwrap()
            });
        });
    }

    // --- Benchmark: json_set/uniform_paths ---
    // Semantic: Updates to different paths in same document
    // Real pattern: Multi-field updates
    {
        let key_count = 100;

        group.bench_function(format!("uniform_paths/{}", dur), |b| {
            let db = Database::builder().in_memory().open_temp().unwrap();
            let json = JsonStore::new(Arc::new(db));
            let run_id = RunId::new();
            let doc_id = JsonDocId::new();

            let value = create_wide_document(key_count);
            json.create(&run_id, &doc_id, value).unwrap();

            // Pre-parse paths
            let paths: Vec<JsonPath> = (0..key_count)
                .map(|i| format!("key_{}", i).parse().unwrap())
                .collect();

            let mut rng_state = BENCH_SEED;
            let mut counter = 0i64;

            b.iter(|| {
                counter += 1;
                let idx = (lcg_next(&mut rng_state) as usize) % key_count;
                json.set(&run_id, &doc_id, &paths[idx], JsonValue::from(counter))
                    .unwrap()
            });
        });
    }

    group.finish();
}

fn json_set_by_depth(c: &mut Criterion) {
    let mut group = c.benchmark_group("json_set");
    group.measurement_time(Duration::from_secs(5));
    let dur = durability_label();

    // --- Benchmark: json_set/depth_* ---
    // Semantic: Path traversal cost for writes
    // Real pattern: Deep state modifications
    for depth in [1, 5, 10, 20] {
        group.bench_with_input(
            BenchmarkId::new(format!("depth/{}", dur), depth),
            &depth,
            |b, &depth| {
                let db = Database::builder().in_memory().open_temp().unwrap();
                let json = JsonStore::new(Arc::new(db));
                let run_id = RunId::new();
                let doc_id = JsonDocId::new();

                let (value, path) = create_nested_document(depth);
                json.create(&run_id, &doc_id, value).unwrap();

                let mut counter = 0i64;

                b.iter(|| {
                    counter += 1;
                    json.set(&run_id, &doc_id, &path, JsonValue::from(counter))
                        .unwrap()
                });
            },
        );
    }

    group.finish();
}

fn json_set_by_value_size(c: &mut Criterion) {
    let mut group = c.benchmark_group("json_set");
    group.measurement_time(Duration::from_secs(5));
    let dur = durability_label();

    // --- Benchmark: json_set/value_size_* ---
    // Semantic: Serialization cost at various sizes
    // Real pattern: Blob updates
    for size in [64, 256, 1024, 4096, 65536] {
        group.throughput(Throughput::Bytes(size as u64));
        group.bench_with_input(
            BenchmarkId::new(format!("value_size/{}", dur), size),
            &size,
            |b, &size| {
                let db = Database::builder().in_memory().open_temp().unwrap();
                let json = JsonStore::new(Arc::new(db));
                let run_id = RunId::new();
                let doc_id = JsonDocId::new();

                json.create(&run_id, &doc_id, JsonValue::object()).unwrap();

                let path: JsonPath = "data".parse().unwrap();
                let value = create_sized_document(size);

                b.iter(|| json.set(&run_id, &doc_id, &path, value.clone()).unwrap());
            },
        );
    }

    group.finish();
}

// ============================================================================
// json_delete - Delete Benchmarks
// ============================================================================

fn json_delete_benchmarks(c: &mut Criterion) {
    let mut group = c.benchmark_group("json_delete");
    group.measurement_time(Duration::from_secs(5));
    let dur = durability_label();

    // --- Benchmark: json_delete/existing_key ---
    // Semantic: Delete removes the value
    // Real pattern: State cleanup
    group.bench_function(format!("existing_key/{}", dur), |b| {
        let db = Database::builder().in_memory().open_temp().unwrap();
        let json = JsonStore::new(Arc::new(db));
        let run_id = RunId::new();

        b.iter_batched(
            || {
                let doc_id = JsonDocId::new();
                json.create(
                    &run_id,
                    &doc_id,
                    serde_json::json!({"to_delete": 42, "keep": 43}).into(),
                )
                .unwrap();
                doc_id
            },
            |doc_id| {
                json.delete_at_path(&run_id, &doc_id, &"to_delete".parse().unwrap())
                    .unwrap()
            },
            criterion::BatchSize::SmallInput,
        )
    });

    // --- Benchmark: json_delete/deep_path ---
    // Semantic: Delete at nested path
    // Real pattern: Nested state cleanup
    for depth in [1, 5, 10] {
        group.bench_with_input(
            BenchmarkId::new(format!("depth/{}", dur), depth),
            &depth,
            |b, &depth| {
                let db = Database::builder().in_memory().open_temp().unwrap();
                let json = JsonStore::new(Arc::new(db));
                let run_id = RunId::new();

                b.iter_batched(
                    || {
                        let doc_id = JsonDocId::new();
                        let (value, _) = create_nested_document(depth);
                        json.create(&run_id, &doc_id, value).unwrap();
                        doc_id
                    },
                    |doc_id| {
                        let path_str = (0..depth).map(|_| "nested").collect::<Vec<_>>().join(".");
                        let path: JsonPath = path_str.parse().unwrap();
                        json.delete_at_path(&run_id, &doc_id, &path).unwrap()
                    },
                    criterion::BatchSize::SmallInput,
                )
            },
        );
    }

    group.finish();
}

// ============================================================================
// json_destroy - Document Destruction
// ============================================================================

fn json_destroy_benchmarks(c: &mut Criterion) {
    let mut group = c.benchmark_group("json_destroy");
    group.measurement_time(Duration::from_secs(5));
    let dur = durability_label();

    // --- Benchmark: json_destroy/small ---
    // Semantic: Entire document removed
    // Real pattern: State cleanup
    group.bench_function(format!("small/{}", dur), |b| {
        let db = Database::builder().in_memory().open_temp().unwrap();
        let json = JsonStore::new(Arc::new(db));
        let run_id = RunId::new();

        b.iter_batched(
            || {
                let doc_id = JsonDocId::new();
                json.create(&run_id, &doc_id, JsonValue::from(42i64))
                    .unwrap();
                doc_id
            },
            |doc_id| json.destroy(&run_id, &doc_id).unwrap(),
            criterion::BatchSize::SmallInput,
        )
    });

    // --- Benchmark: json_destroy/large ---
    // Semantic: Large document destruction cost
    // Real pattern: Cleanup after heavy use
    group.bench_function(format!("large/{}", dur), |b| {
        let db = Database::builder().in_memory().open_temp().unwrap();
        let json = JsonStore::new(Arc::new(db));
        let run_id = RunId::new();

        b.iter_batched(
            || {
                let doc_id = JsonDocId::new();
                let value = create_wide_document(1000);
                json.create(&run_id, &doc_id, value).unwrap();
                doc_id
            },
            |doc_id| json.destroy(&run_id, &doc_id).unwrap(),
            criterion::BatchSize::SmallInput,
        )
    });

    group.finish();
}

// ============================================================================
// json_exists - Existence Check
// ============================================================================

fn json_exists_benchmarks(c: &mut Criterion) {
    let mut group = c.benchmark_group("json_exists");
    group.measurement_time(Duration::from_secs(5));

    // --- Benchmark: json_exists/hit ---
    // Semantic: Fast existence check for existing doc
    // Real pattern: Conditional operations
    group.bench_function("hit", |b| {
        let db = Database::builder().in_memory().open_temp().unwrap();
        let json = JsonStore::new(Arc::new(db));
        let run_id = RunId::new();
        let doc_id = JsonDocId::new();

        json.create(&run_id, &doc_id, JsonValue::from(42i64))
            .unwrap();

        b.iter(|| json.exists(&run_id, &doc_id).unwrap());
    });

    // --- Benchmark: json_exists/miss ---
    // Semantic: Fast existence check for non-existent doc
    // Real pattern: Create-if-not-exists
    group.bench_function("miss", |b| {
        let db = Database::builder().in_memory().open_temp().unwrap();
        let json = JsonStore::new(Arc::new(db));
        let run_id = RunId::new();
        let doc_id = JsonDocId::new(); // Never created

        b.iter(|| json.exists(&run_id, &doc_id).unwrap());
    });

    group.finish();
}

// ============================================================================
// json_version - Version Retrieval
// ============================================================================

fn json_version_benchmarks(c: &mut Criterion) {
    let mut group = c.benchmark_group("json_version");
    group.measurement_time(Duration::from_secs(5));

    // --- Benchmark: json_version/after_N_updates ---
    // Semantic: Version retrieval cost scales with history
    // Real pattern: Optimistic concurrency checks
    for updates in [1, 10, 100, 1000] {
        group.bench_with_input(
            BenchmarkId::new("after_updates", updates),
            &updates,
            |b, &updates| {
                let db = Database::builder().in_memory().open_temp().unwrap();
                let json = JsonStore::new(Arc::new(db));
                let run_id = RunId::new();
                let doc_id = JsonDocId::new();

                json.create(&run_id, &doc_id, JsonValue::object()).unwrap();

                // Perform updates
                let path: JsonPath = "counter".parse().unwrap();
                for i in 0..updates {
                    json.set(&run_id, &doc_id, &path, JsonValue::from(i as i64))
                        .unwrap();
                }

                b.iter(|| json.get_version(&run_id, &doc_id).unwrap());
            },
        );
    }

    group.finish();
}

// ============================================================================
// json_contention - Concurrency Benchmarks
// ============================================================================

fn json_contention_benchmarks(c: &mut Criterion) {
    let mut group = c.benchmark_group("json_contention");
    group.measurement_time(Duration::from_secs(5));
    group.sample_size(50); // Fewer samples for thread tests

    // --- Benchmark: json_contention/disjoint_docs ---
    // Semantic: No conflicts when accessing different documents
    // Real pattern: Partitioned agent state
    for threads in [2, 4, 8] {
        group.bench_with_input(
            BenchmarkId::new("disjoint_docs", threads),
            &threads,
            |b, &threads| {
                let db = Arc::new(Database::builder().in_memory().open_temp().unwrap());
                let run_id = RunId::new();

                // Create one doc per thread
                let doc_ids: Vec<JsonDocId> = (0..threads).map(|_| JsonDocId::new()).collect();
                for doc_id in &doc_ids {
                    let json = JsonStore::new(db.clone());
                    json.create(&run_id, doc_id, JsonValue::from(0i64)).unwrap();
                }

                b.iter(|| {
                    let ops = Arc::new(AtomicU64::new(0));
                    let handles: Vec<_> = (0..threads)
                        .map(|i| {
                            let db = db.clone();
                            let run_id = run_id.clone();
                            let doc_id = doc_ids[i].clone();
                            let ops = ops.clone();

                            std::thread::spawn(move || {
                                let json = JsonStore::new(db);
                                for j in 0..100 {
                                    json.set(
                                        &run_id,
                                        &doc_id,
                                        &JsonPath::root(),
                                        JsonValue::from(j),
                                    )
                                    .unwrap();
                                    ops.fetch_add(1, Ordering::Relaxed);
                                }
                            })
                        })
                        .collect();

                    for h in handles {
                        h.join().unwrap();
                    }

                    ops.load(Ordering::Relaxed)
                });
            },
        );
    }

    // --- Benchmark: json_contention/same_doc_different_paths ---
    // Semantic: Document-level conflicts even for different paths
    // Real pattern: Multiple fields in shared state
    for threads in [2, 4, 8] {
        group.bench_with_input(
            BenchmarkId::new("same_doc_different_paths", threads),
            &threads,
            |b, &threads| {
                let db = Arc::new(Database::builder().in_memory().open_temp().unwrap());
                let run_id = RunId::new();
                let doc_id = JsonDocId::new();

                let json = JsonStore::new(db.clone());
                let value = create_wide_document(threads);
                json.create(&run_id, &doc_id, value).unwrap();

                b.iter(|| {
                    let ops = Arc::new(AtomicU64::new(0));
                    let handles: Vec<_> = (0..threads)
                        .map(|i| {
                            let db = db.clone();
                            let run_id = run_id.clone();
                            let doc_id = doc_id.clone();
                            let path: JsonPath = format!("key_{}", i).parse().unwrap();
                            let ops = ops.clone();

                            std::thread::spawn(move || {
                                let json = JsonStore::new(db);
                                for j in 0..100 {
                                    // May encounter conflicts due to document-level locking
                                    let _ = json.set(&run_id, &doc_id, &path, JsonValue::from(j));
                                    ops.fetch_add(1, Ordering::Relaxed);
                                }
                            })
                        })
                        .collect();

                    for h in handles {
                        h.join().unwrap();
                    }

                    ops.load(Ordering::Relaxed)
                });
            },
        );
    }

    group.finish();
}

// ============================================================================
// json_scaling - Key and Document Scaling
// ============================================================================

fn json_doc_scaling_benchmarks(c: &mut Criterion) {
    let mut group = c.benchmark_group("json_doc_scaling");
    group.measurement_time(Duration::from_secs(5));

    // --- Benchmark: json_doc_scaling/get_rotating ---
    // Semantic: O(log n) lookup holds at scale
    // Real pattern: Large state databases
    for doc_count in [1000, 10000, 100000] {
        group.bench_with_input(
            BenchmarkId::new("get_rotating", doc_count),
            &doc_count,
            |b, &doc_count| {
                let db = Database::builder().in_memory().open_temp().unwrap();
                let json = JsonStore::new(Arc::new(db));
                let run_id = RunId::new();
                let doc_ids = pregenerate_doc_ids(doc_count);

                // Pre-populate
                for (i, doc_id) in doc_ids.iter().enumerate() {
                    json.create(&run_id, doc_id, JsonValue::from(i as i64))
                        .unwrap();
                }

                let path = JsonPath::root();
                let mut idx = 0usize;

                b.iter(|| {
                    idx = (idx + 1) % doc_count;
                    json.get(&run_id, &doc_ids[idx], &path).unwrap()
                });
            },
        );
    }

    group.finish();
}

// ============================================================================
// Non-Regression Benchmarks (M4 Targets)
// ============================================================================

fn kv_put_inmemory_benchmark(c: &mut Criterion) {
    let mut group = c.benchmark_group("m4_regression");
    group.measurement_time(Duration::from_secs(5));

    let db = Database::builder().in_memory().open_temp().unwrap();
    let kv = KVStore::new(Arc::new(db));
    let run_id = RunId::new();

    // Warmup
    for i in 0..100 {
        kv.put(&run_id, &format!("warmup{}", i), Value::I64(i as i64))
            .unwrap();
    }

    let mut counter = 0u64;

    group.bench_function("kv_put_inmemory", |b| {
        b.iter(|| {
            counter += 1;
            let key = format!("key_{}", counter);
            kv.put(&run_id, &key, Value::I64(counter as i64)).unwrap()
        });
    });

    group.finish();
}

fn kv_get_fast_path_benchmark(c: &mut Criterion) {
    let mut group = c.benchmark_group("m4_regression");
    group.measurement_time(Duration::from_secs(5));

    let db = Database::builder().in_memory().open_temp().unwrap();
    let kv = KVStore::new(Arc::new(db));
    let run_id = RunId::new();

    // Pre-populate
    for i in 0..1000 {
        let key = format!("key_{}", i);
        kv.put(&run_id, &key, Value::I64(i as i64)).unwrap();
    }

    let mut counter = 0u64;

    group.bench_function("kv_get_fast_path", |b| {
        b.iter(|| {
            counter = (counter + 1) % 1000;
            let key = format!("key_{}", counter);
            kv.get(&run_id, &key).unwrap()
        });
    });

    group.finish();
}

fn event_append_benchmark(c: &mut Criterion) {
    let mut group = c.benchmark_group("m4_regression");
    group.measurement_time(Duration::from_secs(5));

    let db = Database::builder().in_memory().open_temp().unwrap();
    let events = EventLog::new(Arc::new(db));
    let run_id = RunId::new();

    let payload = Value::String("test data".to_string());

    group.bench_function("event_append", |b| {
        b.iter(|| {
            events
                .append(&run_id, "test_event", payload.clone())
                .unwrap()
        });
    });

    group.finish();
}

fn state_read_benchmark(c: &mut Criterion) {
    let mut group = c.benchmark_group("m4_regression");
    group.measurement_time(Duration::from_secs(5));

    let db = Database::builder().in_memory().open_temp().unwrap();
    let state = StateCell::new(Arc::new(db));
    let run_id = RunId::new();

    state.set(&run_id, "key", Value::I64(42)).unwrap();

    group.bench_function("state_read", |b| {
        b.iter(|| state.read(&run_id, "key").unwrap());
    });

    group.finish();
}

fn trace_record_benchmark(c: &mut Criterion) {
    let mut group = c.benchmark_group("m4_regression");
    group.measurement_time(Duration::from_secs(5));

    let db = Database::builder().in_memory().open_temp().unwrap();
    let trace = TraceStore::new(Arc::new(db));
    let run_id = RunId::new();

    let metadata = Value::String("test data".to_string());
    let trace_type = TraceType::Thought {
        content: "Benchmark thought".to_string(),
        confidence: Some(0.9),
    };

    group.bench_function("trace_record", |b| {
        b.iter(|| {
            trace
                .record(&run_id, trace_type.clone(), vec![], metadata.clone())
                .unwrap()
        });
    });

    group.finish();
}

// ============================================================================
// Mixed Workload Benchmarks
// ============================================================================

fn mixed_json_kv_benchmark(c: &mut Criterion) {
    let mut group = c.benchmark_group("mixed_workload");
    group.measurement_time(Duration::from_secs(5));

    let db = Database::builder().in_memory().open_temp().unwrap();
    let db = Arc::new(db);
    let json = JsonStore::new(db.clone());
    let kv = KVStore::new(db);
    let run_id = RunId::new();

    // Setup
    let doc_id = JsonDocId::new();
    json.create(&run_id, &doc_id, JsonValue::object()).unwrap();

    let mut counter = 0u64;

    group.bench_function("json_and_kv", |b| {
        b.iter(|| {
            counter += 1;

            // JSON operation
            json.set(
                &run_id,
                &doc_id,
                &"counter".parse().unwrap(),
                JsonValue::from(counter as i64),
            )
            .unwrap();

            // KV operation
            let key = format!("key_{}", counter);
            kv.put(&run_id, &key, Value::I64(counter as i64)).unwrap();
        });
    });

    group.finish();
}

// ============================================================================
// Criterion Groups and Main
// ============================================================================

criterion_group!(
    json_create_benches,
    json_create_by_size,
    json_create_by_complexity,
);

criterion_group!(
    json_get_benches,
    json_get_by_access_pattern,
    json_get_by_path_depth,
);

criterion_group!(
    json_set_benches,
    json_set_by_access_pattern,
    json_set_by_depth,
    json_set_by_value_size,
);

criterion_group!(json_delete_benches, json_delete_benchmarks,);

criterion_group!(
    json_other_benches,
    json_destroy_benchmarks,
    json_exists_benchmarks,
    json_version_benchmarks,
);

criterion_group!(json_scaling_benches, json_doc_scaling_benchmarks,);

criterion_group!(json_contention_benches, json_contention_benchmarks,);

criterion_group!(
    regression_benches,
    kv_put_inmemory_benchmark,
    kv_get_fast_path_benchmark,
    event_append_benchmark,
    state_read_benchmark,
    trace_record_benchmark,
);

criterion_group!(mixed_benches, mixed_json_kv_benchmark,);

criterion_main!(
    json_create_benches,
    json_get_benches,
    json_set_benches,
    json_delete_benches,
    json_other_benches,
    json_scaling_benches,
    json_contention_benches,
    regression_benches,
    mixed_benches
);
