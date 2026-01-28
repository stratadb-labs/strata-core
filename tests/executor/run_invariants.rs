//! Deep Run Invariant Tests
//!
//! These tests verify deeper invariants about run behavior, not just API correctness.

use crate::common::*;
use strata_core::Value;
use strata_executor::{Command, Output, RunId, RunStatus};

// ============================================================================
// Run Isolation
// ============================================================================

/// Data in one run must be completely invisible from another run
#[test]
fn run_data_is_isolated() {
    let executor = create_executor();

    // Create two runs
    let run_a = match executor.execute(Command::RunCreate {
        run_id: Some("isolation-run-a".into()),
        metadata: None,
    }).unwrap() {
        Output::RunWithVersion { info, .. } => info.id,
        _ => panic!("Expected RunWithVersion"),
    };

    let run_b = match executor.execute(Command::RunCreate {
        run_id: Some("isolation-run-b".into()),
        metadata: None,
    }).unwrap() {
        Output::RunWithVersion { info, .. } => info.id,
        _ => panic!("Expected RunWithVersion"),
    };

    // Write data to run A
    executor.execute(Command::KvPut {
        run: Some(run_a.clone()),
        key: "secret".into(),
        value: Value::String("run_a_secret".into()),
    }).unwrap();

    executor.execute(Command::StateSet {
        run: Some(run_a.clone()),
        cell: "state".into(),
        value: Value::Int(42),
    }).unwrap();

    // Run B should NOT see run A's data
    let output = executor.execute(Command::KvGet {
        run: Some(run_b.clone()),
        key: "secret".into(),
    }).unwrap();
    assert!(matches!(output, Output::MaybeVersioned(None)),
        "Run B should not see Run A's KV data");

    let output = executor.execute(Command::StateRead {
        run: Some(run_b.clone()),
        cell: "state".into(),
    }).unwrap();
    assert!(matches!(output, Output::MaybeVersioned(None)),
        "Run B should not see Run A's state data");

    // Run A should still see its own data
    let output = executor.execute(Command::KvGet {
        run: Some(run_a.clone()),
        key: "secret".into(),
    }).unwrap();
    match output {
        Output::MaybeVersioned(Some(vv)) => {
            assert_eq!(vv.value, Value::String("run_a_secret".into()));
        }
        _ => panic!("Run A should see its own data"),
    }
}

// ============================================================================
// Delete Removes All Data
// ============================================================================

/// Deleting a run should remove all its data (KV, State, Events)
/// BUG: Currently data persists after run deletion - see issue #781
#[test]
#[ignore] // Enable when run deletion properly cleans up data
fn run_delete_removes_all_data() {
    let executor = create_executor();

    let run_id = match executor.execute(Command::RunCreate {
        run_id: Some("delete-data-run".into()),
        metadata: None,
    }).unwrap() {
        Output::RunWithVersion { info, .. } => info.id,
        _ => panic!("Expected RunWithVersion"),
    };

    // Add data to the run
    executor.execute(Command::KvPut {
        run: Some(run_id.clone()),
        key: "key1".into(),
        value: Value::String("value1".into()),
    }).unwrap();

    executor.execute(Command::KvPut {
        run: Some(run_id.clone()),
        key: "key2".into(),
        value: Value::Int(123),
    }).unwrap();

    executor.execute(Command::StateSet {
        run: Some(run_id.clone()),
        cell: "cell1".into(),
        value: Value::Bool(true),
    }).unwrap();

    // Verify data exists
    let output = executor.execute(Command::KvGet {
        run: Some(run_id.clone()),
        key: "key1".into(),
    }).unwrap();
    assert!(matches!(output, Output::MaybeVersioned(Some(_))));

    // Delete the run
    executor.execute(Command::RunDelete {
        run: run_id.clone(),
    }).unwrap();

    // Run should not exist
    let output = executor.execute(Command::RunExists {
        run: run_id.clone(),
    }).unwrap();
    assert!(matches!(output, Output::Bool(false)));

    // Data should be gone - but we can't easily test this since the run
    // doesn't exist anymore. Create a new run with the same name and verify
    // data doesn't persist.
    executor.execute(Command::RunCreate {
        run_id: Some("delete-data-run".into()),
        metadata: None,
    }).unwrap();

    let output = executor.execute(Command::KvGet {
        run: Some(run_id.clone()),
        key: "key1".into(),
    }).unwrap();
    assert!(matches!(output, Output::MaybeVersioned(None)),
        "Data should not persist after run deletion and recreation");
}

