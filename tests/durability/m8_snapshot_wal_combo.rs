//! Tier 9: Snapshot + WAL Combo Tests
//!
//! Tests that verify recovery from both persisted state and WAL replay.
//! These tests simulate checkpointing by reopening the database.

use crate::common::*;

#[test]
fn test_recovery_from_snapshot_plus_wal() {
    let mut test_db = TestDb::new_strict();
    let run_id = test_db.run_id;

    {
        let vector = test_db.vector();
        vector.create_collection(run_id, "embeddings", config_minilm()).unwrap();

        // Insert vectors before first checkpoint
        for i in 0..50 {
            vector.insert(run_id, "embeddings", &format!("key_{}", i), &seeded_random_vector(384, i as u64), None).unwrap();
        }
    }

    // Simulate checkpoint by reopening
    test_db.reopen();

    {
        let vector = test_db.vector();
        // Insert more after checkpoint
        for i in 50..100 {
            vector.insert(run_id, "embeddings", &format!("key_{}", i), &seeded_random_vector(384, i as u64), None).unwrap();
        }
    }

    let state_before = CapturedVectorState::capture(&test_db.vector(), run_id, "embeddings");

    test_db.reopen();

    let state_after = CapturedVectorState::capture(&test_db.vector(), run_id, "embeddings");

    assert_vector_states_equal(&state_before, &state_after, "Recovery from snapshot+WAL failed");
    assert_eq!(test_db.vector().count(run_id, "embeddings").unwrap(), 100);
}

#[test]
fn test_snapshot_wal_delete_operations() {
    let mut test_db = TestDb::new_strict();
    let run_id = test_db.run_id;

    {
        let vector = test_db.vector();
        vector.create_collection(run_id, "embeddings", config_minilm()).unwrap();

        for i in 0..50 {
            vector.insert(run_id, "embeddings", &format!("key_{}", i), &seeded_random_vector(384, i as u64), None).unwrap();
        }
    }

    // Simulate checkpoint by reopening
    test_db.reopen();

    {
        let vector = test_db.vector();
        // Delete some after checkpoint
        for i in 0..25 {
            vector.delete(run_id, "embeddings", &format!("key_{}", i)).unwrap();
        }
    }

    test_db.reopen();

    let vector = test_db.vector();
    assert_eq!(vector.count(run_id, "embeddings").unwrap(), 25);

    // Deleted keys should not exist
    for i in 0..25 {
        assert!(vector.get(run_id, "embeddings", &format!("key_{}", i)).unwrap().is_none());
    }
}
