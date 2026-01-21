//! Value Model Tests
//!
//! Tests for M11 Value Model: construction, equality, no coercion, size limits, and key validation.
//!
//! Test ID Conventions:
//! - VAL-xxx: Value construction tests
//! - FLT-xxx: Float edge case tests
//! - EQ-xxx: Equality tests
//! - NC-xxx: No coercion tests
//! - SL-xxx: Size limit tests
//! - KV-xxx: Key validation tests

use crate::test_utils::*;
use std::collections::HashMap;

// =============================================================================
// 1.1 Type Construction Tests (VAL-001 to VAL-035)
// =============================================================================

#[cfg(test)]
mod construction {
    use super::*;

    #[test]
    fn val_001_null_construction() {
        let v = Value::Null;
        assert_eq!(v.type_name(), "null");
    }

    #[test]
    fn val_002_bool_true_construction() {
        let v = Value::Bool(true);
        assert!(matches!(v, Value::Bool(true)));
    }

    #[test]
    fn val_003_bool_false_construction() {
        let v = Value::Bool(false);
        assert!(matches!(v, Value::Bool(false)));
    }

    #[test]
    fn val_004_int_positive_construction() {
        let v = Value::Int(123);
        assert!(matches!(v, Value::Int(123)));
    }

    #[test]
    fn val_005_int_negative_construction() {
        let v = Value::Int(-456);
        assert!(matches!(v, Value::Int(-456)));
    }

    #[test]
    fn val_006_int_zero_construction() {
        let v = Value::Int(0);
        assert!(matches!(v, Value::Int(0)));
    }

    #[test]
    fn val_007_int_max_construction() {
        let v = Value::Int(i64::MAX);
        assert!(matches!(v, Value::Int(i64::MAX)));
    }

    #[test]
    fn val_008_int_min_construction() {
        let v = Value::Int(i64::MIN);
        assert!(matches!(v, Value::Int(i64::MIN)));
    }

    #[test]
    fn val_009_float_positive_construction() {
        let v = Value::Float(1.23);
        match v {
            Value::Float(f) => assert!((f - 1.23).abs() < f64::EPSILON),
            _ => panic!("Expected Float"),
        }
    }

    #[test]
    fn val_010_float_negative_construction() {
        let v = Value::Float(-4.56);
        match v {
            Value::Float(f) => assert!((f - (-4.56)).abs() < f64::EPSILON),
            _ => panic!("Expected Float"),
        }
    }

    #[test]
    fn val_011_float_zero_construction() {
        let v = Value::Float(0.0);
        match v {
            Value::Float(f) => assert_eq!(f, 0.0),
            _ => panic!("Expected Float"),
        }
    }

    #[test]
    fn val_012_float_negative_zero_construction() {
        let v = Value::Float(-0.0);
        match v {
            Value::Float(f) => {
                assert!(f.is_sign_negative(), "-0.0 must preserve sign");
            }
            _ => panic!("Expected Float"),
        }
    }

    #[test]
    fn val_013_float_nan_construction() {
        let v = Value::Float(f64::NAN);
        match v {
            Value::Float(f) => assert!(f.is_nan()),
            _ => panic!("Expected Float"),
        }
    }

    #[test]
    fn val_014_float_positive_infinity_construction() {
        let v = Value::Float(f64::INFINITY);
        match v {
            Value::Float(f) => {
                assert!(f.is_infinite());
                assert!(f.is_sign_positive());
            }
            _ => panic!("Expected Float"),
        }
    }

    #[test]
    fn val_015_float_negative_infinity_construction() {
        let v = Value::Float(f64::NEG_INFINITY);
        match v {
            Value::Float(f) => {
                assert!(f.is_infinite());
                assert!(f.is_sign_negative());
            }
            _ => panic!("Expected Float"),
        }
    }

    #[test]
    fn val_016_float_max_construction() {
        let v = Value::Float(f64::MAX);
        match v {
            Value::Float(f) => assert_eq!(f, f64::MAX),
            _ => panic!("Expected Float"),
        }
    }

    #[test]
    fn val_017_float_min_positive_construction() {
        let v = Value::Float(f64::MIN_POSITIVE);
        match v {
            Value::Float(f) => assert_eq!(f, f64::MIN_POSITIVE),
            _ => panic!("Expected Float"),
        }
    }

