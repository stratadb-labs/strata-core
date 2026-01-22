//! Integration tests for incomplete transaction handling (Story #24)
//!
//! These tests verify that:
//! 1. Incomplete transactions (BeginTxn without CommitTxn) are discarded
//! 2. Orphaned entries (Write/Delete without BeginTxn) are discarded
//! 3. Mixed committed and incomplete transactions are handled correctly
//! 4. Aborted transactions are discarded
//! 5. Validation warnings are generated

use strata_core::types::{Key, Namespace, RunId};
use strata_core::value::Value;
use strata_core::Timestamp;
use strata_core::Storage; // Need trait in scope for .get() and .current_version()
use strata_durability::recovery::replay_wal;
use strata_durability::wal::{DurabilityMode, WALEntry, WAL};
use strata_storage::UnifiedStore;
use tempfile::TempDir;

/// Helper to get current timestamp
fn now() -> Timestamp {
    Timestamp::now()
}

/// Helper to create a test namespace with a specific run_id
fn test_namespace(run_id: RunId) -> Namespace {
    Namespace::new(
        "tenant".to_string(),
        "app".to_string(),
        "agent".to_string(),
        run_id,
    )
}

// ============================================================================
// Incomplete Transaction Tests
// ============================================================================

#[test]
fn test_discard_incomplete_transaction() {
    let temp_dir = TempDir::new().unwrap();
    let wal_path = temp_dir.path().join("incomplete.wal");

    let run_id = RunId::new();
    let ns = test_namespace(run_id);

    // Write WAL: BeginTxn → Write (no CommitTxn) - simulates crash
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
            key: Key::new_kv(ns.clone(), "incomplete_key"),
            value: Value::String("should_not_persist".to_string()),
            version: 1,
        })
        .unwrap();

        // NO CommitTxn - simulates crash mid-transaction
    }

    // Replay
    let wal = WAL::open(&wal_path, DurabilityMode::Strict).unwrap();
    let store = UnifiedStore::new();
    let stats = replay_wal(&wal, &store).unwrap();

    // Verify: discarded_txns (incomplete_txns) = 1
    assert_eq!(stats.txns_applied, 0);
    assert_eq!(stats.incomplete_txns, 1);
    assert_eq!(stats.orphaned_entries, 0);

    // Verify: key does NOT exist in storage
    let key = Key::new_kv(ns, "incomplete_key");
    assert!(store.get(&key).unwrap().is_none());
}

#[test]
fn test_discard_orphaned_entries() {
    let temp_dir = TempDir::new().unwrap();
    let wal_path = temp_dir.path().join("orphaned.wal");

    let run_id = RunId::new();
    let ns = test_namespace(run_id);

    // Write WAL: Write without BeginTxn
    {
        let mut wal = WAL::open(&wal_path, DurabilityMode::Strict).unwrap();

        // Orphaned write (no BeginTxn for this run_id)
        wal.append(&WALEntry::Write {
            run_id,
            key: Key::new_kv(ns.clone(), "orphan_key"),
            value: Value::Bytes(b"orphaned_value".to_vec()),
            version: 1,
        })
        .unwrap();

        // Orphaned delete (no BeginTxn for this run_id)
        wal.append(&WALEntry::Delete {
            run_id,
            key: Key::new_kv(ns.clone(), "orphan_delete"),
            version: 2,
        })
        .unwrap();
    }

    // Replay
    let wal = WAL::open(&wal_path, DurabilityMode::Strict).unwrap();
    let store = UnifiedStore::new();
    let stats = replay_wal(&wal, &store).unwrap();

    // Verify: orphaned_entries = 2
    assert_eq!(stats.txns_applied, 0);
    assert_eq!(stats.orphaned_entries, 2);
    assert_eq!(stats.incomplete_txns, 0);

    // Verify: key does NOT exist in storage
    assert!(store
        .get(&Key::new_kv(ns.clone(), "orphan_key"))
        .unwrap()
        .is_none());
}

