//! WAL Replay Logic for Recovery
//!
//! This module implements WAL replay to restore storage state from the write-ahead log.
//! It scans WAL entries, groups them by transaction ID, and applies only committed
//! transactions to storage.
//!
//! ## Replay Process
//!
//! 1. Scan WAL entries from the beginning
//! 2. Group entries by txn_id into Transaction structs
//! 3. Identify committed transactions (those with CommitTxn entry)
//! 4. Discard incomplete transactions (BeginTxn without CommitTxn - crashed mid-transaction)
//! 5. Apply committed transactions in order to storage
//! 6. Preserve version numbers from WAL (don't allocate new versions)
//!
//! ## Version Preservation
//!
//! CRITICAL: Replay must preserve the exact version numbers from WAL entries.
//! This ensures that after replay, the database state is identical to what it was
//! before the crash. We use `put_with_version()` and `delete_with_version()` to
//! bypass normal version allocation.

use crate::wal::{WALEntry, WAL};
use in_mem_core::error::Result;
use in_mem_core::types::RunId;
use in_mem_storage::UnifiedStore;
use std::collections::HashMap;

/// Statistics from WAL replay
///
/// Tracks how many transactions and operations were applied during replay,
/// useful for debugging and monitoring recovery performance.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct ReplayStats {
    /// Number of committed transactions that were applied
    pub txns_applied: usize,
    /// Number of Write operations applied
    pub writes_applied: usize,
    /// Number of Delete operations applied
    pub deletes_applied: usize,
    /// Final version after replay (highest version seen in WAL)
    pub final_version: u64,
    /// Number of incomplete transactions discarded (no CommitTxn)
    pub incomplete_txns: usize,
    /// Number of aborted transactions discarded
    pub aborted_txns: usize,
}

/// Transaction state during replay
///
/// Groups WAL entries belonging to a single transaction.
/// A transaction is committed only if it has a CommitTxn entry.
#[derive(Debug)]
struct Transaction {
    /// Transaction identifier (stored for debugging via Debug trait)
    #[allow(dead_code)]
    txn_id: u64,
    /// Run this transaction belongs to (stored for debugging via Debug trait)
    #[allow(dead_code)]
    run_id: RunId,
    /// Entries in this transaction (in order)
    entries: Vec<WALEntry>,
    /// Whether this transaction was committed
    committed: bool,
    /// Whether this transaction was aborted
    aborted: bool,
}

impl Transaction {
    /// Create a new transaction from a BeginTxn entry
    fn new(txn_id: u64, run_id: RunId) -> Self {
        Self {
            txn_id,
            run_id,
            entries: Vec::new(),
            committed: false,
            aborted: false,
        }
    }
}

