//! Tier 9: Snapshot Recovery Tests

use crate::common::*;

#[test]
fn test_recovery_from_snapshot() {
    let mut test_db = TestDb::new_strict();
    let run_id = test_db.run_id;

    {
        let vector = test_db.vector();
        vector.create_collection(run_id, "embeddings", config_minilm()).unwrap();

        for i in 0..50 {
            vector.insert(run_id, "embeddings", &format!("key_{}", i), &seeded_random_vector(384, i as u64), None).unwrap();
        }
        // Implicit snapshot/flush on close
    }

    let state_before = CapturedVectorState::capture(&test_db.vector(), run_id, "embeddings");

    test_db.reopen();

    let state_after = CapturedVectorState::capture(&test_db.vector(), run_id, "embeddings");

    assert_vector_states_equal(&state_before, &state_after, "Snapshot recovery failed");
}

#[test]
fn test_recovery_multiple_collections() {
    let mut test_db = TestDb::new_strict();
    let run_id = test_db.run_id;

    {
        let vector = test_db.vector();
        vector.create_collection(run_id, "col1", config_minilm()).unwrap();
        vector.create_collection(run_id, "col2", config_small()).unwrap();

        for i in 0..20 {
            vector.insert(run_id, "col1", &format!("key_{}", i), &seeded_random_vector(384, i as u64), None).unwrap();
            vector.insert(run_id, "col2", &format!("key_{}", i), &seeded_random_vector(3, i as u64 + 100), None).unwrap();
        }
        // Implicit snapshot/flush on close
    }

    test_db.reopen();

    let vector = test_db.vector();
    assert_eq!(vector.count(run_id, "col1").unwrap(), 20);
    assert_eq!(vector.count(run_id, "col2").unwrap(), 20);
}
