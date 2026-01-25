//! Value types for Strata
//!
//! This module defines:
//! - Value: Unified enum for all primitive data types
//!
//! ## Canonical Value Model (Frozen)
//!
//! The Value enum has exactly 8 variants, matching the core contract:
//! - Null, Bool, Int, Float, String, Bytes, Array, Object
//!
//! ### Type Rules (VAL-1 to VAL-5)
//!
//! - **VAL-1**: Eight types only
//! - **VAL-2**: No implicit type coercions
//! - **VAL-3**: `Int(1) != Float(1.0)` - different types are NEVER equal
//! - **VAL-4**: `Bytes` are not `String`
//! - **VAL-5**: Float uses IEEE-754 equality: `NaN != NaN`, `-0.0 == 0.0`
//!
//! ## Migration Note
//!
//! - `Timestamp` is now in `contract::Timestamp` (microseconds, not seconds)
//! - `VersionedValue` is now `contract::Versioned<Value>`
//!
//! Import from crate root: `use strata_core::{Timestamp, VersionedValue, Version};`

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Canonical Strata value type for all API surfaces
///
/// This enum represents the 8 canonical value types in the Strata data model.
/// All SDKs must map to this model. JSON is a strict subset.
///
/// ## Type Equality
///
/// Different types are NEVER equal, even if they contain the same "value":
/// - `Int(1) != Float(1.0)`
/// - `Bytes(b"hello") != String("hello")`
///
/// Float equality follows IEEE-754 semantics:
/// - `NaN != NaN`
/// - `-0.0 == 0.0`
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Value {
    /// Null value
    Null,
    /// Boolean value
    Bool(bool),
    /// 64-bit signed integer
    Int(i64),
    /// 64-bit floating point (IEEE-754)
    Float(f64),
    /// UTF-8 string
    String(String),
    /// Raw bytes
    Bytes(Vec<u8>),
    /// Array of values
    Array(Vec<Value>),
    /// Object with string keys (JSON object)
    Object(HashMap<String, Value>),
}

// Custom PartialEq implementation for IEEE-754 float semantics
impl PartialEq for Value {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Value::Null, Value::Null) => true,
            (Value::Bool(a), Value::Bool(b)) => a == b,
            (Value::Int(a), Value::Int(b)) => a == b,
            // IEEE-754: NaN != NaN, -0.0 == 0.0
            (Value::Float(a), Value::Float(b)) => a == b,
            (Value::String(a), Value::String(b)) => a == b,
            (Value::Bytes(a), Value::Bytes(b)) => a == b,
            (Value::Array(a), Value::Array(b)) => a == b,
            (Value::Object(a), Value::Object(b)) => {
                a.len() == b.len() && a.iter().all(|(k, v)| b.get(k) == Some(v))
            }
            // Different types are NEVER equal (VAL-3)
            _ => false,
        }
    }
}

impl Value {
    /// Get the type name as a string
    pub fn type_name(&self) -> &'static str {
        match self {
            Value::Null => "Null",
            Value::Bool(_) => "Bool",
            Value::Int(_) => "Int",
            Value::Float(_) => "Float",
            Value::String(_) => "String",
            Value::Bytes(_) => "Bytes",
            Value::Array(_) => "Array",
            Value::Object(_) => "Object",
        }
    }

    /// Check if this is a null value
    pub fn is_null(&self) -> bool {
        matches!(self, Value::Null)
    }

    /// Check if this is a boolean value
    pub fn is_bool(&self) -> bool {
        matches!(self, Value::Bool(_))
    }

    /// Check if this is an integer value
    pub fn is_int(&self) -> bool {
        matches!(self, Value::Int(_))
    }

    /// Check if this is a float value
    pub fn is_float(&self) -> bool {
        matches!(self, Value::Float(_))
    }

    /// Check if this is a string value
    pub fn is_string(&self) -> bool {
        matches!(self, Value::String(_))
    }

    /// Check if this is a bytes value
    pub fn is_bytes(&self) -> bool {
        matches!(self, Value::Bytes(_))
    }

    /// Check if this is an array value
    pub fn is_array(&self) -> bool {
        matches!(self, Value::Array(_))
    }

    /// Check if this is an object value
    pub fn is_object(&self) -> bool {
        matches!(self, Value::Object(_))
    }

    /// Get as bool if this is a Bool value
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Value::Bool(b) => Some(*b),
            _ => None,
        }
    }

    /// Get as i64 if this is an Int value
    pub fn as_int(&self) -> Option<i64> {
        match self {
            Value::Int(i) => Some(*i),
            _ => None,
        }
    }

    /// Get as f64 if this is a Float value
    pub fn as_float(&self) -> Option<f64> {
        match self {
            Value::Float(f) => Some(*f),
            _ => None,
        }
    }

    /// Get as &str if this is a String value
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Value::String(s) => Some(s),
            _ => None,
        }
    }

    /// Get as &[u8] if this is a Bytes value
    pub fn as_bytes(&self) -> Option<&[u8]> {
        match self {
            Value::Bytes(b) => Some(b),
            _ => None,
        }
    }

    /// Get as &[Value] if this is an Array value
    pub fn as_array(&self) -> Option<&[Value]> {
        match self {
            Value::Array(a) => Some(a),
            _ => None,
        }
    }

    /// Get as &HashMap if this is an Object value
    pub fn as_object(&self) -> Option<&HashMap<String, Value>> {
        match self {
            Value::Object(o) => Some(o),
            _ => None,
        }
    }
}

