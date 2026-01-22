//! KV Value Types Tests
//!
//! Tests all 8 value types with edge cases:
//! - Null
//! - Bool (true/false)
//! - Int (positive, negative, MAX, MIN)
//! - Float (positive, negative, infinity, NaN)
//! - String (ASCII, unicode, empty)
//! - Bytes (binary data, empty)
//! - Array (nested structures)
//! - Object (nested structures)
//!
//! All test data is loaded from testdata/kv_test_data.jsonl

use super::*;
use crate::test_data::load_kv_test_data;

// =============================================================================
// NULL VALUES
// =============================================================================

#[test]
fn test_null_value() {
    let (_, substrate) = quick_setup();
    let run = ApiRunId::default();
    let test_data = load_kv_test_data();

    for entry in test_data.get_type("null") {
        substrate.kv_put(&run, &entry.key, entry.value.clone()).unwrap();
        let value = substrate
            .kv_get(&run, &entry.key)
            .unwrap()
            .unwrap()
            .value;
        assert_eq!(value, Value::Null, "Failed for key '{}'", entry.key);
    }
}

// =============================================================================
// BOOL VALUES
// =============================================================================

#[test]
fn test_bool_values() {
    let (_, substrate) = quick_setup();
    let run = ApiRunId::default();
    let test_data = load_kv_test_data();

    for entry in test_data.get_type("bool") {
        substrate.kv_put(&run, &entry.key, entry.value.clone()).unwrap();
        let value = substrate
            .kv_get(&run, &entry.key)
            .unwrap()
            .unwrap()
            .value;
        assert_eq!(value, entry.value, "Failed for key '{}'", entry.key);
    }
}

// =============================================================================
// INT VALUES
// =============================================================================

#[test]
fn test_int_values() {
    let (_, substrate) = quick_setup();
    let run = ApiRunId::default();
    let test_data = load_kv_test_data();

    for entry in test_data.get_type("int") {
        substrate.kv_put(&run, &entry.key, entry.value.clone()).unwrap();
        let result = substrate.kv_get(&run, &entry.key).unwrap().unwrap().value;
        assert_eq!(result, entry.value, "Failed for key '{}'", entry.key);
    }
}

#[test]
fn test_int_boundary_values() {
    let (_, substrate) = quick_setup();
    let run = ApiRunId::default();
    let test_data = load_kv_test_data();

    // Find entries with i64::MAX and i64::MIN from test data
    let int_entries = test_data.get_type("int");

    let max_entry = int_entries.iter().find(|e| {
        matches!(&e.value, Value::Int(n) if *n == i64::MAX)
    });

    let min_entry = int_entries.iter().find(|e| {
        matches!(&e.value, Value::Int(n) if *n == i64::MIN)
    });

    if let Some(entry) = max_entry {
        substrate.kv_put(&run, &entry.key, entry.value.clone()).unwrap();
        let result = substrate.kv_get(&run, &entry.key).unwrap().unwrap().value;
        assert_eq!(result, Value::Int(i64::MAX), "i64::MAX should roundtrip");
    }

    if let Some(entry) = min_entry {
        substrate.kv_put(&run, &entry.key, entry.value.clone()).unwrap();
        let result = substrate.kv_get(&run, &entry.key).unwrap().unwrap().value;
        assert_eq!(result, Value::Int(i64::MIN), "i64::MIN should roundtrip");
    }
}

// =============================================================================
// FLOAT VALUES
// =============================================================================

#[test]
fn test_float_values() {
    let (_, substrate) = quick_setup();
    let run = ApiRunId::default();
    let test_data = load_kv_test_data();

    for entry in test_data.get_type("float") {
        // Skip special float values (Infinity, -Infinity, NaN) for this test
        if let Value::Float(f) = &entry.value {
            if f.is_infinite() || f.is_nan() {
                continue;
            }
        }

        substrate.kv_put(&run, &entry.key, entry.value.clone()).unwrap();
        let result = substrate.kv_get(&run, &entry.key).unwrap().unwrap().value;
        assert!(
            values_equal(&result, &entry.value),
            "Failed for key '{}': expected {:?}, got {:?}",
            entry.key,
            entry.value,
            result
        );
    }
}

#[test]
fn test_float_infinity() {
    let (_, substrate) = quick_setup();
    let run = ApiRunId::default();
    let test_data = load_kv_test_data();

    // Find infinity entries from test data
    for entry in test_data.get_type("float") {
        if let Value::Float(f) = &entry.value {
            if f.is_infinite() {
                substrate.kv_put(&run, &entry.key, entry.value.clone()).unwrap();
                let result = substrate.kv_get(&run, &entry.key).unwrap().unwrap().value;

                match result {
                    Value::Float(rf) => {
                        assert!(rf.is_infinite(), "Should be infinite");
                        assert_eq!(rf.signum(), f.signum(), "Sign should match");
                    }
                    _ => panic!("Expected Float for key '{}'", entry.key),
                }
            }
        }
    }
}

