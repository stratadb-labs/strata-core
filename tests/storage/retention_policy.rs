//! Retention Policy Tests
//!
//! Tests for retention policy semantics and serialization.

use strata_core::PrimitiveType;
use strata_storage::retention::RetentionPolicy;
use std::time::Duration;

// Note: PrimitiveType variants are Kv, Event, State, Run, Json, Vector (not uppercase)

// ============================================================================
// Policy Type Tests
// ============================================================================

#[test]
fn keep_all_retains_everything() {
    let policy = RetentionPolicy::KeepAll;

    // Should always retain
    assert!(policy.should_retain(1, 0, 100, 1_000_000, PrimitiveType::Kv));
    assert!(policy.should_retain(100, 0, 100, 1_000_000, PrimitiveType::Kv));
    assert!(policy.should_retain(1, 0, 1, 1_000_000, PrimitiveType::Event));
}

#[test]
fn keep_last_n_retains_n_versions() {
    let policy = RetentionPolicy::KeepLast(3);

    // version_count = "versions remaining from this to oldest" (1 for newest)
    // KeepLast(3) retains if version_count <= 3
    //
    // With 5 versions (v1 oldest, v5 newest):
    // - v5: version_count = 1 → retained
    // - v4: version_count = 2 → retained
    // - v3: version_count = 3 → retained
    // - v2: version_count = 4 → NOT retained
    // - v1: version_count = 5 → NOT retained
    assert!(policy.should_retain(5, 0, 1, 0, PrimitiveType::Kv)); // Newest, count=1
    assert!(policy.should_retain(4, 0, 2, 0, PrimitiveType::Kv)); // 2nd newest, count=2
    assert!(policy.should_retain(3, 0, 3, 0, PrimitiveType::Kv)); // 3rd newest, count=3
    assert!(!policy.should_retain(2, 0, 4, 0, PrimitiveType::Kv)); // Too old, count=4
    assert!(!policy.should_retain(1, 0, 5, 0, PrimitiveType::Kv)); // Oldest, count=5
}

#[test]
fn keep_for_duration_uses_timestamp() {
    // Keep for 1 hour (in microseconds)
    let one_hour_us = 3600 * 1_000_000u64;
    let policy = RetentionPolicy::KeepFor(Duration::from_micros(one_hour_us));

    // Use a larger current_time to avoid overflow
    let current_time = one_hour_us * 2; // 2 hours in microseconds

    // Within retention window (1 second ago)
    assert!(policy.should_retain(
        1,
        current_time - 1_000_000, // 1 second ago
        1,
        current_time,
        PrimitiveType::Kv
    ));

    // Outside retention window (just over 1 hour ago)
    assert!(!policy.should_retain(
        1,
        current_time - one_hour_us - 1, // Just over 1 hour ago
        1,
        current_time,
        PrimitiveType::Kv
    ));
}

#[test]
fn composite_policy_uses_per_type_overrides() {
    let policy = RetentionPolicy::composite(RetentionPolicy::KeepAll)
        .with_override(PrimitiveType::Event, RetentionPolicy::KeepLast(5))
        .with_override(PrimitiveType::State, RetentionPolicy::KeepLast(1))
        .build();

    // KV uses default (KeepAll)
    assert!(policy.should_retain(1, 0, 100, 0, PrimitiveType::Kv));

    // Event uses KeepLast(5)
    assert!(policy.should_retain(5, 0, 5, 0, PrimitiveType::Event));
    assert!(!policy.should_retain(1, 0, 10, 0, PrimitiveType::Event)); // Outside last 5

    // State uses KeepLast(1)
    assert!(policy.should_retain(1, 0, 1, 0, PrimitiveType::State)); // Only version
    assert!(!policy.should_retain(1, 0, 2, 0, PrimitiveType::State)); // Not latest
}

#[test]
fn composite_falls_back_to_default() {
    let policy = RetentionPolicy::composite(RetentionPolicy::KeepLast(10))
        .with_override(PrimitiveType::Kv, RetentionPolicy::KeepAll)
        .build();

    // KV has override
    assert!(policy.should_retain(1, 0, 100, 0, PrimitiveType::Kv));

    // Event falls back to default (KeepLast(10))
    assert!(policy.should_retain(10, 0, 10, 0, PrimitiveType::Event));
    assert!(!policy.should_retain(1, 0, 20, 0, PrimitiveType::Event)); // Outside last 10
}

// ============================================================================
// Safety Invariants
// ============================================================================

