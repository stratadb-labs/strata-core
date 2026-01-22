//! Version identifier types
//!
//! This type expresses Invariant 2: Everything is Versioned.
//! Every mutation produces a version. Every read returns version information.
//!
//! ## Version Variants
//!
//! Different primitives use different versioning schemes:
//!
//! - **TxnId**: Transaction-based versioning (KV, Json, Vector, Run, Trace)
//!   Multiple entities modified in the same transaction share this version.
//!
//! - **Sequence**: Position-based versioning (EventLog)
//!   Represents position in an append-only log. Unique within a run's event log.
//!
//! - **Counter**: Per-entity counter (StateCell)
//!   Increments on each modification. Used for CAS operations.
//!
//! ## Comparison
//!
//! Versions are comparable **within the same variant type**.
//! Cross-variant comparison is undefined (returns `None` for `partial_cmp`).

use serde::{Deserialize, Serialize};

/// Version identifier for an entity
///
/// Versions track mutations and enable optimistic concurrency control.
/// Every write operation returns a Version indicating what was created.
///
/// ## Invariants
///
/// - Versions are monotonically increasing within an entity
/// - Versions within the same variant are totally ordered
/// - Cross-variant comparison is not meaningful
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Version {
    /// Transaction-based version
    ///
    /// Used by: KV, Json, Vector, Run, Trace
    ///
    /// Represents a global transaction ID. Multiple entities modified
    /// in the same transaction share this version.
    TxnId(u64),

    /// Sequence-based version
    ///
    /// Used by: EventLog
    ///
    /// Represents position in an append-only log.
    /// Unique within a run's event log.
    Sequence(u64),

    /// Counter-based version
    ///
    /// Used by: StateCell
    ///
    /// Per-entity mutation counter. Increments on each modification.
    /// Used for compare-and-swap (CAS) operations.
    Counter(u64),
}

impl Version {
    // =========================================================================
    // Constructors
    // =========================================================================

    /// Create a TxnId version with value 0
    pub const fn zero_txn() -> Self {
        Version::TxnId(0)
    }

    /// Create a Sequence version with value 0
    pub const fn zero_sequence() -> Self {
        Version::Sequence(0)
    }

    /// Create a Counter version with value 0
    pub const fn zero_counter() -> Self {
        Version::Counter(0)
    }

    /// Create a TxnId version
    pub const fn txn(id: u64) -> Self {
        Version::TxnId(id)
    }

    /// Create a Sequence version
    pub const fn seq(n: u64) -> Self {
        Version::Sequence(n)
    }

    /// Create a Counter version
    pub const fn counter(n: u64) -> Self {
        Version::Counter(n)
    }

    // =========================================================================
    // Accessors
    // =========================================================================

    /// Get the numeric value
    ///
    /// Useful for storage and display, but NOT for cross-variant comparison.
    #[inline]
    pub const fn as_u64(&self) -> u64 {
        match self {
            Version::TxnId(v) => *v,
            Version::Sequence(v) => *v,
            Version::Counter(v) => *v,
        }
    }

    /// Check if this is a transaction-based version
    #[inline]
    pub const fn is_txn_id(&self) -> bool {
        matches!(self, Version::TxnId(_))
    }

    /// Check if this is a sequence-based version
    #[inline]
    pub const fn is_sequence(&self) -> bool {
        matches!(self, Version::Sequence(_))
    }

    /// Check if this is a counter-based version
    #[inline]
    pub const fn is_counter(&self) -> bool {
        matches!(self, Version::Counter(_))
    }

    // =========================================================================
    // Operations
    // =========================================================================

    /// Increment the version, returning a new version
    ///
    /// Preserves the variant type.
    pub const fn increment(&self) -> Self {
        match self {
            Version::TxnId(v) => Version::TxnId(*v + 1),
            Version::Sequence(v) => Version::Sequence(*v + 1),
            Version::Counter(v) => Version::Counter(*v + 1),
        }
    }

    /// Saturating increment (won't overflow)
    pub const fn saturating_increment(&self) -> Self {
        match self {
            Version::TxnId(v) => Version::TxnId(v.saturating_add(1)),
            Version::Sequence(v) => Version::Sequence(v.saturating_add(1)),
            Version::Counter(v) => Version::Counter(v.saturating_add(1)),
        }
    }

