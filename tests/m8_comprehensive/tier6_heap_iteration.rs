//! Tier 6: Heap Iteration Tests

use crate::test_utils::*;

#[test]
fn test_heap_count_after_inserts() {
    let test_db = TestDb::new();
    let vector = test_db.vector();

    vector.create_collection(test_db.run_id, "embeddings", config_minilm()).unwrap();

    for i in 0..20 {
        vector.insert(test_db.run_id, "embeddings", &format!("key_{:02}", i), &random_vector(384), None).unwrap();
    }

    // Count should be 20
    assert_eq!(vector.count(test_db.run_id, "embeddings").unwrap(), 20);

    // All keys should exist
    for i in 0..20 {
        assert!(vector.get(test_db.run_id, "embeddings", &format!("key_{:02}", i)).unwrap().is_some());
    }
}

#[test]
fn test_heap_get_all_keys() {
    let test_db = TestDb::new();
    let vector = test_db.vector();

    vector.create_collection(test_db.run_id, "embeddings", config_minilm()).unwrap();

    for i in 0..10 {
        vector.insert(test_db.run_id, "embeddings", &format!("key_{}", i), &random_vector(384), None).unwrap();
    }

    assert_eq!(vector.count(test_db.run_id, "embeddings").unwrap(), 10);

    for i in 0..10 {
        assert!(vector.get(test_db.run_id, "embeddings", &format!("key_{}", i)).unwrap().is_some());
    }
}

#[test]
fn test_heap_iteration_after_deletes() {
    let test_db = TestDb::new();
    let vector = test_db.vector();

    vector.create_collection(test_db.run_id, "embeddings", config_minilm()).unwrap();

    for i in 0..20 {
        vector.insert(test_db.run_id, "embeddings", &format!("key_{}", i), &random_vector(384), None).unwrap();
    }

    for i in 0..10 {
        vector.delete(test_db.run_id, "embeddings", &format!("key_{}", i)).unwrap();
    }

    assert_eq!(vector.count(test_db.run_id, "embeddings").unwrap(), 10);

    // Deleted keys should not exist
    for i in 0..10 {
        assert!(vector.get(test_db.run_id, "embeddings", &format!("key_{}", i)).unwrap().is_none());
    }

    // Remaining keys should exist
    for i in 10..20 {
        assert!(vector.get(test_db.run_id, "embeddings", &format!("key_{}", i)).unwrap().is_some());
    }
}