#[test]
fn test_float_nan() {
    let (_, substrate) = quick_setup();
    let run = ApiRunId::default();

    // NaN may not be in test data, so test explicitly
    substrate
        .kv_put(&run, "float_nan", Value::Float(f64::NAN))
        .unwrap();

    let result = substrate
        .kv_get(&run, "float_nan")
        .unwrap()
        .unwrap()
        .value;
    match result {
        Value::Float(f) => assert!(f.is_nan(), "Should be NaN"),
        _ => panic!("Expected Float"),
    }
}

// =============================================================================
// STRING VALUES
// =============================================================================

#[test]
fn test_string_values() {
    let (_, substrate) = quick_setup();
    let run = ApiRunId::default();
    let test_data = load_kv_test_data();

    for entry in test_data.get_type("string") {
        substrate.kv_put(&run, &entry.key, entry.value.clone()).unwrap();
        let result = substrate.kv_get(&run, &entry.key).unwrap().unwrap().value;
        assert_eq!(result, entry.value, "Failed for key '{}'", entry.key);
    }
}

#[test]
fn test_string_edge_cases() {
    let (_, substrate) = quick_setup();
    let run = ApiRunId::default();
    let test_data = load_kv_test_data();

    // Find specific edge case strings from test data
    let string_entries = test_data.get_type("string");

    // Test empty string
    if let Some(entry) = string_entries.iter().find(|e| {
        matches!(&e.value, Value::String(s) if s.is_empty())
    }) {
        substrate.kv_put(&run, &entry.key, entry.value.clone()).unwrap();
        let result = substrate.kv_get(&run, &entry.key).unwrap().unwrap().value;
        assert_eq!(result, Value::String("".to_string()), "Empty string should roundtrip");
    }

    // Test unicode string
    if let Some(entry) = string_entries.iter().find(|e| {
        matches!(&e.value, Value::String(s) if s.contains("🌍") || s.contains("世界"))
    }) {
        substrate.kv_put(&run, &entry.key, entry.value.clone()).unwrap();
        let result = substrate.kv_get(&run, &entry.key).unwrap().unwrap().value;
        assert_eq!(result, entry.value, "Unicode string should roundtrip");
    }
}

// =============================================================================
// BYTES VALUES
// =============================================================================

#[test]
fn test_bytes_values() {
    let (_, substrate) = quick_setup();
    let run = ApiRunId::default();
    let test_data = load_kv_test_data();

    for entry in test_data.get_type("bytes") {
        substrate.kv_put(&run, &entry.key, entry.value.clone()).unwrap();
        let result = substrate.kv_get(&run, &entry.key).unwrap().unwrap().value;
        assert_eq!(result, entry.value, "Failed for key '{}'", entry.key);
    }
}

#[test]
fn test_bytes_vs_string_distinct() {
    let (_, substrate) = quick_setup();
    let run = ApiRunId::default();

    // Same content as bytes vs string should be different types
    substrate
        .kv_put(&run, "as_string", Value::String("hello".to_string()))
        .unwrap();
    substrate
        .kv_put(&run, "as_bytes", Value::Bytes(b"hello".to_vec()))
        .unwrap();

    let string_val = substrate.kv_get(&run, "as_string").unwrap().unwrap().value;
    let bytes_val = substrate.kv_get(&run, "as_bytes").unwrap().unwrap().value;

    assert!(matches!(string_val, Value::String(_)));
    assert!(matches!(bytes_val, Value::Bytes(_)));
    assert_ne!(string_val, bytes_val, "String and Bytes should be distinct");
}

// =============================================================================
// ARRAY VALUES
// =============================================================================

#[test]
fn test_array_values() {
    let (_, substrate) = quick_setup();
    let run = ApiRunId::default();
    let test_data = load_kv_test_data();

    for entry in test_data.get_type("array") {
        substrate.kv_put(&run, &entry.key, entry.value.clone()).unwrap();
        let result = substrate.kv_get(&run, &entry.key).unwrap().unwrap().value;
        assert_eq!(result, entry.value, "Failed for key '{}'", entry.key);
    }
}

