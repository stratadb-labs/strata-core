//! Recovery infrastructure for transaction-aware database recovery
//!
//! Per spec Section 5 (Replay Semantics):
//! - Replays do NOT re-run conflict detection
//! - Replays apply commit decisions, not re-execute logic
//! - Replays are single-threaded
//! - Versions are preserved exactly
//!
//! ## Recovery Procedure (Section 5.4)
//!
//! 1. Load snapshot (if exists) - not implemented in M2
//! 2. Open WAL, scan for entries
//! 3. Build map: txn_id → [entries]
//! 4. Track which txn_ids have CommitTxn markers
//! 5. Apply COMPLETE transactions (has CommitTxn)
//! 6. DISCARD incomplete transactions
//! 7. Initialize TransactionManager with final version

use crate::TransactionManager;
use in_mem_core::error::Result;
use in_mem_durability::recovery::replay_wal;
use in_mem_durability::wal::{DurabilityMode, WAL};
use in_mem_storage::UnifiedStore;
use std::path::PathBuf;

/// Coordinates database recovery after crash or restart
///
/// Per spec Section 5.4:
/// 1. Loads checkpoint (if exists) - not implemented in M2
/// 2. Replays WAL from beginning
/// 3. Discards incomplete transactions
/// 4. Restores global version counter
/// 5. Initializes TransactionManager with final version
pub struct RecoveryCoordinator {
    /// Path to WAL file
    wal_path: PathBuf,
    /// Path to snapshot directory (optional, not used in M2)
    #[allow(dead_code)]
    snapshot_path: Option<PathBuf>,
}

impl RecoveryCoordinator {
    /// Create a new recovery coordinator
    ///
    /// # Arguments
    /// * `wal_path` - Path to the WAL file
    pub fn new(wal_path: PathBuf) -> Self {
        RecoveryCoordinator {
            wal_path,
            snapshot_path: None,
        }
    }

    /// Set snapshot path for checkpoint-based recovery (M3+ feature)
    ///
    /// Note: Snapshot-based recovery is not implemented in M2.
    /// This method is provided for future extensibility.
    pub fn with_snapshot_path(mut self, path: PathBuf) -> Self {
        self.snapshot_path = Some(path);
        self
    }

    /// Perform recovery and return initialized components
    ///
    /// Per spec Section 5.4: Recovery Procedure
    ///
    /// # Returns
    /// - `RecoveryResult` containing storage, transaction manager, and stats
    ///
    /// # Errors
    /// - If WAL cannot be opened or read
    /// - If replay fails
    ///
    /// # Recovery Guarantees
    ///
    /// Per spec:
    /// - **Deterministic**: Given the same WAL, replay always produces identical state
    /// - **Version preservation**: Replay preserves exact version numbers from WAL
    /// - **Incomplete = discarded**: Transactions without CommitTxn are discarded
    /// - **Single-threaded**: Replay processes entries in WAL order
    pub fn recover(&self) -> Result<RecoveryResult> {
        // Step 1: Open WAL
        let wal = WAL::open(&self.wal_path, DurabilityMode::Strict)?;

        // Step 2: Create empty storage
        let storage = UnifiedStore::new();

        // Step 3: Replay WAL using existing durability layer function
        // This handles:
        // - Grouping entries by txn_id
        // - Identifying committed transactions (those with CommitTxn)
        // - Discarding incomplete transactions
        // - Applying writes with version preservation
        let durability_stats = replay_wal(&wal, &storage)?;

        // Step 4: Create TransactionManager with recovered version
        // Per spec Section 6.1: Global version counter must be restored
        let txn_manager = TransactionManager::new(durability_stats.final_version);

        // Step 5: Convert stats to our format
        let stats = RecoveryStats {
            txns_replayed: durability_stats.txns_applied,
            incomplete_txns: durability_stats.incomplete_txns,
            aborted_txns: durability_stats.aborted_txns,
            writes_applied: durability_stats.writes_applied,
            deletes_applied: durability_stats.deletes_applied,
            final_version: durability_stats.final_version,
            from_checkpoint: false, // Checkpoint not implemented in M2
        };

        Ok(RecoveryResult {
            storage,
            txn_manager,
            stats,
        })
    }
}

/// Result of recovery operation
pub struct RecoveryResult {
    /// Recovered storage with all committed transactions applied
    pub storage: UnifiedStore,
    /// Transaction manager initialized with recovered version
    ///
    /// Per spec Section 6.1: The global version counter is set to the
    /// highest version seen in the WAL, ensuring new transactions get
    /// monotonically increasing versions.
    pub txn_manager: TransactionManager,
    /// Statistics about the recovery process
    pub stats: RecoveryStats,
}

/// Statistics from recovery
///
/// Provides detailed information about what happened during recovery,
/// useful for debugging, monitoring, and verification.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct RecoveryStats {
    /// Number of committed transactions replayed
    ///
    /// These are transactions that had both BeginTxn and CommitTxn markers
    /// in the WAL and were successfully applied to storage.
    pub txns_replayed: usize,

    /// Number of incomplete transactions discarded
    ///
    /// Per spec Section 5.5: Transactions with BeginTxn but no CommitTxn
    /// represent crashed-during-commit scenarios and are discarded.
    pub incomplete_txns: usize,

    /// Number of aborted transactions discarded
    ///
    /// Transactions that were explicitly aborted (AbortTxn in WAL).
    pub aborted_txns: usize,

    /// Number of write operations applied
    pub writes_applied: usize,

    /// Number of delete operations applied
    pub deletes_applied: usize,

    /// Final version after recovery
    ///
    /// This is the highest version seen in the WAL, used to initialize
    /// the TransactionManager's version counter.
    pub final_version: u64,

    /// Whether recovery was from checkpoint
    ///
    /// In M2, this is always false as checkpoint-based recovery is not implemented.
    pub from_checkpoint: bool,
}

impl RecoveryStats {
    /// Total operations applied (writes + deletes)
    pub fn total_operations(&self) -> usize {
        self.writes_applied + self.deletes_applied
    }

