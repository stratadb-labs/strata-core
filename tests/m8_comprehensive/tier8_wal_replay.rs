//! Tier 8: WAL Replay Tests

use crate::test_utils::*;

#[test]
fn test_wal_replay_produces_identical_state() {
    let mut test_db = TestDb::new_strict();
    let run_id = test_db.run_id;

    let embeddings: Vec<(String, Vec<f32>)> = (0..50)
        .map(|i| (format!("key_{}", i), seeded_random_vector(384, i as u64)))
        .collect();

    {
        let vector = test_db.vector();
        vector.create_collection(run_id, "embeddings", config_minilm()).unwrap();

        for (key, emb) in &embeddings {
            vector.insert(run_id, "embeddings", key, emb, None).unwrap();
        }

        // Delete some
        for i in 0..10 {
            vector.delete(run_id, "embeddings", &format!("key_{}", i)).unwrap();
        }
    }

    let state_before = CapturedVectorState::capture(&test_db.vector(), run_id, "embeddings");

    // Delete snapshots to force pure WAL replay
    delete_snapshots(&test_db.snapshot_dir());

    // Replay WAL
    test_db.reopen();

    let state_after = CapturedVectorState::capture(&test_db.vector(), run_id, "embeddings");

    assert_vector_states_equal(&state_before, &state_after, "WAL replay produced different state");
}

#[test]
fn test_wal_replay_preserves_vectorids() {
    let mut test_db = TestDb::new_strict();
    let run_id = test_db.run_id;

    let mut ids_before = std::collections::HashMap::new();
    {
        let vector = test_db.vector();
        vector.create_collection(run_id, "embeddings", config_minilm()).unwrap();

        for i in 0..30 {
            vector.insert(run_id, "embeddings", &format!("key_{}", i), &seeded_random_vector(384, i as u64), None).unwrap();
            let id = vector.get(run_id, "embeddings", &format!("key_{}", i)).unwrap().unwrap().value.vector_id();
            ids_before.insert(format!("key_{}", i), id);
        }
    }

    delete_snapshots(&test_db.snapshot_dir());
    test_db.reopen();

    let vector = test_db.vector();
    for (key, id_before) in &ids_before {
        let id_after = vector.get(run_id, "embeddings", key).unwrap().unwrap().value.vector_id();
        assert_eq!(*id_before, id_after, "VectorId for {} changed after replay", key);
    }
}
