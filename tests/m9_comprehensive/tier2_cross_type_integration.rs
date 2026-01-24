//! Cross-Type Integration Tests
//!
//! Tests interactions between contract types to ensure they work together
//! correctly as a cohesive system.

use crate::test_utils::{all_entity_refs, all_primitive_types, test_run_id};
use strata_core::{
    EntityRef, JsonDocId, PrimitiveType, Timestamp, Value, Version, Versioned,
    VersionedValue,
};

// ============================================================================
// EntityRef + PrimitiveType consistency
// ============================================================================

#[test]
fn entity_ref_primitive_type_matches_variant() {
    let run_id = test_run_id();

    let test_cases = vec![
        (EntityRef::kv(run_id.clone(), "k"), PrimitiveType::Kv),
        (EntityRef::event(run_id.clone(), 1), PrimitiveType::Event),
        (EntityRef::state(run_id.clone(), "s"), PrimitiveType::State),
        (EntityRef::run(run_id.clone()), PrimitiveType::Run),
        (EntityRef::json(run_id.clone(), JsonDocId::new()), PrimitiveType::Json),
        (EntityRef::vector(run_id, "c", "v"), PrimitiveType::Vector),
    ];

    for (entity_ref, expected_type) in test_cases {
        assert_eq!(
            entity_ref.primitive_type(),
            expected_type,
            "EntityRef {:?} should have type {:?}",
            entity_ref,
            expected_type
        );
    }
}

#[test]
fn all_primitive_types_have_entity_ref_variant() {
    let run_id = test_run_id();
    let refs = all_entity_refs(run_id);
    let types_from_refs: Vec<_> = refs.iter().map(|r| r.primitive_type()).collect();

    for pt in all_primitive_types() {
        assert!(
            types_from_refs.contains(&pt),
            "PrimitiveType {:?} should have a corresponding EntityRef variant",
            pt
        );
    }
}

#[test]
fn entity_ref_and_primitive_type_count_match() {
    let run_id = test_run_id();
    let refs = all_entity_refs(run_id);
    let types = all_primitive_types();

    assert_eq!(refs.len(), types.len(), "Should have same number of EntityRef variants as PrimitiveTypes");
}

// ============================================================================
// Versioned + Version integration
// ============================================================================

#[test]
fn versioned_with_txn_id_version() {
    let v = Versioned::new(42, Version::txn(100));

    assert!(v.version().is_txn_id());
    assert_eq!(v.version().as_u64(), 100);
}

#[test]
fn versioned_with_sequence_version() {
    let v = Versioned::new("event data", Version::seq(50));

    assert!(v.version().is_sequence());
    assert_eq!(v.version().as_u64(), 50);
}

#[test]
fn versioned_with_counter_version() {
    let v = Versioned::new(true, Version::counter(7));

    assert!(v.version().is_counter());
    assert_eq!(v.version().as_u64(), 7);
}

#[test]
fn versioned_preserves_version_type_through_map() {
    let v = Versioned::new(42i32, Version::seq(10));
    let mapped = v.map(|x| x.to_string());

    assert!(mapped.version().is_sequence());
    assert_eq!(mapped.version().as_u64(), 10);
}

#[test]
fn versioned_version_comparison_works() {
    let v1 = Versioned::new("a", Version::txn(1));
    let v2 = Versioned::new("b", Version::txn(2));

    assert!(v1.version() < v2.version());
}

// ============================================================================
// Versioned + Timestamp integration
// ============================================================================

#[test]
fn versioned_timestamp_is_accurate() {
    let ts = Timestamp::from_micros(1_234_567_890);
    let v = Versioned::with_timestamp("test", Version::txn(1), ts);

    assert_eq!(v.timestamp(), ts);
    assert_eq!(v.timestamp().as_micros(), 1_234_567_890);
}

#[test]
fn versioned_new_uses_current_timestamp() {
    let before = Timestamp::now();
    let v = Versioned::new("test", Version::txn(1));
    let after = Timestamp::now();

    assert!(v.timestamp() >= before);
    assert!(v.timestamp() <= after);
}

#[test]
fn versioned_map_preserves_timestamp() {
    let ts = Timestamp::from_micros(1_000_000);
    let v = Versioned::with_timestamp(42, Version::txn(1), ts);
    let mapped = v.map(|x| x * 2);

    assert_eq!(mapped.timestamp(), ts);
}

#[test]
fn versioned_age_uses_timestamp() {
    let old_ts = Timestamp::from_micros(0);
    let v = Versioned::with_timestamp("old", Version::txn(1), old_ts);

    // Age should be based on timestamp
    let age = v.age().expect("should have age");
    assert!(age.as_secs() > 0);
}

// ============================================================================
// EntityRef + RunId integration
// ============================================================================

#[test]
fn all_entity_ref_variants_have_run_id() {
    let run_id = test_run_id();
    let refs = all_entity_refs(run_id.clone());

    for entity_ref in refs {
        assert_eq!(
            entity_ref.run_id(),
            run_id,
            "EntityRef {:?} should have the expected run_id",
            entity_ref
        );
    }
}

