//! Storage-layer value wrapper with TTL support
//!
//! The contract type `Versioned<T>` doesn't include TTL because TTL is
//! a storage concern, not a contract concern. This module provides
//! `StoredValue` which combines a `VersionedValue` with optional TTL
//! for the storage layer.

use std::time::Duration;

use strata_core::{Timestamp, Value, Version, VersionedValue};

/// A stored value with optional TTL
///
/// Wraps `VersionedValue` with TTL metadata for the storage layer.
/// This separation keeps TTL as a storage concern, not part of the
/// contract types.
#[derive(Debug, Clone, PartialEq)]
pub struct StoredValue {
    /// The versioned value (value + version + timestamp)
    inner: VersionedValue,
    /// Optional time-to-live
    ttl: Option<Duration>,
}

impl StoredValue {
    /// Create a new stored value with TTL
    pub fn new(value: Value, version: Version, ttl: Option<Duration>) -> Self {
        StoredValue {
            inner: VersionedValue::new(value, version),
            ttl,
        }
    }

    /// Create a stored value with explicit timestamp
    pub fn with_timestamp(
        value: Value,
        version: Version,
        timestamp: Timestamp,
        ttl: Option<Duration>,
    ) -> Self {
        StoredValue {
            inner: VersionedValue::with_timestamp(value, version, timestamp),
            ttl,
        }
    }

    /// Create from a VersionedValue without TTL
    pub fn from_versioned(vv: VersionedValue) -> Self {
        StoredValue {
            inner: vv,
            ttl: None,
        }
    }

    /// Create from a VersionedValue with TTL
    pub fn from_versioned_with_ttl(vv: VersionedValue, ttl: Option<Duration>) -> Self {
        StoredValue { inner: vv, ttl }
    }

    /// Get the inner VersionedValue
    #[inline]
    pub fn versioned(&self) -> &VersionedValue {
        &self.inner
    }

    /// Consume and return the inner VersionedValue
    #[inline]
    pub fn into_versioned(self) -> VersionedValue {
        self.inner
    }

    /// Get the value
    #[inline]
    pub fn value(&self) -> &Value {
        &self.inner.value
    }

    /// Get the version
    #[inline]
    pub fn version(&self) -> Version {
        self.inner.version
    }

    /// Get the timestamp
    #[inline]
    pub fn timestamp(&self) -> Timestamp {
        self.inner.timestamp
    }

    /// Get the TTL
    #[inline]
    pub fn ttl(&self) -> Option<Duration> {
        self.ttl
    }

    /// Check if this value has expired
    pub fn is_expired(&self) -> bool {
        if let Some(ttl) = self.ttl {
            let now = Timestamp::now();
            if let Some(age) = now.duration_since(self.inner.timestamp) {
                return age >= ttl;
            }
        }
        false
    }

    /// Calculate the expiry timestamp
    ///
    /// Returns `Some(timestamp)` when the value will expire, or `None` if no TTL.
    pub fn expiry_timestamp(&self) -> Option<Timestamp> {
        self.ttl
            .map(|ttl| self.inner.timestamp.saturating_add(ttl))
    }
}

impl From<StoredValue> for VersionedValue {
    fn from(sv: StoredValue) -> Self {
        sv.inner
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stored_value_new() {
        let sv = StoredValue::new(Value::I64(42), Version::txn(1), None);
        assert_eq!(*sv.value(), Value::I64(42));
        assert_eq!(sv.version(), Version::TxnId(1));
        assert!(sv.ttl().is_none());
        assert!(!sv.is_expired());
    }

    #[test]
    fn test_stored_value_with_ttl() {
        let sv = StoredValue::new(
            Value::String("test".to_string()),
            Version::txn(1),
            Some(Duration::from_secs(60)),
        );
        assert!(sv.ttl().is_some());
        assert_eq!(sv.ttl().unwrap(), Duration::from_secs(60));
        assert!(!sv.is_expired()); // Not expired immediately
    }

    #[test]
    fn test_stored_value_expired() {
        // Create with old timestamp
        let old_ts = Timestamp::from_micros(0);
        let sv = StoredValue::with_timestamp(
            Value::Null,
            Version::txn(1),
            old_ts,
            Some(Duration::from_secs(1)),
        );
        // Should be expired (timestamp is epoch, TTL is 1 second)
        assert!(sv.is_expired());
    }

    #[test]
    fn test_stored_value_expiry_timestamp() {
        let ts = Timestamp::from_micros(1_000_000); // 1 second
        let sv = StoredValue::with_timestamp(
            Value::Null,
            Version::txn(1),
            ts,
            Some(Duration::from_secs(60)),
        );

        let expiry = sv.expiry_timestamp().unwrap();
        // 1 second + 60 seconds = 61 seconds = 61_000_000 microseconds
        assert_eq!(expiry.as_micros(), 61_000_000);
    }

    #[test]
    fn test_stored_value_no_ttl_expiry() {
        let sv = StoredValue::new(Value::Null, Version::txn(1), None);
        assert!(sv.expiry_timestamp().is_none());
    }

    #[test]
    fn test_stored_value_into_versioned() {
        let sv = StoredValue::new(Value::I64(42), Version::txn(5), Some(Duration::from_secs(10)));
        let vv = sv.into_versioned();
        assert_eq!(vv.value, Value::I64(42));
        assert_eq!(vv.version, Version::TxnId(5));
    }

    #[test]
    fn test_stored_value_from_versioned() {
        let vv = VersionedValue::new(Value::Bool(true), Version::seq(10));
        let sv = StoredValue::from_versioned(vv);
        assert_eq!(*sv.value(), Value::Bool(true));
        assert_eq!(sv.version(), Version::Sequence(10));
        assert!(sv.ttl().is_none());
    }
}
