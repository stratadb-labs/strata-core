//! Audit test for issue #944: commit_locks in ConcurrencyManager never removed on branch delete
//! Verdict: ARCHITECTURAL CHOICE (memory leak)
//!
//! In concurrency/src/manager.rs, the `TransactionManager` uses a `DashMap<BranchId, Mutex<()>>`
//! for per-branch commit locks (line 83):
//!
//! ```ignore
//! commit_locks: DashMap<BranchId, Mutex<()>>,
//! ```
//!
//! When a branch is deleted, the corresponding entry in `commit_locks` is never removed.
//! The lock is created lazily in `commit()` via `entry(...).or_insert_with(...)` but there
//! is no cleanup path.
//!
//! This is a bounded memory leak:
//! - Each entry is a BranchId (16 bytes UUID) + Mutex<()> (small fixed size)
//! - In practice, the number of branches is typically small
//! - But in workloads that create and delete many branches (e.g., per-request isolation),
//!   the DashMap grows without bound
//!
//! The fix would be to add a `remove_branch_lock(&self, branch_id: BranchId)` method
//! and call it during branch deletion. Care must be taken to ensure no transaction
//! is in-flight for that branch when the lock is removed.
//!
//! This test cannot directly verify the internal DashMap size because the
//! `commit_locks` field is private. We document the issue here for reference.

/// Documents that commit_locks in TransactionManager are never cleaned up.
///
/// This is a documentation-only test. The commit_locks DashMap is private,
/// so we cannot directly observe the memory leak from outside the module.
///
/// The bug exists in concurrency/src/manager.rs:
/// - `commit_locks: DashMap<BranchId, Mutex<()>>` (line 83)
/// - Created via `self.commit_locks.entry(txn.branch_id).or_insert_with(...)` in `commit()`
/// - Never removed, even when branches are deleted
///
/// Theoretical impact: In a workload that creates and deletes thousands of branches,
/// the DashMap will accumulate stale entries indefinitely.
#[test]
fn issue_944_commit_locks_never_removed_on_branch_delete() {
    // This test documents a confirmed architectural issue.
    //
    // The TransactionManager's commit_locks DashMap grows monotonically:
    // - New entry created for each unique BranchId that commits a transaction
    // - No entry is ever removed, even when the branch is deleted
    //
    // Since commit_locks is private, we cannot directly test the leak.
    // A proper fix would add a cleanup method:
    //
    //   pub fn remove_branch_lock(&self, branch_id: BranchId) {
    //       self.commit_locks.remove(&branch_id);
    //   }
    //
    // This method would be called by the branch deletion handler after
    // ensuring no in-flight transactions exist for that branch.

    // Demonstrate the lifecycle that causes the leak:
    use strata_engine::database::Database;
    use strata_executor::BranchId;
    use strata_executor::{Command, Executor, Output};

    let db = Database::cache().unwrap();
    let executor = Executor::new(db);

    // Create and delete multiple branches
    for i in 0..10 {
        let branch_name = format!("temp_branch_{}", i);

        // Create branch
        let create_result = executor.execute(Command::BranchCreate {
            branch_id: Some(branch_name.clone()),
            metadata: None,
        });

        if let Ok(Output::BranchWithVersion { .. }) = create_result {
            // Write some data to force a commit (which creates a commit_lock entry)
            let branch = BranchId::from(branch_name.as_str());
            let _ = executor.execute(Command::KvPut {
                branch: Some(branch.clone()),
                key: "test".into(),
                value: strata_core::value::Value::Int(i),
            });

            // Delete the branch
            let _ = executor.execute(Command::BranchDelete { branch });

            // BUG: The commit_lock entry for this branch remains in the DashMap
            // even though the branch no longer exists.
        }
    }

    // After this loop, the commit_locks DashMap has up to 10 stale entries
    // (one for each deleted branch that had a committed transaction).
    // There is no way to verify this from outside the module.
}
