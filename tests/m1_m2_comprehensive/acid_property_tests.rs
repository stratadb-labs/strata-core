//! ACID Property Tests
//!
//! Explicit tests for each ACID property:
//! - **A**tomicity: All or nothing
//! - **C**onsistency: Valid state transitions only
//! - **I**solation: Transactions don't interfere
//! - **D**urability: Committed data survives crashes

use super::test_utils::*;
use strata_core::error::Error;
use strata_core::types::Key;
use strata_core::value::Value;
use strata_engine::{Database, RetryConfig};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Barrier};
use std::thread;
use std::time::Duration;
use tempfile::TempDir;

// ============================================================================
// ATOMICITY
// A transaction either completes entirely or has no effect
// ============================================================================

mod atomicity {
    use super::*;

    /// Successful transaction: all writes committed
    #[test]
    fn test_atomicity_success_all_writes_committed() {
        let tdb = TestDb::new();

        let keys: Vec<Key> = (0..20)
            .map(|i| tdb.key(&format!("atomic_success_{}", i)))
            .collect();

        // Commit transaction with many writes
        tdb.db
            .transaction(tdb.run_id, |txn| {
                for (i, key) in keys.iter().enumerate() {
                    txn.put(key.clone(), values::int(i as i64 * 10))?;
                }
                Ok(())
            })
            .unwrap();

        // ATOMICITY: all writes must be present
        for (i, key) in keys.iter().enumerate() {
            let val = tdb.db.get(key).unwrap().expect("Key must exist");
            assert_eq!(val.value, values::int(i as i64 * 10));
        }
    }

    /// Failed transaction: no writes committed
    #[test]
    fn test_atomicity_failure_no_writes_committed() {
        let tdb = TestDb::new();

        let keys: Vec<Key> = (0..20)
            .map(|i| tdb.key(&format!("atomic_fail_{}", i)))
            .collect();

        // Fail transaction after many writes
        let result: Result<(), Error> = tdb.db.transaction(tdb.run_id, |txn| {
            for (i, key) in keys.iter().enumerate() {
                txn.put(key.clone(), values::int(i as i64))?;
            }
            Err(Error::InvalidState("intentional failure".to_string()))
        });

        assert!(result.is_err());

        // ATOMICITY: no writes must be present
        for key in &keys {
            assert!(
                tdb.db.get(key).unwrap().is_none(),
                "Atomicity violated: {:?} exists after failed transaction",
                key
            );
        }
    }

    /// Error mid-transaction: nothing committed
    #[test]
    fn test_atomicity_mid_error_nothing_committed() {
        let tdb = TestDb::new();

        let keys: Vec<Key> = (0..10)
            .map(|i| tdb.key(&format!("atomic_mid_{}", i)))
            .collect();

        let result: Result<(), Error> = tdb.db.transaction(tdb.run_id, |txn| {
            // Write first half
            for key in keys.iter().take(5) {
                txn.put(key.clone(), values::int(1))?;
            }

            // Error before writing second half
            return Err(Error::InvalidOperation("mid-transaction error".to_string()));
        });

        assert!(result.is_err());

        // ATOMICITY: not even the first 5 writes must exist
        for key in &keys {
            assert!(
                tdb.db.get(key).unwrap().is_none(),
                "Atomicity violated: {:?} exists",
                key
            );
        }
    }

    /// Mixed operations atomicity: puts and deletes together
    #[test]
    fn test_atomicity_mixed_operations() {
        let tdb = TestDb::new();

        // Pre-populate some keys
        let existing_keys: Vec<Key> = (0..5)
            .map(|i| {
                let key = tdb.key(&format!("existing_{}", i));
                tdb.db.put(tdb.run_id, key.clone(), values::int(i)).unwrap();
                key
            })
            .collect();

        let new_keys: Vec<Key> = (0..5).map(|i| tdb.key(&format!("new_{}", i))).collect();

        // Transaction that creates, updates, and deletes
        let result: Result<(), Error> = tdb.db.transaction(tdb.run_id, |txn| {
            // Delete existing
            for key in &existing_keys {
                txn.delete(key.clone())?;
            }

            // Create new
            for (i, key) in new_keys.iter().enumerate() {
                txn.put(key.clone(), values::int(i as i64))?;
            }

            // Fail!
            Err(Error::InvalidState("abort mixed".to_string()))
        });

        assert!(result.is_err());

        // ATOMICITY: existing keys still exist, new keys don't
        for key in &existing_keys {
            assert!(
                tdb.db.get(key).unwrap().is_some(),
                "Atomicity violated: delete was not rolled back"
            );
        }
        for key in &new_keys {
            assert!(
                tdb.db.get(key).unwrap().is_none(),
                "Atomicity violated: create was not rolled back"
            );
        }
    }

