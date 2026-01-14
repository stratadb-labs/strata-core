//! Integration tests for WAL replay logic
//!
//! These tests verify that the WAL replay mechanism correctly:
//! 1. Applies committed transactions
//! 2. Discards incomplete/aborted transactions
//! 3. Preserves version numbers from WAL entries
//! 4. Handles multiple transactions correctly

use chrono::Utc;
use in_mem_core::types::{Key, Namespace, RunId};
use in_mem_core::value::Value;
use in_mem_core::Storage; // Need trait in scope for .get() and .current_version()
use in_mem_durability::recovery::replay_wal;
use in_mem_durability::wal::{DurabilityMode, WALEntry, WAL};
use in_mem_storage::UnifiedStore;
use tempfile::TempDir;

/// Helper to get current timestamp
fn now() -> i64 {
    Utc::now().timestamp()
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
// Single Transaction Tests
// ============================================================================

#[test]
fn test_replay_single_committed_transaction() {
    let temp_dir = TempDir::new().unwrap();
    let wal_path = temp_dir.path().join("single.wal");

    let run_id = RunId::new();
    let ns = test_namespace(run_id);

    // Write a simple committed transaction
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
            key: Key::new_kv(ns.clone(), "hello"),
            value: Value::String("world".to_string()),
            version: 1,
        })
        .unwrap();

        wal.append(&WALEntry::CommitTxn { txn_id: 1, run_id })
            .unwrap();
    }

    // Replay to empty storage
    let wal = WAL::open(&wal_path, DurabilityMode::Strict).unwrap();
    let store = UnifiedStore::new();
    let stats = replay_wal(&wal, &store).unwrap();

    // Verify stats
    assert_eq!(stats.txns_applied, 1);
    assert_eq!(stats.writes_applied, 1);
    assert_eq!(stats.deletes_applied, 0);
    assert_eq!(stats.incomplete_txns, 0);
    assert_eq!(stats.aborted_txns, 0);

    // Verify storage has the correct data
    let key = Key::new_kv(ns, "hello");
    let result = store.get(&key).unwrap();
    assert!(result.is_some());
    let vv = result.unwrap();
    assert_eq!(vv.value, Value::String("world".to_string()));
    assert_eq!(vv.version, 1);
}

#[test]
fn test_replay_single_incomplete_transaction() {
    let temp_dir = TempDir::new().unwrap();
    let wal_path = temp_dir.path().join("incomplete.wal");

    let run_id = RunId::new();
    let ns = test_namespace(run_id);

    // Write an incomplete transaction (simulates crash before commit)
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
            key: Key::new_kv(ns.clone(), "crash_data"),
            value: Value::Bytes(b"should_not_persist".to_vec()),
            version: 1,
        })
        .unwrap();

        // NO CommitTxn - simulates crash
    }

    // Replay to empty storage
    let wal = WAL::open(&wal_path, DurabilityMode::Strict).unwrap();
    let store = UnifiedStore::new();
    let stats = replay_wal(&wal, &store).unwrap();

    // Verify stats - transaction should be discarded
    assert_eq!(stats.txns_applied, 0);
    assert_eq!(stats.writes_applied, 0);
    assert_eq!(stats.incomplete_txns, 1);

    // Verify storage is empty
    let key = Key::new_kv(ns, "crash_data");
    assert!(store.get(&key).unwrap().is_none());
}

// ============================================================================
// Multiple Transaction Tests
// ============================================================================

