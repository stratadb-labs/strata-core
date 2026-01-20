//! Tier 6: Heap Free Slot Reuse Tests

use crate::test_utils::*;

#[test]
fn test_heap_slot_reuse() {
    let test_db = TestDb::new();
    let vector = test_db.vector();

    vector.create_collection(test_db.run_id, "embeddings", config_minilm()).unwrap();

    // Insert vectors
    for i in 0..10 {
        vector.insert(test_db.run_id, "embeddings", &format!("key_{}", i), &random_vector(384), None).unwrap();
    }

    // Delete half
    for i in 0..5 {
        vector.delete(test_db.run_id, "embeddings", &format!("key_{}", i)).unwrap();
    }

    // Insert new vectors - storage slots may be reused (but IDs are not!)
    for i in 10..15 {
        vector.insert(test_db.run_id, "embeddings", &format!("key_{}", i), &random_vector(384), None).unwrap();
    }

    // Count should be 10
    assert_eq!(vector.count(test_db.run_id, "embeddings").unwrap(), 10);
}

#[test]
fn test_slot_reuse_not_id_reuse() {
    let test_db = TestDb::new();
    let vector = test_db.vector();

    vector.create_collection(test_db.run_id, "embeddings", config_minilm()).unwrap();

    // Insert and track ID
    vector.insert(test_db.run_id, "embeddings", "key1", &random_vector(384), None).unwrap();
    let id1 = vector.get(test_db.run_id, "embeddings", "key1").unwrap().unwrap().value.vector_id();

    // Delete
    vector.delete(test_db.run_id, "embeddings", "key1").unwrap();

    // Insert same key again
    vector.insert(test_db.run_id, "embeddings", "key1", &random_vector(384), None).unwrap();
    let id2 = vector.get(test_db.run_id, "embeddings", "key1").unwrap().unwrap().value.vector_id();

    // New ID should be different (IDs are never reused, even if slots are)
    assert!(id2.as_u64() > id1.as_u64(), "VectorId should not be reused");
}

#[test]
fn test_many_delete_insert_cycles() {
    let test_db = TestDb::new();
    let vector = test_db.vector();

    vector.create_collection(test_db.run_id, "embeddings", config_minilm()).unwrap();

    for cycle in 0..50 {
        let key = format!("key_{}", cycle);
        vector.insert(test_db.run_id, "embeddings", &key, &random_vector(384), None).unwrap();
        vector.delete(test_db.run_id, "embeddings", &key).unwrap();
    }

    // Should be empty
    assert_eq!(vector.count(test_db.run_id, "embeddings").unwrap(), 0);

    // Can still insert
    vector.insert(test_db.run_id, "embeddings", "final", &random_vector(384), None).unwrap();
    assert_eq!(vector.count(test_db.run_id, "embeddings").unwrap(), 1);
}
