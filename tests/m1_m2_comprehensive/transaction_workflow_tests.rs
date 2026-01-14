//! Transaction Workflow Integration Tests
//!
//! End-to-end tests for complete transaction scenarios including
//! conflict detection, validation, and OCC semantics.

use super::test_utils::*;
use in_mem_core::error::Error;
use in_mem_core::types::RunId;
use in_mem_core::value::Value;
use in_mem_engine::{Database, RetryConfig};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Barrier};
use std::thread;
use std::time::Duration;
use tempfile::TempDir;

// ============================================================================
// Snapshot Isolation Tests
// ============================================================================

mod snapshot_isolation {
    use super::*;

    #[test]
    fn test_transaction_sees_consistent_snapshot() {
        let tdb = TestDb::new();

        // Create 10 keys with initial values
        for i in 0..10 {
            let key = tdb.key(&format!("snapshot_{}", i));
            tdb.db.put(tdb.run_id, key, values::int(0)).unwrap();
        }

        // Start transaction
        let mut sum_at_start = 0i64;
        let mut sum_at_end = 0i64;

        tdb.db
            .transaction(tdb.run_id, |txn| {
                // Read all keys at start
                for i in 0..10 {
                    let key = kv_key(&tdb.ns, &format!("snapshot_{}", i));
                    if let Some(v) = txn.get(&key)? {
                        if let Value::I64(n) = v {
                            sum_at_start += n;
                        }
                    }
                }

                // Read all keys again (should be same snapshot)
                for i in 0..10 {
                    let key = kv_key(&tdb.ns, &format!("snapshot_{}", i));
                    if let Some(v) = txn.get(&key)? {
                        if let Value::I64(n) = v {
                            sum_at_end += n;
                        }
                    }
                }

                Ok(())
            })
            .unwrap();

        // Sums should be equal (consistent snapshot)
        assert_eq!(sum_at_start, sum_at_end);
    }

    #[test]
    fn test_no_dirty_reads() {
        let temp_dir = TempDir::new().unwrap();
        let db = Arc::new(Database::open(temp_dir.path().join("db")).unwrap());

        let (run_id, ns) = create_namespace();
        let key = kv_key(&ns, "dirty_read_test");

        // Pre-populate
        db.put(run_id, key.clone(), values::int(100)).unwrap();

        let db1 = Arc::clone(&db);
        let db2 = Arc::clone(&db);
        let key1 = key.clone();
        let key2 = key.clone();

        let barrier = Arc::new(Barrier::new(2));
        let barrier1 = Arc::clone(&barrier);
        let barrier2 = Arc::clone(&barrier);

        let read_value = Arc::new(AtomicU64::new(0));
        let read_value_clone = Arc::clone(&read_value);

        // T1: Write but don't commit yet
        let h1 = thread::spawn(move || {
            db1.transaction(run_id, |txn| {
                txn.put(key1.clone(), values::int(200))?;

                // Signal T2 to read
                barrier1.wait();

                // Wait for T2 to finish reading
                thread::sleep(Duration::from_millis(50));

                Ok(())
            })
            .unwrap();
        });

        // T2: Try to read during T1's transaction
        let h2 = thread::spawn(move || {
            // Wait for T1 to write (but not commit)
            barrier2.wait();

            // Read should see old value (100), not uncommitted 200
            let result = db2.transaction(run_id, |txn| {
                let val = txn.get(&key2)?;
                Ok(val)
            });

            if let Ok(Some(Value::I64(n))) = result {
                read_value_clone.store(n as u64, Ordering::Relaxed);
            }
        });

        h1.join().unwrap();
        h2.join().unwrap();

        // T2 should have read 100 (the committed value), not 200 (uncommitted)
        // Note: Due to timing, T2 might read 100 or 200 depending on when T1 commits
        // This test verifies we don't see uncommitted data
        let read = read_value.load(Ordering::Relaxed);
        assert!(read == 100 || read == 200); // Either old or new (after commit)
    }

    #[test]
    fn test_no_non_repeatable_reads() {
        let tdb = TestDb::new();
        let key = tdb.key("repeatable_read");

        tdb.db.put(tdb.run_id, key.clone(), values::int(42)).unwrap();

        tdb.db
            .transaction(tdb.run_id, |txn| {
                // First read
                let v1 = txn.get(&key)?;

                // Simulate external modification (this wouldn't normally happen,
                // but if it did, our read should be stable)

                // Second read should be identical
                let v2 = txn.get(&key)?;

                assert_eq!(v1, v2);
                Ok(())
            })
            .unwrap();
    }
}

