//! Key validation for Strata
//!
//! This module defines key validation rules that are enforced by all API layers.
//! Keys are Unicode strings with specific constraints.
//!
//! ## Contract
//!
//! After stabilization, these validation rules are FROZEN:
//! - Keys must be valid UTF-8 (guaranteed by Rust's &str type)
//! - Keys must not be empty
//! - Keys must not contain NUL bytes (\0)
//! - Keys must not start with reserved prefix `_strata/`
//! - Keys must not exceed `max_key_bytes` (default: 1024)

use crate::limits::Limits;
use thiserror::Error;

/// Reserved system prefix for internal keys
pub const RESERVED_PREFIX: &str = "_strata/";

/// Validate a key using default limits
///
/// This is the primary validation function for user-facing APIs.
/// It validates all key rules: non-empty, no NUL, no reserved prefix, length.
///
/// # Examples
///
/// ```
/// use strata_core::key::validate_key;
///
/// // Valid keys
/// assert!(validate_key("mykey").is_ok());
/// assert!(validate_key("user:123").is_ok());
/// assert!(validate_key("日本語").is_ok());
///
/// // Invalid keys
/// assert!(validate_key("").is_err()); // empty
/// assert!(validate_key("a\x00b").is_err()); // contains NUL
/// assert!(validate_key("_strata/internal").is_err()); // reserved prefix
/// ```
pub fn validate_key(key: &str) -> Result<(), KeyError> {
    validate_key_with_limits(key, &Limits::default())
}

/// Validate a key with custom limits
///
/// This is useful when the database is opened with custom limits.
pub fn validate_key_with_limits(key: &str, limits: &Limits) -> Result<(), KeyError> {
    // Rule 1: Key cannot be empty
    if key.is_empty() {
        return Err(KeyError::Empty);
    }

    // Rule 2: Key cannot contain NUL bytes
    if key.contains('\x00') {
        return Err(KeyError::ContainsNul);
    }

    // Rule 3: Key cannot use reserved prefix
    if key.starts_with(RESERVED_PREFIX) {
        return Err(KeyError::ReservedPrefix);
    }

    // Rule 4: Key cannot exceed max length
    let len = key.len();
    if len > limits.max_key_bytes {
        return Err(KeyError::TooLong {
            actual: len,
            max: limits.max_key_bytes,
        });
    }

    // Note: UTF-8 validity is guaranteed by Rust's &str type

    Ok(())
}

/// Key validation errors
///
/// These errors map to `InvalidKey` error code in the wire protocol.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum KeyError {
    /// Key is empty (length 0)
    #[error("Key cannot be empty")]
    Empty,

    /// Key contains NUL byte (\0)
    #[error("Key cannot contain NUL bytes")]
    ContainsNul,

    /// Key uses reserved system prefix `_strata/`
    #[error("Key cannot use reserved prefix '{}'", RESERVED_PREFIX)]
    ReservedPrefix,

    /// Key exceeds maximum length
    #[error("Key too long: {actual} bytes exceeds maximum {max}")]
    TooLong {
        /// Actual key length in bytes
        actual: usize,
        /// Maximum allowed length
        max: usize,
    },
}

