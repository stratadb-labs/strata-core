//! Tier 1.4: R4 - Never Invents Data Tests
//!
//! **Invariant R4**: Only data explicitly written appears.
//!
//! These tests verify:
//! - No phantom keys appear after recovery
//! - No phantom values appear after recovery
//! - Corruption results in data loss, not invention

use crate::test_utils::*;
use strata_core::types::RunId;
use strata_core::value::Value;
use strata_primitives::KVStore;
use std::collections::HashSet;
use std::sync::Arc;

/// R4: No phantom keys appear
#[test]
fn test_r4_no_phantom_keys_basic() {
    let mut test_db = TestDb::new();
    let run_id = test_db.run_id;

    // Write specific keys
    let written_keys: HashSet<String> = (0..100).map(|i| format!("key_{}", i)).collect();

    let kv = test_db.kv();
    for key in &written_keys {
        kv.put(&run_id, key, Value::String("value".into())).unwrap();
    }

    // Simulate crash and recovery
    test_db.reopen();

    // Scan all keys
    let kv = test_db.kv();
    let recovered_state = CapturedState::capture(&test_db.db, &run_id);

    // No extra keys should exist
    for key in recovered_state.kv_entries.keys() {
        // Only keys that start with our prefix should exist
        assert!(
            key.starts_with("key_") || key.starts_with("health_check_"),
            "R4 VIOLATED: Phantom key appeared: {}",
            key
        );
    }
}

/// R4: No phantom values
#[test]
fn test_r4_no_phantom_values() {
    let mut test_db = TestDb::new();
    let run_id = test_db.run_id;

    let original_value = "original_value_12345";
    let kv = test_db.kv();
    kv.put(&run_id, "key", Value::String(original_value.into()))
        .unwrap();

    test_db.reopen();

    let kv = test_db.kv();
    if let Some(versioned) = kv.get(&run_id, "key").unwrap() {
        if let Value::String(s) = versioned.value {
            assert_eq!(
                s, original_value,
                "R4 VIOLATED: Value changed to phantom: {}",
                s
            );
        } else {
            panic!("R4 VIOLATED: Value type changed");
        }
    }
}

/// R4: Recovery with empty database produces empty database
#[test]
fn test_r4_empty_stays_empty() {
    let mut test_db = TestDb::new();
    let run_id = test_db.run_id;

    // Don't write anything

    test_db.reopen();

    let state = CapturedState::capture(&test_db.db, &run_id);
    assert!(
        state.kv_entries.is_empty(),
        "R4 VIOLATED: Empty database invented {} entries",
        state.kv_entries.len()
    );
}

/// R4: Value integrity preserved
#[test]
fn test_r4_value_integrity_preserved() {
    let mut test_db = TestDb::new();
    let run_id = test_db.run_id;

    let kv = test_db.kv();

    // Write specific values
    let test_values = vec![
        ("str", Value::String("hello world".into())),
        ("int", Value::I64(12345)),
        ("float", Value::F64(3.14159)),
        ("bool_t", Value::Bool(true)),
        ("bool_f", Value::Bool(false)),
        ("null", Value::Null),
    ];

    for (key, value) in &test_values {
        kv.put(&run_id, key, value.clone()).unwrap();
    }

    test_db.reopen();

    let kv = test_db.kv();

    // Verify all values are exactly as written
    for (key, expected) in &test_values {
        let actual = kv.get(&run_id, key).unwrap();
        assert_eq!(
            actual.as_ref().map(|v| &v.value),
            Some(expected),
            "R4 VIOLATED: Value for {} changed",
            key
        );
    }
}

/// R4: Delete actually removes data
#[test]
fn test_r4_deleted_keys_stay_deleted() {
    let mut test_db = TestDb::new();
    let run_id = test_db.run_id;

    let kv = test_db.kv();

    // Create then delete
    kv.put(
        &run_id,
        "to_delete",
        Value::String("will be deleted".into()),
    )
    .unwrap();
    kv.delete(&run_id, "to_delete").unwrap();

    test_db.reopen();

    let kv = test_db.kv();
    let value = kv.get(&run_id, "to_delete").unwrap();
    assert!(
        value.is_none(),
        "R4 VIOLATED: Deleted key reappeared after recovery"
    );
}

