//! Microsecond-precision timestamp type
//!
//! This type expresses the temporal aspect of Invariant 2: Everything is Versioned.
//! Every version has a timestamp recording when it was created.
//!
//! ## Precision
//!
//! Timestamps are stored as microseconds since Unix epoch (1970-01-01 00:00:00 UTC).
//! This provides:
//! - Sufficient precision for ordering concurrent operations
//! - 584,554 years of range (u64::MAX microseconds)
//! - Compatibility with common time libraries
//!
//! ## Usage
//!
//! Never expose raw arithmetic. Use explicit constructors:
//!
//! ```
//! use strata_core::Timestamp;
//!
//! let now = Timestamp::now();
//! let from_secs = Timestamp::from_secs(1000);
//! let from_micros = Timestamp::from_micros(1_000_000_000);
//! ```

use serde::{Deserialize, Serialize};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Microsecond-precision timestamp
///
/// Represents a point in time as microseconds since Unix epoch.
/// This is the canonical time representation in the database.
///
/// ## Invariants
///
/// - Timestamps are always non-negative (u64)
/// - Timestamps are always in microseconds
/// - Timestamps are comparable and orderable
/// - The zero timestamp represents Unix epoch
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Timestamp(u64);

impl Timestamp {
    /// Unix epoch (1970-01-01 00:00:00 UTC)
    pub const EPOCH: Timestamp = Timestamp(0);

    /// Maximum representable timestamp
    pub const MAX: Timestamp = Timestamp(u64::MAX);

    // =========================================================================
    // Constructors
    // =========================================================================

    /// Create a timestamp for the current moment
    ///
    /// Uses system time. Returns epoch (0) if system clock is before Unix epoch
    /// (e.g., clock went backwards due to NTP adjustment).
    pub fn now() -> Self {
        let duration = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default();
        Timestamp(duration.as_micros() as u64)
    }

    /// Create a timestamp from microseconds since epoch
    #[inline]
    pub const fn from_micros(micros: u64) -> Self {
        Timestamp(micros)
    }

    /// Create a timestamp from milliseconds since epoch
    #[inline]
    pub const fn from_millis(millis: u64) -> Self {
        Timestamp(millis.saturating_mul(1_000))
    }

    /// Create a timestamp from seconds since epoch
    #[inline]
    pub const fn from_secs(secs: u64) -> Self {
        Timestamp(secs.saturating_mul(1_000_000))
    }

    // =========================================================================
    // Accessors
    // =========================================================================

    /// Get microseconds since Unix epoch
    #[inline]
    pub const fn as_micros(&self) -> u64 {
        self.0
    }

    /// Get milliseconds since Unix epoch (truncates)
    #[inline]
    pub const fn as_millis(&self) -> u64 {
        self.0 / 1_000
    }

    /// Get seconds since Unix epoch (truncates)
    #[inline]
    pub const fn as_secs(&self) -> u64 {
        self.0 / 1_000_000
    }

    // =========================================================================
    // Duration Operations
    // =========================================================================

    /// Compute duration since an earlier timestamp
    ///
    /// Returns `None` if `earlier` is actually later than `self`.
    pub fn duration_since(&self, earlier: Timestamp) -> Option<Duration> {
        if self.0 >= earlier.0 {
            Some(Duration::from_micros(self.0 - earlier.0))
        } else {
            None
        }
    }

    /// Add a duration to this timestamp
    ///
    /// Saturates at `Timestamp::MAX` on overflow.
    pub fn saturating_add(&self, duration: Duration) -> Self {
        Timestamp(self.0.saturating_add(duration.as_micros() as u64))
    }

    /// Subtract a duration from this timestamp
    ///
    /// Saturates at `Timestamp::EPOCH` on underflow.
    pub fn saturating_sub(&self, duration: Duration) -> Self {
        Timestamp(self.0.saturating_sub(duration.as_micros() as u64))
    }

    /// Check if this timestamp is before another
    #[inline]
    pub fn is_before(&self, other: Timestamp) -> bool {
        self.0 < other.0
    }

    /// Check if this timestamp is after another
    #[inline]
    pub fn is_after(&self, other: Timestamp) -> bool {
        self.0 > other.0
    }
}

impl Default for Timestamp {
    fn default() -> Self {
        Timestamp::EPOCH
    }
}

impl std::fmt::Display for Timestamp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Format as "seconds.microseconds" for readability
        let secs = self.0 / 1_000_000;
        let micros = self.0 % 1_000_000;
        write!(f, "{}.{:06}", secs, micros)
    }
}

// ============================================================================
// From Implementations
// ============================================================================

impl From<u64> for Timestamp {
    /// Create from raw microseconds
    fn from(micros: u64) -> Self {
        Timestamp::from_micros(micros)
    }
}

impl From<Timestamp> for u64 {
    /// Extract raw microseconds
    fn from(ts: Timestamp) -> Self {
        ts.0
    }
}

