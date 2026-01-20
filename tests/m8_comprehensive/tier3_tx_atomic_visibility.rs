//! T1: Atomic Visibility Tests
//!
//! Invariant T1: Insert/delete atomic with other primitives.

use crate::test_utils::*;
use in_mem_core::value::Value;

/// Test that committed vector is visible
#[test]
fn test_t1_committed_vector_visible() {
    let test_db = TestDb::new();
    let vector = test_db.vector();
    let kv = test_db.kv();

    vector
        .create_collection(test_db.run_id, "embeddings", config_minilm())
        .unwrap();

    // Insert vector
    vector
        .insert(
            test_db.run_id,
            "embeddings",
            "doc1",
            &random_vector(384),
            None,
        )
        .unwrap();

    // Also insert related KV entry
    kv.put(&test_db.run_id, "doc1_status", Value::String("indexed".into()))
        .unwrap();

    // Both should be visible
    assert!(vector
        .get(test_db.run_id, "embeddings", "doc1")
        .unwrap()
        .is_some());
    assert!(kv.get(&test_db.run_id, "doc1_status").unwrap().is_some());
}

/// Test vector and KV atomicity
#[test]
fn test_t1_vector_kv_atomicity() {
    let test_db = TestDb::new();
    let vector = test_db.vector();
    let kv = test_db.kv();

    vector
        .create_collection(test_db.run_id, "embeddings", config_minilm())
        .unwrap();

    // Multiple related operations
    for i in 0..10 {
        let key = format!("doc_{}", i);
        vector
            .insert(test_db.run_id, "embeddings", &key, &random_vector(384), None)
            .unwrap();
        kv.put(&test_db.run_id, &key, Value::String(format!("content_{}", i)))
            .unwrap();
    }

    // All should be visible
    for i in 0..10 {
        let key = format!("doc_{}", i);
        assert!(
            vector.get(test_db.run_id, "embeddings", &key).unwrap().is_some(),
            "T1 VIOLATED: Vector {} not visible",
            key
        );
        assert!(
            kv.get(&test_db.run_id, &key).unwrap().is_some(),
            "T1 VIOLATED: KV {} not visible",
            key
        );
    }
}

/// Test delete atomicity
#[test]
fn test_t1_delete_atomicity() {
    let test_db = TestDb::new();
    let vector = test_db.vector();
    let kv = test_db.kv();

    vector
        .create_collection(test_db.run_id, "embeddings", config_minilm())
        .unwrap();

    // Insert
    vector
        .insert(test_db.run_id, "embeddings", "doc1", &random_vector(384), None)
        .unwrap();
    kv.put(&test_db.run_id, "doc1", Value::String("content".into()))
        .unwrap();

    // Verify both exist
    assert!(vector.get(test_db.run_id, "embeddings", "doc1").unwrap().is_some());
    assert!(kv.get(&test_db.run_id, "doc1").unwrap().is_some());

    // Delete both
    vector.delete(test_db.run_id, "embeddings", "doc1").unwrap();
    kv.delete(&test_db.run_id, "doc1").unwrap();

    // Both should be gone
    assert!(
        vector.get(test_db.run_id, "embeddings", "doc1").unwrap().is_none(),
        "T1 VIOLATED: Vector should be deleted"
    );
    assert!(
        kv.get(&test_db.run_id, "doc1").unwrap().is_none(),
        "T1 VIOLATED: KV should be deleted"
    );
}

/// Test atomicity survives restart
#[test]
fn test_t1_atomicity_survives_restart() {
    let mut test_db = TestDb::new_strict();
    let run_id = test_db.run_id;

    {
        let vector = test_db.vector();
        let kv = test_db.kv();

        vector
            .create_collection(run_id, "embeddings", config_minilm())
            .unwrap();

        for i in 0..20 {
            let key = format!("doc_{}", i);
            vector
                .insert(run_id, "embeddings", &key, &random_vector(384), None)
                .unwrap();
            kv.put(&run_id, &key, Value::String(format!("content_{}", i)))
                .unwrap();
        }
    }

    // Restart
    test_db.reopen();

    let vector = test_db.vector();
    let kv = test_db.kv();

    // All should still be visible
    for i in 0..20 {
        let key = format!("doc_{}", i);
        assert!(
            vector.get(run_id, "embeddings", &key).unwrap().is_some(),
            "T1 VIOLATED: Vector {} not visible after restart",
            key
        );
        assert!(
            kv.get(&run_id, &key).unwrap().is_some(),
            "T1 VIOLATED: KV {} not visible after restart",
            key
        );
    }
}

/// Test partial operation visibility
#[test]
fn test_t1_operations_always_complete() {
    let test_db = TestDb::new();
    let vector = test_db.vector();

    vector
        .create_collection(test_db.run_id, "embeddings", config_minilm())
        .unwrap();

    // Insert multiple vectors
    for i in 0..50 {
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

    // All should be visible (no partial states)
    let count = vector.count(test_db.run_id, "embeddings").unwrap();
    assert_eq!(count, 50, "T1 VIOLATED: Some vectors not visible");

    // Each individual vector should be fully formed
    for i in 0..50 {
        let entry = vector
            .get(test_db.run_id, "embeddings", &format!("key_{}", i))
            .unwrap();
        assert!(entry.is_some(), "T1 VIOLATED: Vector {} not visible", i);
        let entry = entry.unwrap();
        assert_eq!(entry.value.embedding.len(), 384, "T1 VIOLATED: Vector {} incomplete", i);
    }
}
