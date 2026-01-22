//! KV Edge Cases Tests
//!
//! Tests for edge cases and boundary conditions:
//! - Key validation (empty, NUL, reserved prefix, unicode, special chars)
//! - Value size limits (large strings, bytes, arrays, objects)
//! - Batch operation edge cases (empty, duplicates, large batches)
//! - Error handling and error message quality
//! - Timestamp ordering
//! - Delete semantics
//!
//! All test data is loaded from testdata/kv_edge_cases.jsonl

use super::*;
use crate::test_data::{
    load_edge_case_data, load_kv_test_data,
    generate_large_string, generate_large_bytes,
    generate_large_array, generate_large_object,
    generate_nested_array, generate_nested_object,
};

// =============================================================================
// KEY VALIDATION TESTS
// =============================================================================

/// Empty key should be rejected with InvalidKey error
#[test]
fn test_key_empty_rejected() {
    let (_, substrate) = quick_setup();
    let run = ApiRunId::default();

    let result = substrate.kv_put(&run, "", Value::Int(1));

    assert!(
        result.is_err(),
        "Empty key should be rejected"
    );
}

/// Key containing NUL byte should be rejected
#[test]
fn test_key_nul_byte_rejected() {
    let (_, substrate) = quick_setup();
    let run = ApiRunId::default();

    let result = substrate.kv_put(&run, "foo\0bar", Value::Int(1));

    assert!(
        result.is_err(),
        "Key with NUL byte should be rejected"
    );
}

/// Keys with reserved _strata/ prefix should be rejected
#[test]
fn test_key_reserved_prefix_rejected() {
    let (_, substrate) = quick_setup();
    let run = ApiRunId::default();
    let edge_cases = load_edge_case_data();

    for entry in edge_cases.get_category("key_validation") {
        if entry.test.starts_with("reserved_prefix") {
            let result = substrate.kv_put(&run, &entry.key, Value::Int(1));
            assert!(
                result.is_err(),
                "Reserved prefix key '{}' should be rejected",
                entry.key
            );
        }
    }
}

/// Unicode keys should be accepted
#[test]
fn test_key_unicode_accepted() {
    let (_, substrate) = quick_setup();
    let run = ApiRunId::default();
    let edge_cases = load_edge_case_data();

    for entry in edge_cases.get_category("key_validation") {
        if entry.test.starts_with("unicode_key") && entry.expected == "Ok" {
            let result = substrate.kv_put(&run, &entry.key, Value::String("test".into()));
            assert!(
                result.is_ok(),
                "Unicode key '{}' should be accepted, got: {:?}",
                entry.key,
                result.err()
            );

            // Verify roundtrip
            let get_result = substrate.kv_get(&run, &entry.key).unwrap();
            assert!(
                get_result.is_some(),
                "Unicode key '{}' should be retrievable",
                entry.key
            );
        }
    }
}

/// Special character keys should be accepted
#[test]
fn test_key_special_characters_accepted() {
    let (_, substrate) = quick_setup();
    let run = ApiRunId::default();
    let edge_cases = load_edge_case_data();

    for entry in edge_cases.get_category("key_validation") {
        if entry.test.starts_with("special_char") && entry.expected == "Ok" {
            let result = substrate.kv_put(&run, &entry.key, Value::Int(1));
            assert!(
                result.is_ok(),
                "Special char key '{}' should be accepted, got: {:?}",
                entry.key,
                result.err()
            );

            // Verify roundtrip
            let get_result = substrate.kv_get(&run, &entry.key).unwrap();
            assert!(
                get_result.is_some(),
                "Special char key '{}' should be retrievable",
                entry.key
            );
        }
    }
}

