//! Industry Comparison Benchmarks
//!
//! Run with: cargo bench --bench industry_comparison --features=comparison-benchmarks
//!
//! Compares in-mem primitives against industry-leading databases:
//!
//! ## Comparisons by Primitive
//!
//! | Primitive    | Comparison Target      | Category       |
//! |--------------|------------------------|----------------|
//! | KVStore      | redb, LMDB (heed)      | Embedded KV    |
//! | JsonStore    | SQLite + JSON1         | Document Store |
//! | VectorStore  | USearch                | Vector Search  |
//! | EventLog     | Custom WAL baseline    | Append-only    |
//! | StateCell    | redb                   | State Store    |
//!
//! ## Running Comparisons
//!
//! ```bash
//! # Run all comparison benchmarks (requires feature flag)
//! cargo bench --bench industry_comparison --features=comparison-benchmarks
//!
//! # Run specific comparison
//! cargo bench --bench industry_comparison --features=comparison-benchmarks -- kv_comparison
//!
//! # Run with baseline tracking
//! ./scripts/bench_runner.sh --comparison --baseline=sota_comparison
//! ```
//!
//! ## Notes on Fairness
//!
//! These comparisons are inherently "unfair" in several ways:
//! 1. in-mem provides transaction support, other embedded KVs may not
//! 2. in-mem is optimized for agent workloads, not general-purpose
//! 3. Each database has different design goals and trade-offs
//!
//! The purpose is to understand where we stand relative to SOTA,
//! not to claim superiority.

use criterion::{
    black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput,
};
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

/// Pre-generate keys for deterministic benchmarks
fn pregenerate_keys(count: usize) -> Vec<String> {
    (0..count).map(|i| format!("key_{:08}", i)).collect()
}

/// Pre-generate values for deterministic benchmarks
fn pregenerate_values(count: usize, size: usize) -> Vec<Vec<u8>> {
    let mut seed = BENCH_SEED;
    (0..count)
        .map(|_| {
            (0..size)
                .map(|_| (lcg_next(&mut seed) & 0xFF) as u8)
                .collect()
        })
        .collect()
}

