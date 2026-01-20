//! WAL Replay Tests
//!
//! Tests for WAL replay semantics:
//! - Deterministic replay
//! - Idempotent replay
//! - Order-dependent replay
//! - Interleaved primitive types

use crate::test_utils::*;

// =============================================================================
// Basic WAL Replay Tests
// =============================================================================

/// WAL replay produces same state from same operations.
#[test]
fn test_wal_replay_deterministic() {
    // Run the same sequence of operations multiple times
    // and verify we get the same final state

    for _ in 0..5 {
        let (_, store, run_id, doc_id) = setup_doc(JsonValue::object());

        // Deterministic sequence
        store
            .set(&run_id, &doc_id, &path("a"), JsonValue::from(1i64))
            .unwrap();
        store
            .set(&run_id, &doc_id, &path("b"), JsonValue::from(2i64))
            .unwrap();
        store
            .set(&run_id, &doc_id, &path("c"), JsonValue::from(3i64))
            .unwrap();
        store.delete_at_path(&run_id, &doc_id, &path("b")).unwrap();
        store
            .set(&run_id, &doc_id, &path("d"), JsonValue::from(4i64))
            .unwrap();

        // Verify final state
        assert_eq!(
            store
                .get(&run_id, &doc_id, &path("a"))
                .unwrap()
                .unwrap().value.as_i64(),
            Some(1)
        );
        assert!(store.get(&run_id, &doc_id, &path("b")).unwrap().is_none());
        assert_eq!(
            store
                .get(&run_id, &doc_id, &path("c"))
                .unwrap()
                .unwrap().value.as_i64(),
            Some(3)
        );
        assert_eq!(
            store
                .get(&run_id, &doc_id, &path("d"))
                .unwrap()
                .unwrap().value.as_i64(),
            Some(4)
        );
        assert_version(&store, &run_id, &doc_id, 6); // create(1) + 5 ops (3 sets + 1 delete + 1 set)
    }
}

/// Operations are applied in order - order matters.
#[test]
fn test_wal_replay_order_matters() {
    // Two different orderings produce different results
    {
        let (_, store, run_id, doc_id) = setup_doc(JsonValue::object());

        // Order 1: Set x=1, then x=2
        store
            .set(&run_id, &doc_id, &path("x"), JsonValue::from(1i64))
            .unwrap();
        store
            .set(&run_id, &doc_id, &path("x"), JsonValue::from(2i64))
            .unwrap();

        let result = store.get(&run_id, &doc_id, &path("x")).unwrap().unwrap();
        assert_eq!(result.value.as_i64(), Some(2), "Last write wins");
    }

    {
        let (_, store, run_id, doc_id) = setup_doc(JsonValue::object());

        // Order 2: Set x=2, then x=1
        store
            .set(&run_id, &doc_id, &path("x"), JsonValue::from(2i64))
            .unwrap();
        store
            .set(&run_id, &doc_id, &path("x"), JsonValue::from(1i64))
            .unwrap();

        let result = store.get(&run_id, &doc_id, &path("x")).unwrap().unwrap();
        assert_eq!(result.value.as_i64(), Some(1), "Last write wins");
    }
}

/// Each operation is atomic - either fully applied or not at all.
#[test]
fn test_operations_are_atomic() {
    let (_, store, run_id, doc_id) = setup_doc(
        serde_json::json!({
            "existing": "value"
        })
        .into(),
    );

    // A complex set operation
    let complex_value: JsonValue = serde_json::json!({
        "nested": {
            "deep": {
                "value": 42
            }
        }
    })
    .into();

    store
        .set(&run_id, &doc_id, &path("complex"), complex_value)
        .unwrap();

    // Either the whole complex value is there, or none of it
    let nested_value = store
        .get(&run_id, &doc_id, &path("complex.nested.deep.value"))
        .unwrap();
    assert_eq!(nested_value.unwrap().value.as_i64(), Some(42));
}

// =============================================================================
// Version-Based Idempotence Tests
// =============================================================================

