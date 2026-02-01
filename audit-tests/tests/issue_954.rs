//! Audit test for issue #954: Output docstring says KvGet returns MaybeVersioned but it
//! returned Maybe
//! Verdict: FIXED
//!
//! The `Output` enum docstring (output.rs) shows an example where KvGet returns
//! `Output::MaybeVersioned`, and the `Command::KvGet` variant docstring in
//! command.rs says "Returns: `Output::MaybeValue`". The handler in handlers/kv.rs
//! now correctly returns `Output::MaybeVersioned(result)` — which wraps
//! `Option<VersionedValue>`, consistent with the documentation.

use strata_core::value::Value;
use strata_engine::database::Database;
use strata_executor::{BranchId, Command, Executor, Output};

/// Verify that KvGet now returns Output::MaybeVersioned as documented.
#[test]
fn issue_954_kvget_returns_maybe_versioned() {
    let db = Database::cache().unwrap();
    let executor = Executor::new(db);
    let branch = BranchId::from("default");

    // Store a value
    executor
        .execute(Command::KvPut {
            branch: Some(branch.clone()),
            key: "k1".into(),
            value: Value::Int(42),
        })
        .unwrap();

    // Retrieve it
    let result = executor
        .execute(Command::KvGet {
            branch: Some(branch.clone()),
            key: "k1".into(),
        })
        .unwrap();

    // The handler now returns MaybeVersioned, consistent with documentation
    assert!(
        matches!(result, Output::MaybeVersioned(Some(_))),
        "KvGet should return MaybeVersioned(Some). Got: {:?}",
        result
    );
}

/// Verify that KvGet for a missing key returns MaybeVersioned(None).
#[test]
fn issue_954_kvget_missing_key_returns_maybe_versioned_none() {
    let db = Database::cache().unwrap();
    let executor = Executor::new(db);
    let branch = BranchId::from("default");

    let result = executor
        .execute(Command::KvGet {
            branch: Some(branch.clone()),
            key: "nonexistent".into(),
        })
        .unwrap();

    assert!(
        matches!(result, Output::MaybeVersioned(None)),
        "KvGet for missing key should return MaybeVersioned(None). Got: {:?}",
        result
    );
}
