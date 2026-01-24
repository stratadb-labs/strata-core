//! EventLog Semantic Invariant Tests
//!
//! EventLog has hash chain integrity that must be preserved.
//! These tests verify append-only semantics, sequence monotonicity,
//! and hash chain validity across all durability modes.
//!
//! Note: EventLog payloads must be JSON objects (Value::Object).

use super::*;
use strata_core::contract::Version;
use strata_core::types::RunId;
use strata_core::value::Value;
use strata_primitives::EventLog;
use std::sync::{Arc, Barrier};
use std::thread;

/// Sequence numbers are monotonic with no gaps
#[test]
fn eventlog_sequence_monotonicity() {
    test_across_modes("eventlog_sequence_monotonicity", |db| {
        let events = EventLog::new(db);
        let run_id = RunId::new();

        // Append 100 events
        for i in 0..100 {
            events.append(&run_id, "test", wrap_payload(Value::Int(i))).unwrap();
        }

        // Read all and verify sequence
        let all = events.read_range(&run_id, 0, 100).unwrap();

        assert_eq!(all.len(), 100, "Should have 100 events");

        for (i, event) in all.iter().enumerate() {
            assert_eq!(
                event.value.sequence, i as u64,
                "Sequence mismatch at index {}: expected {}, got {}",
                i, i, event.value.sequence
            );
        }

        true
    });
}

/// Hash chain integrity: Each event's prev_hash matches previous event's hash
#[test]
fn eventlog_hash_chain_integrity() {
    test_across_modes("eventlog_hash_chain_integrity", |db| {
        let events = EventLog::new(db);
        let run_id = RunId::new();

        // Append events
        for i in 0..50 {
            events
                .append(&run_id, &format!("type_{}", i % 5), wrap_payload(Value::Int(i)))
                .unwrap();
        }

        // Verify chain
        let verification = events.verify_chain(&run_id).unwrap();

        assert!(
            verification.is_valid,
            "Hash chain invalid: {:?}",
            verification.error
        );
        assert_eq!(verification.length, 50);

        true
    });
}

/// Concurrent appends maintain sequence and hash chain
#[test]
fn eventlog_concurrent_append_integrity() {
    let db = create_inmemory_db();
    let events = EventLog::new(db);
    let run_id = RunId::new();

    const NUM_THREADS: usize = 4;
    const EVENTS_PER_THREAD: usize = 25;

    let barrier = Arc::new(Barrier::new(NUM_THREADS));

    let handles: Vec<_> = (0..NUM_THREADS)
        .map(|thread_id| {
            let events = EventLog::new(events.database().clone());
            let run_id = run_id;
            let barrier = Arc::clone(&barrier);

            thread::spawn(move || {
                barrier.wait();
                for i in 0..EVENTS_PER_THREAD {
                    events
                        .append(
                            &run_id,
                            &format!("thread_{}", thread_id),
                            wrap_payload(Value::Int(i as i64)),
                        )
                        .unwrap();
                }
            })
        })
        .collect();

    for h in handles {
        h.join().unwrap();
    }

    // Verify results
    let total_expected = NUM_THREADS * EVENTS_PER_THREAD;
    let len = events.len(&run_id).unwrap();
    assert_eq!(
        len, total_expected as u64,
        "Expected {} events, got {}",
        total_expected, len
    );

    // Check sequence numbers
    let all = events
        .read_range(&run_id, 0, total_expected as u64)
        .unwrap();
    for (i, event) in all.iter().enumerate() {
        assert_eq!(
            event.value.sequence, i as u64,
            "Sequence gap or duplicate at index {}",
            i
        );
    }

    // Verify hash chain
    let verification = events.verify_chain(&run_id).unwrap();
    assert!(
        verification.is_valid,
        "Hash chain broken after concurrent appends: {:?}",
        verification.error
    );
}

/// Append returns correct sequence number
#[test]
fn eventlog_append_returns_sequence() {
    test_across_modes("eventlog_append_returns_sequence", |db| {
        let events = EventLog::new(db);
        let run_id = RunId::new();

        let mut sequences = Vec::new();
        for i in 0..10 {
            let version = events.append(&run_id, "test", wrap_payload(Value::Int(i))).unwrap();
            if let Version::Sequence(seq) = version {
                sequences.push(seq);
            }
        }

        // Sequences should be 0, 1, 2, ..., 9
        for (i, seq) in sequences.iter().enumerate() {
            assert_eq!(*seq, i as u64, "Sequence mismatch");
        }

        true
    });
}

