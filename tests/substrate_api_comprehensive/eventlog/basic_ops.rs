//! EventLog Basic Operations Tests
//!
//! Tests for fundamental EventLog operations:
//! - event_append: Append event to stream
//! - event_get: Get specific event by sequence
//! - event_range: Read events in range
//! - event_len: Get event count in stream
//! - event_latest_sequence: Get latest sequence number
//!
//! All test data is loaded from testdata/eventlog_test_data.jsonl

use crate::test_data::load_eventlog_test_data;
use crate::*;
use std::collections::HashMap;

// =============================================================================
// APPEND TESTS
// =============================================================================

#[test]
fn test_append_returns_sequence_version() {
    let (_, substrate) = quick_setup();
    let run = ApiRunId::default();
    let test_data = load_eventlog_test_data();

    // Use first entry from test data
    let entry = &test_data.entries[0];

    let version = substrate
        .event_append(&run, &entry.stream, entry.payload.clone())
        .expect("append should succeed");

    // Version should be a sequence version (0-indexed)
    match version {
        Version::Sequence(_) => {} // Sequences start at 0
        _ => panic!("Expected Version::Sequence, got {:?}", version),
    }
}

#[test]
fn test_append_sequences_are_monotonic() {
    let (_, substrate) = quick_setup();
    let run = ApiRunId::default();
    let test_data = load_eventlog_test_data();

    let mut sequences = Vec::new();

    // Use first 10 entries from test data
    for entry in test_data.take(10) {
        let version = substrate
            .event_append(&run, &entry.stream, entry.payload.clone())
            .expect("append should succeed");

        if let Version::Sequence(seq) = version {
            sequences.push(seq);
        }
    }

    // Verify sequences are strictly increasing
    for window in sequences.windows(2) {
        assert!(
            window[1] > window[0],
            "Sequences should be strictly increasing: {} -> {}",
            window[0],
            window[1]
        );
    }
}

#[test]
fn test_append_creates_stream_on_first_event() {
    let (_, substrate) = quick_setup();
    let run = ApiRunId::default();
    let test_data = load_eventlog_test_data();

    // Stream doesn't exist yet
    let len_before = substrate
        .event_len(&run, "new_stream")
        .expect("len should succeed");
    assert_eq!(len_before, 0, "Stream should be empty before first append");

    // Append creates stream using first test entry's payload
    let entry = &test_data.entries[0];
    substrate
        .event_append(&run, "new_stream", entry.payload.clone())
        .expect("append should succeed");

    // Stream now has event
    let len_after = substrate
        .event_len(&run, "new_stream")
        .expect("len should succeed");
    assert_eq!(len_after, 1, "Stream should have one event after append");
}

#[test]
fn test_append_empty_object_payload() {
    let (_, substrate) = quick_setup();
    let run = ApiRunId::default();

    // Empty object should be valid payload
    let payload = Value::Object(HashMap::new());

    let result = substrate.event_append(&run, "stream1", payload);
    assert!(
        result.is_ok(),
        "Empty object payload should be accepted: {:?}",
        result
    );
}

// =============================================================================
// GET TESTS
// =============================================================================

#[test]
fn test_get_existing_event() {
    let (_, substrate) = quick_setup();
    let run = ApiRunId::default();
    let test_data = load_eventlog_test_data();

    // Use first entry from test data
    let entry = &test_data.entries[0];

    let version = substrate
        .event_append(&run, &entry.stream, entry.payload.clone())
        .expect("append should succeed");

    let sequence = match version {
        Version::Sequence(n) => n,
        _ => panic!("Expected sequence version"),
    };

    let event = substrate
        .event_get(&run, &entry.stream, sequence)
        .expect("get should succeed")
        .expect("event should exist");

    // Verify payload
    assert_eq!(event.value, entry.payload);
}

#[test]
fn test_get_missing_event_returns_none() {
    let (_, substrate) = quick_setup();
    let run = ApiRunId::default();

    // No events appended
    let result = substrate
        .event_get(&run, "stream1", 999)
        .expect("get should succeed");

    assert!(
        result.is_none(),
        "Getting nonexistent event should return None"
    );
}

