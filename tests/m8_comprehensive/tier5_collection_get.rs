//! Tier 5: Collection Get Tests

use crate::test_utils::*;
use strata_primitives::vector::DistanceMetric;

#[test]
fn test_get_collection_info() {
    let test_db = TestDb::new();
    let vector = test_db.vector();

    vector.create_collection(test_db.run_id, "embeddings", config_custom(768, DistanceMetric::Euclidean)).unwrap();

    for i in 0..10 {
        vector.insert(test_db.run_id, "embeddings", &format!("key_{}", i), &random_vector(768), None).unwrap();
    }

    let info = vector.get_collection(test_db.run_id, "embeddings").unwrap().unwrap();

    assert_eq!(info.value.name, "embeddings");
    assert_eq!(info.value.config.dimension, 768);
    assert_eq!(info.value.config.metric, DistanceMetric::Euclidean);
    assert_eq!(info.value.count, 10);
}

#[test]
fn test_get_nonexistent_collection() {
    let test_db = TestDb::new();
    let vector = test_db.vector();

    let info = vector.get_collection(test_db.run_id, "nonexistent").unwrap();
    assert!(info.is_none());
}

#[test]
fn test_get_collection_count_updates() {
    let test_db = TestDb::new();
    let vector = test_db.vector();

    vector.create_collection(test_db.run_id, "embeddings", config_minilm()).unwrap();

    assert_eq!(vector.get_collection(test_db.run_id, "embeddings").unwrap().unwrap().value.count, 0);

    vector.insert(test_db.run_id, "embeddings", "key1", &random_vector(384), None).unwrap();
    assert_eq!(vector.get_collection(test_db.run_id, "embeddings").unwrap().unwrap().value.count, 1);

    vector.delete(test_db.run_id, "embeddings", "key1").unwrap();
    assert_eq!(vector.get_collection(test_db.run_id, "embeddings").unwrap().unwrap().value.count, 0);
}
