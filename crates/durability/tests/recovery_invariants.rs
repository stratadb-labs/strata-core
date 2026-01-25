//! Recovery Invariant Tests
//!
//! These tests validate the recovery invariants (R1-R6):
//!
//! - R1: Recovery is deterministic - Same WAL + Snapshot = Same state
//! - R2: Recovery is idempotent - Replaying recovery produces identical state
//! - R3: Recovery is prefix-consistent - No partial transactions visible after recovery
//! - R4: Recovery never invents data - Only committed data appears
//! - R5: Recovery never drops committed data - All durable commits survive
//! - R6: Recovery may drop uncommitted data - Depending on durability mode
//!
//! These invariants are non-negotiable and define correctness.

use strata_core::contract::Version;
use strata_core::types::{Key, Namespace, RunId};
use strata_core::value::Value;
use strata_core::Timestamp;
use strata_core::Storage;
use strata_durability::recovery::replay_wal;
use strata_durability::wal::{DurabilityMode, WALEntry, WAL};
use strata_storage::UnifiedStore;
use std::collections::HashSet;
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
// R1: Recovery is Deterministic
// Same WAL + Snapshot = Same state
// ============================================================================

/// R1: Recovery from the same WAL produces identical state every time
#[test]
fn test_recovery_deterministic_r1() {
    let temp_dir = TempDir::new().unwrap();
    let wal_path = temp_dir.path().join("deterministic.wal");

    let run_id = RunId::new();
    let ns = test_namespace(run_id);

    // Write deterministic test data
    {
        let mut wal = WAL::open(&wal_path, DurabilityMode::Strict).unwrap();

        for i in 0..20u64 {
            wal.append(&WALEntry::BeginTxn {
                txn_id: i,
                run_id,
                timestamp: now(),
            })
            .unwrap();

            wal.append(&WALEntry::Write {
                run_id,
                key: Key::new_kv(ns.clone(), format!("key{}", i)),
                value: Value::Int(i as i64 * 100),
                version: i + 1,
            })
            .unwrap();

            wal.append(&WALEntry::CommitTxn { txn_id: i, run_id })
                .unwrap();
        }

        wal.fsync().unwrap();
    }

    // Recover multiple times and verify identical state
    let mut previous_state: Option<Vec<(String, Value)>> = None;

    for iteration in 0..5 {
        let wal = WAL::open(&wal_path, DurabilityMode::Strict).unwrap();
        let store = UnifiedStore::new();
        let stats = replay_wal(&wal, &store).unwrap();

        // Collect current state
        let mut current_state: Vec<(String, Value)> = Vec::new();
        for i in 0..20u64 {
            let key = Key::new_kv(ns.clone(), format!("key{}", i));
            if let Some(vv) = store.get(&key).unwrap() {
                current_state.push((format!("key{}", i), vv.value.clone()));
            }
        }

        // Verify stats are identical across recoveries
        assert_eq!(
            stats.txns_applied, 20,
            "Iteration {}: Expected 20 transactions",
            iteration
        );
        assert_eq!(
            stats.writes_applied, 20,
            "Iteration {}: Expected 20 writes",
            iteration
        );
        assert_eq!(
            stats.final_version, 20,
            "Iteration {}: Expected final version 20",
            iteration
        );

        // Verify state is identical to previous recovery
        if let Some(ref prev) = previous_state {
            assert_eq!(
                current_state.len(),
                prev.len(),
                "Iteration {}: State size mismatch",
                iteration
            );
            for (i, ((k1, v1), (k2, v2))) in current_state.iter().zip(prev.iter()).enumerate() {
                assert_eq!(k1, k2, "Iteration {}: Key {} mismatch", iteration, i);
                assert_eq!(v1, v2, "Iteration {}: Value {} mismatch", iteration, i);
            }
        }

        previous_state = Some(current_state);
    }
}