// ============================================================================
// From implementations for ergonomic API usage
// ============================================================================

impl From<&str> for Value {
    fn from(s: &str) -> Self {
        Value::String(s.to_string())
    }
}

impl From<String> for Value {
    fn from(s: String) -> Self {
        Value::String(s)
    }
}

impl From<bool> for Value {
    fn from(b: bool) -> Self {
        Value::Bool(b)
    }
}

impl From<i64> for Value {
    fn from(i: i64) -> Self {
        Value::Int(i)
    }
}

impl From<i32> for Value {
    fn from(i: i32) -> Self {
        Value::Int(i as i64)
    }
}

impl From<f64> for Value {
    fn from(f: f64) -> Self {
        Value::Float(f)
    }
}

impl From<f32> for Value {
    fn from(f: f32) -> Self {
        Value::Float(f as f64)
    }
}

impl From<Vec<u8>> for Value {
    fn from(b: Vec<u8>) -> Self {
        Value::Bytes(b)
    }
}

impl From<&[u8]> for Value {
    fn from(b: &[u8]) -> Self {
        Value::Bytes(b.to_vec())
    }
}

impl From<Vec<Value>> for Value {
    fn from(a: Vec<Value>) -> Self {
        Value::Array(a)
    }
}

impl From<HashMap<String, Value>> for Value {
    fn from(o: HashMap<String, Value>) -> Self {
        Value::Object(o)
    }
}

impl From<()> for Value {
    fn from(_: ()) -> Self {
        Value::Null
    }
}

// ============================================================================
// serde_json interop for ergonomic JSON construction
// ============================================================================

impl From<serde_json::Value> for Value {
    fn from(v: serde_json::Value) -> Self {
        match v {
            serde_json::Value::Null => Value::Null,
            serde_json::Value::Bool(b) => Value::Bool(b),
            serde_json::Value::Number(n) => {
                if let Some(i) = n.as_i64() {
                    Value::Int(i)
                } else if let Some(f) = n.as_f64() {
                    Value::Float(f)
                } else {
                    // Fallback for u64 that doesn't fit in i64
                    Value::Float(n.as_f64().unwrap_or(0.0))
                }
            }
            serde_json::Value::String(s) => Value::String(s),
            serde_json::Value::Array(arr) => {
                Value::Array(arr.into_iter().map(Value::from).collect())
            }
            serde_json::Value::Object(obj) => {
                Value::Object(obj.into_iter().map(|(k, v)| (k, Value::from(v))).collect())
            }
        }
    }
}

impl From<Value> for serde_json::Value {
    fn from(v: Value) -> Self {
        match v {
            Value::Null => serde_json::Value::Null,
            Value::Bool(b) => serde_json::Value::Bool(b),
            Value::Int(i) => serde_json::Value::Number(i.into()),
            Value::Float(f) => {
                serde_json::Number::from_f64(f)
                    .map(serde_json::Value::Number)
                    .unwrap_or(serde_json::Value::Null)
            }
            Value::String(s) => serde_json::Value::String(s),
            Value::Bytes(b) => {
                // Encode bytes as base64 string for JSON compatibility
                serde_json::Value::String(base64_encode(&b))
            }
            Value::Array(arr) => {
                serde_json::Value::Array(arr.into_iter().map(serde_json::Value::from).collect())
            }
            Value::Object(obj) => {
                serde_json::Value::Object(
                    obj.into_iter()
                        .map(|(k, v)| (k, serde_json::Value::from(v)))
                        .collect(),
                )
            }
        }
    }
}

