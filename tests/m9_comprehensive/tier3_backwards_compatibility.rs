//! Backwards Compatibility Tests
//!
//! Tests that existing code continues to work with type aliases:
//! - DocRef = EntityRef
//! - PrimitiveKind = PrimitiveType (deprecated)
//! - VersionedValue = Versioned<Value>

use crate::test_utils::test_run_id;
use strata_core::{
    contract::DocRef, // Alias for EntityRef
    EntityRef, JsonDocId, PrimitiveType, Value, Version, Versioned, VersionedValue,
};

// ============================================================================
// DocRef alias tests
// ============================================================================

#[test]
fn doc_ref_is_entity_ref_alias() {
    let run_id = test_run_id();

    // Create using DocRef type
    let doc_ref: DocRef = EntityRef::kv(run_id.clone(), "key");

    // Should be usable as EntityRef
    let entity_ref: EntityRef = doc_ref.clone();
    assert_eq!(doc_ref, entity_ref);
}

#[test]
fn doc_ref_variant_construction_works() {
    let run_id = test_run_id();

    // All EntityRef constructors work when assigned to DocRef
    let _: DocRef = EntityRef::kv(run_id.clone(), "key");
    let _: DocRef = EntityRef::event(run_id.clone(), 1);
    let _: DocRef = EntityRef::state(run_id.clone(), "state");
    let _: DocRef = EntityRef::run(run_id.clone());
    let _: DocRef = EntityRef::json(run_id.clone(), JsonDocId::new());
    let _: DocRef = EntityRef::vector(run_id, "coll", "vec");
}

#[test]
fn doc_ref_usable_in_existing_patterns() {
    let run_id = test_run_id();
    let doc_ref: DocRef = EntityRef::kv(run_id.clone(), "my_key");

    // Pattern: extract run_id
    assert_eq!(doc_ref.run_id(), run_id);

    // Pattern: get primitive type
    assert_eq!(doc_ref.primitive_type(), PrimitiveType::Kv);

    // Pattern: type check
    assert!(doc_ref.is_kv());

    // Pattern: extract key
    assert_eq!(doc_ref.kv_key(), Some("my_key"));
}

#[test]
fn doc_ref_and_entity_ref_are_interchangeable() {
    let run_id = test_run_id();

    fn accept_doc_ref(dr: DocRef) -> String {
        format!("{}", dr)
    }

    fn accept_entity_ref(er: EntityRef) -> String {
        format!("{}", er)
    }

    let entity_ref = EntityRef::kv(run_id.clone(), "key");
    let doc_ref: DocRef = entity_ref.clone();

    // Both should work with either function
    let s1 = accept_doc_ref(entity_ref.clone());
    let s2 = accept_entity_ref(doc_ref.clone());
    assert_eq!(s1, s2);
}

#[test]
fn doc_ref_in_collections() {
    use std::collections::{HashMap, HashSet};
    let run_id = test_run_id();

    // Can use DocRef in HashSet
    let mut set: HashSet<DocRef> = HashSet::new();
    set.insert(EntityRef::kv(run_id.clone(), "key1"));
    set.insert(EntityRef::kv(run_id.clone(), "key2"));
    assert_eq!(set.len(), 2);

    // Can use DocRef in HashMap
    let mut map: HashMap<DocRef, String> = HashMap::new();
    map.insert(EntityRef::kv(run_id, "key"), "value".to_string());
    assert_eq!(map.len(), 1);
}

// ============================================================================
// PrimitiveKind alias tests (deprecated)
// ============================================================================

#[test]
#[allow(deprecated)]
fn primitive_kind_is_primitive_type_alias() {
    use strata_core::PrimitiveKind;

    // PrimitiveKind should be usable as PrimitiveType
    let kind: PrimitiveKind = PrimitiveType::Kv;
    let ptype: PrimitiveType = kind;
    assert_eq!(kind, ptype);
}

#[test]
#[allow(deprecated)]
fn primitive_kind_all_variants_work() {
    use strata_core::PrimitiveKind;

    let kinds: Vec<PrimitiveKind> = vec![
        PrimitiveType::Kv,
        PrimitiveType::Event,
        PrimitiveType::State,
        PrimitiveType::Run,
        PrimitiveType::Json,
        PrimitiveType::Vector,
    ];

    assert_eq!(kinds.len(), 6);
}

#[test]
#[allow(deprecated)]
fn primitive_kind_methods_work() {
    use strata_core::PrimitiveKind;

    let kind: PrimitiveKind = PrimitiveType::Kv;

    // All methods should work
    assert_eq!(kind.name(), "KVStore");
    assert_eq!(kind.id(), "kv");
    assert!(kind.supports_crud());
    assert!(!kind.is_append_only());
}

// ============================================================================
// VersionedValue alias tests
// ============================================================================

#[test]
fn versioned_value_is_versioned_value_alias() {
    let vv: VersionedValue = Versioned::new(Value::Int(42), Version::txn(1));

    // Should be assignable to Versioned<Value>
    let v: Versioned<Value> = vv;
    assert_eq!(v.value(), &Value::Int(42));
}

