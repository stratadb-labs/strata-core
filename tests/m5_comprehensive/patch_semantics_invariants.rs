//! Patch Semantics Invariants
//!
//! **Invariant**: Patches are ordered, non-commutative, and sequentially applied.
//!
//! These tests ensure patch ordering and conflict semantics are preserved.

use crate::test_utils::*;
use in_mem_core::json::JsonPatch;

// =============================================================================
// Patch Ordering Tests
// =============================================================================

/// Patch ordering matters - [A, B] != [B, A].
/// This is fundamental: patches are programs, not sets.
#[test]
fn test_patch_ordering_matters() {
    // Scenario 1: Set ancestor then descendant
    {
        let (_, store, run_id, doc_id) = setup_doc(JsonValue::object());

        // Apply: Set $.a = {}, then Set $.a.b = 1
        store
            .set(&run_id, &doc_id, &path("a"), JsonValue::object())
            .unwrap();
        store
            .set(&run_id, &doc_id, &path("a.b"), JsonValue::from(1i64))
            .unwrap();

        // Result: $.a = { "b": 1 }
        let result = store.get(&run_id, &doc_id, &path("a.b")).unwrap().unwrap();
        assert_eq!(result.value.as_i64(), Some(1));
    }

    // Scenario 2: Reverse order
    {
        let (_, store, run_id, doc_id) = setup_doc(JsonValue::object());

        // Need to create intermediate first for second set to work
        store
            .set(&run_id, &doc_id, &path("a"), JsonValue::object())
            .unwrap();
        store
            .set(&run_id, &doc_id, &path("a.b"), JsonValue::from(1i64))
            .unwrap();

        // Now overwrite $.a with empty object
        store
            .set(&run_id, &doc_id, &path("a"), JsonValue::object())
            .unwrap();

        // Result: $.a = {} (the second set to $.a overwrote everything)
        let result = store.get(&run_id, &doc_id, &path("a.b")).unwrap();
        assert!(result.is_none(), "$.a.b should be gone after $.a overwrite");
    }
}

/// Patches are not commutative - order determines result.
#[test]
fn test_patches_are_not_commutative() {
    // Order 1: Set x=1, then Set x=2
    let (_, store1, run_id1, doc_id1) = setup_doc(JsonValue::object());
    store1
        .set(&run_id1, &doc_id1, &path("x"), JsonValue::from(1i64))
        .unwrap();
    store1
        .set(&run_id1, &doc_id1, &path("x"), JsonValue::from(2i64))
        .unwrap();
    let result1 = store1.get(&run_id1, &doc_id1, &path("x")).unwrap().unwrap();

    // Order 2: Set x=2, then Set x=1
    let (_, store2, run_id2, doc_id2) = setup_doc(JsonValue::object());
    store2
        .set(&run_id2, &doc_id2, &path("x"), JsonValue::from(2i64))
        .unwrap();
    store2
        .set(&run_id2, &doc_id2, &path("x"), JsonValue::from(1i64))
        .unwrap();
    let result2 = store2.get(&run_id2, &doc_id2, &path("x")).unwrap().unwrap();

    // Results should be different (last write wins)
    assert_eq!(result1.value.as_i64(), Some(2));
    assert_eq!(result2.value.as_i64(), Some(1));
}

/// Last write wins for same-path operations.
#[test]
fn test_last_write_wins_same_path() {
    let (_, store, run_id, doc_id) = setup_doc(JsonValue::object());

    // Multiple writes to same path
    for i in 1..=10 {
        store
            .set(&run_id, &doc_id, &path("value"), JsonValue::from(i as i64))
            .unwrap();
    }

    // Final value should be 10
    let result = store
        .get(&run_id, &doc_id, &path("value"))
        .unwrap()
        .unwrap();
    assert_eq!(result.value.as_i64(), Some(10));
}

// =============================================================================
// Patch Conflict Detection Tests
// =============================================================================

/// Overlapping patches conflict.
#[test]
fn test_overlapping_patches_conflict() {
    let p1 = JsonPatch::set("user", JsonValue::object());
    let p2 = JsonPatch::set("user.name", JsonValue::from("Alice"));

    assert!(
        p1.conflicts_with(&p2),
        "Ancestor-descendant patches should conflict"
    );
    assert!(p2.conflicts_with(&p1), "Conflict should be symmetric");
}

/// Disjoint patches do not conflict.
#[test]
fn test_disjoint_patches_no_conflict() {
    let p1 = JsonPatch::set("user.name", JsonValue::from("Alice"));
    let p2 = JsonPatch::set("user.age", JsonValue::from(30i64));

    assert!(
        !p1.conflicts_with(&p2),
        "Sibling patches should not conflict"
    );
}

/// Same path patches conflict.
#[test]
fn test_same_path_patches_conflict() {
    let p1 = JsonPatch::set("user.name", JsonValue::from("Alice"));
    let p2 = JsonPatch::set("user.name", JsonValue::from("Bob"));

    assert!(p1.conflicts_with(&p2), "Same path patches should conflict");
}