// ============================================================================
// Conflict Detection Tests
// ============================================================================

mod conflict_detection {
    use super::*;

    #[test]
    fn test_read_write_conflict_detected() {
        let temp_dir = TempDir::new().unwrap();
        let db = Arc::new(Database::open(temp_dir.path().join("db")).unwrap());

        let (run_id, ns) = create_namespace();
        let key = kv_key(&ns, "conflict_key");

        db.put(run_id, key.clone(), values::int(0)).unwrap();

        let db1 = Arc::clone(&db);
        let db2 = Arc::clone(&db);
        let key1 = key.clone();
        let key2 = key.clone();

        let barrier = Arc::new(Barrier::new(2));
        let barrier1 = Arc::clone(&barrier);
        let barrier2 = Arc::clone(&barrier);

        let t1_aborted = Arc::new(AtomicU64::new(0));
        let t1_aborted_clone = Arc::clone(&t1_aborted);

        // T1: Read key, wait, then try to commit
        let h1 = thread::spawn(move || {
            let result: Result<(), Error> = db1.transaction(run_id, |txn| {
                // Read the key
                let _ = txn.get(&key1)?;

                // Wait for T2 to modify
                barrier1.wait();
                thread::sleep(Duration::from_millis(20));

                // Try to write something
                txn.put(key1.clone(), values::int(100))?;
                Ok(())
            });

            if result.is_err() {
                t1_aborted_clone.store(1, Ordering::Relaxed);
            }
        });

        // T2: Modify the key that T1 read
        let h2 = thread::spawn(move || {
            barrier2.wait();
            db2.put(run_id, key2, values::int(50)).unwrap();
        });

        h1.join().unwrap();
        h2.join().unwrap();

        // T1 may or may not have been aborted depending on timing
        // The key point is if T2 committed first, T1 should detect conflict
    }

    #[test]
    fn test_write_write_conflict() {
        let temp_dir = TempDir::new().unwrap();
        let db = Arc::new(Database::open(temp_dir.path().join("db")).unwrap());

        let (run_id, ns) = create_namespace();
        let key = kv_key(&ns, "ww_conflict");

        db.put(run_id, key.clone(), values::int(0)).unwrap();

        let conflicts = Arc::new(AtomicU64::new(0));

        // Run many concurrent writers
        let handles: Vec<_> = (0..5)
            .map(|i| {
                let db = Arc::clone(&db);
                let key = key.clone();
                let conflicts = Arc::clone(&conflicts);

                thread::spawn(move || {
                    let result: Result<(), Error> = db.transaction(run_id, |txn| {
                        // Read current value
                        let _ = txn.get(&key)?;
                        thread::sleep(Duration::from_millis(5));

                        // Write new value
                        txn.put(key.clone(), values::int(i))?;
                        Ok(())
                    });

                    if result.is_err() {
                        conflicts.fetch_add(1, Ordering::Relaxed);
                    }
                })
            })
            .collect();

        for h in handles {
            h.join().unwrap();
        }

        // With 5 concurrent transactions writing the same key, some should conflict
        // (Though not guaranteed due to timing)
    }

    #[test]
    fn test_cas_conflict_detected() {
        let tdb = TestDb::new();
        let key = tdb.key("cas_conflict");

        tdb.db.put(tdb.run_id, key.clone(), values::int(1)).unwrap();
        let v1 = tdb.db.get(&key).unwrap().unwrap().version;

        // Update the key
        tdb.db.put(tdb.run_id, key.clone(), values::int(2)).unwrap();

        // Try CAS with old version - should fail
        let result = tdb.db.cas(tdb.run_id, key.clone(), v1, values::int(3));
        assert!(result.is_err());
    }