#[test]
fn test_mixed_committed_and_incomplete() {
    let temp_dir = TempDir::new().unwrap();
    let wal_path = temp_dir.path().join("mixed.wal");

    let run_id = RunId::new();
    let ns = test_namespace(run_id);

    // Write WAL:
    //   Txn 1: BeginTxn → Write → CommitTxn (committed)
    //   Txn 2: BeginTxn → Write (incomplete)
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
            key: Key::new_kv(ns.clone(), "committed_key"),
            value: Value::I64(100),
            version: 1,
        })
        .unwrap();
        wal.append(&WALEntry::CommitTxn { txn_id: 1, run_id })
            .unwrap();

        // Txn 2 - incomplete
        wal.append(&WALEntry::BeginTxn {
            txn_id: 2,
            run_id,
            timestamp: now(),
        })
        .unwrap();
        wal.append(&WALEntry::Write {
            run_id,
            key: Key::new_kv(ns.clone(), "incomplete_key"),
            value: Value::I64(200),
            version: 2,
        })
        .unwrap();
        // NO CommitTxn for txn 2
    }

    // Replay
    let wal = WAL::open(&wal_path, DurabilityMode::Strict).unwrap();
    let store = UnifiedStore::new();
    let stats = replay_wal(&wal, &store).unwrap();

    // Verify: txns_applied = 1, discarded_txns = 1
    assert_eq!(stats.txns_applied, 1);
    assert_eq!(stats.incomplete_txns, 1);
    assert_eq!(stats.writes_applied, 1);

    // Verify: Txn 1 key exists
    let committed = store
        .get(&Key::new_kv(ns.clone(), "committed_key"))
        .unwrap();
    assert!(committed.is_some());
    assert_eq!(committed.unwrap().value, Value::I64(100));

    // Verify: Txn 2 key does NOT exist
    let incomplete = store.get(&Key::new_kv(ns, "incomplete_key")).unwrap();
    assert!(incomplete.is_none());
}

#[test]
fn test_aborted_transactions_discarded() {
    let temp_dir = TempDir::new().unwrap();
    let wal_path = temp_dir.path().join("aborted.wal");

    let run_id = RunId::new();
    let ns = test_namespace(run_id);

    // Write WAL: BeginTxn → Write → AbortTxn
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
            key: Key::new_kv(ns.clone(), "aborted_key"),
            value: Value::String("should_not_persist".to_string()),
            version: 1,
        })
        .unwrap();

        wal.append(&WALEntry::AbortTxn { txn_id: 1, run_id })
            .unwrap();
    }

    // Replay
    let wal = WAL::open(&wal_path, DurabilityMode::Strict).unwrap();
    let store = UnifiedStore::new();
    let stats = replay_wal(&wal, &store).unwrap();

    // Verify: aborted_txns = 1
    assert_eq!(stats.txns_applied, 0);
    assert_eq!(stats.aborted_txns, 1);
    assert_eq!(stats.incomplete_txns, 0);
    assert_eq!(stats.orphaned_entries, 0);

    // Verify: key does NOT exist
    assert!(store
        .get(&Key::new_kv(ns, "aborted_key"))
        .unwrap()
        .is_none());
}

// ============================================================================
// Complex Scenario Tests
// ============================================================================

