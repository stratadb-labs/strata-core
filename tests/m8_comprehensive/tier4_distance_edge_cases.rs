//! Tier 4: Distance Metric Edge Cases Tests

use crate::test_utils::*;
use strata_primitives::vector::DistanceMetric;

/// Test zero vector with cosine (should handle gracefully)
#[test]
fn test_zero_vector_cosine() {
    let zero = vec![0.0, 0.0, 0.0];
    let non_zero = vec![1.0, 2.0, 3.0];

    let score = cosine_similarity(&zero, &non_zero);
    // Implementation should return 0 for undefined case
    assert!(score.is_finite(), "Cosine with zero vector should be finite");
}

/// Test very small values
#[test]
fn test_very_small_values() {
    let a = vec![1e-38, 1e-38, 1e-38];
    let b = vec![1e-38, 1e-38, 1e-38];

    let score = cosine_similarity(&a, &b);
    assert!(score.is_finite(), "Should handle very small values");
}

/// Test very large values
#[test]
fn test_very_large_values() {
    let a = vec![1e10, 1e10, 1e10];
    let b = vec![1e10, 1e10, 1e10];

    let score = cosine_similarity(&a, &b);
    assert!(score.is_finite(), "Should handle large values");
    assert!((score - 1.0).abs() < 0.01, "Identical large vectors should be similar");
}

/// Test mixed positive and negative values
#[test]
fn test_mixed_positive_negative() {
    let a = vec![1.0, -1.0, 1.0, -1.0];
    let b = vec![-1.0, 1.0, -1.0, 1.0];

    let score = cosine_similarity(&a, &b);
    assert!(
        (score - (-1.0)).abs() < 1e-6,
        "Opposite alternating vectors should have similarity -1"
    );
}

/// Test single dimension vectors
#[test]
fn test_single_dimension() {
    let test_db = TestDb::new();
    let vector = test_db.vector();

    // While unusual, 1-D should work
    vector
        .create_collection(test_db.run_id, "embeddings", config_custom(1, DistanceMetric::Cosine))
        .unwrap();

    vector.insert(test_db.run_id, "embeddings", "pos", &vec![1.0], None).unwrap();
    vector.insert(test_db.run_id, "embeddings", "neg", &vec![-1.0], None).unwrap();

    let results = vector.search(test_db.run_id, "embeddings", &vec![1.0], 2, None).unwrap();

    assert_eq!(results[0].key, "pos", "Positive should match positive");
    assert_eq!(results[1].key, "neg", "Negative should be second");
}

/// Test nearly zero magnitude
#[test]
fn test_nearly_zero_magnitude() {
    let a = vec![1e-20, 0.0, 0.0];
    let b = vec![1.0, 0.0, 0.0];

    let score = cosine_similarity(&a, &b);
    // Both point in same direction, should be similar
    assert!(score.is_finite());
}

/// Test all zeros vs all zeros
#[test]
fn test_zero_vs_zero() {
    let zero1 = vec![0.0, 0.0, 0.0];
    let zero2 = vec![0.0, 0.0, 0.0];

    let score = cosine_similarity(&zero1, &zero2);
    assert!(score.is_finite(), "Zero vs zero should be finite (not NaN)");
}

/// Test negative squared distance doesn't happen
#[test]
fn test_euclidean_non_negative() {
    let a = random_vector(100);
    let b = random_vector(100);

    let dist = euclidean_distance(&a, &b);
    assert!(dist >= 0.0, "Euclidean distance should be non-negative");
}

/// Test edge case in vector store search
#[test]
fn test_edge_cases_vectorstore() {
    let test_db = TestDb::new();
    let vector = test_db.vector();

    vector
        .create_collection(test_db.run_id, "embeddings", config_custom(3, DistanceMetric::Cosine))
        .unwrap();

    // Insert vectors with edge case values
    vector.insert(test_db.run_id, "embeddings", "normal", &vec![1.0, 2.0, 3.0], None).unwrap();
    vector.insert(test_db.run_id, "embeddings", "small", &vec![0.001, 0.001, 0.001], None).unwrap();
    vector.insert(test_db.run_id, "embeddings", "large", &vec![1000.0, 2000.0, 3000.0], None).unwrap();

    // All should be searchable
    let query = vec![1.0, 2.0, 3.0];
    let results = vector.search(test_db.run_id, "embeddings", &query, 3, None).unwrap();

    assert_eq!(results.len(), 3, "All vectors should be returned");

    // All scores should be finite
    for result in &results {
        assert!(result.score.is_finite(), "Score should be finite for {}", result.key);
    }
}

/// Test precision at boundaries
#[test]
fn test_precision_boundaries() {
    // Test that we don't lose precision near 1.0 for cosine
    let a = vec![1.0, 0.0, 0.0];
    let b = vec![0.99999, 0.00001, 0.0]; // Very close to a

    let score = cosine_similarity(&a, &b);
    assert!(score > 0.99999, "Very similar vectors should have score > 0.99999");
}
