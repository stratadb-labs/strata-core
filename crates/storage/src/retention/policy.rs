//! Retention policy types
//!
//! Controls how much version history is retained per run.
//! Policies are stored as database entries and are themselves versioned.
//!
//! # Policy Types
//!
//! - **KeepAll**: Keep all versions forever (default, safest)
//! - **KeepLast(n)**: Keep only the last N versions
//! - **KeepFor(duration)**: Keep versions newer than the specified duration
//! - **Composite**: Different policies for different primitive types
//!
//! # Example
//!
//! ```ignore
//! use strata_storage::retention::{RetentionPolicy, CompositeBuilder};
//! use std::time::Duration;
//!
//! // Keep all versions (default)
//! let policy = RetentionPolicy::keep_all();
//!
//! // Keep last 10 versions
//! let policy = RetentionPolicy::keep_last(10);
//!
//! // Keep versions from last 7 days
//! let policy = RetentionPolicy::keep_for(Duration::from_secs(7 * 24 * 3600));
//! ```

use std::collections::HashMap;
use std::time::Duration;

use strata_core::PrimitiveType;

/// Retention policy for a run
///
/// Controls how much version history is retained.
/// Policies are stored as database entries and are themselves versioned.
#[derive(Debug, Clone, PartialEq, Default)]
pub enum RetentionPolicy {
    /// Keep all versions forever (default)
    ///
    /// Safest policy. No data is ever deleted.
    /// This is the default when no policy is specified.
    #[default]
    KeepAll,

    /// Keep only the last N versions
    ///
    /// Versions beyond the Nth oldest become eligible for removal.
    /// N must be at least 1.
    KeepLast(usize),

    /// Keep versions newer than the specified duration
    ///
    /// Versions older than `now - duration` become eligible for removal.
    /// Duration must be non-zero.
    KeepFor(Duration),

    /// Different policies for different primitive types
    ///
    /// Allows fine-grained control per primitive type.
    /// Unspecified types use the default policy.
    Composite {
        /// Default policy for types without override
        default: Box<RetentionPolicy>,
        /// Per-type policy overrides
        overrides: HashMap<PrimitiveType, Box<RetentionPolicy>>,
    },
}

impl RetentionPolicy {
    /// Create a KeepAll policy (recommended default)
    pub fn keep_all() -> Self {
        RetentionPolicy::KeepAll
    }

    /// Create a KeepLast policy
    ///
    /// # Panics
    ///
    /// Panics if n is 0.
    pub fn keep_last(n: usize) -> Self {
        assert!(n > 0, "KeepLast(n) requires n > 0");
        RetentionPolicy::KeepLast(n)
    }

    /// Create a KeepFor policy
    ///
    /// # Panics
    ///
    /// Panics if duration is zero.
    pub fn keep_for(duration: Duration) -> Self {
        assert!(!duration.is_zero(), "KeepFor requires non-zero duration");
        RetentionPolicy::KeepFor(duration)
    }

    /// Create a Composite policy builder
    pub fn composite(default: RetentionPolicy) -> CompositeBuilder {
        CompositeBuilder {
            default: Box::new(default),
            overrides: HashMap::new(),
        }
    }

    /// Check if a version should be retained
    ///
    /// # Arguments
    ///
    /// * `version` - The version number to check
    /// * `timestamp` - The timestamp of the version (in microseconds)
    /// * `version_count` - Number of versions remaining (including this one)
    /// * `current_time` - Current time (in microseconds)
    /// * `primitive_type` - The type of primitive
    ///
    /// # Returns
    ///
    /// `true` if the version should be retained, `false` if eligible for removal
    pub fn should_retain(
        &self,
        _version: u64,
        timestamp: u64,
        version_count: usize,
        current_time: u64,
        primitive_type: PrimitiveType,
    ) -> bool {
        match self {
            RetentionPolicy::KeepAll => true,

            RetentionPolicy::KeepLast(n) => version_count <= *n,

            RetentionPolicy::KeepFor(duration) => {
                let cutoff = current_time.saturating_sub(duration.as_micros() as u64);
                timestamp >= cutoff
            }

            RetentionPolicy::Composite { default, overrides } => {
                if let Some(override_policy) = overrides.get(&primitive_type) {
                    override_policy.should_retain(
                        _version,
                        timestamp,
                        version_count,
                        current_time,
                        primitive_type,
                    )
                } else {
                    default.should_retain(
                        _version,
                        timestamp,
                        version_count,
                        current_time,
                        primitive_type,
                    )
                }
            }
        }
    }

