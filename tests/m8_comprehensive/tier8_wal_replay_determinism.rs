//! Tier 8: WAL Replay Determinism Tests

use crate::test_utils::*;

#[test]
fn test_wal_replay_deterministic() {
    let mut test_db = TestDb::new_strict();
    let run_id = test_db.run_id;

    {
        let vector = test_db.vector();
        vector.create_collection(run_id, "embeddings", config_minilm()).unwrap();

        for i in 0..100 {
            vector.insert(run_id, "embeddings", &format!("key_{}", i), &seeded_random_vector(384, i as u64), None).unwrap();
        }
    }

    let state_original = CapturedVectorState::capture(&test_db.vector(), run_id, "embeddings");

    // Replay multiple times
    for replay_count in 0..3 {
        delete_snapshots(&test_db.snapshot_dir());
        test_db.reopen();

        let state_replayed = CapturedVectorState::capture(&test_db.vector(), run_id, "embeddings");
        assert_vector_states_equal(&state_original, &state_replayed, &format!("Replay {} produced different state", replay_count));
    }
}

#[test]
fn test_search_results_deterministic_after_replay() {
    let mut test_db = TestDb::new_strict();
    let run_id = test_db.run_id;
    let query = seeded_random_vector(384, 99999);

    let results_before;
    {
        let vector = test_db.vector();
        vector.create_collection(run_id, "embeddings", config_minilm()).unwrap();

        for i in 0..50 {
            vector.insert(run_id, "embeddings", &format!("key_{}", i), &seeded_random_vector(384, i as u64), None).unwrap();
        }

        results_before = vector.search(run_id, "embeddings", &query, 20, None).unwrap();
    }

    delete_snapshots(&test_db.snapshot_dir());
    test_db.reopen();

    let results_after = test_db.vector().search(run_id, "embeddings", &query, 20, None).unwrap();

    let keys_before: Vec<&str> = results_before.iter().map(|r| r.key.as_str()).collect();
    let keys_after: Vec<&str> = results_after.iter().map(|r| r.key.as_str()).collect();

    assert_eq!(keys_before, keys_after, "Search results differ after replay");
}
