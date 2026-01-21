//! M6 Search Performance Benchmarks
//!
//! Run with: cargo bench --bench m6_search
//!
//! These benchmarks follow the established taxonomy with explicit labels for:
//! - Layer (search_*, hybrid_*, index_*)
//! - Access pattern (hot_query, uniform, working_set)
//! - Document count (small, medium, large datasets)
//! - Index state (enabled, disabled)
//!
//! Search Performance Targets:
//! - search_kv/hot_query: < 100µs (small dataset)
//! - search_hybrid/uniform: < 500µs (medium dataset)
//! - index_lookup: < 10µs
//! - bm25_score: < 1µs per document
//!
//! Non-Regression Targets (M5):
//! - json_get/hot_doc: < 10µs
//! - kv_put InMemory: < 3µs

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use strata_core::search_types::{PrimitiveKind, SearchBudget, SearchRequest};
use strata_core::types::RunId;
use strata_core::value::Value;
use strata_engine::Database;
use strata_primitives::{EventLog, KVStore};
use strata_search::{DatabaseSearchExt, InvertedIndex};
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

/// Pre-generate search queries
fn pregenerate_queries(count: usize) -> Vec<String> {
    let words = [
        "searchable",
        "content",
        "data",
        "test",
        "benchmark",
        "value",
        "important",
        "quick",
    ];
    (0..count)
        .map(|i| words[i % words.len()].to_string())
        .collect()
}

/// Populate database with searchable content
fn populate_searchable_data(db: &Arc<Database>, run_id: &RunId, count: usize) {
    let kv = KVStore::new(db.clone());
    for i in 0..count {
        let content = if i % 10 == 0 {
            format!("important searchable content item {}", i)
        } else if i % 5 == 0 {
            format!("quick test data entry {}", i)
        } else {
            format!("regular content value {}", i)
        };
        kv.put(run_id, &format!("doc_{}", i), Value::String(content))
            .unwrap();
    }
}

/// Populate database with events
fn populate_events(db: &Arc<Database>, run_id: &RunId, count: usize) {
    let events = EventLog::new(db.clone());
    for i in 0..count {
        let event_type = if i % 3 == 0 {
            "error"
        } else if i % 2 == 0 {
            "warning"
        } else {
            "info"
        };
        let payload = Value::String(format!("Event {} with searchable content", i));
        events.append(run_id, event_type, payload).unwrap();
    }
}

// ============================================================================
// search_kv - KV Store Search Benchmarks
// ============================================================================

fn search_kv_by_dataset_size(c: &mut Criterion) {
    let mut group = c.benchmark_group("search_kv");
    group.measurement_time(Duration::from_secs(5));

    // --- Benchmark: search_kv/small ---
    // Semantic: Search through small dataset
    // Real pattern: Agent with limited state
    for doc_count in [100, 1000, 10000] {
        let label = match doc_count {
            100 => "small",
            1000 => "medium",
            10000 => "large",
            _ => "custom",
        };

        group.throughput(Throughput::Elements(doc_count as u64));
        group.bench_with_input(
            BenchmarkId::new(label, doc_count),
            &doc_count,
            |b, &doc_count| {
                let db = Arc::new(Database::builder().in_memory().open_temp().unwrap());
                let run_id = RunId::new();

                populate_searchable_data(&db, &run_id, doc_count);

                let hybrid = db.hybrid();
                let req = SearchRequest::new(run_id.clone(), "searchable").with_k(10);

                b.iter(|| hybrid.search(&req).unwrap());
            },
        );
    }

    group.finish();
}

fn search_kv_by_access_pattern(c: &mut Criterion) {
    let mut group = c.benchmark_group("search_kv");
    group.measurement_time(Duration::from_secs(5));

    let doc_count = 1000;

    // --- Benchmark: search_kv/hot_query ---
    // Semantic: Same query repeated
    // Real pattern: Repeated lookups
    group.bench_function("hot_query", |b| {
        let db = Arc::new(Database::builder().in_memory().open_temp().unwrap());
        let run_id = RunId::new();

        populate_searchable_data(&db, &run_id, doc_count);

        let hybrid = db.hybrid();
        let req = SearchRequest::new(run_id.clone(), "important").with_k(10);

        b.iter(|| hybrid.search(&req).unwrap());
    });

    // --- Benchmark: search_kv/uniform ---
    // Semantic: Random queries
    // Real pattern: Diverse search patterns
    group.bench_function("uniform", |b| {
        let db = Arc::new(Database::builder().in_memory().open_temp().unwrap());
        let run_id = RunId::new();

        populate_searchable_data(&db, &run_id, doc_count);

        let hybrid = db.hybrid();
        let queries = pregenerate_queries(100);
        let mut rng_state = BENCH_SEED;

        b.iter(|| {
            let idx = (lcg_next(&mut rng_state) as usize) % queries.len();
            let req = SearchRequest::new(run_id.clone(), &queries[idx]).with_k(10);
            hybrid.search(&req).unwrap()
        });
    });

    group.finish();
}

