//! KV Basic Operations Tests
//!
//! Tests for fundamental KV operations:
//! - put: Store a value
//! - get: Retrieve a value
//! - delete: Remove a value
//! - exists: Check if key exists
//! - overwrite: Replace existing value
//!
//! All test data is loaded from testdata/kv_test_data.jsonl

use super::*;
use crate::test_data::load_kv_test_data;

// =============================================================================
// PUT TESTS
// =============================================================================

#[test]
fn test_put_returns_version() {
    let (_, substrate) = quick_setup();
    let run = ApiRunId::default();
    let test_data = load_kv_test_data();

    // Use first entry from test data
    let entry = &test_data.entries[0];

    let version = substrate
        .kv_put(&run, &entry.key, entry.value.clone())
        .expect("put should succeed");

    // Version should be a transaction version
    match version {
        Version::Txn(n) => assert!(n > 0, "Transaction version should be positive"),
        _ => panic!("Expected Version::Txn, got {:?}", version),
    }
}

#[test]
fn test_put_get_roundtrip() {
    let (_, substrate) = quick_setup();
    let run = ApiRunId::default();
    let test_data = load_kv_test_data();

    // Use a string entry from test data
    let entry = test_data.get_type("string").first().expect("need string entry");

    substrate
        .kv_put(&run, &entry.key, entry.value.clone())
        .expect("put should succeed");

    let result = substrate
        .kv_get(&run, &entry.key)
        .expect("get should succeed");

    assert!(result.is_some(), "Key should exist after put");
    let versioned = result.unwrap();
    assert!(
        values_equal(&versioned.value, &entry.value),
        "Value should match what was put"
    );
}

#[test]
fn test_put_all_types_roundtrip() {
    let (_, substrate) = quick_setup();
    let run = ApiRunId::default();
    let test_data = load_kv_test_data();

    // Test roundtrip for entries of each type from run 0
    for entry in test_data.get_run(0) {
        substrate
            .kv_put(&run, &entry.key, entry.value.clone())
            .expect(&format!("put should succeed for {}", entry.key));

        let result = substrate
            .kv_get(&run, &entry.key)
            .expect("get should succeed")
            .expect("key should exist");

        assert!(
            values_equal(&result.value, &entry.value),
            "Roundtrip failed for key '{}' type '{}': expected {:?}, got {:?}",
            entry.key,
            entry.value_type,
            entry.value,
            result.value
        );
    }
}

// =============================================================================
// GET TESTS
// =============================================================================

#[test]
fn test_get_returns_versioned() {
    let (_, substrate) = quick_setup();
    let run = ApiRunId::default();
    let test_data = load_kv_test_data();

    let entry = test_data.get_type("int").first().expect("need int entry");

    let put_version = substrate
        .kv_put(&run, &entry.key, entry.value.clone())
        .expect("put should succeed");

    let versioned = substrate
        .kv_get(&run, &entry.key)
        .expect("get should succeed")
        .expect("key should exist");

    // Check version matches
    assert_eq!(versioned.version, put_version, "Versions should match");

    // Check timestamp is reasonable (after year 2020, before year 2100)
    let ts_micros = versioned.timestamp.as_micros();
    let year_2020_micros: u64 = 1577836800_000_000;
    let year_2100_micros: u64 = 4102444800_000_000;
    assert!(
        ts_micros > year_2020_micros && ts_micros < year_2100_micros,
        "Timestamp {} should be reasonable",
        ts_micros
    );
}

#[test]
fn test_get_missing_returns_none() {
    let (_, substrate) = quick_setup();
    let run = ApiRunId::default();

    // Use a key that definitely doesn't exist
    let result = substrate
        .kv_get(&run, "nonexistent_key_xyz_12345")
        .expect("get should succeed even for missing key");

    assert!(result.is_none(), "Missing key should return None");
}

// =============================================================================
// DELETE TESTS
// =============================================================================

