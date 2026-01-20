//! S5: Heap + KV Consistency Tests
//!
//! Invariant S5: Vector heap and KV metadata always in sync.

use crate::test_utils::*;
use serde_json::json;

/// Test that insert updates both heap and metadata
#[test]
fn test_s5_insert_updates_both() {
    let test_db = TestDb::new();
    let vector = test_db.vector();

    vector
        .create_collection(test_db.run_id, "embeddings", config_minilm())
        .unwrap();

    let embedding = random_vector(384);
    let metadata = json!({"source": "test", "type": "document"});

    vector
        .insert(
            test_db.run_id,
            "embeddings",
            "key1",
            &embedding,
            Some(metadata.clone()),
        )
        .unwrap();

    // Verify entry has both embedding and metadata
    let entry = vector
        .get(test_db.run_id, "embeddings", "key1")
        .unwrap()
        .unwrap();

    assert_eq!(entry.value.embedding, embedding, "S5 VIOLATED: Embedding not stored correctly");
    assert_eq!(
        entry.value.metadata,
        Some(metadata),
        "S5 VIOLATED: Metadata not stored correctly"
    );
}

/// Test that delete removes both
#[test]
fn test_s5_delete_removes_both() {
    let test_db = TestDb::new();
    let vector = test_db.vector();

    vector
        .create_collection(test_db.run_id, "embeddings", config_minilm())
        .unwrap();

    vector
        .insert(
            test_db.run_id,
            "embeddings",
            "key1",
            &random_vector(384),
            Some(json!({"x": 1})),
        )
        .unwrap();

    // Verify it exists
    assert!(vector.get(test_db.run_id, "embeddings", "key1").unwrap().is_some());

    // Delete
    vector.delete(test_db.run_id, "embeddings", "key1").unwrap();

    // Verify it's gone (both embedding and metadata)
    assert!(
        vector.get(test_db.run_id, "embeddings", "key1").unwrap().is_none(),
        "S5 VIOLATED: Entry still exists after delete"
    );
}

/// Test consistency after crash and recovery
#[test]
fn test_s5_consistency_after_crash() {
    let mut test_db = TestDb::new();
    let run_id = test_db.run_id;

    {
        let vector = test_db.vector();
        vector
            .create_collection(run_id, "embeddings", config_minilm())
            .unwrap();

        for i in 0..50 {
            vector
                .insert(
                    run_id,
                    "embeddings",
                    &format!("key_{}", i),
                    &random_vector(384),
                    Some(json!({"index": i})),
                )
                .unwrap();
        }
    }

    // Simulate crash and recovery
    test_db.reopen();

    let vector = test_db.vector();

    // Verify heap and metadata are consistent
    for i in 0..50 {
        let key = format!("key_{}", i);
        let entry = vector.get(run_id, "embeddings", &key).unwrap();

        assert!(
            entry.is_some(),
            "S5 VIOLATED: Key {} missing after recovery",
            key
        );

        let entry = entry.unwrap();
        assert_eq!(entry.value.embedding.len(), 384, "S5 VIOLATED: Embedding missing for {}", key);
        assert!(entry.value.metadata.is_some(), "S5 VIOLATED: Metadata missing for {}", key);
    }
}

/// Test upsert updates both embedding and metadata
#[test]
fn test_s5_upsert_updates_both() {
    let test_db = TestDb::new();
    let vector = test_db.vector();

    vector
        .create_collection(test_db.run_id, "embeddings", config_minilm())
        .unwrap();

    // Initial insert
    let embedding1 = random_vector(384);
    let metadata1 = json!({"version": 1});
    vector
        .insert(
            test_db.run_id,
            "embeddings",
            "key1",
            &embedding1,
            Some(metadata1),
        )
        .unwrap();

    // Upsert with new values
    let embedding2 = random_vector(384);
    let metadata2 = json!({"version": 2});
    vector
        .insert(
            test_db.run_id,
            "embeddings",
            "key1",
            &embedding2,
            Some(metadata2.clone()),
        )
        .unwrap();

    let entry = vector
        .get(test_db.run_id, "embeddings", "key1")
        .unwrap()
        .unwrap();

    assert_eq!(entry.value.embedding, embedding2, "S5 VIOLATED: Embedding not updated on upsert");
    assert_eq!(
        entry.value.metadata,
        Some(metadata2),
        "S5 VIOLATED: Metadata not updated on upsert"
    );
}

/// Test no partial state on failed operations
#[test]
fn test_s5_no_partial_state_on_error() {
    let test_db = TestDb::new();
    let vector = test_db.vector();

    vector
        .create_collection(test_db.run_id, "embeddings", config_minilm())
        .unwrap();

    // Try to insert with wrong dimension (should fail)
    let result = vector.insert(
        test_db.run_id,
        "embeddings",
        "key1",
        &random_vector(768), // Wrong dimension
        Some(json!({"should": "not exist"})),
    );
    assert!(result.is_err());

    // Verify nothing was stored
    assert!(
        vector.get(test_db.run_id, "embeddings", "key1").unwrap().is_none(),
        "S5 VIOLATED: Partial state exists after failed insert"
    );
}

/// Test consistency with many operations
#[test]
fn test_s5_consistency_many_operations() {
    let test_db = TestDb::new();
    let vector = test_db.vector();

    vector
        .create_collection(test_db.run_id, "embeddings", config_minilm())
        .unwrap();

    // Insert 100 vectors
    for i in 0..100 {
        vector
            .insert(
                test_db.run_id,
                "embeddings",
                &format!("key_{}", i),
                &random_vector(384),
                Some(json!({"index": i})),
            )
            .unwrap();
    }

    // Delete half
    for i in 0..50 {
        vector
            .delete(test_db.run_id, "embeddings", &format!("key_{}", i))
            .unwrap();
    }

    // Update remaining
    for i in 50..100 {
        vector
            .insert(
                test_db.run_id,
                "embeddings",
                &format!("key_{}", i),
                &random_vector(384),
                Some(json!({"index": i, "updated": true})),
            )
            .unwrap();
    }

    // Verify consistency
    for i in 0..100 {
        let key = format!("key_{}", i);
        let entry = vector.get(test_db.run_id, "embeddings", &key).unwrap();

        if i < 50 {
            assert!(entry.is_none(), "S5 VIOLATED: Deleted key {} still exists", key);
        } else {
            assert!(entry.is_some(), "S5 VIOLATED: Key {} missing", key);
            let entry = entry.unwrap();
            assert_eq!(entry.value.embedding.len(), 384);
            assert!(entry.value.metadata.is_some());
        }
    }

    // Count should be 50
    let count = vector.count(test_db.run_id, "embeddings").unwrap();
    assert_eq!(count, 50, "S5 VIOLATED: Count mismatch");
}
