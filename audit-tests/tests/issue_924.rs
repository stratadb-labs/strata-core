//! Audit test for issue #924: EventReadByType O(N) scan over all events
//! Verdict: ARCHITECTURAL CHOICE
//!
//! EventReadByType reads ALL events in the branch and filters by type in memory.
//! There is no type-based index -- the engine scans every event from sequence 0
//! to the end, checking if each event's type matches the requested type.
//!
//! For a branch with M total events and K matching events, this is O(M) regardless
//! of K. A branch with 1 million events and only 10 matching "error" events still
//! scans all 1 million events.

use strata_executor::{Command, Output};

/// Demonstrate that EventReadByType scans all events and returns only matching ones.
#[test]
fn issue_924_event_read_by_type_filters_correctly() {
    let db = strata_engine::database::Database::cache().unwrap();
    let executor = strata_executor::Executor::new(db);

    let branch = strata_executor::BranchId::from("default");

    // Append events of different types
    let types = [
        "click", "view", "click", "purchase", "view", "click", "view", "error",
    ];
    for (i, event_type) in types.iter().enumerate() {
        executor
            .execute(Command::EventAppend {
                branch: Some(branch.clone()),
                event_type: event_type.to_string(),
                payload: strata_core::value::Value::Object(
                    vec![(
                        "index".to_string(),
                        strata_core::value::Value::Int(i as i64),
                    )]
                    .into_iter()
                    .collect(),
                ),
            })
            .unwrap();
    }

    // Verify total event count
    let len_result = executor
        .execute(Command::EventLen {
            branch: Some(branch.clone()),
        })
        .unwrap();
    assert!(
        matches!(len_result, Output::Uint(8)),
        "Should have 8 total events"
    );

    // Read by type "click" -- should return 3 events
    let click_result = executor
        .execute(Command::EventReadByType {
            branch: Some(branch.clone()),
            event_type: "click".into(),
            limit: None,
            after_sequence: None,
        })
        .unwrap();

    match click_result {
        Output::VersionedValues(events) => {
            assert_eq!(events.len(), 3, "Should find exactly 3 'click' events");
        }
        other => panic!("Expected VersionedValues, got: {:?}", other),
    }

    // Read by type "error" -- should return 1 event
    let error_result = executor
        .execute(Command::EventReadByType {
            branch: Some(branch.clone()),
            event_type: "error".into(),
            limit: None,
            after_sequence: None,
        })
        .unwrap();

    match error_result {
        Output::VersionedValues(events) => {
            assert_eq!(events.len(), 1, "Should find exactly 1 'error' event");
        }
        other => panic!("Expected VersionedValues, got: {:?}", other),
    }

    // Read by type that doesn't exist -- should return empty
    let none_result = executor
        .execute(Command::EventReadByType {
            branch: Some(branch.clone()),
            event_type: "nonexistent".into(),
            limit: None,
            after_sequence: None,
        })
        .unwrap();

    match none_result {
        Output::VersionedValues(events) => {
            assert_eq!(
                events.len(),
                0,
                "Should find 0 events for nonexistent type. \
                 Note: the engine still scanned all 8 events to determine this."
            );
        }
        other => panic!("Expected VersionedValues, got: {:?}", other),
    }

    // ARCHITECTURAL NOTE:
    // All three queries above scan the same 8 events internally.
    // The O(N) scan is observable in the engine code: it reads every event
    // sequentially from sequence 0 and filters by event_type string comparison.
    //
    // For production workloads with millions of events, this would be
    // prohibitively slow. A secondary index on event_type would reduce this
    // to O(K) where K is the number of matching events.
}
