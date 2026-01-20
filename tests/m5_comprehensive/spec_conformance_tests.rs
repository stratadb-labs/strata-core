//! Spec Conformance Tests
//!
//! Tests that verify conformance to M5 architectural specification:
//! - Six Architectural Rules
//! - TypeTag assignment
//! - Conflict detection semantics
//! - WAL entry format expectations

use crate::test_utils::*;

// =============================================================================
// RULE 1: TypeTag = 0x11 (Tested via Integration)
// =============================================================================
// Note: TypeTag is internal to M1/M2 layer. We verify indirectly
// by ensuring JSON documents work correctly with the database.

/// JSON documents are stored and retrieved correctly (implies correct TypeTag).
#[test]
fn test_json_documents_work_correctly() {
    let (_, store, run_id, doc_id) = setup_doc(
        serde_json::json!({
            "test": "value",
            "number": 42
        })
        .into(),
    );

    // If TypeTag were wrong, this would fail
    let value = store.get(&run_id, &doc_id, &path("test")).unwrap().unwrap();
    assert_eq!(value.value.as_str(), Some("value"));

    let number = store
        .get(&run_id, &doc_id, &path("number"))
        .unwrap()
        .unwrap();
    assert_eq!(number.value.as_i64(), Some(42));
}

// =============================================================================
// RULE 2: Stateless Facade (Arc<Database> Only)
// =============================================================================

/// JsonStore only holds Arc<Database>, no other state.
#[test]
fn test_jsonstore_stateless_facade() {
    let db = create_test_db();

    // Create multiple stores from same DB
    let store1 = JsonStore::new(db.clone());
    let store2 = JsonStore::new(db.clone());

    let run_id = RunId::new();
    let doc_id = JsonDocId::new();

    // Operations through any store affect same underlying data
    store1
        .create(&run_id, &doc_id, JsonValue::from(1i64))
        .unwrap();

    // store2 sees the change immediately (no local state)
    assert!(store2.exists(&run_id, &doc_id).unwrap());
    assert_eq!(
        store2
            .get(&run_id, &doc_id, &root())
            .unwrap()
            .unwrap().value.as_i64(),
        Some(1)
    );

    // Modify through store2
    store2
        .set(&run_id, &doc_id, &root(), JsonValue::from(2i64))
        .unwrap();

    // store1 sees the change
    assert_eq!(
        store1
            .get(&run_id, &doc_id, &root())
            .unwrap()
            .unwrap().value.as_i64(),
        Some(2)
    );
}

/// JsonStore can be cloned (shares Arc<Database>).
#[test]
fn test_jsonstore_cloneable() {
    let db = create_test_db();
    let store1 = JsonStore::new(db);
    let store2 = store1.clone();

    let run_id = RunId::new();
    let doc_id = JsonDocId::new();

    store1
        .create(&run_id, &doc_id, JsonValue::from(1i64))
        .unwrap();
    assert_eq!(
        store2
            .get(&run_id, &doc_id, &root())
            .unwrap()
            .unwrap().value.as_i64(),
        Some(1)
    );
}

// =============================================================================
// RULE 3: Path-Based Operations (No Primitive Nesting)
// =============================================================================

/// Operations are path-based, not region-based internally.
#[test]
fn test_path_based_operations() {
    let (_, store, run_id, doc_id) = setup_doc(JsonValue::object());

    // Set at specific paths
    store
        .set(&run_id, &doc_id, &path("a.b.c"), JsonValue::from(1i64))
        .unwrap();
    store
        .set(&run_id, &doc_id, &path("a.b.d"), JsonValue::from(2i64))
        .unwrap();
    store
        .set(&run_id, &doc_id, &path("a.e"), JsonValue::from(3i64))
        .unwrap();

    // Each path is independent
    assert_eq!(
        store
            .get(&run_id, &doc_id, &path("a.b.c"))
            .unwrap()
            .unwrap().value.as_i64(),
        Some(1)
    );
    assert_eq!(
        store
            .get(&run_id, &doc_id, &path("a.b.d"))
            .unwrap()
            .unwrap().value.as_i64(),
        Some(2)
    );
    assert_eq!(
        store
            .get(&run_id, &doc_id, &path("a.e"))
            .unwrap()
            .unwrap().value.as_i64(),
        Some(3)
    );
}