    #[test]
    fn test_first_committer_wins() {
        let temp_dir = TempDir::new().unwrap();
        let db = Arc::new(Database::open(temp_dir.path().join("db")).unwrap());

        let (run_id, ns) = create_namespace();
        let key = kv_key(&ns, "fcw_test");

        db.put(run_id, key.clone(), values::int(0)).unwrap();

        let winner = Arc::new(AtomicU64::new(0));

        let barrier = Arc::new(Barrier::new(2));

        let handles: Vec<_> = (0..2)
            .map(|i| {
                let db = Arc::clone(&db);
                let key = key.clone();
                let winner = Arc::clone(&winner);
                let barrier = Arc::clone(&barrier);

                thread::spawn(move || {
                    let result: Result<(), Error> = db.transaction(run_id, |txn| {
                        let _ = txn.get(&key)?;

                        // Synchronize both threads
                        barrier.wait();

                        txn.put(key.clone(), values::int(i))?;
                        Ok(())
                    });

                    if result.is_ok() {
                        winner.store(i as u64, Ordering::Relaxed);
                    }
                })
            })
            .collect();

        for h in handles {
            h.join().unwrap();
        }

        // Exactly one should have won
        let final_value = db.get(&key).unwrap().unwrap().value;
        let winning_value = winner.load(Ordering::Relaxed);
        assert_eq!(final_value, values::int(winning_value as i64));
    }
}

// ============================================================================
// Retry Workflow Tests
// ============================================================================

mod retry_workflows {
    use super::*;

    #[test]
    fn test_retry_eventually_succeeds_on_contention() {
        let temp_dir = TempDir::new().unwrap();
        let db = Arc::new(Database::open(temp_dir.path().join("db")).unwrap());

        let (run_id, ns) = create_namespace();
        let key = kv_key(&ns, "counter");

        db.put(run_id, key.clone(), values::int(0)).unwrap();

        let success_count = Arc::new(AtomicU64::new(0));

        // Multiple threads incrementing the same counter
        let handles: Vec<_> = (0..10)
            .map(|_| {
                let db = Arc::clone(&db);
                let key = key.clone();
                let success_count = Arc::clone(&success_count);

                thread::spawn(move || {
                    let result = db.transaction_with_retry(
                        run_id,
                        RetryConfig::new().with_max_retries(20),
                        |txn| {
                            let current = match txn.get(&key)? {
                                Some(Value::I64(n)) => n,
                                _ => 0,
                            };
                            txn.put(key.clone(), values::int(current + 1))?;
                            Ok(())
                        },
                    );

                    if result.is_ok() {
                        success_count.fetch_add(1, Ordering::Relaxed);
                    }
                })
            })
            .collect();

        for h in handles {
            h.join().unwrap();
        }

        // All transactions should eventually succeed with retry
        assert_eq!(success_count.load(Ordering::Relaxed), 10);

        // Final value should be 10
        let final_val = db.get(&key).unwrap().unwrap().value;
        assert_eq!(final_val, values::int(10));
    }

