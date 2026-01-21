//! Tier 1: EventLog Chain Tests (M3.7-M3.11)
//!
//! These tests verify EventLog invariants around append-only semantics,
//! sequence numbering, and hash chain integrity.
//!
//! ## Invariants Tested
//!
//! - M3.7: Append-Only - Events cannot be modified after append
//! - M3.8: Monotonic Sequences - Sequences are contiguous (0, 1, 2, ...)
//! - M3.9: Hash Chain Integrity - Each event's prev_hash matches previous hash
//! - M3.10: Total Order Under Concurrency - Concurrent appends serialize to total order
//! - M3.11: Metadata Consistency - len() matches actual count, head() returns most recent

use super::test_utils::*;
use strata_core::contract::Version;
use strata_core::types::RunId;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

// ============================================================================
// M3.7: Append-Only Invariant
// ============================================================================
// Events cannot be modified after append.
// Sequence numbers are immutable.
//
// What breaks if this fails?
// Audit log tampering. Events can be silently modified.

mod append_only {
    use super::*;

    #[test]
    fn test_events_are_immutable() {
        let tp = TestPrimitives::new();
        let run_id = tp.run_id;

        // Append an event
        tp.event_log
            .append(&run_id, "initial", values::int(1))
            .unwrap();

        // Read the event to get its hash
        let first_event = tp.event_log.read(&run_id, 0).unwrap().unwrap();
        let seq = first_event.value.sequence;
        let hash = first_event.value.hash;

        // Read it back multiple times - should always be the same
        for _ in 0..10 {
            let event = tp.event_log.read(&run_id, seq).unwrap().unwrap();
            assert_eq!(event.value.sequence, seq);
            assert_eq!(event.value.hash, hash);
            assert_eq!(event.value.event_type, "initial");
            assert_eq!(event.value.payload, values::int(1));
        }
    }

    #[test]
    fn test_append_creates_new_event_not_update() {
        let tp = TestPrimitives::new();
        let run_id = tp.run_id;

        // Append same event type multiple times
        tp.event_log
            .append(&run_id, "event_type", values::int(1))
            .unwrap();
        tp.event_log
            .append(&run_id, "event_type", values::int(2))
            .unwrap();
        tp.event_log
            .append(&run_id, "event_type", values::int(3))
            .unwrap();

        // Should have 3 separate events
        let events = tp.event_log.read_range(&run_id, 0, 100).unwrap();
        assert_eq!(events.len(), 3);

        assert_eq!(events[0].value.payload, values::int(1));
        assert_eq!(events[1].value.payload, values::int(2));
        assert_eq!(events[2].value.payload, values::int(3));
    }

    #[test]
    fn test_sequence_numbers_immutable() {
        let tp = TestPrimitives::new();
        let run_id = tp.run_id;

        // Append multiple events
        for i in 0..10 {
            let version = tp
                .event_log
                .append(&run_id, "event", values::int(i))
                .unwrap();
            if let Version::Sequence(seq) = version {
                assert_eq!(seq, i as u64, "Sequence should be {}", i);
            }
        }

        // Re-read and verify sequences haven't changed
        for i in 0..10 {
            let event = tp.event_log.read(&run_id, i as u64).unwrap().unwrap();
            assert_eq!(event.value.sequence, i as u64, "Sequence {} changed", i);
        }
    }

    #[test]
    fn test_hash_immutable_after_append() {
        let tp = TestPrimitives::new();
        let run_id = tp.run_id;

        tp.event_log
            .append(&run_id, "test", values::int(42))
            .unwrap();

        // Read the event to get its hash
        let first_read = tp.event_log.read(&run_id, 0).unwrap().unwrap();
        let seq = first_read.value.sequence;
        let original_hash = first_read.value.hash;

        // Read multiple times, hash should never change
        for _ in 0..10 {
            let event = tp.event_log.read(&run_id, seq).unwrap().unwrap();
            assert_eq!(event.value.hash, original_hash, "Hash changed!");
        }
    }
}

// ============================================================================
// M3.8: Monotonic Sequence Numbers
// ============================================================================
// Sequences are contiguous (0, 1, 2, ...).
// No gaps after transaction failure.
// Sequence numbers never reused.
//
// What breaks if this fails?
// Lost events or duplicate events. Sequence gaps indicate missing data.

mod monotonic_sequences {
    use super::*;
    use strata_core::error::Error;

