//! Test utilities for M9 comprehensive tests
//!
//! This module provides helper functions for creating test data and performing
//! common test assertions. These utilities are designed to be used across all
//! M9 test tiers and will be extended as Phase 2 and beyond are implemented.

#![allow(dead_code)] // Utilities are for future use across phases

use strata_core::{EntityRef, JsonDocId, PrimitiveType, RunId, RunName, Timestamp, Version, Versioned};
use std::collections::HashSet;

/// Create a test RunId
pub fn test_run_id() -> RunId {
    RunId::new()
}

/// Create a test RunName
pub fn test_run_name(name: &str) -> RunName {
    RunName::new(name.to_string()).expect("valid test run name")
}

/// Create a test Timestamp at a specific microsecond value
pub fn test_timestamp(micros: u64) -> Timestamp {
    Timestamp::from_micros(micros)
}

/// Create a test Version with TxnId variant
pub fn test_txn_version(id: u64) -> Version {
    Version::txn(id)
}

/// Create a test Version with Sequence variant
pub fn test_seq_version(n: u64) -> Version {
    Version::seq(n)
}

/// Create a test Version with Counter variant
pub fn test_counter_version(n: u64) -> Version {
    Version::counter(n)
}

/// Create a test Versioned wrapper
pub fn test_versioned<T>(value: T, version: Version) -> Versioned<T> {
    Versioned::new(value, version)
}

/// Create all six EntityRef variants for a given run
pub fn all_entity_refs(run_id: RunId) -> Vec<EntityRef> {
    vec![
        EntityRef::kv(run_id, "test_key"),
        EntityRef::event(run_id, 1),
        EntityRef::state(run_id, "test_state"),
        EntityRef::run(run_id),
        EntityRef::json(run_id, JsonDocId::new()),
        EntityRef::vector(run_id, "test_collection", "test_vector"),
    ]
}

/// Assert that an EntityRef can be used as a HashMap key
pub fn assert_hashable<T: std::hash::Hash + Eq>(value: &T) {
    let mut set = HashSet::new();
    set.insert(value);
    assert!(set.contains(value));
}

/// Assert that two values hash to the same value
pub fn assert_same_hash<T: std::hash::Hash>(a: &T, b: &T) {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::Hasher;

    let hash_a = {
        let mut hasher = DefaultHasher::new();
        a.hash(&mut hasher);
        hasher.finish()
    };

    let hash_b = {
        let mut hasher = DefaultHasher::new();
        b.hash(&mut hasher);
        hasher.finish()
    };

    assert_eq!(hash_a, hash_b, "Expected same hash for equal values");
}

/// All six primitive types
pub fn all_primitive_types() -> Vec<PrimitiveType> {
    vec![
        PrimitiveType::Kv,
        PrimitiveType::Event,
        PrimitiveType::State,
        PrimitiveType::Run,
        PrimitiveType::Json,
        PrimitiveType::Vector,
    ]
}

/// Test helper macro for checking that a type implements required traits
#[macro_export]
macro_rules! assert_traits {
    ($type:ty: $($trait:ident),+ $(,)?) => {
        fn _assert_traits<T: $($trait +)+>() {}
        _assert_traits::<$type>();
    };
}