/// Delete and Set on same subtree conflict.
#[test]
fn test_delete_set_same_subtree_conflict() {
    let delete = JsonPatch::delete("user");
    let set = JsonPatch::set("user.name", JsonValue::from("Alice"));

    assert!(
        delete.conflicts_with(&set),
        "Delete ancestor should conflict with set descendant"
    );
}

/// Root patch conflicts with everything.
#[test]
fn test_root_patch_conflicts_with_all() {
    let root_patch = JsonPatch::set_at(JsonPath::root(), JsonValue::object());
    let any_patch = JsonPatch::set("some.deep.path", JsonValue::from(1i64));

    assert!(
        root_patch.conflicts_with(&any_patch),
        "Root patch conflicts with all"
    );
}

// =============================================================================
// Sequential Application Tests
// =============================================================================

/// Each patch sees effects of prior patches.
#[test]
fn test_patches_see_prior_effects() {
    let (_, store, run_id, doc_id) = setup_doc(
        serde_json::json!({
            "counter": 0
        })
        .into(),
    );

    // Simulate sequential patches that depend on prior state
    for i in 1..=5 {
        // Read current
        let current = store
            .get(&run_id, &doc_id, &path("counter"))
            .unwrap()
            .unwrap().value.as_i64()
            .unwrap();

        // Write incremented (simulating patch seeing prior state)
        store
            .set(
                &run_id,
                &doc_id,
                &path("counter"),
                JsonValue::from(current + 1),
            )
            .unwrap();
    }

    let final_val = store
        .get(&run_id, &doc_id, &path("counter"))
        .unwrap()
        .unwrap();
    assert_eq!(final_val.value.as_i64(), Some(5));
}

/// Intermediate objects created by Set are visible to subsequent operations.
#[test]
fn test_intermediate_objects_visible_to_subsequent_ops() {
    let (_, store, run_id, doc_id) = setup_doc(JsonValue::object());

    // Set creates intermediate objects
    store
        .set(&run_id, &doc_id, &path("a.b.c"), JsonValue::from(1i64))
        .unwrap();

    // Subsequent set on sibling path should work
    store
        .set(&run_id, &doc_id, &path("a.b.d"), JsonValue::from(2i64))
        .unwrap();

    // Both should exist
    let c = store
        .get(&run_id, &doc_id, &path("a.b.c"))
        .unwrap()
        .unwrap();
    let d = store
        .get(&run_id, &doc_id, &path("a.b.d"))
        .unwrap()
        .unwrap();

    assert_eq!(c.value.as_i64(), Some(1));
    assert_eq!(d.value.as_i64(), Some(2));
}

// =============================================================================
// Delete Semantics Tests
// =============================================================================

/// Delete at path removes the value.
#[test]
fn test_delete_removes_value() {
    let (_, store, run_id, doc_id) = setup_doc(
        serde_json::json!({
            "keep": "this",
            "remove": "this"
        })
        .into(),
    );

    store
        .delete_at_path(&run_id, &doc_id, &path("remove"))
        .unwrap();

    assert!(store
        .get(&run_id, &doc_id, &path("remove"))
        .unwrap()
        .is_none());
    assert_eq!(
        store
            .get(&run_id, &doc_id, &path("keep"))
            .unwrap()
            .unwrap().value.as_str(),
        Some("this")
    );
}

/// Delete is idempotent on missing path (increments version).
#[test]
fn test_delete_idempotent_on_missing() {
    let (_, store, run_id, doc_id) = setup_doc(JsonValue::object());

    let v1 = store.get_version(&run_id, &doc_id).unwrap().unwrap();

    // Delete nonexistent path
    store
        .delete_at_path(&run_id, &doc_id, &path("nonexistent"))
        .unwrap();

    let v2 = store.get_version(&run_id, &doc_id).unwrap().unwrap();

    // Version should still increment (operation was recorded)
    assert!(v2 > v1);
}

/// Deleting parent removes all children.
#[test]
fn test_delete_parent_removes_children() {
    let (_, store, run_id, doc_id) = setup_doc(
        serde_json::json!({
            "parent": {
                "child1": 1,
                "child2": 2,
                "nested": {
                    "deep": "value"
                }
            }
        })
        .into(),
    );

    // Delete parent
    store
        .delete_at_path(&run_id, &doc_id, &path("parent"))
        .unwrap();

    // All children should be gone
    assert!(store
        .get(&run_id, &doc_id, &path("parent"))
        .unwrap()
        .is_none());
    assert!(store
        .get(&run_id, &doc_id, &path("parent.child1"))
        .unwrap()
        .is_none());
    assert!(store
        .get(&run_id, &doc_id, &path("parent.nested.deep"))
        .unwrap()
        .is_none());
}

