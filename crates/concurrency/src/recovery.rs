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
}
