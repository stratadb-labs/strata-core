//! M3 Primitive Benchmarks - Redis Competitiveness Harness
//!
//! ## Mission Statement
//!
//! We are building a state substrate for agents with Redis-class hot-path
//! performance and strictly stronger semantics: transactions, causal ordering,
//! structured state, and replayability.
//!
//! ## Performance Tiers
//!
//! - **Tier A0**: Core data structure (no transaction machinery) - target <200-500ns
//! - **Tier A1**: Minimal correctness wrapper (snapshot + commit, no WAL) - M3 gate: <3µs
//! - **Tier B**: Transactional operations (primitive facades) - target <5-40µs
//! - **Tier C**: Indexed operations (TraceStore, RunIndex) - target <20-200µs
//! - **Tier D**: Contention behavior (relative scaling, not absolute ops/sec)
//!
//! ## Benchmark Categories
//!
//! | Prefix | Tier | What It Measures |
//! |--------|------|------------------|
//! | `core_*` | A0 | Raw data structure access |
//! | `engine_*` | A1 | Transaction overhead without facades |
//! | `kvstore_*`, `eventlog_*`, etc. | B | Primitive facade operations |
//! | `tracestore_query_*`, `runindex_*` | C | Indexed operations |
//! | `contention/*` | D | Multi-thread behavior |
//! | `cache_*` | - | Cache locality effects |
//! | `*_noalloc` | - | Allocation-free variants |
//! | `cross_txn_*` | B | Cross-primitive transactions |
//! | `index_amp_*` | C | Index amplification cost |
//! | `mem_*` | - | Memory overhead |
//!
//! ## Running
//!
//! ```bash
//! # Full suite
//! cargo bench --bench m3_primitives
//!
//! # By tier
//! cargo bench --bench m3_primitives -- "core_"        # Tier A0
//! cargo bench --bench m3_primitives -- "engine_"      # Tier A1
//! cargo bench --bench m3_primitives -- "kvstore_"     # Tier B
//! cargo bench --bench m3_primitives -- "contention/"  # Tier D
//!
//! # Special categories
//! cargo bench --bench m3_primitives -- "cache_"       # Cache locality
//! cargo bench --bench m3_primitives -- "_noalloc"     # Allocation-free
//! cargo bench --bench m3_primitives -- "cross_txn_"   # Cross-primitive
//! ```

mod bench_env;

use bench_env::{
    default_output_dir, BenchEnvironment, BenchmarkReport, ContentionResults, FacadeTaxReport,
    PerfConfig,
};
use criterion::{black_box, BenchmarkId, Criterion, Throughput};
use strata_core::traits::Storage;
use strata_core::types::{Key, Namespace, RunId};
use strata_core::value::Value;
use strata_durability::wal::DurabilityMode;
use strata_engine::Database;
use strata_primitives::{EventLog, KVStore, RunIndex, RunStatus, StateCell, TraceStore, TraceType};
use strata_storage::UnifiedStore;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Barrier};
use std::thread;
use std::time::{Duration, Instant};
use tempfile::TempDir;

// =============================================================================
// Constants and Configuration
// =============================================================================

/// Fixed seed for deterministic "random" key selection.
const BENCH_SEED: u64 = 0xDEADBEEF_CAFEBABE;

/// Duration for fixed-time contention benchmarks
const CONTENTION_BENCH_DURATION: Duration = Duration::from_secs(2);

// =============================================================================
// Test Utilities
// =============================================================================

/// Get durability mode from INMEM_DURABILITY_MODE environment variable
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

/// Create a test database and return both DB and temp directory
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

/// Create a standalone UnifiedStore for Tier A0 benchmarks
fn create_store() -> UnifiedStore {
    UnifiedStore::new()
}

/// Create a test namespace
fn test_namespace(run_id: RunId) -> Namespace {
    Namespace::new(
        "tenant".to_string(),
        "app".to_string(),
        "agent".to_string(),
        run_id,
    )
}

/// Simple LCG for deterministic "random" key selection without allocation.
fn lcg_next(state: &mut u64) -> u64 {
    *state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
    *state
}

// =============================================================================
// TIER A0: Core Data Structure Benchmarks
// =============================================================================
// These bypass ALL transaction machinery - pure data structure access.
// Purpose: Establish the absolute floor for performance.

fn tier_a0_benchmarks(c: &mut Criterion) {
    let mut group = c.benchmark_group("core");
    group.throughput(Throughput::Elements(1));

    // --- core_get_hot: Raw storage lookup ---
    // Asymptotic goal: <200 ns (Redis: ~100-200 ns)
    {
        let store = create_store();
        let run_id = RunId::new();
        let ns = test_namespace(run_id);
        let key = Key::new_kv(ns, "hot_key");

        // Pre-populate
        store.put(key.clone(), Value::I64(42), None).unwrap();

        group.bench_function("get_hot", |b| {
            b.iter(|| {
                let result = store.get(black_box(&key));
                black_box(result.unwrap())
            });
        });
    }

    // --- core_put_hot: Raw storage insert ---
    // Asymptotic goal: <300 ns (Redis: ~200-300 ns)
    {
        let store = create_store();
        let run_id = RunId::new();
        let ns = test_namespace(run_id);

        let counter = AtomicU64::new(0);

        group.bench_function("put_hot", |b| {
            b.iter(|| {
                let i = counter.fetch_add(1, Ordering::Relaxed);
                // Pre-create key outside of hot path in real usage
                let key = Key::new_kv(ns.clone(), format!("key_{}", i));
                let result = store.put(key, Value::I64(i as i64), None);
                black_box(result.unwrap())
            });
        });
    }

    // --- core_put_hot_prealloc: Raw storage insert with pre-allocated key ---
    // This isolates the true storage cost from key construction
    {
        let store = create_store();
        let run_id = RunId::new();
        let ns = test_namespace(run_id);

        // Pre-allocate keys
        let keys: Vec<Key> = (0..100_000)
            .map(|i| Key::new_kv(ns.clone(), format!("prealloc_{}", i)))
            .collect();

        let counter = AtomicU64::new(0);

        group.bench_function("put_hot_prealloc", |b| {
            b.iter(|| {
                let i = (counter.fetch_add(1, Ordering::Relaxed) as usize) % keys.len();
                let result = store.put(keys[i].clone(), Value::I64(i as i64), None);
                black_box(result.unwrap())
            });
        });
    }

    // --- core_get_versioned: Raw versioned lookup ---
    {
        let store = create_store();
        let run_id = RunId::new();
        let ns = test_namespace(run_id);
        let key = Key::new_kv(ns, "versioned_key");

        store.put(key.clone(), Value::I64(42), None).unwrap();
        let version = store.current_version();

        group.bench_function("get_versioned", |b| {
            b.iter(|| {
                let result = store.get_versioned(black_box(&key), black_box(version));
                black_box(result.unwrap())
            });
        });
    }

    // --- core_scan_prefix: Raw prefix scan ---
    {
        let store = create_store();
        let run_id = RunId::new();
        let ns = test_namespace(run_id);

        // Pre-populate with 100 keys
        for i in 0..100 {
            let key = Key::new_kv(ns.clone(), format!("scan_{}", i));
            store.put(key, Value::I64(i), None).unwrap();
        }

        let prefix = Key::new_kv(ns.clone(), "scan_");
        let version = store.current_version();

        group.bench_function("scan_prefix_100", |b| {
            b.iter(|| {
                let result = store.scan_prefix(black_box(&prefix), black_box(version));
                black_box(result.unwrap())
            });
        });
    }

    group.finish();
}