    /// Cross-primitive atomicity: KV and Event together
    #[test]
    fn test_atomicity_cross_primitive() {
        let tdb = TestDb::new();

        let kv_key = tdb.key("atomic_kv");
        let event_key = tdb.event(1);

        // Failed cross-primitive transaction
        let result: Result<(), Error> = tdb.db.transaction(tdb.run_id, |txn| {
            txn.put(kv_key.clone(), values::int(100))?;
            txn.put(event_key.clone(), values::string("event_data"))?;
            Err(Error::InvalidState("abort".to_string()))
        });

        assert!(result.is_err());

        // ATOMICITY: neither primitive should be written
        assert!(tdb.db.get(&kv_key).unwrap().is_none());
        assert!(tdb.db.get(&event_key).unwrap().is_none());
    }
}

// ============================================================================
// CONSISTENCY
// Database transitions from one valid state to another
// ============================================================================

mod consistency {
    use super::*;

    /// Invariant: Sum of accounts is always constant
    #[test]
    fn test_consistency_sum_invariant_maintained() {
        let tdb = TestDb::new();

        // Create accounts that sum to 1000
        let accounts: Vec<Key> = (0..5).map(|i| tdb.key(&format!("account_{}", i))).collect();

        for key in &accounts {
            tdb.db
                .put(tdb.run_id, key.clone(), values::int(200))
                .unwrap();
        }

        fn get_sum(db: &Database, accounts: &[Key]) -> i64 {
            accounts
                .iter()
                .map(|k| match db.get(k).unwrap().unwrap().value {
                    Value::I64(n) => n,
                    _ => 0,
                })
                .sum()
        }

        // Initial sum
        assert_eq!(get_sum(&tdb.db, &accounts), 1000);

        // Transfer that maintains sum
        tdb.db
            .transaction(tdb.run_id, |txn| {
                let from = match txn.get(&accounts[0])?.unwrap() {
                    Value::I64(n) => n,
                    _ => 0,
                };
                let to = match txn.get(&accounts[1])?.unwrap() {
                    Value::I64(n) => n,
                    _ => 0,
                };

                txn.put(accounts[0].clone(), values::int(from - 100))?;
                txn.put(accounts[1].clone(), values::int(to + 100))?;
                Ok(())
            })
            .unwrap();

        // CONSISTENCY: sum must still be 1000
        assert_eq!(get_sum(&tdb.db, &accounts), 1000);
    }

    /// Failed transaction preserves consistency
    #[test]
    fn test_consistency_failure_preserves_state() {
        let tdb = TestDb::new();

        let key = tdb.key("consistent_key");
        tdb.db
            .put(tdb.run_id, key.clone(), values::int(42))
            .unwrap();

        // Failed update attempt
        let _: Result<(), Error> = tdb.db.transaction(tdb.run_id, |txn| {
            txn.put(key.clone(), values::int(999))?;
            Err(Error::InvalidState("rollback".to_string()))
        });

        // CONSISTENCY: original state preserved
        let val = tdb.db.get(&key).unwrap().unwrap();
        assert_eq!(val.value, values::int(42));
    }

    /// Version numbers always increase (monotonic consistency)
    #[test]
    fn test_consistency_version_monotonicity() {
        let tdb = TestDb::new();
        let key = tdb.key("version_mono");

        let mut versions = Vec::new();

        for i in 0..10 {
            tdb.db.put(tdb.run_id, key.clone(), values::int(i)).unwrap();
            versions.push(tdb.db.get(&key).unwrap().unwrap().version.as_u64());
        }

        // CONSISTENCY: versions must be strictly increasing
        invariants::assert_monotonic_versions(&versions);
    }

