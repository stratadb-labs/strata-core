//! M6 Determinism and Consistency Tests
//!
//! Validates that search operations are deterministic and consistent.
//! Determinism tests
//!
//! See `docs/architecture/M6_ARCHITECTURE.md` for authoritative specification.

use strata_core::search_types::{PrimitiveType, SearchRequest};
use strata_core::types::RunId;
use strata_core::value::Value;
use strata_engine::Database;
use strata_primitives::{KVStore, RunIndex};
use strata_search::{DatabaseSearchExt, HybridSearch, RRFFuser};
use std::sync::Arc;

// ============================================================================
// Test Helpers
// ============================================================================

fn test_db() -> Arc<Database> {
    Arc::new(
        Database::builder()
            .in_memory()
            .open_temp()
            .expect("Failed to create test database"),
    )
}

fn populate_determinism_data(db: &Arc<Database>, run_id: &RunId) {
    let kv = KVStore::new(db.clone());
    let run_index = RunIndex::new(db.clone());

    // Create run
    run_index.create_run(&run_id.to_string()).unwrap();

    // Add multiple documents with similar scores
    kv.put(run_id, "doc_a", Value::String("test document alpha".into()))
        .unwrap();
    kv.put(run_id, "doc_b", Value::String("test document beta".into()))
        .unwrap();
    kv.put(run_id, "doc_c", Value::String("test document gamma".into()))
        .unwrap();
    kv.put(run_id, "doc_d", Value::String("test document delta".into()))
        .unwrap();
    kv.put(
        run_id,
        "doc_e",
        Value::String("test document epsilon".into()),
    )
    .unwrap();
}

// ============================================================================
// Search Determinism Tests
// ============================================================================

/// Same request produces identical results
#[test]
fn test_search_deterministic() {
    let db = test_db();
    let run_id = RunId::new();
    populate_determinism_data(&db, &run_id);

    let hybrid = db.hybrid();
    let req = SearchRequest::new(run_id, "test");

    // Execute same search twice
    let r1 = hybrid.search(&req).unwrap();
    let r2 = hybrid.search(&req).unwrap();

    // Should have same number of hits
    assert_eq!(
        r1.hits.len(),
        r2.hits.len(),
        "Same query should return same number of hits"
    );

    // Hits should be in same order
    for (h1, h2) in r1.hits.iter().zip(r2.hits.iter()) {
        assert_eq!(h1.doc_ref, h2.doc_ref, "DocRefs should be in same order");
        assert_eq!(h1.rank, h2.rank, "Ranks should be identical");
        assert!(
            (h1.score - h2.score).abs() < 0.0001,
            "Scores should be identical"
        );
    }
}

/// Primitive search is deterministic
#[test]
fn test_primitive_search_deterministic() {
    let db = test_db();
    let run_id = RunId::new();
    populate_determinism_data(&db, &run_id);

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
        }
    }
}

/// RRF fusion is deterministic even with equal scores
#[test]
fn test_rrf_fusion_deterministic() {
    let db = test_db();
    let run_id = RunId::new();

    let kv = KVStore::new(db.clone());
    let run_index = RunIndex::new(db.clone());

    // Create run
    run_index.create_run(&run_id.to_string()).unwrap();

    // Add documents that will have equal BM25 scores (same content length)
    kv.put(&run_id, "a", Value::String("test".into())).unwrap();
    kv.put(&run_id, "b", Value::String("test".into())).unwrap();
    kv.put(&run_id, "c", Value::String("test".into())).unwrap();

    let hybrid = HybridSearch::new(db.clone()).with_fuser(Arc::new(RRFFuser::default()));
    let req = SearchRequest::new(run_id, "test").with_primitive_filter(vec![PrimitiveType::Kv]);

    // Execute same search multiple times
    let r1 = hybrid.search(&req).unwrap();
    let r2 = hybrid.search(&req).unwrap();

    // Order should be deterministic even with equal scores
    let order1: Vec<_> = r1.hits.iter().map(|h| &h.doc_ref).collect();
    let order2: Vec<_> = r2.hits.iter().map(|h| &h.doc_ref).collect();

    assert_eq!(order1, order2, "Order should be deterministic with RRF");
}

