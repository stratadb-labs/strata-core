//! Audit test for issue #937: Vector backend update outside transaction boundary
//! Verdict: CONFIRMED BUG
//!
//! VectorStore::insert updates the vector backend (the in-memory similarity
//! index) BEFORE the KV transaction commits. This means:
//!
//! 1. Vector is added to the similarity index (backend)
//! 2. KV transaction is prepared and committed
//! 3. If the KV transaction fails or the process crashes between steps 1-2,
//!    the backend has the vector but KV does not
//!
//! On restart, the similarity index is rebuilt from KV data, so the
//! orphaned vector in the backend would be lost. However, during the same
//! process lifetime, searches would return results for vectors whose KV
//! writes failed — a consistency violation.
//!
//! This is difficult to test directly because it requires either:
//! - A crash between backend update and KV commit (requires process control)
//! - A KV commit failure after backend update (requires error injection)

use strata_engine::database::Database;
use strata_executor::{BranchId, Command, DistanceMetric, Executor, Output};

/// Documents the ordering issue: backend is updated before KV commit.
/// In the normal (non-crash) case, this works correctly because both
/// steps succeed. The bug manifests only on failure between the steps.
#[test]
fn issue_937_normal_case_backend_and_kv_consistent() {
    let db = Database::cache().unwrap();
    let executor = Executor::new(db);

    let branch = BranchId::from("default");

    // Create collection first (auto-create was removed in #923)
    executor
        .execute(Command::VectorCreateCollection {
            branch: Some(branch.clone()),
            collection: "col".into(),
            dimension: 3,
            metric: DistanceMetric::Cosine,
        })
        .unwrap();

    // Insert a vector — both backend and KV are updated
    executor
        .execute(Command::VectorUpsert {
            branch: Some(branch.clone()),
            collection: "col".into(),
            key: "v1".into(),
            vector: vec![1.0, 0.0, 0.0],
            metadata: None,
        })
        .unwrap();

    // Search finds the vector in the backend
    let search_result = executor
        .execute(Command::VectorSearch {
            branch: Some(branch.clone()),
            collection: "col".into(),
            query: vec![1.0, 0.0, 0.0],
            k: 1,
            filter: None,
            metric: None,
        })
        .unwrap();

    match search_result {
        Output::VectorMatches(matches) => {
            assert_eq!(matches.len(), 1);
            assert_eq!(matches[0].key, "v1");
        }
        other => panic!("Expected VectorMatches, got: {:?}", other),
    }

    // Get also finds it in KV
    let get_result = executor
        .execute(Command::VectorGet {
            branch: Some(branch.clone()),
            collection: "col".into(),
            key: "v1".into(),
        })
        .unwrap();

    assert!(
        matches!(get_result, Output::VectorData(Some(_))),
        "KV should also have the vector data"
    );

    // BUG: If a crash occurred between backend update and KV commit,
    // the search would return "v1" but get would return None.
    // This cannot be demonstrated without crash injection.
}