/// R4: Only written keys exist after complex operations
#[test]
fn test_r4_only_final_keys_exist() {
    let mut test_db = TestDb::new();
    let run_id = test_db.run_id;

    let kv = test_db.kv();

    // Complex sequence
    for i in 0..50 {
        kv.put(&run_id, &format!("key_{}", i), Value::I64(i))
            .unwrap();
    }
    for i in 0..25 {
        kv.delete(&run_id, &format!("key_{}", i)).unwrap();
    }

    // Track what should exist
    let expected_keys: HashSet<String> = (25..50).map(|i| format!("key_{}", i)).collect();

    test_db.reopen();

    let state = CapturedState::capture(&test_db.db, &run_id);

    // Check no phantom keys
    for key in state.kv_entries.keys() {
        if key.starts_with("key_") {
            assert!(
                expected_keys.contains(key),
                "R4 VIOLATED: Phantom or deleted key found: {}",
                key
            );
        }
    }
}

/// R4: Recovery never creates keys with wrong prefixes
#[test]
fn test_r4_no_wrong_prefix_keys() {
    let mut test_db = TestDb::new();
    let run_id = test_db.run_id;

    let kv = test_db.kv();

    // Write keys with specific prefix
    for i in 0..20 {
        kv.put(&run_id, &format!("user_{}", i), Value::I64(i))
            .unwrap();
    }

    test_db.reopen();

    let state = CapturedState::capture(&test_db.db, &run_id);

    // All keys should have known prefixes
    for key in state.kv_entries.keys() {
        let valid_prefix = key.starts_with("user_") || key.starts_with("health_check_");
        assert!(valid_prefix, "R4 VIOLATED: Key with wrong prefix: {}", key);
    }
}

/// R4: Large values preserved exactly
#[test]
fn test_r4_large_values_preserved() {
    let mut test_db = TestDb::new();
    let run_id = test_db.run_id;

    let kv = test_db.kv();

    // Create large value
    let large_value: String = (0..10000).map(|i| format!("{:04}", i % 10000)).collect();
    kv.put(&run_id, "large", Value::String(large_value.clone()))
        .unwrap();

    test_db.reopen();

    let kv = test_db.kv();
    if let Some(versioned) = kv.get(&run_id, "large").unwrap() {
        if let Value::String(recovered) = versioned.value {
            assert_eq!(
                recovered.len(),
                large_value.len(),
                "R4 VIOLATED: Large value length changed"
            );
            assert_eq!(
                recovered, large_value,
                "R4 VIOLATED: Large value content changed"
            );
        } else {
            panic!("R4 VIOLATED: Large value wrong type");
        }
    } else {
        panic!("R4 VIOLATED: Large value missing");
    }
}

/// R4: Multiple runs don't cross-contaminate
#[test]
fn test_r4_no_cross_run_contamination() {
    let mut test_db = TestDb::new();
    let run_id1 = test_db.run_id;
    let run_id2 = RunId::new();

    let kv = test_db.kv();

    // Write to different runs
    kv.put(&run_id1, "key", Value::String("run1_value".into()))
        .unwrap();
    kv.put(&run_id2, "key", Value::String("run2_value".into()))
        .unwrap();

    test_db.reopen();

    let kv = test_db.kv();

    // Each run should have its own data
    if let Some(versioned) = kv.get(&run_id1, "key").unwrap() {
        if let Value::String(v1) = versioned.value {
            assert_eq!(v1, "run1_value", "R4: run1 contaminated");
        }
    }
    if let Some(versioned) = kv.get(&run_id2, "key").unwrap() {
        if let Value::String(v2) = versioned.value {
            assert_eq!(v2, "run2_value", "R4: run2 contaminated");
        }
    }
}
