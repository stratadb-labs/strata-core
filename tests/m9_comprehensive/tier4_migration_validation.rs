//! Migration Validation Tests
//!
//! Tests that migrated code produces correct results:
//! - Timestamp: microseconds vs seconds conversion
//! - Version: comparison semantics across variants
//! - EntityRef: run_id extraction from all variants

use crate::test_utils::{all_entity_refs, test_run_id};
use strata_core::{EntityRef, JsonDocId, PrimitiveType, Timestamp, Version};
use std::time::Duration;

// ============================================================================
// Timestamp migration (seconds → microseconds)
// ============================================================================

#[test]
fn timestamp_microsecond_precision_preserved() {
    // The old Timestamp was seconds (i64). New is microseconds (u64).
    // Verify microsecond precision is maintained.

    let precise_micros = 1_234_567_890_123_456u64;
    let ts = Timestamp::from_micros(precise_micros);

    assert_eq!(ts.as_micros(), precise_micros);
}

#[test]
fn timestamp_from_seconds_converts_correctly() {
    // Code that was working with seconds should still work
    let seconds = 1000u64;
    let ts = Timestamp::from_secs(seconds);

    assert_eq!(ts.as_secs(), seconds);
    assert_eq!(ts.as_micros(), seconds * 1_000_000);
}

#[test]
fn timestamp_to_seconds_truncates_correctly() {
    // When reading as seconds, should truncate (not round)
    let ts = Timestamp::from_micros(1_999_999);

    assert_eq!(ts.as_secs(), 1); // Truncated, not 2
}

#[test]
fn timestamp_comparison_semantics_unchanged() {
    // Timestamps should compare correctly regardless of precision
    let ts1 = Timestamp::from_secs(100);
    let ts2 = Timestamp::from_secs(200);
    let ts3 = Timestamp::from_micros(100_000_001); // Just over 100 seconds

    assert!(ts1 < ts2);
    assert!(ts1 < ts3);
    assert!(ts3 < ts2);
}

#[test]
fn timestamp_arithmetic_works_with_duration() {
    let ts = Timestamp::from_secs(100);
    let duration = Duration::from_secs(50);

    let added = ts.saturating_add(duration);
    assert_eq!(added.as_secs(), 150);

    let subtracted = added.saturating_sub(duration);
    assert_eq!(subtracted.as_secs(), 100);
}

#[test]
fn timestamp_epoch_still_zero() {
    // EPOCH should still represent zero/epoch
    assert_eq!(Timestamp::EPOCH.as_micros(), 0);
    assert_eq!(Timestamp::EPOCH.as_secs(), 0);
}

// ============================================================================
// Version migration (u64 → Version enum)
// ============================================================================

#[test]
fn version_from_raw_u64() {
    // Old code used raw u64 for versions. New code uses Version enum.
    // From<u64> should create TxnId variant for backwards compatibility.
    let raw: u64 = 42;
    let version: Version = raw.into();

    assert!(version.is_txn_id());
    assert_eq!(version.as_u64(), 42);
}

#[test]
fn version_as_u64_for_comparison() {
    // Old code compared versions as u64. This should still work.
    let v1 = Version::txn(10);
    let v2 = Version::txn(20);

    // Can extract u64 for comparison
    assert!(v1.as_u64() < v2.as_u64());
}

#[test]
fn version_comparison_within_variant_unchanged() {
    // Same-variant comparison should work as before
    let v1 = Version::txn(10);
    let v2 = Version::txn(20);
    let v3 = Version::txn(10);

    assert!(v1 < v2);
    assert!(v2 > v1);
    assert_eq!(v1, v3);
}

#[test]
fn version_zero_semantics_preserved() {
    // Zero versions should still be recognized
    let zero_txn = Version::zero_txn();
    let zero_seq = Version::zero_sequence();
    let zero_ctr = Version::zero_counter();

    assert!(zero_txn.is_zero());
    assert!(zero_seq.is_zero());
    assert!(zero_ctr.is_zero());

    assert_eq!(zero_txn.as_u64(), 0);
    assert_eq!(zero_seq.as_u64(), 0);
    assert_eq!(zero_ctr.as_u64(), 0);
}

#[test]
fn version_increment_semantics_preserved() {
    // Incrementing should produce higher version
    let v1 = Version::txn(10);
    let v2 = v1.increment();

    assert!(v2 > v1);
    assert_eq!(v2.as_u64(), 11);
}

#[test]
fn version_default_is_zero() {
    // Default should be zero (TxnId(0))
    let default = Version::default();
    assert!(default.is_zero());
    assert!(default.is_txn_id());
}

// ============================================================================
// EntityRef migration (implicit run_id → explicit)
// ============================================================================

#[test]
fn entity_ref_run_id_always_accessible() {
    // Old DocRef had run_id embedded in Key. New EntityRef has explicit run_id.
    // run_id() should always return the run_id for any variant.
    let run_id = test_run_id();
    let refs = all_entity_refs(run_id.clone());

    for entity_ref in refs {
        assert_eq!(
            entity_ref.run_id(),
            run_id,
            "run_id should be accessible for {:?}",
            entity_ref
        );
    }
}

#[test]
fn entity_ref_kv_uses_string_key() {
    // Old DocRef::Kv used Key type. New uses String.
    let run_id = test_run_id();
    let entity_ref = EntityRef::kv(run_id, "my_key");

    // Key is now a simple string
    assert_eq!(entity_ref.kv_key(), Some("my_key"));
}

#[test]
fn entity_ref_primitive_type_method_replaces_kind() {
    // Old: primitive_kind(). New: primitive_type().
    let run_id = test_run_id();
    let entity_ref = EntityRef::kv(run_id, "key");

    // primitive_type() should work
    assert_eq!(entity_ref.primitive_type(), PrimitiveType::Kv);
}

