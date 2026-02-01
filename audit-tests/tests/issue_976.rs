//! Audit test for issue #976: kv/delete produces 2 WAL appends instead of 1
//!
//! The kv_delete handler calls require_branch_exists() (read-only transaction)
//! before calling kv.delete() (write transaction). After #970, the read-only
//! branch check should produce 0 WAL appends, leaving only the actual delete
//! as a single WAL append.

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
fn kv_delete_existing_key_produces_one_wal_write() {
    let dir = TempDir::new().expect("tempdir");
    let db = Database::builder()
        .path(dir.path())
        .always()
        .open()
        .expect("open db");
    let strata = Strata::from_database(db).expect("strata");

    // Create a branch and put a key
    strata.branches().create("del-branch").unwrap();
    strata
        .executor()
        .execute(Command::KvPut {
            branch: Some("del-branch".into()),
            key: "key1".into(),
            value: Value::String("value1".into()),
        })
        .unwrap();

    let before = wal_appends(&strata);

    // Delete the key
    strata
        .executor()
        .execute(Command::KvDelete {
            branch: Some("del-branch".into()),
            key: "key1".into(),
        })
        .unwrap();

    let after = wal_appends(&strata);
    assert_eq!(
        after - before,
        1,
        "kv delete should produce exactly 1 WAL append, but produced {}",
        after - before
    );
}

#[test]
fn kv_delete_nonexistent_key_produces_no_wal_writes() {
    let dir = TempDir::new().expect("tempdir");
    let db = Database::builder()
        .path(dir.path())
        .always()
        .open()
        .expect("open db");
    let strata = Strata::from_database(db).expect("strata");

    // Create a branch but don't put any keys
    strata.branches().create("empty-branch").unwrap();

    let before = wal_appends(&strata);

    // Delete a nonexistent key — should be read-only (get returns None, no delete issued)
    let output = strata
        .executor()
        .execute(Command::KvDelete {
            branch: Some("empty-branch".into()),
            key: "nonexistent".into(),
        })
        .unwrap();

    assert!(matches!(output, strata_executor::Output::Bool(false)));

    let after = wal_appends(&strata);
    assert_eq!(
        after, before,
        "kv delete of nonexistent key should produce 0 WAL appends, but produced {}",
        after - before
    );
}

#[test]
fn kv_delete_default_branch_produces_one_wal_write() {
    let dir = TempDir::new().expect("tempdir");
    let db = Database::builder()
        .path(dir.path())
        .always()
        .open()
        .expect("open db");
    let strata = Strata::from_database(db).expect("strata");

    // Put on default branch
    strata
        .executor()
        .execute(Command::KvPut {
            branch: None,
            key: "default-key".into(),
            value: Value::Int(42),
        })
        .unwrap();

    let before = wal_appends(&strata);

    // Delete from default branch (skips require_branch_exists for default)
    strata
        .executor()
        .execute(Command::KvDelete {
            branch: None,
            key: "default-key".into(),
        })
        .unwrap();

    let after = wal_appends(&strata);
    assert_eq!(
        after - before,
        1,
        "kv delete on default branch should produce 1 WAL append, but produced {}",
        after - before
    );
}
