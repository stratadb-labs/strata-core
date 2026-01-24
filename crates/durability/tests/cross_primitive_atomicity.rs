//! Cross-Primitive Atomicity Integration Tests (Story #320)
//!
//! These tests verify that transactions spanning multiple primitives (KV, JSON,
//! Event, State, Run) are atomic - after crash recovery, you see either
//! all effects of a transaction or none.
//!
//! ## Core Guarantee
//!
//! > After crash recovery, the database must correspond to a **prefix of the
//! > committed transaction history**. No partial transactions may be visible.

use strata_durability::wal::DurabilityMode;
use strata_durability::wal_entry_types::WalEntryType;
use strata_durability::Transaction;
use strata_durability::WalWriter;
use strata_durability::{RecoveryEngine, RecoveryOptions};
use tempfile::TempDir;

fn create_test_dir() -> TempDir {
    TempDir::new().unwrap()
}

// ============================================================================
// Basic Cross-Primitive Tests
// ============================================================================

/// Test: Cross-primitive commit - all primitives in one transaction
#[test]
fn test_cross_primitive_commit() {
    let temp_dir = create_test_dir();
    let wal_path = temp_dir.path().join("wal.dat");

    // Create and commit cross-primitive transaction
    {
        let mut writer = WalWriter::open(&wal_path, DurabilityMode::Strict).unwrap();

        let mut tx = Transaction::new();
        tx.kv_put("kv_key", "kv_value")
            .json_set("json_key", b"{\"field\":\"value\"}".to_vec())
            .event_append(b"task_started".to_vec())
            .state_set("state_key", "active");

        writer.commit_atomic(tx).unwrap();
    }

    // Verify all entries are recovered
    let (transactions, result) =
        RecoveryEngine::replay_wal_committed(&wal_path, 0, &RecoveryOptions::default()).unwrap();

    assert_eq!(transactions.len(), 1);
    assert_eq!(result.transactions_recovered, 1);
    assert_eq!(result.orphaned_transactions, 0);

    // Should have 4 entries (KV, JSON, Event, State)
    let (_, entries) = &transactions[0];
    assert_eq!(entries.len(), 4);

    // Verify entry types
    assert_eq!(entries[0].entry_type, WalEntryType::KvPut);
    assert_eq!(entries[1].entry_type, WalEntryType::JsonSet);
    assert_eq!(entries[2].entry_type, WalEntryType::EventAppend);
    assert_eq!(entries[3].entry_type, WalEntryType::StateSet);
}

/// Test: All 5 primitives in one transaction
#[test]
fn test_all_primitives_atomic() {
    let temp_dir = create_test_dir();
    let wal_path = temp_dir.path().join("wal.dat");

    {
        let mut writer = WalWriter::open(&wal_path, DurabilityMode::Strict).unwrap();

        let mut tx = Transaction::new();

        // KV operations
        tx.kv_put("kv_key", "kv_value");

        // JSON operations
        tx.json_set("json_key", b"{\"hello\":\"world\"}".to_vec());

        // Event operation
        tx.event_append(b"event1".to_vec());

        // State operations
        tx.state_set("state_key", "running");

        // Run operations
        tx.run_create(b"run_metadata".to_vec());

        writer.commit_atomic(tx).unwrap();
    }

    let (transactions, result) =
        RecoveryEngine::replay_wal_committed(&wal_path, 0, &RecoveryOptions::default()).unwrap();

    assert_eq!(transactions.len(), 1);
    assert_eq!(result.transactions_recovered, 1);

    let (_, entries) = &transactions[0];
    assert_eq!(entries.len(), 5); // One entry per primitive
}

// ============================================================================
// Rollback / Abort Tests
// ============================================================================

/// Test: Cross-primitive rollback (abort)
#[test]
fn test_cross_primitive_rollback() {
    let temp_dir = create_test_dir();
    let wal_path = temp_dir.path().join("wal.dat");

    {
        let mut writer = WalWriter::open(&wal_path, DurabilityMode::Strict).unwrap();

        // Transaction that gets aborted
        let tx_id = writer.begin_transaction();
        writer
            .write_tx_entry(tx_id, WalEntryType::KvPut, b"key1=value1".to_vec())
            .unwrap();
        writer
            .write_tx_entry(tx_id, WalEntryType::JsonSet, b"doc1={}".to_vec())
            .unwrap();
        writer.abort_transaction(tx_id).unwrap();

        // Committed transaction
        let mut tx = Transaction::new();
        tx.kv_put("committed_key", "committed_value");
        writer.commit_atomic(tx).unwrap();
    }

    let (transactions, result) =
        RecoveryEngine::replay_wal_committed(&wal_path, 0, &RecoveryOptions::default()).unwrap();

    // Only the committed transaction should be visible
    assert_eq!(transactions.len(), 1);
    assert_eq!(result.transactions_recovered, 1);
    assert_eq!(result.aborted_transactions, 1);
}

