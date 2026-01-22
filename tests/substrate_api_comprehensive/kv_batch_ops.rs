//! KV Batch Operations Tests
//!
//! Tests for batch operations:
//! - mget: Get multiple keys at once
//! - mput: Put multiple key-value pairs atomically
//! - mdelete: Delete multiple keys
//! - mexists: Check existence of multiple keys
//!
//! All test data is loaded from testdata/kv_test_data.jsonl

use super::*;
use crate::test_data::load_kv_test_data;

// =============================================================================
// MGET TESTS
// =============================================================================

#[test]
fn test_mget_all_exist() {
    let (_, substrate) = quick_setup();
    let run = ApiRunId::default();
    let test_data = load_kv_test_data();

    // Use first 5 entries from run 0
    let entries: Vec<_> = test_data.get_run(0).iter().take(5).collect();
    assert!(entries.len() >= 5, "Need at least 5 entries");

    // Setup keys using testdata
    for entry in &entries {
        substrate.kv_put(&run, &entry.key, entry.value.clone()).unwrap();
    }

    // Collect keys for mget
    let key_strings: Vec<String> = entries.iter().map(|e| e.key.clone()).collect();
    let keys: Vec<&str> = key_strings.iter().map(|s| s.as_str()).collect();

    let results = substrate.kv_mget(&run, &keys).expect("mget should succeed");

    assert_eq!(results.len(), 5, "Should return 5 results");
    for (i, result) in results.iter().enumerate() {
        assert!(result.is_some(), "Key {} should exist", i);
        assert!(
            values_equal(&result.as_ref().unwrap().value, &entries[i].value),
            "Value {} should match",
            i
        );
    }
}

#[test]
fn test_mget_preserves_order() {
    let (_, substrate) = quick_setup();
    let run = ApiRunId::default();
    let test_data = load_kv_test_data();

    // Use 3 entries from test data
    let entries: Vec<_> = test_data.get_run(0).iter().take(3).collect();
    assert!(entries.len() >= 3, "Need at least 3 entries");

    for entry in &entries {
        substrate.kv_put(&run, &entry.key, entry.value.clone()).unwrap();
    }

    // Request in reverse order
    let key_strings: Vec<String> = entries.iter().rev().map(|e| e.key.clone()).collect();
    let keys: Vec<&str> = key_strings.iter().map(|s| s.as_str()).collect();

    let results = substrate.kv_mget(&run, &keys).unwrap();

    // Verify order matches request order (reversed)
    for (i, result) in results.iter().enumerate() {
        let expected_entry = &entries[entries.len() - 1 - i];
        assert!(
            values_equal(&result.as_ref().unwrap().value, &expected_entry.value),
            "Position {} should have correct value",
            i
        );
    }
}

#[test]
fn test_mget_with_missing_keys() {
    let (_, substrate) = quick_setup();
    let run = ApiRunId::default();
    let test_data = load_kv_test_data();

    // Use 2 entries from test data
    let entries: Vec<_> = test_data.get_run(0).iter().take(2).collect();
    assert!(entries.len() >= 2, "Need at least 2 entries");

    for entry in &entries {
        substrate.kv_put(&run, &entry.key, entry.value.clone()).unwrap();
    }

    // Mix existing and non-existing keys
    let keys = [
        entries[0].key.as_str(),
        "missing_key_xyz_12345",
        entries[1].key.as_str(),
        "another_missing_key_99999",
    ];
    let results = substrate.kv_mget(&run, &keys).unwrap();

    assert!(results[0].is_some(), "First entry should be Some");
    assert!(results[1].is_none(), "missing1 should be None");
    assert!(results[2].is_some(), "Second entry should be Some");
    assert!(results[3].is_none(), "missing2 should be None");
}

// =============================================================================
// MGET EDGE CASES
// =============================================================================

#[test]
fn test_mget_empty() {
    let (_, substrate) = quick_setup();
    let run = ApiRunId::default();

    let keys: Vec<&str> = vec![];
    let results = substrate.kv_mget(&run, &keys).unwrap();
    assert!(results.is_empty(), "Empty mget should return empty vec");
}

// =============================================================================
// MPUT TESTS
// =============================================================================

