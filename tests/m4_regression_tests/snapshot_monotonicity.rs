//! Snapshot Monotonicity Tests
//!
//! Critical for version chain correctness: Once a snapshot sees version X,
//! it must never later see something older than X.
//!
//! This catches bugs in version chain traversal where the wrong version
//! might be returned due to race conditions or incorrect version filtering.

use super::*;
use strata_core::types::RunId;
use strata_core::value::Value;
use strata_primitives::{EventLog, KVStore, StateCell};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Barrier};
use std::thread;
use std::time::Duration;

/// KV: Repeated reads within a session must return consistent values
#[test]
fn kv_repeated_read_consistency() {
    test_across_modes("kv_repeated_read_consistency", |db| {
        let kv = KVStore::new(db);
        let run_id = RunId::new();

        // Write initial value
        kv.put(&run_id, "key", Value::Int(42)).unwrap();

        // Read multiple times - should always get same value
        let mut values = Vec::new();
        for _ in 0..100 {
            let v = kv.get(&run_id, "key").unwrap();
            values.push(v);
        }

        // All values should be identical
        let first = &values[0];
        for (i, v) in values.iter().enumerate() {
            assert_eq!(
                v, first,
                "READ INCONSISTENCY at iteration {}: got {:?}, expected {:?}",
                i, v, first
            );
        }

        true
    });
}

/// KV: Reads under concurrent writes must not see version regression
#[test]
fn kv_no_version_regression_under_writes() {
    let db = create_inmemory_db();
    let kv = KVStore::new(db.clone());
    let run_id = RunId::new();

    // Initialize
    kv.put(&run_id, "counter", Value::Int(0)).unwrap();

    let running = Arc::new(AtomicBool::new(true));
    let regression_detected = Arc::new(AtomicBool::new(false));
    let max_version_seen = Arc::new(AtomicU64::new(0));

    // Writer thread: continuously increment
    let writer_running = Arc::clone(&running);
    let writer_kv = KVStore::new(db.clone());
    let writer_run_id = run_id;
    let writer_handle = thread::spawn(move || {
        let mut counter = 0i64;
        while writer_running.load(Ordering::Relaxed) {
            counter += 1;
            let _ = writer_kv.put(&writer_run_id, "counter", Value::Int(counter));
            // Small sleep to not completely saturate
            thread::sleep(Duration::from_micros(10));
        }
    });

    // Reader thread: check for version regression
    let _reader_running = Arc::clone(&running);
    let reader_regression = Arc::clone(&regression_detected);
    let reader_max = Arc::clone(&max_version_seen);
    let reader_kv = KVStore::new(db);
    let reader_run_id = run_id;
    let reader_handle = thread::spawn(move || {
        let mut last_value = 0i64;

        for _ in 0..1000 {
            if let Ok(Some(versioned)) = reader_kv.get(&reader_run_id, "counter") {
                if let Value::Int(current) = versioned.value {
                    // Value should never decrease (version regression proxy)
                    if current < last_value {
                        reader_regression.store(true, Ordering::Relaxed);
                        eprintln!(
                            "VERSION REGRESSION: saw {} after seeing {}",
                            current, last_value
                        );
                    }
                    if current > last_value {
                        last_value = current;
                        reader_max.store(current as u64, Ordering::Relaxed);
                    }
                }
            }
            thread::sleep(Duration::from_micros(5));
        }
    });

    // Let it run for a bit
    thread::sleep(Duration::from_millis(100));
    running.store(false, Ordering::Relaxed);

    writer_handle.join().unwrap();
    reader_handle.join().unwrap();

    assert!(
        !regression_detected.load(Ordering::Relaxed),
        "VERSION REGRESSION DETECTED in concurrent read/write test"
    );

    let max = max_version_seen.load(Ordering::Relaxed);
    println!("Max version seen during test: {}", max);
    assert!(max > 0, "Should have seen some updates");
}

