//! Conflict Matrix Tests
//!
//! Every row of the M5 spec conflict matrix becomes an executable test.
//! These tests validate region-based conflict detection semantics.
//!
//! From M5_ARCHITECTURE.md Section 8.2 (Conflict Matrix)

use crate::test_utils::*;
use strata_core::json::JsonPatch;

// =============================================================================
// Path Overlap Conflict Matrix Tests (Section 8.2)
// =============================================================================

/// Row 1: $.a.b vs $.a.c → NO conflict (siblings)
#[test]
fn test_conflict_matrix_siblings_no_conflict() {
    let path_a = path("a.b");
    let path_b = path("a.c");

    assert!(!path_a.overlaps(&path_b), "Siblings should NOT conflict");

    // Test with patches too
    let patch_a = JsonPatch::set("a.b", JsonValue::from(1i64));
    let patch_b = JsonPatch::set("a.c", JsonValue::from(2i64));

    assert!(
        !patch_a.conflicts_with(&patch_b),
        "Sibling patches should NOT conflict"
    );
}

/// Row 2: $.a.b vs $.a.b → YES conflict (same path)
#[test]
fn test_conflict_matrix_same_path_conflict() {
    let path_a = path("a.b");
    let path_b = path("a.b");

    assert!(path_a.overlaps(&path_b), "Same path should conflict");

    let patch_a = JsonPatch::set("a.b", JsonValue::from(1i64));
    let patch_b = JsonPatch::set("a.b", JsonValue::from(2i64));

    assert!(
        patch_a.conflicts_with(&patch_b),
        "Same path patches should conflict"
    );
}

/// Row 3: $.a vs $.a.b → YES conflict (A is ancestor of B)
#[test]
fn test_conflict_matrix_ancestor_conflict() {
    let ancestor = path("a");
    let descendant = path("a.b");

    assert!(
        ancestor.overlaps(&descendant),
        "Ancestor should conflict with descendant"
    );
    assert!(
        ancestor.is_ancestor_of(&descendant),
        "$.a is ancestor of $.a.b"
    );

    let patch_ancestor = JsonPatch::set("a", JsonValue::object());
    let patch_descendant = JsonPatch::set("a.b", JsonValue::from(1i64));

    assert!(patch_ancestor.conflicts_with(&patch_descendant));
}

/// Row 4: $.a.b vs $.a → YES conflict (B is ancestor of A)
#[test]
fn test_conflict_matrix_descendant_conflict() {
    let descendant = path("a.b");
    let ancestor = path("a");

    assert!(
        descendant.overlaps(&ancestor),
        "Descendant should conflict with ancestor"
    );
    assert!(
        descendant.is_descendant_of(&ancestor),
        "$.a.b is descendant of $.a"
    );

    let patch_descendant = JsonPatch::set("a.b", JsonValue::from(1i64));
    let patch_ancestor = JsonPatch::set("a", JsonValue::object());

    assert!(patch_descendant.conflicts_with(&patch_ancestor));
}

/// Row 5: $.x vs $.y → NO conflict (different subtrees)
#[test]
fn test_conflict_matrix_different_subtrees_no_conflict() {
    let path_x = path("x");
    let path_y = path("y");

    assert!(
        !path_x.overlaps(&path_y),
        "Different subtrees should NOT conflict"
    );

    let patch_x = JsonPatch::set("x", JsonValue::from(1i64));
    let patch_y = JsonPatch::set("y", JsonValue::from(2i64));

    assert!(!patch_x.conflicts_with(&patch_y));
}

/// Row 6: $ (root) vs $.anything → YES conflict (root overlaps everything)
#[test]
fn test_conflict_matrix_root_vs_any_conflict() {
    let root = JsonPath::root();
    let any_path = path("deeply.nested.path[0].value");

    assert!(
        root.overlaps(&any_path),
        "Root should conflict with any path"
    );
    assert!(
        any_path.overlaps(&root),
        "Any path should conflict with root"
    );

    let patch_root = JsonPatch::set_at(JsonPath::root(), JsonValue::object());
    let patch_any = JsonPatch::set("deeply.nested.path[0].value", JsonValue::from(1i64));

    assert!(patch_root.conflicts_with(&patch_any));
}

// =============================================================================
// Array Conflict Tests (Section 8.3)
// =============================================================================

// Note: M5 doesn't have direct array insert/remove APIs, but these tests
// verify the semantic invariant: array mutations conflict with element access.

