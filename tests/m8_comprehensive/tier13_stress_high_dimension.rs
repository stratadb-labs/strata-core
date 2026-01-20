//! Tier 13: Stress - High Dimension Tests

use crate::test_utils::*;

#[test]
fn test_high_dimension_vectors() {
    let test_db = TestDb::new_strict();
    let run_id = test_db.run_id;
    let vector = test_db.vector();

    // 1536 dimensions (OpenAI ada-002)
    vector.create_collection(run_id, "embeddings", config_openai_ada()).unwrap();

    for i in 0..50 {
        vector.insert(run_id, "embeddings", &format!("key_{}", i), &seeded_random_vector(1536, i as u64), None).unwrap();
    }

    assert_eq!(vector.count(run_id, "embeddings").unwrap(), 50);

    // Search should work
    let query = seeded_random_vector(1536, 999);
    let results = vector.search(run_id, "embeddings", &query, 10, None).unwrap();
    assert_eq!(results.len(), 10);
}

#[test]
fn test_high_dimension_recovery() {
    let mut test_db = TestDb::new_strict();
    let run_id = test_db.run_id;

    {
        let vector = test_db.vector();
        vector.create_collection(run_id, "embeddings", config_openai_ada()).unwrap();

        for i in 0..30 {
            vector.insert(run_id, "embeddings", &format!("key_{}", i), &seeded_random_vector(1536, i as u64), None).unwrap();
        }
    }

    test_db.reopen();

    let vector = test_db.vector();
    assert_eq!(vector.count(run_id, "embeddings").unwrap(), 30);

    // Verify vector data
    for i in [0, 10, 20, 29].iter() {
        let result = vector.get(run_id, "embeddings", &format!("key_{}", i)).unwrap().unwrap();
        assert_eq!(result.value.embedding.len(), 1536);
        assert_eq!(result.value.embedding, seeded_random_vector(1536, *i as u64));
    }
}

#[test]
fn test_high_dimension_search_accuracy() {
    let test_db = TestDb::new_strict();
    let run_id = test_db.run_id;
    let vector = test_db.vector();

    vector.create_collection(run_id, "embeddings", config_openai_ada()).unwrap();

    // Insert vectors
    for i in 0..20 {
        vector.insert(run_id, "embeddings", &format!("key_{}", i), &seeded_random_vector(1536, i as u64), None).unwrap();
    }

    // Search for a known vector - it should be the top result
    let query = seeded_random_vector(1536, 5);
    let results = vector.search(run_id, "embeddings", &query, 5, None).unwrap();

    assert_eq!(results[0].key, "key_5");
    assert!((results[0].score - 1.0).abs() < 1e-6, "Exact match should have score ~1.0 for cosine");
}

#[test]
fn test_mixed_dimensions_multiple_collections() {
    let test_db = TestDb::new_strict();
    let run_id = test_db.run_id;
    let vector = test_db.vector();

    // Create collections with different dimensions
    vector.create_collection(run_id, "small", config_small()).unwrap();        // 3 dims
    vector.create_collection(run_id, "medium", config_minilm()).unwrap();      // 384 dims
    vector.create_collection(run_id, "large", config_openai_ada()).unwrap();   // 1536 dims

    // Insert data into each
    for i in 0..20 {
        vector.insert(run_id, "small", &format!("key_{}", i), &seeded_random_vector(3, i as u64), None).unwrap();
        vector.insert(run_id, "medium", &format!("key_{}", i), &seeded_random_vector(384, i as u64), None).unwrap();
        vector.insert(run_id, "large", &format!("key_{}", i), &seeded_random_vector(1536, i as u64), None).unwrap();
    }

    // Search each
    let results_small = vector.search(run_id, "small", &seeded_random_vector(3, 999), 5, None).unwrap();
    let results_medium = vector.search(run_id, "medium", &seeded_random_vector(384, 999), 5, None).unwrap();
    let results_large = vector.search(run_id, "large", &seeded_random_vector(1536, 999), 5, None).unwrap();

    assert_eq!(results_small.len(), 5);
    assert_eq!(results_medium.len(), 5);
    assert_eq!(results_large.len(), 5);
}