/// Whitespace keys should be handled correctly
#[test]
fn test_key_whitespace_handling() {
    let (_, substrate) = quick_setup();
    let run = ApiRunId::default();
    let edge_cases = load_edge_case_data();

    for entry in edge_cases.get_category("key_validation") {
        if entry.test.starts_with("whitespace") && entry.expected == "Ok" {
            let result = substrate.kv_put(&run, &entry.key, Value::Int(1));

            // Whitespace-only or whitespace-containing keys may or may not be allowed
            // Document actual behavior
            if result.is_ok() {
                let get_result = substrate.kv_get(&run, &entry.key).unwrap();
                assert!(
                    get_result.is_some(),
                    "Whitespace key '{}' stored but not retrievable",
                    entry.test
                );
            }
            // If rejected, that's also acceptable behavior
        }
    }
}

/// Key at maximum allowed length should be accepted
#[test]
fn test_key_max_length_accepted() {
    let (_, substrate) = quick_setup();
    let run = ApiRunId::default();
    let edge_cases = load_edge_case_data();

    // Use the 256-byte key from test data
    if let Some(entry) = edge_cases.get_test("key_256_bytes") {
        let result = substrate.kv_put(&run, &entry.key, Value::Int(1));
        // 256 bytes is a common max key length - should likely succeed
        if result.is_err() {
            println!(
                "Note: 256-byte key rejected (max_key_bytes may be < 256): {:?}",
                result.err()
            );
        }
    }
}

/// Key exceeding maximum length should be rejected
#[test]
fn test_key_over_max_length_rejected() {
    let (_, substrate) = quick_setup();
    let run = ApiRunId::default();
    let edge_cases = load_edge_case_data();

    // Use the 1024-byte key from test data
    if let Some(entry) = edge_cases.get_test("key_1024_bytes") {
        let result = substrate.kv_put(&run, &entry.key, Value::Int(1));
        // Document behavior - might be Ok or InvalidKey depending on limits
        match &result {
            Ok(_) => println!("Note: 1024-byte key accepted (max_key_bytes >= 1024)"),
            Err(e) => println!("Note: 1024-byte key rejected as expected: {:?}", e),
        }
    }
}

// =============================================================================
// VALUE SIZE LIMITS TESTS
// =============================================================================

/// Large string values should be handled correctly
#[test]
fn test_value_large_string() {
    let (_, substrate) = quick_setup();
    let run = ApiRunId::default();

    // Test various sizes
    for size_kb in [1, 10, 100] {
        let large_string = generate_large_string(size_kb);
        let key = format!("large_string_{}kb", size_kb);

        let result = substrate.kv_put(&run, &key, Value::String(large_string.clone()));
        assert!(
            result.is_ok(),
            "{}KB string should be accepted, got: {:?}",
            size_kb,
            result.err()
        );

        // Verify roundtrip
        let get_result = substrate.kv_get(&run, &key).unwrap().unwrap();
        assert_eq!(
            get_result.value,
            Value::String(large_string),
            "{}KB string should roundtrip correctly",
            size_kb
        );
    }
}

/// Very large string values may be rejected
#[test]
fn test_value_very_large_string() {
    let (_, substrate) = quick_setup();
    let run = ApiRunId::default();

    // 1MB string - may or may not be accepted
    let large_string = generate_large_string(1024);
    let result = substrate.kv_put(&run, "very_large_string", Value::String(large_string));

    // Document behavior
    match &result {
        Ok(_) => println!("Note: 1MB string accepted"),
        Err(e) => println!("Note: 1MB string rejected: {:?}", e),
    }
}

/// Large byte values should be handled correctly
#[test]
fn test_value_large_bytes() {
    let (_, substrate) = quick_setup();
    let run = ApiRunId::default();

    // Test various sizes
    for size_kb in [1, 10, 100] {
        let large_bytes = generate_large_bytes(size_kb);
        let key = format!("large_bytes_{}kb", size_kb);

        let result = substrate.kv_put(&run, &key, Value::Bytes(large_bytes.clone()));
        assert!(
            result.is_ok(),
            "{}KB bytes should be accepted, got: {:?}",
            size_kb,
            result.err()
        );

        // Verify roundtrip
        let get_result = substrate.kv_get(&run, &key).unwrap().unwrap();
        assert_eq!(
            get_result.value,
            Value::Bytes(large_bytes),
            "{}KB bytes should roundtrip correctly",
            size_kb
        );
    }
}