#[test]
fn test_replay_multiple_committed_transactions() {
    let temp_dir = TempDir::new().unwrap();
    let wal_path = temp_dir.path().join("multi_commit.wal");

    let run_id = RunId::new();
    let ns = test_namespace(run_id);

    // Write 3 committed transactions
    {
        let mut wal = WAL::open(&wal_path, DurabilityMode::Strict).unwrap();

        for i in 1..=3u64 {
            wal.append(&WALEntry::BeginTxn {
                txn_id: i,
                run_id,
                timestamp: now(),
            })
            .unwrap();

            wal.append(&WALEntry::Write {
                run_id,
                key: Key::new_kv(ns.clone(), format!("key{}", i)),
                value: Value::I64(i as i64 * 100),
                version: i,
            })
            .unwrap();

            wal.append(&WALEntry::CommitTxn { txn_id: i, run_id })
                .unwrap();
        }
    }

    // Replay
    let wal = WAL::open(&wal_path, DurabilityMode::Strict).unwrap();
    let store = UnifiedStore::new();
    let stats = replay_wal(&wal, &store).unwrap();

    // Verify stats
    assert_eq!(stats.txns_applied, 3);
    assert_eq!(stats.writes_applied, 3);
    assert_eq!(stats.final_version, 3);

    // Verify all keys exist with correct values
    for i in 1..=3u64 {
        let key = Key::new_kv(ns.clone(), format!("key{}", i));
        let result = store.get(&key).unwrap().unwrap();
        assert_eq!(result.value, Value::I64(i as i64 * 100));
        assert_eq!(result.version, i);
    }
}

#[test]
fn test_replay_mixed_committed_and_incomplete() {
    let temp_dir = TempDir::new().unwrap();
    let wal_path = temp_dir.path().join("mixed.wal");

    let run_id = RunId::new();
    let ns = test_namespace(run_id);

    // Write: committed, incomplete, committed, aborted, incomplete
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
            value: Value::String("v1".to_string()),
            version: 1,
        })
        .unwrap();
        wal.append(&WALEntry::CommitTxn { txn_id: 1, run_id })
            .unwrap();

        // Txn 2 - incomplete (crash simulation)
        wal.append(&WALEntry::BeginTxn {
            txn_id: 2,
            run_id,
            timestamp: now(),
        })
        .unwrap();
        wal.append(&WALEntry::Write {
            run_id,
            key: Key::new_kv(ns.clone(), "key2"),
            value: Value::String("v2".to_string()),
            version: 2,
        })
        .unwrap();
        // NO commit

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
            value: Value::String("v3".to_string()),
            version: 3,
        })
        .unwrap();
        wal.append(&WALEntry::CommitTxn { txn_id: 3, run_id })
            .unwrap();

        // Txn 4 - aborted
        wal.append(&WALEntry::BeginTxn {
            txn_id: 4,
            run_id,
            timestamp: now(),
        })
        .unwrap();
        wal.append(&WALEntry::Write {
            run_id,
            key: Key::new_kv(ns.clone(), "key4"),
            value: Value::String("v4".to_string()),
            version: 4,
        })
        .unwrap();
        wal.append(&WALEntry::AbortTxn { txn_id: 4, run_id })
            .unwrap();

        // Txn 5 - incomplete
        wal.append(&WALEntry::BeginTxn {
            txn_id: 5,
            run_id,
            timestamp: now(),
        })
        .unwrap();
        wal.append(&WALEntry::Write {
            run_id,
            key: Key::new_kv(ns.clone(), "key5"),
            value: Value::String("v5".to_string()),
            version: 5,
        })
        .unwrap();
        // NO commit
    }

    // Replay
    let wal = WAL::open(&wal_path, DurabilityMode::Strict).unwrap();
    let store = UnifiedStore::new();
    let stats = replay_wal(&wal, &store).unwrap();

    // Verify stats
    assert_eq!(stats.txns_applied, 2); // Txn 1 and 3
    assert_eq!(stats.writes_applied, 2);
    assert_eq!(stats.incomplete_txns, 2); // Txn 2 and 5
    assert_eq!(stats.aborted_txns, 1); // Txn 4

    // Verify only committed data exists
    assert!(store
        .get(&Key::new_kv(ns.clone(), "key1"))
        .unwrap()
        .is_some());
    assert!(store
        .get(&Key::new_kv(ns.clone(), "key2"))
        .unwrap()
        .is_none()); // Incomplete
    assert!(store
        .get(&Key::new_kv(ns.clone(), "key3"))
        .unwrap()
        .is_some());
    assert!(store
        .get(&Key::new_kv(ns.clone(), "key4"))
        .unwrap()
        .is_none()); // Aborted
    assert!(store
        .get(&Key::new_kv(ns.clone(), "key5"))
        .unwrap()
        .is_none()); // Incomplete
}

// ============================================================================
// Version Preservation Tests
// ============================================================================

