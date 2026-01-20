//! Tier 11: Crash During Collection Create Tests
//!
//! Tests for durability of collection creation.

use crate::test_utils::*;

#[test]
fn test_committed_collection_persisted() {
    let mut test_db = TestDb::new_strict();
    let run_id = test_db.run_id;

    {
        let vector = test_db.vector();
        vector.create_collection(run_id, "committed_col", config_minilm()).unwrap();
    }

    test_db.reopen();

    let vector = test_db.vector();
    let collections = vector.list_collections(run_id).unwrap();

    assert!(collections.iter().any(|c| c.name == "committed_col"));
}

#[test]
fn test_collection_with_data_persisted() {
    let mut test_db = TestDb::new_strict();
    let run_id = test_db.run_id;

    {
        let vector = test_db.vector();
        vector.create_collection(run_id, "committed_col", config_minilm()).unwrap();
        vector.insert(run_id, "committed_col", "key1", &random_vector(384), None).unwrap();
    }

    test_db.reopen();

    let vector = test_db.vector();

    // Collection and its data should exist
    assert!(vector.get_collection(run_id, "committed_col").unwrap().is_some());
    assert!(vector.get(run_id, "committed_col", "key1").unwrap().is_some());
}

#[test]
fn test_multiple_collections_persisted() {
    let mut test_db = TestDb::new_strict();
    let run_id = test_db.run_id;

    {
        let vector = test_db.vector();
        for i in 0..5 {
            vector.create_collection(run_id, &format!("col_{}", i), config_minilm()).unwrap();
        }
    }

    test_db.reopen();

    let vector = test_db.vector();
    let collections = vector.list_collections(run_id).unwrap();

    assert_eq!(collections.len(), 5);
    for i in 0..5 {
        assert!(collections.iter().any(|c| c.name == format!("col_{}", i)));
    }
}

#[test]
fn test_collection_config_preserved() {
    let mut test_db = TestDb::new_strict();
    let run_id = test_db.run_id;

    {
        let vector = test_db.vector();
        vector.create_collection(run_id, "minilm_col", config_minilm()).unwrap();
        vector.create_collection(run_id, "small_col", config_small()).unwrap();

        vector.insert(run_id, "minilm_col", "key1", &random_vector(384), None).unwrap();
        vector.insert(run_id, "small_col", "key1", &random_vector(3), None).unwrap();
    }

    test_db.reopen();

    let vector = test_db.vector();

    let minilm_info = vector.get_collection(run_id, "minilm_col").unwrap().unwrap();
    let small_info = vector.get_collection(run_id, "small_col").unwrap().unwrap();

    assert_eq!(minilm_info.value.config.dimension, 384);
    assert_eq!(small_info.value.config.dimension, 3);
}
