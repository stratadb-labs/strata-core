//! Audit test for issue #941: BranchExport/Import/Validate route through catch-all in session
//! Verdict: CONFIRMED BUG (minor)
//!
//! In session.rs, the `execute()` method has three match arms:
//! 1. Transaction lifecycle commands (TxnBegin, TxnCommit, etc.)
//! 2. Non-transactional commands (Branch*, Vector*, Ping, etc.)
//! 3. A catch-all `_ =>` arm for data commands
//!
//! BranchExport, BranchImport, and BranchBundleValidate are NOT listed in the
//! non-transactional arm (#2). They fall through to the catch-all arm (#3).
//!
//! When no transaction is active, the catch-all delegates to `self.executor.execute(cmd)`,
//! which is the correct behavior. However, when a transaction IS active, the catch-all
//! routes to `self.execute_in_txn(cmd)`, which in turn falls through its own catch-all
//! (`other => executor.execute(other)`) back to the executor.
//!
//! The result is that BranchExport/Import/Validate still work correctly, but they take
//! a circuitous path through the transaction machinery when a transaction is active.
//! This is a minor routing bug -- they should be in the non-transactional arm alongside
//! the other Branch* commands.
//!
//! Evidence from session.rs lines 82-108:
//! ```ignore
//! // Non-transactional commands always go to executor
//! Command::BranchCreate { .. }
//! | Command::BranchGet { .. }
//! | Command::BranchList { .. }
//! | Command::BranchExists { .. }
//! | Command::BranchDelete { .. }
//! | Command::VectorUpsert { .. }
//! // ... etc ...
//! => self.executor.execute(cmd),
//! ```
//!
//! Missing from this list:
//! - Command::BranchExport { .. }
//! - Command::BranchImport { .. }
//! - Command::BranchBundleValidate { .. }

use strata_engine::database::Database;
use strata_executor::{Command, Session};

/// Verify that BranchExport command exists and can be constructed.
/// The actual export requires a valid path and branch, so we just confirm
/// the command variant exists and routes through the session without panicking.
#[test]
fn issue_941_branch_export_routes_through_session() {
    let db = Database::cache().unwrap();
    let mut session = Session::new(db);

    // BranchExport requires a valid branch_id and path.
    // We use a non-existent branch and path to trigger an error,
    // but the important thing is that the command routes correctly
    // (i.e., it doesn't panic or route to the wrong handler).
    let result = session.execute(Command::BranchExport {
        branch_id: "nonexistent_branch".into(),
        path: "/tmp/strata_test_941_export.tar.zst".into(),
    });

    // We expect an error because the branch doesn't exist,
    // but the command should route correctly through the session.
    // The bug is about routing, not about the operation itself.
    assert!(
        result.is_err(),
        "BranchExport on nonexistent branch should error, confirming it routes through executor"
    );
}

/// Verify that BranchBundleValidate command routes through session.
#[test]
fn issue_941_branch_bundle_validate_routes_through_session() {
    let db = Database::cache().unwrap();
    let mut session = Session::new(db);

    let result = session.execute(Command::BranchBundleValidate {
        path: "/tmp/nonexistent_bundle.tar.zst".into(),
    });

    // Should error because file doesn't exist, but routes correctly
    assert!(
        result.is_err(),
        "BranchBundleValidate on nonexistent file should error"
    );
}

/// Demonstrate the routing issue: when a transaction is active,
/// BranchExport takes the circuitous path through execute_in_txn.
///
/// With a transaction active, these commands should still go directly
/// to the executor (they are non-transactional), but instead they go
/// through execute_in_txn -> catch-all -> executor.execute().
#[test]
fn issue_941_branch_export_during_active_transaction() {
    let db = Database::cache().unwrap();
    let mut session = Session::new(db);

    // Begin a transaction
    let begin_result = session.execute(Command::TxnBegin {
        branch: None,
        options: None,
    });
    assert!(begin_result.is_ok(), "TxnBegin should succeed");

    // BranchExport during active transaction.
    // BUG: This goes through execute_in_txn's catch-all instead of
    // being routed directly to self.executor.execute(cmd) like other
    // Branch* commands.
    let result = session.execute(Command::BranchExport {
        branch_id: "nonexistent_branch".into(),
        path: "/tmp/strata_test_941_txn_export.tar.zst".into(),
    });

    // The result should be the same error regardless of routing path,
    // but the code path is different (and less efficient) than intended.
    assert!(
        result.is_err(),
        "BranchExport should still work (via circuitous path) during active transaction"
    );

    // Clean up: rollback the transaction
    let _ = session.execute(Command::TxnRollback);
}
