//! Audit test for issue #932: vector_upsert previously ignored ALL create_collection errors
//! Verdict: FIXED (auto-create removed)
//!
//! The auto-create logic in vector_upsert has been removed entirely. Collections must
//! now be explicitly created via VectorCreateCollection before upserting. This eliminates
//! the `let _ =` pattern that silently swallowed all create_collection errors.

use strata_engine::database::Database;
use strata_executor::{BranchId, Command, DistanceMetric, Executor};

/// Demonstrates that vector_upsert now requires explicit collection creation.
/// Without VectorCreateCollection, upsert returns CollectionNotFound.
#[test]
fn issue_932_vector_upsert_requires_explicit_collection() {
    let db = Database::cache().unwrap();
    let executor = Executor::new(db);

    let branch = BranchId::from("default");

    // Upsert without creating collection first -- should now fail
    let result = executor.execute(Command::VectorUpsert {
        branch: Some(branch.clone()),
        collection: "auto_created".into(),
        key: "v1".into(),
        vector: vec![1.0, 0.0, 0.0],
        metadata: None,
    });

    assert!(
        result.is_err(),
        "VectorUpsert should fail without explicit collection creation (auto-create removed)"
    );
}

/// Demonstrates that repeated upsert to the same collection works correctly
/// when the collection is explicitly created first.
#[test]
fn issue_932_repeated_upsert_with_explicit_collection() {
    let db = Database::cache().unwrap();
    let executor = Executor::new(db);

    let branch = BranchId::from("default");

    // Explicitly create collection first
    executor
        .execute(Command::VectorCreateCollection {
            branch: Some(branch.clone()),
            collection: "col".into(),
            dimension: 3,
            metric: DistanceMetric::Cosine,
        })
        .unwrap();

    // First upsert
    executor
        .execute(Command::VectorUpsert {
            branch: Some(branch.clone()),
            collection: "col".into(),
            key: "v1".into(),
            vector: vec![1.0, 2.0, 3.0],
            metadata: None,
        })
        .unwrap();

    // Second upsert to same collection should also succeed
    let result = executor.execute(Command::VectorUpsert {
        branch: Some(branch.clone()),
        collection: "col".into(),
        key: "v2".into(),
        vector: vec![4.0, 5.0, 6.0],
        metadata: None,
    });

    assert!(
        result.is_ok(),
        "Second upsert should succeed on explicitly created collection"
    );
}

/// Demonstrates that upserting with a different dimension to an existing
/// collection fails with a dimension mismatch at the insert step.
#[test]
fn issue_932_dimension_mismatch_error() {
    let db = Database::cache().unwrap();
    let executor = Executor::new(db);

    let branch = BranchId::from("default");

    // Create collection with dimension 3
    executor
        .execute(Command::VectorCreateCollection {
            branch: Some(branch.clone()),
            collection: "dim_test".into(),
            dimension: 3,
            metric: DistanceMetric::Cosine,
        })
        .unwrap();

    // Upsert with correct dimension
    executor
        .execute(Command::VectorUpsert {
            branch: Some(branch.clone()),
            collection: "dim_test".into(),
            key: "v1".into(),
            vector: vec![1.0, 2.0, 3.0],
            metadata: None,
        })
        .unwrap();

    // Try to upsert with wrong dimension (5 instead of 3)
    let result = executor.execute(Command::VectorUpsert {
        branch: Some(branch.clone()),
        collection: "dim_test".into(),
        key: "v2".into(),
        vector: vec![1.0, 2.0, 3.0, 4.0, 5.0],
        metadata: None,
    });

    assert!(
        result.is_err(),
        "Should fail with dimension mismatch on insert"
    );
}
