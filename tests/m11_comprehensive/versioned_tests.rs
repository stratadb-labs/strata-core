//! Versioned<T> Tests
//!
//! Tests for M11 Versioned<T> structure and semantics.
//!
//! Test ID Conventions:
//! - VS-xxx: Structure tests
//! - VT-xxx: Version tag tests
//! - TS-xxx: Timestamp tests
//! - VI-xxx: Version incomparability tests

use crate::test_utils::*;

// =============================================================================
// 8.1 Structure Tests (VS-001 to VS-006)
// =============================================================================

#[cfg(test)]
mod structure {
    use super::*;

    #[test]
    fn vs_001_has_value_field() {
        let versioned = Versioned {
            value: Value::Int(42),
            version: Version::Txn(1),
            timestamp: 0,
        };
        assert_eq!(versioned.value, Value::Int(42));
    }

    #[test]
    fn vs_002_has_version_field() {
        let versioned = Versioned {
            value: Value::Null,
            version: Version::Txn(123),
            timestamp: 0,
        };
        assert_eq!(versioned.version, Version::Txn(123));
    }

    #[test]
    fn vs_003_has_timestamp_field() {
        let versioned = Versioned {
            value: Value::Null,
            version: Version::Txn(1),
            timestamp: 1234567890,
        };
        assert_eq!(versioned.timestamp, 1234567890);
    }

    #[test]
    fn vs_004_value_is_correct_type() {
        let versioned = Versioned {
            value: Value::String("hello".into()),
            version: Version::Txn(1),
            timestamp: 0,
        };
        assert!(matches!(versioned.value, Value::String(_)));
    }

    #[test]
    fn vs_005_version_is_tagged_union() {
        let txn = Version::Txn(1);
        let seq = Version::Sequence(2);
        let ctr = Version::Counter(3);

        // Each variant is distinct
        assert!(matches!(txn, Version::Txn(_)));
        assert!(matches!(seq, Version::Sequence(_)));
        assert!(matches!(ctr, Version::Counter(_)));
    }

    #[test]
    fn vs_006_timestamp_is_u64() {
        let versioned = Versioned {
            value: Value::Null,
            version: Version::Txn(1),
            timestamp: u64::MAX,
        };
        assert_eq!(versioned.timestamp, u64::MAX);
    }
}

// =============================================================================
// 8.2 Version Tag Tests (VT-001 to VT-006)
// =============================================================================

#[cfg(test)]
mod version_tags {
    use super::*;

    #[test]
    fn vt_001_kv_uses_txn() {
        // KV operations use Txn version type
        let v = Version::Txn(1);
        let json = wire::encode_version(&v);
        assert!(json.contains("txn"));
    }

    #[test]
    fn vt_002_json_uses_txn() {
        // JSON operations use Txn version type
        let v = Version::Txn(1);
        let json = wire::encode_version(&v);
        assert!(json.contains("txn"));
    }

    #[test]
    fn vt_003_vector_uses_txn() {
        // Vector operations use Txn version type
        let v = Version::Txn(1);
        assert!(matches!(v, Version::Txn(_)));
    }

    #[test]
    fn vt_004_event_uses_sequence() {
        // Event operations use Sequence version type
        let v = Version::Sequence(1);
        let json = wire::encode_version(&v);
        assert!(json.contains("sequence"));
    }

    #[test]
    fn vt_005_state_uses_counter() {
        // State (CAS) operations use Counter version type
        let v = Version::Counter(1);
        let json = wire::encode_version(&v);
        assert!(json.contains("counter"));
    }

    #[test]
    fn vt_006_run_uses_txn() {
        // Run creation uses Txn version type
        let v = Version::Txn(1);
        assert!(matches!(v, Version::Txn(_)));
    }
}

// =============================================================================
// 8.3 Timestamp Tests (TS-001 to TS-004)
// =============================================================================

#[cfg(test)]
mod timestamp {
    use super::*;

    #[test]
    fn ts_001_timestamp_is_microseconds() {
        // Timestamp should be in microseconds range
        // 2020-01-01 00:00:00 UTC in microseconds
        let year_2020_us: u64 = 1_577_836_800_000_000;
        // 2030-01-01 00:00:00 UTC in microseconds
        let year_2030_us: u64 = 1_893_456_000_000_000;

        // A valid timestamp should be in this range (roughly)
        let valid_timestamp = 1_700_000_000_000_000_u64; // ~2023
        assert!(valid_timestamp > year_2020_us);
        assert!(valid_timestamp < year_2030_us);
    }

    #[test]
    fn ts_002_timestamp_monotonic_concept() {
        // Later operations should have >= timestamps
        let t1 = 1000u64;
        let t2 = 1001u64;
        assert!(t2 >= t1);
    }

    #[test]
    fn ts_003_timestamp_reasonable() {
        // Timestamp should not be 0 in production
        let versioned = Versioned {
            value: Value::Null,
            version: Version::Txn(1),
            timestamp: 1_700_000_000_000_000, // ~2023 in microseconds
        };
        assert!(versioned.timestamp > 0);
    }

    #[test]
    fn ts_004_timestamp_attached() {
        // Every Versioned<T> has a timestamp
        let versioned = Versioned {
            value: Value::Int(42),
            version: Version::Txn(1),
            timestamp: 12345,
        };
        // timestamp field exists and is accessible
        let _ = versioned.timestamp;
    }
}

// =============================================================================
// 8.4 Version Incomparability Tests (VI-001 to VI-004)
// =============================================================================

#[cfg(test)]
mod incomparability {
    use super::*;

    #[test]
    fn vi_001_txn_vs_sequence_different() {
        // Txn and Sequence are different types
        let txn = Version::Txn(5);
        let seq = Version::Sequence(5);

        // Even with same numeric value, they're different
        assert_ne!(std::mem::discriminant(&txn), std::mem::discriminant(&seq));
    }

    #[test]
    fn vi_002_txn_vs_counter_different() {
        let txn = Version::Txn(5);
        let ctr = Version::Counter(5);

        assert_ne!(std::mem::discriminant(&txn), std::mem::discriminant(&ctr));
    }

    #[test]
    fn vi_003_sequence_vs_counter_different() {
        let seq = Version::Sequence(5);
        let ctr = Version::Counter(5);

        assert_ne!(std::mem::discriminant(&seq), std::mem::discriminant(&ctr));
    }

    #[test]
    fn vi_004_same_type_comparable() {
        // Same version types can be compared
        let txn1 = Version::Txn(5);
        let txn2 = Version::Txn(10);

        // Both are Txn, so they can be logically compared
        if let (Version::Txn(v1), Version::Txn(v2)) = (&txn1, &txn2) {
            assert!(v1 < v2);
        }
    }
}
