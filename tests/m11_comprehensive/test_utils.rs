//! Test utilities for M11 comprehensive tests
//!
//! Provides helpers for creating test values, assertions, and test harness setup.

use std::collections::HashMap;

/// Placeholder for Value type - will be replaced with actual import
/// when the core crate implements the M11 Value model
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    String(String),
    Bytes(Vec<u8>),
    Array(Vec<Value>),
    Object(HashMap<String, Value>),
}

impl Value {
    /// Returns the type name of this value
    pub fn type_name(&self) -> &'static str {
        match self {
            Value::Null => "null",
            Value::Bool(_) => "bool",
            Value::Int(_) => "int",
            Value::Float(_) => "float",
            Value::String(_) => "string",
            Value::Bytes(_) => "bytes",
            Value::Array(_) => "array",
            Value::Object(_) => "object",
        }
    }

    /// Check if this is a special float (NaN, Infinity, -0.0)
    pub fn is_special_float(&self) -> bool {
        match self {
            Value::Float(f) => f.is_nan() || f.is_infinite() || (f.to_bits() == (-0.0_f64).to_bits()),
            _ => false,
        }
    }
}

/// Compare two values for equality, handling NaN specially
pub fn values_equal(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Float(fa), Value::Float(fb)) => {
            if fa.is_nan() && fb.is_nan() {
                // Both NaN - consider equal for round-trip testing
                true
            } else {
                fa == fb
            }
        }
        (Value::Array(aa), Value::Array(ab)) => {
            aa.len() == ab.len() && aa.iter().zip(ab.iter()).all(|(x, y)| values_equal(x, y))
        }
        (Value::Object(oa), Value::Object(ob)) => {
            oa.len() == ob.len()
                && oa.iter().all(|(k, v)| ob.get(k).map_or(false, |v2| values_equal(v, v2)))
        }
        _ => a == b,
    }
}

/// Generate a deeply nested value
pub fn gen_nested_value(depth: usize) -> Value {
    if depth == 0 {
        Value::Int(depth as i64)
    } else {
        let mut obj = HashMap::new();
        obj.insert("nested".to_string(), gen_nested_value(depth - 1));
        Value::Object(obj)
    }
}

/// Generate a deeply nested array
pub fn gen_nested_array(depth: usize) -> Value {
    if depth == 0 {
        Value::Int(depth as i64)
    } else {
        Value::Array(vec![gen_nested_array(depth - 1)])
    }
}

/// Generate a string of specified byte length
pub fn gen_string_of_length(len: usize) -> String {
    "x".repeat(len)
}

/// Generate bytes of specified length
pub fn gen_bytes_of_length(len: usize) -> Vec<u8> {
    vec![0u8; len]
}

/// Generate a key of specified byte length
pub fn gen_key_of_length(len: usize) -> String {
    "k".repeat(len)
}

/// Placeholder Version type
#[derive(Debug, Clone, PartialEq)]
pub enum Version {
    Txn(u64),
    Sequence(u64),
    Counter(u64),
}

/// Placeholder Versioned type
#[derive(Debug, Clone)]
pub struct Versioned<T> {
    pub value: T,
    pub version: Version,
    pub timestamp: u64,
}

/// Placeholder error types
#[derive(Debug, Clone, PartialEq)]
pub enum StrataError {
    NotFound { key: String },
    WrongType { expected: &'static str, actual: &'static str },
    InvalidKey { key: String, reason: String },
    InvalidPath { path: String, reason: String },
    ConstraintViolation { reason: String },
    Conflict { expected: Value, actual: Value },
    RunNotFound { run_id: String },
    RunClosed { run_id: String },
    RunExists { run_id: String },
    HistoryTrimmed { requested: Version, earliest_retained: Version },
    Overflow,
    Internal { message: String },
}

impl StrataError {
    pub fn error_code(&self) -> &'static str {
        match self {
            StrataError::NotFound { .. } => "NotFound",
            StrataError::WrongType { .. } => "WrongType",
            StrataError::InvalidKey { .. } => "InvalidKey",
            StrataError::InvalidPath { .. } => "InvalidPath",
            StrataError::ConstraintViolation { .. } => "ConstraintViolation",
            StrataError::Conflict { .. } => "Conflict",
            StrataError::RunNotFound { .. } => "RunNotFound",
            StrataError::RunClosed { .. } => "RunClosed",
            StrataError::RunExists { .. } => "RunExists",
            StrataError::HistoryTrimmed { .. } => "HistoryTrimmed",
            StrataError::Overflow => "Overflow",
            StrataError::Internal { .. } => "Internal",
        }
    }
}

/// Size limits configuration
pub struct Limits {
    pub max_key_bytes: usize,
    pub max_string_bytes: usize,
    pub max_bytes_len: usize,
    pub max_value_bytes_encoded: usize,
    pub max_array_len: usize,
    pub max_object_entries: usize,
    pub max_nesting_depth: usize,
    pub max_vector_dim: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_key_bytes: 1024,
            max_string_bytes: 16 * 1024 * 1024,      // 16 MiB
            max_bytes_len: 16 * 1024 * 1024,         // 16 MiB
            max_value_bytes_encoded: 32 * 1024 * 1024, // 32 MiB
            max_array_len: 1_000_000,
            max_object_entries: 1_000_000,
            max_nesting_depth: 128,
            max_vector_dim: 8192,
        }
    }
}