    /// Get a human-readable summary of the policy
    pub fn summary(&self) -> String {
        match self {
            RetentionPolicy::KeepAll => "KeepAll".to_string(),
            RetentionPolicy::KeepLast(n) => format!("KeepLast({})", n),
            RetentionPolicy::KeepFor(d) => format!("KeepFor({:?})", d),
            RetentionPolicy::Composite { default, overrides } => {
                let override_count = overrides.len();
                format!(
                    "Composite(default={}, overrides={})",
                    default.summary(),
                    override_count
                )
            }
        }
    }

    /// Serialize policy to bytes
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::new();

        match self {
            RetentionPolicy::KeepAll => {
                bytes.push(0x01);
            }
            RetentionPolicy::KeepLast(n) => {
                bytes.push(0x02);
                bytes.extend_from_slice(&(*n as u64).to_le_bytes());
            }
            RetentionPolicy::KeepFor(duration) => {
                bytes.push(0x03);
                // Store as u64 microseconds (fits most practical durations)
                bytes.extend_from_slice(&(duration.as_micros() as u64).to_le_bytes());
            }
            RetentionPolicy::Composite { default, overrides } => {
                bytes.push(0x04);

                // Serialize default policy
                let default_bytes = default.to_bytes();
                bytes.extend_from_slice(&(default_bytes.len() as u32).to_le_bytes());
                bytes.extend_from_slice(&default_bytes);

                // Serialize overrides
                bytes.extend_from_slice(&(overrides.len() as u32).to_le_bytes());
                for (ptype, policy) in overrides {
                    bytes.push(primitive_type_to_byte(*ptype));
                    let policy_bytes = policy.to_bytes();
                    bytes.extend_from_slice(&(policy_bytes.len() as u32).to_le_bytes());
                    bytes.extend_from_slice(&policy_bytes);
                }
            }
        }

        bytes
    }

    /// Deserialize policy from bytes
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, RetentionPolicyError> {
        if bytes.is_empty() {
            return Err(RetentionPolicyError::Empty);
        }

        match bytes[0] {
            0x01 => Ok(RetentionPolicy::KeepAll),

            0x02 => {
                if bytes.len() < 9 {
                    return Err(RetentionPolicyError::InsufficientData);
                }
                let n = u64::from_le_bytes(bytes[1..9].try_into().unwrap()) as usize;
                if n == 0 {
                    return Err(RetentionPolicyError::InvalidValue(
                        "KeepLast(0) is invalid".to_string(),
                    ));
                }
                Ok(RetentionPolicy::KeepLast(n))
            }

            0x03 => {
                if bytes.len() < 9 {
                    return Err(RetentionPolicyError::InsufficientData);
                }
                let micros = u64::from_le_bytes(bytes[1..9].try_into().unwrap());
                if micros == 0 {
                    return Err(RetentionPolicyError::InvalidValue(
                        "KeepFor(0) is invalid".to_string(),
                    ));
                }
                Ok(RetentionPolicy::KeepFor(Duration::from_micros(micros)))
            }

            0x04 => {
                let mut cursor = 1;

                // Read default policy
                if bytes.len() < cursor + 4 {
                    return Err(RetentionPolicyError::InsufficientData);
                }
                let default_len =
                    u32::from_le_bytes(bytes[cursor..cursor + 4].try_into().unwrap()) as usize;
                cursor += 4;

                if bytes.len() < cursor + default_len {
                    return Err(RetentionPolicyError::InsufficientData);
                }
                let default = RetentionPolicy::from_bytes(&bytes[cursor..cursor + default_len])?;
                cursor += default_len;

                // Read override count
                if bytes.len() < cursor + 4 {
                    return Err(RetentionPolicyError::InsufficientData);
                }
                let override_count =
                    u32::from_le_bytes(bytes[cursor..cursor + 4].try_into().unwrap()) as usize;
                cursor += 4;

                // Read overrides
                let mut overrides = HashMap::new();
                for _ in 0..override_count {
                    if bytes.len() < cursor + 1 {
                        return Err(RetentionPolicyError::InsufficientData);
                    }
                    let ptype = primitive_type_from_byte(bytes[cursor])
                        .ok_or(RetentionPolicyError::InvalidPrimitiveType(bytes[cursor]))?;
                    cursor += 1;

                    if bytes.len() < cursor + 4 {
                        return Err(RetentionPolicyError::InsufficientData);
                    }
                    let policy_len =
                        u32::from_le_bytes(bytes[cursor..cursor + 4].try_into().unwrap()) as usize;
                    cursor += 4;

                    if bytes.len() < cursor + policy_len {
                        return Err(RetentionPolicyError::InsufficientData);
                    }
                    let policy = RetentionPolicy::from_bytes(&bytes[cursor..cursor + policy_len])?;
                    cursor += policy_len;

                    overrides.insert(ptype, Box::new(policy));
                }

                Ok(RetentionPolicy::Composite {
                    default: Box::new(default),
                    overrides,
                })
            }

            tag => Err(RetentionPolicyError::InvalidTag(tag)),
        }
    }
}