#[test]
fn gc_never_removes_latest_version() {
    // Even aggressive policies keep at least 1 version
    let policy = RetentionPolicy::KeepLast(1);

    // Single version always retained (version_count = 1)
    assert!(policy.should_retain(1, 0, 1, 0, PrimitiveType::Kv));

    // Latest of multiple versions always retained (version_count = 1 for newest)
    assert!(policy.should_retain(100, 0, 1, 0, PrimitiveType::Kv));
}

#[test]
#[should_panic]
fn zero_keep_last_panics() {
    // Factory method panics on zero
    let _ = RetentionPolicy::keep_last(0);
}

#[test]
#[should_panic]
fn zero_keep_for_panics() {
    // Factory method panics on zero duration
    let _ = RetentionPolicy::keep_for(Duration::from_secs(0));
}

// ============================================================================
// Serialization
// ============================================================================

#[test]
fn keep_all_serialization_roundtrip() {
    let policy = RetentionPolicy::KeepAll;
    let bytes = policy.to_bytes();
    let parsed = RetentionPolicy::from_bytes(&bytes).unwrap();
    assert!(matches!(parsed, RetentionPolicy::KeepAll));
}

#[test]
fn keep_last_serialization_roundtrip() {
    let policy = RetentionPolicy::KeepLast(42);
    let bytes = policy.to_bytes();
    let parsed = RetentionPolicy::from_bytes(&bytes).unwrap();
    match parsed {
        RetentionPolicy::KeepLast(n) => assert_eq!(n, 42),
        _ => panic!("Expected KeepLast"),
    }
}

#[test]
fn keep_for_serialization_roundtrip() {
    let policy = RetentionPolicy::KeepFor(Duration::from_secs(3600));
    let bytes = policy.to_bytes();
    let parsed = RetentionPolicy::from_bytes(&bytes).unwrap();
    match parsed {
        RetentionPolicy::KeepFor(d) => assert_eq!(d, Duration::from_secs(3600)),
        _ => panic!("Expected KeepFor"),
    }
}

#[test]
fn composite_serialization_roundtrip() {
    let policy = RetentionPolicy::composite(RetentionPolicy::KeepAll)
        .with_override(PrimitiveType::Event, RetentionPolicy::KeepLast(100))
        .build();

    let bytes = policy.to_bytes();
    let parsed = RetentionPolicy::from_bytes(&bytes).unwrap();

    // Verify structure
    match parsed {
        RetentionPolicy::Composite { default, overrides } => {
            assert!(matches!(*default, RetentionPolicy::KeepAll));
            assert!(overrides.contains_key(&PrimitiveType::Event));
        }
        _ => panic!("Expected Composite"),
    }
}

#[test]
fn deserialization_rejects_empty() {
    let result = RetentionPolicy::from_bytes(&[]);
    assert!(result.is_err());
}

#[test]
fn deserialization_rejects_invalid_tag() {
    let result = RetentionPolicy::from_bytes(&[0xFF]);
    assert!(result.is_err());
}

#[test]
fn deserialization_rejects_truncated_keep_last() {
    // Tag for KeepLast but not enough bytes
    let result = RetentionPolicy::from_bytes(&[0x02, 0x00, 0x00]);
    assert!(result.is_err());
}

#[test]
fn deserialization_rejects_zero_keep_last() {
    // KeepLast(0) is invalid
    let bytes = [0x02, 0, 0, 0, 0, 0, 0, 0, 0]; // Tag + 8 bytes of 0
    let result = RetentionPolicy::from_bytes(&bytes);
    assert!(result.is_err());
}

#[test]
fn deserialization_rejects_zero_keep_for() {
    // KeepFor(0) is invalid
    let bytes = [0x03, 0, 0, 0, 0, 0, 0, 0, 0]; // Tag + 8 bytes of 0
    let result = RetentionPolicy::from_bytes(&bytes);
    assert!(result.is_err());
}

// ============================================================================
// Policy Summary
// ============================================================================

#[test]
fn policy_summary_is_descriptive() {
    let keep_all = RetentionPolicy::KeepAll;
    assert!(keep_all.summary().contains("KeepAll"));

    let keep_last = RetentionPolicy::KeepLast(10);
    assert!(keep_last.summary().contains("10"));

    let keep_for = RetentionPolicy::KeepFor(Duration::from_secs(3600));
    // Duration is formatted as Debug, so it's "3600s" not "3600"
    let summary = keep_for.summary();
    assert!(
        summary.contains("KeepFor"),
        "Expected 'KeepFor' in summary: {}",
        summary
    );
}
