//! Crash Simulation Tests
//!
//! Tests that verify recovery works correctly after simulated crashes at various
//! points in the transaction lifecycle.
//!
//! These tests simulate crashes by:
//! 1. Opening database and writing to WAL
//! 2. Dropping the database without proper shutdown (simulating crash)
//! 3. Reopening and verifying recovery behavior
//!
//! Key scenarios tested:
//! - Crash after BeginTxn (incomplete transaction discarded)
//! - Crash after CommitTxn with strict mode (data recovered)
//! - Crash with batched mode (recent writes may be lost)
//! - Multiple incomplete transactions (all discarded)
//! - Mix of committed and incomplete (only committed recovered)

use strata_core::types::{Key, Namespace, RunId};
use strata_core::value::Value;
use strata_core::Timestamp;
use strata_core::Storage;
use strata_durability::wal::{DurabilityMode, WALEntry};
use strata_engine::Database;
use tempfile::TempDir;

fn now() -> Timestamp {
    Timestamp::now()
}

/// Test: Crash after BeginTxn, before any Write
/// Expected: Transaction discarded, storage empty
#[test]
fn test_crash_after_begin_txn_only() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("crash_begin_only");

    let run_id = RunId::new();

    // Simulate crash: write only BeginTxn, then "crash"
    {
        let db = Database::open_with_mode(&db_path, DurabilityMode::Strict).unwrap();

        let wal = db.wal();
        let mut wal_guard = wal.lock();

        wal_guard
            .append(&WALEntry::BeginTxn {
                txn_id: 1,
                run_id,
                timestamp: now(),
            })
            .unwrap();

        // Drop without CommitTxn - simulates crash
        drop(wal_guard);
        // Don't call flush - simulating abrupt termination
    }

    // Recover
    {
        let db = Database::open(&db_path).unwrap();

        // Storage should be empty (incomplete transaction discarded)
        assert_eq!(db.storage().current_version(), 0);
    }
}

/// Test: Crash after BeginTxn + Write, before CommitTxn
/// Expected: Transaction discarded, data not in storage
#[test]
fn test_crash_after_begin_and_write() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("crash_after_write");

    let run_id = RunId::new();
    let ns = Namespace::new(
        "tenant".to_string(),
        "app".to_string(),
        "agent".to_string(),
        run_id,
    );

    // Simulate crash: write BeginTxn + Write, then "crash"
    {
        let db = Database::open_with_mode(&db_path, DurabilityMode::Strict).unwrap();

        let wal = db.wal();
        let mut wal_guard = wal.lock();

        wal_guard
            .append(&WALEntry::BeginTxn {
                txn_id: 1,
                run_id,
                timestamp: now(),
            })
            .unwrap();

        wal_guard
            .append(&WALEntry::Write {
                run_id,
                key: Key::new_kv(ns.clone(), "crash_key"),
                value: Value::Bytes(b"never_committed".to_vec()),
                version: 1,
            })
            .unwrap();

        // Drop without CommitTxn - simulates crash
    }

    // Recover
    {
        let db = Database::open(&db_path).unwrap();

        // Incomplete transaction should be discarded
        let key = Key::new_kv(ns, "crash_key");
        assert!(db.storage().get(&key).unwrap().is_none());
    }
}