/// Replay WAL entries to restore storage state
///
/// This is the main recovery function. It scans all WAL entries, groups them
/// by transaction, and applies only committed transactions to storage.
///
/// # Arguments
///
/// * `wal` - The WAL to replay from
/// * `storage` - The storage to apply transactions to (must be empty or at checkpoint)
///
/// # Returns
///
/// * `Ok(ReplayStats)` - Statistics about the replay
/// * `Err` - If reading WAL or applying transactions fails
///
/// # Example
///
/// ```ignore
/// use in_mem_durability::recovery::replay_wal;
/// use in_mem_durability::wal::{WAL, DurabilityMode};
/// use in_mem_storage::UnifiedStore;
///
/// let wal = WAL::open("data/wal/segment.wal", DurabilityMode::default())?;
/// let storage = UnifiedStore::new();
/// let stats = replay_wal(&wal, &storage)?;
/// println!("Applied {} transactions, {} writes", stats.txns_applied, stats.writes_applied);
/// ```
pub fn replay_wal(wal: &WAL, storage: &UnifiedStore) -> Result<ReplayStats> {
    // Read all entries from WAL
    let entries = wal.read_all()?;

    // Group entries by transaction
    let mut transactions: HashMap<u64, Transaction> = HashMap::new();
    // Track the currently active transaction for each run_id
    // When a BeginTxn comes in, it becomes the active transaction for that run_id
    let mut active_txn_per_run: HashMap<RunId, u64> = HashMap::new();
    let mut max_version: u64 = 0;

    for entry in entries {
        // Track max version for final_version stat
        if let Some(version) = entry.version() {
            max_version = max_version.max(version);
        }

        match &entry {
            WALEntry::BeginTxn { txn_id, run_id, .. } => {
                // Start a new transaction and make it the active one for this run_id
                transactions.insert(*txn_id, Transaction::new(*txn_id, *run_id));
                active_txn_per_run.insert(*run_id, *txn_id);
            }
            WALEntry::Write { run_id, .. } | WALEntry::Delete { run_id, .. } => {
                // Add to the currently active transaction for this run_id
                if let Some(&active_txn_id) = active_txn_per_run.get(run_id) {
                    if let Some(txn) = transactions.get_mut(&active_txn_id) {
                        txn.entries.push(entry.clone());
                    }
                }
                // If no active transaction for this run_id, skip the entry (orphaned)
            }
            WALEntry::CommitTxn { txn_id, run_id } => {
                // Mark transaction as committed and clear active status
                if let Some(txn) = transactions.get_mut(txn_id) {
                    txn.committed = true;
                }
                // Clear active transaction for this run_id
                if active_txn_per_run.get(run_id) == Some(txn_id) {
                    active_txn_per_run.remove(run_id);
                }
            }
            WALEntry::AbortTxn { txn_id, run_id } => {
                // Mark transaction as aborted and clear active status
                if let Some(txn) = transactions.get_mut(txn_id) {
                    txn.aborted = true;
                }
                // Clear active transaction for this run_id
                if active_txn_per_run.get(run_id) == Some(txn_id) {
                    active_txn_per_run.remove(run_id);
                }
            }
            WALEntry::Checkpoint { version, .. } => {
                // Track checkpoint version
                max_version = max_version.max(*version);
            }
        }
    }

    // Apply committed transactions and collect stats
    let mut stats = ReplayStats {
        final_version: max_version,
        ..Default::default()
    };

    // Sort transactions by txn_id to ensure deterministic replay order
    let mut txn_ids: Vec<u64> = transactions.keys().copied().collect();
    txn_ids.sort();

    for txn_id in txn_ids {
        let txn = transactions.get(&txn_id).unwrap();

        if txn.committed {
            apply_transaction(storage, txn, &mut stats)?;
        } else if txn.aborted {
            stats.aborted_txns += 1;
        } else {
            // Incomplete transaction (no CommitTxn or AbortTxn)
            stats.incomplete_txns += 1;
        }
    }

    Ok(stats)
}

