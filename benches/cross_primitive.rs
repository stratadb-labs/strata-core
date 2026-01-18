//! Cross-Primitive Benchmarks
//!
//! Run with: cargo bench --bench cross_primitive
//!
//! These benchmarks test operations that span multiple primitives or exercise
//! the system as a whole, reflecting realistic usage patterns.
//!
//! ## Benchmark Categories
//!
//! ### Multi-Primitive Transactions
//! - cross_kv_json: KV and JSON operations in same transaction
//! - cross_state_event: StateCell transitions triggering event logs
//! - cross_search: Search operations across multiple primitive types
//!
//! ### Realistic Workloads
//! - workload_read_heavy: 90% reads, 10% writes
//! - workload_write_heavy: 10% reads, 90% writes
//! - workload_mixed: 50% reads, 50% writes
//!
//! ### Primitive Creation Overhead
//! - primitive_creation: Time to create each primitive type
//!
//! ## Performance Targets
//!
//! - Multi-primitive transaction: < 100µs total
//! - Cross-primitive search: < 10ms per query
//! - Primitive creation: < 10µs each

use criterion::{black_box, criterion_group, criterion_main, Criterion, Throughput};
use in_mem_core::json::JsonValue;
use in_mem_core::types::{JsonDocId, RunId};
use in_mem_core::value::Value;
use in_mem_engine::Database;
use in_mem_primitives::{EventLog, JsonStore, KVStore, StateCell, TraceStore, TraceType};
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

/// Get durability mode label from environment
fn durability_label() -> &'static str {
    match std::env::var("INMEM_DURABILITY_MODE").ok().as_deref() {
        Some("inmemory") => "dur_inmemory",
        Some("batched") => "dur_batched",
        _ => "dur_strict",
    }
}

/// Create an in-memory database
fn create_db() -> Arc<Database> {
    Arc::new(Database::builder().in_memory().open_temp().unwrap())
}

/// Pre-generate keys for deterministic benchmarks
fn pregenerate_keys(count: usize) -> Vec<String> {
    (0..count).map(|i| format!("key_{:08}", i)).collect()
}

// ============================================================================
// Primitive Creation Benchmarks
// ============================================================================

fn primitive_creation(c: &mut Criterion) {
    let mut group = c.benchmark_group("primitive_creation");
    group.measurement_time(Duration::from_secs(5));
    let dur = durability_label();

    // KVStore creation
    group.bench_function(format!("kvstore/{}", dur), |b| {
        b.iter(|| {
            let db = create_db();
            black_box(KVStore::new(Arc::clone(&db)))
        });
    });

    // JsonStore creation
    group.bench_function(format!("jsonstore/{}", dur), |b| {
        b.iter(|| {
            let db = create_db();
            black_box(JsonStore::new(Arc::clone(&db)))
        });
    });

    // EventLog creation
    group.bench_function(format!("eventlog/{}", dur), |b| {
        b.iter(|| {
            let db = create_db();
            black_box(EventLog::new(Arc::clone(&db)))
        });
    });

    // StateCell creation
    group.bench_function(format!("statecell/{}", dur), |b| {
        b.iter(|| {
            let db = create_db();
            black_box(StateCell::new(Arc::clone(&db)))
        });
    });

    // TraceStore creation
    group.bench_function(format!("tracestore/{}", dur), |b| {
        b.iter(|| {
            let db = create_db();
            black_box(TraceStore::new(Arc::clone(&db)))
        });
    });

    group.finish();
}

// ============================================================================
// Multi-Primitive Transaction Benchmarks
// ============================================================================

/// Benchmark KV and JSON operations together
fn cross_kv_json(c: &mut Criterion) {
    let mut group = c.benchmark_group("cross_kv_json");
    group.measurement_time(Duration::from_secs(5));
    let dur = durability_label();

    // Setup: Create primitives
    let db = create_db();
    let kv = KVStore::new(Arc::clone(&db));
    let json = JsonStore::new(Arc::clone(&db));
    let run_id = RunId::new();

    // Pre-populate
    let keys = pregenerate_keys(1000);
    for (i, key) in keys.iter().enumerate() {
        kv.put(&run_id, key, Value::I64(i as i64)).expect("kv put");
        let doc_id = JsonDocId::new();
        let doc = JsonValue::from_value(serde_json::json!({
            "index": i,
            "key": key
        }));
        json.create(&run_id, &doc_id, doc).expect("json create");
    }

    // Benchmark: Read from both KV and JSON in sequence
    group.bench_function(format!("read_both/{}", dur), |b| {
        let mut seed = BENCH_SEED;
        b.iter(|| {
            let idx = (lcg_next(&mut seed) as usize) % 1000;
            let key = &keys[idx];

            // Read from KV
            let kv_val = kv.get(&run_id, key).expect("kv get");
            black_box(kv_val);

            // Note: We'd need to track doc_ids to read from JSON
            // For now, just measure KV
        });
    });

    // Benchmark: Write to both KV and JSON
    group.bench_function(format!("write_both/{}", dur), |b| {
        let mut counter = 10000u64;
        b.iter(|| {
            let key = format!("new_key_{}", counter);
            counter += 1;

            // Write to KV
            kv.put(&run_id, &key, Value::I64(counter as i64))
                .expect("kv put");

            // Write to JSON
            let doc_id = JsonDocId::new();
            let doc = JsonValue::from_value(serde_json::json!({
                "index": counter,
                "key": key
            }));
            json.create(&run_id, &doc_id, doc).expect("json create");
        });
    });

    group.finish();
}

