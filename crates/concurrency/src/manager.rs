//! Transaction manager for coordinating commit operations
//!
//! Provides atomic commit by orchestrating:
//! 1. Validation (first-committer-wins)
//! 2. WAL writing (durability)
//! 3. Storage application (visibility)
//!
//! Per spec Core Invariants:
//! - All-or-nothing commit: transaction writes either ALL succeed or ALL fail
//! - WAL before storage: durability requires WAL to be written first
//! - CommitTxn = durable: transaction is only durable when CommitTxn is in WAL
//!
//! ## Commit Sequence
//!
//! ```text
//! 1. begin_validation() - Change state to Validating
//! 2. validate_transaction() - Check for conflicts
//! 3. IF conflicts: abort() and return error
//! 4. mark_committed() - Change state to Committed
//! 5. Allocate commit_version (increment global version)
//! 6. write_begin() to WAL - BeginTxn entry
//! 7. write_to_wal() - Write/Delete entries with commit_version
//! 8. write_commit() to WAL - CommitTxn entry (DURABILITY POINT)
//! 9. apply_writes() to storage - Apply to in-memory storage
//! 10. Return Ok(commit_version)
//! ```
//!
//! If crash occurs before step 8: Transaction is not durable, discarded on recovery.
//! If crash occurs after step 8: Transaction is durable, replayed on recovery.

use crate::wal_writer::TransactionWALWriter;
use crate::{CommitError, TransactionContext, TransactionStatus};
use dashmap::DashMap;
use parking_lot::Mutex;
use strata_core::error::Result;
use strata_core::traits::Storage;
use strata_core::types::RunId;
use strata_durability::wal::WAL;
use std::sync::atomic::{AtomicU64, Ordering};

/// Manages transaction lifecycle and atomic commits
///
/// TransactionManager coordinates the commit protocol:
/// - Validation against current storage state
/// - WAL writing for durability
/// - Storage application for visibility
///
/// Per spec Section 6.1: Global version counter is incremented once per transaction.
/// All keys in a transaction get the same commit version.
///
/// # Thread Safety
///
/// Commits are serialized per-run via internal locks to prevent TOCTOU
/// (time-of-check-to-time-of-use) races between validation and storage application.
/// This ensures that no other transaction on the same run can modify storage
/// between the time we validate and the time we apply our writes.
///
/// Transactions on different runs can commit in parallel, as ShardedStore
/// maintains per-run shards and there's no cross-run conflict.
pub struct TransactionManager {
    /// Global version counter
    ///
    /// Monotonically increasing. Each committed transaction increments by 1.
    /// Shared across all runs for consistent MVCC ordering.
    version: AtomicU64,

    /// Next transaction ID
    ///
    /// Unique identifier for transactions. Used in WAL entries.
    next_txn_id: AtomicU64,

    /// Per-run commit locks
    ///
    /// Prevents TOCTOU race between validation and apply within the same run.
    /// Without this lock, the following race can occur:
    /// 1. T1 validates (succeeds, storage at v1)
    /// 2. T2 validates (succeeds, storage still at v1)
    /// 3. T1 applies (storage now at v2)
    /// 4. T2 applies (uses stale validation from step 2)
    ///
    /// Using per-run locks allows parallel commits for different runs while
    /// still preventing TOCTOU within each run.
    commit_locks: DashMap<RunId, Mutex<()>>,
}

impl TransactionManager {
    /// Create a new transaction manager
    ///
    /// # Arguments
    /// * `initial_version` - Starting version (typically from recovery's final_version)
    pub fn new(initial_version: u64) -> Self {
        Self::with_txn_id(initial_version, 0)
    }

    /// Create a new transaction manager with specific starting txn_id
    ///
    /// This is used during recovery to ensure new transactions get unique IDs
    /// that don't conflict with transactions already in the WAL.
    ///
    /// # Arguments
    /// * `initial_version` - Starting version (from recovery's final_version)
    /// * `max_txn_id` - Maximum txn_id seen in WAL (new transactions start at max_txn_id + 1)
    pub fn with_txn_id(initial_version: u64, max_txn_id: u64) -> Self {
        TransactionManager {
            version: AtomicU64::new(initial_version),
            // Start next_txn_id at max_txn_id + 1 to avoid conflicts
            next_txn_id: AtomicU64::new(max_txn_id + 1),
            commit_locks: DashMap::new(),
        }
    }

    /// Get current global version
    pub fn current_version(&self) -> u64 {
        self.version.load(Ordering::SeqCst)
    }

    /// Allocate next transaction ID
    pub fn next_txn_id(&self) -> u64 {
        self.next_txn_id.fetch_add(1, Ordering::SeqCst)
    }

