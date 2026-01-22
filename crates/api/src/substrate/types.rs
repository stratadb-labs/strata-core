//! Core types for the Substrate API
//!
//! This module defines the fundamental types used across the Substrate API:
//! - `ApiRunId`: Run identifier (either "default" or UUID)
//! - `RunInfo`: Run metadata and state
//! - `RunState`: Run lifecycle state (Active/Closed)
//! - `RetentionPolicy`: History retention configuration

use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;
use std::time::Duration;
use strata_core::Value;
use uuid::Uuid;

// =============================================================================
// Constants
// =============================================================================

/// The canonical name for the default run
pub const DEFAULT_RUN_NAME: &str = "default";

/// The default run ID (literal string "default")
pub const DEFAULT_RUN_ID: &str = "default";

// =============================================================================
// ApiRunId
// =============================================================================

/// Run identifier for the Substrate API
///
/// A run ID is either:
/// - The literal string "default" for the default run
/// - A UUID v4 in lowercase hyphenated format (e.g., "f47ac10b-58cc-4372-a567-0e02b2c3d479")
///
/// ## Validation
///
/// - "default" is always valid
/// - UUIDs must be valid UUID v4 format
/// - Empty strings are invalid
/// - Other strings are invalid
///
/// ## Examples
///
/// ```
/// use strata_api::substrate::ApiRunId;
///
/// // Default run
/// let default = ApiRunId::default_run_id();
/// assert!(default.is_default());
///
/// // New UUID run
/// let custom = ApiRunId::new();
/// assert!(!custom.is_default());
///
/// // Parse from string
/// let parsed = ApiRunId::parse("default").unwrap();
/// assert!(parsed.is_default());
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ApiRunId(String);

impl ApiRunId {
    /// Create a new random run ID (UUID v4)
    pub fn new() -> Self {
        ApiRunId(Uuid::new_v4().to_string())
    }

    /// Create the default run ID
    ///
    /// Returns a run ID for the default run (literal string "default").
    pub fn default_run_id() -> Self {
        ApiRunId(DEFAULT_RUN_NAME.to_string())
    }

    /// Parse a run ID from a string
    ///
    /// Returns `Some(ApiRunId)` if the string is valid, `None` otherwise.
    ///
    /// Valid formats:
    /// - "default" (case-sensitive)
    /// - UUID v4 in lowercase hyphenated format
    pub fn parse(s: &str) -> Option<Self> {
        if s.is_empty() {
            return None;
        }

        if s == DEFAULT_RUN_NAME {
            return Some(ApiRunId(DEFAULT_RUN_NAME.to_string()));
        }

        // Try to parse as UUID
        if Uuid::parse_str(s).is_ok() {
            return Some(ApiRunId(s.to_lowercase()));
        }

        None
    }

    /// Check if this is the default run
    #[inline]
    pub fn is_default(&self) -> bool {
        self.0 == DEFAULT_RUN_NAME
    }

    /// Check if this is a UUID run (not default)
    #[inline]
    pub fn is_uuid(&self) -> bool {
        !self.is_default()
    }

    /// Get the string representation
    #[inline]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Consume and return the inner string
    #[inline]
    pub fn into_string(self) -> String {
        self.0
    }

    /// Create from a UUID
    pub fn from_uuid(uuid: Uuid) -> Self {
        ApiRunId(uuid.to_string())
    }

    /// Try to get the underlying UUID (returns None for default run)
    pub fn as_uuid(&self) -> Option<Uuid> {
        if self.is_default() {
            None
        } else {
            Uuid::parse_str(&self.0).ok()
        }
    }
}

impl Default for ApiRunId {
    fn default() -> Self {
        ApiRunId::default_run_id()
    }
}

impl fmt::Display for ApiRunId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl FromStr for ApiRunId {
    type Err = InvalidRunIdError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        ApiRunId::parse(s).ok_or_else(|| InvalidRunIdError(s.to_string()))
    }
}

impl From<Uuid> for ApiRunId {
    fn from(uuid: Uuid) -> Self {
        ApiRunId::from_uuid(uuid)
    }
}

/// Error returned when parsing an invalid run ID
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InvalidRunIdError(pub String);