/// Array replacement conflicts with element access.
/// (Simulates: insert at index 0 would shift all elements)
#[test]
fn test_array_mutation_conflicts_with_element_access() {
    // $.items overlaps with $.items[1].price
    // because modifying $.items changes what $.items[1] refers to
    let array_path = path("items");
    let element_path = path("items[1].price");

    assert!(
        array_path.overlaps(&element_path),
        "Array path should conflict with element access"
    );
}

/// Different array indices are siblings (no conflict).
#[test]
fn test_array_siblings_no_conflict() {
    let idx_0 = path("items[0]");
    let idx_1 = path("items[1]");
    let idx_2 = path("items[2]");

    assert!(
        !idx_0.overlaps(&idx_1),
        "Different indices should NOT conflict"
    );
    assert!(!idx_1.overlaps(&idx_2));
    assert!(!idx_0.overlaps(&idx_2));
}

/// Array element and its children overlap.
#[test]
fn test_array_element_descendant_conflict() {
    let element = path("items[0]");
    let child = path("items[0].name");

    assert!(
        element.overlaps(&child),
        "Element should conflict with its children"
    );
}

/// Different array subtrees don't conflict.
#[test]
fn test_different_array_subtrees_no_conflict() {
    let users_0 = path("users[0].name");
    let items_0 = path("items[0].name");

    assert!(
        !users_0.overlaps(&items_0),
        "Different array subtrees should NOT conflict"
    );
}

// =============================================================================
// Deep Path Conflict Tests
// =============================================================================

/// Deep ancestor-descendant conflict.
#[test]
fn test_deep_ancestor_descendant_conflict() {
    let shallow = path("a");
    let deep = path("a.b.c.d.e.f.g");

    assert!(
        shallow.overlaps(&deep),
        "Shallow ancestor conflicts with deep descendant"
    );
    assert!(shallow.is_strict_ancestor_of(&deep));
}

/// Deep sibling paths no conflict.
#[test]
fn test_deep_siblings_no_conflict() {
    let path1 = path("a.b.c.d.e.x");
    let path2 = path("a.b.c.d.e.y");

    assert!(!path1.overlaps(&path2), "Deep siblings should NOT conflict");
}

/// Paths that share partial prefix but diverge.
#[test]
fn test_partial_prefix_divergence() {
    let path1 = path("users.alice.profile");
    let path2 = path("users.bob.profile");

    assert!(
        !path1.overlaps(&path2),
        "Paths diverging at same depth should NOT conflict"
    );
}

// =============================================================================
// Mixed Key and Index Conflict Tests
// =============================================================================

/// Mixed key and index paths.
#[test]
fn test_mixed_key_index_conflict() {
    let parent = path("data[0]");
    let child = path("data[0].items[1].value");

    assert!(parent.overlaps(&child));
}

/// Complex path relationships.
#[test]
fn test_complex_path_relationships() {
    // These should conflict (ancestor-descendant)
    assert!(path("users").overlaps(&path("users[0]")));
    assert!(path("users[0]").overlaps(&path("users[0].name")));
    assert!(path("config").overlaps(&path("config.settings.theme")));

    // These should NOT conflict (siblings or different subtrees)
    assert!(!path("users[0]").overlaps(&path("users[1]")));
    assert!(!path("users[0].name").overlaps(&path("users[0].age")));
    assert!(!path("users").overlaps(&path("config")));
}

// =============================================================================
// Edge Cases
// =============================================================================

/// Empty path (root) edge cases.
#[test]
fn test_root_path_edge_cases() {
    let root = JsonPath::root();

    // Root overlaps with itself
    assert!(root.overlaps(&root));

    // Root is ancestor of everything
    assert!(root.is_ancestor_of(&path("anything")));
    assert!(root.is_ancestor_of(&path("a.b.c[0].d")));

    // Root is NOT strict ancestor of root
    assert!(!root.is_strict_ancestor_of(&root));
}

/// Single segment paths.
#[test]
fn test_single_segment_paths() {
    let a = path("a");
    let b = path("b");
    let a_child = path("a.x");

    assert!(!a.overlaps(&b));
    assert!(a.overlaps(&a_child));
}

// =============================================================================
// Comprehensive Conflict Matrix
// =============================================================================