    #[test]
    fn val_018_float_subnormal_construction() {
        let subnormal = f64::from_bits(1); // Smallest positive subnormal
        assert!(subnormal.is_subnormal());
        let v = Value::Float(subnormal);
        match v {
            Value::Float(f) => {
                assert!(f.is_subnormal());
                assert_eq!(f.to_bits(), 1);
            }
            _ => panic!("Expected Float"),
        }
    }

    #[test]
    fn val_019_string_empty_construction() {
        let v = Value::String(String::new());
        assert!(matches!(v, Value::String(s) if s.is_empty()));
    }

    #[test]
    fn val_020_string_ascii_construction() {
        let v = Value::String("hello".to_string());
        assert!(matches!(v, Value::String(s) if s == "hello"));
    }

    #[test]
    fn val_021_string_unicode_construction() {
        let v = Value::String("こんにちは".to_string());
        assert!(matches!(v, Value::String(s) if s == "こんにちは"));
    }

    #[test]
    fn val_022_string_emoji_construction() {
        let v = Value::String("🚀🎉".to_string());
        assert!(matches!(v, Value::String(s) if s == "🚀🎉"));
    }

    #[test]
    fn val_023_string_surrogate_pairs_construction() {
        // 𝄞 is U+1D11E, represented as surrogate pair in UTF-16
        let v = Value::String("𝄞".to_string());
        assert!(matches!(v, Value::String(s) if s == "𝄞"));
    }

    #[test]
    fn val_024_bytes_empty_construction() {
        let v = Value::Bytes(vec![]);
        assert!(matches!(v, Value::Bytes(b) if b.is_empty()));
    }

    #[test]
    fn val_025_bytes_binary_construction() {
        let v = Value::Bytes(vec![0, 255, 128]);
        assert!(matches!(v, Value::Bytes(b) if b == vec![0, 255, 128]));
    }

    #[test]
    fn val_026_bytes_all_values_construction() {
        let all_bytes: Vec<u8> = (0..=255).collect();
        let v = Value::Bytes(all_bytes.clone());
        assert!(matches!(v, Value::Bytes(b) if b == all_bytes));
    }

    #[test]
    fn val_027_array_empty_construction() {
        let v = Value::Array(vec![]);
        assert!(matches!(v, Value::Array(a) if a.is_empty()));
    }

    #[test]
    fn val_028_array_single_element_construction() {
        let v = Value::Array(vec![Value::Int(1)]);
        match v {
            Value::Array(a) => {
                assert_eq!(a.len(), 1);
                assert!(matches!(a[0], Value::Int(1)));
            }
            _ => panic!("Expected Array"),
        }
    }

    #[test]
    fn val_029_array_mixed_types_construction() {
        let v = Value::Array(vec![
            Value::Int(1),
            Value::String("two".into()),
            Value::Bool(true),
            Value::Null,
        ]);
        match v {
            Value::Array(a) => assert_eq!(a.len(), 4),
            _ => panic!("Expected Array"),
        }
    }

    #[test]
    fn val_030_array_nested_construction() {
        let inner = Value::Array(vec![Value::Int(1)]);
        let v = Value::Array(vec![inner]);
        match v {
            Value::Array(outer) => {
                assert_eq!(outer.len(), 1);
                assert!(matches!(&outer[0], Value::Array(_)));
            }
            _ => panic!("Expected Array"),
        }
    }

    #[test]
    fn val_031_object_empty_construction() {
        let v = Value::Object(HashMap::new());
        assert!(matches!(v, Value::Object(o) if o.is_empty()));
    }

    #[test]
    fn val_032_object_single_entry_construction() {
        let mut map = HashMap::new();
        map.insert("key".to_string(), Value::Int(42));
        let v = Value::Object(map);
        match v {
            Value::Object(o) => {
                assert_eq!(o.len(), 1);
                assert!(matches!(o.get("key"), Some(Value::Int(42))));
            }
            _ => panic!("Expected Object"),
        }
    }

    #[test]
    fn val_033_object_multiple_entries_construction() {
        let mut map = HashMap::new();
        map.insert("a".to_string(), Value::Int(1));
        map.insert("b".to_string(), Value::Int(2));
        map.insert("c".to_string(), Value::Int(3));
        let v = Value::Object(map);
        match v {
            Value::Object(o) => assert_eq!(o.len(), 3),
            _ => panic!("Expected Object"),
        }
    }