/// R1: Recovery order is deterministic regardless of WAL write order
#[test]
fn test_recovery_deterministic_ordering_r1() {
    let temp_dir = TempDir::new().unwrap();
    let wal_path = temp_dir.path().join("order.wal");

    let run_id = RunId::new();
    let ns = test_namespace(run_id);

    // Write multiple updates to the same key
    {
        let mut wal = WAL::open(&wal_path, DurabilityMode::Strict).unwrap();

        for i in 0..5u64 {
            wal.append(&WALEntry::BeginTxn {
                txn_id: i,
                run_id,
                timestamp: now(),
            })
            .unwrap();

            // Same key, different values
            wal.append(&WALEntry::Write {
                run_id,
                key: Key::new_kv(ns.clone(), "counter"),
                value: Value::Int(i as i64),
                version: i + 1,
            })
            .unwrap();

            wal.append(&WALEntry::CommitTxn { txn_id: i, run_id })
                .unwrap();
        }

        wal.fsync().unwrap();
    }

    // Recover multiple times - final value must always be the same
    for iteration in 0..3 {
        let wal = WAL::open(&wal_path, DurabilityMode::Strict).unwrap();
        let store = UnifiedStore::new();
        replay_wal(&wal, &store).unwrap();

        let key = Key::new_kv(ns.clone(), "counter");
        let result = store.get(&key).unwrap().unwrap();

        // Last write wins - value should be 4
        assert_eq!(
            result.value,
            Value::Int(4),
            "Iteration {}: Final value should be 4",
            iteration
        );
        assert_eq!(
            result.version, Version::Txn(5),
            "Iteration {}: Final version should be 5",
            iteration
        );
    }
}

// ============================================================================
// R2: Recovery is Idempotent
// Replaying recovery produces identical state
// ============================================================================

/// R2: Running recovery twice produces identical state
#[test]
fn test_recovery_idempotent_r2() {
    let temp_dir = TempDir::new().unwrap();
    let wal_path = temp_dir.path().join("idempotent.wal");

    let run_id = RunId::new();
    let ns = test_namespace(run_id);

    // Write test data
    {
        let mut wal = WAL::open(&wal_path, DurabilityMode::Strict).unwrap();

        for i in 0..10u64 {
            wal.append(&WALEntry::BeginTxn {
                txn_id: i,
                run_id,
                timestamp: now(),
            })
            .unwrap();

            wal.append(&WALEntry::Write {
                run_id,
                key: Key::new_kv(ns.clone(), format!("key{}", i)),
                value: Value::Bytes(format!("value{}", i).into_bytes()),
                version: i + 1,
            })
            .unwrap();

            wal.append(&WALEntry::CommitTxn { txn_id: i, run_id })
                .unwrap();
        }

        wal.fsync().unwrap();
    }

    // First recovery
    let wal1 = WAL::open(&wal_path, DurabilityMode::Strict).unwrap();
    let store1 = UnifiedStore::new();
    let stats1 = replay_wal(&wal1, &store1).unwrap();

    // Second recovery (simulating "crash" after first recovery and re-recovery)
    let wal2 = WAL::open(&wal_path, DurabilityMode::Strict).unwrap();
    let store2 = UnifiedStore::new();
    let stats2 = replay_wal(&wal2, &store2).unwrap();

    // Stats must be identical
    assert_eq!(stats1.txns_applied, stats2.txns_applied);
    assert_eq!(stats1.writes_applied, stats2.writes_applied);
    assert_eq!(stats1.final_version, stats2.final_version);
    assert_eq!(stats1.incomplete_txns, stats2.incomplete_txns);

    // All keys must be identical
    for i in 0..10u64 {
        let key = Key::new_kv(ns.clone(), format!("key{}", i));
        let v1 = store1.get(&key).unwrap();
        let v2 = store2.get(&key).unwrap();

        assert_eq!(
            v1.is_some(),
            v2.is_some(),
            "Key existence mismatch for key{}",
            i
        );

        if let (Some(vv1), Some(vv2)) = (v1, v2) {
            assert_eq!(vv1.value, vv2.value, "Value mismatch for key{}", i);
            assert_eq!(vv1.version, vv2.version, "Version mismatch for key{}", i);
        }
    }
}