/// StateCell: Repeated reads must be stable
#[test]
fn statecell_repeated_read_stability() {
    test_across_modes("statecell_repeated_read_stability", |db| {
        let state = StateCell::new(db);
        let run_id = RunId::new();

        // Initialize
        state.init(&run_id, "cell", Value::Int(100)).unwrap();

        // Read multiple times
        let mut versions = Vec::new();
        let mut values = Vec::new();

        for _ in 0..100 {
            let s = state.read(&run_id, "cell").unwrap().unwrap();
            versions.push(s.version);
            values.push(s.value.clone());
        }

        // All reads should return identical state
        let first_version = versions[0];
        let first_value = &values[0];

        for (i, (v, val)) in versions.iter().zip(values.iter()).enumerate() {
            assert_eq!(
                *v, first_version,
                "VERSION DRIFT at iteration {}: {} != {}",
                i, v, first_version
            );
            assert_eq!(
                val, first_value,
                "VALUE DRIFT at iteration {}: {:?} != {:?}",
                i, val, first_value
            );
        }

        true
    });
}

/// StateCell: Version must never decrease within observation window
#[test]
fn statecell_version_monotonicity() {
    let db = create_inmemory_db();
    let state = StateCell::new(db.clone());
    let run_id = RunId::new();

    state.init(&run_id, "counter", Value::Int(0)).unwrap();

    let running = Arc::new(AtomicBool::new(true));
    let monotonicity_violated = Arc::new(AtomicBool::new(false));

    // Writer: continuous updates
    let writer_running = Arc::clone(&running);
    let writer_state = StateCell::new(db.clone());
    let writer_run_id = run_id;
    let writer_handle = thread::spawn(move || {
        for i in 0..500 {
            let _ = writer_state.set(&writer_run_id, "counter", Value::Int(i));
        }
        writer_running.store(false, Ordering::Relaxed);
    });

    // Reader: check version monotonicity
    let reader_running = Arc::clone(&running);
    let reader_violated = Arc::clone(&monotonicity_violated);
    let reader_state = StateCell::new(db);
    let reader_run_id = run_id;
    let reader_handle = thread::spawn(move || {
        let mut max_version = 0u64;

        while reader_running.load(Ordering::Relaxed) {
            if let Ok(Some(s)) = reader_state.read(&reader_run_id, "counter") {
                let version = s.value.version;
                if version < max_version {
                    reader_violated.store(true, Ordering::Relaxed);
                    eprintln!(
                        "MONOTONICITY VIOLATED: saw version {} after {}",
                        version, max_version
                    );
                }
                if version > max_version {
                    max_version = version;
                }
            }
        }
    });

    writer_handle.join().unwrap();
    reader_handle.join().unwrap();

    assert!(
        !monotonicity_violated.load(Ordering::Relaxed),
        "VERSION MONOTONICITY VIOLATED in StateCell"
    );
}

/// EventLog: Sequence numbers must be stable within reads
#[test]
fn eventlog_sequence_stability() {
    test_across_modes("eventlog_sequence_stability", |db| {
        let events = EventLog::new(db);
        let run_id = RunId::new();

        // Append some events
        for i in 0..10 {
            events.append(&run_id, "test", wrap_payload(Value::Int(i))).unwrap();
        }

        // Read range multiple times
        for _ in 0..50 {
            let range = events.read_range(&run_id, 0, 10).unwrap();

            // Check sequence monotonicity
            for (i, event) in range.iter().enumerate() {
                assert_eq!(
                    event.value.sequence, i as u64,
                    "SEQUENCE MISMATCH: expected {}, got {}",
                    i, event.value.sequence
                );
            }

            // Check we got all events
            assert_eq!(range.len(), 10, "Should have 10 events");
        }

        true
    });
}