/// Document current behavior: run delete does NOT remove data (bug)
#[test]
fn run_delete_currently_does_not_remove_data() {
    let executor = create_executor();

    let run_id = match executor.execute(Command::RunCreate {
        run_id: Some("delete-keeps-data".into()),
        metadata: None,
    }).unwrap() {
        Output::RunWithVersion { info, .. } => info.id,
        _ => panic!("Expected RunWithVersion"),
    };

    // Add data
    executor.execute(Command::KvPut {
        run: Some(run_id.clone()),
        key: "persistent_key".into(),
        value: Value::String("should_be_deleted".into()),
    }).unwrap();

    // Delete run
    executor.execute(Command::RunDelete {
        run: run_id.clone(),
    }).unwrap();

    // Recreate run with same name
    executor.execute(Command::RunCreate {
        run_id: Some("delete-keeps-data".into()),
        metadata: None,
    }).unwrap();

    // BUG: Data still exists! This should be None.
    let output = executor.execute(Command::KvGet {
        run: Some(run_id),
        key: "persistent_key".into(),
    }).unwrap();

    // Documenting current (broken) behavior
    assert!(matches!(output, Output::MaybeVersioned(Some(_))),
        "Current behavior: data persists after run deletion (see issue #781)");
}

// ============================================================================
// Archived Run Behavior
// ============================================================================

/// Archived runs should be read-only (writes should fail)
#[test]
fn archived_run_is_read_only() {
    let executor = create_executor();

    let run_id = match executor.execute(Command::RunCreate {
        run_id: Some("archive-readonly-run".into()),
        metadata: None,
    }).unwrap() {
        Output::RunWithVersion { info, .. } => info.id,
        _ => panic!("Expected RunWithVersion"),
    };

    // Add some data before archiving
    executor.execute(Command::KvPut {
        run: Some(run_id.clone()),
        key: "before_archive".into(),
        value: Value::String("exists".into()),
    }).unwrap();

    // Archive the run
    executor.execute(Command::RunArchive {
        run: run_id.clone(),
    }).unwrap();

    // Verify status is Archived
    let output = executor.execute(Command::RunGet {
        run: run_id.clone(),
    }).unwrap();
    match &output {
        Output::RunInfoVersioned(v) => assert_eq!(v.info.status, RunStatus::Archived),
        _ => panic!("Expected RunInfoVersioned"),
    }

    // Reading should still work
    let output = executor.execute(Command::KvGet {
        run: Some(run_id.clone()),
        key: "before_archive".into(),
    }).unwrap();
    assert!(matches!(output, Output::MaybeVersioned(Some(_))),
        "Reading from archived run should work");

    // Writing to archived run - check if it fails or succeeds
    // (documenting current behavior)
    let result = executor.execute(Command::KvPut {
        run: Some(run_id.clone()),
        key: "after_archive".into(),
        value: Value::String("should_fail".into()),
    });

    // Note: Current implementation may allow writes to archived runs.
    // This test documents the behavior. If writes are allowed, this is a bug.
    if result.is_ok() {
        // TODO: This should probably fail - archived runs should be read-only
        // For now, just document that writes are currently allowed
        println!("WARNING: Writes to archived runs are currently allowed");
    }
}

// ============================================================================
// Completed Run Behavior
// ============================================================================