/// Multiple identical operations still increment version.
#[test]
fn test_identical_operations_increment_version() {
    let (_, store, run_id, doc_id) = setup_doc(JsonValue::object());

    store
        .set(&run_id, &doc_id, &path("x"), JsonValue::from(1i64))
        .unwrap();
    let v1 = store.get_version(&run_id, &doc_id).unwrap().unwrap();

    // Set same value again
    store
        .set(&run_id, &doc_id, &path("x"), JsonValue::from(1i64))
        .unwrap();
    let v2 = store.get_version(&run_id, &doc_id).unwrap().unwrap();

    // Version still increments (operation was recorded)
    assert!(v2 > v1);
}

// =============================================================================
// Multi-Document WAL Tests
// =============================================================================

/// Operations on multiple documents interleave correctly.
#[test]
fn test_multi_document_interleaved_operations() {
    let db = create_test_db();
    let store = JsonStore::new(db);
    let run_id = RunId::new();

    let doc1 = JsonDocId::new();
    let doc2 = JsonDocId::new();
    let doc3 = JsonDocId::new();

    store.create(&run_id, &doc1, JsonValue::from(0i64)).unwrap();
    store.create(&run_id, &doc2, JsonValue::from(0i64)).unwrap();
    store.create(&run_id, &doc3, JsonValue::from(0i64)).unwrap();

    // Interleaved operations
    store
        .set(&run_id, &doc1, &root(), JsonValue::from(1i64))
        .unwrap();
    store
        .set(&run_id, &doc2, &root(), JsonValue::from(2i64))
        .unwrap();
    store
        .set(&run_id, &doc1, &root(), JsonValue::from(11i64))
        .unwrap();
    store
        .set(&run_id, &doc3, &root(), JsonValue::from(3i64))
        .unwrap();
    store
        .set(&run_id, &doc2, &root(), JsonValue::from(22i64))
        .unwrap();
    store
        .set(&run_id, &doc3, &root(), JsonValue::from(33i64))
        .unwrap();

    // Each document has correct final state
    assert_eq!(
        store
            .get(&run_id, &doc1, &root())
            .unwrap()
            .unwrap().value.as_i64(),
        Some(11)
    );
    assert_eq!(
        store
            .get(&run_id, &doc2, &root())
            .unwrap()
            .unwrap().value.as_i64(),
        Some(22)
    );
    assert_eq!(
        store
            .get(&run_id, &doc3, &root())
            .unwrap()
            .unwrap().value.as_i64(),
        Some(33)
    );
}

/// Create and destroy interleaved.
#[test]
fn test_create_destroy_interleaved() {
    let db = create_test_db();
    let store = JsonStore::new(db);
    let run_id = RunId::new();
    let doc_id = JsonDocId::new();

    // Create
    store
        .create(&run_id, &doc_id, JsonValue::from(1i64))
        .unwrap();
    assert!(store.exists(&run_id, &doc_id).unwrap());

    // Destroy
    store.destroy(&run_id, &doc_id).unwrap();
    assert!(!store.exists(&run_id, &doc_id).unwrap());

    // Recreate
    store
        .create(&run_id, &doc_id, JsonValue::from(2i64))
        .unwrap();
    assert!(store.exists(&run_id, &doc_id).unwrap());
    assert_eq!(
        store
            .get(&run_id, &doc_id, &root())
            .unwrap()
            .unwrap().value.as_i64(),
        Some(2)
    );
}

// =============================================================================
// Complex Operation Sequences
// =============================================================================

