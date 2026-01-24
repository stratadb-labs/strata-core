//! EventLog Invariant Tests
//!
//! Tests that verify EventLog conforms to all 7 invariants from PRIMITIVE_CONTRACT.md:
//!
//! - I1: Everything is Addressable (run + stream + sequence)
//! - I2: Everything is Versioned (returns Version::Sequence)
//! - I3: Everything is Transactional (participates in transactions)
//! - I4: Everything Has a Lifecycle (CR - Create/Read only, immutable)
//! - I5: Everything Exists Within a Run (explicit run_id required)
//! - I6: Everything is Introspectable (exists, get, version checks)
//! - I7: Reads and Writes Have Consistent Semantics
//!
//! Also tests CORE_API_SHAPE.md requirements:
//! - EntityRef::Event structure
//! - Versioned<T> wrapper for reads
//! - Version::Sequence for event versions

use crate::test_data::load_eventlog_test_data;
use crate::*;

// =============================================================================
// INVARIANT 1: EVERYTHING IS ADDRESSABLE
// "Every entity has a stable identity (run + stream + sequence)"
// =============================================================================

#[test]
fn test_i1_event_has_stable_identity() {
    // An event is identified by: run_id + stream + sequence
    let (_, substrate) = quick_setup();
    let run = ApiRunId::default();
    let test_data = load_eventlog_test_data();

    let entry = &test_data.entries[0];

    // Append event and get its sequence (part of identity)
    let version = substrate
        .event_append(&run, "i1_stream", entry.payload.clone())
        .expect("append should succeed");

    let sequence = match version {
        Version::Sequence(seq) => seq,
        _ => panic!("Expected Version::Sequence"),
    };

    // The identity is (run, stream, sequence) - we can retrieve it later
    let event = substrate
        .event_get(&run, "i1_stream", sequence)
        .expect("get should succeed")
        .expect("event should exist");

    // Same identity returns same event
    assert_eq!(event.value, entry.payload, "Identity should be stable");
}

#[test]
fn test_i1_identity_components_all_required() {
    // All three components (run, stream, sequence) are required for identity
    let (_, substrate) = quick_setup();
    let run1 = ApiRunId::new();
    let run2 = ApiRunId::new();
    let test_data = load_eventlog_test_data();

    let entry = &test_data.entries[0];

    // Append to run1/stream1
    let version = substrate
        .event_append(&run1, "stream1", entry.payload.clone())
        .expect("append should succeed");

    let sequence = match version {
        Version::Sequence(seq) => seq,
        _ => panic!("Expected sequence"),
    };

    // Different run = different identity (not found)
    let wrong_run = substrate
        .event_get(&run2, "stream1", sequence)
        .expect("get should succeed");
    assert!(wrong_run.is_none(), "Different run should not find event");

    // Different stream = different identity (not found)
    let wrong_stream = substrate
        .event_get(&run1, "stream2", sequence)
        .expect("get should succeed");
    assert!(wrong_stream.is_none(), "Different stream should not find event");

    // Correct identity = found
    let correct = substrate
        .event_get(&run1, "stream1", sequence)
        .expect("get should succeed");
    assert!(correct.is_some(), "Correct identity should find event");
}

#[test]
fn test_i1_sequence_is_unique_within_stream() {
    // Each event in a stream has a unique sequence number
    let (_, substrate) = quick_setup();
    let run = ApiRunId::default();
    let test_data = load_eventlog_test_data();

    let mut sequences = Vec::new();

    for entry in test_data.take(10) {
        let version = substrate
            .event_append(&run, "unique_stream", entry.payload.clone())
            .expect("append should succeed");

        if let Version::Sequence(seq) = version {
            sequences.push(seq);
        }
    }

    // All sequences should be unique
    let mut sorted = sequences.clone();
    sorted.sort();
    sorted.dedup();
    assert_eq!(
        sorted.len(),
        sequences.len(),
        "All sequences should be unique"
    );
}

// =============================================================================
// INVARIANT 2: EVERYTHING IS VERSIONED
// "Every mutation produces a version; reads include version info"
// =============================================================================