// ============================================================================
// search_hybrid - Hybrid Search Benchmarks
// ============================================================================

fn search_hybrid_benchmarks(c: &mut Criterion) {
    let mut group = c.benchmark_group("search_hybrid");
    group.measurement_time(Duration::from_secs(5));

    // --- Benchmark: search_hybrid/all_primitives ---
    // Semantic: Search across all primitive types
    // Real pattern: Agent searching entire state
    group.bench_function("all_primitives", |b| {
        let db = Arc::new(Database::builder().in_memory().open_temp().unwrap());
        let run_id = RunId::new();

        // Populate multiple primitives
        populate_searchable_data(&db, &run_id, 500);
        populate_events(&db, &run_id, 500);

        let hybrid = db.hybrid();
        let req = SearchRequest::new(run_id.clone(), "content").with_k(20);

        b.iter(|| hybrid.search(&req).unwrap());
    });

    // --- Benchmark: search_hybrid/filtered_primitives ---
    // Semantic: Search with primitive filter
    // Real pattern: Targeted search
    group.bench_function("filtered_kv_only", |b| {
        let db = Arc::new(Database::builder().in_memory().open_temp().unwrap());
        let run_id = RunId::new();

        populate_searchable_data(&db, &run_id, 500);
        populate_events(&db, &run_id, 500);

        let hybrid = db.hybrid();
        let req = SearchRequest::new(run_id.clone(), "content")
            .with_k(20)
            .with_primitive_filter(vec![PrimitiveKind::Kv]);

        b.iter(|| hybrid.search(&req).unwrap());
    });

    // --- Benchmark: search_hybrid/with_budget ---
    // Semantic: Search with time budget
    // Real pattern: Latency-sensitive search
    for budget_ms in [10u64, 50, 100] {
        group.bench_with_input(
            BenchmarkId::new("budget_ms", budget_ms),
            &budget_ms,
            |b, &budget_ms| {
                let db = Arc::new(Database::builder().in_memory().open_temp().unwrap());
                let run_id = RunId::new();

                populate_searchable_data(&db, &run_id, 5000);

                let hybrid = db.hybrid();
                let budget = SearchBudget::new(budget_ms * 1000, 10_000); // Convert ms to micros
                let req = SearchRequest::new(run_id.clone(), "searchable")
                    .with_k(50)
                    .with_budget(budget);

                b.iter(|| hybrid.search(&req).unwrap());
            },
        );
    }

    group.finish();
}

// ============================================================================
// search_result_size - Result Set Size Benchmarks
// ============================================================================

fn search_result_size_benchmarks(c: &mut Criterion) {
    let mut group = c.benchmark_group("search_result_size");
    group.measurement_time(Duration::from_secs(5));

    let doc_count = 5000;

    // --- Benchmark: search_result_size/k_* ---
    // Semantic: Different result set sizes
    // Real pattern: Pagination, top-k retrieval
    for k in [1, 10, 50, 100, 500] {
        group.bench_with_input(BenchmarkId::new("k", k), &k, |b, &k| {
            let db = Arc::new(Database::builder().in_memory().open_temp().unwrap());
            let run_id = RunId::new();

            populate_searchable_data(&db, &run_id, doc_count);

            let hybrid = db.hybrid();
            let req = SearchRequest::new(run_id.clone(), "content").with_k(k);

            b.iter(|| hybrid.search(&req).unwrap());
        });
    }

    group.finish();
}

// ============================================================================
// index_operations - Inverted Index Benchmarks
// ============================================================================

