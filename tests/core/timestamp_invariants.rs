//! Timestamp Invariant Tests
//!
//! Tests that Timestamp correctly expresses Invariant 2: Everything is Versioned
//! (temporal component)
//!
//! Timestamps provide microsecond precision temporal tracking for all entities.

use strata_core::Timestamp;
use std::collections::HashSet;
use std::time::Duration;

// ============================================================================
// Timestamp construction
// ============================================================================

#[test]
fn timestamp_from_micros() {
    let ts = Timestamp::from_micros(1_000_000);
    assert_eq!(ts.as_micros(), 1_000_000);
}

#[test]
fn timestamp_from_millis() {
    let ts = Timestamp::from_millis(1000);
    assert_eq!(ts.as_micros(), 1_000_000);
    assert_eq!(ts.as_millis(), 1000);
}

#[test]
fn timestamp_from_secs() {
    let ts = Timestamp::from_secs(1);
    assert_eq!(ts.as_micros(), 1_000_000);
    assert_eq!(ts.as_millis(), 1000);
    assert_eq!(ts.as_secs(), 1);
}

#[test]
fn timestamp_epoch_is_zero() {
    assert_eq!(Timestamp::EPOCH.as_micros(), 0);
    assert_eq!(Timestamp::EPOCH.as_millis(), 0);
    assert_eq!(Timestamp::EPOCH.as_secs(), 0);
}

#[test]
fn timestamp_max_value() {
    assert_eq!(Timestamp::MAX.as_micros(), u64::MAX);
}

// ============================================================================
// Timestamp now() increases over time
// ============================================================================

#[test]
fn timestamp_now_returns_non_zero() {
    let ts = Timestamp::now();
    assert!(ts.as_micros() > 0, "now() should return non-zero timestamp");
}

#[test]
fn timestamp_now_increases_or_same() {
    let ts1 = Timestamp::now();
    std::thread::sleep(Duration::from_millis(1));
    let ts2 = Timestamp::now();

    assert!(ts2 >= ts1, "Timestamps should not go backwards");
}

// ============================================================================
// Timestamps are ordered
// ============================================================================

#[test]
fn timestamp_ordering_consistent() {
    let ts1 = Timestamp::from_micros(100);
    let ts2 = Timestamp::from_micros(200);
    let ts3 = Timestamp::from_micros(200);

    assert!(ts1 < ts2);
    assert!(ts2 > ts1);
    assert_eq!(ts2, ts3);
    assert!(ts1 <= ts2);
    assert!(ts2 >= ts1);
}

#[test]
fn timestamp_is_before_is_after() {
    let ts1 = Timestamp::from_micros(100);
    let ts2 = Timestamp::from_micros(200);

    assert!(ts1.is_before(ts2));
    assert!(!ts1.is_after(ts2));
    assert!(ts2.is_after(ts1));
    assert!(!ts2.is_before(ts1));
}

#[test]
fn timestamp_ordering_transitive() {
    let ts1 = Timestamp::from_micros(100);
    let ts2 = Timestamp::from_micros(200);
    let ts3 = Timestamp::from_micros(300);

    assert!(ts1 < ts2);
    assert!(ts2 < ts3);
    assert!(ts1 < ts3); // Transitivity
}

// ============================================================================
// Microsecond precision
// ============================================================================

#[test]
fn timestamp_preserves_microsecond_precision() {
    // Test that microsecond values are preserved exactly
    let values = [0, 1, 999, 1_000, 1_000_000, 1_234_567_890_123_456];

    for &micros in &values {
        let ts = Timestamp::from_micros(micros);
        assert_eq!(ts.as_micros(), micros, "Microsecond precision lost for {}", micros);
    }
}

#[test]
fn timestamp_millis_truncates_correctly() {
    let ts = Timestamp::from_micros(1_234_567);
    assert_eq!(ts.as_millis(), 1234); // Truncates, doesn't round
}

#[test]
fn timestamp_secs_truncates_correctly() {
    let ts = Timestamp::from_micros(1_999_999);
    assert_eq!(ts.as_secs(), 1); // Truncates, doesn't round
}

// ============================================================================
// Duration operations
// ============================================================================

#[test]
fn timestamp_duration_since_correct() {
    let ts1 = Timestamp::from_micros(1_000_000);
    let ts2 = Timestamp::from_micros(2_500_000);

    let duration = ts2.duration_since(ts1).expect("ts2 > ts1");
    assert_eq!(duration.as_micros(), 1_500_000);
}