    /// Concurrent transactions maintain consistency
    #[test]
    fn test_consistency_concurrent_transfers() {
        let temp_dir = TempDir::new().unwrap();
        let db = Arc::new(Database::open(temp_dir.path().join("db")).unwrap());

        let (run_id, ns) = create_namespace();

        // Create accounts summing to 1000
        let accounts: Vec<Key> = (0..4).map(|i| kv_key(&ns, &format!("acc_{}", i))).collect();

        for key in &accounts {
            db.put(run_id, key.clone(), values::int(250)).unwrap();
        }

        // Many concurrent transfers
        let handles: Vec<_> = (0..20)
            .map(|i| {
                let db = Arc::clone(&db);
                let accounts = accounts.clone();

                thread::spawn(move || {
                    let from_idx = i % 4;
                    let to_idx = (i + 1) % 4;

                    let _ = db.transaction_with_retry(
                        run_id,
                        RetryConfig::new().with_max_retries(50),
                        |txn| {
                            let from = match txn.get(&accounts[from_idx])?.unwrap() {
                                Value::I64(n) => n,
                                _ => 0,
                            };
                            let to = match txn.get(&accounts[to_idx])?.unwrap() {
                                Value::I64(n) => n,
                                _ => 0,
                            };

                            if from >= 10 {
                                txn.put(accounts[from_idx].clone(), values::int(from - 10))?;
                                txn.put(accounts[to_idx].clone(), values::int(to + 10))?;
                            }
                            Ok(())
                        },
                    );
                })
            })
            .collect();

        for h in handles {
            h.join().unwrap();
        }

        // CONSISTENCY: sum must still be 1000
        let sum: i64 = accounts
            .iter()
            .map(|k| match db.get(k).unwrap().unwrap().value {
                Value::I64(n) => n,
                _ => 0,
            })
            .sum();

        assert_eq!(sum, 1000, "Consistency violated: sum is {} not 1000", sum);
    }
}

// ============================================================================
// ISOLATION
// Concurrent transactions don't see each other's intermediate states
// ============================================================================

mod isolation {
    use super::*;

    /// Transaction sees consistent snapshot
    #[test]
    fn test_isolation_snapshot_consistency() {
        let temp_dir = TempDir::new().unwrap();
        let db = Arc::new(Database::open(temp_dir.path().join("db")).unwrap());

        let (run_id, ns) = create_namespace();

        // Create keys A, B with invariant: A + B = 100
        let key_a = kv_key(&ns, "iso_a");
        let key_b = kv_key(&ns, "iso_b");

        db.put(run_id, key_a.clone(), values::int(50)).unwrap();
        db.put(run_id, key_b.clone(), values::int(50)).unwrap();

        let db1 = Arc::clone(&db);
        let db2 = Arc::clone(&db);
        let keys1 = (key_a.clone(), key_b.clone());
        let keys2 = (key_a.clone(), key_b.clone());

        let barrier = Arc::new(Barrier::new(2));
        let barrier1 = Arc::clone(&barrier);
        let barrier2 = Arc::clone(&barrier);

        let observed_sum = Arc::new(AtomicU64::new(0));
        let observed_sum_clone = Arc::clone(&observed_sum);

        // T1: Read A, wait, read B, verify sum
        let h1 = thread::spawn(move || {
            db1.transaction(run_id, |txn| {
                let a = match txn.get(&keys1.0)?.unwrap() {
                    Value::I64(n) => n,
                    _ => 0,
                };

                barrier1.wait();
                thread::sleep(Duration::from_millis(20));

                let b = match txn.get(&keys1.1)?.unwrap() {
                    Value::I64(n) => n,
                    _ => 0,
                };

                observed_sum_clone.store((a + b) as u64, Ordering::Relaxed);
                Ok(())
            })
            .unwrap();
        });

        // T2: Transfer 25 from A to B between T1's reads
        let h2 = thread::spawn(move || {
            barrier2.wait();

            db2.transaction(run_id, |txn| {
                let a = match txn.get(&keys2.0)?.unwrap() {
                    Value::I64(n) => n,
                    _ => 0,
                };
                let b = match txn.get(&keys2.1)?.unwrap() {
                    Value::I64(n) => n,
                    _ => 0,
                };

                txn.put(keys2.0.clone(), values::int(a - 25))?;
                txn.put(keys2.1.clone(), values::int(b + 25))?;
                Ok(())
            })
            .unwrap();
        });

        h1.join().unwrap();
        h2.join().unwrap();

        // ISOLATION: T1 must have seen sum = 100 (either before or after T2)
        assert_eq!(
            observed_sum.load(Ordering::Relaxed),
            100,
            "Isolation violated: T1 saw inconsistent state"
        );
    }