// ============================================================================
// Crash Recovery Tests
// ============================================================================

/// Test: Crash mid-transaction - nothing visible
#[test]
fn test_crash_mid_transaction() {
    let temp_dir = create_test_dir();
    let wal_path = temp_dir.path().join("wal.dat");

    // Simulate crash mid-transaction
    {
        let mut writer = WalWriter::open(&wal_path, DurabilityMode::Strict).unwrap();

        // Write entries but don't commit
        let tx_id = writer.begin_transaction();
        writer
            .write_tx_entry(tx_id, WalEntryType::KvPut, b"key1=value1".to_vec())
            .unwrap();
        writer
            .write_tx_entry(tx_id, WalEntryType::JsonSet, b"doc1={}".to_vec())
            .unwrap();
        writer
            .write_tx_entry(tx_id, WalEntryType::StateSet, b"state1=active".to_vec())
            .unwrap();
        // NO commit marker - simulating crash
    }

    // Recover
    let (transactions, result) =
        RecoveryEngine::replay_wal_committed(&wal_path, 0, &RecoveryOptions::default()).unwrap();

    // Nothing should be visible
    assert_eq!(transactions.len(), 0);
    assert_eq!(result.transactions_recovered, 0);
    assert_eq!(result.orphaned_transactions, 1);
}

/// Test: Partial transaction not visible
#[test]
fn test_partial_transaction_not_visible() {
    let temp_dir = create_test_dir();
    let wal_path = temp_dir.path().join("wal.dat");

    let tx1_id;
    {
        let mut writer = WalWriter::open(&wal_path, DurabilityMode::Strict).unwrap();

        // TX1 - committed
        let mut tx1 = Transaction::new();
        tx1.kv_put("tx1_key", "tx1_value");
        tx1_id = tx1.id();
        writer.commit_atomic(tx1).unwrap();

        // TX2 - partial (multiple primitives, no commit)
        let tx2_id = writer.begin_transaction();
        writer
            .write_tx_entry(tx2_id, WalEntryType::KvPut, b"tx2_key=tx2_value".to_vec())
            .unwrap();
        writer
            .write_tx_entry(tx2_id, WalEntryType::JsonSet, b"tx2_doc={}".to_vec())
            .unwrap();
        writer
            .write_tx_entry(tx2_id, WalEntryType::StateSet, b"tx2_state=active".to_vec())
            .unwrap();
        // NO commit - simulating crash
    }

    let (transactions, result) =
        RecoveryEngine::replay_wal_committed(&wal_path, 0, &RecoveryOptions::default()).unwrap();

    // TX1 should be visible, TX2 should not
    assert_eq!(transactions.len(), 1);
    assert_eq!(transactions[0].0, tx1_id);
    assert_eq!(result.transactions_recovered, 1);
    assert_eq!(result.orphaned_transactions, 1);
}

// ============================================================================
// Interleaved Transaction Tests
// ============================================================================

/// Test: Interleaved transactions
#[test]
fn test_interleaved_transactions() {
    let temp_dir = create_test_dir();
    let wal_path = temp_dir.path().join("wal.dat");

    {
        let mut writer = WalWriter::open(&wal_path, DurabilityMode::Strict).unwrap();

        // TX1
        let mut tx1 = Transaction::new();
        tx1.kv_put("tx1_kv", "value1")
            .json_set("tx1_json", b"{}".to_vec());

        // TX2
        let mut tx2 = Transaction::new();
        tx2.kv_put("tx2_kv", "value2")
            .state_set("tx2_state", "active");

        // TX3
        let mut tx3 = Transaction::new();
        tx3.json_set("tx3_json", b"{\"x\":1}".to_vec())
            .event_append(b"tx3_event".to_vec());

        // Commit in order
        writer.commit_atomic(tx1).unwrap();
        writer.commit_atomic(tx2).unwrap();
        writer.commit_atomic(tx3).unwrap();
    }

    let (transactions, result) =
        RecoveryEngine::replay_wal_committed(&wal_path, 0, &RecoveryOptions::default()).unwrap();

    // All 3 transactions should be recovered
    assert_eq!(transactions.len(), 3);
    assert_eq!(result.transactions_recovered, 3);

    // Verify entry counts per transaction
    assert_eq!(transactions[0].1.len(), 2); // tx1: kv + json
    assert_eq!(transactions[1].1.len(), 2); // tx2: kv + state
    assert_eq!(transactions[2].1.len(), 2); // tx3: json + event
}

