//! Tier 10: Cross-Primitive KV + Vector Tests
//!
//! Tests for KV and Vector operations working together.
//! Note: These tests verify durability across primitives, not explicit transactions.

use crate::test_utils::*;

#[test]
fn test_kv_vector_both_persisted() {
    let mut test_db = TestDb::new_strict();
    let run_id = test_db.run_id;

    {
        let vector = test_db.vector();
        vector.create_collection(run_id, "embeddings", config_minilm()).unwrap();

        // Write to both KV and Vector
        let kv = test_db.kv();
        kv.put(&run_id, "user:1:name", strata_core::value::Value::String("Alice".into())).unwrap();
        kv.put(&run_id, "user:1:email", strata_core::value::Value::String("alice@example.com".into())).unwrap();

        vector.insert(run_id, "embeddings", "user:1:embedding", &random_vector(384), None).unwrap();
    }

    test_db.reopen();

    // Both should be persisted after reopen
    let kv = test_db.kv();
    let vector = test_db.vector();
    assert!(kv.get(&run_id, "user:1:name").unwrap().is_some());
    assert_eq!(vector.count(run_id, "embeddings").unwrap(), 1);
}

#[test]
fn test_kv_vector_multiple_operations() {
    let mut test_db = TestDb::new_strict();
    let run_id = test_db.run_id;

    {
        let vector = test_db.vector();
        vector.create_collection(run_id, "embeddings", config_minilm()).unwrap();

        let kv = test_db.kv();

        for i in 0..10 {
            kv.put(&run_id, &format!("key_{}", i), strata_core::value::Value::String(format!("value_{}", i))).unwrap();
            vector.insert(run_id, "embeddings", &format!("vec_{}", i), &seeded_random_vector(384, i as u64), None).unwrap();
        }
    }

    test_db.reopen();

    // All should be visible after reopen
    let kv = test_db.kv();
    let vector = test_db.vector();
    for i in 0..10 {
        assert!(kv.get(&run_id, &format!("key_{}", i)).unwrap().is_some());
        assert!(vector.get(run_id, "embeddings", &format!("vec_{}", i)).unwrap().is_some());
    }
}

#[test]
fn test_kv_vector_isolation_between_runs() {
    let test_db = TestDb::new_strict();

    // Use new run IDs to test isolation
    let run_id_1 = strata_core::types::RunId::new();
    let run_id_2 = strata_core::types::RunId::new();

    let vector = test_db.vector();
    let kv = test_db.kv();

    // Create collections in different runs
    vector.create_collection(run_id_1, "embeddings", config_minilm()).unwrap();
    vector.create_collection(run_id_2, "embeddings", config_small()).unwrap();

    // Insert into run 1
    kv.put(&run_id_1, "key1", strata_core::value::Value::String("run1_value".into())).unwrap();
    vector.insert(run_id_1, "embeddings", "vec1", &random_vector(384), None).unwrap();

    // Insert into run 2
    kv.put(&run_id_2, "key1", strata_core::value::Value::String("run2_value".into())).unwrap();
    vector.insert(run_id_2, "embeddings", "vec1", &random_vector(3), None).unwrap();

    // Verify isolation
    let v1 = vector.get(run_id_1, "embeddings", "vec1").unwrap().unwrap();
    let v2 = vector.get(run_id_2, "embeddings", "vec1").unwrap().unwrap();

    assert_eq!(v1.value.embedding.len(), 384);
    assert_eq!(v2.value.embedding.len(), 3);
}

#[test]
fn test_kv_vector_delete_operations() {
    let mut test_db = TestDb::new_strict();
    let run_id = test_db.run_id;

    {
        let vector = test_db.vector();
        vector.create_collection(run_id, "embeddings", config_minilm()).unwrap();

        let kv = test_db.kv();

        // Insert
        for i in 0..10 {
            kv.put(&run_id, &format!("key_{}", i), strata_core::value::Value::I64(i)).unwrap();
            vector.insert(run_id, "embeddings", &format!("vec_{}", i), &seeded_random_vector(384, i as u64), None).unwrap();
        }

        // Delete half
        for i in 0..5 {
            kv.delete(&run_id, &format!("key_{}", i)).unwrap();
            vector.delete(run_id, "embeddings", &format!("vec_{}", i)).unwrap();
        }
    }

    test_db.reopen();

    let kv = test_db.kv();
    let vector = test_db.vector();

    // First half deleted
    for i in 0..5 {
        assert!(kv.get(&run_id, &format!("key_{}", i)).unwrap().is_none());
        assert!(vector.get(run_id, "embeddings", &format!("vec_{}", i)).unwrap().is_none());
    }

    // Second half exists
    for i in 5..10 {
        assert!(kv.get(&run_id, &format!("key_{}", i)).unwrap().is_some());
        assert!(vector.get(run_id, "embeddings", &format!("vec_{}", i)).unwrap().is_some());
    }
}