impl KeyError {
    /// Get the reason code for wire protocol
    pub fn reason_code(&self) -> &'static str {
        match self {
            KeyError::Empty => "empty_key",
            KeyError::ContainsNul => "contains_nul",
            KeyError::ReservedPrefix => "reserved_prefix",
            KeyError::TooLong { .. } => "key_too_long",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // === Valid Keys ===

    #[test]
    fn test_valid_simple_key() {
        assert!(validate_key("mykey").is_ok());
    }

    #[test]
    fn test_valid_unicode_key() {
        assert!(validate_key("日本語キー").is_ok());
    }

    #[test]
    fn test_valid_emoji_key() {
        assert!(validate_key("🔑key🔑").is_ok());
    }

    #[test]
    fn test_valid_numeric_string_key() {
        assert!(validate_key("12345").is_ok());
    }

    #[test]
    fn test_valid_special_chars_key() {
        assert!(validate_key("a-b_c.d:e/f").is_ok());
    }

    #[test]
    fn test_valid_single_char_key() {
        assert!(validate_key("a").is_ok());
    }

    #[test]
    fn test_valid_whitespace_key() {
        // Whitespace is allowed
        assert!(validate_key("  spaces  ").is_ok());
    }

    #[test]
    fn test_valid_newline_key() {
        // Newlines are allowed
        assert!(validate_key("line1\nline2").is_ok());
    }

    #[test]
    fn test_valid_tab_key() {
        // Tabs are allowed
        assert!(validate_key("col1\tcol2").is_ok());
    }

    #[test]
    fn test_valid_underscore_prefix() {
        // _mykey is valid (not _strata/)
        assert!(validate_key("_mykey").is_ok());
    }

    #[test]
    fn test_valid_similar_to_reserved() {
        // _stratafoo is valid (no slash after _strata)
        assert!(validate_key("_stratafoo").is_ok());
    }

    #[test]
    fn test_valid_strata_without_underscore() {
        // strata/foo is valid (no underscore prefix)
        assert!(validate_key("strata/foo").is_ok());
    }

    #[test]
    fn test_valid_key_at_max_length() {
        let limits = Limits::default();
        let key = "x".repeat(limits.max_key_bytes);
        assert!(validate_key_with_limits(&key, &limits).is_ok());
    }

    // === Invalid Keys ===

    #[test]
    fn test_invalid_empty_key() {
        let result = validate_key("");
        assert!(matches!(result, Err(KeyError::Empty)));
    }

    #[test]
    fn test_invalid_nul_byte() {
        let result = validate_key("a\x00b");
        assert!(matches!(result, Err(KeyError::ContainsNul)));
    }

    #[test]
    fn test_invalid_nul_at_start() {
        let result = validate_key("\x00abc");
        assert!(matches!(result, Err(KeyError::ContainsNul)));
    }

    #[test]
    fn test_invalid_nul_at_end() {
        let result = validate_key("abc\x00");
        assert!(matches!(result, Err(KeyError::ContainsNul)));
    }

    #[test]
    fn test_invalid_only_nul() {
        let result = validate_key("\x00");
        assert!(matches!(result, Err(KeyError::ContainsNul)));
    }

    #[test]
    fn test_invalid_reserved_prefix() {
        let result = validate_key("_strata/foo");
        assert!(matches!(result, Err(KeyError::ReservedPrefix)));
    }

    #[test]
    fn test_invalid_reserved_prefix_exact() {
        let result = validate_key("_strata/");
        assert!(matches!(result, Err(KeyError::ReservedPrefix)));
    }

    #[test]
    fn test_invalid_reserved_prefix_nested() {
        let result = validate_key("_strata/system/config");
        assert!(matches!(result, Err(KeyError::ReservedPrefix)));
    }

    #[test]
    fn test_invalid_too_long() {
        let limits = Limits::default();
        let key = "x".repeat(limits.max_key_bytes + 1);
        let result = validate_key_with_limits(&key, &limits);
        assert!(matches!(result, Err(KeyError::TooLong { .. })));
    }

    #[test]
    fn test_invalid_much_too_long() {
        let key = "x".repeat(10 * 1024); // 10KB
        let result = validate_key(&key);
        assert!(matches!(result, Err(KeyError::TooLong { .. })));
    }

    // === With Custom Limits ===

    #[test]
    fn test_key_with_custom_max_length() {
        let limits = Limits {
            max_key_bytes: 10,
            ..Limits::default()
        };

        assert!(validate_key_with_limits("short", &limits).is_ok());
        assert!(validate_key_with_limits("exactly10!", &limits).is_ok());
        assert!(validate_key_with_limits("toolongkey!", &limits).is_err());
    }

    // === Reason Code Tests ===

    #[test]
    fn test_reason_codes() {
        assert_eq!(KeyError::Empty.reason_code(), "empty_key");
        assert_eq!(KeyError::ContainsNul.reason_code(), "contains_nul");
        assert_eq!(KeyError::ReservedPrefix.reason_code(), "reserved_prefix");
        assert_eq!(
            KeyError::TooLong {
                actual: 2000,
                max: 1024
            }
            .reason_code(),
            "key_too_long"
        );
    }

    // === Error Message Tests ===

    #[test]
    fn test_error_messages() {
        assert_eq!(KeyError::Empty.to_string(), "Key cannot be empty");
        assert_eq!(
            KeyError::ContainsNul.to_string(),
            "Key cannot contain NUL bytes"
        );
        assert_eq!(
            KeyError::ReservedPrefix.to_string(),
            "Key cannot use reserved prefix '_strata/'"
        );
        assert_eq!(
            KeyError::TooLong {
                actual: 2000,
                max: 1024
            }
            .to_string(),
            "Key too long: 2000 bytes exceeds maximum 1024"
        );
    }

    // === Reserved Prefix Constant ===

    #[test]
    fn test_reserved_prefix_constant() {
        assert_eq!(RESERVED_PREFIX, "_strata/");
    }

    // === Multi-byte UTF-8 Keys ===

    #[test]
    fn test_multibyte_key_length() {
        // "日本語" is 9 bytes in UTF-8 (3 bytes per character)
        let key = "日本語";
        assert_eq!(key.len(), 9);
        assert!(validate_key(key).is_ok());
    }

    #[test]
    fn test_multibyte_key_exceeds_limit() {
        let limits = Limits {
            max_key_bytes: 5,
            ..Limits::default()
        };

        // "日本語" is 9 bytes, exceeds limit of 5
        let result = validate_key_with_limits("日本語", &limits);
        assert!(matches!(
            result,
            Err(KeyError::TooLong { actual: 9, max: 5 })
        ));
    }
}