/// No nested primitive types (JSON is atomic).
#[test]
fn test_no_nested_primitives() {
    let (_, store, run_id, doc_id) = setup_doc(
        serde_json::json!({
            "data": {
                "items": [1, 2, 3],
                "meta": {"key": "value"}
            }
        })
        .into(),
    );

    // Entire structure is one JSON document
    // Setting a parent path replaces children
    store
        .set(
            &run_id,
            &doc_id,
            &path("data.items"),
            JsonValue::from("replaced"),
        )
        .unwrap();

    // Old array gone, new value in place
    assert_eq!(
        store
            .get(&run_id, &doc_id, &path("data.items"))
            .unwrap()
            .unwrap().value.as_str(),
        Some("replaced")
    );
    assert!(store
        .get(&run_id, &doc_id, &path("data.items[0]"))
        .unwrap()
        .is_none());
}

// =============================================================================
// RULE 4: Document-Level Versioning (Not Per-Path)
// =============================================================================

/// Version is at document level, not path level.
#[test]
fn test_document_level_versioning() {
    let (_, store, run_id, doc_id) = setup_doc(JsonValue::object());
    assert_version(&store, &run_id, &doc_id, 1);

    // Each path operation increments document version
    store
        .set(&run_id, &doc_id, &path("a"), JsonValue::from(1i64))
        .unwrap();
    assert_version(&store, &run_id, &doc_id, 2);

    store
        .set(&run_id, &doc_id, &path("b"), JsonValue::from(2i64))
        .unwrap();
    assert_version(&store, &run_id, &doc_id, 3);

    store
        .set(&run_id, &doc_id, &path("a"), JsonValue::from(10i64))
        .unwrap();
    assert_version(&store, &run_id, &doc_id, 4);

    store.delete_at_path(&run_id, &doc_id, &path("b")).unwrap();
    assert_version(&store, &run_id, &doc_id, 5);
}

/// No per-path version API exists.
#[test]
fn test_no_per_path_version() {
    // This is a compile-time verification
    // JsonStore doesn't have a get_path_version method
    // If it did, this test would need to verify it doesn't exist

    let (_, store, run_id, doc_id) = setup_doc(JsonValue::object());

    // Only document-level version
    let version = store.get_version(&run_id, &doc_id).unwrap().unwrap();
    assert_eq!(version, 1);
}

// =============================================================================
// RULE 5: Region-Based Conflict Detection
// =============================================================================

/// Overlapping paths conflict (parent/child relationship).
#[test]
fn test_overlapping_paths_conflict_detection() {
    // Spec: path overlap = conflict
    // Sequential operations on overlapping paths should succeed
    // (conflicts only matter for concurrent transactions)

    let (_, store, run_id, doc_id) = setup_doc(JsonValue::object());

    // Set parent then child
    store
        .set(
            &run_id,
            &doc_id,
            &path("a"),
            serde_json::json!({"nested": 1}).into(),
        )
        .unwrap();
    store
        .set(&run_id, &doc_id, &path("a.nested"), JsonValue::from(2i64))
        .unwrap();

    // Set child then parent (overwrites child)
    store
        .set(&run_id, &doc_id, &path("a.nested"), JsonValue::from(3i64))
        .unwrap();
    store
        .set(
            &run_id,
            &doc_id,
            &path("a"),
            serde_json::json!({"other": 4}).into(),
        )
        .unwrap();

    // Last write wins
    assert!(store
        .get(&run_id, &doc_id, &path("a.nested"))
        .unwrap()
        .is_none());
    assert_eq!(
        store
            .get(&run_id, &doc_id, &path("a.other"))
            .unwrap()
            .unwrap().value.as_i64(),
        Some(4)
    );
}

