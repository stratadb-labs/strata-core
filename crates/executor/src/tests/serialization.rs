//! Serialization round-trip tests for Command and Output enums.
//!
//! These tests verify that all enum variants can be serialized to JSON
//! and deserialized back without loss of information.

use crate::types::*;
use crate::{Command, Output};
use crate::Value;

/// Helper to test round-trip serialization of a Command.
fn test_command_round_trip(cmd: Command) {
    let json = serde_json::to_string(&cmd).expect("Failed to serialize command");
    let restored: Command = serde_json::from_str(&json).expect("Failed to deserialize command");
    assert_eq!(
        serde_json::to_value(&cmd).unwrap(),
        serde_json::to_value(&restored).unwrap(),
        "Command round-trip failed for: {:?}",
        cmd
    );
}

/// Helper to test round-trip serialization of an Output.
fn test_output_round_trip(output: Output) {
    let json = serde_json::to_string(&output).expect("Failed to serialize output");
    let restored: Output = serde_json::from_str(&json).expect("Failed to deserialize output");

    // For outputs containing floats that might be NaN, compare JSON values
    let original_json = serde_json::to_value(&output).unwrap();
    let restored_json = serde_json::to_value(&restored).unwrap();

    // NaN != NaN, so we compare JSON representations
    assert_eq!(
        original_json.to_string(),
        restored_json.to_string(),
        "Output round-trip failed for: {:?}",
        output
    );
}

// =============================================================================
// Database Command Tests
// =============================================================================

#[test]
fn test_command_ping() {
    test_command_round_trip(Command::Ping);
}

#[test]
fn test_command_info() {
    test_command_round_trip(Command::Info);
}

#[test]
fn test_command_flush() {
    test_command_round_trip(Command::Flush);
}

#[test]
fn test_command_compact() {
    test_command_round_trip(Command::Compact);
}

// =============================================================================
// KV Command Tests (4 MVP)
// =============================================================================

#[test]
fn test_command_kv_put() {
    test_command_round_trip(Command::KvPut {
        run: Some(RunId::from("default")),
        key: "test-key".to_string(),
        value: Value::String("test-value".to_string()),
    });
}

#[test]
fn test_command_kv_get() {
    test_command_round_trip(Command::KvGet {
        run: Some(RunId::from("default")),
        key: "test-key".to_string(),
    });
}

#[test]
fn test_command_kv_delete() {
    test_command_round_trip(Command::KvDelete {
        run: Some(RunId::from("default")),
        key: "test-key".to_string(),
    });
}

#[test]
fn test_command_kv_list() {
    test_command_round_trip(Command::KvList {
        run: Some(RunId::from("default")),
        prefix: Some("user:".to_string()),
    });
}

// =============================================================================
// JSON Command Tests
// =============================================================================

#[test]
fn test_command_json_set() {
    test_command_round_trip(Command::JsonSet {
        run: Some(RunId::from("default")),
        key: "doc1".to_string(),
        path: "$.name".to_string(),
        value: Value::String("Alice".to_string()),
    });
}

#[test]
fn test_command_json_get() {
    test_command_round_trip(Command::JsonGet {
        run: Some(RunId::from("default")),
        key: "doc1".to_string(),
        path: "$.name".to_string(),
    });
}

// =============================================================================
// Event Command Tests (4 MVP)
// =============================================================================

#[test]
fn test_command_event_append() {
    test_command_round_trip(Command::EventAppend {
        run: Some(RunId::from("default")),
        event_type: "events".to_string(),
        payload: Value::Object(
            [("type".to_string(), Value::String("click".to_string()))]
                .into_iter()
                .collect(),
        ),
    });
}

#[test]
fn test_command_event_read() {
    test_command_round_trip(Command::EventRead {
        run: Some(RunId::from("default")),
        sequence: 42,
    });
}

#[test]
fn test_command_event_read_by_type() {
    test_command_round_trip(Command::EventReadByType {
        run: Some(RunId::from("default")),
        event_type: "events".to_string(),
    });
}

#[test]
fn test_command_event_len() {
    test_command_round_trip(Command::EventLen {
        run: Some(RunId::from("default")),
    });
}

// =============================================================================
// State Command Tests
// =============================================================================

#[test]
fn test_command_state_set() {
    test_command_round_trip(Command::StateSet {
        run: Some(RunId::from("default")),
        cell: "counter".to_string(),
        value: Value::Int(42),
    });
}

#[test]
fn test_command_state_cas() {
    test_command_round_trip(Command::StateCas {
        run: Some(RunId::from("default")),
        cell: "counter".to_string(),
        expected_counter: Some(5),
        value: Value::Int(6),
    });
}