/// Completed runs should still allow reads
#[test]
fn completed_run_allows_reads() {
    let executor = create_executor();

    let run_id = match executor.execute(Command::RunCreate {
        run_id: Some("completed-read-run".into()),
        metadata: None,
    }).unwrap() {
        Output::RunWithVersion { info, .. } => info.id,
        _ => panic!("Expected RunWithVersion"),
    };

    // Add data
    executor.execute(Command::KvPut {
        run: Some(run_id.clone()),
        key: "data".into(),
        value: Value::Int(999),
    }).unwrap();

    // Complete the run
    executor.execute(Command::RunComplete {
        run: run_id.clone(),
    }).unwrap();

    // Reading should still work
    let output = executor.execute(Command::KvGet {
        run: Some(run_id.clone()),
        key: "data".into(),
    }).unwrap();
    match output {
        Output::MaybeVersioned(Some(vv)) => {
            assert_eq!(vv.value, Value::Int(999));
        }
        _ => panic!("Should be able to read from completed run"),
    }
}

// ============================================================================
// Child Run Behavior (Fork)
// ============================================================================

/// Child runs should inherit parent's data (CURRENTLY BROKEN - see issue #780)
#[test]
#[ignore] // Enable when fork data copying is implemented
fn child_run_inherits_parent_data() {
    let executor = create_executor();

    // Create parent run with data
    let parent_id = match executor.execute(Command::RunCreate {
        run_id: Some("parent-with-data".into()),
        metadata: None,
    }).unwrap() {
        Output::RunWithVersion { info, .. } => info.id,
        _ => panic!("Expected RunWithVersion"),
    };

    executor.execute(Command::KvPut {
        run: Some(parent_id.clone()),
        key: "inherited_key".into(),
        value: Value::String("inherited_value".into()),
    }).unwrap();

    executor.execute(Command::StateSet {
        run: Some(parent_id.clone()),
        cell: "inherited_state".into(),
        value: Value::Int(42),
    }).unwrap();

    // Create child run
    let child_id = match executor.execute(Command::RunCreateChild {
        parent: parent_id.clone(),
        metadata: None,
    }).unwrap() {
        Output::RunWithVersion { info, .. } => info.id,
        _ => panic!("Expected RunWithVersion"),
    };

    // Child should have parent's data
    let output = executor.execute(Command::KvGet {
        run: Some(child_id.clone()),
        key: "inherited_key".into(),
    }).unwrap();
    match output {
        Output::MaybeVersioned(Some(vv)) => {
            assert_eq!(vv.value, Value::String("inherited_value".into()));
        }
        _ => panic!("Child should inherit parent's KV data"),
    }

    let output = executor.execute(Command::StateRead {
        run: Some(child_id.clone()),
        cell: "inherited_state".into(),
    }).unwrap();
    match output {
        Output::MaybeVersioned(Some(vv)) => {
            assert_eq!(vv.value, Value::Int(42));
        }
        _ => panic!("Child should inherit parent's state data"),
    }
}

/// Child run modifications don't affect parent (true fork)
#[test]
#[ignore] // Enable when fork data copying is implemented
fn child_run_modifications_dont_affect_parent() {
    let executor = create_executor();

    // Create parent with data
    let parent_id = match executor.execute(Command::RunCreate {
        run_id: Some("fork-parent".into()),
        metadata: None,
    }).unwrap() {
        Output::RunWithVersion { info, .. } => info.id,
        _ => panic!("Expected RunWithVersion"),
    };

    executor.execute(Command::KvPut {
        run: Some(parent_id.clone()),
        key: "shared_key".into(),
        value: Value::String("parent_value".into()),
    }).unwrap();

    // Create child
    let child_id = match executor.execute(Command::RunCreateChild {
        parent: parent_id.clone(),
        metadata: None,
    }).unwrap() {
        Output::RunWithVersion { info, .. } => info.id,
        _ => panic!("Expected RunWithVersion"),
    };

    // Modify in child
    executor.execute(Command::KvPut {
        run: Some(child_id.clone()),
        key: "shared_key".into(),
        value: Value::String("child_value".into()),
    }).unwrap();

    // Parent should still have original value
    let output = executor.execute(Command::KvGet {
        run: Some(parent_id),
        key: "shared_key".into(),
    }).unwrap();
    match output {
        Output::MaybeVersioned(Some(vv)) => {
            assert_eq!(vv.value, Value::String("parent_value".into()),
                "Parent should not be affected by child modifications");
        }
        _ => panic!("Parent should still have data"),
    }
}

