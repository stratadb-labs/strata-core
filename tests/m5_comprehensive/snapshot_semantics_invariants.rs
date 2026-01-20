//! Snapshot Semantics Invariants
//!
//! **Invariant**: M5 provides weak snapshot isolation.
//! Modified documents fail reads rather than returning stale data.
//!
//! These tests ensure snapshot behavior is correct and explicit.

use crate::test_utils::*;
use std::sync::Arc;
use std::thread;

// =============================================================================
// Read-Your-Writes Tests (Within Same Operation Sequence)
// =============================================================================

/// Within a sequence of operations, reads see prior writes.
#[test]
fn test_read_your_writes_same_sequence() {
    let (_, store, run_id, doc_id) = setup_doc(JsonValue::object());

    // Write
    store
        .set(&run_id, &doc_id, &path("x"), JsonValue::from(42i64))
        .unwrap();

    // Read immediately after
    let val = store.get(&run_id, &doc_id, &path("x")).unwrap().unwrap();
    assert_eq!(val.value.as_i64(), Some(42));
}

/// Sequential writes are visible to subsequent reads.
#[test]
fn test_sequential_writes_visible() {
    let (_, store, run_id, doc_id) = setup_doc(JsonValue::object());

    for i in 1..=10 {
        store
            .set(
                &run_id,
                &doc_id,
                &path("counter"),
                JsonValue::from(i as i64),
            )
            .unwrap();

        // Each write is immediately visible
        let val = store
            .get(&run_id, &doc_id, &path("counter"))
            .unwrap()
            .unwrap();
        assert_eq!(val.value.as_i64(), Some(i as i64));
    }
}

/// Reads at overlapping paths see write effects.
#[test]
fn test_read_overlapping_path_sees_write() {
    let (_, store, run_id, doc_id) = setup_doc(JsonValue::object());

    // Write entire object
    store
        .set(
            &run_id,
            &doc_id,
            &path("user"),
            serde_json::json!({
                "name": "Alice",
                "age": 30
            })
            .into(),
        )
        .unwrap();

    // Read descendant path
    let name = store
        .get(&run_id, &doc_id, &path("user.name"))
        .unwrap()
        .unwrap();
    assert_eq!(name.value.as_str(), Some("Alice"));

    // Read ancestor sees the whole object
    let user = store.get(&run_id, &doc_id, &path("user")).unwrap().unwrap();
    assert!(user.value.is_object());
    assert_eq!(user.value.get("name").and_then(|v| v.as_str()), Some("Alice"));
}

// =============================================================================
// Fast Path Read Tests
// =============================================================================

/// Fast path reads (direct snapshot access) return current committed state.
#[test]
fn test_fast_path_reads_current_state() {
    let db = create_test_db();
    let store = JsonStore::new(db);
    let run_id = RunId::new();
    let doc_id = JsonDocId::new();

    // Create document
    store
        .create(&run_id, &doc_id, JsonValue::from(1i64))
        .unwrap();

    // Fast path read
    let val = store.get(&run_id, &doc_id, &root()).unwrap().unwrap();
    assert_eq!(val.value.as_i64(), Some(1));

    // Update
    store
        .set(&run_id, &doc_id, &root(), JsonValue::from(2i64))
        .unwrap();

    // Fast path read sees new value
    let val = store.get(&run_id, &doc_id, &root()).unwrap().unwrap();
    assert_eq!(val.value.as_i64(), Some(2));
}

/// Multiple readers can read concurrently.
#[test]
fn test_concurrent_reads_safe() {
    let db = create_test_db();
    let store = JsonStore::new(db.clone());
    let run_id = RunId::new();
    let doc_id = JsonDocId::new();

    store
        .create(&run_id, &doc_id, JsonValue::from(42i64))
        .unwrap();

    // Spawn multiple readers
    let handles: Vec<_> = (0..10)
        .map(|_| {
            let store = store.clone();
            let run_id = run_id;
            let doc_id = doc_id;

            thread::spawn(move || {
                for _ in 0..100 {
                    let val = store.get(&run_id, &doc_id, &root()).unwrap().unwrap();
                    assert_eq!(val.value.as_i64(), Some(42));
                }
            })
        })
        .collect();

    for h in handles {
        h.join().expect("Reader thread panicked");
    }
}

// =============================================================================
// No Stale Reads Tests
// =============================================================================

