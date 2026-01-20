//! Tier 11: Crash Mid-Delete Tests
//!
//! Tests for durability of vector delete operations.

use crate::test_utils::*;

#[test]
fn test_committed_delete_persisted() {
    let mut test_db = TestDb::new_strict();
    let run_id = test_db.run_id;

    {
        let vector = test_db.vector();
        vector.create_collection(run_id, "embeddings", config_minilm()).unwrap();

        // Insert then delete
        vector.insert(run_id, "embeddings", "key1", &random_vector(384), None).unwrap();
        vector.delete(run_id, "embeddings", "key1").unwrap();
    }

    test_db.reopen();

    let vector = test_db.vector();
    // Delete was committed, so vector should not exist
    assert!(vector.get(run_id, "embeddings", "key1").unwrap().is_none());
    assert_eq!(vector.count(run_id, "embeddings").unwrap(), 0);
}

#[test]
fn test_batch_delete_all_persisted() {
    let mut test_db = TestDb::new_strict();
    let run_id = test_db.run_id;

    {
        let vector = test_db.vector();
        vector.create_collection(run_id, "embeddings", config_minilm()).unwrap();

        // Insert all vectors
        for i in 0..20 {
            vector.insert(run_id, "embeddings", &format!("key_{}", i), &seeded_random_vector(384, i as u64), None).unwrap();
        }

        // Delete first 10
        for i in 0..10 {
            vector.delete(run_id, "embeddings", &format!("key_{}", i)).unwrap();
        }
    }

    test_db.reopen();

    let vector = test_db.vector();

    // First 10 should be deleted
    for i in 0..10 {
        assert!(vector.get(run_id, "embeddings", &format!("key_{}", i)).unwrap().is_none());
    }

    // Next 10 should exist
    for i in 10..20 {
        assert!(vector.get(run_id, "embeddings", &format!("key_{}", i)).unwrap().is_some());
    }

    assert_eq!(vector.count(run_id, "embeddings").unwrap(), 10);
}

#[test]
fn test_delete_reinsert_preserves_new() {
    let mut test_db = TestDb::new_strict();
    let run_id = test_db.run_id;

    let new_vector = seeded_random_vector(384, 99);

    {
        let vector = test_db.vector();
        vector.create_collection(run_id, "embeddings", config_minilm()).unwrap();

        // Insert, delete, reinsert
        vector.insert(run_id, "embeddings", "key1", &seeded_random_vector(384, 1), None).unwrap();
        vector.delete(run_id, "embeddings", "key1").unwrap();
        vector.insert(run_id, "embeddings", "key1", &new_vector, None).unwrap();
    }

    test_db.reopen();

    let vector = test_db.vector();
    let result = vector.get(run_id, "embeddings", "key1").unwrap().unwrap();

    // Should have the reinserted vector
    assert_eq!(result.value.embedding, new_vector);
}

#[test]
fn test_partial_delete_preserved() {
    let mut test_db = TestDb::new_strict();
    let run_id = test_db.run_id;

    {
        let vector = test_db.vector();
        vector.create_collection(run_id, "embeddings", config_minilm()).unwrap();

        for i in 0..100 {
            vector.insert(run_id, "embeddings", &format!("key_{}", i), &seeded_random_vector(384, i as u64), None).unwrap();
        }

        // Delete only even keys
        for i in (0..100).step_by(2) {
            vector.delete(run_id, "embeddings", &format!("key_{}", i)).unwrap();
        }
    }

    test_db.reopen();

    let vector = test_db.vector();
    assert_eq!(vector.count(run_id, "embeddings").unwrap(), 50);

    // Verify odd keys exist, even keys don't
    for i in 0..100 {
        let exists = vector.get(run_id, "embeddings", &format!("key_{}", i)).unwrap().is_some();
        assert_eq!(exists, i % 2 == 1, "Key {} existence mismatch", i);
    }
}
