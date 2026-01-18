//! Mega-Scale Vector Tests
//!
//! Tests with 1M+ vectors for scale validation.
//!
//! These tests are marked #[ignore] because they take significant time and memory.

use crate::test_utils::*;

/// Test 100K vectors in single collection.
///
/// This is a more reasonable scale test that can run in CI.
#[test]
fn test_100k_vectors() {
    let test_db = TestDb::new_in_memory();
    let vector = test_db.vector();
    let run_id = test_db.run_id;

    vector.create_collection(run_id, "scale_100k", config_small()).expect("create");

    // Insert 100K vectors
    for i in 0..100_000 {
        let key = format!("v_{}", i);
        vector.insert(run_id, "scale_100k", &key, &seeded_vector(3, i as u64), None)
            .expect("insert");

        // Progress indicator
        if i % 10_000 == 0 {
            eprintln!("Inserted {} vectors", i);
        }
    }

    let count = vector.count(run_id, "scale_100k").expect("count");
    assert_eq!(count, 100_000, "Should have 100K vectors");

    // Search should still work efficiently
    let query = seeded_vector(3, 50_000);
    let results = vector.search(run_id, "scale_100k", &query, 10, None).expect("search");
    assert_eq!(results.len(), 10, "Should return k results");
}

/// Test 1M vectors (mega-scale).
///
/// This test is ignored by default due to resource requirements.
#[test]
#[ignore]
fn test_1m_vectors() {
    let test_db = TestDb::new_in_memory();
    let vector = test_db.vector();
    let run_id = test_db.run_id;

    vector.create_collection(run_id, "scale_1m", config_small()).expect("create");

    // Insert 1M vectors
    for i in 0..1_000_000 {
        let key = format!("v_{}", i);
        vector.insert(run_id, "scale_1m", &key, &seeded_vector(3, i as u64), None)
            .expect("insert");

        if i % 100_000 == 0 {
            eprintln!("Inserted {} vectors", i);
        }
    }

    let count = vector.count(run_id, "scale_1m").expect("count");
    assert_eq!(count, 1_000_000, "Should have 1M vectors");
}

/// Test many collections.
#[test]
fn test_many_collections() {
    let test_db = TestDb::new_in_memory();
    let vector = test_db.vector();
    let run_id = test_db.run_id;

    // Create 100 collections
    for i in 0..100 {
        let name = format!("collection_{}", i);
        vector.create_collection(run_id, &name, config_small()).expect("create");

        // Insert 100 vectors per collection
        for j in 0..100 {
            let key = format!("v_{}", j);
            vector.insert(run_id, &name, &key, &seeded_vector(3, (i * 100 + j) as u64), None)
                .expect("insert");
        }
    }

    // Verify all collections accessible
    let collections = vector.list_collections(run_id).expect("list");
    assert_eq!(collections.len(), 100, "Should have 100 collections");
}

/// Test high-dimensional vectors.
#[test]
fn test_high_dimension_vectors() {
    let test_db = TestDb::new_in_memory();
    let vector = test_db.vector();
    let run_id = test_db.run_id;

    // Create collection with high dimensions (1536 like OpenAI embeddings)
    let config = in_mem_primitives::VectorConfig {
        dimension: 1536,
        metric: in_mem_primitives::DistanceMetric::Cosine,
        storage_dtype: in_mem_primitives::StorageDtype::F32,
    };

    vector.create_collection(run_id, "high_dim", config).expect("create");

    // Insert 1000 high-dimensional vectors
    for i in 0..1000 {
        let key = format!("v_{}", i);
        vector.insert(run_id, "high_dim", &key, &seeded_vector(1536, i as u64), None)
            .expect("insert");
    }

    // Search should work
    let query = seeded_vector(1536, 500);
    let results = vector.search(run_id, "high_dim", &query, 10, None).expect("search");
    assert_eq!(results.len(), 10);
}