#[test]
fn test_i2_append_returns_version() {
    // Every append (mutation) returns a version
    let (_, substrate) = quick_setup();
    let run = ApiRunId::default();
    let test_data = load_eventlog_test_data();

    let entry = &test_data.entries[0];

    let version = substrate
        .event_append(&run, "i2_stream", entry.payload.clone())
        .expect("append should succeed");

    // Must return Version::Sequence (not Version::Txn or other)
    match version {
        Version::Sequence(_seq) => {
            // Sequence is u64, always valid
        }
        _ => panic!("EventLog must return Version::Sequence, got {:?}", version),
    }
}

#[test]
fn test_i2_read_includes_version() {
    // Every read includes version information
    let (_, substrate) = quick_setup();
    let run = ApiRunId::default();
    let test_data = load_eventlog_test_data();

    let entry = &test_data.entries[0];

    let write_version = substrate
        .event_append(&run, "i2_read_stream", entry.payload.clone())
        .expect("append should succeed");

    let write_seq = match write_version {
        Version::Sequence(seq) => seq,
        _ => panic!("Expected sequence"),
    };

    // Read returns Versioned<T> with version info
    let event = substrate
        .event_get(&run, "i2_read_stream", write_seq)
        .expect("get should succeed")
        .expect("event should exist");

    // Versioned wrapper includes version
    match event.version {
        Version::Sequence(read_seq) => {
            assert_eq!(
                read_seq, write_seq,
                "Read version should match write version"
            );
        }
        _ => panic!(
            "Read should return Version::Sequence, got {:?}",
            event.version
        ),
    }
}

#[test]
fn test_i2_versions_are_ordered() {
    // Versions are ordered within an entity (stream)
    let (_, substrate) = quick_setup();
    let run = ApiRunId::default();
    let test_data = load_eventlog_test_data();

    let mut versions = Vec::new();

    for entry in test_data.take(5) {
        let version = substrate
            .event_append(&run, "ordered_stream", entry.payload.clone())
            .expect("append should succeed");

        if let Version::Sequence(seq) = version {
            versions.push(seq);
        }
    }

    // Versions should be strictly increasing
    for window in versions.windows(2) {
        assert!(
            window[1] > window[0],
            "Versions must be strictly ordered: {} should be > {}",
            window[1],
            window[0]
        );
    }
}

#[test]
fn test_i2_range_returns_versioned_events() {
    // Range reads also include version information for each event
    let (_, substrate) = quick_setup();
    let run = ApiRunId::default();
    let test_data = load_eventlog_test_data();

    for entry in test_data.take(5) {
        substrate
            .event_append(&run, "range_versioned", entry.payload.clone())
            .expect("append should succeed");
    }

    let events = substrate
        .event_range(&run, "range_versioned", None, None, None)
        .expect("range should succeed");

    // Each event in range has version info
    for event in events {
        match event.version {
            Version::Sequence(_) => {} // Expected
            _ => panic!(
                "Range events should have Version::Sequence, got {:?}",
                event.version
            ),
        }
    }
}

// =============================================================================
// INVARIANT 3: EVERYTHING IS TRANSACTIONAL
// "All primitives participate in transactions the same way"
// =============================================================================

#[test]
fn test_i3_eventlog_participates_in_transactions() {
    // EventLog operations are transactional
    // Note: Cross-primitive transactions need separate audit
    let (_, substrate) = quick_setup();
    let run = ApiRunId::default();
    let test_data = load_eventlog_test_data();

    // Single-operation transactions (implicit)
    let entry = &test_data.entries[0];
    let result = substrate.event_append(&run, "txn_stream", entry.payload.clone());

    assert!(result.is_ok(), "Event append should be transactional");

    // Verify the transaction committed
    let len = substrate
        .event_len(&run, "txn_stream")
        .expect("len should succeed");
    assert_eq!(len, 1, "Transaction should have committed");
}