    /// Total transactions found (replayed + incomplete + aborted)
    pub fn total_transactions(&self) -> usize {
        self.txns_replayed + self.incomplete_txns + self.aborted_txns
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use in_mem_core::traits::Storage;
    use in_mem_core::types::{Key, Namespace, RunId};
    use in_mem_core::value::Value;
    use in_mem_durability::wal::WALEntry;
    use tempfile::TempDir;

    fn create_test_namespace(run_id: RunId) -> Namespace {
        Namespace::new(
            "tenant".to_string(),
            "app".to_string(),
            "agent".to_string(),
            run_id,
        )
    }

    fn now() -> i64 {
        Utc::now().timestamp()
    }

    #[test]
    fn test_recovery_empty_wal() {
        // Per spec: Empty WAL = empty database, version 0
        let temp_dir = TempDir::new().unwrap();
        let wal_path = temp_dir.path().join("empty.wal");

        // Create empty WAL
        let _wal = WAL::open(&wal_path, DurabilityMode::Strict).unwrap();
        drop(_wal);

        let coordinator = RecoveryCoordinator::new(wal_path);
        let result = coordinator.recover().unwrap();

        assert_eq!(result.stats.txns_replayed, 0);
        assert_eq!(result.stats.final_version, 0);
        assert_eq!(result.txn_manager.current_version(), 0);
    }

    #[test]
    fn test_recovery_committed_transaction() {
        // Per spec Section 5.4: COMPLETE transactions are applied
        let temp_dir = TempDir::new().unwrap();
        let wal_path = temp_dir.path().join("committed.wal");

        let run_id = RunId::new();
        let ns = create_test_namespace(run_id);
        let key = Key::new_kv(ns, "test_key");

        // Write committed transaction to WAL
        {
            let mut wal = WAL::open(&wal_path, DurabilityMode::Strict).unwrap();
            wal.append(&WALEntry::BeginTxn {
                txn_id: 1,
                run_id,
                timestamp: now(),
            })
            .unwrap();
            wal.append(&WALEntry::Write {
                run_id,
                key: key.clone(),
                value: Value::I64(42),
                version: 100,
            })
            .unwrap();
            wal.append(&WALEntry::CommitTxn { txn_id: 1, run_id })
                .unwrap();
        }

        let coordinator = RecoveryCoordinator::new(wal_path);
        let result = coordinator.recover().unwrap();

        // Transaction should be replayed
        assert_eq!(result.stats.txns_replayed, 1);
        assert_eq!(result.stats.writes_applied, 1);
        assert_eq!(result.stats.final_version, 100);

        // TransactionManager should have correct version
        assert_eq!(result.txn_manager.current_version(), 100);

        // Storage should have the key with preserved version
        let stored = result.storage.get(&key).unwrap().unwrap();
        assert_eq!(stored.value, Value::I64(42));
        assert_eq!(stored.version, 100); // Version preserved exactly
    }

    #[test]
    fn test_recovery_discards_incomplete_transaction() {
        // Per spec Section 5.5: Incomplete = has BeginTxn but no CommitTxn
        // These are DISCARDED, not applied
        let temp_dir = TempDir::new().unwrap();
        let wal_path = temp_dir.path().join("incomplete.wal");

        let run_id = RunId::new();
        let ns = create_test_namespace(run_id);
        let key = Key::new_kv(ns, "crash_key");

        // Write incomplete transaction (crash scenario)
        {
            let mut wal = WAL::open(&wal_path, DurabilityMode::Strict).unwrap();
            wal.append(&WALEntry::BeginTxn {
                txn_id: 1,
                run_id,
                timestamp: now(),
            })
            .unwrap();
            wal.append(&WALEntry::Write {
                run_id,
                key: key.clone(),
                value: Value::String("should_not_exist".to_string()),
                version: 50,
            })
            .unwrap();
            // NO CommitTxn - simulates crash during commit
        }

        let coordinator = RecoveryCoordinator::new(wal_path);
        let result = coordinator.recover().unwrap();

        // Transaction should be discarded
        assert_eq!(result.stats.txns_replayed, 0);
        assert_eq!(result.stats.incomplete_txns, 1);
        assert_eq!(result.stats.writes_applied, 0);

        // Storage should NOT have the key
        assert!(result.storage.get(&key).unwrap().is_none());
    }

    #[test]
    fn test_recovery_version_preservation() {
        // Per spec Section 5.3 Rule 4: Versions are preserved exactly
        let temp_dir = TempDir::new().unwrap();
        let wal_path = temp_dir.path().join("versions.wal");

        let run_id = RunId::new();
        let ns = create_test_namespace(run_id);

        // Write with non-sequential versions (like real usage)
        {
            let mut wal = WAL::open(&wal_path, DurabilityMode::Strict).unwrap();

            // Transaction 1: version 100
            wal.append(&WALEntry::BeginTxn {
                txn_id: 1,
                run_id,
                timestamp: now(),
            })
            .unwrap();
            wal.append(&WALEntry::Write {
                run_id,
                key: Key::new_kv(ns.clone(), "key1"),
                value: Value::I64(1),
                version: 100,
            })
            .unwrap();
            wal.append(&WALEntry::Write {
                run_id,
                key: Key::new_kv(ns.clone(), "key2"),
                value: Value::I64(2),
                version: 100, // Same version in one txn
            })
            .unwrap();
            wal.append(&WALEntry::CommitTxn { txn_id: 1, run_id })
                .unwrap();

            // Transaction 2: version 200
            wal.append(&WALEntry::BeginTxn {
                txn_id: 2,
                run_id,
                timestamp: now(),
            })
            .unwrap();
            wal.append(&WALEntry::Write {
                run_id,
                key: Key::new_kv(ns.clone(), "key3"),
                value: Value::I64(3),
                version: 200,
            })
            .unwrap();
            wal.append(&WALEntry::CommitTxn { txn_id: 2, run_id })
                .unwrap();
        }

        let coordinator = RecoveryCoordinator::new(wal_path);
        let result = coordinator.recover().unwrap();

        // Final version should be max from WAL
        assert_eq!(result.stats.final_version, 200);
        assert_eq!(result.txn_manager.current_version(), 200);

        // Verify each key has correct version
        let key1 = Key::new_kv(ns.clone(), "key1");
        assert_eq!(result.storage.get(&key1).unwrap().unwrap().version, 100);

        let key2 = Key::new_kv(ns.clone(), "key2");
        assert_eq!(result.storage.get(&key2).unwrap().unwrap().version, 100);

        let key3 = Key::new_kv(ns.clone(), "key3");
        assert_eq!(result.storage.get(&key3).unwrap().unwrap().version, 200);
    }

    #[test]
    fn test_recovery_determinism() {
        // Per spec Section 5.6: replay(W) at T1 == replay(W) at T2
        let temp_dir = TempDir::new().unwrap();
        let wal_path = temp_dir.path().join("determinism.wal");

        let run_id = RunId::new();
        let ns = create_test_namespace(run_id);

        // Create WAL with some transactions
        {
            let mut wal = WAL::open(&wal_path, DurabilityMode::Strict).unwrap();

            for i in 1..=5u64 {
                wal.append(&WALEntry::BeginTxn {
                    txn_id: i,
                    run_id,
                    timestamp: now(),
                })
                .unwrap();
                wal.append(&WALEntry::Write {
                    run_id,
                    key: Key::new_kv(ns.clone(), &format!("key{}", i)),
                    value: Value::I64(i as i64 * 10),
                    version: i * 100,
                })
                .unwrap();
                wal.append(&WALEntry::CommitTxn { txn_id: i, run_id })
                    .unwrap();
            }
        }

        // Recover twice
        let coordinator = RecoveryCoordinator::new(wal_path.clone());
        let result1 = coordinator.recover().unwrap();

        let coordinator = RecoveryCoordinator::new(wal_path);
        let result2 = coordinator.recover().unwrap();

        // Results must be identical
        assert_eq!(result1.stats.final_version, result2.stats.final_version);
        assert_eq!(result1.stats.txns_replayed, result2.stats.txns_replayed);
        assert_eq!(result1.stats.writes_applied, result2.stats.writes_applied);

        // Verify storage state is identical
        for i in 1..=5u64 {
            let key = Key::new_kv(ns.clone(), &format!("key{}", i));
            let v1 = result1.storage.get(&key).unwrap().unwrap();
            let v2 = result2.storage.get(&key).unwrap().unwrap();
            assert_eq!(v1.value, v2.value);
            assert_eq!(v1.version, v2.version);
        }
    }

    #[test]
    fn test_recovery_mixed_transactions() {
        // Mix of committed, incomplete, and aborted transactions
        let temp_dir = TempDir::new().unwrap();
        let wal_path = temp_dir.path().join("mixed.wal");

        let run_id = RunId::new();
        let ns = create_test_namespace(run_id);

        {
            let mut wal = WAL::open(&wal_path, DurabilityMode::Strict).unwrap();

            // Txn 1: Committed
            wal.append(&WALEntry::BeginTxn {
                txn_id: 1,
                run_id,
                timestamp: now(),
            })
            .unwrap();
            wal.append(&WALEntry::Write {
                run_id,
                key: Key::new_kv(ns.clone(), "committed"),
                value: Value::String("yes".to_string()),
                version: 10,
            })
            .unwrap();
            wal.append(&WALEntry::CommitTxn { txn_id: 1, run_id })
                .unwrap();

            // Txn 2: Incomplete (crash)
            wal.append(&WALEntry::BeginTxn {
                txn_id: 2,
                run_id,
                timestamp: now(),
            })
            .unwrap();
            wal.append(&WALEntry::Write {
                run_id,
                key: Key::new_kv(ns.clone(), "incomplete"),
                value: Value::String("no".to_string()),
                version: 20,
            })
            .unwrap();
            // NO CommitTxn

            // Txn 3: Aborted
            wal.append(&WALEntry::BeginTxn {
                txn_id: 3,
                run_id,
                timestamp: now(),
            })
            .unwrap();
            wal.append(&WALEntry::Write {
                run_id,
                key: Key::new_kv(ns.clone(), "aborted"),
                value: Value::String("no".to_string()),
                version: 30,
            })
            .unwrap();
            wal.append(&WALEntry::AbortTxn { txn_id: 3, run_id })
                .unwrap();

            // Txn 4: Committed
            wal.append(&WALEntry::BeginTxn {
                txn_id: 4,
                run_id,
                timestamp: now(),
            })
            .unwrap();
            wal.append(&WALEntry::Write {
                run_id,
                key: Key::new_kv(ns.clone(), "also_committed"),
                value: Value::String("yes".to_string()),
                version: 40,
            })
            .unwrap();
            wal.append(&WALEntry::CommitTxn { txn_id: 4, run_id })
                .unwrap();
        }

        let coordinator = RecoveryCoordinator::new(wal_path);
        let result = coordinator.recover().unwrap();

        assert_eq!(result.stats.txns_replayed, 2); // Txn 1 and 4
        assert_eq!(result.stats.incomplete_txns, 1); // Txn 2
        assert_eq!(result.stats.aborted_txns, 1); // Txn 3
        assert_eq!(result.stats.final_version, 40);

        // Only committed keys exist
        assert!(result
            .storage
            .get(&Key::new_kv(ns.clone(), "committed"))
            .unwrap()
            .is_some());
        assert!(result
            .storage
            .get(&Key::new_kv(ns.clone(), "also_committed"))
            .unwrap()
            .is_some());
        assert!(result
            .storage
            .get(&Key::new_kv(ns.clone(), "incomplete"))
            .unwrap()
            .is_none());
        assert!(result
            .storage
            .get(&Key::new_kv(ns.clone(), "aborted"))
            .unwrap()
            .is_none());
    }

    #[test]
    fn test_recovery_with_deletes() {
        let temp_dir = TempDir::new().unwrap();
        let wal_path = temp_dir.path().join("deletes.wal");

        let run_id = RunId::new();
        let ns = create_test_namespace(run_id);
        let key = Key::new_kv(ns, "deleted_key");

        {
            let mut wal = WAL::open(&wal_path, DurabilityMode::Strict).unwrap();

            // Write then delete in same transaction
            wal.append(&WALEntry::BeginTxn {
                txn_id: 1,
                run_id,
                timestamp: now(),
            })
            .unwrap();
            wal.append(&WALEntry::Write {
                run_id,
                key: key.clone(),
                value: Value::String("exists".to_string()),
                version: 100,
            })
            .unwrap();
            wal.append(&WALEntry::Delete {
                run_id,
                key: key.clone(),
                version: 101,
            })
            .unwrap();
            wal.append(&WALEntry::CommitTxn { txn_id: 1, run_id })
                .unwrap();
        }

        let coordinator = RecoveryCoordinator::new(wal_path);
        let result = coordinator.recover().unwrap();

        assert_eq!(result.stats.writes_applied, 1);
        assert_eq!(result.stats.deletes_applied, 1);

        // Key should be deleted
        assert!(result.storage.get(&key).unwrap().is_none());
    }

    #[test]
    fn test_recovery_stats_helpers() {
        let stats = RecoveryStats {
            txns_replayed: 5,
            incomplete_txns: 2,
            aborted_txns: 1,
            writes_applied: 10,
            deletes_applied: 3,
            final_version: 100,
            from_checkpoint: false,
        };

        assert_eq!(stats.total_operations(), 13);
        assert_eq!(stats.total_transactions(), 8);
    }

    #[test]
    fn test_recovery_coordinator_builder() {
        let temp_dir = TempDir::new().unwrap();
        let wal_path = temp_dir.path().join("builder.wal");
        let snapshot_path = temp_dir.path().join("snapshots");

        let _wal = WAL::open(&wal_path, DurabilityMode::Strict).unwrap();
        drop(_wal);

        let coordinator = RecoveryCoordinator::new(wal_path).with_snapshot_path(snapshot_path);

        // Should still work (snapshot not used in M2)
        let result = coordinator.recover().unwrap();
        assert_eq!(result.stats.from_checkpoint, false);
    }

    // ========================================
    // Crash Scenario Tests (Story #96)
    // ========================================
    //
    // Per spec Section 5.5:
    // "If a crash occurs during commit:
    //  - Sees BeginTxn for txn_id 42
    //  - Sees Write entries for txn_id 42
    //  - Does NOT see CommitTxn for txn_id 42
    //  - Conclusion: Transaction 42 is INCOMPLETE
    //  - Action: DISCARD all entries for txn_id 42
    //  - Result: Keys are NOT modified"

    /// Scenario 1: Crash before any WAL activity
    /// Expected: Empty database
    #[test]
    fn test_crash_before_any_activity() {
        let temp_dir = TempDir::new().unwrap();
        let wal_path = temp_dir.path().join("empty.wal");

        // Create WAL file but write nothing
        let _wal = WAL::open(&wal_path, DurabilityMode::Strict).unwrap();
        drop(_wal);

        // Recovery
        let coordinator = RecoveryCoordinator::new(wal_path);
        let result = coordinator.recover().unwrap();

        assert_eq!(result.stats.txns_replayed, 0);
        assert_eq!(result.stats.final_version, 0);
        assert_eq!(result.stats.incomplete_txns, 0);
        assert_eq!(result.txn_manager.current_version(), 0);
    }

    /// Scenario 2: Crash after BeginTxn, before any writes
    /// Expected: Transaction discarded (no writes to apply anyway)
    #[test]
    fn test_crash_after_begin_before_writes() {
        let temp_dir = TempDir::new().unwrap();
        let wal_path = temp_dir.path().join("begin_only.wal");

        let run_id = RunId::new();

        {
            let mut wal = WAL::open(&wal_path, DurabilityMode::Strict).unwrap();
            wal.append(&WALEntry::BeginTxn {
                txn_id: 1,
                run_id,
                timestamp: now(),
            })
            .unwrap();
            // CRASH - no writes, no commit
        }

        let coordinator = RecoveryCoordinator::new(wal_path);
        let result = coordinator.recover().unwrap();

        assert_eq!(result.stats.txns_replayed, 0);
        assert_eq!(result.stats.incomplete_txns, 1);
        assert_eq!(result.stats.writes_applied, 0);
    }

    /// Scenario 3: Crash mid-writes
    /// Expected: ALL writes from this transaction discarded (all-or-nothing)
    #[test]
    fn test_crash_mid_writes() {
        let temp_dir = TempDir::new().unwrap();
        let wal_path = temp_dir.path().join("mid_writes.wal");

        let run_id = RunId::new();
        let ns = create_test_namespace(run_id);

        {
            let mut wal = WAL::open(&wal_path, DurabilityMode::Strict).unwrap();

            wal.append(&WALEntry::BeginTxn {
                txn_id: 1,
                run_id,
                timestamp: now(),
            })
            .unwrap();

            // Some writes completed
            wal.append(&WALEntry::Write {
                run_id,
                key: Key::new_kv(ns.clone(), "key1"),
                value: Value::I64(1),
                version: 10,
            })
            .unwrap();
            wal.append(&WALEntry::Write {
                run_id,
                key: Key::new_kv(ns.clone(), "key2"),
                value: Value::I64(2),
                version: 10,
            })
            .unwrap();
            // CRASH - more writes planned but not written, no commit
        }

        let coordinator = RecoveryCoordinator::new(wal_path);
        let result = coordinator.recover().unwrap();

        // ALL writes discarded (all-or-nothing)
        assert_eq!(result.stats.txns_replayed, 0);
        assert_eq!(result.stats.incomplete_txns, 1);
        assert_eq!(result.stats.writes_applied, 0);

        // Keys should NOT exist
        assert!(result
            .storage
            .get(&Key::new_kv(ns.clone(), "key1"))
            .unwrap()
            .is_none());
        assert!(result
            .storage
            .get(&Key::new_kv(ns.clone(), "key2"))
            .unwrap()
            .is_none());
    }

    /// Scenario 4: Crash after all writes, before CommitTxn
    /// Expected: Transaction discarded
    #[test]
    fn test_crash_after_writes_before_commit() {
        let temp_dir = TempDir::new().unwrap();
        let wal_path = temp_dir.path().join("no_commit.wal");

        let run_id = RunId::new();
        let ns = create_test_namespace(run_id);

        {
            let mut wal = WAL::open(&wal_path, DurabilityMode::Strict).unwrap();

            wal.append(&WALEntry::BeginTxn {
                txn_id: 1,
                run_id,
                timestamp: now(),
            })
            .unwrap();

            wal.append(&WALEntry::Write {
                run_id,
                key: Key::new_kv(ns.clone(), "key1"),
                value: Value::I64(1),
                version: 10,
            })
            .unwrap();

            // CRASH - about to write CommitTxn but didn't
        }

        let coordinator = RecoveryCoordinator::new(wal_path);
        let result = coordinator.recover().unwrap();

        assert_eq!(result.stats.txns_replayed, 0);
        assert_eq!(result.stats.incomplete_txns, 1);

        // Key should NOT exist
        assert!(result
            .storage
            .get(&Key::new_kv(ns, "key1"))
            .unwrap()
            .is_none());
    }

    /// Scenario 5: Crash after CommitTxn written to WAL
    /// Expected: Transaction IS durable, MUST be recovered
    ///
    /// Per spec: "If crash occurs after step 8: Transaction is durable,
    /// replayed on recovery."
    #[test]
    fn test_crash_after_commit_written() {
        let temp_dir = TempDir::new().unwrap();
        let wal_path = temp_dir.path().join("committed_crash.wal");

        let run_id = RunId::new();
        let ns = create_test_namespace(run_id);

        {
            let mut wal = WAL::open(&wal_path, DurabilityMode::Strict).unwrap();

            wal.append(&WALEntry::BeginTxn {
                txn_id: 1,
                run_id,
                timestamp: now(),
            })
            .unwrap();

            wal.append(&WALEntry::Write {
                run_id,
                key: Key::new_kv(ns.clone(), "durable_key"),
                value: Value::String("must_exist".to_string()),
                version: 100,
            })
            .unwrap();

            wal.append(&WALEntry::CommitTxn { txn_id: 1, run_id })
                .unwrap();

            // CRASH - after commit marker written
            // (Storage may not have been updated yet in real scenario)
        }

        let coordinator = RecoveryCoordinator::new(wal_path);
        let result = coordinator.recover().unwrap();

        // Transaction MUST be recovered
        assert_eq!(result.stats.txns_replayed, 1);
        assert_eq!(result.stats.incomplete_txns, 0);

        // Key MUST exist with correct value and version
        let stored = result
            .storage
            .get(&Key::new_kv(ns, "durable_key"))
            .unwrap()
            .unwrap();
        assert_eq!(stored.value, Value::String("must_exist".to_string()));
        assert_eq!(stored.version, 100);
    }

    /// Scenario 6: One committed, one incomplete
    /// Expected: Committed applies, incomplete discarded
    #[test]
    fn test_crash_one_committed_one_incomplete() {
        let temp_dir = TempDir::new().unwrap();
        let wal_path = temp_dir.path().join("one_each.wal");

        let run_id = RunId::new();
        let ns = create_test_namespace(run_id);

        {
            let mut wal = WAL::open(&wal_path, DurabilityMode::Strict).unwrap();

            // Committed transaction
            wal.append(&WALEntry::BeginTxn {
                txn_id: 1,
                run_id,
                timestamp: now(),
            })
            .unwrap();
            wal.append(&WALEntry::Write {
                run_id,
                key: Key::new_kv(ns.clone(), "committed"),
                value: Value::I64(1),
                version: 10,
            })
            .unwrap();
            wal.append(&WALEntry::CommitTxn { txn_id: 1, run_id })
                .unwrap();

            // Incomplete transaction
            wal.append(&WALEntry::BeginTxn {
                txn_id: 2,
                run_id,
                timestamp: now(),
            })
            .unwrap();
            wal.append(&WALEntry::Write {
                run_id,
                key: Key::new_kv(ns.clone(), "uncommitted"),
                value: Value::I64(2),
                version: 20,
            })
            .unwrap();
            // CRASH - no commit
        }

        let coordinator = RecoveryCoordinator::new(wal_path);
        let result = coordinator.recover().unwrap();

        assert_eq!(result.stats.txns_replayed, 1);
        assert_eq!(result.stats.incomplete_txns, 1);

        assert!(result
            .storage
            .get(&Key::new_kv(ns.clone(), "committed"))
            .unwrap()
            .is_some());
        assert!(result
            .storage
            .get(&Key::new_kv(ns.clone(), "uncommitted"))
            .unwrap()
            .is_none());
    }

    /// Scenario 7: Multiple incomplete transactions
    /// Expected: All discarded
    #[test]
    fn test_crash_multiple_incomplete() {
        let temp_dir = TempDir::new().unwrap();
        let wal_path = temp_dir.path().join("multi_incomplete.wal");

        let run_a = RunId::new();
        let run_b = RunId::new();
        let run_c = RunId::new();
        let ns_a = create_test_namespace(run_a);
        let ns_b = create_test_namespace(run_b);
        let ns_c = create_test_namespace(run_c);

        {
            let mut wal = WAL::open(&wal_path, DurabilityMode::Strict).unwrap();

            // Three incomplete transactions from different runs
            wal.append(&WALEntry::BeginTxn {
                txn_id: 1,
                run_id: run_a,
                timestamp: now(),
            })
            .unwrap();
            wal.append(&WALEntry::Write {
                run_id: run_a,
                key: Key::new_kv(ns_a.clone(), "key_a"),
                value: Value::I64(1),
                version: 10,
            })
            .unwrap();
            // NO commit

            wal.append(&WALEntry::BeginTxn {
                txn_id: 2,
                run_id: run_b,
                timestamp: now(),
            })
            .unwrap();
            wal.append(&WALEntry::Write {
                run_id: run_b,
                key: Key::new_kv(ns_b.clone(), "key_b"),
                value: Value::I64(2),
                version: 20,
            })
            .unwrap();
            // NO commit

            wal.append(&WALEntry::BeginTxn {
                txn_id: 3,
                run_id: run_c,
                timestamp: now(),
            })
            .unwrap();
            wal.append(&WALEntry::Write {
                run_id: run_c,
                key: Key::new_kv(ns_c.clone(), "key_c"),
                value: Value::I64(3),
                version: 30,
            })
            .unwrap();
            // NO commit
        }

        let coordinator = RecoveryCoordinator::new(wal_path);
        let result = coordinator.recover().unwrap();

        // All three should be incomplete
        assert_eq!(result.stats.txns_replayed, 0);
        assert_eq!(result.stats.incomplete_txns, 3);
        assert_eq!(result.stats.writes_applied, 0);

        // No keys should exist
        assert!(result
            .storage
            .get(&Key::new_kv(ns_a, "key_a"))
            .unwrap()
            .is_none());
        assert!(result
            .storage
            .get(&Key::new_kv(ns_b, "key_b"))
            .unwrap()
            .is_none());
        assert!(result
            .storage
            .get(&Key::new_kv(ns_c, "key_c"))
            .unwrap()
            .is_none());
    }

    /// Scenario 8: Recovery is idempotent
    /// Expected: Recovering twice gives same result
    #[test]
    fn test_crash_recovery_idempotent() {
        let temp_dir = TempDir::new().unwrap();
        let wal_path = temp_dir.path().join("idempotent.wal");

        let run_id = RunId::new();
        let ns = create_test_namespace(run_id);

        {
            let mut wal = WAL::open(&wal_path, DurabilityMode::Strict).unwrap();

            wal.append(&WALEntry::BeginTxn {
                txn_id: 1,
                run_id,
                timestamp: now(),
            })
            .unwrap();
            wal.append(&WALEntry::Write {
                run_id,
                key: Key::new_kv(ns.clone(), "key"),
                value: Value::I64(42),
                version: 100,
            })
            .unwrap();
            wal.append(&WALEntry::CommitTxn { txn_id: 1, run_id })
                .unwrap();
        }

        // Recover first time
        let coordinator = RecoveryCoordinator::new(wal_path.clone());
        let result1 = coordinator.recover().unwrap();

        // Recover second time (simulates restart)
        let coordinator = RecoveryCoordinator::new(wal_path);
        let result2 = coordinator.recover().unwrap();

        // Results should be identical
        assert_eq!(result1.stats.txns_replayed, result2.stats.txns_replayed);
        assert_eq!(result1.stats.final_version, result2.stats.final_version);
        assert_eq!(result1.stats.writes_applied, result2.stats.writes_applied);

        let v1 = result1
            .storage
            .get(&Key::new_kv(ns.clone(), "key"))
            .unwrap()
            .unwrap();
        let v2 = result2
            .storage
            .get(&Key::new_kv(ns.clone(), "key"))
            .unwrap()
            .unwrap();
        assert_eq!(v1.value, v2.value);
        assert_eq!(v1.version, v2.version);
    }

    /// Scenario 9: Crash with deletes
    /// Expected: Incomplete delete transaction discarded, committed one applies
    #[test]
    fn test_crash_with_delete_operations() {
        let temp_dir = TempDir::new().unwrap();
        let wal_path = temp_dir.path().join("crash_delete.wal");

        let run_id = RunId::new();
        let ns = create_test_namespace(run_id);

        {
            let mut wal = WAL::open(&wal_path, DurabilityMode::Strict).unwrap();

            // Txn 1: Committed - write and delete
            wal.append(&WALEntry::BeginTxn {
                txn_id: 1,
                run_id,
                timestamp: now(),
            })
            .unwrap();
            wal.append(&WALEntry::Write {
                run_id,
                key: Key::new_kv(ns.clone(), "to_delete"),
                value: Value::I64(1),
                version: 10,
            })
            .unwrap();
            wal.append(&WALEntry::Delete {
                run_id,
                key: Key::new_kv(ns.clone(), "to_delete"),
                version: 11,
            })
            .unwrap();
            wal.append(&WALEntry::CommitTxn { txn_id: 1, run_id })
                .unwrap();

            // Txn 2: Incomplete - has a delete but didn't commit
            wal.append(&WALEntry::BeginTxn {
                txn_id: 2,
                run_id,
                timestamp: now(),
            })
            .unwrap();
            wal.append(&WALEntry::Write {
                run_id,
                key: Key::new_kv(ns.clone(), "should_exist"),
                value: Value::I64(2),
                version: 20,
            })
            .unwrap();
            // CRASH before commit
        }

        let coordinator = RecoveryCoordinator::new(wal_path);
        let result = coordinator.recover().unwrap();

        assert_eq!(result.stats.txns_replayed, 1);
        assert_eq!(result.stats.incomplete_txns, 1);
        assert_eq!(result.stats.deletes_applied, 1);

        // to_delete should be deleted (from committed txn)
        assert!(result
            .storage
            .get(&Key::new_kv(ns.clone(), "to_delete"))
            .unwrap()
            .is_none());

        // should_exist should NOT exist (from incomplete txn)
        assert!(result
            .storage
            .get(&Key::new_kv(ns.clone(), "should_exist"))
            .unwrap()
            .is_none());
    }

    /// Scenario 10: Interleaved crash scenario - Two runs, one commits, one crashes
    #[test]
    fn test_crash_interleaved_runs() {
        let temp_dir = TempDir::new().unwrap();
        let wal_path = temp_dir.path().join("interleaved_crash.wal");

        let run_ok = RunId::new();
        let run_crash = RunId::new();
        let ns_ok = create_test_namespace(run_ok);
        let ns_crash = create_test_namespace(run_crash);

        {
            let mut wal = WAL::open(&wal_path, DurabilityMode::Strict).unwrap();

            // Interleaved operations from two runs
            wal.append(&WALEntry::BeginTxn {
                txn_id: 1,
                run_id: run_ok,
                timestamp: now(),
            })
            .unwrap();
            wal.append(&WALEntry::BeginTxn {
                txn_id: 2,
                run_id: run_crash,
                timestamp: now(),
            })
            .unwrap();

            // Both make writes
            wal.append(&WALEntry::Write {
                run_id: run_ok,
                key: Key::new_kv(ns_ok.clone(), "ok_key"),
                value: Value::I64(1),
                version: 10,
            })
            .unwrap();
            wal.append(&WALEntry::Write {
                run_id: run_crash,
                key: Key::new_kv(ns_crash.clone(), "crash_key"),
                value: Value::I64(2),
                version: 20,
            })
            .unwrap();

            // Only run_ok commits
            wal.append(&WALEntry::CommitTxn {
                txn_id: 1,
                run_id: run_ok,
            })
            .unwrap();
            // run_crash never commits - simulates crash during its commit
        }

        let coordinator = RecoveryCoordinator::new(wal_path);
        let result = coordinator.recover().unwrap();

        assert_eq!(result.stats.txns_replayed, 1);
        assert_eq!(result.stats.incomplete_txns, 1);

        // ok_key exists, crash_key doesn't
        assert!(result
            .storage
            .get(&Key::new_kv(ns_ok, "ok_key"))
            .unwrap()
            .is_some());
        assert!(result
            .storage
            .get(&Key::new_kv(ns_crash, "crash_key"))
            .unwrap()
            .is_none());
    }

    /// Scenario 11: Recovery version counter initialization
    /// Expected: TransactionManager version matches WAL max version
    #[test]
    fn test_crash_recovery_version_counter() {
        let temp_dir = TempDir::new().unwrap();
        let wal_path = temp_dir.path().join("version_counter.wal");

        let run_id = RunId::new();
        let ns = create_test_namespace(run_id);

        {
            let mut wal = WAL::open(&wal_path, DurabilityMode::Strict).unwrap();

            // Create transaction with high version
            wal.append(&WALEntry::BeginTxn {
                txn_id: 1,
                run_id,
                timestamp: now(),
            })
            .unwrap();
            wal.append(&WALEntry::Write {
                run_id,
                key: Key::new_kv(ns.clone(), "key"),
                value: Value::I64(1),
                version: 999,
            })
            .unwrap();
            wal.append(&WALEntry::CommitTxn { txn_id: 1, run_id })
                .unwrap();
        }

        let coordinator = RecoveryCoordinator::new(wal_path);
        let result = coordinator.recover().unwrap();

        // TransactionManager must have correct version
        assert_eq!(result.txn_manager.current_version(), 999);
        assert_eq!(result.stats.final_version, 999);

        // New transactions should get versions > 999
        let next_version = result.txn_manager.current_version() + 1;
        assert!(next_version > 999);
    }

    // ========================================
    // Integration Tests (Story #97)
    // ========================================

    /// Full database lifecycle with recovery
    /// Simulates normal operation, crash, and recovery
    #[test]
    fn test_full_database_lifecycle_with_recovery() {
        let temp_dir = TempDir::new().unwrap();
        let wal_path = temp_dir.path().join("lifecycle.wal");

        let run_id = RunId::new();
        let ns = create_test_namespace(run_id);

        // Phase 1: Normal operation - write 10 transactions
        {
            let mut wal = WAL::open(&wal_path, DurabilityMode::Strict).unwrap();
            let storage = UnifiedStore::new();

            for i in 1..=10u64 {
                let txn_id = i;
                let commit_version = i;

                // Write to WAL
                wal.append(&WALEntry::BeginTxn {
                    txn_id,
                    run_id,
                    timestamp: now(),
                })
                .unwrap();
                wal.append(&WALEntry::Write {
                    run_id,
                    key: Key::new_kv(ns.clone(), &format!("key{}", i)),
                    value: Value::I64(i as i64 * 10),
                    version: commit_version,
                })
                .unwrap();
                wal.append(&WALEntry::CommitTxn { txn_id, run_id })
                    .unwrap();

                // Apply to storage (simulating what would happen in normal operation)
                storage
                    .put_with_version(
                        Key::new_kv(ns.clone(), &format!("key{}", i)),
                        Value::I64(i as i64 * 10),
                        commit_version,
                        None,
                    )
                    .unwrap();
            }

            // Verify state before "crash"
            assert_eq!(storage.current_version(), 10);
        }

        // Phase 2: Simulate crash (scope dropped, storage gone)

        // Phase 3: Recovery
        let coordinator = RecoveryCoordinator::new(wal_path);
        let result = coordinator.recover().unwrap();

        // Verify recovered state matches original
        assert_eq!(result.stats.txns_replayed, 10);
        assert_eq!(result.stats.final_version, 10);
        assert_eq!(result.txn_manager.current_version(), 10);

        for i in 1..=10u64 {
            let key = Key::new_kv(ns.clone(), &format!("key{}", i));
            let stored = result.storage.get(&key).unwrap().unwrap();
            assert_eq!(stored.value, Value::I64(i as i64 * 10));
            assert_eq!(stored.version, i);
        }
    }

    /// Test recovery from WAL with mixed operations
    /// Writes, updates, and deletes
    #[test]
    fn test_recovery_mixed_operations_lifecycle() {
        let temp_dir = TempDir::new().unwrap();
        let wal_path = temp_dir.path().join("mixed_ops.wal");

        let run_id = RunId::new();
        let ns = create_test_namespace(run_id);

        {
            let mut wal = WAL::open(&wal_path, DurabilityMode::Strict).unwrap();

            // Txn 1: Write key1
            wal.append(&WALEntry::BeginTxn {
                txn_id: 1,
                run_id,
                timestamp: now(),
            })
            .unwrap();
            wal.append(&WALEntry::Write {
                run_id,
                key: Key::new_kv(ns.clone(), "key1"),
                value: Value::String("initial".to_string()),
                version: 1,
            })
            .unwrap();
            wal.append(&WALEntry::CommitTxn { txn_id: 1, run_id })
                .unwrap();

            // Txn 2: Update key1
            wal.append(&WALEntry::BeginTxn {
                txn_id: 2,
                run_id,
                timestamp: now(),
            })
            .unwrap();
            wal.append(&WALEntry::Write {
                run_id,
                key: Key::new_kv(ns.clone(), "key1"),
                value: Value::String("updated".to_string()),
                version: 2,
            })
            .unwrap();
            wal.append(&WALEntry::CommitTxn { txn_id: 2, run_id })
                .unwrap();

            // Txn 3: Write key2, then delete it
            wal.append(&WALEntry::BeginTxn {
                txn_id: 3,
                run_id,
                timestamp: now(),
            })
            .unwrap();
            wal.append(&WALEntry::Write {
                run_id,
                key: Key::new_kv(ns.clone(), "key2"),
                value: Value::String("temp".to_string()),
                version: 3,
            })
            .unwrap();
            wal.append(&WALEntry::Delete {
                run_id,
                key: Key::new_kv(ns.clone(), "key2"),
                version: 4,
            })
            .unwrap();
            wal.append(&WALEntry::CommitTxn { txn_id: 3, run_id })
                .unwrap();

            // Txn 4: Write key3
            wal.append(&WALEntry::BeginTxn {
                txn_id: 4,
                run_id,
                timestamp: now(),
            })
            .unwrap();
            wal.append(&WALEntry::Write {
                run_id,
                key: Key::new_kv(ns.clone(), "key3"),
                value: Value::I64(42),
                version: 5,
            })
            .unwrap();
            wal.append(&WALEntry::CommitTxn { txn_id: 4, run_id })
                .unwrap();
        }

        // Recovery
        let coordinator = RecoveryCoordinator::new(wal_path);
        let result = coordinator.recover().unwrap();

        assert_eq!(result.stats.txns_replayed, 4);
        assert_eq!(result.stats.writes_applied, 4);
        assert_eq!(result.stats.deletes_applied, 1);
        assert_eq!(result.stats.final_version, 5);

        // key1 should be "updated" at version 2
        let key1 = result
            .storage
            .get(&Key::new_kv(ns.clone(), "key1"))
            .unwrap()
            .unwrap();
        assert_eq!(key1.value, Value::String("updated".to_string()));
        assert_eq!(key1.version, 2);

        // key2 should be deleted
        assert!(result
            .storage
            .get(&Key::new_kv(ns.clone(), "key2"))
            .unwrap()
            .is_none());

        // key3 should exist
        let key3 = result
            .storage
            .get(&Key::new_kv(ns.clone(), "key3"))
            .unwrap()
            .unwrap();
        assert_eq!(key3.value, Value::I64(42));
        assert_eq!(key3.version, 5);
    }

    /// Test recovery maintains transaction ordering
    #[test]
    fn test_recovery_maintains_transaction_order() {
        let temp_dir = TempDir::new().unwrap();
        let wal_path = temp_dir.path().join("order.wal");

        let run_id = RunId::new();
        let ns = create_test_namespace(run_id);

        {
            let mut wal = WAL::open(&wal_path, DurabilityMode::Strict).unwrap();

            // Write same key in multiple transactions
            for v in [100u64, 200, 300] {
                wal.append(&WALEntry::BeginTxn {
                    txn_id: v,
                    run_id,
                    timestamp: now(),
                })
                .unwrap();
                wal.append(&WALEntry::Write {
                    run_id,
                    key: Key::new_kv(ns.clone(), "counter"),
                    value: Value::I64(v as i64),
                    version: v,
                })
                .unwrap();
                wal.append(&WALEntry::CommitTxn { txn_id: v, run_id })
                    .unwrap();
            }
        }

        // Recovery
        let coordinator = RecoveryCoordinator::new(wal_path);
        let result = coordinator.recover().unwrap();

        // Final value should be from the last transaction (version 300)
        let counter = result
            .storage
            .get(&Key::new_kv(ns.clone(), "counter"))
            .unwrap()
            .unwrap();
        assert_eq!(counter.value, Value::I64(300));
        assert_eq!(counter.version, 300);
    }

    /// Test recovery with new transactions after recovery
    #[test]
    fn test_new_transactions_after_recovery() {
        let temp_dir = TempDir::new().unwrap();
        let wal_path = temp_dir.path().join("post_recovery.wal");

        let run_id = RunId::new();
        let ns = create_test_namespace(run_id);

        // Phase 1: Create initial state
        {
            let mut wal = WAL::open(&wal_path, DurabilityMode::Strict).unwrap();

            wal.append(&WALEntry::BeginTxn {
                txn_id: 1,
                run_id,
                timestamp: now(),
            })
            .unwrap();
            wal.append(&WALEntry::Write {
                run_id,
                key: Key::new_kv(ns.clone(), "existing"),
                value: Value::I64(100),
                version: 100,
            })
            .unwrap();
            wal.append(&WALEntry::CommitTxn { txn_id: 1, run_id })
                .unwrap();
        }

        // Phase 2: Recovery
        let coordinator = RecoveryCoordinator::new(wal_path);
        let result = coordinator.recover().unwrap();

        // Phase 3: Verify TransactionManager is ready for new transactions
        assert_eq!(result.txn_manager.current_version(), 100);

        // New transaction should get version > 100
        let new_txn_id = result.txn_manager.next_txn_id();
        assert!(new_txn_id > 0);

        // Current version is 100, new transactions would use versions starting at 101
        assert_eq!(result.txn_manager.current_version(), 100);
    }

    /// Test recovery handles many transactions
    #[test]
    fn test_recovery_many_transactions() {
        let temp_dir = TempDir::new().unwrap();
        let wal_path = temp_dir.path().join("many_txns.wal");

        let run_id = RunId::new();
        let ns = create_test_namespace(run_id);

        let num_txns = 100;

        {
            let mut wal = WAL::open(&wal_path, DurabilityMode::Strict).unwrap();

            for i in 1..=num_txns {
                wal.append(&WALEntry::BeginTxn {
                    txn_id: i,
                    run_id,
                    timestamp: now(),
                })
                .unwrap();
                wal.append(&WALEntry::Write {
                    run_id,
                    key: Key::new_kv(ns.clone(), &format!("key_{}", i)),
                    value: Value::I64(i as i64),
                    version: i,
                })
                .unwrap();
                wal.append(&WALEntry::CommitTxn { txn_id: i, run_id })
                    .unwrap();
            }
        }

        // Recovery
        let coordinator = RecoveryCoordinator::new(wal_path);
        let result = coordinator.recover().unwrap();

        assert_eq!(result.stats.txns_replayed, num_txns as usize);
        assert_eq!(result.stats.final_version, num_txns);

        // Verify a few random keys
        for i in [1, 50, 100] {
            let key = Key::new_kv(ns.clone(), &format!("key_{}", i));
            let stored = result.storage.get(&key).unwrap().unwrap();
            assert_eq!(stored.value, Value::I64(i as i64));
        }
    }

    /// Spec compliance verification test
    #[test]
    fn test_spec_compliance_summary() {
        // This test documents spec compliance for Epic 9 review
        let temp_dir = TempDir::new().unwrap();
        let wal_path = temp_dir.path().join("spec.wal");

        let run_id = RunId::new();
        let ns = create_test_namespace(run_id);

        // Create a representative WAL
        {
            let mut wal = WAL::open(&wal_path, DurabilityMode::Strict).unwrap();

            // Committed transaction (version 100)
            wal.append(&WALEntry::BeginTxn {
                txn_id: 1,
                run_id,
                timestamp: now(),
            })
            .unwrap();
            wal.append(&WALEntry::Write {
                run_id,
                key: Key::new_kv(ns.clone(), "committed"),
                value: Value::I64(1),
                version: 100,
            })
            .unwrap();
            wal.append(&WALEntry::CommitTxn { txn_id: 1, run_id })
                .unwrap();

            // Incomplete transaction (no commit)
            wal.append(&WALEntry::BeginTxn {
                txn_id: 2,
                run_id,
                timestamp: now(),
            })
            .unwrap();
            wal.append(&WALEntry::Write {
                run_id,
                key: Key::new_kv(ns.clone(), "incomplete"),
                value: Value::I64(2),
                version: 200,
            })
            .unwrap();
            // No CommitTxn - represents crash
        }

        // Recovery
        let coordinator = RecoveryCoordinator::new(wal_path.clone());
        let result = coordinator.recover().unwrap();

        // Spec Section 5.4: COMPLETE transactions applied
        assert_eq!(result.stats.txns_replayed, 1, "Rule: COMPLETE txns applied");

        // Spec Section 5.5: INCOMPLETE transactions discarded
        assert_eq!(
            result.stats.incomplete_txns, 1,
            "Rule: INCOMPLETE txns discarded"
        );

        // Spec Section 5.3 Rule 4: Versions preserved exactly
        let committed = result
            .storage
            .get(&Key::new_kv(ns.clone(), "committed"))
            .unwrap()
            .unwrap();
        assert_eq!(committed.version, 100, "Rule: Versions preserved exactly");

        // Spec Section 6.1: Global version counter restored
        // NOTE: Version counter is set to MAX version seen in WAL (including incomplete txns)
        // This ensures new transactions get unique versions that don't conflict
        assert_eq!(
            result.txn_manager.current_version(),
            200, // Max version seen in WAL (from incomplete txn)
            "Rule: Global version counter restored to max WAL version"
        );

        // Spec Section 5.6: Determinism - recover again and verify identical
        let coordinator2 = RecoveryCoordinator::new(wal_path);
        let result2 = coordinator2.recover().unwrap();
        assert_eq!(
            result.stats.final_version, result2.stats.final_version,
            "Rule: Deterministic replay"
        );
    }
}
