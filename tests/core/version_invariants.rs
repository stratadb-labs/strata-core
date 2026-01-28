//! Version Invariant Tests
//!
//! Tests that Version correctly expresses Invariant 2: Everything is Versioned
//!
//! Every write produces a version. Versions are comparable within the same type
//! and incrementable.

use strata_core::Version;
use std::collections::HashSet;

// ============================================================================
// Version variants and construction
// ============================================================================

#[test]
fn version_txn_id_construction() {
    let v = Version::txn(42);
    assert!(v.is_txn_id());
    assert!(!v.is_sequence());
    assert!(!v.is_counter());
    assert_eq!(v.as_u64(), 42);
}

#[test]
fn version_sequence_construction() {
    let v = Version::seq(100);
    assert!(!v.is_txn_id());
    assert!(v.is_sequence());
    assert!(!v.is_counter());
    assert_eq!(v.as_u64(), 100);
}

#[test]
fn version_counter_construction() {
    let v = Version::counter(7);
    assert!(!v.is_txn_id());
    assert!(!v.is_sequence());
    assert!(v.is_counter());
    assert_eq!(v.as_u64(), 7);
}

#[test]
fn version_zero_constructors() {
    assert_eq!(Version::zero_txn().as_u64(), 0);
    assert_eq!(Version::zero_sequence().as_u64(), 0);
    assert_eq!(Version::zero_counter().as_u64(), 0);

    assert!(Version::zero_txn().is_txn_id());
    assert!(Version::zero_sequence().is_sequence());
    assert!(Version::zero_counter().is_counter());
}

// ============================================================================
// Versions are comparable within same type
// ============================================================================

#[test]
fn version_txn_id_comparable() {
    let v1 = Version::txn(1);
    let v2 = Version::txn(2);
    let v3 = Version::txn(2);

    assert!(v1 < v2);
    assert!(v2 > v1);
    assert_eq!(v2, v3);
    assert!(v1 <= v2);
    assert!(v2 >= v1);
}

#[test]
fn version_sequence_comparable() {
    let v1 = Version::seq(10);
    let v2 = Version::seq(20);
    let v3 = Version::seq(20);

    assert!(v1 < v2);
    assert!(v2 > v1);
    assert_eq!(v2, v3);
}

#[test]
fn version_counter_comparable() {
    let v1 = Version::counter(5);
    let v2 = Version::counter(10);
    let v3 = Version::counter(10);

    assert!(v1 < v2);
    assert!(v2 > v1);
    assert_eq!(v2, v3);
}

// ============================================================================
// Cross-variant comparison semantics
// ============================================================================

#[test]
fn version_cross_variant_ordering_defined() {
    // TxnId < Sequence < Counter (by variant discriminant)
    let txn = Version::txn(100);
    let seq = Version::seq(50);
    let ctr = Version::counter(25);

    // Cross-variant comparison is defined and consistent
    assert!(txn < seq);
    assert!(seq < ctr);
    assert!(txn < ctr);
}

#[test]
fn version_cross_variant_ordering_consistent_with_sorting() {
    let versions = vec![
        Version::counter(1),
        Version::txn(100),
        Version::seq(50),
        Version::txn(1),
        Version::counter(100),
        Version::seq(1),
    ];

    let mut sorted = versions.clone();
    sorted.sort();

    // All TxnId should come first, then Sequence, then Counter
    // Within each variant, sorted by value
    assert!(sorted[0].is_txn_id());
    assert!(sorted[1].is_txn_id());
    assert!(sorted[2].is_sequence());
    assert!(sorted[3].is_sequence());
    assert!(sorted[4].is_counter());
    assert!(sorted[5].is_counter());

    // Check value ordering within variant groups
    assert!(sorted[0].as_u64() <= sorted[1].as_u64()); // TxnId group
    assert!(sorted[2].as_u64() <= sorted[3].as_u64()); // Sequence group
    assert!(sorted[4].as_u64() <= sorted[5].as_u64()); // Counter group
}

// ============================================================================
// Versions increment correctly
// ============================================================================

#[test]
fn version_increment_produces_higher_version() {
    let v1 = Version::txn(10);
    let v2 = v1.increment();

    assert!(v2 > v1);
    assert_eq!(v2.as_u64(), 11);
    assert!(v2.is_txn_id()); // Variant preserved
}