#[test]
fn test_i3_operations_are_atomic() {
    // Each append is atomic - either fully succeeds or fully fails
    let (_, substrate) = quick_setup();
    let run = ApiRunId::default();
    let test_data = load_eventlog_test_data();

    let initial_len = substrate
        .event_len(&run, "atomic_stream")
        .expect("len should succeed");

    // Valid append should succeed completely
    let entry = &test_data.entries[0];
    substrate
        .event_append(&run, "atomic_stream", entry.payload.clone())
        .expect("valid append should succeed");

    let after_valid = substrate
        .event_len(&run, "atomic_stream")
        .expect("len should succeed");
    assert_eq!(
        after_valid,
        initial_len + 1,
        "Valid append should add exactly one event"
    );

    // Invalid append should fail completely (no partial state)
    let invalid_result = substrate.event_append(&run, "atomic_stream", Value::Int(42));

    assert!(invalid_result.is_err(), "Invalid payload should fail");

    let after_invalid = substrate
        .event_len(&run, "atomic_stream")
        .expect("len should succeed");
    assert_eq!(
        after_invalid, after_valid,
        "Failed append should not modify state"
    );
}

// =============================================================================
// INVARIANT 4: EVERYTHING HAS A LIFECYCLE
// "Create/Exist/Evolve/Destroy pattern - EventLog is CR (Create/Read only)"
// =============================================================================

#[test]
fn test_i4_lifecycle_create_via_append() {
    // EventLog creates events via append
    let (_, substrate) = quick_setup();
    let run = ApiRunId::default();
    let test_data = load_eventlog_test_data();

    // Stream doesn't exist yet
    let len_before = substrate
        .event_len(&run, "lifecycle_stream")
        .expect("len should succeed");
    assert_eq!(len_before, 0, "Stream should start empty");

    // Create (append)
    let entry = &test_data.entries[0];
    substrate
        .event_append(&run, "lifecycle_stream", entry.payload.clone())
        .expect("append should succeed");

    // Entity now exists
    let len_after = substrate
        .event_len(&run, "lifecycle_stream")
        .expect("len should succeed");
    assert_eq!(len_after, 1, "Event should be created");
}

#[test]
fn test_i4_lifecycle_read_via_get_and_range() {
    // EventLog reads via get and range
    let (_, substrate) = quick_setup();
    let run = ApiRunId::default();
    let test_data = load_eventlog_test_data();

    let entry = &test_data.entries[0];
    let version = substrate
        .event_append(&run, "read_lifecycle", entry.payload.clone())
        .expect("append should succeed");

    let sequence = match version {
        Version::Sequence(seq) => seq,
        _ => panic!("Expected sequence"),
    };

    // Read via get
    let event = substrate
        .event_get(&run, "read_lifecycle", sequence)
        .expect("get should succeed")
        .expect("event should exist");
    assert_eq!(event.value, entry.payload, "Get should return event");

    // Read via range
    let events = substrate
        .event_range(&run, "read_lifecycle", None, None, None)
        .expect("range should succeed");
    assert_eq!(events.len(), 1, "Range should return events");
}

#[test]
fn test_i4_lifecycle_no_update_or_delete() {
    // EventLog is immutable - no update or delete operations
    // This is verified by the absence of update/delete methods in the trait
    // We document this invariant here

    let (_, substrate) = quick_setup();
    let run = ApiRunId::default();
    let test_data = load_eventlog_test_data();

    let entry = &test_data.entries[0];
    let version = substrate
        .event_append(&run, "immutable_stream", entry.payload.clone())
        .expect("append should succeed");

    let sequence = match version {
        Version::Sequence(seq) => seq,
        _ => panic!("Expected sequence"),
    };

    // No event_update method exists (compile-time guarantee)
    // No event_delete method exists (compile-time guarantee)

    // Verify event is still readable (wasn't deleted)
    let event = substrate
        .event_get(&run, "immutable_stream", sequence)
        .expect("get should succeed")
        .expect("event should exist");

    // Verify event wasn't modified
    assert_eq!(event.value, entry.payload, "Event should be unchanged");
}

// =============================================================================
// INVARIANT 5: EVERYTHING EXISTS WITHIN A RUN
// "All data is scoped to a run"
// =============================================================================