    #[test]
    fn val_034_object_nested_construction() {
        let mut inner = HashMap::new();
        inner.insert("nested".to_string(), Value::Int(1));
        let mut outer = HashMap::new();
        outer.insert("inner".to_string(), Value::Object(inner));
        let v = Value::Object(outer);
        match v {
            Value::Object(o) => {
                assert!(matches!(o.get("inner"), Some(Value::Object(_))));
            }
            _ => panic!("Expected Object"),
        }
    }

    #[test]
    fn val_035_deeply_nested_construction() {
        // Create max nesting depth (128)
        let v = gen_nested_value(128);
        assert!(matches!(v, Value::Object(_)));
    }
}

// =============================================================================
// 1.2 Float Edge Case Tests (FLT-001 to FLT-015)
// =============================================================================

#[cfg(test)]
mod float_edge_cases {
    use super::*;

    #[test]
    fn flt_001_nan_is_nan() {
        assert!(f64::NAN.is_nan());
    }

    #[test]
    fn flt_002_nan_not_equal_to_self() {
        // CRITICAL: NaN != NaN (IEEE-754)
        let v1 = Value::Float(f64::NAN);
        let v2 = Value::Float(f64::NAN);
        assert_ne!(v1, v2, "NaN must not equal NaN");
    }

    #[test]
    fn flt_003_nan_not_equal_to_other_nan() {
        // Different NaN payloads should also not be equal
        let nan1 = f64::from_bits(0x7ff8000000000001);
        let nan2 = f64::from_bits(0x7ff8000000000002);
        assert!(nan1.is_nan());
        assert!(nan2.is_nan());
        assert_ne!(Value::Float(nan1), Value::Float(nan2));
    }

    #[test]
    fn flt_004_positive_infinity_is_infinite() {
        assert!(f64::INFINITY.is_infinite());
        assert!(f64::INFINITY.is_sign_positive());
    }

    #[test]
    fn flt_005_negative_infinity_is_infinite() {
        assert!(f64::NEG_INFINITY.is_infinite());
        assert!(f64::NEG_INFINITY.is_sign_negative());
    }

    #[test]
    fn flt_006_positive_infinity_equals_self() {
        let v1 = Value::Float(f64::INFINITY);
        let v2 = Value::Float(f64::INFINITY);
        assert_eq!(v1, v2);
    }

    #[test]
    fn flt_007_negative_infinity_equals_self() {
        let v1 = Value::Float(f64::NEG_INFINITY);
        let v2 = Value::Float(f64::NEG_INFINITY);
        assert_eq!(v1, v2);
    }

    #[test]
    fn flt_008_positive_negative_infinity_not_equal() {
        let pos = Value::Float(f64::INFINITY);
        let neg = Value::Float(f64::NEG_INFINITY);
        assert_ne!(pos, neg);
    }

    #[test]
    fn flt_009_negative_zero_equals_positive_zero() {
        // CRITICAL: -0.0 == 0.0 (IEEE-754)
        let neg_zero = Value::Float(-0.0);
        let pos_zero = Value::Float(0.0);
        assert_eq!(neg_zero, pos_zero, "-0.0 must equal 0.0 per IEEE-754");
    }

    #[test]
    fn flt_010_negative_zero_preserved_in_storage() {
        let v = Value::Float(-0.0);
        match v {
            Value::Float(f) => {
                assert!(f.is_sign_negative(), "-0.0 sign must be preserved");
                assert_eq!(f.to_bits(), (-0.0_f64).to_bits());
            }
            _ => panic!("Expected Float"),
        }
    }

    #[test]
    fn flt_011_negative_zero_preserved_on_wire() {
        let v = Value::Float(-0.0);
        let encoded = wire::encode_json(&v);
        assert!(encoded.contains("$f64"), "-0.0 must use $f64 wrapper");
        assert!(encoded.contains("-0.0"));
    }

    #[test]
    fn flt_012_subnormal_values_preserved() {
        let subnormal = f64::from_bits(1);
        assert!(subnormal.is_subnormal());
        let v = Value::Float(subnormal);
        match v {
            Value::Float(f) => {
                assert!(f.is_subnormal(), "Subnormal must be preserved");
                assert!(!f.is_normal());
            }
            _ => panic!("Expected Float"),
        }
    }