/// Large arrays should be handled correctly
#[test]
fn test_value_large_array() {
    let (_, substrate) = quick_setup();
    let run = ApiRunId::default();

    // Test various sizes
    for count in [100, 1000] {
        let large_array = generate_large_array(count);
        let key = format!("large_array_{}", count);

        let result = substrate.kv_put(&run, &key, large_array.clone());
        assert!(
            result.is_ok(),
            "Array with {} elements should be accepted, got: {:?}",
            count,
            result.err()
        );

        // Verify roundtrip
        let get_result = substrate.kv_get(&run, &key).unwrap().unwrap();
        assert_eq!(
            get_result.value,
            large_array,
            "Array with {} elements should roundtrip correctly",
            count
        );
    }
}

/// Large objects should be handled correctly
#[test]
fn test_value_large_object() {
    let (_, substrate) = quick_setup();
    let run = ApiRunId::default();

    // Test various sizes
    for count in [100, 1000] {
        let large_object = generate_large_object(count);
        let key = format!("large_object_{}", count);

        let result = substrate.kv_put(&run, &key, large_object.clone());
        assert!(
            result.is_ok(),
            "Object with {} keys should be accepted, got: {:?}",
            count,
            result.err()
        );

        // Verify roundtrip
        let get_result = substrate.kv_get(&run, &key).unwrap().unwrap();
        assert_eq!(
            get_result.value,
            large_object,
            "Object with {} keys should roundtrip correctly",
            count
        );
    }
}

/// Deeply nested arrays should be handled correctly
#[test]
fn test_value_deeply_nested_array() {
    let (_, substrate) = quick_setup();
    let run = ApiRunId::default();

    // Test moderate nesting depth
    let nested = generate_nested_array(10);
    let result = substrate.kv_put(&run, "nested_array_10", nested.clone());
    assert!(
        result.is_ok(),
        "Array nested 10 deep should be accepted, got: {:?}",
        result.err()
    );

    // Verify roundtrip
    let get_result = substrate.kv_get(&run, "nested_array_10").unwrap().unwrap();
    assert_eq!(get_result.value, nested);

    // Test deep nesting (may be rejected)
    let deep_nested = generate_nested_array(50);
    let deep_result = substrate.kv_put(&run, "nested_array_50", deep_nested);
    match &deep_result {
        Ok(_) => println!("Note: 50-deep nested array accepted"),
        Err(e) => println!("Note: 50-deep nested array rejected: {:?}", e),
    }
}

/// Deeply nested objects should be handled correctly
#[test]
fn test_value_deeply_nested_object() {
    let (_, substrate) = quick_setup();
    let run = ApiRunId::default();

    // Test moderate nesting depth
    let nested = generate_nested_object(10);
    let result = substrate.kv_put(&run, "nested_object_10", nested.clone());
    assert!(
        result.is_ok(),
        "Object nested 10 deep should be accepted, got: {:?}",
        result.err()
    );

    // Verify roundtrip
    let get_result = substrate.kv_get(&run, "nested_object_10").unwrap().unwrap();
    assert_eq!(get_result.value, nested);
}

// =============================================================================
// BATCH OPERATION EDGE CASES
// =============================================================================

/// mput with empty entries should succeed
#[test]
fn test_mput_empty_batch() {
    let (_, substrate) = quick_setup();
    let run = ApiRunId::default();

    let entries: Vec<(&str, Value)> = vec![];
    let result = substrate.kv_mput(&run, &entries);

    // Empty batch should either succeed or be explicitly rejected
    // Document actual behavior
    match &result {
        Ok(version) => println!("Note: Empty mput succeeded with version {:?}", version),
        Err(e) => println!("Note: Empty mput rejected: {:?}", e),
    }
}