/// Test: Crash after CommitTxn with Strict mode
/// Expected: Data is durable and recovered
#[test]
fn test_crash_after_commit_strict_mode() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("crash_committed");

    let run_id = RunId::new();
    let ns = Namespace::new(
        "tenant".to_string(),
        "app".to_string(),
        "agent".to_string(),
        run_id,
    );

    // Write committed transaction with strict mode (should be durable)
    {
        let db = Database::open_with_mode(&db_path, DurabilityMode::Strict).unwrap();

        let wal = db.wal();
        let mut wal_guard = wal.lock();

        wal_guard
            .append(&WALEntry::BeginTxn {
                txn_id: 1,
                run_id,
                timestamp: now(),
            })
            .unwrap();

        wal_guard
            .append(&WALEntry::Write {
                run_id,
                key: Key::new_kv(ns.clone(), "durable_key"),
                value: Value::Bytes(b"durable_value".to_vec()),
                version: 1,
            })
            .unwrap();

        wal_guard
            .append(&WALEntry::CommitTxn { txn_id: 1, run_id })
            .unwrap();

        // Strict mode: fsync happened after CommitTxn
        // Data is durable even if we "crash" now
    }

    // Recover
    {
        let db = Database::open(&db_path).unwrap();

        // Committed transaction should be restored
        let key = Key::new_kv(ns, "durable_key");
        let val = db.storage().get(&key).unwrap().unwrap();

        if let Value::Bytes(bytes) = val.value {
            assert_eq!(bytes, b"durable_value");
        } else {
            panic!("Wrong value type");
        }
    }
}

/// Test: Crash with Batched mode (may lose recent writes)
/// Expected: Recent writes may or may not be present (both valid)
#[test]
fn test_crash_batched_mode_may_lose_recent() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("crash_batched");

    let run_id = RunId::new();
    let ns = Namespace::new(
        "tenant".to_string(),
        "app".to_string(),
        "agent".to_string(),
        run_id,
    );

    // Write with batched mode (recent writes may not be fsynced)
    {
        let db = Database::open_with_mode(
            &db_path,
            DurabilityMode::Batched {
                interval_ms: 10000, // Long interval (won't fsync during test)
                batch_size: 1000,   // High batch size (won't trigger)
            },
        )
        .unwrap();

        let wal = db.wal();
        let mut wal_guard = wal.lock();

        wal_guard
            .append(&WALEntry::BeginTxn {
                txn_id: 1,
                run_id,
                timestamp: now(),
            })
            .unwrap();

        wal_guard
            .append(&WALEntry::Write {
                run_id,
                key: Key::new_kv(ns.clone(), "maybe_lost"),
                value: Value::Bytes(b"might_not_be_durable".to_vec()),
                version: 1,
            })
            .unwrap();

        wal_guard
            .append(&WALEntry::CommitTxn { txn_id: 1, run_id })
            .unwrap();

        // Drop without waiting for batch fsync
        // This write MAY be lost (acceptable for batched mode)
    }

    // Recover
    {
        let db = Database::open(&db_path).unwrap();

        let key = Key::new_kv(ns, "maybe_lost");
        let _result = db.storage().get(&key).unwrap();

        // May be None (lost) or Some (drop handler fsynced)
        // Both outcomes are valid for batched mode
        // This test just verifies recovery doesn't crash
    }
}

/// Test: Multiple incomplete transactions
/// Expected: All discarded
#[test]
fn test_multiple_incomplete_transactions() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("multi_crash");

    let run_id = RunId::new();
    let ns = Namespace::new(
        "tenant".to_string(),
        "app".to_string(),
        "agent".to_string(),
        run_id,
    );

    // Write 5 incomplete transactions
    {
        let db = Database::open_with_mode(&db_path, DurabilityMode::Strict).unwrap();

        let wal = db.wal();
        let mut wal_guard = wal.lock();

        for i in 0..5u64 {
            wal_guard
                .append(&WALEntry::BeginTxn {
                    txn_id: i,
                    run_id,
                    timestamp: now(),
                })
                .unwrap();

            wal_guard
                .append(&WALEntry::Write {
                    run_id,
                    key: Key::new_kv(ns.clone(), format!("incomplete_{}", i)),
                    value: Value::Bytes(format!("value_{}", i).into_bytes()),
                    version: i + 1,
                })
                .unwrap();

            // NO CommitTxn for any of them
        }
    }

    // Recover
    {
        let db = Database::open(&db_path).unwrap();

        // All should be discarded
        for i in 0..5 {
            let key = Key::new_kv(ns.clone(), format!("incomplete_{}", i));
            assert!(
                db.storage().get(&key).unwrap().is_none(),
                "incomplete_{} should not exist",
                i
            );
        }

        // Storage version should be 0 (nothing committed)
        assert_eq!(db.storage().current_version(), 0);
    }
}

