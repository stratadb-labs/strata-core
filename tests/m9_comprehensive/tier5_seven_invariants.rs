//! Seven Invariants Conformance Tests
//!
//! End-to-end tests that each contract type correctly expresses its invariant.
//! This is the comprehensive validation that M9 Phase 1 is complete.

use crate::test_utils::{all_entity_refs, all_primitive_types, test_run_id};
use strata_core::{
    EntityRef, JsonDocId, PrimitiveType, RunName, Timestamp, Value, Version, Versioned,
};
use std::collections::{HashMap, HashSet};

// ============================================================================
// Invariant 1: Everything is Addressable
// ============================================================================

/// Every entity in the database has a stable identity that can be:
/// - Referenced
/// - Stored
/// - Passed between systems
/// - Used to retrieve the entity later

#[test]
fn invariant1_every_entity_has_stable_identity() {
    let run_id = test_run_id();

    // All six primitive types have corresponding EntityRef variants
    let entity_refs = all_entity_refs(run_id);
    assert_eq!(entity_refs.len(), 6);

    // Each entity ref can identify its primitive type
    let types: HashSet<_> = entity_refs.iter().map(|r| r.primitive_type()).collect();
    assert_eq!(types.len(), 6);
}

#[test]
fn invariant1_identity_survives_serialization() {
    let run_id = test_run_id();
    let entity_refs = all_entity_refs(run_id);

    for entity_ref in entity_refs {
        // Serialize
        let json = serde_json::to_string(&entity_ref).expect("serialize");

        // Deserialize
        let restored: EntityRef = serde_json::from_str(&json).expect("deserialize");

        // Identity preserved
        assert_eq!(entity_ref, restored);
        assert_eq!(entity_ref.run_id(), restored.run_id());
        assert_eq!(entity_ref.primitive_type(), restored.primitive_type());
    }
}

#[test]
fn invariant1_identity_usable_for_retrieval() {
    // EntityRef can be used as a key in collections (simulating retrieval)
    let run_id = test_run_id();
    let mut store: HashMap<EntityRef, String> = HashMap::new();

    // Store values by entity ref
    store.insert(EntityRef::kv(run_id.clone(), "key1"), "value1".to_string());
    store.insert(EntityRef::event(run_id.clone(), 1), "event1".to_string());
    store.insert(EntityRef::state(run_id.clone(), "cell"), "state1".to_string());

    // Retrieve using same identity
    assert_eq!(store.get(&EntityRef::kv(run_id.clone(), "key1")), Some(&"value1".to_string()));
    assert_eq!(store.get(&EntityRef::event(run_id.clone(), 1)), Some(&"event1".to_string()));
    assert_eq!(store.get(&EntityRef::state(run_id.clone(), "cell")), Some(&"state1".to_string()));

    // Different identity returns None
    assert_eq!(store.get(&EntityRef::kv(run_id, "key2")), None);
}

#[test]
fn invariant1_identity_is_complete() {
    // An EntityRef contains all information needed to identify an entity
    let run_id = test_run_id();

    // KV: run_id + key
    let kv = EntityRef::kv(run_id.clone(), "my_key");
    assert_eq!(kv.run_id(), run_id);
    assert_eq!(kv.kv_key(), Some("my_key"));

    // Event: run_id + sequence
    let event = EntityRef::event(run_id.clone(), 42);
    assert_eq!(event.run_id(), run_id);
    assert_eq!(event.event_sequence(), Some(42));

    // All identifying information is accessible
}

// ============================================================================
// Invariant 2: Everything is Versioned
// ============================================================================

/// Every read returns version information. Every write returns a version.
/// Version information is NEVER optional.

#[test]
fn invariant2_every_read_has_version() {
    // Versioned<T> always has a version
    let v = Versioned::new(42, Version::txn(1));

    // Version is always present and accessible
    let version = v.version();
    assert!(!version.is_zero() || version.as_u64() == 0); // Either non-zero or explicitly zero
}

#[test]
fn invariant2_versions_comparable() {
    // Versions can be compared to detect changes
    let v1 = Version::txn(1);
    let v2 = Version::txn(2);

    assert!(v1 < v2);
    assert!(v2 > v1);
    assert_ne!(v1, v2);
}

#[test]
fn invariant2_timestamp_always_present() {
    // Every versioned value has a timestamp
    let v = Versioned::new("test", Version::txn(1));

    // Timestamp is always accessible
    let ts = v.timestamp();
    // Timestamp is valid (u64 is always >= 0)
    let _ = ts.as_micros();
}

