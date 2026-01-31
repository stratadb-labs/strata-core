//! Error Handling Tests
//!
//! Tests for error conditions in the executor layer.

use crate::common::*;
use strata_core::Value;
use strata_executor::{Command, Error, DistanceMetric, BranchId};

// ============================================================================
// Vector Errors
// ============================================================================

#[test]
fn vector_upsert_to_nonexistent_collection_behavior() {
    let executor = create_executor();

    let result = executor.execute(Command::VectorUpsert {
        run: None,
        collection: "nonexistent".into(),
        key: "v1".into(),
        vector: vec![1.0, 0.0, 0.0, 0.0],
        metadata: None,
    });

    // Note: Current behavior allows upsert to create collection implicitly
    // This is a design choice - documenting current behavior
    // If this changes to require explicit collection creation, update this test
    assert!(result.is_ok(), "Vector upsert should implicitly create collection");
}

#[test]
fn vector_search_in_nonexistent_collection_fails() {
    let executor = create_executor();

    let result = executor.execute(Command::VectorSearch {
        run: None,
        collection: "nonexistent".into(),
        query: vec![1.0, 0.0, 0.0, 0.0],
        k: 10,
        filter: None,
        metric: None,
    });

    match result {
        Err(Error::CollectionNotFound { collection }) => {
            assert!(collection.contains("nonexistent"), "Collection error should reference 'nonexistent', got: {}", collection);
        }
        other => panic!("Expected CollectionNotFound, got {:?}", other),
    }
}

#[test]
fn vector_wrong_dimension_fails() {
    let executor = create_executor();

    executor.execute(Command::VectorCreateCollection {
        run: None,
        collection: "dim4".into(),
        dimension: 4,
        metric: DistanceMetric::Cosine,
    }).unwrap();

    // Try to insert wrong dimension
    let result = executor.execute(Command::VectorUpsert {
        run: None,
        collection: "dim4".into(),
        key: "v1".into(),
        vector: vec![1.0, 0.0], // Only 2 dimensions
        metadata: None,
    });

    match result {
        Err(Error::DimensionMismatch { expected, actual }) => {
            assert_eq!(expected, 4);
            assert_eq!(actual, 2);
        }
        other => panic!("Expected DimensionMismatch, got {:?}", other),
    }
}

#[test]
fn vector_delete_nonexistent_collection_behavior() {
    let executor = create_executor();

    let result = executor.execute(Command::VectorDeleteCollection {
        run: None,
        collection: "nonexistent".into(),
    });

    // Check that deleting nonexistent collection returns false (not error)
    match result {
        Ok(strata_executor::Output::Bool(deleted)) => {
            assert!(!deleted, "Deleting nonexistent should return false");
        }
        Err(_) => {} // Also acceptable if it errors
        other => panic!("Unexpected output: {:?}", other),
    }
}

// ============================================================================
// Run Errors
// ============================================================================

#[test]
fn run_get_nonexistent_returns_none() {
    let executor = create_executor();

    let result = executor.execute(Command::BranchGet {
        run: BranchId::from("nonexistent-run"),
    });

    // RunGet on nonexistent run should either return Maybe(None) or error
    match result {
        Ok(strata_executor::Output::Maybe(None)) => {}
        Err(_) => {} // Also acceptable
        _ => panic!("Unexpected output: {:?}", result),
    }
}

#[test]
fn run_duplicate_id_fails() {
    let executor = create_executor();

    executor.execute(Command::BranchCreate {
        branch_id: Some("unique-run".into()),
        metadata: None,
    }).unwrap();

    // Try to create another with same name
    let result = executor.execute(Command::BranchCreate {
        branch_id: Some("unique-run".into()),
        metadata: None,
    });

    match result {
        Err(Error::BranchExists { branch }) => {
            assert!(branch.contains("unique-run"), "BranchExists error should reference 'unique-run', got: {}", branch);
        }
        Err(Error::InvalidInput { reason }) => {
            assert!(reason.contains("unique-run"), "InvalidInput error should reference 'unique-run', got: {}", reason);
        }
        other => panic!("Expected BranchExists or InvalidInput, got {:?}", other),
    }
}

