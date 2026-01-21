//! Value types for Strata
//!
//! This module defines:
//! - Value: Unified enum for all primitive data types
//!
//! ## Migration Note (M9)
//!
//! - `Timestamp` is now in `contract::Timestamp` (microseconds, not seconds)
//! - `VersionedValue` is now `contract::Versioned<Value>`
//!
//! Import from crate root: `use strata_core::{Timestamp, VersionedValue, Version};`

use serde::{Deserialize, Serialize};

/// Unified value type for all primitives
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Value {
    /// Null value
    Null,
    /// Boolean value
    Bool(bool),
    /// 64-bit signed integer
    I64(i64),
    /// 64-bit floating point
    F64(f64),
    /// UTF-8 string
    String(String),
    /// Raw bytes
    Bytes(Vec<u8>),
    /// Array of values
    Array(Vec<Value>),
    /// Map of string keys to values
    Map(std::collections::HashMap<String, Value>),
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    // Tests for Value enum variants

    #[test]
    fn test_value_null() {
        let value = Value::Null;
        assert!(matches!(value, Value::Null));
    }

    #[test]
    fn test_value_bool() {
        let value_true = Value::Bool(true);
        let value_false = Value::Bool(false);

        assert!(matches!(value_true, Value::Bool(true)));
        assert!(matches!(value_false, Value::Bool(false)));
    }

    #[test]
    fn test_value_i64() {
        let value = Value::I64(42);
        assert!(matches!(value, Value::I64(42)));

        let negative = Value::I64(-100);
        assert!(matches!(negative, Value::I64(-100)));
    }

    #[test]
    fn test_value_f64() {
        let value = Value::F64(3.14);
        assert!(matches!(value, Value::F64(_)));

        if let Value::F64(f) = value {
            assert!((f - 3.14).abs() < f64::EPSILON);
        }
    }

    #[test]
    fn test_value_string() {
        let value = Value::String("hello world".to_string());
        assert!(matches!(value, Value::String(_)));

        if let Value::String(s) = value {
            assert_eq!(s, "hello world");
        }
    }

    #[test]
    fn test_value_bytes() {
        let bytes = vec![1, 2, 3, 4, 5];
        let value = Value::Bytes(bytes.clone());

        assert!(matches!(value, Value::Bytes(_)));
        if let Value::Bytes(b) = value {
            assert_eq!(b, bytes);
        }
    }

    #[test]
    fn test_value_array() {
        let array = vec![
            Value::I64(1),
            Value::String("test".to_string()),
            Value::Bool(true),
        ];
        let value = Value::Array(array.clone());

        assert!(matches!(value, Value::Array(_)));
        if let Value::Array(arr) = value {
            assert_eq!(arr.len(), 3);
            assert_eq!(arr[0], Value::I64(1));
            assert_eq!(arr[1], Value::String("test".to_string()));
            assert_eq!(arr[2], Value::Bool(true));
        }
    }

    #[test]
    fn test_value_map() {
        let mut map = HashMap::new();
        map.insert("key1".to_string(), Value::I64(42));
        map.insert("key2".to_string(), Value::String("value".to_string()));

        let value = Value::Map(map.clone());
        assert!(matches!(value, Value::Map(_)));

        if let Value::Map(m) = value {
            assert_eq!(m.len(), 2);
            assert_eq!(m.get("key1"), Some(&Value::I64(42)));
            assert_eq!(m.get("key2"), Some(&Value::String("value".to_string())));
        }
    }

    #[test]
    fn test_value_serialization_all_variants() {
        let test_values = vec![
            Value::Null,
            Value::Bool(true),
            Value::I64(42),
            Value::F64(3.14),
            Value::String("test".to_string()),
            Value::Bytes(vec![1, 2, 3]),
            Value::Array(vec![Value::I64(1), Value::String("a".to_string())]),
        ];

        for value in test_values {
            let serialized = serde_json::to_string(&value).unwrap();
            let deserialized: Value = serde_json::from_str(&serialized).unwrap();
            assert_eq!(value, deserialized);
        }
    }

    #[test]
    fn test_value_map_serialization() {
        let mut map = HashMap::new();
        map.insert("test".to_string(), Value::I64(123));
        let value = Value::Map(map);

        let serialized = serde_json::to_string(&value).unwrap();
        let deserialized: Value = serde_json::from_str(&serialized).unwrap();
        assert_eq!(value, deserialized);
    }

    // VersionedValue tests are now in contract/versioned.rs
}
