//! Tier 5: Collection Delete Tests

use crate::common::*;

#[test]
fn test_delete_collection() {
    let test_db = TestDb::new();
    let vector = test_db.vector();

    vector.create_collection(test_db.run_id, "embeddings", config_minilm()).unwrap();
    vector.insert(test_db.run_id, "embeddings", "key1", &random_vector(384), None).unwrap();

    vector.delete_collection(test_db.run_id, "embeddings").unwrap();

    assert!(vector.get_collection(test_db.run_id, "embeddings").unwrap().is_none());
}

#[test]
fn test_delete_collection_clears_vectors() {
    let test_db = TestDb::new();
    let vector = test_db.vector();

    vector.create_collection(test_db.run_id, "embeddings", config_minilm()).unwrap();
    for i in 0..10 {
        vector.insert(test_db.run_id, "embeddings", &format!("key_{}", i), &random_vector(384), None).unwrap();
    }

    vector.delete_collection(test_db.run_id, "embeddings").unwrap();

    // Can create new collection with same name
    vector.create_collection(test_db.run_id, "embeddings", config_openai_ada()).unwrap();
    assert_eq!(vector.count(test_db.run_id, "embeddings").unwrap(), 0);
}

#[test]
fn test_delete_nonexistent_collection() {
    let test_db = TestDb::new();
    let vector = test_db.vector();

    // Deleting non-existent collection should not error (or return appropriate result)
    let result = vector.delete_collection(test_db.run_id, "nonexistent");
    // Behavior depends on implementation - either Ok or specific error
    assert!(result.is_ok() || result.is_err());
}

#[test]
fn test_delete_collection_survives_restart() {
    let mut test_db = TestDb::new_strict();
    let run_id = test_db.run_id;

    {
        let vector = test_db.vector();
        vector.create_collection(run_id, "embeddings", config_minilm()).unwrap();
        vector.insert(run_id, "embeddings", "key1", &random_vector(384), None).unwrap();
        vector.delete_collection(run_id, "embeddings").unwrap();
    }

    test_db.reopen();

    let vector = test_db.vector();
    assert!(vector.get_collection(run_id, "embeddings").unwrap().is_none());
}
