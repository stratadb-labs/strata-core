//! Audit test for issue #938: Vector writes bypass session transaction
//! Verdict: FIXED
//!
//! Previously, Session routed VectorUpsert, VectorDelete, etc. through the
//! executor directly, bypassing any active transaction. This meant vector
//! writes during a transaction were immediately committed and NOT rolled
//! back on TxnRollback.
//!
//! The fix blocks vector write operations inside transactions, returning
//! an InvalidInput error. Vector read operations (VectorGet, VectorSearch,
//! VectorListCollections) still work inside transactions.

use strata_engine::database::Database;
use strata_executor::{BranchId, Command, DistanceMetric, Error, Session};

/// Verifies that vector upsert is now blocked inside a transaction.
#[test]
fn issue_938_vector_writes_bypass_transaction() {
    let db = Database::cache().unwrap();
    let mut session = Session::new(db);
    let branch = BranchId::from("default");

    // Create collection outside transaction
    session
        .execute(Command::VectorCreateCollection {
            branch: Some(branch.clone()),
            collection: "test_col".into(),
            dimension: 3,
            metric: DistanceMetric::Cosine,
        })
        .unwrap();

    // Begin transaction
    session
        .execute(Command::TxnBegin {
            branch: Some(branch.clone()),
            options: None,
        })
        .unwrap();

    // Vector upsert should now be BLOCKED inside a transaction
    let result = session.execute(Command::VectorUpsert {
        branch: Some(branch.clone()),
        collection: "test_col".into(),
        key: "v1".into(),
        vector: vec![1.0, 0.0, 0.0],
        metadata: None,
    });

    // BUG FIXED: Vector writes are now blocked inside transactions
    assert!(
        result.is_err(),
        "Vector upsert should be blocked inside a transaction"
    );
    match result {
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

/// Verifies that vector delete is now blocked inside a transaction.
#[test]
fn issue_938_vector_delete_bypasses_transaction() {
    let db = Database::cache().unwrap();
    let mut session = Session::new(db);
    let branch = BranchId::from("default");

    // Create collection and vector outside transaction
    session
        .execute(Command::VectorCreateCollection {
            branch: Some(branch.clone()),
            collection: "col2".into(),
            dimension: 3,
            metric: DistanceMetric::Cosine,
        })
        .unwrap();

    session
        .execute(Command::VectorUpsert {
            branch: Some(branch.clone()),
            collection: "col2".into(),
            key: "v1".into(),
            vector: vec![1.0, 2.0, 3.0],
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

    // Delete the vector inside the transaction — should now be BLOCKED
    let del_result = session.execute(Command::VectorDelete {
        branch: Some(branch.clone()),
        collection: "col2".into(),
        key: "v1".into(),
    });

    // BUG FIXED: Vector deletes are now blocked inside transactions
    assert!(
        del_result.is_err(),
        "Vector delete should be blocked inside a transaction"
    );

    session.execute(Command::TxnRollback).unwrap();
}
