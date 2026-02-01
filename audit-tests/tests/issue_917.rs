//! Audit test for issue #917: JsonGet root path in transaction returns raw serialized bytes
//! instead of deserialized Value
//! Verdict: FIXED
//!
//! The session.rs `dispatch_in_txn` handler for JsonGet with root path was deserializing
//! the MessagePack bytes as a generic `JsonValue` instead of as a `JsonDoc` struct.
//! Since MessagePack represents structs as arrays, this returned the full internal record
//! (id, value, version, timestamps) as an Array rather than just the document value.
//!
//! Fix: Deserialize as `JsonDoc` and extract `.value` before converting to output.

use strata_executor::{Command, Output, Session};

/// Verify that JsonGet with root path "$" inside a transaction returns a
/// deserialized Value::Object (not raw bytes or internal struct fields).
#[test]
fn issue_917_json_get_root_path_in_transaction() {
    let db = strata_engine::database::Database::cache().unwrap();
    let mut session = Session::new(db);

    let branch = strata_executor::BranchId::from("default");

    // Create a JSON document outside the transaction
    let result = session
        .execute(Command::JsonSet {
            branch: Some(branch.clone()),
            key: "doc1".into(),
            path: "$".into(),
            value: strata_core::value::Value::Object(
                vec![
                    (
                        "name".to_string(),
                        strata_core::value::Value::String("Alice".into()),
                    ),
                    ("age".to_string(), strata_core::value::Value::Int(30)),
                ]
                .into_iter()
                .collect(),
            ),
        })
        .unwrap();
    assert!(
        matches!(result, Output::Version(_)),
        "JsonSet should return a version"
    );

    // Begin a transaction
    session
        .execute(Command::TxnBegin {
            branch: Some(branch.clone()),
            options: None,
        })
        .unwrap();

    // JsonGet with root path inside the transaction
    let result = session
        .execute(Command::JsonGet {
            branch: Some(branch.clone()),
            key: "doc1".into(),
            path: "$".into(),
        })
        .unwrap();

    match result {
        Output::Maybe(Some(val)) => {
            // FIXED: The value should be a properly deserialized Object
            assert!(
                matches!(&val, strata_core::value::Value::Object(_)),
                "JsonGet root path in transaction should return Object. Got: {:?}",
                val
            );

            // Verify the content is correct
            if let strata_core::value::Value::Object(map) = &val {
                assert_eq!(
                    map.get("name"),
                    Some(&strata_core::value::Value::String("Alice".into()))
                );
                assert_eq!(map.get("age"), Some(&strata_core::value::Value::Int(30)));
            }
        }
        Output::Maybe(None) => {
            panic!("JsonGet should find the document that was just set");
        }
        other => {
            panic!("Unexpected output from JsonGet: {:?}", other);
        }
    }

    // Commit the transaction
    session.execute(Command::TxnCommit).unwrap();
}

/// Confirm that JSON root path works correctly OUTSIDE a transaction too.
#[test]
fn issue_917_json_get_root_path_outside_transaction_works() {
    let db = strata_engine::database::Database::cache().unwrap();
    let mut session = Session::new(db);

    let branch = strata_executor::BranchId::from("default");

    // Create a JSON document
    session
        .execute(Command::JsonSet {
            branch: Some(branch.clone()),
            key: "doc2".into(),
            path: "$".into(),
            value: strata_core::value::Value::Object(
                vec![(
                    "status".to_string(),
                    strata_core::value::Value::String("active".into()),
                )]
                .into_iter()
                .collect(),
            ),
        })
        .unwrap();

    // Read outside transaction -- delegates to executor, which uses the JSON primitive
    // properly and returns a deserialized value
    let result = session
        .execute(Command::JsonGet {
            branch: Some(branch.clone()),
            key: "doc2".into(),
            path: "$".into(),
        })
        .unwrap();

    match result {
        Output::MaybeVersioned(Some(vv)) => {
            assert!(
                matches!(&vv.value, strata_core::value::Value::Object(_)),
                "JsonGet root path outside transaction should return Object. Got: {:?}",
                vv.value
            );
        }
        Output::Maybe(Some(val)) => {
            assert!(
                matches!(&val, strata_core::value::Value::Object(_)),
                "JsonGet root path outside transaction should return Object. Got: {:?}",
                val
            );
        }
        other => panic!("Expected MaybeVersioned(Some(Object)), got: {:?}", other),
    }
}