/// Builder for composite policies
pub struct CompositeBuilder {
    default: Box<RetentionPolicy>,
    overrides: HashMap<PrimitiveType, Box<RetentionPolicy>>,
}

impl CompositeBuilder {
    /// Add a policy override for a specific primitive type
    pub fn with_override(mut self, ptype: PrimitiveType, policy: RetentionPolicy) -> Self {
        self.overrides.insert(ptype, Box::new(policy));
        self
    }

    /// Build the composite policy
    pub fn build(self) -> RetentionPolicy {
        RetentionPolicy::Composite {
            default: self.default,
            overrides: self.overrides,
        }
    }
}

/// Retention policy errors
#[derive(Debug, thiserror::Error)]
pub enum RetentionPolicyError {
    /// Empty policy data
    #[error("Empty policy data")]
    Empty,

    /// Insufficient data for deserialization
    #[error("Insufficient data")]
    InsufficientData,

    /// Invalid tag byte
    #[error("Invalid tag: {0}")]
    InvalidTag(u8),

    /// Invalid primitive type byte
    #[error("Invalid primitive type: {0}")]
    InvalidPrimitiveType(u8),

    /// Invalid value
    #[error("Invalid value: {0}")]
    InvalidValue(String),
}

/// Convert PrimitiveType to byte for serialization
fn primitive_type_to_byte(ptype: PrimitiveType) -> u8 {
    match ptype {
        PrimitiveType::Kv => 0x01,
        PrimitiveType::Event => 0x02,
        PrimitiveType::State => 0x03,
        PrimitiveType::Trace => 0x04,
        PrimitiveType::Run => 0x05,
        PrimitiveType::Json => 0x06,
        PrimitiveType::Vector => 0x07,
    }
}

