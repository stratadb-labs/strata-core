//! Audit test for issue #851: state_cas handler swallows all errors as None
//! Verdict: CONFIRMED BUG
//!
//! The state_cas handler catches all Err(_) from both init() and cas() paths
//! and converts them to Output::MaybeVersion(None), which is the CAS-failure
//! signal. I/O errors and internal errors are silently swallowed.

use strata_engine::Database;
use strata_executor::{Command, Executor, Output, Value};

fn setup() -> Executor {
    let db = Database::ephemeral().unwrap();
    Executor::new(db)
}

#[test]
fn issue_851_state_cas_returns_none_on_version_mismatch() {
    let executor = setup();

    // First, init a state cell
    let result = executor.execute(Command::StateInit {
        branch: None,
        cell: "counter".to_string(),
        value: Value::Int(0),
    });
    assert!(result.is_ok());

    // CAS with wrong expected version should return None (expected behavior)
    let cas_result = executor.execute(Command::StateCas {
        branch: None,
        cell: "counter".to_string(),
        expected_counter: Some(999), // Wrong version
        value: Value::Int(1),
    });

    match cas_result {
        Ok(Output::MaybeVersion(None)) => {
            // This is correct behavior for a version mismatch
        }
        other => panic!("Expected MaybeVersion(None) for mismatch, got: {:?}", other),
    }
}

#[test]
fn issue_851_state_cas_none_path_swallows_errors() {
    let executor = setup();

    // Init a cell first
    let _ = executor.execute(Command::StateInit {
        branch: None,
        cell: "existing".to_string(),
        value: Value::Int(0),
    });

    // CAS with None expected_counter on an existing cell
    // The init semantics (None case) first reads to check existence,
    // then returns MaybeVersion(None) if cell exists.
    // But if state.init() returns an error for another reason, it's also None.
    let cas_result = executor.execute(Command::StateCas {
        branch: None,
        cell: "existing".to_string(),
        expected_counter: None, // Init semantics
        value: Value::Int(1),
    });

    // BUG EVIDENCE: The result is always MaybeVersion(None) for *any* error,
    // whether it's "cell already exists" or an I/O failure.
    // We can't easily trigger an I/O error in a unit test, but we can verify
    // the code path: Err(_) => Ok(Output::MaybeVersion(None))
    match cas_result {
        Ok(Output::MaybeVersion(None)) => {
            // This is returned both for "already exists" AND for I/O errors
            // The bug is that these two cases are indistinguishable
        }
        other => panic!("Expected MaybeVersion(None), got: {:?}", other),
    }
}

#[test]
fn issue_851_state_cas_some_path_swallows_errors() {
    let executor = setup();

    // CAS on a non-existent cell with Some(0) expected counter
    // This should fail because the cell doesn't exist, and the error
    // is caught by the Err(_) => None path
    let cas_result = executor.execute(Command::StateCas {
        branch: None,
        cell: "nonexistent".to_string(),
        expected_counter: Some(0),
        value: Value::Int(1),
    });

    match cas_result {
        Ok(Output::MaybeVersion(None)) => {
            // BUG: This could be a version mismatch OR an internal error.
            // The caller cannot distinguish between the two.
        }
        Ok(Output::MaybeVersion(Some(_))) => {
            // CAS succeeded (also valid if the engine allows it)
        }
        other => panic!("Unexpected result: {:?}", other),
    }
}