// =============================================================================
// Set Semantics Tests
// =============================================================================

/// Set at path creates value.
#[test]
fn test_set_creates_value() {
    let (_, store, run_id, doc_id) = setup_doc(JsonValue::object());

    store
        .set(
            &run_id,
            &doc_id,
            &path("new.path"),
            JsonValue::from("created"),
        )
        .unwrap();

    let val = store
        .get(&run_id, &doc_id, &path("new.path"))
        .unwrap()
        .unwrap();
    assert_eq!(val.value.as_str(), Some("created"));
}

/// Set at path overwrites value.
#[test]
fn test_set_overwrites_value() {
    let (_, store, run_id, doc_id) = setup_doc(
        serde_json::json!({
            "field": "original"
        })
        .into(),
    );

    store
        .set(&run_id, &doc_id, &path("field"), JsonValue::from("updated"))
        .unwrap();

    let val = store
        .get(&run_id, &doc_id, &path("field"))
        .unwrap()
        .unwrap();
    assert_eq!(val.value.as_str(), Some("updated"));
}

/// Set at root replaces entire document.
#[test]
fn test_set_root_replaces_all() {
    let (_, store, run_id, doc_id) = setup_doc(
        serde_json::json!({
            "old": "data",
            "lots": "of",
            "stuff": "here"
        })
        .into(),
    );

    // Replace entire document
    store
        .set(
            &run_id,
            &doc_id,
            &root(),
            serde_json::json!({"new": "doc"}).into(),
        )
        .unwrap();

    // Old data gone
    assert!(store.get(&run_id, &doc_id, &path("old")).unwrap().is_none());
    assert!(store
        .get(&run_id, &doc_id, &path("lots"))
        .unwrap()
        .is_none());

    // New data present
    let new = store.get(&run_id, &doc_id, &path("new")).unwrap().unwrap();
    assert_eq!(new.value.as_str(), Some("doc"));
}

/// Set preserves siblings.
#[test]
fn test_set_preserves_siblings() {
    let (_, store, run_id, doc_id) = setup_doc(
        serde_json::json!({
            "parent": {
                "sibling1": "keep",
                "sibling2": "keep",
                "modify": "change"
            }
        })
        .into(),
    );

    store
        .set(
            &run_id,
            &doc_id,
            &path("parent.modify"),
            JsonValue::from("changed"),
        )
        .unwrap();

    // Siblings preserved
    assert_eq!(
        store
            .get(&run_id, &doc_id, &path("parent.sibling1"))
            .unwrap()
            .unwrap().value.as_str(),
        Some("keep")
    );
    assert_eq!(
        store
            .get(&run_id, &doc_id, &path("parent.sibling2"))
            .unwrap()
            .unwrap().value.as_str(),
        Some("keep")
    );
    assert_eq!(
        store
            .get(&run_id, &doc_id, &path("parent.modify"))
            .unwrap()
            .unwrap().value.as_str(),
        Some("changed")
    );
}

// =============================================================================
// Version Semantics Tests
// =============================================================================

/// Every modification increments version.
#[test]
fn test_every_modification_increments_version() {
    let (_, store, run_id, doc_id) = setup_doc(JsonValue::object());

    // Version starts at 1
    assert_version(&store, &run_id, &doc_id, 1);

    // Set increments
    store
        .set(&run_id, &doc_id, &path("a"), JsonValue::from(1i64))
        .unwrap();
    assert_version(&store, &run_id, &doc_id, 2);

    // Another set increments
    store
        .set(&run_id, &doc_id, &path("b"), JsonValue::from(2i64))
        .unwrap();
    assert_version(&store, &run_id, &doc_id, 3);

    // Delete increments
    store.delete_at_path(&run_id, &doc_id, &path("a")).unwrap();
    assert_version(&store, &run_id, &doc_id, 4);

    // Delete on missing still increments
    store
        .delete_at_path(&run_id, &doc_id, &path("nonexistent"))
        .unwrap();
    assert_version(&store, &run_id, &doc_id, 5);
}

/// Version is document-scoped, not path-scoped.
#[test]
fn test_version_is_document_scoped() {
    let (_, store, run_id, doc_id) = setup_doc(
        serde_json::json!({
            "a": 1,
            "b": 2
        })
        .into(),
    );

    // Any change to any path increments the single document version
    let v1 = store.get_version(&run_id, &doc_id).unwrap().unwrap();

    store
        .set(&run_id, &doc_id, &path("a"), JsonValue::from(100i64))
        .unwrap();
    let v2 = store.get_version(&run_id, &doc_id).unwrap().unwrap();

    store
        .set(&run_id, &doc_id, &path("b"), JsonValue::from(200i64))
        .unwrap();
    let v3 = store.get_version(&run_id, &doc_id).unwrap().unwrap();

    // All increments happen on the same version counter
    assert_eq!(v2, v1 + 1);
    assert_eq!(v3, v2 + 1);
}