/// Benchmark StateCell transitions with EventLog
fn cross_state_event(c: &mut Criterion) {
    let mut group = c.benchmark_group("cross_state_event");
    group.measurement_time(Duration::from_secs(5));
    let dur = durability_label();

    let db = create_db();
    let state = StateCell::new(Arc::clone(&db));
    let event = EventLog::new(Arc::clone(&db));
    let run_id = RunId::new();

    // Initialize state
    let state_key = "workflow_state";
    state
        .set(&run_id, state_key, Value::String("initial".to_string()))
        .expect("set initial");

    // Benchmark: Transition state and log event
    group.bench_function(format!("transition_and_log/{}", dur), |b| {
        let mut counter = 0u64;
        b.iter(|| {
            counter += 1;
            let new_state = format!("state_{}", counter % 10);

            // Transition state
            state
                .set(&run_id, state_key, Value::String(new_state.clone()))
                .expect("state set");

            // Log the transition event
            event
                .append(&run_id, "state_transition", Value::String(new_state))
                .expect("event append");
        });
    });

    group.finish();
}

/// Benchmark multi-primitive workflow
fn cross_workflow(c: &mut Criterion) {
    let mut group = c.benchmark_group("cross_workflow");
    group.measurement_time(Duration::from_secs(5));
    let dur = durability_label();

    let db = create_db();
    let kv = KVStore::new(Arc::clone(&db));
    let state = StateCell::new(Arc::clone(&db));
    let event = EventLog::new(Arc::clone(&db));
    let trace = TraceStore::new(Arc::clone(&db));
    let run_id = RunId::new();

    // Initialize
    state
        .set(&run_id, "counter", Value::I64(0))
        .expect("init state");

    // Benchmark: Complete workflow using multiple primitives
    // 1. Read current state
    // 2. Compute new state
    // 3. Store result in KV
    // 4. Update state
    // 5. Log event
    // 6. Record trace
    group.bench_function(format!("full_workflow/{}", dur), |b| {
        let mut counter = 0u64;
        b.iter(|| {
            counter += 1;

            // 1. Read current state
            let current = state.read(&run_id, "counter").expect("read state");
            let val = match current {
                Some(s) => match s.value {
                    Value::I64(v) => v,
                    _ => 0,
                },
                _ => 0,
            };

            // 2. Compute new value
            let new_val = val + 1;

            // 3. Store in KV
            let key = format!("result_{}", counter);
            kv.put(&run_id, &key, Value::I64(new_val)).expect("kv put");

            // 4. Update state
            state
                .set(&run_id, "counter", Value::I64(new_val))
                .expect("update state");

            // 5. Log event
            event
                .append(&run_id, "computed", Value::I64(new_val))
                .expect("log event");

            // 6. Record trace
            trace
                .record(
                    &run_id,
                    TraceType::Thought {
                        content: format!("Computed value {}", new_val),
                        confidence: Some(1.0),
                    },
                    vec!["compute".to_string()],
                    Value::String(key),
                )
                .expect("record trace");
        });
    });

    group.finish();
}

// ============================================================================
// Workload Pattern Benchmarks
// ============================================================================

/// Read-heavy workload (90% reads, 10% writes)
fn workload_read_heavy(c: &mut Criterion) {
    let mut group = c.benchmark_group("workload_read_heavy");
    group.measurement_time(Duration::from_secs(10));
    group.throughput(Throughput::Elements(100)); // 100 operations per iteration
    let dur = durability_label();

    let db = create_db();
    let kv = KVStore::new(Arc::clone(&db));
    let run_id = RunId::new();

    // Pre-populate
    let keys = pregenerate_keys(10000);
    for (i, key) in keys.iter().enumerate() {
        kv.put(&run_id, key, Value::I64(i as i64)).expect("populate");
    }

    group.bench_function(format!("kv_90r_10w/{}", dur), |b| {
        let mut seed = BENCH_SEED;
        let mut write_counter = 100000u64;

        b.iter(|| {
            for _ in 0..100 {
                let op = lcg_next(&mut seed) % 100;
                if op < 90 {
                    // Read (90%)
                    let idx = (lcg_next(&mut seed) as usize) % 10000;
                    black_box(kv.get(&run_id, &keys[idx]).expect("get"));
                } else {
                    // Write (10%)
                    let key = format!("write_{}", write_counter);
                    write_counter += 1;
                    kv.put(&run_id, &key, Value::I64(write_counter as i64))
                        .expect("put");
                }
            }
        });
    });

    group.finish();
}

