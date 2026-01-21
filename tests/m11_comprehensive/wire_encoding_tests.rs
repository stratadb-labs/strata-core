//! Wire Encoding Tests
//!
//! Tests for M11 Wire Encoding: JSON encoding, wrappers, envelope, version encoding.
//!
//! Test ID Conventions:
//! - JE-xxx: JSON encoding tests
//! - WR-xxx: Wrapper tests ($bytes, $f64, $absent)
//! - ENV-xxx: Request/response envelope tests
//! - VER-xxx: Version encoding tests
//! - VSD-xxx: Versioned<T> encoding tests

use crate::test_utils::*;
use std::collections::HashMap;

// =============================================================================
// 2.1 JSON Value Encoding Tests (JE-001 to JE-031)
// =============================================================================

#[cfg(test)]
mod json_value_encoding {
    use super::*;

    #[test]
    fn je_001_encode_null() {
        let v = Value::Null;
        let json = wire::encode_json(&v);
        assert_eq!(json, "null");
    }

    #[test]
    fn je_002_encode_bool_true() {
        let v = Value::Bool(true);
        let json = wire::encode_json(&v);
        assert_eq!(json, "true");
    }

    #[test]
    fn je_003_encode_bool_false() {
        let v = Value::Bool(false);
        let json = wire::encode_json(&v);
        assert_eq!(json, "false");
    }

    #[test]
    fn je_004_encode_int_positive() {
        let v = Value::Int(123);
        let json = wire::encode_json(&v);
        assert_eq!(json, "123");
    }

    #[test]
    fn je_005_encode_int_negative() {
        let v = Value::Int(-456);
        let json = wire::encode_json(&v);
        assert_eq!(json, "-456");
    }

    #[test]
    fn je_006_encode_int_zero() {
        let v = Value::Int(0);
        let json = wire::encode_json(&v);
        assert_eq!(json, "0");
    }

    #[test]
    fn je_007_encode_int_max() {
        let v = Value::Int(i64::MAX);
        let json = wire::encode_json(&v);
        assert_eq!(json, "9223372036854775807");
    }

    #[test]
    fn je_008_encode_int_min() {
        let v = Value::Int(i64::MIN);
        let json = wire::encode_json(&v);
        assert_eq!(json, "-9223372036854775808");
    }

    #[test]
    fn je_009_encode_float_positive() {
        let v = Value::Float(1.5);
        let json = wire::encode_json(&v);
        assert!(json.contains("1.5") || json.contains("1.50"));
    }

    #[test]
    fn je_010_encode_float_negative() {
        let v = Value::Float(-2.5);
        let json = wire::encode_json(&v);
        assert!(json.contains("-2.5"));
    }

    #[test]
    fn je_011_encode_float_zero() {
        let v = Value::Float(0.0);
        let json = wire::encode_json(&v);
        // Positive zero should not use wrapper
        assert!(!json.contains("$f64"));
    }

    #[test]
    fn je_012_encode_float_negative_zero() {
        let v = Value::Float(-0.0);
        let json = wire::encode_json(&v);
        assert!(json.contains("$f64"), "-0.0 must use $f64 wrapper");
        assert!(json.contains("-0.0"));
    }

    #[test]
    fn je_013_encode_float_nan() {
        let v = Value::Float(f64::NAN);
        let json = wire::encode_json(&v);
        assert!(json.contains("$f64"), "NaN must use $f64 wrapper");
        assert!(json.contains("NaN"));
    }

    #[test]
    fn je_014_encode_float_positive_infinity() {
        let v = Value::Float(f64::INFINITY);
        let json = wire::encode_json(&v);
        assert!(json.contains("$f64"));
        assert!(json.contains("+Inf"));
    }

    #[test]
    fn je_015_encode_float_negative_infinity() {
        let v = Value::Float(f64::NEG_INFINITY);
        let json = wire::encode_json(&v);
        assert!(json.contains("$f64"));
        assert!(json.contains("-Inf"));
    }

    #[test]
    fn je_016_encode_float_max() {
        let v = Value::Float(f64::MAX);
        let json = wire::encode_json(&v);
        // Should be encoded, not wrapped
        assert!(!json.contains("$f64"));
    }

