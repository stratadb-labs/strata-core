//! Tier 4: Dot Product Correctness Tests

use crate::test_utils::*;
use strata_primitives::vector::DistanceMetric;

/// Test dot product calculation
#[test]
fn test_dotproduct_calculation() {
    let a = vec![1.0, 2.0, 3.0];
    let b = vec![4.0, 5.0, 6.0];
    let score = dot_product(&a, &b);
    // 1*4 + 2*5 + 3*6 = 4 + 10 + 18 = 32
    assert!(
        (score - 32.0).abs() < 1e-6,
        "Dot product should be 32, got {}",
        score
    );
}

/// Test dot product of orthogonal vectors
#[test]
fn test_dotproduct_orthogonal() {
    let a = vec![1.0, 0.0];
    let b = vec![0.0, 1.0];
    let score = dot_product(&a, &b);
    assert!(score.abs() < 1e-6, "Orthogonal vectors should have dot product 0, got {}", score);
}

/// Test dot product of unit vectors
#[test]
fn test_dotproduct_unit_vectors() {
    let a = vec![1.0, 0.0, 0.0];
    let b = vec![1.0, 0.0, 0.0];
    let score = dot_product(&a, &b);
    assert!(
        (score - 1.0).abs() < 1e-6,
        "Parallel unit vectors should have dot product 1, got {}",
        score
    );
}

/// Test dot product of opposite vectors
#[test]
fn test_dotproduct_opposite() {
    let a = vec![1.0, 2.0, 3.0];
    let b = vec![-1.0, -2.0, -3.0];
    let score = dot_product(&a, &b);
    // 1*(-1) + 2*(-2) + 3*(-3) = -1 - 4 - 9 = -14
    assert!(
        (score - (-14.0)).abs() < 1e-6,
        "Opposite vectors dot product should be -14, got {}",
        score
    );
}

/// Test dot product through VectorStore search
#[test]
fn test_dotproduct_via_vectorstore() {
    let test_db = TestDb::new();
    let vector = test_db.vector();

    vector
        .create_collection(
            test_db.run_id,
            "embeddings",
            config_custom(3, DistanceMetric::DotProduct),
        )
        .unwrap();

    let query = vec![1.0, 1.0, 1.0];

    vector.insert(test_db.run_id, "embeddings", "aligned", &vec![1.0, 1.0, 1.0], None).unwrap();
    vector.insert(test_db.run_id, "embeddings", "partial", &vec![1.0, 0.0, 0.0], None).unwrap();
    vector.insert(test_db.run_id, "embeddings", "negative", &vec![-1.0, -1.0, -1.0], None).unwrap();

    let results = vector.search(test_db.run_id, "embeddings", &query, 3, None).unwrap();

    assert_eq!(results[0].key, "aligned", "Aligned should have highest dot product");
    assert_eq!(results[2].key, "negative", "Negative should have lowest dot product");
}

/// Test dot product is symmetric
#[test]
fn test_dotproduct_symmetric() {
    let a = vec![1.0, 2.0, 3.0, 4.0];
    let b = vec![5.0, 6.0, 7.0, 8.0];

    let dot_ab = dot_product(&a, &b);
    let dot_ba = dot_product(&b, &a);

    assert!(
        (dot_ab - dot_ba).abs() < 1e-6,
        "Dot product should be symmetric: {} vs {}",
        dot_ab,
        dot_ba
    );
}

/// Test dot product with scaled vectors
#[test]
fn test_dotproduct_scaled() {
    let a = vec![1.0, 2.0, 3.0];
    let b = vec![2.0, 4.0, 6.0]; // 2x scale of a

    let self_dot = dot_product(&a, &a); // 1 + 4 + 9 = 14
    let scaled_dot = dot_product(&a, &b); // 2 + 8 + 18 = 28 = 2 * 14

    assert!(
        (scaled_dot - 2.0 * self_dot).abs() < 1e-6,
        "Scaled vector dot product should be 2x: {} vs {}",
        scaled_dot,
        2.0 * self_dot
    );
}

/// Test dot product ordering in search
#[test]
fn test_dotproduct_search_ordering() {
    let test_db = TestDb::new();
    let vector = test_db.vector();

    vector
        .create_collection(
            test_db.run_id,
            "embeddings",
            config_custom(3, DistanceMetric::DotProduct),
        )
        .unwrap();

    let query = vec![1.0, 0.0, 0.0];

    // Insert vectors with different x components
    for i in 0..10 {
        let v = vec![i as f32 - 5.0, 0.0, 0.0]; // -5, -4, ..., 4
        vector.insert(test_db.run_id, "embeddings", &format!("v_{}", i), &v, None).unwrap();
    }

    let results = vector.search(test_db.run_id, "embeddings", &query, 10, None).unwrap();

    // Highest x component (4) should be first
    assert_eq!(results[0].key, "v_9", "Highest dot product should be first");
    // Lowest x component (-5) should be last
    assert_eq!(results[9].key, "v_0", "Lowest dot product should be last");
}
