//! M6 Search API Contract Tests
//!
//! Validates all search API contracts across primitives.
//! Part of Epic 39: Validation & Non-Regression (Story #334)
//!
//! See `docs/architecture/M6_ARCHITECTURE.md` for authoritative specification.

use in_mem_core::search_types::{PrimitiveKind, SearchRequest, SearchResponse};
use in_mem_core::types::RunId;
use in_mem_core::value::Value;
use in_mem_engine::Database;
use in_mem_primitives::{KVStore, RunIndex};
use in_mem_search::{DatabaseSearchExt, HybridSearch, RRFFuser};
use std::collections::HashSet;
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

fn populate_test_data(db: &Arc<Database>, run_id: &RunId) {
    let kv = KVStore::new(db.clone());
    let run_index = RunIndex::new(db.clone());

    // Create run in run_index
    run_index.create_run(&run_id.to_string()).unwrap();

    // KV data - this is the primary test data
    kv.put(run_id, "hello", Value::String("world test data".into()))
        .unwrap();
    kv.put(run_id, "test_key", Value::String("this is test content".into()))
        .unwrap();
    kv.put(run_id, "another", Value::String("more test values".into()))
        .unwrap();

    // Note: Other primitives (Json, Event, State, Trace) have more complex APIs
    // and are tested individually in their respective test modules
}

// ============================================================================
// Primitive Search Contract Tests
// ============================================================================

/// KV primitive search returns a valid SearchResponse
#[test]
fn test_kv_primitive_returns_search_response() {
    let db = test_db();
    let run_id = RunId::new();
    populate_test_data(&db, &run_id);

    let kv = KVStore::new(db.clone());
    let req = SearchRequest::new(run_id, "test");

    // KV search
    let kv_response: SearchResponse = kv.search(&req).expect("KV search should succeed");
    assert!(
        !kv_response.hits.is_empty(),
        "KV should have test data matches"
    );

    // Response should have valid structure
    let _ = kv_response.truncated;
    let _ = kv_response.stats.elapsed_micros;
    let _ = kv_response.stats.candidates_considered;
}

/// RunIndex primitive search returns a valid SearchResponse
#[test]
fn test_run_index_primitive_returns_search_response() {
    let db = test_db();
    let run_id = RunId::new();
    populate_test_data(&db, &run_id);

    let run_index = RunIndex::new(db.clone());
    let req = SearchRequest::new(run_id, "test");

    // Run search (may or may not have matches depending on run name)
    let run_response: SearchResponse = run_index
        .search(&req)
        .expect("RunIndex search should succeed");

    // Response should have valid structure
    let _ = run_response.truncated;
    let _ = run_response.is_empty();
}

/// DocRef from each primitive correctly reports its primitive kind
#[test]
fn test_docref_primitive_kind_matches() {
    let db = test_db();
    let run_id = RunId::new();
    populate_test_data(&db, &run_id);

    let kv = KVStore::new(db.clone());
    let req = SearchRequest::new(run_id, "test");

    let response = kv.search(&req).unwrap();

    for hit in &response.hits {
        assert_eq!(
            hit.doc_ref.primitive_kind(),
            PrimitiveKind::Kv,
            "DocRef from KV search should report Kv primitive kind"
        );
        assert_eq!(
            hit.doc_ref.run_id(),
            run_id,
            "DocRef should contain correct run_id"
        );
    }
}

/// Search respects run_id filter - results only from requested run
#[test]
fn test_search_respects_run_id() {
    let db = test_db();
    let run1 = RunId::new();
    let run2 = RunId::new();

    let kv = KVStore::new(db.clone());
    let run_index = RunIndex::new(db.clone());

    // Create runs
    run_index.create_run(&run1.to_string()).unwrap();
    run_index.create_run(&run2.to_string()).unwrap();

    // Add shared term to both runs
    kv.put(
        &run1,
        "key1",
        Value::String("shared test term".to_string()),
    )
    .unwrap();
    kv.put(
        &run2,
        "key2",
        Value::String("shared test term".to_string()),
    )
    .unwrap();

    // Search run1 only
    let req = SearchRequest::new(run1, "shared");
    let response = kv.search(&req).unwrap();

    // All results should belong to run1
    for hit in &response.hits {
        assert_eq!(
            hit.doc_ref.run_id(),
            run1,
            "All hits should belong to the requested run"
        );
    }
}

// ============================================================================
// Composite Search (Hybrid) Contract Tests
// ============================================================================

/// db.hybrid() returns a HybridSearch orchestrator
#[test]
fn test_database_search_ext() {
    let db = test_db();
    let hybrid = db.hybrid();

    // Should be able to use the hybrid search
    let run_id = RunId::new();
    let req = SearchRequest::new(run_id, "test");
    let response = hybrid.search(&req).unwrap();
    assert!(response.hits.is_empty()); // No data yet
}

/// Composite search orchestrates across multiple primitives
#[test]
fn test_hybrid_search_orchestrates() {
    let db = test_db();
    let run_id = RunId::new();
    populate_test_data(&db, &run_id);

    let hybrid = db.hybrid();
    let req = SearchRequest::new(run_id, "test");
    let response = hybrid.search(&req).unwrap();

    // Should have results from at least KV
    assert!(
        !response.hits.is_empty(),
        "Hybrid search should find matches"
    );

    // Check that results come from multiple primitives (if data exists)
    let primitives: HashSet<_> = response
        .hits
        .iter()
        .map(|h| h.doc_ref.primitive_kind())
        .collect();

    // At minimum, KV should be represented
    assert!(
        primitives.contains(&PrimitiveKind::Kv),
        "Hybrid search should include KV results"
    );
}

