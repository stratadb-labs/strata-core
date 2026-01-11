//! Value types and versioning for in-mem
//!
//! This module defines:
//! - Value: Unified enum for all primitive data types
//! - VersionedValue: Wrapper with version, timestamp, and TTL
//! - RunMetadataEntry: Metadata for agent runs
//! - Associated types for structured data (events, traces, etc.)

use crate::types::RunId;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Get current timestamp in milliseconds since Unix epoch
///
/// Used for run metadata timestamps.
pub fn now() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
}

/// Metadata entry for an agent run
///
/// Tracks the lifecycle and metadata of a run, including:
/// - Creation and completion timestamps
/// - Version range (first_version to last_version)
/// - Parent run (for forked runs)
/// - Custom tags for categorization
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RunMetadataEntry {
    /// Unique identifier for this run
    pub run_id: RunId,
    /// Parent run ID (if this run was forked)
    pub parent_run_id: Option<RunId>,
    /// Current status (e.g., "running", "completed", "failed")
    pub status: String,
    /// Timestamp when the run was created (millis since epoch)
    pub created_at: u64,
    /// Timestamp when the run completed (millis since epoch)
    pub completed_at: Option<u64>,
    /// First version assigned during this run
    pub first_version: u64,
    /// Last version assigned during this run
    pub last_version: u64,
    /// Custom tags for categorization
    pub tags: Vec<(String, String)>,
}

/// Timestamp type (Unix timestamp in seconds)
pub type Timestamp = i64;

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
    /// Run metadata entry
    RunMetadata(RunMetadataEntry),
}

/// Versioned value with metadata
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VersionedValue {
    /// The actual value
    pub value: Value,
    /// Monotonically increasing version number
    pub version: u64,
    /// Unix timestamp when the value was created
    pub timestamp: Timestamp,
    /// Optional time-to-live duration
    pub ttl: Option<Duration>,
}

impl VersionedValue {
    /// Create a new versioned value
    pub fn new(value: Value, version: u64, ttl: Option<Duration>) -> Self {
        Self {
            value,
            version,
            timestamp: Utc::now().timestamp(),
            ttl,
        }
    }

    /// Check if the value has expired based on TTL
    pub fn is_expired(&self) -> bool {
        if let Some(ttl) = self.ttl {
            let now = Utc::now().timestamp();
            let elapsed = now - self.timestamp;
            elapsed as u64 >= ttl.as_secs()
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::thread;

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

    // Tests for VersionedValue

    #[test]
    fn test_versioned_value_creation() {
        let value = Value::String("test".to_string());
        let versioned = VersionedValue::new(value.clone(), 42, None);

        assert_eq!(versioned.value, value);
        assert_eq!(versioned.version, 42);
        assert!(versioned.ttl.is_none());
        assert!(!versioned.is_expired());
    }

    #[test]
    fn test_versioned_value_with_ttl() {
        let value = Value::I64(100);
        let ttl = Duration::from_secs(60);
        let versioned = VersionedValue::new(value.clone(), 1, Some(ttl));

        assert_eq!(versioned.value, value);
        assert_eq!(versioned.version, 1);
        assert_eq!(versioned.ttl, Some(ttl));

        // Should not be expired immediately
        assert!(!versioned.is_expired());
    }

    #[test]
    fn test_versioned_value_ttl_expired() {
        let value = Value::String("ephemeral".to_string());
        let ttl = Duration::from_secs(1);
        let mut versioned = VersionedValue::new(value, 1, Some(ttl));

        // Should not be expired immediately
        assert!(!versioned.is_expired());

        // Manually set timestamp to the past to simulate expiration
        versioned.timestamp -= 2; // 2 seconds ago

        // Should be expired now
        assert!(versioned.is_expired());
    }

    #[test]
    fn test_versioned_value_no_ttl_never_expires() {
        let value = Value::Bool(true);
        let versioned = VersionedValue::new(value, 99, None);

        // Should never expire without TTL
        assert!(!versioned.is_expired());

        // Even after some time
        thread::sleep(Duration::from_millis(10));
        assert!(!versioned.is_expired());
    }

    #[test]
    fn test_versioned_value_serialization() {
        let value = Value::Array(vec![Value::I64(1), Value::String("test".to_string())]);
        let ttl = Duration::from_secs(300);
        let versioned = VersionedValue::new(value, 5, Some(ttl));

        let serialized = serde_json::to_string(&versioned).unwrap();
        let deserialized: VersionedValue = serde_json::from_str(&serialized).unwrap();

        assert_eq!(versioned.value, deserialized.value);
        assert_eq!(versioned.version, deserialized.version);
        assert_eq!(versioned.ttl, deserialized.ttl);
        // Timestamp should be very close (within a second)
        assert!((versioned.timestamp - deserialized.timestamp).abs() <= 1);
    }

    #[test]
    fn test_versioned_value_different_versions() {
        let value = Value::String("data".to_string());
        let v1 = VersionedValue::new(value.clone(), 1, None);
        let v2 = VersionedValue::new(value.clone(), 2, None);

        assert_ne!(v1.version, v2.version);
        assert_eq!(v1.value, v2.value);
    }

    #[test]
    fn test_versioned_value_timestamp_set() {
        let value = Value::Null;
        let versioned = VersionedValue::new(value, 0, None);

        let now = Utc::now().timestamp();
        // Timestamp should be within 1 second of current time
        assert!((versioned.timestamp - now).abs() <= 1);
    }
}
