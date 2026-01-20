//! Path Semantics Invariants
//!
//! **Invariant**: Paths are positional, not identity-based.
//! Path meaning changes when structure changes.
//!
//! These tests ensure the M5 semantic contract is never violated.

use crate::test_utils::*;

// =============================================================================
// Core Path Semantic Tests
// =============================================================================

/// Paths are positional, not identity-based.
/// When array structure changes, paths refer to different elements.
#[test]
fn test_paths_are_positional_not_identity() {
    let (_, store, run_id, doc_id) = setup_doc(
        serde_json::json!({
            "items": ["A", "B", "C"]
        })
        .into(),
    );

    // Initial state: items[0] = "A"
    let item0 = store
        .get(&run_id, &doc_id, &path("items[0]"))
        .unwrap()
        .unwrap();
    assert_eq!(item0.value.as_str(), Some("A"));

    // Replace entire array with X at front
    let new_array: JsonValue = serde_json::json!(["X", "A", "B", "C"]).into();
    store
        .set(&run_id, &doc_id, &path("items"), new_array)
        .unwrap();

    // Now items[0] refers to "X", not "A"
    // This is fundamental to M5 semantics
    let item0_after = store
        .get(&run_id, &doc_id, &path("items[0]"))
        .unwrap()
        .unwrap();
    assert_eq!(item0_after.value.as_str(), Some("X"));

    // "A" is now at items[1]
    let item1 = store
        .get(&run_id, &doc_id, &path("items[1]"))
        .unwrap()
        .unwrap();
    assert_eq!(item1.value.as_str(), Some("A"));
}

/// Root path overlaps with everything.
#[test]
fn test_root_path_overlaps_everything() {
    let root = JsonPath::root();
    let any_path = path("a.b.c.d[0].e");

    assert!(root.overlaps(&any_path));
    assert!(any_path.overlaps(&root));
    assert!(root.is_ancestor_of(&any_path));
    assert!(any_path.is_descendant_of(&root));
}

/// Sibling paths do not overlap.
#[test]
fn test_sibling_paths_do_not_overlap() {
    let path_a = path("user.name");
    let path_b = path("user.age");
    let path_c = path("user.email");

    assert!(!path_a.overlaps(&path_b));
    assert!(!path_b.overlaps(&path_c));
    assert!(!path_a.overlaps(&path_c));
}

/// Ancestor-descendant paths overlap.
#[test]
fn test_ancestor_descendant_paths_overlap() {
    let ancestor = path("user");
    let descendant = path("user.profile.name");

    assert!(ancestor.overlaps(&descendant));
    assert!(descendant.overlaps(&ancestor));
    assert!(ancestor.is_ancestor_of(&descendant));
    assert!(descendant.is_descendant_of(&ancestor));
}

/// Same paths overlap.
#[test]
fn test_same_paths_overlap() {
    let path1 = path("user.profile.name");
    let path2 = path("user.profile.name");

    assert!(path1.overlaps(&path2));
    assert!(path1 == path2);
}

/// Different subtrees do not overlap.
#[test]
fn test_different_subtrees_do_not_overlap() {
    let path_x = path("users[0].name");
    let path_y = path("config.settings.theme");

    assert!(!path_x.overlaps(&path_y));
}

// =============================================================================
// Read Invalidation by Mutation Tests
// =============================================================================

/// A read at a path is logically invalidated when an ancestor is mutated.
/// This is a semantic invariant - the read path's meaning changed.
#[test]
fn test_read_path_semantically_invalidated_by_ancestor_mutation() {
    let (_, store, run_id, doc_id) = setup_standard_doc();

    // Read a deep path
    let original = store
        .get(&run_id, &doc_id, &path("user.name"))
        .unwrap()
        .unwrap();
    assert_eq!(original.value.as_str(), Some("Alice"));

    // Mutate the ancestor (replace entire "user" object)
    let new_user: JsonValue = serde_json::json!({
        "name": "Bob",
        "age": 25
    })
    .into();
    store
        .set(&run_id, &doc_id, &path("user"), new_user)
        .unwrap();

    // The path "user.name" now refers to different data
    let after = store
        .get(&run_id, &doc_id, &path("user.name"))
        .unwrap()
        .unwrap();
    assert_eq!(after.value.as_str(), Some("Bob"));

    // The semantic invariant: ancestor mutation changes descendant meaning
    assert_ne!(original, after);
}

/// Sibling writes do not affect each other.
#[test]
fn test_sibling_writes_independent() {
    let (_, store, run_id, doc_id) = setup_standard_doc();

    // Read initial values
    let name_before = store
        .get(&run_id, &doc_id, &path("user.name"))
        .unwrap()
        .unwrap();
    assert_eq!(name_before.value.as_str(), Some("Alice"));

    // Write to sibling
    store
        .set(&run_id, &doc_id, &path("user.age"), JsonValue::from(99i64))
        .unwrap();

    // Name should be unaffected (value same, but version may change due to document-level versioning)
    let name_after = store
        .get(&run_id, &doc_id, &path("user.name"))
        .unwrap()
        .unwrap();
    assert_eq!(name_after.value.as_str(), Some("Alice"));
    assert_eq!(name_before.value, name_after.value);
}