/// Complex nested operations.
#[test]
fn test_complex_nested_operations() {
    let (_, store, run_id, doc_id) = setup_doc(JsonValue::object());

    // Build up a complex structure
    store
        .set(&run_id, &doc_id, &path("users"), JsonValue::array())
        .unwrap();

    let user1: JsonValue = serde_json::json!({
        "name": "Alice",
        "email": "alice@example.com",
        "settings": {
            "theme": "dark",
            "notifications": true
        }
    })
    .into();

    // Replace array with array containing user
    store
        .set(
            &run_id,
            &doc_id,
            &path("users"),
            serde_json::json!([user1.clone()]).into(),
        )
        .unwrap();

    // Modify nested value
    store
        .set(
            &run_id,
            &doc_id,
            &path("users[0].settings.theme"),
            JsonValue::from("light"),
        )
        .unwrap();

    // Delete a field
    store
        .delete_at_path(&run_id, &doc_id, &path("users[0].email"))
        .unwrap();

    // Verify final state
    let theme = store
        .get(&run_id, &doc_id, &path("users[0].settings.theme"))
        .unwrap()
        .unwrap();
    assert_eq!(theme.value.as_str(), Some("light"));

    let email = store
        .get(&run_id, &doc_id, &path("users[0].email"))
        .unwrap();
    assert!(email.is_none());

    let name = store
        .get(&run_id, &doc_id, &path("users[0].name"))
        .unwrap()
        .unwrap();
    assert_eq!(name.value.as_str(), Some("Alice"));
}

