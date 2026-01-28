//! R9: No Implicit Normalization Tests
//!
//! Invariant R9: Embeddings stored as-is, no silent normalization.

use crate::common::*;

/// Test embedding stored verbatim
#[test]
fn test_r9_embedding_stored_verbatim() {
    let test_db = TestDb::new();
    let vector = test_db.vector();

    vector
        .create_collection(test_db.run_id, "embeddings", config_minilm())
        .unwrap();

    // Non-normalized vector
    let mut embedding = vec![0.0f32; 384];
    embedding[0] = 2.0;
    embedding[1] = 3.0;
    embedding[2] = 4.0;

    vector
        .insert(test_db.run_id, "embeddings", "key1", &embedding, None)
        .unwrap();

    // Retrieve and verify exact values
    let entry = vector
        .get(test_db.run_id, "embeddings", "key1")
        .unwrap()
        .unwrap();

    assert!(
        (entry.value.embedding[0] - 2.0).abs() < f32::EPSILON,
        "R9 VIOLATED: Embedding[0] was normalized: {}",
        entry.value.embedding[0]
    );
    assert!(
        (entry.value.embedding[1] - 3.0).abs() < f32::EPSILON,
        "R9 VIOLATED: Embedding[1] was normalized: {}",
        entry.value.embedding[1]
    );
    assert!(
        (entry.value.embedding[2] - 4.0).abs() < f32::EPSILON,
        "R9 VIOLATED: Embedding[2] was normalized: {}",
        entry.value.embedding[2]
    );
}

/// Test large magnitude embeddings stored correctly
#[test]
fn test_r9_large_magnitude_preserved() {
    let test_db = TestDb::new();
    let vector = test_db.vector();

    vector
        .create_collection(test_db.run_id, "embeddings", config_minilm())
        .unwrap();

    // Large magnitude vector
    let mut embedding = vec![0.0f32; 384];
    for i in 0..384 {
        embedding[i] = 100.0;
    }

    vector
        .insert(test_db.run_id, "embeddings", "key1", &embedding, None)
        .unwrap();

    let entry = vector
        .get(test_db.run_id, "embeddings", "key1")
        .unwrap()
        .unwrap();

    // All values should still be 100.0
    for (i, &val) in entry.value.embedding.iter().enumerate() {
        assert!(
            (val - 100.0).abs() < f32::EPSILON,
            "R9 VIOLATED: Embedding[{}] was normalized: {} (expected 100.0)",
            i,
            val
        );
    }
}

/// Test small magnitude embeddings stored correctly
#[test]
fn test_r9_small_magnitude_preserved() {
    let test_db = TestDb::new();
    let vector = test_db.vector();

    vector
        .create_collection(test_db.run_id, "embeddings", config_minilm())
        .unwrap();

    // Small magnitude vector
    let mut embedding = vec![0.0f32; 384];
    for i in 0..384 {
        embedding[i] = 0.001;
    }

    vector
        .insert(test_db.run_id, "embeddings", "key1", &embedding, None)
        .unwrap();

    let entry = vector
        .get(test_db.run_id, "embeddings", "key1")
        .unwrap()
        .unwrap();

    for (i, &val) in entry.value.embedding.iter().enumerate() {
        assert!(
            (val - 0.001).abs() < 1e-6,
            "R9 VIOLATED: Embedding[{}] was normalized: {} (expected 0.001)",
            i,
            val
        );
    }
}

/// Test negative values preserved
#[test]
fn test_r9_negative_values_preserved() {
    let test_db = TestDb::new();
    let vector = test_db.vector();

    vector
        .create_collection(test_db.run_id, "embeddings", config_minilm())
        .unwrap();

    let mut embedding = vec![0.0f32; 384];
    for i in 0..192 {
        embedding[i] = -1.0;
    }
    for i in 192..384 {
        embedding[i] = 1.0;
    }

    vector
        .insert(test_db.run_id, "embeddings", "key1", &embedding, None)
        .unwrap();

    let entry = vector
        .get(test_db.run_id, "embeddings", "key1")
        .unwrap()
        .unwrap();

    for i in 0..192 {
        assert!(
            (entry.value.embedding[i] - (-1.0)).abs() < f32::EPSILON,
            "R9 VIOLATED: Negative value not preserved at {}",
            i
        );
    }
    for i in 192..384 {
        assert!(
            (entry.value.embedding[i] - 1.0).abs() < f32::EPSILON,
            "R9 VIOLATED: Positive value not preserved at {}",
            i
        );
    }
}

/// Test embedding preserves across restart
#[test]
fn test_r9_embedding_preserved_across_restart() {
    let mut test_db = TestDb::new_strict();
    let run_id = test_db.run_id;

    let embedding: Vec<f32> = (0..384).map(|i| i as f32 * 0.123).collect();

    {
        let vector = test_db.vector();
        vector
            .create_collection(run_id, "embeddings", config_minilm())
            .unwrap();
        vector
            .insert(run_id, "embeddings", "key1", &embedding, None)
            .unwrap();
    }

    test_db.reopen();

    let vector = test_db.vector();
    let entry = vector.get(run_id, "embeddings", "key1").unwrap().unwrap();

    for (i, (&expected, &actual)) in embedding.iter().zip(entry.value.embedding.iter()).enumerate() {
        assert!(
            (expected - actual).abs() < 1e-6,
            "R9 VIOLATED: Embedding[{}] changed after restart: {} vs {}",
            i,
            expected,
            actual
        );
    }
}

/// Test unit vectors stored as-is
#[test]
fn test_r9_unit_vector_not_modified() {
    let test_db = TestDb::new();
    let vector = test_db.vector();

    vector
        .create_collection(test_db.run_id, "embeddings", config_minilm())
        .unwrap();

    // Already normalized vector
    let embedding = unit_vector(384);

    vector
        .insert(test_db.run_id, "embeddings", "key1", &embedding, None)
        .unwrap();

    let entry = vector
        .get(test_db.run_id, "embeddings", "key1")
        .unwrap()
        .unwrap();

    assert!(
        (entry.value.embedding[0] - 1.0).abs() < f32::EPSILON,
        "R9: Unit vector modified"
    );
    for i in 1..384 {
        assert!(
            entry.value.embedding[i].abs() < f32::EPSILON,
            "R9: Unit vector modified at {}",
            i
        );
    }
}
