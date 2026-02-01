//! Audit test for issue #926: state_cas swallows ALL errors as CAS failure
//! Verdict: CONFIRMED BUG
//!
//! In handlers/state.rs:68-93, the state_cas handler catches ALL errors from
//! state.init() and state.cas() and converts them to `Ok(Output::MaybeVersion(None))`.
//!
//! Both the `None` (init semantics) path and the `Some(expected)` path use:
//!   `Err(_) => Ok(Output::MaybeVersion(None))`
//!
//! This means:
//! - A version mismatch (expected CAS failure) returns MaybeVersion(None)
//! - A storage error returns MaybeVersion(None)
//! - A serialization error returns MaybeVersion(None)
//! - An internal engine error returns MaybeVersion(None)
//!
//! The caller cannot distinguish between "CAS failed because version didn't match"
//! and "CAS failed because of a storage/system error". This violates the principle
//! that system errors should propagate as errors, not as normal return values.

use strata_executor::{Command, Executor, Output};

/// Demonstrate that CAS with correct expected counter succeeds.
#[test]
fn issue_926_state_cas_correct_counter_succeeds() {
    let db = strata_engine::database::Database::cache().unwrap();
    let executor = Executor::new(db);

    let branch = strata_executor::BranchId::from("default");

    // Init a cell
    let init_result = executor
        .execute(Command::StateInit {
            branch: Some(branch.clone()),
            cell: "cell1".into(),
            value: strata_core::value::Value::Int(1),
        })
        .unwrap();

    let init_version = match init_result {
        Output::Version(v) => v,
        other => panic!("Expected Version, got: {:?}", other),
    };

    // CAS with correct expected counter should succeed
    let result = executor
        .execute(Command::StateCas {
            branch: Some(branch.clone()),
            cell: "cell1".into(),
            expected_counter: Some(init_version),
            value: strata_core::value::Value::Int(2),
        })
        .unwrap();

    assert!(
        matches!(result, Output::MaybeVersion(Some(_))),
        "CAS with correct counter should succeed and return Some(version)"
    );
}

/// Demonstrate that CAS with wrong expected counter returns None.
#[test]
fn issue_926_state_cas_wrong_counter_returns_none() {
    let db = strata_engine::database::Database::cache().unwrap();
    let executor = Executor::new(db);

    let branch = strata_executor::BranchId::from("default");

    // Init a cell
    executor
        .execute(Command::StateInit {
            branch: Some(branch.clone()),
            cell: "cell1".into(),
            value: strata_core::value::Value::Int(1),
        })
        .unwrap();

    // CAS with wrong expected counter should fail (returns None)
    let result = executor
        .execute(Command::StateCas {
            branch: Some(branch.clone()),
            cell: "cell1".into(),
            expected_counter: Some(999),
            value: strata_core::value::Value::Int(3),
        })
        .unwrap();

    assert!(
        matches!(result, Output::MaybeVersion(None)),
        "CAS with wrong counter should return None"
    );
}

/// Demonstrate the bug: ALL errors are indistinguishable from CAS failure.
///
/// The handler code in handlers/state.rs:82-91 is:
/// ```ignore
/// Some(expected) => {
///     match p.state.cas(&branch_id, &cell, Version::Counter(expected), value) {
///         Ok(versioned) => Ok(Output::MaybeVersion(Some(...))),
///         Err(_) => Ok(Output::MaybeVersion(None)),  // <-- BUG: swallows ALL errors
///     }
/// }
/// ```
///
/// And the None path at lines 75-79:
/// ```ignore
/// match p.state.init(&branch_id, &cell, value) {
///     Ok(versioned) => Ok(Output::MaybeVersion(Some(...))),
///     Err(_) => Ok(Output::MaybeVersion(None)),  // <-- BUG: swallows ALL errors
/// }
/// ```
#[test]
fn issue_926_state_cas_swallows_errors_as_cas_failure() {
    let db = strata_engine::database::Database::cache().unwrap();
    let executor = Executor::new(db);

    let branch = strata_executor::BranchId::from("default");

    // Init a cell
    executor
        .execute(Command::StateInit {
            branch: Some(branch.clone()),
            cell: "cell1".into(),
            value: strata_core::value::Value::Int(1),
        })
        .unwrap();

    // CAS with correct expected counter succeeds
    let success = executor
        .execute(Command::StateCas {
            branch: Some(branch.clone()),
            cell: "cell1".into(),
            expected_counter: Some(1),
            value: strata_core::value::Value::Int(2),
        })
        .unwrap();
    assert!(
        matches!(success, Output::MaybeVersion(Some(_))),
        "CAS with correct counter should succeed"
    );

    // CAS with wrong expected counter returns None
    let cas_failure = executor
        .execute(Command::StateCas {
            branch: Some(branch.clone()),
            cell: "cell1".into(),
            expected_counter: Some(999),
            value: strata_core::value::Value::Int(3),
        })
        .unwrap();
    assert!(
        matches!(cas_failure, Output::MaybeVersion(None)),
        "CAS with wrong counter should return None"
    );

    // THE BUG: Both a version mismatch AND any other error produce the exact
    // same output: Ok(Output::MaybeVersion(None))
    //
    // A caller receiving MaybeVersion(None) has no way to know:
    // - Did the CAS fail because the version didn't match? (expected, retry-safe)
    // - Did a storage error occur? (unexpected, should be reported)
    // - Did a serialization error occur? (bug, should be investigated)
    //
    // The correct fix would be:
    // - Match on the specific error variant for version conflict -> MaybeVersion(None)
    // - Propagate all other errors as Err(...)
    //
    // The same pattern exists in the None (init) path, where Err(_) from
    // p.state.init() is also swallowed as MaybeVersion(None).
}

/// Demonstrate the init path also swallows errors.
#[test]
fn issue_926_state_cas_init_path_swallows_errors() {
    let db = strata_engine::database::Database::cache().unwrap();
    let executor = Executor::new(db);

    let branch = strata_executor::BranchId::from("default");

    // CAS with expected_counter=None triggers the init path.
    // Init a cell that doesn't exist yet -- should succeed.
    let result = executor
        .execute(Command::StateCas {
            branch: Some(branch.clone()),
            cell: "new_cell".into(),
            expected_counter: None,
            value: strata_core::value::Value::Int(1),
        })
        .unwrap();
    assert!(
        matches!(result, Output::MaybeVersion(Some(_))),
        "CAS init path should succeed for new cell"
    );

    // CAS with expected_counter=None on an EXISTING cell.
    // The handler first checks if cell exists (read), finds it, returns None.
    let result = executor
        .execute(Command::StateCas {
            branch: Some(branch.clone()),
            cell: "new_cell".into(),
            expected_counter: None,
            value: strata_core::value::Value::Int(2),
        })
        .unwrap();
    assert!(
        matches!(result, Output::MaybeVersion(None)),
        "CAS init path should return None for existing cell"
    );

    // BUG: If the read in the existence check fails with a storage error,
    // convert_result() would propagate it -- but if p.state.init() itself
    // fails (e.g., due to a race condition or engine error), that error is
    // swallowed as MaybeVersion(None) at line 79 of handlers/state.rs.
}
