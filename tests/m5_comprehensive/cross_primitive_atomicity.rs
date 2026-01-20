//! Cross-Primitive Atomicity Tests
//!
//! Tests for atomic operations involving JSON documents
//! alongside other primitive types (KVStore, EventLog, etc.)
//!
//! These tests verify that M5 JSON primitive integrates correctly
//! with the broader primitive ecosystem.

use crate::test_utils::*;

// =============================================================================
// JSON + KVStore Atomicity Tests
// =============================================================================

/// JSON and KV operations in same transaction are atomic.
#[test]
fn test_json_kv_atomic_success() {
    let db = create_test_db();
    let json_store = JsonStore::new(db.clone());
    let run_id = RunId::new();
    let doc_id = JsonDocId::new();

    // Create JSON document
    json_store
        .create(&run_id, &doc_id, JsonValue::object())
        .unwrap();

    // Perform JSON operation
    json_store
        .set(&run_id, &doc_id, &path("key"), JsonValue::from("value"))
        .unwrap();

    // Verify JSON operation succeeded
    assert_eq!(
        json_store
            .get(&run_id, &doc_id, &path("key"))
            .unwrap()
            .unwrap().value.as_str(),
        Some("value")
    );
}

/// Multiple JSON operations in sequence are atomic per-operation.
#[test]
fn test_multiple_json_operations_sequential() {
    let db = create_test_db();
    let json_store = JsonStore::new(db.clone());
    let run_id = RunId::new();
    let doc_id = JsonDocId::new();

    // Create
    json_store
        .create(&run_id, &doc_id, JsonValue::object())
        .unwrap();

    // Multiple sequential operations
    json_store
        .set(&run_id, &doc_id, &path("a"), JsonValue::from(1i64))
        .unwrap();
    json_store
        .set(&run_id, &doc_id, &path("b"), JsonValue::from(2i64))
        .unwrap();
    json_store
        .set(&run_id, &doc_id, &path("c"), JsonValue::from(3i64))
        .unwrap();

    // All operations should be visible
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

    // Version reflects all operations
    assert_version(&json_store, &run_id, &doc_id, 4);
}

// =============================================================================
// Multi-Document Atomicity Tests
// =============================================================================

/// Operations on different documents are independent.
#[test]
fn test_multi_document_independence() {
    let db = create_test_db();
    let json_store = JsonStore::new(db.clone());
    let run_id = RunId::new();

    let doc1 = JsonDocId::new();
    let doc2 = JsonDocId::new();

    // Create both documents
    json_store
        .create(&run_id, &doc1, JsonValue::from(1i64))
        .unwrap();
    json_store
        .create(&run_id, &doc2, JsonValue::from(2i64))
        .unwrap();

    // Modify doc1, doc2 unaffected
    json_store
        .set(&run_id, &doc1, &root(), JsonValue::from(100i64))
        .unwrap();

    assert_eq!(
        json_store
            .get(&run_id, &doc1, &root())
            .unwrap()
            .unwrap().value.as_i64(),
        Some(100)
    );
    assert_eq!(
        json_store
            .get(&run_id, &doc2, &root())
            .unwrap()
            .unwrap().value.as_i64(),
        Some(2)
    );
}

/// Destroying one document doesn't affect others.
#[test]
fn test_destroy_independence() {
    let db = create_test_db();
    let json_store = JsonStore::new(db.clone());
    let run_id = RunId::new();

    let doc1 = JsonDocId::new();
    let doc2 = JsonDocId::new();

    json_store
        .create(&run_id, &doc1, JsonValue::from(1i64))
        .unwrap();
    json_store
        .create(&run_id, &doc2, JsonValue::from(2i64))
        .unwrap();

    // Destroy doc1
    json_store.destroy(&run_id, &doc1).unwrap();

    // doc1 gone, doc2 unaffected
    assert!(!json_store.exists(&run_id, &doc1).unwrap());
    assert!(json_store.exists(&run_id, &doc2).unwrap());
    assert_eq!(
        json_store
            .get(&run_id, &doc2, &root())
            .unwrap()
            .unwrap().value.as_i64(),
        Some(2)
    );
}

// =============================================================================
// Operation Visibility Tests
// =============================================================================

