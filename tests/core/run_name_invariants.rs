//! RunName Invariant Tests
//!
//! Tests that RunName correctly expresses Invariant 5: Everything is Run-Scoped
//!
//! Every entity belongs to exactly one run. Runs have semantic names for users.

use strata_core::{RunName, RunNameError, MAX_RUN_NAME_LENGTH};
use std::collections::HashSet;

// ============================================================================
// Validation rules enforced
// ============================================================================

#[test]
fn run_name_rejects_empty() {
    let result = RunName::new("".to_string());
    assert!(result.is_err());
    assert!(matches!(result.unwrap_err(), RunNameError::Empty));
}

#[test]
fn run_name_rejects_too_long() {
    let long_name = "a".repeat(MAX_RUN_NAME_LENGTH + 1);
    let result = RunName::new(long_name);
    assert!(result.is_err());
    assert!(matches!(result.unwrap_err(), RunNameError::TooLong { .. }));
}

#[test]
fn run_name_accepts_max_length() {
    let max_name = "a".repeat(MAX_RUN_NAME_LENGTH);
    let result = RunName::new(max_name);
    assert!(result.is_ok());
}

#[test]
fn run_name_rejects_invalid_chars_space() {
    let result = RunName::new("hello world".to_string());
    assert!(result.is_err());
    assert!(matches!(result.unwrap_err(), RunNameError::InvalidChar { .. }));
}

#[test]
fn run_name_rejects_invalid_chars_special() {
    let invalid_chars = ['@', '#', '$', '%', '^', '&', '*', '!', '?', '/'];
    for ch in invalid_chars {
        let name = format!("test{}name", ch);
        let result = RunName::new(name);
        assert!(result.is_err(), "Should reject char: {}", ch);
    }
}

#[test]
fn run_name_rejects_invalid_start_dash() {
    let result = RunName::new("-invalid".to_string());
    assert!(result.is_err());
    assert!(matches!(result.unwrap_err(), RunNameError::InvalidStart { .. }));
}

#[test]
fn run_name_rejects_invalid_start_dot() {
    let result = RunName::new(".invalid".to_string());
    assert!(result.is_err());
    assert!(matches!(result.unwrap_err(), RunNameError::InvalidStart { .. }));
}

// ============================================================================
// Valid names accepted
// ============================================================================

#[test]
fn run_name_accepts_alphanumeric() {
    let names = ["test", "Test123", "abc", "ABC", "a1b2c3"];
    for name in names {
        let result = RunName::new(name.to_string());
        assert!(result.is_ok(), "Should accept: {}", name);
    }
}

#[test]
fn run_name_accepts_underscores() {
    let names = ["test_name", "_test", "test_", "_test_name_"];
    for name in names {
        let result = RunName::new(name.to_string());
        assert!(result.is_ok(), "Should accept: {}", name);
    }
}

#[test]
fn run_name_accepts_dots() {
    // Dots are allowed in the middle
    let names = ["test.name", "my.run.name", "v1.0.0"];
    for name in names {
        let result = RunName::new(name.to_string());
        assert!(result.is_ok(), "Should accept: {}", name);
    }
}

#[test]
fn run_name_accepts_hyphens() {
    // Hyphens are allowed in the middle
    let names = ["test-name", "my-run-name", "v1-0-0"];
    for name in names {
        let result = RunName::new(name.to_string());
        assert!(result.is_ok(), "Should accept: {}", name);
    }
}

#[test]
fn run_name_accepts_mixed_valid_chars() {
    let names = [
        "my_test.run-1",
        "agent_v2.0-beta",
        "production_run.2024-01-15",
    ];
    for name in names {
        let result = RunName::new(name.to_string());
        assert!(result.is_ok(), "Should accept: {}", name);
    }
}

#[test]
fn run_name_accepts_single_char() {
    let names = ["a", "Z", "0", "_"];
    for name in names {
        let result = RunName::new(name.to_string());
        assert!(result.is_ok(), "Should accept single char: {}", name);
    }
}

// ============================================================================
// RunName accessors
// ============================================================================

#[test]
fn run_name_as_str() {
    let name = RunName::new("my_run".to_string()).unwrap();
    assert_eq!(name.as_str(), "my_run");
}

#[test]
fn run_name_into_inner() {
    let name = RunName::new("my_run".to_string()).unwrap();
    let inner: String = name.into_inner();
    assert_eq!(inner, "my_run");
}

// ============================================================================
// RunName pattern matching
// ============================================================================

#[test]
fn run_name_starts_with() {
    let name = RunName::new("test_run_123".to_string()).unwrap();
    assert!(name.starts_with("test"));
    assert!(name.starts_with("test_"));
    assert!(!name.starts_with("run"));
}