#[test]
fn versioned_value_construction_works() {
    // All construction patterns should work
    let vv1: VersionedValue = Versioned::new(Value::Null, Version::txn(1));
    let vv2: VersionedValue = Versioned::new(Value::Bool(true), Version::seq(1));
    let vv3: VersionedValue = Versioned::new(Value::Int(42), Version::counter(1));

    assert!(vv1.version().is_txn_id());
    assert!(vv2.version().is_sequence());
    assert!(vv3.version().is_counter());
}

#[test]
fn versioned_value_methods_work() {
    let vv: VersionedValue = Versioned::new(Value::String("test".to_string()), Version::txn(5));

    // All Versioned<T> methods should work
    assert_eq!(vv.value(), &Value::String("test".to_string()));
    assert_eq!(vv.version().as_u64(), 5);
    assert!(vv.timestamp().as_micros() > 0);
}

#[test]
fn versioned_value_map_works() {
    let vv: VersionedValue = Versioned::new(Value::Int(42), Version::txn(1));

    let mapped: Versioned<String> = vv.map(|v| format!("{:?}", v));
    assert!(mapped.value().contains("42"));
}

#[test]
fn versioned_value_into_parts_works() {
    let vv: VersionedValue = Versioned::new(Value::Bool(true), Version::txn(10));
    let (value, version, _timestamp) = vv.into_parts();

    assert_eq!(value, Value::Bool(true));
    assert_eq!(version.as_u64(), 10);
}

// ============================================================================
// Mixed usage patterns
// ============================================================================

#[test]
fn mixed_alias_and_canonical_types() {
    let run_id = test_run_id();

    // Create with alias
    let doc_ref: DocRef = EntityRef::kv(run_id.clone(), "key");

    // Use with canonical type in Versioned
    let versioned: Versioned<EntityRef> = Versioned::new(doc_ref.clone(), Version::txn(1));

    // Extract and use as DocRef again
    let extracted: DocRef = versioned.into_value();
    assert_eq!(extracted, doc_ref);
}

#[test]
fn function_accepting_canonical_works_with_alias() {
    fn process_entity(er: &EntityRef) -> PrimitiveType {
        er.primitive_type()
    }

    let run_id = test_run_id();
    let doc_ref: DocRef = EntityRef::event(run_id, 1);

    // DocRef should work with function expecting EntityRef
    let pt = process_entity(&doc_ref);
    assert_eq!(pt, PrimitiveType::Event);
}

#[test]
fn function_accepting_alias_works_with_canonical() {
    fn process_doc(dr: &DocRef) -> PrimitiveType {
        dr.primitive_type()
    }

    let run_id = test_run_id();
    let entity_ref = EntityRef::state(run_id, "cell");

    // EntityRef should work with function expecting DocRef
    let pt = process_doc(&entity_ref);
    assert_eq!(pt, PrimitiveType::State);
}

// ============================================================================
// Serialization compatibility
// ============================================================================

#[test]
fn doc_ref_serialization_compatible_with_entity_ref() {
    let run_id = test_run_id();
    let doc_ref: DocRef = EntityRef::kv(run_id, "key");

    // Serialize as DocRef
    let json = serde_json::to_string(&doc_ref).expect("serialize");

    // Deserialize as EntityRef
    let entity_ref: EntityRef = serde_json::from_str(&json).expect("deserialize");

    assert_eq!(doc_ref, entity_ref);
}

#[test]
fn versioned_value_serialization_compatible() {
    let vv: VersionedValue = Versioned::new(Value::Int(42), Version::txn(1));

    // Serialize as VersionedValue
    let json = serde_json::to_string(&vv).expect("serialize");

    // Deserialize as Versioned<Value>
    let v: Versioned<Value> = serde_json::from_str(&json).expect("deserialize");

    assert_eq!(vv.value(), v.value());
    assert_eq!(vv.version(), v.version());
}

// ============================================================================
// Edge cases
// ============================================================================

#[test]
fn alias_equality_with_canonical() {
    let run_id = test_run_id();

    let doc_ref: DocRef = EntityRef::kv(run_id.clone(), "key");
    let entity_ref: EntityRef = EntityRef::kv(run_id, "key");

    // Should be equal
    assert_eq!(doc_ref, entity_ref);
}

#[test]
fn alias_hash_equals_canonical() {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let run_id = test_run_id();

    let doc_ref: DocRef = EntityRef::kv(run_id.clone(), "key");
    let entity_ref: EntityRef = EntityRef::kv(run_id, "key");

    let hash1 = {
        let mut h = DefaultHasher::new();
        doc_ref.hash(&mut h);
        h.finish()
    };

    let hash2 = {
        let mut h = DefaultHasher::new();
        entity_ref.hash(&mut h);
        h.finish()
    };

    assert_eq!(hash1, hash2);
}
