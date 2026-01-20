//! Mutation Algebra Tests
//!
//! Tests for mutation operation properties:
//! - Associativity of operations
//! - Non-commutativity of operations
//! - Identity operations
//! - Inverse operations (set/delete)

use crate::test_utils::*;

// =============================================================================
// Operation Non-Commutativity Tests
// =============================================================================

/// Set operations on same path are not commutative.
#[test]
fn test_set_not_commutative_same_path() {
    // Order 1: set x=1, then x=2
    let (_, store1, run_id1, doc_id1) = setup_doc(JsonValue::object());
    store1
        .set(&run_id1, &doc_id1, &path("x"), JsonValue::from(1i64))
        .unwrap();
    store1
        .set(&run_id1, &doc_id1, &path("x"), JsonValue::from(2i64))
        .unwrap();
    let result1 = store1
        .get(&run_id1, &doc_id1, &path("x"))
        .unwrap()
        .unwrap().value.as_i64();

    // Order 2: set x=2, then x=1
    let (_, store2, run_id2, doc_id2) = setup_doc(JsonValue::object());
    store2
        .set(&run_id2, &doc_id2, &path("x"), JsonValue::from(2i64))
        .unwrap();
    store2
        .set(&run_id2, &doc_id2, &path("x"), JsonValue::from(1i64))
        .unwrap();
    let result2 = store2
        .get(&run_id2, &doc_id2, &path("x"))
        .unwrap()
        .unwrap().value.as_i64();

    // Different order produces different results
    assert_eq!(result1, Some(2));
    assert_eq!(result2, Some(1));
    assert_ne!(result1, result2, "Set operations should not be commutative");
}

/// Set and delete on same path are not commutative.
#[test]
fn test_set_delete_not_commutative() {
    // Order 1: set x=1, then delete x
    let (_, store1, run_id1, doc_id1) = setup_doc(JsonValue::object());
    store1
        .set(&run_id1, &doc_id1, &path("x"), JsonValue::from(1i64))
        .unwrap();
    store1
        .delete_at_path(&run_id1, &doc_id1, &path("x"))
        .unwrap();
    let result1 = store1.get(&run_id1, &doc_id1, &path("x")).unwrap();

    // Order 2: delete x, then set x=1
    let (_, store2, run_id2, doc_id2) = setup_doc(serde_json::json!({"x": 0}).into());
    store2
        .delete_at_path(&run_id2, &doc_id2, &path("x"))
        .unwrap();
    store2
        .set(&run_id2, &doc_id2, &path("x"), JsonValue::from(1i64))
        .unwrap();
    let result2 = store2.get(&run_id2, &doc_id2, &path("x")).unwrap();

    // Different order produces different results
    assert!(result1.is_none(), "Order 1 should have x deleted");
    assert!(result2.is_some(), "Order 2 should have x set");
}

/// Operations on different paths are commutative.
#[test]
fn test_different_paths_commutative() {
    // Order 1: set a=1, then b=2
    let (_, store1, run_id1, doc_id1) = setup_doc(JsonValue::object());
    store1
        .set(&run_id1, &doc_id1, &path("a"), JsonValue::from(1i64))
        .unwrap();
    store1
        .set(&run_id1, &doc_id1, &path("b"), JsonValue::from(2i64))
        .unwrap();
    let a1 = store1
        .get(&run_id1, &doc_id1, &path("a"))
        .unwrap()
        .unwrap().value.as_i64();
    let b1 = store1
        .get(&run_id1, &doc_id1, &path("b"))
        .unwrap()
        .unwrap().value.as_i64();

    // Order 2: set b=2, then a=1
    let (_, store2, run_id2, doc_id2) = setup_doc(JsonValue::object());
    store2
        .set(&run_id2, &doc_id2, &path("b"), JsonValue::from(2i64))
        .unwrap();
    store2
        .set(&run_id2, &doc_id2, &path("a"), JsonValue::from(1i64))
        .unwrap();
    let a2 = store2
        .get(&run_id2, &doc_id2, &path("a"))
        .unwrap()
        .unwrap().value.as_i64();
    let b2 = store2
        .get(&run_id2, &doc_id2, &path("b"))
        .unwrap()
        .unwrap().value.as_i64();

    // Same result regardless of order
    assert_eq!(a1, a2);
    assert_eq!(b1, b2);
}