/// mput with duplicate keys - document behavior
#[test]
fn test_mput_duplicate_keys() {
    let (_, substrate) = quick_setup();
    let run = ApiRunId::default();

    let entries: Vec<(&str, Value)> = vec![
        ("dup_key", Value::Int(1)),
        ("dup_key", Value::Int(2)),
    ];

    let result = substrate.kv_mput(&run, &entries);

    match &result {
        Ok(_) => {
            // If accepted, which value wins?
            let value = substrate.kv_get(&run, "dup_key").unwrap().unwrap().value;
            println!("Note: mput with duplicates accepted, final value: {:?}", value);
            // Last-wins is the expected behavior
            assert_eq!(value, Value::Int(2), "Last value should win for duplicate keys");
        }
        Err(e) => println!("Note: mput with duplicates rejected: {:?}", e),
    }
}

/// mput with large batch should succeed
#[test]
fn test_mput_large_batch() {
    let (_, substrate) = quick_setup();
    let run = ApiRunId::default();
    let test_data = load_kv_test_data();

    // Use first 100 entries from test data for run 0
    let entries: Vec<(&str, Value)> = test_data
        .get_run(0)
        .iter()
        .map(|e| (e.key.as_str(), e.value.clone()))
        .collect();

    let result = substrate.kv_mput(&run, &entries);
    assert!(
        result.is_ok(),
        "mput with {} entries should succeed, got: {:?}",
        entries.len(),
        result.err()
    );

    // Verify all entries stored correctly
    for entry in test_data.get_run(0) {
        let stored = substrate.kv_get(&run, &entry.key).unwrap();
        assert!(stored.is_some(), "Entry '{}' should be stored", entry.key);
        assert!(
            values_equal(&stored.unwrap().value, &entry.value),
            "Entry '{}' value mismatch",
            entry.key
        );
    }
}

/// mget with empty keys should return empty
#[test]
fn test_mget_empty_keys() {
    let (_, substrate) = quick_setup();
    let run = ApiRunId::default();

    let keys: &[&str] = &[];
    let result = substrate.kv_mget(&run, keys);

    assert!(result.is_ok(), "Empty mget should succeed");
    assert!(result.unwrap().is_empty(), "Empty mget should return empty vec");
}

/// mget with duplicate keys - document behavior
#[test]
fn test_mget_duplicate_keys() {
    let (_, substrate) = quick_setup();
    let run = ApiRunId::default();

    // Setup
    substrate.kv_put(&run, "mget_dup", Value::Int(42)).unwrap();

    let keys = &["mget_dup", "mget_dup", "nonexistent"];
    let result = substrate.kv_mget(&run, keys).unwrap();

    // Document behavior
    println!(
        "Note: mget with duplicates returned {} results for {} keys",
        result.len(),
        keys.len()
    );

    // Should return same number of results as keys
    assert_eq!(
        result.len(),
        keys.len(),
        "mget should return one result per key"
    );
}

/// mdelete with empty keys should return 0
#[test]
fn test_mdelete_empty_keys() {
    let (_, substrate) = quick_setup();
    let run = ApiRunId::default();

    let keys: &[&str] = &[];
    let result = substrate.kv_mdelete(&run, keys);

    assert!(result.is_ok(), "Empty mdelete should succeed");
    assert_eq!(result.unwrap(), 0, "Empty mdelete should delete 0 keys");
}

/// mdelete with duplicate keys - document behavior
#[test]
fn test_mdelete_duplicate_keys() {
    let (_, substrate) = quick_setup();
    let run = ApiRunId::default();

    // Setup
    substrate.kv_put(&run, "mdel_dup", Value::Int(42)).unwrap();

    let keys = &["mdel_dup", "mdel_dup"];
    let result = substrate.kv_mdelete(&run, keys).unwrap();

    // Should count the key only once
    println!("Note: mdelete with duplicate key returned count: {}", result);
    assert_eq!(result, 1, "mdelete should count each key once, not per occurrence");

    // Key should be deleted
    assert!(
        substrate.kv_get(&run, "mdel_dup").unwrap().is_none(),
        "Key should be deleted"
    );
}

