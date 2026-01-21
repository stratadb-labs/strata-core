//! Tier 5: Collection Config Persistence Tests

use crate::test_utils::*;
use strata_primitives::vector::DistanceMetric;

#[test]
fn test_collection_config_survives_restart() {
    let mut test_db = TestDb::new_strict();
    let run_id = test_db.run_id;

    {
        let vector = test_db.vector();
        vector.create_collection(run_id, "embeddings", config_custom(512, DistanceMetric::DotProduct)).unwrap();
    }

    test_db.reopen();

    let info = test_db.vector().get_collection(run_id, "embeddings").unwrap().unwrap();

    assert_eq!(info.value.config.dimension, 512);
    assert_eq!(info.value.config.metric, DistanceMetric::DotProduct);
}

#[test]
fn test_multiple_collection_configs_persist() {
    let mut test_db = TestDb::new_strict();
    let run_id = test_db.run_id;

    {
        let vector = test_db.vector();
        vector.create_collection(run_id, "cosine", config_custom(384, DistanceMetric::Cosine)).unwrap();
        vector.create_collection(run_id, "euclidean", config_custom(768, DistanceMetric::Euclidean)).unwrap();
        vector.create_collection(run_id, "dot", config_custom(1536, DistanceMetric::DotProduct)).unwrap();
    }

    test_db.reopen();

    let vector = test_db.vector();

    let cosine = vector.get_collection(run_id, "cosine").unwrap().unwrap();
    assert_eq!(cosine.value.config.dimension, 384);
    assert_eq!(cosine.value.config.metric, DistanceMetric::Cosine);

    let euclidean = vector.get_collection(run_id, "euclidean").unwrap().unwrap();
    assert_eq!(euclidean.value.config.dimension, 768);
    assert_eq!(euclidean.value.config.metric, DistanceMetric::Euclidean);

    let dot = vector.get_collection(run_id, "dot").unwrap().unwrap();
    assert_eq!(dot.value.config.dimension, 1536);
    assert_eq!(dot.value.config.metric, DistanceMetric::DotProduct);
}
