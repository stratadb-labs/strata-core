//! Audit test for issue #974: Branch delete produces multiple WAL appends
//!
//! Branch deletion previously used 3+ separate write transactions:
//! one to delete executor namespace data, one for metadata namespace data,
//! and one to remove the branch metadata entry. This should be consolidated
//! into a single atomic transaction.

use strata_core::Value;
use strata_engine::Database;
use strata_executor::{Command, Strata};
use tempfile::TempDir;

/// Helper: get current WAL append count.
fn wal_appends(strata: &Strata) -> u64 {
    strata
        .database()
        .durability_counters()
        .map(|c| c.wal_appends)
        .unwrap_or(0)
}

#[test]
fn branch_delete_empty_branch_produces_one_wal_write() {
    let dir = TempDir::new().expect("tempdir");
    let db = Database::builder()
        .path(dir.path())
        .always()
        .open()
        .expect("open db");
    let strata = Strata::from_database(db).expect("strata");

    // Create a branch
    strata.branches().create("to-delete").unwrap();

    let before = wal_appends(&strata);

    // Delete the branch (no data in it)
    strata
        .executor()
        .execute(Command::BranchDelete {
            branch: "to-delete".into(),
        })
        .unwrap();

    let after = wal_appends(&strata);
    assert_eq!(
        after - before,
        1,
        "branch delete (empty) should produce 1 WAL append, but produced {}",
        after - before
    );
}

#[test]
fn branch_delete_with_data_produces_one_wal_write() {
    let dir = TempDir::new().expect("tempdir");
    let db = Database::builder()
        .path(dir.path())
        .always()
        .open()
        .expect("open db");
    let strata = Strata::from_database(db).expect("strata");

    // Create a branch and add data
    strata.branches().create("data-branch").unwrap();
    strata
        .executor()
        .execute(Command::KvPut {
            branch: Some("data-branch".into()),
            key: "key1".into(),
            value: Value::String("value1".into()),
        })
        .unwrap();
    strata
        .executor()
        .execute(Command::StateSet {
            branch: Some("data-branch".into()),
            cell: "cell1".into(),
            value: Value::Int(42),
        })
        .unwrap();

    let before = wal_appends(&strata);

    // Delete the branch (has KV + State data)
    strata
        .executor()
        .execute(Command::BranchDelete {
            branch: "data-branch".into(),
        })
        .unwrap();

    let after = wal_appends(&strata);
    assert_eq!(
        after - before,
        1,
        "branch delete (with data) should produce 1 WAL append, but produced {}",
        after - before
    );
}

#[test]
fn branch_delete_actually_removes_data() {
    let dir = TempDir::new().expect("tempdir");
    let db = Database::builder()
        .path(dir.path())
        .always()
        .open()
        .expect("open db");
    let strata = Strata::from_database(db).expect("strata");

    // Create branch with data
    strata.branches().create("verify-branch").unwrap();
    strata
        .executor()
        .execute(Command::KvPut {
            branch: Some("verify-branch".into()),
            key: "test-key".into(),
            value: Value::String("test-value".into()),
        })
        .unwrap();

    // Delete the branch
    strata
        .executor()
        .execute(Command::BranchDelete {
            branch: "verify-branch".into(),
        })
        .unwrap();

    // Verify branch no longer exists
    let output = strata
        .executor()
        .execute(Command::BranchExists {
            branch: "verify-branch".into(),
        })
        .unwrap();
    assert!(
        matches!(output, strata_executor::Output::Bool(false)),
        "Branch should not exist after deletion"
    );
}
