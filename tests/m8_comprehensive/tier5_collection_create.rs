//! Tier 5: Collection Create Tests

use crate::test_utils::*;
use strata_primitives::vector::{DistanceMetric, VectorError};

#[test]
fn test_create_collection() {
    let test_db = TestDb::new();
    let vector = test_db.vector();

    let result = vector.create_collection(test_db.run_id, "embeddings", config_minilm());
    assert!(result.is_ok());

    let info = vector.get_collection(test_db.run_id, "embeddings").unwrap();
    assert!(info.is_some());
}

#[test]
fn test_create_duplicate_collection_fails() {
    let test_db = TestDb::new();
    let vector = test_db.vector();

    vector.create_collection(test_db.run_id, "embeddings", config_minilm()).unwrap();

    let result = vector.create_collection(test_db.run_id, "embeddings", config_minilm());
    assert!(matches!(result, Err(VectorError::CollectionAlreadyExists { .. })));
}

#[test]
fn test_create_collection_various_configs() {
    let test_db = TestDb::new();
    let vector = test_db.vector();

    // MiniLM config
    vector.create_collection(test_db.run_id, "minilm", config_minilm()).unwrap();

    // OpenAI Ada config
    vector.create_collection(test_db.run_id, "ada", config_openai_ada()).unwrap();

    // Small config
    vector.create_collection(test_db.run_id, "small", config_small()).unwrap();

    // Custom Euclidean
    vector.create_collection(test_db.run_id, "euclidean", config_custom(256, DistanceMetric::Euclidean)).unwrap();

    // Custom DotProduct
    vector.create_collection(test_db.run_id, "dot", config_custom(512, DistanceMetric::DotProduct)).unwrap();

    assert!(vector.get_collection(test_db.run_id, "minilm").unwrap().is_some());
    assert!(vector.get_collection(test_db.run_id, "ada").unwrap().is_some());
    assert!(vector.get_collection(test_db.run_id, "small").unwrap().is_some());
    assert!(vector.get_collection(test_db.run_id, "euclidean").unwrap().is_some());
    assert!(vector.get_collection(test_db.run_id, "dot").unwrap().is_some());
}

#[test]
fn test_create_collection_survives_restart() {
    let mut test_db = TestDb::new_strict();
    let run_id = test_db.run_id;

    {
        let vector = test_db.vector();
        vector.create_collection(run_id, "embeddings", config_minilm()).unwrap();
    }

    test_db.reopen();

    let vector = test_db.vector();
    assert!(vector.get_collection(run_id, "embeddings").unwrap().is_some());
}