    #[test]
    fn flt_013_max_float_preserved() {
        let v = Value::Float(f64::MAX);
        match v {
            Value::Float(f) => assert_eq!(f, f64::MAX),
            _ => panic!("Expected Float"),
        }
    }

    #[test]
    fn flt_014_min_positive_float_preserved() {
        let v = Value::Float(f64::MIN_POSITIVE);
        match v {
            Value::Float(f) => assert_eq!(f, f64::MIN_POSITIVE),
            _ => panic!("Expected Float"),
        }
    }

    #[test]
    fn flt_015_float_precision_preserved() {
        // This is the smallest increment from 1.0
        let precise = 1.0000000000000002_f64;
        let v = Value::Float(precise);
        match v {
            Value::Float(f) => {
                assert_eq!(f.to_bits(), precise.to_bits(), "Full f64 precision must be preserved");
            }
            _ => panic!("Expected Float"),
        }
    }
}

// =============================================================================
// 1.3 Value Equality Tests (EQ-001 to EQ-030)
// =============================================================================

#[cfg(test)]
mod equality {
    use super::*;

    #[test]
    fn eq_001_null_equals_null() {
        assert_eq!(Value::Null, Value::Null);
    }

    #[test]
    fn eq_002_null_not_equals_bool() {
        assert_ne!(Value::Null, Value::Bool(false));
        assert_ne!(Value::Null, Value::Bool(true));
    }

    #[test]
    fn eq_003_null_not_equals_int_zero() {
        assert_ne!(Value::Null, Value::Int(0));
    }

    #[test]
    fn eq_004_bool_true_equals_true() {
        assert_eq!(Value::Bool(true), Value::Bool(true));
    }

    #[test]
    fn eq_005_bool_false_equals_false() {
        assert_eq!(Value::Bool(false), Value::Bool(false));
    }

    #[test]
    fn eq_006_bool_true_not_equals_false() {
        assert_ne!(Value::Bool(true), Value::Bool(false));
    }

    #[test]
    fn eq_007_bool_not_equals_int_one() {
        // CRITICAL: No type coercion
        assert_ne!(Value::Bool(true), Value::Int(1));
    }

    #[test]
    fn eq_008_int_equals_same_int() {
        assert_eq!(Value::Int(42), Value::Int(42));
    }

    #[test]
    fn eq_009_int_not_equals_different_int() {
        assert_ne!(Value::Int(42), Value::Int(43));
    }

    #[test]
    fn eq_010_int_not_equals_float() {
        // CRITICAL: Int(1) != Float(1.0)
        assert_ne!(Value::Int(1), Value::Float(1.0), "Int(1) must NOT equal Float(1.0)");
    }

    #[test]
    fn eq_011_int_zero_not_equals_float_zero() {
        // CRITICAL: Int(0) != Float(0.0)
        assert_ne!(Value::Int(0), Value::Float(0.0), "Int(0) must NOT equal Float(0.0)");
    }

    #[test]
    fn eq_012_float_equals_same_float() {
        assert_eq!(Value::Float(3.14), Value::Float(3.14));
    }

    #[test]
    fn eq_013_float_nan_not_equals_nan() {
        // CRITICAL: NaN != NaN
        assert_ne!(Value::Float(f64::NAN), Value::Float(f64::NAN));
    }

    #[test]
    fn eq_014_float_negative_zero_equals_zero() {
        assert_eq!(Value::Float(-0.0), Value::Float(0.0));
    }

    #[test]
    fn eq_015_string_equals_same_string() {
        assert_eq!(Value::String("a".into()), Value::String("a".into()));
    }

    #[test]
    fn eq_016_string_not_equals_different_string() {
        assert_ne!(Value::String("a".into()), Value::String("b".into()));
    }

    #[test]
    fn eq_017_string_empty_equals_empty() {
        assert_eq!(Value::String(String::new()), Value::String(String::new()));
    }

    #[test]
    fn eq_018_string_not_equals_bytes() {
        // CRITICAL: String("abc") != Bytes([97,98,99])
        assert_ne!(
            Value::String("abc".into()),
            Value::Bytes(vec![97, 98, 99]),
            "String must NOT equal Bytes with same content"
        );
    }

    #[test]
    fn eq_019_bytes_equals_same_bytes() {
        assert_eq!(Value::Bytes(vec![1, 2]), Value::Bytes(vec![1, 2]));
    }