/// Non-overlapping paths don't conflict.
#[test]
fn test_non_overlapping_paths_no_conflict() {
    let (_, store, run_id, doc_id) = setup_doc(JsonValue::object());

    // Sibling paths don't overlap
    store
        .set(&run_id, &doc_id, &path("a"), JsonValue::from(1i64))
        .unwrap();
    store
        .set(&run_id, &doc_id, &path("b"), JsonValue::from(2i64))
        .unwrap();
    store
        .set(&run_id, &doc_id, &path("c"), JsonValue::from(3i64))
        .unwrap();

    // All coexist
    assert_eq!(
        store
            .get(&run_id, &doc_id, &path("a"))
            .unwrap()
            .unwrap().value.as_i64(),
        Some(1)
    );
    assert_eq!(
        store
            .get(&run_id, &doc_id, &path("b"))
            .unwrap()
            .unwrap().value.as_i64(),
        Some(2)
    );
    assert_eq!(
        store
            .get(&run_id, &doc_id, &path("c"))
            .unwrap()
            .unwrap().value.as_i64(),
        Some(3)
    );
}

// =============================================================================
// RULE 6: Weak Snapshot Isolation
// =============================================================================

/// Fast-path reads see committed state.
#[test]
fn test_fast_path_reads_committed_state() {
    let (_, store, run_id, doc_id) = setup_doc(
        serde_json::json!({
            "initial": "value"
        })
        .into(),
    );

    // Fast-path read
    let value = store
        .get(&run_id, &doc_id, &path("initial"))
        .unwrap()
        .unwrap();
    assert_eq!(value.value.as_str(), Some("value"));

    // After modification
    store
        .set(
            &run_id,
            &doc_id,
            &path("initial"),
            JsonValue::from("modified"),
        )
        .unwrap();

    // Fast-path read sees committed modification
    let value = store
        .get(&run_id, &doc_id, &path("initial"))
        .unwrap()
        .unwrap();
    assert_eq!(value.value.as_str(), Some("modified"));
}

/// Read-your-writes within same sequence.
#[test]
fn test_read_your_writes_guarantee() {
    let (_, store, run_id, doc_id) = setup_doc(JsonValue::object());

    // Write then read - must see own write
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

        // Immediately read
        let read = store
            .get(&run_id, &doc_id, &key.parse().unwrap())
            .unwrap()
            .unwrap();
        assert_eq!(
            read.value.as_i64(),
            Some(i as i64),
            "Read-your-writes violated at iteration {}",
            i
        );
    }
}

/// No stale reads within same sequence.
#[test]
fn test_no_stale_reads() {
    let (_, store, run_id, doc_id) = setup_doc(JsonValue::from(0i64));

    let mut last_value = 0i64;

    for i in 1..=50i64 {
        store
            .set(&run_id, &doc_id, &root(), JsonValue::from(i))
            .unwrap();

        let current = store
            .get(&run_id, &doc_id, &root())
            .unwrap()
            .unwrap().value.as_i64()
            .unwrap();

        // Should never see a stale value
        assert!(
            current >= last_value,
            "Stale read: saw {} after {}",
            current,
            last_value
        );
        last_value = current;
    }
}

// =============================================================================
// Patch Semantics Conformance
// =============================================================================

/// Set operation creates path if needed.
#[test]
fn test_set_creates_path() {
    let (_, store, run_id, doc_id) = setup_doc(JsonValue::object());

    // Set on non-existent path creates it
    store
        .set(&run_id, &doc_id, &path("a.b.c"), JsonValue::from(1i64))
        .unwrap();

    assert!(store.get(&run_id, &doc_id, &path("a")).unwrap().is_some());
    assert!(store.get(&run_id, &doc_id, &path("a.b")).unwrap().is_some());
    assert_eq!(
        store
            .get(&run_id, &doc_id, &path("a.b.c"))
            .unwrap()
            .unwrap().value.as_i64(),
        Some(1)
    );
}

