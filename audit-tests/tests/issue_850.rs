//! Audit test for issue #850: Vector handler does not validate key format
//! Verdict: FIXED
//!
//! Vector operations now validate keys using the same `validate_key()` function
//! as KV/State handlers. Empty keys, reserved-prefix keys, and keys with NUL
//! bytes are all rejected.

use strata_engine::Database;
use strata_executor::{Command, Executor, Value};

fn setup() -> Executor {
    let db = Database::cache().unwrap();
    Executor::new(db)
}

#[test]
fn issue_850_vector_upsert_rejects_empty_key() {
    let executor = setup();

    // KV put correctly rejects empty key
    let kv_result = executor.execute(Command::KvPut {
        branch: None,
        key: "".to_string(),
        value: Value::Int(1),
    });
    assert!(kv_result.is_err(), "KV put should reject empty key");

    // Vector upsert now also rejects empty key (FIXED)
    let vec_result = executor.execute(Command::VectorUpsert {
        branch: None,
        collection: "test_coll".to_string(),
        key: "".to_string(),
        vector: vec![1.0, 2.0, 3.0],
        metadata: None,
    });
    assert!(vec_result.is_err(), "Vector upsert should reject empty key");
}

#[test]
fn issue_850_vector_upsert_rejects_nul_bytes_in_key() {
    let executor = setup();

    // KV put correctly rejects key with NUL bytes
    let kv_result = executor.execute(Command::KvPut {
        branch: None,
        key: "hello\0world".to_string(),
        value: Value::Int(1),
    });
    assert!(
        kv_result.is_err(),
        "KV put should reject key with NUL bytes"
    );

    // Vector upsert now also rejects NUL bytes via validate_key() (FIXED)
    let vec_result = executor.execute(Command::VectorUpsert {
        branch: None,
        collection: "test_coll".to_string(),
        key: "hello\0world".to_string(),
        vector: vec![1.0, 2.0, 3.0],
        metadata: None,
    });
    assert!(
        vec_result.is_err(),
        "Vector upsert should reject key with NUL bytes"
    );
}

#[test]
fn issue_850_vector_upsert_rejects_reserved_prefix_key() {
    let executor = setup();

    // KV put correctly rejects reserved prefix
    let kv_result = executor.execute(Command::KvPut {
        branch: None,
        key: "_strata/internal".to_string(),
        value: Value::Int(1),
    });
    assert!(
        kv_result.is_err(),
        "KV put should reject reserved prefix key"
    );

    // Vector upsert now also rejects reserved prefix (FIXED)
    let vec_result = executor.execute(Command::VectorUpsert {
        branch: None,
        collection: "test_coll".to_string(),
        key: "_strata/internal".to_string(),
        vector: vec![1.0, 2.0, 3.0],
        metadata: None,
    });
    assert!(
        vec_result.is_err(),
        "Vector upsert should reject reserved prefix key"
    );
}
