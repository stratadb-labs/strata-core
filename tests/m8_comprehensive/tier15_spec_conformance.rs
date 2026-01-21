//! Tier 15: Spec Conformance Tests
//!
//! Tests that validate conformance to the M8 Vector Primitive specification.
//! Reference: docs/contracts/M8_CONTRACT.md

use crate::test_utils::*;

// =============================================================================
// Storage Invariants (S1-S9) Spec Conformance
// =============================================================================

/// S1: Dimension is immutable after collection creation
#[test]
fn spec_s1_dimension_immutable() {
    let test_db = TestDb::new_strict();
    let run_id = test_db.run_id;
    let vector = test_db.vector();

    vector.create_collection(run_id, "embeddings", config_minilm()).unwrap();

    // Should reject wrong dimension
    let wrong_dim_vec = random_vector(128);
    let result = vector.insert(run_id, "embeddings", "key1", &wrong_dim_vec, None);
    assert!(result.is_err(), "S1: Must reject wrong dimension vector");

    // Correct dimension should work
    let correct_dim_vec = random_vector(384);
    assert!(vector.insert(run_id, "embeddings", "key2", &correct_dim_vec, None).is_ok());
}

/// S3: VectorId remains stable across operations
#[test]
fn spec_s3_vectorid_stable() {
    let test_db = TestDb::new_strict();
    let run_id = test_db.run_id;
    let vector = test_db.vector();

    vector.create_collection(run_id, "embeddings", config_minilm()).unwrap();

    // Insert
    vector.insert(run_id, "embeddings", "key1", &seeded_random_vector(384, 1), None).unwrap();
    let id_after_insert = vector.get(run_id, "embeddings", "key1").unwrap().unwrap().value.vector_id().as_u64();

    // Update
    vector.insert(run_id, "embeddings", "key1", &seeded_random_vector(384, 2), None).unwrap();
    let id_after_update = vector.get(run_id, "embeddings", "key1").unwrap().unwrap().value.vector_id().as_u64();

    assert_eq!(id_after_insert, id_after_update, "S3: VectorId must remain stable across updates");
}

/// S4: VectorIds are never reused
#[test]
fn spec_s4_vectorid_never_reused() {
    let test_db = TestDb::new_strict();
    let run_id = test_db.run_id;
    let vector = test_db.vector();

    vector.create_collection(run_id, "embeddings", config_minilm()).unwrap();

    // Insert and capture ID
    vector.insert(run_id, "embeddings", "key1", &random_vector(384), None).unwrap();
    let original_id = vector.get(run_id, "embeddings", "key1").unwrap().unwrap().value.vector_id().as_u64();

    // Delete
    vector.delete(run_id, "embeddings", "key1").unwrap();

    // Reinsert same key
    vector.insert(run_id, "embeddings", "key1", &random_vector(384), None).unwrap();
    let new_id = vector.get(run_id, "embeddings", "key1").unwrap().unwrap().value.vector_id().as_u64();

    assert!(new_id > original_id, "S4: New VectorId {} must be > old VectorId {}", new_id, original_id);
}

/// S6: Run isolation
#[test]
fn spec_s6_run_isolation() {
    let test_db = TestDb::new_strict();
    let vector = test_db.vector();

    let run_id_1 = strata_core::types::RunId::new();
    let run_id_2 = strata_core::types::RunId::new();

    // Create same collection name in different runs
    vector.create_collection(run_id_1, "embeddings", config_minilm()).unwrap();
    vector.create_collection(run_id_2, "embeddings", config_small()).unwrap();

    // Insert into each
    vector.insert(run_id_1, "embeddings", "key1", &random_vector(384), None).unwrap();
    vector.insert(run_id_2, "embeddings", "key1", &random_vector(3), None).unwrap();

    // Verify isolation
    let vec1 = vector.get(run_id_1, "embeddings", "key1").unwrap().unwrap();
    let vec2 = vector.get(run_id_2, "embeddings", "key1").unwrap().unwrap();

    assert_eq!(vec1.value.embedding.len(), 384);
    assert_eq!(vec2.value.embedding.len(), 3);

    // Count should be independent
    assert_eq!(vector.count(run_id_1, "embeddings").unwrap(), 1);
    assert_eq!(vector.count(run_id_2, "embeddings").unwrap(), 1);
}