#[test]
fn test_get_returns_versioned_with_sequence() {
    let (_, substrate) = quick_setup();
    let run = ApiRunId::default();
    let test_data = load_eventlog_test_data();

    // Use first entry from test data
    let entry = &test_data.entries[0];

    let version = substrate
        .event_append(&run, &entry.stream, entry.payload.clone())
        .expect("append should succeed");

    let sequence = match version {
        Version::Sequence(n) => n,
        _ => panic!("Expected sequence version"),
    };

    let event = substrate
        .event_get(&run, &entry.stream, sequence)
        .expect("get should succeed")
        .expect("event should exist");

    // Verify version in returned event matches
    match event.version {
        Version::Sequence(n) => assert_eq!(n, sequence),
        _ => panic!("Expected Version::Sequence in event, got {:?}", event.version),
    }
}

// =============================================================================
// RANGE TESTS
// =============================================================================

#[test]
fn test_range_all_events() {
    let (_, substrate) = quick_setup();
    let run = ApiRunId::default();
    let test_data = load_eventlog_test_data();

    // Append 5 events from test data to same stream
    let entries: Vec<_> = test_data.take(5).to_vec();
    for entry in &entries {
        substrate
            .event_append(&run, "range_test_stream", entry.payload.clone())
            .expect("append should succeed");
    }

    // Read all events (no bounds)
    let events = substrate
        .event_range(&run, "range_test_stream", None, None, None)
        .expect("range should succeed");

    assert_eq!(events.len(), 5, "Should have 5 events");

    // Verify order is ascending (oldest first)
    for (i, event) in events.iter().enumerate() {
        assert_eq!(event.value, entries[i].payload);
    }
}

#[test]
fn test_range_with_start_bound() {
    let (_, substrate) = quick_setup();
    let run = ApiRunId::default();
    let test_data = load_eventlog_test_data();

    let mut sequences = Vec::new();
    for entry in test_data.take(5) {
        let version = substrate
            .event_append(&run, "start_bound_stream", entry.payload.clone())
            .expect("append should succeed");
        if let Version::Sequence(seq) = version {
            sequences.push(seq);
        }
    }

    // Read from third event onwards
    let events = substrate
        .event_range(&run, "start_bound_stream", Some(sequences[2]), None, None)
        .expect("range should succeed");

    assert_eq!(events.len(), 3, "Should have 3 events from start bound");
}

#[test]
fn test_range_with_end_bound() {
    let (_, substrate) = quick_setup();
    let run = ApiRunId::default();
    let test_data = load_eventlog_test_data();

    let mut sequences = Vec::new();
    for entry in test_data.take(5) {
        let version = substrate
            .event_append(&run, "end_bound_stream", entry.payload.clone())
            .expect("append should succeed");
        if let Version::Sequence(seq) = version {
            sequences.push(seq);
        }
    }

    // Read up to third event (inclusive)
    let events = substrate
        .event_range(&run, "end_bound_stream", None, Some(sequences[2]), None)
        .expect("range should succeed");

    assert_eq!(events.len(), 3, "Should have 3 events up to end bound");
}

#[test]
fn test_range_with_both_bounds() {
    let (_, substrate) = quick_setup();
    let run = ApiRunId::default();
    let test_data = load_eventlog_test_data();

    let mut sequences = Vec::new();
    for entry in test_data.take(10) {
        let version = substrate
            .event_append(&run, "both_bounds_stream", entry.payload.clone())
            .expect("append should succeed");
        if let Version::Sequence(seq) = version {
            sequences.push(seq);
        }
    }

    // Read middle range
    let events = substrate
        .event_range(&run, "both_bounds_stream", Some(sequences[3]), Some(sequences[6]), None)
        .expect("range should succeed");

    assert_eq!(events.len(), 4, "Should have 4 events in middle range");
}

#[test]
fn test_range_with_limit() {
    let (_, substrate) = quick_setup();
    let run = ApiRunId::default();
    let test_data = load_eventlog_test_data();

    for entry in test_data.take(10) {
        substrate
            .event_append(&run, "limit_stream", entry.payload.clone())
            .expect("append should succeed");
    }

    // Read with limit
    let events = substrate
        .event_range(&run, "limit_stream", None, None, Some(3))
        .expect("range should succeed");

    assert_eq!(events.len(), 3, "Should respect limit of 3");
}

