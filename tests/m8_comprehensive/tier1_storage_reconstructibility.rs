//! S9: Heap-KV Reconstructibility Tests
//!
//! Invariant S9: VectorHeap and KV metadata can both be fully reconstructed from snapshot + WAL.

use crate::test_utils::*;
use serde_json::json;

/// Test heap is fully reconstructible from snapshot + WAL
#[test]
fn test_s9_heap_reconstructible() {
    let mut test_db = TestDb::new_strict();
    let run_id = test_db.run_id;

    let embeddings: Vec<(String, Vec<f32>)> = (0..20)
        .map(|i| (format!("key_{}", i), seeded_random_vector(384, i as u64)))
        .collect();

    {
        let vector = test_db.vector();
        vector
            .create_collection(run_id, "embeddings", config_minilm())
            .unwrap();

        for (key, emb) in &embeddings {
            vector.insert(run_id, "embeddings", key, emb, None).unwrap();
        }
    }

    // Capture heap state (embeddings)
    let state_before = CapturedVectorState::capture(&test_db.vector(), run_id, "embeddings");

    // Restart
    test_db.reopen();

    let state_after = CapturedVectorState::capture(&test_db.vector(), run_id, "embeddings");

    assert_vector_states_equal(
        &state_before,
        &state_after,
        "S9 VIOLATED: Heap not reconstructed correctly",
    );

    // Additionally verify each embedding matches exactly
    let vector = test_db.vector();
    for (key, expected_emb) in &embeddings {
        let entry = vector.get(run_id, "embeddings", key).unwrap().unwrap();
        for (i, (expected, actual)) in expected_emb.iter().zip(entry.value.embedding.iter()).enumerate() {
            assert!(
                (expected - actual).abs() < 1e-6,
                "S9 VIOLATED: Embedding mismatch for {} at index {}",
                key,
                i
            );
        }
    }
}

/// Test metadata is fully reconstructible
#[test]
fn test_s9_kv_metadata_reconstructible() {
    let mut test_db = TestDb::new_strict();
    let run_id = test_db.run_id;

    {
        let vector = test_db.vector();
        vector
            .create_collection(run_id, "embeddings", config_minilm())
            .unwrap();

        for i in 0..20 {
            vector
                .insert(
                    run_id,
                    "embeddings",
                    &format!("key_{}", i),
                    &seeded_random_vector(384, i as u64),
                    Some(json!({
                        "index": i,
                        "category": format!("cat_{}", i % 5),
                        "nested": {"value": i * 10}
                    })),
                )
                .unwrap();
        }
    }

    // Capture metadata
    let state_before = CapturedVectorState::capture(&test_db.vector(), run_id, "embeddings");

    // Restart
    test_db.reopen();

    let state_after = CapturedVectorState::capture(&test_db.vector(), run_id, "embeddings");

    assert_vector_states_equal(
        &state_before,
        &state_after,
        "S9 VIOLATED: Metadata not reconstructed correctly",
    );
}

/// Test reconstruction after delete operations
#[test]
fn test_s9_reconstructible_after_deletes() {
    let mut test_db = TestDb::new_strict();
    let run_id = test_db.run_id;

    {
        let vector = test_db.vector();
        vector
            .create_collection(run_id, "embeddings", config_minilm())
            .unwrap();

        // Insert 50 vectors
        for i in 0..50 {
            vector
                .insert(
                    run_id,
                    "embeddings",
                    &format!("key_{}", i),
                    &seeded_random_vector(384, i as u64),
                    Some(json!({"index": i})),
                )
                .unwrap();
        }

        // Delete half
        for i in 0..25 {
            vector.delete(run_id, "embeddings", &format!("key_{}", i)).unwrap();
        }
    }

    let state_before = CapturedVectorState::capture(&test_db.vector(), run_id, "embeddings");

    test_db.reopen();

    let state_after = CapturedVectorState::capture(&test_db.vector(), run_id, "embeddings");

    assert_vector_states_equal(
        &state_before,
        &state_after,
        "S9 VIOLATED: State not reconstructed after deletes",
    );

    // Verify deleted keys don't exist
    let vector = test_db.vector();
    for i in 0..25 {
        assert!(
            vector.get(run_id, "embeddings", &format!("key_{}", i)).unwrap().is_none(),
            "S9 VIOLATED: Deleted key {} exists after reconstruction",
            i
        );
    }
}

