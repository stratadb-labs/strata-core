//! Size limits for values and keys
//!
//! This module defines configurable size limits that are enforced by the engine
//! and wire decoding. Violations result in `ConstraintViolation` errors.
//!
//! ## Contract
//!
//! After stabilization, the default limits are FROZEN and cannot change without a major
//! version bump. Custom limits can be set at database open time.

use crate::Value;
use thiserror::Error;

/// Size limits for values and keys
///
/// These limits are enforced by the engine and wire decoding.
/// Violations return `ConstraintViolation` with appropriate reason codes.
#[derive(Debug, Clone)]
pub struct Limits {
    /// Maximum key length in bytes (default: 1024)
    pub max_key_bytes: usize,

    /// Maximum string length in bytes (default: 16MB)
    pub max_string_bytes: usize,

    /// Maximum bytes length (default: 16MB)
    pub max_bytes_len: usize,

    /// Maximum encoded value size in bytes (default: 32MB)
    pub max_value_bytes_encoded: usize,

    /// Maximum array length (default: 1M elements)
    pub max_array_len: usize,

    /// Maximum object entries (default: 1M entries)
    pub max_object_entries: usize,

    /// Maximum nesting depth (default: 128)
    pub max_nesting_depth: usize,

    /// Maximum vector dimensions (default: 8192)
    pub max_vector_dim: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Limits {
            max_key_bytes: 1024,
            max_string_bytes: 16 * 1024 * 1024,        // 16MB
            max_bytes_len: 16 * 1024 * 1024,           // 16MB
            max_value_bytes_encoded: 32 * 1024 * 1024, // 32MB
            max_array_len: 1_000_000,
            max_object_entries: 1_000_000,
            max_nesting_depth: 128,
            max_vector_dim: 8192,
        }
    }
}

impl Limits {
    /// Create limits with small values for testing
    ///
    /// This is useful for unit tests that need to test limit enforcement
    /// without creating extremely large values.
    pub fn with_small_limits() -> Self {
        Limits {
            max_key_bytes: 100,
            max_string_bytes: 1000,
            max_bytes_len: 1000,
            max_value_bytes_encoded: 2000,
            max_array_len: 100,
            max_object_entries: 100,
            max_nesting_depth: 10,
            max_vector_dim: 100,
        }
    }

    /// Validate a key length
    ///
    /// Returns `Ok(())` if the key length is valid, or `Err(LimitError::KeyTooLong)`
    /// if it exceeds the maximum.
    ///
    /// Note: This only validates length. For full key validation including
    /// NUL bytes and reserved prefixes, use `validate_key()` from the `key` module.
    pub fn validate_key_length(&self, key: &str) -> Result<(), LimitError> {
        let len = key.len();
        if len > self.max_key_bytes {
            return Err(LimitError::KeyTooLong {
                actual: len,
                max: self.max_key_bytes,
            });
        }
        Ok(())
    }

    /// Validate a value against size limits
    ///
    /// This validates:
    /// - String length
    /// - Bytes length
    /// - Array length
    /// - Object entries count
    /// - Nesting depth (recursive)
    ///
    /// Does NOT validate encoded size (that must be checked separately).
    pub fn validate_value(&self, value: &Value) -> Result<(), LimitError> {
        self.validate_value_impl(value, 0)
    }

