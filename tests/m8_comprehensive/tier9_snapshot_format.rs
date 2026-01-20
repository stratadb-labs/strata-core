//! Tier 9: Snapshot Format Tests
//!
//! Tests that verify data persistence format and recovery.

use crate::test_utils::*;

#[test]
fn test_data_persists_after_close() {
    let mut test_db = TestDb::new_strict();
    let run_id = test_db.run_id;

    {
        let vector = test_db.vector();
        vector.create_collection(run_id, "embeddings", config_minilm()).unwrap();
        vector.insert(run_id, "embeddings", "key1", &random_vector(384), None).unwrap();
    }

    test_db.reopen();

    // Verify data persists
    assert!(test_db.vector().get(run_id, "embeddings", "key1").unwrap().is_some());
}

#[test]
fn test_snapshot_preserves_collection_config() {
    let mut test_db = TestDb::new_strict();
    let run_id = test_db.run_id;

    {
        let vector = test_db.vector();
        vector.create_collection(run_id, "embeddings", config_minilm()).unwrap();
        for i in 0..20 {
            vector.insert(run_id, "embeddings", &format!("key_{}", i), &seeded_random_vector(384, i as u64), None).unwrap();
        }
    }

    test_db.reopen();

    let info = test_db.vector().get_collection(run_id, "embeddings").unwrap().unwrap();
    assert_eq!(info.value.config.dimension, 384);
    assert_eq!(info.value.count, 20);
}