// =============================================================================
// TIER A1: Engine Microbenchmarks
// =============================================================================
// Minimal correctness wrapper - snapshot + commit, no primitive facades.
// M3 Hard Gate: ALL operations < 3 µs

fn tier_a1_benchmarks(c: &mut Criterion) {
    let mut group = c.benchmark_group("engine");
    group.throughput(Throughput::Elements(1));

    // --- engine_get_direct: Snapshot + key lookup ---
    // M3 Gate: <3 µs
    {
        let (db, _temp) = create_db();
        let run_id = RunId::new();
        let ns = test_namespace(run_id);
        let key = Key::new_kv(ns, "direct_get");

        // Pre-populate via transaction
        db.transaction(run_id, |txn| {
            txn.put(key.clone(), Value::I64(42))?;
            Ok(())
        })
        .unwrap();

        group.bench_function("get_direct", |b| {
            b.iter(|| {
                let result = db.transaction(run_id, |txn| {
                    let val = txn.get(black_box(&key))?;
                    Ok(val)
                });
                black_box(result.unwrap())
            });
        });
    }

    // --- engine_put_direct: Snapshot + write + commit ---
    // M3 Gate: <3 µs
    {
        let (db, _temp) = create_db();
        let run_id = RunId::new();
        let ns = test_namespace(run_id);

        let counter = AtomicU64::new(0);

        group.bench_function("put_direct", |b| {
            b.iter(|| {
                let i = counter.fetch_add(1, Ordering::Relaxed);
                let key = Key::new_kv(ns.clone(), format!("put_{}", i));
                let result = db.transaction(run_id, |txn| {
                    txn.put(key, Value::I64(i as i64))?;
                    Ok(())
                });
                black_box(result.unwrap())
            });
        });
    }

    // --- engine_cas_direct: Read + validate + commit ---
    // M3 Gate: <3 µs
    {
        let (db, _temp) = create_db();
        let run_id = RunId::new();
        let ns = test_namespace(run_id);
        let key = Key::new_kv(ns, "cas_key");

        // Initialize
        db.put(run_id, key.clone(), Value::I64(0)).unwrap();

        group.bench_function("cas_direct", |b| {
            b.iter(|| {
                // Get current version
                let current = db.get(&key).unwrap().unwrap();
                let new_val = match current.value {
                    Value::I64(n) => n + 1,
                    _ => 1,
                };
                let result = db.cas(run_id, key.clone(), current.version, Value::I64(new_val));
                black_box(result.unwrap())
            });
        });
    }

    // --- engine_snapshot_acquire: Snapshot overhead alone ---
    // M3 Gate: <1 µs
    {
        let (db, _temp) = create_db();
        let run_id = RunId::new();

        group.bench_function("snapshot_acquire", |b| {
            b.iter(|| {
                // begin_transaction creates a snapshot
                let txn = db.begin_transaction(run_id);
                black_box(txn)
            });
        });
    }

    // --- engine_txn_empty_commit: Transaction overhead alone ---
    // M3 Gate: <2 µs
    {
        let (db, _temp) = create_db();
        let run_id = RunId::new();

        group.bench_function("txn_empty_commit", |b| {
            b.iter(|| {
                let result = db.transaction(run_id, |_txn| Ok(()));
                black_box(result.unwrap())
            });
        });
    }

    // --- engine_read_your_writes: Write then read in same txn ---
    {
        let (db, _temp) = create_db();
        let run_id = RunId::new();
        let ns = test_namespace(run_id);

        let counter = AtomicU64::new(0);

        group.bench_function("read_your_writes", |b| {
            b.iter(|| {
                let i = counter.fetch_add(1, Ordering::Relaxed);
                let key = Key::new_kv(ns.clone(), format!("ryw_{}", i));
                let result = db.transaction(run_id, |txn| {
                    txn.put(key.clone(), Value::I64(i as i64))?;
                    let val = txn.get(&key)?;
                    Ok(val)
                });
                black_box(result.unwrap())
            });
        });
    }

    group.finish();
}

// =============================================================================
// TIER B: Primitive Facade Benchmarks
// =============================================================================