/// Read by sequence returns correct event
#[test]
fn eventlog_read_by_sequence() {
    test_across_modes("eventlog_read_by_sequence", |db| {
        let events = EventLog::new(db);
        let run_id = RunId::new();

        // Append events with distinct payloads
        for i in 0..20 {
            events
                .append(&run_id, "numbered", wrap_payload(Value::Int(i * 100)))
                .unwrap();
        }

        // Read specific sequences
        for i in 0..20 {
            let event = events.read(&run_id, i).unwrap().unwrap();
            assert_eq!(event.value.sequence, i);
            assert_eq!(event.value.payload, wrap_payload(Value::Int(i as i64 * 100)));
        }

        true
    });
}

/// Read range returns correct slice
#[test]
fn eventlog_read_range() {
    test_across_modes("eventlog_read_range", |db| {
        let events = EventLog::new(db);
        let run_id = RunId::new();

        for i in 0..100 {
            events.append(&run_id, "range_test", wrap_payload(Value::Int(i))).unwrap();
        }

        // Read middle slice
        let slice = events.read_range(&run_id, 25, 75).unwrap();

        assert_eq!(slice.len(), 50);
        assert_eq!(slice[0].value.sequence, 25);
        assert_eq!(slice[49].value.sequence, 74);

        for (i, event) in slice.iter().enumerate() {
            assert_eq!(event.value.sequence, (25 + i) as u64);
            assert_eq!(event.value.payload, wrap_payload(Value::Int((25 + i) as i64)));
        }

        true
    });
}

/// Head returns most recent event
#[test]
fn eventlog_head() {
    test_across_modes("eventlog_head", |db| {
        let events = EventLog::new(db);
        let run_id = RunId::new();

        // Empty log has no head
        assert!(events.head(&run_id).unwrap().is_none());

        // After appends, head is latest
        for i in 0..10 {
            events.append(&run_id, "test", wrap_payload(Value::Int(i))).unwrap();

            let head = events.head(&run_id).unwrap().unwrap();
            assert_eq!(head.value.sequence, i as u64);
            assert_eq!(head.value.payload, wrap_payload(Value::Int(i)));
        }

        true
    });
}

/// Event types are preserved
#[test]
fn eventlog_event_type_preserved() {
    test_across_modes("eventlog_event_type_preserved", |db| {
        let events = EventLog::new(db);
        let run_id = RunId::new();

        events.append(&run_id, "type_a", wrap_payload(Value::Int(1))).unwrap();
        events.append(&run_id, "type_b", wrap_payload(Value::Int(2))).unwrap();
        events.append(&run_id, "type_a", wrap_payload(Value::Int(3))).unwrap();

        let all = events.read_range(&run_id, 0, 3).unwrap();

        assert_eq!(all[0].value.event_type, "type_a");
        assert_eq!(all[1].value.event_type, "type_b");
        assert_eq!(all[2].value.event_type, "type_a");

        true
    });
}

/// Read by type filters correctly
#[test]
fn eventlog_read_by_type() {
    test_across_modes("eventlog_read_by_type", |db| {
        let events = EventLog::new(db);
        let run_id = RunId::new();

        // Mix of types
        for i in 0..30 {
            let event_type = match i % 3 {
                0 => "alpha",
                1 => "beta",
                _ => "gamma",
            };
            events.append(&run_id, event_type, wrap_payload(Value::Int(i))).unwrap();
        }

        let alphas = events.read_by_type(&run_id, "alpha").unwrap();
        let betas = events.read_by_type(&run_id, "beta").unwrap();
        let gammas = events.read_by_type(&run_id, "gamma").unwrap();

        assert_eq!(alphas.len(), 10);
        assert_eq!(betas.len(), 10);
        assert_eq!(gammas.len(), 10);

        // Verify all alphas have correct type
        for event in &alphas {
            assert_eq!(event.value.event_type, "alpha");
        }

        true
    });
}