    #[test]
    fn test_sequences_start_at_zero() {
        let tp = TestPrimitives::new();
        let run_id = tp.run_id;

        let version = tp
            .event_log
            .append(&run_id, "first", values::null())
            .unwrap();
        assert!(matches!(version, Version::Sequence(0)), "First sequence should be 0");
    }

    #[test]
    fn test_sequences_are_contiguous() {
        let tp = TestPrimitives::new();
        let run_id = tp.run_id;

        for expected_seq in 0..100 {
            let version = tp
                .event_log
                .append(&run_id, "event", values::int(expected_seq))
                .unwrap();
            if let Version::Sequence(seq) = version {
                assert_eq!(seq, expected_seq as u64, "Sequence gap at {}", expected_seq);
            }
        }

        // Verify by reading all
        let events = tp.event_log.read_range(&run_id, 0, 100).unwrap();
        invariants::assert_sequences_contiguous(&events);
    }

    #[test]
    fn test_no_sequence_gap_after_failed_append() {
        let tp = TestPrimitives::new();
        let run_id = tp.run_id;

        // Successful append: seq 0
        let version0 = tp
            .event_log
            .append(&run_id, "event", values::int(0))
            .unwrap();
        assert!(matches!(version0, Version::Sequence(0)));

        // Failed transaction that tries to append
        use strata_primitives::extensions::*;
        let result: Result<(), Error> = tp.db.transaction(run_id, |txn| {
            txn.event_append("failed", values::int(1))?;
            Err(Error::InvalidState("abort".to_string()))
        });
        assert!(result.is_err());

        // Next successful append should be seq 1, not seq 2
        let version1 = tp
            .event_log
            .append(&run_id, "event", values::int(1))
            .unwrap();
        assert!(matches!(version1, Version::Sequence(1)), "Sequence gap after failed append");

        // Verify contiguity
        let events = tp.event_log.read_range(&run_id, 0, 100).unwrap();
        assert_eq!(events.len(), 2);
        invariants::assert_sequences_contiguous(&events);
    }

    #[test]
    fn test_sequences_never_reused() {
        let tp = TestPrimitives::new();
        let run_id = tp.run_id;

        // Append some events
        for _ in 0..5 {
            tp.event_log
                .append(&run_id, "event", values::null())
                .unwrap();
        }

        // Even if we could theoretically "delete" (which we can't),
        // new appends should continue from 5
        let version = tp
            .event_log
            .append(&run_id, "event", values::null())
            .unwrap();
        assert!(matches!(version, Version::Sequence(5)));
    }

    #[test]
    fn test_sequences_independent_per_run() {
        let tp = TestPrimitives::new();
        let run1 = tp.run_id;
        let run2 = RunId::new();

        // Append to run1
        for i in 0..10 {
            let version = tp.event_log.append(&run1, "event", values::int(i)).unwrap();
            if let Version::Sequence(seq) = version {
                assert_eq!(seq, i as u64);
            }
        }

        // run2 should start at 0
        let version = tp.event_log.append(&run2, "event", values::int(0)).unwrap();
        assert!(matches!(version, Version::Sequence(0)), "run2 should start at sequence 0");
    }
}

// ============================================================================
// M3.9: Hash Chain Integrity
// ============================================================================
// Each event's prev_hash matches previous event's hash.
// Chain verification passes for valid chains.
// Chain verification detects corruption.
//
// What breaks if this fails?
// Undetected corruption. Events can be reordered without detection.

mod hash_chain_integrity {
    use super::*;

    #[test]
    fn test_first_event_has_zero_prev_hash() {
        let tp = TestPrimitives::new();
        let run_id = tp.run_id;

        tp.event_log
            .append(&run_id, "first", values::null())
            .unwrap();

        let event = tp.event_log.read(&run_id, 0).unwrap().unwrap();
        assert_eq!(
            event.value.prev_hash, [0u8; 32],
            "First event prev_hash should be zero"
        );
    }

    #[test]
    fn test_subsequent_events_chain_correctly() {
        let tp = TestPrimitives::new();
        let run_id = tp.run_id;

        // Append multiple events
        for i in 0..10 {
            tp.event_log
                .append(&run_id, &format!("event_{}", i), values::int(i))
                .unwrap();
        }

        // Verify chain integrity
        let events = tp.event_log.read_range(&run_id, 0, 100).unwrap();
        invariants::assert_chain_integrity(&events);
    }