fn eventlog_benchmarks(c: &mut Criterion) {
    let mut group = c.benchmark_group("eventlog");
    group.throughput(Throughput::Elements(1));

    // --- eventlog_append: Single event append with hash chain ---
    // M3 Target: <10 µs
    {
        let (db, _temp) = create_db();
        let log = EventLog::new(db);
        let run_id = RunId::new();

        let counter = AtomicU64::new(0);

        group.bench_function("append", |b| {
            b.iter(|| {
                let i = counter.fetch_add(1, Ordering::Relaxed);
                let result = log.append(&run_id, "test_event", Value::I64(i as i64));
                black_box(result.unwrap())
            });
        });
    }

    // --- eventlog_append_with_payload: Event with complex payload ---
    {
        let (db, _temp) = create_db();
        let log = EventLog::new(db);
        let run_id = RunId::new();

        let counter = AtomicU64::new(0);

        group.bench_function("append_with_payload", |b| {
            b.iter(|| {
                let i = counter.fetch_add(1, Ordering::Relaxed);
                let payload = Value::Map(HashMap::from([
                    ("tool".to_string(), Value::String("search".into())),
                    ("query".to_string(), Value::String(format!("query_{}", i))),
                    ("results".to_string(), Value::I64(42)),
                ]));
                let result = log.append(&run_id, "tool_call", payload);
                black_box(result.unwrap())
            });
        });
    }

    // --- eventlog_read: Point read ---
    // M3 Target: <5 µs
    {
        let (db, _temp) = create_db();
        let log = EventLog::new(db);
        let run_id = RunId::new();

        for i in 0..1000 {
            log.append(&run_id, "test", Value::I64(i)).unwrap();
        }

        let mut rng_state = BENCH_SEED;

        group.bench_function("read", |b| {
            b.iter(|| {
                let seq = lcg_next(&mut rng_state) % 1000;
                let result = log.read(&run_id, seq);
                black_box(result.unwrap())
            });
        });
    }

    // --- eventlog_read_range: Batch read ---
    // M3 Target: <100 µs for 100 events
    for range_size in [10, 50, 100] {
        let (db, _temp) = create_db();
        let log = EventLog::new(db);
        let run_id = RunId::new();

        for i in 0..1000 {
            log.append(&run_id, "test", Value::I64(i)).unwrap();
        }

        group.bench_with_input(
            BenchmarkId::new("read_range", range_size),
            &range_size,
            |b, &range_size| {
                b.iter(|| {
                    let start = 500;
                    let end = start + range_size;
                    let result = log.read_range(&run_id, start, end);
                    black_box(result.unwrap())
                });
            },
        );
    }

    // --- eventlog_verify_chain: Chain integrity verification ---
    // M3 Target: <2 ms for 1000 events
    for chain_length in [100, 500, 1000] {
        let (db, _temp) = create_db();
        let log = EventLog::new(db);
        let run_id = RunId::new();

        for i in 0..chain_length {
            log.append(&run_id, "test", Value::I64(i as i64)).unwrap();
        }

        group.bench_with_input(
            BenchmarkId::new("verify_chain", chain_length),
            &chain_length,
            |b, _| {
                b.iter(|| {
                    let result = log.verify_chain(&run_id);
                    black_box(result.unwrap())
                });
            },
        );
    }

    group.finish();
}

fn statecell_benchmarks(c: &mut Criterion) {
    let mut group = c.benchmark_group("statecell");
    group.throughput(Throughput::Elements(1));

    // --- statecell_init: Initialize cell ---
    // M3 Target: <10 µs
    {
        let (db, _temp) = create_db();
        let sc = StateCell::new(db);
        let run_id = RunId::new();

        let counter = AtomicU64::new(0);

        group.bench_function("init", |b| {
            b.iter(|| {
                let i = counter.fetch_add(1, Ordering::Relaxed);
                let name = format!("cell_{}", i);
                let result = sc.init(&run_id, &name, Value::I64(0));
                black_box(result.unwrap())
            });
        });
    }

    // --- statecell_read: Fetch state ---
    // M3 Target: <5 µs
    {
        let (db, _temp) = create_db();
        let sc = StateCell::new(db);
        let run_id = RunId::new();

        sc.init(&run_id, "test_cell", Value::I64(0)).unwrap();

        group.bench_function("read", |b| {
            b.iter(|| {
                let result = sc.read(&run_id, "test_cell");
                black_box(result.unwrap())
            });
        });
    }

    // --- statecell_cas: CAS update ---
    // M3 Target: <10 µs
    {
        let (db, _temp) = create_db();
        let sc = StateCell::new(db);
        let run_id = RunId::new();

        sc.init(&run_id, "counter", Value::I64(0)).unwrap();

        group.bench_function("cas", |b| {
            b.iter(|| {
                let state = sc.read(&run_id, "counter").unwrap().unwrap();
                let new_val = match state.value {
                    Value::I64(n) => n + 1,
                    _ => 1,
                };
                let result = sc.cas(&run_id, "counter", state.version, Value::I64(new_val));
                black_box(result.unwrap())
            });
        });
    }

    // --- statecell_transition: Atomic read-modify-write ---
    // M3 Target: <15 µs
    {
        let (db, _temp) = create_db();
        let sc = StateCell::new(db);
        let run_id = RunId::new();

        sc.init(&run_id, "counter", Value::I64(0)).unwrap();

        group.bench_function("transition", |b| {
            b.iter(|| {
                let result = sc.transition(&run_id, "counter", |state| {
                    let current = match &state.value {
                        Value::I64(n) => *n,
                        _ => 0,
                    };
                    Ok((Value::I64(current + 1), current + 1))
                });
                black_box(result.unwrap())
            });
        });
    }

    // --- statecell_set: Unconditional write ---
    {
        let (db, _temp) = create_db();
        let sc = StateCell::new(db);
        let run_id = RunId::new();

        sc.init(&run_id, "cell", Value::I64(0)).unwrap();

        let counter = AtomicU64::new(0);

        group.bench_function("set", |b| {
            b.iter(|| {
                let i = counter.fetch_add(1, Ordering::Relaxed);
                let result = sc.set(&run_id, "cell", Value::I64(i as i64));
                black_box(result.unwrap())
            });
        });
    }

    group.finish();
}

fn kvstore_benchmarks(c: &mut Criterion) {
    let mut group = c.benchmark_group("kvstore");
    group.throughput(Throughput::Elements(1));

    // --- kvstore_put ---
    // M3 Target: <8 µs
    {
        let (db, _temp) = create_db();
        let kv = KVStore::new(db);
        let run_id = RunId::new();

        let counter = AtomicU64::new(0);

        group.bench_function("put", |b| {
            b.iter(|| {
                let i = counter.fetch_add(1, Ordering::Relaxed);
                let key = format!("key_{}", i);
                let result = kv.put(&run_id, &key, Value::I64(i as i64));
                black_box(result.unwrap())
            });
        });
    }

    // --- kvstore_get ---
    // M3 Target: <5 µs
    {
        let (db, _temp) = create_db();
        let kv = KVStore::new(db);
        let run_id = RunId::new();

        for i in 0..1000 {
            kv.put(&run_id, &format!("key_{}", i), Value::I64(i))
                .unwrap();
        }

        let mut rng_state = BENCH_SEED;

        group.bench_function("get", |b| {
            b.iter(|| {
                let i = lcg_next(&mut rng_state) % 1000;
                let key = format!("key_{}", i);
                let result = kv.get(&run_id, &key);
                black_box(result.unwrap())
            });
        });
    }

    // --- kvstore_get_missing ---
    {
        let (db, _temp) = create_db();
        let kv = KVStore::new(db);
        let run_id = RunId::new();

        let counter = AtomicU64::new(0);

        group.bench_function("get_missing", |b| {
            b.iter(|| {
                let i = counter.fetch_add(1, Ordering::Relaxed);
                let key = format!("missing_{}", i);
                let result = kv.get(&run_id, &key);
                black_box(result.unwrap())
            });
        });
    }

    // --- kvstore_delete ---
    // M3 Target: <8 µs
    {
        let (db, _temp) = create_db();
        let kv = KVStore::new(db);
        let run_id = RunId::new();

        let counter = AtomicU64::new(0);

        group.bench_function("delete", |b| {
            b.iter(|| {
                let i = counter.fetch_add(1, Ordering::Relaxed);
                let key = format!("del_{}", i);
                // Create the key first, then delete it
                kv.put(&run_id, &key, Value::I64(i as i64)).unwrap();
                let result = kv.delete(&run_id, &key);
                black_box(result.unwrap())
            });
        });
    }

    group.finish();
}

