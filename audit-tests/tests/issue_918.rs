//! Audit test for issue #918: Inconsistent serialization formats across primitives
//! Verdict: ARCHITECTURAL CHOICE (questionable)
//!
//! Different primitives use different serialization formats for storing values:
//! - KV: Stores Value directly (in-memory representation)
//! - State: Uses serde_json::to_string (JSON text)
//! - Event: Uses serde_json (JSON text)
//! - JSON: Uses rmp_serde (MessagePack binary)
//!
//! This inconsistency means cross-primitive operations cannot share a common
//! serialization/deserialization path, and debugging raw storage requires
//! knowing which primitive wrote the data.
//!
//! This test demonstrates that all four primitives successfully store and
//! retrieve data, but notes the format inconsistency.

use strata_executor::{Command, Output};

/// Demonstrate that each primitive can successfully round-trip data,
/// despite using different internal serialization formats.
#[test]
fn issue_918_all_primitives_store_and_retrieve_successfully() {
    let db = strata_engine::database::Database::cache().unwrap();
    let executor = strata_executor::Executor::new(db);

    let branch = strata_executor::BranchId::from("default");

    // === KV: stores Value directly ===
    executor
        .execute(Command::KvPut {
            branch: Some(branch.clone()),
            key: "kv_key".into(),
            value: strata_core::value::Value::String("kv_value".into()),
        })
        .unwrap();

    let kv_result = executor
        .execute(Command::KvGet {
            branch: Some(branch.clone()),
            key: "kv_key".into(),
        })
        .unwrap();
    match kv_result {
        Output::MaybeVersioned(Some(vv)) => {
            assert_eq!(
                vv.value,
                strata_core::value::Value::String("kv_value".into()),
                "KV should round-trip the value exactly"
            );
        }
        other => panic!("Expected MaybeVersioned(Some), got: {:?}", other),
    }

    // === State: internally serializes via serde_json::to_string ===
    executor
        .execute(Command::StateInit {
            branch: Some(branch.clone()),
            cell: "state_cell".into(),
            value: strata_core::value::Value::Int(42),
        })
        .unwrap();

    let state_result = executor
        .execute(Command::StateRead {
            branch: Some(branch.clone()),
            cell: "state_cell".into(),
        })
        .unwrap();
    match state_result {
        Output::MaybeVersioned(Some(vv)) => {
            assert_eq!(
                vv.value,
                strata_core::value::Value::Int(42),
                "State should round-trip the value"
            );
        }
        other => panic!("Expected MaybeVersioned(Some), got: {:?}", other),
    }

    // === Event: internally serializes via serde_json ===
    executor
        .execute(Command::EventAppend {
            branch: Some(branch.clone()),
            event_type: "test_event".into(),
            payload: strata_core::value::Value::Object(
                vec![(
                    "data".to_string(),
                    strata_core::value::Value::String("event_payload".into()),
                )]
                .into_iter()
                .collect(),
            ),
        })
        .unwrap();

    let event_result = executor
        .execute(Command::EventRead {
            branch: Some(branch.clone()),
            sequence: 0,
        })
        .unwrap();
    match event_result {
        Output::MaybeVersioned(Some(vv)) => {
            // Event payloads are returned with version info
            match &vv.value {
                strata_core::value::Value::Object(map) => {
                    assert!(
                        map.contains_key("data"),
                        "Event payload should contain 'data' field"
                    );
                }
                other => panic!("Expected Object payload, got: {:?}", other),
            }
        }
        other => panic!("Expected MaybeVersioned(Some), got: {:?}", other),
    }

    // === JSON: internally serializes via rmp_serde (MessagePack) ===
    executor
        .execute(Command::JsonSet {
            branch: Some(branch.clone()),
            key: "json_doc".into(),
            path: "$".into(),
            value: strata_core::value::Value::Object(
                vec![(
                    "field".to_string(),
                    strata_core::value::Value::String("json_value".into()),
                )]
                .into_iter()
                .collect(),
            ),
        })
        .unwrap();

    let json_result = executor
        .execute(Command::JsonGet {
            branch: Some(branch.clone()),
            key: "json_doc".into(),
            path: "$".into(),
        })
        .unwrap();
    match json_result {
        Output::MaybeVersioned(Some(vv)) => match &vv.value {
            strata_core::value::Value::Object(map) => {
                assert!(
                    map.contains_key("field"),
                    "JSON document should contain 'field'"
                );
            }
            other => panic!("Expected Object from JsonGet, got: {:?}", other),
        },
        other => panic!("Expected MaybeVersioned(Some), got: {:?}", other),
    }

    // ARCHITECTURAL NOTE:
    // All four primitives successfully store and retrieve data.
    // However, if you were to inspect the raw storage layer:
    //   - KV entries are stored as the native Value enum variant
    //   - State entries are stored as Value::String containing JSON text
    //   - Event entries are stored as Value::String containing JSON text
    //   - JSON entries are stored as Value::Bytes containing MessagePack binary
    //
    // This inconsistency is an architectural choice. It makes cross-primitive
    // tools (backup, migration, debugging) more complex because each primitive
    // requires its own deserialization logic.
}
