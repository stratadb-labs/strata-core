//! Tier 10: Cross-Primitive All Primitives Tests
//!
//! Tests for KV and Vector operations working together.
//! Note: JSON tests require more complex setup with JsonDocId, JsonPath, and JsonValue types.

use crate::test_utils::*;

#[test]
fn test_kv_vector_all_persisted() {
    let mut test_db = TestDb::new_strict();
    let run_id = test_db.run_id;

    {
        let vector = test_db.vector();
        vector.create_collection(run_id, "embeddings", config_minilm()).unwrap();

        let kv = test_db.kv();
        kv.put(&run_id, "counter", strata_core::value::Value::I64(1)).unwrap();

        vector.insert(run_id, "embeddings", "user1", &random_vector(384), None).unwrap();
    }

    test_db.reopen();

    // All should be persisted
    let kv = test_db.kv();
    let vector = test_db.vector();

    assert!(kv.get(&run_id, "counter").unwrap().is_some());
    assert_eq!(vector.count(run_id, "embeddings").unwrap(), 1);
}

#[test]
fn test_all_primitives_complex_workflow() {
    let mut test_db = TestDb::new_strict();
    let run_id = test_db.run_id;

    {
        let vector = test_db.vector();
        vector.create_collection(run_id, "embeddings", config_minilm()).unwrap();

        let kv = test_db.kv();

        for i in 0..5 {
            // KV: simple flags/counters
            kv.put(&run_id, &format!("user:{}:active", i), strata_core::value::Value::Bool(true)).unwrap();
            kv.put(&run_id, &format!("user:{}:login_count", i), strata_core::value::Value::I64(0)).unwrap();

            // Vector: embeddings
            vector.insert(run_id, "embeddings", &format!("user_{}", i), &seeded_random_vector(384, i as u64), None).unwrap();
        }
    }

    test_db.reopen();

    // Verify all data
    let kv = test_db.kv();
    let vector = test_db.vector();

    for i in 0..5 {
        assert!(kv.get(&run_id, &format!("user:{}:active", i)).unwrap().is_some());
        assert!(vector.get(run_id, "embeddings", &format!("user_{}", i)).unwrap().is_some());
    }
}

#[test]
fn test_all_primitives_partial_update() {
    let mut test_db = TestDb::new_strict();
    let run_id = test_db.run_id;

    {
        let vector = test_db.vector();
        vector.create_collection(run_id, "embeddings", config_minilm()).unwrap();

        let kv = test_db.kv();

        // Initial state
        kv.put(&run_id, "version", strata_core::value::Value::I64(1)).unwrap();
        vector.insert(run_id, "embeddings", "main", &seeded_random_vector(384, 1), None).unwrap();

        // Update both
        kv.put(&run_id, "version", strata_core::value::Value::I64(2)).unwrap();
        vector.insert(run_id, "embeddings", "main", &seeded_random_vector(384, 2), None).unwrap();
    }

    test_db.reopen();

    // Both updated
    let kv = test_db.kv();

    if let Some(versioned) = kv.get(&run_id, "version").unwrap() {
        if let strata_core::value::Value::I64(v) = versioned.value {
            assert_eq!(v, 2);
        }
    }
}
