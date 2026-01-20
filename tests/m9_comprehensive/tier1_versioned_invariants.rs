//! Versioned<T> Invariant Tests
//!
//! Tests that Versioned<T> correctly expresses Invariant 2: Everything is Versioned
//!
//! Every read returns a Versioned<T> which contains the value, its version,
//! and its timestamp.

use in_mem_core::{Timestamp, Value, Version, Versioned, VersionedValue};
use std::time::Duration;

// ============================================================================
// Every read returns version info
// ============================================================================

#[test]
fn versioned_always_has_version() {
    let v = Versioned::new("hello", Version::txn(1));

    // Version is always accessible
    assert_eq!(v.version(), Version::txn(1));
}

#[test]
fn versioned_always_has_timestamp() {
    let v = Versioned::new("hello", Version::txn(1));

    // Timestamp is always accessible (defaults to now())
    assert!(v.timestamp().as_micros() > 0);
}

#[test]
fn versioned_with_timestamp_preserves_timestamp() {
    let ts = Timestamp::from_micros(1_000_000);
    let v = Versioned::with_timestamp("hello", Version::txn(1), ts);

    assert_eq!(v.timestamp(), ts);
}

// ============================================================================
// Versioned value access
// ============================================================================

#[test]
fn versioned_value_accessor() {
    let v = Versioned::new(42i32, Version::txn(1));
    assert_eq!(*v.value(), 42);
}

#[test]
fn versioned_value_mut_accessor() {
    let mut v = Versioned::new(42i32, Version::txn(1));
    *v.value_mut() = 100;
    assert_eq!(*v.value(), 100);
}

#[test]
fn versioned_into_value_consumes() {
    let v = Versioned::new("hello".to_string(), Version::txn(1));
    let value = v.into_value();
    assert_eq!(value, "hello".to_string());
}

#[test]
fn versioned_into_parts_returns_all() {
    let ts = Timestamp::from_micros(1000);
    let v = Versioned::with_timestamp("hello", Version::txn(42), ts);
    let (value, version, timestamp) = v.into_parts();

    assert_eq!(value, "hello");
    assert_eq!(version, Version::txn(42));
    assert_eq!(timestamp, ts);
}

// ============================================================================
// Map preserves version info
// ============================================================================

#[test]
fn versioned_map_preserves_version() {
    let v = Versioned::new(42i32, Version::txn(5));
    let mapped = v.map(|x| x.to_string());

    assert_eq!(mapped.value(), "42");
    assert_eq!(mapped.version(), Version::txn(5));
}

#[test]
fn versioned_map_preserves_timestamp() {
    let ts = Timestamp::from_micros(1_000_000);
    let v = Versioned::with_timestamp(42i32, Version::txn(1), ts);
    let mapped = v.map(|x| x * 2);

    assert_eq!(mapped.timestamp(), ts);
}

#[test]
fn versioned_map_chains_correctly() {
    let v = Versioned::new(10i32, Version::seq(1));
    let result = v
        .map(|x| x + 5)
        .map(|x| x * 2)
        .map(|x| x.to_string());

    assert_eq!(result.value(), "30");
    assert_eq!(result.version(), Version::seq(1));
}

// ============================================================================
// Age calculations
// ============================================================================

#[test]
fn versioned_age_calculated_correctly() {
    let old_ts = Timestamp::from_micros(0); // Epoch
    let v = Versioned::with_timestamp("old", Version::txn(1), old_ts);

    let age = v.age().expect("should have age");
    // Age should be significant (since epoch)
    assert!(age.as_secs() > 0);
}

#[test]
fn versioned_is_older_than_correct() {
    let old_ts = Timestamp::from_micros(0);
    let v = Versioned::with_timestamp("old", Version::txn(1), old_ts);

    // Should be older than 1 second
    assert!(v.is_older_than(Duration::from_secs(1)));

    // Recent value should not be older than 1 hour
    let recent = Versioned::new("recent", Version::txn(2));
    assert!(!recent.is_older_than(Duration::from_secs(3600)));
}

// ============================================================================
// Versioned with different types
// ============================================================================

#[test]
fn versioned_with_string() {
    let v = Versioned::new("hello world".to_string(), Version::txn(1));
    assert_eq!(v.value(), "hello world");
}

#[test]
fn versioned_with_vec() {
    let v = Versioned::new(vec![1, 2, 3], Version::txn(1));
    assert_eq!(v.value(), &vec![1, 2, 3]);
}

#[test]
fn versioned_with_option() {
    let v: Versioned<Option<i32>> = Versioned::new(Some(42), Version::txn(1));
    assert_eq!(v.value(), &Some(42));
}

#[test]
fn versioned_with_value_enum() {
    let v = Versioned::new(Value::I64(42), Version::txn(1));
    assert_eq!(v.value(), &Value::I64(42));
}

// ============================================================================
// VersionedValue alias
// ============================================================================