    #[test]
    fn eq_020_bytes_not_equals_different_bytes() {
        assert_ne!(Value::Bytes(vec![1, 2]), Value::Bytes(vec![1, 3]));
    }

    #[test]
    fn eq_021_bytes_empty_equals_empty() {
        assert_eq!(Value::Bytes(vec![]), Value::Bytes(vec![]));
    }

    #[test]
    fn eq_022_array_equals_same_elements() {
        let a1 = Value::Array(vec![Value::Int(1), Value::Int(2)]);
        let a2 = Value::Array(vec![Value::Int(1), Value::Int(2)]);
        assert_eq!(a1, a2);
    }

    #[test]
    fn eq_023_array_not_equals_different_order() {
        let a1 = Value::Array(vec![Value::Int(1), Value::Int(2)]);
        let a2 = Value::Array(vec![Value::Int(2), Value::Int(1)]);
        assert_ne!(a1, a2, "Array order matters");
    }

    #[test]
    fn eq_024_array_not_equals_different_length() {
        let a1 = Value::Array(vec![Value::Int(1), Value::Int(2)]);
        let a2 = Value::Array(vec![Value::Int(1), Value::Int(2), Value::Int(3)]);
        assert_ne!(a1, a2);
    }

    #[test]
    fn eq_025_array_recursive_equality() {
        let a1 = Value::Array(vec![Value::Array(vec![Value::Int(1)])]);
        let a2 = Value::Array(vec![Value::Array(vec![Value::Int(1)])]);
        assert_eq!(a1, a2);
    }

    #[test]
    fn eq_026_object_equals_same_entries() {
        let mut m1 = HashMap::new();
        m1.insert("a".to_string(), Value::Int(1));
        let mut m2 = HashMap::new();
        m2.insert("a".to_string(), Value::Int(1));
        assert_eq!(Value::Object(m1), Value::Object(m2));
    }

    #[test]
    fn eq_027_object_equals_regardless_of_insertion_order() {
        let mut m1 = HashMap::new();
        m1.insert("a".to_string(), Value::Int(1));
        m1.insert("b".to_string(), Value::Int(2));

        let mut m2 = HashMap::new();
        m2.insert("b".to_string(), Value::Int(2));
        m2.insert("a".to_string(), Value::Int(1));

        assert_eq!(Value::Object(m1), Value::Object(m2), "Object equality ignores insertion order");
    }

    #[test]
    fn eq_028_object_not_equals_different_keys() {
        let mut m1 = HashMap::new();
        m1.insert("a".to_string(), Value::Int(1));
        let mut m2 = HashMap::new();
        m2.insert("b".to_string(), Value::Int(1));
        assert_ne!(Value::Object(m1), Value::Object(m2));
    }

    #[test]
    fn eq_029_object_not_equals_different_values() {
        let mut m1 = HashMap::new();
        m1.insert("a".to_string(), Value::Int(1));
        let mut m2 = HashMap::new();
        m2.insert("a".to_string(), Value::Int(2));
        assert_ne!(Value::Object(m1), Value::Object(m2));
    }

    #[test]
    fn eq_030_object_recursive_equality() {
        let mut inner1 = HashMap::new();
        inner1.insert("b".to_string(), Value::Int(1));
        let mut m1 = HashMap::new();
        m1.insert("a".to_string(), Value::Object(inner1));

        let mut inner2 = HashMap::new();
        inner2.insert("b".to_string(), Value::Int(1));
        let mut m2 = HashMap::new();
        m2.insert("a".to_string(), Value::Object(inner2));

        assert_eq!(Value::Object(m1), Value::Object(m2));
    }
}

// =============================================================================
// 1.4 No Type Coercion Tests (NC-001 to NC-015)
// =============================================================================

#[cfg(test)]
mod no_coercion {
    use super::*;

    #[test]
    fn nc_001_int_one_not_float_one() {
        // CRITICAL: No implicit type coercion
        assert_ne!(Value::Int(1), Value::Float(1.0));
    }

    #[test]
    fn nc_002_int_zero_not_float_zero() {
        assert_ne!(Value::Int(0), Value::Float(0.0));
    }

    #[test]
    fn nc_003_int_max_not_float() {
        // Even if i64::MAX as f64 is approximately equal
        assert_ne!(Value::Int(i64::MAX), Value::Float(i64::MAX as f64));
    }

