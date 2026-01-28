//! Tier 6: Heap Delete Tests

use crate::common::*;

#[test]
fn test_heap_delete() {
    let test_db = TestDb::new();
    let vector = test_db.vector();

    vector.create_collection(test_db.run_id, "embeddings", config_minilm()).unwrap();
    vector.insert(test_db.run_id, "embeddings", "key1", &random_vector(384), None).unwrap();

    let deleted = vector.delete(test_db.run_id, "embeddings", "key1").unwrap();
    assert!(deleted);
    assert!(vector.get(test_db.run_id, "embeddings", "key1").unwrap().is_none());
}

#[test]
fn test_heap_delete_nonexistent() {
    let test_db = TestDb::new();
    let vector = test_db.vector();

    vector.create_collection(test_db.run_id, "embeddings", config_minilm()).unwrap();

    let deleted = vector.delete(test_db.run_id, "embeddings", "nonexistent").unwrap();
    assert!(!deleted);
}

#[test]
fn test_heap_delete_twice() {
    let test_db = TestDb::new();
    let vector = test_db.vector();

    vector.create_collection(test_db.run_id, "embeddings", config_minilm()).unwrap();
    vector.insert(test_db.run_id, "embeddings", "key1", &random_vector(384), None).unwrap();

    assert!(vector.delete(test_db.run_id, "embeddings", "key1").unwrap());
    assert!(!vector.delete(test_db.run_id, "embeddings", "key1").unwrap());
}
