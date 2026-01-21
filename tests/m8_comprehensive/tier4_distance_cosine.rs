//! Tier 4: Cosine Similarity Correctness Tests

use crate::test_utils::*;
use strata_primitives::vector::DistanceMetric;

/// Test cosine similarity for identical vectors
#[test]
fn test_cosine_identical_vectors() {
    let a = vec![1.0, 2.0, 3.0];
    let score = cosine_similarity(&a, &a);
    assert!(
        (score - 1.0).abs() < 1e-6,
        "Identical vectors should have similarity 1.0, got {}",
        score
    );
}

/// Test cosine similarity for orthogonal vectors
#[test]
fn test_cosine_orthogonal_vectors() {
    let a = vec![1.0, 0.0, 0.0];
    let b = vec![0.0, 1.0, 0.0];
    let score = cosine_similarity(&a, &b);
    assert!(score.abs() < 1e-6, "Orthogonal vectors should have similarity 0, got {}", score);
}

/// Test cosine similarity for opposite vectors
#[test]
fn test_cosine_opposite_vectors() {
    let a = vec![1.0, 2.0, 3.0];
    let b = vec![-1.0, -2.0, -3.0];
    let score = cosine_similarity(&a, &b);
    assert!(
        (score - (-1.0)).abs() < 1e-6,
        "Opposite vectors should have similarity -1.0, got {}",
        score
    );
}

/// Test cosine with scaled vectors (same direction, different magnitude)
#[test]
fn test_cosine_scaled_vectors() {
    let a = vec![1.0, 2.0, 3.0];
    let b = vec![2.0, 4.0, 6.0]; // 2x scale
    let score = cosine_similarity(&a, &b);
    assert!(
        (score - 1.0).abs() < 1e-6,
        "Scaled vectors should have same cosine similarity, got {}",
        score
    );
}

/// Test cosine through VectorStore search
#[test]
fn test_cosine_via_vectorstore() {
    let test_db = TestDb::new();
    let vector = test_db.vector();

    vector
        .create_collection(test_db.run_id, "embeddings", config_custom(3, DistanceMetric::Cosine))
        .unwrap();

    let query = vec![1.0, 0.0, 0.0];

    // Insert vectors at known angles
    vector.insert(test_db.run_id, "embeddings", "identical", &vec![1.0, 0.0, 0.0], None).unwrap();
    vector.insert(test_db.run_id, "embeddings", "orthogonal", &vec![0.0, 1.0, 0.0], None).unwrap();
    vector.insert(test_db.run_id, "embeddings", "opposite", &vec![-1.0, 0.0, 0.0], None).unwrap();

    let results = vector.search(test_db.run_id, "embeddings", &query, 3, None).unwrap();

    // Verify ordering
    assert_eq!(results[0].key, "identical", "Identical should be most similar");
    assert_eq!(results[2].key, "opposite", "Opposite should be least similar");

    // Verify scores are reasonable
    assert!(results[0].score > 0.99, "Identical score should be ~1.0");
}

/// Test cosine with 45-degree angle
#[test]
fn test_cosine_45_degree() {
    let a = vec![1.0, 0.0];
    let b = vec![1.0, 1.0]; // 45 degrees from a
    let score = cosine_similarity(&a, &b);
    let expected = 1.0 / 2.0_f32.sqrt(); // cos(45°) ≈ 0.707

    assert!(
        (score - expected).abs() < 1e-5,
        "45° angle should have cos ≈ 0.707, got {}",
        score
    );
}

/// Test cosine similarity is symmetric
#[test]
fn test_cosine_symmetric() {
    let a = vec![1.0, 2.0, 3.0, 4.0];
    let b = vec![4.0, 3.0, 2.0, 1.0];

    let score_ab = cosine_similarity(&a, &b);
    let score_ba = cosine_similarity(&b, &a);

    assert!(
        (score_ab - score_ba).abs() < 1e-6,
        "Cosine should be symmetric: {} vs {}",
        score_ab,
        score_ba
    );
}

/// Test cosine with high-dimensional vectors
#[test]
fn test_cosine_high_dimensional() {
    let test_db = TestDb::new();
    let vector = test_db.vector();

    vector
        .create_collection(test_db.run_id, "embeddings", config_minilm())
        .unwrap();

    // Create vectors with known relationship
    let mut a = vec![0.0f32; 384];
    let mut b = vec![0.0f32; 384];
    a[0] = 1.0;
    b[0] = 1.0;
    b[1] = 1.0; // b has additional component

    let expected_similarity = 1.0 / 2.0_f32.sqrt(); // ~0.707

    vector.insert(test_db.run_id, "embeddings", "a", &a, None).unwrap();
    vector.insert(test_db.run_id, "embeddings", "b", &b, None).unwrap();

    let results = vector.search(test_db.run_id, "embeddings", &a, 2, None).unwrap();

    // 'a' should match itself perfectly
    assert_eq!(results[0].key, "a");
    assert!(results[0].score > 0.99);

    // 'b' should be second with expected similarity
    assert_eq!(results[1].key, "b");
    assert!(
        (results[1].score - expected_similarity).abs() < 0.01,
        "Expected ~{}, got {}",
        expected_similarity,
        results[1].score
    );
}
