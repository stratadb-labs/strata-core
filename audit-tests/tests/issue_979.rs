//! Audit test for issue #979: kv/list_prefix triggers spurious WAL write on read-only scan
//!
//! Read-only scan operations (kv/list_prefix, event/read, json/list) were producing
//! 47-49 byte WAL appends on every call despite being read-only. After #970 (skip WAL
//! for read-only transactions), these operations should produce 0 WAL appends.

use strata_core::Value;
use strata_engine::Database;
use strata_executor::{Command, Strata};
use tempfile::TempDir;

/// Helper: get current WAL append count.
fn wal_appends(strata: &Strata) -> u64 {
    strata
        .durability_counters()
        .map(|c| c.wal_appends)
        .unwrap_or(0)
}

#[test]
fn kv_list_prefix_produces_no_wal_writes() {
    let dir = TempDir::new().expect("tempdir");
    std::fs::write(dir.path().join("strata.toml"), "durability = \"always\"\n").unwrap();
    let db = Database::open(dir.path()).expect("open db");
    let strata = Strata::from_database(db).expect("strata");

    // Seed some data so the scan has results to return
    for i in 0..10 {
        strata
            .executor()
            .execute(Command::KvPut {
                branch: None,
                key: format!("prefix:{}", i),
                value: Value::Int(i),
            })
            .unwrap();
    }

    let before = wal_appends(&strata);

    // Read-only prefix scan
    let output = strata
        .executor()
        .execute(Command::KvList {
            branch: None,
            prefix: Some("prefix:".into()),
            cursor: None,
            limit: None,
        })
        .unwrap();

    // Verify we got results (scan actually ran)
    if let strata_executor::Output::Keys(keys) = &output {
        assert_eq!(keys.len(), 10, "Should find 10 keys with prefix");
    } else {
        panic!("Expected Keys output");
    }

    let after = wal_appends(&strata);
    assert_eq!(
        after, before,
        "kv/list_prefix should produce 0 WAL appends (read-only), but produced {}",
        after - before
    );
}

#[test]
fn kv_list_no_prefix_produces_no_wal_writes() {
    let dir = TempDir::new().expect("tempdir");
    std::fs::write(dir.path().join("strata.toml"), "durability = \"always\"\n").unwrap();
    let db = Database::open(dir.path()).expect("open db");
    let strata = Strata::from_database(db).expect("strata");

    // Seed data
    strata
        .executor()
        .execute(Command::KvPut {
            branch: None,
            key: "key1".into(),
            value: Value::Int(1),
        })
        .unwrap();

    let before = wal_appends(&strata);

    // Full list (no prefix filter)
    strata
        .executor()
        .execute(Command::KvList {
            branch: None,
            prefix: None,
            cursor: None,
            limit: None,
        })
        .unwrap();

    let after = wal_appends(&strata);
    assert_eq!(
        after, before,
        "kv/list (no prefix) should produce 0 WAL appends, but produced {}",
        after - before
    );
}

#[test]
fn event_read_produces_no_wal_writes() {
    let dir = TempDir::new().expect("tempdir");
    std::fs::write(dir.path().join("strata.toml"), "durability = \"always\"\n").unwrap();
    let db = Database::open(dir.path()).expect("open db");
    let strata = Strata::from_database(db).expect("strata");

    // Append an event so there's something to read
    strata
        .executor()
        .execute(Command::EventAppend {
            branch: None,
            event_type: "test".into(),
            payload: Value::Object(std::collections::HashMap::from([
                ("data".into(), Value::Int(42)),
            ])),
        })
        .unwrap();

    let before = wal_appends(&strata);

    // Read the event (read-only)
    strata
        .executor()
        .execute(Command::EventRead {
            branch: None,
            sequence: 1,
        })
        .unwrap();

    let after = wal_appends(&strata);
    assert_eq!(
        after, before,
        "event/read should produce 0 WAL appends (read-only), but produced {}",
        after - before
    );
}

#[test]
fn event_read_nonexistent_produces_no_wal_writes() {
    let dir = TempDir::new().expect("tempdir");
    std::fs::write(dir.path().join("strata.toml"), "durability = \"always\"\n").unwrap();
    let db = Database::open(dir.path()).expect("open db");
    let strata = Strata::from_database(db).expect("strata");

    let before = wal_appends(&strata);

    // Read a nonexistent sequence (read-only, returns None)
    strata
        .executor()
        .execute(Command::EventRead {
            branch: None,
            sequence: 999,
        })
        .unwrap();

    let after = wal_appends(&strata);
    assert_eq!(
        after, before,
        "event/read (nonexistent) should produce 0 WAL appends, but produced {}",
        after - before
    );
}

#[test]
fn json_list_produces_no_wal_writes() {
    let dir = TempDir::new().expect("tempdir");
    std::fs::write(dir.path().join("strata.toml"), "durability = \"always\"\n").unwrap();
    let db = Database::open(dir.path()).expect("open db");
    let strata = Strata::from_database(db).expect("strata");

    // Create some JSON documents so the list has results
    for i in 0..5 {
        strata
            .executor()
            .execute(Command::JsonSet {
                branch: None,
                key: format!("doc:{}", i),
                path: "$".into(),
                value: serde_json::json!({"index": i}).into(),
            })
            .unwrap();
    }

    let before = wal_appends(&strata);

    // List JSON documents (read-only)
    let output = strata
        .executor()
        .execute(Command::JsonList {
            branch: None,
            prefix: None,
            cursor: None,
            limit: 100,
        })
        .unwrap();

    // Verify we got results
    if let strata_executor::Output::JsonListResult { keys, .. } = &output {
        assert_eq!(keys.len(), 5, "Should find 5 JSON documents");
    } else {
        panic!("Expected JsonListResult output");
    }

    let after = wal_appends(&strata);
    assert_eq!(
        after, before,
        "json/list should produce 0 WAL appends (read-only), but produced {}",
        after - before
    );
}

#[test]
fn json_list_with_prefix_produces_no_wal_writes() {
    let dir = TempDir::new().expect("tempdir");
    std::fs::write(dir.path().join("strata.toml"), "durability = \"always\"\n").unwrap();
    let db = Database::open(dir.path()).expect("open db");
    let strata = Strata::from_database(db).expect("strata");

    // Create documents with different prefixes
    for i in 0..3 {
        strata
            .executor()
            .execute(Command::JsonSet {
                branch: None,
                key: format!("alpha:{}", i),
                path: "$".into(),
                value: serde_json::json!({"v": i}).into(),
            })
            .unwrap();
    }
    for i in 0..2 {
        strata
            .executor()
            .execute(Command::JsonSet {
                branch: None,
                key: format!("beta:{}", i),
                path: "$".into(),
                value: serde_json::json!({"v": i}).into(),
            })
            .unwrap();
    }

    let before = wal_appends(&strata);

    // List with prefix filter (read-only)
    let output = strata
        .executor()
        .execute(Command::JsonList {
            branch: None,
            prefix: Some("alpha:".into()),
            cursor: None,
            limit: 100,
        })
        .unwrap();

    if let strata_executor::Output::JsonListResult { keys, .. } = &output {
        assert_eq!(keys.len(), 3, "Should find 3 alpha: documents");
    } else {
        panic!("Expected JsonListResult output");
    }

    let after = wal_appends(&strata);
    assert_eq!(
        after, before,
        "json/list with prefix should produce 0 WAL appends, but produced {}",
        after - before
    );
}
