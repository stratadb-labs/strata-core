//! Audit test for issue #922: Event 200 retries vs State no retry on version conflict
//! Verdict: ARCHITECTURAL CHOICE
//!
//! EventLog::append uses transaction_with_retry with max_retries=200 and
//! exponential backoff (1ms base, 50ms max). This is because event appends
//! serialize through CAS on a metadata key, so conflicts are expected under
//! concurrency.
//!
//! StateCell::set and StateCell::cas use the standard transaction() method
//! which retries with the default RetryConfig (typically 3 retries).
//!
//! This asymmetry means:
//! - Event appends are highly resilient to contention (200 retries)
//! - State operations can fail under moderate contention (few retries)
//! - The difference is intentional: events serialize through a single metadata
//!   key (high contention), while state cells are per-cell (lower contention).

use strata_executor::{Command, Output};

/// Verify that event append works correctly (basic functionality).
/// The 200-retry mechanism is internal to the engine and not directly
/// observable through the executor API, but we can verify the append succeeds.
#[test]
fn issue_922_event_append_succeeds() {
    let db = strata_engine::database::Database::cache().unwrap();
    let executor = strata_executor::Executor::new(db);

    let branch = strata_executor::BranchId::from("default");

    // Append several events sequentially
    for i in 0..10 {
        let result = executor
            .execute(Command::EventAppend {
                branch: Some(branch.clone()),
                event_type: "test".into(),
                payload: strata_core::value::Value::Object(
                    vec![("seq".to_string(), strata_core::value::Value::Int(i))]
                        .into_iter()
                        .collect(),
                ),
            })
            .unwrap();

        assert!(
            matches!(result, Output::Version(_)),
            "Event append should succeed and return a version"
        );
    }

    // Verify all events were appended
    let len_result = executor
        .execute(Command::EventLen {
            branch: Some(branch.clone()),
        })
        .unwrap();

    match len_result {
        Output::Uint(count) => {
            assert_eq!(count, 10, "All 10 events should be appended");
        }
        other => panic!("Expected Uint, got: {:?}", other),
    }
}

/// Verify that state CAS works with correct expected counter.
/// State does not have 200 retries -- it uses the default retry count.
#[test]
fn issue_922_state_cas_works_without_retries() {
    let db = strata_engine::database::Database::cache().unwrap();
    let executor = strata_executor::Executor::new(db);

    let branch = strata_executor::BranchId::from("default");

    // Initialize a state cell
    let init_result = executor
        .execute(Command::StateInit {
            branch: Some(branch.clone()),
            cell: "counter".into(),
            value: strata_core::value::Value::Int(0),
        })
        .unwrap();

    let init_version = match init_result {
        Output::Version(v) => v,
        other => panic!("Expected Version, got: {:?}", other),
    };

    // CAS with the correct expected counter
    let cas_result = executor
        .execute(Command::StateCas {
            branch: Some(branch.clone()),
            cell: "counter".into(),
            expected_counter: Some(init_version),
            value: strata_core::value::Value::Int(1),
        })
        .unwrap();

    assert!(
        matches!(cas_result, Output::MaybeVersion(Some(_))),
        "CAS with correct counter should succeed"
    );

    // CAS with wrong expected counter should fail (no retry)
    let cas_fail = executor
        .execute(Command::StateCas {
            branch: Some(branch.clone()),
            cell: "counter".into(),
            expected_counter: Some(999),
            value: strata_core::value::Value::Int(2),
        })
        .unwrap();

    assert!(
        matches!(cas_fail, Output::MaybeVersion(None)),
        "CAS with wrong counter should return None (no retry attempted)"
    );

    // ARCHITECTURAL NOTE:
    // EventLog::append in engine/src/primitives/event.rs:311-317 uses:
    //   RetryConfig::default().with_max_retries(200).with_base_delay_ms(1).with_max_delay_ms(50)
    //
    // StateCell::set/cas uses the standard transaction() method with default retries.
    // This is intentional: event appends funnel through a single metadata key per branch,
    // creating high contention. State cells are independent, so contention is lower.
}