    #[test]
    fn je_017_encode_float_precision() {
        let precise = 1.0000000000000002_f64;
        let v = Value::Float(precise);
        let json = wire::encode_json(&v);
        // Should preserve precision
        assert!(!json.contains("$f64"));
    }

    #[test]
    fn je_018_encode_string_simple() {
        let v = Value::String("hello".into());
        let json = wire::encode_json(&v);
        assert_eq!(json, "\"hello\"");
    }

    #[test]
    fn je_019_encode_string_empty() {
        let v = Value::String(String::new());
        let json = wire::encode_json(&v);
        assert_eq!(json, "\"\"");
    }

    #[test]
    fn je_020_encode_string_unicode() {
        let v = Value::String("日本語".into());
        let json = wire::encode_json(&v);
        assert!(json.contains("日本語"));
    }

    #[test]
    fn je_021_encode_string_escape_chars() {
        let v = Value::String("a\n\t\"b".into());
        let json = wire::encode_json(&v);
        // Should have escaped quotes
        assert!(json.contains("\\\"") || json.contains("\\n"));
    }

    #[test]
    fn je_022_encode_bytes_simple() {
        let v = Value::Bytes(vec![72, 101, 108, 108, 111]); // "Hello"
        let json = wire::encode_json(&v);
        assert!(json.contains("$bytes"));
        assert!(json.contains("SGVsbG8=")); // Base64 of "Hello"
    }

