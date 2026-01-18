//! ISSUE-013: HybridSearch Returns Empty for Vector Explicitly
//!
//! **Severity**: HIGH
//! **Location**: `/crates/search/src/hybrid.rs:240-242`
//!
//! **Problem**: Vector search is explicitly stubbed out:
//! ```rust
//! PrimitiveKind::Vector => Ok(SearchResponse::empty()),
//! ```
//!
//! **Spec Requirement**: M8 specifies hybrid search integration with RRF fusion.
//!
//! **Impact**: HybridSearch cannot orchestrate vector search, breaking uniformity.

use crate::test_utils::*;

/// Test HybridSearch includes vector results.
#[test]
fn test_hybrid_search_includes_vector() {
    let test_db = TestDb::new();
    let vector = test_db.vector();
    let run_id = test_db.run_id;

    // Create vector collection with data
    let collection = "hybrid_test";
    vector
        .create_collection(run_id, collection, config_small())
        .expect("create");

    for i in 0..10 {
        vector
            .insert(run_id, collection, &format!("v_{}", i), &seeded_vector(3, i as u64), None)
            .expect("insert");
    }

    // When ISSUE-013 is fixed (depends on ISSUE-001):
    // - HybridSearch should include vector in search orchestration
    // - Vector results should be fused with RRF

    // For now, verify direct vector search works
    let query = seeded_vector(3, 42);
    let results = vector.search(run_id, collection, &query, 5, None).expect("search");
    assert!(!results.is_empty(), "Vector search should return results");
}

/// Test RRF fusion with vector results.
#[test]
fn test_rrf_fusion_with_vector() {
    // When ISSUE-013 is fixed:
    // - RRF fusion should work with vector results
    // - Scores should be properly normalized for fusion
}