/// Generate a deterministic random vector of given dimension
fn random_vector(dimension: usize, seed: u64) -> Vec<f32> {
    let mut state = seed;
    (0..dimension)
        .map(|_| {
            let bits = lcg_next(&mut state);
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

// ============================================================================
// In-Mem Imports
// ============================================================================

use in_mem_core::types::RunId;
use in_mem_core::value::Value;
use in_mem_engine::Database;
use in_mem_primitives::{EventLog, JsonStore, KVStore, StateCell};

#[cfg(feature = "comparison-benchmarks")]
use in_mem_primitives::vector::{DistanceMetric, VectorConfig, VectorStore};

// ============================================================================
// KVStore Comparison: in-mem vs redb vs LMDB
// ============================================================================

/// Compare KVStore point reads against redb and LMDB
fn kv_comparison_read(c: &mut Criterion) {
    let mut group = c.benchmark_group("kv_comparison/read");
    group.measurement_time(Duration::from_secs(10));

    let keys = pregenerate_keys(10000);
    let values = pregenerate_values(10000, 100); // 100-byte values

    // === in-mem KVStore ===
    {
        let db = Arc::new(Database::builder().in_memory().open_temp().unwrap());
        let kv = KVStore::new(Arc::clone(&db));
        let run_id = RunId::new();

        // Populate
        for (i, key) in keys.iter().enumerate() {
            kv.put(&run_id, key, Value::Bytes(values[i].clone()))
                .expect("populate");
        }

        group.bench_function("inmem_kvstore", |b| {
            let mut seed = BENCH_SEED;
            b.iter(|| {
                let idx = (lcg_next(&mut seed) as usize) % 10000;
                black_box(kv.get(&run_id, &keys[idx]).expect("get"))
            });
        });
    }

    // === redb (if available) ===
    #[cfg(feature = "comparison-benchmarks")]
    {
        use redb::{Database as RedbDatabase, TableDefinition};

        const TABLE: TableDefinition<&str, &[u8]> = TableDefinition::new("kv");

        let tmpfile = tempfile::NamedTempFile::new().unwrap();
        let db = RedbDatabase::create(tmpfile.path()).unwrap();

        // Populate
        {
            let write_txn = db.begin_write().unwrap();
            {
                let mut table = write_txn.open_table(TABLE).unwrap();
                for (i, key) in keys.iter().enumerate() {
                    table.insert(key.as_str(), values[i].as_slice()).unwrap();
                }
            }
            write_txn.commit().unwrap();
        }

        group.bench_function("redb", |b| {
            let mut seed = BENCH_SEED;
            b.iter(|| {
                let idx = (lcg_next(&mut seed) as usize) % 10000;
                let read_txn = db.begin_read().unwrap();
                let table = read_txn.open_table(TABLE).unwrap();
                black_box(table.get(keys[idx].as_str()).unwrap())
            });
        });
    }

    // === LMDB via heed (if available) ===
    #[cfg(feature = "comparison-benchmarks")]
    {
        use heed::{Database as HeedDatabase, EnvOpenOptions};
        use heed::types::{Str, Bytes};

        let tmpdir = tempfile::tempdir().unwrap();
        let env = unsafe {
            EnvOpenOptions::new()
                .map_size(1024 * 1024 * 1024) // 1GB
                .open(tmpdir.path())
                .unwrap()
        };
        let mut wtxn = env.write_txn().unwrap();
        let db: HeedDatabase<Str, Bytes> = env.create_database(&mut wtxn, None).unwrap();

        // Populate
        for (i, key) in keys.iter().enumerate() {
            db.put(&mut wtxn, key, &values[i]).unwrap();
        }
        wtxn.commit().unwrap();

        group.bench_function("lmdb_heed", |b| {
            let mut seed = BENCH_SEED;
            b.iter(|| {
                let idx = (lcg_next(&mut seed) as usize) % 10000;
                let rtxn = env.read_txn().unwrap();
                let result = db.get(&rtxn, &keys[idx]).unwrap();
                // Clone the result to avoid lifetime issues
                let cloned = result.map(|v| v.to_vec());
                drop(rtxn);
                black_box(cloned)
            });
        });
    }

    group.finish();
}

/// Compare KVStore point writes against redb and LMDB
fn kv_comparison_write(c: &mut Criterion) {
    let mut group = c.benchmark_group("kv_comparison/write");
    group.measurement_time(Duration::from_secs(10));

    let values = pregenerate_values(1, 100)[0].clone();

    // === in-mem KVStore ===
    {
        let db = Arc::new(Database::builder().in_memory().open_temp().unwrap());
        let kv = KVStore::new(Arc::clone(&db));
        let run_id = RunId::new();

        let mut counter = 0u64;
        group.bench_function("inmem_kvstore", |b| {
            b.iter(|| {
                counter += 1;
                let key = format!("write_key_{}", counter);
                black_box(kv.put(&run_id, &key, Value::Bytes(values.clone())).expect("put"))
            });
        });
    }

    // === redb ===
    #[cfg(feature = "comparison-benchmarks")]
    {
        use redb::{Database as RedbDatabase, TableDefinition};

        const TABLE: TableDefinition<&str, &[u8]> = TableDefinition::new("kv");

        let tmpfile = tempfile::NamedTempFile::new().unwrap();
        let db = RedbDatabase::create(tmpfile.path()).unwrap();

        // Create table
        {
            let write_txn = db.begin_write().unwrap();
            write_txn.open_table(TABLE).unwrap();
            write_txn.commit().unwrap();
        }

        let mut counter = 0u64;
        group.bench_function("redb", |b| {
            b.iter(|| {
                counter += 1;
                let key = format!("write_key_{}", counter);
                let write_txn = db.begin_write().unwrap();
                {
                    let mut table = write_txn.open_table(TABLE).unwrap();
                    table.insert(key.as_str(), values.as_slice()).unwrap();
                }
                write_txn.commit().unwrap();
            });
        });
    }

    // === LMDB via heed ===
    #[cfg(feature = "comparison-benchmarks")]
    {
        use heed::{Database as HeedDatabase, EnvOpenOptions};
        use heed::types::{Str, Bytes};

        let tmpdir = tempfile::tempdir().unwrap();
        let env = unsafe {
            EnvOpenOptions::new()
                .map_size(1024 * 1024 * 1024)
                .open(tmpdir.path())
                .unwrap()
        };
        let mut wtxn = env.write_txn().unwrap();
        let db: HeedDatabase<Str, Bytes> = env.create_database(&mut wtxn, None).unwrap();
        wtxn.commit().unwrap();

        let mut counter = 0u64;
        group.bench_function("lmdb_heed", |b| {
            b.iter(|| {
                counter += 1;
                let key = format!("write_key_{}", counter);
                let mut wtxn = env.write_txn().unwrap();
                db.put(&mut wtxn, &key, &values).unwrap();
                wtxn.commit().unwrap();
            });
        });
    }

    group.finish();
}

/// Compare KVStore batch writes
fn kv_comparison_batch_write(c: &mut Criterion) {
    let mut group = c.benchmark_group("kv_comparison/batch_write");
    group.measurement_time(Duration::from_secs(10));
    group.throughput(Throughput::Elements(100));

    let keys = pregenerate_keys(100);
    let values = pregenerate_values(100, 100);

    // === in-mem KVStore ===
    {
        let db = Arc::new(Database::builder().in_memory().open_temp().unwrap());
        let kv = KVStore::new(Arc::clone(&db));
        let run_id = RunId::new();

        let mut batch = 0u64;
        group.bench_function("inmem_kvstore", |b| {
            b.iter(|| {
                batch += 1;
                for (i, key) in keys.iter().enumerate() {
                    let unique_key = format!("{}_{}", key, batch);
                    kv.put(&run_id, &unique_key, Value::Bytes(values[i].clone()))
                        .expect("put");
                }
            });
        });
    }

    // === redb (batch in single transaction) ===
    #[cfg(feature = "comparison-benchmarks")]
    {
        use redb::{Database as RedbDatabase, TableDefinition};

        const TABLE: TableDefinition<&str, &[u8]> = TableDefinition::new("kv");

        let tmpfile = tempfile::NamedTempFile::new().unwrap();
        let db = RedbDatabase::create(tmpfile.path()).unwrap();

        {
            let write_txn = db.begin_write().unwrap();
            write_txn.open_table(TABLE).unwrap();
            write_txn.commit().unwrap();
        }

        let mut batch = 0u64;
        group.bench_function("redb", |b| {
            b.iter(|| {
                batch += 1;
                let write_txn = db.begin_write().unwrap();
                {
                    let mut table = write_txn.open_table(TABLE).unwrap();
                    for (i, key) in keys.iter().enumerate() {
                        let unique_key = format!("{}_{}", key, batch);
                        table.insert(unique_key.as_str(), values[i].as_slice()).unwrap();
                    }
                }
                write_txn.commit().unwrap();
            });
        });
    }

    // === LMDB via heed ===
    #[cfg(feature = "comparison-benchmarks")]
    {
        use heed::{Database as HeedDatabase, EnvOpenOptions};
        use heed::types::{Str, Bytes};

        let tmpdir = tempfile::tempdir().unwrap();
        let env = unsafe {
            EnvOpenOptions::new()
                .map_size(1024 * 1024 * 1024)
                .open(tmpdir.path())
                .unwrap()
        };
        let mut wtxn = env.write_txn().unwrap();
        let db: HeedDatabase<Str, Bytes> = env.create_database(&mut wtxn, None).unwrap();
        wtxn.commit().unwrap();

        let mut batch = 0u64;
        group.bench_function("lmdb_heed", |b| {
            b.iter(|| {
                batch += 1;
                let mut wtxn = env.write_txn().unwrap();
                for (i, key) in keys.iter().enumerate() {
                    let unique_key = format!("{}_{}", key, batch);
                    db.put(&mut wtxn, &unique_key, &values[i]).unwrap();
                }
                wtxn.commit().unwrap();
            });
        });
    }

    group.finish();
}

// ============================================================================
// JsonStore Comparison: in-mem vs SQLite JSON1
// ============================================================================

/// Compare JsonStore document insert against SQLite
fn json_comparison_insert(c: &mut Criterion) {
    let mut group = c.benchmark_group("json_comparison/insert");
    group.measurement_time(Duration::from_secs(10));

    use in_mem_core::json::JsonValue;
    use in_mem_core::types::JsonDocId;

    // Create test documents
    let docs: Vec<serde_json::Value> = (0..100)
        .map(|i| {
            serde_json::json!({
                "id": i,
                "name": format!("user_{}", i),
                "email": format!("user{}@example.com", i),
                "active": i % 2 == 0,
                "score": i as f64 * 1.5,
                "tags": ["tag1", "tag2", "tag3"],
                "metadata": {
                    "created_at": "2025-01-15T10:00:00Z",
                    "updated_at": "2025-01-16T15:30:00Z"
                }
            })
        })
        .collect();

    // === in-mem JsonStore ===
    {
        let db = Arc::new(Database::builder().in_memory().open_temp().unwrap());
        let json_store = JsonStore::new(Arc::clone(&db));
        let run_id = RunId::new();

        let mut counter = 0usize;
        group.bench_function("inmem_jsonstore", |b| {
            b.iter(|| {
                let doc_id = JsonDocId::new();
                let doc = JsonValue::from_value(docs[counter % 100].clone());
                counter += 1;
                black_box(json_store.create(&run_id, &doc_id, doc).expect("create"))
            });
        });
    }

    // === SQLite with JSON1 ===
    #[cfg(feature = "comparison-benchmarks")]
    {
        use rusqlite::{Connection, params};

        let conn = Connection::open_in_memory().unwrap();
        conn.execute(
            "CREATE TABLE documents (
                id TEXT PRIMARY KEY,
                data JSON NOT NULL
            )",
            [],
        )
        .unwrap();

        let mut counter = 0usize;
        group.bench_function("sqlite_json1", |b| {
            b.iter(|| {
                let id = uuid::Uuid::new_v4().to_string();
                let json = serde_json::to_string(&docs[counter % 100]).unwrap();
                counter += 1;
                conn.execute(
                    "INSERT INTO documents (id, data) VALUES (?1, json(?2))",
                    params![id, json],
                )
                .unwrap();
            });
        });
    }

    group.finish();
}

/// Compare JsonStore document read against SQLite
fn json_comparison_read(c: &mut Criterion) {
    let mut group = c.benchmark_group("json_comparison/read");
    group.measurement_time(Duration::from_secs(10));

    use in_mem_core::json::JsonValue;
    use in_mem_core::types::JsonDocId;

    // Create and store test documents
    let docs: Vec<serde_json::Value> = (0..1000)
        .map(|i| {
            serde_json::json!({
                "id": i,
                "name": format!("user_{}", i),
                "email": format!("user{}@example.com", i),
                "active": i % 2 == 0,
                "score": i as f64 * 1.5,
            })
        })
        .collect();

    // === in-mem JsonStore ===
    {
        let db = Arc::new(Database::builder().in_memory().open_temp().unwrap());
        let json_store = JsonStore::new(Arc::clone(&db));
        let run_id = RunId::new();

        // Populate and collect doc_ids
        let doc_ids: Vec<JsonDocId> = docs
            .iter()
            .map(|doc| {
                let doc_id = JsonDocId::new();
                json_store
                    .create(&run_id, &doc_id, JsonValue::from_value(doc.clone()))
                    .expect("create");
                doc_id
            })
            .collect();

        let mut seed = BENCH_SEED;
        group.bench_function("inmem_jsonstore", |b| {
            b.iter(|| {
                let idx = (lcg_next(&mut seed) as usize) % 1000;
                black_box(json_store.get_doc(&run_id, &doc_ids[idx]).expect("get"))
            });
        });
    }

    // === SQLite with JSON1 ===
    #[cfg(feature = "comparison-benchmarks")]
    {
        use rusqlite::{Connection, params};

        let conn = Connection::open_in_memory().unwrap();
        conn.execute(
            "CREATE TABLE documents (
                id TEXT PRIMARY KEY,
                data JSON NOT NULL
            )",
            [],
        )
        .unwrap();

        // Populate
        let ids: Vec<String> = docs
            .iter()
            .map(|doc| {
                let id = uuid::Uuid::new_v4().to_string();
                let json = serde_json::to_string(doc).unwrap();
                conn.execute(
                    "INSERT INTO documents (id, data) VALUES (?1, json(?2))",
                    params![id, json],
                )
                .unwrap();
                id
            })
            .collect();

        let mut seed = BENCH_SEED;
        group.bench_function("sqlite_json1", |b| {
            b.iter(|| {
                let idx = (lcg_next(&mut seed) as usize) % 1000;
                let mut stmt = conn.prepare_cached("SELECT data FROM documents WHERE id = ?1").unwrap();
                black_box(stmt.query_row(params![ids[idx]], |row| {
                    row.get::<_, String>(0)
                }).unwrap())
            });
        });
    }

    group.finish();
}

/// Compare JsonStore JSON path query against SQLite
fn json_comparison_query(c: &mut Criterion) {
    let mut group = c.benchmark_group("json_comparison/query_by_field");
    group.measurement_time(Duration::from_secs(10));

    use in_mem_core::json::JsonValue;
    use in_mem_core::types::JsonDocId;

    // Create test documents with varying scores
    let docs: Vec<serde_json::Value> = (0..1000)
        .map(|i| {
            serde_json::json!({
                "id": i,
                "name": format!("user_{}", i),
                "active": i % 2 == 0,
                "score": (i % 100) as f64,
            })
        })
        .collect();

    // === in-mem JsonStore (scan-based) ===
    {
        let db = Arc::new(Database::builder().in_memory().open_temp().unwrap());
        let json_store = JsonStore::new(Arc::clone(&db));
        let run_id = RunId::new();

        // Populate and track doc_ids
        let doc_ids: Vec<JsonDocId> = docs
            .iter()
            .map(|doc| {
                let doc_id = JsonDocId::new();
                json_store
                    .create(&run_id, &doc_id, JsonValue::from_value(doc.clone()))
                    .expect("create");
                doc_id
            })
            .collect();

        // in-mem doesn't have native field query - needs to scan and filter
        // This measures sequential scan cost
        group.bench_function("inmem_jsonstore/scan_filter", |b| {
            let mut seed = BENCH_SEED;
            b.iter(|| {
                let target_score = (lcg_next(&mut seed) % 100) as f64;
                // Simulate scan by reading all docs and filtering
                let results: Vec<_> = doc_ids
                    .iter()
                    .filter_map(|id| json_store.get_doc(&run_id, id).ok().flatten())
                    .filter(|doc| {
                        // Check if score matches (scan filter)
                        doc.value
                            .as_inner()
                            .get("score")
                            .and_then(|v| v.as_f64())
                            .map(|s| (s - target_score).abs() < 0.001)
                            .unwrap_or(false)
                    })
                    .collect();
                black_box(results)
            });
        });
    }

    // === SQLite with JSON1 (indexed query) ===
    #[cfg(feature = "comparison-benchmarks")]
    {
        use rusqlite::{Connection, params};

        let conn = Connection::open_in_memory().unwrap();
        conn.execute(
            "CREATE TABLE documents (
                id TEXT PRIMARY KEY,
                data JSON NOT NULL
            )",
            [],
        )
        .unwrap();

        // Create index on score field
        conn.execute(
            "CREATE INDEX idx_score ON documents(json_extract(data, '$.score'))",
            [],
        )
        .unwrap();

        // Populate
        for doc in &docs {
            let id = uuid::Uuid::new_v4().to_string();
            let json = serde_json::to_string(doc).unwrap();
            conn.execute(
                "INSERT INTO documents (id, data) VALUES (?1, json(?2))",
                params![id, json],
            )
            .unwrap();
        }

        let mut seed = BENCH_SEED;
        group.bench_function("sqlite_json1/indexed", |b| {
            b.iter(|| {
                let target_score = (lcg_next(&mut seed) % 100) as f64;
                let mut stmt = conn
                    .prepare_cached("SELECT data FROM documents WHERE json_extract(data, '$.score') = ?1")
                    .unwrap();
                let results: Vec<String> = stmt
                    .query_map(params![target_score], |row| row.get(0))
                    .unwrap()
                    .filter_map(|r| r.ok())
                    .collect();
                black_box(results)
            });
        });
    }

    group.finish();
}

