//! PrimitiveType Invariant Tests
//!
//! Tests that PrimitiveType correctly expresses Invariant 6: Everything is Introspectable
//!
//! Every primitive has a type that can be queried and enumerated.

use strata_core::PrimitiveType;
use std::collections::HashSet;

// ============================================================================
// All 6 primitives enumerated
// ============================================================================

#[test]
fn primitive_type_has_exactly_six_variants() {
    let all = PrimitiveType::all();
    assert_eq!(all.len(), 6);
}

#[test]
fn primitive_type_all_returns_all_variants() {
    let all = PrimitiveType::all();
    let expected = vec![
        PrimitiveType::Kv,
        PrimitiveType::Event,
        PrimitiveType::State,
        PrimitiveType::Run,
        PrimitiveType::Json,
        PrimitiveType::Vector,
    ];

    assert_eq!(all.len(), expected.len());
    for pt in &expected {
        assert!(all.contains(pt), "Missing primitive type: {:?}", pt);
    }
}

#[test]
fn primitive_type_variants_are_unique() {
    let all = PrimitiveType::all();
    let unique: HashSet<_> = all.iter().collect();
    assert_eq!(unique.len(), 6);
}

// ============================================================================
// Each primitive has name and id
// ============================================================================

#[test]
fn primitive_type_name_returns_human_readable() {
    assert_eq!(PrimitiveType::Kv.name(), "KVStore");
    assert_eq!(PrimitiveType::Event.name(), "EventLog");
    assert_eq!(PrimitiveType::State.name(), "StateCell");
    assert_eq!(PrimitiveType::Run.name(), "RunIndex");
    assert_eq!(PrimitiveType::Json.name(), "JsonStore");
    assert_eq!(PrimitiveType::Vector.name(), "VectorStore");
}

#[test]
fn primitive_type_id_returns_short_form() {
    assert_eq!(PrimitiveType::Kv.id(), "kv");
    assert_eq!(PrimitiveType::Event.id(), "event");
    assert_eq!(PrimitiveType::State.id(), "state");
    assert_eq!(PrimitiveType::Run.id(), "run");
    assert_eq!(PrimitiveType::Json.id(), "json");
    assert_eq!(PrimitiveType::Vector.id(), "vector");
}

#[test]
fn primitive_type_from_id_parses_correctly() {
    assert_eq!(PrimitiveType::from_id("kv"), Some(PrimitiveType::Kv));
    assert_eq!(PrimitiveType::from_id("event"), Some(PrimitiveType::Event));
    assert_eq!(PrimitiveType::from_id("state"), Some(PrimitiveType::State));
    assert_eq!(PrimitiveType::from_id("run"), Some(PrimitiveType::Run));
    assert_eq!(PrimitiveType::from_id("json"), Some(PrimitiveType::Json));
    assert_eq!(PrimitiveType::from_id("vector"), Some(PrimitiveType::Vector));
}

#[test]
fn primitive_type_from_id_rejects_invalid() {
    assert_eq!(PrimitiveType::from_id("invalid"), None);
    assert_eq!(PrimitiveType::from_id(""), None);
    assert_eq!(PrimitiveType::from_id("KV"), None); // Case sensitive
    assert_eq!(PrimitiveType::from_id("EVENT"), None);
}

#[test]
fn primitive_type_id_roundtrips() {
    for pt in PrimitiveType::all() {
        let id = pt.id();
        let parsed = PrimitiveType::from_id(id);
        assert_eq!(parsed, Some(*pt), "Roundtrip failed for {:?}", pt);
    }
}

// ============================================================================
// CRUD vs append-only classification
// ============================================================================

#[test]
fn primitive_type_crud_classification_correct() {
    // CRUD primitives: KV, State, Json, Vector, Run
    assert!(PrimitiveType::Kv.supports_crud());
    assert!(PrimitiveType::State.supports_crud());
    assert!(PrimitiveType::Json.supports_crud());
    assert!(PrimitiveType::Vector.supports_crud());
    assert!(PrimitiveType::Run.supports_crud());

    // Append-only primitives don't support CRUD
    assert!(!PrimitiveType::Event.supports_crud());
}

