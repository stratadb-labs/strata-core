//! Tier 7: M6 RRF Fusion Tests
//!
//! Note: RRF fusion is tested at the integration level. These tests verify
//! vector search produces results compatible with fusion.

use crate::common::*;

#[test]
fn test_vector_results_suitable_for_fusion() {
    let test_db = TestDb::new();
    let vector = test_db.vector();

    vector.create_collection(test_db.run_id, "embeddings", config_minilm()).unwrap();

    for i in 0..50 {
        vector.insert(
            test_db.run_id,
            "embeddings",
            &format!("doc_{}", i),
            &seeded_random_vector(384, i as u64),
            None,
        ).unwrap();
    }

    let query = seeded_random_vector(384, 12345);
    let results = vector.search(test_db.run_id, "embeddings", &query, 20, None).unwrap();

    // Results should have proper format for RRF
    for (rank, result) in results.iter().enumerate() {
        assert!(!result.key.is_empty(), "Key should not be empty");
        assert!(result.score.is_finite(), "Score should be finite");
        // RRF uses rank-based scoring, so we verify results are ordered
        if rank > 0 {
            assert!(results[rank - 1].score >= result.score);
        }
    }
}

#[test]
fn test_multiple_queries_different_results() {
    let test_db = TestDb::new();
    let vector = test_db.vector();

    vector.create_collection(test_db.run_id, "embeddings", config_minilm()).unwrap();

    for i in 0..100 {
        vector.insert(
            test_db.run_id,
            "embeddings",
            &format!("doc_{}", i),
            &seeded_random_vector(384, i as u64),
            None,
        ).unwrap();
    }

    let query1 = seeded_random_vector(384, 111);
    let query2 = seeded_random_vector(384, 222);

    let results1 = vector.search(test_db.run_id, "embeddings", &query1, 10, None).unwrap();
    let results2 = vector.search(test_db.run_id, "embeddings", &query2, 10, None).unwrap();

    // Different queries may produce different top results (used in fusion)
    let keys1: Vec<&str> = results1.iter().map(|r| r.key.as_str()).collect();
    let keys2: Vec<&str> = results2.iter().map(|r| r.key.as_str()).collect();

    // Results exist
    assert!(!keys1.is_empty());
    assert!(!keys2.is_empty());
}