// ============================================================================
// VectorStore Comparison: in-mem vs USearch
// ============================================================================

#[cfg(feature = "comparison-benchmarks")]
fn vector_comparison_insert(c: &mut Criterion) {
    let mut group = c.benchmark_group("vector_comparison/insert");
    group.measurement_time(Duration::from_secs(10));

    const DIMENSION: usize = 128;

    // Pre-generate vectors
    let vectors: Vec<Vec<f32>> = (0..1000)
        .map(|i| random_normalized_vector(DIMENSION, BENCH_SEED + i as u64))
        .collect();

    // === in-mem VectorStore ===
    {
        let db = Arc::new(Database::builder().in_memory().open_temp().unwrap());
        let vector_store = VectorStore::new(Arc::clone(&db));
        let run_id = RunId::new();

        let config = VectorConfig::new(DIMENSION, DistanceMetric::Cosine)
            .expect("valid config");
        vector_store.create_collection(run_id, "test", config).expect("create collection");

        let mut counter = 0usize;
        group.bench_function("inmem_vectorstore", |b| {
            b.iter(|| {
                let key = format!("vec_{}", counter);
                counter += 1;
                black_box(
                    vector_store
                        .insert(run_id, "test", &key, &vectors[counter % 1000], None)
                        .expect("insert"),
                )
            });
        });
    }

    // === USearch ===
    // Note: USearch comparison disabled by default due to build complexity
    // Enable with: cargo bench --features=comparison-benchmarks,usearch-enabled
    #[cfg(feature = "usearch-enabled")]
    {
        use usearch::{Index, IndexOptions, MetricKind, ScalarKind};

        let options = IndexOptions {
            dimensions: DIMENSION,
            metric: MetricKind::Cos,
            quantization: ScalarKind::F32,
            connectivity: 16,
            expansion_add: 128,
            expansion_search: 64,
            ..Default::default()
        };
        let index = Index::new(&options).unwrap();
        index.reserve(10000).unwrap();

        let mut counter = 0u64;
        group.bench_function("usearch", |b| {
            b.iter(|| {
                counter += 1;
                black_box(index.add(counter, &vectors[(counter as usize) % 1000]).unwrap())
            });
        });
    }

    group.finish();
}