    fn validate_value_impl(&self, value: &Value, depth: usize) -> Result<(), LimitError> {
        if depth > self.max_nesting_depth {
            return Err(LimitError::NestingTooDeep {
                actual: depth,
                max: self.max_nesting_depth,
            });
        }

        match value {
            Value::Null | Value::Bool(_) | Value::Int(_) | Value::Float(_) => Ok(()),

            Value::String(s) => {
                if s.len() > self.max_string_bytes {
                    return Err(LimitError::ValueTooLarge {
                        reason: "string_too_long".to_string(),
                        actual: s.len(),
                        max: self.max_string_bytes,
                    });
                }
                Ok(())
            }

            Value::Bytes(b) => {
                if b.len() > self.max_bytes_len {
                    return Err(LimitError::ValueTooLarge {
                        reason: "bytes_too_long".to_string(),
                        actual: b.len(),
                        max: self.max_bytes_len,
                    });
                }
                Ok(())
            }

            Value::Array(arr) => {
                if arr.len() > self.max_array_len {
                    return Err(LimitError::ValueTooLarge {
                        reason: "array_too_long".to_string(),
                        actual: arr.len(),
                        max: self.max_array_len,
                    });
                }
                for v in arr {
                    self.validate_value_impl(v, depth + 1)?;
                }
                Ok(())
            }

            Value::Object(obj) => {
                if obj.len() > self.max_object_entries {
                    return Err(LimitError::ValueTooLarge {
                        reason: "object_too_many_entries".to_string(),
                        actual: obj.len(),
                        max: self.max_object_entries,
                    });
                }
                for v in obj.values() {
                    self.validate_value_impl(v, depth + 1)?;
                }
                Ok(())
            }
        }
    }

    /// Validate a vector against dimension limits
    pub fn validate_vector(&self, vec: &[f32]) -> Result<(), LimitError> {
        if vec.len() > self.max_vector_dim {
            return Err(LimitError::VectorDimExceeded {
                actual: vec.len(),
                max: self.max_vector_dim,
            });
        }
        Ok(())
    }

    /// Validate vector dimension matches existing dimension
    pub fn validate_vector_dimension_match(
        &self,
        existing_dim: usize,
        new_dim: usize,
    ) -> Result<(), LimitError> {
        if existing_dim != new_dim {
            return Err(LimitError::VectorDimMismatch {
                expected: existing_dim,
                actual: new_dim,
            });
        }
        Ok(())
    }
}

/// Limit validation errors
///
/// These errors map to `ConstraintViolation` error codes in the wire protocol.
#[derive(Debug, Error)]
pub enum LimitError {
    /// Key exceeds maximum length
    #[error("Key too long: {actual} bytes exceeds maximum {max}")]
    KeyTooLong {
        /// Actual key length in bytes
        actual: usize,
        /// Maximum allowed length
        max: usize,
    },

    /// Value exceeds size limits
    #[error("Value too large ({reason}): {actual} exceeds maximum {max}")]
    ValueTooLarge {
        /// Reason code for the violation
        reason: String,
        /// Actual size
        actual: usize,
        /// Maximum allowed size
        max: usize,
    },

    /// Value nesting exceeds maximum depth
    #[error("Nesting too deep: {actual} levels exceeds maximum {max}")]
    NestingTooDeep {
        /// Actual nesting depth
        actual: usize,
        /// Maximum allowed depth
        max: usize,
    },

    /// Vector dimension exceeds maximum
    #[error("Vector dimension exceeded: {actual} exceeds maximum {max}")]
    VectorDimExceeded {
        /// Actual vector dimension
        actual: usize,
        /// Maximum allowed dimension
        max: usize,
    },

    /// Vector dimension mismatch with existing vector
    #[error("Vector dimension mismatch: expected {expected}, got {actual}")]
    VectorDimMismatch {
        /// Expected dimension
        expected: usize,
        /// Actual dimension
        actual: usize,
    },
}