fn tracestore_benchmarks(c: &mut Criterion) {
    let mut group = c.benchmark_group("tracestore");
    group.throughput(Throughput::Elements(1));

    // --- tracestore_record_minimal: Minimal trace (2 indices) ---
    // M3 Target: <20 µs
    {
        let (db, _temp) = create_db();
        let ts = TraceStore::new(db);
        let run_id = RunId::new();

        let counter = AtomicU64::new(0);

        group.bench_function("record_minimal", |b| {
            b.iter(|| {
                let i = counter.fetch_add(1, Ordering::Relaxed);
                let result = ts.record(
                    &run_id,
                    TraceType::Thought {
                        content: format!("Thought {}", i),
                        confidence: Some(0.9),
                    },
                    vec![],
                    Value::Null,
                );
                black_box(result.unwrap())
            });
        });
    }

    // --- tracestore_record_3_tags: Trace with 3 tags (5 indices) ---
    // M3 Target: <40 µs
    {
        let (db, _temp) = create_db();
        let ts = TraceStore::new(db);
        let run_id = RunId::new();

        let counter = AtomicU64::new(0);

        group.bench_function("record_3_tags", |b| {
            b.iter(|| {
                let i = counter.fetch_add(1, Ordering::Relaxed);
                let result = ts.record(
                    &run_id,
                    TraceType::Decision {
                        question: format!("Decision {}", i),
                        options: vec!["A".to_string(), "B".to_string()],
                        chosen: "A".to_string(),
                        reasoning: Some("Because A is better".to_string()),
                    },
                    vec![
                        "important".to_string(),
                        "reviewed".to_string(),
                        "final".to_string(),
                    ],
                    Value::Null,
                );
                black_box(result.unwrap())
            });
        });
    }

    // --- tracestore_query_by_type: Index lookup ---
    // M3 Target: <200 µs
    {
        let (db, _temp) = create_db();
        let ts = TraceStore::new(db);
        let run_id = RunId::new();

        // Pre-populate with mixed types
        for i in 0..300 {
            match i % 3 {
                0 => {
                    ts.record(
                        &run_id,
                        TraceType::ToolCall {
                            tool_name: "search".to_string(),
                            arguments: Value::Null,
                            result: None,
                            duration_ms: None,
                        },
                        vec![],
                        Value::Null,
                    )
                    .unwrap();
                }
                1 => {
                    ts.record(
                        &run_id,
                        TraceType::Thought {
                            content: "thinking".to_string(),
                            confidence: None,
                        },
                        vec![],
                        Value::Null,
                    )
                    .unwrap();
                }
                _ => {
                    ts.record(
                        &run_id,
                        TraceType::Decision {
                            question: "q".to_string(),
                            options: vec![],
                            chosen: "a".to_string(),
                            reasoning: None,
                        },
                        vec![],
                        Value::Null,
                    )
                    .unwrap();
                }
            }
        }

        group.bench_function("query_by_type", |b| {
            b.iter(|| {
                let result = ts.query_by_type(&run_id, "ToolCall");
                black_box(result.unwrap())
            });
        });
    }

    // --- tracestore_get_tree: Hierarchy reconstruction ---
    // M3 Target: <150 µs for 13 nodes
    {
        let (db, _temp) = create_db();
        let ts = TraceStore::new(db);
        let run_id = RunId::new();

        // Create tree: root + 3 children + 9 grandchildren = 13 nodes
        let root_id = ts
            .record(
                &run_id,
                TraceType::Decision {
                    question: "Root".to_string(),
                    options: vec![],
                    chosen: "A".to_string(),
                    reasoning: None,
                },
                vec![],
                Value::Null,
            )
            .unwrap();

        for i in 0..3 {
            let child_id = ts
                .record_child(
                    &run_id,
                    &root_id,
                    TraceType::Thought {
                        content: format!("Level 1 - {}", i),
                        confidence: None,
                    },
                    vec![],
                    Value::Null,
                )
                .unwrap();

            for j in 0..3 {
                ts.record_child(
                    &run_id,
                    &child_id,
                    TraceType::Thought {
                        content: format!("Level 2 - {}.{}", i, j),
                        confidence: None,
                    },
                    vec![],
                    Value::Null,
                )
                .unwrap();
            }
        }

        group.bench_function("get_tree", |b| {
            b.iter(|| {
                let result = ts.get_tree(&run_id, &root_id);
                black_box(result.unwrap())
            });
        });
    }

    group.finish();
}

fn runindex_benchmarks(c: &mut Criterion) {
    let mut group = c.benchmark_group("runindex");
    group.throughput(Throughput::Elements(1));

    // --- runindex_create ---
    // M3 Target: <15 µs
    {
        let (db, _temp) = create_db();
        let ri = RunIndex::new(db);

        let counter = AtomicU64::new(0);

        group.bench_function("create", |b| {
            b.iter(|| {
                let i = counter.fetch_add(1, Ordering::Relaxed);
                let run_name = format!("run_{}", i);
                let result = ri.create_run(&run_name);
                black_box(result.unwrap())
            });
        });
    }

    // --- runindex_transition (complete_run) ---
    // M3 Target: <20 µs
    {
        let (db, _temp) = create_db();
        let ri = RunIndex::new(db);

        let counter = AtomicU64::new(0);

        group.bench_function("transition", |b| {
            b.iter(|| {
                let i = counter.fetch_add(1, Ordering::Relaxed);
                let run_name = format!("status_run_{}", i);
                // Create the run first, then transition it
                ri.create_run(&run_name).unwrap();
                let result = ri.complete_run(&run_name);
                black_box(result.unwrap())
            });
        });
    }

    // --- runindex_lifecycle: create -> complete -> archive ---
    // M3 Target: <50 µs
    {
        let (db, _temp) = create_db();
        let ri = RunIndex::new(db);

        let counter = AtomicU64::new(0);

        group.bench_function("lifecycle", |b| {
            b.iter(|| {
                let i = counter.fetch_add(1, Ordering::Relaxed);
                let run_name = format!("lifecycle_{}", i);

                ri.create_run(&run_name).unwrap();
                ri.complete_run(&run_name).unwrap();
                let result = ri.archive_run(&run_name);
                black_box(result.unwrap())
            });
        });
    }

    // --- runindex_query_by_status ---
    {
        let (db, _temp) = create_db();
        let ri = RunIndex::new(db);

        for i in 0..100 {
            let run_name = format!("query_run_{}", i);
            ri.create_run(&run_name).unwrap();
            if i % 2 == 0 {
                ri.complete_run(&run_name).unwrap();
            }
        }

        group.bench_function("query_by_status", |b| {
            b.iter(|| {
                let result = ri.query_by_status(RunStatus::Completed);
                black_box(result.unwrap())
            });
        });
    }

    group.finish();
}