#[test]
fn test_delete_existing_key() {
    let (_, substrate) = quick_setup();
    let run = ApiRunId::default();
    let test_data = load_kv_test_data();

    let entry = &test_data.entries[0];

    // Put then delete
    substrate
        .kv_put(&run, &entry.key, entry.value.clone())
        .expect("put should succeed");

    let deleted = substrate
        .kv_delete(&run, &entry.key)
        .expect("delete should succeed");

    assert!(deleted, "delete should return true for existing key");

    // Verify key is gone
    let result = substrate
        .kv_get(&run, &entry.key)
        .expect("get should succeed");
    assert!(result.is_none(), "Key should be gone after delete");
}

#[test]
fn test_delete_nonexistent_key() {
    let (_, substrate) = quick_setup();
    let run = ApiRunId::default();

    let deleted = substrate
        .kv_delete(&run, "never_existed_xyz_99999")
        .expect("delete should succeed");

    assert!(!deleted, "delete should return false for nonexistent key");
}

// =============================================================================
// EXISTS TESTS
// =============================================================================

#[test]
fn test_exists_accuracy() {
    let (_, substrate) = quick_setup();
    let run = ApiRunId::default();
    let test_data = load_kv_test_data();

    let entry = &test_data.entries[5]; // Use an entry from test data

    // Initially doesn't exist
    assert!(
        !substrate.kv_exists(&run, &entry.key).unwrap(),
        "Key should not exist initially"
    );

    // After put, exists
    substrate
        .kv_put(&run, &entry.key, entry.value.clone())
        .unwrap();
    assert!(
        substrate.kv_exists(&run, &entry.key).unwrap(),
        "Key should exist after put"
    );

    // After delete, doesn't exist
    substrate.kv_delete(&run, &entry.key).unwrap();
    assert!(
        !substrate.kv_exists(&run, &entry.key).unwrap(),
        "Key should not exist after delete"
    );
}

// =============================================================================
// OVERWRITE TESTS
// =============================================================================

#[test]
fn test_overwrite_same_type() {
    let (_, substrate) = quick_setup();
    let run = ApiRunId::default();
    let test_data = load_kv_test_data();

    // Get multiple int entries
    let int_entries: Vec<_> = test_data.get_type("int").iter().take(3).collect();
    assert!(int_entries.len() >= 3, "Need at least 3 int entries");

    let key = "overwrite_test_key";

    // Overwrite with different int values
    for entry in &int_entries {
        substrate.kv_put(&run, key, entry.value.clone()).unwrap();
    }

    let final_value = substrate
        .kv_get(&run, key)
        .unwrap()
        .unwrap()
        .value;

    assert_eq!(
        final_value,
        int_entries.last().unwrap().value,
        "Should have final overwritten value"
    );
}

#[test]
fn test_overwrite_different_type() {
    let (_, substrate) = quick_setup();
    let run = ApiRunId::default();
    let test_data = load_kv_test_data();

    let key = "type_change_key";

    // Get entries of different types
    let int_entry = test_data.get_type("int").first().expect("need int");
    let string_entry = test_data.get_type("string").first().expect("need string");

    // Put int, then string
    substrate.kv_put(&run, key, int_entry.value.clone()).unwrap();
    substrate.kv_put(&run, key, string_entry.value.clone()).unwrap();

    let value = substrate
        .kv_get(&run, key)
        .unwrap()
        .unwrap()
        .value;

    assert!(
        values_equal(&value, &string_entry.value),
        "Should allow type change on overwrite"
    );
}

// =============================================================================
// RUN ISOLATION TESTS
// =============================================================================