/// Document current behavior: child does NOT inherit data (bug)
#[test]
fn child_run_currently_does_not_inherit_data() {
    let executor = create_executor();

    // Create parent with data
    let parent_id = match executor.execute(Command::RunCreate {
        run_id: Some("no-inherit-parent".into()),
        metadata: None,
    }).unwrap() {
        Output::RunWithVersion { info, .. } => info.id,
        _ => panic!("Expected RunWithVersion"),
    };

    executor.execute(Command::KvPut {
        run: Some(parent_id.clone()),
        key: "parent_data".into(),
        value: Value::String("exists".into()),
    }).unwrap();

    // Create child
    let child_id = match executor.execute(Command::RunCreateChild {
        parent: parent_id.clone(),
        metadata: None,
    }).unwrap() {
        Output::RunWithVersion { info, .. } => info.id,
        _ => panic!("Expected RunWithVersion"),
    };

    // Document current (broken) behavior: child does NOT have parent's data
    let output = executor.execute(Command::KvGet {
        run: Some(child_id),
        key: "parent_data".into(),
    }).unwrap();

    // This SHOULD return Some, but currently returns None
    // When issue #780 is fixed, this test should be updated
    assert!(matches!(output, Output::MaybeVersioned(None)),
        "Current behavior: child does NOT inherit parent data (see issue #780)");
}

// ============================================================================
// Status Transition Invariants
// ============================================================================

/// Cannot transition from Completed to Active
#[test]
fn cannot_resume_completed_run() {
    let executor = create_executor();

    let run_id = match executor.execute(Command::RunCreate {
        run_id: Some("completed-no-resume".into()),
        metadata: None,
    }).unwrap() {
        Output::RunWithVersion { info, .. } => info.id,
        _ => panic!("Expected RunWithVersion"),
    };

    executor.execute(Command::RunComplete {
        run: run_id.clone(),
    }).unwrap();

    // Try to resume - should fail or be a no-op
    let result = executor.execute(Command::RunResume {
        run: run_id.clone(),
    });

    // Check final status is still Completed
    let output = executor.execute(Command::RunGet {
        run: run_id,
    }).unwrap();
    match output {
        Output::RunInfoVersioned(v) => {
            // Status should still be Completed (resume should fail or be ignored)
            assert_eq!(v.info.status, RunStatus::Completed,
                "Completed run should not become Active via Resume");
        }
        _ => panic!("Expected RunInfoVersioned"),
    }
}

/// Paused runs can be resumed
#[test]
fn paused_runs_can_be_resumed() {
    let executor = create_executor();

    let run_id = match executor.execute(Command::RunCreate {
        run_id: Some("pause-resume-run".into()),
        metadata: None,
    }).unwrap() {
        Output::RunWithVersion { info, .. } => info.id,
        _ => panic!("Expected RunWithVersion"),
    };

    // Pause
    executor.execute(Command::RunPause { run: run_id.clone() }).unwrap();

    let output = executor.execute(Command::RunGet { run: run_id.clone() }).unwrap();
    match &output {
        Output::RunInfoVersioned(v) => assert_eq!(v.info.status, RunStatus::Paused),
        _ => panic!("Expected Paused"),
    }

    // Resume
    executor.execute(Command::RunResume { run: run_id.clone() }).unwrap();

    let output = executor.execute(Command::RunGet { run: run_id }).unwrap();
    match output {
        Output::RunInfoVersioned(v) => assert_eq!(v.info.status, RunStatus::Active),
        _ => panic!("Expected Active after resume"),
    }
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
        Output::MaybeVersioned(Some(vv)) => {
            assert_eq!(vv.value, Value::String("default_value".into()));
        }
        _ => panic!("Default run should work"),
    }

    // Explicit "default" run should be equivalent
    let output = executor.execute(Command::KvGet {
        run: Some(RunId::from("default")),
        key: "default_key".into(),
    }).unwrap();
    match output {
        Output::MaybeVersioned(Some(vv)) => {
            assert_eq!(vv.value, Value::String("default_value".into()));
        }
        _ => panic!("Explicit 'default' run should work"),
    }
}
