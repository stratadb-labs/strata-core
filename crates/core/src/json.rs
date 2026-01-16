//! JSON types for M5 JSON primitive
//!
//! This module defines types for the JSON document storage system:
//! - JsonValue: Newtype wrapper around serde_json::Value

use serde::{Deserialize, Serialize};
use std::fmt;
use std::ops::{Deref, DerefMut};
use std::str::FromStr;

/// JSON value wrapper
///
/// Newtype around serde_json::Value providing:
/// - Direct access to underlying serde_json::Value via Deref/DerefMut
/// - Easy construction from common types
/// - Serialization/deserialization support
///
/// # Examples
///
/// ```
/// use in_mem_core::JsonValue;
///
/// // From JSON literals
/// let obj = JsonValue::object();
/// let arr = JsonValue::array();
/// let null = JsonValue::null();
///
/// // From common types
/// let s = JsonValue::from("hello");
/// let n = JsonValue::from(42i64);
/// let b = JsonValue::from(true);
///
/// // Access underlying value
/// assert!(obj.is_object());
/// assert!(arr.is_array());
/// assert!(null.is_null());
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct JsonValue(serde_json::Value);

impl JsonValue {
    /// Create a null JSON value
    pub fn null() -> Self {
        JsonValue(serde_json::Value::Null)
    }

    /// Create an empty JSON object
    pub fn object() -> Self {
        JsonValue(serde_json::Value::Object(serde_json::Map::new()))
    }

    /// Create an empty JSON array
    pub fn array() -> Self {
        JsonValue(serde_json::Value::Array(Vec::new()))
    }

    /// Create from a serde_json::Value
    pub fn from_value(value: serde_json::Value) -> Self {
        JsonValue(value)
    }

    /// Get the underlying serde_json::Value
    pub fn into_inner(self) -> serde_json::Value {
        self.0
    }

    /// Get a reference to the underlying serde_json::Value
    pub fn as_inner(&self) -> &serde_json::Value {
        &self.0
    }

    /// Get a mutable reference to the underlying serde_json::Value
    pub fn as_inner_mut(&mut self) -> &mut serde_json::Value {
        &mut self.0
    }

    /// Serialize to compact JSON string
    pub fn to_json_string(&self) -> String {
        self.0.to_string()
    }

    /// Serialize to pretty JSON string
    pub fn to_json_string_pretty(&self) -> String {
        serde_json::to_string_pretty(&self.0).unwrap_or_else(|_| self.to_json_string())
    }

    /// Calculate approximate size in bytes (for limit checking)
    ///
    /// This is an estimate based on the JSON string representation.
    /// Actual in-memory size may differ.
    pub fn size_bytes(&self) -> usize {
        self.to_json_string().len()
    }
}

// Implement FromStr for parsing from strings
impl FromStr for JsonValue {
    type Err = serde_json::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        serde_json::from_str(s).map(JsonValue)
    }
}