/// R2: Recovery after partial recovery is still idempotent
#[test]
fn test_recovery_idempotent_after_partial_r2() {
    let temp_dir = TempDir::new().unwrap();
    let wal_path = temp_dir.path().join("partial_idempotent.wal");

    let run_id = RunId::new();
    let ns = test_namespace(run_id);

    // Write committed + incomplete transactions
    {
        let mut wal = WAL::open(&wal_path, DurabilityMode::Strict).unwrap();

        // Committed transactions
        for i in 0..5u64 {
            wal.append(&WALEntry::BeginTxn {
                txn_id: i,
                run_id,
                timestamp: now(),
            })
            .unwrap();

            wal.append(&WALEntry::Write {
                run_id,
                key: Key::new_kv(ns.clone(), format!("committed{}", i)),
                value: Value::Int(i as i64),
                version: i + 1,
            })
            .unwrap();

            wal.append(&WALEntry::CommitTxn { txn_id: i, run_id })
                .unwrap();
        }

        // Incomplete transaction (simulating crash)
        wal.append(&WALEntry::BeginTxn {
            txn_id: 99,
            run_id,
            timestamp: now(),
        })
        .unwrap();

        wal.append(&WALEntry::Write {
            run_id,
            key: Key::new_kv(ns.clone(), "incomplete"),
            value: Value::String("should_not_exist".to_string()),
            version: 100,
        })
        .unwrap();

        // NO CommitTxn
        wal.fsync().unwrap();
    }

    // Recovery multiple times must produce same state
    for iteration in 0..3 {
        let wal = WAL::open(&wal_path, DurabilityMode::Strict).unwrap();
        let store = UnifiedStore::new();
        let stats = replay_wal(&wal, &store).unwrap();

        // Verify committed data exists
        for i in 0..5u64 {
            let key = Key::new_kv(ns.clone(), format!("committed{}", i));
            assert!(
                store.get(&key).unwrap().is_some(),
                "Iteration {}: committed{} should exist",
                iteration,
                i
            );
        }

        // Verify incomplete data does NOT exist
        let incomplete_key = Key::new_kv(ns.clone(), "incomplete");
        assert!(
            store.get(&incomplete_key).unwrap().is_none(),
            "Iteration {}: incomplete key should not exist",
            iteration
        );

        // Stats must be consistent
        assert_eq!(
            stats.txns_applied, 5,
            "Iteration {}: Should apply 5 transactions",
            iteration
        );
        assert_eq!(
            stats.incomplete_txns, 1,
            "Iteration {}: Should have 1 incomplete",
            iteration
        );
    }
}

// ============================================================================
// R3: Recovery is Prefix-Consistent
// No partial transactions visible after recovery
// ============================================================================

/// R3: Either all writes in a transaction are visible, or none
#[test]
fn test_recovery_prefix_consistent_r3() {
    let temp_dir = TempDir::new().unwrap();
    let wal_path = temp_dir.path().join("prefix.wal");

    let run_id = RunId::new();
    let ns = test_namespace(run_id);

    // Write transactions with multiple writes each
    {
        let mut wal = WAL::open(&wal_path, DurabilityMode::Strict).unwrap();

        // Transaction 1 - committed (3 writes)
        wal.append(&WALEntry::BeginTxn {
            txn_id: 1,
            run_id,
            timestamp: now(),
        })
        .unwrap();

        for j in 0..3 {
            wal.append(&WALEntry::Write {
                run_id,
                key: Key::new_kv(ns.clone(), format!("tx1_key{}", j)),
                value: Value::String(format!("tx1_value{}", j)),
                version: j + 1,
            })
            .unwrap();
        }

        wal.append(&WALEntry::CommitTxn { txn_id: 1, run_id })
            .unwrap();

        // Transaction 2 - incomplete (3 writes, no commit)
        wal.append(&WALEntry::BeginTxn {
            txn_id: 2,
            run_id,
            timestamp: now(),
        })
        .unwrap();

        for j in 0..3 {
            wal.append(&WALEntry::Write {
                run_id,
                key: Key::new_kv(ns.clone(), format!("tx2_key{}", j)),
                value: Value::String(format!("tx2_value{}", j)),
                version: j + 10,
            })
            .unwrap();
        }

        // NO CommitTxn for transaction 2

        // Transaction 3 - committed (2 writes)
        wal.append(&WALEntry::BeginTxn {
            txn_id: 3,
            run_id,
            timestamp: now(),
        })
        .unwrap();

        for j in 0..2 {
            wal.append(&WALEntry::Write {
                run_id,
                key: Key::new_kv(ns.clone(), format!("tx3_key{}", j)),
                value: Value::String(format!("tx3_value{}", j)),
                version: j + 20,
            })
            .unwrap();
        }

        wal.append(&WALEntry::CommitTxn { txn_id: 3, run_id })
            .unwrap();

        wal.fsync().unwrap();
    }

    // Recovery
    let wal = WAL::open(&wal_path, DurabilityMode::Strict).unwrap();
    let store = UnifiedStore::new();
    let stats = replay_wal(&wal, &store).unwrap();

    // Transaction 1: ALL keys must exist (committed)
    let tx1_count = (0..3)
        .filter(|j| {
            store
                .get(&Key::new_kv(ns.clone(), format!("tx1_key{}", j)))
                .unwrap()
                .is_some()
        })
        .count();
    assert_eq!(tx1_count, 3, "Transaction 1: All 3 keys should exist");

    // Transaction 2: NO keys must exist (incomplete)
    let tx2_count = (0..3)
        .filter(|j| {
            store
                .get(&Key::new_kv(ns.clone(), format!("tx2_key{}", j)))
                .unwrap()
                .is_some()
        })
        .count();
    assert_eq!(
        tx2_count, 0,
        "Transaction 2: No keys should exist (incomplete)"
    );

    // Transaction 3: ALL keys must exist (committed)
    let tx3_count = (0..2)
        .filter(|j| {
            store
                .get(&Key::new_kv(ns.clone(), format!("tx3_key{}", j)))
                .unwrap()
                .is_some()
        })
        .count();
    assert_eq!(tx3_count, 2, "Transaction 3: All 2 keys should exist");

    // Verify stats
    assert_eq!(
        stats.txns_applied, 2,
        "Should have 2 committed transactions"
    );
    assert_eq!(
        stats.incomplete_txns, 1,
        "Should have 1 incomplete transaction"
    );
}