// =============================================================================
// Cross-Primitive Transaction Benchmarks
// =============================================================================

fn cross_primitive_benchmarks(c: &mut Criterion) {
    let mut group = c.benchmark_group("cross_txn");
    group.throughput(Throughput::Elements(1));

    // --- cross_txn_kv_event: KV + EventLog atomic ---
    // M3 Target: <20 µs
    {
        let (db, _temp) = create_db();
        let run_id = RunId::new();
        let kv = KVStore::new(Arc::clone(&db));
        let log = EventLog::new(Arc::clone(&db));

        let counter = AtomicU64::new(0);

        group.bench_function("kv_event", |b| {
            b.iter(|| {
                let i = counter.fetch_add(1, Ordering::Relaxed);
                // KV put
                kv.put(&run_id, &format!("key_{}", i), Value::I64(i as i64))
                    .unwrap();
                // EventLog append
                let result = log.append(&run_id, "kv_written", Value::I64(i as i64));
                black_box(result.unwrap())
            });
        });
    }

    // --- cross_txn_kv_event_state: KV + EventLog + StateCell ---
    // M3 Target: <30 µs
    {
        let (db, _temp) = create_db();
        let run_id = RunId::new();
        let kv = KVStore::new(Arc::clone(&db));
        let log = EventLog::new(Arc::clone(&db));
        let sc = StateCell::new(Arc::clone(&db));

        // Initialize state cell
        sc.init(&run_id, "counter", Value::I64(0)).unwrap();

        group.bench_function("kv_event_state", |b| {
            b.iter(|| {
                // StateCell transition returns (T, version) where T is the closure's return value
                let (i, _version) = sc
                    .transition(&run_id, "counter", |state| {
                        let current = match &state.value {
                            Value::I64(n) => *n,
                            _ => 0,
                        };
                        Ok((Value::I64(current + 1), current + 1))
                    })
                    .unwrap();

                // KV put
                kv.put(&run_id, &format!("key_{}", i), Value::I64(i))
                    .unwrap();

                // EventLog append
                let result = log.append(&run_id, "step_complete", Value::I64(i));
                black_box(result.unwrap())
            });
        });
    }

    // --- cross_snapshot_read: Multi-primitive read ---
    // M3 Target: <15 µs
    {
        let (db, _temp) = create_db();
        let run_id = RunId::new();
        let kv = KVStore::new(Arc::clone(&db));
        let sc = StateCell::new(Arc::clone(&db));

        // Pre-populate
        kv.put(&run_id, "key1", Value::I64(42)).unwrap();
        sc.init(&run_id, "state1", Value::I64(100)).unwrap();

        group.bench_function("snapshot_read", |b| {
            b.iter(|| {
                let kv_val = kv.get(&run_id, "key1").unwrap();
                let sc_val = sc.read(&run_id, "state1").unwrap();
                black_box((kv_val, sc_val))
            });
        });
    }

    group.finish();
}

// =============================================================================
// Index Amplification Benchmarks (Tier C)
// =============================================================================

fn index_amplification_benchmarks(c: &mut Criterion) {
    let mut group = c.benchmark_group("index_amp");
    group.throughput(Throughput::Elements(1));

    // Test index amplification with varying tag counts
    for tag_count in [0, 1, 3, 5] {
        let (db, _temp) = create_db();
        let ts = TraceStore::new(db);
        let run_id = RunId::new();

        let counter = AtomicU64::new(0);
        let tags: Vec<String> = (0..tag_count).map(|i| format!("tag_{}", i)).collect();

        group.bench_with_input(
            BenchmarkId::new("trace_tags", tag_count),
            &tag_count,
            |b, _| {
                b.iter(|| {
                    let i = counter.fetch_add(1, Ordering::Relaxed);
                    let result = ts.record(
                        &run_id,
                        TraceType::Thought {
                            content: format!("Thought {}", i),
                            confidence: Some(0.9),
                        },
                        tags.clone(),
                        Value::Null,
                    );
                    black_box(result.unwrap())
                });
            },
        );
    }

    group.finish();
}

// =============================================================================
// Cache Locality Tests
// =============================================================================

fn cache_locality_benchmarks(c: &mut Criterion) {
    let mut group = c.benchmark_group("cache");
    group.throughput(Throughput::Elements(1));

    // Working set sizes to test
    let working_set_sizes = [1, 8, 64, 512, 10_000];

    for &ws_size in &working_set_sizes {
        let store = create_store();
        let run_id = RunId::new();
        let ns = test_namespace(run_id);

        // Pre-populate with keys
        let keys: Vec<Key> = (0..ws_size.max(10_000))
            .map(|i| Key::new_kv(ns.clone(), format!("cache_key_{}", i)))
            .collect();

        for key in &keys {
            store.put(key.clone(), Value::I64(42), None).unwrap();
        }

        let mut rng_state = BENCH_SEED;

        group.bench_with_input(
            BenchmarkId::new("working_set", ws_size),
            &ws_size,
            |b, &ws_size| {
                b.iter(|| {
                    let idx = (lcg_next(&mut rng_state) as usize) % ws_size;
                    let result = store.get(&keys[idx]);
                    black_box(result.unwrap())
                });
            },
        );
    }

    group.finish();
}

// =============================================================================
// Branch Predictor Tests
// =============================================================================