/// Convert byte to PrimitiveType for deserialization
fn primitive_type_from_byte(byte: u8) -> Option<PrimitiveType> {
    match byte {
        0x01 => Some(PrimitiveType::Kv),
        0x02 => Some(PrimitiveType::Event),
        0x03 => Some(PrimitiveType::State),
        0x04 => Some(PrimitiveType::Trace),
        0x05 => Some(PrimitiveType::Run),
        0x06 => Some(PrimitiveType::Json),
        0x07 => Some(PrimitiveType::Vector),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_keep_all_default() {
        let policy = RetentionPolicy::default();
        assert!(matches!(policy, RetentionPolicy::KeepAll));
    }

    #[test]
    fn test_keep_all_always_retains() {
        let policy = RetentionPolicy::keep_all();
        assert!(policy.should_retain(1, 0, 100, 1_000_000, PrimitiveType::Kv));
        assert!(policy.should_retain(1, 0, 1000000, 1_000_000, PrimitiveType::Event));
    }

    #[test]
    fn test_keep_last_retains_within_limit() {
        let policy = RetentionPolicy::keep_last(5);

        // Within limit
        assert!(policy.should_retain(1, 0, 5, 0, PrimitiveType::Kv));
        assert!(policy.should_retain(1, 0, 3, 0, PrimitiveType::Kv));
        assert!(policy.should_retain(1, 0, 1, 0, PrimitiveType::Kv));

        // Beyond limit
        assert!(!policy.should_retain(1, 0, 6, 0, PrimitiveType::Kv));
        assert!(!policy.should_retain(1, 0, 10, 0, PrimitiveType::Kv));
    }

    #[test]
    #[should_panic(expected = "KeepLast(n) requires n > 0")]
    fn test_keep_last_zero_panics() {
        RetentionPolicy::keep_last(0);
    }

    #[test]
    fn test_keep_for_retains_within_duration() {
        let policy = RetentionPolicy::keep_for(Duration::from_secs(3600)); // 1 hour
        let now = 1_000_000_000_000u64; // Current time in micros

        // Within duration (30 min ago)
        let recent = now - 30 * 60 * 1_000_000;
        assert!(policy.should_retain(1, recent, 1, now, PrimitiveType::Kv));

        // Beyond duration (2 hours ago)
        let old = now - 2 * 60 * 60 * 1_000_000;
        assert!(!policy.should_retain(1, old, 1, now, PrimitiveType::Kv));
    }

    #[test]
    #[should_panic(expected = "KeepFor requires non-zero duration")]
    fn test_keep_for_zero_panics() {
        RetentionPolicy::keep_for(Duration::ZERO);
    }

    #[test]
    fn test_composite_uses_default() {
        let policy = RetentionPolicy::composite(RetentionPolicy::keep_last(5))
            .with_override(PrimitiveType::Event, RetentionPolicy::keep_all())
            .build();

        // Kv uses default (keep_last(5))
        assert!(!policy.should_retain(1, 0, 10, 0, PrimitiveType::Kv));

        // Event uses override (keep_all)
        assert!(policy.should_retain(1, 0, 10, 0, PrimitiveType::Event));
    }

    #[test]
    fn test_composite_builder() {
        let policy = RetentionPolicy::composite(RetentionPolicy::keep_all())
            .with_override(PrimitiveType::Kv, RetentionPolicy::keep_last(100))
            .with_override(PrimitiveType::Json, RetentionPolicy::keep_last(50))
            .build();

        if let RetentionPolicy::Composite { default, overrides } = policy {
            assert!(matches!(*default, RetentionPolicy::KeepAll));
            assert_eq!(overrides.len(), 2);
            assert!(overrides.contains_key(&PrimitiveType::Kv));
            assert!(overrides.contains_key(&PrimitiveType::Json));
        } else {
            panic!("Expected Composite policy");
        }
    }

    #[test]
    fn test_serialization_keep_all() {
        let policy = RetentionPolicy::keep_all();
        let bytes = policy.to_bytes();
        let restored = RetentionPolicy::from_bytes(&bytes).unwrap();
        assert_eq!(policy, restored);
    }

    #[test]
    fn test_serialization_keep_last() {
        let policy = RetentionPolicy::keep_last(42);
        let bytes = policy.to_bytes();
        let restored = RetentionPolicy::from_bytes(&bytes).unwrap();
        assert_eq!(policy, restored);
    }

    #[test]
    fn test_serialization_keep_for() {
        let policy = RetentionPolicy::keep_for(Duration::from_secs(3600));
        let bytes = policy.to_bytes();
        let restored = RetentionPolicy::from_bytes(&bytes).unwrap();
        assert_eq!(policy, restored);
    }

    #[test]
    fn test_serialization_composite() {
        let policy = RetentionPolicy::composite(RetentionPolicy::keep_all())
            .with_override(PrimitiveType::Kv, RetentionPolicy::keep_last(100))
            .with_override(
                PrimitiveType::Event,
                RetentionPolicy::keep_for(Duration::from_secs(86400)),
            )
            .build();

        let bytes = policy.to_bytes();
        let restored = RetentionPolicy::from_bytes(&bytes).unwrap();

        // Compare summaries since HashMap iteration order may differ
        assert_eq!(policy.summary(), restored.summary());
    }

    #[test]
    fn test_deserialization_errors() {
        // Empty
        assert!(matches!(
            RetentionPolicy::from_bytes(&[]),
            Err(RetentionPolicyError::Empty)
        ));

        // Invalid tag
        assert!(matches!(
            RetentionPolicy::from_bytes(&[0xFF]),
            Err(RetentionPolicyError::InvalidTag(0xFF))
        ));

        // Insufficient data for KeepLast
        assert!(matches!(
            RetentionPolicy::from_bytes(&[0x02, 0x00]),
            Err(RetentionPolicyError::InsufficientData)
        ));

        // Invalid KeepLast(0)
        assert!(matches!(
            RetentionPolicy::from_bytes(&[0x02, 0, 0, 0, 0, 0, 0, 0, 0]),
            Err(RetentionPolicyError::InvalidValue(_))
        ));
    }

    #[test]
    fn test_summary() {
        assert_eq!(RetentionPolicy::keep_all().summary(), "KeepAll");
        assert_eq!(RetentionPolicy::keep_last(10).summary(), "KeepLast(10)");
        assert!(RetentionPolicy::keep_for(Duration::from_secs(60))
            .summary()
            .starts_with("KeepFor("));

        let composite = RetentionPolicy::composite(RetentionPolicy::keep_all())
            .with_override(PrimitiveType::Kv, RetentionPolicy::keep_last(10))
            .build();
        assert!(composite.summary().contains("Composite"));
    }

    #[test]
    fn test_primitive_type_byte_roundtrip() {
        for ptype in PrimitiveType::all() {
            let byte = primitive_type_to_byte(*ptype);
            let restored = primitive_type_from_byte(byte).unwrap();
            assert_eq!(*ptype, restored);
        }
    }
}