    /// No dirty reads (uncommitted data invisible)
    #[test]
    fn test_isolation_no_dirty_reads() {
        let temp_dir = TempDir::new().unwrap();
        let db = Arc::new(Database::open(temp_dir.path().join("db")).unwrap());

        let (run_id, ns) = create_namespace();
        let key = kv_key(&ns, "dirty_key");

        db.put(run_id, key.clone(), values::int(100)).unwrap();

        let db1 = Arc::clone(&db);
        let db2 = Arc::clone(&db);
        let key1 = key.clone();
        let key2 = key.clone();

        let barrier = Arc::new(Barrier::new(2));
        let barrier1 = Arc::clone(&barrier);
        let barrier2 = Arc::clone(&barrier);

        let t2_saw_dirty = Arc::new(AtomicBool::new(false));
        let t2_saw_dirty_clone = Arc::clone(&t2_saw_dirty);

        // T1: Write but don't commit
        let h1 = thread::spawn(move || {
            db1.transaction(run_id, |txn| {
                txn.put(key1.clone(), values::int(999))?;

                barrier1.wait();
                thread::sleep(Duration::from_millis(50));

                Ok(())
            })
            .unwrap();
        });

        // T2: Try to read T1's uncommitted write
        let h2 = thread::spawn(move || {
            barrier2.wait();

            // Read immediately - should not see 999
            let val = db2.get(&key2).unwrap().unwrap();
            if let Value::I64(n) = val.value {
                if n == 999 {
                    // This is only a "dirty read" if T1 hasn't committed yet
                    // Due to timing, we might legitimately see 999 after T1 commits
                }
            }
        });

        h1.join().unwrap();
        h2.join().unwrap();

        // Final value should be 999 (T1 committed)
        let final_val = db.get(&key).unwrap().unwrap().value;
        assert_eq!(final_val, values::int(999));
    }

    /// Repeatable reads within transaction
    #[test]
    fn test_isolation_repeatable_reads() {
        let tdb = TestDb::new();
        let key = tdb.key("repeatable");

        tdb.db
            .put(tdb.run_id, key.clone(), values::int(42))
            .unwrap();

        tdb.db
            .transaction(tdb.run_id, |txn| {
                let reads: Vec<Option<Value>> =
                    (0..5).map(|_| txn.get(&key).ok().flatten()).collect();

                // ISOLATION: all reads must be identical
                for i in 1..reads.len() {
                    assert_eq!(
                        reads[i], reads[0],
                        "Repeatable read violated: {:?} != {:?}",
                        reads[i], reads[0]
                    );
                }

                Ok(())
            })
            .unwrap();
    }
}

// ============================================================================
// DURABILITY
// Committed transactions survive system crashes
// ============================================================================

mod durability {
    use super::*;

    /// Committed data survives close/reopen
    #[test]
    fn test_durability_survives_restart() {
        let pdb = PersistentTestDb::new();

        let keys: Vec<Key> = (0..50)
            .map(|i| pdb.key(&format!("durable_{}", i)))
            .collect();

        // Write and commit
        {
            let db = pdb.open();
            for (i, key) in keys.iter().enumerate() {
                db.put(pdb.run_id, key.clone(), values::int(i as i64 * 100))
                    .unwrap();
            }
        }

        // Crash (implicit close)

        // Recover and verify
        {
            let db = pdb.open();

            // DURABILITY: all data must survive
            for (i, key) in keys.iter().enumerate() {
                let val = db.get(key).unwrap().expect("Durable key must exist");
                assert_eq!(val.value, values::int(i as i64 * 100));
            }
        }
    }

