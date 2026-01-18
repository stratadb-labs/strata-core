//! Tier 6: Cross-Primitive Identity
//!
//! Tests for DocRef identity policies and deduplication behavior.

use super::test_utils::*;
use in_mem_core::search_types::{DocRef, PrimitiveKind, SearchRequest};
use in_mem_core::types::{JsonDocId, Key, Namespace, RunId};
use in_mem_core::value::Value;
use in_mem_primitives::{KVStore, RunIndex};
use in_mem_search::DatabaseSearchExt;
use std::collections::HashSet;

// ============================================================================
// DocRef Identity Policy Tests
// ============================================================================

/// DocRefs from different primitives are NEVER equal
#[test]
fn test_tier6_docrefs_different_primitives_never_equal() {
    let run_id = RunId::new();
    let ns = Namespace::for_run(run_id);

    let kv_ref = DocRef::Kv {
        key: Key::new_kv(ns.clone(), "shared_name"),
    };
    let json_ref = DocRef::Json {
        key: Key::new_json(ns.clone(), &JsonDocId::new()),
        doc_id: JsonDocId::new(),
    };
    let run_ref = DocRef::Run { run_id };

    // POLICY: DocRefs from different primitives are NEVER equal
    assert_ne!(kv_ref, json_ref);
    assert_ne!(kv_ref, run_ref);
    assert_ne!(json_ref, run_ref);
}

/// DocRefs from same primitive with same key ARE equal
#[test]
fn test_tier6_docrefs_same_primitive_same_key_equal() {
    let run_id = RunId::new();
    let ns = Namespace::for_run(run_id);

    let ref1 = DocRef::Kv {
        key: Key::new_kv(ns.clone(), "same_key"),
    };
    let ref2 = DocRef::Kv {
        key: Key::new_kv(ns.clone(), "same_key"),
    };

    assert_eq!(ref1, ref2);
}

/// DocRefs from same primitive with different keys are NOT equal
#[test]
fn test_tier6_docrefs_same_primitive_different_key_not_equal() {
    let run_id = RunId::new();
    let ns = Namespace::for_run(run_id);

    let ref1 = DocRef::Kv {
        key: Key::new_kv(ns.clone(), "key1"),
    };
    let ref2 = DocRef::Kv {
        key: Key::new_kv(ns.clone(), "key2"),
    };

    assert_ne!(ref1, ref2);
}

/// DocRefs from same primitive but different runs are NOT equal
#[test]
fn test_tier6_docrefs_different_runs_not_equal() {
    let run1 = RunId::new();
    let run2 = RunId::new();
    let ns1 = Namespace::for_run(run1);
    let ns2 = Namespace::for_run(run2);

    let ref1 = DocRef::Kv {
        key: Key::new_kv(ns1, "same_key"),
    };
    let ref2 = DocRef::Kv {
        key: Key::new_kv(ns2, "same_key"),
    };

    // Same key name but different runs = NOT equal
    assert_ne!(ref1, ref2);
}

// ============================================================================
// DocRef Hashing Tests
// ============================================================================

/// DocRefs can be used in HashSet
#[test]
fn test_tier6_docrefs_hashable() {
    let run_id = RunId::new();
    let ns = Namespace::for_run(run_id);

    let ref1 = DocRef::Kv {
        key: Key::new_kv(ns.clone(), "key1"),
    };
    let ref2 = DocRef::Kv {
        key: Key::new_kv(ns.clone(), "key2"),
    };
    let ref3 = DocRef::Kv {
        key: Key::new_kv(ns.clone(), "key1"), // Duplicate of ref1
    };

    let mut set = HashSet::new();
    set.insert(ref1.clone());
    set.insert(ref2.clone());
    set.insert(ref3.clone());

    // ref3 is duplicate of ref1, so set should have 2 elements
    assert_eq!(set.len(), 2);
    assert!(set.contains(&ref1));
    assert!(set.contains(&ref2));
}

// ============================================================================
// Deduplication Policy Tests
// ============================================================================

/// Within-primitive search never returns duplicates
#[test]
fn test_tier6_within_primitive_no_duplicates() {
    let db = create_test_db();
    let run_id = test_run_id();

    let kv = KVStore::new(db.clone());
    let run_index = RunIndex::new(db.clone());

    run_index.create_run(&run_id.to_string()).unwrap();

    // Add multiple entries with overlapping content
    for i in 0..10 {
        kv.put(
            &run_id,
            &format!("key_{}", i),
            Value::String("common search term".into()),
        )
        .unwrap();
    }

    let req = SearchRequest::new(run_id, "common").with_k(20);
    let response = kv.search(&req).unwrap();

    // Check for duplicates
    let refs: HashSet<_> = response.hits.iter().map(|h| &h.doc_ref).collect();
    assert_eq!(
        refs.len(),
        response.hits.len(),
        "Within-primitive search should never have duplicates"
    );
}

/// Cross-primitive NO deduplication (application layer responsibility)
#[test]
fn test_tier6_cross_primitive_no_deduplication() {
    // This is a POLICY test: we document that cross-primitive
    // deduplication is NOT performed by the search layer.
    // The application layer must handle it if needed.

    let db = create_test_db();
    let run_id = test_run_id();
    populate_test_data(&db, &run_id);

    let hybrid = db.hybrid();
    let req = SearchRequest::new(run_id, "test");
    let response = hybrid.search(&req).unwrap();

    // Results from different primitives may logically refer to the same
    // entity, but DocRefs are distinct (different variants)
    for hit in &response.hits {
        // Each hit should have a valid primitive kind
        let _kind = hit.doc_ref.primitive_kind();
    }
}

// ============================================================================
// Primitive Kind Correctness Tests
// ============================================================================

/// DocRef.primitive_kind() returns correct variant
#[test]
fn test_tier6_primitive_kind_correct() {
    let run_id = RunId::new();
    let ns = Namespace::for_run(run_id);

    let kv_ref = DocRef::Kv {
        key: Key::new_kv(ns.clone(), "test"),
    };
    assert_eq!(kv_ref.primitive_kind(), PrimitiveKind::Kv);

    let json_ref = DocRef::Json {
        key: Key::new_json(ns.clone(), &JsonDocId::new()),
        doc_id: JsonDocId::new(),
    };
    assert_eq!(json_ref.primitive_kind(), PrimitiveKind::Json);

    let event_ref = DocRef::Event {
        log_key: Key::new_event(ns.clone(), 0),
        seq: 42,
    };
    assert_eq!(event_ref.primitive_kind(), PrimitiveKind::Event);

    let state_ref = DocRef::State {
        key: Key::new_state(ns.clone(), "cell"),
    };
    assert_eq!(state_ref.primitive_kind(), PrimitiveKind::State);

    let trace_ref = DocRef::Trace {
        key: Key::new_trace(ns.clone(), 0),
        span_id: 123,
    };
    assert_eq!(trace_ref.primitive_kind(), PrimitiveKind::Trace);

    let run_ref = DocRef::Run { run_id };
    assert_eq!(run_ref.primitive_kind(), PrimitiveKind::Run);
}

/// DocRef.run_id() returns correct run
#[test]
fn test_tier6_run_id_correct() {
    let run_id = RunId::new();
    let ns = Namespace::for_run(run_id);

    let kv_ref = DocRef::Kv {
        key: Key::new_kv(ns.clone(), "test"),
    };
    assert_eq!(kv_ref.run_id(), run_id);

    let run_ref = DocRef::Run { run_id };
    assert_eq!(run_ref.run_id(), run_id);
}