#[test]
fn test_mput_basic() {
    let (_, substrate) = quick_setup();
    let run = ApiRunId::default();
    let test_data = load_kv_test_data();

    // Use entries from test data
    let entries: Vec<_> = test_data.get_run(0).iter().take(3).collect();
    assert!(entries.len() >= 3, "Need at least 3 entries");

    // Prepare entries for mput
    let mput_entries: Vec<(&str, Value)> = entries
        .iter()
        .map(|e| (e.key.as_str(), e.value.clone()))
        .collect();

    substrate.kv_mput(&run, &mput_entries).expect("mput should succeed");

    // Verify all written
    for entry in &entries {
        let result = substrate.kv_get(&run, &entry.key).unwrap().unwrap().value;
        assert!(
            values_equal(&result, &entry.value),
            "Key {} should have correct value",
            entry.key
        );
    }
}

#[test]
fn test_mput_shares_version() {
    let (_, substrate) = quick_setup();
    let run = ApiRunId::default();
    let test_data = load_kv_test_data();

    // Use entries from test data
    let entries: Vec<_> = test_data.get_run(0).iter().take(3).collect();
    assert!(entries.len() >= 3, "Need at least 3 entries");

    let mput_entries: Vec<(&str, Value)> = entries
        .iter()
        .map(|e| (e.key.as_str(), e.value.clone()))
        .collect();

    substrate.kv_mput(&run, &mput_entries).unwrap();

    // All entries should share the same version (atomic batch)
    let versions: Vec<_> = entries
        .iter()
        .map(|e| substrate.kv_get(&run, &e.key).unwrap().unwrap().version)
        .collect();

    for i in 1..versions.len() {
        assert_eq!(
            versions[0], versions[i],
            "Entry {} should share version with entry 0",
            i
        );
    }
}

#[test]
fn test_mput_overwrites() {
    let (_, substrate) = quick_setup();
    let run = ApiRunId::default();
    let test_data = load_kv_test_data();

    // Get different value types for overwrite test
    let int_entries: Vec<_> = test_data.get_type("int").iter().take(2).collect();
    let string_entries: Vec<_> = test_data.get_type("string").iter().take(2).collect();
    assert!(int_entries.len() >= 2 && string_entries.len() >= 2, "Need enough entries");

    // Use fixed keys for overwrite test
    let key_a = "overwrite_batch_key_a";
    let key_b = "overwrite_batch_key_b";
    let key_c = "overwrite_batch_key_c";

    // Initial values
    substrate.kv_put(&run, key_a, int_entries[0].value.clone()).unwrap();
    substrate.kv_put(&run, key_b, int_entries[1].value.clone()).unwrap();

    // mput with new values (different values)
    let mput_entries: Vec<(&str, Value)> = vec![
        (key_a, string_entries[0].value.clone()),
        (key_b, string_entries[1].value.clone()),
        (key_c, int_entries[0].value.clone()), // new key
    ];

    substrate.kv_mput(&run, &mput_entries).unwrap();

    // Verify overwrites
    assert!(
        values_equal(
            &substrate.kv_get(&run, key_a).unwrap().unwrap().value,
            &string_entries[0].value
        ),
        "key_a should have new value"
    );
    assert!(
        values_equal(
            &substrate.kv_get(&run, key_b).unwrap().unwrap().value,
            &string_entries[1].value
        ),
        "key_b should have new value"
    );
    assert!(
        values_equal(
            &substrate.kv_get(&run, key_c).unwrap().unwrap().value,
            &int_entries[0].value
        ),
        "key_c should be created"
    );
}

// =============================================================================
// MDELETE TESTS
// =============================================================================

#[test]
fn test_mdelete_basic() {
    let (_, substrate) = quick_setup();
    let run = ApiRunId::default();
    let test_data = load_kv_test_data();

    // Use entries from test data
    let entries: Vec<_> = test_data.get_run(0).iter().take(3).collect();
    assert!(entries.len() >= 3, "Need at least 3 entries");

    // Setup keys
    for entry in &entries {
        substrate.kv_put(&run, &entry.key, entry.value.clone()).unwrap();
    }

    // Delete first and third
    let keys_to_delete = [entries[0].key.as_str(), entries[2].key.as_str()];
    let deleted = substrate.kv_mdelete(&run, &keys_to_delete).expect("mdelete should succeed");

    assert_eq!(deleted, 2, "Should report 2 deleted");

    assert!(
        substrate.kv_get(&run, &entries[0].key).unwrap().is_none(),
        "First key should be gone"
    );
    assert!(
        substrate.kv_get(&run, &entries[1].key).unwrap().is_some(),
        "Second key should still exist"
    );
    assert!(
        substrate.kv_get(&run, &entries[2].key).unwrap().is_none(),
        "Third key should be gone"
    );
}

