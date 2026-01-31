//! BranchIndex Primitive Tests
//!
//! Tests for branch lifecycle management.

use crate::common::*;
use strata_engine::BranchStatus;

// ============================================================================
// Basic CRUD
// ============================================================================

#[test]
fn create_branch() {
    let test_db = TestDb::new();
    let run_idx = test_db.run_index();

    let result = run_idx.create_branch("test_run").unwrap();
    assert_eq!(result.value.name, "test_run");
    // Initial status is Active
    assert_eq!(result.value.status, BranchStatus::Active);
}

#[test]
fn create_run_duplicate_fails() {
    let test_db = TestDb::new();
    let run_idx = test_db.run_index();

    run_idx.create_branch("test_run").unwrap();

    let result = run_idx.create_branch("test_run");
    assert!(result.is_err());
}

#[test]
fn get_run() {
    let test_db = TestDb::new();
    let run_idx = test_db.run_index();

    run_idx.create_branch("test_run").unwrap();

    let result = run_idx.get_branch("test_run").unwrap();
    assert!(result.is_some());
    assert_eq!(result.unwrap().value.name, "test_run");
}

#[test]
fn get_nonexistent_returns_none() {
    let test_db = TestDb::new();
    let run_idx = test_db.run_index();

    let result = run_idx.get_branch("nonexistent").unwrap();
    assert!(result.is_none());
}

#[test]
fn exists_returns_correct_status() {
    let test_db = TestDb::new();
    let run_idx = test_db.run_index();

    assert!(!run_idx.exists("test_run").unwrap());

    run_idx.create_branch("test_run").unwrap();
    assert!(run_idx.exists("test_run").unwrap());
}

#[test]
fn list_branches() {
    let test_db = TestDb::new();
    let run_idx = test_db.run_index();

    run_idx.create_branch("branch_a").unwrap();
    run_idx.create_branch("branch_b").unwrap();
    run_idx.create_branch("run_c").unwrap();

    let runs = run_idx.list_branches().unwrap();
    assert_eq!(runs.len(), 3);
    assert!(runs.contains(&"branch_a".to_string()));
    assert!(runs.contains(&"branch_b".to_string()));
    assert!(runs.contains(&"run_c".to_string()));
}

#[test]
fn count_runs() {
    let test_db = TestDb::new();
    let run_idx = test_db.run_index();

    // count rewritten as list_branches().len()
    assert_eq!(run_idx.list_branches().unwrap().len(), 0);

    run_idx.create_branch("branch_a").unwrap();
    run_idx.create_branch("branch_b").unwrap();

    assert_eq!(run_idx.list_branches().unwrap().len(), 2);
}

#[test]
fn delete_branch() {
    let test_db = TestDb::new();
    let run_idx = test_db.run_index();

    run_idx.create_branch("test_run").unwrap();
    assert!(run_idx.exists("test_run").unwrap());

    run_idx.delete_branch("test_run").unwrap();
    assert!(!run_idx.exists("test_run").unwrap());
}

// ============================================================================
// Status Transitions
// ============================================================================

#[test]
#[ignore = "requires: BranchIndex lifecycle methods"]
fn complete_run() {
    let _test_db = TestDb::new();
    // Status transitions are post-MVP
}

#[test]
#[ignore = "requires: BranchIndex lifecycle methods"]
fn fail_run() {
    let _test_db = TestDb::new();
    // Status transitions are post-MVP
}

#[test]
#[ignore = "requires: BranchIndex lifecycle methods"]
fn cancel_run() {
    let _test_db = TestDb::new();
    // Status transitions are post-MVP
}

#[test]
#[ignore = "requires: BranchIndex lifecycle methods"]
fn pause_and_resume_run() {
    let _test_db = TestDb::new();
    // Status transitions are post-MVP
}

#[test]
#[ignore = "requires: BranchIndex lifecycle methods"]
fn archive_completed_run() {
    let _test_db = TestDb::new();
    // Status transitions are post-MVP
}

#[test]
#[ignore = "requires: BranchIndex lifecycle methods"]
fn terminal_states_cannot_transition_to_active() {
    let _test_db = TestDb::new();
    // Status transitions are post-MVP
}

// ============================================================================
// Tags
// ============================================================================

#[test]
#[ignore = "requires: BranchIndex lifecycle methods"]
fn add_tags() {
    let _test_db = TestDb::new();
    // Tag management is post-MVP
}

#[test]
#[ignore = "requires: BranchIndex lifecycle methods"]
fn remove_tags() {
    let _test_db = TestDb::new();
    // Tag management is post-MVP
}

#[test]
#[ignore = "requires: BranchIndex lifecycle methods"]
fn query_by_tag() {
    let _test_db = TestDb::new();
    // Tag management is post-MVP
}

// ============================================================================
// Metadata
// ============================================================================

#[test]
#[ignore = "requires: BranchIndex lifecycle methods"]
fn update_metadata() {
    let _test_db = TestDb::new();
    // Metadata update is post-MVP
}

// ============================================================================
// Query by Status
// ============================================================================

#[test]
#[ignore = "requires: BranchIndex lifecycle methods"]
fn query_by_status() {
    let _test_db = TestDb::new();
    // Status query is post-MVP
}

// ============================================================================
// Run Status State Machine
// ============================================================================

#[test]
#[ignore = "requires: BranchStatus state machine methods"]
fn status_is_terminal_check() {
    // BranchStatus::is_terminal() does not exist in MVP
}

#[test]
#[ignore = "requires: BranchStatus state machine methods"]
fn status_is_finished_check() {
    // BranchStatus::is_finished() does not exist in MVP
}

#[test]
#[ignore = "requires: BranchStatus state machine methods"]
fn status_can_transition_to() {
    // BranchStatus::can_transition_to() does not exist in MVP
}

// ============================================================================
// Edge Cases
// ============================================================================

#[test]
fn empty_run_name() {
    let test_db = TestDb::new();
    let run_idx = test_db.run_index();

    // Empty name should work
    run_idx.create_branch("").unwrap();
    assert!(run_idx.exists("").unwrap());
}

#[test]
fn special_characters_in_name() {
    let test_db = TestDb::new();
    let run_idx = test_db.run_index();

    let name = "run/with:special@chars";
    run_idx.create_branch(name).unwrap();
    assert!(run_idx.exists(name).unwrap());
}
