//! Generic versioned wrapper type
//!
//! This type expresses Invariant 2: Everything is Versioned.
//! Every read operation returns data wrapped in `Versioned<T>`.
//!
//! ## The Contract
//!
//! ```text
//! fn get(&self, ...) -> Result<Option<Versioned<T>>>
//! fn put(&self, ...) -> Result<Version>
//! ```
//!
//! - Reads return `Versioned<T>` (value + version + timestamp)
//! - Writes return `Version` (the version that was created)
//!
//! ## Migration from VersionedValue
//!
//! The old `VersionedValue` type is replaced by `Versioned<Value>`.
//! This generic version allows any type to be versioned.

use super::{Timestamp, Version};
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// A value with its version information
///
/// This wrapper adds version metadata to any value type.
/// Every read operation in the database returns `Versioned<T>`.
///
/// ## Fields
///
/// - `value`: The actual data
/// - `version`: The version identifier (TxnId, Sequence, or Counter)
/// - `timestamp`: When this version was created (microseconds since epoch)
///
/// ## Invariants
///
/// - `version` always matches the mutation that created this data
/// - `timestamp` is always the creation time of this version
/// - Value is never modified after creation (immutable versions)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Versioned<T> {
    /// The actual value
    pub value: T,

    /// Version identifier
    pub version: Version,

    /// Creation timestamp (microseconds since epoch)
    pub timestamp: Timestamp,
}

impl<T> Versioned<T> {
    /// Create a new versioned value with current timestamp
    pub fn new(value: T, version: Version) -> Self {
        Versioned {
            value,
            version,
            timestamp: Timestamp::now(),
        }
    }

    /// Create a versioned value with explicit timestamp
    pub fn with_timestamp(value: T, version: Version, timestamp: Timestamp) -> Self {
        Versioned {
            value,
            version,
            timestamp,
        }
    }

    /// Map the inner value to a new type
    pub fn map<U, F>(self, f: F) -> Versioned<U>
    where
        F: FnOnce(T) -> U,
    {
        Versioned {
            value: f(self.value),
            version: self.version,
            timestamp: self.timestamp,
        }
    }

    /// Get a reference to the inner value
    #[inline]
    pub fn value(&self) -> &T {
        &self.value
    }

    /// Get a mutable reference to the inner value
    #[inline]
    pub fn value_mut(&mut self) -> &mut T {
        &mut self.value
    }

    /// Consume and return the inner value
    #[inline]
    pub fn into_value(self) -> T {
        self.value
    }

    /// Get the version
    #[inline]
    pub fn version(&self) -> Version {
        self.version
    }

    /// Get the timestamp
    #[inline]
    pub fn timestamp(&self) -> Timestamp {
        self.timestamp
    }

    /// Check if this version is older than a duration
    pub fn is_older_than(&self, duration: Duration) -> bool {
        let now = Timestamp::now();
        if let Some(age) = now.duration_since(self.timestamp) {
            age > duration
        } else {
            false
        }
    }

    /// Get the age of this version
    pub fn age(&self) -> Option<Duration> {
        Timestamp::now().duration_since(self.timestamp)
    }

    /// Extract value and version as a tuple
    pub fn into_parts(self) -> (T, Version, Timestamp) {
        (self.value, self.version, self.timestamp)
    }
}

impl<T: Default> Default for Versioned<T> {
    fn default() -> Self {
        Versioned::new(T::default(), Version::default())
    }
}

impl<T> AsRef<T> for Versioned<T> {
    fn as_ref(&self) -> &T {
        &self.value
    }
}

impl<T> AsMut<T> for Versioned<T> {
    fn as_mut(&mut self) -> &mut T {
        &mut self.value
    }
}

// ============================================================================
// VersionedValue type alias for backwards compatibility
// ============================================================================

use crate::value::Value;