fn branch_predictor_benchmarks(c: &mut Criterion) {
    let mut group = c.benchmark_group("branch");
    group.throughput(Throughput::Elements(1));

    let store = create_store();
    let run_id = RunId::new();
    let ns = test_namespace(run_id);

    // Pre-populate with 10k keys
    let keys: Vec<Key> = (0..10_000)
        .map(|i| Key::new_kv(ns.clone(), format!("branch_key_{:05}", i)))
        .collect();

    for key in &keys {
        store.put(key.clone(), Value::I64(42), None).unwrap();
    }

    // Sequential access (most predictable)
    {
        let counter = AtomicU64::new(0);

        group.bench_function("sequential", |b| {
            b.iter(|| {
                let idx = (counter.fetch_add(1, Ordering::Relaxed) as usize) % keys.len();
                let result = store.get(&keys[idx]);
                black_box(result.unwrap())
            });
        });
    }

    // Strided access (somewhat predictable)
    {
        let counter = AtomicU64::new(0);

        group.bench_function("strided", |b| {
            b.iter(|| {
                let idx = ((counter.fetch_add(8, Ordering::Relaxed) as usize) * 8) % keys.len();
                let result = store.get(&keys[idx]);
                black_box(result.unwrap())
            });
        });
    }

    // Random access (least predictable)
    {
        let mut rng_state = BENCH_SEED;

        group.bench_function("random", |b| {
            b.iter(|| {
                let idx = (lcg_next(&mut rng_state) as usize) % keys.len();
                let result = store.get(&keys[idx]);
                black_box(result.unwrap())
            });
        });
    }

    group.finish();
}

// =============================================================================
// Contention Benchmarks (Tier D)
// =============================================================================

fn contention_benchmarks(c: &mut Criterion) {
    let mut group = c.benchmark_group("contention");
    group.sample_size(10);

    // --- Same-key contention: StateCell ---
    // Target: 4 threads ≥25% of 1-thread, 8 threads ≥15%
    for num_threads in [1, 2, 4, 8] {
        group.bench_with_input(
            BenchmarkId::new("statecell_same_key", num_threads),
            &num_threads,
            |b, &num_threads| {
                b.iter_custom(|_iters| {
                    let (db, _temp) = create_db();
                    let sc = Arc::new(StateCell::new(db));
                    let run_id = RunId::new();

                    sc.init(&run_id, "counter", Value::I64(0)).unwrap();

                    let barrier = Arc::new(Barrier::new(num_threads + 1));
                    let total_transitions = Arc::new(AtomicU64::new(0));
                    let stop_flag = Arc::new(AtomicU64::new(0));

                    let handles: Vec<_> = (0..num_threads)
                        .map(|_| {
                            let sc = Arc::clone(&sc);
                            let barrier = Arc::clone(&barrier);
                            let total_transitions = Arc::clone(&total_transitions);
                            let stop_flag = Arc::clone(&stop_flag);

                            thread::spawn(move || {
                                barrier.wait();

                                let mut local_transitions = 0u64;
                                while stop_flag.load(Ordering::Relaxed) == 0 {
                                    let result = sc.transition(&run_id, "counter", |state| {
                                        let current = match &state.value {
                                            Value::I64(n) => *n,
                                            _ => 0,
                                        };
                                        Ok((Value::I64(current + 1), ()))
                                    });
                                    if result.is_ok() {
                                        local_transitions += 1;
                                    }
                                }
                                total_transitions.fetch_add(local_transitions, Ordering::Relaxed);
                            })
                        })
                        .collect();

                    let start = Instant::now();
                    barrier.wait();
                    thread::sleep(CONTENTION_BENCH_DURATION);
                    stop_flag.store(1, Ordering::Relaxed);

                    for h in handles {
                        h.join().unwrap();
                    }

                    let elapsed = start.elapsed();
                    let transitions = total_transitions.load(Ordering::Relaxed);

                    // Verify invariant: final count == transitions
                    let final_state = sc.read(&run_id, "counter").unwrap().unwrap();
                    let final_count = match final_state.value {
                        Value::I64(n) => n as u64,
                        _ => 0,
                    };

                    assert_eq!(
                        final_count, transitions,
                        "INVARIANT VIOLATION: final_count ({}) != transitions ({})",
                        final_count, transitions
                    );

                    eprintln!(
                        "contention/statecell_same_key/{}: {} ops in {:?} ({:.0} ops/s)",
                        num_threads,
                        transitions,
                        elapsed,
                        transitions as f64 / elapsed.as_secs_f64()
                    );

                    elapsed
                });
            },
        );
    }

    // --- Same-key contention: EventLog ---
    for num_threads in [1, 2, 4, 8] {
        group.bench_with_input(
            BenchmarkId::new("eventlog_same_key", num_threads),
            &num_threads,
            |b, &num_threads| {
                b.iter_custom(|_iters| {
                    let (db, _temp) = create_db();
                    let log = Arc::new(EventLog::new(db));
                    let run_id = RunId::new();

                    let barrier = Arc::new(Barrier::new(num_threads + 1));
                    let total_appends = Arc::new(AtomicU64::new(0));
                    let stop_flag = Arc::new(AtomicU64::new(0));

                    let handles: Vec<_> = (0..num_threads)
                        .map(|thread_id| {
                            let log = Arc::clone(&log);
                            let barrier = Arc::clone(&barrier);
                            let total_appends = Arc::clone(&total_appends);
                            let stop_flag = Arc::clone(&stop_flag);

                            thread::spawn(move || {
                                barrier.wait();

                                let mut local_appends = 0u64;
                                while stop_flag.load(Ordering::Relaxed) == 0 {
                                    if log
                                        .append(
                                            &run_id,
                                            "thread_event",
                                            Value::I64(thread_id as i64),
                                        )
                                        .is_ok()
                                    {
                                        local_appends += 1;
                                    }
                                }
                                total_appends.fetch_add(local_appends, Ordering::Relaxed);
                            })
                        })
                        .collect();

                    let start = Instant::now();
                    barrier.wait();
                    thread::sleep(CONTENTION_BENCH_DURATION);
                    stop_flag.store(1, Ordering::Relaxed);

                    for h in handles {
                        h.join().unwrap();
                    }

                    let elapsed = start.elapsed();
                    let appends = total_appends.load(Ordering::Relaxed);

                    eprintln!(
                        "contention/eventlog_same_key/{}: {} ops in {:?} ({:.0} ops/s)",
                        num_threads,
                        appends,
                        elapsed,
                        appends as f64 / elapsed.as_secs_f64()
                    );

                    elapsed
                });
            },
        );
    }

    // --- Disjoint-key scaling (must scale) ---
    // Target: 2 threads ≥1.8×, 4 threads ≥3.2×, 8 threads ≥6.0×
    for num_threads in [1, 2, 4, 8] {
        group.bench_with_input(
            BenchmarkId::new("disjoint_key", num_threads),
            &num_threads,
            |b, &num_threads| {
                b.iter_custom(|_iters| {
                    let (db, _temp) = create_db();
                    let sc = Arc::new(StateCell::new(db));
                    let run_id = RunId::new();

                    // Initialize separate counter for each thread
                    for i in 0..num_threads {
                        sc.init(&run_id, &format!("counter_{}", i), Value::I64(0))
                            .unwrap();
                    }

                    let barrier = Arc::new(Barrier::new(num_threads + 1));
                    let total_ops = Arc::new(AtomicU64::new(0));
                    let stop_flag = Arc::new(AtomicU64::new(0));

                    let handles: Vec<_> = (0..num_threads)
                        .map(|thread_id| {
                            let sc = Arc::clone(&sc);
                            let barrier = Arc::clone(&barrier);
                            let total_ops = Arc::clone(&total_ops);
                            let stop_flag = Arc::clone(&stop_flag);
                            let cell_name = format!("counter_{}", thread_id);

                            thread::spawn(move || {
                                barrier.wait();

                                let mut local_ops = 0u64;
                                while stop_flag.load(Ordering::Relaxed) == 0 {
                                    let result = sc.transition(&run_id, &cell_name, |state| {
                                        let current = match &state.value {
                                            Value::I64(n) => *n,
                                            _ => 0,
                                        };
                                        Ok((Value::I64(current + 1), ()))
                                    });
                                    if result.is_ok() {
                                        local_ops += 1;
                                    }
                                }
                                total_ops.fetch_add(local_ops, Ordering::Relaxed);
                            })
                        })
                        .collect();

                    let start = Instant::now();
                    barrier.wait();
                    thread::sleep(CONTENTION_BENCH_DURATION);
                    stop_flag.store(1, Ordering::Relaxed);

                    for h in handles {
                        h.join().unwrap();
                    }

                    let elapsed = start.elapsed();
                    let ops = total_ops.load(Ordering::Relaxed);

                    eprintln!(
                        "contention/disjoint_key/{}: {} ops in {:?} ({:.0} ops/s)",
                        num_threads,
                        ops,
                        elapsed,
                        ops as f64 / elapsed.as_secs_f64()
                    );

                    elapsed
                });
            },
        );
    }

    group.finish();
}