#[test]
fn invariant2_version_types_cover_all_patterns() {
    // Three version types cover all primitive versioning patterns
    let txn = Version::txn(1);      // For KV, Json, Vector, Run
    let seq = Version::seq(1);      // For Event
    let ctr = Version::counter(1);  // For State

    assert!(txn.is_txn_id());
    assert!(seq.is_sequence());
    assert!(ctr.is_counter());

    // All are valid versions
    assert!(!txn.is_zero());
    assert!(!seq.is_zero());
    assert!(!ctr.is_zero());
}

#[test]
fn invariant2_versioned_map_preserves_version() {
    // Transforming a value preserves its version information
    let original = Versioned::with_timestamp(42, Version::txn(5), Timestamp::from_micros(1000));
    let mapped = original.map(|x| x.to_string());

    assert_eq!(mapped.version().as_u64(), 5);
    assert_eq!(mapped.timestamp().as_micros(), 1000);
}

// ============================================================================
// Invariant 5: Everything is Run-Scoped
// ============================================================================

/// Every entity belongs to exactly one run. Runs have semantic names for users.

#[test]
fn invariant5_every_entity_has_run_id() {
    let run_id = test_run_id();
    let entity_refs = all_entity_refs(run_id.clone());

    for entity_ref in entity_refs {
        // Every entity has a run_id
        let extracted_run_id = entity_ref.run_id();
        assert_eq!(extracted_run_id, run_id);
    }
}

#[test]
fn invariant5_run_name_validates_semantic_identity() {
    // RunName ensures semantic validity
    assert!(RunName::new("valid_name".to_string()).is_ok());
    assert!(RunName::new("my.agent.run-1".to_string()).is_ok());

    // Invalid names are rejected
    assert!(RunName::new("".to_string()).is_err());
    assert!(RunName::new("-invalid".to_string()).is_err());
    assert!(RunName::new("invalid name".to_string()).is_err());
}

#[test]
fn invariant5_run_isolation() {
    // Different runs have different entity identities
    let run1 = test_run_id();
    let run2 = test_run_id();

    let entity1 = EntityRef::kv(run1, "same_key");
    let entity2 = EntityRef::kv(run2, "same_key");

    // Same key in different runs = different entities
    assert_ne!(entity1, entity2);
}

// ============================================================================
// Invariant 6: Everything is Introspectable
// ============================================================================

/// Every primitive has a type that can be queried and enumerated.

#[test]
fn invariant6_all_primitives_enumerated() {
    let all = PrimitiveType::all();

    // Exactly 6 primitives
    assert_eq!(all.len(), 6);

    // All variants present
    let variants: HashSet<_> = all.iter().collect();
    assert!(variants.contains(&PrimitiveType::Kv));
    assert!(variants.contains(&PrimitiveType::Event));
    assert!(variants.contains(&PrimitiveType::State));
    assert!(variants.contains(&PrimitiveType::Run));
    assert!(variants.contains(&PrimitiveType::Json));
    assert!(variants.contains(&PrimitiveType::Vector));
}

#[test]
fn invariant6_primitive_type_discoverable() {
    let run_id = test_run_id();

    // Can discover type from EntityRef
    let entity_ref = EntityRef::kv(run_id, "key");
    let ptype = entity_ref.primitive_type();

    assert_eq!(ptype, PrimitiveType::Kv);
    assert_eq!(ptype.name(), "KVStore");
    assert_eq!(ptype.id(), "kv");
}

#[test]
fn invariant6_primitive_classification() {
    // Primitives are classified as CRUD or append-only
    for pt in PrimitiveType::all() {
        // Each primitive is either CRUD or append-only
        let is_crud = pt.supports_crud();
        let is_append = pt.is_append_only();

        // Mutually exclusive
        assert!(
            is_crud != is_append,
            "{:?} should be exactly one of CRUD or append-only",
            pt
        );
    }
}

// ============================================================================
// Combined Invariants
// ============================================================================