impl fmt::Display for InvalidRunIdError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Invalid run ID '{}': must be 'default' or a valid UUID",
            self.0
        )
    }
}

impl std::error::Error for InvalidRunIdError {}

// =============================================================================
// RunState
// =============================================================================

/// Run lifecycle state
///
/// A run can be in one of two states:
/// - `Active`: The run is open and accepting operations
/// - `Closed`: The run has been closed and is read-only
///
/// ## State Transitions
///
/// ```text
/// [Created] --> Active --> Closed
/// ```
///
/// Once a run is closed, it cannot be reopened.
/// The default run cannot be closed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RunState {
    /// Run is active and accepting operations
    #[default]
    Active,
    /// Run has been closed (read-only)
    Closed,
}

impl RunState {
    /// Check if the run is active
    #[inline]
    pub fn is_active(&self) -> bool {
        matches!(self, RunState::Active)
    }

    /// Check if the run is closed
    #[inline]
    pub fn is_closed(&self) -> bool {
        matches!(self, RunState::Closed)
    }

    /// Get the string representation
    pub fn as_str(&self) -> &'static str {
        match self {
            RunState::Active => "active",
            RunState::Closed => "closed",
        }
    }
}

impl fmt::Display for RunState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

// =============================================================================
// RunInfo
// =============================================================================

/// Run information and metadata
///
/// Contains all user-visible information about a run.
///
/// ## Fields
///
/// - `run_id`: The run identifier
/// - `created_at`: Creation timestamp (microseconds since Unix epoch)
/// - `metadata`: User-provided metadata (Value::Object or Value::Null)
/// - `state`: Current lifecycle state
///
/// ## Wire Encoding
///
/// ```json
/// {
///   "run_id": "f47ac10b-58cc-4372-a567-0e02b2c3d479",
///   "created_at": 1700000000000000,
///   "metadata": {"name": "experiment-1"},
///   "state": "active"
/// }
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RunInfo {
    /// Run identifier
    pub run_id: ApiRunId,

    /// Creation timestamp (microseconds since Unix epoch)
    pub created_at: u64,

    /// User-provided metadata
    pub metadata: Value,

    /// Current lifecycle state
    pub state: RunState,
}

impl RunInfo {
    /// Create new run info with the given parameters
    pub fn new(run_id: ApiRunId, created_at: u64, metadata: Value) -> Self {
        RunInfo {
            run_id,
            created_at,
            metadata,
            state: RunState::Active,
        }
    }

    /// Create run info for the default run
    pub fn default_run(created_at: u64) -> Self {
        RunInfo {
            run_id: ApiRunId::default_run_id(),
            created_at,
            metadata: Value::Null,
            state: RunState::Active,
        }
    }

    /// Check if this is the default run
    #[inline]
    pub fn is_default(&self) -> bool {
        self.run_id.is_default()
    }

    /// Check if the run is active
    #[inline]
    pub fn is_active(&self) -> bool {
        self.state.is_active()
    }

    /// Check if the run is closed
    #[inline]
    pub fn is_closed(&self) -> bool {
        self.state.is_closed()
    }

    /// Mark the run as closed
    pub fn close(&mut self) {
        self.state = RunState::Closed;
    }
}

// =============================================================================
// RetentionPolicy
// =============================================================================

/// Retention policy for version history
///
/// Controls how long historical versions are retained before being eligible
/// for garbage collection.
///
/// ## Variants
///
/// - `KeepAll`: Keep all versions indefinitely (default)
/// - `KeepLast(n)`: Keep the N most recent versions
/// - `KeepFor(duration)`: Keep versions within a time window
/// - `Composite(policies)`: Union of multiple policies (most permissive wins)
///
/// ## Examples
///
/// ```
/// use strata_api::substrate::RetentionPolicy;
/// use std::time::Duration;
///
/// // Keep all history (default)
/// let policy = RetentionPolicy::KeepAll;
///
/// // Keep last 100 versions
/// let policy = RetentionPolicy::KeepLast(100);
///
/// // Keep 7 days of history
/// let policy = RetentionPolicy::KeepFor(Duration::from_secs(7 * 24 * 60 * 60));
///
/// // Keep either last 100 or 7 days, whichever is more permissive
/// let policy = RetentionPolicy::Composite(vec![
///     RetentionPolicy::KeepLast(100),
///     RetentionPolicy::KeepFor(Duration::from_secs(7 * 24 * 60 * 60)),
/// ]);
/// ```
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(tag = "type", content = "value")]
pub enum RetentionPolicy {
    /// Keep all versions indefinitely
    #[serde(rename = "keep_all")]
    #[default]
    KeepAll,

