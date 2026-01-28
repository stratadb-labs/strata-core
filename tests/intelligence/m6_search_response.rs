//! Tier 7: M6 SearchResponse Compatibility Tests

use crate::common::*;

#[test]
fn test_m6_response_format() {
    let test_db = TestDb::new();
    let vector = test_db.vector();

    vector.create_collection(test_db.run_id, "embeddings", config_minilm()).unwrap();

    for i in 0..20 {
        vector.insert(test_db.run_id, "embeddings", &format!("key_{}", i), &seeded_random_vector(384, i as u64), None).unwrap();
    }

    let query = seeded_random_vector(384, 999);
    let results = vector.search(test_db.run_id, "embeddings", &query, 10, None).unwrap();

    // Results should have key and score
    for result in &results {
        assert!(!result.key.is_empty());
        assert!(result.score.is_finite());
    }
}

#[test]
fn test_m6_response_ordering() {
    let test_db = TestDb::new();
    let vector = test_db.vector();

    vector.create_collection(test_db.run_id, "embeddings", config_minilm()).unwrap();

    for i in 0..20 {
        vector.insert(test_db.run_id, "embeddings", &format!("key_{}", i), &seeded_random_vector(384, i as u64), None).unwrap();
    }

    let query = seeded_random_vector(384, 999);
    let results = vector.search(test_db.run_id, "embeddings", &query, 10, None).unwrap();

    // Results should be ordered by score (descending)
    for i in 1..results.len() {
        assert!(results[i - 1].score >= results[i].score, "Results not in descending score order");
    }
}

#[test]
fn test_m6_response_k_limit() {
    let test_db = TestDb::new();
    let vector = test_db.vector();

    vector.create_collection(test_db.run_id, "embeddings", config_minilm()).unwrap();

    for i in 0..100 {
        vector.insert(test_db.run_id, "embeddings", &format!("key_{}", i), &seeded_random_vector(384, i as u64), None).unwrap();
    }

    let query = seeded_random_vector(384, 999);

    for k in [1, 5, 10, 50, 100] {
        let results = vector.search(test_db.run_id, "embeddings", &query, k, None).unwrap();
        assert_eq!(results.len(), k);
    }
}