#[cfg(feature = "comparison-benchmarks")]
fn vector_comparison_search(c: &mut Criterion) {
    let mut group = c.benchmark_group("vector_comparison/search_k10");
    group.measurement_time(Duration::from_secs(10));

    const DIMENSION: usize = 128;
    const COLLECTION_SIZE: usize = 10000;
    const K: usize = 10;

    // Pre-generate vectors
    let vectors: Vec<Vec<f32>> = (0..COLLECTION_SIZE)
        .map(|i| random_normalized_vector(DIMENSION, BENCH_SEED + i as u64))
        .collect();

    // Query vectors
    let queries: Vec<Vec<f32>> = (0..100)
        .map(|i| random_normalized_vector(DIMENSION, BENCH_SEED + 100000 + i as u64))
        .collect();

    // === in-mem VectorStore ===
    {
        let db = Arc::new(Database::builder().in_memory().open_temp().unwrap());
        let vector_store = VectorStore::new(Arc::clone(&db));
        let run_id = RunId::new();

        let config = VectorConfig::new(DIMENSION, DistanceMetric::Cosine)
            .expect("valid config");
        vector_store.create_collection(run_id, "test", config).expect("create collection");

        // Populate
        for (i, vec) in vectors.iter().enumerate() {
            let key = format!("vec_{}", i);
            vector_store.insert(run_id, "test", &key, vec, None).expect("insert");
        }

        let mut seed = BENCH_SEED;
        group.bench_function(
            BenchmarkId::new("inmem_vectorstore", format!("n={}/k={}", COLLECTION_SIZE, K)),
            |b| {
                b.iter(|| {
                    let idx = (lcg_next(&mut seed) as usize) % queries.len();
                    black_box(
                        vector_store
                            .search(run_id, "test", &queries[idx], K, None)
                            .expect("search"),
                    )
                });
            },
        );
    }

    // === USearch ===
    // Note: USearch comparison disabled by default due to build complexity
    // Enable with: cargo bench --features=comparison-benchmarks,usearch-enabled
    #[cfg(feature = "usearch-enabled")]
    {
        use usearch::{Index, IndexOptions, MetricKind, ScalarKind};

        let options = IndexOptions {
            dimensions: DIMENSION,
            metric: MetricKind::Cos,
            quantization: ScalarKind::F32,
            connectivity: 16,
            expansion_add: 128,
            expansion_search: 64,
            ..Default::default()
        };
        let index = Index::new(&options).unwrap();
        index.reserve(COLLECTION_SIZE).unwrap();

        // Populate
        for (i, vec) in vectors.iter().enumerate() {
            index.add(i as u64, vec).unwrap();
        }

        let mut seed = BENCH_SEED;
        group.bench_function(
            BenchmarkId::new("usearch", format!("n={}/k={}", COLLECTION_SIZE, K)),
            |b| {
                b.iter(|| {
                    let idx = (lcg_next(&mut seed) as usize) % queries.len();
                    black_box(index.search(&queries[idx], K).unwrap())
                });
            },
        );
    }

    group.finish();
}

