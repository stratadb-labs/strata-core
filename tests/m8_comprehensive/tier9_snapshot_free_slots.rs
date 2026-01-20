//! Tier 9: Snapshot free_slots Preservation Tests

use crate::test_utils::*;

#[test]
fn test_snapshot_free_slots_preserved() {
    let mut test_db = TestDb::new_strict();
    let run_id = test_db.run_id;

    let max_id_before_delete;
    {
        let vector = test_db.vector();
        vector.create_collection(run_id, "embeddings", config_minilm()).unwrap();

        // Insert and delete to create free slots
        for i in 0..20 {
            vector.insert(run_id, "embeddings", &format!("key_{}", i), &seeded_random_vector(384, i as u64), None).unwrap();
        }

        max_id_before_delete = (0..20)
            .map(|i| vector.get(run_id, "embeddings", &format!("key_{}", i)).unwrap().unwrap().value.vector_id().as_u64())
            .max()
            .unwrap();

        // Delete first 10 (creates free slots)
        for i in 0..10 {
            vector.delete(run_id, "embeddings", &format!("key_{}", i)).unwrap();
        }
    }

    test_db.reopen();

    let vector = test_db.vector();

    // Insert new vectors - should get new IDs (not reuse deleted ones)
    for i in 20..25 {
        vector.insert(run_id, "embeddings", &format!("key_{}", i), &seeded_random_vector(384, i as u64), None).unwrap();
        let new_id = vector.get(run_id, "embeddings", &format!("key_{}", i)).unwrap().unwrap().value.vector_id().as_u64();
        assert!(new_id > max_id_before_delete, "VectorId {} should be > {} (IDs not reused)", new_id, max_id_before_delete);
    }
}
