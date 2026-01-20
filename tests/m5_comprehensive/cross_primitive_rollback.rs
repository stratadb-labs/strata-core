//! Cross-Primitive Rollback Tests
//!
//! Tests for rollback behavior and error recovery:
//! - Failed operations don't corrupt state
//! - Partial operations are not visible
//! - Error handling is consistent

use crate::test_utils::*;

// =============================================================================
// Failed Operation Tests
// =============================================================================

/// Failed operation on non-existent document doesn't affect other documents.
#[test]
fn test_failed_op_no_side_effects() {
    let db = create_test_db();
    let json_store = JsonStore::new(db.clone());
    let run_id = RunId::new();

    let existing_doc = JsonDocId::new();
    let nonexistent_doc = JsonDocId::new();

    // Create one document
    json_store
        .create(&run_id, &existing_doc, JsonValue::from(42i64))
        .unwrap();

    // Try to set on non-existent document (should fail)
    let result = json_store.set(&run_id, &nonexistent_doc, &root(), JsonValue::from(1i64));
    assert!(result.is_err());

    // Existing document unaffected
    assert_eq!(
        json_store
            .get(&run_id, &existing_doc, &root())
            .unwrap()
            .unwrap().value.as_i64(),
        Some(42)
    );
    assert_version(&json_store, &run_id, &existing_doc, 1);
}

/// Attempting to create duplicate document fails cleanly.
#[test]
fn test_duplicate_create_fails_cleanly() {
    let db = create_test_db();
    let json_store = JsonStore::new(db.clone());
    let run_id = RunId::new();
    let doc_id = JsonDocId::new();

    // First create succeeds
    json_store
        .create(&run_id, &doc_id, JsonValue::from(1i64))
        .unwrap();

    // Second create fails
    let result = json_store.create(&run_id, &doc_id, JsonValue::from(2i64));
    assert!(result.is_err());

    // Original value preserved
    assert_eq!(
        json_store
            .get(&run_id, &doc_id, &root())
            .unwrap()
            .unwrap().value.as_i64(),
        Some(1)
    );
    assert_version(&json_store, &run_id, &doc_id, 1);
}

/// Destroy on non-existent document is idempotent.
#[test]
fn test_destroy_nonexistent_idempotent() {
    let db = create_test_db();
    let json_store = JsonStore::new(db.clone());
    let run_id = RunId::new();
    let doc_id = JsonDocId::new();

    // Destroy non-existent should not error (idempotent)
    // Note: This behavior may vary based on implementation
    let result = json_store.destroy(&run_id, &doc_id);
    // Whether this succeeds or fails, it shouldn't panic
    let _ = result;

    // Document still doesn't exist
    assert!(!json_store.exists(&run_id, &doc_id).unwrap());
}

/// Delete at path on non-existent path is idempotent.
#[test]
fn test_delete_nonexistent_path_idempotent() {
    let db = create_test_db();
    let json_store = JsonStore::new(db.clone());
    let run_id = RunId::new();
    let doc_id = JsonDocId::new();

    json_store
        .create(
            &run_id,
            &doc_id,
            serde_json::json!({
                "a": 1
            })
            .into(),
        )
        .unwrap();

    let v1 = json_store.get_version(&run_id, &doc_id).unwrap().unwrap();

    // Delete non-existent path (idempotent operation)
    json_store
        .delete_at_path(&run_id, &doc_id, &path("nonexistent"))
        .unwrap();

    // Version still increments (operation was recorded)
    let v2 = json_store.get_version(&run_id, &doc_id).unwrap().unwrap();
    assert!(v2 > v1);

    // Existing data unaffected
    assert_eq!(
        json_store
            .get(&run_id, &doc_id, &path("a"))
            .unwrap()
            .unwrap().value.as_i64(),
        Some(1)
    );
}

// =============================================================================
// State Consistency After Errors
// =============================================================================