#[test]
fn test_replay_preserves_exact_versions() {
    let temp_dir = TempDir::new().unwrap();
    let wal_path = temp_dir.path().join("versions.wal");

    let run_id = RunId::new();
    let ns = test_namespace(run_id);

    // Write with non-sequential versions (like after checkpoint)
    {
        let mut wal = WAL::open(&wal_path, DurabilityMode::Strict).unwrap();

        wal.append(&WALEntry::BeginTxn {
            txn_id: 1,
            run_id,
            timestamp: now(),
        })
        .unwrap();

        // Use specific versions that are not sequential
        wal.append(&WALEntry::Write {
            run_id,
            key: Key::new_kv(ns.clone(), "alpha"),
            value: Value::I64(111),
            version: 1000, // High version number
        })
        .unwrap();

        wal.append(&WALEntry::Write {
            run_id,
            key: Key::new_kv(ns.clone(), "beta"),
            value: Value::I64(222),
            version: 2000,
        })
        .unwrap();

        wal.append(&WALEntry::Write {
            run_id,
            key: Key::new_kv(ns.clone(), "gamma"),
            value: Value::I64(333),
            version: 3000,
        })
        .unwrap();

        wal.append(&WALEntry::CommitTxn { txn_id: 1, run_id })
            .unwrap();
    }

    // Replay
    let wal = WAL::open(&wal_path, DurabilityMode::Strict).unwrap();
    let store = UnifiedStore::new();
    let stats = replay_wal(&wal, &store).unwrap();

    // Verify final version reflects max from WAL
    assert_eq!(stats.final_version, 3000);
    assert_eq!(store.current_version(), 3000);

    // Verify each key has its exact version preserved
    let alpha = store
        .get(&Key::new_kv(ns.clone(), "alpha"))
        .unwrap()
        .unwrap();
    assert_eq!(alpha.version, 1000); // Not 1, but 1000!
    assert_eq!(alpha.value, Value::I64(111));

    let beta = store
        .get(&Key::new_kv(ns.clone(), "beta"))
        .unwrap()
        .unwrap();
    assert_eq!(beta.version, 2000); // Not 2, but 2000!
    assert_eq!(beta.value, Value::I64(222));

    let gamma = store
        .get(&Key::new_kv(ns.clone(), "gamma"))
        .unwrap()
        .unwrap();
    assert_eq!(gamma.version, 3000); // Not 3, but 3000!
    assert_eq!(gamma.value, Value::I64(333));
}

#[test]
fn test_replay_version_ordering_preserved() {
    let temp_dir = TempDir::new().unwrap();
    let wal_path = temp_dir.path().join("order.wal");

    let run_id = RunId::new();
    let ns = test_namespace(run_id);

    // Write with out-of-order versions in WAL
    {
        let mut wal = WAL::open(&wal_path, DurabilityMode::Strict).unwrap();

        wal.append(&WALEntry::BeginTxn {
            txn_id: 1,
            run_id,
            timestamp: now(),
        })
        .unwrap();

        // Versions are not in increasing order in WAL
        wal.append(&WALEntry::Write {
            run_id,
            key: Key::new_kv(ns.clone(), "key_z"),
            value: Value::Bool(true),
            version: 50,
        })
        .unwrap();

        wal.append(&WALEntry::Write {
            run_id,
            key: Key::new_kv(ns.clone(), "key_a"),
            value: Value::Bool(false),
            version: 10,
        })
        .unwrap();

        wal.append(&WALEntry::Write {
            run_id,
            key: Key::new_kv(ns.clone(), "key_m"),
            value: Value::Bool(true),
            version: 30,
        })
        .unwrap();

        wal.append(&WALEntry::CommitTxn { txn_id: 1, run_id })
            .unwrap();
    }

    // Replay
    let wal = WAL::open(&wal_path, DurabilityMode::Strict).unwrap();
    let store = UnifiedStore::new();
    let stats = replay_wal(&wal, &store).unwrap();

    // Global version should be the max
    assert_eq!(stats.final_version, 50);
    assert_eq!(store.current_version(), 50);

    // Each key should have its original version
    assert_eq!(
        store
            .get(&Key::new_kv(ns.clone(), "key_z"))
            .unwrap()
            .unwrap()
            .version,
        50
    );
    assert_eq!(
        store
            .get(&Key::new_kv(ns.clone(), "key_a"))
            .unwrap()
            .unwrap()
            .version,
        10
    );
    assert_eq!(
        store
            .get(&Key::new_kv(ns.clone(), "key_m"))
            .unwrap()
            .unwrap()
            .version,
        30
    );
}

