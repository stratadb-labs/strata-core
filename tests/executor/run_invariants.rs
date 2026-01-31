//! Deep Run Invariant Tests
//!
//! These tests verify deeper invariants about run behavior, not just API correctness.

use crate::common::*;
use strata_core::Value;
use strata_executor::{Command, Output, BranchId};

// ============================================================================
// Run Isolation
// ============================================================================

/// Data in one run must be completely invisible from another run
#[test]
fn run_data_is_isolated() {
    let executor = create_executor();

    // Create two runs
    let branch_a = match executor.execute(Command::BranchCreate {
        branch_id: Some("isolation-run-a".into()),
        metadata: None,
    }).unwrap() {
        Output::BranchWithVersion { info, .. } => info.id,
        _ => panic!("Expected BranchWithVersion"),
    };

    let branch_b = match executor.execute(Command::BranchCreate {
        branch_id: Some("isolation-run-b".into()),
        metadata: None,
    }).unwrap() {
        Output::BranchWithVersion { info, .. } => info.id,
        _ => panic!("Expected BranchWithVersion"),
    };

    // Write data to run A
    executor.execute(Command::KvPut {
        run: Some(branch_a.clone()),
        key: "secret".into(),
        value: Value::String("run_a_secret".into()),
    }).unwrap();

    executor.execute(Command::StateSet {
        run: Some(branch_a.clone()),
        cell: "state".into(),
        value: Value::Int(42),
    }).unwrap();

    // Run B should NOT see run A's data
    let output = executor.execute(Command::KvGet {
        run: Some(branch_b.clone()),
        key: "secret".into(),
    }).unwrap();
    assert!(matches!(output, Output::Maybe(None)),
        "Run B should not see Run A's KV data");

    let output = executor.execute(Command::StateRead {
        run: Some(branch_b.clone()),
        cell: "state".into(),
    }).unwrap();
    assert!(matches!(output, Output::Maybe(None)),
        "Run B should not see Run A's state data");

    // Run A should still see its own data
    let output = executor.execute(Command::KvGet {
        run: Some(branch_a.clone()),
        key: "secret".into(),
    }).unwrap();
    match output {
        Output::Maybe(Some(val)) => {
            assert_eq!(val, Value::String("run_a_secret".into()));
        }
        _ => panic!("Run A should see its own data"),
    }
}

// ============================================================================
// Delete Removes All Data
// ============================================================================

/// Deleting a run should remove all its data (KV, State, Events)
#[test]
fn run_delete_removes_all_data() {
    let executor = create_executor();

    let branch_id = match executor.execute(Command::BranchCreate {
        branch_id: Some("delete-data-run".into()),
        metadata: None,
    }).unwrap() {
        Output::BranchWithVersion { info, .. } => info.id,
        _ => panic!("Expected BranchWithVersion"),
    };

    // Add data to the run
    executor.execute(Command::KvPut {
        run: Some(branch_id.clone()),
        key: "key1".into(),
        value: Value::String("value1".into()),
    }).unwrap();

    executor.execute(Command::KvPut {
        run: Some(branch_id.clone()),
        key: "key2".into(),
        value: Value::Int(123),
    }).unwrap();

    executor.execute(Command::StateSet {
        run: Some(branch_id.clone()),
        cell: "cell1".into(),
        value: Value::Bool(true),
    }).unwrap();

    // Verify data exists
    let output = executor.execute(Command::KvGet {
        run: Some(branch_id.clone()),
        key: "key1".into(),
    }).unwrap();
    assert!(matches!(output, Output::Maybe(Some(_))));

    // Delete the run
    executor.execute(Command::BranchDelete {
        run: branch_id.clone(),
    }).unwrap();

    // Run should not exist
    let output = executor.execute(Command::BranchExists {
        run: branch_id.clone(),
    }).unwrap();
    assert!(matches!(output, Output::Bool(false)));

    // Data should be gone - but we can't easily test this since the run
    // doesn't exist anymore. Create a new run with the same name and verify
    // data doesn't persist.
    executor.execute(Command::BranchCreate {
        branch_id: Some("delete-data-run".into()),
        metadata: None,
    }).unwrap();

    let output = executor.execute(Command::KvGet {
        run: Some(branch_id.clone()),
        key: "key1".into(),
    }).unwrap();
    assert!(matches!(output, Output::Maybe(None)),
        "Data should not persist after run deletion and recreation");
}

/// Verify run delete properly cleans up data (issue #781 fixed)
#[test]
fn run_delete_cleans_up_data() {
    let executor = create_executor();

    let branch_id = match executor.execute(Command::BranchCreate {
        branch_id: Some("delete-keeps-data".into()),
        metadata: None,
    }).unwrap() {
        Output::BranchWithVersion { info, .. } => info.id,
        _ => panic!("Expected BranchWithVersion"),
    };

    // Add data
    executor.execute(Command::KvPut {
        run: Some(branch_id.clone()),
        key: "persistent_key".into(),
        value: Value::String("should_be_deleted".into()),
    }).unwrap();

    // Delete run
    executor.execute(Command::BranchDelete {
        run: branch_id.clone(),
    }).unwrap();

    // Recreate run with same name
    executor.execute(Command::BranchCreate {
        branch_id: Some("delete-keeps-data".into()),
        metadata: None,
    }).unwrap();

    // Data should be gone after deletion
    let output = executor.execute(Command::KvGet {
        run: Some(branch_id),
        key: "persistent_key".into(),
    }).unwrap();

    assert!(matches!(output, Output::Maybe(None)),
        "Data should not persist after run deletion (issue #781)");
}

// ============================================================================
// Default Run Behavior
// ============================================================================

/// Default run always exists and can be used
#[test]
fn default_run_always_works() {
    let executor = create_executor();

    // Write to default run (run: None)
    executor.execute(Command::KvPut {
        run: None,
        key: "default_key".into(),
        value: Value::String("default_value".into()),
    }).unwrap();

    // Read from default run
    let output = executor.execute(Command::KvGet {
        run: None,
        key: "default_key".into(),
    }).unwrap();
    match output {
        Output::Maybe(Some(val)) => {
            assert_eq!(val, Value::String("default_value".into()));
        }
        _ => panic!("Default run should work"),
    }

    // Explicit "default" run should be equivalent
    let output = executor.execute(Command::KvGet {
        run: Some(BranchId::from("default")),
        key: "default_key".into(),
    }).unwrap();
    match output {
        Output::Maybe(Some(val)) => {
            assert_eq!(val, Value::String("default_value".into()));
        }
        _ => panic!("Explicit 'default' run should work"),
    }
}