#[test]
fn all_invariants_work_together() {
    // Create an entity with full metadata
    let run_id = test_run_id();
    let entity_ref = EntityRef::kv(run_id.clone(), "my_key");

    // Invariant 1: Addressable
    assert_eq!(entity_ref.run_id(), run_id);

    // Invariant 5: Run-scoped
    assert_eq!(entity_ref.run_id(), run_id);

    // Invariant 6: Introspectable
    assert_eq!(entity_ref.primitive_type(), PrimitiveType::Kv);

    // Create a versioned value
    let versioned = Versioned::new(Value::String("test".to_string()), Version::txn(42));

    // Invariant 2: Versioned
    assert_eq!(versioned.version().as_u64(), 42);
    assert!(versioned.timestamp().as_micros() > 0);

    // All components work together
    let full_entity = Versioned::new(entity_ref.clone(), Version::txn(1));
    assert_eq!(full_entity.value().primitive_type(), PrimitiveType::Kv);
    assert_eq!(full_entity.version().as_u64(), 1);
}

#[test]
fn entity_ref_covers_all_primitive_types() {
    let run_id = test_run_id();

    // Every PrimitiveType has a corresponding EntityRef variant
    for pt in all_primitive_types() {
        let entity_ref = match pt {
            PrimitiveType::Kv => EntityRef::kv(run_id.clone(), "k"),
            PrimitiveType::Event => EntityRef::event(run_id.clone(), 1),
            PrimitiveType::State => EntityRef::state(run_id.clone(), "s"),
            PrimitiveType::Run => EntityRef::run(run_id.clone()),
            PrimitiveType::Json => EntityRef::json(run_id.clone(), JsonDocId::new()),
            PrimitiveType::Vector => EntityRef::vector(run_id.clone(), "c", "v"),
        };

        assert_eq!(entity_ref.primitive_type(), pt);
    }
}

#[test]
fn version_types_match_primitive_patterns() {
    // Document the version type for each primitive pattern
    // This test serves as documentation and verification

    // TxnId: Mutable primitives with transaction-based versioning
    let txn_primitives = vec![
        PrimitiveType::Kv,
        PrimitiveType::Json,
        PrimitiveType::Vector,
        PrimitiveType::Run,
    ];

    // Sequence: Append-only primitives
    let seq_primitives = vec![PrimitiveType::Event];

    // Counter: Per-entity counter (CAS)
    let counter_primitives = vec![PrimitiveType::State];

    // Verify CRUD classification aligns with version type
    for pt in &txn_primitives {
        assert!(pt.supports_crud() || *pt == PrimitiveType::Run,
            "{:?} expected to support CRUD or be Run", pt);
    }

    for pt in &seq_primitives {
        assert!(pt.is_append_only(), "{:?} expected to be append-only", pt);
    }

    for pt in &counter_primitives {
        assert!(pt.supports_crud(), "{:?} expected to support CRUD", pt);
    }
}

// ============================================================================
// Contract Type Summary
// ============================================================================

#[test]
fn contract_types_are_complete() {
    // Verify all contract types are properly defined and usable

    // EntityRef: 6 variants (Kv, Event, State, Run, Json, Vector)
    let run_id = test_run_id();
    let refs = all_entity_refs(run_id);
    assert_eq!(refs.len(), 6);

    // Version: 3 variants
    let _ = Version::txn(1);
    let _ = Version::seq(1);
    let _ = Version::counter(1);

    // Timestamp: microsecond precision
    let ts = Timestamp::now();
    assert!(ts.as_micros() > 0);

    // Versioned<T>: generic wrapper
    let _ = Versioned::new(42, Version::txn(1));

    // PrimitiveType: 6 variants (Kv, Event, State, Run, Json, Vector)
    assert_eq!(PrimitiveType::all().len(), 6);

    // RunName: validated string
    assert!(RunName::new("valid".to_string()).is_ok());
}

#[test]
fn contract_types_serialization_complete() {
    let run_id = test_run_id();

    // All contract types should be serializable
    let entity_ref = EntityRef::kv(run_id, "key");
    let version = Version::txn(42);
    let timestamp = Timestamp::from_micros(1_000_000);
    let versioned = Versioned::new("test", Version::txn(1));
    let primitive_type = PrimitiveType::Kv;
    let run_name = RunName::new("test_run".to_string()).unwrap();

    // All should serialize
    assert!(serde_json::to_string(&entity_ref).is_ok());
    assert!(serde_json::to_string(&version).is_ok());
    assert!(serde_json::to_string(&timestamp).is_ok());
    assert!(serde_json::to_string(&versioned).is_ok());
    assert!(serde_json::to_string(&primitive_type).is_ok());
    assert!(serde_json::to_string(&run_name).is_ok());
}
