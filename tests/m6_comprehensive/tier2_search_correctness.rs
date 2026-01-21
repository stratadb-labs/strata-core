//! Tier 2: Search Correctness
//!
//! Validates search determinism, exhaustiveness, and filter behavior.

use super::test_utils::*;
use strata_core::search_types::{PrimitiveType, SearchRequest};
use strata_core::types::RunId;
use strata_core::value::Value;
use strata_primitives::{KVStore, RunIndex};
use strata_search::DatabaseSearchExt;

// ============================================================================
// Determinism Tests
// ============================================================================

/// Same request produces identical results
#[test]
fn test_tier2_search_deterministic() {
    let db = create_test_db();
    let run_id = test_run_id();
    populate_test_data(&db, &run_id);

    let req = SearchRequest::new(run_id, "test");
    verify_deterministic(&db, &req);
}

/// Primitive search is deterministic
#[test]
fn test_tier2_primitive_search_deterministic() {
    let db = create_test_db();
    let run_id = test_run_id();
    populate_test_data(&db, &run_id);

    let kv = KVStore::new(db.clone());
    let req = SearchRequest::new(run_id, "test");

    // Execute same search multiple times
    let results: Vec<_> = (0..5).map(|_| kv.search(&req).unwrap()).collect();

    // All results should be identical
    for (i, result) in results.iter().enumerate().skip(1) {
        assert_eq!(
            result.hits.len(),
            results[0].hits.len(),
            "Iteration {} should have same hit count",
            i
        );

        for (h1, h2) in result.hits.iter().zip(results[0].hits.iter()) {
            assert_eq!(h1.doc_ref, h2.doc_ref, "DocRefs should match");
            assert_eq!(h1.rank, h2.rank, "Ranks should match");
        }
    }
}

/// Hybrid search is deterministic
#[test]
fn test_tier2_hybrid_search_deterministic() {
    let db = create_test_db();
    let run_id = test_run_id();
    populate_test_data(&db, &run_id);

    let hybrid = db.hybrid();
    let req = SearchRequest::new(run_id, "test");

    let r1 = hybrid.search(&req).unwrap();
    let r2 = hybrid.search(&req).unwrap();

    assert_eq!(r1.hits.len(), r2.hits.len());
    for (h1, h2) in r1.hits.iter().zip(r2.hits.iter()) {
        assert_eq!(h1.doc_ref, h2.doc_ref);
    }
}

// ============================================================================
// Run Isolation Tests
// ============================================================================

/// Search respects run_id filter
#[test]
fn test_tier2_search_respects_run_id() {
    let db = create_test_db();
    let run1 = RunId::new();
    let run2 = RunId::new();

    let kv = KVStore::new(db.clone());
    let run_index = RunIndex::new(db.clone());

    run_index.create_run(&run1.to_string()).unwrap();
    run_index.create_run(&run2.to_string()).unwrap();

    // Add shared term to both runs
    kv.put(&run1, "key1", Value::String("shared test term".into()))
        .unwrap();
    kv.put(&run2, "key2", Value::String("shared test term".into()))
        .unwrap();

    // Search run1 only
    let req = SearchRequest::new(run1, "shared");
    let response = kv.search(&req).unwrap();

    // All results should belong to run1
    assert_all_from_run(&response, run1);
}

/// Run isolation between different runs
#[test]
fn test_tier2_run_isolation() {
    let db = create_test_db();
    let run1 = RunId::new();
    let run2 = RunId::new();

    let kv = KVStore::new(db.clone());
    let run_index = RunIndex::new(db.clone());

    run_index.create_run(&run1.to_string()).unwrap();
    run_index.create_run(&run2.to_string()).unwrap();

    // Add same key with different values to different runs
    kv.put(&run1, "key", Value::String("run1 test value".into()))
        .unwrap();
    kv.put(&run2, "key", Value::String("run2 test value".into()))
        .unwrap();

    // Search run1
    let req1 = SearchRequest::new(run1, "test");
    let r1 = kv.search(&req1).unwrap();

    // Search run2
    let req2 = SearchRequest::new(run2, "test");
    let r2 = kv.search(&req2).unwrap();

    // Results should be isolated
    assert_all_from_run(&r1, run1);
    assert_all_from_run(&r2, run2);
}