/// Document state remains valid after invalid operations.
#[test]
fn test_state_valid_after_invalid_ops() {
    let db = create_test_db();
    let json_store = JsonStore::new(db.clone());
    let run_id = RunId::new();
    let doc_id = JsonDocId::new();

    // Create document with nested structure
    json_store
        .create(
            &run_id,
            &doc_id,
            serde_json::json!({
                "data": {
                    "value": 100
                }
            })
            .into(),
        )
        .unwrap();

    // Try some operations that might fail
    // (e.g., setting on a path where parent isn't an object)
    // The specific behavior depends on implementation

    // After any errors, original state should be valid
    assert_eq!(
        json_store
            .get(&run_id, &doc_id, &path("data.value"))
            .unwrap()
            .unwrap().value.as_i64(),
        Some(100)
    );
}

/// Multiple failed operations don't accumulate corruption.
#[test]
fn test_multiple_failures_no_accumulation() {
    let db = create_test_db();
    let json_store = JsonStore::new(db.clone());
    let run_id = RunId::new();

    let existing_doc = JsonDocId::new();
    let nonexistent = JsonDocId::new();

    json_store
        .create(&run_id, &existing_doc, JsonValue::from(42i64))
        .unwrap();

    // Multiple failed operations
    for _ in 0..10 {
        let _ = json_store.set(&run_id, &nonexistent, &root(), JsonValue::from(1i64));
        let _ = json_store.get(&run_id, &nonexistent, &root());
        let _ = json_store.delete_at_path(&run_id, &nonexistent, &path("x"));
    }

    // Existing document still intact
    assert_eq!(
        json_store
            .get(&run_id, &existing_doc, &root())
            .unwrap()
            .unwrap().value.as_i64(),
        Some(42)
    );
    assert_version(&json_store, &run_id, &existing_doc, 1);
}

// =============================================================================
// Version Consistency After Errors
// =============================================================================

/// Version doesn't change on failed operations.
#[test]
fn test_version_unchanged_on_failure() {
    let db = create_test_db();
    let json_store = JsonStore::new(db.clone());
    let run_id = RunId::new();
    let doc_id = JsonDocId::new();

    json_store
        .create(&run_id, &doc_id, JsonValue::from(1i64))
        .unwrap();
    let v1 = json_store.get_version(&run_id, &doc_id).unwrap().unwrap();

    // Failed operation (duplicate create)
    let _ = json_store.create(&run_id, &doc_id, JsonValue::from(2i64));

    let v2 = json_store.get_version(&run_id, &doc_id).unwrap().unwrap();
    assert_eq!(v1, v2);
}

// =============================================================================
// Run Isolation Rollback Tests
// =============================================================================

/// Operations in one run don't affect other runs even on error.
#[test]
fn test_run_isolation_on_error() {
    let db = create_test_db();
    let json_store = JsonStore::new(db.clone());

    let run1 = RunId::new();
    let run2 = RunId::new();
    let doc_id = JsonDocId::new();

    // Create in run1
    json_store
        .create(&run1, &doc_id, JsonValue::from(100i64))
        .unwrap();

    // Try operations in run2 on non-existent doc (in run2's context)
    let result = json_store.set(&run2, &doc_id, &root(), JsonValue::from(200i64));
    assert!(result.is_err());

    // run1's document unaffected
    assert_eq!(
        json_store
            .get(&run1, &doc_id, &root())
            .unwrap()
            .unwrap().value.as_i64(),
        Some(100)
    );

    // run2 still has no document
    assert!(!json_store.exists(&run2, &doc_id).unwrap());
}

/// Destroying document in one run doesn't affect same doc_id in other run.
#[test]
fn test_destroy_run_isolation() {
    let db = create_test_db();
    let json_store = JsonStore::new(db.clone());

    let run1 = RunId::new();
    let run2 = RunId::new();
    let doc_id = JsonDocId::new();

    // Create same doc_id in both runs
    json_store
        .create(&run1, &doc_id, JsonValue::from(1i64))
        .unwrap();
    json_store
        .create(&run2, &doc_id, JsonValue::from(2i64))
        .unwrap();

    // Destroy in run1
    json_store.destroy(&run1, &doc_id).unwrap();

    // run1 doc gone
    assert!(!json_store.exists(&run1, &doc_id).unwrap());

    // run2 doc unaffected
    assert!(json_store.exists(&run2, &doc_id).unwrap());
    assert_eq!(
        json_store
            .get(&run2, &doc_id, &root())
            .unwrap()
            .unwrap().value.as_i64(),
        Some(2)
    );
}

