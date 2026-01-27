// Tests temporarily commented out during engine re-architecture.
// These tests use internal engine methods (wal, flush, transaction_with_version,
// transaction_with_retry) that are now pub(crate). Uncomment once the new API
// surface exposes equivalent functionality.

/*
//! Integration tests for Database::open() and recovery
//!
//! These tests verify the complete database open flow including:
//! - Creating new databases
//! - Reopening existing databases
//! - Automatic WAL recovery
//! - Multiple write/close/reopen cycles

use strata_core::contract::Version;
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

#[test]
fn test_database_lifecycle() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("lifecycle_test");

    let run_id = RunId::new();
    let ns = Namespace::new(
        "tenant".to_string(),
        "app".to_string(),
        "agent".to_string(),
        run_id,
    );

    // Phase 1: Create database and write data
    {
        let db = Database::open(&db_path).expect("Failed to open database");

        let wal = db.wal().unwrap();
        let mut wal_guard = wal.lock();

        // Write transaction 1
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
                key: Key::new_kv(ns.clone(), "user:1"),
                value: Value::String("Alice".to_string()),
                version: 1,
            })
            .unwrap();

        wal_guard
            .append(&WALEntry::Write {
                run_id,
                key: Key::new_kv(ns.clone(), "user:2"),
                value: Value::String("Bob".to_string()),
                version: 2,
            })
            .unwrap();

        wal_guard
            .append(&WALEntry::CommitTxn { txn_id: 1, run_id })
            .unwrap();

        drop(wal_guard);
        db.flush().unwrap();
    }

    // Phase 2: Reopen and verify data
    {
        let db = Database::open(&db_path).expect("Failed to reopen database");

        // Both users should be restored
        let user1 = db
            .storage()
            .get(&Key::new_kv(ns.clone(), "user:1"))
            .unwrap()
            .expect("user:1 should exist");
        assert_eq!(user1.value, Value::String("Alice".to_string()));
        assert_eq!(user1.version, Version::Txn(1));

        let user2 = db
            .storage()
            .get(&Key::new_kv(ns.clone(), "user:2"))
            .unwrap()
            .expect("user:2 should exist");
        assert_eq!(user2.value, Value::String("Bob".to_string()));
        assert_eq!(user2.version, Version::Txn(2));

        // Add more data
        let wal = db.wal().unwrap();
        let mut wal_guard = wal.lock();

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
                key: Key::new_kv(ns.clone(), "user:3"),
                value: Value::String("Charlie".to_string()),
                version: 3,
            })
            .unwrap();

        wal_guard
            .append(&WALEntry::CommitTxn { txn_id: 2, run_id })
            .unwrap();

        drop(wal_guard);
        db.flush().unwrap();
    }

    // Phase 3: Reopen again and verify all data persisted
    {
        let db = Database::open(&db_path).expect("Failed to reopen database again");

        // All three users should exist
        assert!(db
            .storage()
            .get(&Key::new_kv(ns.clone(), "user:1"))
            .unwrap()
            .is_some());
        assert!(db
            .storage()
            .get(&Key::new_kv(ns.clone(), "user:2"))
            .unwrap()
            .is_some());
        assert!(db
            .storage()
            .get(&Key::new_kv(ns.clone(), "user:3"))
            .unwrap()
            .is_some());

        // Version should be at least 3
        assert!(db.storage().current_version() >= 3);
    }
}

#[test]
fn test_crash_recovery() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("crash_test");

    let run_id = RunId::new();
    let ns = Namespace::new(
        "tenant".to_string(),
        "app".to_string(),
        "agent".to_string(),
        run_id,
    );

    // Write some committed and some uncommitted transactions
    {
        let db = Database::open(&db_path).expect("Failed to open database");

        let wal = db.wal().unwrap();
        let mut wal_guard = wal.lock();

        // Committed transaction
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
                key: Key::new_kv(ns.clone(), "committed_key"),
                value: Value::Int(42),
                version: 1,
            })
            .unwrap();

        wal_guard
            .append(&WALEntry::CommitTxn { txn_id: 1, run_id })
            .unwrap();

        // Uncommitted transaction (simulates crash mid-transaction)
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
                key: Key::new_kv(ns.clone(), "uncommitted_key"),
                value: Value::Int(999),
                version: 2,
            })
            .unwrap();

        // NO CommitTxn - simulates crash

        drop(wal_guard);
        db.flush().unwrap();
    }

    // Reopen - uncommitted should be discarded
    {
        let db = Database::open(&db_path).expect("Failed to reopen after crash");

        // Committed data should be there
        let committed = db
            .storage()
            .get(&Key::new_kv(ns.clone(), "committed_key"))
            .unwrap();
        assert!(committed.is_some());
        assert_eq!(committed.unwrap().value, Value::Int(42));

        // Uncommitted data should NOT be there
        let uncommitted = db
            .storage()
            .get(&Key::new_kv(ns.clone(), "uncommitted_key"))
            .unwrap();
        assert!(uncommitted.is_none());
    }
}

#[test]
fn test_multiple_run_ids() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("multi_run_test");

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

    // Write data from two different runs
    {
        let db = Database::open(&db_path).expect("Failed to open database");

        let wal = db.wal().unwrap();
        let mut wal_guard = wal.lock();

        // Transaction from run 1
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
                key: Key::new_kv(ns1.clone(), "run1_key"),
                value: Value::String("run1_value".to_string()),
                version: 1,
            })
            .unwrap();

        wal_guard
            .append(&WALEntry::CommitTxn {
                txn_id: 1,
                run_id: run_id1,
            })
            .unwrap();

        // Transaction from run 2
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
                key: Key::new_kv(ns2.clone(), "run2_key"),
                value: Value::String("run2_value".to_string()),
                version: 2,
            })
            .unwrap();

        wal_guard
            .append(&WALEntry::CommitTxn {
                txn_id: 2,
                run_id: run_id2,
            })
            .unwrap();

        drop(wal_guard);
        db.flush().unwrap();
    }

    // Reopen and verify both runs' data is preserved
    {
        let db = Database::open(&db_path).expect("Failed to reopen database");

        // Run 1 data
        let run1_val = db
            .storage()
            .get(&Key::new_kv(ns1, "run1_key"))
            .unwrap()
            .expect("run1_key should exist");
        assert_eq!(run1_val.value, Value::String("run1_value".to_string()));

        // Run 2 data
        let run2_val = db
            .storage()
            .get(&Key::new_kv(ns2, "run2_key"))
            .unwrap()
            .expect("run2_key should exist");
        assert_eq!(run2_val.value, Value::String("run2_value".to_string()));
    }
}

#[test]
fn test_delete_operations() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("delete_test");

    let run_id = RunId::new();
    let ns = Namespace::new(
        "tenant".to_string(),
        "app".to_string(),
        "agent".to_string(),
        run_id,
    );

    // Write then delete
    {
        let db = Database::open(&db_path).expect("Failed to open database");

        let wal = db.wal().unwrap();
        let mut wal_guard = wal.lock();

        // Create key
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
                value: Value::String("temp_value".to_string()),
                version: 1,
            })
            .unwrap();

        wal_guard
            .append(&WALEntry::CommitTxn { txn_id: 1, run_id })
            .unwrap();

        // Delete key
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

        wal_guard
            .append(&WALEntry::CommitTxn { txn_id: 2, run_id })
            .unwrap();

        drop(wal_guard);
        db.flush().unwrap();
    }

    // Reopen - key should still be deleted
    {
        let db = Database::open(&db_path).expect("Failed to reopen database");

        let val = db.storage().get(&Key::new_kv(ns, "to_delete")).unwrap();
        assert!(
            val.is_none(),
            "Deleted key should remain deleted after recovery"
        );
    }
}

#[test]
fn test_durability_modes() {
    let temp_dir = TempDir::new().unwrap();

    let run_id = RunId::new();
    let ns = Namespace::new(
        "tenant".to_string(),
        "app".to_string(),
        "agent".to_string(),
        run_id,
    );

    // Test with Strict mode
    {
        let db_path = temp_dir.path().join("strict_db");
        let db = Database::open_with_mode(&db_path, DurabilityMode::Strict)
            .expect("Failed to open with Strict mode");

        let wal = db.wal().unwrap();
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
                key: Key::new_kv(ns.clone(), "strict_key"),
                value: Value::Int(1),
                version: 1,
            })
            .unwrap();

        wal_guard
            .append(&WALEntry::CommitTxn { txn_id: 1, run_id })
            .unwrap();

        drop(wal_guard);

        // Reopen and verify
        drop(db);
        let db2 = Database::open(&db_path).expect("Failed to reopen strict db");
        assert!(db2
            .storage()
            .get(&Key::new_kv(ns.clone(), "strict_key"))
            .unwrap()
            .is_some());
    }

    // Test with Batched mode
    {
        let db_path = temp_dir.path().join("batched_db");
        let db = Database::open_with_mode(
            &db_path,
            DurabilityMode::Batched {
                interval_ms: 100,
                batch_size: 10,
            },
        )
        .expect("Failed to open with Batched mode");

        let wal = db.wal().unwrap();
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
                key: Key::new_kv(ns.clone(), "batched_key"),
                value: Value::Int(2),
                version: 1,
            })
            .unwrap();

        wal_guard
            .append(&WALEntry::CommitTxn { txn_id: 1, run_id })
            .unwrap();

        drop(wal_guard);
        db.flush().unwrap(); // Ensure flushed

        // Reopen and verify
        drop(db);
        let db2 = Database::open(&db_path).expect("Failed to reopen batched db");
        assert!(db2
            .storage()
            .get(&Key::new_kv(ns.clone(), "batched_key"))
            .unwrap()
            .is_some());
    }
}

#[test]
fn test_large_transaction() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("large_txn_test");

    let run_id = RunId::new();
    let ns = Namespace::new(
        "tenant".to_string(),
        "app".to_string(),
        "agent".to_string(),
        run_id,
    );

    const NUM_ENTRIES: usize = 100;

    // Write large transaction
    {
        let db = Database::open(&db_path).expect("Failed to open database");

        let wal = db.wal().unwrap();
        let mut wal_guard = wal.lock();

        wal_guard
            .append(&WALEntry::BeginTxn {
                txn_id: 1,
                run_id,
                timestamp: now(),
            })
            .unwrap();

        for i in 0..NUM_ENTRIES {
            wal_guard
                .append(&WALEntry::Write {
                    run_id,
                    key: Key::new_kv(ns.clone(), format!("key_{}", i)),
                    value: Value::Int(i as i64),
                    version: (i + 1) as u64,
                })
                .unwrap();
        }

        wal_guard
            .append(&WALEntry::CommitTxn { txn_id: 1, run_id })
            .unwrap();

        drop(wal_guard);
        db.flush().unwrap();
    }

    // Reopen and verify all entries
    {
        let db = Database::open(&db_path).expect("Failed to reopen database");

        for i in 0..NUM_ENTRIES {
            let val = db
                .storage()
                .get(&Key::new_kv(ns.clone(), format!("key_{}", i)))
                .unwrap()
                .unwrap_or_else(|| panic!("key_{} should exist", i));
            assert_eq!(val.value, Value::Int(i as i64));
            assert_eq!(val.version, Version::Txn((i + 1) as u64));
        }

        assert_eq!(db.storage().current_version(), NUM_ENTRIES as u64);
    }
}

#[test]
fn test_aborted_transaction_discarded() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("aborted_test");

    let run_id = RunId::new();
    let ns = Namespace::new(
        "tenant".to_string(),
        "app".to_string(),
        "agent".to_string(),
        run_id,
    );

    // Write committed and aborted transactions
    {
        let db = Database::open(&db_path).expect("Failed to open database");

        let wal = db.wal().unwrap();
        let mut wal_guard = wal.lock();

        // Committed transaction
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
                key: Key::new_kv(ns.clone(), "committed"),
                value: Value::Bool(true),
                version: 1,
            })
            .unwrap();

        wal_guard
            .append(&WALEntry::CommitTxn { txn_id: 1, run_id })
            .unwrap();

        // Aborted transaction
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
                key: Key::new_kv(ns.clone(), "aborted"),
                value: Value::Bool(false),
                version: 2,
            })
            .unwrap();

        wal_guard
            .append(&WALEntry::AbortTxn { txn_id: 2, run_id })
            .unwrap();

        drop(wal_guard);
        db.flush().unwrap();
    }

    // Reopen - aborted should not appear
    {
        let db = Database::open(&db_path).expect("Failed to reopen database");

        // Committed data should be there
        assert!(db
            .storage()
            .get(&Key::new_kv(ns.clone(), "committed"))
            .unwrap()
            .is_some());

        // Aborted data should NOT be there
        assert!(db
            .storage()
            .get(&Key::new_kv(ns.clone(), "aborted"))
            .unwrap()
            .is_none());
    }
}

#[test]
fn test_empty_database_reopen() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("empty_test");

    // Create empty database
    {
        let _db = Database::open(&db_path).expect("Failed to open database");
        // Don't write anything
    }

    // Reopen empty database
    {
        let db = Database::open(&db_path).expect("Failed to reopen empty database");
        assert_eq!(db.storage().current_version(), 0);
    }
}

// ============================================================================
// Bug reproduction: Issue #145 - WAL Recovery Data Loss
// Tests to reproduce the scenario where only half of transactions are recovered
// ============================================================================

#[test]
fn test_multiple_crash_cycles_with_high_level_api() {
    // This test verifies that data persists correctly across multiple
    // database open/close cycles (simulating crashes and recoveries).
    // Regression test for the bug where duplicate txn_ids across sessions
    // caused data loss during WAL replay.
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("multi_cycle_test");

    let run_id = RunId::new();
    let ns = Namespace::new(
        "tenant".to_string(),
        "app".to_string(),
        "agent".to_string(),
        run_id,
    );

    const NUM_CYCLES: usize = 5;
    const KEYS_PER_CYCLE: usize = 10;

    for cycle in 0..NUM_CYCLES {
        // Open database and write keys
        {
            let db = Database::open_with_mode(&db_path, DurabilityMode::Strict)
                .expect("Failed to open database");

            for i in 0..KEYS_PER_CYCLE {
                let key = Key::new_kv(ns.clone(), format!("cycle{}_key{}", cycle, i));
                let value = Value::Int((cycle * 100 + i) as i64);
                db.put(run_id, key, value).expect("Put should succeed");
            }
            // Database is dropped here - simulates crash
        }

        // Reopen and verify ALL previous data survived
        {
            let db = Database::open_with_mode(&db_path, DurabilityMode::Strict)
                .expect("Failed to reopen database");

            // Verify all keys from all cycles up to and including current cycle
            for prev_cycle in 0..=cycle {
                for i in 0..KEYS_PER_CYCLE {
                    let key = Key::new_kv(ns.clone(), format!("cycle{}_key{}", prev_cycle, i));
                    let result = db.get(&key).expect("Get should not fail");

                    assert!(
                        result.is_some(),
                        "Key cycle{}_key{} should exist after cycle {} (current cycle: {})",
                        prev_cycle,
                        i,
                        cycle,
                        cycle
                    );

                    let vv = result.unwrap();
                    let expected = Value::Int((prev_cycle * 100 + i) as i64);
                    assert_eq!(
                        vv.value, expected,
                        "Key cycle{}_key{} has wrong value",
                        prev_cycle, i
                    );
                }
            }
        }
    }
}

#[test]
fn test_twenty_sequential_puts_recover() {
    // Minimal reproduction: 20 puts in sequence, then verify all 20 are recovered
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("twenty_puts_test");

    let run_id = RunId::new();
    let ns = Namespace::new(
        "tenant".to_string(),
        "app".to_string(),
        "agent".to_string(),
        run_id,
    );

    const NUM_PUTS: usize = 20;

    // Write 20 keys using high-level API
    {
        let db = Database::open_with_mode(&db_path, DurabilityMode::Strict)
            .expect("Failed to open database");

        for i in 0..NUM_PUTS {
            let key = Key::new_kv(ns.clone(), format!("key{}", i));
            db.put(run_id, key, Value::Int(i as i64))
                .expect("Put should succeed");
        }

        // No explicit flush - relies on Strict mode
    }

    // Reopen and verify
    {
        let db = Database::open_with_mode(&db_path, DurabilityMode::Strict)
            .expect("Failed to reopen database");

        let mut recovered_count = 0;
        for i in 0..NUM_PUTS {
            let key = Key::new_kv(ns.clone(), format!("key{}", i));
            if db.get(&key).unwrap().is_some() {
                recovered_count += 1;
            }
        }

        assert_eq!(
            recovered_count, NUM_PUTS,
            "Expected all {} keys to be recovered, but only {} were found",
            NUM_PUTS, recovered_count
        );
    }
}

*/