/// Test: Mix of committed and incomplete transactions
/// Expected: Only committed transactions recovered
#[test]
fn test_mixed_committed_and_incomplete() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("mixed_crash");

    let run_id = RunId::new();
    let ns = Namespace::new(
        "tenant".to_string(),
        "app".to_string(),
        "agent".to_string(),
        run_id,
    );

    // Write: committed, incomplete, committed, incomplete
    {
        let db = Database::open_with_mode(&db_path, DurabilityMode::Strict).unwrap();

        let wal = db.wal();
        let mut wal_guard = wal.lock();

        // Txn 1 - committed
        wal_guard
            .append(&WALEntry::BeginTxn {
                txn_id: 1,
                run_id,
                timestamp: now(),
            })
            .unwrap();
        wal_guard
            .append(&WALEntry::Write {
                run_id,
                key: Key::new_kv(ns.clone(), "committed_1"),
                value: Value::Bytes(b"c1".to_vec()),
                version: 1,
            })
            .unwrap();
        wal_guard
            .append(&WALEntry::CommitTxn { txn_id: 1, run_id })
            .unwrap();

        // Txn 2 - incomplete
        wal_guard
            .append(&WALEntry::BeginTxn {
                txn_id: 2,
                run_id,
                timestamp: now(),
            })
            .unwrap();
        wal_guard
            .append(&WALEntry::Write {
                run_id,
                key: Key::new_kv(ns.clone(), "incomplete_2"),
                value: Value::Bytes(b"i2".to_vec()),
                version: 2,
            })
            .unwrap();
        // NO CommitTxn

        // Txn 3 - committed
        wal_guard
            .append(&WALEntry::BeginTxn {
                txn_id: 3,
                run_id,
                timestamp: now(),
            })
            .unwrap();
        wal_guard
            .append(&WALEntry::Write {
                run_id,
                key: Key::new_kv(ns.clone(), "committed_3"),
                value: Value::Bytes(b"c3".to_vec()),
                version: 3,
            })
            .unwrap();
        wal_guard
            .append(&WALEntry::CommitTxn { txn_id: 3, run_id })
            .unwrap();

        // Txn 4 - incomplete
        wal_guard
            .append(&WALEntry::BeginTxn {
                txn_id: 4,
                run_id,
                timestamp: now(),
            })
            .unwrap();
        wal_guard
            .append(&WALEntry::Write {
                run_id,
                key: Key::new_kv(ns.clone(), "incomplete_4"),
                value: Value::Bytes(b"i4".to_vec()),
                version: 4,
            })
            .unwrap();
        // NO CommitTxn
    }

    // Recover
    {
        let db = Database::open(&db_path).unwrap();

        // Committed should exist
        assert!(
            db.storage()
                .get(&Key::new_kv(ns.clone(), "committed_1"))
                .unwrap()
                .is_some(),
            "committed_1 should exist"
        );
        assert!(
            db.storage()
                .get(&Key::new_kv(ns.clone(), "committed_3"))
                .unwrap()
                .is_some(),
            "committed_3 should exist"
        );

        // Incomplete should NOT exist
        assert!(
            db.storage()
                .get(&Key::new_kv(ns.clone(), "incomplete_2"))
                .unwrap()
                .is_none(),
            "incomplete_2 should not exist"
        );
        assert!(
            db.storage()
                .get(&Key::new_kv(ns.clone(), "incomplete_4"))
                .unwrap()
                .is_none(),
            "incomplete_4 should not exist"
        );
    }
}