    #[test]
    fn nc_004_string_not_bytes() {
        // String("abc") != Bytes(b"abc")
        assert_ne!(Value::String("abc".into()), Value::Bytes(b"abc".to_vec()));
    }

    #[test]
    fn nc_005_null_not_empty_string() {
        assert_ne!(Value::Null, Value::String(String::new()));
    }

    #[test]
    fn nc_006_null_not_zero() {
        assert_ne!(Value::Null, Value::Int(0));
    }

    #[test]
    fn nc_007_null_not_false() {
        assert_ne!(Value::Null, Value::Bool(false));
    }

    #[test]
    fn nc_008_empty_array_not_null() {
        assert_ne!(Value::Array(vec![]), Value::Null);
    }

    #[test]
    fn nc_009_empty_object_not_null() {
        assert_ne!(Value::Object(HashMap::new()), Value::Null);
    }

    #[test]
    fn nc_010_bool_true_not_int_one() {
        assert_ne!(Value::Bool(true), Value::Int(1));
    }

    #[test]
    fn nc_011_bool_false_not_int_zero() {
        assert_ne!(Value::Bool(false), Value::Int(0));
    }

    #[test]
    fn nc_012_string_number_not_int() {
        assert_ne!(Value::String("123".into()), Value::Int(123));
    }

    #[test]
    fn nc_013_no_implicit_string_to_bytes() {
        // Even if content would be identical as UTF-8
        let s = Value::String("hello".into());
        let b = Value::Bytes(b"hello".to_vec());
        assert_ne!(s, b);
    }

    #[test]
    fn nc_014_no_implicit_int_promotion() {
        // Int should never be implicitly promoted to Float
        let i = Value::Int(42);
        let f = Value::Float(42.0);
        assert_ne!(i, f);
        // Verify types are preserved
        assert_eq!(i.type_name(), "int");
        assert_eq!(f.type_name(), "float");
    }

    #[test]
    fn nc_015_types_all_distinct() {
        // Verify all 8 types are distinct from each other
        let values = vec![
            Value::Null,
            Value::Bool(false),
            Value::Int(0),
            Value::Float(0.0),
            Value::String(String::new()),
            Value::Bytes(vec![]),
            Value::Array(vec![]),
            Value::Object(HashMap::new()),
        ];

        for (i, v1) in values.iter().enumerate() {
            for (j, v2) in values.iter().enumerate() {
                if i != j {
                    assert_ne!(v1, v2, "{} must not equal {}", v1.type_name(), v2.type_name());
                }
            }
        }
    }
}

// =============================================================================
// 1.5 Size Limits Tests (SL-001 to SL-018)
// =============================================================================

#[cfg(test)]
mod size_limits {
    use super::*;

    #[test]
    fn sl_001_key_at_max_length() {
        let key = gen_key_of_length(1024);
        assert!(validate_key(&key).is_ok());
    }

    #[test]
    fn sl_002_key_exceeds_max_length() {
        let key = gen_key_of_length(1025);
        let result = validate_key(&key);
        assert!(result.is_err());
        if let Err(StrataError::InvalidKey { reason, .. }) = result {
            assert!(reason.contains("exceeds") || reason.contains("max"));
        }
    }

    #[test]
    fn sl_003_key_much_larger_than_max() {
        let key = gen_key_of_length(10_000);
        assert!(validate_key(&key).is_err());
    }

    #[test]
    fn sl_014_nesting_at_max_depth() {
        // 128 levels of nesting should succeed
        let _v = gen_nested_value(128);
        // Construction should not panic
    }

    #[test]
    fn sl_015_nesting_exceeds_max_depth() {
        // 129 levels should fail validation (when validation is implemented)
        let _v = gen_nested_value(129);
        // TODO: Add validation call when implemented
        // For now, just verify construction doesn't panic
    }

    // Note: Tests SL-004 to SL-013 and SL-016 to SL-018 require integration
    // with the actual storage layer to validate size limits at write time.
    // These are marked as placeholders to be filled in during implementation.

    #[test]
    #[ignore = "Requires storage layer integration"]
    fn sl_004_string_at_max_length() {
        // 16 MiB string should succeed
    }

    #[test]
    #[ignore = "Requires storage layer integration"]
    fn sl_005_string_exceeds_max_length() {
        // 16 MiB + 1 byte should fail with ConstraintViolation
    }

    #[test]
    #[ignore = "Requires storage layer integration"]
    fn sl_006_bytes_at_max_length() {
        // 16 MiB bytes should succeed
    }

