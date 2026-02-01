//! Audit test for issue #919: State primitive missing delete and list operations
//! Verdict: ARCHITECTURAL CHOICE
//!
//! The State primitive provides: StateInit, StateRead, StateSet, StateCas, StateReadv
//! But it does NOT provide: StateDelete, StateList
//!
//! This means:
//! - Once a state cell is created, it cannot be removed (only overwritten).
//! - There is no way to enumerate existing state cells for a branch.
//!   You can only read cells whose names you already know.
//!
//! Compare this with KV which has: KvPut, KvGet, KvDelete, KvList, KvGetv

use strata_executor::{Command, Output};

/// Demonstrate that there is no StateDelete command.
/// After initializing a state cell, the only option is to overwrite its value.
#[test]
fn issue_919_no_state_delete_command() {
    let db = strata_engine::database::Database::cache().unwrap();
    let executor = strata_executor::Executor::new(db);

    let branch = strata_executor::BranchId::from("default");

    // Initialize a state cell
    executor
        .execute(Command::StateInit {
            branch: Some(branch.clone()),
            cell: "persistent_cell".into(),
            value: strata_core::value::Value::String("initial".into()),
        })
        .unwrap();

    // Verify the cell exists
    let result = executor
        .execute(Command::StateRead {
            branch: Some(branch.clone()),
            cell: "persistent_cell".into(),
        })
        .unwrap();
    assert!(
        matches!(
            result,
            Output::MaybeVersioned(Some(_)) | Output::Maybe(Some(_))
        ),
        "Cell should exist after init"
    );

    // There is no Command::StateDelete variant.
    // The Command enum has KvDelete but no StateDelete.
    // The only way to "remove" data is to overwrite with a sentinel value.
    executor
        .execute(Command::StateSet {
            branch: Some(branch.clone()),
            cell: "persistent_cell".into(),
            value: strata_core::value::Value::Null,
        })
        .unwrap();

    // Even after setting to Null, the cell still exists and is readable
    let result = executor
        .execute(Command::StateRead {
            branch: Some(branch.clone()),
            cell: "persistent_cell".into(),
        })
        .unwrap();

    // The cell still returns a value (Null), it was not deleted
    match result {
        Output::MaybeVersioned(Some(vv)) => {
            assert_eq!(
                vv.value,
                strata_core::value::Value::Null,
                "Cell still exists with Null value -- cannot truly delete state cells"
            );
        }
        Output::MaybeVersioned(None) | Output::Maybe(None) => {
            // If this happens, Null is treated as deletion, which is a different behavior
            // but still not an explicit delete operation
        }
        Output::Maybe(Some(val)) => {
            assert_eq!(
                val,
                strata_core::value::Value::Null,
                "Cell still exists with Null value -- cannot truly delete state cells"
            );
        }
        other => panic!("Unexpected result: {:?}", other),
    }
}

/// Demonstrate that there is no StateList command.
/// You cannot enumerate state cells for a branch.
#[test]
fn issue_919_no_state_list_command() {
    let db = strata_engine::database::Database::cache().unwrap();
    let executor = strata_executor::Executor::new(db);

    let branch = strata_executor::BranchId::from("default");

    // Create multiple state cells
    for i in 0..5 {
        executor
            .execute(Command::StateInit {
                branch: Some(branch.clone()),
                cell: format!("cell_{}", i),
                value: strata_core::value::Value::Int(i),
            })
            .unwrap();
    }

    // We can read each cell individually if we know its name
    for i in 0..5 {
        let result = executor
            .execute(Command::StateRead {
                branch: Some(branch.clone()),
                cell: format!("cell_{}", i),
            })
            .unwrap();
        assert!(
            matches!(
                result,
                Output::MaybeVersioned(Some(_)) | Output::Maybe(Some(_))
            ),
            "Cell {} should be readable by name",
            i
        );
    }

    // But there is no way to list all cells. Compare with KvList:
    let kv_list_result = executor
        .execute(Command::KvList {
            branch: Some(branch.clone()),
            prefix: None,
            cursor: None,
            limit: None,
        })
        .unwrap();
    match kv_list_result {
        Output::Keys(keys) => {
            // KvList exists and works (may return empty since we only wrote State cells)
            // The point is: KvList EXISTS, but StateList does NOT.
            let _ = keys;
        }
        other => panic!("Unexpected KvList result: {:?}", other),
    }

    // ARCHITECTURAL NOTE:
    // The absence of StateList means:
    // 1. Applications must track their own cell names externally
    // 2. There is no way to discover what state cells exist in a branch
    // 3. Cleanup/migration tools cannot enumerate state cells
    //
    // The absence of StateDelete means:
    // 1. State cells accumulate over the lifetime of a branch
    // 2. Storage cannot be reclaimed at the cell level
    // 3. The only cleanup mechanism is deleting the entire branch
}