// Deref to access serde_json::Value methods directly
impl Deref for JsonValue {
    type Target = serde_json::Value;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for JsonValue {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

// Display for easy printing
impl fmt::Display for JsonValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

// Default is null
impl Default for JsonValue {
    fn default() -> Self {
        Self::null()
    }
}

// From implementations for common types
impl From<serde_json::Value> for JsonValue {
    fn from(v: serde_json::Value) -> Self {
        JsonValue(v)
    }
}

impl From<JsonValue> for serde_json::Value {
    fn from(v: JsonValue) -> Self {
        v.0
    }
}

impl From<bool> for JsonValue {
    fn from(v: bool) -> Self {
        JsonValue(serde_json::Value::Bool(v))
    }
}

impl From<i64> for JsonValue {
    fn from(v: i64) -> Self {
        JsonValue(serde_json::Value::Number(v.into()))
    }
}

impl From<i32> for JsonValue {
    fn from(v: i32) -> Self {
        JsonValue(serde_json::Value::Number(v.into()))
    }
}

impl From<u64> for JsonValue {
    fn from(v: u64) -> Self {
        JsonValue(serde_json::Value::Number(v.into()))
    }
}

impl From<u32> for JsonValue {
    fn from(v: u32) -> Self {
        JsonValue(serde_json::Value::Number(v.into()))
    }
}

impl From<f64> for JsonValue {
    fn from(v: f64) -> Self {
        JsonValue(
            serde_json::Number::from_f64(v)
                .map_or(serde_json::Value::Null, |n| serde_json::Value::Number(n)),
        )
    }
}

impl From<&str> for JsonValue {
    fn from(v: &str) -> Self {
        JsonValue(serde_json::Value::String(v.to_string()))
    }
}

impl From<String> for JsonValue {
    fn from(v: String) -> Self {
        JsonValue(serde_json::Value::String(v))
    }
}

impl<T: Into<JsonValue>> From<Vec<T>> for JsonValue {
    fn from(v: Vec<T>) -> Self {
        JsonValue(serde_json::Value::Array(
            v.into_iter().map(|x| x.into().0).collect(),
        ))
    }
}

impl<T: Into<JsonValue>> From<Option<T>> for JsonValue {
    fn from(v: Option<T>) -> Self {
        match v {
            Some(v) => v.into(),
            None => JsonValue::null(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_json_value_null() {
        let v = JsonValue::null();
        assert!(v.is_null());
    }

    #[test]
    fn test_json_value_object() {
        let v = JsonValue::object();
        assert!(v.is_object());
        assert_eq!(v.as_object().unwrap().len(), 0);
    }

    #[test]
    fn test_json_value_array() {
        let v = JsonValue::array();
        assert!(v.is_array());
        assert_eq!(v.as_array().unwrap().len(), 0);
    }

    #[test]
    fn test_json_value_from_bool() {
        let t = JsonValue::from(true);
        let f = JsonValue::from(false);
        assert_eq!(t.as_bool(), Some(true));
        assert_eq!(f.as_bool(), Some(false));
    }

    #[test]
    fn test_json_value_from_i64() {
        let v = JsonValue::from(42i64);
        assert_eq!(v.as_i64(), Some(42));
    }

    #[test]
    fn test_json_value_from_i32() {
        let v = JsonValue::from(42i32);
        assert_eq!(v.as_i64(), Some(42));
    }

    #[test]
    fn test_json_value_from_u64() {
        let v = JsonValue::from(42u64);
        assert_eq!(v.as_u64(), Some(42));
    }

    #[test]
    fn test_json_value_from_f64() {
        let v = JsonValue::from(3.14f64);
        assert!((v.as_f64().unwrap() - 3.14).abs() < f64::EPSILON);
    }

    #[test]
    fn test_json_value_from_str_ref() {
        let v = JsonValue::from("hello");
        assert_eq!(v.as_str(), Some("hello"));
    }

    #[test]
    fn test_json_value_from_string() {
        let v = JsonValue::from("world".to_string());
        assert_eq!(v.as_str(), Some("world"));
    }

    #[test]
    fn test_json_value_from_vec() {
        let v: JsonValue = vec![1i64, 2, 3].into();
        let arr = v.as_array().unwrap();
        assert_eq!(arr.len(), 3);
        assert_eq!(arr[0].as_i64(), Some(1));
    }

    #[test]
    fn test_json_value_from_option_some() {
        let v: JsonValue = Some(42i64).into();
        assert_eq!(v.as_i64(), Some(42));
    }

    #[test]
    fn test_json_value_from_option_none() {
        let v: JsonValue = Option::<i64>::None.into();
        assert!(v.is_null());
    }

    #[test]
    fn test_json_value_deref() {
        let v = JsonValue::from(42i64);
        // Access serde_json::Value methods via Deref
        assert!(v.is_number());
        assert!(!v.is_string());
    }

    #[test]
    fn test_json_value_deref_mut() {
        let mut v = JsonValue::object();
        // Mutate via DerefMut
        v.as_object_mut()
            .unwrap()
            .insert("key".to_string(), serde_json::json!(123));
        assert_eq!(v["key"].as_i64(), Some(123));
    }

    #[test]
    fn test_json_value_parse() {
        let v: JsonValue = r#"{"name": "test", "value": 42}"#.parse().unwrap();
        assert!(v.is_object());
        assert_eq!(v["name"].as_str(), Some("test"));
        assert_eq!(v["value"].as_i64(), Some(42));
    }

    #[test]
    fn test_json_value_parse_invalid() {
        let result: Result<JsonValue, _> = "not valid json {".parse();
        assert!(result.is_err());
    }

    #[test]
    fn test_json_value_to_json_string() {
        let v: JsonValue = r#"{"a":1}"#.parse().unwrap();
        let s = v.to_json_string();
        assert!(s.contains("\"a\""));
        assert!(s.contains("1"));
    }

    #[test]
    fn test_json_value_display() {
        let v = JsonValue::from(42i64);
        let s = format!("{}", v);
        assert_eq!(s, "42");
    }

    #[test]
    fn test_json_value_default() {
        let v = JsonValue::default();
        assert!(v.is_null());
    }

    #[test]
    fn test_json_value_clone() {
        let v1 = JsonValue::from("test");
        let v2 = v1.clone();
        assert_eq!(v1, v2);
    }

    #[test]
    fn test_json_value_equality() {
        let v1 = JsonValue::from(42i64);
        let v2 = JsonValue::from(42i64);
        let v3 = JsonValue::from(43i64);
        assert_eq!(v1, v2);
        assert_ne!(v1, v3);
    }

    #[test]
    fn test_json_value_serialization() {
        let v: JsonValue = r#"{"key": "value"}"#.parse().unwrap();
        let json = serde_json::to_string(&v).unwrap();
        let v2: JsonValue = serde_json::from_str(&json).unwrap();
        assert_eq!(v, v2);
    }

    #[test]
    fn test_json_value_into_inner() {
        let v = JsonValue::from(42i64);
        let inner: serde_json::Value = v.into_inner();
        assert_eq!(inner.as_i64(), Some(42));
    }

    #[test]
    fn test_json_value_as_inner() {
        let v = JsonValue::from(42i64);
        let inner: &serde_json::Value = v.as_inner();
        assert_eq!(inner.as_i64(), Some(42));
    }

    #[test]
    fn test_json_value_size_bytes() {
        let v: JsonValue = r#"{"key": "value"}"#.parse().unwrap();
        let size = v.size_bytes();
        // Should be at least the length of the JSON string
        assert!(size > 0);
        assert!(size <= 20); // Reasonable upper bound for this small object
    }

    #[test]
    fn test_json_value_from_serde_json_value() {
        let serde_val = serde_json::json!({"nested": {"deep": true}});
        let v = JsonValue::from(serde_val);
        assert!(v.is_object());
        assert!(v["nested"]["deep"].as_bool().unwrap());
    }

    #[test]
    fn test_json_value_into_serde_json_value() {
        let v = JsonValue::from(42i64);
        let serde_val: serde_json::Value = v.into();
        assert_eq!(serde_val.as_i64(), Some(42));
    }

    #[test]
    fn test_json_value_f64_nan() {
        // NaN/Infinity cannot be represented in JSON, should become null
        let v = JsonValue::from(f64::NAN);
        assert!(v.is_null());
    }

    #[test]
    fn test_json_value_f64_infinity() {
        // Infinity cannot be represented in JSON, should become null
        let v = JsonValue::from(f64::INFINITY);
        assert!(v.is_null());
    }

    #[test]
    fn test_json_value_nested_modification() {
        let mut v: JsonValue = r#"{"user": {"name": "Alice"}}"#.parse().unwrap();
        v["user"]["name"] = serde_json::json!("Bob");
        assert_eq!(v["user"]["name"].as_str(), Some("Bob"));
    }

    #[test]
    fn test_json_value_to_json_string_pretty() {
        let v: JsonValue = r#"{"a":1,"b":2}"#.parse().unwrap();
        let pretty = v.to_json_string_pretty();
        // Pretty output should have newlines
        assert!(pretty.contains('\n'));
    }

    #[test]
    fn test_json_value_from_value() {
        let serde_val = serde_json::json!([1, 2, 3]);
        let v = JsonValue::from_value(serde_val);
        assert!(v.is_array());
        assert_eq!(v.as_array().unwrap().len(), 3);
    }
}
