//! ISSUE-018: Vector Search Over-Fetches with Hardcoded Multiplier
//!
//! **Severity**: MEDIUM
//! **Location**: `/crates/primitives/src/vector/store.rs:829`
//!
//! **Problem**: Hardcoded 3x multiplier for filtering may not be sufficient
//! for selective filters, potentially returning fewer than k results.
//!
//! **Impact**: Search may return fewer results than requested.

use crate::test_utils::*;

/// Test search with selective filter.
#[test]
fn test_search_with_selective_filter() {
    let test_db = TestDb::new();
    let vector = test_db.vector();
    let run_id = test_db.run_id;

    vector.create_collection(run_id, "filter_test", config_small()).expect("create");

    // Insert 100 vectors with metadata
    for i in 0..100 {
        let metadata = serde_json::json!({"category": i % 10});
        vector.insert(run_id, "filter_test", &format!("v_{}", i), &seeded_vector(3, i as u64), Some(metadata))
            .expect("insert");
    }

    // Search with filter that matches only 10% of vectors
    // With 3x multiplier and k=10, we need to scan at least 30 vectors
    // to reliably get 10 matches when filter matches 10%

    // When ISSUE-018 is fixed:
    // - Adaptive multiplier should increase for selective filters
    // - Or document that fewer results may be returned
}

/// Test search returns requested k results.
#[test]
fn test_search_returns_k_results() {
    let test_db = TestDb::new();
    let vector = test_db.vector();
    let run_id = test_db.run_id;

    vector.create_collection(run_id, "k_test", config_small()).expect("create");

    for i in 0..50 {
        vector.insert(run_id, "k_test", &format!("v_{}", i), &seeded_vector(3, i as u64), None)
            .expect("insert");
    }

    let query = seeded_vector(3, 42);

    // Without filter, should always return k results (if k <= count)
    let results = vector.search(run_id, "k_test", &query, 10, None).expect("search");
    assert_eq!(results.len(), 10, "Should return exactly k results");

    let results = vector.search(run_id, "k_test", &query, 100, None).expect("search");
    assert_eq!(results.len(), 50, "Should return all vectors when k > count");
}