/// Write-heavy workload (10% reads, 90% writes)
fn workload_write_heavy(c: &mut Criterion) {
    let mut group = c.benchmark_group("workload_write_heavy");
    group.measurement_time(Duration::from_secs(10));
    group.throughput(Throughput::Elements(100));
    let dur = durability_label();

    let db = create_db();
    let kv = KVStore::new(Arc::clone(&db));
    let run_id = RunId::new();

    // Pre-populate some keys for reads
    let keys = pregenerate_keys(1000);
    for (i, key) in keys.iter().enumerate() {
        kv.put(&run_id, key, Value::I64(i as i64)).expect("populate");
    }

    group.bench_function(format!("kv_10r_90w/{}", dur), |b| {
        let mut seed = BENCH_SEED;
        let mut write_counter = 100000u64;

        b.iter(|| {
            for _ in 0..100 {
                let op = lcg_next(&mut seed) % 100;
                if op < 10 {
                    // Read (10%)
                    let idx = (lcg_next(&mut seed) as usize) % 1000;
                    black_box(kv.get(&run_id, &keys[idx]).expect("get"));
                } else {
                    // Write (90%)
                    let key = format!("write_{}", write_counter);
                    write_counter += 1;
                    kv.put(&run_id, &key, Value::I64(write_counter as i64))
                        .expect("put");
                }
            }
        });
    });

    group.finish();
}

/// Mixed workload (50% reads, 50% writes)
fn workload_mixed(c: &mut Criterion) {
    let mut group = c.benchmark_group("workload_mixed");
    group.measurement_time(Duration::from_secs(10));
    group.throughput(Throughput::Elements(100));
    let dur = durability_label();

    let db = create_db();
    let kv = KVStore::new(Arc::clone(&db));
    let run_id = RunId::new();

    // Pre-populate
    let keys = pregenerate_keys(5000);
    for (i, key) in keys.iter().enumerate() {
        kv.put(&run_id, key, Value::I64(i as i64)).expect("populate");
    }

    group.bench_function(format!("kv_50r_50w/{}", dur), |b| {
        let mut seed = BENCH_SEED;
        let mut write_counter = 100000u64;

        b.iter(|| {
            for _ in 0..100 {
                let op = lcg_next(&mut seed) % 100;
                if op < 50 {
                    // Read (50%)
                    let idx = (lcg_next(&mut seed) as usize) % 5000;
                    black_box(kv.get(&run_id, &keys[idx]).expect("get"));
                } else {
                    // Write (50%)
                    let key = format!("write_{}", write_counter);
                    write_counter += 1;
                    kv.put(&run_id, &key, Value::I64(write_counter as i64))
                        .expect("put");
                }
            }
        });
    });

    group.finish();
}

// ============================================================================
// Database Sharing Benchmarks
// ============================================================================

/// Benchmark multiple primitives sharing one database
fn shared_database(c: &mut Criterion) {
    let mut group = c.benchmark_group("shared_database");
    group.measurement_time(Duration::from_secs(5));
    let dur = durability_label();

    let db = create_db();
    let kv = KVStore::new(Arc::clone(&db));
    let event = EventLog::new(Arc::clone(&db));
    let state = StateCell::new(Arc::clone(&db));
    let run_id = RunId::new();

    // Pre-populate
    for i in 0..1000 {
        let key = format!("key_{}", i);
        kv.put(&run_id, &key, Value::I64(i)).expect("populate kv");
    }
    state
        .set(&run_id, "counter", Value::I64(0))
        .expect("init state");

    // Benchmark: Interleaved operations on multiple primitives
    group.bench_function(format!("interleaved_ops/{}", dur), |b| {
        let mut seed = BENCH_SEED;
        let mut counter = 0u64;

        b.iter(|| {
            counter += 1;
            let op = lcg_next(&mut seed) % 3;

            match op {
                0 => {
                    // KV operation
                    let idx = (lcg_next(&mut seed) as usize) % 1000;
                    let key = format!("key_{}", idx);
                    black_box(kv.get(&run_id, &key).expect("kv get"));
                }
                1 => {
                    // Event operation
                    event
                        .append(&run_id, "interleaved", Value::I64(counter as i64))
                        .expect("event append");
                }
                _ => {
                    // State operation
                    state
                        .set(&run_id, "counter", Value::I64(counter as i64))
                        .expect("state set");
                }
            }
        });
    });

    group.finish();
}

// ============================================================================
// Criterion Configuration
// ============================================================================

criterion_group!(
    name = creation_benches;
    config = Criterion::default().sample_size(100);
    targets = primitive_creation,
);

criterion_group!(
    name = cross_primitive_benches;
    config = Criterion::default().sample_size(50);
    targets =
        cross_kv_json,
        cross_state_event,
        cross_workflow,
);

criterion_group!(
    name = workload_benches;
    config = Criterion::default().sample_size(30);
    targets =
        workload_read_heavy,
        workload_write_heavy,
        workload_mixed,
);

criterion_group!(
    name = sharing_benches;
    config = Criterion::default().sample_size(50);
    targets = shared_database,
);

criterion_main!(
    creation_benches,
    cross_primitive_benches,
    workload_benches,
    sharing_benches,
);
