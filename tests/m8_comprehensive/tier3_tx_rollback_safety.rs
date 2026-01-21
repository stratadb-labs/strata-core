//! T3: Rollback Safety Tests
//!
//! Invariant T3: Failed transactions leave no partial state.

use crate::test_utils::*;
use strata_primitives::vector::VectorError;

/// Test that failed insert leaves no partial state
#[test]
fn test_t3_failed_insert_no_partial_state() {
    let test_db = TestDb::new();
    let vector = test_db.vector();

    vector
        .create_collection(test_db.run_id, "embeddings", config_minilm())
        .unwrap();

    let count_before = vector.count(test_db.run_id, "embeddings").unwrap();

    // Try to insert with wrong dimension (should fail)
    let result = vector.insert(
        test_db.run_id,
        "embeddings",
        "bad_key",
        &random_vector(768), // Wrong dimension
        None,
    );
    assert!(result.is_err());

    let count_after = vector.count(test_db.run_id, "embeddings").unwrap();

    assert_eq!(
        count_before, count_after,
        "T3 VIOLATED: Failed insert left partial state"
    );

    // The key should not exist
    assert!(
        vector.get(test_db.run_id, "embeddings", "bad_key").unwrap().is_none(),
        "T3 VIOLATED: Failed insert created partial entry"
    );
}

/// Test that failed collection operation leaves no partial state
#[test]
fn test_t3_failed_collection_op_no_partial_state() {
    let test_db = TestDb::new();
    let vector = test_db.vector();

    // Create first collection
    vector
        .create_collection(test_db.run_id, "embeddings", config_minilm())
        .unwrap();

    // Try to create same collection again (should fail)
    let result = vector.create_collection(test_db.run_id, "embeddings", config_minilm());
    assert!(matches!(result, Err(VectorError::CollectionAlreadyExists { .. })));

    // Original collection should still work
    vector
        .insert(test_db.run_id, "embeddings", "key1", &random_vector(384), None)
        .unwrap();

    assert!(vector.get(test_db.run_id, "embeddings", "key1").unwrap().is_some());
}

/// Test that successful operations don't create ghost entries on subsequent failure
#[test]
fn test_t3_no_ghost_entries() {
    let test_db = TestDb::new();
    let vector = test_db.vector();

    vector
        .create_collection(test_db.run_id, "embeddings", config_minilm())
        .unwrap();

    // Successfully insert some vectors
    for i in 0..10 {
        vector
            .insert(
                test_db.run_id,
                "embeddings",
                &format!("good_key_{}", i),
                &random_vector(384),
                None,
            )
            .unwrap();
    }

    // Try failed insert
    let _ = vector.insert(
        test_db.run_id,
        "embeddings",
        "bad_key",
        &random_vector(768),
        None,
    );

    // Original vectors should still be intact
    let count = vector.count(test_db.run_id, "embeddings").unwrap();
    assert_eq!(count, 10, "T3 VIOLATED: Failed operation affected existing data");

    // All original keys should exist
    for i in 0..10 {
        assert!(
            vector
                .get(test_db.run_id, "embeddings", &format!("good_key_{}", i))
                .unwrap()
                .is_some(),
            "T3 VIOLATED: Original key {} missing after failed operation",
            i
        );
    }
}

/// Test delete on non-existent key doesn't corrupt state
#[test]
fn test_t3_delete_nonexistent_safe() {
    let test_db = TestDb::new();
    let vector = test_db.vector();

    vector
        .create_collection(test_db.run_id, "embeddings", config_minilm())
        .unwrap();

    // Insert some vectors
    for i in 0..5 {
        vector
            .insert(
                test_db.run_id,
                "embeddings",
                &format!("key_{}", i),
                &random_vector(384),
                None,
            )
            .unwrap();
    }

    let count_before = vector.count(test_db.run_id, "embeddings").unwrap();

    // Try to delete non-existent key
    let deleted = vector.delete(test_db.run_id, "embeddings", "nonexistent").unwrap();
    assert!(!deleted, "Delete of nonexistent key should return false");

    let count_after = vector.count(test_db.run_id, "embeddings").unwrap();

    assert_eq!(
        count_before, count_after,
        "T3 VIOLATED: Delete of nonexistent key affected count"
    );
}

/// Test operation on non-existent collection is safe
#[test]
fn test_t3_op_on_nonexistent_collection_safe() {
    let test_db = TestDb::new();
    let vector = test_db.vector();

    // Try to insert into non-existent collection
    let result = vector.insert(
        test_db.run_id,
        "nonexistent",
        "key1",
        &random_vector(384),
        None,
    );
    assert!(
        result.is_err(),
        "Insert to non-existent collection should fail"
    );

    // Database should still be healthy
    vector
        .create_collection(test_db.run_id, "real_collection", config_minilm())
        .unwrap();
    vector
        .insert(
            test_db.run_id,
            "real_collection",
            "key1",
            &random_vector(384),
            None,
        )
        .unwrap();
    assert!(
        vector
            .get(test_db.run_id, "real_collection", "key1")
            .unwrap()
            .is_some()
    );
}

/// Test state consistency after multiple failures
#[test]
fn test_t3_state_consistent_after_failures() {
    let test_db = TestDb::new();
    let vector = test_db.vector();

    vector
        .create_collection(test_db.run_id, "embeddings", config_minilm())
        .unwrap();

    // Insert good data
    for i in 0..20 {
        vector
            .insert(
                test_db.run_id,
                "embeddings",
                &format!("key_{}", i),
                &random_vector(384),
                None,
            )
            .unwrap();
    }

    // Generate multiple failures
    for _ in 0..50 {
        // Wrong dimension
        let _ = vector.insert(
            test_db.run_id,
            "embeddings",
            "bad",
            &random_vector(768),
            None,
        );
        // Non-existent collection
        let _ = vector.insert(
            test_db.run_id,
            "nonexistent",
            "key",
            &random_vector(384),
            None,
        );
        // Delete non-existent
        let _ = vector.delete(test_db.run_id, "embeddings", "nonexistent");
    }

    // State should be exactly what we put there
    let count = vector.count(test_db.run_id, "embeddings").unwrap();
    assert_eq!(count, 20, "T3 VIOLATED: State corrupted after failures");

    for i in 0..20 {
        assert!(
            vector
                .get(test_db.run_id, "embeddings", &format!("key_{}", i))
                .unwrap()
                .is_some(),
            "T3 VIOLATED: Key {} missing after failures",
            i
        );
    }
}