// =============================================================================
// Search Invariants (R1-R10) Spec Conformance
// =============================================================================

/// R1: Query dimension must match collection dimension
#[test]
fn spec_r1_query_dimension_match() {
    let test_db = TestDb::new_strict();
    let run_id = test_db.run_id;
    let vector = test_db.vector();

    vector.create_collection(run_id, "embeddings", config_minilm()).unwrap();
    vector.insert(run_id, "embeddings", "key1", &random_vector(384), None).unwrap();

    // Wrong dimension query should fail
    let wrong_dim_query = random_vector(128);
    let result = vector.search(run_id, "embeddings", &wrong_dim_query, 10, None);
    assert!(result.is_err(), "R1: Must reject wrong dimension query");

    // Correct dimension should work
    let correct_dim_query = random_vector(384);
    assert!(vector.search(run_id, "embeddings", &correct_dim_query, 10, None).is_ok());
}

/// R2: Higher score = more similar (score normalization)
#[test]
fn spec_r2_score_normalization() {
    let test_db = TestDb::new_strict();
    let run_id = test_db.run_id;
    let vector = test_db.vector();

    vector.create_collection(run_id, "embeddings", config_minilm()).unwrap();

    let query = seeded_random_vector(384, 999);

    // Insert identical vector (should have highest score)
    vector.insert(run_id, "embeddings", "identical", &query, None).unwrap();

    // Insert different vectors
    for i in 0..10 {
        vector.insert(run_id, "embeddings", &format!("different_{}", i), &seeded_random_vector(384, i as u64), None).unwrap();
    }

    let results = vector.search(run_id, "embeddings", &query, 11, None).unwrap();

    // Identical should be first with highest score
    assert_eq!(results[0].key, "identical");

    // Scores should be descending
    for i in 1..results.len() {
        assert!(results[i - 1].score >= results[i].score, "R2: Scores must be descending");
    }
}

/// R3: Same query, same order (deterministic)
#[test]
fn spec_r3_deterministic_order() {
    let test_db = TestDb::new_strict();
    let run_id = test_db.run_id;
    let vector = test_db.vector();

    vector.create_collection(run_id, "embeddings", config_minilm()).unwrap();

    for i in 0..50 {
        vector.insert(run_id, "embeddings", &format!("key_{}", i), &seeded_random_vector(384, i as u64), None).unwrap();
    }

    let query = seeded_random_vector(384, 999);

    let results1 = vector.search(run_id, "embeddings", &query, 20, None).unwrap();
    let results2 = vector.search(run_id, "embeddings", &query, 20, None).unwrap();

    assert_eq!(results1.len(), results2.len());
    for (r1, r2) in results1.iter().zip(results2.iter()) {
        assert_eq!(r1.key, r2.key, "R3: Same query must return same order");
        assert_eq!(r1.score, r2.score);
    }
}

/// R5: Facade tie-breaking by key ascending
#[test]
fn spec_r5_facade_tiebreak() {
    let test_db = TestDb::new_strict();
    let run_id = test_db.run_id;
    let vector = test_db.vector();

    vector.create_collection(run_id, "embeddings", config_small()).unwrap();

    // Insert identical vectors with different keys
    let identical_vec = vec![0.5, 0.5, 0.5];
    vector.insert(run_id, "embeddings", "key_c", &identical_vec, None).unwrap();
    vector.insert(run_id, "embeddings", "key_a", &identical_vec, None).unwrap();
    vector.insert(run_id, "embeddings", "key_b", &identical_vec, None).unwrap();

    let results = vector.search(run_id, "embeddings", &identical_vec, 3, None).unwrap();

    // Should be sorted by key ascending when scores tie
    assert_eq!(results[0].key, "key_a");
    assert_eq!(results[1].key, "key_b");
    assert_eq!(results[2].key, "key_c");
}

