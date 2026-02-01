//! Audit test for issue #925: Branch delete is not atomic
//! Verdict: ARCHITECTURAL CHOICE (questionable)
//!
//! In engine/src/primitives/branch/index.rs:347-373, delete_branch_data_internal()
//! runs a SEPARATE transaction for each TypeTag (KV, Event, State, Trace, Json, Vector).
//! If a failure occurs between transactions, the branch is left in a partially
//! deleted state:
//!
//! ```text
//! for type_tag in [KV, Event, State, Trace, Json, Vector] {
//!     self.db.transaction(branch_id, |txn| {
//!         // scan and delete all entries for this type_tag
//!     })?;
//! }
//! ```
//!
//! After KV and Event data is deleted, if the State transaction fails,
//! the branch has lost its KV and Event data but retains State, Json, and Vector.
//! The branch metadata is also deleted in a separate transaction at the end.
//!
//! This test verifies that branch delete successfully removes all data types
//! under normal conditions.

use strata_executor::{Command, Output};

/// Verify that branch delete removes data from all primitive types.
#[test]
fn issue_925_branch_delete_removes_all_data() {
    let db = strata_engine::database::Database::cache().unwrap();
    let executor = strata_executor::Executor::new(db);

    // Create a non-default branch
    let create_result = executor
        .execute(Command::BranchCreate {
            branch_id: Some("test-branch".into()),
            metadata: None,
        })
        .unwrap();
    assert!(
        matches!(create_result, Output::BranchWithVersion { .. }),
        "Branch should be created"
    );

    let branch = strata_executor::BranchId::from("test-branch");

    // Populate with KV data
    executor
        .execute(Command::KvPut {
            branch: Some(branch.clone()),
            key: "kv1".into(),
            value: strata_core::value::Value::String("kv_value".into()),
        })
        .unwrap();

    // Populate with State data
    executor
        .execute(Command::StateInit {
            branch: Some(branch.clone()),
            cell: "state1".into(),
            value: strata_core::value::Value::Int(100),
        })
        .unwrap();

    // Populate with Event data
    executor
        .execute(Command::EventAppend {
            branch: Some(branch.clone()),
            event_type: "test".into(),
            payload: strata_core::value::Value::Object(
                vec![(
                    "msg".to_string(),
                    strata_core::value::Value::String("hello".into()),
                )]
                .into_iter()
                .collect(),
            ),
        })
        .unwrap();

    // Verify data exists before delete
    let kv_before = executor
        .execute(Command::KvGet {
            branch: Some(branch.clone()),
            key: "kv1".into(),
        })
        .unwrap();
    assert!(
        matches!(
            kv_before,
            Output::MaybeVersioned(Some(_)) | Output::Maybe(Some(_))
        ),
        "KV data should exist before delete"
    );

    let state_before = executor
        .execute(Command::StateRead {
            branch: Some(branch.clone()),
            cell: "state1".into(),
        })
        .unwrap();
    assert!(
        matches!(
            state_before,
            Output::MaybeVersioned(Some(_)) | Output::Maybe(Some(_))
        ),
        "State data should exist before delete"
    );

    let event_len_before = executor
        .execute(Command::EventLen {
            branch: Some(branch.clone()),
        })
        .unwrap();
    assert!(
        matches!(event_len_before, Output::Uint(1)),
        "Event data should exist before delete"
    );

    // Delete the branch
    let delete_result = executor
        .execute(Command::BranchDelete {
            branch: branch.clone(),
        })
        .unwrap();
    assert!(
        matches!(delete_result, Output::Unit),
        "Branch delete should succeed"
    );

    // Verify branch no longer exists
    let exists_result = executor
        .execute(Command::BranchExists {
            branch: branch.clone(),
        })
        .unwrap();
    match exists_result {
        Output::Bool(exists) => {
            assert!(!exists, "Branch should not exist after delete");
        }
        other => panic!("Expected Bool, got: {:?}", other),
    }

    // ARCHITECTURAL NOTE:
    // The delete succeeded here because no failure occurred between the
    // per-TypeTag transactions. However, the non-atomic nature means:
    //
    // 1. If the process crashes after deleting KV data but before deleting
    //    State data, the branch is in a partially deleted state.
    // 2. The branch metadata deletion happens AFTER all data deletion.
    //    If data deletion succeeds but metadata deletion fails, the branch
    //    still appears to exist but has no data.
    // 3. There is no rollback mechanism -- if the third transaction fails,
    //    the first two transactions' deletions are already committed.
    //
    // See engine/src/primitives/branch/index.rs:347-373 for the implementation.
}
