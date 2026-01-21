//! EntityRef Invariant Tests
//!
//! Tests that EntityRef correctly expresses Invariant 1: Everything is Addressable
//!
//! Every entity in the database has a stable identity that can be:
//! - Referenced
//! - Stored
//! - Passed between systems
//! - Used to retrieve the entity later

use crate::test_utils::{all_entity_refs, assert_same_hash, test_run_id};
use strata_core::{EntityRef, JsonDocId, PrimitiveType};
use std::collections::{HashMap, HashSet};

// ============================================================================
// Invariant 1: Every entity has a stable identity
// ============================================================================

#[test]
fn entity_ref_uniquely_identifies_kv_entry() {
    let run_id = test_run_id();
    let ref1 = EntityRef::kv(run_id.clone(), "key1");
    let ref2 = EntityRef::kv(run_id.clone(), "key1");
    let ref3 = EntityRef::kv(run_id.clone(), "key2");

    // Same key = same ref
    assert_eq!(ref1, ref2);

    // Different key = different ref
    assert_ne!(ref1, ref3);
}

#[test]
fn entity_ref_uniquely_identifies_event() {
    let run_id = test_run_id();
    let ref1 = EntityRef::event(run_id.clone(), 1);
    let ref2 = EntityRef::event(run_id.clone(), 1);
    let ref3 = EntityRef::event(run_id.clone(), 2);

    assert_eq!(ref1, ref2);
    assert_ne!(ref1, ref3);
}

#[test]
fn entity_ref_uniquely_identifies_state() {
    let run_id = test_run_id();
    let ref1 = EntityRef::state(run_id.clone(), "cell1");
    let ref2 = EntityRef::state(run_id.clone(), "cell1");
    let ref3 = EntityRef::state(run_id.clone(), "cell2");

    assert_eq!(ref1, ref2);
    assert_ne!(ref1, ref3);
}

#[test]
fn entity_ref_uniquely_identifies_trace() {
    let run_id = test_run_id();
    let ref1 = EntityRef::trace(run_id, "trace-1001");
    let ref2 = EntityRef::trace(run_id, "trace-1001");
    let ref3 = EntityRef::trace(run_id, "trace-1002");

    assert_eq!(ref1, ref2);
    assert_ne!(ref1, ref3);
}

#[test]
fn entity_ref_uniquely_identifies_run() {
    let run_id1 = test_run_id();
    let run_id2 = test_run_id();
    let ref1 = EntityRef::run(run_id1.clone());
    let ref2 = EntityRef::run(run_id1.clone());
    let ref3 = EntityRef::run(run_id2);

    assert_eq!(ref1, ref2);
    assert_ne!(ref1, ref3);
}

#[test]
fn entity_ref_uniquely_identifies_json_doc() {
    let run_id = test_run_id();
    let doc_id1 = JsonDocId::new();
    let doc_id2 = JsonDocId::new();
    let ref1 = EntityRef::json(run_id.clone(), doc_id1.clone());
    let ref2 = EntityRef::json(run_id.clone(), doc_id1);
    let ref3 = EntityRef::json(run_id.clone(), doc_id2);

    assert_eq!(ref1, ref2);
    assert_ne!(ref1, ref3);
}

#[test]
fn entity_ref_uniquely_identifies_vector() {
    let run_id = test_run_id();
    let ref1 = EntityRef::vector(run_id.clone(), "coll1", "vec1");
    let ref2 = EntityRef::vector(run_id.clone(), "coll1", "vec1");
    let ref3 = EntityRef::vector(run_id.clone(), "coll1", "vec2");
    let ref4 = EntityRef::vector(run_id.clone(), "coll2", "vec1");

    assert_eq!(ref1, ref2);
    assert_ne!(ref1, ref3); // Different vector
    assert_ne!(ref1, ref4); // Different collection
}

// ============================================================================
// Same entity = same reference
// ============================================================================

