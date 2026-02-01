//! Audit test for issue #943: EventReadByType returns version 0 for non-Sequence versions
//! Verdict: CONFIRMED BUG
//!
//! In executor/src/handlers/event.rs, the `event_read_by_type` handler constructs
//! VersionedValue with this version extraction logic (lines 57-59):
//!
//! ```ignore
//! version: match e.version {
//!     Version::Sequence(s) => s,
//!     _ => 0,
//! },
//! ```
//!
//! This means if the event's version is NOT a Sequence variant (e.g., if it's a
//! Txn or Counter variant), the version is silently replaced with 0.
//!
//! Compare this to the `event_read` handler which uses `bridge::extract_version(&e.version)`,
//! which correctly extracts the u64 from ANY variant. The `event_read_by_type` handler
//! should use the same approach.
//!
//! Impact: If events are stored with a non-Sequence version (which can happen if the
//! versioning scheme changes), EventReadByType will return version 0 for all events,
//! losing version information.
//!
//! Currently events always use Sequence versions, so the match works. But
//! event_read uses the robust `extract_version()` while event_read_by_type uses
//! the fragile pattern match -- an inconsistency that will break if the version
//! scheme ever changes.

use strata_core::value::Value;
use strata_engine::database::Database;
use strata_executor::BranchId;
use strata_executor::{Command, Executor, Output};

/// Demonstrates the inconsistency between EventRead and EventReadByType version extraction.
///
/// EventRead uses `bridge::extract_version()` (handles all variants).
/// EventReadByType uses `match { Sequence(s) => s, _ => 0 }` (only handles Sequence).
///
/// Currently both work because events use Sequence versions, but the code paths differ.
#[test]
fn issue_943_event_read_by_type_version_extraction_inconsistency() {
    let db = Database::cache().unwrap();
    let executor = Executor::new(db);
    let branch = BranchId::from("default");

    // Append multiple events to get non-zero sequence numbers
    for i in 0..5 {
        executor
            .execute(Command::EventAppend {
                branch: Some(branch.clone()),
                event_type: "user.created".into(),
                payload: Value::Object(std::collections::HashMap::from([(
                    "index".to_string(),
                    Value::Int(i),
                )])),
            })
            .unwrap();
    }

    // Read by type - uses fragile version extraction
    let by_type_result = executor
        .execute(Command::EventReadByType {
            branch: Some(branch.clone()),
            event_type: "user.created".into(),
            limit: None,
            after_sequence: None,
        })
        .unwrap();

    let by_type_versions: Vec<u64> = match by_type_result {
        Output::VersionedValues(events) => {
            assert_eq!(events.len(), 5, "Should have 5 events");
            events.iter().map(|e| e.version).collect()
        }
        other => panic!("Expected VersionedValues, got {:?}", other),
    };

    // Read each event individually - uses robust extract_version
    let mut individual_versions = Vec::new();
    for seq in 0..5u64 {
        let read_result = executor
            .execute(Command::EventRead {
                branch: Some(branch.clone()),
                sequence: seq,
            })
            .unwrap();

        match read_result {
            Output::MaybeVersioned(Some(versioned)) => {
                individual_versions.push(versioned.version);
            }
            _ => {
                // Event not found at this sequence, try to continue
            }
        }
    }

    // Both should return the same version numbers.
    // Currently they do because events use Sequence versions.
    // BUG: If events were stored with Txn or Counter versions,
    // event_read_by_type would return 0 for all versions while
    // event_read would return the correct values.
    assert_eq!(
        by_type_versions.len(),
        individual_versions.len(),
        "Should have same number of events from both read paths"
    );

    for (i, (by_type_v, individual_v)) in by_type_versions
        .iter()
        .zip(individual_versions.iter())
        .enumerate()
    {
        assert_eq!(
            by_type_v, individual_v,
            "Event {} version mismatch: read_by_type={}, read={}. \
             If these differ, the bug has manifested (non-Sequence version returned 0 from read_by_type).",
            i, by_type_v, individual_v
        );
    }

    // Verify that versions are sequential (0, 1, 2, 3, 4)
    for (i, v) in by_type_versions.iter().enumerate() {
        assert_eq!(
            *v, i as u64,
            "Event {} should have sequence version {}, got {}",
            i, i, v
        );
    }
}

/// Documents the fragile code pattern in event_read_by_type.
///
/// The version extraction uses a match that only handles Sequence:
///   version: match e.version { Version::Sequence(s) => s, _ => 0 }
///
/// While event_read uses the generic extract_version:
///   version: bridge::extract_version(&e.version)
///
/// This test confirms the current behavior works (events use Sequence versions)
/// but documents the inconsistency for future reference.
#[test]
fn issue_943_documents_fragile_version_pattern() {
    let db = Database::cache().unwrap();
    let executor = Executor::new(db);
    let branch = BranchId::from("default");

    // Append an event
    let append_result = executor
        .execute(Command::EventAppend {
            branch: Some(branch.clone()),
            event_type: "test.type".into(),
            payload: Value::Object(std::collections::HashMap::from([(
                "msg".to_string(),
                Value::String("hello".into()),
            )])),
        })
        .unwrap();

    let append_version = match append_result {
        Output::Version(v) => v,
        other => panic!("Expected Version, got {:?}", other),
    };

    // The append returns sequence 0 for the first event
    assert_eq!(append_version, 0, "First event should have sequence 0");

    // Read by type returns the same version (currently works because Sequence)
    let by_type = executor
        .execute(Command::EventReadByType {
            branch: Some(branch.clone()),
            event_type: "test.type".into(),
            limit: None,
            after_sequence: None,
        })
        .unwrap();

    match by_type {
        Output::VersionedValues(events) => {
            assert_eq!(events.len(), 1);
            // Version matches because it IS a Sequence variant.
            // BUG: If this were Version::Txn(0), event_read_by_type would
            // return 0 (coincidentally correct) but for Version::Txn(42)
            // it would incorrectly return 0 instead of 42.
            assert_eq!(
                events[0].version, append_version,
                "Version from read_by_type should match append version"
            );
        }
        other => panic!("Expected VersionedValues, got {:?}", other),
    }
}