// ============================================================================
// EventLog Comparison: in-mem vs append-only file baseline
// ============================================================================

fn eventlog_comparison_append(c: &mut Criterion) {
    let mut group = c.benchmark_group("eventlog_comparison/append");
    group.measurement_time(Duration::from_secs(10));

    // Create event payload
    let payload = "event_payload_with_some_reasonable_size_for_testing";

    // === in-mem EventLog ===
    {
        let db = Arc::new(Database::builder().in_memory().open_temp().unwrap());
        let event_log = EventLog::new(Arc::clone(&db));
        let run_id = RunId::new();

        let mut counter = 0u64;
        group.bench_function("inmem_eventlog", |b| {
            b.iter(|| {
                counter += 1;
                let topic = format!("topic_{}", counter % 10);
                black_box(
                    event_log
                        .append(&run_id, &topic, Value::String(payload.to_string()))
                        .expect("append"),
                )
            });
        });
    }

    // === Simple file append baseline ===
    {
        use std::fs::OpenOptions;
        use std::io::Write;

        let tmpfile = tempfile::NamedTempFile::new().unwrap();
        let path = tmpfile.path().to_path_buf();

        let mut counter = 0u64;
        group.bench_function("file_append/no_sync", |b| {
            let mut file = OpenOptions::new()
                .create(true)
                .append(true)
                .open(&path)
                .unwrap();

            b.iter(|| {
                counter += 1;
                let line = format!("{},{}\n", counter, payload);
                black_box(file.write_all(line.as_bytes()).unwrap())
            });
        });
    }

    // === File append with fsync ===
    {
        use std::fs::OpenOptions;
        use std::io::Write;

        let tmpfile = tempfile::NamedTempFile::new().unwrap();
        let path = tmpfile.path().to_path_buf();

        let mut counter = 0u64;
        group.bench_function("file_append/with_sync", |b| {
            let mut file = OpenOptions::new()
                .create(true)
                .append(true)
                .open(&path)
                .unwrap();

            b.iter(|| {
                counter += 1;
                let line = format!("{},{}\n", counter, payload);
                file.write_all(line.as_bytes()).unwrap();
                black_box(file.sync_all().unwrap())
            });
        });
    }

    group.finish();
}