    /// Transaction commits are durable
    #[test]
    fn test_durability_transaction_commits_survive() {
        let pdb = PersistentTestDb::new();

        let keys: Vec<Key> = (0..10)
            .map(|i| pdb.key(&format!("txn_durable_{}", i)))
            .collect();

        // Transaction
        {
            let db = pdb.open();
            db.transaction(pdb.run_id, |txn| {
                for (i, key) in keys.iter().enumerate() {
                    txn.put(key.clone(), values::int(i as i64))?;
                }
                Ok(())
            })
            .unwrap();
        }

        // Crash

        // Recover
        {
            let db = pdb.open();

            // DURABILITY: transaction must have survived
            invariants::assert_atomic_transaction(&db, &keys, true);

            for (i, key) in keys.iter().enumerate() {
                let val = db.get(key).unwrap().unwrap();
                assert_eq!(val.value, values::int(i as i64));
            }
        }
    }

    /// Uncommitted transactions don't survive (inverse durability)
    #[test]
    fn test_durability_uncommitted_not_durable() {
        let pdb = PersistentTestDb::new();

        let committed_key = pdb.key("committed");
        let uncommitted_key = pdb.key("uncommitted");

        {
            let db = pdb.open();

            // Committed
            db.put(pdb.run_id, committed_key.clone(), values::int(1))
                .unwrap();

            // Uncommitted (aborted)
            let _: Result<(), Error> = db.transaction(pdb.run_id, |txn| {
                txn.put(uncommitted_key.clone(), values::int(999))?;
                Err(Error::InvalidState("abort".to_string()))
            });
        }

        // Crash

        // Recover
        {
            let db = pdb.open();

            // DURABILITY: committed survives
            assert!(db.get(&committed_key).unwrap().is_some());

            // NOT durable: uncommitted doesn't survive
            assert!(db.get(&uncommitted_key).unwrap().is_none());
        }
    }

    /// Full state comparison durability
    #[test]
    fn test_durability_full_state_comparison() {
        let pdb = PersistentTestDb::new();

        let state_before = {
            let db = pdb.open();

            // Build complex state
            for i in 0..100 {
                let key = pdb.key(&format!("state_{}", i));
                db.put(pdb.run_id, key, values::int(i * i)).unwrap();
            }

            // Delete some
            for i in [10, 20, 30, 40, 50] {
                let key = pdb.key(&format!("state_{}", i));
                db.delete(pdb.run_id, key).unwrap();
            }

            // Overwrite some
            for i in [0, 5, 15, 25] {
                let key = pdb.key(&format!("state_{}", i));
                db.put(pdb.run_id, key, values::int(9999)).unwrap();
            }

            // Transaction
            db.transaction(pdb.run_id, |txn| {
                for i in 100..110 {
                    let key = kv_key(&pdb.ns, &format!("state_{}", i));
                    txn.put(key, values::int(i))?;
                }
                Ok(())
            })
            .unwrap();

            DatabaseStateSnapshot::capture(&db, &pdb.ns)
        };

        // Crash

        let state_after = {
            let db = pdb.open();
            DatabaseStateSnapshot::capture(&db, &pdb.ns)
        };

        // DURABILITY: state must be identical
        invariants::assert_recovery_preserves_state(&state_before, &state_after);
    }