// ============================================================================
// Transaction Errors
// ============================================================================

#[test]
fn transaction_already_active_error() {
    let mut session = create_session();

    session.execute(Command::TxnBegin {
        run: None,
        options: None,
    }).unwrap();

    let result = session.execute(Command::TxnBegin {
        run: None,
        options: None,
    });

    match result {
        Err(Error::TransactionAlreadyActive) => {}
        Err(e) => panic!("Expected TransactionAlreadyActive, got {:?}", e),
        Ok(_) => panic!("Expected error"),
    }
}

#[test]
fn transaction_not_active_commit_error() {
    let mut session = create_session();

    let result = session.execute(Command::TxnCommit);

    match result {
        Err(Error::TransactionNotActive) => {}
        Err(e) => panic!("Expected TransactionNotActive, got {:?}", e),
        Ok(_) => panic!("Expected error"),
    }
}

#[test]
fn transaction_not_active_rollback_error() {
    let mut session = create_session();

    let result = session.execute(Command::TxnRollback);

    match result {
        Err(Error::TransactionNotActive) => {}
        Err(e) => panic!("Expected TransactionNotActive, got {:?}", e),
        Ok(_) => panic!("Expected error"),
    }
}

// ============================================================================
// Event Errors
// ============================================================================

#[test]
fn event_append_non_object_fails() {
    let executor = create_executor();

    // Event payloads must be Objects
    let result = executor.execute(Command::EventAppend {
        run: None,
        event_type: "stream".into(),
        payload: Value::Int(42), // Not an object
    });

    match result {
        Err(Error::InvalidInput { .. }) => {}
        other => panic!("Expected InvalidInput, got {:?}", other),
    }
}

// ============================================================================
// JSON Errors
// ============================================================================

#[test]
fn json_get_nonexistent_returns_none() {
    let executor = create_executor();

    let result = executor.execute(Command::JsonGet {
        run: None,
        key: "nonexistent".into(),
        path: "$".into(),
    }).unwrap();

    match result {
        strata_executor::Output::Maybe(None) => {}
        _ => panic!("Expected None for nonexistent document"),
    }
}

// ============================================================================
// Error Type Inspection
// ============================================================================

#[test]
fn error_is_serializable() {
    let error = Error::TransactionAlreadyActive;
    let json = serde_json::to_string(&error).unwrap();
    assert!(!json.is_empty());
}

#[test]
fn error_display() {
    let error = Error::TransactionAlreadyActive;
    let msg = error.to_string();
    assert!(!msg.is_empty());
}

// ============================================================================
// Concurrent Error Scenarios
// ============================================================================

#[test]
fn concurrent_sessions_independent_transactions() {
    let db = create_db();

    let mut session1 = strata_executor::Session::new(db.clone());
    let mut session2 = strata_executor::Session::new(db.clone());

    // Both can start transactions
    session1.execute(Command::TxnBegin {
        run: None,
        options: None,
    }).unwrap();

    session2.execute(Command::TxnBegin {
        run: None,
        options: None,
    }).unwrap();

    // Both are in transaction
    assert!(session1.in_transaction());
    assert!(session2.in_transaction());

    // Both can commit
    session1.execute(Command::TxnCommit).unwrap();
    session2.execute(Command::TxnCommit).unwrap();

    assert!(!session1.in_transaction());
    assert!(!session2.in_transaction());
}

// ============================================================================
// State Errors
// ============================================================================

#[test]
fn state_read_nonexistent_returns_none() {
    let executor = create_executor();

    let result = executor.execute(Command::StateRead {
        run: None,
        cell: "nonexistent".into(),
    }).unwrap();

    match result {
        strata_executor::Output::Maybe(None) => {}
        _ => panic!("Expected None for nonexistent cell"),
    }
}
