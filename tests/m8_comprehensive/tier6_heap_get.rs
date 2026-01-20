//! Tier 6: Heap Get Tests

use crate::test_utils::*;
use serde_json::json;

#[test]
fn test_heap_get_existing() {
    let test_db = TestDb::new();
    let vector = test_db.vector();

    vector.create_collection(test_db.run_id, "embeddings", config_minilm()).unwrap();

    let embedding = seeded_random_vector(384, 123);
    let metadata = json!({"type": "document"});

    vector.insert(test_db.run_id, "embeddings", "key1", &embedding, Some(metadata.clone())).unwrap();

    let entry = vector.get(test_db.run_id, "embeddings", "key1").unwrap().unwrap();
    assert_eq!(entry.value.key, "key1");
    assert_eq!(entry.value.embedding, embedding);
    assert_eq!(entry.value.metadata, Some(metadata));
}

#[test]
fn test_heap_get_nonexistent() {
    let test_db = TestDb::new();
    let vector = test_db.vector();

    vector.create_collection(test_db.run_id, "embeddings", config_minilm()).unwrap();

    let entry = vector.get(test_db.run_id, "embeddings", "nonexistent").unwrap();
    assert!(entry.is_none());
}

#[test]
fn test_heap_get_after_delete() {
    let test_db = TestDb::new();
    let vector = test_db.vector();

    vector.create_collection(test_db.run_id, "embeddings", config_minilm()).unwrap();
    vector.insert(test_db.run_id, "embeddings", "key1", &random_vector(384), None).unwrap();
    vector.delete(test_db.run_id, "embeddings", "key1").unwrap();

    let entry = vector.get(test_db.run_id, "embeddings", "key1").unwrap();
    assert!(entry.is_none());
}
