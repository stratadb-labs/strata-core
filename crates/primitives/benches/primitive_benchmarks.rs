//! Primitive Performance Benchmarks (Story #200)
//!
//! Performance targets from M3_ARCHITECTURE.md Section 14:
//! - KV put: >10K ops/sec
//! - KV get: >20K ops/sec
//! - EventLog append: >5K ops/sec
//! - StateCell CAS: >5K ops/sec
//! - TraceStore record: >2K ops/sec (index overhead)
//! - Cross-primitive txn: >1K ops/sec

use criterion::{criterion_group, criterion_main, Criterion, Throughput};
use strata_core::types::RunId;
use strata_core::value::Value;
use strata_engine::Database;
use strata_primitives::{
    EventLog, EventLogExt, KVStore, KVStoreExt, StateCell, StateCellExt, TraceStore, TraceStoreExt,
    TraceType,
};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tempfile::TempDir;

fn setup_db() -> (Arc<Database>, TempDir, RunId) {
    let temp_dir = TempDir::new().unwrap();
    let db = Arc::new(Database::open(temp_dir.path()).unwrap());
    let run_id = RunId::new();
    (db, temp_dir, run_id)
}

/// Benchmark KV put operations
/// Target: >10K ops/sec
fn bench_kv_put(c: &mut Criterion) {
    let (db, _temp, run_id) = setup_db();
    let kv = KVStore::new(db.clone());

    let mut group = c.benchmark_group("kv");
    group.throughput(Throughput::Elements(1));

    let counter = AtomicU64::new(0);
    group.bench_function("put", |b| {
        b.iter(|| {
            let i = counter.fetch_add(1, Ordering::SeqCst);
            kv.put(&run_id, &format!("key{}", i), Value::I64(i as i64))
                .unwrap()
        })
    });
    group.finish();
}

/// Benchmark KV get operations
/// Target: >20K ops/sec
fn bench_kv_get(c: &mut Criterion) {
    let (db, _temp, run_id) = setup_db();
    let kv = KVStore::new(db.clone());

    // Pre-populate 1000 keys
    for i in 0..1000 {
        kv.put(&run_id, &format!("key{}", i), Value::I64(i as i64))
            .unwrap();
    }

    let mut group = c.benchmark_group("kv");
    group.throughput(Throughput::Elements(1));

    let counter = AtomicU64::new(0);
    group.bench_function("get", |b| {
        b.iter(|| {
            let i = counter.fetch_add(1, Ordering::SeqCst) % 1000;
            kv.get(&run_id, &format!("key{}", i)).unwrap()
        })
    });
    group.finish();
}

/// Benchmark EventLog append operations
/// Target: >5K ops/sec
fn bench_event_append(c: &mut Criterion) {
    let (db, _temp, run_id) = setup_db();
    let event_log = EventLog::new(db.clone());

    let mut group = c.benchmark_group("event_log");
    group.throughput(Throughput::Elements(1));

    group.bench_function("append", |b| {
        b.iter(|| {
            event_log
                .append(&run_id, "benchmark_event", Value::I64(42))
                .unwrap()
        })
    });
    group.finish();
}

/// Benchmark StateCell CAS operations
/// Target: >5K ops/sec
fn bench_state_cas(c: &mut Criterion) {
    let (db, _temp, run_id) = setup_db();
    let state_cell = StateCell::new(db.clone());

    // Initialize the cell
    state_cell
        .init(&run_id, "bench_cell", Value::I64(0))
        .unwrap();

    let mut group = c.benchmark_group("state_cell");
    group.throughput(Throughput::Elements(1));

    // Use transition which handles version automatically
    group.bench_function("cas", |b| {
        b.iter(|| {
            state_cell
                .transition(&run_id, "bench_cell", |state| {
                    let val = match &state.value {
                        Value::I64(n) => *n,
                        _ => 0,
                    };
                    Ok((Value::I64(val + 1), val + 1))
                })
                .unwrap()
        })
    });
    group.finish();
}

/// Benchmark TraceStore record operations
/// Target: >2K ops/sec (higher due to index overhead)
fn bench_trace_record(c: &mut Criterion) {
    let (db, _temp, run_id) = setup_db();
    let trace_store = TraceStore::new(db.clone());

    let mut group = c.benchmark_group("trace_store");
    group.throughput(Throughput::Elements(1));

    group.bench_function("record", |b| {
        b.iter(|| {
            trace_store
                .record(
                    &run_id,
                    TraceType::Thought {
                        content: "benchmark thought".into(),
                        confidence: Some(0.95),
                    },
                    vec![],
                    Value::Null,
                )
                .unwrap()
        })
    });
    group.finish();
}

