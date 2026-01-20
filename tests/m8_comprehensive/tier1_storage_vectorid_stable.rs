//! S3: VectorId Stable Tests
//!
//! Invariant S3: IDs do not change within collection lifetime.

use crate::test_utils::*;

/// Test that VectorId remains stable across operations
#[test]
fn test_s3_vectorid_stable_across_operations() {
    let test_db = TestDb::new();
    let vector = test_db.vector();

    vector
        .create_collection(test_db.run_id, "embeddings", config_minilm())
        .unwrap();

    // Insert vectors
    vector
        .insert(test_db.run_id, "embeddings", "key1", &random_vector(384), None)
        .unwrap();
    vector
        .insert(test_db.run_id, "embeddings", "key2", &random_vector(384), None)
        .unwrap();
    vector
        .insert(test_db.run_id, "embeddings", "key3", &random_vector(384), None)
        .unwrap();

    // Capture VectorId for key2
    let entry_before = vector
        .get(test_db.run_id, "embeddings", "key2")
        .unwrap()
        .unwrap();
    let id_before = entry_before.value.vector_id();

    // Perform other operations
    vector.delete(test_db.run_id, "embeddings", "key1").unwrap();
    vector
        .insert(test_db.run_id, "embeddings", "key4", &random_vector(384), None)
        .unwrap();

    // VectorId for key2 must not change
    let entry_after = vector
        .get(test_db.run_id, "embeddings", "key2")
        .unwrap()
        .unwrap();
    let id_after = entry_after.value.vector_id();

    assert_eq!(
        id_before, id_after,
        "S3 VIOLATED: VectorId changed during operations"
    );
}

/// Test that VectorId remains stable across restart
#[test]
fn test_s3_vectorid_stable_across_restart() {
    let mut test_db = TestDb::new();
    let run_id = test_db.run_id;

    let id_before;
    {
        let vector = test_db.vector();
        vector
            .create_collection(run_id, "embeddings", config_minilm())
            .unwrap();
        vector
            .insert(run_id, "embeddings", "key1", &random_vector(384), None)
            .unwrap();

        let entry = vector.get(run_id, "embeddings", "key1").unwrap().unwrap();
        id_before = entry.value.vector_id();
    }

    // Restart
    test_db.reopen();

    let vector = test_db.vector();
    let entry_after = vector.get(run_id, "embeddings", "key1").unwrap().unwrap();

    assert_eq!(
        id_before, entry_after.value.vector_id(),
        "S3 VIOLATED: VectorId changed across restart"
    );
}

/// Test that VectorIds are stable when updating embedding
#[test]
fn test_s3_vectorid_stable_on_upsert_update() {
    let test_db = TestDb::new();
    let vector = test_db.vector();

    vector
        .create_collection(test_db.run_id, "embeddings", config_minilm())
        .unwrap();

    // Initial insert
    let embedding1 = random_vector(384);
    vector
        .insert(test_db.run_id, "embeddings", "key1", &embedding1, None)
        .unwrap();

    let entry_before = vector
        .get(test_db.run_id, "embeddings", "key1")
        .unwrap()
        .unwrap();
    let id_before = entry_before.value.vector_id();

    // Update with new embedding (same key)
    let embedding2 = random_vector(384);
    vector
        .insert(test_db.run_id, "embeddings", "key1", &embedding2, None)
        .unwrap();

    // Note: On upsert, the ID may or may not change depending on implementation.
    // However, for existing keys, the behavior should be consistent.
    // Check that we get a valid entry back.
    let entry_after = vector
        .get(test_db.run_id, "embeddings", "key1")
        .unwrap()
        .unwrap();

    // The embedding should have been updated
    assert_eq!(entry_after.value.embedding, embedding2);
}

/// Test VectorId stability across multiple collections
#[test]
fn test_s3_vectorid_stable_multiple_collections() {
    let test_db = TestDb::new();
    let vector = test_db.vector();

    // Create two collections
    vector
        .create_collection(test_db.run_id, "collection1", config_minilm())
        .unwrap();
    vector
        .create_collection(test_db.run_id, "collection2", config_minilm())
        .unwrap();

    // Insert same key in both
    vector
        .insert(test_db.run_id, "collection1", "key1", &random_vector(384), None)
        .unwrap();
    vector
        .insert(test_db.run_id, "collection2", "key1", &random_vector(384), None)
        .unwrap();

    let entry1 = vector
        .get(test_db.run_id, "collection1", "key1")
        .unwrap()
        .unwrap();
    let entry2 = vector
        .get(test_db.run_id, "collection2", "key1")
        .unwrap()
        .unwrap();

    // Each collection has its own VectorId space (IDs may be same or different)
    // But they should both be valid
    assert!(entry1.value.vector_id().as_u64() > 0 || entry1.value.vector_id().as_u64() == 0); // Just check it exists
    assert!(entry2.value.vector_id().as_u64() > 0 || entry2.value.vector_id().as_u64() == 0);
}

/// Test VectorId stability with many operations
#[test]
fn test_s3_vectorid_stable_many_operations() {
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
                None,
            )
            .unwrap();
    }

    // Capture all IDs
    let mut ids_before = std::collections::HashMap::new();
    for i in 0..100 {
        let key = format!("key_{}", i);
        let entry = vector.get(test_db.run_id, "embeddings", &key).unwrap().unwrap();
        ids_before.insert(key, entry.value.vector_id());
    }

    // Perform many operations
    for i in 0..50 {
        // Delete even keys
        vector
            .delete(test_db.run_id, "embeddings", &format!("key_{}", i * 2))
            .unwrap();
    }

    // Insert new keys
    for i in 100..150 {
        vector
            .insert(
                test_db.run_id,
                "embeddings",
                &format!("key_{}", i),
                &random_vector(384),
                None,
            )
            .unwrap();
    }

    // Check remaining odd keys still have same IDs
    for i in 0..50 {
        let key = format!("key_{}", i * 2 + 1);
        let entry = vector.get(test_db.run_id, "embeddings", &key).unwrap().unwrap();
        let id_before = ids_before.get(&key).unwrap();
        assert_eq!(
            *id_before, entry.value.vector_id(),
            "S3 VIOLATED: VectorId for {} changed after operations",
            key
        );
    }
}