impl LimitError {
    /// Get the reason code for wire protocol
    pub fn reason_code(&self) -> &'static str {
        match self {
            LimitError::KeyTooLong { .. } => "key_too_long",
            LimitError::ValueTooLarge { reason, .. } => match reason.as_str() {
                "string_too_long" => "value_too_large",
                "bytes_too_long" => "value_too_large",
                "array_too_long" => "value_too_large",
                "object_too_many_entries" => "value_too_large",
                _ => "value_too_large",
            },
            LimitError::NestingTooDeep { .. } => "nesting_too_deep",
            LimitError::VectorDimExceeded { .. } => "vector_dim_exceeded",
            LimitError::VectorDimMismatch { .. } => "vector_dim_mismatch",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    // === Key Length Tests ===

    #[test]
    fn test_key_at_max_length() {
        let limits = Limits::default();
        let key = "x".repeat(limits.max_key_bytes);
        assert!(limits.validate_key_length(&key).is_ok());
    }

    #[test]
    fn test_key_exceeds_max_length() {
        let limits = Limits::default();
        let key = "x".repeat(limits.max_key_bytes + 1);
        let result = limits.validate_key_length(&key);
        assert!(matches!(result, Err(LimitError::KeyTooLong { .. })));
    }

    #[test]
    fn test_key_much_larger_than_max() {
        let limits = Limits::default();
        let key = "x".repeat(10 * 1024); // 10KB
        let result = limits.validate_key_length(&key);
        assert!(matches!(result, Err(LimitError::KeyTooLong { .. })));
    }

    // === String Length Tests ===

    #[test]
    fn test_string_at_max_length() {
        let limits = Limits::with_small_limits();
        let s = "x".repeat(limits.max_string_bytes);
        let value = Value::String(s);
        assert!(limits.validate_value(&value).is_ok());
    }

    #[test]
    fn test_string_exceeds_max_length() {
        let limits = Limits::with_small_limits();
        let s = "x".repeat(limits.max_string_bytes + 1);
        let value = Value::String(s);
        let result = limits.validate_value(&value);
        assert!(matches!(result, Err(LimitError::ValueTooLarge { .. })));
    }

    // === Bytes Length Tests ===

    #[test]
    fn test_bytes_at_max_length() {
        let limits = Limits::with_small_limits();
        let b = vec![0u8; limits.max_bytes_len];
        let value = Value::Bytes(b);
        assert!(limits.validate_value(&value).is_ok());
    }

    #[test]
    fn test_bytes_exceeds_max_length() {
        let limits = Limits::with_small_limits();
        let b = vec![0u8; limits.max_bytes_len + 1];
        let value = Value::Bytes(b);
        let result = limits.validate_value(&value);
        assert!(matches!(result, Err(LimitError::ValueTooLarge { .. })));
    }

    // === Array Length Tests ===

    #[test]
    fn test_array_at_max_length() {
        let limits = Limits::with_small_limits();
        let arr = vec![Value::Null; limits.max_array_len];
        let value = Value::Array(arr);
        assert!(limits.validate_value(&value).is_ok());
    }

    #[test]
    fn test_array_exceeds_max_length() {
        let limits = Limits::with_small_limits();
        let arr = vec![Value::Null; limits.max_array_len + 1];
        let value = Value::Array(arr);
        let result = limits.validate_value(&value);
        assert!(matches!(result, Err(LimitError::ValueTooLarge { .. })));
    }

    // === Object Entries Tests ===

    #[test]
    fn test_object_at_max_entries() {
        let limits = Limits::with_small_limits();
        let mut map = HashMap::new();
        for i in 0..limits.max_object_entries {
            map.insert(format!("key{}", i), Value::Null);
        }
        let value = Value::Object(map);
        assert!(limits.validate_value(&value).is_ok());
    }

    #[test]
    fn test_object_exceeds_max_entries() {
        let limits = Limits::with_small_limits();
        let mut map = HashMap::new();
        for i in 0..=limits.max_object_entries {
            map.insert(format!("key{}", i), Value::Null);
        }
        let value = Value::Object(map);
        let result = limits.validate_value(&value);
        assert!(matches!(result, Err(LimitError::ValueTooLarge { .. })));
    }

    // === Nesting Depth Tests ===

    fn create_nested_array(depth: usize) -> Value {
        let mut value = Value::Null;
        for _ in 0..depth {
            value = Value::Array(vec![value]);
        }
        value
    }

    #[test]
    fn test_nesting_at_max_depth() {
        let limits = Limits::with_small_limits();
        let value = create_nested_array(limits.max_nesting_depth);
        assert!(limits.validate_value(&value).is_ok());
    }

    #[test]
    fn test_nesting_exceeds_max_depth() {
        let limits = Limits::with_small_limits();
        let value = create_nested_array(limits.max_nesting_depth + 1);
        let result = limits.validate_value(&value);
        assert!(matches!(result, Err(LimitError::NestingTooDeep { .. })));
    }

    // === Vector Dimension Tests ===

    #[test]
    fn test_vector_at_max_dim() {
        let limits = Limits::default();
        let vec = vec![0.0f32; limits.max_vector_dim];
        assert!(limits.validate_vector(&vec).is_ok());
    }

    #[test]
    fn test_vector_exceeds_max_dim() {
        let limits = Limits::default();
        let vec = vec![0.0f32; limits.max_vector_dim + 1];
        let result = limits.validate_vector(&vec);
        assert!(matches!(result, Err(LimitError::VectorDimExceeded { .. })));
    }

    #[test]
    fn test_vector_dimension_match() {
        let limits = Limits::default();
        assert!(limits.validate_vector_dimension_match(256, 256).is_ok());
    }

    #[test]
    fn test_vector_dimension_mismatch() {
        let limits = Limits::default();
        let result = limits.validate_vector_dimension_match(256, 512);
        assert!(matches!(result, Err(LimitError::VectorDimMismatch { .. })));
    }

    // === Custom Limits Tests ===

    #[test]
    fn test_custom_limits_respected() {
        let limits = Limits {
            max_key_bytes: 100,
            ..Limits::default()
        };

        let key = "x".repeat(100);
        assert!(limits.validate_key_length(&key).is_ok());

        let key = "x".repeat(101);
        assert!(limits.validate_key_length(&key).is_err());
    }

    // === Default Limits Verification ===

    #[test]
    fn test_default_limits_match_spec() {
        let limits = Limits::default();

        assert_eq!(limits.max_key_bytes, 1024);
        assert_eq!(limits.max_string_bytes, 16 * 1024 * 1024);
        assert_eq!(limits.max_bytes_len, 16 * 1024 * 1024);
        assert_eq!(limits.max_value_bytes_encoded, 32 * 1024 * 1024);
        assert_eq!(limits.max_array_len, 1_000_000);
        assert_eq!(limits.max_object_entries, 1_000_000);
        assert_eq!(limits.max_nesting_depth, 128);
        assert_eq!(limits.max_vector_dim, 8192);
    }

    // === Reason Code Tests ===

    #[test]
    fn test_reason_codes() {
        assert_eq!(
            LimitError::KeyTooLong {
                actual: 2000,
                max: 1024
            }
            .reason_code(),
            "key_too_long"
        );

        assert_eq!(
            LimitError::NestingTooDeep {
                actual: 200,
                max: 128
            }
            .reason_code(),
            "nesting_too_deep"
        );

        assert_eq!(
            LimitError::VectorDimExceeded {
                actual: 10000,
                max: 8192
            }
            .reason_code(),
            "vector_dim_exceeded"
        );

        assert_eq!(
            LimitError::VectorDimMismatch {
                expected: 256,
                actual: 512
            }
            .reason_code(),
            "vector_dim_mismatch"
        );
    }

    // === Primitive Type Tests ===

    #[test]
    fn test_null_always_valid() {
        let limits = Limits::with_small_limits();
        assert!(limits.validate_value(&Value::Null).is_ok());
    }

    #[test]
    fn test_bool_always_valid() {
        let limits = Limits::with_small_limits();
        assert!(limits.validate_value(&Value::Bool(true)).is_ok());
        assert!(limits.validate_value(&Value::Bool(false)).is_ok());
    }

    #[test]
    fn test_int_always_valid() {
        let limits = Limits::with_small_limits();
        assert!(limits.validate_value(&Value::Int(0)).is_ok());
        assert!(limits.validate_value(&Value::Int(i64::MAX)).is_ok());
        assert!(limits.validate_value(&Value::Int(i64::MIN)).is_ok());
    }

    #[test]
    fn test_float_always_valid() {
        let limits = Limits::with_small_limits();
        assert!(limits.validate_value(&Value::Float(0.0)).is_ok());
        assert!(limits.validate_value(&Value::Float(f64::MAX)).is_ok());
        assert!(limits.validate_value(&Value::Float(f64::NAN)).is_ok());
        assert!(limits.validate_value(&Value::Float(f64::INFINITY)).is_ok());
    }
}
