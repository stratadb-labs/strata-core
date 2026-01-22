//! Run name type
//!
//! Runs have dual identity:
//! - **RunId**: Internal, immutable UUID assigned by the system
//! - **RunName**: User-facing, semantic identifier set by the user
//!
//! RunName is the "what" (semantic purpose), RunId is the "which" (unique instance).
//!
//! ## Examples
//!
//! - RunName: "training-gpt-v3-2024"
//! - RunId: 550e8400-e29b-41d4-a716-446655440000
//!
//! ## Validation
//!
//! Run names must:
//! - Be 1-256 characters
//! - Contain only alphanumeric, dash, underscore, dot
//! - Not start with a dash or dot

use serde::{Deserialize, Serialize};
use std::fmt;

/// Maximum length of a run name
pub const MAX_RUN_NAME_LENGTH: usize = 256;

/// User-facing semantic identifier for a run
///
/// RunName is the human-readable, meaningful name for a run.
/// Unlike RunId (which is a UUID), RunName captures the semantic
/// purpose of the run.
///
/// ## Validation Rules
///
/// - Length: 1-256 characters
/// - Characters: `[a-zA-Z0-9_.-]`
/// - Cannot start with `-` or `.`
///
/// ## Examples
///
/// Valid names:
/// - "training-run-1"
/// - "experiment.v2"
/// - "prod_agent_2024"
///
/// Invalid names:
/// - "" (empty)
/// - "-starts-with-dash"
/// - ".hidden"
/// - "has spaces"
/// - "has@special"
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RunName(String);

/// Error when validating a run name
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RunNameError {
    /// Name is empty
    Empty,
    /// Name exceeds maximum length
    TooLong {
        /// Actual length of the name
        length: usize,
        /// Maximum allowed length
        max: usize,
    },
    /// Name contains invalid character
    InvalidChar {
        /// The invalid character
        char: char,
        /// Position of the invalid character
        position: usize,
    },
    /// Name starts with invalid character
    InvalidStart {
        /// The invalid starting character
        char: char,
    },
}

impl std::fmt::Display for RunNameError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RunNameError::Empty => write!(f, "run name cannot be empty"),
            RunNameError::TooLong { length, max } => {
                write!(f, "run name too long: {} chars (max {})", length, max)
            }
            RunNameError::InvalidChar { char, position } => {
                write!(
                    f,
                    "invalid character '{}' at position {} (only alphanumeric, dash, underscore, dot allowed)",
                    char, position
                )
            }
            RunNameError::InvalidStart { char } => {
                write!(
                    f,
                    "run name cannot start with '{}' (must start with alphanumeric or underscore)",
                    char
                )
            }
        }
    }
}

impl std::error::Error for RunNameError {}

impl RunName {
    /// Create a new RunName, validating the input
    ///
    /// # Errors
    ///
    /// Returns `RunNameError` if the name is invalid.
    pub fn new(name: impl Into<String>) -> Result<Self, RunNameError> {
        let name = name.into();
        Self::validate(&name)?;
        Ok(RunName(name))
    }

    /// Create a RunName without validation
    ///
    /// # Safety
    ///
    /// The caller must ensure the name is valid. Use `new()` for untrusted input.
    pub fn new_unchecked(name: impl Into<String>) -> Self {
        RunName(name.into())
    }

    /// Validate a run name
    pub fn validate(name: &str) -> Result<(), RunNameError> {
        // Check empty
        if name.is_empty() {
            return Err(RunNameError::Empty);
        }

        // Check length
        if name.len() > MAX_RUN_NAME_LENGTH {
            return Err(RunNameError::TooLong {
                length: name.len(),
                max: MAX_RUN_NAME_LENGTH,
            });
        }

        // Check first character
        let first = name.chars().next().unwrap();
        if !first.is_ascii_alphanumeric() && first != '_' {
            return Err(RunNameError::InvalidStart { char: first });
        }

        // Check all characters
        for (pos, ch) in name.chars().enumerate() {
            if !Self::is_valid_char(ch) {
                return Err(RunNameError::InvalidChar {
                    char: ch,
                    position: pos,
                });
            }
        }

        Ok(())
    }

    /// Check if a character is valid in a run name
    #[inline]
    fn is_valid_char(c: char) -> bool {
        c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.'
    }

    /// Get the name as a string slice
    #[inline]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Consume and return the inner string
    pub fn into_inner(self) -> String {
        self.0
    }

    /// Check if this name matches a pattern (simple prefix match)
    pub fn starts_with(&self, prefix: &str) -> bool {
        self.0.starts_with(prefix)
    }

    /// Check if this name matches a pattern (simple suffix match)
    pub fn ends_with(&self, suffix: &str) -> bool {
        self.0.ends_with(suffix)
    }

    /// Check if this name contains a substring
    pub fn contains(&self, substr: &str) -> bool {
        self.0.contains(substr)
    }
}

impl AsRef<str> for RunName {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for RunName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl TryFrom<String> for RunName {
    type Error = RunNameError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        RunName::new(value)
    }
}

