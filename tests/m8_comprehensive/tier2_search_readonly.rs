//! R10: Search is Read-Only Tests
//!
//! Invariant R10: Search must not write anything: no counters, no caches, no side effects.

use crate::test_utils::*;

/// Test that search does not modify collection state
#[test]
fn test_r10_search_does_not_modify_state() {
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

    // Capture state before searches
    let state_before = CapturedVectorState::capture(&vector, test_db.run_id, "embeddings");
    let count_before = vector.count(test_db.run_id, "embeddings").unwrap();

    // Many searches
    let query = seeded_random_vector(384, 99999);
    for _ in 0..100 {
        let _ = vector.search(test_db.run_id, "embeddings", &query, 10, None);
    }

    // State should be identical
    let state_after = CapturedVectorState::capture(&vector, test_db.run_id, "embeddings");
    let count_after = vector.count(test_db.run_id, "embeddings").unwrap();

    assert_eq!(
        count_before, count_after,
        "R10 VIOLATED: Search modified count"
    );
    assert_vector_states_equal(&state_before, &state_after, "R10 VIOLATED: Search modified state");
}

/// Test that search does not create new vectors
#[test]
fn test_r10_search_does_not_create_vectors() {
    let test_db = TestDb::new();
    let vector = test_db.vector();

    vector
        .create_collection(test_db.run_id, "embeddings", config_minilm())
        .unwrap();

    for i in 0..20 {
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

    let count_before = vector.count(test_db.run_id, "embeddings").unwrap();

    // Search with many different queries
    for i in 0..100 {
        let query = seeded_random_vector(384, i as u64 + 10000);
        let _ = vector.search(test_db.run_id, "embeddings", &query, 10, None);
    }

    let count_after = vector.count(test_db.run_id, "embeddings").unwrap();

    assert_eq!(
        count_before, count_after,
        "R10 VIOLATED: Search created new vectors"
    );
}

/// Test that search does not modify embeddings
#[test]
fn test_r10_search_does_not_modify_embeddings() {
    let test_db = TestDb::new();
    let vector = test_db.vector();

    vector
        .create_collection(test_db.run_id, "embeddings", config_minilm())
        .unwrap();

    // Insert with known embeddings
    let embeddings: Vec<(String, Vec<f32>)> = (0..20)
        .map(|i| (format!("key_{}", i), seeded_random_vector(384, i as u64)))
        .collect();

    for (key, emb) in &embeddings {
        vector
            .insert(test_db.run_id, "embeddings", key, emb, None)
            .unwrap();
    }

    // Many searches
    for i in 0..50 {
        let query = seeded_random_vector(384, i as u64 + 5000);
        let _ = vector.search(test_db.run_id, "embeddings", &query, 20, None);
    }

    // Verify embeddings unchanged
    for (key, expected_emb) in &embeddings {
        let entry = vector
            .get(test_db.run_id, "embeddings", key)
            .unwrap()
            .unwrap();
        for (i, (&expected, &actual)) in expected_emb.iter().zip(entry.value.embedding.iter()).enumerate()
        {
            assert!(
                (expected - actual).abs() < 1e-6,
                "R10 VIOLATED: Search modified embedding for {} at index {}",
                key,
                i
            );
        }
    }
}

/// Test that search does not modify metadata
#[test]
fn test_r10_search_does_not_modify_metadata() {
    let test_db = TestDb::new();
    let vector = test_db.vector();

    vector
        .create_collection(test_db.run_id, "embeddings", config_minilm())
        .unwrap();

    for i in 0..20 {
        vector
            .insert(
                test_db.run_id,
                "embeddings",
                &format!("key_{}", i),
                &seeded_random_vector(384, i as u64),
                Some(serde_json::json!({"index": i, "data": "original"})),
            )
            .unwrap();
    }

    // Many searches
    for i in 0..50 {
        let query = seeded_random_vector(384, i as u64 + 3000);
        let _ = vector.search(test_db.run_id, "embeddings", &query, 20, None);
    }

    // Verify metadata unchanged
    for i in 0..20 {
        let entry = vector
            .get(test_db.run_id, "embeddings", &format!("key_{}", i))
            .unwrap()
            .unwrap();
        let metadata = entry.value.metadata.unwrap();
        assert_eq!(
            metadata["data"], "original",
            "R10 VIOLATED: Search modified metadata for key_{}",
            i
        );
    }
}

/// Test that search returns results without side effects
#[test]
fn test_r10_search_pure_function() {
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

    let query = seeded_random_vector(384, 77777);

    // First search
    let results1 = vector
        .search(test_db.run_id, "embeddings", &query, 10, None)
        .unwrap();

    // Search should be idempotent - same query, same results
    for _ in 0..20 {
        let results = vector
            .search(test_db.run_id, "embeddings", &query, 10, None)
            .unwrap();
        let keys1: Vec<&str> = results1.iter().map(|r| r.key.as_str()).collect();
        let keys: Vec<&str> = results.iter().map(|r| r.key.as_str()).collect();
        assert_eq!(
            keys1, keys,
            "R10 VIOLATED: Repeated search returned different results (side effects?)"
        );
    }
}

/// Test that search does not affect collection info
#[test]
fn test_r10_search_does_not_affect_collection_info() {
    let test_db = TestDb::new();
    let vector = test_db.vector();

    vector
        .create_collection(test_db.run_id, "embeddings", config_minilm())
        .unwrap();

    for i in 0..25 {
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

    let info_before = vector
        .get_collection(test_db.run_id, "embeddings")
        .unwrap()
        .unwrap();

    // Many searches
    for i in 0..100 {
        let query = seeded_random_vector(384, i as u64 + 8000);
        let _ = vector.search(test_db.run_id, "embeddings", &query, 25, None);
    }

    let info_after = vector
        .get_collection(test_db.run_id, "embeddings")
        .unwrap()
        .unwrap();

    assert_eq!(info_before.value.count, info_after.value.count);
    assert_eq!(info_before.value.config.dimension, info_after.value.config.dimension);
    assert_eq!(info_before.value.config.metric, info_after.value.config.metric);
}