    /// Keep the N most recent versions
    #[serde(rename = "keep_last")]
    KeepLast(u64),

    /// Keep versions within a time window
    #[serde(rename = "keep_for")]
    KeepFor(#[serde(with = "duration_micros")] Duration),

    /// Union of multiple policies (most permissive wins)
    #[serde(rename = "composite")]
    Composite(Vec<RetentionPolicy>),
}

impl RetentionPolicy {
    /// Create a policy to keep the last N versions
    pub fn keep_last(n: u64) -> Self {
        RetentionPolicy::KeepLast(n)
    }

    /// Create a policy to keep versions for a duration
    pub fn keep_for(duration: Duration) -> Self {
        RetentionPolicy::KeepFor(duration)
    }

    /// Create a composite policy
    pub fn composite(policies: Vec<RetentionPolicy>) -> Self {
        RetentionPolicy::Composite(policies)
    }

    /// Check if this is the default "keep all" policy
    pub fn is_keep_all(&self) -> bool {
        matches!(self, RetentionPolicy::KeepAll)
    }

    /// Check if a version count should be retained under KeepLast policy
    ///
    /// Returns true if the version at position `index` (0 = newest) should be kept.
    pub fn should_keep_by_count(&self, index: u64) -> bool {
        match self {
            RetentionPolicy::KeepAll => true,
            RetentionPolicy::KeepLast(n) => index < *n,
            RetentionPolicy::KeepFor(_) => true, // Count-based check doesn't apply
            RetentionPolicy::Composite(policies) => {
                policies.iter().any(|p| p.should_keep_by_count(index))
            }
        }
    }

    /// Check if a version with the given age should be retained
    ///
    /// Returns true if a version with the given age (from now) should be kept.
    pub fn should_keep_by_age(&self, age: Duration) -> bool {
        match self {
            RetentionPolicy::KeepAll => true,
            RetentionPolicy::KeepLast(_) => true, // Age-based check doesn't apply
            RetentionPolicy::KeepFor(max_age) => age <= *max_age,
            RetentionPolicy::Composite(policies) => {
                policies.iter().any(|p| p.should_keep_by_age(age))
            }
        }
    }
}

impl fmt::Display for RetentionPolicy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RetentionPolicy::KeepAll => write!(f, "keep_all"),
            RetentionPolicy::KeepLast(n) => write!(f, "keep_last({})", n),
            RetentionPolicy::KeepFor(d) => write!(f, "keep_for({}s)", d.as_secs()),
            RetentionPolicy::Composite(policies) => {
                write!(f, "composite[")?;
                for (i, p) in policies.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}", p)?;
                }
                write!(f, "]")
            }
        }
    }
}

/// Serde module for Duration as microseconds
mod duration_micros {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    use std::time::Duration;