// =============================================================================
// Associativity Tests
// =============================================================================

/// Sets on same path are "associative" in the sense that grouping doesn't matter.
#[test]
fn test_set_sequence_order_matters() {
    // (set x=1; set x=2); set x=3 = set x=3
    let (_, store1, run_id1, doc_id1) = setup_doc(JsonValue::object());
    store1
        .set(&run_id1, &doc_id1, &path("x"), JsonValue::from(1i64))
        .unwrap();
    store1
        .set(&run_id1, &doc_id1, &path("x"), JsonValue::from(2i64))
        .unwrap();
    store1
        .set(&run_id1, &doc_id1, &path("x"), JsonValue::from(3i64))
        .unwrap();
    let result1 = store1
        .get(&run_id1, &doc_id1, &path("x"))
        .unwrap()
        .unwrap().value.as_i64();

    // set x=1; (set x=2; set x=3) = set x=3
    let (_, store2, run_id2, doc_id2) = setup_doc(JsonValue::object());
    store2
        .set(&run_id2, &doc_id2, &path("x"), JsonValue::from(1i64))
        .unwrap();
    store2
        .set(&run_id2, &doc_id2, &path("x"), JsonValue::from(2i64))
        .unwrap();
    store2
        .set(&run_id2, &doc_id2, &path("x"), JsonValue::from(3i64))
        .unwrap();
    let result2 = store2
        .get(&run_id2, &doc_id2, &path("x"))
        .unwrap()
        .unwrap().value.as_i64();

    // Same sequence, same result
    assert_eq!(result1, result2);
    assert_eq!(result1, Some(3));
}

// =============================================================================
// Identity Operation Tests
// =============================================================================

/// Setting same value is idempotent in result (but increments version).
#[test]
fn test_set_same_value_idempotent_result() {
    let (_, store, run_id, doc_id) = setup_doc(JsonValue::object());

    store
        .set(&run_id, &doc_id, &path("x"), JsonValue::from(42i64))
        .unwrap();
    let v1 = store.get(&run_id, &doc_id, &path("x")).unwrap().unwrap();

    // Set same value again
    store
        .set(&run_id, &doc_id, &path("x"), JsonValue::from(42i64))
        .unwrap();
    let v2 = store.get(&run_id, &doc_id, &path("x")).unwrap().unwrap();

    // Value unchanged (but version increments due to M9 semantics: every write returns a new version)
    assert_eq!(v1.value, v2.value);
}

/// Delete on non-existent path is idempotent.
#[test]
fn test_delete_nonexistent_idempotent() {
    let (_, store, run_id, doc_id) = setup_doc(JsonValue::object());

    // Delete non-existent path multiple times
    store.delete_at_path(&run_id, &doc_id, &path("x")).unwrap();
    store.delete_at_path(&run_id, &doc_id, &path("x")).unwrap();
    store.delete_at_path(&run_id, &doc_id, &path("x")).unwrap();

    // Path still doesn't exist
    assert!(store.get(&run_id, &doc_id, &path("x")).unwrap().is_none());
}

/// Multiple deletes on existing path (first deletes, rest are no-ops).
#[test]
fn test_multiple_deletes_idempotent() {
    let (_, store, run_id, doc_id) = setup_doc(serde_json::json!({"x": 1}).into());

    // First delete removes it
    store.delete_at_path(&run_id, &doc_id, &path("x")).unwrap();
    assert!(store.get(&run_id, &doc_id, &path("x")).unwrap().is_none());

    // Subsequent deletes are idempotent
    store.delete_at_path(&run_id, &doc_id, &path("x")).unwrap();
    store.delete_at_path(&run_id, &doc_id, &path("x")).unwrap();
    assert!(store.get(&run_id, &doc_id, &path("x")).unwrap().is_none());
}