#[test]
fn test_i5_run_is_always_explicit() {
    // Every operation requires an explicit run_id parameter
    // This is a compile-time guarantee through the API design
    let (_, substrate) = quick_setup();
    let run = ApiRunId::default();
    let test_data = load_eventlog_test_data();

    let entry = &test_data.entries[0];

    // All operations take &run as first parameter (compile-time check)
    let _ = substrate.event_append(&run, "explicit_run", entry.payload.clone());
    let _ = substrate.event_get(&run, "explicit_run", 0);
    let _ = substrate.event_range(&run, "explicit_run", None, None, None);
    let _ = substrate.event_len(&run, "explicit_run");
    let _ = substrate.event_latest_sequence(&run, "explicit_run");
}

#[test]
fn test_i5_run_is_unit_of_isolation() {
    // Different runs are completely isolated
    let (_, substrate) = quick_setup();
    let run_a = ApiRunId::new();
    let run_b = ApiRunId::new();
    let test_data = load_eventlog_test_data();

    // Create event in run_a
    let entry = &test_data.entries[0];
    substrate
        .event_append(&run_a, "isolated_stream", entry.payload.clone())
        .expect("append should succeed");

    // run_b cannot see run_a's data
    let run_b_events = substrate
        .event_range(&run_b, "isolated_stream", None, None, None)
        .expect("range should succeed");
    assert!(run_b_events.is_empty(), "run_b should not see run_a's events");

    let run_b_len = substrate
        .event_len(&run_b, "isolated_stream")
        .expect("len should succeed");
    assert_eq!(run_b_len, 0, "run_b should have 0 events");

    // run_a can see its own data
    let run_a_len = substrate
        .event_len(&run_a, "isolated_stream")
        .expect("len should succeed");
    assert_eq!(run_a_len, 1, "run_a should have 1 event");
}

#[test]
fn test_i5_same_stream_name_different_runs_are_independent() {
    // Same stream name in different runs = different streams
    let (_, substrate) = quick_setup();
    let run_a = ApiRunId::new();
    let run_b = ApiRunId::new();
    let test_data = load_eventlog_test_data();

    // Both runs use same stream name
    let entry_a = &test_data.entries[0];
    let entry_b = &test_data.entries[1];

    substrate
        .event_append(&run_a, "shared_name", entry_a.payload.clone())
        .expect("append to run_a");

    substrate
        .event_append(&run_b, "shared_name", entry_b.payload.clone())
        .expect("append to run_b");
    substrate
        .event_append(&run_b, "shared_name", entry_b.payload.clone())
        .expect("append to run_b");

    // Different counts per run
    let len_a = substrate
        .event_len(&run_a, "shared_name")
        .expect("len should succeed");
    let len_b = substrate
        .event_len(&run_b, "shared_name")
        .expect("len should succeed");

    assert_eq!(len_a, 1, "run_a should have 1 event");
    assert_eq!(len_b, 2, "run_b should have 2 events");
}

// =============================================================================
// INVARIANT 6: EVERYTHING IS INTROSPECTABLE
// "Can check existence, current state, and version"
// =============================================================================

#[test]
fn test_i6_can_check_existence_via_len() {
    // Can check if events exist via len > 0
    let (_, substrate) = quick_setup();
    let run = ApiRunId::default();
    let test_data = load_eventlog_test_data();

    // Check empty stream
    let len_before = substrate
        .event_len(&run, "introspect_stream")
        .expect("len should succeed");
    assert_eq!(len_before, 0, "Empty stream should have len 0");

    // Add event
    let entry = &test_data.entries[0];
    substrate
        .event_append(&run, "introspect_stream", entry.payload.clone())
        .expect("append should succeed");

    // Check non-empty stream
    let len_after = substrate
        .event_len(&run, "introspect_stream")
        .expect("len should succeed");
    assert_eq!(len_after, 1, "Stream with events should have len > 0");
}

#[test]
fn test_i6_can_check_existence_via_get() {
    // Can check specific event existence via get returning Some/None
    let (_, substrate) = quick_setup();
    let run = ApiRunId::default();
    let test_data = load_eventlog_test_data();

    // Non-existent event
    let not_found = substrate
        .event_get(&run, "introspect_get", 999)
        .expect("get should succeed");
    assert!(not_found.is_none(), "Non-existent event should return None");

    // Create event
    let entry = &test_data.entries[0];
    let version = substrate
        .event_append(&run, "introspect_get", entry.payload.clone())
        .expect("append should succeed");

    let sequence = match version {
        Version::Sequence(seq) => seq,
        _ => panic!("Expected sequence"),
    };

    // Existing event
    let found = substrate
        .event_get(&run, "introspect_get", sequence)
        .expect("get should succeed");
    assert!(found.is_some(), "Existing event should return Some");
}

