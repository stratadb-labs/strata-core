//! Tier 14: Non-Regression - Known Bug Tests
//!
//! Tests for previously discovered bugs to prevent regression.

use crate::test_utils::*;

#[test]
fn test_vectorid_not_reused_after_delete_and_recovery() {
    // Regression test: VectorIds were being reused after recovery
    let mut test_db = TestDb::new_strict();
    let run_id = test_db.run_id;

    let max_id_before;
    {
        let vector = test_db.vector();
        vector.create_collection(run_id, "embeddings", config_minilm()).unwrap();

        // Insert vectors
        for i in 0..20 {
            vector.insert(run_id, "embeddings", &format!("key_{}", i), &seeded_random_vector(384, i as u64), None).unwrap();
        }

        max_id_before = (0..20)
            .map(|i| vector.get(run_id, "embeddings", &format!("key_{}", i)).unwrap().unwrap().value.vector_id().as_u64())
            .max()
            .unwrap();

        // Delete all vectors
        for i in 0..20 {
            vector.delete(run_id, "embeddings", &format!("key_{}", i)).unwrap();
        }
    }

    test_db.reopen();

    let vector = test_db.vector();

    // Insert new vectors - IDs should be higher than before
    for i in 0..10 {
        vector.insert(run_id, "embeddings", &format!("new_key_{}", i), &seeded_random_vector(384, i as u64 + 100), None).unwrap();
        let new_id = vector.get(run_id, "embeddings", &format!("new_key_{}", i)).unwrap().unwrap().value.vector_id().as_u64();
        assert!(new_id > max_id_before, "VectorId {} should be > {} (no reuse after recovery)", new_id, max_id_before);
    }
}

#[test]
fn test_collection_config_preserved_after_recovery() {
    // Regression test: Collection config was not being preserved correctly
    let mut test_db = TestDb::new_strict();
    let run_id = test_db.run_id;

    {
        let vector = test_db.vector();
        vector.create_collection(run_id, "cosine_col", config_minilm()).unwrap();
        vector.create_collection(run_id, "euclidean_col", config_euclidean()).unwrap();

        vector.insert(run_id, "cosine_col", "key1", &random_vector(384), None).unwrap();
        vector.insert(run_id, "euclidean_col", "key1", &random_vector(384), None).unwrap();
    }

    test_db.reopen();

    let vector = test_db.vector();

    let cosine_info = vector.get_collection(run_id, "cosine_col").unwrap().unwrap();
    let euclidean_info = vector.get_collection(run_id, "euclidean_col").unwrap().unwrap();

    assert_eq!(cosine_info.value.config.dimension, 384);
    assert_eq!(euclidean_info.value.config.dimension, 384);
    // Verify metric type is preserved (depending on how CollectionInfo exposes this)
}

#[test]
fn test_empty_collection_search_does_not_panic() {
    // Regression test: Search on empty collection caused panic
    let test_db = TestDb::new_strict();
    let run_id = test_db.run_id;
    let vector = test_db.vector();

    vector.create_collection(run_id, "embeddings", config_minilm()).unwrap();

    // Search on empty collection should return empty, not panic
    let results = vector.search(run_id, "embeddings", &random_vector(384), 10, None).unwrap();
    assert!(results.is_empty());
}

#[test]
fn test_delete_nonexistent_key_is_no_op() {
    // Regression test: Deleting non-existent key caused errors
    let test_db = TestDb::new_strict();
    let run_id = test_db.run_id;
    let vector = test_db.vector();

    vector.create_collection(run_id, "embeddings", config_minilm()).unwrap();
    vector.insert(run_id, "embeddings", "existing_key", &random_vector(384), None).unwrap();

    // Delete non-existent key should be a no-op (or return Ok with no effect)
    let result = vector.delete(run_id, "embeddings", "nonexistent_key");
    assert!(result.is_ok(), "Delete of non-existent key should not error");

    // Existing key should still be there
    assert!(vector.get(run_id, "embeddings", "existing_key").unwrap().is_some());
    assert_eq!(vector.count(run_id, "embeddings").unwrap(), 1);
}

#[test]
fn test_upsert_same_key_updates_not_duplicates() {
    // Regression test: Upsert was creating duplicates instead of updating
    let test_db = TestDb::new_strict();
    let run_id = test_db.run_id;
    let vector = test_db.vector();

    vector.create_collection(run_id, "embeddings", config_minilm()).unwrap();

    // Multiple upserts to same key
    for i in 0..10 {
        vector.insert(run_id, "embeddings", "same_key", &seeded_random_vector(384, i as u64), None).unwrap();
    }

    // Should only have one vector
    assert_eq!(vector.count(run_id, "embeddings").unwrap(), 1);

    // Search should only return one result for this key
    let query = seeded_random_vector(384, 9); // Last vector inserted
    let results = vector.search(run_id, "embeddings", &query, 10, None).unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].key, "same_key");
}

#[test]
fn test_search_k_greater_than_count() {
    // Regression test: Requesting more results than vectors caused issues
    let test_db = TestDb::new_strict();
    let run_id = test_db.run_id;
    let vector = test_db.vector();

    vector.create_collection(run_id, "embeddings", config_minilm()).unwrap();

    // Insert only 5 vectors
    for i in 0..5 {
        vector.insert(run_id, "embeddings", &format!("key_{}", i), &seeded_random_vector(384, i as u64), None).unwrap();
    }

    // Request 100 results
    let results = vector.search(run_id, "embeddings", &random_vector(384), 100, None).unwrap();

    // Should return only 5
    assert_eq!(results.len(), 5);
}