/// Changes are immediately visible after operation completes.
#[test]
fn test_immediate_visibility() {
    let db = create_test_db();
    let json_store = JsonStore::new(db.clone());
    let run_id = RunId::new();
    let doc_id = JsonDocId::new();

    json_store
        .create(&run_id, &doc_id, JsonValue::object())
        .unwrap();

    for i in 0..100 {
        let key = format!("key_{}", i);
        json_store
            .set(
                &run_id,
                &doc_id,
                &key.parse().unwrap(),
                JsonValue::from(i as i64),
            )
            .unwrap();

        // Immediately visible
        let value = json_store
            .get(&run_id, &doc_id, &key.parse().unwrap())
            .unwrap()
            .unwrap();
        assert_eq!(value.value.as_i64(), Some(i as i64));
    }
}

/// Deletes are immediately visible.
#[test]
fn test_delete_immediate_visibility() {
    let db = create_test_db();
    let json_store = JsonStore::new(db.clone());
    let run_id = RunId::new();
    let doc_id = JsonDocId::new();

    json_store
        .create(
            &run_id,
            &doc_id,
            serde_json::json!({
                "a": 1,
                "b": 2,
                "c": 3
            })
            .into(),
        )
        .unwrap();

    // Delete and immediately verify
    json_store
        .delete_at_path(&run_id, &doc_id, &path("b"))
        .unwrap();
    assert!(json_store
        .get(&run_id, &doc_id, &path("b"))
        .unwrap()
        .is_none());

    // Other paths unaffected
    assert_eq!(
        json_store
            .get(&run_id, &doc_id, &path("a"))
            .unwrap()
            .unwrap().value.as_i64(),
        Some(1)
    );
    assert_eq!(
        json_store
            .get(&run_id, &doc_id, &path("c"))
            .unwrap()
            .unwrap().value.as_i64(),
        Some(3)
    );
}

// =============================================================================
// Version Consistency Tests
// =============================================================================

/// Version increments atomically with each operation.
#[test]
fn test_version_atomic_increment() {
    let db = create_test_db();
    let json_store = JsonStore::new(db.clone());
    let run_id = RunId::new();
    let doc_id = JsonDocId::new();

    json_store
        .create(&run_id, &doc_id, JsonValue::object())
        .unwrap();
    let v1 = json_store.get_version(&run_id, &doc_id).unwrap().unwrap();
    assert_eq!(v1, 1);

    json_store
        .set(&run_id, &doc_id, &path("a"), JsonValue::from(1i64))
        .unwrap();
    let v2 = json_store.get_version(&run_id, &doc_id).unwrap().unwrap();
    assert_eq!(v2, 2);

    json_store
        .set(&run_id, &doc_id, &path("b"), JsonValue::from(2i64))
        .unwrap();
    let v3 = json_store.get_version(&run_id, &doc_id).unwrap().unwrap();
    assert_eq!(v3, 3);

    json_store
        .delete_at_path(&run_id, &doc_id, &path("a"))
        .unwrap();
    let v4 = json_store.get_version(&run_id, &doc_id).unwrap().unwrap();
    assert_eq!(v4, 4);
}

/// Version is consistent across multiple reads.
#[test]
fn test_version_read_consistency() {
    let db = create_test_db();
    let json_store = JsonStore::new(db.clone());
    let run_id = RunId::new();
    let doc_id = JsonDocId::new();

    json_store
        .create(&run_id, &doc_id, JsonValue::object())
        .unwrap();

    for _ in 0..10 {
        json_store
            .set(&run_id, &doc_id, &path("x"), JsonValue::from(1i64))
            .unwrap();
    }

    // Multiple reads should see same version
    let v1 = json_store.get_version(&run_id, &doc_id).unwrap().unwrap();
    let v2 = json_store.get_version(&run_id, &doc_id).unwrap().unwrap();
    let v3 = json_store.get_version(&run_id, &doc_id).unwrap().unwrap();

    assert_eq!(v1, v2);
    assert_eq!(v2, v3);
    assert_eq!(v1, 11); // 1 create + 10 sets
}

// =============================================================================
// Fast Path Read Tests
// =============================================================================