// =============================================================================
// Vector Command Tests
// =============================================================================

#[test]
fn test_command_vector_upsert() {
    test_command_round_trip(Command::VectorUpsert {
        run: Some(RunId::from("default")),
        collection: "embeddings".to_string(),
        key: "vec1".to_string(),
        vector: vec![0.1, 0.2, 0.3, 0.4],
        metadata: Some(Value::Object(
            [("label".to_string(), Value::String("test".to_string()))]
                .into_iter()
                .collect(),
        )),
    });
}

#[test]
fn test_command_vector_search() {
    test_command_round_trip(Command::VectorSearch {
        run: Some(RunId::from("default")),
        collection: "embeddings".to_string(),
        query: vec![0.1, 0.2, 0.3, 0.4],
        k: 10,
        filter: None,
        metric: Some(DistanceMetric::Cosine),
    });
}

#[test]
fn test_command_vector_create_collection() {
    test_command_round_trip(Command::VectorCreateCollection {
        run: Some(RunId::from("default")),
        collection: "embeddings".to_string(),
        dimension: 384,
        metric: DistanceMetric::Cosine,
    });
}

// =============================================================================
// Run Command Tests
// =============================================================================

#[test]
fn test_command_run_create() {
    test_command_round_trip(Command::RunCreate {
        run_id: Some("my-run".to_string()),
        metadata: Some(Value::Object(
            [("name".to_string(), Value::String("Test Run".to_string()))]
                .into_iter()
                .collect(),
        )),
    });
}

#[test]
fn test_command_run_list() {
    test_command_round_trip(Command::RunList {
        state: Some(RunStatus::Active),
        limit: Some(10),
        offset: Some(0),
    });
}

// =============================================================================
// Transaction Command Tests
// =============================================================================

#[test]
fn test_command_txn_begin() {
    test_command_round_trip(Command::TxnBegin {
        run: None,
        options: Some(TxnOptions { read_only: true }),
    });
}

#[test]
fn test_command_txn_commit() {
    test_command_round_trip(Command::TxnCommit);
}

#[test]
fn test_command_txn_rollback() {
    test_command_round_trip(Command::TxnRollback);
}

// =============================================================================
// Output Tests
// =============================================================================

#[test]
fn test_output_unit() {
    test_output_round_trip(Output::Unit);
}

#[test]
fn test_output_bool() {
    test_output_round_trip(Output::Bool(true));
    test_output_round_trip(Output::Bool(false));
}

#[test]
fn test_output_int() {
    test_output_round_trip(Output::Int(42));
    test_output_round_trip(Output::Int(-100));
}

#[test]
fn test_output_uint() {
    test_output_round_trip(Output::Uint(12345));
}

#[test]
fn test_output_float() {
    test_output_round_trip(Output::Float(3.14159));
}

#[test]
fn test_output_version() {
    test_output_round_trip(Output::Version(42));
}

#[test]
fn test_output_versioned() {
    test_output_round_trip(Output::Versioned(VersionedValue {
        value: Value::String("test".to_string()),
        version: 1,
        timestamp: 1000000,
    }));
}

#[test]
fn test_output_maybe_versioned() {
    test_output_round_trip(Output::MaybeVersioned(Some(VersionedValue {
        value: Value::Int(42),
        version: 5,
        timestamp: 2000000,
    })));
    test_output_round_trip(Output::MaybeVersioned(None));
}

#[test]
fn test_output_keys() {
    test_output_round_trip(Output::Keys(vec![
        "key1".to_string(),
        "key2".to_string(),
        "key3".to_string(),
    ]));
}

#[test]
fn test_output_strings() {
    test_output_round_trip(Output::Strings(vec![
        "stream1".to_string(),
        "stream2".to_string(),
    ]));
}

#[test]
fn test_output_versions() {
    test_output_round_trip(Output::Versions(vec![1, 2, 3, 4, 5]));
}

#[test]
fn test_output_versioned_values() {
    test_output_round_trip(Output::VersionedValues(vec![
        VersionedValue {
            value: Value::Int(1),
            version: 1,
            timestamp: 1000,
        },
        VersionedValue {
            value: Value::Int(2),
            version: 2,
            timestamp: 2000,
        },
    ]));
}

#[test]
fn test_output_kv_scan_result() {
    test_output_round_trip(Output::KvScanResult {
        entries: vec![(
            "key1".to_string(),
            VersionedValue {
                value: Value::String("value1".to_string()),
                version: 1,
                timestamp: 1000,
            },
        )],
        cursor: Some("next-cursor".to_string()),
    });
}

#[test]
fn test_output_vector_matches() {
    test_output_round_trip(Output::VectorMatches(vec![VectorMatch {
        key: "vec1".to_string(),
        score: 0.95,
        metadata: Some(Value::String("test".to_string())),
    }]));
}

