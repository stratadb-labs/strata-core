//! Audit test for issue #940: JsonList and EventReadByType don't see uncommitted
//! transaction writes
//! Verdict: CONFIRMED BUG
//!
//! Session explicitly routes JsonList and EventReadByType through the executor
//! (bypassing the active transaction):
//!
//! From session.rs:107-108:
//! ```ignore
//! | Command::JsonList { .. }
//! | Command::EventReadByType { .. } => self.executor.execute(cmd),
//! ```
//!
//! This means these commands read from committed storage, not from the
//! transaction's write set. If a caller writes data within a transaction
//! and then calls these listing commands, the new data will NOT appear.
//!
//! The same issue applies to KvGetv, StateReadv, and JsonGetv (lines 104-106
//! in session.rs) which also bypass the transaction.

use strata_engine::database::Database;
use strata_executor::{BranchId, Command, Output, Session, Value};

/// Demonstrates that EventReadByType bypasses the transaction and does
/// not see events appended within the same transaction.
#[test]
fn issue_940_event_read_by_type_misses_uncommitted() {
    let db = Database::cache().unwrap();
    let mut session = Session::new(db);
    let branch = BranchId::from("default");

    // Begin transaction
    session
        .execute(Command::TxnBegin {
            branch: Some(branch.clone()),
            options: None,
        })
        .unwrap();

    // Append an event inside the transaction
    session
        .execute(Command::EventAppend {
            branch: Some(branch.clone()),
            event_type: "test_event".into(),
            payload: Value::String("payload".into()),
        })
        .unwrap();

    // EventReadByType bypasses the transaction — reads from committed storage
    let read_result = session
        .execute(Command::EventReadByType {
            branch: Some(branch.clone()),
            event_type: "test_event".into(),
            limit: None,
            after_sequence: None,
        })
        .unwrap();

    match read_result {
        Output::VersionedValues(events) => {
            // BUG: The appended event is not visible because
            // EventReadByType reads from committed storage
            if !events.is_empty() {
                // If events are found, the bug may be fixed
                panic!(
                    "EventReadByType sees uncommitted events - bug may be fixed. Found {} events.",
                    events.len()
                );
            }
            assert!(
                events.is_empty(),
                "EventReadByType should NOT see uncommitted events"
            );
        }
        other => panic!("Expected VersionedValues, got: {:?}", other),
    }

    // Commit the transaction
    session.execute(Command::TxnCommit).unwrap();
}

/// Demonstrates that JsonList is explicitly routed through executor,
/// bypassing any active transaction. This is visible in session.rs where
/// JsonList is in the "non-transactional commands" match arm.
#[test]
fn issue_940_json_list_routed_through_executor() {
    let db = Database::cache().unwrap();
    let mut session = Session::new(db);
    let branch = BranchId::from("default");

    // Create a JSON document OUTSIDE any transaction (committed)
    session
        .execute(Command::JsonSet {
            branch: Some(branch.clone()),
            key: "committed_doc".into(),
            path: "$".into(),
            value: Value::String("before-txn".into()),
        })
        .unwrap();

    // Verify JsonList sees the committed document
    let list_before = session
        .execute(Command::JsonList {
            branch: Some(branch.clone()),
            prefix: None,
            cursor: None,
            limit: 100,
        })
        .unwrap();

    match &list_before {
        Output::JsonListResult { keys, .. } => {
            assert!(
                keys.contains(&"committed_doc".to_string()),
                "JsonList should see committed doc"
            );
        }
        other => panic!("Expected JsonListResult, got: {:?}", other),
    }

    // Begin transaction
    session
        .execute(Command::TxnBegin {
            branch: Some(branch.clone()),
            options: None,
        })
        .unwrap();

    // JsonList inside a transaction still reads from committed storage
    // (it bypasses the transaction context)
    let list_in_txn = session
        .execute(Command::JsonList {
            branch: Some(branch.clone()),
            prefix: None,
            cursor: None,
            limit: 100,
        })
        .unwrap();

    match &list_in_txn {
        Output::JsonListResult { keys, .. } => {
            // It still sees committed data because it reads from executor
            assert!(
                keys.contains(&"committed_doc".to_string()),
                "JsonList should still see committed data inside txn"
            );
        }
        other => panic!("Expected JsonListResult, got: {:?}", other),
    }

    // BUG: session.rs:107 explicitly routes JsonList to self.executor.execute(cmd)
    // This means any in-transaction JSON writes (via JsonSet which IS transactional)
    // will not be visible to JsonList until after commit.
    //
    // The code in session.rs is:
    //   | Command::JsonList { .. }
    //   | Command::EventReadByType { .. } => self.executor.execute(cmd),
    //
    // The comment in session.rs says:
    //   "Version history commands require storage-layer version chains
    //    which are not available through the transaction context."
    // But JsonList and EventReadByType are not version history commands —
    // they are listing commands that should see transaction writes.

    session.execute(Command::TxnRollback).unwrap();
}

/// Demonstrates that KvGetv also bypasses the transaction, not seeing
/// version history changes made within the transaction.
#[test]
fn issue_940_kv_getv_bypasses_transaction() {
    let db = Database::cache().unwrap();
    let mut session = Session::new(db);
    let branch = BranchId::from("default");

    // Begin transaction
    session
        .execute(Command::TxnBegin {
            branch: Some(branch.clone()),
            options: None,
        })
        .unwrap();

    // Write a key inside the transaction
    session
        .execute(Command::KvPut {
            branch: Some(branch.clone()),
            key: "ver_key".into(),
            value: Value::Int(1),
        })
        .unwrap();

    // KvGet sees the uncommitted write
    let get = session
        .execute(Command::KvGet {
            branch: Some(branch.clone()),
            key: "ver_key".into(),
        })
        .unwrap();
    assert!(
        matches!(get, Output::Maybe(Some(_))),
        "KvGet should see uncommitted write"
    );

    // KvGetv bypasses the transaction (session.rs:104)
    let getv = session
        .execute(Command::KvGetv {
            branch: Some(branch.clone()),
            key: "ver_key".into(),
        })
        .unwrap();

    match getv {
        Output::VersionHistory(None) => {
            // BUG CONFIRMED: KvGetv does not see the uncommitted write
            // because it reads from committed storage
        }
        Output::VersionHistory(Some(history)) if history.is_empty() => {
            // Also confirms the bug — empty history means nothing committed yet
        }
        Output::VersionHistory(Some(history)) => {
            // If history is non-empty, the write somehow became visible
            panic!(
                "KvGetv should NOT see uncommitted version history, found {} entries",
                history.len()
            );
        }
        other => panic!("Expected VersionHistory, got: {:?}", other),
    }

    session.execute(Command::TxnRollback).unwrap();
}
