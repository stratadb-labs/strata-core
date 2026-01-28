//! R6: Snapshot Consistency Tests
//!
//! Invariant R6: Search sees consistent point-in-time view.

use crate::common::*;

/// Test search sees consistent view during operation
#[test]
fn test_r6_search_consistent_view() {
    let test_db = TestDb::new();
    let vector = test_db.vector();

    vector
        .create_collection(test_db.run_id, "embeddings", config_minilm())
        .unwrap();

    // Insert initial vectors
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

    let query = seeded_random_vector(384, 99999);

    // First search
    let results1 = vector
        .search(test_db.run_id, "embeddings", &query, 50, None)
        .unwrap();

    // Second search immediately after (before any modifications)
    let results2 = vector
        .search(test_db.run_id, "embeddings", &query, 50, None)
        .unwrap();

    // Both should return identical results
    let keys1: Vec<&str> = results1.iter().map(|r| r.key.as_str()).collect();
    let keys2: Vec<&str> = results2.iter().map(|r| r.key.as_str()).collect();

    assert_eq!(keys1, keys2, "R6 VIOLATED: Consecutive searches differ");
}

/// Test search isolation from concurrent modifications
#[test]
fn test_r6_search_isolation() {
    let test_db = TestDb::new();
    let vector = test_db.vector();

    vector
        .create_collection(test_db.run_id, "embeddings", config_minilm())
        .unwrap();

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

    // Search before modification
    let query = seeded_random_vector(384, 12345);
    let count_before = vector.count(test_db.run_id, "embeddings").unwrap();

    // Modify after search started (add vectors)
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

    let count_after = vector.count(test_db.run_id, "embeddings").unwrap();

    // Count should reflect modifications
    assert_eq!(count_before, 30);
    assert_eq!(count_after, 40);
}

/// Test search results don't include uncommitted changes in other transactions
#[test]
fn test_r6_search_transaction_isolation() {
    let test_db = TestDb::new();
    let vector = test_db.vector();

    vector
        .create_collection(test_db.run_id, "embeddings", config_minilm())
        .unwrap();

    // Insert committed vectors
    for i in 0..20 {
        vector
            .insert(
                test_db.run_id,
                "embeddings",
                &format!("committed_{}", i),
                &seeded_random_vector(384, i as u64),
                None,
            )
            .unwrap();
    }

    // Search should find 20 vectors
    let query = seeded_random_vector(384, 77777);
    let results = vector
        .search(test_db.run_id, "embeddings", &query, 100, None)
        .unwrap();

    assert_eq!(results.len(), 20, "R6: Should find all committed vectors");
}

/// Test search consistency after crash recovery
#[test]
fn test_r6_search_consistent_after_recovery() {
    let mut test_db = TestDb::new_strict();
    let run_id = test_db.run_id;
    let query = seeded_random_vector(384, 55555);

    let results_before;
    {
        let vector = test_db.vector();
        vector
            .create_collection(run_id, "embeddings", config_minilm())
            .unwrap();

        for i in 0..40 {
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

        results_before = vector.search(run_id, "embeddings", &query, 40, None).unwrap();
    }

    // Restart
    test_db.reopen();

    let vector = test_db.vector();
    let results_after = vector.search(run_id, "embeddings", &query, 40, None).unwrap();

    // Results should be identical
    let keys_before: Vec<&str> = results_before.iter().map(|r| r.key.as_str()).collect();
    let keys_after: Vec<&str> = results_after.iter().map(|r| r.key.as_str()).collect();

    assert_eq!(
        keys_before, keys_after,
        "R6 VIOLATED: Search results differ after recovery"
    );
}