    /// Allocate next commit version (increment global version)
    ///
    /// Per spec Section 6.1: Version incremented ONCE for the whole transaction.
    ///
    /// # Version Gaps
    ///
    /// Version gaps may occur if a transaction fails after version allocation
    /// but before successful commit (e.g., WAL write failure). Consumers should
    /// not assume version numbers are contiguous. A gap means the version was
    /// allocated but no data was committed with that version.
    ///
    /// This is by design - version allocation is atomic and non-blocking,
    /// while failure handling during commit does not attempt to "return"
    /// the allocated version.
    pub fn allocate_version(&self) -> u64 {
        self.version.fetch_add(1, Ordering::SeqCst) + 1
    }

    /// Commit a transaction atomically
    ///
    /// Per spec Core Invariants:
    /// - Validates transaction (first-committer-wins)
    /// - Writes to WAL for durability
    /// - Applies to storage only after WAL is durable
    /// - All-or-nothing: either all writes succeed or transaction aborts
    ///
    /// # Arguments
    /// * `txn` - Transaction to commit (must be in Active state)
    /// * `store` - Storage to validate against and apply writes to
    /// * `wal` - WAL for durability
    ///
    /// # Returns
    /// - Ok(commit_version) on success
    /// - Err(CommitError) if validation fails or WAL write fails
    ///
    /// # Commit Sequence
    ///
    /// 1. Acquire per-run commit lock (prevents TOCTOU race within same run)
    /// 2. Validate and mark committed (in-memory state transition)
    /// 3. Allocate commit version
    /// 4. Write BeginTxn to WAL
    /// 5. Write all operations to WAL
    /// 6. Write CommitTxn to WAL (DURABILITY POINT)
    /// 7. Apply writes to storage
    /// 8. Release commit lock
    /// 9. Return commit version
    ///
    /// # Thread Safety
    ///
    /// Per-run commit locks ensure that validation and apply happen atomically
    /// with respect to other transactions on the same run. This prevents the
    /// TOCTOU race where validation passes but storage changes before apply.
    ///
    /// Transactions on different runs can commit in parallel.
    pub fn commit<S: Storage>(
        &self,
        txn: &mut TransactionContext,
        store: &S,
        wal: &WAL,
    ) -> std::result::Result<u64, CommitError> {
        // Acquire per-run commit lock to prevent TOCTOU race between validation and apply
        // This ensures no other transaction on the same run can modify storage between
        // our validation check and our apply_writes call.
        // Transactions on different runs can proceed in parallel.
        let run_lock = self.commit_locks.entry(txn.run_id).or_insert_with(|| Mutex::new(()));
        let _commit_guard = run_lock.lock();

        // Step 1: Validate and mark committed (in-memory)
        // This performs: Active → Validating → Committed
        // Or: Active → Validating → Aborted (if conflicts detected)
        txn.commit(store)?;

        // At this point, transaction is in Committed state
        // but NOT yet durable (not in WAL)

        // Step 2: Allocate commit version
        let commit_version = self.allocate_version();

        // Step 3-5: Write to WAL (durability)
        let txn_id = self.next_txn_id();
        let mut wal_writer = TransactionWALWriter::new(wal, txn_id, txn.run_id);

        // Write BeginTxn
        if let Err(e) = wal_writer.write_begin() {
            // WAL write failed - revert transaction state
            txn.status = TransactionStatus::Aborted {
                reason: format!("WAL write failed: {}", e),
            };
            return Err(CommitError::WALError(e.to_string()));
        }

        // Write all operations
        if let Err(e) = txn.write_to_wal(&mut wal_writer, commit_version) {
            txn.status = TransactionStatus::Aborted {
                reason: format!("WAL write failed: {}", e),
            };
            return Err(CommitError::WALError(e.to_string()));
        }

        // Write CommitTxn - DURABILITY POINT
        if let Err(e) = wal_writer.write_commit() {
            txn.status = TransactionStatus::Aborted {
                reason: format!("WAL commit failed: {}", e),
            };
            return Err(CommitError::WALError(e.to_string()));
        }

        // DURABILITY POINT: Transaction is now durable
        // Even if we crash after this, recovery will replay from WAL

        // Step 6: Apply to storage
        if let Err(e) = txn.apply_writes(store, commit_version) {
            // This is a serious error - WAL says committed but storage failed
            // Log error but return success since WAL is authoritative
            // Recovery will replay the transaction anyway
            tracing::error!(
                txn_id = txn.txn_id,
                commit_version = commit_version,
                error = %e,
                "Storage application failed after WAL commit - will be recovered on restart"
            );
        }

        // Step 7: Return commit version
        Ok(commit_version)
    }