#[test]
fn same_entity_produces_equal_refs() {
    let run_id = test_run_id();

    // Creating the same reference twice should be equal
    for _ in 0..10 {
        let ref1 = EntityRef::kv(run_id.clone(), "consistent_key");
        let ref2 = EntityRef::kv(run_id.clone(), "consistent_key");
        assert_eq!(ref1, ref2);
    }
}

#[test]
fn different_entities_produce_different_refs() {
    let run_id = test_run_id();
    let refs = all_entity_refs(run_id);

    // Each variant should be different from all others
    for (i, ref_i) in refs.iter().enumerate() {
        for (j, ref_j) in refs.iter().enumerate() {
            if i != j {
                assert_ne!(ref_i, ref_j, "Entity refs at index {} and {} should differ", i, j);
            }
        }
    }
}

#[test]
fn different_runs_produce_different_refs() {
    let run1 = test_run_id();
    let run2 = test_run_id();

    // Same key in different runs = different refs
    let ref1 = EntityRef::kv(run1, "same_key");
    let ref2 = EntityRef::kv(run2, "same_key");

    assert_ne!(ref1, ref2);
}

// ============================================================================
// EntityRef is hashable (for collections)
// ============================================================================

#[test]
fn entity_ref_hashable_for_collections() {
    let run_id = test_run_id();
    let refs = all_entity_refs(run_id);

    let mut set = HashSet::new();
    for entity_ref in &refs {
        set.insert(entity_ref.clone());
    }

    // All 7 variants should be in the set
    assert_eq!(set.len(), 7);
}

#[test]
fn entity_ref_usable_as_map_key() {
    let run_id = test_run_id();
    let mut map: HashMap<EntityRef, String> = HashMap::new();

    map.insert(EntityRef::kv(run_id.clone(), "key1"), "value1".to_string());
    map.insert(EntityRef::event(run_id.clone(), 1), "event1".to_string());
    map.insert(EntityRef::state(run_id.clone(), "cell"), "state1".to_string());

    assert_eq!(map.get(&EntityRef::kv(run_id.clone(), "key1")), Some(&"value1".to_string()));
    assert_eq!(map.get(&EntityRef::event(run_id.clone(), 1)), Some(&"event1".to_string()));
    assert_eq!(map.get(&EntityRef::state(run_id.clone(), "cell")), Some(&"state1".to_string()));
}

#[test]
fn equal_entity_refs_have_same_hash() {
    let run_id = test_run_id();

    let ref1 = EntityRef::kv(run_id.clone(), "key");
    let ref2 = EntityRef::kv(run_id.clone(), "key");

    assert_same_hash(&ref1, &ref2);
}

// ============================================================================
// EntityRef preserves all identifying information
// ============================================================================

#[test]
fn entity_ref_kv_preserves_key() {
    let run_id = test_run_id();
    let entity_ref = EntityRef::kv(run_id.clone(), "my_key");

    assert_eq!(entity_ref.kv_key(), Some("my_key"));
    assert_eq!(entity_ref.run_id(), run_id);
}

#[test]
fn entity_ref_event_preserves_sequence() {
    let run_id = test_run_id();
    let entity_ref = EntityRef::event(run_id.clone(), 42);

    assert_eq!(entity_ref.event_sequence(), Some(42));
    assert_eq!(entity_ref.run_id(), run_id);
}

#[test]
fn entity_ref_state_preserves_name() {
    let run_id = test_run_id();
    let entity_ref = EntityRef::state(run_id.clone(), "my_cell");

    assert_eq!(entity_ref.state_name(), Some("my_cell"));
    assert_eq!(entity_ref.run_id(), run_id);
}

#[test]
fn entity_ref_trace_preserves_trace_id() {
    let run_id = test_run_id();
    let entity_ref = EntityRef::trace(run_id, "trace-9999");

    assert_eq!(entity_ref.trace_id(), Some("trace-9999"));
    assert_eq!(entity_ref.run_id(), run_id);
}