/// Simple base64 encoding for bytes (no external dependency)
fn base64_encode(data: &[u8]) -> String {
    use std::fmt::Write;
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

    let mut result = String::with_capacity((data.len() + 2) / 3 * 4);

    for chunk in data.chunks(3) {
        let b0 = chunk[0] as usize;
        let b1 = chunk.get(1).copied().unwrap_or(0) as usize;
        let b2 = chunk.get(2).copied().unwrap_or(0) as usize;

        let _ = write!(result, "{}", ALPHABET[(b0 >> 2) & 0x3F] as char);
        let _ = write!(result, "{}", ALPHABET[((b0 << 4) | (b1 >> 4)) & 0x3F] as char);

        if chunk.len() > 1 {
            let _ = write!(result, "{}", ALPHABET[((b1 << 2) | (b2 >> 6)) & 0x3F] as char);
        } else {
            result.push('=');
        }

        if chunk.len() > 2 {
            let _ = write!(result, "{}", ALPHABET[b2 & 0x3F] as char);
        } else {
            result.push('=');
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    // Tests for Value enum variants

    #[test]
    fn test_value_null() {
        let value = Value::Null;
        assert!(matches!(value, Value::Null));
        assert!(value.is_null());
    }

    #[test]
    fn test_value_bool() {
        let value_true = Value::Bool(true);
        let value_false = Value::Bool(false);

        assert!(matches!(value_true, Value::Bool(true)));
        assert!(matches!(value_false, Value::Bool(false)));
        assert!(value_true.is_bool());
        assert_eq!(value_true.as_bool(), Some(true));
    }

    #[test]
    fn test_value_int() {
        let value = Value::Int(42);
        assert!(matches!(value, Value::Int(42)));
        assert!(value.is_int());
        assert_eq!(value.as_int(), Some(42));

        let negative = Value::Int(-100);
        assert!(matches!(negative, Value::Int(-100)));
    }

    #[test]
    fn test_value_float() {
        let value = Value::Float(3.14);
        assert!(matches!(value, Value::Float(_)));
        assert!(value.is_float());

        if let Some(f) = value.as_float() {
            assert!((f - 3.14).abs() < f64::EPSILON);
        }
    }

    #[test]
    fn test_value_string() {
        let value = Value::String("hello world".to_string());
        assert!(matches!(value, Value::String(_)));
        assert!(value.is_string());
        assert_eq!(value.as_str(), Some("hello world"));
    }

    #[test]
    fn test_value_bytes() {
        let bytes = vec![1, 2, 3, 4, 5];
        let value = Value::Bytes(bytes.clone());

        assert!(matches!(value, Value::Bytes(_)));
        assert!(value.is_bytes());
        assert_eq!(value.as_bytes(), Some(bytes.as_slice()));
    }

    #[test]
    fn test_value_array() {
        let array = vec![
            Value::Int(1),
            Value::String("test".to_string()),
            Value::Bool(true),
        ];
        let value = Value::Array(array.clone());

        assert!(matches!(value, Value::Array(_)));
        assert!(value.is_array());
        if let Some(arr) = value.as_array() {
            assert_eq!(arr.len(), 3);
            assert_eq!(arr[0], Value::Int(1));
            assert_eq!(arr[1], Value::String("test".to_string()));
            assert_eq!(arr[2], Value::Bool(true));
        }
    }

    #[test]
    fn test_value_object() {
        let mut map = HashMap::new();
        map.insert("key1".to_string(), Value::Int(42));
        map.insert("key2".to_string(), Value::String("value".to_string()));

        let value = Value::Object(map.clone());
        assert!(matches!(value, Value::Object(_)));
        assert!(value.is_object());

        if let Some(m) = value.as_object() {
            assert_eq!(m.len(), 2);
            assert_eq!(m.get("key1"), Some(&Value::Int(42)));
            assert_eq!(m.get("key2"), Some(&Value::String("value".to_string())));
        }
    }

    #[test]
    fn test_value_serialization_all_variants() {
        let test_values = vec![
            Value::Null,
            Value::Bool(true),
            Value::Int(42),
            Value::Float(3.14),
            Value::String("test".to_string()),
            Value::Bytes(vec![1, 2, 3]),
            Value::Array(vec![Value::Int(1), Value::String("a".to_string())]),
        ];

        for value in test_values {
            let serialized = serde_json::to_string(&value).unwrap();
            let deserialized: Value = serde_json::from_str(&serialized).unwrap();
            assert_eq!(value, deserialized);
        }
    }

    #[test]
    fn test_value_object_serialization() {
        let mut map = HashMap::new();
        map.insert("test".to_string(), Value::Int(123));
        let value = Value::Object(map);

        let serialized = serde_json::to_string(&value).unwrap();
        let deserialized: Value = serde_json::from_str(&serialized).unwrap();
        assert_eq!(value, deserialized);
    }

    // VAL-3: Different types are NEVER equal
    #[test]
    fn test_int_not_equal_float() {
        let int_val = Value::Int(1);
        let float_val = Value::Float(1.0);

        assert_ne!(int_val, float_val);
    }

    // VAL-5: IEEE-754 float equality
    #[test]
    fn test_nan_not_equal_nan() {
        let nan1 = Value::Float(f64::NAN);
        let nan2 = Value::Float(f64::NAN);

        assert_ne!(nan1, nan2);
    }

    #[test]
    fn test_negative_zero_equals_zero() {
        let neg_zero = Value::Float(-0.0);
        let zero = Value::Float(0.0);

        assert_eq!(neg_zero, zero);
    }

    #[test]
    fn test_type_name() {
        assert_eq!(Value::Null.type_name(), "Null");
        assert_eq!(Value::Bool(true).type_name(), "Bool");
        assert_eq!(Value::Int(1).type_name(), "Int");
        assert_eq!(Value::Float(1.0).type_name(), "Float");
        assert_eq!(Value::String("".to_string()).type_name(), "String");
        assert_eq!(Value::Bytes(vec![]).type_name(), "Bytes");
        assert_eq!(Value::Array(vec![]).type_name(), "Array");
        assert_eq!(Value::Object(HashMap::new()).type_name(), "Object");
    }
}
