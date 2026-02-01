//! Audit test for issue #955: KvGet/StateRead/JsonGet strip version metadata
//! Verdict: FIXED
//!
//! The get-by-key operations for KV, State, and JSON now return
//! `Output::MaybeVersioned(Option<VersionedValue>)` instead of
//! `Output::Maybe(Option<Value>)`. This means the version number and
//! timestamp associated with the stored value are preserved on read.

use strata_core::value::Value;
use strata_engine::database::Database;
use strata_executor::{BranchId, Command, Executor, Output};

/// KvGet now preserves version metadata.
#[test]
fn issue_955_kvget_preserves_version() {
    let db = Database::cache().unwrap();
    let executor = Executor::new(db);
    let branch = BranchId::from("default");

    // Store a value and capture the version
    let put_result = executor
        .execute(Command::KvPut {
            branch: Some(branch.clone()),
            key: "k1".into(),
            value: Value::Int(42),
        })
        .unwrap();
    let version = match put_result {
        Output::Version(v) => v,
        other => panic!("KvPut should return Version, got: {:?}", other),
    };

    // Get it back -- version is now preserved
    let get_result = executor
        .execute(Command::KvGet {
            branch: Some(branch.clone()),
            key: "k1".into(),
        })
        .unwrap();

    // FIXED: Returns MaybeVersioned with version metadata
    match get_result {
        Output::MaybeVersioned(Some(vv)) => {
            assert_eq!(vv.value, Value::Int(42));
            assert_eq!(
                vv.version, version,
                "Version from get should match version from put"
            );
        }
        other => panic!("KvGet should return MaybeVersioned(Some). Got: {:?}", other),
    }
}

/// StateRead now preserves version metadata.
#[test]
fn issue_955_state_read_preserves_version() {
    let db = Database::cache().unwrap();
    let executor = Executor::new(db);
    let branch = BranchId::from("default");

    // Initialize a state cell
    let init_result = executor
        .execute(Command::StateInit {
            branch: Some(branch.clone()),
            cell: "cell1".into(),
            value: Value::Int(1),
        })
        .unwrap();
    let _version = match init_result {
        Output::Version(v) => v,
        other => panic!("StateInit should return Version, got: {:?}", other),
    };

    // Read the state cell -- version is now preserved
    let state_result = executor
        .execute(Command::StateRead {
            branch: Some(branch.clone()),
            cell: "cell1".into(),
        })
        .unwrap();

    // FIXED: Returns MaybeVersioned
    assert!(
        matches!(state_result, Output::MaybeVersioned(Some(_))),
        "StateRead should return MaybeVersioned(Some). Got: {:?}",
        state_result
    );
}

/// JsonGet now preserves version metadata.
#[test]
fn issue_955_json_get_preserves_version() {
    let db = Database::cache().unwrap();
    let executor = Executor::new(db);
    let branch = BranchId::from("default");

    // Create a JSON document
    let set_result = executor
        .execute(Command::JsonSet {
            branch: Some(branch.clone()),
            key: "doc1".into(),
            path: "$".into(),
            value: Value::Int(99),
        })
        .unwrap();
    let _version = match set_result {
        Output::Version(v) => v,
        other => panic!("JsonSet should return Version, got: {:?}", other),
    };

    // Read the JSON document -- version is now preserved
    let get_result = executor
        .execute(Command::JsonGet {
            branch: Some(branch.clone()),
            key: "doc1".into(),
            path: "$".into(),
        })
        .unwrap();

    // FIXED: Returns MaybeVersioned
    assert!(
        matches!(get_result, Output::MaybeVersioned(Some(_))),
        "JsonGet should return MaybeVersioned(Some). Got: {:?}",
        get_result
    );
}

/// Contrast: the version-history commands (KvGetv, StateReadv) DO preserve versions.
#[test]
fn issue_955_version_history_commands_do_preserve_versions() {
    let db = Database::cache().unwrap();
    let executor = Executor::new(db);
    let branch = BranchId::from("default");

    // Write a KV entry
    executor
        .execute(Command::KvPut {
            branch: Some(branch.clone()),
            key: "k2".into(),
            value: Value::Int(10),
        })
        .unwrap();

    // KvGetv preserves version history
    let getv_result = executor
        .execute(Command::KvGetv {
            branch: Some(branch.clone()),
            key: "k2".into(),
        })
        .unwrap();

    match getv_result {
        Output::VersionHistory(Some(versions)) => {
            assert!(!versions.is_empty(), "Should have at least one version");
            // Each VersionedValue has .version and .timestamp fields
            let first = &versions[0];
            assert!(first.version > 0, "Version should be set");
        }
        Output::VersionHistory(None) => {
            panic!("Key should exist in version history");
        }
        other => panic!("Expected VersionHistory, got: {:?}", other),
    }
}
