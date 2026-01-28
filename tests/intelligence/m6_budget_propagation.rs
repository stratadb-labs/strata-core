//! Tier 7: M6 Budget Propagation Tests

use crate::common::*;

#[test]
fn test_search_completes_within_reasonable_time() {
    let test_db = TestDb::new();
    let vector = test_db.vector();

    vector.create_collection(test_db.run_id, "embeddings", config_minilm()).unwrap();

    for i in 0..500 {
        vector.insert(test_db.run_id, "embeddings", &format!("key_{}", i), &seeded_random_vector(384, i as u64), None).unwrap();
    }

    let query = seeded_random_vector(384, 999);
    let start = std::time::Instant::now();
    let _ = vector.search(test_db.run_id, "embeddings", &query, 100, None).unwrap();
    let elapsed = start.elapsed();

    // Should complete in reasonable time (brute force on 500 vectors)
    assert!(elapsed.as_secs() < 10, "Search took too long: {:?}", elapsed);
}

#[test]
fn test_search_with_various_k_values() {
    let test_db = TestDb::new();
    let vector = test_db.vector();

    vector.create_collection(test_db.run_id, "embeddings", config_minilm()).unwrap();

    for i in 0..100 {
        vector.insert(test_db.run_id, "embeddings", &format!("key_{}", i), &seeded_random_vector(384, i as u64), None).unwrap();
    }

    let query = seeded_random_vector(384, 999);

    // Test with different k values
    for k in [1, 10, 50, 100] {
        let results = vector.search(test_db.run_id, "embeddings", &query, k, None).unwrap();
        assert_eq!(results.len(), k);
    }
}

#[test]
fn test_k_larger_than_collection() {
    let test_db = TestDb::new();
    let vector = test_db.vector();

    vector.create_collection(test_db.run_id, "embeddings", config_minilm()).unwrap();

    for i in 0..10 {
        vector.insert(test_db.run_id, "embeddings", &format!("key_{}", i), &seeded_random_vector(384, i as u64), None).unwrap();
    }

    let query = seeded_random_vector(384, 999);
    let results = vector.search(test_db.run_id, "embeddings", &query, 100, None).unwrap();

    // Should return all available vectors
    assert_eq!(results.len(), 10);
}
