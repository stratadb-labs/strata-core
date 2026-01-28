//! Tier 13: Stress - Large Collections Tests

use crate::common::*;

#[test]
fn test_large_collection_insert() {
    let test_db = TestDb::new_strict();
    let run_id = test_db.run_id;
    let vector = test_db.vector();

    vector.create_collection(run_id, "embeddings", config_minilm()).unwrap();

    // Insert 1000 vectors
    for i in 0..1000 {
        vector.insert(run_id, "embeddings", &format!("key_{}", i), &seeded_random_vector(384, i as u64), None).unwrap();
    }

    assert_eq!(vector.count(run_id, "embeddings").unwrap(), 1000);
}

#[test]
fn test_large_collection_search() {
    let test_db = TestDb::new_strict();
    let run_id = test_db.run_id;
    let vector = test_db.vector();

    vector.create_collection(run_id, "embeddings", config_minilm()).unwrap();

    // Insert 1000 vectors
    for i in 0..1000 {
        vector.insert(run_id, "embeddings", &format!("key_{}", i), &seeded_random_vector(384, i as u64), None).unwrap();
    }

    // Search should work
    let query = seeded_random_vector(384, 999);
    let results = vector.search(run_id, "embeddings", &query, 50, None).unwrap();

    assert_eq!(results.len(), 50);

    // Results should be sorted by score descending
    for i in 1..results.len() {
        assert!(results[i - 1].score >= results[i].score);
    }
}

#[test]
fn test_large_collection_recovery() {
    let mut test_db = TestDb::new_strict();
    let run_id = test_db.run_id;

    {
        let vector = test_db.vector();
        vector.create_collection(run_id, "embeddings", config_minilm()).unwrap();

        for i in 0..500 {
            vector.insert(run_id, "embeddings", &format!("key_{}", i), &seeded_random_vector(384, i as u64), None).unwrap();
        }
    }

    test_db.reopen();

    let vector = test_db.vector();
    assert_eq!(vector.count(run_id, "embeddings").unwrap(), 500);

    // Verify some vectors
    for i in [0, 100, 200, 300, 400].iter() {
        let result = vector.get(run_id, "embeddings", &format!("key_{}", i)).unwrap();
        assert!(result.is_some(), "key_{} should exist after recovery", i);
    }
}

#[test]
fn test_large_collection_delete_many() {
    let test_db = TestDb::new_strict();
    let run_id = test_db.run_id;
    let vector = test_db.vector();

    vector.create_collection(run_id, "embeddings", config_minilm()).unwrap();

    // Insert 500 vectors
    for i in 0..500 {
        vector.insert(run_id, "embeddings", &format!("key_{}", i), &seeded_random_vector(384, i as u64), None).unwrap();
    }

    // Delete half
    for i in 0..250 {
        vector.delete(run_id, "embeddings", &format!("key_{}", i)).unwrap();
    }

    assert_eq!(vector.count(run_id, "embeddings").unwrap(), 250);

    // Deleted keys should not exist
    for i in 0..250 {
        assert!(vector.get(run_id, "embeddings", &format!("key_{}", i)).unwrap().is_none());
    }

    // Remaining keys should exist
    for i in 250..500 {
        assert!(vector.get(run_id, "embeddings", &format!("key_{}", i)).unwrap().is_some());
    }
}
