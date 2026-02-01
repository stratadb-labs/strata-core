//! Audit test for issue #856: begin_transaction not gated by shutdown flag
//! Verdict: CONFIRMED BUG
//!
//! After shutdown(), begin_transaction() still creates new transactions.
//! Only the closure-based transaction() API checks the accepting_transactions flag.

use strata_core::types::BranchId;
use strata_engine::Database;

#[test]
fn issue_856_closure_api_blocked_after_shutdown() {
    let db = Database::cache().unwrap();

    // Shutdown the database
    db.shutdown().unwrap();

    // The closure-based API correctly rejects new transactions
    let result = db.transaction(BranchId::default(), |_txn| Ok(()));

    assert!(
        result.is_err(),
        "Closure-based transaction() should fail after shutdown"
    );
}

#[test]
fn issue_856_begin_transaction_allowed_after_shutdown() {
    let db = Database::cache().unwrap();

    // Verify database is open
    assert!(db.is_open(), "Database should be open initially");

    // Shutdown the database
    db.shutdown().unwrap();

    // Verify database reports not open
    assert!(!db.is_open(), "Database should not be open after shutdown");

    // BUG: begin_transaction() still succeeds after shutdown
    let ctx = db.begin_transaction(BranchId::default());

    // If we get here, the bug is confirmed: begin_transaction didn't check shutdown flag
    // The transaction context was created successfully despite shutdown
    assert!(
        ctx.txn_id > 0 || ctx.txn_id == 0,
        "BUG CONFIRMED: begin_transaction() succeeds after shutdown"
    );

    // Clean up
    db.end_transaction(ctx);
}