fn eventlog_comparison_throughput(c: &mut Criterion) {
    let mut group = c.benchmark_group("eventlog_comparison/batch_throughput");
    group.measurement_time(Duration::from_secs(10));
    group.throughput(Throughput::Elements(1000));

    let payload = "event_payload_data";

    // === in-mem EventLog (1000 events) ===
    {
        let db = Arc::new(Database::builder().in_memory().open_temp().unwrap());
        let event_log = EventLog::new(Arc::clone(&db));
        let run_id = RunId::new();

        let mut batch = 0u64;
        group.bench_function("inmem_eventlog/1000_events", |b| {
            b.iter(|| {
                batch += 1;
                for i in 0..1000 {
                    let topic = format!("topic_{}", (batch * 1000 + i) % 10);
                    event_log
                        .append(&run_id, &topic, Value::String(payload.to_string()))
                        .expect("append");
                }
            });
        });
    }

    // === Buffered file write (1000 events) ===
    {
        use std::fs::OpenOptions;
        use std::io::{BufWriter, Write};

        let tmpfile = tempfile::NamedTempFile::new().unwrap();
        let path = tmpfile.path().to_path_buf();

        let mut batch = 0u64;
        group.bench_function("buffered_file/1000_events", |b| {
            b.iter(|| {
                batch += 1;
                let file = OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(&path)
                    .unwrap();
                let mut writer = BufWriter::new(file);

                for i in 0..1000 {
                    let line = format!("{},{}\n", batch * 1000 + i, payload);
                    writer.write_all(line.as_bytes()).unwrap();
                }
                writer.flush().unwrap();
            });
        });
    }

    group.finish();
}

