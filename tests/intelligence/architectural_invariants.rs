//! Tier 1: Architectural Rule Invariants
//!
//! These tests verify the six architectural rules from M6_ARCHITECTURE.md.
//! These are sacred invariants that must never break.

use crate::common::*;
use strata_core::search_types::{PrimitiveType, SearchRequest, SearchResponse};
use strata_core::types::RunId;
use strata_core::value::Value;
use strata_engine::{KVStore, RunIndex};
use strata_intelligence::{BM25LiteScorer, DatabaseSearchExt, Fuser, HybridSearch, RRFFuser, Scorer};
use std::collections::HashSet;
use std::sync::Arc;

// ============================================================================
// Rule 1: No Data Movement (DocRef references only)
// ============================================================================

/// Search returns DocRef, not actual data
#[test]
fn test_tier1_rule1_search_returns_docref_not_data() {
    let db = create_test_db();
    let run_id = test_run_id();
    populate_test_data(&db, &run_id);

    let kv = KVStore::new(db.clone());
    let req = SearchRequest::new(run_id, "test");
    let response = kv.search(&req).unwrap();

    // Verify we get DocRefs, not data
    for hit in &response.hits {
        // DocRef should be small (just a reference)
        assert!(std::mem::size_of_val(&hit.doc_ref) < 256);
        // Can get primitive type from DocRef
        let _ = hit.doc_ref.primitive_type();
        // Can get run_id from DocRef
        let _ = hit.doc_ref.run_id();
    }
}

/// DocRef size is bounded
#[test]
fn test_tier1_rule1_docref_size_bounded() {
    use strata_core::search_types::DocRef;

    // DocRef should be reasonably small
    assert!(
        std::mem::size_of::<DocRef>() < 256,
        "DocRef should be small"
    );
}

// ============================================================================
// Rule 2: Primitive Search is First-Class
// ============================================================================

/// All primitives implement Searchable trait
#[test]
fn test_tier1_rule2_all_primitives_searchable() {
    let db = create_test_db();
    let run_id = test_run_id();

    let kv = KVStore::new(db.clone());
    let run_index = RunIndex::new(db.clone());

    // Create a run first
    run_index.create_run(&run_id.to_string()).unwrap();

    let req = SearchRequest::new(run_id, "test");

    // KVStore implements Searchable
    let _: SearchResponse = kv.search(&req).unwrap();

    // RunIndex implements Searchable
    let _: SearchResponse = run_index.search(&req).unwrap();
}

/// Primitive search returns valid SearchResponse
#[test]
fn test_tier1_rule2_primitive_search_returns_search_response() {
    let db = create_test_db();
    let run_id = test_run_id();
    populate_test_data(&db, &run_id);

    let kv = KVStore::new(db.clone());
    let req = SearchRequest::new(run_id, "test");
    let response = kv.search(&req).unwrap();

    // Response has expected structure
    let _: &Vec<_> = &response.hits;
    let _: bool = response.truncated;
    let _: u64 = response.stats.elapsed_micros;
    let _: usize = response.stats.candidates_considered;
}

// ============================================================================
// Rule 3: Composite Orchestrates, Doesn't Replace
// ============================================================================

/// Hybrid search orchestrates primitives
#[test]
fn test_tier1_rule3_hybrid_orchestrates() {
    let db = create_test_db();
    let run_id = test_run_id();
    populate_test_data(&db, &run_id);

    let hybrid = db.hybrid();
    let req = SearchRequest::new(run_id, "test");
    let response = hybrid.search(&req).unwrap();

    // Results should come from primitives
    assert!(!response.hits.is_empty());

    // Results include primitive type
    for hit in &response.hits {
        let kind = hit.doc_ref.primitive_type();
        assert!(PrimitiveType::all().contains(&kind));
    }
}

/// Hybrid search respects primitive filter
#[test]
fn test_tier1_rule3_hybrid_respects_filter() {
    let db = create_test_db();
    let run_id = test_run_id();
    populate_test_data(&db, &run_id);

    let hybrid = db.hybrid();
    let req = SearchRequest::new(run_id, "test").with_primitive_filter(vec![PrimitiveType::Kv]);
    let response = hybrid.search(&req).unwrap();

    // All results should be from KV only
    assert_all_from_primitive(&response, PrimitiveType::Kv);
}

// ============================================================================
// Rule 4: Snapshot-Consistent Search
// ============================================================================