// ============================================================================
// Write and Delete Tests
// ============================================================================

#[test]
fn test_replay_write_then_delete_same_key() {
    let temp_dir = TempDir::new().unwrap();
    let wal_path = temp_dir.path().join("write_delete.wal");

    let run_id = RunId::new();
    let ns = test_namespace(run_id);

    // Write then delete same key in one transaction
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
            key: Key::new_kv(ns.clone(), "temp"),
            value: Value::Bytes(b"created".to_vec()),
            version: 1,
        })
        .unwrap();

        wal.append(&WALEntry::Delete {
            run_id,
            key: Key::new_kv(ns.clone(), "temp"),
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

    // Verify stats
    assert_eq!(stats.txns_applied, 1);
    assert_eq!(stats.writes_applied, 1);
    assert_eq!(stats.deletes_applied, 1);

    // Key should NOT exist after replay (deleted)
    assert!(store.get(&Key::new_kv(ns, "temp")).unwrap().is_none());
}

#[test]
fn test_replay_delete_nonexistent_key() {
    let temp_dir = TempDir::new().unwrap();
    let wal_path = temp_dir.path().join("delete_none.wal");

    let run_id = RunId::new();
    let ns = test_namespace(run_id);

    // Delete a key that was never created
    {
        let mut wal = WAL::open(&wal_path, DurabilityMode::Strict).unwrap();

        wal.append(&WALEntry::BeginTxn {
            txn_id: 1,
            run_id,
            timestamp: now(),
        })
        .unwrap();

        wal.append(&WALEntry::Delete {
            run_id,
            key: Key::new_kv(ns.clone(), "never_existed"),
            version: 1,
        })
        .unwrap();

        wal.append(&WALEntry::CommitTxn { txn_id: 1, run_id })
            .unwrap();
    }

    // Replay should succeed (deleting non-existent key is fine)
    let wal = WAL::open(&wal_path, DurabilityMode::Strict).unwrap();
    let store = UnifiedStore::new();
    let stats = replay_wal(&wal, &store).unwrap();

    // Delete was applied
    assert_eq!(stats.txns_applied, 1);
    assert_eq!(stats.deletes_applied, 1);

    // Key still doesn't exist
    assert!(store
        .get(&Key::new_kv(ns, "never_existed"))
        .unwrap()
        .is_none());
}

#[test]
fn test_replay_multiple_writes_same_key() {
    let temp_dir = TempDir::new().unwrap();
    let wal_path = temp_dir.path().join("multi_write.wal");

    let run_id = RunId::new();
    let ns = test_namespace(run_id);

    // Multiple writes to same key should result in last value
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
            key: Key::new_kv(ns.clone(), "counter"),
            value: Value::I64(1),
            version: 1,
        })
        .unwrap();

        wal.append(&WALEntry::Write {
            run_id,
            key: Key::new_kv(ns.clone(), "counter"),
            value: Value::I64(2),
            version: 2,
        })
        .unwrap();

        wal.append(&WALEntry::Write {
            run_id,
            key: Key::new_kv(ns.clone(), "counter"),
            value: Value::I64(3),
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

    // All 3 writes were applied
    assert_eq!(stats.writes_applied, 3);

    // Final value is the last one
    let result = store.get(&Key::new_kv(ns, "counter")).unwrap().unwrap();
    assert_eq!(result.value, Value::I64(3));
    assert_eq!(result.version, 3);
}

// ============================================================================
// Edge Cases
// ============================================================================

#[test]
fn test_replay_empty_transaction() {
    let temp_dir = TempDir::new().unwrap();
    let wal_path = temp_dir.path().join("empty_txn.wal");

    let run_id = RunId::new();

    // Transaction with no operations
    {
        let mut wal = WAL::open(&wal_path, DurabilityMode::Strict).unwrap();

        wal.append(&WALEntry::BeginTxn {
            txn_id: 1,
            run_id,
            timestamp: now(),
        })
        .unwrap();
        // No writes or deletes
        wal.append(&WALEntry::CommitTxn { txn_id: 1, run_id })
            .unwrap();
    }

    // Replay
    let wal = WAL::open(&wal_path, DurabilityMode::Strict).unwrap();
    let store = UnifiedStore::new();
    let stats = replay_wal(&wal, &store).unwrap();

    // Empty transaction still counts as applied
    assert_eq!(stats.txns_applied, 1);
    assert_eq!(stats.writes_applied, 0);
    assert_eq!(stats.deletes_applied, 0);
}

#[test]
fn test_replay_different_value_types() {
    let temp_dir = TempDir::new().unwrap();
    let wal_path = temp_dir.path().join("types.wal");

    let run_id = RunId::new();
    let ns = test_namespace(run_id);

    // Write different value types
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
            key: Key::new_kv(ns.clone(), "string"),
            value: Value::String("hello".to_string()),
            version: 1,
        })
        .unwrap();

        wal.append(&WALEntry::Write {
            run_id,
            key: Key::new_kv(ns.clone(), "i64"),
            value: Value::I64(-42),
            version: 2,
        })
        .unwrap();

        wal.append(&WALEntry::Write {
            run_id,
            key: Key::new_kv(ns.clone(), "bool"),
            value: Value::Bool(true),
            version: 3,
        })
        .unwrap();

        wal.append(&WALEntry::Write {
            run_id,
            key: Key::new_kv(ns.clone(), "bytes"),
            value: Value::Bytes(vec![0xDE, 0xAD, 0xBE, 0xEF]),
            version: 4,
        })
        .unwrap();

        wal.append(&WALEntry::CommitTxn { txn_id: 1, run_id })
            .unwrap();
    }

    // Replay
    let wal = WAL::open(&wal_path, DurabilityMode::Strict).unwrap();
    let store = UnifiedStore::new();
    let stats = replay_wal(&wal, &store).unwrap();

    assert_eq!(stats.writes_applied, 4);

    // Verify all types preserved correctly
    assert_eq!(
        store
            .get(&Key::new_kv(ns.clone(), "string"))
            .unwrap()
            .unwrap()
            .value,
        Value::String("hello".to_string())
    );
    assert_eq!(
        store
            .get(&Key::new_kv(ns.clone(), "i64"))
            .unwrap()
            .unwrap()
            .value,
        Value::I64(-42)
    );
    assert_eq!(
        store
            .get(&Key::new_kv(ns.clone(), "bool"))
            .unwrap()
            .unwrap()
            .value,
        Value::Bool(true)
    );
    assert_eq!(
        store
            .get(&Key::new_kv(ns.clone(), "bytes"))
            .unwrap()
            .unwrap()
            .value,
        Value::Bytes(vec![0xDE, 0xAD, 0xBE, 0xEF])
    );
}