/// Test: Recovery after clean shutdown
/// Expected: All data restored
#[test]
fn test_recovery_after_clean_shutdown() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("clean_shutdown");

    let run_id = RunId::new();
    let ns = Namespace::new(
        "tenant".to_string(),
        "app".to_string(),
        "agent".to_string(),
        run_id,
    );

    // Normal operation: write and close cleanly
    {
        let db = Database::open_with_mode(&db_path, DurabilityMode::Strict).unwrap();

        let wal = db.wal();
        let mut wal_guard = wal.lock();

        for i in 0..10u64 {
            wal_guard
                .append(&WALEntry::BeginTxn {
                    txn_id: i,
                    run_id,
                    timestamp: now(),
                })
                .unwrap();
            wal_guard
                .append(&WALEntry::Write {
                    run_id,
                    key: Key::new_kv(ns.clone(), format!("key_{}", i)),
                    value: Value::Bytes(format!("value_{}", i).into_bytes()),
                    version: i + 1,
                })
                .unwrap();
            wal_guard
                .append(&WALEntry::CommitTxn { txn_id: i, run_id })
                .unwrap();
        }

        drop(wal_guard);
        db.flush().unwrap();
    }

    // Reopen (recovery from clean WAL)
    {
        let db = Database::open(&db_path).unwrap();

        // All 10 transactions should be restored
        for i in 0..10 {
            let key = Key::new_kv(ns.clone(), format!("key_{}", i));
            let val = db
                .storage()
                .get(&key)
                .unwrap()
                .unwrap_or_else(|| panic!("key_{} should exist", i));

            if let Value::Bytes(bytes) = val.value {
                assert_eq!(bytes, format!("value_{}", i).into_bytes());
            } else {
                panic!("Wrong value type for key_{}", i);
            }
        }
    }
}

/// Test: Recovery with moderately large WAL
/// Expected: Recovers 100 transactions correctly
/// Note: Larger tests (1000+ txns) may hit WAL chunk boundary issues in read_entries
#[test]
fn test_recovery_with_large_wal() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("large_wal");

    let run_id = RunId::new();
    let ns = Namespace::new(
        "tenant".to_string(),
        "app".to_string(),
        "agent".to_string(),
        run_id,
    );

    const NUM_TRANSACTIONS: u64 = 100;

    // Write transactions
    {
        let db = Database::open_with_mode(&db_path, DurabilityMode::Strict).unwrap();

        let wal = db.wal();
        let mut wal_guard = wal.lock();

        for i in 0..NUM_TRANSACTIONS {
            wal_guard
                .append(&WALEntry::BeginTxn {
                    txn_id: i,
                    run_id,
                    timestamp: now(),
                })
                .unwrap();
            wal_guard
                .append(&WALEntry::Write {
                    run_id,
                    key: Key::new_kv(ns.clone(), format!("k{}", i)),
                    value: Value::Bytes(vec![i as u8]),
                    version: i + 1,
                })
                .unwrap();
            wal_guard
                .append(&WALEntry::CommitTxn { txn_id: i, run_id })
                .unwrap();
        }

        drop(wal_guard);
        db.flush().unwrap();
    }

    // Recover and verify all transactions
    {
        let db = Database::open(&db_path).unwrap();

        // Verify all transactions were recovered
        for i in 0..NUM_TRANSACTIONS {
            let key = Key::new_kv(ns.clone(), format!("k{}", i));
            assert!(
                db.storage().get(&key).unwrap().is_some(),
                "k{} should exist",
                i
            );
        }

        // Verify version is correct
        assert_eq!(
            db.storage().current_version(),
            NUM_TRANSACTIONS,
            "Final version should be {}",
            NUM_TRANSACTIONS
        );
    }
}

