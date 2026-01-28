//! S8: Snapshot-WAL Equivalence Tests
//!
//! Invariant S8: State must be recoverable correctly through checkpoints and WAL replay.

use crate::common::*;

/// Test checkpoint + WAL recovery produces correct state
#[test]
fn test_s8_snapshot_wal_equivalence() {
    let mut test_db = TestDb::new_strict();
    let run_id = test_db.run_id;

    // Store embeddings for comparison
    let embeddings: Vec<(String, Vec<f32>)> = (0..50)
        .map(|i| (format!("key_{}", i), seeded_random_vector(384, i as u64)))
        .collect();

    {
        let vector = test_db.vector();
        vector
            .create_collection(run_id, "embeddings", config_minilm())
            .unwrap();

        // Insert vectors
        for (key, emb) in &embeddings {
            vector.insert(run_id, "embeddings", key, emb, None).unwrap();
        }
    }

    // Simulate checkpoint
    test_db.reopen();

    {
        let vector = test_db.vector();
        // More operations after checkpoint
        for i in 50..100 {
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
        vector.delete(run_id, "embeddings", "key_25").unwrap();
    }

    // Capture state before recovery
    let state_before = CapturedVectorState::capture(&test_db.vector(), run_id, "embeddings");

    // Recover using checkpoint + WAL
    test_db.reopen();
    let state_snapshot = CapturedVectorState::capture(&test_db.vector(), run_id, "embeddings");

    // States should be equal
    assert_vector_states_equal(
        &state_before,
        &state_snapshot,
        "S8 VIOLATED: Checkpoint recovery differs from original",
    );
}

/// Test multiple checkpoints with interleaved operations
#[test]
fn test_s8_multiple_snapshots_equivalence() {
    let mut test_db = TestDb::new_strict();
    let run_id = test_db.run_id;

    {
        let vector = test_db.vector();
        vector
            .create_collection(run_id, "embeddings", config_minilm())
            .unwrap();

        // Phase 1: Insert
        for i in 0..20 {
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
    }

    // Checkpoint 1
    test_db.reopen();

    {
        let vector = test_db.vector();
        // Phase 2: More inserts and deletes
        for i in 20..40 {
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
        for i in 0..10 {
            vector.delete(run_id, "embeddings", &format!("key_{}", i)).unwrap();
        }
    }

    // Checkpoint 2
    test_db.reopen();

    {
        let vector = test_db.vector();
        // Phase 3: Final operations
        for i in 40..50 {
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
    }

    let state_before = CapturedVectorState::capture(&test_db.vector(), run_id, "embeddings");

    // Recover
    test_db.reopen();
    let state_after = CapturedVectorState::capture(&test_db.vector(), run_id, "embeddings");

    assert_vector_states_equal(
        &state_before,
        &state_after,
        "S8 VIOLATED: Multiple checkpoint recovery failed",
    );
}

/// Test checkpoint equivalence with metadata
#[test]
fn test_s8_snapshot_preserves_metadata() {
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
                    Some(serde_json::json!({"index": i, "category": format!("cat_{}", i % 3)})),
                )
                .unwrap();
        }
    }

    // Checkpoint
    test_db.reopen();

    {
        let vector = test_db.vector();
        // More with metadata
        for i in 20..30 {
            vector
                .insert(
                    run_id,
                    "embeddings",
                    &format!("key_{}", i),
                    &seeded_random_vector(384, i as u64),
                    Some(serde_json::json!({"index": i, "after_snapshot": true})),
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
        "S8 VIOLATED: Metadata not preserved through checkpoint",
    );
}

/// Test checkpoint with collection deletion and recreation
#[test]
fn test_s8_snapshot_collection_lifecycle() {
    let mut test_db = TestDb::new_strict();
    let run_id = test_db.run_id;

    {
        let vector = test_db.vector();

        // Create and populate first collection
        vector
            .create_collection(run_id, "collection1", config_minilm())
            .unwrap();
        for i in 0..10 {
            vector
                .insert(
                    run_id,
                    "collection1",
                    &format!("key_{}", i),
                    &seeded_random_vector(384, i as u64),
                    None,
                )
                .unwrap();
        }
    }

    // Checkpoint
    test_db.reopen();

    {
        let vector = test_db.vector();
        // Delete collection1, create collection2
        vector.delete_collection(run_id, "collection1").unwrap();
        vector
            .create_collection(run_id, "collection2", config_minilm())
            .unwrap();
        for i in 0..5 {
            vector
                .insert(
                    run_id,
                    "collection2",
                    &format!("key_{}", i),
                    &seeded_random_vector(384, i as u64 + 100),
                    None,
                )
                .unwrap();
        }
    }

    // Capture state
    let vector = test_db.vector();
    let collection1_exists_before = vector.get_collection(run_id, "collection1").unwrap().is_some();
    let collection2_count_before = vector.count(run_id, "collection2").unwrap();

    // Recover
    test_db.reopen();
    let vector = test_db.vector();

    // collection1 should not exist
    let collection1_exists_after = vector.get_collection(run_id, "collection1").unwrap().is_some();
    assert_eq!(
        collection1_exists_before, collection1_exists_after,
        "S8 VIOLATED: Collection existence changed"
    );

    // collection2 should have 5 entries
    let collection2_count_after = vector.count(run_id, "collection2").unwrap();
    assert_eq!(
        collection2_count_before, collection2_count_after,
        "S8 VIOLATED: Collection2 count changed"
    );
}