/// R3: Transactions with multiple operations are atomic
#[test]
fn test_recovery_multi_operation_atomic_r3() {
    let temp_dir = TempDir::new().unwrap();
    let wal_path = temp_dir.path().join("multi_op.wal");

    let run_id = RunId::new();
    let ns = test_namespace(run_id);

    // Write transaction with multiple KV operations (simulating cross-primitive)
    {
        let mut wal = WAL::open(&wal_path, DurabilityMode::Strict).unwrap();

        // Committed multi-operation transaction
        wal.append(&WALEntry::BeginTxn {
            txn_id: 1,
            run_id,
            timestamp: now(),
        })
        .unwrap();

        // Multiple writes in single transaction
        wal.append(&WALEntry::Write {
            run_id,
            key: Key::new_kv(ns.clone(), "kv_key1"),
            value: Value::String("value1".to_string()),
            version: 1,
        })
        .unwrap();

        wal.append(&WALEntry::Write {
            run_id,
            key: Key::new_kv(ns.clone(), "kv_key2"),
            value: Value::Int(42),
            version: 2,
        })
        .unwrap();

        wal.append(&WALEntry::CommitTxn { txn_id: 1, run_id })
            .unwrap();

        // Incomplete multi-operation transaction
        wal.append(&WALEntry::BeginTxn {
            txn_id: 2,
            run_id,
            timestamp: now(),
        })
        .unwrap();

        wal.append(&WALEntry::Write {
            run_id,
            key: Key::new_kv(ns.clone(), "incomplete_key1"),
            value: Value::String("incomplete".to_string()),
            version: 3,
        })
        .unwrap();

        wal.append(&WALEntry::Write {
            run_id,
            key: Key::new_kv(ns.clone(), "incomplete_key2"),
            value: Value::Int(100),
            version: 4,
        })
        .unwrap();

        // NO CommitTxn

        wal.fsync().unwrap();
    }

    // Recovery
    let wal = WAL::open(&wal_path, DurabilityMode::Strict).unwrap();
    let store = UnifiedStore::new();
    let stats = replay_wal(&wal, &store).unwrap();

    // Committed transaction: ALL keys should exist
    let kv_key1 = Key::new_kv(ns.clone(), "kv_key1");
    let kv_key2 = Key::new_kv(ns.clone(), "kv_key2");
    assert!(
        store.get(&kv_key1).unwrap().is_some(),
        "Committed kv_key1 should exist"
    );
    assert!(
        store.get(&kv_key2).unwrap().is_some(),
        "Committed kv_key2 should exist"
    );

    // Incomplete transaction: NO keys should exist
    let incomplete_key1 = Key::new_kv(ns.clone(), "incomplete_key1");
    let incomplete_key2 = Key::new_kv(ns.clone(), "incomplete_key2");
    assert!(
        store.get(&incomplete_key1).unwrap().is_none(),
        "Incomplete key1 should not exist"
    );
    assert!(
        store.get(&incomplete_key2).unwrap().is_none(),
        "Incomplete key2 should not exist"
    );

    assert_eq!(stats.txns_applied, 1);
    assert_eq!(stats.incomplete_txns, 1);
}