#[test]
fn version_increment_preserves_variant() {
    let txn = Version::txn(5).increment();
    let seq = Version::seq(5).increment();
    let ctr = Version::counter(5).increment();

    assert!(txn.is_txn_id());
    assert!(seq.is_sequence());
    assert!(ctr.is_counter());
}

#[test]
fn version_saturating_increment_handles_overflow() {
    let max_txn = Version::txn(u64::MAX);
    let incremented = max_txn.saturating_increment();

    // Should not overflow, stays at max
    assert_eq!(incremented.as_u64(), u64::MAX);
    assert!(incremented.is_txn_id());
}

#[test]
fn version_saturating_increment_normal_case() {
    let v = Version::seq(100);
    let incremented = v.saturating_increment();

    assert_eq!(incremented.as_u64(), 101);
    assert!(incremented.is_sequence());
}

// ============================================================================
// Version is_zero check
// ============================================================================

#[test]
fn version_is_zero_correct() {
    assert!(Version::zero_txn().is_zero());
    assert!(Version::zero_sequence().is_zero());
    assert!(Version::zero_counter().is_zero());

    assert!(!Version::txn(1).is_zero());
    assert!(!Version::seq(1).is_zero());
    assert!(!Version::counter(1).is_zero());
}

// ============================================================================
// Version as_u64 for backwards compatibility
// ============================================================================

#[test]
fn version_as_u64_extracts_value() {
    assert_eq!(Version::txn(42).as_u64(), 42);
    assert_eq!(Version::seq(100).as_u64(), 100);
    assert_eq!(Version::counter(7).as_u64(), 7);
}

#[test]
fn version_from_u64_creates_txn_id() {
    let v: Version = 42u64.into();
    assert!(v.is_txn_id());
    assert_eq!(v.as_u64(), 42);
}

// ============================================================================
// Version is hashable
// ============================================================================

#[test]
fn version_hashable() {
    let mut set = HashSet::new();

    set.insert(Version::txn(1));
    set.insert(Version::txn(2));
    set.insert(Version::seq(1));
    set.insert(Version::counter(1));

    assert_eq!(set.len(), 4);
    assert!(set.contains(&Version::txn(1)));
    assert!(set.contains(&Version::seq(1)));
}

#[test]
fn version_equal_values_same_hash() {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let v1 = Version::txn(42);
    let v2 = Version::txn(42);

    let hash1 = {
        let mut h = DefaultHasher::new();
        v1.hash(&mut h);
        h.finish()
    };
    let hash2 = {
        let mut h = DefaultHasher::new();
        v2.hash(&mut h);
        h.finish()
    };

    assert_eq!(hash1, hash2);
}

// ============================================================================
// Version serialization
// ============================================================================

#[test]
fn version_serialization_roundtrip() {
    let versions = vec![
        Version::txn(42),
        Version::seq(100),
        Version::counter(7),
        Version::zero_txn(),
    ];

    for v in versions {
        let json = serde_json::to_string(&v).expect("serialize");
        let parsed: Version = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(v, parsed);
    }
}

// ============================================================================
// Version Display
// ============================================================================

#[test]
fn version_display_informative() {
    let txn = Version::txn(42);
    let seq = Version::seq(100);
    let ctr = Version::counter(7);

    let txn_str = format!("{}", txn);
    let seq_str = format!("{}", seq);
    let ctr_str = format!("{}", ctr);

    // Display should indicate variant and value
    assert!(txn_str.contains("42") || txn_str.contains("txn"), "TxnId display: {}", txn_str);
    assert!(seq_str.contains("100") || seq_str.contains("seq"), "Sequence display: {}", seq_str);
    assert!(ctr_str.contains("7") || ctr_str.contains("counter"), "Counter display: {}", ctr_str);
}

// ============================================================================
// Version Clone and Copy
// ============================================================================

#[test]
fn version_is_copy() {
    let v1 = Version::txn(42);
    let v2 = v1; // Copy
    let v3 = v1; // Copy again

    assert_eq!(v1, v2);
    assert_eq!(v2, v3);
}

#[test]
fn version_clone() {
    let v1 = Version::seq(100);
    let v2 = v1.clone();

    assert_eq!(v1, v2);
}

// ============================================================================
// Version Default
// ============================================================================

#[test]
fn version_default_is_zero_txn() {
    let default = Version::default();
    assert!(default.is_txn_id());
    assert!(default.is_zero());
}
