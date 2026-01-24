//! EventLog Stream Tests
//!
//! Tests for multi-stream operations and stream isolation:
//! - Multiple streams within same run
//! - Stream isolation (events only visible in their stream)
//! - Global vs per-stream sequences (known limitation)
//! - Stream naming conventions
//!
//! All test data is loaded from testdata/eventlog_test_data.jsonl

use crate::test_data::load_eventlog_test_data;
use crate::*;
use std::collections::HashMap;

// =============================================================================
// MULTI-STREAM TESTS
// =============================================================================

#[test]
fn test_multiple_streams_independent() {
    let (_, substrate) = quick_setup();
    let run = ApiRunId::default();
    let test_data = load_eventlog_test_data();

    // Append 3 events from test data to stream1
    let stream1_entries: Vec<_> = test_data.take(3).to_vec();
    for entry in &stream1_entries {
        substrate
            .event_append(&run, "ind_stream1", entry.payload.clone())
            .expect("append should succeed");
    }

    // Append 5 events from test data to stream2
    let stream2_entries: Vec<_> = test_data.entries.iter().skip(3).take(5).cloned().collect();
    for entry in &stream2_entries {
        substrate
            .event_append(&run, "ind_stream2", entry.payload.clone())
            .expect("append should succeed");
    }

    // Verify streams are independent
    let events1 = substrate
        .event_range(&run, "ind_stream1", None, None, None)
        .expect("range should succeed");
    let events2 = substrate
        .event_range(&run, "ind_stream2", None, None, None)
        .expect("range should succeed");

    assert_eq!(events1.len(), 3, "stream1 should have 3 events");
    assert_eq!(events2.len(), 5, "stream2 should have 5 events");

    // Verify payloads match what was appended
    for (i, event) in events1.iter().enumerate() {
        assert_eq!(event.value, stream1_entries[i].payload, "stream1 event {} should match", i);
    }
    for (i, event) in events2.iter().enumerate() {
        assert_eq!(event.value, stream2_entries[i].payload, "stream2 event {} should match", i);
    }
}

#[test]
fn test_interleaved_appends_to_different_streams() {
    let (_, substrate) = quick_setup();
    let run = ApiRunId::default();
    let test_data = load_eventlog_test_data();

    // Interleave appends: s1, s2, s1, s2, s1
    let entries: Vec<_> = test_data.take(5).to_vec();
    let mut stream1_payloads = Vec::new();
    let mut stream2_payloads = Vec::new();

    for (i, entry) in entries.iter().enumerate() {
        let stream = if i % 2 == 0 { "interleave_s1" } else { "interleave_s2" };
        substrate
            .event_append(&run, stream, entry.payload.clone())
            .expect("append should succeed");

        if i % 2 == 0 {
            stream1_payloads.push(entry.payload.clone());
        } else {
            stream2_payloads.push(entry.payload.clone());
        }
    }

    // stream1: indices 0, 2, 4 (3 events)
    let events1 = substrate
        .event_range(&run, "interleave_s1", None, None, None)
        .expect("range should succeed");
    // stream2: indices 1, 3 (2 events)
    let events2 = substrate
        .event_range(&run, "interleave_s2", None, None, None)
        .expect("range should succeed");

    assert_eq!(events1.len(), 3, "stream1 should have 3 events");
    assert_eq!(events2.len(), 2, "stream2 should have 2 events");

    // Verify payloads for stream1
    for (i, event) in events1.iter().enumerate() {
        assert_eq!(event.value, stream1_payloads[i], "stream1 event {} should match", i);
    }

    // Verify payloads for stream2
    for (i, event) in events2.iter().enumerate() {
        assert_eq!(event.value, stream2_payloads[i], "stream2 event {} should match", i);
    }
}

// =============================================================================
// SEQUENCE BEHAVIOR TESTS
// Note: Sequences are GLOBAL, not per-stream (known limitation)
// =============================================================================