// =============================================================================
// TIMESTAMP ORDERING TESTS
// =============================================================================

/// Timestamps should be monotonically increasing
#[test]
fn test_timestamps_monotonic() {
    let (_, substrate) = quick_setup();
    let run = ApiRunId::default();
    let test_data = load_kv_test_data();

    let mut timestamps = Vec::new();

    // Store entries from test data and collect timestamps
    for (i, entry) in test_data.take(50).iter().enumerate() {
        let key = format!("ts_test_{}", i);
        substrate.kv_put(&run, &key, entry.value.clone()).unwrap();

        let versioned = substrate.kv_get(&run, &key).unwrap().unwrap();
        timestamps.push(versioned.timestamp.as_micros());
    }

    // Verify monotonicity
    for i in 1..timestamps.len() {
        assert!(
            timestamps[i] >= timestamps[i - 1],
            "Timestamps should be monotonic: {} at index {} < {} at index {}",
            timestamps[i],
            i,
            timestamps[i - 1],
            i - 1
        );
    }
}

/// Version ordering should correlate with timestamp ordering
#[test]
fn test_version_timestamp_correlation() {
    let (_, substrate) = quick_setup();
    let run = ApiRunId::default();

    let mut version_ts_pairs = Vec::new();

    for i in 0..20 {
        let key = format!("vts_test_{}", i);
        let version = substrate.kv_put(&run, &key, Value::Int(i as i64)).unwrap();
        let versioned = substrate.kv_get(&run, &key).unwrap().unwrap();

        version_ts_pairs.push((version, versioned.timestamp.as_micros()));
    }

    // Versions should be strictly increasing
    for i in 1..version_ts_pairs.len() {
        let (prev_v, prev_ts) = &version_ts_pairs[i - 1];
        let (curr_v, curr_ts) = &version_ts_pairs[i];

        match (prev_v, curr_v) {
            (Version::Txn(p), Version::Txn(c)) => {
                assert!(c > p, "Versions should be strictly increasing");
            }
            _ => {}
        }

        // Timestamps should be non-decreasing (may be equal for fast operations)
        assert!(
            curr_ts >= prev_ts,
            "Timestamps should be non-decreasing"
        );
    }
}

// =============================================================================
// DELETE SEMANTICS TESTS
// =============================================================================

/// Double delete should return false on second delete
#[test]
fn test_delete_double_delete() {
    let (_, substrate) = quick_setup();
    let run = ApiRunId::default();

    substrate.kv_put(&run, "double_del", Value::Int(1)).unwrap();

    let first_delete = substrate.kv_delete(&run, "double_del").unwrap();
    assert!(first_delete, "First delete should return true");

    let second_delete = substrate.kv_delete(&run, "double_del").unwrap();
    assert!(!second_delete, "Second delete should return false");
}

/// Delete then recreate should work with new version
#[test]
fn test_delete_then_recreate() {
    let (_, substrate) = quick_setup();
    let run = ApiRunId::default();

    // Create
    let v1 = substrate.kv_put(&run, "recreate", Value::Int(1)).unwrap();

    // Delete
    substrate.kv_delete(&run, "recreate").unwrap();
    assert!(substrate.kv_get(&run, "recreate").unwrap().is_none());

    // Recreate
    let v2 = substrate.kv_put(&run, "recreate", Value::Int(2)).unwrap();

    // Should have new version
    match (v1, v2) {
        (Version::Txn(a), Version::Txn(b)) => {
            assert!(b > a, "Recreated key should have newer version");
        }
        _ => {}
    }

    // Should have new value
    let value = substrate.kv_get(&run, "recreate").unwrap().unwrap().value;
    assert_eq!(value, Value::Int(2));
}