/// EventLog: Concurrent appends don't cause sequence regression in reads
#[test]
fn eventlog_no_sequence_regression() {
    let db = create_inmemory_db();
    let _events = EventLog::new(db.clone());
    let run_id = RunId::new();

    let regression_detected = Arc::new(AtomicBool::new(false));
    let barrier = Arc::new(Barrier::new(3));

    // Two writers
    let writer_handles: Vec<_> = (0..2)
        .map(|writer_id| {
            let events = EventLog::new(db.clone());
            let run_id = run_id;
            let barrier = Arc::clone(&barrier);

            thread::spawn(move || {
                barrier.wait();
                for i in 0..100 {
                    let _ = events.append(&run_id, &format!("writer_{}", writer_id), wrap_payload(Value::Int(i)));
                }
            })
        })
        .collect();

    // One reader checking for regression
    let reader_events = EventLog::new(db);
    let reader_run_id = run_id;
    let reader_regression = Arc::clone(&regression_detected);
    let reader_barrier = Arc::clone(&barrier);
    let reader_handle = thread::spawn(move || {
        reader_barrier.wait();
        let mut max_seq = 0u64;

        for _ in 0..200 {
            if let Ok(Some(head)) = reader_events.head(&reader_run_id) {
                if head.value.sequence < max_seq {
                    reader_regression.store(true, Ordering::Relaxed);
                    eprintln!(
                        "SEQUENCE REGRESSION: head sequence {} < previous max {}",
                        head.value.sequence, max_seq
                    );
                }
                max_seq = max_seq.max(head.value.sequence);
            }
            thread::sleep(Duration::from_micros(50));
        }
    });

    for handle in writer_handles {
        handle.join().unwrap();
    }
    reader_handle.join().unwrap();

    assert!(
        !regression_detected.load(Ordering::Relaxed),
        "SEQUENCE REGRESSION DETECTED in EventLog"
    );
}

/// Cross-primitive: Transaction reads must be stable
#[test]
fn transaction_read_stability() {
    test_across_modes("transaction_read_stability", |db| {
        let kv = KVStore::new(db.clone());
        let run_id = RunId::new();

        // Setup: write some data
        kv.put(&run_id, "a", Value::Int(1)).unwrap();
        kv.put(&run_id, "b", Value::Int(2)).unwrap();
        kv.put(&run_id, "c", Value::Int(3)).unwrap();

        // Within a transaction, repeated reads should be identical
        let result = kv.transaction(&run_id, |txn| {
            let mut reads_a = Vec::new();
            let mut reads_b = Vec::new();
            let mut reads_c = Vec::new();

            for _ in 0..10 {
                reads_a.push(txn.get("a")?);
                reads_b.push(txn.get("b")?);
                reads_c.push(txn.get("c")?);
            }

            // Check all reads of same key are identical
            for reads in [&reads_a, &reads_b, &reads_c] {
                let first = &reads[0];
                for r in reads {
                    if r != first {
                        return Ok(false);
                    }
                }
            }

            Ok(true)
        });

        result.unwrap()
    });
}

#[cfg(test)]
mod monotonicity_unit_tests {
    use super::*;

    #[test]
    fn test_single_write_read_consistent() {
        let db = create_inmemory_db();
        let kv = KVStore::new(db);
        let run_id = RunId::new();

        kv.put(&run_id, "x", Value::Int(42)).unwrap();

        for _ in 0..10 {
            let v = kv.get(&run_id, "x").unwrap();
            assert_eq!(v.map(|v| v.value), Some(Value::Int(42)));
        }
    }

    #[test]
    fn test_sequential_writes_monotonic() {
        let db = create_inmemory_db();
        let state = StateCell::new(db);
        let run_id = RunId::new();

        let mut versions = Vec::new();
        versions.push(state.init(&run_id, "x", Value::Int(0)).unwrap().value);

        for i in 1..10 {
            let v = state.set(&run_id, "x", Value::Int(i)).unwrap().value;
            versions.push(v);
        }

        // Check monotonicity
        for i in 1..versions.len() {
            assert!(
                versions[i] > versions[i - 1],
                "Version {} ({}) should be > version {} ({})",
                i,
                versions[i],
                i - 1,
                versions[i - 1]
            );
        }
    }
}
