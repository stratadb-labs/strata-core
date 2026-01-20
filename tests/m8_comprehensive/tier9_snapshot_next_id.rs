//! Tier 9: Snapshot max_id Preservation Tests

use crate::test_utils::*;

#[test]
fn test_snapshot_max_id_persisted() {
    let mut test_db = TestDb::new_strict();
    let run_id = test_db.run_id;

    let max_id_before;
    {
        let vector = test_db.vector();
        vector.create_collection(run_id, "embeddings", config_minilm()).unwrap();

        for i in 0..100 {
            vector.insert(run_id, "embeddings", &format!("key_{}", i), &seeded_random_vector(384, i as u64), None).unwrap();
        }

        max_id_before = (0..100)
            .map(|i| vector.get(run_id, "embeddings", &format!("key_{}", i)).unwrap().unwrap().value.vector_id().as_u64())
            .max()
            .unwrap();
    }

    test_db.reopen();

    let vector = test_db.vector();

    // Insert new vector
    vector.insert(run_id, "embeddings", "new_key", &random_vector(384), None).unwrap();
    let new_id = vector.get(run_id, "embeddings", "new_key").unwrap().unwrap().value.vector_id().as_u64();

    assert!(new_id > max_id_before, "max_id not preserved: {} should be > {}", new_id, max_id_before);
}