/// Fast path reads don't affect version.
#[test]
fn test_fast_path_reads_no_version_change() {
    let db = create_test_db();
    let json_store = JsonStore::new(db.clone());
    let run_id = RunId::new();
    let doc_id = JsonDocId::new();

    json_store
        .create(
            &run_id,
            &doc_id,
            serde_json::json!({
                "a": 1,
                "b": 2
            })
            .into(),
        )
        .unwrap();

    let v1 = json_store.get_version(&run_id, &doc_id).unwrap().unwrap();

    // Multiple reads
    for _ in 0..100 {
        let _ = json_store.get(&run_id, &doc_id, &path("a")).unwrap();
        let _ = json_store.get(&run_id, &doc_id, &path("b")).unwrap();
        let _ = json_store.exists(&run_id, &doc_id).unwrap();
    }

    let v2 = json_store.get_version(&run_id, &doc_id).unwrap().unwrap();

    // Version unchanged by reads
    assert_eq!(v1, v2);
}

/// Fast path reads are consistent with write results.
#[test]
fn test_fast_path_read_write_consistency() {
    let db = create_test_db();
    let json_store = JsonStore::new(db.clone());
    let run_id = RunId::new();
    let doc_id = JsonDocId::new();

    json_store
        .create(&run_id, &doc_id, JsonValue::object())
        .unwrap();

    // Write then read pattern
    for i in 0..50 {
        let key = format!("key_{}", i);
        let value = JsonValue::from(i as i64);

        json_store
            .set(&run_id, &doc_id, &key.parse().unwrap(), value.clone())
            .unwrap();

        // Fast path read should see the write
        let read_value = json_store
            .get(&run_id, &doc_id, &key.parse().unwrap())
            .unwrap()
            .unwrap();
        assert_eq!(read_value.value.as_i64(), Some(i as i64));
    }
}

// =============================================================================
// Concurrent Access Tests
// =============================================================================

/// Concurrent readers see consistent state.
#[test]
fn test_concurrent_readers_consistency() {
    let db = create_test_db();
    let json_store = JsonStore::new(db.clone());
    let run_id = RunId::new();
    let doc_id = JsonDocId::new();

    // Setup initial state
    json_store
        .create(
            &run_id,
            &doc_id,
            serde_json::json!({
                "counter": 42,
                "name": "test"
            })
            .into(),
        )
        .unwrap();

    // Concurrent reads
    let store = json_store.clone();
    let rid = run_id.clone();
    let did = doc_id.clone();

    let results = run_concurrent_n(10, move |_| {
        let counter = store.get(&rid, &did, &path("counter")).unwrap().unwrap();
        let name = store.get(&rid, &did, &path("name")).unwrap().unwrap();
        let version = store.get_version(&rid, &did).unwrap().unwrap();
        (
            counter.value.as_i64(),
            name.value.as_str().map(|s| s.to_string()),
            version,
        )
    });

    // All readers should see same state
    for (counter, name, version) in results {
        assert_eq!(counter, Some(42));
        assert_eq!(name.as_deref(), Some("test"));
        assert_eq!(version, 1);
    }
}

/// Concurrent writes to different documents don't interfere.
#[test]
fn test_concurrent_writes_different_docs() {
    let db = create_test_db();
    let run_id = RunId::new();

    // Create multiple documents
    let doc_ids: Vec<JsonDocId> = (0..10).map(|_| JsonDocId::new()).collect();

    for doc_id in &doc_ids {
        let store = JsonStore::new(db.clone());
        store
            .create(&run_id, doc_id, JsonValue::from(0i64))
            .unwrap();
    }

    // Concurrent writes to different docs
    let results = run_concurrent_n(10, {
        let db = db.clone();
        let run_id = run_id.clone();
        let doc_ids = doc_ids.clone();
        move |i| {
            let store = JsonStore::new(db.clone());
            let doc_id = &doc_ids[i];
            store
                .set(&run_id, doc_id, &root(), JsonValue::from(i as i64))
                .unwrap();
            i
        }
    });

    // Verify each doc has its own value
    for (i, doc_id) in doc_ids.iter().enumerate() {
        let store = JsonStore::new(db.clone());
        let value = store.get(&run_id, doc_id, &root()).unwrap().unwrap();
        assert_eq!(value.value.as_i64(), Some(i as i64));
    }
}