fn index_operations_benchmarks(c: &mut Criterion) {
    let mut group = c.benchmark_group("index_operations");
    group.measurement_time(Duration::from_secs(5));

    // --- Benchmark: index_operations/lookup ---
    // Semantic: Term lookup in inverted index
    // Real pattern: Index-accelerated search
    group.bench_function("lookup", |b| {
        let index = InvertedIndex::new();
        index.enable();

        let run_id = RunId::new();
        let ns = strata_core::types::Namespace::for_run(run_id);

        // Index many documents
        for i in 0..1000 {
            let doc_ref = strata_core::search_types::DocRef::Kv {
                key: strata_core::types::Key::new_kv(ns.clone(), &format!("doc_{}", i)),
            };
            let content = format!("searchable content item {}", i);
            index.index_document(&doc_ref, &content, None);
        }

        b.iter(|| index.lookup("searchable"));
    });

    // --- Benchmark: index_operations/index_document ---
    // Semantic: Add document to index
    // Real pattern: Write-time indexing cost
    group.bench_function("index_document", |b| {
        let index = InvertedIndex::new();
        index.enable();

        let run_id = RunId::new();
        let ns = strata_core::types::Namespace::for_run(run_id);
        let mut counter = 0u64;

        b.iter(|| {
            counter += 1;
            let doc_ref = strata_core::search_types::DocRef::Kv {
                key: strata_core::types::Key::new_kv(ns.clone(), &format!("doc_{}", counter)),
            };
            index.index_document(&doc_ref, "searchable test content data", None);
        });
    });

    // --- Benchmark: index_operations/compute_idf ---
    // Semantic: IDF computation
    // Real pattern: BM25 scoring component
    group.bench_function("compute_idf", |b| {
        let index = InvertedIndex::new();
        index.enable();

        let run_id = RunId::new();
        let ns = strata_core::types::Namespace::for_run(run_id);

        // Index documents with varying term frequencies
        for i in 0..1000 {
            let doc_ref = strata_core::search_types::DocRef::Kv {
                key: strata_core::types::Key::new_kv(ns.clone(), &format!("doc_{}", i)),
            };
            let content = if i % 10 == 0 {
                "rare unique special content"
            } else {
                "common searchable content"
            };
            index.index_document(&doc_ref, content, None);
        }

        b.iter(|| {
            index.compute_idf("rare");
            index.compute_idf("common");
        });
    });

    group.finish();
}

// ============================================================================
// index_scaling - Index Size Scaling Benchmarks
// ============================================================================

fn index_scaling_benchmarks(c: &mut Criterion) {
    let mut group = c.benchmark_group("index_scaling");
    group.measurement_time(Duration::from_secs(5));

    // --- Benchmark: index_scaling/lookup_by_size ---
    // Semantic: Lookup scales with index size
    // Real pattern: Large index performance
    for doc_count in [100, 1000, 10000] {
        group.bench_with_input(
            BenchmarkId::new("lookup", doc_count),
            &doc_count,
            |b, &doc_count| {
                let index = InvertedIndex::new();
                index.enable();

                let run_id = RunId::new();
                let ns = strata_core::types::Namespace::for_run(run_id);

                for i in 0..doc_count {
                    let doc_ref = strata_core::search_types::DocRef::Kv {
                        key: strata_core::types::Key::new_kv(ns.clone(), &format!("doc_{}", i)),
                    };
                    let content = format!("searchable content item {} with various words", i);
                    index.index_document(&doc_ref, &content, None);
                }

                b.iter(|| index.lookup("searchable"));
            },
        );
    }

    group.finish();
}

// ============================================================================
// search_overhead - Index Enabled vs Disabled
// ============================================================================

fn search_overhead_benchmarks(c: &mut Criterion) {
    let mut group = c.benchmark_group("search_overhead");
    group.measurement_time(Duration::from_secs(5));

    let doc_count = 1000;

    // --- Benchmark: search_overhead/index_disabled ---
    // Semantic: Search without index (full scan)
    // Real pattern: Default search behavior
    group.bench_function("index_disabled", |b| {
        let db = Arc::new(Database::builder().in_memory().open_temp().unwrap());
        let run_id = RunId::new();

        populate_searchable_data(&db, &run_id, doc_count);

        let hybrid = db.hybrid();
        let req = SearchRequest::new(run_id.clone(), "searchable").with_k(10);

        b.iter(|| hybrid.search(&req).unwrap());
    });

    // Note: Index-enabled search would require integration with Database
    // This benchmark shows the baseline without index acceleration

    group.finish();
}

// ============================================================================
// Non-Regression Benchmarks (M5 Targets)
// ============================================================================

fn kv_put_inmemory_benchmark(c: &mut Criterion) {
    let mut group = c.benchmark_group("m5_regression");
    group.measurement_time(Duration::from_secs(5));

    let db = Arc::new(Database::builder().in_memory().open_temp().unwrap());
    let kv = KVStore::new(db);
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
    let mut group = c.benchmark_group("m5_regression");
    group.measurement_time(Duration::from_secs(5));

    let db = Arc::new(Database::builder().in_memory().open_temp().unwrap());
    let kv = KVStore::new(db);
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

// ============================================================================
// Criterion Groups and Main
// ============================================================================

criterion_group!(
    search_kv_benches,
    search_kv_by_dataset_size,
    search_kv_by_access_pattern,
);

criterion_group!(search_hybrid_benches, search_hybrid_benchmarks,);

criterion_group!(search_result_benches, search_result_size_benchmarks,);

criterion_group!(
    index_benches,
    index_operations_benchmarks,
    index_scaling_benchmarks,
);

criterion_group!(search_overhead_benches, search_overhead_benchmarks,);

criterion_group!(
    regression_benches,
    kv_put_inmemory_benchmark,
    kv_get_fast_path_benchmark,
);

criterion_main!(
    search_kv_benches,
    search_hybrid_benches,
    search_result_benches,
    index_benches,
    search_overhead_benches,
    regression_benches
);