// =============================================================================
// Inverse Operation Tests
// =============================================================================

/// Set followed by delete restores absence.
#[test]
fn test_set_delete_inverse() {
    let (_, store, run_id, doc_id) = setup_doc(JsonValue::object());

    // Initially x doesn't exist
    assert!(store.get(&run_id, &doc_id, &path("x")).unwrap().is_none());

    // Set x
    store
        .set(&run_id, &doc_id, &path("x"), JsonValue::from(42i64))
        .unwrap();
    assert!(store.get(&run_id, &doc_id, &path("x")).unwrap().is_some());

    // Delete x (inverse of set)
    store.delete_at_path(&run_id, &doc_id, &path("x")).unwrap();
    assert!(store.get(&run_id, &doc_id, &path("x")).unwrap().is_none());
}

/// Create followed by destroy restores non-existence.
#[test]
fn test_create_destroy_inverse() {
    let db = create_test_db();
    let store = JsonStore::new(db);
    let run_id = RunId::new();
    let doc_id = JsonDocId::new();

    // Initially doc doesn't exist
    assert!(!store.exists(&run_id, &doc_id).unwrap());

    // Create doc
    store
        .create(&run_id, &doc_id, JsonValue::from(1i64))
        .unwrap();
    assert!(store.exists(&run_id, &doc_id).unwrap());

    // Destroy doc (inverse of create)
    store.destroy(&run_id, &doc_id).unwrap();
    assert!(!store.exists(&run_id, &doc_id).unwrap());
}

// =============================================================================
// Overwrite Semantics Tests
// =============================================================================

/// Setting a value overwrites previous value completely.
#[test]
fn test_set_overwrites_completely() {
    let (_, store, run_id, doc_id) = setup_doc(
        serde_json::json!({
            "data": {
                "a": 1,
                "b": 2
            }
        })
        .into(),
    );

    // Overwrite entire data object with new value
    store
        .set(
            &run_id,
            &doc_id,
            &path("data"),
            serde_json::json!({"c": 3}).into(),
        )
        .unwrap();

    // Old keys gone, only new key exists
    assert!(store
        .get(&run_id, &doc_id, &path("data.a"))
        .unwrap()
        .is_none());
    assert!(store
        .get(&run_id, &doc_id, &path("data.b"))
        .unwrap()
        .is_none());
    assert_eq!(
        store
            .get(&run_id, &doc_id, &path("data.c"))
            .unwrap()
            .unwrap().value.as_i64(),
        Some(3)
    );
}

/// Setting nested value doesn't affect siblings.
#[test]
fn test_set_nested_no_sibling_effect() {
    let (_, store, run_id, doc_id) = setup_doc(
        serde_json::json!({
            "data": {
                "a": 1,
                "b": 2
            }
        })
        .into(),
    );

    // Set specific path
    store
        .set(&run_id, &doc_id, &path("data.a"), JsonValue::from(100i64))
        .unwrap();

    // Sibling unchanged
    assert_eq!(
        store
            .get(&run_id, &doc_id, &path("data.a"))
            .unwrap()
            .unwrap().value.as_i64(),
        Some(100)
    );
    assert_eq!(
        store
            .get(&run_id, &doc_id, &path("data.b"))
            .unwrap()
            .unwrap().value.as_i64(),
        Some(2)
    );
}

// =============================================================================
// Type Coercion Tests
// =============================================================================

/// Setting different type overwrites existing.
#[test]
fn test_type_change_on_set() {
    let (_, store, run_id, doc_id) = setup_doc(
        serde_json::json!({
            "x": 42
        })
        .into(),
    );

    // x is integer
    assert!(store
        .get(&run_id, &doc_id, &path("x"))
        .unwrap()
        .unwrap().value.is_i64());

    // Set x to string
    store
        .set(&run_id, &doc_id, &path("x"), JsonValue::from("hello"))
        .unwrap();

    // x is now string
    let x = store.get(&run_id, &doc_id, &path("x")).unwrap().unwrap();
    assert!(x.value.is_string());
    assert_eq!(x.value.as_str(), Some("hello"));
}

