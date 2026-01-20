//! Tier 11: Crash Mid-Upsert Tests
//!
//! Tests for durability of vector upsert operations.

use crate::test_utils::*;

#[test]
fn test_committed_insert_visible_after_reopen() {
    let mut test_db = TestDb::new_strict();
    let run_id = test_db.run_id;

    {
        let vector = test_db.vector();
        vector.create_collection(run_id, "embeddings", config_minilm()).unwrap();

        // Committed insert
        vector.insert(run_id, "embeddings", "committed_key", &random_vector(384), None).unwrap();
    }

    test_db.reopen();

    let vector = test_db.vector();
    assert!(vector.get(run_id, "embeddings", "committed_key").unwrap().is_some());
    assert_eq!(vector.count(run_id, "embeddings").unwrap(), 1);
}

#[test]
fn test_batch_insert_all_visible() {
    let mut test_db = TestDb::new_strict();
    let run_id = test_db.run_id;

    {
        let vector = test_db.vector();
        vector.create_collection(run_id, "embeddings", config_minilm()).unwrap();

        // Batch insert
        for i in 0..10 {
            vector.insert(run_id, "embeddings", &format!("key_{}", i), &seeded_random_vector(384, i as u64), None).unwrap();
        }
    }

    test_db.reopen();

    let vector = test_db.vector();

    // All should exist
    for i in 0..10 {
        assert!(vector.get(run_id, "embeddings", &format!("key_{}", i)).unwrap().is_some());
    }

    assert_eq!(vector.count(run_id, "embeddings").unwrap(), 10);
}

#[test]
fn test_insert_update_preserves_latest() {
    let mut test_db = TestDb::new_strict();
    let run_id = test_db.run_id;

    let updated_vector = seeded_random_vector(384, 99);

    {
        let vector = test_db.vector();
        vector.create_collection(run_id, "embeddings", config_minilm()).unwrap();

        // Original insert
        vector.insert(run_id, "embeddings", "key1", &seeded_random_vector(384, 1), None).unwrap();

        // Update
        vector.insert(run_id, "embeddings", "key1", &updated_vector, None).unwrap();
    }

    test_db.reopen();

    let vector = test_db.vector();
    let result = vector.get(run_id, "embeddings", "key1").unwrap().unwrap();

    // Should have updated vector
    assert_eq!(result.value.embedding, updated_vector);
}

#[test]
fn test_multiple_reopens_preserve_data() {
    let mut test_db = TestDb::new_strict();
    let run_id = test_db.run_id;

    {
        let vector = test_db.vector();
        vector.create_collection(run_id, "embeddings", config_minilm()).unwrap();

        for i in 0..20 {
            vector.insert(run_id, "embeddings", &format!("key_{}", i), &seeded_random_vector(384, i as u64), None).unwrap();
        }
    }

    // Multiple reopens
    for _ in 0..3 {
        test_db.reopen();

        let vector = test_db.vector();
        assert_eq!(vector.count(run_id, "embeddings").unwrap(), 20);
    }
}
