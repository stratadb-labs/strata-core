//! Audit test for issue #973: JSON set produces 2 WAL appends instead of 1
//!
//! When a JSON document doesn't exist and the path is non-root, the handler
//! calls create() then set() as two separate transactions, producing 2 WAL
//! appends. This should be consolidated into a single atomic transaction.

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
fn json_set_root_new_doc_produces_one_wal_write() {
    let dir = TempDir::new().expect("tempdir");
    let db = Database::builder()
        .path(dir.path())
        .always()
        .open()
        .expect("open db");
    let strata = Strata::from_database(db).expect("strata");

    let before = wal_appends(&strata);

    // json/set at root path on a new document
    strata
        .executor()
        .execute(Command::JsonSet {
            branch: None,
            key: "doc1".into(),
            path: "$".into(),
            value: Value::String("hello".into()),
        })
        .unwrap();

    let after = wal_appends(&strata);
    assert_eq!(
        after - before,
        1,
        "json set root (new doc) should produce exactly 1 WAL append, but produced {}",
        after - before
    );
}

#[test]
fn json_set_path_new_doc_produces_one_wal_write() {
    let dir = TempDir::new().expect("tempdir");
    let db = Database::builder()
        .path(dir.path())
        .always()
        .open()
        .expect("open db");
    let strata = Strata::from_database(db).expect("strata");

    let before = wal_appends(&strata);

    // json/set at non-root path on a new document
    // Previously this produced 2 WAL appends (create empty + set at path)
    strata
        .executor()
        .execute(Command::JsonSet {
            branch: None,
            key: "doc2".into(),
            path: "user.name".into(),
            value: Value::String("Alice".into()),
        })
        .unwrap();

    let after = wal_appends(&strata);
    assert_eq!(
        after - before,
        1,
        "json set path (new doc) should produce exactly 1 WAL append, but produced {}",
        after - before
    );
}

#[test]
fn json_set_path_existing_doc_produces_one_wal_write() {
    let dir = TempDir::new().expect("tempdir");
    let db = Database::builder()
        .path(dir.path())
        .always()
        .open()
        .expect("open db");
    let strata = Strata::from_database(db).expect("strata");

    // Create the document first
    strata
        .executor()
        .execute(Command::JsonSet {
            branch: None,
            key: "doc3".into(),
            path: "$".into(),
            value: Value::Object(Default::default()),
        })
        .unwrap();

    let before = wal_appends(&strata);

    // Set at path on existing document
    strata
        .executor()
        .execute(Command::JsonSet {
            branch: None,
            key: "doc3".into(),
            path: "name".into(),
            value: Value::String("Bob".into()),
        })
        .unwrap();

    let after = wal_appends(&strata);
    assert_eq!(
        after - before,
        1,
        "json set path (existing doc) should produce exactly 1 WAL append, but produced {}",
        after - before
    );
}

#[test]
fn json_set_data_integrity_after_fix() {
    let dir = TempDir::new().expect("tempdir");
    let db = Database::builder()
        .path(dir.path())
        .always()
        .open()
        .expect("open db");
    let strata = Strata::from_database(db).expect("strata");

    // Set at non-root path on new document (the fixed path)
    strata
        .executor()
        .execute(Command::JsonSet {
            branch: None,
            key: "integrity-doc".into(),
            path: "user.name".into(),
            value: Value::String("Alice".into()),
        })
        .unwrap();

    // Verify the data was written correctly
    let output = strata
        .executor()
        .execute(Command::JsonGet {
            branch: None,
            key: "integrity-doc".into(),
            path: "user.name".into(),
        })
        .unwrap();

    match output {
        strata_executor::Output::MaybeVersioned(Some(vv)) => {
            assert_eq!(vv.value, Value::String("Alice".into()));
        }
        other => panic!("Expected MaybeVersioned(Some), got {:?}", other),
    }

    // Also verify root is an object
    let output = strata
        .executor()
        .execute(Command::JsonGet {
            branch: None,
            key: "integrity-doc".into(),
            path: "$".into(),
        })
        .unwrap();

    match output {
        strata_executor::Output::MaybeVersioned(Some(vv)) => {
            assert!(
                matches!(vv.value, Value::Object(_)),
                "Root should be an object"
            );
        }
        other => panic!("Expected MaybeVersioned(Some), got {:?}", other),
    }
}