    #[test]
    fn test_verify_chain_passes_for_valid_chain() {
        let tp = TestPrimitives::new();
        let run_id = tp.run_id;

        for i in 0..20 {
            tp.event_log
                .append(&run_id, "event", values::int(i))
                .unwrap();
        }

        let verification = tp.event_log.verify_chain(&run_id).unwrap();
        assert!(verification.is_valid, "Valid chain failed verification");
        assert_eq!(verification.length, 20);
    }

    #[test]
    fn test_chain_links_are_deterministic() {
        let tp = TestPrimitives::new();
        let run_id = tp.run_id;

        // Append events
        for i in 0..5 {
            tp.event_log
                .append(&run_id, "event", values::int(i))
                .unwrap();
        }

        // Read chain multiple times, hashes should be consistent
        let events1 = tp.event_log.read_range(&run_id, 0, 100).unwrap();
        let events2 = tp.event_log.read_range(&run_id, 0, 100).unwrap();

        for i in 0..5 {
            assert_eq!(
                events1[i].value.hash, events2[i].value.hash,
                "Hash inconsistent at {}",
                i
            );
            assert_eq!(
                events1[i].value.prev_hash, events2[i].value.prev_hash,
                "Prev hash inconsistent at {}",
                i
            );
        }
    }

    #[test]
    fn test_chain_integrity_survives_recovery() {
        let ptp = PersistentTestPrimitives::new();
        let run_id = ptp.run_id;

        // Write events
        {
            let p = ptp.open_strict();
            for i in 0..10 {
                p.event_log
                    .append(&run_id, "event", values::int(i))
                    .unwrap();
            }
        }

        // Recover and verify chain
        {
            let p = ptp.open();
            let verification = p.event_log.verify_chain(&run_id).unwrap();
            assert!(verification.is_valid, "Chain invalid after recovery");
            assert_eq!(verification.length, 10);
        }
    }

    #[test]
    fn test_hash_includes_payload() {
        let tp = TestPrimitives::new();
        let run1 = tp.run_id;
        let run2 = RunId::new();

        // Same event type, different payload -> different hash
        tp.event_log.append(&run1, "event", values::int(1)).unwrap();
        tp.event_log.append(&run2, "event", values::int(2)).unwrap();

        // Read back to get hashes
        let event1 = tp.event_log.read(&run1, 0).unwrap().unwrap();
        let event2 = tp.event_log.read(&run2, 0).unwrap().unwrap();

        // Hashes should differ because payloads differ
        assert_ne!(
            event1.value.hash, event2.value.hash,
            "Different payloads should produce different hashes"
        );
    }
}

// ============================================================================
// M3.10: Total Order Under Concurrency
// ============================================================================
// Concurrent appends serialize to a total order.
// Final sequence order matches serialization order.
//
// Important: Order reflects commit serialization order, not real-time.
//
// What breaks if this fails?
// Non-deterministic sequence assignment. Replay is non-deterministic.

mod total_order_under_concurrency {
    use super::*;

    #[test]
    fn test_concurrent_appends_produce_total_order() {
        let tp = TestPrimitives::new();
        let run_id = tp.run_id;
        let event_log = tp.event_log.clone();

        // Track how many threads succeed
        let success_count = Arc::new(AtomicU64::new(0));

        let results = concurrent::run_with_shared(
            10,
            (event_log, run_id, success_count.clone()),
            |i, (log, run_id, count)| {
                // Each thread tries to append
                match log.append(run_id, &format!("thread_{}", i), values::int(i as i64)) {
                    Ok(version) => {
                        if let Version::Sequence(seq) = version {
                            count.fetch_add(1, Ordering::Relaxed);
                            Some(seq)
                        } else {
                            None
                        }
                    }
                    Err(_) => None,
                }
            },
        );

        // Some threads should succeed
        let successful_seqs: Vec<u64> = results.into_iter().flatten().collect();
        assert!(!successful_seqs.is_empty(), "No threads succeeded");

        // Read all events
        let events = tp.event_log.read_range(&run_id, 0, 100).unwrap();

        // Sequences should be contiguous starting from 0
        invariants::assert_sequences_contiguous(&events);

        // Chain should be valid
        invariants::assert_chain_integrity(&events);
    }

    #[test]
    fn test_no_duplicate_sequences_under_contention() {
        let tp = TestPrimitives::new();
        let run_id = tp.run_id;
        let event_log = tp.event_log.clone();

        // Many concurrent appends
        let results = concurrent::run_with_shared(20, (event_log, run_id), |i, (log, run_id)| {
            log.append(run_id, "event", values::int(i as i64))
                .ok()
                .and_then(|v| if let Version::Sequence(seq) = v { Some(seq) } else { None })
        });

        let sequences: Vec<u64> = results.into_iter().flatten().collect();

        // Check for duplicates
        let mut sorted = sequences.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(
            sequences.len(),
            sorted.len(),
            "Duplicate sequences assigned: {:?}",
            sequences
        );
    }

