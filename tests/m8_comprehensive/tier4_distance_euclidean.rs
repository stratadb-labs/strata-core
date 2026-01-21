//! Tier 4: Euclidean Distance Correctness Tests

use crate::test_utils::*;
use strata_primitives::vector::DistanceMetric;

/// Test Euclidean distance for identical vectors
#[test]
fn test_euclidean_identical_vectors() {
    let a = vec![1.0, 2.0, 3.0];
    let score = euclidean_similarity(&a, &a);
    // Score = 1 / (1 + 0) = 1.0
    assert!(
        (score - 1.0).abs() < 1e-6,
        "Identical vectors should have similarity 1.0, got {}",
        score
    );
}

/// Test Euclidean distance for unit distance
#[test]
fn test_euclidean_unit_distance() {
    let a = vec![0.0, 0.0, 0.0];
    let b = vec![1.0, 0.0, 0.0];
    let score = euclidean_similarity(&a, &b);
    // Distance = 1.0, Score = 1 / (1 + 1) = 0.5
    assert!(
        (score - 0.5).abs() < 1e-6,
        "Unit distance should have similarity 0.5, got {}",
        score
    );
}

/// Test Euclidean distance for large distance
#[test]
fn test_euclidean_large_distance() {
    let a = vec![0.0, 0.0, 0.0];
    let b = vec![10.0, 0.0, 0.0];
    let score = euclidean_similarity(&a, &b);
    // Distance = 10.0, Score = 1 / (1 + 10) ≈ 0.0909
    let expected = 1.0 / 11.0;
    assert!(
        (score - expected).abs() < 1e-6,
        "Large distance should have similarity ~{}, got {}",
        expected,
        score
    );
}

/// Test Euclidean: higher score = closer distance
#[test]
fn test_euclidean_normalized_higher_is_closer() {
    let origin = vec![0.0, 0.0, 0.0];
    let close = vec![1.0, 0.0, 0.0];
    let far = vec![10.0, 0.0, 0.0];

    let score_close = euclidean_similarity(&origin, &close);
    let score_far = euclidean_similarity(&origin, &far);

    assert!(
        score_close > score_far,
        "Closer vector should have higher normalized score: {} vs {}",
        score_close,
        score_far
    );
}

/// Test Euclidean through VectorStore search
#[test]
fn test_euclidean_via_vectorstore() {
    let test_db = TestDb::new();
    let vector = test_db.vector();

    vector
        .create_collection(
            test_db.run_id,
            "embeddings",
            config_custom(3, DistanceMetric::Euclidean),
        )
        .unwrap();

    let query = vec![0.0, 0.0, 0.0];

    vector.insert(test_db.run_id, "embeddings", "close", &vec![0.1, 0.0, 0.0], None).unwrap();
    vector.insert(test_db.run_id, "embeddings", "medium", &vec![1.0, 0.0, 0.0], None).unwrap();
    vector.insert(test_db.run_id, "embeddings", "far", &vec![10.0, 0.0, 0.0], None).unwrap();

    let results = vector.search(test_db.run_id, "embeddings", &query, 3, None).unwrap();

    assert_eq!(results[0].key, "close", "Closest should be first");
    assert_eq!(results[1].key, "medium", "Medium should be second");
    assert_eq!(results[2].key, "far", "Farthest should be last");
}

/// Test Euclidean distance with diagonal
#[test]
fn test_euclidean_diagonal() {
    let a = vec![0.0, 0.0];
    let b = vec![1.0, 1.0];
    let dist = euclidean_distance(&a, &b);
    let expected = 2.0_f32.sqrt(); // √2 ≈ 1.414

    assert!(
        (dist - expected).abs() < 1e-5,
        "Diagonal distance should be √2 ≈ 1.414, got {}",
        dist
    );
}

/// Test Euclidean distance is symmetric
#[test]
fn test_euclidean_symmetric() {
    let a = vec![1.0, 2.0, 3.0, 4.0];
    let b = vec![4.0, 3.0, 2.0, 1.0];

    let dist_ab = euclidean_distance(&a, &b);
    let dist_ba = euclidean_distance(&b, &a);

    assert!(
        (dist_ab - dist_ba).abs() < 1e-6,
        "Euclidean distance should be symmetric: {} vs {}",
        dist_ab,
        dist_ba
    );
}

/// Test Euclidean distance triangle inequality
#[test]
fn test_euclidean_triangle_inequality() {
    let a = vec![0.0, 0.0];
    let b = vec![1.0, 0.0];
    let c = vec![1.0, 1.0];

    let ab = euclidean_distance(&a, &b);
    let bc = euclidean_distance(&b, &c);
    let ac = euclidean_distance(&a, &c);

    // Triangle inequality: d(a,c) <= d(a,b) + d(b,c)
    assert!(
        ac <= ab + bc + 1e-6, // Small epsilon for floating point
        "Triangle inequality violated: {} > {} + {}",
        ac,
        ab,
        bc
    );
}
