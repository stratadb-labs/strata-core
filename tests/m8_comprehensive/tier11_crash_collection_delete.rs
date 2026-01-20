//! Tier 11: Crash During Collection Delete Tests
//!
//! Tests for durability of collection deletion.

use crate::test_utils::*;

#[test]
fn test_committed_collection_delete_persisted() {
    let mut test_db = TestDb::new_strict();
    let run_id = test_db.run_id;

    {
        let vector = test_db.vector();

        // Create and populate collection
        vector.create_collection(run_id, "embeddings", config_minilm()).unwrap();
        for i in 0..10 {
            vector.insert(run_id, "embeddings", &format!("key_{}", i), &seeded_random_vector(384, i as u64), None).unwrap();
        }

        // Delete collection
        vector.delete_collection(run_id, "embeddings").unwrap();
    }

    test_db.reopen();

    let vector = test_db.vector();

    // Collection should not exist
    assert!(vector.get_collection(run_id, "embeddings").unwrap().is_none());
}

#[test]
fn test_recreate_deleted_collection() {
    let mut test_db = TestDb::new_strict();
    let run_id = test_db.run_id;

    {
        let vector = test_db.vector();

        // Create collection
        vector.create_collection(run_id, "embeddings", config_minilm()).unwrap();
        vector.insert(run_id, "embeddings", "key1", &random_vector(384), None).unwrap();

        // Delete
        vector.delete_collection(run_id, "embeddings").unwrap();

        // Recreate with different config
        vector.create_collection(run_id, "embeddings", config_small()).unwrap();
        vector.insert(run_id, "embeddings", "new_key", &random_vector(3), None).unwrap();
    }

    test_db.reopen();

    let vector = test_db.vector();

    // New collection should exist with new config
    let info = vector.get_collection(run_id, "embeddings").unwrap().unwrap();
    assert_eq!(info.value.config.dimension, 3);
    assert!(vector.get(run_id, "embeddings", "new_key").unwrap().is_some());
    assert!(vector.get(run_id, "embeddings", "key1").unwrap().is_none());
}

#[test]
fn test_delete_multiple_collections() {
    let mut test_db = TestDb::new_strict();
    let run_id = test_db.run_id;

    {
        let vector = test_db.vector();

        // Create multiple collections
        for i in 0..6 {
            vector.create_collection(run_id, &format!("col_{}", i), config_minilm()).unwrap();
        }

        // Delete first 3
        for i in 0..3 {
            vector.delete_collection(run_id, &format!("col_{}", i)).unwrap();
        }
    }

    test_db.reopen();

    let vector = test_db.vector();
    let collections = vector.list_collections(run_id).unwrap();

    // First 3 should be deleted
    for i in 0..3 {
        assert!(!collections.iter().any(|c| c.name == format!("col_{}", i)));
    }

    // Last 3 should exist
    for i in 3..6 {
        assert!(collections.iter().any(|c| c.name == format!("col_{}", i)));
    }

    assert_eq!(collections.len(), 3);
}

#[test]
fn test_delete_empty_collection() {
    let mut test_db = TestDb::new_strict();
    let run_id = test_db.run_id;

    {
        let vector = test_db.vector();

        // Create empty collection
        vector.create_collection(run_id, "empty_col", config_minilm()).unwrap();

        // Delete it
        vector.delete_collection(run_id, "empty_col").unwrap();
    }

    test_db.reopen();

    let vector = test_db.vector();
    assert!(vector.get_collection(run_id, "empty_col").unwrap().is_none());
}
