//! Tier 8: Cross-Primitive Search (Hybrid)
//!
//! Tests for HybridSearch orchestration.

use crate::common::*;
use strata_core::search_types::{PrimitiveType, SearchRequest};
use strata_intelligence::{DatabaseSearchExt, HybridSearch, RRFFuser};
use std::collections::HashSet;
use std::sync::Arc;

// ============================================================================
// Hybrid Search Basic Tests
// ============================================================================

/// db.hybrid() returns a HybridSearch
#[test]
fn test_tier8_db_hybrid_returns_hybrid_search() {
    let db = create_test_db();
    let _hybrid = db.hybrid();
}

/// Hybrid search works on empty database
#[test]
fn test_tier8_hybrid_empty_db() {
    let db = create_test_db();
    let run_id = test_run_id();

    let hybrid = db.hybrid();
    let req = SearchRequest::new(run_id, "test");
    let response = hybrid.search(&req).unwrap();

    assert!(response.hits.is_empty());
}

/// Hybrid search finds results across primitives
#[test]
fn test_tier8_hybrid_finds_results() {
    let db = create_test_db();
    let run_id = test_run_id();
    populate_test_data(&db, &run_id);

    let hybrid = db.hybrid();
    let req = SearchRequest::new(run_id, "test");
    let response = hybrid.search(&req).unwrap();

    assert!(!response.hits.is_empty());
}

/// Hybrid search includes KV results
#[test]
fn test_tier8_hybrid_includes_kv() {
    let db = create_test_db();
    let run_id = test_run_id();
    populate_test_data(&db, &run_id);

    let hybrid = db.hybrid();
    let req = SearchRequest::new(run_id, "test");
    let response = hybrid.search(&req).unwrap();

    let primitives: HashSet<_> = response
        .hits
        .iter()
        .map(|h| h.doc_ref.primitive_type())
        .collect();

    assert!(primitives.contains(&PrimitiveType::Kv));
}

// ============================================================================
// Hybrid Search Filter Tests
// ============================================================================

/// Hybrid search respects primitive filter
#[test]
fn test_tier8_hybrid_respects_filter() {
    let db = create_test_db();
    let run_id = test_run_id();
    populate_test_data(&db, &run_id);

    let hybrid = db.hybrid();
    let req = SearchRequest::new(run_id, "test").with_primitive_filter(vec![PrimitiveType::Kv]);
    let response = hybrid.search(&req).unwrap();

    for hit in &response.hits {
        assert_eq!(hit.doc_ref.primitive_type(), PrimitiveType::Kv);
    }
}

/// Empty filter returns no results
#[test]
fn test_tier8_hybrid_empty_filter() {
    let db = create_test_db();
    let run_id = test_run_id();
    populate_test_data(&db, &run_id);

    let hybrid = db.hybrid();
    let req = SearchRequest::new(run_id, "test").with_primitive_filter(vec![]);
    let response = hybrid.search(&req).unwrap();

    assert!(response.hits.is_empty());
}

/// Multiple primitive filter works
#[test]
fn test_tier8_hybrid_multi_filter() {
    let db = create_test_db();
    let run_id = test_run_id();
    populate_test_data(&db, &run_id);

    let hybrid = db.hybrid();
    let req = SearchRequest::new(run_id, "test")
        .with_primitive_filter(vec![PrimitiveType::Kv, PrimitiveType::Run]);
    let response = hybrid.search(&req).unwrap();

    for hit in &response.hits {
        let kind = hit.doc_ref.primitive_type();
        assert!(
            kind == PrimitiveType::Kv || kind == PrimitiveType::Run,
            "Should only include filtered primitives"
        );
    }
}

// ============================================================================
// Hybrid Search Fuser Tests
// ============================================================================

/// Can use custom fuser
#[test]
fn test_tier8_hybrid_custom_fuser() {
    let db = create_test_db();
    let run_id = test_run_id();
    populate_test_data(&db, &run_id);

    let hybrid = HybridSearch::new(db.clone()).with_fuser(Arc::new(RRFFuser::default()));

    let req = SearchRequest::new(run_id, "test");
    let response = hybrid.search(&req).unwrap();

    assert!(!response.hits.is_empty());
}

/// RRF fuser produces valid results
#[test]
fn test_tier8_hybrid_rrf_valid() {
    let db = create_test_db();
    let run_id = test_run_id();
    populate_test_data(&db, &run_id);

    let hybrid = HybridSearch::new(db.clone()).with_fuser(Arc::new(RRFFuser::default()));

    let req = SearchRequest::new(run_id, "test").with_k(5);
    let response = hybrid.search(&req).unwrap();

    // Should have valid structure
    verify_scores_decreasing(&response);
    verify_ranks_sequential(&response);
}

// ============================================================================
// Hybrid Search Consistency Tests
// ============================================================================

/// Hybrid search is deterministic
#[test]
fn test_tier8_hybrid_deterministic() {
    let db = create_test_db();
    let run_id = test_run_id();
    populate_test_data(&db, &run_id);

    let req = SearchRequest::new(run_id, "test");
    verify_deterministic(&db, &req);
}

/// Hybrid search results have valid ranks
#[test]
fn test_tier8_hybrid_valid_ranks() {
    let db = create_test_db();
    let run_id = test_run_id();
    populate_test_data(&db, &run_id);

    let hybrid = db.hybrid();
    let req = SearchRequest::new(run_id, "test");
    let response = hybrid.search(&req).unwrap();

    verify_ranks_sequential(&response);
}

/// Hybrid search results have valid scores
#[test]
fn test_tier8_hybrid_valid_scores() {
    let db = create_test_db();
    let run_id = test_run_id();
    populate_test_data(&db, &run_id);

    let hybrid = db.hybrid();
    let req = SearchRequest::new(run_id, "test");
    let response = hybrid.search(&req).unwrap();

    verify_scores_decreasing(&response);
}

// ============================================================================
// Hybrid Search Stats Tests
// ============================================================================

/// Hybrid search populates stats
#[test]
fn test_tier8_hybrid_populates_stats() {
    let db = create_test_db();
    let run_id = test_run_id();
    populate_test_data(&db, &run_id);

    let hybrid = db.hybrid();
    let req = SearchRequest::new(run_id, "test");
    let response = hybrid.search(&req).unwrap();

    // Stats should be populated
    let _ = response.stats.elapsed_micros;
    let _ = response.stats.candidates_considered;
}

// ============================================================================
// HybridSearch Thread Safety Tests
// ============================================================================

/// HybridSearch is Send + Sync
#[test]
fn test_tier8_hybrid_send_sync() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<HybridSearch>();
}