/// Non-existent run returns empty results
#[test]
fn test_tier2_nonexistent_run_empty() {
    let db = create_test_db();
    let run_id = RunId::new();

    let hybrid = db.hybrid();
    let req = SearchRequest::new(run_id, "test");
    let response = hybrid.search(&req).unwrap();

    assert!(
        response.hits.is_empty(),
        "Non-existent run should return empty results"
    );
}

// ============================================================================
// Primitive Filter Tests
// ============================================================================

/// Primitive filter limits search scope
#[test]
fn test_tier2_primitive_filter_works() {
    let db = create_test_db();
    let run_id = test_run_id();
    populate_test_data(&db, &run_id);

    let hybrid = db.hybrid();

    // Search only KV primitive
    let req = SearchRequest::new(run_id, "test").with_primitive_filter(vec![PrimitiveType::Kv]);
    let response = hybrid.search(&req).unwrap();

    // All results should be from KV only
    assert_all_from_primitive(&response, PrimitiveType::Kv);
}

/// Empty primitive filter means no results
#[test]
fn test_tier2_empty_filter_no_results() {
    let db = create_test_db();
    let run_id = test_run_id();
    populate_test_data(&db, &run_id);

    let hybrid = db.hybrid();

    // Search with empty filter
    let req = SearchRequest::new(run_id, "test").with_primitive_filter(vec![]);
    let response = hybrid.search(&req).unwrap();

    assert!(
        response.hits.is_empty(),
        "Empty filter should produce no results"
    );
}

// ============================================================================
// Result Ordering Tests
// ============================================================================

/// Scores are monotonically decreasing
#[test]
fn test_tier2_scores_monotonically_decreasing() {
    let db = create_test_db();
    let run_id = test_run_id();
    populate_test_data(&db, &run_id);

    let hybrid = db.hybrid();
    let req = SearchRequest::new(run_id, "test").with_k(20);
    let response = hybrid.search(&req).unwrap();

    verify_scores_decreasing(&response);
}

/// Ranks are sequential starting from 1
#[test]
fn test_tier2_ranks_sequential() {
    let db = create_test_db();
    let run_id = test_run_id();
    populate_test_data(&db, &run_id);

    let kv = KVStore::new(db.clone());
    let req = SearchRequest::new(run_id, "test").with_k(10);
    let response = kv.search(&req).unwrap();

    verify_ranks_sequential(&response);
}

/// Top-k respects k parameter
#[test]
fn test_tier2_respects_k_parameter() {
    let db = create_test_db();
    let run_id = test_run_id();
    populate_large_dataset(&db, &run_id, 50);

    let kv = KVStore::new(db.clone());

    // Search with k=5
    let req = SearchRequest::new(run_id, "searchable").with_k(5);
    let response = kv.search(&req).unwrap();

    assert!(response.hits.len() <= 5, "Should respect k parameter");
}

/// Smaller k is prefix of larger k
#[test]
fn test_tier2_consistent_across_k_values() {
    let db = create_test_db();
    let run_id = test_run_id();
    populate_test_data(&db, &run_id);

    let hybrid = db.hybrid();

    let req_k3 = SearchRequest::new(run_id, "test").with_k(3);
    let req_k10 = SearchRequest::new(run_id, "test").with_k(10);

    let r3 = hybrid.search(&req_k3).unwrap();
    let r10 = hybrid.search(&req_k10).unwrap();

    // Smaller k results should be prefix of larger k results
    for (i, hit) in r3.hits.iter().enumerate() {
        if i < r10.hits.len() {
            assert_eq!(
                hit.doc_ref, r10.hits[i].doc_ref,
                "Top-3 should be prefix of top-10"
            );
        }
    }
}