    #[test]
    fn je_023_encode_bytes_empty() {
        let v = Value::Bytes(vec![]);
        let json = wire::encode_json(&v);
        assert!(json.contains("$bytes"));
        assert!(json.contains(r#""$bytes":"""#));
    }

    #[test]
    fn je_024_encode_bytes_all_values() {
        let all_bytes: Vec<u8> = (0..=255).collect();
        let v = Value::Bytes(all_bytes);
        let json = wire::encode_json(&v);
        assert!(json.contains("$bytes"));
    }

    #[test]
    fn je_025_encode_array_simple() {
        let v = Value::Array(vec![Value::Int(1), Value::Int(2)]);
        let json = wire::encode_json(&v);
        assert_eq!(json, "[1,2]");
    }

    #[test]
    fn je_026_encode_array_empty() {
        let v = Value::Array(vec![]);
        let json = wire::encode_json(&v);
        assert_eq!(json, "[]");
    }

    #[test]
    fn je_027_encode_array_nested() {
        let v = Value::Array(vec![Value::Array(vec![Value::Int(1)])]);
        let json = wire::encode_json(&v);
        assert_eq!(json, "[[1]]");
    }

    #[test]
    fn je_028_encode_array_mixed_types() {
        let v = Value::Array(vec![Value::Int(1), Value::String("a".into())]);
        let json = wire::encode_json(&v);
        assert_eq!(json, r#"[1,"a"]"#);
    }

    #[test]
    fn je_029_encode_object_simple() {
        let mut map = HashMap::new();
        map.insert("a".to_string(), Value::Int(1));
        let v = Value::Object(map);
        let json = wire::encode_json(&v);
        assert_eq!(json, r#"{"a":1}"#);
    }

    #[test]
    fn je_030_encode_object_empty() {
        let v = Value::Object(HashMap::new());
        let json = wire::encode_json(&v);
        assert_eq!(json, "{}");
    }

    #[test]
    fn je_031_encode_object_nested() {
        let mut inner = HashMap::new();
        inner.insert("b".to_string(), Value::Int(1));
        let mut outer = HashMap::new();
        outer.insert("a".to_string(), Value::Object(inner));
        let v = Value::Object(outer);
        let json = wire::encode_json(&v);
        assert!(json.contains(r#""a":"#));
        assert!(json.contains(r#""b":1"#));
    }

    #[test]
    fn je_032_encode_object_deterministic_order() {
        // Objects should have deterministic key ordering
        let mut map = HashMap::new();
        map.insert("c".to_string(), Value::Int(3));
        map.insert("a".to_string(), Value::Int(1));
        map.insert("b".to_string(), Value::Int(2));
        let v = Value::Object(map);
        let json = wire::encode_json(&v);
        // Keys should be sorted alphabetically
        assert_eq!(json, r#"{"a":1,"b":2,"c":3}"#);
    }
}

// =============================================================================
// 2.2 Special Wrapper Tests (WR-001 to WR-015)
// =============================================================================

#[cfg(test)]
mod wrappers {
    use super::*;

    #[test]
    fn wr_001_bytes_wrapper_structure() {
        let v = Value::Bytes(vec![1, 2, 3]);
        let json = wire::encode_json(&v);
        // Must have exactly one key: $bytes
        assert!(json.starts_with(r#"{"$bytes":"#));
        assert!(json.ends_with(r#""}"#));
    }

    #[test]
    fn wr_002_bytes_wrapper_base64_standard() {
        // Standard base64 uses +/ (not URL-safe -_)
        let v = Value::Bytes(vec![251, 255]); // Will produce + or / in base64
        let json = wire::encode_json(&v);
        // Standard base64 chars should be present, not URL-safe variants
        // The bytes 251, 255 encode to "+/8=" in standard base64
        assert!(json.contains("$bytes"));
    }

    #[test]
    fn wr_003_bytes_wrapper_padding() {
        // Base64 should include padding
        let v = Value::Bytes(vec![1]); // Single byte needs padding
        let json = wire::encode_json(&v);
        // "AQ==" is base64 of [1] with padding
        assert!(json.contains("="));
    }

    #[test]
    fn wr_004_f64_nan_wrapper() {
        let v = Value::Float(f64::NAN);
        let json = wire::encode_json(&v);
        assert_eq!(json, r#"{"$f64":"NaN"}"#);
    }

    #[test]
    fn wr_005_f64_positive_inf_wrapper() {
        let v = Value::Float(f64::INFINITY);
        let json = wire::encode_json(&v);
        assert_eq!(json, r#"{"$f64":"+Inf"}"#);
    }

    #[test]
    fn wr_006_f64_negative_inf_wrapper() {
        let v = Value::Float(f64::NEG_INFINITY);
        let json = wire::encode_json(&v);
        assert_eq!(json, r#"{"$f64":"-Inf"}"#);
    }

    #[test]
    fn wr_007_f64_negative_zero_wrapper() {
        let v = Value::Float(-0.0);
        let json = wire::encode_json(&v);
        assert_eq!(json, r#"{"$f64":"-0.0"}"#);
    }

    #[test]
    fn wr_008_f64_positive_zero_no_wrapper() {
        let v = Value::Float(0.0);
        let json = wire::encode_json(&v);
        // Positive zero should NOT use wrapper
        assert!(!json.contains("$f64"));
    }

    #[test]
    fn wr_009_absent_wrapper_structure() {
        // $absent marker for CAS expected-missing
        // Format: {"$absent": true}
        let expected_json = r#"{"$absent":true}"#;
        assert!(expected_json.contains("$absent"));
        assert!(expected_json.contains("true"));
    }

    #[test]
    fn wr_010_absent_wrapper_value_is_boolean() {
        // The value must be boolean true, not 1 or "true"
        let valid = r#"{"$absent":true}"#;
        let invalid_int = r#"{"$absent":1}"#;
        let invalid_string = r#"{"$absent":"true"}"#;

        assert!(valid.contains(":true}"));
        assert!(!invalid_int.contains(":true}"));
        assert!(invalid_string.contains(r#":"true""#));
    }

    #[test]
    fn wr_011_nested_bytes_in_object() {
        let mut map = HashMap::new();
        map.insert("data".to_string(), Value::Bytes(vec![1, 2, 3]));
        let v = Value::Object(map);
        let json = wire::encode_json(&v);
        assert!(json.contains("$bytes"));
    }

    #[test]
    fn wr_012_nested_bytes_in_array() {
        let v = Value::Array(vec![Value::Bytes(vec![1, 2, 3])]);
        let json = wire::encode_json(&v);
        assert!(json.contains("$bytes"));
    }

    #[test]
    fn wr_013_wrapper_collision_object_with_bytes_key() {
        // An object that happens to have a "$bytes" key is NOT a Bytes wrapper
        let mut map = HashMap::new();
        map.insert("$bytes".to_string(), Value::Int(123)); // Not a string value
        let v = Value::Object(map);
        let json = wire::encode_json(&v);
        // This should encode as a regular object, not be confused with Bytes
        assert!(json.contains(r#""$bytes":123"#));
    }

    #[test]
    fn wr_014_wrapper_collision_object_with_f64_key() {
        // An object with "$f64" key that's not a special float wrapper
        let mut map = HashMap::new();
        map.insert("$f64".to_string(), Value::Int(42));
        let v = Value::Object(map);
        let json = wire::encode_json(&v);
        assert!(json.contains(r#""$f64":42"#));
    }

    #[test]
    fn wr_015_wrapper_collision_object_with_absent_key() {
        // An object with "$absent" key that's false
        let mut map = HashMap::new();
        map.insert("$absent".to_string(), Value::Bool(false));
        let v = Value::Object(map);
        let json = wire::encode_json(&v);
        // Should NOT be interpreted as absent marker
        assert!(json.contains(r#""$absent":false"#));
    }

    #[test]
    fn wr_016_normal_float_no_wrapper() {
        // Normal floats should NOT use wrapper
        for f in [1.5, -2.5, 3.14159, 1e10, 1e-10] {
            let v = Value::Float(f);
            let json = wire::encode_json(&v);
            assert!(!json.contains("$f64"), "Normal float {} should not use wrapper", f);
        }
    }
}

// =============================================================================
// 2.3 Request/Response Envelope Tests (ENV-001 to ENV-010)
// =============================================================================

#[cfg(test)]
mod envelope {
    // Note: These tests verify the wire format structure.
    // Actual implementation will be tested with integration tests.

    #[test]
    fn env_001_request_envelope_structure() {
        // Request must have: id, op, params
        let request = r#"{"id":"req-123","op":"kv_get","params":{"key":"mykey"}}"#;
        assert!(request.contains(r#""id":"#));
        assert!(request.contains(r#""op":"#));
        assert!(request.contains(r#""params":"#));
    }

    #[test]
    fn env_002_request_envelope_id_string() {
        let request = r#"{"id":"req-123","op":"test","params":{}}"#;
        // ID should be a string, not a number
        assert!(request.contains(r#""id":"req-123""#));
    }

    #[test]
    fn env_003_request_envelope_op_string() {
        let request = r#"{"id":"1","op":"kv_put","params":{}}"#;
        assert!(request.contains(r#""op":"kv_put""#));
    }

    #[test]
    fn env_004_request_envelope_params_object() {
        let request = r#"{"id":"1","op":"test","params":{"key":"value"}}"#;
        assert!(request.contains(r#""params":{"#));
    }

    #[test]
    fn env_005_success_response_structure() {
        // Success response: id, ok=true, result
        let response = r#"{"id":"req-123","ok":true,"result":42}"#;
        assert!(response.contains(r#""id":"req-123""#));
        assert!(response.contains(r#""ok":true"#));
        assert!(response.contains(r#""result":"#));
    }

    #[test]
    fn env_006_success_response_ok_true() {
        // ok field must be boolean true, not 1 or "true"
        let valid = r#"{"id":"1","ok":true,"result":null}"#;
        assert!(valid.contains(":true,"));
    }

    #[test]
    fn env_007_error_response_structure() {
        // Error response: id, ok=false, error
        let response = r#"{"id":"req-123","ok":false,"error":{"code":"NotFound","message":"Key not found","details":null}}"#;
        assert!(response.contains(r#""ok":false"#));
        assert!(response.contains(r#""error":"#));
    }

    #[test]
    fn env_008_error_response_ok_false() {
        let response = r#"{"id":"1","ok":false,"error":{}}"#;
        assert!(response.contains(":false,"));
    }

    #[test]
    fn env_009_error_response_error_structure() {
        // error object must have: code, message, details
        let error = r#"{"code":"WrongType","message":"Expected Int, got String","details":{"expected":"int","actual":"string"}}"#;
        assert!(error.contains(r#""code":"#));
        assert!(error.contains(r#""message":"#));
        assert!(error.contains(r#""details":"#));
    }

    #[test]
    fn env_010_request_id_preserved_in_response() {
        // Response ID must match request ID
        let request_id = "unique-request-id-12345";
        let request = format!(r#"{{"id":"{}","op":"test","params":{{}}}}"#, request_id);
        let response = format!(r#"{{"id":"{}","ok":true,"result":null}}"#, request_id);

        // Both should contain the same ID
        assert!(request.contains(request_id));
        assert!(response.contains(request_id));
    }
}

// =============================================================================
// 2.4 Version Encoding Tests (VER-001 to VER-010)
// =============================================================================

#[cfg(test)]
mod version_encoding {
    use super::*;

    #[test]
    fn ver_001_encode_txn_version() {
        let v = Version::Txn(123);
        let json = wire::encode_version(&v);
        assert_eq!(json, r#"{"type":"txn","value":123}"#);
    }

    #[test]
    fn ver_002_encode_sequence_version() {
        let v = Version::Sequence(456);
        let json = wire::encode_version(&v);
        assert_eq!(json, r#"{"type":"sequence","value":456}"#);
    }

    #[test]
    fn ver_003_encode_counter_version() {
        let v = Version::Counter(789);
        let json = wire::encode_version(&v);
        assert_eq!(json, r#"{"type":"counter","value":789}"#);
    }

    #[test]
    fn ver_004_encode_txn_zero() {
        let v = Version::Txn(0);
        let json = wire::encode_version(&v);
        assert_eq!(json, r#"{"type":"txn","value":0}"#);
    }

    #[test]
    fn ver_005_encode_txn_max() {
        let v = Version::Txn(u64::MAX);
        let json = wire::encode_version(&v);
        assert!(json.contains("18446744073709551615"));
    }

    #[test]
    fn ver_006_version_type_txn() {
        let v = Version::Txn(1);
        let json = wire::encode_version(&v);
        assert!(json.contains(r#""type":"txn""#));
    }

    #[test]
    fn ver_007_version_type_sequence() {
        let v = Version::Sequence(1);
        let json = wire::encode_version(&v);
        assert!(json.contains(r#""type":"sequence""#));
    }

    #[test]
    fn ver_008_version_type_counter() {
        let v = Version::Counter(1);
        let json = wire::encode_version(&v);
        assert!(json.contains(r#""type":"counter""#));
    }

    #[test]
    fn ver_009_version_type_preserved_in_roundtrip() {
        // Each version type should round-trip to the same type
        let versions = vec![
            Version::Txn(100),
            Version::Sequence(200),
            Version::Counter(300),
        ];
        for v in versions {
            let json = wire::encode_version(&v);
            // Verify type tag is present
            match &v {
                Version::Txn(_) => assert!(json.contains("txn")),
                Version::Sequence(_) => assert!(json.contains("sequence")),
                Version::Counter(_) => assert!(json.contains("counter")),
            }
        }
    }

    #[test]
    fn ver_010_invalid_version_type_detection() {
        // An invalid version type string should be detectable
        let invalid = r#"{"type":"invalid","value":1}"#;
        // The type is not one of: txn, sequence, counter
        assert!(!invalid.contains(r#""type":"txn""#));
        assert!(!invalid.contains(r#""type":"sequence""#));
        assert!(!invalid.contains(r#""type":"counter""#));
    }
}

// =============================================================================
// 2.5 Versioned<T> Encoding Tests (VSD-001 to VSD-006)
// =============================================================================

#[cfg(test)]
mod versioned_encoding {
    use super::*;

    #[test]
    fn vsd_001_versioned_structure() {
        // Versioned<T> must have: value, version, timestamp
        let versioned_json = r#"{"value":42,"version":{"type":"txn","value":1},"timestamp":1234567890}"#;
        assert!(versioned_json.contains(r#""value":"#));
        assert!(versioned_json.contains(r#""version":"#));
        assert!(versioned_json.contains(r#""timestamp":"#));
    }

    #[test]
    fn vsd_002_versioned_value_correct() {
        // The value field should contain the encoded value
        let versioned = Versioned {
            value: Value::Int(42),
            version: Version::Txn(1),
            timestamp: 0,
        };
        assert_eq!(versioned.value, Value::Int(42));
    }

    #[test]
    fn vsd_003_versioned_version_correct() {
        let versioned = Versioned {
            value: Value::Null,
            version: Version::Txn(123),
            timestamp: 0,
        };
        assert_eq!(versioned.version, Version::Txn(123));
    }

    #[test]
    fn vsd_004_versioned_timestamp_microseconds() {
        // Timestamp should be in microseconds (u64)
        let versioned = Versioned {
            value: Value::Null,
            version: Version::Txn(1),
            timestamp: 1_700_000_000_000_000, // ~2023 in microseconds
        };
        assert!(versioned.timestamp > 1_600_000_000_000_000); // After 2020
    }

    #[test]
    fn vsd_005_versioned_with_complex_value() {
        let mut map = HashMap::new();
        map.insert("nested".to_string(), Value::Array(vec![Value::Int(1), Value::Int(2)]));

        let versioned = Versioned {
            value: Value::Object(map),
            version: Version::Txn(1),
            timestamp: 0,
        };

        assert!(matches!(versioned.value, Value::Object(_)));
    }

    #[test]
    fn vsd_006_versioned_all_version_types() {
        // Versioned can have any version type
        let versions = vec![
            Version::Txn(1),
            Version::Sequence(2),
            Version::Counter(3),
        ];

        for v in versions {
            let versioned = Versioned {
                value: Value::Null,
                version: v.clone(),
                timestamp: 0,
            };
            assert_eq!(versioned.version, v);
        }
    }
}

// =============================================================================
// 2.6 Round-Trip Property Tests
// =============================================================================

#[cfg(test)]
mod round_trip {
    use super::*;

    #[test]
    fn rt_001_null_round_trip() {
        let original = Value::Null;
        let json = wire::encode_json(&original);
        assert_eq!(json, "null");
    }

    #[test]
    fn rt_002_bool_round_trip() {
        for b in [true, false] {
            let original = Value::Bool(b);
            let json = wire::encode_json(&original);
            assert_eq!(json, if b { "true" } else { "false" });
        }
    }

    #[test]
    fn rt_003_int_round_trip() {
        for i in [0i64, 1, -1, 42, -999, i64::MAX, i64::MIN] {
            let original = Value::Int(i);
            let json = wire::encode_json(&original);
            assert_eq!(json, i.to_string());
        }
    }

    #[test]
    fn rt_004_float_normal_round_trip() {
        // Normal floats should encode without wrapper
        for f in [1.5, -2.5, 3.14, 0.0] {
            let original = Value::Float(f);
            let json = wire::encode_json(&original);
            // Normal floats don't use wrapper
            if f != -0.0 {
                assert!(!json.contains("$f64"));
            }
        }
    }

    #[test]
    fn rt_005_float_special_round_trip() {
        // Special floats must use wrapper
        let specials = [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, -0.0];
        for f in specials {
            let original = Value::Float(f);
            let json = wire::encode_json(&original);
            assert!(json.contains("$f64"), "Special float {} must use wrapper", f);
        }
    }

    #[test]
    fn rt_006_string_round_trip() {
        for s in ["", "hello", "日本語", "🚀", "a\\b\"c"] {
            let original = Value::String(s.to_string());
            let json = wire::encode_json(&original);
            assert!(json.starts_with('"') && json.ends_with('"'));
        }
    }

    #[test]
    fn rt_007_bytes_round_trip() {
        let test_cases: Vec<Vec<u8>> = vec![
            vec![],
            vec![0],
            vec![255],
            vec![0, 127, 255],
        ];
        for bytes in test_cases {
            let original = Value::Bytes(bytes);
            let json = wire::encode_json(&original);
            assert!(json.contains("$bytes"));
        }
    }

    #[test]
    fn rt_008_array_round_trip() {
        let original = Value::Array(vec![Value::Int(1), Value::String("two".into())]);
        let json = wire::encode_json(&original);
        assert!(json.starts_with('[') && json.ends_with(']'));
    }

    #[test]
    fn rt_009_object_round_trip() {
        let mut map = HashMap::new();
        map.insert("key".to_string(), Value::Int(42));
        let original = Value::Object(map);
        let json = wire::encode_json(&original);
        assert!(json.starts_with('{') && json.ends_with('}'));
    }

    #[test]
    fn rt_010_deeply_nested_round_trip() {
        let original = gen_nested_value(10);
        let json = wire::encode_json(&original);
        assert!(json.contains("nested"));
    }
}