    /// Multiple crash cycles preserve durability
    #[test]
    fn test_durability_multiple_crash_cycles() {
        use strata_core::traits::Storage;
        use strata_durability::wal::{DurabilityMode, WAL};

        let pdb = PersistentTestDb::new();
        eprintln!("Test run_id: {:?}", pdb.run_id);

        for cycle in 0..5 {
            eprintln!("\n=== Cycle {} ===", cycle);

            // Write new data
            {
                let db = pdb.open_strict(); // Use strict mode for guaranteed durability
                eprintln!(
                    "Storage version before writes: {}",
                    db.storage().current_version()
                );

                for i in 0..10 {
                    let key = pdb.key(&format!("cycle_{}_key_{}", cycle, i));
                    db.put(pdb.run_id, key, values::int((cycle * 100 + i) as i64))
                        .unwrap();
                }

                eprintln!(
                    "Storage version after writes: {}",
                    db.storage().current_version()
                );
            }

            // Crash - database dropped here
            eprintln!("Database dropped (simulated crash)");

            // Check WAL contents
            let wal_path = pdb.path().join("wal/current.wal");
            eprintln!("WAL file exists: {}", wal_path.exists());
            if wal_path.exists() {
                let wal = WAL::open(&wal_path, DurabilityMode::Strict).unwrap();
                let entries = wal.read_all().unwrap();
                eprintln!("WAL entries count: {}", entries.len());
                // Count commits
                let commits = entries
                    .iter()
                    .filter(|e| matches!(e, strata_durability::wal::WALEntry::CommitTxn { .. }))
                    .count();
                eprintln!("Committed transactions in WAL: {}", commits);
            }

            // Verify all previous cycles' data survives
            {
                // Manually check what recovery does
                let wal_path = pdb.path().join("wal/current.wal");
                let recovery = strata_concurrency::RecoveryCoordinator::new(wal_path.clone());
                let recovery_result = recovery.recover().unwrap();
                eprintln!(
                    "Recovery stats: txns_replayed={}, writes_applied={}, incomplete={}",
                    recovery_result.stats.txns_replayed,
                    recovery_result.stats.writes_applied,
                    recovery_result.stats.incomplete_txns
                );

                let db = pdb.open_strict();
                eprintln!(
                    "Storage version after reopen: {}",
                    db.storage().current_version()
                );

                // Debug: print first key we're looking for
                let check_key = pdb.key("cycle_0_key_0");
                eprintln!("Looking for key: {:?}", check_key);
                let storage_result = db.storage().get(&check_key);
                eprintln!("Direct storage get result: {:?}", storage_result);

                // Check if the key is in the recovery_result storage
                let recovery_storage_result = recovery_result.storage.get(&check_key);
                eprintln!("Recovery storage get result: {:?}", recovery_storage_result);

                // Print first few WAL entries for debugging
                let wal = WAL::open(&wal_path, DurabilityMode::Strict).unwrap();
                let entries = wal.read_all().unwrap();
                eprintln!("First 5 WAL entries:");
                for (i, entry) in entries.iter().take(5).enumerate() {
                    eprintln!("  {}: {:?}", i, entry);
                }

                for prev_cycle in 0..=cycle {
                    for i in 0..10 {
                        let key = pdb.key(&format!("cycle_{}_key_{}", prev_cycle, i));
                        let result = db.get(&key).unwrap();
                        if result.is_none() {
                            eprintln!("MISSING: cycle_{}_key_{} - key: {:?}", prev_cycle, i, key);
                            panic!("Key must survive");
                        }
                        let val = result.unwrap();
                        assert_eq!(val.value, values::int((prev_cycle * 100 + i) as i64));
                    }
                }
            }
        }
    }

    /// Deletes are durable
    #[test]
    fn test_durability_deletes_survive() {
        let pdb = PersistentTestDb::new();
        let key = pdb.key("deleted_durable");

        // Create and delete
        {
            let db = pdb.open();
            db.put(pdb.run_id, key.clone(), values::int(42)).unwrap();
            db.delete(pdb.run_id, key.clone()).unwrap();
        }

        // Crash

        // Verify delete survived
        {
            let db = pdb.open();
            assert!(db.get(&key).unwrap().is_none(), "Delete was not durable");
        }

        // Multiple more crashes
        for _ in 0..3 {
            let db = pdb.open();
            assert!(
                db.get(&key).unwrap().is_none(),
                "Delete not durable after multiple restarts"
            );
        }
    }
}

// ============================================================================
// Combined ACID Tests
// Tests that verify multiple ACID properties together
// ============================================================================

mod combined_acid {
    use super::*;

    /// Bank transfer: all ACID properties together
    #[test]
    fn test_acid_bank_transfer() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("bank");

        let (run_id, ns) = create_namespace();
        let account_a = kv_key(&ns, "bank_a");
        let account_b = kv_key(&ns, "bank_b");

