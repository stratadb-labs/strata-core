//! Facade configuration types
//!
//! These types control facade behavior without leaking substrate details.

use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Facade configuration
///
/// Controls default behavior for facade operations.
#[derive(Debug, Clone)]
pub struct FacadeConfig {
    /// Default timeout for operations
    pub default_timeout: Option<Duration>,

    /// Whether to return versions with values (default: false)
    pub return_versions: bool,

    /// Whether to auto-commit (default: true)
    pub auto_commit: bool,
}

impl Default for FacadeConfig {
    fn default() -> Self {
        FacadeConfig {
            default_timeout: None,
            return_versions: false,
            auto_commit: true, // Auto-commit by default
        }
    }
}

impl FacadeConfig {
    /// Create a new facade configuration
    pub fn new() -> Self {
        Self::default()
    }

    /// Set default timeout
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.default_timeout = Some(timeout);
        self
    }

    /// Enable version returns on reads
    pub fn with_versions(mut self) -> Self {
        self.return_versions = true;
        self
    }

    /// Disable auto-commit (for batching)
    pub fn without_auto_commit(mut self) -> Self {
        self.auto_commit = false;
        self
    }
}

/// Options for GET operations
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GetOptions {
    /// Return the version along with the value
    pub with_version: bool,

    /// Get value at a specific version (if supported)
    pub at_version: Option<u64>,
}

impl GetOptions {
    /// Create default options
    pub fn new() -> Self {
        Self::default()
    }

    /// Request version information
    pub fn with_version(mut self) -> Self {
        self.with_version = true;
        self
    }

    /// Get value at specific version
    pub fn at_version(mut self, version: u64) -> Self {
        self.at_version = Some(version);
        self
    }
}

/// Options for SET operations
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SetOptions {
    /// Only set if key doesn't exist (NX)
    pub only_if_not_exists: bool,

    /// Only set if key exists (XX)
    pub only_if_exists: bool,

    /// Get the old value while setting
    pub get_old_value: bool,

    /// Expected version for optimistic locking
    pub expected_version: Option<u64>,
}

impl SetOptions {
    /// Create default options
    pub fn new() -> Self {
        Self::default()
    }

    /// Only set if key doesn't exist (NX)
    pub fn nx(mut self) -> Self {
        self.only_if_not_exists = true;
        self.only_if_exists = false;
        self
    }

    /// Only set if key exists (XX)
    pub fn xx(mut self) -> Self {
        self.only_if_exists = true;
        self.only_if_not_exists = false;
        self
    }

    /// Get old value on set
    pub fn get(mut self) -> Self {
        self.get_old_value = true;
        self
    }

    /// Conditional set with version check
    pub fn if_version(mut self, version: u64) -> Self {
        self.expected_version = Some(version);
        self
    }
}

/// Options for INCR operations
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct IncrOptions {
    /// Initialize to this value if key doesn't exist (default: 0)
    pub initial: Option<i64>,
}

impl IncrOptions {
    /// Create default options
    pub fn new() -> Self {
        Self::default()
    }

    /// Set initial value for missing key
    pub fn with_initial(mut self, initial: i64) -> Self {
        self.initial = Some(initial);
        self
    }
}

/// Range options for list/scan operations
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RangeOptions {
    /// Start offset (0-based)
    pub offset: Option<u64>,

    /// Maximum count
    pub limit: Option<u64>,

    /// Pattern for key filtering (if applicable)
    pub pattern: Option<String>,
}

impl RangeOptions {
    /// Create default options
    pub fn new() -> Self {
        Self::default()
    }

    /// Set offset
    pub fn offset(mut self, offset: u64) -> Self {
        self.offset = Some(offset);
        self
    }

    /// Set limit
    pub fn limit(mut self, limit: u64) -> Self {
        self.limit = Some(limit);
        self
    }

    /// Set pattern filter
    pub fn pattern(mut self, pattern: impl Into<String>) -> Self {
        self.pattern = Some(pattern.into());
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_facade_config_default() {
        let config = FacadeConfig::default();
        assert!(config.default_timeout.is_none());
        assert!(!config.return_versions);
        assert!(config.auto_commit);
    }

    #[test]
    fn test_facade_config_builder() {
        let config = FacadeConfig::new()
            .with_timeout(Duration::from_secs(30))
            .with_versions()
            .without_auto_commit();

        assert_eq!(config.default_timeout, Some(Duration::from_secs(30)));
        assert!(config.return_versions);
        assert!(!config.auto_commit);
    }

    #[test]
    fn test_get_options() {
        let opts = GetOptions::new().with_version().at_version(42);
        assert!(opts.with_version);
        assert_eq!(opts.at_version, Some(42));
    }

    #[test]
    fn test_set_options_nx() {
        let opts = SetOptions::new().nx();
        assert!(opts.only_if_not_exists);
        assert!(!opts.only_if_exists);
    }

    #[test]
    fn test_set_options_xx() {
        let opts = SetOptions::new().xx();
        assert!(opts.only_if_exists);
        assert!(!opts.only_if_not_exists);
    }

    #[test]
    fn test_set_options_get() {
        let opts = SetOptions::new().get();
        assert!(opts.get_old_value);
    }

    #[test]
    fn test_range_options() {
        let opts = RangeOptions::new()
            .offset(10)
            .limit(100)
            .pattern("user:*");

        assert_eq!(opts.offset, Some(10));
        assert_eq!(opts.limit, Some(100));
        assert_eq!(opts.pattern, Some("user:*".to_string()));
    }
}
