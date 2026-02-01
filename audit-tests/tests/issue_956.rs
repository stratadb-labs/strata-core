//! Audit test for issue #956: BranchGet returns consistent Output variant
//! Verdict: FIXED
//!
//! In handlers/branch.rs, `branch_get` now returns:
//! - `Output::MaybeBranchInfo(Some(...))` when the branch is found
//! - `Output::MaybeBranchInfo(None)` when the branch is not found
//!
//! This is consistent: a single command returns a single Output variant
//! that can represent both found and not-found states.

use strata_engine::database::Database;
use strata_executor::{BranchId, Command, Executor, Output};

/// BranchGet returns MaybeBranchInfo(Some(...)) for explicitly created branches.
#[test]
fn issue_956_branch_get_existing_returns_maybe_branch_info_some() {
    let db = Database::cache().unwrap();
    let executor = Executor::new(db);

    // Create a branch explicitly so it has metadata
    let create_result = executor
        .execute(Command::BranchCreate {
            branch_id: Some("test-branch".into()),
            metadata: None,
        })
        .unwrap();
    assert!(
        matches!(create_result, Output::BranchWithVersion { .. }),
        "BranchCreate should succeed"
    );

    // Get the explicitly created branch
    let existing = executor
        .execute(Command::BranchGet {
            branch: BranchId::from("test-branch"),
        })
        .unwrap();

    assert!(
        matches!(existing, Output::MaybeBranchInfo(Some(_))),
        "Existing branch returns MaybeBranchInfo(Some(...)). Got: {:?}",
        existing
    );
}

/// BranchGet returns MaybeBranchInfo(None) for non-existent branches.
#[test]
fn issue_956_branch_get_missing_returns_maybe_branch_info_none() {
    let db = Database::cache().unwrap();
    let executor = Executor::new(db);

    // Get non-existent branch
    let missing = executor
        .execute(Command::BranchGet {
            branch: BranchId::from("nonexistent"),
        })
        .unwrap();

    assert!(
        matches!(missing, Output::MaybeBranchInfo(None)),
        "Missing branch returns MaybeBranchInfo(None). Got: {:?}",
        missing
    );
}

/// Both found and not-found now use the same Output variant.
#[test]
fn issue_956_branch_get_consistent_output_variants() {
    let db = Database::cache().unwrap();
    let executor = Executor::new(db);

    // Create a branch explicitly
    executor
        .execute(Command::BranchCreate {
            branch_id: Some("my-branch".into()),
            metadata: None,
        })
        .unwrap();

    let existing = executor
        .execute(Command::BranchGet {
            branch: BranchId::from("my-branch"),
        })
        .unwrap();

    let missing = executor
        .execute(Command::BranchGet {
            branch: BranchId::from("does_not_exist"),
        })
        .unwrap();

    // Both results from the SAME command now use the SAME Output variant
    let existing_is_maybe_branch_info = matches!(existing, Output::MaybeBranchInfo(Some(_)));
    let missing_is_maybe_branch_info = matches!(missing, Output::MaybeBranchInfo(None));

    assert!(
        existing_is_maybe_branch_info,
        "Found branch uses MaybeBranchInfo(Some(...))"
    );
    assert!(
        missing_is_maybe_branch_info,
        "Missing branch uses MaybeBranchInfo(None)"
    );

    // FIXED: Both cases now use the same variant discriminant
    assert_eq!(
        std::mem::discriminant(&existing),
        std::mem::discriminant(&missing),
        "Same command now returns same Output variant discriminant"
    );
}

/// Note: the "default" branch exists implicitly for data operations but does NOT
/// have formal BranchMetadata in the engine's branch index for cache databases.
/// BranchGet for "default" on a cache DB returns MaybeBranchInfo(None), the
/// same as for a truly non-existent branch.
#[test]
fn issue_956_default_branch_has_no_metadata_in_cache_db() {
    let db = Database::cache().unwrap();
    let executor = Executor::new(db);

    let result = executor
        .execute(Command::BranchGet {
            branch: BranchId::from("default"),
        })
        .unwrap();

    // The "default" branch is usable for data operations (KvPut, etc.)
    // but BranchGet returns MaybeBranchInfo(None) because there is no explicit
    // BranchMetadata record.
    assert!(
        matches!(result, Output::MaybeBranchInfo(None)),
        "Default branch returns MaybeBranchInfo(None) from BranchGet in cache DB. Got: {:?}",
        result
    );
}