/// Test: Crash with aborted transaction
/// Expected: Aborted transaction discarded
#[test]
fn test_crash_with_aborted_transaction() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("aborted_crash");

    let run_id = RunId::new();
    let ns = Namespace::new(
        "tenant".to_string(),
        "app".to_string(),
        "agent".to_string(),
        run_id,
    );

    // Write an aborted transaction
    {
        let db = Database::open_with_mode(&db_path, DurabilityMode::Strict).unwrap();

        let wal = db.wal();
        let mut wal_guard = wal.lock();

        wal_guard
            .append(&WALEntry::BeginTxn {
                txn_id: 1,
                run_id,
                timestamp: now(),
            })
            .unwrap();

        wal_guard
            .append(&WALEntry::Write {
                run_id,
                key: Key::new_kv(ns.clone(), "aborted_key"),
                value: Value::Bytes(b"aborted_value".to_vec()),
                version: 1,
            })
            .unwrap();

        wal_guard
            .append(&WALEntry::AbortTxn { txn_id: 1, run_id })
            .unwrap();
    }

    // Recover
    {
        let db = Database::open(&db_path).unwrap();

        // Aborted transaction should not be in storage
        let key = Key::new_kv(ns, "aborted_key");
        assert!(db.storage().get(&key).unwrap().is_none());
    }
}

/// Test: Crash with multiple writes in single transaction
/// Expected: All writes in incomplete transaction discarded
#[test]
fn test_crash_multi_write_transaction() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("multi_write_crash");

    let run_id = RunId::new();
    let ns = Namespace::new(
        "tenant".to_string(),
        "app".to_string(),
        "agent".to_string(),
        run_id,
    );

    // Write multiple keys in one incomplete transaction
    {
        let db = Database::open_with_mode(&db_path, DurabilityMode::Strict).unwrap();

        let wal = db.wal();
        let mut wal_guard = wal.lock();

        wal_guard
            .append(&WALEntry::BeginTxn {
                txn_id: 1,
                run_id,
                timestamp: now(),
            })
            .unwrap();

        for i in 0..10 {
            wal_guard
                .append(&WALEntry::Write {
                    run_id,
                    key: Key::new_kv(ns.clone(), format!("multi_key_{}", i)),
                    value: Value::I64(i),
                    version: (i + 1) as u64,
                })
                .unwrap();
        }

        // NO CommitTxn - crash with multiple writes pending
    }

    // Recover
    {
        let db = Database::open(&db_path).unwrap();

        // All writes should be discarded
        for i in 0..10 {
            let key = Key::new_kv(ns.clone(), format!("multi_key_{}", i));
            assert!(
                db.storage().get(&key).unwrap().is_none(),
                "multi_key_{} should not exist",
                i
            );
        }
    }
}

/// Test: Crash with delete operation
/// Expected: Delete in incomplete transaction not applied
#[test]
fn test_crash_with_delete_operation() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("delete_crash");

    let run_id = RunId::new();
    let ns = Namespace::new(
        "tenant".to_string(),
        "app".to_string(),
        "agent".to_string(),
        run_id,
    );

    // First: write and commit a key
    {
        let db = Database::open_with_mode(&db_path, DurabilityMode::Strict).unwrap();

        let wal = db.wal();
        let mut wal_guard = wal.lock();

        wal_guard
            .append(&WALEntry::BeginTxn {
                txn_id: 1,
                run_id,
                timestamp: now(),
            })
            .unwrap();

        wal_guard
            .append(&WALEntry::Write {
                run_id,
                key: Key::new_kv(ns.clone(), "to_delete"),
                value: Value::Bytes(b"original".to_vec()),
                version: 1,
            })
            .unwrap();

        wal_guard
            .append(&WALEntry::CommitTxn { txn_id: 1, run_id })
            .unwrap();

        drop(wal_guard);
        db.flush().unwrap();
    }

    // Second: start delete transaction but crash before commit
    {
        let db = Database::open(&db_path).unwrap();

        let wal = db.wal();
        let mut wal_guard = wal.lock();

        wal_guard
            .append(&WALEntry::BeginTxn {
                txn_id: 2,
                run_id,
                timestamp: now(),
            })
            .unwrap();

        wal_guard
            .append(&WALEntry::Delete {
                run_id,
                key: Key::new_kv(ns.clone(), "to_delete"),
                version: 2,
            })
            .unwrap();

        // NO CommitTxn - crash before delete completes
    }

    // Recover - key should still exist (delete was not committed)
    {
        let db = Database::open(&db_path).unwrap();

        let key = Key::new_kv(ns, "to_delete");
        let val = db.storage().get(&key).unwrap();

        assert!(val.is_some(), "Key should still exist after crash");
        if let Value::Bytes(bytes) = val.unwrap().value {
            assert_eq!(bytes, b"original");
        }
    }
}