#[test]
fn test_array_edge_cases() {
    let (_, substrate) = quick_setup();
    let run = ApiRunId::default();
    let test_data = load_kv_test_data();

    let array_entries = test_data.get_type("array");

    // Test empty array
    if let Some(entry) = array_entries.iter().find(|e| {
        matches!(&e.value, Value::Array(arr) if arr.is_empty())
    }) {
        substrate.kv_put(&run, &entry.key, entry.value.clone()).unwrap();
        let result = substrate.kv_get(&run, &entry.key).unwrap().unwrap().value;
        assert_eq!(result, Value::Array(vec![]), "Empty array should roundtrip");
    }

    // Test nested array
    if let Some(entry) = array_entries.iter().find(|e| {
        matches!(&e.value, Value::Array(arr) if arr.iter().any(|v| matches!(v, Value::Array(_))))
    }) {
        substrate.kv_put(&run, &entry.key, entry.value.clone()).unwrap();
        let result = substrate.kv_get(&run, &entry.key).unwrap().unwrap().value;
        assert_eq!(result, entry.value, "Nested array should roundtrip");
    }
}

// =============================================================================
// OBJECT VALUES
// =============================================================================

#[test]
fn test_object_values() {
    let (_, substrate) = quick_setup();
    let run = ApiRunId::default();
    let test_data = load_kv_test_data();

    for entry in test_data.get_type("object") {
        substrate.kv_put(&run, &entry.key, entry.value.clone()).unwrap();
        let result = substrate.kv_get(&run, &entry.key).unwrap().unwrap().value;
        assert_eq!(result, entry.value, "Failed for key '{}'", entry.key);
    }
}

#[test]
fn test_object_edge_cases() {
    let (_, substrate) = quick_setup();
    let run = ApiRunId::default();
    let test_data = load_kv_test_data();

    let object_entries = test_data.get_type("object");

    // Test empty object
    if let Some(entry) = object_entries.iter().find(|e| {
        matches!(&e.value, Value::Object(obj) if obj.is_empty())
    }) {
        substrate.kv_put(&run, &entry.key, entry.value.clone()).unwrap();
        let result = substrate.kv_get(&run, &entry.key).unwrap().unwrap().value;
        assert_eq!(result, Value::Object(HashMap::new()), "Empty object should roundtrip");
    }

    // Test nested object
    if let Some(entry) = object_entries.iter().find(|e| {
        matches!(&e.value, Value::Object(obj) if obj.values().any(|v| matches!(v, Value::Object(_))))
    }) {
        substrate.kv_put(&run, &entry.key, entry.value.clone()).unwrap();
        let result = substrate.kv_get(&run, &entry.key).unwrap().unwrap().value;
        assert_eq!(result, entry.value, "Nested object should roundtrip");
    }
}

// =============================================================================
// CROSS-MODE EQUIVALENCE FOR ALL TYPES
// =============================================================================

#[test]
fn test_all_types_cross_mode() {
    let test_data = load_kv_test_data();

    for value_type in ["null", "bool", "int", "float", "string", "bytes", "array", "object"] {
        // Get first entry of each type
        if let Some(entry) = test_data.get_type(value_type).first() {
            let entry_clone = entry.clone();
            let type_name = value_type.to_string();

            test_across_modes(&format!("value_type_{}", type_name), move |db| {
                let substrate = create_substrate(db);
                let run = ApiRunId::default();

                substrate.kv_put(&run, &entry_clone.key, entry_clone.value.clone()).unwrap();
                let result = substrate.kv_get(&run, &entry_clone.key).unwrap().map(|v| v.value);

                // Use custom comparison for floats
                match (&result, &entry_clone.value) {
                    (Some(Value::Float(a)), Value::Float(b)) => {
                        if a.is_nan() && b.is_nan() {
                            Some(true)
                        } else if a.is_infinite() && b.is_infinite() {
                            Some(a.signum() == b.signum())
                        } else {
                            Some(a == b)
                        }
                    }
                    (Some(v), expected) => Some(v == expected),
                    (None, _) => None,
                }
            });
        }
    }
}

#[test]
fn test_all_entries_from_run0() {
    let (_, substrate) = quick_setup();
    let run = ApiRunId::default();
    let test_data = load_kv_test_data();

    // Test all 100 entries from run 0
    let mut success_count = 0;
    let mut fail_count = 0;

    for entry in test_data.get_run(0) {
        substrate.kv_put(&run, &entry.key, entry.value.clone()).unwrap();
        let result = substrate.kv_get(&run, &entry.key).unwrap().unwrap().value;

        if values_equal(&result, &entry.value) {
            success_count += 1;
        } else {
            fail_count += 1;
            println!(
                "Mismatch for '{}' ({}): expected {:?}, got {:?}",
                entry.key, entry.value_type, entry.value, result
            );
        }
    }

    assert_eq!(
        fail_count, 0,
        "{} of {} entries failed roundtrip",
        fail_count,
        success_count + fail_count
    );
}