/// SimpleFuser is deterministic
#[test]
fn test_simple_fuser_deterministic() {
    let db = test_db();
    let run_id = RunId::new();
    populate_determinism_data(&db, &run_id);

    let hybrid = db.hybrid(); // Uses SimpleFuser by default
    let req = SearchRequest::new(run_id, "test").with_primitive_filter(vec![PrimitiveType::Kv]);

    let r1 = hybrid.search(&req).unwrap();
    let r2 = hybrid.search(&req).unwrap();

    let order1: Vec<_> = r1.hits.iter().map(|h| &h.doc_ref).collect();
    let order2: Vec<_> = r2.hits.iter().map(|h| &h.doc_ref).collect();

    assert_eq!(order1, order2, "SimpleFuser should be deterministic");
}

// ============================================================================
// Consistency Tests
// ============================================================================

/// Results are consistent across different k values
#[test]
fn test_consistent_across_k_values() {
    let db = test_db();
    let run_id = RunId::new();
    populate_determinism_data(&db, &run_id);

    let hybrid = db.hybrid();

    // Search with different k values
    let req_k5 = SearchRequest::new(run_id, "test").with_k(5);
    let req_k10 = SearchRequest::new(run_id, "test").with_k(10);
    let req_k3 = SearchRequest::new(run_id, "test").with_k(3);

    let r5 = hybrid.search(&req_k5).unwrap();
    let r10 = hybrid.search(&req_k10).unwrap();
    let r3 = hybrid.search(&req_k3).unwrap();

    // Smaller k results should be prefix of larger k results
    for (i, hit) in r3.hits.iter().enumerate() {
        if i < r5.hits.len() {
            assert_eq!(
                hit.doc_ref, r5.hits[i].doc_ref,
                "Top-3 should be prefix of top-5"
            );
        }
    }

    for (i, hit) in r5.hits.iter().enumerate() {
        if i < r10.hits.len() {
            assert_eq!(
                hit.doc_ref, r10.hits[i].doc_ref,
                "Top-5 should be prefix of top-10"
            );
        }
    }
}

/// Scores are monotonically decreasing
#[test]
fn test_scores_monotonically_decreasing() {
    let db = test_db();
    let run_id = RunId::new();
    populate_determinism_data(&db, &run_id);

    let hybrid = db.hybrid();
    let req = SearchRequest::new(run_id, "test").with_k(20);
    let response = hybrid.search(&req).unwrap();

    if response.hits.len() >= 2 {
        for i in 1..response.hits.len() {
            assert!(
                response.hits[i - 1].score >= response.hits[i].score,
                "Scores should be monotonically decreasing: {} vs {}",
                response.hits[i - 1].score,
                response.hits[i].score
            );
        }
    }
}

/// Ranks are sequential starting from 1
#[test]
fn test_ranks_are_sequential() {
    let db = test_db();
    let run_id = RunId::new();
    populate_determinism_data(&db, &run_id);

    let kv = KVStore::new(db.clone());
    let req = SearchRequest::new(run_id, "test").with_k(10);
    let response = kv.search(&req).unwrap();

    for (i, hit) in response.hits.iter().enumerate() {
        assert_eq!(
            hit.rank as usize,
            i + 1,
            "Ranks should be sequential starting from 1"
        );
    }
}

// ============================================================================
// Run Isolation Tests
// ============================================================================

/// Search results are isolated to requested run
#[test]
fn test_run_isolation() {
    let db = test_db();
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
    for hit in &r1.hits {
        assert_eq!(
            hit.doc_ref.run_id(),
            run1,
            "Run1 results should be from run1"
        );
    }

    for hit in &r2.hits {
        assert_eq!(
            hit.doc_ref.run_id(),
            run2,
            "Run2 results should be from run2"
        );
    }
}

/// Non-existent run returns empty results
#[test]
fn test_nonexistent_run_empty() {
    let db = test_db();
    let run_id = RunId::new();
    // Don't create any data

    let hybrid = db.hybrid();
    let req = SearchRequest::new(run_id, "test");
    let response = hybrid.search(&req).unwrap();

    assert!(
        response.hits.is_empty(),
        "Non-existent run should return empty results"
    );
}