/// Test reconstruction after upsert (overwrite) operations
#[test]
fn test_s9_reconstructible_after_upserts() {
    let mut test_db = TestDb::new_strict();
    let run_id = test_db.run_id;

    {
        let vector = test_db.vector();
        vector
            .create_collection(run_id, "embeddings", config_minilm())
            .unwrap();

        // Insert vectors
        for i in 0..20 {
            vector
                .insert(
                    run_id,
                    "embeddings",
                    &format!("key_{}", i),
                    &seeded_random_vector(384, i as u64),
                    Some(json!({"version": 1})),
                )
                .unwrap();
        }

        // Update half with new embeddings and metadata
        for i in 0..10 {
            vector
                .insert(
                    run_id,
                    "embeddings",
                    &format!("key_{}", i),
                    &seeded_random_vector(384, i as u64 + 1000), // Different seed
                    Some(json!({"version": 2, "updated": true})),
                )
                .unwrap();
        }
    }

    let state_before = CapturedVectorState::capture(&test_db.vector(), run_id, "embeddings");

    test_db.reopen();

    let state_after = CapturedVectorState::capture(&test_db.vector(), run_id, "embeddings");

    assert_vector_states_equal(
        &state_before,
        &state_after,
        "S9 VIOLATED: State not reconstructed after upserts",
    );
}

/// Test reconstruction preserves VectorIds
#[test]
fn test_s9_vectorid_preserved_in_reconstruction() {
    let mut test_db = TestDb::new_strict();
    let run_id = test_db.run_id;

    let mut ids_before = std::collections::HashMap::new();

    {
        let vector = test_db.vector();
        vector
            .create_collection(run_id, "embeddings", config_minilm())
            .unwrap();

        for i in 0..30 {
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

        // Capture IDs
        for i in 0..30 {
            let entry = vector
                .get(run_id, "embeddings", &format!("key_{}", i))
                .unwrap()
                .unwrap();
            ids_before.insert(format!("key_{}", i), entry.value.vector_id());
        }
    }

    // Restart
    test_db.reopen();

    // Verify IDs are preserved
    let vector = test_db.vector();
    for (key, id_before) in &ids_before {
        let entry = vector.get(run_id, "embeddings", key).unwrap().unwrap();
        assert_eq!(
            *id_before, entry.value.vector_id(),
            "S9 VIOLATED: VectorId for {} changed after reconstruction",
            key
        );
    }
}

/// Test reconstruction with multiple collections
#[test]
fn test_s9_multiple_collections_reconstructible() {
    let mut test_db = TestDb::new_strict();
    let run_id = test_db.run_id;

    {
        let vector = test_db.vector();

        // Create multiple collections with different configs
        vector
            .create_collection(run_id, "minilm", config_minilm())
            .unwrap();
        vector
            .create_collection(run_id, "small", config_small())
            .unwrap();

        // Populate both
        for i in 0..10 {
            vector
                .insert(
                    run_id,
                    "minilm",
                    &format!("key_{}", i),
                    &seeded_random_vector(384, i as u64),
                    None,
                )
                .unwrap();
            vector
                .insert(
                    run_id,
                    "small",
                    &format!("key_{}", i),
                    &seeded_random_vector(3, i as u64 + 100),
                    None,
                )
                .unwrap();
        }
    }

    let state_minilm_before = CapturedVectorState::capture(&test_db.vector(), run_id, "minilm");
    let state_small_before = CapturedVectorState::capture(&test_db.vector(), run_id, "small");

    test_db.reopen();

    let state_minilm_after = CapturedVectorState::capture(&test_db.vector(), run_id, "minilm");
    let state_small_after = CapturedVectorState::capture(&test_db.vector(), run_id, "small");

    assert_vector_states_equal(
        &state_minilm_before,
        &state_minilm_after,
        "S9 VIOLATED: minilm collection not reconstructed",
    );
    assert_vector_states_equal(
        &state_small_before,
        &state_small_after,
        "S9 VIOLATED: small collection not reconstructed",
    );
}

/// Test reconstruction with checkpoint and WAL
#[test]
fn test_s9_snapshot_plus_wal_reconstruction() {
    let mut test_db = TestDb::new_strict();
    let run_id = test_db.run_id;

    {
        let vector = test_db.vector();
        vector
            .create_collection(run_id, "embeddings", config_minilm())
            .unwrap();

        // Phase 1: Before checkpoint
        for i in 0..30 {
            vector
                .insert(
                    run_id,
                    "embeddings",
                    &format!("key_{}", i),
                    &seeded_random_vector(384, i as u64),
                    Some(json!({"phase": 1})),
                )
                .unwrap();
        }
    }

    // Simulate checkpoint
    test_db.reopen();

    {
        let vector = test_db.vector();
        // Phase 2: After checkpoint
        for i in 30..50 {
            vector
                .insert(
                    run_id,
                    "embeddings",
                    &format!("key_{}", i),
                    &seeded_random_vector(384, i as u64),
                    Some(json!({"phase": 2})),
                )
                .unwrap();
        }

        // Delete some from phase 1
        for i in 0..10 {
            vector.delete(run_id, "embeddings", &format!("key_{}", i)).unwrap();
        }
    }

    let state_before = CapturedVectorState::capture(&test_db.vector(), run_id, "embeddings");

    test_db.reopen();

    let state_after = CapturedVectorState::capture(&test_db.vector(), run_id, "embeddings");

    assert_vector_states_equal(
        &state_before,
        &state_after,
        "S9 VIOLATED: Checkpoint + WAL reconstruction failed",
    );
}