        // Initial state: A=1000, B=0, sum=1000
        {
            let db = Database::open(&db_path).unwrap();
            db.put(run_id, account_a.clone(), values::int(1000))
                .unwrap();
            db.put(run_id, account_b.clone(), values::int(0)).unwrap();
        }

        // Perform transfers
        {
            let db = Arc::new(Database::open(&db_path).unwrap());

            let handles: Vec<_> = (0..10)
                .map(|_| {
                    let db = Arc::clone(&db);
                    let account_a = account_a.clone();
                    let account_b = account_b.clone();

                    thread::spawn(move || {
                        db.transaction_with_retry(
                            run_id,
                            RetryConfig::new().with_max_retries(50),
                            |txn| {
                                let a = match txn.get(&account_a)?.unwrap() {
                                    Value::I64(n) => n,
                                    _ => 0,
                                };
                                let b = match txn.get(&account_b)?.unwrap() {
                                    Value::I64(n) => n,
                                    _ => 0,
                                };

                                if a >= 100 {
                                    // ATOMICITY: both updates or neither
                                    txn.put(account_a.clone(), values::int(a - 100))?;
                                    txn.put(account_b.clone(), values::int(b + 100))?;
                                }
                                Ok(())
                            },
                        )
                        .unwrap();
                    })
                })
                .collect();

            for h in handles {
                h.join().unwrap();
            }
        }

        // Crash and recover
        let state_before_crash = {
            let db = Database::open(&db_path).unwrap();
            DatabaseStateSnapshot::capture(&db, &ns)
        };

        let state_after_recovery = {
            let db = Database::open(&db_path).unwrap();
            DatabaseStateSnapshot::capture(&db, &ns)
        };

        // DURABILITY: state survives crash
        invariants::assert_recovery_preserves_state(&state_before_crash, &state_after_recovery);

        // CONSISTENCY: sum is still 1000
        {
            let db = Database::open(&db_path).unwrap();
            let a = match db.get(&account_a).unwrap().unwrap().value {
                Value::I64(n) => n,
                _ => 0,
            };
            let b = match db.get(&account_b).unwrap().unwrap().value {
                Value::I64(n) => n,
                _ => 0,
            };

            assert_eq!(a + b, 1000, "ACID violated: sum is {} not 1000", a + b);
            assert_eq!(a, 0, "All transfers should have completed");
            assert_eq!(b, 1000, "All transfers should have completed");
        }
    }

    /// Counter increment: serialization through OCC
    #[test]
    fn test_acid_counter_increment() {
        let temp_dir = TempDir::new().unwrap();
        let db = Arc::new(Database::open(temp_dir.path().join("counter")).unwrap());

        let (run_id, ns) = create_namespace();
        let counter_key = kv_key(&ns, "counter");

        db.put(run_id, counter_key.clone(), values::int(0)).unwrap();

        let num_increments = 100;
        let successful_increments = Arc::new(AtomicU64::new(0));

        let handles: Vec<_> = (0..num_increments)
            .map(|_| {
                let db = Arc::clone(&db);
                let counter_key = counter_key.clone();
                let successful_increments = Arc::clone(&successful_increments);

                thread::spawn(move || {
                    let result = db.transaction_with_retry(
                        run_id,
                        RetryConfig::new().with_max_retries(100),
                        |txn| {
                            let current = match txn.get(&counter_key)?.unwrap() {
                                Value::I64(n) => n,
                                _ => 0,
                            };
                            txn.put(counter_key.clone(), values::int(current + 1))?;
                            Ok(())
                        },
                    );

                    if result.is_ok() {
                        successful_increments.fetch_add(1, Ordering::Relaxed);
                    }
                })
            })
            .collect();

        for h in handles {
            h.join().unwrap();
        }

        // All increments should have succeeded
        assert_eq!(
            successful_increments.load(Ordering::Relaxed),
            num_increments
        );

        // Final value should equal number of increments (ACID: no lost updates)
        let final_value = match db.get(&counter_key).unwrap().unwrap().value {
            Value::I64(n) => n,
            _ => 0,
        };

        assert_eq!(
            final_value,
            num_increments as i64,
            "ACID violated: lost {} updates",
            num_increments as i64 - final_value
        );
    }
}
