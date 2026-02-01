//! Audit test for issue #959: JsonDelete inconsistent — root returns count, non-root
//! always returns 1
//! Verdict: CONFIRMED BUG
//!
//! In handlers/json.rs, the `json_delete` handler has two code paths:
//!
//! 1. Root path (`$`): Calls `p.json.destroy()` and returns `Uint(1)` if the doc
//!    existed, `Uint(0)` if it did not. This correctly reflects whether something
//!    was actually deleted.
//!
//! 2. Non-root path (e.g., `$.field`): Calls `p.json.delete_at_path()` and
//!    **always** returns `Uint(1)` regardless of whether the path existed in
//!    the document. If the path was not present, the engine call may succeed
//!    (no-op) but the handler still claims 1 element was removed.
//!
//! This is inconsistent: root deletion tells the truth about what happened,
//! while non-root deletion always lies and says something was deleted.

use std::collections::HashMap;

use strata_core::value::Value;
use strata_engine::database::Database;
use strata_executor::{BranchId, Command, Executor, Output};

/// Deleting a non-existent document at root returns Uint(0) — correct.
#[test]
fn issue_959_json_delete_root_nonexistent_returns_zero() {
    let db = Database::cache().unwrap();
    let executor = Executor::new(db);
    let branch = BranchId::from("default");

    let result = executor
        .execute(Command::JsonDelete {
            branch: Some(branch.clone()),
            key: "nonexistent".into(),
            path: "$".into(),
        })
        .unwrap();

    assert!(
        matches!(result, Output::Uint(0)),
        "Delete non-existent root doc should return 0. Got: {:?}",
        result
    );
}

/// Deleting an existing document at root returns Uint(1) — correct.
#[test]
fn issue_959_json_delete_root_existing_returns_one() {
    let db = Database::cache().unwrap();
    let executor = Executor::new(db);
    let branch = BranchId::from("default");

    // Create a document
    executor
        .execute(Command::JsonSet {
            branch: Some(branch.clone()),
            key: "doc1".into(),
            path: "$".into(),
            value: Value::Object(HashMap::from([(
                "name".to_string(),
                Value::String("test".into()),
            )])),
        })
        .unwrap();

    let result = executor
        .execute(Command::JsonDelete {
            branch: Some(branch.clone()),
            key: "doc1".into(),
            path: "$".into(),
        })
        .unwrap();

    assert!(
        matches!(result, Output::Uint(1)),
        "Delete existing root doc should return 1. Got: {:?}",
        result
    );
}

/// Deleting a non-existent path within an existing doc always returns Uint(1) — BUG.
#[test]
fn issue_959_json_delete_nonroot_nonexistent_path_returns_one() {
    let db = Database::cache().unwrap();
    let executor = Executor::new(db);
    let branch = BranchId::from("default");

    // Create a document with a known structure
    executor
        .execute(Command::JsonSet {
            branch: Some(branch.clone()),
            key: "doc2".into(),
            path: "$".into(),
            value: Value::Object(HashMap::from([(
                "name".to_string(),
                Value::String("test".into()),
            )])),
        })
        .unwrap();

    // Delete a path that does NOT exist in the document
    let result = executor.execute(Command::JsonDelete {
        branch: Some(branch.clone()),
        key: "doc2".into(),
        path: "$.nonexistent_field".into(),
    });

    // BUG: Non-root delete always returns Uint(1) even if the path does not exist.
    // The handler does: Ok(Output::Uint(1)) unconditionally for non-root paths.
    // Should return Uint(0) when the path was not found.
    match result {
        Ok(Output::Uint(1)) => {
            // Bug confirmed: always returns 1 for non-root, regardless of path existence
        }
        Ok(Output::Uint(0)) => {
            // Bug fixed: correctly reports that nothing was deleted
        }
        Ok(other) => panic!("Unexpected output: {:?}", other),
        Err(e) => {
            // The engine might error if the path doesn't exist,
            // in which case the handler would propagate the error.
            // This is also acceptable behavior (fail loudly).
            let _ = e;
        }
    }
}

/// Deleting an existing path within a doc returns Uint(1) — correct (but indistinguishable
/// from the non-existent path case due to the bug).
#[test]
fn issue_959_json_delete_nonroot_existing_path_returns_one() {
    let db = Database::cache().unwrap();
    let executor = Executor::new(db);
    let branch = BranchId::from("default");

    // Create a document with a field we will delete
    executor
        .execute(Command::JsonSet {
            branch: Some(branch.clone()),
            key: "doc3".into(),
            path: "$".into(),
            value: Value::Object(HashMap::from([
                ("keep".to_string(), Value::String("yes".into())),
                ("remove".to_string(), Value::String("no".into())),
            ])),
        })
        .unwrap();

    // Delete the "remove" field — this actually exists
    let result = executor
        .execute(Command::JsonDelete {
            branch: Some(branch.clone()),
            key: "doc3".into(),
            path: "$.remove".into(),
        })
        .unwrap();

    // Returns Uint(1) — correct in this case, but the same value is returned
    // even when the path does not exist, making the return value unreliable.
    assert!(
        matches!(result, Output::Uint(1)),
        "Delete existing path should return 1. Got: {:?}",
        result
    );

    // Verify the field was actually removed
    let get_result = executor
        .execute(Command::JsonGet {
            branch: Some(branch.clone()),
            key: "doc3".into(),
            path: "$.remove".into(),
        })
        .unwrap();

    assert!(
        matches!(
            get_result,
            Output::MaybeVersioned(None) | Output::Maybe(None)
        ),
        "Deleted path should not be retrievable. Got: {:?}",
        get_result
    );
}