#[test]
fn test_output_run_info() {
    test_output_round_trip(Output::RunWithVersion {
        info: RunInfo {
            id: RunId::from("test-run"),
            status: RunStatus::Active,
            metadata: Some(Value::Null),
            created_at: 1000000,
            updated_at: 1000000,
            parent_id: None,
            tags: vec!["test".to_string()],
        },
        version: 1,
    });
}

#[test]
fn test_output_pong() {
    test_output_round_trip(Output::Pong {
        version: "0.1.0".to_string(),
    });
}

#[test]
fn test_output_database_info() {
    test_output_round_trip(Output::DatabaseInfo(DatabaseInfo {
        version: "0.1.0".to_string(),
        uptime_secs: 3600,
        run_count: 10,
        total_keys: 1000,
    }));
}

// =============================================================================
// Complex Value Serialization Tests
// =============================================================================

#[test]
fn test_command_with_complex_value() {
    let complex_value = Value::Object(
        [
            ("string".to_string(), Value::String("hello".to_string())),
            ("int".to_string(), Value::Int(42)),
            ("float".to_string(), Value::Float(3.14)),
            ("bool".to_string(), Value::Bool(true)),
            ("null".to_string(), Value::Null),
            (
                "array".to_string(),
                Value::Array(vec![Value::Int(1), Value::Int(2), Value::Int(3)]),
            ),
            (
                "nested".to_string(),
                Value::Object(
                    [("deep".to_string(), Value::String("value".to_string()))]
                        .into_iter()
                        .collect(),
                ),
            ),
        ]
        .into_iter()
        .collect(),
    );

    test_command_round_trip(Command::KvPut {
        run: Some(RunId::from("default")),
        key: "complex".to_string(),
        value: complex_value,
    });
}

#[test]
fn test_command_with_bytes_value() {
    test_command_round_trip(Command::KvPut {
        run: Some(RunId::from("default")),
        key: "binary".to_string(),
        value: Value::Bytes(vec![0, 1, 2, 255, 254, 253]),
    });
}

// =============================================================================
// Optional Run Serialization Tests
// =============================================================================

#[test]
fn test_command_with_run_none_round_trip() {
    // Commands with run: None should serialize without a run field
    // and deserialize back to run: None
    let cmd = Command::KvPut {
        run: None,
        key: "test".to_string(),
        value: Value::Int(42),
    };
    let json = serde_json::to_string(&cmd).unwrap();
    assert!(!json.contains("run"), "run: None should be skipped in serialization");
    let restored: Command = serde_json::from_str(&json).unwrap();
    match restored {
        Command::KvPut { run, key, value } => {
            assert!(run.is_none(), "run should deserialize as None when omitted");
            assert_eq!(key, "test");
            assert_eq!(value, Value::Int(42));
        }
        _ => panic!("Wrong command variant"),
    }
}

#[test]
fn test_command_with_run_some_round_trip() {
    // Commands with run: Some(...) should include the run field
    let cmd = Command::KvGet {
        run: Some(RunId::from("my-run")),
        key: "test".to_string(),
    };
    let json = serde_json::to_string(&cmd).unwrap();
    assert!(json.contains("run"), "run: Some should be included in serialization");
    let restored: Command = serde_json::from_str(&json).unwrap();
    match restored {
        Command::KvGet { run, key } => {
            assert_eq!(run, Some(RunId::from("my-run")));
            assert_eq!(key, "test");
        }
        _ => panic!("Wrong command variant"),
    }
}

#[test]
fn test_command_json_omitted_run_deserializes() {
    // Verify that JSON without a "run" field deserializes to run: None
    let json = r#"{"KvPut":{"key":"foo","value":{"Int":42}}}"#;
    let cmd: Command = serde_json::from_str(json).unwrap();
    match cmd {
        Command::KvPut { run, key, value } => {
            assert!(run.is_none());
            assert_eq!(key, "foo");
            assert_eq!(value, Value::Int(42));
        }
        _ => panic!("Wrong command variant"),
    }
}

#[test]
fn test_command_json_explicit_run_deserializes() {
    // Verify that JSON with "run": "default" still works
    let json = r#"{"KvPut":{"run":"default","key":"foo","value":{"Int":42}}}"#;
    let cmd: Command = serde_json::from_str(json).unwrap();
    match cmd {
        Command::KvPut { run, key, value } => {
            assert_eq!(run, Some(RunId::from("default")));
            assert_eq!(key, "foo");
            assert_eq!(value, Value::Int(42));
        }
        _ => panic!("Wrong command variant"),
    }
}