#[test]
fn versioned_value_is_versioned_value() {
    let vv: VersionedValue = Versioned::new(Value::String("test".to_string()), Version::txn(1));
    assert_eq!(vv.value(), &Value::String("test".to_string()));
}

#[test]
fn versioned_value_map_to_different_type() {
    let vv: VersionedValue = Versioned::new(Value::I64(42), Version::txn(1));

    // Can map to extract inner value
    let mapped = vv.map(|v| match v {
        Value::I64(n) => n,
        _ => panic!("expected i64"),
    });

    assert_eq!(*mapped.value(), 42i64);
}

// ============================================================================
// Versioned equality
// ============================================================================

#[test]
fn versioned_equality_considers_value_and_version() {
    // Use explicit timestamps to avoid timing-related flakiness
    let ts = Timestamp::from_micros(1000);

    let v1 = Versioned::with_timestamp(42, Version::txn(1), ts);
    let v2 = Versioned::with_timestamp(42, Version::txn(1), ts);
    let v3 = Versioned::with_timestamp(42, Version::txn(2), ts); // Different version
    let v4 = Versioned::with_timestamp(100, Version::txn(1), ts); // Different value

    assert_eq!(v1, v2);
    assert_ne!(v1, v3); // Different version
    assert_ne!(v1, v4); // Different value
}

#[test]
fn versioned_equality_includes_timestamp() {
    // Two Versioned with same value/version but different timestamps
    // are NOT equal (derived PartialEq includes all fields)
    let v1 = Versioned::with_timestamp(42, Version::txn(1), Timestamp::from_micros(1000));
    let v2 = Versioned::with_timestamp(42, Version::txn(1), Timestamp::from_micros(2000));
    let v3 = Versioned::with_timestamp(42, Version::txn(1), Timestamp::from_micros(1000));

    // Different timestamps mean different values
    assert_ne!(v1, v2);
    // Same timestamps mean equal
    assert_eq!(v1, v3);
}

// ============================================================================
// Versioned serialization
// ============================================================================

#[test]
fn versioned_serialization_roundtrip() {
    let v = Versioned::with_timestamp(
        "hello".to_string(),
        Version::txn(42),
        Timestamp::from_micros(1_000_000),
    );

    let json = serde_json::to_string(&v).expect("serialize");
    let parsed: Versioned<String> = serde_json::from_str(&json).expect("deserialize");

    assert_eq!(v.value(), parsed.value());
    assert_eq!(v.version(), parsed.version());
    assert_eq!(v.timestamp(), parsed.timestamp());
}

// ============================================================================
// Versioned AsRef/AsMut
// ============================================================================

#[test]
fn versioned_as_ref_returns_value_ref() {
    let v = Versioned::new(vec![1, 2, 3], Version::txn(1));
    let slice: &[i32] = v.as_ref();
    assert_eq!(slice, &[1, 2, 3]);
}

#[test]
fn versioned_as_mut_returns_value_mut() {
    let mut v = Versioned::new(vec![1, 2, 3], Version::txn(1));
    let slice: &mut Vec<i32> = v.as_mut();
    slice.push(4);
    assert_eq!(v.value(), &vec![1, 2, 3, 4]);
}

// ============================================================================
// Versioned Default
// ============================================================================

#[test]
fn versioned_default_uses_type_default() {
    let v: Versioned<i32> = Versioned::default();
    assert_eq!(*v.value(), 0);
    assert!(v.version().is_zero());
}

#[test]
fn versioned_default_string() {
    let v: Versioned<String> = Versioned::default();
    assert_eq!(v.value(), "");
}

// ============================================================================
// Versioned Clone
// ============================================================================

#[test]
fn versioned_clone_is_independent() {
    let v1 = Versioned::new("hello".to_string(), Version::txn(1));
    let mut v2 = v1.clone();

    *v2.value_mut() = "world".to_string();

    assert_eq!(v1.value(), "hello");
    assert_eq!(v2.value(), "world");
}

// ============================================================================
// Edge cases
// ============================================================================

#[test]
fn versioned_with_unit_type() {
    let v: Versioned<()> = Versioned::new((), Version::txn(1));
    assert_eq!(v.value(), &());
}

#[test]
fn versioned_with_nested_versioned() {
    // Versioned of Versioned (unusual but should work)
    let inner = Versioned::new(42, Version::txn(1));
    let outer = Versioned::new(inner, Version::txn(2));

    assert_eq!(*outer.value().value(), 42);
    assert_eq!(outer.version(), Version::txn(2));
}

#[test]
fn versioned_preserves_version_variant() {
    let txn = Versioned::new(1, Version::txn(1));
    let seq = Versioned::new(2, Version::seq(1));
    let ctr = Versioned::new(3, Version::counter(1));

    assert!(txn.version().is_txn_id());
    assert!(seq.version().is_sequence());
    assert!(ctr.version().is_counter());
}