/// M5 never returns stale data - either current data or explicit failure.
#[test]
fn test_no_stale_reads() {
    let db = create_test_db();
    let store = JsonStore::new(db);
    let run_id = RunId::new();
    let doc_id = JsonDocId::new();

    // Create with initial value
    store
        .create(&run_id, &doc_id, JsonValue::from("v1"))
        .unwrap();

    // Read
    let r1 = store.get(&run_id, &doc_id, &root()).unwrap().unwrap();
    assert_eq!(r1.value.as_str(), Some("v1"));

    // Update
    store
        .set(&run_id, &doc_id, &root(), JsonValue::from("v2"))
        .unwrap();

    // Read again - must see v2, not v1
    let r2 = store.get(&run_id, &doc_id, &root()).unwrap().unwrap();
    assert_eq!(r2.value.as_str(), Some("v2"), "Must see current value, not stale");
}

// =============================================================================
// Document Existence Tests
// =============================================================================

/// Reading non-existent document returns None, not error.
#[test]
fn test_read_nonexistent_returns_none() {
    let db = create_test_db();
    let store = JsonStore::new(db);
    let run_id = RunId::new();
    let doc_id = JsonDocId::new();

    let result = store.get(&run_id, &doc_id, &root()).unwrap();
    assert!(result.is_none());
}

/// Reading path in non-existent document returns None.
#[test]
fn test_read_path_nonexistent_doc_returns_none() {
    let db = create_test_db();
    let store = JsonStore::new(db);
    let run_id = RunId::new();
    let doc_id = JsonDocId::new();

    let result = store.get(&run_id, &doc_id, &path("some.path")).unwrap();
    assert!(result.is_none());
}

/// Reading non-existent path in existing document returns None.
#[test]
fn test_read_nonexistent_path_returns_none() {
    let (_, store, run_id, doc_id) = setup_doc(JsonValue::object());

    let result = store.get(&run_id, &doc_id, &path("nonexistent")).unwrap();
    assert!(result.is_none());
}

// =============================================================================
// Version Visibility Tests
// =============================================================================

/// get_version returns current document version.
#[test]
fn test_version_visibility() {
    let (_, store, run_id, doc_id) = setup_doc(JsonValue::object());

    assert_version(&store, &run_id, &doc_id, 1);

    store
        .set(&run_id, &doc_id, &path("x"), JsonValue::from(1i64))
        .unwrap();
    assert_version(&store, &run_id, &doc_id, 2);

    store
        .set(&run_id, &doc_id, &path("y"), JsonValue::from(2i64))
        .unwrap();
    assert_version(&store, &run_id, &doc_id, 3);
}

/// get_version for non-existent document returns None.
#[test]
fn test_version_nonexistent_returns_none() {
    let db = create_test_db();
    let store = JsonStore::new(db);
    let run_id = RunId::new();
    let doc_id = JsonDocId::new();

    let version = store.get_version(&run_id, &doc_id).unwrap();
    assert!(version.is_none());
}

// =============================================================================
// Destroy Visibility Tests
// =============================================================================

/// Destroyed document is immediately invisible.
#[test]
fn test_destroy_immediately_invisible() {
    let (_, store, run_id, doc_id) = setup_doc(JsonValue::from(42i64));

    // Exists before destroy
    assert!(store.exists(&run_id, &doc_id).unwrap());

    // Destroy
    store.destroy(&run_id, &doc_id).unwrap();

    // Immediately invisible
    assert!(!store.exists(&run_id, &doc_id).unwrap());
    assert!(store.get(&run_id, &doc_id, &root()).unwrap().is_none());
    assert!(store.get_version(&run_id, &doc_id).unwrap().is_none());
}

/// Destroyed document can be recreated.
#[test]
fn test_destroy_then_recreate() {
    let db = create_test_db();
    let store = JsonStore::new(db);
    let run_id = RunId::new();
    let doc_id = JsonDocId::new();

    // Create
    store
        .create(&run_id, &doc_id, JsonValue::from(1i64))
        .unwrap();
    assert_eq!(
        store
            .get(&run_id, &doc_id, &root())
            .unwrap()
            .unwrap().value.as_i64(),
        Some(1)
    );

    // Destroy
    store.destroy(&run_id, &doc_id).unwrap();
    assert!(!store.exists(&run_id, &doc_id).unwrap());

    // Recreate with different value
    store
        .create(&run_id, &doc_id, JsonValue::from(2i64))
        .unwrap();

    // New value visible, version reset
    assert_eq!(
        store
            .get(&run_id, &doc_id, &root())
            .unwrap()
            .unwrap().value.as_i64(),
        Some(2)
    );
    assert_version(&store, &run_id, &doc_id, 1);
}

// =============================================================================
// Create Visibility Tests
// =============================================================================

