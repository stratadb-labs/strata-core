//! Audit test for issue #939: Branch writes bypass session transaction
//! Verdict: FIXED
//!
//! Previously, Session routed BranchCreate, BranchDelete, etc. through the
//! executor directly, bypassing any active transaction. This meant branch
//! creation/deletion during a transaction was immediately committed and
//! NOT rolled back on TxnRollback.
//!
//! The fix blocks branch create/delete operations inside transactions,
//! returning an InvalidInput error. Branch read operations (BranchGet,
//! BranchList, BranchExists) still work inside transactions.

use strata_engine::database::Database;
use strata_executor::{BranchId, Command, Error, Session};

/// Verifies that BranchCreate is now blocked inside a transaction.
#[test]
fn issue_939_branch_create_bypasses_transaction() {
    let db = Database::cache().unwrap();
    let mut session = Session::new(db);
    let branch = BranchId::from("default");

    // Begin transaction
    session
        .execute(Command::TxnBegin {
            branch: Some(branch.clone()),
            options: None,
        })
        .unwrap();

    // Create a new branch inside the transaction - should now be BLOCKED
    let create_result = session.execute(Command::BranchCreate {
        branch_id: Some("test-branch-939".into()),
        metadata: None,
    });

    // BUG FIXED: Branch create is now blocked inside transactions
    assert!(
        create_result.is_err(),
        "BranchCreate should be blocked inside a transaction"
    );
    match create_result {
        Err(Error::InvalidInput { reason }) => {
            assert!(
                reason.contains("not supported inside a transaction"),
                "Error should explain operation is not supported in transaction, got: {}",
                reason
            );
        }
        Err(other) => panic!("Expected InvalidInput, got: {:?}", other),
        Ok(_) => unreachable!(),
    }

    session.execute(Command::TxnRollback).unwrap();
}

/// Verifies that BranchDelete is now blocked inside a transaction.
#[test]
fn issue_939_branch_delete_bypasses_transaction() {
    let db = Database::cache().unwrap();
    let mut session = Session::new(db);
    let branch = BranchId::from("default");

    // Create a branch outside any transaction
    session
        .execute(Command::BranchCreate {
            branch_id: Some("to-delete-939".into()),
            metadata: None,
        })
        .unwrap();

    // Begin transaction
    session
        .execute(Command::TxnBegin {
            branch: Some(branch.clone()),
            options: None,
        })
        .unwrap();

    // Delete the branch inside the transaction — should now be BLOCKED
    let del_result = session.execute(Command::BranchDelete {
        branch: BranchId::from("to-delete-939"),
    });

    // BUG FIXED: Branch delete is now blocked inside transactions
    assert!(
        del_result.is_err(),
        "BranchDelete should be blocked inside a transaction"
    );

    session.execute(Command::TxnRollback).unwrap();

    // Branch should still exist since delete was blocked
    let exists_after = session
        .execute(Command::BranchExists {
            branch: BranchId::from("to-delete-939"),
        })
        .unwrap();

    assert!(
        matches!(exists_after, strata_executor::Output::Bool(true)),
        "Branch should still exist since delete was blocked by transaction"
    );
}
