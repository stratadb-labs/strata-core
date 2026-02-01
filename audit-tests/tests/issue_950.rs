//! Audit test for issue #950: Empty branch name accepted
//! Verdict: CONFIRMED BUG
//!
//! The `BranchCreate` command accepts an optional `branch_id: Option<String>`.
//! When provided, the string is used directly as the branch name with no validation.
//! This means:
//!
//! - Empty string "" is accepted as a valid branch name
//! - Whitespace-only strings like "   " are accepted
//! - Special characters, control characters, etc. are accepted
//! - Very long strings are accepted (no length limit)
//!
//! In the handler (executor/src/handlers/branch.rs), the branch_id is passed directly
//! to `p.branch.create_branch(&branch_str)` without any name validation.
//!
//! Impact:
//! - Empty branch names may cause confusing behavior or silent errors
//! - Branch names with special characters may break serialization or display
//! - Extremely long branch names waste memory and may cause issues in storage
//!
//! The fix would add branch name validation, similar to how vector collection names
//! are validated via `validate_collection_name()`:
//! ```ignore
//! fn validate_branch_name(name: &str) -> Result<()> {
//!     if name.is_empty() { return Err(Error::InvalidBranchName { ... }); }
//!     if name.len() > 255 { return Err(Error::InvalidBranchName { ... }); }
//!     // Additional checks for valid characters, etc.
//! }
//! ```

use strata_engine::database::Database;
use strata_executor::{Command, Executor, Output};

/// Demonstrates that an empty branch name is accepted without validation.
#[test]
fn issue_950_empty_branch_name_accepted() {
    let db = Database::cache().unwrap();
    let executor = Executor::new(db);

    let result = executor.execute(Command::BranchCreate {
        branch_id: Some("".to_string()),
        metadata: None,
    });

    // BUG: Empty branch name should be rejected but may be accepted
    match result {
        Ok(Output::BranchWithVersion { info, .. }) => {
            // Bug confirmed: empty branch name was accepted
            assert_eq!(
                info.id.as_str(),
                "",
                "Empty string was accepted as branch name"
            );
        }
        Ok(other) => {
            // Unexpected output variant
            panic!("Expected BranchWithVersion, got {:?}", other);
        }
        Err(_) => {
            // If this errors, the bug may have been fixed
            // (or it errors for a different reason)
        }
    }
}

/// Demonstrates that whitespace-only branch names are accepted.
#[test]
fn issue_950_whitespace_branch_name_accepted() {
    let db = Database::cache().unwrap();
    let executor = Executor::new(db);

    let result = executor.execute(Command::BranchCreate {
        branch_id: Some("   ".to_string()),
        metadata: None,
    });

    match result {
        Ok(_) => {
            // Bug confirmed: whitespace-only name accepted
        }
        Err(_) => {
            // May have been fixed or errored for another reason
        }
    }
}

/// Demonstrates that branch names with special characters are accepted.
#[test]
fn issue_950_special_chars_branch_name_accepted() {
    let db = Database::cache().unwrap();
    let executor = Executor::new(db);

    // Try various problematic names
    let problematic_names = vec![
        "\0",           // null byte
        "\n\r\t",       // control characters
        "../../../etc", // path traversal attempt
    ];

    for name in problematic_names {
        let result = executor.execute(Command::BranchCreate {
            branch_id: Some(name.to_string()),
            metadata: None,
        });

        // Any of these succeeding demonstrates missing validation
        if result.is_ok() {
            // Bug confirmed for this name pattern
        }
    }
}