#[test]
fn test_replay_deterministic_order() {
    // Replay should be deterministic - same WAL always produces same result
    let temp_dir = TempDir::new().unwrap();
    let wal_path = temp_dir.path().join("deterministic.wal");

    let run_id = RunId::new();
    let ns = test_namespace(run_id);

    // Write transactions in specific order
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
                key: Key::new_kv(ns.clone(), format!("k{}", i)),
                value: Value::I64(i as i64),
                version: i,
            })
            .unwrap();
            wal.append(&WALEntry::CommitTxn { txn_id: i, run_id })
                .unwrap();
        }
    }

    // Replay multiple times - should always get same result
    for _ in 0..3 {
        let wal = WAL::open(&wal_path, DurabilityMode::Strict).unwrap();
        let store = UnifiedStore::new();
        let stats = replay_wal(&wal, &store).unwrap();

        assert_eq!(stats.txns_applied, 5);
        assert_eq!(store.current_version(), 5);

        for i in 1..=5u64 {
            let key = Key::new_kv(ns.clone(), format!("k{}", i));
            let vv = store.get(&key).unwrap().unwrap();
            assert_eq!(vv.value, Value::I64(i as i64));
            assert_eq!(vv.version, i);
        }
    }
}

// ============================================================================
// Bug Reproduction Tests - Issue #145 WAL Recovery Data Loss
// ============================================================================

