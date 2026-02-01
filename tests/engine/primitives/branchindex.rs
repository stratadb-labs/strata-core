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
    let branch_index = test_db.branch_index();

    let result = branch_index.create_branch("test_branch").unwrap();
    assert_eq!(result.value.name, "test_branch");
    // Initial status is Active
    assert_eq!(result.value.status, BranchStatus::Active);
}

#[test]
fn create_branch_duplicate_fails() {
    let test_db = TestDb::new();
    let branch_index = test_db.branch_index();

    branch_index.create_branch("test_branch").unwrap();

    let result = branch_index.create_branch("test_branch");
    assert!(result.is_err());
}

#[test]
fn get_branch() {
    let test_db = TestDb::new();
    let branch_index = test_db.branch_index();

    branch_index.create_branch("test_branch").unwrap();

    let result = branch_index.get_branch("test_branch").unwrap();
    assert!(result.is_some());
    assert_eq!(result.unwrap().value.name, "test_branch");
}

#[test]
fn get_nonexistent_returns_none() {
    let test_db = TestDb::new();
    let branch_index = test_db.branch_index();

    let result = branch_index.get_branch("nonexistent").unwrap();
    assert!(result.is_none());
}

#[test]
fn exists_returns_correct_status() {
    let test_db = TestDb::new();
    let branch_index = test_db.branch_index();

    assert!(!branch_index.exists("test_branch").unwrap());

    branch_index.create_branch("test_branch").unwrap();
    assert!(branch_index.exists("test_branch").unwrap());
}

#[test]
fn list_branches() {
    let test_db = TestDb::new();
    let branch_index = test_db.branch_index();

    branch_index.create_branch("branch_a").unwrap();
    branch_index.create_branch("branch_b").unwrap();
    branch_index.create_branch("branch_c").unwrap();

    let branches = branch_index.list_branches().unwrap();
    assert_eq!(branches.len(), 3);
    assert!(branches.contains(&"branch_a".to_string()));
    assert!(branches.contains(&"branch_b".to_string()));
    assert!(branches.contains(&"branch_c".to_string()));
}

#[test]
fn count_branches() {
    let test_db = TestDb::new();
    let branch_index = test_db.branch_index();

    // count rewritten as list_branches().len()
    assert_eq!(branch_index.list_branches().unwrap().len(), 0);

    branch_index.create_branch("branch_a").unwrap();
    branch_index.create_branch("branch_b").unwrap();

    assert_eq!(branch_index.list_branches().unwrap().len(), 2);
}

#[test]
fn delete_branch() {
    let test_db = TestDb::new();
    let branch_index = test_db.branch_index();

    branch_index.create_branch("test_branch").unwrap();
    assert!(branch_index.exists("test_branch").unwrap());

    branch_index.delete_branch("test_branch").unwrap();
    assert!(!branch_index.exists("test_branch").unwrap());
}

// ============================================================================
// Edge Cases
// ============================================================================

#[test]
fn empty_branch_name() {
    let test_db = TestDb::new();
    let branch_index = test_db.branch_index();

    // Empty name should work
    branch_index.create_branch("").unwrap();
    assert!(branch_index.exists("").unwrap());
}

#[test]
fn special_characters_in_name() {
    let test_db = TestDb::new();
    let branch_index = test_db.branch_index();

    let name = "branch/with:special@chars";
    branch_index.create_branch(name).unwrap();
    assert!(branch_index.exists(name).unwrap());
}