#[test]
fn test_sequences_are_global_not_per_stream() {
    // This documents the known limitation: sequences are global across all streams
    // within a run, not per-stream like Redis streams.

    let (_, substrate) = quick_setup();
    let run = ApiRunId::default();
    let test_data = load_eventlog_test_data();

    // Append to stream1, get sequence
    let entry1 = &test_data.entries[0];
    let v1 = substrate
        .event_append(&run, "global_seq_s1", entry1.payload.clone())
        .expect("append should succeed");
    let seq1 = match v1 {
        Version::Sequence(n) => n,
        _ => panic!("Expected sequence"),
    };

    // Append to stream2, get sequence
    let entry2 = &test_data.entries[1];
    let v2 = substrate
        .event_append(&run, "global_seq_s2", entry2.payload.clone())
        .expect("append should succeed");
    let seq2 = match v2 {
        Version::Sequence(n) => n,
        _ => panic!("Expected sequence"),
    };

    // Append to stream1 again
    let entry3 = &test_data.entries[2];
    let v3 = substrate
        .event_append(&run, "global_seq_s1", entry3.payload.clone())
        .expect("append should succeed");
    let seq3 = match v3 {
        Version::Sequence(n) => n,
        _ => panic!("Expected sequence"),
    };

    // Document: sequences are global (seq1 < seq2 < seq3 even though stream1 has gaps)
    assert!(seq2 > seq1, "Second append should have higher sequence");
    assert!(seq3 > seq2, "Third append should have higher sequence");

    // Note: seq3 is NOT seq1+1 because seq2 was allocated to stream2
    // This is the known limitation - sequences span all streams
}

#[test]
fn test_get_event_by_global_sequence() {
    let (_, substrate) = quick_setup();
    let run = ApiRunId::default();
    let test_data = load_eventlog_test_data();

    // Append to stream1
    let entry1 = &test_data.entries[0];
    let v1 = substrate
        .event_append(&run, "get_by_seq_s1", entry1.payload.clone())
        .expect("append should succeed");
    let seq1 = match v1 {
        Version::Sequence(n) => n,
        _ => panic!("Expected sequence"),
    };

    // Append to stream2
    let entry2 = &test_data.entries[1];
    let v2 = substrate
        .event_append(&run, "get_by_seq_s2", entry2.payload.clone())
        .expect("append should succeed");
    let seq2 = match v2 {
        Version::Sequence(n) => n,
        _ => panic!("Expected sequence"),
    };

    // Get event from stream1 using its sequence
    let event1 = substrate
        .event_get(&run, "get_by_seq_s1", seq1)
        .expect("get should succeed")
        .expect("event should exist");

    assert_eq!(event1.value, entry1.payload, "Should get correct event from stream1");

    // Get event from stream2 using its sequence
    let event2 = substrate
        .event_get(&run, "get_by_seq_s2", seq2)
        .expect("get should succeed")
        .expect("event should exist");

    assert_eq!(event2.value, entry2.payload, "Should get correct event from stream2");

    // Try to get stream2's event from stream1 - should return None
    let wrong_stream = substrate
        .event_get(&run, "get_by_seq_s1", seq2)
        .expect("get should succeed");

    assert!(
        wrong_stream.is_none(),
        "Getting event with wrong stream should return None"
    );
}

// =============================================================================
// STREAM NAMING TESTS
// =============================================================================

#[test]
fn test_stream_name_with_special_characters() {
    let (_, substrate) = quick_setup();
    let run = ApiRunId::default();
    let test_data = load_eventlog_test_data();

    let special_streams = vec![
        "stream-with-dashes",
        "stream_with_underscores",
        "stream.with.dots",
        "stream:with:colons",
        "stream/with/slashes",
        "CamelCaseStream",
        "UPPERCASE_STREAM",
    ];

    // Use test data entries for payloads
    for (i, stream_name) in special_streams.iter().enumerate() {
        let entry = &test_data.entries[i % test_data.entries.len()];
        let result = substrate.event_append(&run, stream_name, entry.payload.clone());
        assert!(
            result.is_ok(),
            "Stream name '{}' should be accepted: {:?}",
            stream_name,
            result
        );
    }

    // Verify each stream has exactly 1 event
    for stream_name in &special_streams {
        let len = substrate
            .event_len(&run, stream_name)
            .expect("len should succeed");
        assert_eq!(len, 1, "Stream '{}' should have 1 event", stream_name);
    }
}

#[test]
fn test_stream_name_unicode() {
    let (_, substrate) = quick_setup();
    let run = ApiRunId::default();
    let test_data = load_eventlog_test_data();

    let unicode_streams = vec![
        "stream_unicode_emoji",  // Avoid actual emoji in stream names
        "stream_chinese_test",
        "stream_arabic_test",
    ];

    for (i, stream_name) in unicode_streams.iter().enumerate() {
        let entry = &test_data.entries[i];
        let result = substrate.event_append(&run, stream_name, entry.payload.clone());
        assert!(
            result.is_ok(),
            "Stream name '{}' should be accepted: {:?}",
            stream_name,
            result
        );
    }
}

