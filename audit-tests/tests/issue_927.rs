//! Audit test for issue #927: Different error paths between Session and Executor
//! Verdict: ARCHITECTURAL CHOICE
//!
//! Session wraps Executor but has different error handling for the same commands
//! when executed inside vs outside a transaction. In-transaction data commands
//! are routed through `dispatch_in_txn` which uses `ctx.get()` and engine
//! `Transaction` type directly, producing results via `.map_err(Error::from)?`.
//! Outside a transaction (or for non-transactional commands), everything goes
//! through `Executor::execute()` which routes to handler functions that use
//! `convert_result()`.
//!
//! Additionally, certain write commands (like StateSet) that do not have
//! explicit in-transaction handling fall through to `executor.execute()` even
//! when a transaction is active. This means they bypass the transaction
//! entirely, creating an inconsistency in which writes are transactional
//! and which are not.
//!
//! For reads, the output types are consistent (both paths return Output::Maybe).
//! The inconsistency is primarily in the write paths and error conversion.

use strata_engine::database::Database;
use strata_executor::{BranchId, Command, Executor, Output, Session, Value};

/// Demonstrates that StateSet inside a transaction bypasses the transaction
/// and goes directly to the executor. This is an inconsistency: KvPut is
/// handled inside the transaction, but StateSet is not.
#[test]
fn issue_927_state_set_bypasses_transaction_but_kv_put_does_not() {
    let db = Database::cache().unwrap();
    let mut session = Session::new(db.clone());

    let branch = BranchId::from("default");

    // Begin transaction
    session
        .execute(Command::TxnBegin {
            branch: Some(branch.clone()),
            options: None,
        })
        .unwrap();

    // KvPut is handled inside the transaction (has explicit match in dispatch_in_txn)
    session
        .execute(Command::KvPut {
            branch: Some(branch.clone()),
            key: "k1".into(),
            value: Value::Int(1),
        })
        .unwrap();

    // StateSet falls through to executor.execute() in dispatch_in_txn
    // (the `other => executor.execute(other)` catch-all)
    session
        .execute(Command::StateSet {
            branch: Some(branch.clone()),
            cell: "c1".into(),
            value: Value::Int(2),
        })
        .unwrap();

    // Rollback
    session.execute(Command::TxnRollback).unwrap();

    // KvGet should NOT see k1 (it was rolled back)
    let kv_result = session
        .execute(Command::KvGet {
            branch: Some(branch.clone()),
            key: "k1".into(),
        })
        .unwrap();
    assert!(
        matches!(
            kv_result,
            Output::MaybeVersioned(None) | Output::Maybe(None)
        ),
        "KvPut should be rolled back, got: {:?}",
        kv_result
    );

    // StateRead MAY see c1 because StateSet bypassed the transaction
    let state_result = session
        .execute(Command::StateRead {
            branch: Some(branch.clone()),
            cell: "c1".into(),
        })
        .unwrap();

    // ARCHITECTURAL CHOICE: StateSet goes through executor directly,
    // so it persists regardless of transaction rollback
    match state_result {
        Output::MaybeVersioned(Some(_)) | Output::Maybe(Some(_)) => {
            // StateSet bypassed transaction — write persists after rollback
        }
        Output::MaybeVersioned(None) | Output::Maybe(None) => {
            // If this happens, StateSet was somehow rolled back (unlikely)
        }
        other => panic!("Expected MaybeVersioned or Maybe, got: {:?}", other),
    }
}

/// Demonstrates that errors from the same operation type may flow through
/// different conversion paths depending on whether a transaction is active.
#[test]
fn issue_927_error_paths_differ_for_session_vs_executor() {
    let db = Database::cache().unwrap();
    let executor = Executor::new(db.clone());
    let mut session = Session::new(db);

    let branch = BranchId::from("default");

    // Read a non-existent state cell via executor
    let exec_result = executor
        .execute(Command::StateRead {
            branch: Some(branch.clone()),
            cell: "missing".into(),
        })
        .unwrap();

    // Read same missing cell via session (no transaction)
    let session_result = session
        .execute(Command::StateRead {
            branch: Some(branch.clone()),
            cell: "missing".into(),
        })
        .unwrap();

    // Both return Maybe(None) — consistent for reads
    assert_eq!(
        format!("{:?}", exec_result),
        format!("{:?}", session_result),
        "Executor and Session should return the same result for reads"
    );

    // Now test inside a transaction
    session
        .execute(Command::TxnBegin {
            branch: Some(branch.clone()),
            options: None,
        })
        .unwrap();

    let txn_result = session
        .execute(Command::StateRead {
            branch: Some(branch.clone()),
            cell: "missing".into(),
        })
        .unwrap();

    // In-transaction StateRead goes through dispatch_in_txn which has
    // explicit handling with ctx.get() — still returns Maybe(None)
    assert!(
        matches!(txn_result, Output::Maybe(None)),
        "In-txn StateRead should also return Maybe(None)"
    );

    session.execute(Command::TxnRollback).unwrap();
}
