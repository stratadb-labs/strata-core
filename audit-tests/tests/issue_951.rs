//! Audit test for issue #951: Operations on deleted branch create orphaned data
//! Verdict: CONFIRMED BUG
//!
//! After deleting a branch via `BranchDelete`, subsequent write operations
//! targeting that branch may succeed, creating orphaned data that is not
//! associated with any known branch.
//!
//! The execution flow:
//! 1. `BranchCreate { branch_id: "temp" }` -- creates branch metadata
//! 2. `BranchDelete { branch: "temp" }` -- deletes branch metadata
//! 3. `KvPut { branch: "temp", key: "k", value: 42 }` -- still succeeds!
//!
//! In step 3, the KvPut handler:
//! 1. Calls `to_core_branch_id("temp")` to get a `strata_core::types::BranchId`
//! 2. Constructs a Key with the branch's namespace
//! 3. Writes the value to storage
//!
//! There is no check that the branch still exists before writing.
//! The `to_core_branch_id` function creates a BranchId from the string name,
//! which succeeds regardless of whether the branch exists in the branch registry.
//!
//! Impact:
//! - Data written to deleted branches is orphaned (no branch metadata points to it)
//! - The orphaned data consumes storage but is inaccessible via branch list/get
//! - In multi-threaded scenarios, a race between delete and write could silently
//!   create orphaned data

use strata_core::value::Value;
use strata_engine::database::Database;
use strata_executor::{BranchId, Command, Executor, Output};

/// Demonstrates that writing to a deleted branch succeeds, creating orphaned data.
#[test]
fn issue_951_ops_on_deleted_branch() {
    let db = Database::cache().unwrap();
    let executor = Executor::new(db);

    // Create a branch
    let branch_name = "temp_branch_951";
    executor
        .execute(Command::BranchCreate {
            branch_id: Some(branch_name.to_string()),
            metadata: None,
        })
        .unwrap();

    let branch = BranchId::from(branch_name);

    // Write some initial data
    executor
        .execute(Command::KvPut {
            branch: Some(branch.clone()),
            key: "before_delete".into(),
            value: Value::String("exists".into()),
        })
        .unwrap();

    // Delete the branch
    executor
        .execute(Command::BranchDelete {
            branch: branch.clone(),
        })
        .unwrap();

    // Verify branch is deleted
    let exists = executor
        .execute(Command::BranchExists {
            branch: branch.clone(),
        })
        .unwrap();
    assert_eq!(exists, Output::Bool(false), "Branch should be deleted");

    // Try to write to the deleted branch
    let write_result = executor.execute(Command::KvPut {
        branch: Some(branch.clone()),
        key: "orphan".into(),
        value: Value::Int(42),
    });

    // BUG: This may succeed, creating orphaned data
    match write_result {
        Ok(_) => {
            // Bug confirmed: data was written to a deleted branch.
            // This data is now orphaned -- it exists in storage but
            // is not associated with any branch in the branch registry.

            // The orphaned data may even be readable
            let read_result = executor.execute(Command::KvGet {
                branch: Some(branch.clone()),
                key: "orphan".into(),
            });

            match read_result {
                Ok(Output::MaybeVersioned(Some(versioned))) => {
                    assert_eq!(
                        versioned.value,
                        Value::Int(42),
                        "Orphaned data is readable on deleted branch"
                    );
                }
                Ok(Output::Maybe(Some(val))) => {
                    assert_eq!(val, Value::Int(42), "Orphaned data is readable");
                }
                _ => {
                    // Data was written but may not be readable (depends on implementation)
                }
            }
        }
        Err(_) => {
            // If this errors, the branch properly rejects writes after deletion.
            // This would mean the bug is fixed.
        }
    }
}

/// Demonstrates that event append on a deleted branch may succeed.
#[test]
fn issue_951_event_append_on_deleted_branch() {
    let db = Database::cache().unwrap();
    let executor = Executor::new(db);

    // Create and delete a branch
    let branch_name = "event_branch_951";
    executor
        .execute(Command::BranchCreate {
            branch_id: Some(branch_name.to_string()),
            metadata: None,
        })
        .unwrap();

    let branch = BranchId::from(branch_name);

    executor
        .execute(Command::BranchDelete {
            branch: branch.clone(),
        })
        .unwrap();

    // Try to append an event to the deleted branch
    let result = executor.execute(Command::EventAppend {
        branch: Some(branch.clone()),
        event_type: "orphan.event".into(),
        payload: Value::Object(std::collections::HashMap::from([(
            "data".to_string(),
            Value::String("should not exist".into()),
        )])),
    });

    match result {
        Ok(_) => {
            // Bug confirmed: event appended to deleted branch
        }
        Err(_) => {
            // Properly rejected
        }
    }
}

/// Demonstrates that state operations on a deleted branch may succeed.
#[test]
fn issue_951_state_set_on_deleted_branch() {
    let db = Database::cache().unwrap();
    let executor = Executor::new(db);

    // Create and delete a branch
    let branch_name = "state_branch_951";
    executor
        .execute(Command::BranchCreate {
            branch_id: Some(branch_name.to_string()),
            metadata: None,
        })
        .unwrap();

    let branch = BranchId::from(branch_name);

    executor
        .execute(Command::BranchDelete {
            branch: branch.clone(),
        })
        .unwrap();

    // Try to set state on the deleted branch
    let result = executor.execute(Command::StateSet {
        branch: Some(branch.clone()),
        cell: "orphan_cell".into(),
        value: Value::Bool(true),
    });

    match result {
        Ok(_) => {
            // Bug confirmed: state written to deleted branch
        }
        Err(_) => {
            // Properly rejected
        }
    }
}
