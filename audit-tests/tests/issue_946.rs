//! Audit test for issue #946: Branch delete doesn't clean up search index or vector backends
//! Verdict: ARCHITECTURAL CHOICE (questionable)
//!
//! When a branch is deleted via `BranchDelete`, the branch metadata and KV data are
//! removed, but:
//!
//! 1. Vector backend state (in-memory VectorHeap/HNSW indexes) is NOT cleaned up
//! 2. Search index entries are NOT cleaned up
//! 3. The commit_lock entry in TransactionManager is NOT cleaned up (see #944)
//!
//! The vector backend state is stored in `Database::extension::<VectorBackendState>()`,
//! keyed by `CollectionId(BranchId, collection_name)`. When a branch is deleted, the
//! backend entries for that branch's collections remain in the BTreeMap.
//!
//! Impact:
//! - Memory leak: in-memory vector indexes for deleted branches persist
//! - Stale data: if a new branch is created with the same ID (unlikely with UUIDs),
//!   it could see ghost data from the old branch's vector collections
//! - The search index may return results from deleted branches
//!
//! The fix would require the branch deletion handler to:
//! 1. Enumerate all vector collections for the branch
//! 2. Remove their in-memory backends
//! 3. Remove search index entries

use strata_core::value::Value;
use strata_engine::database::Database;
use strata_executor::{BranchId, Command, DistanceMetric, Executor, Output};

/// Demonstrates that deleting a branch does not clean up vector data.
///
/// After creating a branch with vector collections, deleting the branch,
/// and then trying to access vector data, the behavior is inconsistent:
/// the branch is gone but vector backend state may linger.
#[test]
fn issue_946_branch_delete_leaves_vector_data() {
    let db = Database::cache().unwrap();
    let executor = Executor::new(db);

    // Create a branch
    let branch_name = "vector_branch";
    let create_result = executor
        .execute(Command::BranchCreate {
            branch_id: Some(branch_name.to_string()),
            metadata: None,
        })
        .unwrap();

    match create_result {
        Output::BranchWithVersion { .. } => {}
        other => panic!("Expected BranchWithVersion, got {:?}", other),
    }

    let branch = BranchId::from(branch_name);

    // Create a vector collection on this branch
    executor
        .execute(Command::VectorCreateCollection {
            branch: Some(branch.clone()),
            collection: "embeddings".into(),
            dimension: 3,
            metric: DistanceMetric::Cosine,
        })
        .unwrap();

    // Insert a vector
    executor
        .execute(Command::VectorUpsert {
            branch: Some(branch.clone()),
            collection: "embeddings".into(),
            key: "doc1".into(),
            vector: vec![1.0, 0.0, 0.0],
            metadata: Some(Value::String("test doc".into())),
        })
        .unwrap();

    // Verify vector exists before deletion
    let get_before = executor
        .execute(Command::VectorGet {
            branch: Some(branch.clone()),
            collection: "embeddings".into(),
            key: "doc1".into(),
        })
        .unwrap();

    match &get_before {
        Output::VectorData(Some(_)) => {} // Vector exists
        other => panic!("Expected VectorData(Some), got {:?}", other),
    }

    // Delete the branch
    executor
        .execute(Command::BranchDelete {
            branch: branch.clone(),
        })
        .unwrap();

    // Verify branch is deleted
    let exists_result = executor
        .execute(Command::BranchExists {
            branch: branch.clone(),
        })
        .unwrap();

    match exists_result {
        Output::Bool(false) => {} // Branch is deleted
        Output::Bool(true) => panic!("Branch should be deleted"),
        other => panic!("Expected Bool, got {:?}", other),
    }

    // BUG: The in-memory vector backend state for this branch's collections
    // is NOT cleaned up. The VectorBackendState BTreeMap still has entries
    // for CollectionId(branch_id, "embeddings").
    //
    // We cannot directly verify this from outside the module, but the
    // architectural issue is clear from the code: BranchDelete handler
    // calls p.branch.delete_branch() which removes the branch metadata
    // and KV data, but does NOT iterate over vector collections to clean
    // up their in-memory backends.
    //
    // Attempting to access vector data on the deleted branch will fail
    // because the branch no longer exists, but the in-memory index
    // memory is never reclaimed.
}