#[test]
fn primitive_type_append_only_classification_correct() {
    // Append-only primitives: Event
    assert!(PrimitiveType::Event.is_append_only());

    // CRUD primitives are not append-only
    assert!(!PrimitiveType::Kv.is_append_only());
    assert!(!PrimitiveType::State.is_append_only());
    assert!(!PrimitiveType::Json.is_append_only());
    assert!(!PrimitiveType::Vector.is_append_only());
    assert!(!PrimitiveType::Run.is_append_only());
}

#[test]
fn primitive_type_crud_and_append_only_are_exclusive() {
    for pt in PrimitiveType::all() {
        // A primitive is either CRUD or append-only, not both
        assert!(
            pt.supports_crud() != pt.is_append_only(),
            "{:?} should be either CRUD or append-only, not both or neither",
            pt
        );
    }
}

// ============================================================================
// PrimitiveType is hashable
// ============================================================================

#[test]
fn primitive_type_hashable() {
    let mut set = HashSet::new();
    for pt in PrimitiveType::all() {
        set.insert(*pt);
    }
    assert_eq!(set.len(), 6);
}

#[test]
fn primitive_type_equal_values_same_hash() {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let pt1 = PrimitiveType::Kv;
    let pt2 = PrimitiveType::Kv;

    let hash1 = {
        let mut h = DefaultHasher::new();
        pt1.hash(&mut h);
        h.finish()
    };
    let hash2 = {
        let mut h = DefaultHasher::new();
        pt2.hash(&mut h);
        h.finish()
    };

    assert_eq!(hash1, hash2);
}

// ============================================================================
// PrimitiveType serialization
// ============================================================================

#[test]
fn primitive_type_serialization_roundtrip() {
    for pt in PrimitiveType::all() {
        let json = serde_json::to_string(pt).expect("serialize");
        let parsed: PrimitiveType = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(*pt, parsed);
    }
}

// ============================================================================
// PrimitiveType Display
// ============================================================================

#[test]
fn primitive_type_display_is_informative() {
    for pt in PrimitiveType::all() {
        let display = format!("{}", pt);
        assert!(!display.is_empty());
        // Display should be the name
        assert_eq!(display, pt.name());
    }
}

// ============================================================================
// PrimitiveType Clone and Copy
// ============================================================================

#[test]
fn primitive_type_is_copy() {
    let pt1 = PrimitiveType::Kv;
    let pt2 = pt1; // Copy
    let pt3 = pt1; // Copy again

    assert_eq!(pt1, pt2);
    assert_eq!(pt2, pt3);
}

#[test]
fn primitive_type_clone() {
    let pt1 = PrimitiveType::Event;
    let pt2 = pt1.clone();

    assert_eq!(pt1, pt2);
}

// ============================================================================
// PrimitiveType equality
// ============================================================================

#[test]
fn primitive_type_equality() {
    assert_eq!(PrimitiveType::Kv, PrimitiveType::Kv);
    assert_ne!(PrimitiveType::Kv, PrimitiveType::Event);
}

// ============================================================================
// Coverage of all primitives
// ============================================================================

#[test]
fn all_primitives_have_names() {
    for pt in PrimitiveType::all() {
        let name = pt.name();
        assert!(!name.is_empty(), "{:?} has empty name", pt);
    }
}

#[test]
fn all_primitives_have_ids() {
    for pt in PrimitiveType::all() {
        let id = pt.id();
        assert!(!id.is_empty(), "{:?} has empty id", pt);
        // IDs should be lowercase
        assert_eq!(id, id.to_lowercase(), "{:?} id should be lowercase", pt);
    }
}

#[test]
fn primitive_ids_are_unique() {
    let ids: Vec<_> = PrimitiveType::all().iter().map(|pt| pt.id()).collect();
    let unique: HashSet<_> = ids.iter().collect();
    assert_eq!(unique.len(), 6, "Primitive IDs should be unique");
}

#[test]
fn primitive_names_are_unique() {
    let names: Vec<_> = PrimitiveType::all().iter().map(|pt| pt.name()).collect();
    let unique: HashSet<_> = names.iter().collect();
    assert_eq!(unique.len(), 6, "Primitive names should be unique");
}
