//! Test utilities for M6 comprehensive tests

use strata_core::search_types::{PrimitiveType, SearchRequest, SearchResponse};
use strata_core::types::RunId;
use strata_core::value::Value;
use strata_engine::Database;
use strata_primitives::{KVStore, RunIndex};
use strata_search::DatabaseSearchExt;
use std::sync::Arc;

/// Create a test database with in-memory storage
pub fn create_test_db() -> Arc<Database> {
    Arc::new(
        Database::builder()
            .in_memory()
            .open_temp()
            .expect("Failed to create test database"),
    )
}

/// Create a unique test run ID
pub fn test_run_id() -> RunId {
    RunId::new()
}

/// Populate basic test data across primitives
pub fn populate_test_data(db: &Arc<Database>, run_id: &RunId) {
    let kv = KVStore::new(db.clone());
    let run_index = RunIndex::new(db.clone());

    // Create run in run_index
    run_index.create_run(&run_id.to_string()).unwrap();

    // KV data
    kv.put(run_id, "hello", Value::String("world test data".into()))
        .unwrap();
    kv.put(
        run_id,
        "test_key",
        Value::String("this is test content".into()),
    )
    .unwrap();
    kv.put(run_id, "another", Value::String("more test values".into()))
        .unwrap();
}

/// Populate larger dataset for stress testing
pub fn populate_large_dataset(db: &Arc<Database>, run_id: &RunId, count: usize) {
    let kv = KVStore::new(db.clone());
    let run_index = RunIndex::new(db.clone());

    run_index.create_run(&run_id.to_string()).unwrap();

    for i in 0..count {
        kv.put(
            run_id,
            &format!("key_{}", i),
            Value::String(format!("value with searchable term {}", i)),
        )
        .unwrap();
    }
}

/// Assert that all hits are from a specific primitive
pub fn assert_all_from_primitive(response: &SearchResponse, kind: PrimitiveType) {
    for hit in &response.hits {
        assert_eq!(
            hit.doc_ref.primitive_type(),
            kind,
            "Expected all hits from {:?}, found {:?}",
            kind,
            hit.doc_ref.primitive_type()
        );
    }
}

/// Assert that all hits belong to a specific run
pub fn assert_all_from_run(response: &SearchResponse, run_id: RunId) {
    for hit in &response.hits {
        assert_eq!(
            hit.doc_ref.run_id(),
            run_id,
            "Expected all hits from run {:?}",
            run_id
        );
    }
}

/// Verify search results are deterministic
pub fn verify_deterministic(db: &Arc<Database>, req: &SearchRequest) {
    let hybrid = db.hybrid();
    let r1 = hybrid.search(req).unwrap();
    let r2 = hybrid.search(req).unwrap();

    assert_eq!(r1.hits.len(), r2.hits.len());
    for (h1, h2) in r1.hits.iter().zip(r2.hits.iter()) {
        assert_eq!(h1.doc_ref, h2.doc_ref);
        assert_eq!(h1.rank, h2.rank);
        assert!((h1.score - h2.score).abs() < 0.0001);
    }
}

/// Verify scores are monotonically decreasing
pub fn verify_scores_decreasing(response: &SearchResponse) {
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

/// Verify ranks are sequential starting from 1
pub fn verify_ranks_sequential(response: &SearchResponse) {
    for (i, hit) in response.hits.iter().enumerate() {
        assert_eq!(
            hit.rank as usize,
            i + 1,
            "Ranks should be sequential starting from 1"
        );
    }
}