// ============================================================================
// R4: Recovery Never Invents Data
// Only committed data appears
// ============================================================================

/// R4: Recovered state contains only keys that were written
#[test]
fn test_recovery_never_invents_data_r4() {
    let temp_dir = TempDir::new().unwrap();
    let wal_path = temp_dir.path().join("no_invent.wal");

    let run_id = RunId::new();
    let ns = test_namespace(run_id);

    // Track all keys we write
    let mut written_keys: HashSet<String> = HashSet::new();

    // Write specific known keys
    {
        let mut wal = WAL::open(&wal_path, DurabilityMode::Strict).unwrap();

        for i in 0..50u64 {
            let key_name = format!("known_key_{}", i);
            written_keys.insert(key_name.clone());

            wal.append(&WALEntry::BeginTxn {
                txn_id: i,
                run_id,
                timestamp: now(),
            })
            .unwrap();

            wal.append(&WALEntry::Write {
                run_id,
                key: Key::new_kv(ns.clone(), key_name),
                value: Value::Int(i as i64),
                version: i + 1,
            })
            .unwrap();

            wal.append(&WALEntry::CommitTxn { txn_id: i, run_id })
                .unwrap();
        }

        wal.fsync().unwrap();
    }

    // Recovery
    let wal = WAL::open(&wal_path, DurabilityMode::Strict).unwrap();
    let store = UnifiedStore::new();
    replay_wal(&wal, &store).unwrap();

    // Verify only known keys exist
    for i in 0..50 {
        let key_name = format!("known_key_{}", i);
        let key = Key::new_kv(ns.clone(), &key_name);
        let result = store.get(&key).unwrap();

        assert!(
            result.is_some(),
            "Key '{}' should exist after recovery",
            key_name
        );

        // Value should match what we wrote
        let vv = result.unwrap();
        assert_eq!(
            vv.value,
            Value::Int(i as i64),
            "Key '{}' has wrong value",
            key_name
        );
    }

    // Verify unknown keys don't exist
    for unknown in ["invented_key", "phantom_data", "random_123"] {
        let key = Key::new_kv(ns.clone(), unknown);
        assert!(
            store.get(&key).unwrap().is_none(),
            "Unknown key '{}' should not exist",
            unknown
        );
    }
}

/// R4: Recovery does not recover uncommitted data
#[test]
fn test_recovery_no_uncommitted_data_r4() {
    let temp_dir = TempDir::new().unwrap();
    let wal_path = temp_dir.path().join("no_uncommitted.wal");

    let run_id = RunId::new();
    let ns = test_namespace(run_id);

    // Write committed and uncommitted data
    {
        let mut wal = WAL::open(&wal_path, DurabilityMode::Strict).unwrap();

        // Committed
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
            version: 1,
        })
        .unwrap();

        wal.append(&WALEntry::CommitTxn { txn_id: 1, run_id })
            .unwrap();

        // Aborted
        wal.append(&WALEntry::BeginTxn {
            txn_id: 2,
            run_id,
            timestamp: now(),
        })
        .unwrap();

        wal.append(&WALEntry::Write {
            run_id,
            key: Key::new_kv(ns.clone(), "aborted"),
            value: Value::String("no".to_string()),
            version: 2,
        })
        .unwrap();

        wal.append(&WALEntry::AbortTxn { txn_id: 2, run_id })
            .unwrap();

        // Incomplete (crashed)
        wal.append(&WALEntry::BeginTxn {
            txn_id: 3,
            run_id,
            timestamp: now(),
        })
        .unwrap();

        wal.append(&WALEntry::Write {
            run_id,
            key: Key::new_kv(ns.clone(), "incomplete"),
            value: Value::String("crashed".to_string()),
            version: 3,
        })
        .unwrap();

        // NO CommitTxn

        wal.fsync().unwrap();
    }

    // Recovery
    let wal = WAL::open(&wal_path, DurabilityMode::Strict).unwrap();
    let store = UnifiedStore::new();
    let stats = replay_wal(&wal, &store).unwrap();

    // Only committed data exists
    assert!(
        store
            .get(&Key::new_kv(ns.clone(), "committed"))
            .unwrap()
            .is_some(),
        "Committed key should exist"
    );
    assert!(
        store
            .get(&Key::new_kv(ns.clone(), "aborted"))
            .unwrap()
            .is_none(),
        "Aborted key should not exist"
    );
    assert!(
        store
            .get(&Key::new_kv(ns.clone(), "incomplete"))
            .unwrap()
            .is_none(),
        "Incomplete key should not exist"
    );

    assert_eq!(stats.txns_applied, 1);
    assert_eq!(stats.aborted_txns, 1);
    assert_eq!(stats.incomplete_txns, 1);
}