/// Test all combinations systematically.
#[test]
fn test_comprehensive_conflict_matrix() {
    struct Case {
        path_a: &'static str,
        path_b: &'static str,
        conflicts: bool,
        description: &'static str,
    }

    let cases = vec![
        // Same path
        Case {
            path_a: "x",
            path_b: "x",
            conflicts: true,
            description: "same single key",
        },
        Case {
            path_a: "a.b.c",
            path_b: "a.b.c",
            conflicts: true,
            description: "same nested path",
        },
        Case {
            path_a: "arr[0]",
            path_b: "arr[0]",
            conflicts: true,
            description: "same array index",
        },
        // Ancestor/descendant
        Case {
            path_a: "a",
            path_b: "a.b",
            conflicts: true,
            description: "direct ancestor",
        },
        Case {
            path_a: "a.b",
            path_b: "a",
            conflicts: true,
            description: "direct descendant",
        },
        Case {
            path_a: "a",
            path_b: "a.b.c.d",
            conflicts: true,
            description: "deep descendant",
        },
        Case {
            path_a: "arr",
            path_b: "arr[0]",
            conflicts: true,
            description: "array ancestor",
        },
        Case {
            path_a: "arr[0]",
            path_b: "arr[0].x",
            conflicts: true,
            description: "element ancestor",
        },
        // Siblings (no conflict)
        Case {
            path_a: "a",
            path_b: "b",
            conflicts: false,
            description: "root siblings",
        },
        Case {
            path_a: "x.a",
            path_b: "x.b",
            conflicts: false,
            description: "nested siblings",
        },
        Case {
            path_a: "arr[0]",
            path_b: "arr[1]",
            conflicts: false,
            description: "index siblings",
        },
        Case {
            path_a: "x.a.deep",
            path_b: "x.b.deep",
            conflicts: false,
            description: "deep siblings",
        },
        // Different subtrees (no conflict)
        Case {
            path_a: "users.alice",
            path_b: "config.theme",
            conflicts: false,
            description: "different roots",
        },
        Case {
            path_a: "a[0].x",
            path_b: "b[0].x",
            conflicts: false,
            description: "different array roots",
        },
        // Complex mixed
        Case {
            path_a: "data[0].items[1]",
            path_b: "data[0].items[1].value",
            conflicts: true,
            description: "nested array ancestor",
        },
        Case {
            path_a: "data[0].items[1]",
            path_b: "data[0].items[2]",
            conflicts: false,
            description: "nested array siblings",
        },
        Case {
            path_a: "data[0]",
            path_b: "data[0].items[1].value",
            conflicts: true,
            description: "array element deep descendant",
        },
    ];

    for case in cases {
        let a = path(case.path_a);
        let b = path(case.path_b);

        let actual = a.overlaps(&b);
        assert_eq!(
            actual, case.conflicts,
            "Conflict test failed for '{}' vs '{}' ({}): expected {}, got {}",
            case.path_a, case.path_b, case.description, case.conflicts, actual
        );

        // Also verify symmetry
        let reverse = b.overlaps(&a);
        assert_eq!(
            actual, reverse,
            "Overlap should be symmetric for '{}' vs '{}'",
            case.path_a, case.path_b
        );
    }
}

// =============================================================================
// Patch-Specific Conflict Tests
// =============================================================================

/// Set-Set conflicts.
#[test]
fn test_set_set_conflicts() {
    // Overlapping
    assert!(JsonPatch::set("a", JsonValue::from(1i64))
        .conflicts_with(&JsonPatch::set("a.b", JsonValue::from(2i64))));

    // Non-overlapping
    assert!(!JsonPatch::set("a", JsonValue::from(1i64))
        .conflicts_with(&JsonPatch::set("b", JsonValue::from(2i64))));
}

/// Set-Delete conflicts.
#[test]
fn test_set_delete_conflicts() {
    // Overlapping
    assert!(JsonPatch::set("a.b", JsonValue::from(1i64)).conflicts_with(&JsonPatch::delete("a")));

    // Non-overlapping
    assert!(!JsonPatch::set("a", JsonValue::from(1i64)).conflicts_with(&JsonPatch::delete("b")));
}

/// Delete-Delete conflicts.
#[test]
fn test_delete_delete_conflicts() {
    // Overlapping
    assert!(JsonPatch::delete("a").conflicts_with(&JsonPatch::delete("a.b")));

    // Non-overlapping
    assert!(!JsonPatch::delete("a").conflicts_with(&JsonPatch::delete("b")));
}
