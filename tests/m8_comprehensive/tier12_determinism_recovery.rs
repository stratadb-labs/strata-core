//! Tier 12: Determinism - Recovery Determinism Tests

use crate::test_utils::*;

#[test]
fn test_wal_replay_determinism() {
    let mut test_db = TestDb::new_strict();
    let run_id = test_db.run_id;

    let state_before;
    {
        let vector = test_db.vector();
        vector.create_collection(run_id, "embeddings", config_minilm()).unwrap();

        // Perform deterministic operations
        for i in 0..50 {
            vector.insert(run_id, "embeddings", &format!("key_{}", i), &seeded_random_vector(384, i as u64), None).unwrap();
        }

        // Delete some
        for i in 0..10 {
            vector.delete(run_id, "embeddings", &format!("key_{}", i)).unwrap();
        }

        state_before = CapturedVectorState::capture(&vector, run_id, "embeddings");
    }

    // Reopen multiple times
    for _ in 0..3 {
        test_db.reopen();

        let vector = test_db.vector();
        let state_after = CapturedVectorState::capture(&vector, run_id, "embeddings");

        assert_vector_states_equal(&state_before, &state_after, "WAL replay should be deterministic");
    }
}

#[test]
fn test_snapshot_recovery_determinism() {
    let mut test_db = TestDb::new_strict();
    let run_id = test_db.run_id;

    let state_before;
    {
        let vector = test_db.vector();
        vector.create_collection(run_id, "embeddings", config_minilm()).unwrap();

        for i in 0..50 {
            vector.insert(run_id, "embeddings", &format!("key_{}", i), &seeded_random_vector(384, i as u64), None).unwrap();
        }

        state_before = CapturedVectorState::capture(&vector, run_id, "embeddings");
    }

    // Reopen multiple times
    for _ in 0..3 {
        test_db.reopen();

        let vector = test_db.vector();
        let state_after = CapturedVectorState::capture(&vector, run_id, "embeddings");

        assert_vector_states_equal(&state_before, &state_after, "Snapshot recovery should be deterministic");
    }
}

#[test]
fn test_mixed_recovery_determinism() {
    let mut test_db = TestDb::new_strict();
    let run_id = test_db.run_id;

    {
        let vector = test_db.vector();
        vector.create_collection(run_id, "embeddings", config_minilm()).unwrap();

        // Insert before checkpoint
        for i in 0..30 {
            vector.insert(run_id, "embeddings", &format!("key_{}", i), &seeded_random_vector(384, i as u64), None).unwrap();
        }
    }

    // Simulate checkpoint
    test_db.reopen();

    {
        let vector = test_db.vector();
        // More operations after checkpoint
        for i in 30..50 {
            vector.insert(run_id, "embeddings", &format!("key_{}", i), &seeded_random_vector(384, i as u64), None).unwrap();
        }

        for i in 0..10 {
            vector.delete(run_id, "embeddings", &format!("key_{}", i)).unwrap();
        }
    }

    let state_before = CapturedVectorState::capture(&test_db.vector(), run_id, "embeddings");

    // Reopen multiple times
    for _ in 0..3 {
        test_db.reopen();

        let vector = test_db.vector();
        let state_after = CapturedVectorState::capture(&vector, run_id, "embeddings");

        assert_vector_states_equal(&state_before, &state_after, "Mixed recovery should be deterministic");
    }
}