// =============================================================================
// Memory Overhead Benchmarks
// =============================================================================

fn memory_overhead_benchmarks(c: &mut Criterion) {
    use std::mem::size_of;

    let mut group = c.benchmark_group("mem");

    // Report sizes of key data structures
    eprintln!("=== Memory Overhead Report ===");
    eprintln!("Key size: {} bytes", size_of::<Key>());
    eprintln!(
        "VersionedValue size: {} bytes",
        size_of::<strata_core::VersionedValue>()
    );
    eprintln!("Value size: {} bytes", size_of::<Value>());
    eprintln!("RunId size: {} bytes", size_of::<RunId>());
    eprintln!("Namespace size: {} bytes", size_of::<Namespace>());

    // Benchmark to measure actual memory usage per entry
    {
        group.bench_function("measure_kv_overhead", |b| {
            b.iter(|| {
                let store = create_store();
                let run_id = RunId::new();
                let ns = test_namespace(run_id);

                // Insert 1000 entries and measure
                for i in 0..1000 {
                    let key = Key::new_kv(ns.clone(), format!("key_{}", i));
                    store.put(key, Value::I64(i), None).unwrap();
                }

                black_box(store.current_version())
            });
        });
    }

    group.finish();
}

// =============================================================================
// Environment Capture (printed at benchmark start)
// =============================================================================

fn print_environment() -> BenchEnvironment {
    let env = BenchEnvironment::capture();

    // Print full environment report
    env.print_report();

    // Print durability mode
    let mode = get_durability_mode();
    eprintln!("=== Durability Mode ===");
    eprintln!("  Mode: {:?}", mode);
    eprintln!("  (Set INMEM_DURABILITY_MODE env var to: inmemory, batched, or strict)");
    eprintln!();

    // Print perf configuration status
    let perf_config = PerfConfig::default();
    perf_config.print_status();

    env
}

// Custom main to print environment before running benchmarks
fn main() {
    let env = print_environment();

    // Initialize report structures
    let mut report = BenchmarkReport::new(env.clone());

    // Use criterion's main runner with all benchmark groups
    let mut criterion = Criterion::default().configure_from_args();

    // Run all benchmark functions
    tier_a0_benchmarks(&mut criterion);
    tier_a1_benchmarks(&mut criterion);
    eventlog_benchmarks(&mut criterion);
    statecell_benchmarks(&mut criterion);
    kvstore_benchmarks(&mut criterion);
    tracestore_benchmarks(&mut criterion);
    runindex_benchmarks(&mut criterion);
    cross_primitive_benchmarks(&mut criterion);
    index_amplification_benchmarks(&mut criterion);
    cache_locality_benchmarks(&mut criterion);
    branch_predictor_benchmarks(&mut criterion);
    contention_benchmarks(&mut criterion);
    memory_overhead_benchmarks(&mut criterion);

    // Finalize criterion
    criterion.final_summary();

    // Parse criterion results and populate report
    // Note: Criterion writes results to target/criterion, we can parse those
    populate_report_from_criterion(&mut report);

    // Print facade tax summary to console
    print_facade_tax_summary(&env);

    // Write reports to markdown files
    let output_dir = default_output_dir();
    if let Err(e) = std::fs::create_dir_all(&output_dir) {
        eprintln!("Warning: Could not create output directory: {}", e);
    } else {
        // Write environment report
        if let Err(e) = env.write_report(&output_dir) {
            eprintln!("Warning: Could not write environment report: {}", e);
        }

        // Write JSON for CI/CD
        if let Err(e) = env.write_json(&output_dir) {
            eprintln!("Warning: Could not write environment JSON: {}", e);
        }

        // Write full benchmark report
        if let Err(e) = report.write_report(&output_dir) {
            eprintln!("Warning: Could not write full benchmark report: {}", e);
        }

        // Write standalone facade tax report
        if let Err(e) = write_facade_tax_report(&output_dir, &env, &report.facade_tax) {
            eprintln!("Warning: Could not write facade tax report: {}", e);
        }

        eprintln!("\nReports written to: {}", output_dir.display());
    }
}

