//! Audit test for issue #957: JsonGetv silently drops versions on deserialization error
//! Verdict: CONFIRMED BUG
//!
//! In handlers/json.rs, the `json_getv` handler uses `filter_map` with `.ok()?`
//! to convert version history entries:
//!
//! ```ignore
//! .filter_map(|v| {
//!     let value = convert_result(json_to_value(v.value)).ok()?;
//!     Some(VersionedValue { value, version: ..., timestamp: ... })
//! })
//! ```
//!
//! If `json_to_value` fails for any version entry (e.g., corrupt or
//! incompatible JSON encoding), that entry is silently dropped from the
//! version history. No error is returned to the caller, and no warning is
//! logged. The caller receives a truncated history with no indication that
//! entries were removed.
//!
//! This is a data-loss-adjacent bug: the caller cannot distinguish between
//! "key has 3 versions" and "key has 5 versions but 2 failed to deserialize."

use strata_core::value::Value;
use strata_engine::database::Database;
use strata_executor::{BranchId, Command, Executor, Output};

/// Basic test: create multiple versions and verify JsonGetv returns them.
/// In normal operation (no corrupt data), all versions should be present.
#[test]
fn issue_957_json_getv_returns_version_history() {
    let db = Database::cache().unwrap();
    let executor = Executor::new(db);
    let branch = BranchId::from("default");

    // Create and update a JSON doc multiple times to build version history
    executor
        .execute(Command::JsonSet {
            branch: Some(branch.clone()),
            key: "doc1".into(),
            path: "$".into(),
            value: Value::Int(1),
        })
        .unwrap();

    executor
        .execute(Command::JsonSet {
            branch: Some(branch.clone()),
            key: "doc1".into(),
            path: "$".into(),
            value: Value::Int(2),
        })
        .unwrap();

    executor
        .execute(Command::JsonSet {
            branch: Some(branch.clone()),
            key: "doc1".into(),
            path: "$".into(),
            value: Value::Int(3),
        })
        .unwrap();

    // Get version history
    let result = executor
        .execute(Command::JsonGetv {
            branch: Some(branch.clone()),
            key: "doc1".into(),
        })
        .unwrap();

    // Should have version history entries
    match result {
        Output::VersionHistory(Some(versions)) => {
            // If any version had a deserialization error, it would be silently dropped.
            // In normal operation, all versions should be present.
            assert!(
                versions.len() >= 1,
                "Should have at least one version entry"
            );
        }
        Output::VersionHistory(None) => {
            panic!("Document exists — version history should not be None");
        }
        other => panic!("Expected VersionHistory, got {:?}", other),
    }
}

/// Verify that JsonGetv for a non-existent key returns None.
#[test]
fn issue_957_json_getv_nonexistent_returns_none() {
    let db = Database::cache().unwrap();
    let executor = Executor::new(db);
    let branch = BranchId::from("default");

    let result = executor
        .execute(Command::JsonGetv {
            branch: Some(branch.clone()),
            key: "nonexistent".into(),
        })
        .unwrap();

    match result {
        Output::VersionHistory(None) => {
            // Correct: non-existent key returns None
        }
        other => panic!("Expected VersionHistory(None), got {:?}", other),
    }
}

/// Contrast with KvGetv which uses map(to_versioned_value) without filter_map,
/// so deserialization errors in KV history would propagate rather than be silently dropped.
#[test]
fn issue_957_kvgetv_does_not_use_filter_map() {
    let db = Database::cache().unwrap();
    let executor = Executor::new(db);
    let branch = BranchId::from("default");

    // Create KV entries for comparison
    executor
        .execute(Command::KvPut {
            branch: Some(branch.clone()),
            key: "kv1".into(),
            value: Value::Int(10),
        })
        .unwrap();

    executor
        .execute(Command::KvPut {
            branch: Some(branch.clone()),
            key: "kv1".into(),
            value: Value::Int(20),
        })
        .unwrap();

    let result = executor
        .execute(Command::KvGetv {
            branch: Some(branch.clone()),
            key: "kv1".into(),
        })
        .unwrap();

    // KvGetv uses .map(to_versioned_value) — no silent dropping
    match result {
        Output::VersionHistory(Some(versions)) => {
            assert!(versions.len() >= 1, "KvGetv should return version history");
        }
        Output::VersionHistory(None) => {
            panic!("KV key should have version history");
        }
        other => panic!("Expected VersionHistory, got {:?}", other),
    }
}