#[test]
fn timestamp_duration_since_same_timestamp() {
    let ts = Timestamp::from_micros(1_000_000);
    let duration = ts.duration_since(ts).expect("same timestamp");
    assert_eq!(duration.as_micros(), 0);
}

#[test]
fn timestamp_saturating_add() {
    let ts = Timestamp::from_micros(1_000_000);
    let duration = Duration::from_secs(1);
    let result = ts.saturating_add(duration);

    assert_eq!(result.as_micros(), 2_000_000);
}

#[test]
fn timestamp_saturating_add_overflow() {
    let ts = Timestamp::MAX;
    let duration = Duration::from_secs(1);
    let result = ts.saturating_add(duration);

    assert_eq!(result, Timestamp::MAX); // Should saturate
}

#[test]
fn timestamp_saturating_sub() {
    let ts = Timestamp::from_micros(2_000_000);
    let duration = Duration::from_secs(1);
    let result = ts.saturating_sub(duration);

    assert_eq!(result.as_micros(), 1_000_000);
}

#[test]
fn timestamp_saturating_sub_underflow() {
    let ts = Timestamp::from_micros(500_000);
    let duration = Duration::from_secs(1);
    let result = ts.saturating_sub(duration);

    assert_eq!(result, Timestamp::EPOCH); // Should saturate to zero
}

// ============================================================================
// Timestamp is hashable
// ============================================================================

#[test]
fn timestamp_hashable() {
    let mut set = HashSet::new();

    set.insert(Timestamp::from_micros(100));
    set.insert(Timestamp::from_micros(200));
    set.insert(Timestamp::from_micros(100)); // Duplicate

    assert_eq!(set.len(), 2);
    assert!(set.contains(&Timestamp::from_micros(100)));
    assert!(set.contains(&Timestamp::from_micros(200)));
}

// ============================================================================
// Timestamp conversions
// ============================================================================

#[test]
fn timestamp_from_u64() {
    let ts: Timestamp = 1_000_000u64.into();
    assert_eq!(ts.as_micros(), 1_000_000);
}

#[test]
fn timestamp_into_u64() {
    let ts = Timestamp::from_micros(1_000_000);
    let micros: u64 = ts.into();
    assert_eq!(micros, 1_000_000);
}

#[test]
fn timestamp_from_duration() {
    let duration = Duration::from_secs(5);
    let ts: Timestamp = duration.into();
    assert_eq!(ts.as_secs(), 5);
}

// ============================================================================
// Timestamp serialization
// ============================================================================

#[test]
fn timestamp_serialization_roundtrip() {
    let timestamps = vec![
        Timestamp::EPOCH,
        Timestamp::from_micros(1_234_567),
        Timestamp::from_secs(1000),
        Timestamp::now(),
    ];

    for ts in timestamps {
        let json = serde_json::to_string(&ts).expect("serialize");
        let parsed: Timestamp = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(ts, parsed);
    }
}

// ============================================================================
// Timestamp Display
// ============================================================================

#[test]
fn timestamp_display_is_informative() {
    let ts = Timestamp::from_micros(1_234_567_890);
    let display = format!("{}", ts);

    // Display should contain numeric value
    assert!(!display.is_empty());
}

// ============================================================================
// Timestamp Clone and Copy
// ============================================================================

#[test]
fn timestamp_is_copy() {
    let ts1 = Timestamp::from_micros(1000);
    let ts2 = ts1; // Copy
    let ts3 = ts1; // Copy again

    assert_eq!(ts1, ts2);
    assert_eq!(ts2, ts3);
}

// ============================================================================
// Timestamp Default
// ============================================================================

#[test]
fn timestamp_default_is_epoch() {
    let default = Timestamp::default();
    assert_eq!(default, Timestamp::EPOCH);
}

// ============================================================================
// Edge cases
// ============================================================================

#[test]
fn timestamp_handles_large_values() {
    let large = Timestamp::from_micros(u64::MAX - 1);
    assert_eq!(large.as_micros(), u64::MAX - 1);

    let incremented = large.saturating_add(Duration::from_micros(1));
    assert_eq!(incremented.as_micros(), u64::MAX);
}

#[test]
fn timestamp_zero_duration_operations() {
    let ts = Timestamp::from_micros(1000);

    let added = ts.saturating_add(Duration::ZERO);
    assert_eq!(added, ts);

    let subtracted = ts.saturating_sub(Duration::ZERO);
    assert_eq!(subtracted, ts);
}