    #[test]
    #[ignore = "Requires storage layer integration"]
    fn sl_007_bytes_exceeds_max_length() {
        // 16 MiB + 1 byte should fail
    }

    #[test]
    #[ignore = "Requires storage layer integration"]
    fn sl_010_array_at_max_length() {
        // 1M elements should succeed
    }

    #[test]
    #[ignore = "Requires storage layer integration"]
    fn sl_011_array_exceeds_max_length() {
        // 1M + 1 elements should fail
    }

    #[test]
    #[ignore = "Requires storage layer integration"]
    fn sl_012_object_at_max_entries() {
        // 1M entries should succeed
    }

    #[test]
    #[ignore = "Requires storage layer integration"]
    fn sl_013_object_exceeds_max_entries() {
        // 1M + 1 entries should fail
    }

    #[test]
    #[ignore = "Requires storage layer integration"]
    fn sl_016_vector_at_max_dim() {
        // 8192 dimensions should succeed
    }

    #[test]
    #[ignore = "Requires storage layer integration"]
    fn sl_017_vector_exceeds_max_dim() {
        // 8193 dimensions should fail
    }
}

// =============================================================================
// 1.6 Key Validation Tests (KV-001 to KV-020)
// =============================================================================

#[cfg(test)]
mod key_validation {
    use super::*;

    #[test]
    fn kv_001_valid_simple_key() {
        assert!(validate_key("mykey").is_ok());
    }

    #[test]
    fn kv_002_valid_unicode_key() {
        assert!(validate_key("日本語キー").is_ok());
    }

    #[test]
    fn kv_003_valid_emoji_key() {
        assert!(validate_key("🔑key🔑").is_ok());
    }

    #[test]
    fn kv_004_valid_numeric_string_key() {
        assert!(validate_key("12345").is_ok());
    }

    #[test]
    fn kv_005_valid_special_chars_key() {
        assert!(validate_key("a-b_c.d:e/f").is_ok());
    }

    #[test]
    fn kv_006_invalid_empty_key() {
        let result = validate_key("");
        assert!(result.is_err());
        assert!(matches!(result, Err(StrataError::InvalidKey { .. })));
    }

    #[test]
    fn kv_007_invalid_nul_byte() {
        let result = validate_key("a\0b");
        assert!(result.is_err());
        assert!(matches!(result, Err(StrataError::InvalidKey { .. })));
    }

    #[test]
    fn kv_008_invalid_nul_at_start() {
        let result = validate_key("\0abc");
        assert!(result.is_err());
    }

    #[test]
    fn kv_009_invalid_nul_at_end() {
        let result = validate_key("abc\0");
        assert!(result.is_err());
    }

    #[test]
    fn kv_010_invalid_reserved_prefix() {
        let result = validate_key("_strata/foo");
        assert!(result.is_err());
        if let Err(StrataError::InvalidKey { reason, .. }) = result {
            assert!(reason.contains("reserved"));
        }
    }

    #[test]
    fn kv_011_invalid_reserved_prefix_exact() {
        let result = validate_key("_strata/");
        assert!(result.is_err());
    }

    #[test]
    fn kv_012_valid_similar_to_reserved() {
        // _stratafoo is OK - no slash after _strata
        assert!(validate_key("_stratafoo").is_ok());
    }

    #[test]
    fn kv_013_valid_underscore_prefix() {
        assert!(validate_key("_mykey").is_ok());
    }

    #[test]
    fn kv_015_valid_at_max_length() {
        let key = gen_key_of_length(1024);
        assert!(validate_key(&key).is_ok());
    }

    #[test]
    fn kv_016_invalid_exceeds_max_length() {
        let key = gen_key_of_length(1025);
        assert!(validate_key(&key).is_err());
    }

    #[test]
    fn kv_017_valid_single_char() {
        assert!(validate_key("a").is_ok());
    }

    #[test]
    fn kv_018_valid_single_byte() {
        assert!(validate_key("x").is_ok());
    }

    #[test]
    fn kv_019_valid_whitespace_key() {
        // Whitespace is allowed in keys
        assert!(validate_key("  spaces  ").is_ok());
    }

    #[test]
    fn kv_020_valid_newline_key() {
        // Newlines are allowed in keys
        assert!(validate_key("line1\nline2").is_ok());
    }
}