    /// Check if this version is zero
    #[inline]
    pub const fn is_zero(&self) -> bool {
        self.as_u64() == 0
    }
}

impl PartialOrd for Version {
    /// Compare versions
    ///
    /// Delegates to `Ord::cmp` for consistency.
    /// For semantic same-variant comparison, use dedicated methods.
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Version {
    /// Total ordering for sorting purposes
    ///
    /// WARNING: This provides a total ordering by comparing numeric values
    /// regardless of variant. Use `partial_cmp` for semantic comparison.
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        // First compare by variant discriminant, then by value
        let self_discriminant = match self {
            Version::TxnId(_) => 0,
            Version::Sequence(_) => 1,
            Version::Counter(_) => 2,
        };
        let other_discriminant = match other {
            Version::TxnId(_) => 0,
            Version::Sequence(_) => 1,
            Version::Counter(_) => 2,
        };

        self_discriminant
            .cmp(&other_discriminant)
            .then_with(|| self.as_u64().cmp(&other.as_u64()))
    }
}

impl std::fmt::Display for Version {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Version::TxnId(v) => write!(f, "txn:{}", v),
            Version::Sequence(v) => write!(f, "seq:{}", v),
            Version::Counter(v) => write!(f, "cnt:{}", v),
        }
    }
}

impl Default for Version {
    /// Default is TxnId(0)
    fn default() -> Self {
        Version::zero_txn()
    }
}