// ============================================================================
// R5: Recovery Never Drops Committed Data
// All durable commits survive
// ============================================================================

/// R5: All committed data survives recovery
#[test]
fn test_recovery_never_drops_committed_r5() {
    let temp_dir = TempDir::new().unwrap();
    let wal_path = temp_dir.path().join("no_drop.wal");

    let run_id = RunId::new();
    let ns = test_namespace(run_id);

    let mut committed_keys: Vec<String> = Vec::new();

    // Write many committed transactions
    {
        let mut wal = WAL::open(&wal_path, DurabilityMode::Strict).unwrap();

        for i in 0..100u64 {
            let key_name = format!("durable_{}", i);
            committed_keys.push(key_name.clone());

            wal.append(&WALEntry::BeginTxn {
                txn_id: i,
                run_id,
                timestamp: now(),
            })
            .unwrap();

            wal.append(&WALEntry::Write {
                run_id,
                key: Key::new_kv(ns.clone(), key_name),
                value: Value::Int(i as i64 * 1000),
                version: i + 1,
            })
            .unwrap();

            wal.append(&WALEntry::CommitTxn { txn_id: i, run_id })
                .unwrap();
        }

        // Explicit fsync for strict durability
        wal.fsync().unwrap();
    }

    // Recovery
    let wal = WAL::open(&wal_path, DurabilityMode::Strict).unwrap();
    let store = UnifiedStore::new();
    let stats = replay_wal(&wal, &store).unwrap();

    // EVERY committed key must be present
    for (i, key_name) in committed_keys.iter().enumerate() {
        let key = Key::new_kv(ns.clone(), key_name);
        let result = store.get(&key).unwrap();

        assert!(
            result.is_some(),
            "R5 VIOLATION: Committed key '{}' was dropped during recovery!",
            key_name
        );

        let vv = result.unwrap();
        assert_eq!(
            vv.value,
            Value::Int(i as i64 * 1000),
            "Key '{}' has wrong value after recovery",
            key_name
        );
    }

    assert_eq!(
        stats.txns_applied, 100,
        "All 100 transactions should be applied"
    );
}

/// R5: Committed deletes are also preserved
#[test]
fn test_recovery_preserves_committed_deletes_r5() {
    let temp_dir = TempDir::new().unwrap();
    let wal_path = temp_dir.path().join("delete_preserved.wal");

    let run_id = RunId::new();
    let ns = test_namespace(run_id);

    {
        let mut wal = WAL::open(&wal_path, DurabilityMode::Strict).unwrap();

        // Create key
        wal.append(&WALEntry::BeginTxn {
            txn_id: 1,
            run_id,
            timestamp: now(),
        })
        .unwrap();

        wal.append(&WALEntry::Write {
            run_id,
            key: Key::new_kv(ns.clone(), "to_delete"),
            value: Value::String("temporary".to_string()),
            version: 1,
        })
        .unwrap();

        wal.append(&WALEntry::CommitTxn { txn_id: 1, run_id })
            .unwrap();

        // Delete key
        wal.append(&WALEntry::BeginTxn {
            txn_id: 2,
            run_id,
            timestamp: now(),
        })
        .unwrap();

        wal.append(&WALEntry::Delete {
            run_id,
            key: Key::new_kv(ns.clone(), "to_delete"),
            version: 2,
        })
        .unwrap();

        wal.append(&WALEntry::CommitTxn { txn_id: 2, run_id })
            .unwrap();

        wal.fsync().unwrap();
    }

    // Recovery
    let wal = WAL::open(&wal_path, DurabilityMode::Strict).unwrap();
    let store = UnifiedStore::new();
    let stats = replay_wal(&wal, &store).unwrap();

    // Key should NOT exist (delete was committed)
    let key = Key::new_kv(ns.clone(), "to_delete");
    assert!(
        store.get(&key).unwrap().is_none(),
        "Deleted key should not exist after recovery"
    );

    assert_eq!(stats.txns_applied, 2);
    assert_eq!(stats.deletes_applied, 1);
}