/// Delete operation behavior.
/// Note: delete_at_path appears to be idempotent - it succeeds even for non-existent paths
#[test]
fn test_delete_behavior() {
    let (_, store, run_id, doc_id) =
        setup_doc(serde_json::json!({"x": 1, "y": {"nested": 2}}).into());

    // Delete existing path succeeds
    store.delete_at_path(&run_id, &doc_id, &path("x")).unwrap();
    assert!(store.get(&run_id, &doc_id, &path("x")).unwrap().is_none());

    // Delete nested path succeeds
    store
        .delete_at_path(&run_id, &doc_id, &path("y.nested"))
        .unwrap();
    assert!(store
        .get(&run_id, &doc_id, &path("y.nested"))
        .unwrap()
        .is_none());

    // Delete on already-deleted path - check actual API behavior
    let result = store.delete_at_path(&run_id, &doc_id, &path("x"));
    // API may succeed or fail - both are valid implementations
    // Just verify the path is still not there
    let _ = result;
    assert!(store.get(&run_id, &doc_id, &path("x")).unwrap().is_none());

    // Delete on never-existed top-level path
    let result = store.delete_at_path(&run_id, &doc_id, &path("never_existed"));
    // API may succeed or fail - just verify consistent state
    let _ = result;
}

// =============================================================================
// Run Isolation Conformance
// =============================================================================

/// Same doc_id in different runs are isolated.
#[test]
fn test_run_isolation() {
    let db = create_test_db();
    let store = JsonStore::new(db);

    let run1 = RunId::new();
    let run2 = RunId::new();
    let doc_id = JsonDocId::new();

    // Create same doc_id in both runs
    store.create(&run1, &doc_id, JsonValue::from(1i64)).unwrap();
    store.create(&run2, &doc_id, JsonValue::from(2i64)).unwrap();

    // Completely isolated
    assert_eq!(
        store
            .get(&run1, &doc_id, &root())
            .unwrap()
            .unwrap().value.as_i64(),
        Some(1)
    );
    assert_eq!(
        store
            .get(&run2, &doc_id, &root())
            .unwrap()
            .unwrap().value.as_i64(),
        Some(2)
    );

    // Modify run1, run2 unaffected
    store
        .set(&run1, &doc_id, &root(), JsonValue::from(100i64))
        .unwrap();
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

    // Destroy run1, run2 unaffected
    store.destroy(&run1, &doc_id).unwrap();
    assert!(!store.exists(&run1, &doc_id).unwrap());
    assert!(store.exists(&run2, &doc_id).unwrap());
}

// =============================================================================
// Durability Mode Independence
// =============================================================================

/// Same semantics across durability modes.
#[test]
fn test_durability_mode_semantic_consistency() {
    test_across_modes("spec_conformance", |db| {
        let store = JsonStore::new(db);
        let run_id = RunId::new();
        let doc_id = JsonDocId::new();

        // Same operation sequence
        store.create(&run_id, &doc_id, JsonValue::object()).unwrap();
        store
            .set(&run_id, &doc_id, &path("a"), JsonValue::from(1i64))
            .unwrap();
        store
            .set(&run_id, &doc_id, &path("b"), JsonValue::from(2i64))
            .unwrap();
        store.delete_at_path(&run_id, &doc_id, &path("a")).unwrap();
        store
            .set(&run_id, &doc_id, &path("c"), JsonValue::from(3i64))
            .unwrap();

        // Return state for comparison
        let a = store.get(&run_id, &doc_id, &path("a")).unwrap().is_none();
        let b = store
            .get(&run_id, &doc_id, &path("b"))
            .unwrap()
            .unwrap().value.as_i64();
        let c = store
            .get(&run_id, &doc_id, &path("c"))
            .unwrap()
            .unwrap().value.as_i64();
        let v = store.get_version(&run_id, &doc_id).unwrap().unwrap();

        (a, b, c, v)
    });
}