#[test]
fn test_i6_can_read_current_state() {
    // Can read current state via get and range
    let (_, substrate) = quick_setup();
    let run = ApiRunId::default();
    let test_data = load_eventlog_test_data();

    let entries: Vec<_> = test_data.take(3).to_vec();
    for entry in &entries {
        substrate
            .event_append(&run, "current_state", entry.payload.clone())
            .expect("append should succeed");
    }

    // Read all events (current state of stream)
    let events = substrate
        .event_range(&run, "current_state", None, None, None)
        .expect("range should succeed");

    assert_eq!(events.len(), 3, "Should see all 3 events");

    // Verify payloads match
    for (i, event) in events.iter().enumerate() {
        assert_eq!(event.value, entries[i].payload, "Event {} should match", i);
    }
}

#[test]
fn test_i6_can_check_version_via_latest_sequence() {
    // Can check latest version via latest_sequence
    let (_, substrate) = quick_setup();
    let run = ApiRunId::default();
    let test_data = load_eventlog_test_data();

    // Empty stream has no latest
    let none_latest = substrate
        .event_latest_sequence(&run, "latest_check")
        .expect("latest should succeed");
    assert!(none_latest.is_none(), "Empty stream should have no latest");

    // Add events
    let mut last_seq = 0;
    for entry in test_data.take(3) {
        let version = substrate
            .event_append(&run, "latest_check", entry.payload.clone())
            .expect("append should succeed");
        if let Version::Sequence(seq) = version {
            last_seq = seq;
        }
    }

    // Latest should match last append
    let latest = substrate
        .event_latest_sequence(&run, "latest_check")
        .expect("latest should succeed")
        .expect("should have latest");

    assert_eq!(latest, last_seq, "Latest should match last append sequence");
}

// =============================================================================
// INVARIANT 7: READS AND WRITES HAVE CONSISTENT SEMANTICS
// "Reads don't modify; writes produce versions"
// =============================================================================

#[test]
fn test_i7_reads_never_modify_state() {
    // Read operations never modify state
    let (_, substrate) = quick_setup();
    let run = ApiRunId::default();
    let test_data = load_eventlog_test_data();

    let entry = &test_data.entries[0];
    substrate
        .event_append(&run, "read_safe", entry.payload.clone())
        .expect("append should succeed");

    let len_before = substrate
        .event_len(&run, "read_safe")
        .expect("len should succeed");

    // Multiple reads
    for _ in 0..10 {
        let _ = substrate.event_range(&run, "read_safe", None, None, None);
        let _ = substrate.event_get(&run, "read_safe", 0);
        let _ = substrate.event_len(&run, "read_safe");
        let _ = substrate.event_latest_sequence(&run, "read_safe");
    }

    let len_after = substrate
        .event_len(&run, "read_safe")
        .expect("len should succeed");

    assert_eq!(len_before, len_after, "Reads should not modify state");
}

#[test]
fn test_i7_writes_always_produce_versions() {
    // Every write (append) produces a version
    let (_, substrate) = quick_setup();
    let run = ApiRunId::default();
    let test_data = load_eventlog_test_data();

    for entry in test_data.take(5) {
        let result = substrate.event_append(&run, "write_version", entry.payload.clone());

        match result {
            Ok(version) => match version {
                Version::Sequence(_) => {} // Expected
                _ => panic!("Append should return Version::Sequence"),
            },
            Err(e) => panic!("Append failed: {:?}", e),
        }
    }
}