#[test]
fn entity_ref_display_format_informative() {
    // Display should show useful information
    let run_id = test_run_id();
    let entity_ref = EntityRef::kv(run_id, "my_key");

    let display = format!("{}", entity_ref);
    assert!(!display.is_empty());
}

// ============================================================================
// Cross-migration: Version in Versioned
// ============================================================================

#[test]
fn versioned_accepts_all_version_variants() {
    use strata_core::Versioned;

    // Old VersionedValue had version: u64. New uses Version enum.
    let v1 = Versioned::new(42, Version::txn(1));
    let v2 = Versioned::new(42, Version::seq(1));
    let v3 = Versioned::new(42, Version::counter(1));

    // All should work
    assert_eq!(v1.version().as_u64(), 1);
    assert_eq!(v2.version().as_u64(), 1);
    assert_eq!(v3.version().as_u64(), 1);

    // But they're different versions
    assert_ne!(v1.version(), v2.version());
    assert_ne!(v2.version(), v3.version());
}

#[test]
fn versioned_timestamp_is_microseconds() {
    use strata_core::Versioned;

    let v = Versioned::new(42, Version::txn(1));

    // Timestamp should be in microseconds
    let ts = v.timestamp();
    assert!(ts.as_micros() > 0);
}

// ============================================================================
// Semantic preservation
// ============================================================================

#[test]
fn entity_ref_equality_semantics_preserved() {
    let run_id = test_run_id();

    // Same entity should equal itself
    let ref1 = EntityRef::kv(run_id.clone(), "key");
    let ref2 = EntityRef::kv(run_id.clone(), "key");
    assert_eq!(ref1, ref2);

    // Different entities should not equal
    let ref3 = EntityRef::kv(run_id, "other_key");
    assert_ne!(ref1, ref3);
}

#[test]
fn entity_ref_hash_semantics_preserved() {
    use std::collections::HashSet;
    let run_id = test_run_id();

    let mut set = HashSet::new();
    set.insert(EntityRef::kv(run_id.clone(), "key1"));
    set.insert(EntityRef::kv(run_id.clone(), "key2"));
    set.insert(EntityRef::kv(run_id.clone(), "key1")); // Duplicate

    assert_eq!(set.len(), 2);
}

#[test]
fn version_hash_semantics_preserved() {
    use std::collections::HashSet;

    let mut set = HashSet::new();
    set.insert(Version::txn(1));
    set.insert(Version::txn(2));
    set.insert(Version::txn(1)); // Duplicate

    assert_eq!(set.len(), 2);
}

#[test]
fn timestamp_hash_semantics_preserved() {
    use std::collections::HashSet;

    let mut set = HashSet::new();
    set.insert(Timestamp::from_micros(1000));
    set.insert(Timestamp::from_micros(2000));
    set.insert(Timestamp::from_micros(1000)); // Duplicate

    assert_eq!(set.len(), 2);
}

// ============================================================================
// Ordering preservation
// ============================================================================

#[test]
fn timestamp_ordering_preserved() {
    let timestamps: Vec<Timestamp> = vec![
        Timestamp::from_micros(300),
        Timestamp::from_micros(100),
        Timestamp::from_micros(200),
    ];

    let mut sorted = timestamps.clone();
    sorted.sort();

    assert_eq!(sorted[0].as_micros(), 100);
    assert_eq!(sorted[1].as_micros(), 200);
    assert_eq!(sorted[2].as_micros(), 300);
}

#[test]
fn version_ordering_preserved_within_variant() {
    let versions: Vec<Version> = vec![
        Version::txn(30),
        Version::txn(10),
        Version::txn(20),
    ];

    let mut sorted = versions.clone();
    sorted.sort();

    assert_eq!(sorted[0].as_u64(), 10);
    assert_eq!(sorted[1].as_u64(), 20);
    assert_eq!(sorted[2].as_u64(), 30);
}

// ============================================================================
// Type-specific extraction preserved
// ============================================================================

#[test]
fn entity_ref_extraction_methods_return_correct_types() {
    let run_id = test_run_id();
    let doc_id = JsonDocId::new();

    // Each variant should have its extraction method
    let kv = EntityRef::kv(run_id.clone(), "key");
    let event = EntityRef::event(run_id.clone(), 42);
    let state = EntityRef::state(run_id.clone(), "cell");
    let trace = EntityRef::trace(run_id, "trace-123");
    let json = EntityRef::json(run_id.clone(), doc_id.clone());
    let vector = EntityRef::vector(run_id.clone(), "coll", "vec");
    let run = EntityRef::run(run_id);

    // Extraction methods return correct values
    assert_eq!(kv.kv_key(), Some("key"));
    assert_eq!(event.event_sequence(), Some(42));
    assert_eq!(state.state_name(), Some("cell"));
    assert_eq!(trace.trace_id(), Some("trace-123"));
    assert_eq!(json.json_doc_id(), Some(doc_id));
    assert_eq!(vector.vector_location(), Some(("coll", "vec")));
    assert!(run.is_run()); // Run variant just has run_id
}

#[test]
fn entity_ref_extraction_returns_none_for_wrong_variant() {
    let run_id = test_run_id();
    let kv = EntityRef::kv(run_id, "key");

    // Wrong variant extraction returns None
    assert_eq!(kv.event_sequence(), None);
    assert_eq!(kv.state_name(), None);
    assert_eq!(kv.trace_id(), None);
    assert_eq!(kv.json_doc_id(), None);
    assert_eq!(kv.vector_location(), None);
}