    #[test]
    fn test_final_order_is_deterministic() {
        let tp = TestPrimitives::new();
        let run_id = tp.run_id;

        // Sequential appends (deterministic baseline)
        for i in 0..10 {
            tp.event_log
                .append(&run_id, "event", values::int(i))
                .unwrap();
        }

        // Read multiple times
        let order1: Vec<i64> = tp
            .event_log
            .read_range(&run_id, 0, 100)
            .unwrap()
            .iter()
            .map(|e| {
                if let strata_core::value::Value::I64(v) = e.value.payload {
                    v
                } else {
                    panic!("Wrong type")
                }
            })
            .collect();

        let order2: Vec<i64> = tp
            .event_log
            .read_range(&run_id, 0, 100)
            .unwrap()
            .iter()
            .map(|e| {
                if let strata_core::value::Value::I64(v) = e.value.payload {
                    v
                } else {
                    panic!("Wrong type")
                }
            })
            .collect();

        assert_eq!(order1, order2, "Order is not deterministic");
    }
}

// ============================================================================
// M3.11: Metadata Consistency
// ============================================================================
// len() matches actual event count.
// head() returns most recent event.
// Metadata survives recovery.
//
// What breaks if this fails?
// Off-by-one errors everywhere. len() disagrees with actual count.

mod metadata_consistency {
    use super::*;

    #[test]
    fn test_len_matches_event_count() {
        let tp = TestPrimitives::new();
        let run_id = tp.run_id;

        // Empty log
        assert_eq!(tp.event_log.len(&run_id).unwrap(), 0);

        // Add events one by one
        for expected in 1..=20 {
            tp.event_log
                .append(&run_id, "event", values::int(expected))
                .unwrap();
            let len = tp.event_log.len(&run_id).unwrap();
            assert_eq!(len, expected as u64, "len() mismatch at count {}", expected);
        }

        // Verify against read_range
        let events = tp.event_log.read_range(&run_id, 0, 100).unwrap();
        assert_eq!(events.len() as u64, tp.event_log.len(&run_id).unwrap());
    }

    #[test]
    fn test_head_returns_most_recent() {
        let tp = TestPrimitives::new();
        let run_id = tp.run_id;

        // Empty log has no head
        let head = tp.event_log.head(&run_id).unwrap();
        assert!(head.is_none(), "Empty log should have no head");

        // Add events, head should always be most recent
        for i in 0..10 {
            tp.event_log
                .append(&run_id, "event", values::int(i))
                .unwrap();

            let head = tp.event_log.head(&run_id).unwrap().unwrap();
            assert_eq!(head.value.sequence, i as u64, "Head sequence mismatch");
            assert_eq!(head.value.payload, values::int(i), "Head payload mismatch");
        }
    }

    #[test]
    fn test_head_sequence_equals_len_minus_one() {
        let tp = TestPrimitives::new();
        let run_id = tp.run_id;

        for i in 0..10 {
            tp.event_log
                .append(&run_id, "event", values::int(i))
                .unwrap();

            let len = tp.event_log.len(&run_id).unwrap();
            let head = tp.event_log.head(&run_id).unwrap().unwrap();

            // head.sequence should be len - 1 (zero-indexed)
            assert_eq!(
                head.value.sequence,
                len - 1,
                "head.sequence ({}) != len-1 ({})",
                head.value.sequence,
                len - 1
            );
        }
    }

    #[test]
    fn test_metadata_survives_recovery() {
        let ptp = PersistentTestPrimitives::new();
        let run_id = ptp.run_id;

        let expected_len = 15u64;

        // Write events
        {
            let p = ptp.open_strict();
            for i in 0..expected_len {
                p.event_log
                    .append(&run_id, "event", values::int(i as i64))
                    .unwrap();
            }

            // Capture metadata before close
            let len = p.event_log.len(&run_id).unwrap();
            let head = p.event_log.head(&run_id).unwrap().unwrap();
            assert_eq!(len, expected_len);
            assert_eq!(head.value.sequence, expected_len - 1);
        }

        // Recover and verify metadata
        {
            let p = ptp.open();
            let len = p.event_log.len(&run_id).unwrap();
            let head = p.event_log.head(&run_id).unwrap().unwrap();

            assert_eq!(len, expected_len, "len() changed after recovery");
            assert_eq!(
                head.value.sequence,
                expected_len - 1,
                "head sequence changed after recovery"
            );
        }
    }

