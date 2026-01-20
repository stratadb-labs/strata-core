//! T2: Conflict Detection Tests
//!
//! Invariant T2: Concurrent writes to same key conflict.

use crate::test_utils::*;

/// Test that upsert to same key overwrites
#[test]
fn test_t2_upsert_overwrites_same_key() {
    let test_db = TestDb::new();
    let vector = test_db.vector();

    vector
        .create_collection(test_db.run_id, "embeddings", config_minilm())
        .unwrap();

    let embedding1 = seeded_random_vector(384, 111);
    let embedding2 = seeded_random_vector(384, 222);

    // First insert
    vector
        .insert(test_db.run_id, "embeddings", "key1", &embedding1, None)
        .unwrap();

    // Second insert (same key)
    vector
        .insert(test_db.run_id, "embeddings", "key1", &embedding2, None)
        .unwrap();

    // Should have the second embedding
    let entry = vector
        .get(test_db.run_id, "embeddings", "key1")
        .unwrap()
        .unwrap();

    // Verify it's embedding2, not embedding1
    assert!(
        (entry.value.embedding[0] - embedding2[0]).abs() < 1e-6,
        "T2: Upsert should overwrite with new embedding"
    );
}

/// Test count after multiple upserts to same key
#[test]
fn test_t2_upsert_same_key_count_stable() {
    let test_db = TestDb::new();
    let vector = test_db.vector();

    vector
        .create_collection(test_db.run_id, "embeddings", config_minilm())
        .unwrap();

    // Multiple upserts to same key
    for i in 0..10 {
        vector
            .insert(
                test_db.run_id,
                "embeddings",
                "key1",
                &seeded_random_vector(384, i as u64),
                None,
            )
            .unwrap();
    }

    // Count should be 1
    let count = vector.count(test_db.run_id, "embeddings").unwrap();
    assert_eq!(count, 1, "T2 VIOLATED: Multiple upserts created duplicates");
}

/// Test write-write on different keys doesn't conflict
#[test]
fn test_t2_different_keys_no_conflict() {
    let test_db = TestDb::new();
    let vector = test_db.vector();

    vector
        .create_collection(test_db.run_id, "embeddings", config_minilm())
        .unwrap();

    // Write to different keys
    for i in 0..50 {
        vector
            .insert(
                test_db.run_id,
                "embeddings",
                &format!("key_{}", i),
                &seeded_random_vector(384, i as u64),
                None,
            )
            .unwrap();
    }

    // All should exist
    let count = vector.count(test_db.run_id, "embeddings").unwrap();
    assert_eq!(count, 50, "T2: Different keys should not conflict");
}

/// Test upsert with metadata update
#[test]
fn test_t2_upsert_updates_metadata() {
    let test_db = TestDb::new();
    let vector = test_db.vector();

    vector
        .create_collection(test_db.run_id, "embeddings", config_minilm())
        .unwrap();

    // Insert with metadata v1
    vector
        .insert(
            test_db.run_id,
            "embeddings",
            "key1",
            &random_vector(384),
            Some(serde_json::json!({"version": 1})),
        )
        .unwrap();

    // Upsert with metadata v2
    vector
        .insert(
            test_db.run_id,
            "embeddings",
            "key1",
            &random_vector(384),
            Some(serde_json::json!({"version": 2})),
        )
        .unwrap();

    let entry = vector
        .get(test_db.run_id, "embeddings", "key1")
        .unwrap()
        .unwrap();

    assert_eq!(
        entry.value.metadata.unwrap()["version"],
        2,
        "T2: Upsert should update metadata"
    );
}

/// Test concurrent write simulation
#[test]
fn test_t2_simulated_concurrent_writes() {
    let test_db = TestDb::new();
    let vector = test_db.vector();

    vector
        .create_collection(test_db.run_id, "embeddings", config_minilm())
        .unwrap();

    // Simulate two "writers" alternating writes to same key
    for round in 0..20 {
        // Writer A
        let embedding_a = seeded_random_vector(384, round as u64 * 2);
        vector
            .insert(test_db.run_id, "embeddings", "shared_key", &embedding_a, None)
            .unwrap();

        // Writer B (same key)
        let embedding_b = seeded_random_vector(384, round as u64 * 2 + 1);
        vector
            .insert(test_db.run_id, "embeddings", "shared_key", &embedding_b, None)
            .unwrap();
    }

    // Should have exactly 1 vector
    let count = vector.count(test_db.run_id, "embeddings").unwrap();
    assert_eq!(count, 1, "T2: Concurrent writes to same key should result in single entry");

    // Should have last writer's embedding
    let entry = vector
        .get(test_db.run_id, "embeddings", "shared_key")
        .unwrap()
        .unwrap();
    let expected = seeded_random_vector(384, 39); // Last write: round 19, writer B = 19*2+1 = 39
    assert!(
        (entry.value.embedding[0] - expected[0]).abs() < 1e-6,
        "T2: Should have last writer's embedding"
    );
}