#[test]
fn test_run_isolation() {
    let (_, substrate) = quick_setup();
    let run1 = ApiRunId::default();
    let run2 = ApiRunId::new();
    let test_data = load_kv_test_data();

    // Use entries from different runs in test data
    let entry1 = &test_data.get_run(0)[0];
    let entry2 = &test_data.get_run(1)[0];

    // Same key name in different runs
    let shared_key = "shared_isolation_key";

    substrate.kv_put(&run1, shared_key, entry1.value.clone()).unwrap();
    substrate.kv_put(&run2, shared_key, entry2.value.clone()).unwrap();

    let v1 = substrate.kv_get(&run1, shared_key).unwrap().unwrap().value;
    let v2 = substrate.kv_get(&run2, shared_key).unwrap().unwrap().value;

    assert!(values_equal(&v1, &entry1.value), "Run 1 should have its own value");
    assert!(values_equal(&v2, &entry2.value), "Run 2 should have its own value");

    // Delete in one run doesn't affect other
    substrate.kv_delete(&run1, shared_key).unwrap();
    assert!(
        substrate.kv_get(&run1, shared_key).unwrap().is_none(),
        "Run 1 key should be deleted"
    );
    assert!(
        substrate.kv_get(&run2, shared_key).unwrap().is_some(),
        "Run 2 key should still exist"
    );
}

#[test]
fn test_run_isolation_multiple_runs() {
    let (_, substrate) = quick_setup();
    let test_data = load_kv_test_data();

    // Create 5 runs and populate each with its corresponding test data
    let runs: Vec<ApiRunId> = (0..5)
        .map(|i| if i == 0 { ApiRunId::default() } else { ApiRunId::new() })
        .collect();

    // Store entries in each run
    for (run_idx, run_id) in runs.iter().enumerate() {
        for entry in test_data.get_run(run_idx).iter().take(10) {
            substrate.kv_put(run_id, &entry.key, entry.value.clone()).unwrap();
        }
    }

    // Verify each run only sees its own entries
    for (run_idx, run_id) in runs.iter().enumerate() {
        for entry in test_data.get_run(run_idx).iter().take(10) {
            let result = substrate.kv_get(run_id, &entry.key).unwrap();
            assert!(
                result.is_some(),
                "Run {} should see key '{}'",
                run_idx,
                entry.key
            );
        }

        // Check that keys from other runs are not visible
        for other_idx in 0..5 {
            if other_idx != run_idx {
                // Keys are named with run index prefix, so they won't collide
                let other_entry = &test_data.get_run(other_idx)[0];
                let result = substrate.kv_get(run_id, &other_entry.key).unwrap();
                assert!(
                    result.is_none(),
                    "Run {} should not see key '{}' from run {}",
                    run_idx,
                    other_entry.key,
                    other_idx
                );
            }
        }
    }
}

// =============================================================================
// CROSS-MODE EQUIVALENCE
// =============================================================================

#[test]
fn test_cross_mode_equivalence() {
    let test_data = load_kv_test_data();
    let entry = &test_data.entries[0];

    test_across_modes("basic_put_get", |db| {
        let substrate = create_substrate(db);
        let run = ApiRunId::default();

        substrate
            .kv_put(&run, &entry.key, entry.value.clone())
            .expect("put should succeed");
        substrate.kv_get(&run, &entry.key).unwrap().map(|v| v.value)
    });
}

#[test]
fn test_cross_mode_all_types() {
    let test_data = load_kv_test_data();

    // Test each value type across all modes
    for value_type in ["null", "bool", "int", "float", "string", "bytes", "array", "object"] {
        if let Some(entry) = test_data.get_type(value_type).first() {
            let entry_clone = entry.clone();
            test_across_modes(&format!("type_{}", value_type), move |db| {
                let substrate = create_substrate(db);
                let run = ApiRunId::default();

                substrate
                    .kv_put(&run, &entry_clone.key, entry_clone.value.clone())
                    .expect("put should succeed");

                let result = substrate.kv_get(&run, &entry_clone.key).unwrap();
                result.map(|v| {
                    // Return a comparable representation
                    match &v.value {
                        Value::Float(f) if f.is_nan() => "NaN".to_string(),
                        Value::Float(f) if f.is_infinite() => format!("Inf:{}", f.signum()),
                        other => format!("{:?}", other),
                    }
                })
            });
        }
    }
}