// ============================================================================
// R6: Recovery May Drop Uncommitted Data
// Depending on durability mode
// ============================================================================

/// R6: Uncommitted transactions are correctly dropped
#[test]
fn test_recovery_drops_uncommitted_r6() {
    let temp_dir = TempDir::new().unwrap();
    let wal_path = temp_dir.path().join("drop_uncommitted.wal");

    let run_id = RunId::new();
    let ns = test_namespace(run_id);

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
            key: Key::new_kv(ns.clone(), "committed_key"),
            value: Value::String("durable".to_string()),
            version: 1,
        })
        .unwrap();

        wal.append(&WALEntry::CommitTxn { txn_id: 1, run_id })
            .unwrap();

        // Multiple uncommitted transactions (simulating various crash points)
        for i in 10..15u64 {
            wal.append(&WALEntry::BeginTxn {
                txn_id: i,
                run_id,
                timestamp: now(),
            })
            .unwrap();

            wal.append(&WALEntry::Write {
                run_id,
                key: Key::new_kv(ns.clone(), format!("uncommitted_{}", i)),
                value: Value::String("lost".to_string()),
                version: i,
            })
            .unwrap();

            // NO CommitTxn - simulating crash
        }

        wal.fsync().unwrap();
    }

    // Recovery
    let wal = WAL::open(&wal_path, DurabilityMode::Strict).unwrap();
    let store = UnifiedStore::new();
    let stats = replay_wal(&wal, &store).unwrap();

    // Committed data exists
    assert!(
        store
            .get(&Key::new_kv(ns.clone(), "committed_key"))
            .unwrap()
            .is_some(),
        "Committed data should survive"
    );

    // Uncommitted data dropped
    for i in 10..15 {
        let key = Key::new_kv(ns.clone(), format!("uncommitted_{}", i));
        assert!(
            store.get(&key).unwrap().is_none(),
            "Uncommitted key uncommitted_{} should be dropped",
            i
        );
    }

    assert_eq!(stats.txns_applied, 1);
    assert_eq!(stats.incomplete_txns, 5);
}

/// R6: Aborted transactions are dropped even with complete WAL entries
#[test]
fn test_recovery_drops_aborted_r6() {
    let temp_dir = TempDir::new().unwrap();
    let wal_path = temp_dir.path().join("drop_aborted.wal");

    let run_id = RunId::new();
    let ns = test_namespace(run_id);

    {
        let mut wal = WAL::open(&wal_path, DurabilityMode::Strict).unwrap();

        // Aborted transaction with lots of data
        wal.append(&WALEntry::BeginTxn {
            txn_id: 1,
            run_id,
            timestamp: now(),
        })
        .unwrap();

        for i in 0..10 {
            wal.append(&WALEntry::Write {
                run_id,
                key: Key::new_kv(ns.clone(), format!("aborted_key_{}", i)),
                value: Value::Bytes(vec![0u8; 1000]), // 1KB each
                version: i + 1,
            })
            .unwrap();
        }

        wal.append(&WALEntry::AbortTxn { txn_id: 1, run_id })
            .unwrap();

        wal.fsync().unwrap();
    }

    // Recovery
    let wal = WAL::open(&wal_path, DurabilityMode::Strict).unwrap();
    let store = UnifiedStore::new();
    let stats = replay_wal(&wal, &store).unwrap();

    // All aborted data should be gone
    for i in 0..10 {
        let key = Key::new_kv(ns.clone(), format!("aborted_key_{}", i));
        assert!(
            store.get(&key).unwrap().is_none(),
            "Aborted key aborted_key_{} should not exist",
            i
        );
    }

    assert_eq!(stats.txns_applied, 0);
    assert_eq!(stats.aborted_txns, 1);
    assert_eq!(stats.writes_applied, 0);
}