// ============================================================================
// StateCell Comparison: in-mem vs redb
// ============================================================================

fn statecell_comparison_read_write(c: &mut Criterion) {
    let mut group = c.benchmark_group("statecell_comparison");
    group.measurement_time(Duration::from_secs(10));

    // === in-mem StateCell Read ===
    {
        let db = Arc::new(Database::builder().in_memory().open_temp().unwrap());
        let state = StateCell::new(Arc::clone(&db));
        let run_id = RunId::new();

        // Initialize state
        state.set(&run_id, "counter", Value::I64(0)).expect("init");

        group.bench_function("inmem_statecell/read", |b| {
            b.iter(|| black_box(state.read(&run_id, "counter").expect("read")));
        });
    }

    // === in-mem StateCell Write ===
    {
        let db = Arc::new(Database::builder().in_memory().open_temp().unwrap());
        let state = StateCell::new(Arc::clone(&db));
        let run_id = RunId::new();

        let mut counter = 0i64;
        group.bench_function("inmem_statecell/write", |b| {
            b.iter(|| {
                counter += 1;
                black_box(state.set(&run_id, "counter", Value::I64(counter)).expect("set"))
            });
        });
    }

    // === redb for state (if available) ===
    #[cfg(feature = "comparison-benchmarks")]
    {
        use redb::{Database as RedbDatabase, TableDefinition};

        const TABLE: TableDefinition<&str, i64> = TableDefinition::new("state");

        let tmpfile = tempfile::NamedTempFile::new().unwrap();
        let db = RedbDatabase::create(tmpfile.path()).unwrap();

        // Initialize
        {
            let write_txn = db.begin_write().unwrap();
            {
                let mut table = write_txn.open_table(TABLE).unwrap();
                table.insert("counter", 0i64).unwrap();
            }
            write_txn.commit().unwrap();
        }

        group.bench_function("redb/read", |b| {
            b.iter(|| {
                let read_txn = db.begin_read().unwrap();
                let table = read_txn.open_table(TABLE).unwrap();
                black_box(table.get("counter").unwrap())
            });
        });

        let tmpfile2 = tempfile::NamedTempFile::new().unwrap();
        let db2 = RedbDatabase::create(tmpfile2.path()).unwrap();

        {
            let write_txn = db2.begin_write().unwrap();
            write_txn.open_table(TABLE).unwrap();
            write_txn.commit().unwrap();
        }

        let mut counter = 0i64;
        group.bench_function("redb/write", |b| {
            b.iter(|| {
                counter += 1;
                let write_txn = db2.begin_write().unwrap();
                {
                    let mut table = write_txn.open_table(TABLE).unwrap();
                    table.insert("counter", counter).unwrap();
                }
                black_box(write_txn.commit().unwrap())
            });
        });
    }

    group.finish();
}

// ============================================================================
// Concurrency Comparison
// ============================================================================