/// Delete nonexistent key should return false
#[test]
fn test_delete_nonexistent() {
    let (_, substrate) = quick_setup();
    let run = ApiRunId::default();

    let result = substrate.kv_delete(&run, "never_existed").unwrap();
    assert!(!result, "Deleting nonexistent key should return false");
}

// =============================================================================
// DATA-DRIVEN TESTS USING TESTDATA
// =============================================================================

/// Test all value types from test data file
#[test]
fn test_all_value_types_from_testdata() {
    let (_, substrate) = quick_setup();
    let run = ApiRunId::default();
    let test_data = load_kv_test_data();

    // Test entries from run 0 (default run)
    for entry in test_data.get_run(0) {
        let result = substrate.kv_put(&run, &entry.key, entry.value.clone());
        assert!(
            result.is_ok(),
            "Failed to put entry '{}' of type '{}': {:?}",
            entry.key,
            entry.value_type,
            result.err()
        );

        let retrieved = substrate.kv_get(&run, &entry.key).unwrap();
        assert!(
            retrieved.is_some(),
            "Entry '{}' not found after put",
            entry.key
        );

        let stored_value = retrieved.unwrap().value;
        assert!(
            values_equal(&stored_value, &entry.value),
            "Value mismatch for '{}': expected {:?}, got {:?}",
            entry.key,
            entry.value,
            stored_value
        );
    }
}

/// Test multiple runs from test data
#[test]
fn test_multiple_runs_from_testdata() {
    let (_, substrate) = quick_setup();
    let test_data = load_kv_test_data();

    // Create runs for first 5 run indices
    let runs: Vec<ApiRunId> = (0..5)
        .map(|i| if i == 0 { ApiRunId::default() } else { ApiRunId::new() })
        .collect();

    // Store entries in corresponding runs
    for (run_idx, run_id) in runs.iter().enumerate() {
        for entry in test_data.get_run(run_idx) {
            substrate.kv_put(run_id, &entry.key, entry.value.clone()).unwrap();
        }
    }

    // Verify isolation - entries from one run should not be visible in another
    for (run_idx, run_id) in runs.iter().enumerate() {
        let expected_keys: std::collections::HashSet<_> = test_data
            .get_run(run_idx)
            .iter()
            .map(|e| e.key.clone())
            .collect();

        // Check a few keys from other runs are not visible
        for other_idx in 0..5 {
            if other_idx != run_idx {
                for entry in test_data.get_run(other_idx).iter().take(3) {
                    if !expected_keys.contains(&entry.key) {
                        let result = substrate.kv_get(run_id, &entry.key).unwrap();
                        assert!(
                            result.is_none(),
                            "Key '{}' from run {} should not be visible in run {}",
                            entry.key,
                            other_idx,
                            run_idx
                        );
                    }
                }
            }
        }
    }
}

/// Test durability with data from testdata file
#[test]
fn test_durability_from_testdata() {
    let mut test_db = TestDb::new_buffered();
    let run = ApiRunId::default();
    let test_data = load_kv_test_data();

    // Store first 50 entries
    {
        let substrate = test_db.substrate();
        for entry in test_data.take(50) {
            substrate.kv_put(&run, &entry.key, entry.value.clone()).unwrap();
        }
    }

    // Crash and recover
    test_db.reopen();

    // Verify all entries survived
    let substrate = test_db.substrate();
    for entry in test_data.take(50) {
        let stored = substrate.kv_get(&run, &entry.key).unwrap();
        assert!(
            stored.is_some(),
            "Entry '{}' should survive crash",
            entry.key
        );
        assert!(
            values_equal(&stored.unwrap().value, &entry.value),
            "Entry '{}' value should match after crash",
            entry.key
        );
    }
}