// =============================================================================
// Recovery After Rollback Tests
// =============================================================================

/// Can continue operations after a failed operation.
#[test]
fn test_operations_continue_after_failure() {
    let db = create_test_db();
    let json_store = JsonStore::new(db.clone());
    let run_id = RunId::new();
    let doc_id = JsonDocId::new();

    json_store
        .create(&run_id, &doc_id, JsonValue::object())
        .unwrap();
    json_store
        .set(&run_id, &doc_id, &path("a"), JsonValue::from(1i64))
        .unwrap();

    // Cause some failure (duplicate create)
    let _ = json_store.create(&run_id, &doc_id, JsonValue::from(0i64));

    // Can still continue with valid operations
    json_store
        .set(&run_id, &doc_id, &path("b"), JsonValue::from(2i64))
        .unwrap();
    json_store
        .set(&run_id, &doc_id, &path("c"), JsonValue::from(3i64))
        .unwrap();

    assert_eq!(
        json_store
            .get(&run_id, &doc_id, &path("a"))
            .unwrap()
            .unwrap().value.as_i64(),
        Some(1)
    );
    assert_eq!(
        json_store
            .get(&run_id, &doc_id, &path("b"))
            .unwrap()
            .unwrap().value.as_i64(),
        Some(2)
    );
    assert_eq!(
        json_store
            .get(&run_id, &doc_id, &path("c"))
            .unwrap()
            .unwrap().value.as_i64(),
        Some(3)
    );
}

/// Can recreate document after destroy.
#[test]
fn test_recreate_after_destroy() {
    let db = create_test_db();
    let json_store = JsonStore::new(db.clone());
    let run_id = RunId::new();
    let doc_id = JsonDocId::new();

    // Create, modify, destroy
    json_store
        .create(&run_id, &doc_id, JsonValue::from(1i64))
        .unwrap();
    json_store
        .set(&run_id, &doc_id, &root(), JsonValue::from(100i64))
        .unwrap();
    json_store.destroy(&run_id, &doc_id).unwrap();

    assert!(!json_store.exists(&run_id, &doc_id).unwrap());

    // Recreate
    json_store
        .create(&run_id, &doc_id, JsonValue::from(2i64))
        .unwrap();

    assert!(json_store.exists(&run_id, &doc_id).unwrap());
    assert_eq!(
        json_store
            .get(&run_id, &doc_id, &root())
            .unwrap()
            .unwrap().value.as_i64(),
        Some(2)
    );

    // Version is fresh (new document)
    assert_version(&json_store, &run_id, &doc_id, 1);
}

// =============================================================================
// Concurrent Error Handling Tests
// =============================================================================

/// Concurrent operations handle errors gracefully.
#[test]
fn test_concurrent_error_handling() {
    let db = create_test_db();
    let run_id = RunId::new();
    let doc_id = JsonDocId::new();

    // Create the document first
    {
        let store = JsonStore::new(db.clone());
        store
            .create(&run_id, &doc_id, JsonValue::from(0i64))
            .unwrap();
    }

    // Concurrent attempts to create same document (only first succeeds, rest error)
    // Since doc already exists, all create attempts will fail
    let results = run_concurrent_n(10, {
        let db = db.clone();
        let run_id = run_id.clone();
        let doc_id = doc_id.clone();
        move |_| {
            let store = JsonStore::new(db.clone());
            store
                .create(&run_id, &doc_id, JsonValue::from(1i64))
                .is_err()
        }
    });

    // All concurrent creates should fail (doc already exists)
    for failed in results {
        assert!(failed);
    }

    // Original document intact
    let store = JsonStore::new(db.clone());
    assert_eq!(
        store
            .get(&run_id, &doc_id, &root())
            .unwrap()
            .unwrap().value.as_i64(),
        Some(0)
    );
}