/// Benchmark cross-primitive transactions
/// Target: >1K ops/sec
fn bench_cross_primitive_transaction(c: &mut Criterion) {
    let (db, _temp, run_id) = setup_db();

    // Initialize state cell for the transaction
    let state_cell = StateCell::new(db.clone());
    state_cell.init(&run_id, "txn_cell", Value::I64(0)).unwrap();

    let mut group = c.benchmark_group("cross_primitive");
    group.throughput(Throughput::Elements(1));

    let counter = AtomicU64::new(0);
    group.bench_function("4_primitive_txn", |b| {
        b.iter(|| {
            let n = counter.fetch_add(1, Ordering::SeqCst);
            db.transaction(run_id, |txn| {
                txn.kv_put(&format!("txn_key{}", n), Value::I64(n as i64))?;
                txn.event_append("txn_event", Value::I64(n as i64))?;
                txn.state_set("txn_cell", Value::I64(n as i64))?;
                txn.trace_record("Thought", Value::String(format!("txn {}", n)))?;
                Ok(())
            })
            .unwrap()
        })
    });
    group.finish();
}

/// Benchmark EventLog read operations
fn bench_event_read(c: &mut Criterion) {
    let (db, _temp, run_id) = setup_db();
    let event_log = EventLog::new(db.clone());

    // Pre-populate 1000 events
    for i in 0..1000 {
        event_log
            .append(&run_id, "numbered", Value::I64(i as i64))
            .unwrap();
    }

    let mut group = c.benchmark_group("event_log");
    group.throughput(Throughput::Elements(1));

    let counter = AtomicU64::new(0);
    group.bench_function("read", |b| {
        b.iter(|| {
            let i = counter.fetch_add(1, Ordering::SeqCst) % 1000;
            event_log.read(&run_id, i).unwrap()
        })
    });
    group.finish();
}

/// Benchmark StateCell read operations
fn bench_state_read(c: &mut Criterion) {
    let (db, _temp, run_id) = setup_db();
    let state_cell = StateCell::new(db.clone());

    // Initialize the cell
    state_cell
        .init(&run_id, "read_cell", Value::I64(42))
        .unwrap();

    let mut group = c.benchmark_group("state_cell");
    group.throughput(Throughput::Elements(1));

    group.bench_function("read", |b| {
        b.iter(|| state_cell.read(&run_id, "read_cell").unwrap())
    });
    group.finish();
}

/// Benchmark TraceStore query by type
fn bench_trace_query(c: &mut Criterion) {
    let (db, _temp, run_id) = setup_db();
    let trace_store = TraceStore::new(db.clone());

    // Pre-populate 100 traces of each type
    for _ in 0..100 {
        trace_store
            .record(
                &run_id,
                TraceType::Thought {
                    content: "thought".into(),
                    confidence: None,
                },
                vec![],
                Value::Null,
            )
            .unwrap();
        trace_store
            .record(
                &run_id,
                TraceType::ToolCall {
                    tool_name: "tool".into(),
                    arguments: Value::Null,
                    result: None,
                    duration_ms: None,
                },
                vec![],
                Value::Null,
            )
            .unwrap();
    }

    let mut group = c.benchmark_group("trace_store");
    group.throughput(Throughput::Elements(1));

    group.bench_function("query_by_type", |b| {
        b.iter(|| trace_store.query_by_type(&run_id, "Thought").unwrap())
    });
    group.finish();
}

/// Benchmark KV list operations
fn bench_kv_list(c: &mut Criterion) {
    let (db, _temp, run_id) = setup_db();
    let kv = KVStore::new(db.clone());

    // Pre-populate keys with prefix
    for i in 0..100 {
        kv.put(&run_id, &format!("prefix/key{}", i), Value::I64(i as i64))
            .unwrap();
    }
    for i in 0..100 {
        kv.put(&run_id, &format!("other/key{}", i), Value::I64(i as i64))
            .unwrap();
    }

    let mut group = c.benchmark_group("kv");
    group.throughput(Throughput::Elements(1));

    group.bench_function("list", |b| {
        b.iter(|| kv.list(&run_id, Some("prefix/")).unwrap())
    });
    group.finish();
}

criterion_group!(
    benches,
    bench_kv_put,
    bench_kv_get,
    bench_kv_list,
    bench_event_append,
    bench_event_read,
    bench_state_cas,
    bench_state_read,
    bench_trace_record,
    bench_trace_query,
    bench_cross_primitive_transaction,
);
criterion_main!(benches);