/// Parse criterion results to populate the report
fn populate_report_from_criterion(report: &mut BenchmarkReport) {
    let criterion_dir = std::path::Path::new("target/criterion");

    // Try to read estimates.json files from criterion output
    if criterion_dir.exists() {
        // Tier A0 benchmarks
        for name in &[
            "get_hot",
            "put_hot",
            "put_hot_prealloc",
            "get_versioned",
            "scan_prefix_100",
        ] {
            if let Some(ns) = read_criterion_estimate(&criterion_dir.join("core").join(name)) {
                report.facade_tax.add_a0(name, ns);
            }
        }

        // Tier A1 benchmarks
        for name in &[
            "get_direct",
            "put_direct",
            "cas_direct",
            "snapshot_acquire",
            "txn_empty_commit",
            "read_your_writes",
        ] {
            if let Some(ns) = read_criterion_estimate(&criterion_dir.join("engine").join(name)) {
                report.facade_tax.add_a1(name, ns);
            }
        }

        // Tier B benchmarks - KVStore
        for name in &["put", "get", "get_missing", "delete"] {
            if let Some(ns) = read_criterion_estimate(&criterion_dir.join("kvstore").join(name)) {
                report.facade_tax.add_b(&format!("kvstore_{}", name), ns);
            }
        }

        // Tier B benchmarks - EventLog
        for name in &["append", "read"] {
            if let Some(ns) = read_criterion_estimate(&criterion_dir.join("eventlog").join(name)) {
                report.facade_tax.add_b(&format!("eventlog_{}", name), ns);
            }
        }

        // Tier B benchmarks - StateCell
        for name in &["init", "read", "cas", "transition", "set"] {
            if let Some(ns) = read_criterion_estimate(&criterion_dir.join("statecell").join(name)) {
                report.facade_tax.add_b(&format!("statecell_{}", name), ns);
            }
        }

        // Contention results
        let mut contention = ContentionResults::new();
        for threads in [1, 2, 4, 8] {
            let name = format!("statecell_same_key/{}", threads);
            if let Some(ns) = read_criterion_estimate(&criterion_dir.join("contention").join(&name))
            {
                // Convert ns to ops/sec (approximate)
                contention.add("statecell_same_key", threads, 1_000_000_000.0 / ns);
            }

            let name = format!("disjoint_key/{}", threads);
            if let Some(ns) = read_criterion_estimate(&criterion_dir.join("contention").join(&name))
            {
                contention.add("disjoint_key", threads, 1_000_000_000.0 / ns);
            }
        }

        if !contention.results.is_empty() {
            report.contention_results = Some(contention);
        }
    }
}

/// Read a criterion estimate from the benchmark directory
fn read_criterion_estimate(bench_dir: &std::path::Path) -> Option<f64> {
    let estimates_path = bench_dir.join("new/estimates.json");
    if !estimates_path.exists() {
        return None;
    }

    let content = std::fs::read_to_string(&estimates_path).ok()?;
    let json: serde_json::Value = serde_json::from_str(&content).ok()?;

    // Get the point estimate in nanoseconds
    json.get("mean")?.get("point_estimate")?.as_f64()
}

/// Print facade tax summary to console
fn print_facade_tax_summary(env: &BenchEnvironment) {
    eprintln!("\n{}", "=".repeat(60));
    eprintln!("FACADE TAX REPORT");
    eprintln!("{}", "=".repeat(60));
    eprintln!();
    eprintln!("Compare these benchmark groups to calculate abstraction cost:");
    eprintln!();
    eprintln!("  GET operation:");
    eprintln!("    A0: core/get_hot          (raw data structure)");
    eprintln!("    A1: engine/get_direct     (+ snapshot + commit)");
    eprintln!("    B:  kvstore/get           (+ facade overhead)");
    eprintln!();
    eprintln!("  PUT operation:");
    eprintln!("    A0: core/put_hot          (raw data structure)");
    eprintln!("    A1: engine/put_direct     (+ snapshot + commit)");
    eprintln!("    B:  kvstore/put           (+ facade overhead)");
    eprintln!();
    eprintln!("Target ratios:");
    eprintln!("  A1/A0 < 20× (correctness overhead)");
    eprintln!("  B/A1  < 10× (facade overhead)");
    eprintln!("  B/A0  < 50× (total abstraction cost)");
    eprintln!();

    // Platform warning in summary
    if !env.is_reference_platform {
        eprintln!("⚠️  WARNING: Results above are from non-reference platform.");
        eprintln!("   Do NOT use for performance gates or Redis comparisons.");
        eprintln!("   Run on Linux with 'performance' governor for official results.");
        eprintln!();
    }

    eprintln!(
        "Environment: {} | {} | {} | {}",
        env.os.name, env.cpu.model, env.governor.current, env.git.commit,
    );
    eprintln!("{}", "=".repeat(60));
}

/// Write standalone facade tax report
fn write_facade_tax_report(
    output_dir: &std::path::Path,
    env: &BenchEnvironment,
    facade_tax: &FacadeTaxReport,
) -> std::io::Result<std::path::PathBuf> {
    use std::io::Write;

    let timestamp = env
        .timestamp
        .replace(":", "-")
        .replace("T", "_")
        .replace("Z", "");
    let filename = format!("facade_tax_{}.md", timestamp);
    let filepath = output_dir.join(&filename);

    let mut content = String::new();
    content.push_str(&facade_tax.to_markdown());

    // Add platform warning
    if !env.is_reference_platform {
        content.push_str("\n---\n\n");
        content.push_str("> **WARNING:** Results from non-reference platform.\n");
        content.push_str("> Do NOT use for performance gates or Redis comparisons.\n");
    }

    // Add environment summary
    content.push_str("\n---\n\n");
    content.push_str("## Environment\n\n");
    content.push_str(&format!("- **Generated:** {}\n", env.timestamp));
    content.push_str(&format!("- **OS:** {}\n", env.os.name));
    content.push_str(&format!("- **CPU:** {}\n", env.cpu.model));
    content.push_str(&format!("- **Governor:** {}\n", env.governor.current));
    content.push_str(&format!("- **Commit:** `{}`\n", env.git.commit));

    let mut file = std::fs::File::create(&filepath)?;
    file.write_all(content.as_bytes())?;

    eprintln!("Facade tax report written to: {}", filepath.display());
    Ok(filepath)
}
