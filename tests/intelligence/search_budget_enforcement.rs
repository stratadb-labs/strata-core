//! R7: Coarse-Grained Budget Tests
//!
//! Invariant R7: Budget checked at phase boundaries; brute-force may overshoot.

use crate::common::*;

/// Test that search completes even with many vectors
#[test]
fn test_r7_search_completes_large_collection() {
    let test_db = TestDb::new();
    let vector = test_db.vector();

    vector
        .create_collection(test_db.run_id, "embeddings", config_minilm())
        .unwrap();

    // Insert 1000 vectors (brute force)
    for i in 0..1000 {
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

    // Should complete without budget issues
    let result = vector.search(test_db.run_id, "embeddings", &query, 100, None);
    assert!(result.is_ok(), "Search should complete on large collection");
    assert_eq!(result.unwrap().len(), 100);
}

/// Test search with different k values
#[test]
fn test_r7_search_various_k() {
    let test_db = TestDb::new();
    let vector = test_db.vector();

    vector
        .create_collection(test_db.run_id, "embeddings", config_minilm())
        .unwrap();

    for i in 0..500 {
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

    // Test various k values
    for k in [1, 10, 50, 100, 500] {
        let result = vector.search(test_db.run_id, "embeddings", &query, k, None);
        assert!(result.is_ok(), "Search with k={} should complete", k);
        assert_eq!(
            result.unwrap().len(),
            k,
            "Should return exactly {} results",
            k
        );
    }
}

/// Test search with k > collection size
#[test]
fn test_r7_k_larger_than_collection() {
    let test_db = TestDb::new();
    let vector = test_db.vector();

    vector
        .create_collection(test_db.run_id, "embeddings", config_minilm())
        .unwrap();

    // Insert only 10 vectors
    for i in 0..10 {
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

    // Request k=100 but only 10 exist
    let result = vector
        .search(test_db.run_id, "embeddings", &query, 100, None)
        .unwrap();

    assert_eq!(
        result.len(),
        10,
        "R7: Should return all available vectors when k > count"
    );
}

/// Test search on empty collection
#[test]
fn test_r7_search_empty_collection() {
    let test_db = TestDb::new();
    let vector = test_db.vector();

    vector
        .create_collection(test_db.run_id, "embeddings", config_minilm())
        .unwrap();

    let query = seeded_random_vector(384, 22222);

    let result = vector.search(test_db.run_id, "embeddings", &query, 100, None);
    assert!(result.is_ok(), "Search on empty collection should succeed");
    assert!(result.unwrap().is_empty(), "Empty collection returns no results");
}

/// Test search performance doesn't degrade unreasonably
#[test]
fn test_r7_reasonable_performance() {
    let test_db = TestDb::new();
    let vector = test_db.vector();

    vector
        .create_collection(test_db.run_id, "embeddings", config_minilm())
        .unwrap();

    // Insert 500 vectors
    for i in 0..500 {
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

    let query = seeded_random_vector(384, 33333);

    // Run 10 searches and ensure they complete
    let start = std::time::Instant::now();
    for _ in 0..10 {
        let _ = vector
            .search(test_db.run_id, "embeddings", &query, 50, None)
            .unwrap();
    }
    let elapsed = start.elapsed();

    // 10 searches on 500 vectors should complete in reasonable time
    // Being lenient here as this is just a sanity check
    assert!(
        elapsed.as_secs() < 30,
        "R7: Search took unreasonably long: {:?}",
        elapsed
    );
}