#[test]
fn test_replay_twenty_sequential_transactions() {
    // This test reproduces the bug where only 10 of 20 transactions are replayed
    let temp_dir = TempDir::new().unwrap();
    let wal_path = temp_dir.path().join("twenty_txns.wal");

    let run_id = RunId::new();
    let ns = test_namespace(run_id);

    const NUM_TRANSACTIONS: u64 = 20;

    // Write 20 sequential transactions: each has Begin -> Write -> Commit
    {
        let mut wal = WAL::open(&wal_path, DurabilityMode::Strict).unwrap();

        for i in 1..=NUM_TRANSACTIONS {
            wal.append(&WALEntry::BeginTxn {
                txn_id: i,
                run_id,
                timestamp: now(),
            })
            .unwrap();

            wal.append(&WALEntry::Write {
                run_id,
                key: Key::new_kv(ns.clone(), format!("key{}", i)),
                value: Value::I64(i as i64),
                version: i,
            })
            .unwrap();

            wal.append(&WALEntry::CommitTxn { txn_id: i, run_id })
                .unwrap();
        }

        wal.fsync().unwrap();
    }

    // Read back entries to verify they're all there
    {
        let wal = WAL::open(&wal_path, DurabilityMode::Strict).unwrap();
        let entries = wal.read_all().unwrap();
        assert_eq!(
            entries.len(),
            NUM_TRANSACTIONS as usize * 3,
            "Should have {} entries (3 per transaction)",
            NUM_TRANSACTIONS * 3
        );
    }

    // Replay to storage
    let wal = WAL::open(&wal_path, DurabilityMode::Strict).unwrap();
    let store = UnifiedStore::new();
    let stats = replay_wal(&wal, &store).unwrap();

    // Verify all transactions were replayed
    assert_eq!(
        stats.txns_applied, NUM_TRANSACTIONS as usize,
        "Expected {} transactions applied, got {}",
        NUM_TRANSACTIONS, stats.txns_applied
    );
    assert_eq!(
        stats.writes_applied, NUM_TRANSACTIONS as usize,
        "Expected {} writes applied, got {}",
        NUM_TRANSACTIONS, stats.writes_applied
    );
    assert_eq!(stats.incomplete_txns, 0, "Should have no incomplete transactions");
    assert_eq!(stats.aborted_txns, 0, "Should have no aborted transactions");

    // Verify all keys exist
    for i in 1..=NUM_TRANSACTIONS {
        let key = Key::new_kv(ns.clone(), format!("key{}", i));
        let result = store.get(&key).unwrap();
        assert!(
            result.is_some(),
            "key{} should exist after replay",
            i
        );
        let vv = result.unwrap();
        assert_eq!(
            vv.value,
            Value::I64(i as i64),
            "key{} should have value {}",
            i,
            i
        );
        assert_eq!(vv.version, i, "key{} should have version {}", i, i);
    }
}

#[test]
fn test_replay_many_sequential_transactions_same_run() {
    // Test with 100 transactions to ensure no off-by-one or boundary issues
    let temp_dir = TempDir::new().unwrap();
    let wal_path = temp_dir.path().join("hundred_txns.wal");

    let run_id = RunId::new();
    let ns = test_namespace(run_id);

    const NUM_TRANSACTIONS: u64 = 100;

    {
        let mut wal = WAL::open(&wal_path, DurabilityMode::Strict).unwrap();

        for i in 1..=NUM_TRANSACTIONS {
            wal.append(&WALEntry::BeginTxn {
                txn_id: i,
                run_id,
                timestamp: now(),
            })
            .unwrap();

            wal.append(&WALEntry::Write {
                run_id,
                key: Key::new_kv(ns.clone(), format!("k{}", i)),
                value: Value::Bytes(vec![i as u8]),
                version: i,
            })
            .unwrap();

            wal.append(&WALEntry::CommitTxn { txn_id: i, run_id })
                .unwrap();
        }

        wal.fsync().unwrap();
    }

    // Replay
    let wal = WAL::open(&wal_path, DurabilityMode::Strict).unwrap();
    let store = UnifiedStore::new();
    let stats = replay_wal(&wal, &store).unwrap();

    assert_eq!(
        stats.txns_applied, NUM_TRANSACTIONS as usize,
        "All {} transactions should be applied",
        NUM_TRANSACTIONS
    );
    assert_eq!(stats.incomplete_txns, 0);
    assert_eq!(stats.final_version, NUM_TRANSACTIONS);
    assert_eq!(store.current_version(), NUM_TRANSACTIONS);

    // Verify all keys
    for i in 1..=NUM_TRANSACTIONS {
        let key = Key::new_kv(ns.clone(), format!("k{}", i));
        assert!(
            store.get(&key).unwrap().is_some(),
            "k{} should exist",
            i
        );
    }
}