#[test]
fn run_name_ends_with() {
    let name = RunName::new("test_run_123".to_string()).unwrap();
    assert!(name.ends_with("123"));
    assert!(name.ends_with("_123"));
    assert!(!name.ends_with("test"));
}

#[test]
fn run_name_contains() {
    let name = RunName::new("test_run_123".to_string()).unwrap();
    assert!(name.contains("run"));
    assert!(name.contains("_"));
    assert!(!name.contains("xyz"));
}

// ============================================================================
// RunName equality
// ============================================================================

#[test]
fn run_name_equality() {
    let name1 = RunName::new("test".to_string()).unwrap();
    let name2 = RunName::new("test".to_string()).unwrap();
    let name3 = RunName::new("other".to_string()).unwrap();

    assert_eq!(name1, name2);
    assert_ne!(name1, name3);
}

#[test]
fn run_name_case_sensitive() {
    let lower = RunName::new("test".to_string()).unwrap();
    let upper = RunName::new("TEST".to_string()).unwrap();
    let mixed = RunName::new("Test".to_string()).unwrap();

    assert_ne!(lower, upper);
    assert_ne!(lower, mixed);
    assert_ne!(upper, mixed);
}

// ============================================================================
// RunName hashable
// ============================================================================

#[test]
fn run_name_hashable() {
    let mut set = HashSet::new();

    set.insert(RunName::new("test1".to_string()).unwrap());
    set.insert(RunName::new("test2".to_string()).unwrap());
    set.insert(RunName::new("test1".to_string()).unwrap()); // Duplicate

    assert_eq!(set.len(), 2);
}

// ============================================================================
// RunName serialization
// ============================================================================

#[test]
fn run_name_serialization_roundtrip() {
    let name = RunName::new("my_test_run.v1".to_string()).unwrap();
    let json = serde_json::to_string(&name).expect("serialize");
    let parsed: RunName = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(name, parsed);
}

// ============================================================================
// RunName Display
// ============================================================================

#[test]
fn run_name_display() {
    let name = RunName::new("my_run".to_string()).unwrap();
    let display = format!("{}", name);
    assert_eq!(display, "my_run");
}

// ============================================================================
// RunName TryFrom conversions
// ============================================================================

#[test]
fn run_name_try_from_string() {
    let result: Result<RunName, _> = "valid_name".to_string().try_into();
    assert!(result.is_ok());
    assert_eq!(result.unwrap().as_str(), "valid_name");
}

#[test]
fn run_name_try_from_str() {
    let result: Result<RunName, _> = "valid_name".try_into();
    assert!(result.is_ok());
    assert_eq!(result.unwrap().as_str(), "valid_name");
}

#[test]
fn run_name_try_from_invalid() {
    let result: Result<RunName, _> = "".to_string().try_into();
    assert!(result.is_err());
}

// ============================================================================
// RunNameError Display
// ============================================================================

#[test]
fn run_name_error_display_empty() {
    let err = RunNameError::Empty;
    let display = format!("{}", err);
    assert!(display.contains("empty") || display.len() > 0);
}

#[test]
fn run_name_error_display_too_long() {
    let err = RunNameError::TooLong { length: 300, max: 256 };
    let display = format!("{}", err);
    assert!(display.contains("300") || display.contains("256") || display.len() > 0);
}

#[test]
fn run_name_error_display_invalid_char() {
    let err = RunNameError::InvalidChar { char: '@', position: 5 };
    let display = format!("{}", err);
    assert!(display.len() > 0);
}

#[test]
fn run_name_error_display_invalid_start() {
    let err = RunNameError::InvalidStart { char: '-' };
    let display = format!("{}", err);
    assert!(display.len() > 0);
}

// ============================================================================
// MAX_RUN_NAME_LENGTH constant
// ============================================================================

#[test]
fn max_run_name_length_is_reasonable() {
    // Should be long enough for practical names but bounded
    assert!(MAX_RUN_NAME_LENGTH >= 64);
    assert!(MAX_RUN_NAME_LENGTH <= 512);
}

// ============================================================================
// RunName Clone
// ============================================================================

#[test]
fn run_name_clone() {
    let name1 = RunName::new("test".to_string()).unwrap();
    let name2 = name1.clone();
    assert_eq!(name1, name2);
}

// ============================================================================
// Edge cases
// ============================================================================

#[test]
fn run_name_unicode_rejected() {
    // Unicode characters should be rejected
    let names = ["tëst", "名前", "тест", "🚀test"];
    for name in names {
        let result = RunName::new(name.to_string());
        assert!(result.is_err(), "Should reject unicode: {}", name);
    }
}

#[test]
fn run_name_numeric_start_allowed() {
    let result = RunName::new("123test".to_string());
    assert!(result.is_ok());
}

#[test]
fn run_name_all_digits_allowed() {
    let result = RunName::new("12345".to_string());
    assert!(result.is_ok());
}