/// Search sees consistent snapshot
#[test]
fn test_tier1_rule4_snapshot_consistent() {
    let db = create_test_db();
    let run_id = test_run_id();

    let kv = KVStore::new(db.clone());
    let run_index = RunIndex::new(db.clone());

    run_index.create_run(&run_id.to_string()).unwrap();
    kv.put(&run_id, "initial", Value::String("searchable term".into()))
        .unwrap();

    // Start search
    let req = SearchRequest::new(run_id, "searchable");
    let response1 = kv.search(&req).unwrap();

    // Add more data
    kv.put(&run_id, "new", Value::String("searchable new".into()))
        .unwrap();

    // New search should see new data
    let response2 = kv.search(&req).unwrap();

    // Both searches should return valid results
    assert!(!response1.hits.is_empty());
    assert!(!response2.hits.is_empty());
}

// ============================================================================
// Rule 5: Zero Overhead When Disabled
// ============================================================================

/// Index is disabled by default
#[test]
fn test_tier1_rule5_index_disabled_by_default() {
    use strata_intelligence::InvertedIndex;

    let index = InvertedIndex::new();
    assert!(!index.is_enabled(), "Index should be disabled by default");
}

/// No index overhead when disabled
#[test]
fn test_tier1_rule5_no_overhead_when_disabled() {
    use strata_core::search_types::DocRef;
    use strata_intelligence::InvertedIndex;

    let index = InvertedIndex::new();
    let run_id = RunId::new();
    let doc_ref = DocRef::Kv {
        run_id,
        key: "test".to_string(),
    };

    // Adding documents when disabled should be a no-op
    index.index_document(&doc_ref, "test content", None);

    // Should still be empty (lookup returns None when disabled)
    assert!(index.lookup("test").is_none());
}

// ============================================================================
// Rule 6: Algorithm Swappable
// ============================================================================

/// Scorer is a trait (pluggable)
#[test]
fn test_tier1_rule6_scorer_is_trait() {
    fn accept_scorer<S: Scorer>(_: &S) {}

    let scorer = BM25LiteScorer::default();
    accept_scorer(&scorer);
}

/// Fuser is a trait (pluggable)
#[test]
fn test_tier1_rule6_fuser_is_trait() {
    fn accept_fuser<F: Fuser>(_: &F) {}

    let fuser = RRFFuser::default();
    accept_fuser(&fuser);
}

/// Can swap fuser in hybrid search
#[test]
fn test_tier1_rule6_can_swap_fuser() {
    let db = create_test_db();
    let run_id = test_run_id();
    populate_test_data(&db, &run_id);

    // Use custom fuser
    let hybrid = HybridSearch::new(db.clone()).with_fuser(Arc::new(RRFFuser::default()));

    let req = SearchRequest::new(run_id, "test");
    let response = hybrid.search(&req).unwrap();

    assert!(!response.hits.is_empty());
}

// ============================================================================
// Additional Invariants
// ============================================================================

/// PrimitiveType has exactly 6 variants
/// The six primitives are: Kv, Event, State, Run, Json, Vector
#[test]
fn test_tier1_primitive_type_count() {
    let all = PrimitiveType::all();
    assert_eq!(all.len(), 6, "Should have exactly 6 primitives");
}

/// All primitive types are distinct
#[test]
fn test_tier1_primitive_types_distinct() {
    let all = PrimitiveType::all();
    let set: HashSet<_> = all.iter().collect();
    assert_eq!(set.len(), 6, "All primitive types should be distinct");
}

/// DocRef correctly reports primitive type
#[test]
fn test_tier1_docref_primitive_type_correct() {
    let db = create_test_db();
    let run_id = test_run_id();
    populate_test_data(&db, &run_id);

    let kv = KVStore::new(db.clone());
    let req = SearchRequest::new(run_id, "test");
    let response = kv.search(&req).unwrap();

    for hit in &response.hits {
        assert_eq!(
            hit.doc_ref.primitive_type(),
            PrimitiveType::Kv,
            "DocRef from KV should report Kv primitive type"
        );
    }
}

/// DocRef correctly reports run_id
#[test]
fn test_tier1_docref_run_id_correct() {
    let db = create_test_db();
    let run_id = test_run_id();
    populate_test_data(&db, &run_id);

    let kv = KVStore::new(db.clone());
    let req = SearchRequest::new(run_id, "test");
    let response = kv.search(&req).unwrap();

    for hit in &response.hits {
        assert_eq!(
            hit.doc_ref.run_id(),
            run_id,
            "DocRef should contain correct run_id"
        );
    }
}