/// Versioned value for the Value enum
///
/// This is the most common use case - versioned arbitrary values.
/// Equivalent to the old `VersionedValue` struct.
pub type VersionedValue = Versioned<Value>;

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_versioned_new() {
        let v = Versioned::new(42i32, Version::txn(1));

        assert_eq!(v.value, 42);
        assert_eq!(v.version, Version::TxnId(1));
        // Timestamp should be recent
        assert!(v.timestamp.as_micros() > 0);
    }

    #[test]
    fn test_versioned_with_timestamp() {
        let ts = Timestamp::from_micros(12345);
        let v = Versioned::with_timestamp("hello", Version::seq(10), ts);

        assert_eq!(v.value, "hello");
        assert_eq!(v.version, Version::Sequence(10));
        assert_eq!(v.timestamp, ts);
    }

    #[test]
    fn test_versioned_accessors() {
        let v = Versioned::new(100u64, Version::counter(5));

        assert_eq!(*v.value(), 100);
        assert_eq!(v.version(), Version::Counter(5));
        assert!(v.timestamp().as_micros() > 0);
    }

    #[test]
    fn test_versioned_value_mut() {
        let mut v = Versioned::new(vec![1, 2, 3], Version::txn(1));
        v.value_mut().push(4);

        assert_eq!(v.value, vec![1, 2, 3, 4]);
    }

    #[test]
    fn test_versioned_into_value() {
        let v = Versioned::new("owned".to_string(), Version::txn(1));
        let s: String = v.into_value();
        assert_eq!(s, "owned");
    }

    #[test]
    fn test_versioned_map() {
        let v = Versioned::new(5i32, Version::txn(1));
        let v2 = v.map(|n| n * 2);

        assert_eq!(v2.value, 10);
        assert_eq!(v2.version, Version::TxnId(1));
    }

    #[test]
    fn test_versioned_into_parts() {
        let ts = Timestamp::from_micros(1000);
        let v = Versioned::with_timestamp(42, Version::txn(5), ts);
        let (val, ver, time) = v.into_parts();

        assert_eq!(val, 42);
        assert_eq!(ver, Version::TxnId(5));
        assert_eq!(time, ts);
    }

    #[test]
    fn test_versioned_is_older_than() {
        // Create with old timestamp
        let old_ts = Timestamp::from_micros(0);
        let v = Versioned::with_timestamp(1, Version::txn(1), old_ts);

        // Should be older than 1 second
        assert!(v.is_older_than(Duration::from_secs(1)));
    }

    #[test]
    fn test_versioned_age() {
        let v = Versioned::new(1, Version::txn(1));
        let age = v.age();

        // Should have an age (created just now)
        assert!(age.is_some());
        assert!(age.unwrap() < Duration::from_secs(1));
    }

    #[test]
    fn test_versioned_default() {
        let v: Versioned<i32> = Versioned::default();

        assert_eq!(v.value, 0);
        assert_eq!(v.version, Version::default());
    }

    #[test]
    fn test_versioned_as_ref() {
        let v = Versioned::new(vec![1, 2, 3], Version::txn(1));
        let slice: &Vec<i32> = v.as_ref();
        assert_eq!(slice, &vec![1, 2, 3]);
    }

    #[test]
    fn test_versioned_as_mut() {
        let mut v = Versioned::new(vec![1, 2, 3], Version::txn(1));
        let vec: &mut Vec<i32> = v.as_mut();
        vec.push(4);
        assert_eq!(v.value, vec![1, 2, 3, 4]);
    }

    #[test]
    fn test_versioned_equality() {
        let ts = Timestamp::from_micros(1000);

        let v1 = Versioned::with_timestamp(42, Version::txn(1), ts);
        let v2 = Versioned::with_timestamp(42, Version::txn(1), ts);
        let v3 = Versioned::with_timestamp(42, Version::txn(2), ts);
        let v4 = Versioned::with_timestamp(43, Version::txn(1), ts);

        assert_eq!(v1, v2);
        assert_ne!(v1, v3); // Different version
        assert_ne!(v1, v4); // Different value
    }

    #[test]
    fn test_versioned_serialization() {
        let ts = Timestamp::from_micros(12345);
        let v = Versioned::with_timestamp("test", Version::seq(10), ts);

        let json = serde_json::to_string(&v).unwrap();
        let restored: Versioned<&str> = serde_json::from_str(&json).unwrap();

        assert_eq!(v.value, restored.value);
        assert_eq!(v.version, restored.version);
        assert_eq!(v.timestamp, restored.timestamp);
    }

    #[test]
    fn test_versioned_value_alias() {
        // VersionedValue should work as Versioned<Value>
        let v: VersionedValue = Versioned::new(Value::I64(42), Version::txn(1));
        assert!(matches!(v.value, Value::I64(42)));
    }

    #[test]
    fn test_versioned_clone() {
        let v1 = Versioned::new(vec![1, 2, 3], Version::txn(1));
        let v2 = v1.clone();

        assert_eq!(v1, v2);
    }
}