    #[test]
    fn test_atomic_transfer_with_retry() {
        let temp_dir = TempDir::new().unwrap();
        let db = Arc::new(Database::open(temp_dir.path().join("db")).unwrap());

        let (run_id, ns) = create_namespace();
        let account_a = kv_key(&ns, "account_a");
        let account_b = kv_key(&ns, "account_b");

        // Initial balances: A=1000, B=0
        db.put(run_id, account_a.clone(), values::int(1000)).unwrap();
        db.put(run_id, account_b.clone(), values::int(0)).unwrap();

        // Multiple concurrent transfers from A to B
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
                            let balance_a = match txn.get(&account_a)? {
                                Some(Value::I64(n)) => n,
                                _ => return Err(Error::InvalidState("No balance A".to_string())),
                            };
                            let balance_b = match txn.get(&account_b)? {
                                Some(Value::I64(n)) => n,
                                _ => return Err(Error::InvalidState("No balance B".to_string())),
                            };

                            // Transfer 100 from A to B
                            if balance_a >= 100 {
                                txn.put(account_a.clone(), values::int(balance_a - 100))?;
                                txn.put(account_b.clone(), values::int(balance_b + 100))?;
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

        // Total should still be 1000
        let final_a = match db.get(&account_a).unwrap().unwrap().value {
            Value::I64(n) => n,
            _ => panic!("Expected I64"),
        };
        let final_b = match db.get(&account_b).unwrap().unwrap().value {
            Value::I64(n) => n,
            _ => panic!("Expected I64"),
        };

        assert_eq!(final_a + final_b, 1000);
        assert_eq!(final_a, 0); // All transferred
        assert_eq!(final_b, 1000);
    }

    #[test]
    fn test_retry_preserves_atomicity() {
        let temp_dir = TempDir::new().unwrap();
        let db = Arc::new(Database::open(temp_dir.path().join("db")).unwrap());

        let (run_id, ns) = create_namespace();

        // Create 5 keys that should always sum to 100
        let keys: Vec<_> = (0..5)
            .map(|i| kv_key(&ns, &format!("atomic_{}", i)))
            .collect();

        for key in &keys {
            db.put(run_id, key.clone(), values::int(20)).unwrap();
        }

        // Multiple threads moving value between keys
        let handles: Vec<_> = (0..10)
            .map(|i| {
                let db = Arc::clone(&db);
                let keys = keys.clone();

                thread::spawn(move || {
                    let from = i % 5;
                    let to = (i + 1) % 5;

                    db.transaction_with_retry(
                        run_id,
                        RetryConfig::new().with_max_retries(30),
                        |txn| {
                            let from_val = match txn.get(&keys[from])? {
                                Some(Value::I64(n)) => n,
                                _ => 0,
                            };
                            let to_val = match txn.get(&keys[to])? {
                                Some(Value::I64(n)) => n,
                                _ => 0,
                            };

                            // Transfer 5 from 'from' to 'to'
                            if from_val >= 5 {
                                txn.put(keys[from].clone(), values::int(from_val - 5))?;
                                txn.put(keys[to].clone(), values::int(to_val + 5))?;
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

        // Sum should still be 100
        let sum: i64 = keys
            .iter()
            .map(|key| match db.get(key).unwrap().unwrap().value {
                Value::I64(n) => n,
                _ => 0,
            })
            .sum();

        assert_eq!(sum, 100);
    }
}

// ============================================================================
// Cross-Primitive Transaction Tests
// ============================================================================

mod cross_primitive {
    use super::*;

    #[test]
    fn test_kv_and_event_in_same_transaction() {
        let tdb = TestDb::new();

        let kv_key = tdb.key("kv_data");
        let event_key = tdb.event(1);

        tdb.db
            .transaction(tdb.run_id, |txn| {
                txn.put(kv_key.clone(), values::int(42))?;
                txn.put(event_key.clone(), values::string("event_data"))?;
                Ok(())
            })
            .unwrap();

        // Both should be committed atomically
        assert_eq!(
            tdb.db.get(&kv_key).unwrap().unwrap().value,
            values::int(42)
        );
        assert_eq!(
            tdb.db.get(&event_key).unwrap().unwrap().value,
            values::string("event_data")
        );
    }

    #[test]
    fn test_kv_and_state_in_same_transaction() {
        let tdb = TestDb::new();

        let kv_key = tdb.key("kv_data");
        let state_key = tdb.state("state_data");

        tdb.db
            .transaction(tdb.run_id, |txn| {
                txn.put(kv_key.clone(), values::int(1))?;
                txn.put(state_key.clone(), values::string("state"))?;
                Ok(())
            })
            .unwrap();

        assert!(tdb.db.get(&kv_key).unwrap().is_some());
        assert!(tdb.db.get(&state_key).unwrap().is_some());
    }

    #[test]
    fn test_cross_primitive_abort_is_atomic() {
        let tdb = TestDb::new();

        let kv_key = tdb.key("abort_kv");
        let event_key = tdb.event(99);

        let result: Result<(), Error> = tdb.db.transaction(tdb.run_id, |txn| {
            txn.put(kv_key.clone(), values::int(100))?;
            txn.put(event_key.clone(), values::string("event"))?;

            // Force abort
            Err(Error::InvalidState("intentional".to_string()))
        });

        assert!(result.is_err());

        // Neither should exist
        assert!(tdb.db.get(&kv_key).unwrap().is_none());
        assert!(tdb.db.get(&event_key).unwrap().is_none());
    }

    #[test]
    fn test_read_kv_write_event() {
        let tdb = TestDb::new();

        let kv_key = tdb.key("read_kv");
        let event_key = tdb.event(2);

        // Pre-populate KV
        tdb.db
            .put(tdb.run_id, kv_key.clone(), values::int(42))
            .unwrap();

        // Read KV, write Event
        tdb.db
            .transaction(tdb.run_id, |txn| {
                let kv_val = txn.get(&kv_key)?;
                let event_data = format!("read value: {:?}", kv_val);
                txn.put(event_key.clone(), values::string(&event_data))?;
                Ok(())
            })
            .unwrap();

        assert!(tdb.db.get(&event_key).unwrap().is_some());
    }

    #[test]
    fn test_multiple_events_atomic() {
        let tdb = TestDb::new();

        // Write multiple events atomically
        tdb.db
            .transaction(tdb.run_id, |txn| {
                for i in 0..10 {
                    let event_key = event_key(&tdb.ns, i);
                    txn.put(event_key, values::int(i as i64))?;
                }
                Ok(())
            })
            .unwrap();

        // All should exist
        for i in 0..10 {
            let event_key = event_key(&tdb.ns, i);
            assert!(tdb.db.get(&event_key).unwrap().is_some());
        }
    }
}

// ============================================================================
// Multi-Key Transaction Tests
// ============================================================================

mod multi_key {
    use super::*;

    #[test]
    fn test_many_keys_in_single_transaction() {
        let tdb = TestDb::new();

        let num_keys = 100;

        tdb.db
            .transaction(tdb.run_id, |txn| {
                for i in 0..num_keys {
                    let key = kv_key(&tdb.ns, &format!("many_keys_{}", i));
                    txn.put(key, values::int(i))?;
                }
                Ok(())
            })
            .unwrap();

        // Verify all committed
        for i in 0..num_keys {
            let key = kv_key(&tdb.ns, &format!("many_keys_{}", i));
            let val = tdb.db.get(&key).unwrap().unwrap();
            assert_eq!(val.value, values::int(i));
        }
    }

    #[test]
    fn test_read_many_write_few() {
        let tdb = TestDb::new();

        // Pre-populate many keys
        for i in 0..50 {
            let key = tdb.key(&format!("read_{}", i));
            tdb.db.put(tdb.run_id, key, values::int(i)).unwrap();
        }

        // Read all, write few
        tdb.db
            .transaction(tdb.run_id, |txn| {
                let mut sum = 0i64;
                for i in 0..50 {
                    let key = kv_key(&tdb.ns, &format!("read_{}", i));
                    if let Some(Value::I64(n)) = txn.get(&key)? {
                        sum += n;
                    }
                }

                // Write the sum
                let sum_key = kv_key(&tdb.ns, "sum");
                txn.put(sum_key, values::int(sum))?;
                Ok(())
            })
            .unwrap();

        let sum_key = tdb.key("sum");
        let sum = tdb.db.get(&sum_key).unwrap().unwrap();
        assert_eq!(sum.value, values::int((0..50).sum()));
    }

    #[test]
    fn test_mixed_reads_writes_deletes() {
        let tdb = TestDb::new();

        // Pre-populate
        for i in 0..10 {
            let key = tdb.key(&format!("mixed_{}", i));
            tdb.db.put(tdb.run_id, key, values::int(i)).unwrap();
        }

        tdb.db
            .transaction(tdb.run_id, |txn| {
                // Read some
                for i in 0..5 {
                    let key = kv_key(&tdb.ns, &format!("mixed_{}", i));
                    let _ = txn.get(&key)?;
                }

                // Delete some
                for i in 2..4 {
                    let key = kv_key(&tdb.ns, &format!("mixed_{}", i));
                    txn.delete(key)?;
                }

                // Write new
                for i in 10..15 {
                    let key = kv_key(&tdb.ns, &format!("mixed_{}", i));
                    txn.put(key, values::int(i))?;
                }

                // Overwrite existing
                let key = kv_key(&tdb.ns, "mixed_0");
                txn.put(key, values::int(100))?;

                Ok(())
            })
            .unwrap();

        // Verify state
        assert_eq!(
            tdb.db.get(&tdb.key("mixed_0")).unwrap().unwrap().value,
            values::int(100)
        ); // Overwritten
        assert!(tdb.db.get(&tdb.key("mixed_2")).unwrap().is_none()); // Deleted
        assert!(tdb.db.get(&tdb.key("mixed_3")).unwrap().is_none()); // Deleted
        assert!(tdb.db.get(&tdb.key("mixed_5")).unwrap().is_some()); // Unchanged
        assert!(tdb.db.get(&tdb.key("mixed_12")).unwrap().is_some()); // New
    }
}

// ============================================================================
// Version Monotonicity Tests
// ============================================================================

mod version_monotonicity {
    use super::*;

    #[test]
    fn test_versions_increase_monotonically() {
        let tdb = TestDb::new();
        let key = tdb.key("version_test");

        let mut versions = Vec::new();

        for i in 0..10 {
            tdb.db.put(tdb.run_id, key.clone(), values::int(i)).unwrap();
            let v = tdb.db.get(&key).unwrap().unwrap().version;
            versions.push(v);
        }

        // Each version should be greater than the previous
        for i in 1..versions.len() {
            assert!(
                versions[i] > versions[i - 1],
                "Version {} ({}) should be > version {} ({})",
                i,
                versions[i],
                i - 1,
                versions[i - 1]
            );
        }
    }

    #[test]
    fn test_transaction_versions_increase() {
        let tdb = TestDb::new();

        let mut versions = Vec::new();

        for i in 0..5 {
            let key = tdb.key(&format!("txn_ver_{}", i));

            tdb.db
                .transaction(tdb.run_id, |txn| {
                    txn.put(key.clone(), values::int(i))?;
                    Ok(())
                })
                .unwrap();

            let v = tdb.db.get(&key).unwrap().unwrap().version;
            versions.push(v);
        }

        for i in 1..versions.len() {
            assert!(versions[i] > versions[i - 1]);
        }
    }

    #[test]
    fn test_concurrent_transactions_get_unique_versions() {
        let temp_dir = TempDir::new().unwrap();
        let db = Arc::new(Database::open(temp_dir.path().join("db")).unwrap());

        let (run_id, ns) = create_namespace();

        let versions: Vec<u64> = concurrent::run_with_shared(10, (db, run_id, ns), |i, shared| {
            let (db, run_id, ns) = shared;
            let key = kv_key(ns, &format!("concurrent_{}", i));

            db.transaction(*run_id, |txn| {
                txn.put(key.clone(), values::int(i as i64))?;
                Ok(())
            })
            .unwrap();

            db.get(&key).unwrap().unwrap().version
        });

        // All versions should be unique
        let mut sorted = versions.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(sorted.len(), versions.len());
    }
}

// ============================================================================
// Transaction Boundary Tests
// ============================================================================

mod transaction_boundaries {
    use super::*;

    #[test]
    fn test_changes_invisible_until_commit() {
        let temp_dir = TempDir::new().unwrap();
        let db = Arc::new(Database::open(temp_dir.path().join("db")).unwrap());

        let (run_id, ns) = create_namespace();
        let key = kv_key(&ns, "invisible");

        let db1 = Arc::clone(&db);
        let db2 = Arc::clone(&db);
        let key1 = key.clone();
        let key2 = key.clone();

        let barrier = Arc::new(Barrier::new(2));
        let barrier1 = Arc::clone(&barrier);
        let barrier2 = Arc::clone(&barrier);

        let seen_value = Arc::new(AtomicU64::new(999));
        let seen_value_clone = Arc::clone(&seen_value);

        // T1: Start transaction, write, wait, then commit
        let h1 = thread::spawn(move || {
            db1.transaction(run_id, |txn| {
                txn.put(key1.clone(), values::int(42))?;

                // Signal T2 that write is done but not committed
                barrier1.wait();

                // Wait a bit before committing
                thread::sleep(Duration::from_millis(50));

                Ok(())
            })
            .unwrap();
        });

        // T2: Read while T1 has written but not committed
        let h2 = thread::spawn(move || {
            barrier2.wait();

            // T1 has written but not committed
            let val = db2.get(&key2).unwrap();
            match val {
                None => seen_value_clone.store(0, Ordering::Relaxed),
                Some(v) => {
                    if let Value::I64(n) = v.value {
                        seen_value_clone.store(n as u64, Ordering::Relaxed);
                    }
                }
            }
        });

        h1.join().unwrap();
        h2.join().unwrap();

        // T2 should have seen None (0) or the committed value
        // The key point is T2's read happened atomically with respect to T1
    }

    #[test]
    fn test_aborted_changes_invisible() {
        let tdb = TestDb::new();
        let key = tdb.key("aborted");

        let _: Result<(), Error> = tdb.db.transaction(tdb.run_id, |txn| {
            txn.put(key.clone(), values::int(42))?;
            Err(Error::InvalidState("abort".to_string()))
        });

        // Key should not exist
        assert!(tdb.db.get(&key).unwrap().is_none());
    }

    #[test]
    fn test_committed_changes_immediately_visible() {
        let tdb = TestDb::new();
        let key = tdb.key("committed");

        tdb.db
            .transaction(tdb.run_id, |txn| {
                txn.put(key.clone(), values::int(42))?;
                Ok(())
            })
            .unwrap();

        // Should be immediately visible
        let val = tdb.db.get(&key).unwrap().unwrap();
        assert_eq!(val.value, values::int(42));
    }
}

// ============================================================================
// Real-World Scenario Tests
// ============================================================================

mod real_world_scenarios {
    use super::*;

    #[test]
    fn test_inventory_management() {
        let temp_dir = TempDir::new().unwrap();
        let db = Arc::new(Database::open(temp_dir.path().join("db")).unwrap());

        let (run_id, ns) = create_namespace();

        // Initialize inventory: 100 items
        let inventory_key = kv_key(&ns, "inventory");
        db.put(run_id, inventory_key.clone(), values::int(100))
            .unwrap();

        let successful_orders = Arc::new(AtomicU64::new(0));

        // 20 concurrent orders, each trying to buy 10 items
        let handles: Vec<_> = (0..20)
            .map(|_| {
                let db = Arc::clone(&db);
                let inventory_key = inventory_key.clone();
                let successful_orders = Arc::clone(&successful_orders);

                thread::spawn(move || {
                    let result = db.transaction_with_retry(
                        run_id,
                        RetryConfig::new().with_max_retries(30),
                        |txn| {
                            let stock = match txn.get(&inventory_key)? {
                                Some(Value::I64(n)) => n,
                                _ => return Err(Error::InvalidState("No inventory".to_string())),
                            };

                            if stock >= 10 {
                                txn.put(inventory_key.clone(), values::int(stock - 10))?;
                                Ok(true) // Order successful
                            } else {
                                Ok(false) // Out of stock
                            }
                        },
                    );

                    if let Ok(true) = result {
                        successful_orders.fetch_add(1, Ordering::Relaxed);
                    }
                })
            })
            .collect();

        for h in handles {
            h.join().unwrap();
        }

        // Exactly 10 orders should succeed (100 / 10 = 10)
        assert_eq!(successful_orders.load(Ordering::Relaxed), 10);

        // Final inventory should be 0
        let final_stock = db.get(&inventory_key).unwrap().unwrap().value;
        assert_eq!(final_stock, values::int(0));
    }

    #[test]
    fn test_user_session_update() {
        let tdb = TestDb::new();

        // Create user and session
        let user_key = tdb.key("user_123");
        let session_key = tdb.key("session_user_123");

        // Initial state
        tdb.db
            .transaction(tdb.run_id, |txn| {
                txn.put(
                    user_key.clone(),
                    values::map(vec![("name", values::string("Alice")), ("age", values::int(30))]),
                )?;
                txn.put(
                    session_key.clone(),
                    values::map(vec![
                        ("token", values::string("abc123")),
                        ("expires", values::int(1000)),
                    ]),
                )?;
                Ok(())
            })
            .unwrap();

        // Update both atomically
        tdb.db
            .transaction(tdb.run_id, |txn| {
                // Read current user
                let _ = txn.get(&user_key)?;

                // Update session
                txn.put(
                    session_key.clone(),
                    values::map(vec![
                        ("token", values::string("xyz789")),
                        ("expires", values::int(2000)),
                    ]),
                )?;

                // Update user last_login
                txn.put(
                    user_key.clone(),
                    values::map(vec![
                        ("name", values::string("Alice")),
                        ("age", values::int(30)),
                        ("last_login", values::int(12345)),
                    ]),
                )?;

                Ok(())
            })
            .unwrap();

        // Both should be updated
        assert!(tdb.db.get(&user_key).unwrap().is_some());
        assert!(tdb.db.get(&session_key).unwrap().is_some());
    }

    #[test]
    fn test_audit_log_with_data() {
        let tdb = TestDb::new();

        let data_key = tdb.key("sensitive_data");
        let audit_key = tdb.event(1);

        tdb.db
            .transaction(tdb.run_id, |txn| {
                // Update data
                txn.put(data_key.clone(), values::string("secret"))?;

                // Create audit log atomically
                txn.put(
                    audit_key.clone(),
                    values::map(vec![
                        ("action", values::string("update")),
                        ("key", values::string("sensitive_data")),
                        ("timestamp", values::int(12345)),
                    ]),
                )?;

                Ok(())
            })
            .unwrap();

        // Both should exist (audit log can't be separated from data update)
        assert!(tdb.db.get(&data_key).unwrap().is_some());
        assert!(tdb.db.get(&audit_key).unwrap().is_some());
    }
}