/// Created document is immediately visible.
#[test]
fn test_create_immediately_visible() {
    let db = create_test_db();
    let store = JsonStore::new(db);
    let run_id = RunId::new();
    let doc_id = JsonDocId::new();

    // Not visible before create
    assert!(!store.exists(&run_id, &doc_id).unwrap());

    // Create
    store
        .create(&run_id, &doc_id, JsonValue::from(42i64))
        .unwrap();

    // Immediately visible
    assert!(store.exists(&run_id, &doc_id).unwrap());
    assert_eq!(
        store
            .get(&run_id, &doc_id, &root())
            .unwrap()
            .unwrap().value.as_i64(),
        Some(42)
    );
}

/// Duplicate create fails.
#[test]
fn test_duplicate_create_fails() {
    let db = create_test_db();
    let store = JsonStore::new(db);
    let run_id = RunId::new();
    let doc_id = JsonDocId::new();

    // First create succeeds
    store
        .create(&run_id, &doc_id, JsonValue::from(1i64))
        .unwrap();

    // Second create fails
    let result = store.create(&run_id, &doc_id, JsonValue::from(2i64));
    assert!(result.is_err());

    // Original value preserved
    assert_eq!(
        store
            .get(&run_id, &doc_id, &root())
            .unwrap()
            .unwrap().value.as_i64(),
        Some(1)
    );
}

// =============================================================================
// Consistency Under Concurrent Writes Tests
// =============================================================================

/// Concurrent writes to same document are serialized.
/// Some writes may fail due to conflicts, but the document remains consistent.
#[test]
fn test_concurrent_writes_serialized() {
    let db = create_test_db();
    let store = Arc::new(JsonStore::new(db));
    let run_id = RunId::new();
    let doc_id = JsonDocId::new();

    // Create document with initial value at the path
    store
        .create(&run_id, &doc_id, serde_json::json!({"value": 0}).into())
        .unwrap();

    let num_writers = 10;
    let writes_per_thread = 10;

    use std::sync::atomic::{AtomicU64, Ordering};
    let success_count = Arc::new(AtomicU64::new(0));

    let handles: Vec<_> = (0..num_writers)
        .map(|writer_id| {
            let store = store.clone();
            let run_id = run_id;
            let doc_id = doc_id;
            let success_count = success_count.clone();

            thread::spawn(move || {
                for i in 0..writes_per_thread {
                    let value = writer_id * 1000 + i;
                    if store
                        .set(
                            &run_id,
                            &doc_id,
                            &path("value"),
                            JsonValue::from(value as i64),
                        )
                        .is_ok()
                    {
                        success_count.fetch_add(1, Ordering::Relaxed);
                    }
                }
            })
        })
        .collect();

    for h in handles {
        h.join().expect("Writer thread panicked");
    }

    // At least some writes should have succeeded
    let successful_writes = success_count.load(Ordering::Relaxed);
    assert!(successful_writes >= 1, "At least one write should succeed");

    // Final state should be consistent (some value, version incremented properly)
    let version = store.get_version(&run_id, &doc_id).unwrap().unwrap();
    assert!(version >= 2); // At least create + 1 successful write

    // Value should be one of the written values (not corrupted)
    let final_value = store.get(&run_id, &doc_id, &path("value")).unwrap();
    assert!(final_value.is_some(), "Value should exist");
    assert!(final_value.unwrap().value.is_i64());
}

/// Concurrent writes to different documents don't interfere.
#[test]
fn test_concurrent_writes_different_docs_independent() {
    let db = create_test_db();
    let store = Arc::new(JsonStore::new(db));
    let run_id = RunId::new();

    let num_docs = 10;
    let writes_per_doc = 100;

    // Create docs
    let doc_ids: Vec<_> = (0..num_docs)
        .map(|i| {
            let doc_id = JsonDocId::new();
            store
                .create(&run_id, &doc_id, JsonValue::from(0i64))
                .unwrap();
            (doc_id, i)
        })
        .collect();

    let handles: Vec<_> = doc_ids
        .iter()
        .map(|(doc_id, expected_final)| {
            let store = store.clone();
            let run_id = run_id;
            let doc_id = *doc_id;
            let expected = *expected_final;

            thread::spawn(move || {
                for _ in 0..writes_per_doc {
                    store
                        .set(&run_id, &doc_id, &root(), JsonValue::from(expected as i64))
                        .unwrap();
                }
            })
        })
        .collect();

    for h in handles {
        h.join().expect("Writer thread panicked");
    }

    // Each doc should have its expected value
    for (doc_id, expected) in &doc_ids {
        let val = store.get(&run_id, doc_id, &root()).unwrap().unwrap();
        assert_eq!(val.value.as_i64(), Some(*expected as i64));
    }
}