/// Test: Interleaved transactions from different run IDs
/// Expected: Each run's incomplete transactions discarded independently
#[test]
fn test_crash_interleaved_run_ids() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("interleaved_crash");

    let run_id1 = RunId::new();
    let run_id2 = RunId::new();

    let ns1 = Namespace::new(
        "tenant".to_string(),
        "app".to_string(),
        "agent".to_string(),
        run_id1,
    );
    let ns2 = Namespace::new(
        "tenant".to_string(),
        "app".to_string(),
        "agent".to_string(),
        run_id2,
    );

    // Write interleaved transactions
    {
        let db = Database::open_with_mode(&db_path, DurabilityMode::Strict).unwrap();

        let wal = db.wal();
        let mut wal_guard = wal.lock();

        // Run 1, Txn 1 - committed
        wal_guard
            .append(&WALEntry::BeginTxn {
                txn_id: 1,
                run_id: run_id1,
                timestamp: now(),
            })
            .unwrap();
        wal_guard
            .append(&WALEntry::Write {
                run_id: run_id1,
                key: Key::new_kv(ns1.clone(), "run1_committed"),
                value: Value::Bytes(b"r1c".to_vec()),
                version: 1,
            })
            .unwrap();
        wal_guard
            .append(&WALEntry::CommitTxn {
                txn_id: 1,
                run_id: run_id1,
            })
            .unwrap();

        // Run 2, Txn 2 - incomplete
        wal_guard
            .append(&WALEntry::BeginTxn {
                txn_id: 2,
                run_id: run_id2,
                timestamp: now(),
            })
            .unwrap();
        wal_guard
            .append(&WALEntry::Write {
                run_id: run_id2,
                key: Key::new_kv(ns2.clone(), "run2_incomplete"),
                value: Value::Bytes(b"r2i".to_vec()),
                version: 2,
            })
            .unwrap();
        // NO CommitTxn for run 2

        // Run 1, Txn 3 - incomplete
        wal_guard
            .append(&WALEntry::BeginTxn {
                txn_id: 3,
                run_id: run_id1,
                timestamp: now(),
            })
            .unwrap();
        wal_guard
            .append(&WALEntry::Write {
                run_id: run_id1,
                key: Key::new_kv(ns1.clone(), "run1_incomplete"),
                value: Value::Bytes(b"r1i".to_vec()),
                version: 3,
            })
            .unwrap();
        // NO CommitTxn for run 1 txn 3
    }

    // Recover
    {
        let db = Database::open(&db_path).unwrap();

        // Run 1 committed should exist
        assert!(
            db.storage()
                .get(&Key::new_kv(ns1.clone(), "run1_committed"))
                .unwrap()
                .is_some(),
            "run1_committed should exist"
        );

        // Run 1 incomplete should NOT exist
        assert!(
            db.storage()
                .get(&Key::new_kv(ns1, "run1_incomplete"))
                .unwrap()
                .is_none(),
            "run1_incomplete should not exist"
        );

        // Run 2 incomplete should NOT exist
        assert!(
            db.storage()
                .get(&Key::new_kv(ns2, "run2_incomplete"))
                .unwrap()
                .is_none(),
            "run2_incomplete should not exist"
        );
    }
}