/// Setting scalar to object works.
#[test]
fn test_scalar_to_object() {
    let (_, store, run_id, doc_id) = setup_doc(
        serde_json::json!({
            "x": 42
        })
        .into(),
    );

    // Set x to object
    store
        .set(
            &run_id,
            &doc_id,
            &path("x"),
            serde_json::json!({"nested": true}).into(),
        )
        .unwrap();

    // Can access nested field
    assert_eq!(
        store
            .get(&run_id, &doc_id, &path("x.nested"))
            .unwrap()
            .unwrap().value.as_bool(),
        Some(true)
    );
}

/// Setting object to scalar works.
#[test]
fn test_object_to_scalar() {
    let (_, store, run_id, doc_id) = setup_doc(
        serde_json::json!({
            "x": {"nested": true}
        })
        .into(),
    );

    // Set x to scalar
    store
        .set(&run_id, &doc_id, &path("x"), JsonValue::from(42i64))
        .unwrap();

    // x is now scalar, nested path doesn't exist
    assert_eq!(
        store
            .get(&run_id, &doc_id, &path("x"))
            .unwrap()
            .unwrap().value.as_i64(),
        Some(42)
    );
    assert!(store
        .get(&run_id, &doc_id, &path("x.nested"))
        .unwrap()
        .is_none());
}

// =============================================================================
// Operation Composition Tests
// =============================================================================

/// Complex sequence of operations produces correct result.
#[test]
fn test_complex_operation_sequence() {
    let (_, store, run_id, doc_id) = setup_doc(JsonValue::object());

    // Complex sequence
    store
        .set(&run_id, &doc_id, &path("users"), JsonValue::array())
        .unwrap();
    store
        .set(
            &run_id,
            &doc_id,
            &path("users"),
            serde_json::json!([{"name": "Alice"}]).into(),
        )
        .unwrap();
    store
        .set(
            &run_id,
            &doc_id,
            &path("users[0].age"),
            JsonValue::from(30i64),
        )
        .unwrap();
    store
        .set(
            &run_id,
            &doc_id,
            &path("config"),
            serde_json::json!({"enabled": true}).into(),
        )
        .unwrap();
    store
        .delete_at_path(&run_id, &doc_id, &path("config.enabled"))
        .unwrap();
    store
        .set(
            &run_id,
            &doc_id,
            &path("config.debug"),
            JsonValue::from(false),
        )
        .unwrap();

    // Verify final state
    assert_eq!(
        store
            .get(&run_id, &doc_id, &path("users[0].name"))
            .unwrap()
            .unwrap().value.as_str(),
        Some("Alice")
    );
    assert_eq!(
        store
            .get(&run_id, &doc_id, &path("users[0].age"))
            .unwrap()
            .unwrap().value.as_i64(),
        Some(30)
    );
    assert!(store
        .get(&run_id, &doc_id, &path("config.enabled"))
        .unwrap()
        .is_none());
    assert_eq!(
        store
            .get(&run_id, &doc_id, &path("config.debug"))
            .unwrap()
            .unwrap().value.as_bool(),
        Some(false)
    );
}

/// Operations accumulate correctly.
#[test]
fn test_operations_accumulate() {
    let (_, store, run_id, doc_id) = setup_doc(JsonValue::object());

    // Add multiple fields one by one
    for i in 0..10 {
        let key = format!("key_{}", i);
        store
            .set(
                &run_id,
                &doc_id,
                &key.parse().unwrap(),
                JsonValue::from(i as i64),
            )
            .unwrap();
    }

    // All fields present
    for i in 0..10 {
        let key = format!("key_{}", i);
        let value = store
            .get(&run_id, &doc_id, &key.parse().unwrap())
            .unwrap()
            .unwrap();
        assert_eq!(value.value.as_i64(), Some(i as i64));
    }
}