/// Writing to descendant modifies ancestor's subtree.
#[test]
fn test_descendant_write_modifies_ancestor_subtree() {
    let (_, store, run_id, doc_id) = setup_standard_doc();

    // Get version before
    let version_before = store.get_version(&run_id, &doc_id).unwrap().unwrap();

    // Write to a deep descendant
    store
        .set(
            &run_id,
            &doc_id,
            &path("config.settings.theme"),
            JsonValue::from("light"),
        )
        .unwrap();

    // Version should increment (document changed)
    let version_after = store.get_version(&run_id, &doc_id).unwrap().unwrap();
    assert!(version_after > version_before);

    // The ancestor "config" now contains the modified subtree
    let theme = store
        .get(&run_id, &doc_id, &path("config.settings.theme"))
        .unwrap()
        .unwrap();
    assert_eq!(theme.value.as_str(), Some("light"));
}

// =============================================================================
// Array Path Semantics
// =============================================================================

/// Array indices are positional - rewriting array changes what indices refer to.
#[test]
fn test_array_indices_are_positional() {
    let (_, store, run_id, doc_id) = setup_doc(
        serde_json::json!({
            "items": [
                {"id": 1, "val": "first"},
                {"id": 2, "val": "second"},
                {"id": 3, "val": "third"}
            ]
        })
        .into(),
    );

    // items[0].id is initially 1
    let id_at_0 = store
        .get(&run_id, &doc_id, &path("items[0].id"))
        .unwrap()
        .unwrap();
    assert_eq!(id_at_0.value.as_i64(), Some(1));

    // Replace entire items array with reversed order
    let reversed: JsonValue = serde_json::json!([
        {"id": 3, "val": "third"},
        {"id": 2, "val": "second"},
        {"id": 1, "val": "first"}
    ])
    .into();
    store
        .set(&run_id, &doc_id, &path("items"), reversed)
        .unwrap();

    // items[0].id is now 3 (position changed, same path)
    let id_at_0_after = store
        .get(&run_id, &doc_id, &path("items[0].id"))
        .unwrap()
        .unwrap();
    assert_eq!(id_at_0_after.value.as_i64(), Some(3));
}

/// Deleting an array element shifts indices.
#[test]
fn test_array_delete_shifts_indices() {
    let (_, store, run_id, doc_id) = setup_doc(
        serde_json::json!({
            "items": ["a", "b", "c", "d"]
        })
        .into(),
    );

    // items[2] is "c"
    let at_2 = store
        .get(&run_id, &doc_id, &path("items[2]"))
        .unwrap()
        .unwrap();
    assert_eq!(at_2.value.as_str(), Some("c"));

    // Delete items[1] ("b")
    store
        .delete_at_path(&run_id, &doc_id, &path("items[1]"))
        .unwrap();

    // Now items[2] is "d" (shifted down from [3])
    let at_2_after = store
        .get(&run_id, &doc_id, &path("items[2]"))
        .unwrap()
        .unwrap();
    assert_eq!(at_2_after.value.as_str(), Some("d"));

    // And items[1] is now "c" (shifted down from [2])
    let at_1 = store
        .get(&run_id, &doc_id, &path("items[1]"))
        .unwrap()
        .unwrap();
    assert_eq!(at_1.value.as_str(), Some("c"));
}

// =============================================================================
// Path Overlap Detection Tests
// =============================================================================

/// Test all overlap cases systematically.
#[test]
fn test_path_overlap_comprehensive() {
    struct TestCase {
        path_a: &'static str,
        path_b: &'static str,
        should_overlap: bool,
        reason: &'static str,
    }

    let cases = vec![
        // Same path
        TestCase {
            path_a: "a.b",
            path_b: "a.b",
            should_overlap: true,
            reason: "same path",
        },
        TestCase {
            path_a: "x[0]",
            path_b: "x[0]",
            should_overlap: true,
            reason: "same array path",
        },
        // Ancestor/descendant
        TestCase {
            path_a: "a",
            path_b: "a.b",
            should_overlap: true,
            reason: "ancestor",
        },
        TestCase {
            path_a: "a.b",
            path_b: "a",
            should_overlap: true,
            reason: "descendant",
        },
        TestCase {
            path_a: "a",
            path_b: "a.b.c.d",
            should_overlap: true,
            reason: "deep descendant",
        },
        TestCase {
            path_a: "x",
            path_b: "x[0]",
            should_overlap: true,
            reason: "array descendant",
        },
        TestCase {
            path_a: "x[0]",
            path_b: "x[0].foo",
            should_overlap: true,
            reason: "array element descendant",
        },
        // Siblings (no overlap)
        TestCase {
            path_a: "a.b",
            path_b: "a.c",
            should_overlap: false,
            reason: "siblings",
        },
        TestCase {
            path_a: "x[0]",
            path_b: "x[1]",
            should_overlap: false,
            reason: "array siblings",
        },
        TestCase {
            path_a: "a.b.c",
            path_b: "a.b.d",
            should_overlap: false,
            reason: "deep siblings",
        },
        // Different subtrees (no overlap)
        TestCase {
            path_a: "users",
            path_b: "config",
            should_overlap: false,
            reason: "different roots",
        },
        TestCase {
            path_a: "a.b.c",
            path_b: "x.y.z",
            should_overlap: false,
            reason: "different subtrees",
        },
        // Root cases
        TestCase {
            path_a: "",
            path_b: "anything",
            should_overlap: true,
            reason: "root overlaps all",
        },
        TestCase {
            path_a: "",
            path_b: "a.b.c[0].d",
            should_overlap: true,
            reason: "root overlaps deep",
        },
    ];

    for case in cases {
        let a = if case.path_a.is_empty() {
            JsonPath::root()
        } else {
            path(case.path_a)
        };
        let b = if case.path_b.is_empty() {
            JsonPath::root()
        } else {
            path(case.path_b)
        };

        let overlaps = a.overlaps(&b);
        assert_eq!(
            overlaps, case.should_overlap,
            "Path overlap test failed: '{}' vs '{}' ({}): expected {}, got {}",
            case.path_a, case.path_b, case.reason, case.should_overlap, overlaps
        );
    }
}