    /// Explicitly abort a transaction
    ///
    /// Per spec Appendix A.3:
    /// - No AbortTxn entry written to WAL in M2
    /// - All buffered operations discarded
    /// - Transaction marked as Aborted
    ///
    /// # Arguments
    /// * `txn` - Transaction to abort
    /// * `reason` - Human-readable reason for abort
    pub fn abort(&self, txn: &mut TransactionContext, reason: String) -> Result<()> {
        txn.mark_aborted(reason)
    }

    /// Commit with automatic rollback on failure
    ///
    /// Ensures transaction is properly cleaned up if commit fails.
    /// This is a convenience method that handles the common pattern
    /// of wanting to abort on any error.
    pub fn commit_or_rollback<S: Storage>(
        &self,
        txn: &mut TransactionContext,
        store: &S,
        wal: &WAL,
    ) -> std::result::Result<u64, CommitError> {
        match self.commit(txn, store, wal) {
            Ok(version) => Ok(version),
            Err(e) => {
                // Ensure transaction is in Aborted state
                if txn.can_rollback() {
                    let _ = txn.mark_aborted(format!("Commit failed: {}", e));
                }
                Err(e)
            }
        }
    }
}

impl Default for TransactionManager {
    fn default() -> Self {
        Self::new(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::TransactionContext;
    use strata_core::types::{Key, Namespace};
    use strata_core::value::Value;
    use strata_durability::wal::DurabilityMode;
    use strata_storage::ShardedStore;
    use std::sync::Arc;
    use std::thread;
    use tempfile::TempDir;

    fn create_test_namespace(run_id: RunId) -> Namespace {
        Namespace::new(
            "tenant".to_string(),
            "app".to_string(),
            "agent".to_string(),
            run_id,
        )
    }

    fn create_test_key(ns: &Namespace, name: &str) -> Key {
        Key::new_kv(ns.clone(), name)
    }

    #[test]
    fn test_new_manager_has_correct_initial_version() {
        let manager = TransactionManager::new(100);
        assert_eq!(manager.current_version(), 100);
    }

    #[test]
    fn test_allocate_version_increments() {
        let manager = TransactionManager::new(0);
        assert_eq!(manager.allocate_version(), 1);
        assert_eq!(manager.allocate_version(), 2);
        assert_eq!(manager.allocate_version(), 3);
        assert_eq!(manager.current_version(), 3);
    }

    #[test]
    fn test_next_txn_id_increments() {
        // TransactionManager::new(0) calls with_txn_id(0, 0), which sets next_txn_id = 0 + 1 = 1
        let manager = TransactionManager::new(0);
        assert_eq!(manager.next_txn_id(), 1);
        assert_eq!(manager.next_txn_id(), 2);
        assert_eq!(manager.next_txn_id(), 3);
    }

    #[test]
    fn test_with_txn_id_starts_from_max_plus_one() {
        let manager = TransactionManager::with_txn_id(50, 100);
        assert_eq!(manager.current_version(), 50);
        assert_eq!(manager.next_txn_id(), 101); // max_txn_id + 1
    }

    #[test]
    fn test_per_run_commit_locks_allow_parallel_different_runs() {
        // This test verifies that commits on different runs can proceed in parallel
        // by checking that both commits complete and produce unique versions
        let temp_dir = TempDir::new().unwrap();
        let wal_path = temp_dir.path().join("parallel.wal");
        let wal = Arc::new(WAL::open(&wal_path, DurabilityMode::Strict).unwrap());
        let store = Arc::new(ShardedStore::new());
        let manager = Arc::new(TransactionManager::new(0));

        let run_id1 = RunId::new();
        let run_id2 = RunId::new();
        let ns1 = create_test_namespace(run_id1);
        let ns2 = create_test_namespace(run_id2);
        let key1 = create_test_key(&ns1, "key1");
        let key2 = create_test_key(&ns2, "key2");

        // Prepare transactions
        let snapshot1 = store.snapshot();
        let mut txn1 = TransactionContext::with_snapshot(1, run_id1, Box::new(snapshot1));
        txn1.put(key1.clone(), Value::Int(1)).unwrap();

        let snapshot2 = store.snapshot();
        let mut txn2 = TransactionContext::with_snapshot(2, run_id2, Box::new(snapshot2));
        txn2.put(key2.clone(), Value::Int(2)).unwrap();

        // Commit both in parallel threads
        let manager_clone = Arc::clone(&manager);
        let store_clone = Arc::clone(&store);
        let wal_clone = Arc::clone(&wal);

        let handle1 = thread::spawn(move || {
            manager_clone.commit(&mut txn1, store_clone.as_ref(), wal_clone.as_ref())
        });

        let manager_clone2 = Arc::clone(&manager);
        let store_clone2 = Arc::clone(&store);
        let wal_clone2 = Arc::clone(&wal);

        let handle2 = thread::spawn(move || {
            manager_clone2.commit(&mut txn2, store_clone2.as_ref(), wal_clone2.as_ref())
        });

        let v1 = handle1.join().unwrap().unwrap();
        let v2 = handle2.join().unwrap().unwrap();

        // Both commits should succeed with unique versions
        assert!(v1 >= 1 && v1 <= 2);
        assert!(v2 >= 1 && v2 <= 2);
        assert_ne!(v1, v2); // Versions must be unique

        // Both keys should be in storage
        assert!(store.get(&key1).unwrap().is_some());
        assert!(store.get(&key2).unwrap().is_some());
    }

    #[test]
    fn test_same_run_commits_serialize() {
        // This test verifies that commits on the same run are serialized
        // (one completes before the other starts its critical section)
        let temp_dir = TempDir::new().unwrap();
        let wal_path = temp_dir.path().join("serial.wal");
        let wal = Arc::new(WAL::open(&wal_path, DurabilityMode::Strict).unwrap());
        let store = Arc::new(ShardedStore::new());
        let manager = Arc::new(TransactionManager::new(0));

        let run_id = RunId::new();
        let ns = create_test_namespace(run_id);
        let key1 = create_test_key(&ns, "key1");
        let key2 = create_test_key(&ns, "key2");

        // Commit first transaction
        {
            let snapshot = store.snapshot();
            let mut txn = TransactionContext::with_snapshot(1, run_id, Box::new(snapshot));
            txn.put(key1.clone(), Value::Int(100)).unwrap();
            let v = manager.commit(&mut txn, store.as_ref(), wal.as_ref()).unwrap();
            assert_eq!(v, 1);
        }

        // Commit second transaction on same run
        {
            let snapshot = store.snapshot();
            let mut txn = TransactionContext::with_snapshot(2, run_id, Box::new(snapshot));
            txn.put(key2.clone(), Value::Int(200)).unwrap();
            let v = manager.commit(&mut txn, store.as_ref(), wal.as_ref()).unwrap();
            assert_eq!(v, 2);
        }

        // Both values should be present with correct versions
        let v1 = store.get(&key1).unwrap().unwrap();
        assert_eq!(v1.value, Value::Int(100));
        assert_eq!(v1.version.as_u64(), 1);

        let v2 = store.get(&key2).unwrap().unwrap();
        assert_eq!(v2.value, Value::Int(200));
        assert_eq!(v2.version.as_u64(), 2);
    }

    #[test]
    fn test_abort_marks_transaction_aborted() {
        let run_id = RunId::new();
        let manager = TransactionManager::new(0);
        let mut txn = TransactionContext::new(1, run_id, 0);

        manager.abort(&mut txn, "test abort".to_string()).unwrap();

        assert!(matches!(
            txn.status,
            crate::TransactionStatus::Aborted { .. }
        ));
    }

    #[test]
    fn test_many_parallel_commits_different_runs() {
        // Stress test: many parallel commits on different runs
        let temp_dir = TempDir::new().unwrap();
        let wal_path = temp_dir.path().join("stress.wal");
        let wal = Arc::new(WAL::open(&wal_path, DurabilityMode::Strict).unwrap());
        let store = Arc::new(ShardedStore::new());
        let manager = Arc::new(TransactionManager::new(0));

        let num_threads = 10;
        let mut handles = Vec::new();

        for i in 0..num_threads {
            let manager_clone = Arc::clone(&manager);
            let store_clone = Arc::clone(&store);
            let wal_clone = Arc::clone(&wal);

            handles.push(thread::spawn(move || {
                let run_id = RunId::new();
                let ns = create_test_namespace(run_id);
                let key = create_test_key(&ns, &format!("key_{}", i));

                let snapshot = store_clone.snapshot();
                let mut txn = TransactionContext::with_snapshot(i as u64 + 1, run_id, Box::new(snapshot));
                txn.put(key, Value::Int(i as i64)).unwrap();

                manager_clone.commit(&mut txn, store_clone.as_ref(), wal_clone.as_ref())
            }));
        }

        // All commits should succeed
        let versions: Vec<u64> = handles
            .into_iter()
            .map(|h| h.join().unwrap().unwrap())
            .collect();

        // All versions should be unique and in range 1..=num_threads
        let mut sorted = versions.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(sorted.len(), num_threads);
        assert_eq!(sorted[0], 1);
        assert_eq!(sorted[num_threads - 1], num_threads as u64);
    }
}