impl From<Duration> for Timestamp {
    /// Create from duration since epoch
    fn from(duration: Duration) -> Self {
        Timestamp::from_micros(duration.as_micros() as u64)
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_timestamp_epoch() {
        assert_eq!(Timestamp::EPOCH.as_micros(), 0);
        assert_eq!(Timestamp::EPOCH.as_millis(), 0);
        assert_eq!(Timestamp::EPOCH.as_secs(), 0);
    }

    #[test]
    fn test_timestamp_from_secs() {
        let ts = Timestamp::from_secs(1000);
        assert_eq!(ts.as_secs(), 1000);
        assert_eq!(ts.as_millis(), 1_000_000);
        assert_eq!(ts.as_micros(), 1_000_000_000);
    }

    #[test]
    fn test_timestamp_from_millis() {
        let ts = Timestamp::from_millis(5000);
        assert_eq!(ts.as_millis(), 5000);
        assert_eq!(ts.as_micros(), 5_000_000);
        assert_eq!(ts.as_secs(), 5);
    }

    #[test]
    fn test_timestamp_from_micros() {
        let ts = Timestamp::from_micros(1_234_567);
        assert_eq!(ts.as_micros(), 1_234_567);
        assert_eq!(ts.as_millis(), 1_234);
        assert_eq!(ts.as_secs(), 1);
    }

    #[test]
    fn test_timestamp_now() {
        let before = Timestamp::now();
        std::thread::sleep(Duration::from_millis(1));
        let after = Timestamp::now();

        assert!(after > before, "Time should advance");
        assert!(after.as_micros() > before.as_micros());
    }

    #[test]
    fn test_timestamp_ordering() {
        let t1 = Timestamp::from_micros(100);
        let t2 = Timestamp::from_micros(200);
        let t3 = Timestamp::from_micros(100);

        assert!(t1 < t2);
        assert!(t2 > t1);
        assert_eq!(t1, t3);
        assert!(t1.is_before(t2));
        assert!(t2.is_after(t1));
    }

    #[test]
    fn test_timestamp_duration_since() {
        let t1 = Timestamp::from_micros(1000);
        let t2 = Timestamp::from_micros(3000);

        let duration = t2.duration_since(t1).unwrap();
        assert_eq!(duration.as_micros(), 2000);

        // Earlier timestamp returns None
        assert!(t1.duration_since(t2).is_none());
    }

    #[test]
    fn test_timestamp_saturating_add() {
        let ts = Timestamp::from_micros(1000);
        let added = ts.saturating_add(Duration::from_micros(500));
        assert_eq!(added.as_micros(), 1500);

        // Saturation at MAX
        let max_added = Timestamp::MAX.saturating_add(Duration::from_micros(1));
        assert_eq!(max_added, Timestamp::MAX);
    }

    #[test]
    fn test_timestamp_saturating_sub() {
        let ts = Timestamp::from_micros(1000);
        let subtracted = ts.saturating_sub(Duration::from_micros(500));
        assert_eq!(subtracted.as_micros(), 500);

        // Saturation at EPOCH
        let epoch_sub = Timestamp::EPOCH.saturating_sub(Duration::from_micros(1));
        assert_eq!(epoch_sub, Timestamp::EPOCH);
    }

    #[test]
    fn test_timestamp_display() {
        let ts = Timestamp::from_micros(1_234_567_890);
        let display = format!("{}", ts);
        assert_eq!(display, "1234.567890");

        let epoch = format!("{}", Timestamp::EPOCH);
        assert_eq!(epoch, "0.000000");
    }

    #[test]
    fn test_timestamp_from_u64() {
        let ts: Timestamp = 12345u64.into();
        assert_eq!(ts.as_micros(), 12345);
    }

    #[test]
    fn test_timestamp_into_u64() {
        let ts = Timestamp::from_micros(12345);
        let micros: u64 = ts.into();
        assert_eq!(micros, 12345);
    }

    #[test]
    fn test_timestamp_from_duration() {
        let duration = Duration::from_micros(5000);
        let ts: Timestamp = duration.into();
        assert_eq!(ts.as_micros(), 5000);
    }

    #[test]
    fn test_timestamp_hash() {
        use std::collections::HashSet;

        let t1 = Timestamp::from_micros(100);
        let t2 = Timestamp::from_micros(100);
        let t3 = Timestamp::from_micros(200);

        let mut set = HashSet::new();
        set.insert(t1);

        assert!(set.contains(&t2));
        assert!(!set.contains(&t3));
    }

    #[test]
    fn test_timestamp_serialization() {
        let ts = Timestamp::from_micros(1_234_567);
        let json = serde_json::to_string(&ts).unwrap();
        let restored: Timestamp = serde_json::from_str(&json).unwrap();
        assert_eq!(ts, restored);
    }

    #[test]
    fn test_timestamp_default() {
        let ts = Timestamp::default();
        assert_eq!(ts, Timestamp::EPOCH);
    }
}
