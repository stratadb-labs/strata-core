//! Audit test for issue #853: No branch existence check on TxnBegin
//! Verdict: CONFIRMED BUG
//!
//! TxnBegin with a nonexistent branch succeeds. The error only surfaces
//! on the first read/write operation within the transaction.

use strata_engine::Database;
use strata_executor::{BranchId, Command, Output, Session};

fn setup() -> Session {
    let db = Database::cache().unwrap();
    Session::new(db)
}

#[test]
fn issue_853_txn_begin_nonexistent_branch_succeeds() {
    let mut session = setup();

    // Begin a transaction on a branch that was never created
    let result = session.execute(Command::TxnBegin {
        branch: Some(BranchId::from("this-branch-does-not-exist")),
        options: None,
    });

    // BUG: This succeeds instead of returning an error
    match result {
        Ok(Output::TxnBegun) => {
            // BUG CONFIRMED: Transaction begins on nonexistent branch
        }
        Err(_) => {
            panic!("If this branch is reached, the bug is fixed");
        }
        other => panic!("Unexpected result: {:?}", other),
    }

    // Clean up: rollback the transaction
    let _ = session.execute(Command::TxnRollback);
}

#[test]
fn issue_853_operations_on_nonexistent_branch_in_txn() {
    let mut session = setup();

    // Begin transaction on nonexistent branch
    let begin_result = session.execute(Command::TxnBegin {
        branch: Some(BranchId::from("ghost-branch")),
        options: None,
    });
    assert!(
        begin_result.is_ok(),
        "TxnBegin should succeed (this is the bug)"
    );

    // Operations within the transaction work on an empty namespace
    // Reads return None (no data), writes appear to succeed
    let get_result = session.execute(Command::KvGet {
        branch: Some(BranchId::from("ghost-branch")),
        key: "key1".to_string(),
    });

    match get_result {
        Ok(Output::Maybe(None)) => {
            // Reads silently return None instead of erroring about nonexistent branch
        }
        other => {
            // Any result is fine for this test; we're demonstrating the begin succeeds
            let _ = other;
        }
    }

    // Rollback
    let _ = session.execute(Command::TxnRollback);
}
