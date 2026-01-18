//! ISSUE-016: Vector Budget Enforcement Not Integrated
//!
//! **Severity**: MEDIUM
//! **Location**: `/crates/primitives/src/vector/store.rs:791-910`
//!
//! **Problem**: Vector search methods don't take `SearchBudget` parameter.
//! Vector search doesn't respect time/candidate limits.
//!
//! **Spec Requirement**: M6 budget model should apply to all search.
//!
//! **Impact**: Vector search can run unbounded.

use crate::test_utils::*;

/// Test vector search respects budget.
#[test]
fn test_vector_search_respects_budget() {
    let test_db = TestDb::new();
    let vector = test_db.vector();
    let run_id = test_db.run_id;

    // Create collection with many vectors
    vector.create_collection(run_id, "budget_test", config_small()).expect("create");

    for i in 0..100 {
        vector.insert(run_id, "budget_test", &format!("v_{}", i), &seeded_vector(3, i as u64), None)
            .expect("insert");
    }

    // When ISSUE-016 is fixed:
    // - Vector search should accept SearchBudget
    // - Search should terminate within budget.max_time_ms
    // - Search should respect budget.max_candidates

    // For now, verify search returns results
    let query = seeded_vector(3, 42);
    let results = vector.search(run_id, "budget_test", &query, 10, None).expect("search");
    assert!(!results.is_empty());
}

/// Test budget propagation from SearchRequest.
#[test]
fn test_budget_from_search_request() {
    // When ISSUE-016 is fixed:
    // - SearchRequest.budget should be passed to vector search
    // - Vector search should respect the budget
}