#[test]
fn entity_ref_json_preserves_doc_id() {
    let run_id = test_run_id();
    let doc_id = JsonDocId::new();
    let entity_ref = EntityRef::json(run_id.clone(), doc_id.clone());

    assert_eq!(entity_ref.json_doc_id(), Some(doc_id));
    assert_eq!(entity_ref.run_id(), run_id);
}

#[test]
fn entity_ref_vector_preserves_location() {
    let run_id = test_run_id();
    let entity_ref = EntityRef::vector(run_id.clone(), "my_coll", "my_vec");

    assert_eq!(entity_ref.vector_location(), Some(("my_coll", "my_vec")));
    assert_eq!(entity_ref.run_id(), run_id);
}

// ============================================================================
// EntityRef correctly reports primitive type
// ============================================================================

#[test]
fn entity_ref_reports_correct_primitive_type() {
    let run_id = test_run_id();

    assert_eq!(EntityRef::kv(run_id.clone(), "k").primitive_type(), PrimitiveType::Kv);
    assert_eq!(EntityRef::event(run_id.clone(), 1).primitive_type(), PrimitiveType::Event);
    assert_eq!(EntityRef::state(run_id.clone(), "s").primitive_type(), PrimitiveType::State);
    assert_eq!(EntityRef::trace(run_id, "trace-1").primitive_type(), PrimitiveType::Trace);
    assert_eq!(EntityRef::run(run_id.clone()).primitive_type(), PrimitiveType::Run);
    assert_eq!(EntityRef::json(run_id.clone(), JsonDocId::new()).primitive_type(), PrimitiveType::Json);
    assert_eq!(EntityRef::vector(run_id, "c", "v").primitive_type(), PrimitiveType::Vector);
}

// ============================================================================
// EntityRef type check methods
// ============================================================================

#[test]
fn entity_ref_type_checks_are_exclusive() {
    let run_id = test_run_id();
    let refs = all_entity_refs(run_id);

    for entity_ref in refs {
        let checks = vec![
            entity_ref.is_kv(),
            entity_ref.is_event(),
            entity_ref.is_state(),
            entity_ref.is_trace(),
            entity_ref.is_run(),
            entity_ref.is_json(),
            entity_ref.is_vector(),
        ];

        // Exactly one check should be true
        let true_count = checks.iter().filter(|&&x| x).count();
        assert_eq!(true_count, 1, "Exactly one type check should be true for {:?}", entity_ref);
    }
}

// ============================================================================
// EntityRef serialization roundtrip
// ============================================================================

#[test]
fn entity_ref_serialization_roundtrip() {
    let run_id = test_run_id();
    let refs = all_entity_refs(run_id);

    for entity_ref in refs {
        let serialized = serde_json::to_string(&entity_ref).expect("serialize");
        let deserialized: EntityRef = serde_json::from_str(&serialized).expect("deserialize");
        assert_eq!(entity_ref, deserialized, "Roundtrip failed for {:?}", entity_ref);
    }
}

// ============================================================================
// EntityRef Display formatting
// ============================================================================

#[test]
fn entity_ref_display_is_informative() {
    let run_id = test_run_id();
    let refs = all_entity_refs(run_id);

    for entity_ref in refs {
        let display = format!("{}", entity_ref);
        // Display should not be empty
        assert!(!display.is_empty());
        // Display should contain some identifying information
        assert!(display.len() > 5, "Display too short: {}", display);
    }
}

// ============================================================================
// EntityRef Clone and Debug
// ============================================================================

#[test]
fn entity_ref_clone_produces_equal_ref() {
    let run_id = test_run_id();
    let entity_ref = EntityRef::kv(run_id, "key");
    let cloned = entity_ref.clone();

    assert_eq!(entity_ref, cloned);
}

#[test]
fn entity_ref_debug_is_implemented() {
    let run_id = test_run_id();
    let entity_ref = EntityRef::kv(run_id, "key");
    let debug = format!("{:?}", entity_ref);

    assert!(debug.contains("Kv"));
    assert!(debug.contains("key"));
}