/// R10: Search is read-only
#[test]
fn spec_r10_search_readonly() {
    let test_db = TestDb::new_strict();
    let run_id = test_db.run_id;
    let vector = test_db.vector();

    vector.create_collection(run_id, "embeddings", config_minilm()).unwrap();

    for i in 0..20 {
        vector.insert(run_id, "embeddings", &format!("key_{}", i), &seeded_random_vector(384, i as u64), None).unwrap();
    }

    let count_before = vector.count(run_id, "embeddings").unwrap();

    // Perform many searches
    let query = seeded_random_vector(384, 999);
    for _ in 0..100 {
        let _ = vector.search(run_id, "embeddings", &query, 10, None).unwrap();
    }

    let count_after = vector.count(run_id, "embeddings").unwrap();

    assert_eq!(count_before, count_after, "R10: Search must not modify state");
}

// =============================================================================
// Transaction/Durability Invariants Spec Conformance
// =============================================================================

/// T4: VectorId monotonicity across restarts
#[test]
fn spec_t4_vectorid_monotonicity_across_restarts() {
    let mut test_db = TestDb::new_strict();
    let run_id = test_db.run_id;

    let max_id_before;
    {
        let vector = test_db.vector();
        vector.create_collection(run_id, "embeddings", config_minilm()).unwrap();

        for i in 0..50 {
            vector.insert(run_id, "embeddings", &format!("key_{}", i), &seeded_random_vector(384, i as u64), None).unwrap();
        }

        max_id_before = (0..50)
            .map(|i| vector.get(run_id, "embeddings", &format!("key_{}", i)).unwrap().unwrap().value.vector_id().as_u64())
            .max()
            .unwrap();
    }

    // Simulate crash/restart
    test_db.reopen();

    let vector = test_db.vector();

    // New vector should get higher ID
    vector.insert(run_id, "embeddings", "new_key", &random_vector(384), None).unwrap();
    let new_id = vector.get(run_id, "embeddings", "new_key").unwrap().unwrap().value.vector_id().as_u64();

    assert!(new_id > max_id_before, "T4: VectorId {} must be > {} after crash recovery", new_id, max_id_before);
}

/// Durability: All committed operations persist across restarts
#[test]
fn spec_durability_across_restart() {
    let mut test_db = TestDb::new_strict();
    let run_id = test_db.run_id;

    {
        let vector = test_db.vector();
        vector.create_collection(run_id, "embeddings", config_minilm()).unwrap();

        // Insert, update, delete operations
        for i in 0..30 {
            vector.insert(run_id, "embeddings", &format!("key_{}", i), &seeded_random_vector(384, i as u64), None).unwrap();
        }

        // Delete first 10
        for i in 0..10 {
            vector.delete(run_id, "embeddings", &format!("key_{}", i)).unwrap();
        }

        // Update next 10
        for i in 10..20 {
            vector.insert(run_id, "embeddings", &format!("key_{}", i), &seeded_random_vector(384, i as u64 + 100), None).unwrap();
        }
    }

    test_db.reopen();

    let vector = test_db.vector();

    // Verify state
    assert_eq!(vector.count(run_id, "embeddings").unwrap(), 20);

    // First 10 should be deleted
    for i in 0..10 {
        assert!(vector.get(run_id, "embeddings", &format!("key_{}", i)).unwrap().is_none());
    }

    // 10-29 should exist
    for i in 10..30 {
        assert!(vector.get(run_id, "embeddings", &format!("key_{}", i)).unwrap().is_some());
    }
}