/// Test: Mixed committed and uncommitted transactions
#[test]
fn test_mixed_committed_uncommitted() {
    let temp_dir = create_test_dir();
    let wal_path = temp_dir.path().join("wal.dat");

    {
        let mut writer = WalWriter::open(&wal_path, DurabilityMode::Strict).unwrap();

        // TX1 - committed
        let mut tx1 = Transaction::new();
        tx1.kv_put("tx1_key", "value1");
        writer.commit_atomic(tx1).unwrap();

        // TX2 - uncommitted (orphaned)
        let tx2_id = writer.begin_transaction();
        writer
            .write_tx_entry(tx2_id, WalEntryType::KvPut, b"tx2_key=value2".to_vec())
            .unwrap();
        // No commit

        // TX3 - committed
        let mut tx3 = Transaction::new();
        tx3.kv_put("tx3_key", "value3");
        writer.commit_atomic(tx3).unwrap();

        // TX4 - aborted
        let tx4_id = writer.begin_transaction();
        writer
            .write_tx_entry(tx4_id, WalEntryType::KvPut, b"tx4_key=value4".to_vec())
            .unwrap();
        writer.abort_transaction(tx4_id).unwrap();

        // TX5 - committed
        let mut tx5 = Transaction::new();
        tx5.kv_put("tx5_key", "value5");
        writer.commit_atomic(tx5).unwrap();
    }

    let (transactions, result) =
        RecoveryEngine::replay_wal_committed(&wal_path, 0, &RecoveryOptions::default()).unwrap();

    // TX1, TX3, TX5 committed; TX2 orphaned; TX4 aborted
    assert_eq!(transactions.len(), 3);
    assert_eq!(result.transactions_recovered, 3);
    assert_eq!(result.orphaned_transactions, 1);
    assert_eq!(result.aborted_transactions, 1);
}

// ============================================================================
// Large Transaction Tests
// ============================================================================

/// Test: Large transaction with many entries
#[test]
fn test_large_transaction() {
    let temp_dir = create_test_dir();
    let wal_path = temp_dir.path().join("wal.dat");

    {
        let mut writer = WalWriter::open(&wal_path, DurabilityMode::Strict).unwrap();

        let mut tx = Transaction::new();
        for i in 0..1000 {
            tx.kv_put(format!("key_{}", i), format!("value_{}", i));
        }
        writer.commit_atomic(tx).unwrap();
    }

    let (transactions, result) =
        RecoveryEngine::replay_wal_committed(&wal_path, 0, &RecoveryOptions::default()).unwrap();

    // All 1000 keys should be present
    assert_eq!(transactions.len(), 1);
    assert_eq!(transactions[0].1.len(), 1000);
    assert_eq!(result.transactions_recovered, 1);
}