fn concurrency_comparison_readers(c: &mut Criterion) {
    let mut group = c.benchmark_group("concurrency_comparison/parallel_reads");
    group.measurement_time(Duration::from_secs(10));

    let keys = pregenerate_keys(10000);
    let values = pregenerate_values(10000, 100);

    // === in-mem KVStore (parallel reads) ===
    {
        let db = Arc::new(Database::builder().in_memory().open_temp().unwrap());
        let kv = KVStore::new(Arc::clone(&db));
        let run_id = RunId::new();

        // Populate
        for (i, key) in keys.iter().enumerate() {
            kv.put(&run_id, key, Value::Bytes(values[i].clone())).expect("put");
        }

        for num_threads in [1, 2, 4, 8] {
            group.bench_function(
                BenchmarkId::new("inmem_kvstore", format!("{}t", num_threads)),
                |b| {
                    b.iter(|| {
                        std::thread::scope(|s| {
                            for t in 0..num_threads {
                                let kv_ref = &kv;
                                let keys_ref = &keys;
                                let run_id_ref = &run_id;
                                s.spawn(move || {
                                    let mut seed = BENCH_SEED + t as u64;
                                    for _ in 0..100 {
                                        let idx = (lcg_next(&mut seed) as usize) % 10000;
                                        black_box(kv_ref.get(run_id_ref, &keys_ref[idx]).expect("get"));
                                    }
                                });
                            }
                        });
                    });
                },
            );
        }
    }

    // === LMDB parallel reads (if available) ===
    #[cfg(feature = "comparison-benchmarks")]
    {
        use heed::{Database as HeedDatabase, EnvOpenOptions};
        use heed::types::{Str, Bytes};

        let tmpdir = tempfile::tempdir().unwrap();
        let env = Arc::new(unsafe {
            EnvOpenOptions::new()
                .map_size(1024 * 1024 * 1024)
                .max_readers(32)
                .open(tmpdir.path())
                .unwrap()
        });

        let mut wtxn = env.write_txn().unwrap();
        let db: HeedDatabase<Str, Bytes> = env.create_database(&mut wtxn, None).unwrap();
        for (i, key) in keys.iter().enumerate() {
            db.put(&mut wtxn, key, &values[i]).unwrap();
        }
        wtxn.commit().unwrap();

        for num_threads in [1, 2, 4, 8] {
            let env_clone = Arc::clone(&env);
            group.bench_function(
                BenchmarkId::new("lmdb_heed", format!("{}t", num_threads)),
                |b| {
                    b.iter(|| {
                        std::thread::scope(|s| {
                            for t in 0..num_threads {
                                let env_ref = Arc::clone(&env_clone);
                                let keys_ref = &keys;
                                s.spawn(move || {
                                    let mut seed = BENCH_SEED + t as u64;
                                    for _ in 0..100 {
                                        let idx = (lcg_next(&mut seed) as usize) % 10000;
                                        let rtxn = env_ref.read_txn().unwrap();
                                        // Need to reconstruct db handle in each read
                                        let db: HeedDatabase<Str, Bytes> =
                                            env_ref.open_database(&rtxn, None).unwrap().unwrap();
                                        black_box(db.get(&rtxn, &keys_ref[idx]).unwrap());
                                    }
                                });
                            }
                        });
                    });
                },
            );
        }
    }

    group.finish();
}

// ============================================================================
// Summary Benchmark (Key Operations Comparison)
// ============================================================================

fn summary_comparison(c: &mut Criterion) {
    let mut group = c.benchmark_group("summary_comparison");
    group.measurement_time(Duration::from_secs(5));

    // This benchmark provides a quick summary comparison of key operations

    eprintln!("\n");
    eprintln!("╔══════════════════════════════════════════════════════════════╗");
    eprintln!("║           INDUSTRY COMPARISON BENCHMARK SUMMARY              ║");
    eprintln!("╠══════════════════════════════════════════════════════════════╣");
    eprintln!("║ Primitive      │ in-mem         │ SOTA Comparison           ║");
    eprintln!("╠════════════════╪════════════════╪═══════════════════════════╣");
    eprintln!("║ KVStore        │ KVStore        │ redb, LMDB (heed)         ║");
    eprintln!("║ JsonStore      │ JsonStore      │ SQLite + JSON1            ║");
    eprintln!("║ VectorStore    │ VectorStore    │ USearch                   ║");
    eprintln!("║ EventLog       │ EventLog       │ File append baseline      ║");
    eprintln!("║ StateCell      │ StateCell      │ redb                      ║");
    eprintln!("╚════════════════╧════════════════╧═══════════════════════════╝");
    eprintln!("\n");
    eprintln!("Run with --features=comparison-benchmarks to enable SOTA comparisons");
    eprintln!("\n");

    // Simple baseline benchmark for summary
    group.bench_function("baseline_noop", |b| {
        b.iter(|| black_box(1 + 1));
    });

    group.finish();
}

// ============================================================================
// Criterion Groups
// ============================================================================

// Base benchmarks (always available)
criterion_group!(
    name = base_benchmarks;
    config = Criterion::default().sample_size(50);
    targets =
        kv_comparison_read,
        kv_comparison_write,
        kv_comparison_batch_write,
        json_comparison_insert,
        json_comparison_read,
        json_comparison_query,
        eventlog_comparison_append,
        eventlog_comparison_throughput,
        statecell_comparison_read_write,
        concurrency_comparison_readers,
        summary_comparison,
);

// Vector comparison benchmarks (only with feature flag)
#[cfg(feature = "comparison-benchmarks")]
criterion_group!(
    name = vector_benchmarks;
    config = Criterion::default().sample_size(50);
    targets =
        vector_comparison_insert,
        vector_comparison_search,
);

#[cfg(feature = "comparison-benchmarks")]
criterion_main!(base_benchmarks, vector_benchmarks);

#[cfg(not(feature = "comparison-benchmarks"))]
criterion_main!(base_benchmarks);