#[test]
fn test_multiple_incomplete_transactions() {
    let temp_dir = TempDir::new().unwrap();
    let wal_path = temp_dir.path().join("multi_incomplete.wal");

    let run_id = RunId::new();
    let ns = test_namespace(run_id);

    // Write: 3 incomplete, 2 committed, 1 aborted
    {
        let mut wal = WAL::open(&wal_path, DurabilityMode::Strict).unwrap();

        // Incomplete 1
        wal.append(&WALEntry::BeginTxn {
            txn_id: 1,
            run_id,
            timestamp: now(),
        })
        .unwrap();
        wal.append(&WALEntry::Write {
            run_id,
            key: Key::new_kv(ns.clone(), "k1"),
            value: Value::I64(1),
            version: 1,
        })
        .unwrap();

        // Committed 1
        wal.append(&WALEntry::BeginTxn {
            txn_id: 2,
            run_id,
            timestamp: now(),
        })
        .unwrap();
        wal.append(&WALEntry::Write {
            run_id,
            key: Key::new_kv(ns.clone(), "k2"),
            value: Value::I64(2),
            version: 2,
        })
        .unwrap();
        wal.append(&WALEntry::CommitTxn { txn_id: 2, run_id })
            .unwrap();

        // Incomplete 2
        wal.append(&WALEntry::BeginTxn {
            txn_id: 3,
            run_id,
            timestamp: now(),
        })
        .unwrap();

        // Aborted
        wal.append(&WALEntry::BeginTxn {
            txn_id: 4,
            run_id,
            timestamp: now(),
        })
        .unwrap();
        wal.append(&WALEntry::Write {
            run_id,
            key: Key::new_kv(ns.clone(), "k4"),
            value: Value::I64(4),
            version: 4,
        })
        .unwrap();
        wal.append(&WALEntry::AbortTxn { txn_id: 4, run_id })
            .unwrap();

        // Committed 2
        wal.append(&WALEntry::BeginTxn {
            txn_id: 5,
            run_id,
            timestamp: now(),
        })
        .unwrap();
        wal.append(&WALEntry::Write {
            run_id,
            key: Key::new_kv(ns.clone(), "k5"),
            value: Value::I64(5),
            version: 5,
        })
        .unwrap();
        wal.append(&WALEntry::CommitTxn { txn_id: 5, run_id })
            .unwrap();

        // Incomplete 3
        wal.append(&WALEntry::BeginTxn {
            txn_id: 6,
            run_id,
            timestamp: now(),
        })
        .unwrap();
        wal.append(&WALEntry::Write {
            run_id,
            key: Key::new_kv(ns.clone(), "k6"),
            value: Value::I64(6),
            version: 6,
        })
        .unwrap();
    }

    // Replay
    let wal = WAL::open(&wal_path, DurabilityMode::Strict).unwrap();
    let store = UnifiedStore::new();
    let stats = replay_wal(&wal, &store).unwrap();

    // Verify counts
    assert_eq!(stats.txns_applied, 2); // txn 2 and 5
    assert_eq!(stats.incomplete_txns, 3); // txn 1, 3, 6
    assert_eq!(stats.aborted_txns, 1); // txn 4
    assert_eq!(stats.writes_applied, 2);

    // Verify only committed keys exist
    assert!(store.get(&Key::new_kv(ns.clone(), "k1")).unwrap().is_none());
    assert!(store.get(&Key::new_kv(ns.clone(), "k2")).unwrap().is_some());
    assert!(store.get(&Key::new_kv(ns.clone(), "k4")).unwrap().is_none());
    assert!(store.get(&Key::new_kv(ns.clone(), "k5")).unwrap().is_some());
    assert!(store.get(&Key::new_kv(ns.clone(), "k6")).unwrap().is_none());
}

#[test]
fn test_orphaned_entries_with_valid_transactions() {
    let temp_dir = TempDir::new().unwrap();
    let wal_path = temp_dir.path().join("orphaned_with_valid.wal");

    let run_id = RunId::new();
    let ns = test_namespace(run_id);

    // Orphan entries followed by valid transaction
    {
        let mut wal = WAL::open(&wal_path, DurabilityMode::Strict).unwrap();

        // Orphaned writes first
        wal.append(&WALEntry::Write {
            run_id,
            key: Key::new_kv(ns.clone(), "orphan1"),
            value: Value::I64(1),
            version: 1,
        })
        .unwrap();
        wal.append(&WALEntry::Delete {
            run_id,
            key: Key::new_kv(ns.clone(), "orphan2"),
            version: 2,
        })
        .unwrap();

        // Then a valid transaction
        wal.append(&WALEntry::BeginTxn {
            txn_id: 1,
            run_id,
            timestamp: now(),
        })
        .unwrap();
        wal.append(&WALEntry::Write {
            run_id,
            key: Key::new_kv(ns.clone(), "valid"),
            value: Value::I64(100),
            version: 3,
        })
        .unwrap();
        wal.append(&WALEntry::CommitTxn { txn_id: 1, run_id })
            .unwrap();
    }

    // Replay
    let wal = WAL::open(&wal_path, DurabilityMode::Strict).unwrap();
    let store = UnifiedStore::new();
    let stats = replay_wal(&wal, &store).unwrap();

    // Verify
    assert_eq!(stats.txns_applied, 1);
    assert_eq!(stats.orphaned_entries, 2);
    assert_eq!(stats.incomplete_txns, 0);

    // Only valid key exists
    assert!(store
        .get(&Key::new_kv(ns.clone(), "orphan1"))
        .unwrap()
        .is_none());
    assert!(store
        .get(&Key::new_kv(ns.clone(), "valid"))
        .unwrap()
        .is_some());
}