// ============================================================================
// Combined Invariant Tests
// ============================================================================

/// Test all invariants together with complex workload
#[test]
fn test_all_recovery_invariants_combined() {
    let temp_dir = TempDir::new().unwrap();
    let wal_path = temp_dir.path().join("all_invariants.wal");

    let run_id = RunId::new();
    let ns = test_namespace(run_id);

    // Track what we expect to see after recovery
    let mut expected_keys: HashSet<String> = HashSet::new();
    let mut expected_absent: HashSet<String> = HashSet::new();

    {
        let mut wal = WAL::open(&wal_path, DurabilityMode::Strict).unwrap();

        // Committed transactions (R5: never dropped)
        for i in 0..20u64 {
            let key_name = format!("committed_{}", i);
            expected_keys.insert(key_name.clone());

            wal.append(&WALEntry::BeginTxn {
                txn_id: i,
                run_id,
                timestamp: now(),
            })
            .unwrap();

            wal.append(&WALEntry::Write {
                run_id,
                key: Key::new_kv(ns.clone(), key_name),
                value: Value::Int(i as i64),
                version: i + 1,
            })
            .unwrap();

            wal.append(&WALEntry::CommitTxn { txn_id: i, run_id })
                .unwrap();
        }

        // Aborted transactions (R6: dropped)
        for i in 100..105u64 {
            let key_name = format!("aborted_{}", i);
            expected_absent.insert(key_name.clone());

            wal.append(&WALEntry::BeginTxn {
                txn_id: i,
                run_id,
                timestamp: now(),
            })
            .unwrap();

            wal.append(&WALEntry::Write {
                run_id,
                key: Key::new_kv(ns.clone(), key_name),
                value: Value::Int(i as i64),
                version: i,
            })
            .unwrap();

            wal.append(&WALEntry::AbortTxn { txn_id: i, run_id })
                .unwrap();
        }

        // Incomplete transactions (R6: dropped, R3: prefix-consistent)
        for i in 200..210u64 {
            let key_name = format!("incomplete_{}", i);
            expected_absent.insert(key_name.clone());

            wal.append(&WALEntry::BeginTxn {
                txn_id: i,
                run_id,
                timestamp: now(),
            })
            .unwrap();

            // Multiple writes per transaction
            for j in 0..3 {
                wal.append(&WALEntry::Write {
                    run_id,
                    key: Key::new_kv(ns.clone(), format!("{}_{}", key_name, j)),
                    value: Value::Int((i * 10 + j) as i64),
                    version: i * 10 + j,
                })
                .unwrap();
                expected_absent.insert(format!("{}_{}", key_name, j));
            }

            // NO CommitTxn
        }

        wal.fsync().unwrap();
    }

    // Recover multiple times (R1: deterministic, R2: idempotent)
    for iteration in 0..3 {
        let wal = WAL::open(&wal_path, DurabilityMode::Strict).unwrap();
        let store = UnifiedStore::new();
        let stats = replay_wal(&wal, &store).unwrap();

        // R4: Only expected keys exist
        for key_name in &expected_keys {
            let key = Key::new_kv(ns.clone(), key_name);
            assert!(
                store.get(&key).unwrap().is_some(),
                "Iteration {}: Expected key '{}' should exist (R4/R5 violation)",
                iteration,
                key_name
            );
        }

        // R4/R6: Unexpected keys do not exist
        for key_name in &expected_absent {
            let key = Key::new_kv(ns.clone(), key_name);
            assert!(
                store.get(&key).unwrap().is_none(),
                "Iteration {}: Unexpected key '{}' should not exist (R4/R6 violation)",
                iteration,
                key_name
            );
        }

        // Verify consistent stats across iterations
        assert_eq!(
            stats.txns_applied, 20,
            "Iteration {}: Wrong txns_applied",
            iteration
        );
        assert_eq!(
            stats.aborted_txns, 5,
            "Iteration {}: Wrong aborted_txns",
            iteration
        );
        assert_eq!(
            stats.incomplete_txns, 10,
            "Iteration {}: Wrong incomplete_txns",
            iteration
        );
    }
}