/// Test: Large cross-primitive transaction
#[test]
fn test_large_cross_primitive_transaction() {
    let temp_dir = create_test_dir();
    let wal_path = temp_dir.path().join("wal.dat");

    {
        let mut writer = WalWriter::open(&wal_path, DurabilityMode::Strict).unwrap();

        let mut tx = Transaction::new();

        // 100 KV entries
        for i in 0..100 {
            tx.kv_put(format!("kv_{}", i), format!("value_{}", i));
        }

        // 50 JSON entries
        for i in 0..50 {
            tx.json_set(
                format!("doc_{}", i),
                format!(r#"{{"id":{}}}"#, i).into_bytes(),
            );
        }

        // 30 State entries
        for i in 0..30 {
            tx.state_set(format!("state_{}", i), format!("value_{}", i));
        }

        // 20 Event entries
        for i in 0..20 {
            tx.event_append(format!("event_{}", i).into_bytes());
        }

        writer.commit_atomic(tx).unwrap();
    }

    let (transactions, result) =
        RecoveryEngine::replay_wal_committed(&wal_path, 0, &RecoveryOptions::default()).unwrap();

    assert_eq!(transactions.len(), 1);
    assert_eq!(transactions[0].1.len(), 200); // 100 + 50 + 30 + 20
    assert_eq!(result.transactions_recovered, 1);
}

// ============================================================================
// Determinism Tests
// ============================================================================

/// Test: Recovery is deterministic
#[test]
fn test_recovery_deterministic() {
    let temp_dir = create_test_dir();
    let wal_path = temp_dir.path().join("wal.dat");

    // Create and populate
    {
        let mut writer = WalWriter::open(&wal_path, DurabilityMode::Strict).unwrap();

        for i in 0..100 {
            let mut tx = Transaction::new();
            tx.kv_put(format!("key{}", i), format!("value{}", i));
            tx.json_set(
                format!("doc{}", i),
                format!(r#"{{"i":{}}}"#, i).into_bytes(),
            );
            writer.commit_atomic(tx).unwrap();
        }
    }

    // Recover twice
    let (txs1, result1) =
        RecoveryEngine::replay_wal_committed(&wal_path, 0, &RecoveryOptions::default()).unwrap();
    let (txs2, result2) =
        RecoveryEngine::replay_wal_committed(&wal_path, 0, &RecoveryOptions::default()).unwrap();

    // Must be identical
    assert_eq!(
        result1.transactions_recovered,
        result2.transactions_recovered
    );
    assert_eq!(txs1.len(), txs2.len());

    for i in 0..txs1.len() {
        assert_eq!(txs1[i].0, txs2[i].0); // Same tx_id
        assert_eq!(txs1[i].1.len(), txs2[i].1.len()); // Same entry count
    }
}

/// Test: Order preservation across multiple recoveries
#[test]
fn test_order_preservation() {
    let temp_dir = create_test_dir();
    let wal_path = temp_dir.path().join("wal.dat");

    let mut original_tx_ids = Vec::new();

    {
        let mut writer = WalWriter::open(&wal_path, DurabilityMode::Strict).unwrap();

        for i in 0..50 {
            let mut tx = Transaction::new();
            tx.kv_put(format!("key{}", i), format!("value{}", i));
            original_tx_ids.push(tx.id());
            writer.commit_atomic(tx).unwrap();
        }
    }

    // Recover
    let (transactions, _) =
        RecoveryEngine::replay_wal_committed(&wal_path, 0, &RecoveryOptions::default()).unwrap();

    // Order should be preserved
    assert_eq!(transactions.len(), 50);
    for (i, (tx_id, _)) in transactions.iter().enumerate() {
        assert_eq!(*tx_id, original_tx_ids[i]);
    }
}

// ============================================================================
// Entry Reconstruction Tests
// ============================================================================

/// Test: Rebuild transaction from recovered entries
#[test]
fn test_rebuild_transaction() {
    let temp_dir = create_test_dir();
    let wal_path = temp_dir.path().join("wal.dat");

    let original_tx_id;
    {
        let mut writer = WalWriter::open(&wal_path, DurabilityMode::Strict).unwrap();

        let mut tx = Transaction::new();
        tx.kv_put("key1", "value1")
            .json_set("doc1", b"{}".to_vec())
            .state_set("state1", "active");

        original_tx_id = tx.id();
        writer.commit_atomic(tx).unwrap();
    }

    // Recover
    let (transactions, _) =
        RecoveryEngine::replay_wal_committed(&wal_path, 0, &RecoveryOptions::default()).unwrap();

    let (tx_id, entries) = &transactions[0];
    let rebuilt = RecoveryEngine::rebuild_transaction(*tx_id, entries);

    assert_eq!(rebuilt.id(), original_tx_id);
    assert_eq!(rebuilt.len(), 3);
}

/// Test: Convert entries to TxEntry format
#[test]
fn test_entries_to_tx_entries() {
    let temp_dir = create_test_dir();
    let wal_path = temp_dir.path().join("wal.dat");

    {
        let mut writer = WalWriter::open(&wal_path, DurabilityMode::Strict).unwrap();

        let mut tx = Transaction::new();
        tx.kv_put("key", "value");
        tx.json_create("doc", b"{}".to_vec());
        tx.event_append(b"event".to_vec());
        writer.commit_atomic(tx).unwrap();
    }

    let (transactions, _) =
        RecoveryEngine::replay_wal_committed(&wal_path, 0, &RecoveryOptions::default()).unwrap();

    let (_, entries) = &transactions[0];
    let tx_entries = RecoveryEngine::entries_to_tx_entries(entries);

    assert_eq!(tx_entries.len(), 3);

    use strata_durability::TxEntry;
    assert!(matches!(tx_entries[0], TxEntry::KvPut { .. }));
    assert!(matches!(tx_entries[1], TxEntry::JsonCreate { .. }));
    assert!(matches!(tx_entries[2], TxEntry::EventAppend { .. }));
}

// ============================================================================
// Edge Cases
// ============================================================================

/// Test: Empty transaction
#[test]
fn test_empty_transaction() {
    let temp_dir = create_test_dir();
    let wal_path = temp_dir.path().join("wal.dat");

    {
        let mut writer = WalWriter::open(&wal_path, DurabilityMode::Strict).unwrap();

        // Empty transaction (nothing written)
        let tx = Transaction::new();
        writer.commit_atomic(tx).unwrap();

        // Non-empty transaction
        let mut tx2 = Transaction::new();
        tx2.kv_put("key", "value");
        writer.commit_atomic(tx2).unwrap();
    }

    let (transactions, result) =
        RecoveryEngine::replay_wal_committed(&wal_path, 0, &RecoveryOptions::default()).unwrap();

    // Only non-empty transaction should appear
    assert_eq!(transactions.len(), 1);
    assert_eq!(result.transactions_recovered, 1);
}

/// Test: Many small transactions
#[test]
fn test_many_small_transactions() {
    let temp_dir = create_test_dir();
    let wal_path = temp_dir.path().join("wal.dat");

    {
        let mut writer = WalWriter::open(&wal_path, DurabilityMode::Strict).unwrap();

        for i in 0..1000 {
            let mut tx = Transaction::new();
            tx.kv_put(format!("key_{}", i), format!("value_{}", i));
            writer.commit_atomic(tx).unwrap();
        }
    }

    let (transactions, result) =
        RecoveryEngine::replay_wal_committed(&wal_path, 0, &RecoveryOptions::default()).unwrap();

    assert_eq!(transactions.len(), 1000);
    assert_eq!(result.transactions_recovered, 1000);
}

/// Test: Transaction with all operations
#[test]
fn test_all_operations() {
    let temp_dir = create_test_dir();
    let wal_path = temp_dir.path().join("wal.dat");

    {
        let mut writer = WalWriter::open(&wal_path, DurabilityMode::Strict).unwrap();

        let mut tx = Transaction::new();

        // KV operations
        tx.kv_put("key1", "value1");
        tx.kv_delete("key2");

        // JSON operations
        tx.json_create("doc1", b"{}".to_vec());
        tx.json_set("doc2", b"{\"a\":1}".to_vec());
        tx.json_delete("doc3");
        tx.json_patch("doc4", b"[]".to_vec());

        // Event operation
        tx.event_append(b"event".to_vec());

        // State operations
        tx.state_init("state1", "init");
        tx.state_set("state2", "set");
        tx.state_transition("state3", "from", "to");

        // Run operations
        tx.run_create(b"create".to_vec());
        tx.run_begin(b"begin".to_vec());
        tx.run_update(b"update".to_vec());
        tx.run_end(b"end".to_vec());

        writer.commit_atomic(tx).unwrap();
    }

    let (transactions, result) =
        RecoveryEngine::replay_wal_committed(&wal_path, 0, &RecoveryOptions::default()).unwrap();

    assert_eq!(transactions.len(), 1);
    assert_eq!(result.transactions_recovered, 1);

    // Should have 14 entries total
    let (_, entries) = &transactions[0];
    assert_eq!(entries.len(), 14);

    // Verify entry types
    assert_eq!(entries[0].entry_type, WalEntryType::KvPut);
    assert_eq!(entries[1].entry_type, WalEntryType::KvDelete);
    assert_eq!(entries[2].entry_type, WalEntryType::JsonCreate);
    assert_eq!(entries[3].entry_type, WalEntryType::JsonSet);
    assert_eq!(entries[4].entry_type, WalEntryType::JsonDelete);
    assert_eq!(entries[5].entry_type, WalEntryType::JsonPatch);
    assert_eq!(entries[6].entry_type, WalEntryType::EventAppend);
    assert_eq!(entries[7].entry_type, WalEntryType::StateInit);
    assert_eq!(entries[8].entry_type, WalEntryType::StateSet);
    assert_eq!(entries[9].entry_type, WalEntryType::StateTransition);
    assert_eq!(entries[10].entry_type, WalEntryType::RunCreate);
    assert_eq!(entries[11].entry_type, WalEntryType::RunBegin);
    assert_eq!(entries[12].entry_type, WalEntryType::RunUpdate);
    assert_eq!(entries[13].entry_type, WalEntryType::RunEnd);
}
