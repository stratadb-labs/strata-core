//! S2: Metric Immutable Tests
//!
//! Invariant S2: Distance metric cannot change after creation.

use crate::test_utils::*;
use strata_primitives::vector::{DistanceMetric, VectorError};

/// Test that recreating collection with different metric fails
#[test]
fn test_s2_metric_cannot_change() {
    let test_db = TestDb::new();
    let vector = test_db.vector();

    // Create collection with Cosine metric
    vector
        .create_collection(test_db.run_id, "embeddings", config_custom(384, DistanceMetric::Cosine))
        .unwrap();

    // Cannot recreate with different metric
    let result = vector.create_collection(
        test_db.run_id,
        "embeddings",
        config_custom(384, DistanceMetric::Euclidean),
    );
    assert!(
        matches!(result, Err(VectorError::CollectionAlreadyExists { .. })),
        "S2 VIOLATED: Should not allow recreating with different metric, got {:?}",
        result
    );
}

/// Test that metric survives restart
#[test]
fn test_s2_metric_survives_restart() {
    let mut test_db = TestDb::new();
    let run_id = test_db.run_id;

    {
        let vector = test_db.vector();
        vector
            .create_collection(run_id, "embeddings", config_custom(384, DistanceMetric::Euclidean))
            .unwrap();
        vector
            .insert(run_id, "embeddings", "key1", &random_vector(384), None)
            .unwrap();
    }

    // Restart
    test_db.reopen();

    let vector = test_db.vector();
    let info = vector.get_collection(run_id, "embeddings").unwrap().unwrap();
    assert_eq!(
        info.value.config.metric,
        DistanceMetric::Euclidean,
        "S2 VIOLATED: Metric changed after restart"
    );
}

/// Test all three distance metrics are preserved
#[test]
fn test_s2_all_metrics_preserved() {
    let mut test_db = TestDb::new();
    let run_id = test_db.run_id;

    {
        let vector = test_db.vector();
        vector
            .create_collection(run_id, "cosine", config_custom(384, DistanceMetric::Cosine))
            .unwrap();
        vector
            .create_collection(run_id, "euclidean", config_custom(384, DistanceMetric::Euclidean))
            .unwrap();
        vector
            .create_collection(run_id, "dotproduct", config_custom(384, DistanceMetric::DotProduct))
            .unwrap();
    }

    // Restart
    test_db.reopen();

    let vector = test_db.vector();

    let cosine_info = vector.get_collection(run_id, "cosine").unwrap().unwrap();
    let euclidean_info = vector.get_collection(run_id, "euclidean").unwrap().unwrap();
    let dotproduct_info = vector.get_collection(run_id, "dotproduct").unwrap().unwrap();

    assert_eq!(cosine_info.value.config.metric, DistanceMetric::Cosine);
    assert_eq!(euclidean_info.value.config.metric, DistanceMetric::Euclidean);
    assert_eq!(dotproduct_info.value.config.metric, DistanceMetric::DotProduct);
}

/// Test that metric affects search results correctly
#[test]
fn test_s2_metric_affects_search_ranking() {
    let test_db = TestDb::new();
    let vector = test_db.vector();

    // Create two collections with different metrics
    vector
        .create_collection(test_db.run_id, "cosine", config_custom(3, DistanceMetric::Cosine))
        .unwrap();
    vector
        .create_collection(test_db.run_id, "euclidean", config_custom(3, DistanceMetric::Euclidean))
        .unwrap();

    // Insert same vectors in both
    // For cosine: same_dir has identical direction as query, diff_dir is slightly off
    // For euclidean: close_dist is closer in distance to query
    let query = vec![1.0, 0.0, 0.0];
    let same_dir = vec![2.0, 0.0, 0.0]; // Identical direction to query (cosine = 1.0)
    let close_dist = vec![1.0, 0.1, 0.0]; // Slightly different direction (cosine < 1.0) but closer euclidean

    vector
        .insert(test_db.run_id, "cosine", "same_dir", &same_dir, None)
        .unwrap();
    vector
        .insert(test_db.run_id, "cosine", "close_dist", &close_dist, None)
        .unwrap();
    vector
        .insert(test_db.run_id, "euclidean", "same_dir", &same_dir, None)
        .unwrap();
    vector
        .insert(test_db.run_id, "euclidean", "close_dist", &close_dist, None)
        .unwrap();

    // Cosine: same_dir should rank higher (identical direction)
    let cosine_results = vector
        .search(test_db.run_id, "cosine", &query, 2, None)
        .unwrap();
    assert_eq!(cosine_results[0].key, "same_dir", "Cosine should rank same direction higher");

    // Euclidean: close_dist should rank higher (smaller distance)
    let euclidean_results = vector
        .search(test_db.run_id, "euclidean", &query, 2, None)
        .unwrap();
    assert_eq!(
        euclidean_results[0].key, "close_dist",
        "Euclidean should rank closer point higher"
    );
}
