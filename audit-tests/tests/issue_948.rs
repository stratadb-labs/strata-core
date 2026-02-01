//! Audit test for issue #948: NaN/Infinity in vector embeddings not validated
//! Verdict: FIXED (in PR #915, commit 17e7148)
//!
//! The VectorStore's insert() method now validates float values in the embedding,
//! rejecting NaN and Infinity with VectorError::InvalidEmbedding before any storage
//! or index operations occur. This prevents poisoned similarity calculations.

use strata_engine::database::Database;
use strata_executor::{BranchId, Command, DistanceMetric, Executor};

/// Verify that NaN values in vector embeddings are rejected.
#[test]
fn issue_948_nan_vector_rejected() {
    let db = Database::cache().unwrap();
    let executor = Executor::new(db);
    let branch = BranchId::from("default");

    executor
        .execute(Command::VectorCreateCollection {
            branch: Some(branch.clone()),
            collection: "col1".into(),
            dimension: 3,
            metric: DistanceMetric::Cosine,
        })
        .unwrap();

    let result = executor.execute(Command::VectorUpsert {
        branch: Some(branch.clone()),
        collection: "col1".into(),
        key: "nan_vec".into(),
        vector: vec![f32::NAN, 0.0, 0.0],
        metadata: None,
    });

    assert!(
        result.is_err(),
        "NaN vector should be rejected with an InvalidEmbedding error"
    );
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("NaN") || err.contains("Infinity") || err.contains("Invalid embedding"),
        "Error should mention invalid embedding. Got: {}",
        err
    );
}

/// Verify that Infinity values in vector embeddings are rejected.
#[test]
fn issue_948_infinity_vector_rejected() {
    let db = Database::cache().unwrap();
    let executor = Executor::new(db);
    let branch = BranchId::from("default");

    executor
        .execute(Command::VectorCreateCollection {
            branch: Some(branch.clone()),
            collection: "col1".into(),
            dimension: 3,
            metric: DistanceMetric::Cosine,
        })
        .unwrap();

    let result = executor.execute(Command::VectorUpsert {
        branch: Some(branch.clone()),
        collection: "col1".into(),
        key: "inf_vec".into(),
        vector: vec![f32::INFINITY, 0.0, 0.0],
        metadata: None,
    });

    assert!(
        result.is_err(),
        "Infinity vector should be rejected with an InvalidEmbedding error"
    );
}

/// Verify that negative infinity is also rejected.
#[test]
fn issue_948_neg_infinity_vector_rejected() {
    let db = Database::cache().unwrap();
    let executor = Executor::new(db);
    let branch = BranchId::from("default");

    executor
        .execute(Command::VectorCreateCollection {
            branch: Some(branch.clone()),
            collection: "col1".into(),
            dimension: 3,
            metric: DistanceMetric::Cosine,
        })
        .unwrap();

    let result = executor.execute(Command::VectorUpsert {
        branch: Some(branch.clone()),
        collection: "col1".into(),
        key: "neg_inf_vec".into(),
        vector: vec![f32::NEG_INFINITY, 0.0, 0.0],
        metadata: None,
    });

    assert!(
        result.is_err(),
        "Negative infinity vector should be rejected with an InvalidEmbedding error"
    );
}

/// Verify that valid embeddings still work after the validation was added.
#[test]
fn issue_948_valid_vector_still_accepted() {
    let db = Database::cache().unwrap();
    let executor = Executor::new(db);
    let branch = BranchId::from("default");

    executor
        .execute(Command::VectorCreateCollection {
            branch: Some(branch.clone()),
            collection: "col1".into(),
            dimension: 3,
            metric: DistanceMetric::Cosine,
        })
        .unwrap();

    let result = executor.execute(Command::VectorUpsert {
        branch: Some(branch.clone()),
        collection: "col1".into(),
        key: "good_vec".into(),
        vector: vec![1.0, 0.0, 0.0],
        metadata: None,
    });

    assert!(
        result.is_ok(),
        "Valid vector should be accepted. Got error: {:?}",
        result.err()
    );
}