#[test]
fn test_range_empty_stream() {
    let (_, substrate) = quick_setup();
    let run = ApiRunId::default();

    // Read from nonexistent stream
    let events = substrate
        .event_range(&run, "empty_stream", None, None, None)
        .expect("range should succeed");

    assert!(events.is_empty(), "Empty stream should return empty vec");
}

#[test]
fn test_range_ascending_order() {
    let (_, substrate) = quick_setup();
    let run = ApiRunId::default();
    let test_data = load_eventlog_test_data();

    for entry in test_data.take(5) {
        substrate
            .event_append(&run, "order_stream", entry.payload.clone())
            .expect("append should succeed");
    }

    let events = substrate
        .event_range(&run, "order_stream", None, None, None)
        .expect("range should succeed");

    // Verify ascending order by sequence
    for window in events.windows(2) {
        let seq0 = match window[0].version {
            Version::Sequence(n) => n,
            _ => panic!("Expected sequence"),
        };
        let seq1 = match window[1].version {
            Version::Sequence(n) => n,
            _ => panic!("Expected sequence"),
        };
        assert!(
            seq1 > seq0,
            "Events should be in ascending order: {} < {}",
            seq0,
            seq1
        );
    }
}

// =============================================================================
// LEN TESTS
// =============================================================================

#[test]
fn test_len_empty_stream() {
    let (_, substrate) = quick_setup();
    let run = ApiRunId::default();

    let len = substrate
        .event_len(&run, "nonexistent")
        .expect("len should succeed");

    assert_eq!(len, 0, "Empty/nonexistent stream should have len 0");
}

#[test]
fn test_len_after_appends() {
    let (_, substrate) = quick_setup();
    let run = ApiRunId::default();
    let test_data = load_eventlog_test_data();

    for entry in test_data.take(7) {
        substrate
            .event_append(&run, "len_stream", entry.payload.clone())
            .expect("append should succeed");
    }

    let len = substrate
        .event_len(&run, "len_stream")
        .expect("len should succeed");

    assert_eq!(len, 7, "Stream should have 7 events");
}

#[test]
fn test_len_multiple_streams() {
    let (_, substrate) = quick_setup();
    let run = ApiRunId::default();
    let test_data = load_eventlog_test_data();

    // Add 3 events to stream1
    for entry in test_data.take(3) {
        substrate
            .event_append(&run, "multi_stream1", entry.payload.clone())
            .expect("append should succeed");
    }

    // Add 5 events to stream2 (skip first 3)
    for entry in test_data.entries.iter().skip(3).take(5) {
        substrate
            .event_append(&run, "multi_stream2", entry.payload.clone())
            .expect("append should succeed");
    }

    let len1 = substrate
        .event_len(&run, "multi_stream1")
        .expect("len should succeed");
    let len2 = substrate
        .event_len(&run, "multi_stream2")
        .expect("len should succeed");

    assert_eq!(len1, 3, "stream1 should have 3 events");
    assert_eq!(len2, 5, "stream2 should have 5 events");
}

// =============================================================================
// LATEST_SEQUENCE TESTS
// =============================================================================

#[test]
fn test_latest_sequence_empty_stream() {
    let (_, substrate) = quick_setup();
    let run = ApiRunId::default();

    let latest = substrate
        .event_latest_sequence(&run, "nonexistent")
        .expect("latest_sequence should succeed");

    assert!(
        latest.is_none(),
        "Empty/nonexistent stream should have no latest sequence"
    );
}

#[test]
fn test_latest_sequence_after_appends() {
    let (_, substrate) = quick_setup();
    let run = ApiRunId::default();
    let test_data = load_eventlog_test_data();

    let mut last_seq = 0;
    for entry in test_data.take(5) {
        let version = substrate
            .event_append(&run, "latest_seq_stream", entry.payload.clone())
            .expect("append should succeed");
        if let Version::Sequence(seq) = version {
            last_seq = seq;
        }
    }

    let latest = substrate
        .event_latest_sequence(&run, "latest_seq_stream")
        .expect("latest_sequence should succeed")
        .expect("should have latest");

    assert_eq!(latest, last_seq, "Latest sequence should match last append");
}