/// Primitive filter limits search scope
#[test]
fn test_hybrid_search_respects_filter() {
    let db = test_db();
    let run_id = RunId::new();
    populate_test_data(&db, &run_id);

    let hybrid = db.hybrid();

    // Search only KV primitive
    let req = SearchRequest::new(run_id, "test").with_primitive_filter(vec![PrimitiveKind::Kv]);
    let response = hybrid.search(&req).unwrap();

    // All results should be from KV only
    for hit in &response.hits {
        assert_eq!(
            hit.doc_ref.primitive_kind(),
            PrimitiveKind::Kv,
            "Results should only come from filtered primitives"
        );
    }
}

/// Empty primitive filter means no results
#[test]
fn test_hybrid_search_empty_filter() {
    let db = test_db();
    let run_id = RunId::new();
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

/// Custom fuser can be set
#[test]
fn test_hybrid_search_custom_fuser() {
    let db = test_db();
    let run_id = RunId::new();
    populate_test_data(&db, &run_id);

    // Use RRF fuser instead of simple fuser
    let hybrid = HybridSearch::new(db.clone()).with_fuser(Arc::new(RRFFuser::default()));

    let req = SearchRequest::new(run_id, "test");
    let response = hybrid.search(&req).unwrap();

    // Should still return valid results
    assert!(
        !response.hits.is_empty(),
        "RRF fuser should produce results"
    );
}

// ============================================================================
// SearchRequest Contract Tests
// ============================================================================

/// SearchRequest builder works correctly
#[test]
fn test_search_request_builder() {
    let run_id = RunId::new();

    let req = SearchRequest::new(run_id, "query text")
        .with_k(20)
        .with_primitive_filter(vec![PrimitiveKind::Kv, PrimitiveKind::Json]);

    assert_eq!(req.run_id, run_id);
    assert_eq!(req.query, "query text");
    assert_eq!(req.k, 20);
    assert_eq!(
        req.primitive_filter,
        Some(vec![PrimitiveKind::Kv, PrimitiveKind::Json])
    );
}

/// includes_primitive() respects filter
#[test]
fn test_includes_primitive() {
    let run_id = RunId::new();

    // No filter - includes all
    let req1 = SearchRequest::new(run_id, "test");
    for kind in PrimitiveKind::all() {
        assert!(
            req1.includes_primitive(*kind),
            "No filter should include all primitives"
        );
    }

    // With filter
    let req2 =
        SearchRequest::new(run_id, "test").with_primitive_filter(vec![PrimitiveKind::Kv]);
    assert!(req2.includes_primitive(PrimitiveKind::Kv));
    assert!(!req2.includes_primitive(PrimitiveKind::Json));
    assert!(!req2.includes_primitive(PrimitiveKind::Event));
}

// ============================================================================
// SearchResponse Contract Tests
// ============================================================================

/// SearchResponse has expected structure
#[test]
fn test_search_response_structure() {
    let db = test_db();
    let run_id = RunId::new();
    populate_test_data(&db, &run_id);

    let hybrid = db.hybrid();
    let req = SearchRequest::new(run_id, "test");
    let response = hybrid.search(&req).unwrap();

    // Response should have hits, truncated flag, and stats
    let _hits = &response.hits;
    let _truncated = response.truncated;
    let _stats = &response.stats;

    // Stats should have expected fields
    let _elapsed = response.stats.elapsed_micros;
    let _candidates = response.stats.candidates_considered;
}

/// Hits are ranked by score (descending)
#[test]
fn test_hits_are_ranked() {
    let db = test_db();
    let run_id = RunId::new();
    populate_test_data(&db, &run_id);

    let kv = KVStore::new(db.clone());
    let req = SearchRequest::new(run_id, "test");
    let response = kv.search(&req).unwrap();

    if response.hits.len() >= 2 {
        for i in 1..response.hits.len() {
            assert!(
                response.hits[i - 1].score >= response.hits[i].score,
                "Hits should be sorted by score descending"
            );
        }
    }
}

// ============================================================================
// PrimitiveKind Contract Tests
// ============================================================================

/// PrimitiveKind::all() returns all 6 primitives
#[test]
fn test_primitive_kind_all() {
    let all = PrimitiveKind::all();
    assert_eq!(all.len(), 6, "Should have exactly 6 primitives");

    assert!(all.contains(&PrimitiveKind::Kv));
    assert!(all.contains(&PrimitiveKind::Json));
    assert!(all.contains(&PrimitiveKind::Event));
    assert!(all.contains(&PrimitiveKind::State));
    assert!(all.contains(&PrimitiveKind::Trace));
    assert!(all.contains(&PrimitiveKind::Run));
}

/// PrimitiveKind has correct display strings
#[test]
fn test_primitive_kind_display() {
    assert_eq!(format!("{}", PrimitiveKind::Kv), "kv");
    assert_eq!(format!("{}", PrimitiveKind::Json), "json");
    assert_eq!(format!("{}", PrimitiveKind::Event), "event");
    assert_eq!(format!("{}", PrimitiveKind::State), "state");
    assert_eq!(format!("{}", PrimitiveKind::Trace), "trace");
    assert_eq!(format!("{}", PrimitiveKind::Run), "run");
}
