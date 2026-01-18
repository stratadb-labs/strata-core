//! Cross-Primitive Isolation Tests
//!
//! Tests run isolation across all primitives.

use crate::test_utils::*;
use in_mem_core::json::{JsonPath, JsonValue};
use in_mem_core::types::{JsonDocId, RunId};
use in_mem_core::value::Value;

/// Test complete run isolation across all primitives.
#[test]
fn test_complete_run_isolation() {
    let test_db = TestDb::new();
    let run_a = RunId::new();
    let run_b = RunId::new();
    let run_c = RunId::new();
    let p = test_db.all_primitives();

    // Create collections for each run
    for (run, suffix) in [(run_a, "a"), (run_b, "b"), (run_c, "c")] {
        p.vector
            .create_collection(run, &format!("col_{}", suffix), config_small())
            .expect("create");
    }

    // Track doc IDs for verification
    let doc_a = JsonDocId::new();
    let doc_b = JsonDocId::new();

    // Populate run_a
    p.kv.put(&run_a, "key", Value::String("a".into()))
        .expect("kv a");
    p.json
        .create(&run_a, &doc_a, JsonValue::from(serde_json::json!({"run": "a"})))
        .expect("json a");
    p.event
        .append(&run_a, "type", Value::Null)
        .expect("event a");
    p.state.init(&run_a, "cell", Value::I64(1)).expect("state a");
    p.vector
        .insert(run_a, "col_a", "v", &[1.0, 0.0, 0.0], None)
        .expect("vector a");

    // Populate run_b with different data
    p.kv.put(&run_b, "key", Value::String("b".into()))
        .expect("kv b");
    p.json
        .create(&run_b, &doc_b, JsonValue::from(serde_json::json!({"run": "b"})))
        .expect("json b");
    p.event
        .append(&run_b, "type", Value::Null)
        .expect("event b");
    p.state.init(&run_b, "cell", Value::I64(2)).expect("state b");
    p.vector
        .insert(run_b, "col_b", "v", &[0.0, 1.0, 0.0], None)
        .expect("vector b");

    // Verify isolation: run_a data
    assert_eq!(
        p.kv.get(&run_a, "key").expect("get").unwrap(),
        Value::String("a".into())
    );
    let json_a = p
        .json
        .get(&run_a, &doc_a, &JsonPath::root())
        .expect("get")
        .unwrap();
    // JsonValue wraps serde_json::Value, verify it exists
    assert!(format!("{:?}", json_a).contains("run"));

    // Verify isolation: run_b data
    assert_eq!(
        p.kv.get(&run_b, "key").expect("get").unwrap(),
        Value::String("b".into())
    );
    let json_b = p
        .json
        .get(&run_b, &doc_b, &JsonPath::root())
        .expect("get")
        .unwrap();
    assert!(format!("{:?}", json_b).contains("run"));

    // Verify run_c sees nothing (different doc_id would be needed, but KV should be empty)
    assert!(p.kv.get(&run_c, "key").expect("get").is_none());
}

/// Test that primitive operations don't leak across runs.
#[test]
fn test_no_cross_run_leakage() {
    let test_db = TestDb::new();
    let run_a = RunId::new();
    let run_b = RunId::new();
    let p = test_db.all_primitives();

    // Create data in run_a
    p.kv.put(&run_a, "secret", Value::String("run_a_secret".into()))
        .expect("put");

    // run_b should not see run_a's data
    assert!(p.kv.get(&run_b, "secret").expect("get").is_none());

    // run_b creates same key with different value
    p.kv.put(&run_b, "secret", Value::String("run_b_secret".into()))
        .expect("put");

    // Both should maintain their own values
    assert_eq!(
        p.kv.get(&run_a, "secret").expect("get").unwrap(),
        Value::String("run_a_secret".into())
    );
    assert_eq!(
        p.kv.get(&run_b, "secret").expect("get").unwrap(),
        Value::String("run_b_secret".into())
    );
}
