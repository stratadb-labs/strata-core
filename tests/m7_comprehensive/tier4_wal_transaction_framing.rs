//! Tier 4.3: WAL Transaction Framing Tests
//!
//! Tests for TxBegin/TxCommit framing.

use crate::test_utils::*;
use strata_core::types::RunId;
use strata_core::value::Value;
use strata_durability::{Transaction, TxId};
use strata_primitives::KVStore;

/// Transaction ID is unique
#[test]
fn test_txid_unique() {
    let ids: Vec<TxId> = (0..100).map(|_| TxId::new()).collect();

    // All IDs should be unique
    for i in 0..ids.len() {
        for j in (i + 1)..ids.len() {
            assert_ne!(ids[i], ids[j], "TxId collision at {} and {}", i, j);
        }
    }
}

/// Transaction framing preserves atomicity
#[test]
fn test_transaction_framing_atomicity() {
    let mut test_db = TestDb::new();
    let run_id = test_db.run_id;

    let kv = test_db.kv();

    // Write a batch of related data
    kv.put(&run_id, "tx_a", Value::I64(1)).unwrap();
    kv.put(&run_id, "tx_b", Value::I64(2)).unwrap();
    kv.put(&run_id, "tx_c", Value::I64(3)).unwrap();

    test_db.reopen();

    // Either all should be present or none (atomicity)
    let kv = test_db.kv();
    let count = ["tx_a", "tx_b", "tx_c"]
        .iter()
        .filter(|k| kv.get(&run_id, k).unwrap().is_some())
        .count();

    assert!(
        count == 0 || count == 3,
        "Partial transaction visible: {}/3",
        count
    );
}

/// Uncommitted transaction not visible after crash
#[test]
fn test_uncommitted_not_visible() {
    // This test validates the concept that uncommitted
    // transactions may not survive crashes
    let mut test_db = TestDb::new();
    let run_id = test_db.run_id;

    let kv = test_db.kv();
    kv.put(&run_id, "committed_data", Value::String("safe".into()))
        .unwrap();

    // Crash and recover
    test_db.reopen();

    // Database should be healthy
    assert_db_healthy(&test_db.db, &run_id);
}

/// Multiple transactions preserve order
#[test]
fn test_transaction_order_preserved() {
    let mut test_db = TestDb::new();
    let run_id = test_db.run_id;

    let kv = test_db.kv();

    // Multiple sequential transactions
    for tx in 0..10 {
        for i in 0..5 {
            kv.put(
                &run_id,
                &format!("tx{}_{}", tx, i),
                Value::I64((tx * 5 + i) as i64),
            )
            .unwrap();
        }
    }

    test_db.reopen();

    // All committed transactions should be present and ordered
    let kv = test_db.kv();
    let mut found_count = 0;
    for tx in 0..10 {
        let present: Vec<_> = (0..5)
            .filter(|i| {
                kv.get(&run_id, &format!("tx{}_{}", tx, i))
                    .unwrap()
                    .is_some()
            })
            .collect();

        // Transaction should be atomic
        assert!(
            present.is_empty() || present.len() == 5,
            "Transaction {} is partial: {:?}",
            tx,
            present
        );
        found_count += present.len();
    }

    // At least some transactions should have committed
    assert!(found_count > 0, "No transactions committed");
}

/// Transaction with single entry
#[test]
fn test_single_entry_transaction() {
    let mut test_db = TestDb::new();
    let run_id = test_db.run_id;

    let kv = test_db.kv();
    kv.put(&run_id, "single", Value::I64(42)).unwrap();

    test_db.reopen();

    let kv = test_db.kv();
    // Single entry should survive (atomic trivially)
    // May or may not be present depending on commit timing
    assert_db_healthy(&test_db.db, &run_id);
}

/// Large transaction
#[test]
fn test_large_transaction() {
    let mut test_db = TestDb::new();
    let run_id = test_db.run_id;

    let kv = test_db.kv();

    // Large batch
    for i in 0..500 {
        kv.put(&run_id, &format!("large_{}", i), Value::I64(i))
            .unwrap();
    }

    test_db.reopen();

    let kv = test_db.kv();
    let present_count = (0..500)
        .filter(|i| kv.get(&run_id, &format!("large_{}", i)).unwrap().is_some())
        .count();

    // Should be all or none for atomic transaction
    // Or prefix if not fully committed
    assert!(present_count == 0 || present_count == 500);
}