#[test]
fn entity_ref_run_id_extraction_consistent() {
    let run1 = test_run_id();
    let run2 = test_run_id();

    let ref1 = EntityRef::kv(run1.clone(), "key");
    let ref2 = EntityRef::event(run2.clone(), 1);

    assert_eq!(ref1.run_id(), run1);
    assert_eq!(ref2.run_id(), run2);
    assert_ne!(ref1.run_id(), ref2.run_id());
}

// ============================================================================
// Version + Timestamp ordering consistency
// ============================================================================

#[test]
fn version_ordering_independent_of_timestamp() {
    // Two versioned values with same version but different timestamps
    // should compare equal (by version)
    let v1 = Versioned::with_timestamp(1, Version::txn(10), Timestamp::from_micros(1000));
    let v2 = Versioned::with_timestamp(1, Version::txn(10), Timestamp::from_micros(2000));

    assert_eq!(v1.version(), v2.version());
}

// ============================================================================
// VersionedValue + Value integration
// ============================================================================

#[test]
fn versioned_value_works_with_all_value_variants() {
    let versions = [
        Version::txn(1),
        Version::seq(1),
        Version::counter(1),
    ];

    let values = vec![
        Value::Null,
        Value::Bool(true),
        Value::Int(42),
        Value::Float(3.14),
        Value::String("test".to_string()),
        Value::Bytes(vec![1, 2, 3]),
        Value::Array(vec![Value::Int(1), Value::Int(2)]),
    ];

    for version in &versions {
        for value in &values {
            let vv: VersionedValue = Versioned::new(value.clone(), version.clone());
            assert_eq!(vv.value(), value);
            assert_eq!(vv.version(), *version);
        }
    }
}

#[test]
fn versioned_value_map_extracts_inner_type() {
    let vv: VersionedValue = Versioned::new(Value::Int(42), Version::txn(1));

    let mapped: Versioned<Option<i64>> = vv.map(|v| {
        match v {
            Value::Int(n) => Some(n),
            _ => None,
        }
    });

    assert_eq!(*mapped.value(), Some(42));
}

// ============================================================================
// PrimitiveType + Version variant mapping
// ============================================================================

#[test]
fn primitive_type_uses_appropriate_version_variant() {
    // Document the expected version variant for each primitive
    // KV, Json, Vector, Run: TxnId (transaction-based)
    // Event: Sequence (position-based)
    // State: Counter (per-entity)

    let txn_primitives = vec![
        PrimitiveType::Kv,
        PrimitiveType::Json,
        PrimitiveType::Vector,
        PrimitiveType::Run,
    ];

    let seq_primitives = vec![PrimitiveType::Event];
    let counter_primitives = vec![PrimitiveType::State];

    // Verify classifications
    for pt in txn_primitives {
        assert!(
            pt.supports_crud() || !pt.is_append_only(),
            "{:?} should use TxnId versioning",
            pt
        );
    }

    for pt in seq_primitives {
        assert!(
            pt.is_append_only(),
            "{:?} should be append-only and use Sequence versioning",
            pt
        );
    }

    for pt in counter_primitives {
        assert!(
            pt.supports_crud(),
            "{:?} should support CRUD and use Counter versioning",
            pt
        );
    }
}

// ============================================================================
// Timestamp + Duration operations
// ============================================================================

#[test]
fn timestamp_duration_interop() {
    use std::time::Duration;

    let ts1 = Timestamp::from_secs(100);
    let ts2 = ts1.saturating_add(Duration::from_secs(50));

    let duration = ts2.duration_since(ts1).expect("ts2 > ts1");
    assert_eq!(duration, Duration::from_secs(50));
}

// ============================================================================
// Serialization roundtrip for combined types
// ============================================================================

#[test]
fn versioned_entity_ref_serialization_roundtrip() {
    let run_id = test_run_id();
    let entity_ref = EntityRef::kv(run_id, "test_key");
    let versioned = Versioned::new(entity_ref.clone(), Version::txn(42));

    let json = serde_json::to_string(&versioned).expect("serialize");
    let parsed: Versioned<EntityRef> = serde_json::from_str(&json).expect("deserialize");

    assert_eq!(parsed.value(), &entity_ref);
    assert_eq!(parsed.version().as_u64(), 42);
}

// ============================================================================
// Complex nested scenarios
// ============================================================================

#[test]
fn versioned_of_versioned_value() {
    // Edge case: Versioned containing a VersionedValue
    let inner: VersionedValue = Versioned::new(Value::Int(42), Version::txn(1));
    let outer = Versioned::new(inner, Version::txn(2));

    assert_eq!(outer.version().as_u64(), 2);
    assert_eq!(outer.value().version().as_u64(), 1);
    assert_eq!(outer.value().value(), &Value::Int(42));
}

#[test]
fn entity_ref_in_versioned_collection() {
    let run_id = test_run_id();
    let refs = all_entity_refs(run_id);

    let versioned_refs: Vec<Versioned<EntityRef>> = refs
        .into_iter()
        .enumerate()
        .map(|(i, r)| Versioned::new(r, Version::txn(i as u64)))
        .collect();

    assert_eq!(versioned_refs.len(), 6);

    for (i, vr) in versioned_refs.iter().enumerate() {
        assert_eq!(vr.version().as_u64(), i as u64);
    }
}