impl TryFrom<&str> for RunName {
    type Error = RunNameError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        RunName::new(value)
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_run_name_valid() {
        // Simple names
        assert!(RunName::new("test").is_ok());
        assert!(RunName::new("test-run").is_ok());
        assert!(RunName::new("test_run").is_ok());
        assert!(RunName::new("test.run").is_ok());
        assert!(RunName::new("TestRun123").is_ok());

        // Starting with underscore
        assert!(RunName::new("_private").is_ok());

        // Mixed
        assert!(RunName::new("training-gpt-v3-2024").is_ok());
        assert!(RunName::new("experiment.v2.1").is_ok());
        assert!(RunName::new("prod_agent_2024_01_15").is_ok());
    }

    #[test]
    fn test_run_name_empty() {
        let err = RunName::new("").unwrap_err();
        assert_eq!(err, RunNameError::Empty);
    }

    #[test]
    fn test_run_name_too_long() {
        let long_name = "a".repeat(MAX_RUN_NAME_LENGTH + 1);
        let err = RunName::new(long_name).unwrap_err();
        assert!(matches!(err, RunNameError::TooLong { .. }));
    }

    #[test]
    fn test_run_name_max_length_ok() {
        let max_name = "a".repeat(MAX_RUN_NAME_LENGTH);
        assert!(RunName::new(max_name).is_ok());
    }

    #[test]
    fn test_run_name_invalid_start_dash() {
        let err = RunName::new("-starts-with-dash").unwrap_err();
        assert!(matches!(err, RunNameError::InvalidStart { char: '-' }));
    }

    #[test]
    fn test_run_name_invalid_start_dot() {
        let err = RunName::new(".hidden").unwrap_err();
        assert!(matches!(err, RunNameError::InvalidStart { char: '.' }));
    }

    #[test]
    fn test_run_name_invalid_chars() {
        // Space
        let err = RunName::new("has space").unwrap_err();
        assert!(matches!(err, RunNameError::InvalidChar { char: ' ', .. }));

        // Special characters
        let err = RunName::new("has@special").unwrap_err();
        assert!(matches!(err, RunNameError::InvalidChar { char: '@', .. }));

        // Unicode
        let err = RunName::new("has\u{1F600}emoji").unwrap_err();
        assert!(matches!(err, RunNameError::InvalidChar { .. }));
    }

    #[test]
    fn test_run_name_as_str() {
        let name = RunName::new("test-run").unwrap();
        assert_eq!(name.as_str(), "test-run");
    }

    #[test]
    fn test_run_name_into_inner() {
        let name = RunName::new("test-run").unwrap();
        assert_eq!(name.into_inner(), "test-run".to_string());
    }

    #[test]
    fn test_run_name_display() {
        let name = RunName::new("test-run").unwrap();
        assert_eq!(format!("{}", name), "test-run");
    }

    #[test]
    fn test_run_name_equality() {
        let name1 = RunName::new("test").unwrap();
        let name2 = RunName::new("test").unwrap();
        let name3 = RunName::new("other").unwrap();

        assert_eq!(name1, name2);
        assert_ne!(name1, name3);
    }

    #[test]
    fn test_run_name_hash() {
        use std::collections::HashSet;

        let mut set = HashSet::new();
        set.insert(RunName::new("name1").unwrap());
        set.insert(RunName::new("name2").unwrap());
        set.insert(RunName::new("name1").unwrap()); // Duplicate

        assert_eq!(set.len(), 2);
    }

    #[test]
    fn test_run_name_serialization() {
        let name = RunName::new("test-run").unwrap();
        let json = serde_json::to_string(&name).unwrap();
        let restored: RunName = serde_json::from_str(&json).unwrap();
        assert_eq!(name, restored);
    }

    #[test]
    fn test_run_name_try_from_string() {
        let name: Result<RunName, _> = "test-run".to_string().try_into();
        assert!(name.is_ok());

        let name: Result<RunName, _> = "".to_string().try_into();
        assert!(name.is_err());
    }

    #[test]
    fn test_run_name_try_from_str() {
        let name: Result<RunName, _> = "test-run".try_into();
        assert!(name.is_ok());

        let name: Result<RunName, _> = "".try_into();
        assert!(name.is_err());
    }

    #[test]
    fn test_run_name_starts_with() {
        let name = RunName::new("training-run-1").unwrap();
        assert!(name.starts_with("training"));
        assert!(!name.starts_with("test"));
    }

    #[test]
    fn test_run_name_ends_with() {
        let name = RunName::new("training-run-1").unwrap();
        assert!(name.ends_with("-1"));
        assert!(!name.ends_with("-2"));
    }

    #[test]
    fn test_run_name_contains() {
        let name = RunName::new("training-run-1").unwrap();
        assert!(name.contains("run"));
        assert!(!name.contains("test"));
    }

    #[test]
    fn test_run_name_error_display() {
        assert_eq!(
            format!("{}", RunNameError::Empty),
            "run name cannot be empty"
        );
        assert!(format!("{}", RunNameError::TooLong { length: 300, max: 256 }).contains("too long"));
        assert!(format!("{}", RunNameError::InvalidChar { char: '@', position: 5 }).contains("@"));
        assert!(format!("{}", RunNameError::InvalidStart { char: '-' }).contains("start"));
    }

    #[test]
    fn test_run_name_new_unchecked() {
        // This bypasses validation - use carefully
        let name = RunName::new_unchecked("any-string-even-invalid!");
        assert_eq!(name.as_str(), "any-string-even-invalid!");
    }
}