/// Events are isolated per run
#[test]
fn eventlog_run_isolation() {
    test_across_modes("eventlog_run_isolation", |db| {
        let events = EventLog::new(db);
        let run_a = RunId::new();
        let run_b = RunId::new();

        // Append to run A
        for i in 0..10 {
            events.append(&run_a, "run_a", wrap_payload(Value::Int(i))).unwrap();
        }

        // Append to run B
        for i in 0..5 {
            events.append(&run_b, "run_b", wrap_payload(Value::Int(i * 100))).unwrap();
        }

        // Verify isolation
        assert_eq!(events.len(&run_a).unwrap(), 10);
        assert_eq!(events.len(&run_b).unwrap(), 5);

        let a_events = events.read_range(&run_a, 0, 10).unwrap();
        let b_events = events.read_range(&run_b, 0, 5).unwrap();

        for event in &a_events {
            assert_eq!(event.value.event_type, "run_a");
        }

        for event in &b_events {
            assert_eq!(event.value.event_type, "run_b");
        }

        true
    });
}

/// Timestamps are monotonically non-decreasing
#[test]
fn eventlog_timestamp_ordering() {
    test_across_modes("eventlog_timestamp_ordering", |db| {
        let events = EventLog::new(db);
        let run_id = RunId::new();

        for i in 0..20 {
            events.append(&run_id, "timed", wrap_payload(Value::Int(i))).unwrap();
        }

        let all = events.read_range(&run_id, 0, 20).unwrap();

        let mut last_ts = 0i64;
        for event in &all {
            assert!(
                event.value.timestamp >= last_ts,
                "Timestamp regression: {} < {}",
                event.value.timestamp,
                last_ts
            );
            last_ts = event.value.timestamp;
        }

        true
    });
}

/// Empty log behaviors
#[test]
fn eventlog_empty_behaviors() {
    test_across_modes("eventlog_empty_behaviors", |db| {
        let events = EventLog::new(db);
        let run_id = RunId::new();

        assert_eq!(events.len(&run_id).unwrap(), 0);
        assert!(events.is_empty(&run_id).unwrap());
        assert!(events.head(&run_id).unwrap().is_none());
        assert!(events.read(&run_id, 0).unwrap().is_none());
        assert!(events.read_range(&run_id, 0, 10).unwrap().is_empty());

        // Verify chain on empty is valid
        let verification = events.verify_chain(&run_id).unwrap();
        assert!(verification.is_valid);

        true
    });
}

#[cfg(test)]
mod eventlog_unit_tests {
    use super::*;

    #[test]
    fn test_simple_append_read() {
        let db = create_inmemory_db();
        let events = EventLog::new(db);
        let run_id = RunId::new();

        let version = events
            .append(&run_id, "test", wrap_payload(Value::String("hello".to_string())))
            .unwrap();

        assert!(matches!(version, Version::Sequence(0)));

        let read = events.read(&run_id, 0).unwrap().unwrap();
        assert_eq!(read.value.event_type, "test");
        assert_eq!(read.value.payload, wrap_payload(Value::String("hello".to_string())));
    }

    #[test]
    fn test_hash_links() {
        let db = create_inmemory_db();
        let events = EventLog::new(db);
        let run_id = RunId::new();

        let version0 = events.append(&run_id, "a", wrap_payload(Value::Int(0))).unwrap();
        let version1 = events.append(&run_id, "b", wrap_payload(Value::Int(1))).unwrap();
        let version2 = events.append(&run_id, "c", wrap_payload(Value::Int(2))).unwrap();

        // Sequences should be monotonic
        assert!(matches!(version0, Version::Sequence(0)));
        assert!(matches!(version1, Version::Sequence(1)));
        assert!(matches!(version2, Version::Sequence(2)));

        // Read back and verify chain
        let e0 = events.read(&run_id, 0).unwrap().unwrap();
        let e1 = events.read(&run_id, 1).unwrap().unwrap();
        let e2 = events.read(&run_id, 2).unwrap().unwrap();

        // Hashes should be non-zero (indicating actual computation happened)
        assert_ne!(e0.value.hash, [0u8; 32]);
        assert_ne!(e1.value.hash, [0u8; 32]);
        assert_ne!(e2.value.hash, [0u8; 32]);

        // e1's prev_hash should match e0's hash
        assert_eq!(e1.value.prev_hash, e0.value.hash);
    }
}