/// Key validation result
pub fn validate_key(key: &str) -> Result<(), StrataError> {
    if key.is_empty() {
        return Err(StrataError::InvalidKey {
            key: key.to_string(),
            reason: "key cannot be empty".to_string(),
        });
    }
    if key.contains('\0') {
        return Err(StrataError::InvalidKey {
            key: key.to_string(),
            reason: "key cannot contain NUL bytes".to_string(),
        });
    }
    if key.starts_with("_strata/") {
        return Err(StrataError::InvalidKey {
            key: key.to_string(),
            reason: "reserved prefix _strata/".to_string(),
        });
    }
    let limits = Limits::default();
    if key.len() > limits.max_key_bytes {
        return Err(StrataError::InvalidKey {
            key: key.to_string(),
            reason: format!("key exceeds max length {}", limits.max_key_bytes),
        });
    }
    Ok(())
}

/// Placeholder wire encoding functions
pub mod wire {
    use super::*;

    /// Simple base64 encoding for test purposes
    /// This is a minimal implementation - real code would use the base64 crate
    fn simple_base64_encode(data: &[u8]) -> String {
        const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let mut result = String::new();
        for chunk in data.chunks(3) {
            let b0 = chunk[0] as usize;
            let b1 = chunk.get(1).copied().unwrap_or(0) as usize;
            let b2 = chunk.get(2).copied().unwrap_or(0) as usize;

            result.push(ALPHABET[(b0 >> 2) & 0x3F] as char);
            result.push(ALPHABET[((b0 << 4) | (b1 >> 4)) & 0x3F] as char);
            if chunk.len() > 1 {
                result.push(ALPHABET[((b1 << 2) | (b2 >> 6)) & 0x3F] as char);
            } else {
                result.push('=');
            }
            if chunk.len() > 2 {
                result.push(ALPHABET[b2 & 0x3F] as char);
            } else {
                result.push('=');
            }
        }
        result
    }

    /// Encode a value to JSON string
    pub fn encode_json(value: &Value) -> String {
        match value {
            Value::Null => "null".to_string(),
            Value::Bool(true) => "true".to_string(),
            Value::Bool(false) => "false".to_string(),
            Value::Int(i) => i.to_string(),
            Value::Float(f) => {
                if f.is_nan() {
                    r#"{"$f64":"NaN"}"#.to_string()
                } else if *f == f64::INFINITY {
                    r#"{"$f64":"+Inf"}"#.to_string()
                } else if *f == f64::NEG_INFINITY {
                    r#"{"$f64":"-Inf"}"#.to_string()
                } else if f.to_bits() == (-0.0_f64).to_bits() {
                    r#"{"$f64":"-0.0"}"#.to_string()
                } else {
                    format!("{}", f)
                }
            }
            Value::String(s) => format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\"")),
            Value::Bytes(b) => {
                // Simple base64-like encoding for test purposes
                // (actual implementation would use proper base64)
                let encoded = simple_base64_encode(b);
                format!(r#"{{"$bytes":"{}"}}"#, encoded)
            }
            Value::Array(arr) => {
                let items: Vec<String> = arr.iter().map(encode_json).collect();
                format!("[{}]", items.join(","))
            }
            Value::Object(obj) => {
                let mut pairs: Vec<_> = obj.iter().collect();
                pairs.sort_by_key(|(k, _)| *k);
                let items: Vec<String> = pairs
                    .iter()
                    .map(|(k, v)| format!("\"{}\":{}", k, encode_json(v)))
                    .collect();
                format!("{{{}}}", items.join(","))
            }
        }
    }

    /// Decode a JSON string to a value (simplified)
    pub fn decode_json(_json: &str) -> Result<Value, String> {
        // Placeholder - actual implementation would parse JSON
        // This is a stub for testing structure
        Err("decode_json not implemented".to_string())
    }

    /// Encode a Version to JSON
    pub fn encode_version(version: &Version) -> String {
        match version {
            Version::Txn(v) => format!(r#"{{"type":"txn","value":{}}}"#, v),
            Version::Sequence(v) => format!(r#"{{"type":"sequence","value":{}}}"#, v),
            Version::Counter(v) => format!(r#"{{"type":"counter","value":{}}}"#, v),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_value_type_names() {
        assert_eq!(Value::Null.type_name(), "null");
        assert_eq!(Value::Bool(true).type_name(), "bool");
        assert_eq!(Value::Int(0).type_name(), "int");
        assert_eq!(Value::Float(0.0).type_name(), "float");
        assert_eq!(Value::String("".into()).type_name(), "string");
        assert_eq!(Value::Bytes(vec![]).type_name(), "bytes");
        assert_eq!(Value::Array(vec![]).type_name(), "array");
        assert_eq!(Value::Object(HashMap::new()).type_name(), "object");
    }

    #[test]
    fn test_validate_key() {
        assert!(validate_key("valid").is_ok());
        assert!(validate_key("").is_err());
        assert!(validate_key("a\0b").is_err());
        assert!(validate_key("_strata/foo").is_err());
        assert!(validate_key("_stratafoo").is_ok()); // No slash
    }
}
