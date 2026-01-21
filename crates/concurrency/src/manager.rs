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
use strata_core::error::Result;
use strata_core::traits::Storage;
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
pub struct TransactionManager {
    /// Global version counter
    ///
    /// Monotonically increasing. Each committed transaction increments by 1.
    version: AtomicU64,

    /// Next transaction ID
    ///
    /// Unique identifier for transactions. Used in WAL entries.
    next_txn_id: AtomicU64,
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
    /// 1. Validate and mark committed (in-memory state transition)
    /// 2. Allocate commit version
    /// 3. Write BeginTxn to WAL
    /// 4. Write all operations to WAL
    /// 5. Write CommitTxn to WAL (DURABILITY POINT)
    /// 6. Apply writes to storage
    /// 7. Return commit version
    pub fn commit<S: Storage>(
        &self,
        txn: &mut TransactionContext,
        store: &S,
        wal: &mut WAL,
    ) -> std::result::Result<u64, CommitError> {
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
        wal: &mut WAL,
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
    use strata_core::types::{Key, Namespace, RunId, TypeTag};
    use strata_core::value::Value;
    use strata_durability::wal::DurabilityMode;
    use strata_storage::UnifiedStore;
    use tempfile::TempDir;

    fn create_test_namespace() -> Namespace {
        let run_id = RunId::new();
        Namespace::new(
            "tenant".to_string(),
            "app".to_string(),
            "agent".to_string(),
            run_id,
        )
    }

    fn create_test_key(ns: &Namespace, name: &str) -> Key {
        Key::new(ns.clone(), TypeTag::KV, name.as_bytes().to_vec())
    }

    fn setup_test_env() -> (TransactionManager, UnifiedStore, WAL, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let wal_path = temp_dir.path().join("test.wal");
        let wal = WAL::open(&wal_path, DurabilityMode::default()).unwrap();
        let store = UnifiedStore::new();
        let manager = TransactionManager::new(store.current_version());
        (manager, store, wal, temp_dir)
    }

    fn create_txn_with_store(
        store: &UnifiedStore,
        manager: &TransactionManager,
    ) -> TransactionContext {
        let run_id = RunId::new();
        TransactionContext::with_snapshot(
            manager.current_version(),
            run_id,
            Box::new(store.create_snapshot()),
        )
    }

    #[test]
    fn test_new_manager() {
        let manager = TransactionManager::new(100);
        assert_eq!(manager.current_version(), 100);
    }

    #[test]
    fn test_default_manager() {
        let manager = TransactionManager::default();
        assert_eq!(manager.current_version(), 0);
    }

    #[test]
    fn test_next_txn_id() {
        let manager = TransactionManager::new(0);
        assert_eq!(manager.next_txn_id(), 1);
        assert_eq!(manager.next_txn_id(), 2);
        assert_eq!(manager.next_txn_id(), 3);
    }

    #[test]
    fn test_allocate_version() {
        let manager = TransactionManager::new(10);
        assert_eq!(manager.allocate_version(), 11);
        assert_eq!(manager.allocate_version(), 12);
        assert_eq!(manager.current_version(), 12);
    }

    #[test]
    fn test_atomic_commit_success() {
        let (manager, store, mut wal, _temp) = setup_test_env();
        let ns = create_test_namespace();
        let key = create_test_key(&ns, "key");

        let mut txn = create_txn_with_store(&store, &manager);
        txn.put(key.clone(), Value::I64(42)).unwrap();

        let result = manager.commit(&mut txn, &store, &mut wal);

        assert!(result.is_ok());
        let commit_version = result.unwrap();

        // Verify transaction is committed
        assert_eq!(txn.status, TransactionStatus::Committed);

        // Verify storage was updated
        let stored = store.get(&key).unwrap().unwrap();
        assert_eq!(stored.value, Value::I64(42));
        assert_eq!(stored.version.as_u64(), commit_version);

        // Verify WAL was written
        let entries = wal.read_all().unwrap();
        assert!(entries.len() >= 3); // BeginTxn + Write + CommitTxn
    }

    #[test]
    fn test_atomic_commit_validation_failure() {
        let (manager, store, mut wal, _temp) = setup_test_env();
        let ns = create_test_namespace();
        let key = create_test_key(&ns, "key");
        let run_id = RunId::new();

        // Pre-existing key
        store.put(key.clone(), Value::I64(100), None).unwrap();

        let mut txn = TransactionContext::with_snapshot(
            manager.current_version(),
            run_id,
            Box::new(store.create_snapshot()),
        );
        let _ = txn.get(&key).unwrap(); // Read adds to read_set
        txn.put(key.clone(), Value::I64(200)).unwrap();

        // Concurrent modification
        store.put(key.clone(), Value::I64(300), None).unwrap();

        let result = manager.commit(&mut txn, &store, &mut wal);

        assert!(result.is_err());
        assert!(matches!(txn.status, TransactionStatus::Aborted { .. }));

        // WAL should be empty (no entries written for failed validation)
        let entries = wal.read_all().unwrap();
        assert!(entries.is_empty());
    }

    #[test]
    fn test_atomic_commit_version_increment() {
        let (manager, store, mut wal, _temp) = setup_test_env();
        let ns = create_test_namespace();
        let key1 = create_test_key(&ns, "key1");
        let key2 = create_test_key(&ns, "key2");

        let initial_version = manager.current_version();

        // First transaction
        let mut txn1 = create_txn_with_store(&store, &manager);
        txn1.put(key1.clone(), Value::I64(1)).unwrap();
        let v1 = manager.commit(&mut txn1, &store, &mut wal).unwrap();

        // Second transaction
        let mut txn2 = create_txn_with_store(&store, &manager);
        txn2.put(key2.clone(), Value::I64(2)).unwrap();
        let v2 = manager.commit(&mut txn2, &store, &mut wal).unwrap();

        // Versions should be monotonically increasing
        assert!(v1 > initial_version);
        assert!(v2 > v1);
    }

    #[test]
    fn test_atomic_commit_all_keys_same_version() {
        let (manager, store, mut wal, _temp) = setup_test_env();
        let ns = create_test_namespace();
        let key1 = create_test_key(&ns, "key1");
        let key2 = create_test_key(&ns, "key2");
        let key3 = create_test_key(&ns, "key3");

        let mut txn = create_txn_with_store(&store, &manager);
        txn.put(key1.clone(), Value::I64(1)).unwrap();
        txn.put(key2.clone(), Value::I64(2)).unwrap();
        txn.put(key3.clone(), Value::I64(3)).unwrap();

        let commit_version = manager.commit(&mut txn, &store, &mut wal).unwrap();

        // Per spec Section 6.1: All keys in a transaction get the same commit version
        assert_eq!(store.get(&key1).unwrap().unwrap().version.as_u64(), commit_version);
        assert_eq!(store.get(&key2).unwrap().unwrap().version.as_u64(), commit_version);
        assert_eq!(store.get(&key3).unwrap().unwrap().version.as_u64(), commit_version);
    }

    #[test]
    fn test_first_committer_wins_with_manager() {
        // Set up initial data FIRST, then create manager with synced version
        let temp_dir = TempDir::new().unwrap();
        let wal_path = temp_dir.path().join("test.wal");
        let mut wal = WAL::open(&wal_path, DurabilityMode::default()).unwrap();
        let store = UnifiedStore::new();
        let ns = create_test_namespace();
        let key = create_test_key(&ns, "shared");

        // Put initial data
        store
            .put(key.clone(), Value::String("initial".into()), None)
            .unwrap();

        // Create manager with current store version (important: after initial data)
        let manager = TransactionManager::new(store.current_version());

        // Both transactions read and write same key
        let run_id1 = RunId::new();
        let mut txn1 = TransactionContext::with_snapshot(
            manager.current_version(),
            run_id1,
            Box::new(store.create_snapshot()),
        );
        let _ = txn1.get(&key).unwrap();
        txn1.put(key.clone(), Value::String("from_t1".into()))
            .unwrap();

        let run_id2 = RunId::new();
        let mut txn2 = TransactionContext::with_snapshot(
            manager.current_version(),
            run_id2,
            Box::new(store.create_snapshot()),
        );
        let _ = txn2.get(&key).unwrap();
        txn2.put(key.clone(), Value::String("from_t2".into()))
            .unwrap();

        // T1 commits first - succeeds
        let result1 = manager.commit(&mut txn1, &store, &mut wal);
        assert!(result1.is_ok());

        // T2 commits second - fails due to read-write conflict
        // T2's read_set has version 1 (initial), but T1's commit changed it to version 2
        let result2 = manager.commit(&mut txn2, &store, &mut wal);
        assert!(result2.is_err());

        // Final value should be from T1
        let stored = store.get(&key).unwrap().unwrap();
        assert_eq!(stored.value, Value::String("from_t1".into()));
    }

    #[test]
    fn test_commit_empty_transaction() {
        let (manager, store, mut wal, _temp) = setup_test_env();

        let mut txn = create_txn_with_store(&store, &manager);
        // No operations

        let result = manager.commit(&mut txn, &store, &mut wal);

        assert!(result.is_ok());
        assert_eq!(txn.status, TransactionStatus::Committed);

        // WAL should have BeginTxn + CommitTxn even for empty transaction
        let entries = wal.read_all().unwrap();
        assert_eq!(entries.len(), 2);
    }

    #[test]
    fn test_commit_with_cas() {
        let (manager, store, mut wal, _temp) = setup_test_env();
        let ns = create_test_namespace();
        let key = create_test_key(&ns, "counter");

        store.put(key.clone(), Value::I64(0), None).unwrap();
        let v1 = store.get(&key).unwrap().unwrap().version.as_u64();

        let mut txn = create_txn_with_store(&store, &manager);
        txn.cas(key.clone(), v1, Value::I64(1)).unwrap();

        let result = manager.commit(&mut txn, &store, &mut wal);

        assert!(result.is_ok());

        // Verify CAS was applied
        let stored = store.get(&key).unwrap().unwrap();
        assert_eq!(stored.value, Value::I64(1));
    }

    #[test]
    fn test_commit_with_delete() {
        let (manager, store, mut wal, _temp) = setup_test_env();
        let ns = create_test_namespace();
        let key = create_test_key(&ns, "to_delete");

        store.put(key.clone(), Value::I64(100), None).unwrap();

        let mut txn = create_txn_with_store(&store, &manager);
        txn.delete(key.clone()).unwrap();

        let result = manager.commit(&mut txn, &store, &mut wal);

        assert!(result.is_ok());

        // Key should be deleted
        assert!(store.get(&key).unwrap().is_none());
    }

    #[test]
    fn test_abort_transaction() {
        let (manager, store, _wal, _temp) = setup_test_env();
        let ns = create_test_namespace();
        let key = create_test_key(&ns, "key");

        let mut txn = create_txn_with_store(&store, &manager);
        txn.put(key.clone(), Value::I64(42)).unwrap();

        manager
            .abort(&mut txn, "User requested".to_string())
            .unwrap();

        assert!(matches!(txn.status, TransactionStatus::Aborted { .. }));
        assert_eq!(txn.abort_reason(), Some("User requested"));

        // Storage should be unchanged
        assert!(store.get(&key).unwrap().is_none());
    }

    #[test]
    fn test_commit_or_rollback_success() {
        let (manager, store, mut wal, _temp) = setup_test_env();
        let ns = create_test_namespace();
        let key = create_test_key(&ns, "key");

        let mut txn = create_txn_with_store(&store, &manager);
        txn.put(key.clone(), Value::I64(42)).unwrap();

        let result = manager.commit_or_rollback(&mut txn, &store, &mut wal);

        assert!(result.is_ok());
        assert_eq!(txn.status, TransactionStatus::Committed);
    }

    #[test]
    fn test_commit_or_rollback_failure_cleans_up() {
        let (manager, store, mut wal, _temp) = setup_test_env();
        let ns = create_test_namespace();
        let key = create_test_key(&ns, "key");
        let run_id = RunId::new();

        store.put(key.clone(), Value::I64(100), None).unwrap();

        let mut txn = TransactionContext::with_snapshot(
            manager.current_version(),
            run_id,
            Box::new(store.create_snapshot()),
        );
        let _ = txn.get(&key).unwrap();
        txn.put(key.clone(), Value::I64(200)).unwrap();

        // Concurrent modification causes conflict
        store.put(key.clone(), Value::I64(300), None).unwrap();

        let result = manager.commit_or_rollback(&mut txn, &store, &mut wal);

        assert!(result.is_err());
        assert!(matches!(txn.status, TransactionStatus::Aborted { .. }));
    }

    #[test]
    fn test_wal_entries_have_correct_structure() {
        use strata_durability::wal::WALEntry;

        let (manager, store, mut wal, _temp) = setup_test_env();
        let ns = create_test_namespace();
        let key = create_test_key(&ns, "key");

        let mut txn = create_txn_with_store(&store, &manager);
        txn.put(key.clone(), Value::I64(42)).unwrap();

        let commit_version = manager.commit(&mut txn, &store, &mut wal).unwrap();

        let entries = wal.read_all().unwrap();
        assert_eq!(entries.len(), 3);

        // First entry should be BeginTxn
        assert!(matches!(entries[0], WALEntry::BeginTxn { .. }));

        // Second entry should be Write with correct version
        if let WALEntry::Write { version, .. } = &entries[1] {
            assert_eq!(*version, commit_version);
        } else {
            panic!("Expected Write entry");
        }

        // Third entry should be CommitTxn
        assert!(matches!(entries[2], WALEntry::CommitTxn { .. }));
    }

    #[test]
    fn test_concurrent_transactions_different_keys() {
        let (manager, store, mut wal, _temp) = setup_test_env();
        let ns = create_test_namespace();
        let key1 = create_test_key(&ns, "key1");
        let key2 = create_test_key(&ns, "key2");

        // T1 writes key1
        let mut txn1 = create_txn_with_store(&store, &manager);
        txn1.put(key1.clone(), Value::I64(1)).unwrap();

        // T2 writes key2
        let mut txn2 = create_txn_with_store(&store, &manager);
        txn2.put(key2.clone(), Value::I64(2)).unwrap();

        // Both should succeed (no conflict - different keys)
        let result1 = manager.commit(&mut txn1, &store, &mut wal);
        let result2 = manager.commit(&mut txn2, &store, &mut wal);

        assert!(result1.is_ok());
        assert!(result2.is_ok());

        // Both values should be in storage
        assert_eq!(store.get(&key1).unwrap().unwrap().value, Value::I64(1));
        assert_eq!(store.get(&key2).unwrap().unwrap().value, Value::I64(2));
    }

    #[test]
    fn test_blind_writes_no_conflict() {
        let (manager, store, mut wal, _temp) = setup_test_env();
        let ns = create_test_namespace();
        let key = create_test_key(&ns, "shared");

        store.put(key.clone(), Value::I64(0), None).unwrap();

        // T1 blind writes (no read)
        let mut txn1 = create_txn_with_store(&store, &manager);
        txn1.put(key.clone(), Value::I64(1)).unwrap();

        // T2 also blind writes
        let mut txn2 = create_txn_with_store(&store, &manager);
        txn2.put(key.clone(), Value::I64(2)).unwrap();

        // T1 commits first
        let result1 = manager.commit(&mut txn1, &store, &mut wal);
        assert!(result1.is_ok());

        // T2 should also succeed - blind writes don't conflict
        // Per spec Section 3.2 Scenario 1
        let result2 = manager.commit(&mut txn2, &store, &mut wal);
        assert!(result2.is_ok());

        // Final value is T2's write (last writer wins for blind writes)
        assert_eq!(store.get(&key).unwrap().unwrap().value, Value::I64(2));
    }
}
