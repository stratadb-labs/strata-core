//! Hybrid Search Orchestration Tests
//!
//! Tests HybridSearch across multiple primitives.

use crate::test_utils::*;
use strata_core::json::JsonValue;
use strata_core::types::JsonDocId;

/// Test hybrid search orchestrates multiple primitives.
#[test]
fn test_hybrid_orchestrates_primitives() {
    let test_db = TestDb::new();
    let run_id = test_db.run_id;
    let p = test_db.all_primitives();

    // Populate primitives with related content
    p.kv.put(&run_id, "topic1", strata_core::value::Value::String("machine learning algorithm".into()))
        .expect("kv");
    let doc_id = JsonDocId::new();
    p.json.create(&run_id, &doc_id, JsonValue::from(serde_json::json!({
        "title": "Introduction to Machine Learning",
        "content": "ML algorithms are powerful"
    }))).expect("json");

    // HybridSearch should combine results from all primitives
    // When ISSUE-001 and ISSUE-013 are fixed:
    // - Vector results should be included
    // - RRF fusion should combine all results
}

/// Test RRF fusion with multiple result sets.
#[test]
fn test_rrf_fusion_multiple_sources() {
    // RRF (Reciprocal Rank Fusion) should:
    // 1. Collect results from each primitive
    // 2. Normalize scores
    // 3. Apply RRF formula
    // 4. Return unified ranked list
}
