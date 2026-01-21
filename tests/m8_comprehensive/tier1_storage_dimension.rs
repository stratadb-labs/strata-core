//! S1: Dimension Immutable Tests
//!
//! Invariant S1: Collection dimension cannot change after creation.

use crate::test_utils::*;
use strata_primitives::vector::{DistanceMetric, VectorError};

/// Test that inserting a vector with wrong dimension fails
#[test]
fn test_s1_dimension_mismatch_on_insert() {
    let test_db = TestDb::new();
    let vector = test_db.vector();

    // Create collection with dimension 384
    vector
        .create_collection(test_db.run_id, "embeddings", config_minilm())
        .unwrap();

    // Insert vector with correct dimension should succeed
    vector
        .insert(test_db.run_id, "embeddings", "key1", &random_vector(384), None)
        .unwrap();

    // Insert vector with wrong dimension MUST fail
    let result = vector.insert(test_db.run_id, "embeddings", "key2", &random_vector(768), None);
    assert!(
        matches!(result, Err(VectorError::DimensionMismatch { expected: 384, got: 768 })),
        "S1 VIOLATED: Should reject wrong dimension, got {:?}",
        result
    );
}

/// Test that searching with wrong dimension fails
#[test]
fn test_s1_dimension_enforced_on_search() {
    let test_db = TestDb::new();
    let vector = test_db.vector();

    vector
        .create_collection(test_db.run_id, "embeddings", config_minilm())
        .unwrap();

    vector
        .insert(test_db.run_id, "embeddings", "key1", &random_vector(384), None)
        .unwrap();

    // Search with wrong dimension MUST fail
    let result = vector.search(test_db.run_id, "embeddings", &random_vector(768), 10, None);
    assert!(
        matches!(result, Err(VectorError::DimensionMismatch { expected: 384, got: 768 })),
        "S1 VIOLATED: Should reject wrong search dimension, got {:?}",
        result
    );
}

/// Test that dimension constraint survives restart
#[test]
fn test_s1_dimension_survives_restart() {
    let mut test_db = TestDb::new();
    let run_id = test_db.run_id;

    {
        let vector = test_db.vector();
        vector
            .create_collection(run_id, "embeddings", config_minilm())
            .unwrap();
        vector
            .insert(run_id, "embeddings", "key1", &random_vector(384), None)
            .unwrap();
    }

    // Restart
    test_db.reopen();

    let vector = test_db.vector();

    // Dimension constraint must still be enforced
    let result = vector.insert(run_id, "embeddings", "key2", &random_vector(768), None);
    assert!(
        matches!(result, Err(VectorError::DimensionMismatch { .. })),
        "S1 VIOLATED: Dimension not enforced after restart, got {:?}",
        result
    );

    // Correct dimension should still work
    let result = vector.insert(run_id, "embeddings", "key2", &random_vector(384), None);
    assert!(result.is_ok(), "Correct dimension should work after restart");
}

/// Test dimension with different collection configurations
#[test]
fn test_s1_dimension_various_sizes() {
    let test_db = TestDb::new();
    let vector = test_db.vector();

    // Test with small dimension (3)
    vector
        .create_collection(test_db.run_id, "small", config_small())
        .unwrap();
    vector
        .insert(test_db.run_id, "small", "key1", &vec![1.0, 2.0, 3.0], None)
        .unwrap();

    let result = vector.insert(test_db.run_id, "small", "key2", &vec![1.0, 2.0], None);
    assert!(matches!(result, Err(VectorError::DimensionMismatch { .. })));

    // Test with large dimension (1536)
    vector
        .create_collection(test_db.run_id, "large", config_openai_ada())
        .unwrap();
    vector
        .insert(test_db.run_id, "large", "key1", &random_vector(1536), None)
        .unwrap();

    let result = vector.insert(test_db.run_id, "large", "key2", &random_vector(384), None);
    assert!(matches!(result, Err(VectorError::DimensionMismatch { .. })));
}

/// Test that upsert (overwrite) also enforces dimension
#[test]
fn test_s1_dimension_enforced_on_upsert_overwrite() {
    let test_db = TestDb::new();
    let vector = test_db.vector();

    vector
        .create_collection(test_db.run_id, "embeddings", config_minilm())
        .unwrap();

    // Initial insert
    vector
        .insert(test_db.run_id, "embeddings", "key1", &random_vector(384), None)
        .unwrap();

    // Upsert with wrong dimension should fail
    let result = vector.insert(test_db.run_id, "embeddings", "key1", &random_vector(768), None);
    assert!(
        matches!(result, Err(VectorError::DimensionMismatch { .. })),
        "S1 VIOLATED: Upsert should enforce dimension, got {:?}",
        result
    );
}
