//! R3: Deterministic Order Tests
//!
//! Invariant R3: Same query = same result order (enforced at backend level).

use crate::common::*;

/// Test that same query returns same order
#[test]
fn test_r3_same_query_same_order() {
    let test_db = TestDb::new();
    let vector = test_db.vector();

    vector
        .create_collection(test_db.run_id, "embeddings", config_minilm())
        .unwrap();

    // Insert 100 vectors
    for i in 0..100 {
        vector
            .insert(
                test_db.run_id,
                "embeddings",
                &format!("key_{}", i),
                &seeded_random_vector(384, i as u64),
                None,
            )
            .unwrap();
    }

    let query = seeded_random_vector(384, 12345);

    // Run same search 100 times
    let mut results_list: Vec<Vec<String>> = Vec::new();
    for _ in 0..100 {
        let results = vector
            .search(test_db.run_id, "embeddings", &query, 20, None)
            .unwrap();
        let keys: Vec<String> = results.iter().map(|r| r.key.clone()).collect();
        results_list.push(keys);
    }

    // All results must be identical
    for (i, results) in results_list.iter().enumerate().skip(1) {
        assert_eq!(
            &results_list[0], results,
            "R3 VIOLATED: Search {} returned different order",
            i
        );
    }
}

/// Test deterministic order across restart
#[test]
fn test_r3_deterministic_across_restart() {
    let mut test_db = TestDb::new();
    let run_id = test_db.run_id;
    let query = seeded_random_vector(384, 99999);

    let results_before;
    {
        let vector = test_db.vector();
        vector
            .create_collection(run_id, "embeddings", config_minilm())
            .unwrap();

        for i in 0..50 {
            vector
                .insert(
                    run_id,
                    "embeddings",
                    &format!("key_{}", i),
                    &seeded_random_vector(384, i as u64),
                    None,
                )
                .unwrap();
        }

        results_before = vector
            .search(run_id, "embeddings", &query, 20, None)
            .unwrap();
    }

    test_db.reopen();

    let vector = test_db.vector();
    let results_after = vector.search(run_id, "embeddings", &query, 20, None).unwrap();

    let keys_before: Vec<&str> = results_before.iter().map(|r| r.key.as_str()).collect();
    let keys_after: Vec<&str> = results_after.iter().map(|r| r.key.as_str()).collect();

    assert_eq!(
        keys_before, keys_after,
        "R3 VIOLATED: Order changed across restart"
    );
}

/// Test deterministic order with different k values
#[test]
fn test_r3_deterministic_different_k() {
    let test_db = TestDb::new();
    let vector = test_db.vector();

    vector
        .create_collection(test_db.run_id, "embeddings", config_minilm())
        .unwrap();

    for i in 0..100 {
        vector
            .insert(
                test_db.run_id,
                "embeddings",
                &format!("key_{}", i),
                &seeded_random_vector(384, i as u64),
                None,
            )
            .unwrap();
    }

    let query = seeded_random_vector(384, 54321);

    // Get top 50
    let results_50 = vector
        .search(test_db.run_id, "embeddings", &query, 50, None)
        .unwrap();

    // Get top 20
    let results_20 = vector
        .search(test_db.run_id, "embeddings", &query, 20, None)
        .unwrap();

    // Top 20 from k=50 should match k=20
    let keys_50_top20: Vec<&str> = results_50.iter().take(20).map(|r| r.key.as_str()).collect();
    let keys_20: Vec<&str> = results_20.iter().map(|r| r.key.as_str()).collect();

    assert_eq!(
        keys_50_top20, keys_20,
        "R3 VIOLATED: Different k values give different ordering for overlapping results"
    );
}

/// Test deterministic scores
#[test]
fn test_r3_deterministic_scores() {
    let test_db = TestDb::new();
    let vector = test_db.vector();

    vector
        .create_collection(test_db.run_id, "embeddings", config_minilm())
        .unwrap();

    for i in 0..50 {
        vector
            .insert(
                test_db.run_id,
                "embeddings",
                &format!("key_{}", i),
                &seeded_random_vector(384, i as u64),
                None,
            )
            .unwrap();
    }

    let query = seeded_random_vector(384, 11111);

    // Run search twice
    let results1 = vector
        .search(test_db.run_id, "embeddings", &query, 20, None)
        .unwrap();
    let results2 = vector
        .search(test_db.run_id, "embeddings", &query, 20, None)
        .unwrap();

    // Scores should be identical
    for (r1, r2) in results1.iter().zip(results2.iter()) {
        assert_eq!(r1.key, r2.key, "R3 VIOLATED: Different keys");
        assert!(
            (r1.score - r2.score).abs() < 1e-6,
            "R3 VIOLATED: Score for {} differs: {} vs {}",
            r1.key,
            r1.score,
            r2.score
        );
    }
}

/// Test deterministic order after modifications
#[test]
fn test_r3_deterministic_after_modifications() {
    let test_db = TestDb::new();
    let vector = test_db.vector();

    vector
        .create_collection(test_db.run_id, "embeddings", config_minilm())
        .unwrap();

    // Insert initial vectors
    for i in 0..30 {
        vector
            .insert(
                test_db.run_id,
                "embeddings",
                &format!("key_{}", i),
                &seeded_random_vector(384, i as u64),
                None,
            )
            .unwrap();
    }

    let query = seeded_random_vector(384, 77777);

    // Search before modifications
    let results_before = vector
        .search(test_db.run_id, "embeddings", &query, 10, None)
        .unwrap();

    // Modify unrelated vectors
    for i in 30..40 {
        vector
            .insert(
                test_db.run_id,
                "embeddings",
                &format!("key_{}", i),
                &seeded_random_vector(384, i as u64),
                None,
            )
            .unwrap();
    }

    // Search after modifications (limiting to original keys)
    let results_after = vector
        .search(test_db.run_id, "embeddings", &query, 40, None)
        .unwrap();

    // Find original keys in results_after
    let original_keys: std::collections::HashSet<&str> =
        results_before.iter().map(|r| r.key.as_str()).collect();

    let results_after_filtered: Vec<_> = results_after
        .iter()
        .filter(|r| original_keys.contains(r.key.as_str()))
        .collect();

    // Relative order of original keys should be preserved
    let keys_before: Vec<&str> = results_before.iter().map(|r| r.key.as_str()).collect();
    let keys_after: Vec<&str> = results_after_filtered.iter().map(|r| r.key.as_str()).collect();

    assert_eq!(
        keys_before, keys_after,
        "R3 VIOLATED: Order changed after adding unrelated vectors"
    );
}