impl From<u64> for Version {
    /// Create a TxnId version from u64
    fn from(v: u64) -> Self {
        Version::TxnId(v)
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_version_constructors() {
        assert_eq!(Version::zero_txn(), Version::TxnId(0));
        assert_eq!(Version::zero_sequence(), Version::Sequence(0));
        assert_eq!(Version::zero_counter(), Version::Counter(0));

        assert_eq!(Version::txn(42), Version::TxnId(42));
        assert_eq!(Version::seq(100), Version::Sequence(100));
        assert_eq!(Version::counter(5), Version::Counter(5));
    }

    #[test]
    fn test_version_as_u64() {
        assert_eq!(Version::TxnId(42).as_u64(), 42);
        assert_eq!(Version::Sequence(100).as_u64(), 100);
        assert_eq!(Version::Counter(5).as_u64(), 5);
    }

    #[test]
    fn test_version_type_checks() {
        let txn = Version::TxnId(1);
        let seq = Version::Sequence(1);
        let cnt = Version::Counter(1);

        assert!(txn.is_txn_id());
        assert!(!txn.is_sequence());
        assert!(!txn.is_counter());

        assert!(!seq.is_txn_id());
        assert!(seq.is_sequence());
        assert!(!seq.is_counter());

        assert!(!cnt.is_txn_id());
        assert!(!cnt.is_sequence());
        assert!(cnt.is_counter());
    }

    #[test]
    fn test_version_increment() {
        assert_eq!(Version::TxnId(1).increment(), Version::TxnId(2));
        assert_eq!(Version::Sequence(10).increment(), Version::Sequence(11));
        assert_eq!(Version::Counter(5).increment(), Version::Counter(6));
    }

    #[test]
    fn test_version_saturating_increment() {
        assert_eq!(
            Version::TxnId(u64::MAX).saturating_increment(),
            Version::TxnId(u64::MAX)
        );
    }

    #[test]
    fn test_version_is_zero() {
        assert!(Version::TxnId(0).is_zero());
        assert!(Version::Sequence(0).is_zero());
        assert!(Version::Counter(0).is_zero());

        assert!(!Version::TxnId(1).is_zero());
        assert!(!Version::Sequence(1).is_zero());
        assert!(!Version::Counter(1).is_zero());
    }

    #[test]
    fn test_version_partial_ord_same_type() {
        assert!(Version::TxnId(1) < Version::TxnId(2));
        assert!(Version::TxnId(2) > Version::TxnId(1));
        assert!(Version::TxnId(1) == Version::TxnId(1));

        assert!(Version::Sequence(10) < Version::Sequence(20));
        assert!(Version::Counter(3) == Version::Counter(3));
    }

    #[test]
    fn test_version_partial_ord_different_types() {
        use std::cmp::Ordering;

        // Cross-variant comparison delegates to Ord for consistency
        // TxnId < Sequence < Counter
        assert_eq!(
            Version::TxnId(1).partial_cmp(&Version::Sequence(1)),
            Some(Ordering::Less)
        );
        assert_eq!(
            Version::Sequence(1).partial_cmp(&Version::Counter(1)),
            Some(Ordering::Less)
        );
        assert_eq!(
            Version::TxnId(1).partial_cmp(&Version::Counter(1)),
            Some(Ordering::Less)
        );
    }

    #[test]
    fn test_version_cmp_directly() {
        use std::cmp::Ordering;

        // TxnId < Sequence < Counter (by discriminant)
        assert_eq!(Version::TxnId(5).cmp(&Version::Sequence(5)), Ordering::Less);
        assert_eq!(
            Version::TxnId(5).cmp(&Version::Counter(5)),
            Ordering::Less,
            "TxnId should be less than Counter"
        );
        assert_eq!(
            Version::Sequence(5).cmp(&Version::Counter(5)),
            Ordering::Less
        );

        // Same variant, compare by value
        assert_eq!(Version::TxnId(5).cmp(&Version::TxnId(10)), Ordering::Less);
        assert_eq!(
            Version::TxnId(10).cmp(&Version::TxnId(5)),
            Ordering::Greater
        );
        assert_eq!(Version::TxnId(5).cmp(&Version::TxnId(5)), Ordering::Equal);
    }

    #[test]
    fn test_version_ord_total_ordering() {
        // Total ordering groups by variant first, then by value
        let mut versions = vec![
            Version::Counter(5),
            Version::TxnId(10),
            Version::Sequence(1),
            Version::TxnId(5),
            Version::Sequence(10),
            Version::Counter(1),
        ];
        versions.sort();

        // Expected order: TxnId(5), TxnId(10), Sequence(1), Sequence(10), Counter(1), Counter(5)
        assert_eq!(versions[0], Version::TxnId(5), "First should be TxnId(5)");
        assert_eq!(versions[1], Version::TxnId(10), "Second should be TxnId(10)");
        assert_eq!(
            versions[2],
            Version::Sequence(1),
            "Third should be Sequence(1)"
        );
        assert_eq!(
            versions[3],
            Version::Sequence(10),
            "Fourth should be Sequence(10)"
        );
        assert_eq!(
            versions[4],
            Version::Counter(1),
            "Fifth should be Counter(1)"
        );
        assert_eq!(
            versions[5],
            Version::Counter(5),
            "Sixth should be Counter(5)"
        );
    }

    #[test]
    fn test_version_display() {
        assert_eq!(format!("{}", Version::TxnId(42)), "txn:42");
        assert_eq!(format!("{}", Version::Sequence(100)), "seq:100");
        assert_eq!(format!("{}", Version::Counter(5)), "cnt:5");
    }

    #[test]
    fn test_version_default() {
        assert_eq!(Version::default(), Version::TxnId(0));
    }

    #[test]
    fn test_version_from_u64() {
        let v: Version = 42u64.into();
        assert_eq!(v, Version::TxnId(42));
    }

    #[test]
    fn test_version_hash() {
        use std::collections::HashSet;

        let mut set = HashSet::new();
        set.insert(Version::TxnId(1));
        set.insert(Version::TxnId(2));
        set.insert(Version::TxnId(1)); // Duplicate

        assert_eq!(set.len(), 2);
        assert!(set.contains(&Version::TxnId(1)));
        assert!(set.contains(&Version::TxnId(2)));
    }

    #[test]
    fn test_version_serialization() {
        let versions = vec![
            Version::TxnId(42),
            Version::Sequence(100),
            Version::Counter(5),
        ];

        for v in versions {
            let json = serde_json::to_string(&v).unwrap();
            let restored: Version = serde_json::from_str(&json).unwrap();
            assert_eq!(v, restored);
        }
    }

    #[test]
    fn test_version_equality() {
        // Same type, same value
        assert_eq!(Version::TxnId(1), Version::TxnId(1));

        // Same type, different value
        assert_ne!(Version::TxnId(1), Version::TxnId(2));

        // Different type, same value
        assert_ne!(Version::TxnId(1), Version::Sequence(1));
        assert_ne!(Version::TxnId(1), Version::Counter(1));
        assert_ne!(Version::Sequence(1), Version::Counter(1));
    }
}