#[test]
fn test_mdelete_with_missing() {
    let (_, substrate) = quick_setup();
    let run = ApiRunId::default();
    let test_data = load_kv_test_data();

    // Use one entry from test data
    let entry = &test_data.entries[0];
    substrate.kv_put(&run, &entry.key, entry.value.clone()).unwrap();

    let keys = [entry.key.as_str(), "missing_xyz_12345", "missing_abc_99999"];
    let deleted = substrate.kv_mdelete(&run, &keys).unwrap();

    assert_eq!(deleted, 1, "Should only count existing keys deleted");
}

// =============================================================================
// MEXISTS TESTS
// =============================================================================

#[test]
fn test_mexists_basic() {
    let (_, substrate) = quick_setup();
    let run = ApiRunId::default();
    let test_data = load_kv_test_data();

    // Use entries from test data
    let entries: Vec<_> = test_data.get_run(0).iter().take(2).collect();
    assert!(entries.len() >= 2, "Need at least 2 entries");

    for entry in &entries {
        substrate.kv_put(&run, &entry.key, entry.value.clone()).unwrap();
    }

    let keys = [
        entries[0].key.as_str(),
        entries[1].key.as_str(),
        "missing_key_xyz_99999",
    ];
    let count = substrate.kv_mexists(&run, &keys).expect("mexists should succeed");

    assert_eq!(count, 2, "Should count 2 existing keys");
}

#[test]
fn test_mexists_none_exist() {
    let (_, substrate) = quick_setup();
    let run = ApiRunId::default();

    // Use keys that definitely don't exist
    let keys = ["nonexistent_batch_1", "nonexistent_batch_2", "nonexistent_batch_3"];
    let count = substrate.kv_mexists(&run, &keys).unwrap();

    assert_eq!(count, 0, "Should count 0 when none exist");
}

#[test]
fn test_mexists_all_exist() {
    let (_, substrate) = quick_setup();
    let run = ApiRunId::default();
    let test_data = load_kv_test_data();

    // Use entries from test data
    let entries: Vec<_> = test_data.get_run(0).iter().take(3).collect();
    assert!(entries.len() >= 3, "Need at least 3 entries");

    for entry in &entries {
        substrate.kv_put(&run, &entry.key, entry.value.clone()).unwrap();
    }

    let keys: Vec<&str> = entries.iter().map(|e| e.key.as_str()).collect();
    let count = substrate.kv_mexists(&run, &keys).unwrap();

    assert_eq!(count, 3, "Should count all 3");
}

// =============================================================================
// CROSS-MODE EQUIVALENCE
// =============================================================================

#[test]
fn test_batch_ops_cross_mode() {
    let test_data = load_kv_test_data();
    let entries: Vec<_> = test_data.get_run(0).iter().take(3).collect();

    // Store the entries for use in closure
    let entry_data: Vec<(String, Value)> = entries
        .iter()
        .map(|e| (e.key.clone(), e.value.clone()))
        .collect();

    test_across_modes("batch_mput_mget", move |db| {
        let substrate = create_substrate(db);
        let run = ApiRunId::default();

        let mput_entries: Vec<(&str, Value)> = entry_data
            .iter()
            .map(|(k, v)| (k.as_str(), v.clone()))
            .collect();
        substrate.kv_mput(&run, &mput_entries).unwrap();

        let keys: Vec<&str> = entry_data.iter().map(|(k, _)| k.as_str()).collect();
        let results: Vec<_> = substrate
            .kv_mget(&run, &keys)
            .unwrap()
            .into_iter()
            .map(|r| r.map(|v| format!("{:?}", v.value)))
            .collect();

        results
    });
}
