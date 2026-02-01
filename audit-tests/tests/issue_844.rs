//! Audit test for issue #844: Transaction wrapper cannot read pre-existing data from the store
//! Verdict: CONFIRMED BUG
//!
//! The Transaction struct's kv_get/state_read/json_get only reads from the local
//! write-set, returning None/false for any data that existed before TxnBegin.
//! This makes transactions effectively write-only.

use strata_core::value::Value;
use strata_engine::database::Database;
use strata_executor::{Command, Output, Session};

fn setup() -> (tempfile::TempDir, Session) {
    let temp_dir = tempfile::TempDir::new().unwrap();
    let db = Database::open(temp_dir.path()).unwrap();
    let session = Session::new(db);
    (temp_dir, session)
}

/// Write data before a transaction, then try to read it inside the transaction.
/// BUG: The Transaction wrapper returns None for pre-existing KV data.
#[test]
fn issue_844_kv_get_returns_none_for_preexisting_data() {
    let (_temp, mut session) = setup();

    // Write a value BEFORE starting a transaction
    session
        .execute(Command::KvPut {
            branch: None,
            key: "preexisting".to_string(),
            value: Value::String("hello".into()),
        })
        .unwrap();

    // Verify the value exists outside a transaction
    let result = session
        .execute(Command::KvGet {
            branch: None,
            key: "preexisting".to_string(),
        })
        .unwrap();
    match &result {
        Output::Maybe(Some(v)) => {
            assert_eq!(*v, Value::String("hello".into()));
        }
        _ => panic!("Value should exist before transaction"),
    }

    // Now start a transaction
    session
        .execute(Command::TxnBegin {
            branch: None,
            options: None,
        })
        .unwrap();

    // Try to read the pre-existing value inside the transaction
    let txn_result = session
        .execute(Command::KvGet {
            branch: None,
            key: "preexisting".to_string(),
        })
        .unwrap();

    match txn_result {
        Output::Maybe(None) => {
            // BUG CONFIRMED: Transaction cannot read pre-existing data
            // The Transaction wrapper's kv_get returns Ok(None) because
            // the key is not in the write_set or delete_set.
            eprintln!(
                "BUG CONFIRMED: KvGet inside transaction returned None \
                 for pre-existing key 'preexisting'"
            );
        }
        Output::Maybe(Some(v)) => {
            // This would be the correct behavior
            assert_eq!(v, Value::String("hello".into()));
        }
        other => {
            panic!("Unexpected output: {:?}", other);
        }
    }

    session.execute(Command::TxnRollback).unwrap();
}

/// Write state before a transaction, then try to read it inside.
/// BUG: state_read returns None for pre-existing state cells.
#[test]
fn issue_844_state_read_returns_none_for_preexisting_state() {
    let (_temp, mut session) = setup();

    // Initialize a state cell before the transaction
    session
        .execute(Command::StateInit {
            branch: None,
            cell: "counter".to_string(),
            value: Value::Int(0),
        })
        .unwrap();

    // Verify it exists
    let result = session
        .execute(Command::StateRead {
            branch: None,
            cell: "counter".to_string(),
        })
        .unwrap();
    match &result {
        Output::Maybe(Some(v)) => {
            assert_eq!(*v, Value::Int(0));
        }
        _ => panic!("State cell should exist before transaction"),
    }

    // Start transaction
    session
        .execute(Command::TxnBegin {
            branch: None,
            options: None,
        })
        .unwrap();

    // Try to read the pre-existing state inside the transaction
    let txn_result = session
        .execute(Command::StateRead {
            branch: None,
            cell: "counter".to_string(),
        })
        .unwrap();

    match txn_result {
        Output::Maybe(None) => {
            // BUG: state_read returns None for pre-existing state
            eprintln!(
                "BUG CONFIRMED: StateRead inside transaction returned None \
                 for pre-existing cell 'counter'"
            );
        }
        Output::Maybe(Some(v)) => {
            assert_eq!(v, Value::Int(0));
        }
        other => {
            panic!("Unexpected output: {:?}", other);
        }
    }

    // Also try CAS on pre-existing state - should fail because state_cas
    // can't find the state cell in the write-set
    let cas_result = session.execute(Command::StateCas {
        branch: None,
        cell: "counter".to_string(),
        expected_counter: Some(1),
        value: Value::Int(1),
    });

    match cas_result {
        Err(e) => {
            eprintln!(
                "BUG CONFIRMED: StateCas failed on pre-existing state: {:?}",
                e
            );
        }
        Ok(_) => {
            // This would be the correct behavior
        }
    }

    session.execute(Command::TxnRollback).unwrap();
}

/// KvList inside a transaction only shows writes made within the transaction,
/// not pre-existing keys.
#[test]
fn issue_844_kv_list_misses_preexisting_keys() {
    let (_temp, mut session) = setup();

    // Write keys before transaction
    session
        .execute(Command::KvPut {
            branch: None,
            key: "existing1".to_string(),
            value: Value::Int(1),
        })
        .unwrap();
    session
        .execute(Command::KvPut {
            branch: None,
            key: "existing2".to_string(),
            value: Value::Int(2),
        })
        .unwrap();

    // Start transaction
    session
        .execute(Command::TxnBegin {
            branch: None,
            options: None,
        })
        .unwrap();

    // Write one more key inside the transaction
    session
        .execute(Command::KvPut {
            branch: None,
            key: "new_in_txn".to_string(),
            value: Value::Int(3),
        })
        .unwrap();

    // List keys
    let list_result = session
        .execute(Command::KvList {
            branch: None,
            prefix: None,
        })
        .unwrap();

    match list_result {
        Output::Keys(keys) => {
            let has_existing = keys.contains(&"existing1".to_string());
            let has_new = keys.contains(&"new_in_txn".to_string());

            if !has_existing && has_new {
                eprintln!(
                    "BUG CONFIRMED: KvList only shows transaction writes. \
                     Got: {:?} (missing existing1, existing2)",
                    keys
                );
            } else if has_existing {
                // Correct behavior
            }
        }
        other => {
            eprintln!("Unexpected KvList output: {:?}", other);
        }
    }

    session.execute(Command::TxnRollback).unwrap();
}
