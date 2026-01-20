//! Tier 6: Heap Insert Tests

use crate::test_utils::*;

#[test]
fn test_heap_insert_and_get() {
    let test_db = TestDb::new();
    let vector = test_db.vector();

    vector.create_collection(test_db.run_id, "embeddings", config_minilm()).unwrap();

    let embedding = seeded_random_vector(384, 42);
    vector.insert(test_db.run_id, "embeddings", "key1", &embedding, None).unwrap();

    let entry = vector.get(test_db.run_id, "embeddings", "key1").unwrap().unwrap();
    assert_eq!(entry.value.embedding, embedding);
}

#[test]
fn test_heap_upsert_overwrites() {
    let test_db = TestDb::new();
    let vector = test_db.vector();

    vector.create_collection(test_db.run_id, "embeddings", config_minilm()).unwrap();

    let embedding1 = seeded_random_vector(384, 1);
    let embedding2 = seeded_random_vector(384, 2);

    vector.insert(test_db.run_id, "embeddings", "key1", &embedding1, None).unwrap();
    vector.insert(test_db.run_id, "embeddings", "key1", &embedding2, None).unwrap();

    let entry = vector.get(test_db.run_id, "embeddings", "key1").unwrap().unwrap();
    assert_eq!(entry.value.embedding, embedding2);
    assert_eq!(vector.count(test_db.run_id, "embeddings").unwrap(), 1);
}

#[test]
fn test_heap_insert_many() {
    let test_db = TestDb::new();
    let vector = test_db.vector();

    vector.create_collection(test_db.run_id, "embeddings", config_minilm()).unwrap();

    for i in 0..100 {
        vector.insert(test_db.run_id, "embeddings", &format!("key_{}", i), &seeded_random_vector(384, i as u64), None).unwrap();
    }

    assert_eq!(vector.count(test_db.run_id, "embeddings").unwrap(), 100);
}