#[test]
fn test_i7_read_write_separation() {
    // Read methods take &self, write methods take &mut self (compile-time)
    // This is verified by the trait definition
    // Here we verify the semantic separation

    let (_, substrate) = quick_setup();
    let run = ApiRunId::default();
    let test_data = load_eventlog_test_data();

    // Write operation (produces version)
    let entry = &test_data.entries[0];
    let version = substrate
        .event_append(&run, "rw_sep", entry.payload.clone())
        .expect("write should succeed");

    assert!(matches!(version, Version::Sequence(_)), "Write returns version");

    // Read operations (return data, not versions)
    let get_result = substrate
        .event_get(&run, "rw_sep", 0)
        .expect("read should succeed");
    // get returns Option<Versioned<T>>, not just Version

    let range_result = substrate
        .event_range(&run, "rw_sep", None, None, None)
        .expect("read should succeed");
    // range returns Vec<Versioned<T>>, not versions

    let len_result = substrate
        .event_len(&run, "rw_sep")
        .expect("read should succeed");
    // len returns u64, not Version

    // All reads succeeded without modifying state
    let _ = get_result; // Just checking it ran
    let _ = range_result; // Just checking it ran
    let _ = len_result; // Just checking it ran
}

// =============================================================================
// CORE_API_SHAPE REQUIREMENTS
// =============================================================================

#[test]
fn test_api_shape_versioned_wrapper() {
    // Reads return Versioned<T> with value, version, and timestamp
    let (_, substrate) = quick_setup();
    let run = ApiRunId::default();
    let test_data = load_eventlog_test_data();

    let entry = &test_data.entries[0];
    let version = substrate
        .event_append(&run, "versioned_wrapper", entry.payload.clone())
        .expect("append should succeed");

    let sequence = match version {
        Version::Sequence(seq) => seq,
        _ => panic!("Expected sequence"),
    };

    let event = substrate
        .event_get(&run, "versioned_wrapper", sequence)
        .expect("get should succeed")
        .expect("event should exist");

    // Versioned<T> has:
    // - value: the actual data
    assert_eq!(event.value, entry.payload);

    // - version: Version::Sequence for EventLog
    assert!(matches!(event.version, Version::Sequence(_)));

    // Note: timestamp may or may not be present depending on implementation
}

#[test]
fn test_api_shape_version_sequence_type() {
    // EventLog uses Version::Sequence (not TxnId or Counter)
    let (_, substrate) = quick_setup();
    let run = ApiRunId::default();
    let test_data = load_eventlog_test_data();

    let entry = &test_data.entries[0];

    // Write returns Version::Sequence
    let write_version = substrate
        .event_append(&run, "version_type", entry.payload.clone())
        .expect("append should succeed");

    match write_version {
        Version::Sequence(seq) => {
            // Read also returns Version::Sequence
            let event = substrate
                .event_get(&run, "version_type", seq)
                .expect("get should succeed")
                .expect("event should exist");

            match event.version {
                Version::Sequence(read_seq) => {
                    assert_eq!(seq, read_seq, "Sequences should match");
                }
                other => panic!("Expected Version::Sequence in read, got {:?}", other),
            }
        }
        other => panic!("Expected Version::Sequence in write, got {:?}", other),
    }
}

// =============================================================================
// CROSS-MODE CONSISTENCY
// =============================================================================

#[test]
fn test_invariants_hold_across_modes() {
    // All invariants should hold in all durability modes
    let test_data = load_eventlog_test_data();
    let entry = test_data.entries[0].clone();

    test_across_modes("eventlog_invariants", move |db| {
        let substrate = create_substrate(db);
        let run = ApiRunId::default();

        // I1: Addressable - returns sequence
        let version = substrate
            .event_append(&run, "invariant_test", entry.payload.clone())
            .expect("append should succeed");

        let sequence = match version {
            Version::Sequence(seq) => seq,
            _ => panic!("Expected sequence"),
        };

        // I2: Versioned - read includes version
        let event = substrate
            .event_get(&run, "invariant_test", sequence)
            .expect("get should succeed")
            .expect("event should exist");

        assert!(matches!(event.version, Version::Sequence(_)));

        // I5: Run-scoped
        let other_run = ApiRunId::new();
        let other_events = substrate
            .event_range(&other_run, "invariant_test", None, None, None)
            .expect("range should succeed");
        assert!(other_events.is_empty(), "Other run should be isolated");

        // I6: Introspectable
        let len = substrate
            .event_len(&run, "invariant_test")
            .expect("len should succeed");
        assert_eq!(len, 1, "Should have 1 event");

        true // All invariants hold
    });
}
