//! Audit test for issue #972: Event append 100x slower than KV put in Cache mode
//!
//! Event append in Cache mode takes ~1ms p50 — orders of magnitude
//! slower than KV put (~1µs). The root cause is that EventLogMeta stores a
//! `sequences: Vec<u64>` per stream type that grows linearly with every append.
//! Each append must deserialize and re-serialize this growing metadata, making
//! append O(N) instead of O(1).
//!
//! The fix: remove the unbounded `sequences` vector from StreamMeta and use
//! separate per-type index keys instead. This keeps metadata constant-size
//! regardless of event count.

use std::collections::HashMap;
use std::time::Instant;
use strata_core::types::BranchId;
use strata_core::Value;
use strata_engine::primitives::EventLog;
use strata_engine::Database;
use tempfile::TempDir;

/// Helper: create a cache-mode database.
fn cache_db() -> (std::sync::Arc<Database>, TempDir) {
    let dir = TempDir::new().expect("tempdir");
    let db = Database::open(dir.path()).expect("open db");
    (db, dir)
}

/// Helper: create a payload object.
fn test_payload(i: usize) -> Value {
    Value::Object(HashMap::from([
        ("data".to_string(), Value::String(format!("event_{}", i))),
        ("index".to_string(), Value::Int(i as i64)),
    ]))
}

#[test]
fn event_append_does_not_degrade_linearly() {
    // This test verifies that append latency doesn't grow with event count.
    // Before the fix, appending event #N required serializing metadata containing
    // all N-1 previous sequence numbers, making it O(N) per append.
    let (db, _dir) = cache_db();
    let log = EventLog::new(db);
    let branch = BranchId::new();

    let warmup = 50;
    let batch_size = 50;

    // Warm up: append some events
    for i in 0..warmup {
        log.append(&branch, "test", test_payload(i)).unwrap();
    }

    // Measure first batch (events 50-99)
    let start = Instant::now();
    for i in warmup..(warmup + batch_size) {
        log.append(&branch, "test", test_payload(i)).unwrap();
    }
    let early_elapsed = start.elapsed();

    // Append more events to grow the metadata
    for i in (warmup + batch_size)..2000 {
        log.append(&branch, "test", test_payload(i)).unwrap();
    }

    // Measure late batch (events 2000-2049)
    let start = Instant::now();
    for i in 2000..(2000 + batch_size) {
        log.append(&branch, "test", test_payload(i)).unwrap();
    }
    let late_elapsed = start.elapsed();

    // With the fix, late appends should not be significantly slower than early appends.
    // Before the fix, late appends were ~10x slower due to O(N) metadata serialization.
    // Allow up to 3x degradation for noise/overhead margin.
    let ratio = late_elapsed.as_nanos() as f64 / early_elapsed.as_nanos() as f64;
    assert!(
        ratio < 3.0,
        "Event append degraded {:.1}x from early ({:?}) to late ({:?}). \
         Expected < 3x degradation (O(1) metadata). \
         Before fix, metadata grows O(N) causing ~10x degradation.",
        ratio,
        early_elapsed,
        late_elapsed
    );
}

#[test]
fn event_append_is_fast_in_cache_mode() {
    // Event append in cache mode (no durability) should complete in reasonable time.
    // 100 appends should take well under 100ms with constant-size metadata.
    let (db, _dir) = cache_db();
    let log = EventLog::new(db);
    let branch = BranchId::new();

    // Warm up
    for i in 0..10 {
        log.append(&branch, "warmup", test_payload(i)).unwrap();
    }

    let start = Instant::now();
    for i in 0..100 {
        log.append(&branch, "perf", test_payload(i)).unwrap();
    }
    let elapsed = start.elapsed();

    // 100 appends should complete in under 50ms in cache mode
    assert!(
        elapsed < std::time::Duration::from_millis(50),
        "100 event appends took {:?}, expected < 50ms in cache mode",
        elapsed
    );
}

#[test]
fn read_by_type_still_works_after_optimization() {
    // Verify that read_by_type returns correct results after removing
    // the sequences index from StreamMeta.
    let (db, _dir) = cache_db();
    let log = EventLog::new(db);
    let branch = BranchId::new();

    // Append events of different types
    log.append(&branch, "type_a", test_payload(1)).unwrap();
    log.append(&branch, "type_b", test_payload(2)).unwrap();
    log.append(&branch, "type_a", test_payload(3)).unwrap();
    log.append(&branch, "type_c", test_payload(4)).unwrap();
    log.append(&branch, "type_a", test_payload(5)).unwrap();
    log.append(&branch, "type_b", test_payload(6)).unwrap();

    // Read by type should return correct events
    let type_a = log.read_by_type(&branch, "type_a").unwrap();
    assert_eq!(type_a.len(), 3, "Expected 3 type_a events");
    assert_eq!(type_a[0].value.sequence, 0);
    assert_eq!(type_a[1].value.sequence, 2);
    assert_eq!(type_a[2].value.sequence, 4);

    let type_b = log.read_by_type(&branch, "type_b").unwrap();
    assert_eq!(type_b.len(), 2, "Expected 2 type_b events");
    assert_eq!(type_b[0].value.sequence, 1);
    assert_eq!(type_b[1].value.sequence, 5);

    let type_c = log.read_by_type(&branch, "type_c").unwrap();
    assert_eq!(type_c.len(), 1, "Expected 1 type_c event");

    let none = log.read_by_type(&branch, "nonexistent").unwrap();
    assert!(none.is_empty());
}

#[test]
fn event_data_integrity_preserved() {
    // Verify that event data (payload, hash chain, timestamps) is correct.
    let (db, _dir) = cache_db();
    let log = EventLog::new(db);
    let branch = BranchId::new();

    let payload = Value::Object(HashMap::from([
        ("tool".to_string(), Value::String("search".into())),
        ("query".to_string(), Value::String("rust async".into())),
    ]));

    log.append(&branch, "tool_call", payload.clone()).unwrap();
    log.append(&branch, "tool_call", test_payload(2)).unwrap();

    // Verify first event
    let event0 = log.read(&branch, 0).unwrap().unwrap();
    assert_eq!(event0.value.event_type, "tool_call");
    assert_eq!(event0.value.payload, payload);
    assert_eq!(event0.value.sequence, 0);
    assert_ne!(event0.value.hash, [0u8; 32]);

    // Verify hash chain
    let event1 = log.read(&branch, 1).unwrap().unwrap();
    assert_eq!(event1.value.prev_hash, event0.value.hash);
    assert_eq!(event1.value.sequence, 1);
}