#[test]
fn test_latest_sequence_updates_on_append() {
    let (_, substrate) = quick_setup();
    let run = ApiRunId::default();
    let test_data = load_eventlog_test_data();

    // First append
    let entry1 = &test_data.entries[0];
    substrate
        .event_append(&run, "update_seq_stream", entry1.payload.clone())
        .expect("append should succeed");

    let latest1 = substrate
        .event_latest_sequence(&run, "update_seq_stream")
        .expect("latest_sequence should succeed")
        .expect("should have latest");

    // Second append
    let entry2 = &test_data.entries[1];
    let version2 = substrate
        .event_append(&run, "update_seq_stream", entry2.payload.clone())
        .expect("append should succeed");

    let latest2 = substrate
        .event_latest_sequence(&run, "update_seq_stream")
        .expect("latest_sequence should succeed")
        .expect("should have latest");

    assert!(
        latest2 > latest1,
        "Latest sequence should increase after append"
    );

    if let Version::Sequence(seq) = version2 {
        assert_eq!(latest2, seq, "Latest should match second append sequence");
    }
}

// =============================================================================
// RUN ISOLATION TESTS
// =============================================================================

#[test]
fn test_run_isolation_separate_streams() {
    let (_, substrate) = quick_setup();
    let run1 = ApiRunId::new();
    let run2 = ApiRunId::new();
    let test_data = load_eventlog_test_data();

    // Use entries from different runs in test data
    let entry1 = &test_data.get_run(0)[0];
    let entry2 = &test_data.get_run(1)[0];

    // Append to run1
    substrate
        .event_append(&run1, "iso_stream", entry1.payload.clone())
        .expect("append should succeed");

    // Append to run2
    substrate
        .event_append(&run2, "iso_stream", entry2.payload.clone())
        .expect("append should succeed");

    // Each run should see only its own event
    let events1 = substrate
        .event_range(&run1, "iso_stream", None, None, None)
        .expect("range should succeed");
    let events2 = substrate
        .event_range(&run2, "iso_stream", None, None, None)
        .expect("range should succeed");

    assert_eq!(events1.len(), 1, "run1 should see 1 event");
    assert_eq!(events2.len(), 1, "run2 should see 1 event");

    // Verify correct payloads (each run sees its own data)
    assert_eq!(events1[0].value, entry1.payload, "run1 should see entry1 payload");
    assert_eq!(events2[0].value, entry2.payload, "run2 should see entry2 payload");
}

#[test]
fn test_run_isolation_len() {
    let (_, substrate) = quick_setup();
    let run1 = ApiRunId::new();
    let run2 = ApiRunId::new();
    let test_data = load_eventlog_test_data();

    // Append 3 events to run1
    for entry in test_data.take(3) {
        substrate
            .event_append(&run1, "iso_len_stream", entry.payload.clone())
            .expect("append should succeed");
    }

    // Append 7 events to run2 (using different entries)
    for entry in test_data.entries.iter().skip(3).take(7) {
        substrate
            .event_append(&run2, "iso_len_stream", entry.payload.clone())
            .expect("append should succeed");
    }

    let len1 = substrate
        .event_len(&run1, "iso_len_stream")
        .expect("len should succeed");
    let len2 = substrate
        .event_len(&run2, "iso_len_stream")
        .expect("len should succeed");

    assert_eq!(len1, 3, "run1 should have 3 events");
    assert_eq!(len2, 7, "run2 should have 7 events");
}

// =============================================================================
// CROSS-MODE EQUIVALENCE
// =============================================================================

#[test]
fn test_cross_mode_equivalence() {
    let test_data = load_eventlog_test_data();
    let entry = test_data.entries[0].clone();

    test_across_modes("eventlog_basic_ops", move |db| {
        let substrate = create_substrate(db);
        let run = ApiRunId::default();

        substrate
            .event_append(&run, "test_stream", entry.payload.clone())
            .expect("append should succeed");

        let events = substrate
            .event_range(&run, "test_stream", None, None, None)
            .expect("range should succeed");

        events.len()
    });
}