    pub fn serialize<S>(duration: &Duration, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        duration.as_micros().serialize(serializer)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Duration, D::Error>
    where
        D: Deserializer<'de>,
    {
        let micros = u128::deserialize(deserializer)?;
        Ok(Duration::from_micros(micros as u64))
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // =========================================================================
    // ApiRunId Tests
    // =========================================================================

    #[test]
    fn test_api_run_id_default() {
        let id = ApiRunId::default_run_id();
        assert!(id.is_default());
        assert!(!id.is_uuid());
        assert_eq!(id.as_str(), "default");
    }

    #[test]
    fn test_api_run_id_new() {
        let id = ApiRunId::new();
        assert!(!id.is_default());
        assert!(id.is_uuid());
        assert!(id.as_uuid().is_some());
    }

    #[test]
    fn test_api_run_id_parse_default() {
        let id = ApiRunId::parse("default").unwrap();
        assert!(id.is_default());
    }

    #[test]
    fn test_api_run_id_parse_uuid() {
        let uuid_str = "f47ac10b-58cc-4372-a567-0e02b2c3d479";
        let id = ApiRunId::parse(uuid_str).unwrap();
        assert!(!id.is_default());
        assert!(id.is_uuid());
        assert_eq!(id.as_str(), uuid_str);
    }

    #[test]
    fn test_api_run_id_parse_invalid() {
        assert!(ApiRunId::parse("").is_none());
        assert!(ApiRunId::parse("invalid").is_none());
        assert!(ApiRunId::parse("Default").is_none()); // case-sensitive
        assert!(ApiRunId::parse("not-a-uuid").is_none());
    }

    #[test]
    fn test_api_run_id_from_str() {
        let id: ApiRunId = "default".parse().unwrap();
        assert!(id.is_default());

        let result: Result<ApiRunId, _> = "invalid".parse();
        assert!(result.is_err());
    }

    #[test]
    fn test_api_run_id_display() {
        let default = ApiRunId::default_run_id();
        assert_eq!(format!("{}", default), "default");

        let custom = ApiRunId::new();
        let display = format!("{}", custom);
        assert!(Uuid::parse_str(&display).is_ok());
    }

    #[test]
    fn test_api_run_id_serialization() {
        let id = ApiRunId::default_run_id();
        let json = serde_json::to_string(&id).unwrap();
        let restored: ApiRunId = serde_json::from_str(&json).unwrap();
        assert_eq!(id, restored);

        let uuid_id = ApiRunId::new();
        let json = serde_json::to_string(&uuid_id).unwrap();
        let restored: ApiRunId = serde_json::from_str(&json).unwrap();
        assert_eq!(uuid_id, restored);
    }

    #[test]
    fn test_api_run_id_equality() {
        let id1 = ApiRunId::default_run_id();
        let id2 = ApiRunId::default_run_id();
        assert_eq!(id1, id2);

        let id3 = ApiRunId::new();
        let id4 = ApiRunId::new();
        assert_ne!(id3, id4); // Different UUIDs
    }

    #[test]
    fn test_api_run_id_hash() {
        use std::collections::HashSet;

        let mut set = HashSet::new();
        set.insert(ApiRunId::default_run_id());
        set.insert(ApiRunId::default_run_id()); // Duplicate

        assert_eq!(set.len(), 1);
        assert!(set.contains(&ApiRunId::default_run_id()));
    }

    // =========================================================================
    // RunState Tests
    // =========================================================================

    #[test]
    fn test_run_state_active() {
        let state = RunState::Active;
        assert!(state.is_active());
        assert!(!state.is_closed());
        assert_eq!(state.as_str(), "active");
    }

    #[test]
    fn test_run_state_closed() {
        let state = RunState::Closed;
        assert!(!state.is_active());
        assert!(state.is_closed());
        assert_eq!(state.as_str(), "closed");
    }

    #[test]
    fn test_run_state_default() {
        assert_eq!(RunState::default(), RunState::Active);
    }

    #[test]
    fn test_run_state_serialization() {
        let active = RunState::Active;
        let json = serde_json::to_string(&active).unwrap();
        assert_eq!(json, "\"active\"");

        let closed = RunState::Closed;
        let json = serde_json::to_string(&closed).unwrap();
        assert_eq!(json, "\"closed\"");

        let restored: RunState = serde_json::from_str("\"active\"").unwrap();
        assert_eq!(restored, RunState::Active);
    }

    // =========================================================================
    // RunInfo Tests
    // =========================================================================

    #[test]
    fn test_run_info_new() {
        let run_id = ApiRunId::new();
        let metadata = Value::Object(std::collections::HashMap::from([(
            "name".to_string(),
            Value::String("test".to_string()),
        )]));
        let info = RunInfo::new(run_id.clone(), 1000000, metadata.clone());

        assert_eq!(info.run_id, run_id);
        assert_eq!(info.created_at, 1000000);
        assert_eq!(info.metadata, metadata);
        assert!(info.is_active());
    }

    #[test]
    fn test_run_info_default_run() {
        let info = RunInfo::default_run(1000000);
        assert!(info.is_default());
        assert!(info.is_active());
        assert_eq!(info.metadata, Value::Null);
    }

    #[test]
    fn test_run_info_close() {
        let mut info = RunInfo::default_run(1000000);
        assert!(info.is_active());

        info.close();
        assert!(info.is_closed());
        assert!(!info.is_active());
    }

    #[test]
    fn test_run_info_serialization() {
        let info = RunInfo::new(ApiRunId::default_run_id(), 1000000, Value::Null);

        let json = serde_json::to_string(&info).unwrap();
        let restored: RunInfo = serde_json::from_str(&json).unwrap();

        assert_eq!(info, restored);
    }

    // =========================================================================
    // RetentionPolicy Tests
    // =========================================================================

    #[test]
    fn test_retention_policy_keep_all() {
        let policy = RetentionPolicy::KeepAll;
        assert!(policy.is_keep_all());
        assert!(policy.should_keep_by_count(0));
        assert!(policy.should_keep_by_count(1000000));
        assert!(policy.should_keep_by_age(Duration::from_secs(0)));
        assert!(policy.should_keep_by_age(Duration::from_secs(1000000)));
    }

    #[test]
    fn test_retention_policy_keep_last() {
        let policy = RetentionPolicy::KeepLast(10);
        assert!(!policy.is_keep_all());

        // Should keep first 10
        assert!(policy.should_keep_by_count(0));
        assert!(policy.should_keep_by_count(9));
        assert!(!policy.should_keep_by_count(10));
        assert!(!policy.should_keep_by_count(100));

        // Age doesn't apply
        assert!(policy.should_keep_by_age(Duration::from_secs(1000000)));
    }

    #[test]
    fn test_retention_policy_keep_for() {
        let policy = RetentionPolicy::KeepFor(Duration::from_secs(3600)); // 1 hour

        // Count doesn't apply
        assert!(policy.should_keep_by_count(1000000));

        // Should keep within 1 hour
        assert!(policy.should_keep_by_age(Duration::from_secs(0)));
        assert!(policy.should_keep_by_age(Duration::from_secs(3600)));
        assert!(!policy.should_keep_by_age(Duration::from_secs(3601)));
    }

    #[test]
    fn test_retention_policy_composite() {
        let policy = RetentionPolicy::Composite(vec![
            RetentionPolicy::KeepLast(10),
            RetentionPolicy::KeepFor(Duration::from_secs(3600)),
        ]);

        // Should keep if either policy allows
        assert!(policy.should_keep_by_count(0)); // KeepLast allows
        assert!(policy.should_keep_by_count(9)); // KeepLast allows
        assert!(policy.should_keep_by_count(10)); // KeepFor allows (count doesn't apply)

        assert!(policy.should_keep_by_age(Duration::from_secs(3600))); // KeepFor allows
        assert!(policy.should_keep_by_age(Duration::from_secs(3601))); // KeepLast allows (age doesn't apply)
    }

    #[test]
    fn test_retention_policy_default() {
        assert_eq!(RetentionPolicy::default(), RetentionPolicy::KeepAll);
    }

    #[test]
    fn test_retention_policy_display() {
        assert_eq!(format!("{}", RetentionPolicy::KeepAll), "keep_all");
        assert_eq!(format!("{}", RetentionPolicy::KeepLast(10)), "keep_last(10)");
        assert_eq!(
            format!("{}", RetentionPolicy::KeepFor(Duration::from_secs(3600))),
            "keep_for(3600s)"
        );
    }

    #[test]
    fn test_retention_policy_serialization() {
        let policies = vec![
            RetentionPolicy::KeepAll,
            RetentionPolicy::KeepLast(10),
            RetentionPolicy::KeepFor(Duration::from_secs(3600)),
            RetentionPolicy::Composite(vec![
                RetentionPolicy::KeepLast(10),
                RetentionPolicy::KeepFor(Duration::from_secs(3600)),
            ]),
        ];

        for policy in policies {
            let json = serde_json::to_string(&policy).unwrap();
            let restored: RetentionPolicy = serde_json::from_str(&json).unwrap();
            assert_eq!(policy, restored);
        }
    }
}