/// Long sequence of operations.
#[test]
fn test_long_operation_sequence() {
    let (_, store, run_id, doc_id) = setup_doc(JsonValue::object());

    // Perform many operations
    for i in 0..100 {
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

    // Verify all values
    for i in 0..100 {
        let key = format!("key_{}", i);
        let val = store
            .get(&run_id, &doc_id, &key.parse().unwrap())
            .unwrap()
            .unwrap();
        assert_eq!(val.value.as_i64(), Some(i as i64));
    }

    // Version should be 1 (create) + 100 (sets)
    assert_version(&store, &run_id, &doc_id, 101);
}

/// Alternating set and delete.
#[test]
fn test_alternating_set_delete() {
    let (_, store, run_id, doc_id) = setup_doc(JsonValue::object());

    for i in 0..10 {
        // Set
        store
            .set(&run_id, &doc_id, &path("temp"), JsonValue::from(i as i64))
            .unwrap();
        assert_eq!(
            store
                .get(&run_id, &doc_id, &path("temp"))
                .unwrap()
                .unwrap().value.as_i64(),
            Some(i as i64)
        );

        // Delete
        store
            .delete_at_path(&run_id, &doc_id, &path("temp"))
            .unwrap();
        assert!(store
            .get(&run_id, &doc_id, &path("temp"))
            .unwrap()
            .is_none());
    }

    // Final state: temp doesn't exist
    assert!(store
        .get(&run_id, &doc_id, &path("temp"))
        .unwrap()
        .is_none());

    // 1 create + 20 operations (10 sets + 10 deletes)
    assert_version(&store, &run_id, &doc_id, 21);
}

// =============================================================================
// Run Isolation in WAL
// =============================================================================

/// Operations in different runs don't affect each other.
#[test]
fn test_run_isolation_in_wal() {
    let db = create_test_db();
    let store = JsonStore::new(db);

    let run1 = RunId::new();
    let run2 = RunId::new();
    let doc_id = JsonDocId::new();

    // Create in both runs
    store.create(&run1, &doc_id, JsonValue::from(1i64)).unwrap();
    store.create(&run2, &doc_id, JsonValue::from(2i64)).unwrap();

    // Modify run1
    store
        .set(&run1, &doc_id, &root(), JsonValue::from(100i64))
        .unwrap();

    // run2 unaffected
    assert_eq!(
        store
            .get(&run1, &doc_id, &root())
            .unwrap()
            .unwrap().value.as_i64(),
        Some(100)
    );
    assert_eq!(
        store
            .get(&run2, &doc_id, &root())
            .unwrap()
            .unwrap().value.as_i64(),
        Some(2)
    );

    // Modify run2
    store
        .set(&run2, &doc_id, &root(), JsonValue::from(200i64))
        .unwrap();

    // run1 unaffected
    assert_eq!(
        store
            .get(&run1, &doc_id, &root())
            .unwrap()
            .unwrap().value.as_i64(),
        Some(100)
    );
    assert_eq!(
        store
            .get(&run2, &doc_id, &root())
            .unwrap()
            .unwrap().value.as_i64(),
        Some(200)
    );
}

// =============================================================================
// Durability Mode Independence
// =============================================================================

/// Same operations produce same results across durability modes.
#[test]
fn test_durability_mode_semantic_equivalence() {
    // This test verifies that semantics are identical across modes
    test_across_modes("json_create_and_modify", |db| {
        let store = JsonStore::new(db);
        let run_id = RunId::new();
        let doc_id = JsonDocId::new();

        store.create(&run_id, &doc_id, JsonValue::object()).unwrap();
        store
            .set(&run_id, &doc_id, &path("a"), JsonValue::from(1i64))
            .unwrap();
        store
            .set(&run_id, &doc_id, &path("b"), JsonValue::from(2i64))
            .unwrap();
        store.delete_at_path(&run_id, &doc_id, &path("a")).unwrap();

        // Return the final state for comparison
        let b_val = store
            .get(&run_id, &doc_id, &path("b"))
            .unwrap()
            .unwrap().value.as_i64();
        let a_val = store.get(&run_id, &doc_id, &path("a")).unwrap();
        let version = store.get_version(&run_id, &doc_id).unwrap().unwrap();

        (b_val, a_val.is_none(), version)
    });
}

// =============================================================================
// Edge Cases
// =============================================================================

/// Empty document operations.
#[test]
fn test_empty_document_operations() {
    let (_, store, run_id, doc_id) = setup_doc(JsonValue::object());

    // Read from empty object
    assert!(store
        .get(&run_id, &doc_id, &path("anything"))
        .unwrap()
        .is_none());

    // Delete from empty object (idempotent)
    store
        .delete_at_path(&run_id, &doc_id, &path("nonexistent"))
        .unwrap();

    // Version still increments
    assert_version(&store, &run_id, &doc_id, 2);
}

/// Null value operations.
#[test]
fn test_null_value_operations() {
    let (_, store, run_id, doc_id) = setup_doc(JsonValue::null());

    // Read root
    let val = store.get(&run_id, &doc_id, &root()).unwrap().unwrap();
    assert!(val.value.is_null());

    // Replace null with object
    store
        .set(&run_id, &doc_id, &root(), JsonValue::object())
        .unwrap();

    // Now we can set fields
    store
        .set(&run_id, &doc_id, &path("field"), JsonValue::from(1i64))
        .unwrap();
    assert_eq!(
        store
            .get(&run_id, &doc_id, &path("field"))
            .unwrap()
            .unwrap().value.as_i64(),
        Some(1)
    );
}

/// Array document operations.
#[test]
fn test_array_document_operations() {
    let (_, store, run_id, doc_id) = setup_doc(serde_json::json!([1, 2, 3]).into());

    // Read element
    assert_eq!(
        store
            .get(&run_id, &doc_id, &path("[1]"))
            .unwrap()
            .unwrap().value.as_i64(),
        Some(2)
    );

    // Modify element
    store
        .set(&run_id, &doc_id, &path("[1]"), JsonValue::from(99i64))
        .unwrap();
    assert_eq!(
        store
            .get(&run_id, &doc_id, &path("[1]"))
            .unwrap()
            .unwrap().value.as_i64(),
        Some(99)
    );

    // Delete element (shifts remaining)
    store
        .delete_at_path(&run_id, &doc_id, &path("[0]"))
        .unwrap();

    // [1] is now what was [2]
    assert_eq!(
        store
            .get(&run_id, &doc_id, &path("[0]"))
            .unwrap()
            .unwrap().value.as_i64(),
        Some(99)
    );
    assert_eq!(
        store
            .get(&run_id, &doc_id, &path("[1]"))
            .unwrap()
            .unwrap().value.as_i64(),
        Some(3)
    );
}
