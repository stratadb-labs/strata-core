//! ISSUE-020: Buffered Default Parameters Hardcoded
//!
//! **Severity**: MEDIUM
//! **Location**: `/crates/engine/src/durability/buffered.rs`
//!
//! **Problem**: Default flush interval (100ms, 1000 writes) is hardcoded.
//! Users must use `buffered_with()` for customization.
//!
//! **Impact**: Limited configurability.

use crate::test_utils::*;

/// Test default buffered parameters.
#[test]
fn test_default_buffered_params() {
    let test_db = TestDb::new();

    // Default should be 100ms flush interval, 1000 write threshold
    // When ISSUE-020 is addressed:
    // - Document default parameters clearly
    // - Or make them configurable via buffered_with()
}

/// Test custom buffered parameters.
#[test]
fn test_custom_buffered_params() {
    // When buffered_with() is properly documented:
    // let db = Database::builder()
    //     .buffered_with(Duration::from_millis(50), 500)
    //     .open()?;
}