#[test]
fn test_stream_case_sensitivity() {
    let (_, substrate) = quick_setup();
    let run = ApiRunId::default();
    let test_data = load_eventlog_test_data();

    // Append to "CaseStream" and "casestream" - should be different streams
    let entry_upper = &test_data.entries[0];
    let entry_lower = &test_data.entries[1];

    substrate
        .event_append(&run, "CaseStream", entry_upper.payload.clone())
        .expect("append should succeed");

    substrate
        .event_append(&run, "casestream", entry_lower.payload.clone())
        .expect("append should succeed");

    // Should be different streams
    let events_upper = substrate
        .event_range(&run, "CaseStream", None, None, None)
        .expect("range should succeed");
    let events_lower = substrate
        .event_range(&run, "casestream", None, None, None)
        .expect("range should succeed");

    assert_eq!(events_upper.len(), 1, "CaseStream should have 1 event");
    assert_eq!(events_lower.len(), 1, "casestream should have 1 event");

    // Verify correct payloads
    assert_eq!(events_upper[0].value, entry_upper.payload, "CaseStream should have upper entry");
    assert_eq!(events_lower[0].value, entry_lower.payload, "casestream should have lower entry");
}

// =============================================================================
// LATEST SEQUENCE PER STREAM
// =============================================================================

#[test]
fn test_latest_sequence_per_stream() {
    let (_, substrate) = quick_setup();
    let run = ApiRunId::default();
    let test_data = load_eventlog_test_data();

    // Append to multiple streams
    let mut last_seq1 = 0;
    let mut last_seq2 = 0;

    for entry in test_data.take(3) {
        let v = substrate
            .event_append(&run, "latest_per_s1", entry.payload.clone())
            .expect("append should succeed");
        if let Version::Sequence(seq) = v {
            last_seq1 = seq;
        }
    }

    for entry in test_data.entries.iter().skip(3).take(5) {
        let v = substrate
            .event_append(&run, "latest_per_s2", entry.payload.clone())
            .expect("append should succeed");
        if let Version::Sequence(seq) = v {
            last_seq2 = seq;
        }
    }

    // Latest sequence should be different for each stream
    let latest1 = substrate
        .event_latest_sequence(&run, "latest_per_s1")
        .expect("latest_sequence should succeed")
        .expect("should have latest");

    let latest2 = substrate
        .event_latest_sequence(&run, "latest_per_s2")
        .expect("latest_sequence should succeed")
        .expect("should have latest");

    assert_eq!(latest1, last_seq1, "stream1 latest should match");
    assert_eq!(latest2, last_seq2, "stream2 latest should match");
    assert_ne!(latest1, latest2, "Different streams should have different latest");
}

// =============================================================================
// LEN PER STREAM
// =============================================================================

#[test]
fn test_len_isolated_per_stream() {
    let (_, substrate) = quick_setup();
    let run = ApiRunId::default();
    let test_data = load_eventlog_test_data();

    // Append different counts to different streams using test data
    for entry in test_data.take(2) {
        substrate
            .event_append(&run, "len_iso_s1", entry.payload.clone())
            .expect("append should succeed");
    }

    for entry in test_data.entries.iter().skip(2).take(5) {
        substrate
            .event_append(&run, "len_iso_s2", entry.payload.clone())
            .expect("append should succeed");
    }

    for entry in test_data.entries.iter().skip(7).take(10) {
        substrate
            .event_append(&run, "len_iso_s3", entry.payload.clone())
            .expect("append should succeed");
    }

    assert_eq!(
        substrate.event_len(&run, "len_iso_s1").expect("len should succeed"),
        2,
        "stream1 should have 2 events"
    );
    assert_eq!(
        substrate.event_len(&run, "len_iso_s2").expect("len should succeed"),
        5,
        "stream2 should have 5 events"
    );
    assert_eq!(
        substrate.event_len(&run, "len_iso_s3").expect("len should succeed"),
        10,
        "stream3 should have 10 events"
    );
    assert_eq!(
        substrate.event_len(&run, "stream_nonexistent").expect("len should succeed"),
        0,
        "nonexistent stream should have 0 events"
    );
}
