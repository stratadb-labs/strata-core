//! Tier 12: Determinism - Search Result Determinism Tests

use crate::test_utils::*;

#[test]
fn test_same_query_same_results() {
    let test_db = TestDb::new_strict();
    let run_id = test_db.run_id;
    let vector = test_db.vector();

    vector.create_collection(run_id, "embeddings", config_minilm()).unwrap();

    // Insert vectors with seeded random data for reproducibility
    for i in 0..100 {
        vector.insert(run_id, "embeddings", &format!("key_{}", i), &seeded_random_vector(384, i as u64), None).unwrap();
    }

    let query = seeded_random_vector(384, 999);

    // Run same query multiple times
    let results: Vec<_> = (0..10)
        .map(|_| vector.search(run_id, "embeddings", &query, 10, None).unwrap())
        .collect();

    // All results should be identical
    for i in 1..results.len() {
        assert_eq!(results[0].len(), results[i].len());
        for (a, b) in results[0].iter().zip(results[i].iter()) {
            assert_eq!(a.key, b.key);
            assert_eq!(a.score, b.score);
        }
    }
}

#[test]
fn test_determinism_across_reopen() {
    let mut test_db = TestDb::new_strict();
    let run_id = test_db.run_id;

    let query = seeded_random_vector(384, 999);
    let results_before;

    {
        let vector = test_db.vector();
        vector.create_collection(run_id, "embeddings", config_minilm()).unwrap();

        for i in 0..50 {
            vector.insert(run_id, "embeddings", &format!("key_{}", i), &seeded_random_vector(384, i as u64), None).unwrap();
        }

        results_before = vector.search(run_id, "embeddings", &query, 10, None).unwrap();
    }

    test_db.reopen();

    let vector = test_db.vector();
    let results_after = vector.search(run_id, "embeddings", &query, 10, None).unwrap();

    // Results should be identical after reopen
    assert_eq!(results_before.len(), results_after.len());
    for (before, after) in results_before.iter().zip(results_after.iter()) {
        assert_eq!(before.key, after.key);
        assert_eq!(before.score, after.score);
    }
}

#[test]
fn test_determinism_with_different_k_values() {
    let test_db = TestDb::new_strict();
    let run_id = test_db.run_id;
    let vector = test_db.vector();

    vector.create_collection(run_id, "embeddings", config_minilm()).unwrap();

    for i in 0..100 {
        vector.insert(run_id, "embeddings", &format!("key_{}", i), &seeded_random_vector(384, i as u64), None).unwrap();
    }

    let query = seeded_random_vector(384, 999);

    // Get top 20 results
    let results_20 = vector.search(run_id, "embeddings", &query, 20, None).unwrap();

    // Get top 10 results
    let results_10 = vector.search(run_id, "embeddings", &query, 10, None).unwrap();

    // Top 10 of results_20 should equal results_10
    for i in 0..10 {
        assert_eq!(results_20[i].key, results_10[i].key);
        assert_eq!(results_20[i].score, results_10[i].score);
    }
}

#[test]
fn test_determinism_with_tied_scores() {
    let test_db = TestDb::new_strict();
    let run_id = test_db.run_id;
    let vector = test_db.vector();

    vector.create_collection(run_id, "embeddings", config_small()).unwrap();

    // Insert identical vectors with different keys
    let identical_vec = vec![0.5, 0.5, 0.5];
    for i in 0..10 {
        vector.insert(run_id, "embeddings", &format!("key_{:02}", i), &identical_vec, None).unwrap();
    }

    let query = vec![0.5, 0.5, 0.5];

    // Run query multiple times
    let results: Vec<_> = (0..5)
        .map(|_| vector.search(run_id, "embeddings", &query, 10, None).unwrap())
        .collect();

    // All results should have same order (tiebreak by key ascending)
    for i in 1..results.len() {
        for (a, b) in results[0].iter().zip(results[i].iter()) {
            assert_eq!(a.key, b.key);
        }
    }

    // Verify tiebreak order (key ascending)
    for i in 1..results[0].len() {
        assert!(results[0][i - 1].key < results[0][i].key);
    }
}