/// Apply a committed transaction to storage
///
/// Applies all Write and Delete operations from a transaction to storage,
/// preserving the version numbers from the WAL entries.
///
/// # Arguments
///
/// * `storage` - The storage to apply to
/// * `txn` - The committed transaction to apply
/// * `stats` - Statistics to update
///
/// # Returns
///
/// * `Ok(())` - If all operations succeeded
/// * `Err` - If any operation fails
fn apply_transaction(
    storage: &UnifiedStore,
    txn: &Transaction,
    stats: &mut ReplayStats,
) -> Result<()> {
    for entry in &txn.entries {
        match entry {
            WALEntry::Write {
                key,
                value,
                version,
                ..
            } => {
                // Apply write, preserving version from WAL
                storage.put_with_version(key.clone(), value.clone(), *version)?;
                stats.writes_applied += 1;
                stats.final_version = stats.final_version.max(*version);
            }
            WALEntry::Delete { key, version, .. } => {
                // Apply delete, preserving version from WAL
                storage.delete_with_version(key, *version)?;
                stats.deletes_applied += 1;
                stats.final_version = stats.final_version.max(*version);
            }
            _ => {
                // BeginTxn, CommitTxn, etc. are not applied to storage
            }
        }
    }

    stats.txns_applied += 1;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wal::DurabilityMode;
    use chrono::Utc;
    use in_mem_core::types::{Key, Namespace};
    use in_mem_core::value::Value;
    use in_mem_core::Storage; // Need trait in scope for .get() and .current_version()
    use tempfile::TempDir;

    /// Helper to get current timestamp
    fn now() -> i64 {
        Utc::now().timestamp()
    }

    #[test]
    fn test_replay_empty_wal() {
        let temp_dir = TempDir::new().unwrap();
        let wal_path = temp_dir.path().join("empty.wal");

        let wal = WAL::open(&wal_path, DurabilityMode::Strict).unwrap();
        let store = UnifiedStore::new();

        let stats = replay_wal(&wal, &store).unwrap();

        assert_eq!(stats.txns_applied, 0);
        assert_eq!(stats.writes_applied, 0);
        assert_eq!(stats.deletes_applied, 0);
        assert_eq!(stats.incomplete_txns, 0);
        assert_eq!(stats.aborted_txns, 0);
    }

    #[test]
    fn test_replay_committed_transaction() {
        let temp_dir = TempDir::new().unwrap();
        let wal_path = temp_dir.path().join("committed.wal");

        let run_id = RunId::new();
        let ns = Namespace::new(
            "tenant".to_string(),
            "app".to_string(),
            "agent".to_string(),
            run_id,
        );

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
                key: Key::new_kv(ns.clone(), "key1"),
                value: Value::Bytes(b"value1".to_vec()),
                version: 1,
            })
            .unwrap();

            wal.append(&WALEntry::Write {
                run_id,
                key: Key::new_kv(ns.clone(), "key2"),
                value: Value::String("value2".to_string()),
                version: 2,
            })
            .unwrap();

            wal.append(&WALEntry::CommitTxn { txn_id: 1, run_id })
                .unwrap();
        }

        // Replay to storage
        let wal = WAL::open(&wal_path, DurabilityMode::Strict).unwrap();
        let store = UnifiedStore::new();
        let stats = replay_wal(&wal, &store).unwrap();

        assert_eq!(stats.txns_applied, 1);
        assert_eq!(stats.writes_applied, 2);
        assert_eq!(stats.deletes_applied, 0);
        assert_eq!(stats.incomplete_txns, 0);
        assert_eq!(stats.final_version, 2);

        // Verify storage has data with correct versions
        let key1 = Key::new_kv(ns.clone(), "key1");
        let val1 = store.get(&key1).unwrap().unwrap();
        assert_eq!(val1.value, Value::Bytes(b"value1".to_vec()));
        assert_eq!(val1.version, 1);

        let key2 = Key::new_kv(ns.clone(), "key2");
        let val2 = store.get(&key2).unwrap().unwrap();
        assert_eq!(val2.value, Value::String("value2".to_string()));
        assert_eq!(val2.version, 2);
    }

    #[test]
    fn test_replay_discards_incomplete_transaction() {
        let temp_dir = TempDir::new().unwrap();
        let wal_path = temp_dir.path().join("incomplete.wal");

        let run_id = RunId::new();
        let ns = Namespace::new(
            "tenant".to_string(),
            "app".to_string(),
            "agent".to_string(),
            run_id,
        );

        // Write incomplete transaction (no CommitTxn - simulates crash)
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
                value: Value::Bytes(b"value1".to_vec()),
                version: 1,
            })
            .unwrap();

            // NO CommitTxn - simulates crash
        }

        // Replay to storage
        let wal = WAL::open(&wal_path, DurabilityMode::Strict).unwrap();
        let store = UnifiedStore::new();
        let stats = replay_wal(&wal, &store).unwrap();

        assert_eq!(stats.txns_applied, 0);
        assert_eq!(stats.writes_applied, 0);
        assert_eq!(stats.incomplete_txns, 1);

        // Storage should be empty
        let key1 = Key::new_kv(ns, "key1");
        assert!(store.get(&key1).unwrap().is_none());
    }

    #[test]
    fn test_replay_discards_aborted_transaction() {
        let temp_dir = TempDir::new().unwrap();
        let wal_path = temp_dir.path().join("aborted.wal");

        let run_id = RunId::new();
        let ns = Namespace::new(
            "tenant".to_string(),
            "app".to_string(),
            "agent".to_string(),
            run_id,
        );

        // Write aborted transaction
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
                value: Value::Bytes(b"value1".to_vec()),
                version: 1,
            })
            .unwrap();

            wal.append(&WALEntry::AbortTxn { txn_id: 1, run_id })
                .unwrap();
        }

        // Replay to storage
        let wal = WAL::open(&wal_path, DurabilityMode::Strict).unwrap();
        let store = UnifiedStore::new();
        let stats = replay_wal(&wal, &store).unwrap();

        assert_eq!(stats.txns_applied, 0);
        assert_eq!(stats.writes_applied, 0);
        assert_eq!(stats.aborted_txns, 1);
        assert_eq!(stats.incomplete_txns, 0);

        // Storage should be empty
        let key1 = Key::new_kv(ns, "key1");
        assert!(store.get(&key1).unwrap().is_none());
    }

    #[test]
    fn test_replay_multiple_transactions() {
        let temp_dir = TempDir::new().unwrap();
        let wal_path = temp_dir.path().join("multi.wal");

        let run_id = RunId::new();
        let ns = Namespace::new(
            "tenant".to_string(),
            "app".to_string(),
            "agent".to_string(),
            run_id,
        );

        // Write 3 transactions: 2 committed, 1 incomplete
        {
            let mut wal = WAL::open(&wal_path, DurabilityMode::Strict).unwrap();

            // Txn 1 - committed
            wal.append(&WALEntry::BeginTxn {
                txn_id: 1,
                run_id,
                timestamp: now(),
            })
            .unwrap();
            wal.append(&WALEntry::Write {
                run_id,
                key: Key::new_kv(ns.clone(), "key1"),
                value: Value::Bytes(b"v1".to_vec()),
                version: 1,
            })
            .unwrap();
            wal.append(&WALEntry::CommitTxn { txn_id: 1, run_id })
                .unwrap();

            // Txn 2 - incomplete (no commit)
            wal.append(&WALEntry::BeginTxn {
                txn_id: 2,
                run_id,
                timestamp: now(),
            })
            .unwrap();
            wal.append(&WALEntry::Write {
                run_id,
                key: Key::new_kv(ns.clone(), "key2"),
                value: Value::Bytes(b"v2".to_vec()),
                version: 2,
            })
            .unwrap();
            // NO CommitTxn

            // Txn 3 - committed
            wal.append(&WALEntry::BeginTxn {
                txn_id: 3,
                run_id,
                timestamp: now(),
            })
            .unwrap();
            wal.append(&WALEntry::Write {
                run_id,
                key: Key::new_kv(ns.clone(), "key3"),
                value: Value::Bytes(b"v3".to_vec()),
                version: 3,
            })
            .unwrap();
            wal.append(&WALEntry::CommitTxn { txn_id: 3, run_id })
                .unwrap();
        }

        // Replay
        let wal = WAL::open(&wal_path, DurabilityMode::Strict).unwrap();
        let store = UnifiedStore::new();
        let stats = replay_wal(&wal, &store).unwrap();

        assert_eq!(stats.txns_applied, 2); // Txn 1 and 3
        assert_eq!(stats.writes_applied, 2);
        assert_eq!(stats.incomplete_txns, 1); // Txn 2

        // Verify key1 and key3 exist, key2 doesn't
        assert!(store
            .get(&Key::new_kv(ns.clone(), "key1"))
            .unwrap()
            .is_some());
        assert!(store
            .get(&Key::new_kv(ns.clone(), "key2"))
            .unwrap()
            .is_none());
        assert!(store
            .get(&Key::new_kv(ns.clone(), "key3"))
            .unwrap()
            .is_some());
    }

    #[test]
    fn test_replay_with_deletes() {
        let temp_dir = TempDir::new().unwrap();
        let wal_path = temp_dir.path().join("deletes.wal");

        let run_id = RunId::new();
        let ns = Namespace::new(
            "tenant".to_string(),
            "app".to_string(),
            "agent".to_string(),
            run_id,
        );

        // Write transaction with write then delete
        {
            let mut wal = WAL::open(&wal_path, DurabilityMode::Strict).unwrap();

            wal.append(&WALEntry::BeginTxn {
                txn_id: 1,
                run_id,
                timestamp: now(),
            })
            .unwrap();

            // Write then delete same key
            wal.append(&WALEntry::Write {
                run_id,
                key: Key::new_kv(ns.clone(), "key1"),
                value: Value::Bytes(b"v1".to_vec()),
                version: 1,
            })
            .unwrap();

            wal.append(&WALEntry::Delete {
                run_id,
                key: Key::new_kv(ns.clone(), "key1"),
                version: 2,
            })
            .unwrap();

            wal.append(&WALEntry::CommitTxn { txn_id: 1, run_id })
                .unwrap();
        }

        // Replay
        let wal = WAL::open(&wal_path, DurabilityMode::Strict).unwrap();
        let store = UnifiedStore::new();
        let stats = replay_wal(&wal, &store).unwrap();

        assert_eq!(stats.txns_applied, 1);
        assert_eq!(stats.writes_applied, 1);
        assert_eq!(stats.deletes_applied, 1);

        // Key should be deleted (final state)
        assert!(store.get(&Key::new_kv(ns, "key1")).unwrap().is_none());
    }

    #[test]
    fn test_replay_preserves_versions() {
        let temp_dir = TempDir::new().unwrap();
        let wal_path = temp_dir.path().join("versions.wal");

        let run_id = RunId::new();
        let ns = Namespace::new(
            "tenant".to_string(),
            "app".to_string(),
            "agent".to_string(),
            run_id,
        );

        // Write with specific versions (not 1, 2, 3 but 100, 200, 300)
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
                value: Value::I64(100),
                version: 100, // Non-sequential version
            })
            .unwrap();

            wal.append(&WALEntry::Write {
                run_id,
                key: Key::new_kv(ns.clone(), "key2"),
                value: Value::I64(200),
                version: 200,
            })
            .unwrap();

            wal.append(&WALEntry::Write {
                run_id,
                key: Key::new_kv(ns.clone(), "key3"),
                value: Value::I64(300),
                version: 300,
            })
            .unwrap();

            wal.append(&WALEntry::CommitTxn { txn_id: 1, run_id })
                .unwrap();
        }

        // Replay
        let wal = WAL::open(&wal_path, DurabilityMode::Strict).unwrap();
        let store = UnifiedStore::new();
        let stats = replay_wal(&wal, &store).unwrap();

        assert_eq!(stats.final_version, 300);

        // Verify versions are preserved exactly
        let key1 = Key::new_kv(ns.clone(), "key1");
        let val1 = store.get(&key1).unwrap().unwrap();
        assert_eq!(val1.version, 100); // Version preserved, not re-allocated

        let key2 = Key::new_kv(ns.clone(), "key2");
        let val2 = store.get(&key2).unwrap().unwrap();
        assert_eq!(val2.version, 200);

        let key3 = Key::new_kv(ns.clone(), "key3");
        let val3 = store.get(&key3).unwrap().unwrap();
        assert_eq!(val3.version, 300);

        // Global version should reflect max version from WAL
        assert_eq!(store.current_version(), 300);
    }
}
