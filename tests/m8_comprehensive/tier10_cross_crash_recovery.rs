//! Tier 10: Cross-Primitive Crash Recovery Tests
//!
//! Tests for recovery of cross-primitive operations after restart.

use crate::test_utils::*;

#[test]
fn test_cross_primitive_recovery_after_reopen() {
    let mut test_db = TestDb::new_strict();
    let run_id = test_db.run_id;

    {
        let vector = test_db.vector();
        vector.create_collection(run_id, "embeddings", config_minilm()).unwrap();

        let kv = test_db.kv();
        kv.put(&run_id, "key1", strata_core::value::Value::String("value1".into())).unwrap();

        vector.insert(run_id, "embeddings", "vec1", &random_vector(384), None).unwrap();
    }

    test_db.reopen();

    // All should be recovered
    let kv = test_db.kv();
    let vector = test_db.vector();

    assert!(kv.get(&run_id, "key1").unwrap().is_some());
    assert_eq!(vector.count(run_id, "embeddings").unwrap(), 1);
}

#[test]
fn test_cross_primitive_multiple_reopens() {
    let mut test_db = TestDb::new_strict();
    let run_id = test_db.run_id;

    {
        let vector = test_db.vector();
        vector.create_collection(run_id, "embeddings", config_minilm()).unwrap();

        for i in 0..5 {
            let kv = test_db.kv();
            kv.put(&run_id, &format!("key_{}", i), strata_core::value::Value::I64(i)).unwrap();

            vector.insert(run_id, "embeddings", &format!("vec_{}", i), &seeded_random_vector(384, i as u64), None).unwrap();
        }
    }

    // Multiple reopens should preserve data
    for _ in 0..3 {
        test_db.reopen();

        let kv = test_db.kv();
        let vector = test_db.vector();

        for i in 0..5 {
            assert!(kv.get(&run_id, &format!("key_{}", i)).unwrap().is_some());
            assert!(vector.get(run_id, "embeddings", &format!("vec_{}", i)).unwrap().is_some());
        }
        assert_eq!(vector.count(run_id, "embeddings").unwrap(), 5);
    }
}

#[test]
fn test_cross_primitive_incremental_writes() {
    let mut test_db = TestDb::new_strict();
    let run_id = test_db.run_id;

    // Initial data
    {
        let vector = test_db.vector();
        vector.create_collection(run_id, "embeddings", config_minilm()).unwrap();

        let kv = test_db.kv();
        kv.put(&run_id, "batch", strata_core::value::Value::I64(1)).unwrap();
        vector.insert(run_id, "embeddings", "vec_0", &random_vector(384), None).unwrap();
    }

    test_db.reopen();

    // Add more data
    {
        let kv = test_db.kv();
        kv.put(&run_id, "batch", strata_core::value::Value::I64(2)).unwrap();

        let vector = test_db.vector();
        vector.insert(run_id, "embeddings", "vec_1", &random_vector(384), None).unwrap();
    }

    test_db.reopen();

    // Verify all data
    let kv = test_db.kv();
    let vector = test_db.vector();

    if let Some(versioned) = kv.get(&run_id, "batch").unwrap() {
        if let strata_core::value::Value::I64(v) = versioned.value {
            assert_eq!(v, 2);
        }
    }
    assert_eq!(vector.count(run_id, "embeddings").unwrap(), 2);
}