// =============================================================================
// Path Construction and Parsing Tests
// =============================================================================

/// Test path parsing roundtrips.
#[test]
fn test_path_parse_roundtrip() {
    let paths = vec![
        "user",
        "user.name",
        "user.profile.settings",
        "items[0]",
        "items[0].name",
        "data[0][1][2]",
        "complex.path[0].with[1].mixed.types",
    ];

    for path_str in paths {
        let parsed: JsonPath = path_str.parse().expect("parse failed");
        let reparsed: JsonPath = parsed.to_path_string().parse().expect("reparse failed");
        assert_eq!(parsed, reparsed, "Roundtrip failed for '{}'", path_str);
    }
}

/// Test that empty path is root.
#[test]
fn test_empty_path_is_root() {
    let root = JsonPath::root();
    let empty: JsonPath = "".parse().unwrap();

    assert!(root.is_root());
    assert!(empty.is_root());
    assert_eq!(root, empty);
}

// =============================================================================
// Common Ancestor Tests
// =============================================================================

/// Test common ancestor calculation.
#[test]
fn test_common_ancestor() {
    let p1 = path("a.b.c.d");
    let p2 = path("a.b.x.y");

    let common = p1.common_ancestor(&p2);
    assert_eq!(common, path("a.b"));
}

/// Common ancestor of sibling paths is their parent.
#[test]
fn test_common_ancestor_of_siblings() {
    let p1 = path("user.name");
    let p2 = path("user.age");

    let common = p1.common_ancestor(&p2);
    assert_eq!(common, path("user"));
}

/// Common ancestor of unrelated paths is root.
#[test]
fn test_common_ancestor_different_roots() {
    let p1 = path("users.alice");
    let p2 = path("config.settings");

    let common = p1.common_ancestor(&p2);
    assert!(common.is_root());
}

// =============================================================================
// Run Isolation Tests
// =============================================================================

/// Same doc_id in different runs are independent.
#[test]
fn test_run_isolation_same_doc_id() {
    let db = create_test_db();
    let store = JsonStore::new(db);

    let run1 = RunId::new();
    let run2 = RunId::new();
    let doc_id = JsonDocId::new();

    // Create doc in run1 with value 1
    store.create(&run1, &doc_id, JsonValue::from(1i64)).unwrap();

    // Create doc in run2 with value 2 (same doc_id!)
    store.create(&run2, &doc_id, JsonValue::from(2i64)).unwrap();

    // They are completely independent
    let val1 = store.get(&run1, &doc_id, &root()).unwrap().unwrap();
    let val2 = store.get(&run2, &doc_id, &root()).unwrap().unwrap();

    assert_eq!(val1.value.as_i64(), Some(1));
    assert_eq!(val2.value.as_i64(), Some(2));

    // Modifying one doesn't affect the other
    store
        .set(&run1, &doc_id, &root(), JsonValue::from(100i64))
        .unwrap();

    let val1_after = store.get(&run1, &doc_id, &root()).unwrap().unwrap();
    let val2_after = store.get(&run2, &doc_id, &root()).unwrap().unwrap();

    assert_eq!(val1_after.value.as_i64(), Some(100));
    assert_eq!(val2_after.value.as_i64(), Some(2)); // Unchanged
}

/// Destroying doc in one run doesn't affect another.
#[test]
fn test_run_isolation_destroy() {
    let db = create_test_db();
    let store = JsonStore::new(db);

    let run1 = RunId::new();
    let run2 = RunId::new();
    let doc_id = JsonDocId::new();

    store.create(&run1, &doc_id, JsonValue::from(1i64)).unwrap();
    store.create(&run2, &doc_id, JsonValue::from(2i64)).unwrap();

    // Destroy in run1
    store.destroy(&run1, &doc_id).unwrap();

    // run2 should be unaffected
    assert!(!store.exists(&run1, &doc_id).unwrap());
    assert!(store.exists(&run2, &doc_id).unwrap());
}
