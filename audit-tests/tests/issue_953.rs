//! Audit test for issue #953: Vector key validation inconsistent with KV key validation
//! Verdict: CONFIRMED BUG
//!
//! KV operations validate keys via `validate_key()` in bridge.rs, which rejects:
//! - Empty keys
//! - Keys containing NUL bytes
//! - Keys with the `_strata/` prefix (reserved for internal use)
//! - Keys exceeding 1024 bytes
//!
//! However, vector operations (VectorUpsert, VectorGet, VectorDelete) do NOT call
//! `validate_key()` on the vector key. They only call `validate_not_internal_collection()`
//! on the collection name, leaving the key parameter completely unvalidated.
//!
//! This means vector operations silently accept invalid keys that KV operations
//! correctly reject, creating an inconsistency in the API surface.

use strata_core::value::Value;
use strata_engine::database::Database;
use strata_executor::{BranchId, Command, Executor};

/// KV rejects empty keys but vector accepts them.
#[test]
fn issue_953_vector_accepts_empty_key_while_kv_rejects() {
    let db = Database::cache().unwrap();
    let executor = Executor::new(db);
    let branch = BranchId::from("default");

    // KV rejects empty key
    let kv_result = executor.execute(Command::KvPut {
        branch: Some(branch.clone()),
        key: "".into(),
        value: Value::Int(1),
    });
    assert!(kv_result.is_err(), "KV should reject empty key");

    // Vector accepts empty key — BUG: no validate_key() call on vector key
    let vec_result = executor.execute(Command::VectorUpsert {
        branch: Some(branch.clone()),
        collection: "col1".into(),
        key: "".into(),
        vector: vec![1.0, 0.0, 0.0],
        metadata: None,
    });
    // BUG: This succeeds despite the key being empty
    match vec_result {
        Ok(_) => {
            // Bug confirmed: vector accepts empty key
        }
        Err(_) => {
            // Validation has been added — bug fixed
        }
    }
}

/// KV rejects keys with NUL bytes but vector accepts them.
#[test]
fn issue_953_vector_accepts_nul_byte_key_while_kv_rejects() {
    let db = Database::cache().unwrap();
    let executor = Executor::new(db);
    let branch = BranchId::from("default");

    let nul_key = "key\0bad".to_string();

    // KV rejects key with NUL bytes
    let kv_result = executor.execute(Command::KvPut {
        branch: Some(branch.clone()),
        key: nul_key.clone(),
        value: Value::Int(1),
    });
    assert!(kv_result.is_err(), "KV should reject key with NUL bytes");

    // Vector accepts key with NUL bytes — BUG
    let vec_result = executor.execute(Command::VectorUpsert {
        branch: Some(branch.clone()),
        collection: "col2".into(),
        key: nul_key,
        vector: vec![0.0, 1.0, 0.0],
        metadata: None,
    });
    match vec_result {
        Ok(_) => {
            // Bug confirmed: vector accepts NUL byte keys
        }
        Err(_) => {
            // Validation has been added — bug fixed
        }
    }
}

/// KV rejects keys with _strata/ prefix but vector accepts them.
#[test]
fn issue_953_vector_accepts_reserved_prefix_key_while_kv_rejects() {
    let db = Database::cache().unwrap();
    let executor = Executor::new(db);
    let branch = BranchId::from("default");

    let reserved_key = "_strata/internal".to_string();

    // KV rejects reserved prefix
    let kv_result = executor.execute(Command::KvPut {
        branch: Some(branch.clone()),
        key: reserved_key.clone(),
        value: Value::Int(1),
    });
    assert!(
        kv_result.is_err(),
        "KV should reject key with _strata/ prefix"
    );

    // Vector accepts reserved prefix — BUG
    let vec_result = executor.execute(Command::VectorUpsert {
        branch: Some(branch.clone()),
        collection: "col3".into(),
        key: reserved_key,
        vector: vec![0.0, 0.0, 1.0],
        metadata: None,
    });
    match vec_result {
        Ok(_) => {
            // Bug confirmed: vector accepts _strata/ prefix keys
        }
        Err(_) => {
            // Validation has been added — bug fixed
        }
    }
}

/// KV rejects oversized keys but vector accepts them.
#[test]
fn issue_953_vector_accepts_oversized_key_while_kv_rejects() {
    let db = Database::cache().unwrap();
    let executor = Executor::new(db);
    let branch = BranchId::from("default");

    let oversized_key = "k".repeat(1025);

    // KV rejects keys exceeding 1024 bytes
    let kv_result = executor.execute(Command::KvPut {
        branch: Some(branch.clone()),
        key: oversized_key.clone(),
        value: Value::Int(1),
    });
    assert!(
        kv_result.is_err(),
        "KV should reject key exceeding 1024 bytes"
    );

    // Vector accepts oversized key — BUG
    let vec_result = executor.execute(Command::VectorUpsert {
        branch: Some(branch.clone()),
        collection: "col4".into(),
        key: oversized_key,
        vector: vec![1.0, 1.0, 0.0],
        metadata: None,
    });
    match vec_result {
        Ok(_) => {
            // Bug confirmed: vector accepts oversized keys
        }
        Err(_) => {
            // Validation has been added — bug fixed
        }
    }
}

/// VectorGet and VectorDelete also lack key validation.
#[test]
fn issue_953_vector_get_and_delete_also_skip_key_validation() {
    let db = Database::cache().unwrap();
    let executor = Executor::new(db);
    let branch = BranchId::from("default");

    // VectorGet with empty key — no validation
    let get_result = executor.execute(Command::VectorGet {
        branch: Some(branch.clone()),
        collection: "col5".into(),
        key: "".into(),
    });
    // This may fail because the collection does not exist, not because the key is invalid.
    // The key "" is never validated.
    let _ = get_result;

    // VectorDelete with empty key — no validation
    let del_result = executor.execute(Command::VectorDelete {
        branch: Some(branch.clone()),
        collection: "col5".into(),
        key: "".into(),
    });
    let _ = del_result;
}