    #[test]
    fn test_metadata_consistent_across_runs() {
        let tp = TestPrimitives::new();
        let run1 = tp.run_id;
        let run2 = RunId::new();

        // Different event counts per run
        for _ in 0..5 {
            tp.event_log.append(&run1, "event", values::null()).unwrap();
        }
        for _ in 0..10 {
            tp.event_log.append(&run2, "event", values::null()).unwrap();
        }

        // Each run has correct metadata
        assert_eq!(tp.event_log.len(&run1).unwrap(), 5);
        assert_eq!(tp.event_log.len(&run2).unwrap(), 10);

        assert_eq!(tp.event_log.head(&run1).unwrap().unwrap().value.sequence, 4);
        assert_eq!(tp.event_log.head(&run2).unwrap().unwrap().value.sequence, 9);
    }

    #[test]
    fn test_read_range_respects_boundaries() {
        let tp = TestPrimitives::new();
        let run_id = tp.run_id;

        // Add 20 events
        for i in 0..20 {
            tp.event_log
                .append(&run_id, "event", values::int(i))
                .unwrap();
        }

        // read_range(start, end) reads [start, end)
        // So read_range(5, 10) should read sequences 5, 6, 7, 8, 9
        let events = tp.event_log.read_range(&run_id, 5, 10).unwrap();
        assert_eq!(events.len(), 5);
        assert_eq!(events[0].value.sequence, 5);
        assert_eq!(events[4].value.sequence, 9);

        // Read beyond end - only existing events returned
        let events = tp.event_log.read_range(&run_id, 15, 30).unwrap();
        assert_eq!(events.len(), 5); // Only 15-19 exist
    }

    #[test]
    fn test_is_empty() {
        let tp = TestPrimitives::new();
        let run_id = tp.run_id;

        assert!(tp.event_log.is_empty(&run_id).unwrap());

        tp.event_log
            .append(&run_id, "event", values::null())
            .unwrap();

        assert!(!tp.event_log.is_empty(&run_id).unwrap());
    }

    #[test]
    fn test_read_by_type() {
        let tp = TestPrimitives::new();
        let run_id = tp.run_id;

        // Append events of different types
        tp.event_log
            .append(&run_id, "type_a", values::int(1))
            .unwrap();
        tp.event_log
            .append(&run_id, "type_b", values::int(2))
            .unwrap();
        tp.event_log
            .append(&run_id, "type_a", values::int(3))
            .unwrap();
        tp.event_log
            .append(&run_id, "type_c", values::int(4))
            .unwrap();
        tp.event_log
            .append(&run_id, "type_a", values::int(5))
            .unwrap();

        // Query by type
        let type_a_events = tp.event_log.read_by_type(&run_id, "type_a").unwrap();
        assert_eq!(type_a_events.len(), 3);
        assert_eq!(type_a_events[0].value.payload, values::int(1));
        assert_eq!(type_a_events[1].value.payload, values::int(3));
        assert_eq!(type_a_events[2].value.payload, values::int(5));

        let type_b_events = tp.event_log.read_by_type(&run_id, "type_b").unwrap();
        assert_eq!(type_b_events.len(), 1);

        let type_c_events = tp.event_log.read_by_type(&run_id, "type_c").unwrap();
        assert_eq!(type_c_events.len(), 1);

        let nonexistent = tp.event_log.read_by_type(&run_id, "nonexistent").unwrap();
        assert!(nonexistent.is_empty());
    }

    #[test]
    fn test_event_types() {
        let tp = TestPrimitives::new();
        let run_id = tp.run_id;

        tp.event_log
            .append(&run_id, "alpha", values::null())
            .unwrap();
        tp.event_log
            .append(&run_id, "beta", values::null())
            .unwrap();
        tp.event_log
            .append(&run_id, "alpha", values::null())
            .unwrap();
        tp.event_log
            .append(&run_id, "gamma", values::null())
            .unwrap();

        let types = tp.event_log.event_types(&run_id).unwrap();
        assert_eq!(types.len(), 3);
        assert!(types.contains(&"alpha".to_string()));
        assert!(types.contains(&"beta".to_string()));
        assert!(types.contains(&"gamma".to_string()));
    }
}