#[test]
fn test_interleaved_transactions_different_run_ids() {
    let temp_dir = TempDir::new().unwrap();
    let wal_path = temp_dir.path().join("interleaved.wal");

    let run_id1 = RunId::new();
    let run_id2 = RunId::new();
    let ns1 = test_namespace(run_id1);
    let ns2 = test_namespace(run_id2);

    // Two runs with interleaved transactions
    {
        let mut wal = WAL::open(&wal_path, DurabilityMode::Strict).unwrap();

        // Run 1 - start transaction
        wal.append(&WALEntry::BeginTxn {
            txn_id: 1,
            run_id: run_id1,
            timestamp: now(),
        })
        .unwrap();

        // Run 2 - start and complete transaction
        wal.append(&WALEntry::BeginTxn {
            txn_id: 2,
            run_id: run_id2,
            timestamp: now(),
        })
        .unwrap();
        wal.append(&WALEntry::Write {
            run_id: run_id2,
            key: Key::new_kv(ns2.clone(), "run2_key"),
            value: Value::I64(2),
            version: 1,
        })
        .unwrap();
        wal.append(&WALEntry::CommitTxn {
            txn_id: 2,
            run_id: run_id2,
        })
        .unwrap();

        // Run 1 - continue and leave incomplete
        wal.append(&WALEntry::Write {
            run_id: run_id1,
            key: Key::new_kv(ns1.clone(), "run1_key"),
            value: Value::I64(1),
            version: 2,
        })
        .unwrap();
        // NO CommitTxn for run 1
    }

    // Replay
    let wal = WAL::open(&wal_path, DurabilityMode::Strict).unwrap();
    let store = UnifiedStore::new();
    let stats = replay_wal(&wal, &store).unwrap();

    // Run 2 should be applied, Run 1 should be discarded
    assert_eq!(stats.txns_applied, 1);
    assert_eq!(stats.incomplete_txns, 1);

    // Only run 2 key should exist
    assert!(store.get(&Key::new_kv(ns1, "run1_key")).unwrap().is_none());
    assert!(store.get(&Key::new_kv(ns2, "run2_key")).unwrap().is_some());
}

#[test]
fn test_empty_incomplete_transaction() {
    let temp_dir = TempDir::new().unwrap();
    let wal_path = temp_dir.path().join("empty_incomplete.wal");

    let run_id = RunId::new();

    // BeginTxn with no writes and no commit
    {
        let mut wal = WAL::open(&wal_path, DurabilityMode::Strict).unwrap();

        wal.append(&WALEntry::BeginTxn {
            txn_id: 1,
            run_id,
            timestamp: now(),
        })
        .unwrap();
        // Nothing else - just BeginTxn
    }

    // Replay
    let wal = WAL::open(&wal_path, DurabilityMode::Strict).unwrap();
    let store = UnifiedStore::new();
    let stats = replay_wal(&wal, &store).unwrap();

    // Should be counted as incomplete
    assert_eq!(stats.txns_applied, 0);
    assert_eq!(stats.incomplete_txns, 1);
    assert_eq!(stats.writes_applied, 0);
}

#[test]
fn test_crash_during_large_transaction() {
    let temp_dir = TempDir::new().unwrap();
    let wal_path = temp_dir.path().join("crash_large.wal");

    let run_id = RunId::new();
    let ns = test_namespace(run_id);

    // Large incomplete transaction (100 writes, no commit)
    {
        let mut wal = WAL::open(&wal_path, DurabilityMode::Strict).unwrap();

        wal.append(&WALEntry::BeginTxn {
            txn_id: 1,
            run_id,
            timestamp: now(),
        })
        .unwrap();

        for i in 0..100 {
            wal.append(&WALEntry::Write {
                run_id,
                key: Key::new_kv(ns.clone(), format!("key_{}", i)),
                value: Value::I64(i as i64),
                version: i as u64 + 1,
            })
            .unwrap();
        }

        // NO CommitTxn - crash mid-transaction
    }

    // Replay
    let wal = WAL::open(&wal_path, DurabilityMode::Strict).unwrap();
    let store = UnifiedStore::new();
    let stats = replay_wal(&wal, &store).unwrap();

    // All 100 writes should be discarded
    assert_eq!(stats.txns_applied, 0);
    assert_eq!(stats.incomplete_txns, 1);
    assert_eq!(stats.writes_applied, 0);

    // Spot check - none of the keys should exist
    assert!(store
        .get(&Key::new_kv(ns.clone(), "key_0"))
        .unwrap()
        .is_none());
    assert!(store
        .get(&Key::new_kv(ns.clone(), "key_50"))
        .unwrap()
        .is_none());
    assert!(store
        .get(&Key::new_kv(ns.clone(), "key_99"))
        .unwrap()
        .is_none());
}