#[test]
fn test_replay_appended_wal_multiple_sessions() {
    // Simulates multiple database sessions appending to the same WAL file
    // This is the pattern that was failing: each session adds transactions,
    // but on recovery only half were being replayed
    let temp_dir = TempDir::new().unwrap();
    let wal_path = temp_dir.path().join("multi_session.wal");

    let run_id = RunId::new();
    let ns = test_namespace(run_id);

    // Session 1: Write 10 transactions
    {
        let mut wal = WAL::open(&wal_path, DurabilityMode::Strict).unwrap();
        for i in 1..=10u64 {
            wal.append(&WALEntry::BeginTxn {
                txn_id: i,
                run_id,
                timestamp: now(),
            })
            .unwrap();
            wal.append(&WALEntry::Write {
                run_id,
                key: Key::new_kv(ns.clone(), format!("session1_key{}", i)),
                value: Value::I64(i as i64),
                version: i,
            })
            .unwrap();
            wal.append(&WALEntry::CommitTxn { txn_id: i, run_id })
                .unwrap();
        }
        wal.fsync().unwrap();
    }

    // Verify session 1 entries
    {
        let wal = WAL::open(&wal_path, DurabilityMode::Strict).unwrap();
        let entries = wal.read_all().unwrap();
        assert_eq!(entries.len(), 30, "Session 1: 10 txns * 3 entries = 30");
    }

    // Session 2: Append 10 more transactions (txn_id continues from 11)
    {
        let mut wal = WAL::open(&wal_path, DurabilityMode::Strict).unwrap();
        for i in 11..=20u64 {
            wal.append(&WALEntry::BeginTxn {
                txn_id: i,
                run_id,
                timestamp: now(),
            })
            .unwrap();
            wal.append(&WALEntry::Write {
                run_id,
                key: Key::new_kv(ns.clone(), format!("session2_key{}", i)),
                value: Value::I64(i as i64),
                version: i,
            })
            .unwrap();
            wal.append(&WALEntry::CommitTxn { txn_id: i, run_id })
                .unwrap();
        }
        wal.fsync().unwrap();
    }

    // Verify all entries present
    {
        let wal = WAL::open(&wal_path, DurabilityMode::Strict).unwrap();
        let entries = wal.read_all().unwrap();
        assert_eq!(entries.len(), 60, "Should have 60 entries (20 txns * 3)");

        // Count commits
        let commits = entries.iter().filter(|e| matches!(e, WALEntry::CommitTxn { .. })).count();
        assert_eq!(commits, 20, "Should have 20 commit entries");
    }

    // Replay to storage
    let wal = WAL::open(&wal_path, DurabilityMode::Strict).unwrap();
    let store = UnifiedStore::new();
    let stats = replay_wal(&wal, &store).unwrap();

    // This is the assertion that was failing: only 10 of 20 were replayed
    assert_eq!(
        stats.txns_applied, 20,
        "All 20 transactions should be applied, got {}",
        stats.txns_applied
    );
    assert_eq!(stats.writes_applied, 20);
    assert_eq!(stats.incomplete_txns, 0);
    assert_eq!(stats.aborted_txns, 0);

    // Verify all keys exist
    for i in 1..=10u64 {
        let key = Key::new_kv(ns.clone(), format!("session1_key{}", i));
        assert!(store.get(&key).unwrap().is_some(), "session1_key{} should exist", i);
    }
    for i in 11..=20u64 {
        let key = Key::new_kv(ns.clone(), format!("session2_key{}", i));
        assert!(store.get(&key).unwrap().is_some(), "session2_key{} should exist", i);
    }
}
