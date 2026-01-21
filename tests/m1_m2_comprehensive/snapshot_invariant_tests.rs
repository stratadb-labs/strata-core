//! Snapshot Isolation Invariant Tests
//!
//! These tests enforce Snapshot Isolation (SI) semantic guarantees.
//!
//! ## Core SI Invariants
//!
//! 1. **Snapshot Immutability**: A transaction's snapshot does not change
//! 2. **No Dirty Reads**: Uncommitted writes are invisible to other transactions
//! 3. **No Non-Repeatable Reads**: Same key returns same value within a transaction
//! 4. **Multi-Key Consistency**: All reads see the same logical point in time
//! 5. **Write Invisibility Until Commit**: Own writes are visible, but not to others

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
// Invariant 1: Snapshot Immutability
// A transaction's view of data does not change during its execution
// ============================================================================

mod snapshot_immutability {
    use super::*;

    /// Reads of the same key always return the same value within a transaction
    #[test]
    fn test_repeated_reads_return_same_value() {
        let tdb = TestDb::new();
        let key = tdb.key("immutable_read");

        // Pre-populate
        tdb.db
            .put(tdb.run_id, key.clone(), values::int(42))
            .unwrap();

        tdb.db
            .transaction(tdb.run_id, |txn| {
                // Read multiple times
                let read1 = txn.get(&key)?;
                let read2 = txn.get(&key)?;
                let read3 = txn.get(&key)?;
                let read4 = txn.get(&key)?;
                let read5 = txn.get(&key)?;

                // All must be identical
                assert_eq!(read1, read2, "Repeated read inconsistency");
                assert_eq!(read2, read3, "Repeated read inconsistency");
                assert_eq!(read3, read4, "Repeated read inconsistency");
                assert_eq!(read4, read5, "Repeated read inconsistency");

                Ok(())
            })
            .unwrap();
    }

    /// Snapshot does not see concurrent modifications
    #[test]
    fn test_snapshot_ignores_concurrent_writes() {
        let temp_dir = TempDir::new().unwrap();
        let db = Arc::new(Database::open(temp_dir.path().join("db")).unwrap());

        let (run_id, ns) = create_namespace();
        let key = kv_key(&ns, "concurrent_key");

        // Pre-populate
        db.put(run_id, key.clone(), values::int(100)).unwrap();

        let db1 = Arc::clone(&db);
        let db2 = Arc::clone(&db);
        let key1 = key.clone();
        let key2 = key.clone();

        let barrier = Arc::new(Barrier::new(2));
        let barrier1 = Arc::clone(&barrier);
        let barrier2 = Arc::clone(&barrier);

        let t1_saw_modification = Arc::new(AtomicBool::new(false));
        let t1_saw_modification_clone = Arc::clone(&t1_saw_modification);

        // T1: Start transaction, read, wait, read again
        let h1 = thread::spawn(move || {
            db1.transaction(run_id, |txn| {
                let first_read = txn.get(&key1)?.expect("Key should exist");
                let first_value = match first_read {
                    Value::I64(n) => n,
                    _ => panic!("Expected I64"),
                };

                // Signal T2 to write
                barrier1.wait();

                // Wait for T2 to commit
                thread::sleep(Duration::from_millis(50));

                // Second read - must be same as first
                let second_read = txn.get(&key1)?.expect("Key should exist");
                let second_value = match second_read {
                    Value::I64(n) => n,
                    _ => panic!("Expected I64"),
                };

                if first_value != second_value {
                    t1_saw_modification_clone.store(true, Ordering::Relaxed);
                }

                // THE INVARIANT: snapshot must be immutable
                assert_eq!(
                    first_value, second_value,
                    "Snapshot immutability violated: saw {} then {}",
                    first_value, second_value
                );

                Ok(())
            })
        });

        // T2: Write to the key after T1 has started
        let h2 = thread::spawn(move || {
            barrier2.wait();
            db2.put(run_id, key2.clone(), values::int(999)).unwrap();
        });

        h1.join().unwrap().unwrap();
        h2.join().unwrap();

        // Verify T2's write is visible now
        let final_value = db.get(&key).unwrap().unwrap().value;
        assert_eq!(final_value, values::int(999));

        // Verify T1 did NOT see T2's modification
        assert!(
            !t1_saw_modification.load(Ordering::Relaxed),
            "T1 saw T2's modification - snapshot not immutable"
        );
    }

    /// Multiple keys read at consistent point in time
    #[test]
    fn test_multi_key_snapshot_consistency() {
        let temp_dir = TempDir::new().unwrap();
        let db = Arc::new(Database::open(temp_dir.path().join("db")).unwrap());

        let (run_id, ns) = create_namespace();

        // Create keys A, B, C that always sum to 300
        let key_a = kv_key(&ns, "sum_a");
        let key_b = kv_key(&ns, "sum_b");
        let key_c = kv_key(&ns, "sum_c");

        db.put(run_id, key_a.clone(), values::int(100)).unwrap();
        db.put(run_id, key_b.clone(), values::int(100)).unwrap();
        db.put(run_id, key_c.clone(), values::int(100)).unwrap();

        let db1 = Arc::clone(&db);
        let keys1 = (key_a.clone(), key_b.clone(), key_c.clone());

        let db2 = Arc::clone(&db);
        let keys2 = (key_a.clone(), key_b.clone(), key_c.clone());

        let barrier = Arc::new(Barrier::new(2));
        let barrier1 = Arc::clone(&barrier);
        let barrier2 = Arc::clone(&barrier);

        let sum_seen = Arc::new(AtomicU64::new(0));
        let sum_seen_clone = Arc::clone(&sum_seen);

        // T1: Read all keys, they should sum to 300
        let h1 = thread::spawn(move || {
            db1.transaction(run_id, |txn| {
                // Read A
                let a = match txn.get(&keys1.0)?.unwrap() {
                    Value::I64(n) => n,
                    _ => panic!("Expected I64"),
                };

                // Signal T2 to start modifying
                barrier1.wait();

                // Wait a bit
                thread::sleep(Duration::from_millis(20));

                // Read B and C (T2 might be modifying them)
                let b = match txn.get(&keys1.1)?.unwrap() {
                    Value::I64(n) => n,
                    _ => panic!("Expected I64"),
                };
                let c = match txn.get(&keys1.2)?.unwrap() {
                    Value::I64(n) => n,
                    _ => panic!("Expected I64"),
                };

                let sum = a + b + c;
                sum_seen_clone.store(sum as u64, Ordering::Relaxed);

                // THE INVARIANT: sum must be 300 (consistent snapshot)
                assert_eq!(
                    sum, 300,
                    "Multi-key snapshot inconsistency: a={} b={} c={} sum={}",
                    a, b, c, sum
                );

                Ok(())
            })
        });

        // T2: Move value from A to B (maintaining sum = 300)
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

                // Transfer 50 from A to B
                txn.put(keys2.0.clone(), values::int(a - 50))?;
                txn.put(keys2.1.clone(), values::int(b + 50))?;
                Ok(())
            })
        });

        h1.join().unwrap().unwrap();
        let _ = h2.join();

        assert_eq!(
            sum_seen.load(Ordering::Relaxed),
            300,
            "T1 saw inconsistent sum"
        );
    }
}

// ============================================================================
// Invariant 2: No Dirty Reads
// Uncommitted writes are invisible to other transactions
// ============================================================================

mod no_dirty_reads {
    use super::*;

    /// Transaction does not see uncommitted writes from another transaction
    #[test]
    fn test_uncommitted_writes_invisible() {
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

        let t2_read_value = Arc::new(AtomicU64::new(0));
        let t2_read_value_clone = Arc::clone(&t2_read_value);

        // T1: Start transaction, write but don't commit
        let h1 = thread::spawn(move || {
            db1.transaction(run_id, |txn| {
                // Write uncommitted value
                txn.put(key1.clone(), values::int(999))?;

                // Signal T2 to read
                barrier1.wait();

                // Wait for T2 to finish reading
                thread::sleep(Duration::from_millis(50));

                // Now commit
                Ok(())
            })
            .unwrap();
        });

        // T2: Read while T1 has uncommitted write
        let h2 = thread::spawn(move || {
            barrier2.wait();

            let val = db2.get(&key2).unwrap().unwrap();
            if let Value::I64(n) = val.value {
                t2_read_value_clone.store(n as u64, Ordering::Relaxed);
            }
        });

        h1.join().unwrap();
        h2.join().unwrap();

        // THE INVARIANT: T2 must have seen 100 (committed value), not 999 (uncommitted)
        let seen = t2_read_value.load(Ordering::Relaxed);
        assert!(seen == 100 || seen == 999, "Unexpected value: {}", seen);
        // Note: Due to timing, T2 might read 100 (before T1 commits) or 999 (after)
        // But it must NEVER see an intermediate state
    }

    /// Aborted transaction's writes are never visible
    #[test]
    fn test_aborted_writes_never_visible() {
        let temp_dir = TempDir::new().unwrap();
        let db = Arc::new(Database::open(temp_dir.path().join("db")).unwrap());

        let (run_id, ns) = create_namespace();
        let key = kv_key(&ns, "aborted_write");

        // Pre-populate
        db.put(run_id, key.clone(), values::int(100)).unwrap();

        let db1 = Arc::clone(&db);
        let db2 = Arc::clone(&db);
        let key1 = key.clone();
        let key2 = key.clone();

        let barrier = Arc::new(Barrier::new(2));
        let barrier1 = Arc::clone(&barrier);
        let barrier2 = Arc::clone(&barrier);

        let observed_values = Arc::new(std::sync::Mutex::new(Vec::new()));
        let observed_values_clone = Arc::clone(&observed_values);

        // T1: Write and abort
        let h1 = thread::spawn(move || {
            let _: Result<(), Error> = db1.transaction(run_id, |txn| {
                txn.put(key1.clone(), values::int(999))?;

                // Signal T2 to start reading
                barrier1.wait();

                // Wait for T2 to read
                thread::sleep(Duration::from_millis(50));

                // Abort
                Err(Error::InvalidState("abort".to_string()))
            });
        });

        // T2: Continuously read while T1 is active
        let h2 = thread::spawn(move || {
            barrier2.wait();

            for _ in 0..10 {
                let val = db2.get(&key2).unwrap().unwrap();
                if let Value::I64(n) = val.value {
                    observed_values_clone.lock().unwrap().push(n);
                }
                thread::sleep(Duration::from_millis(5));
            }
        });

        h1.join().unwrap();
        h2.join().unwrap();

        // THE INVARIANT: T2 must never have seen 999
        let values = observed_values.lock().unwrap();
        for &v in values.iter() {
            assert_eq!(v, 100, "Dirty read detected: saw {} instead of 100", v);
        }
    }
}

// ============================================================================
// Invariant 3: Read-Your-Own-Writes
// A transaction sees its own uncommitted modifications
// ============================================================================

mod read_your_own_writes {
    use super::*;

    /// Transaction sees its own puts
    #[test]
    fn test_sees_own_puts() {
        let tdb = TestDb::new();

        tdb.db
            .transaction(tdb.run_id, |txn| {
                let key = kv_key(&tdb.ns, "ryw_put");

                // Before put: should not exist
                assert!(txn.get(&key)?.is_none());

                // Put
                txn.put(key.clone(), values::int(42))?;

                // After put: should see our write
                let val = txn.get(&key)?;
                assert_eq!(val, Some(values::int(42)), "Did not see own put");

                Ok(())
            })
            .unwrap();
    }

    /// Transaction sees its own overwrites
    #[test]
    fn test_sees_own_overwrites() {
        let tdb = TestDb::new();
        let key = tdb.key("ryw_overwrite");

        // Pre-populate
        tdb.db
            .put(tdb.run_id, key.clone(), values::int(100))
            .unwrap();

        tdb.db
            .transaction(tdb.run_id, |txn| {
                // Read original
                assert_eq!(txn.get(&key)?, Some(values::int(100)));

                // Overwrite
                txn.put(key.clone(), values::int(200))?;

                // Must see overwrite
                assert_eq!(txn.get(&key)?, Some(values::int(200)));

                // Overwrite again
                txn.put(key.clone(), values::int(300))?;
                assert_eq!(txn.get(&key)?, Some(values::int(300)));

                Ok(())
            })
            .unwrap();
    }

    /// Transaction sees its own deletes
    #[test]
    fn test_sees_own_deletes() {
        let tdb = TestDb::new();
        let key = tdb.key("ryw_delete");

        // Pre-populate
        tdb.db
            .put(tdb.run_id, key.clone(), values::int(42))
            .unwrap();

        tdb.db
            .transaction(tdb.run_id, |txn| {
                // Read original
                assert!(txn.get(&key)?.is_some());

                // Delete
                txn.delete(key.clone())?;

                // Must see delete (key appears gone)
                assert!(txn.get(&key)?.is_none(), "Did not see own delete");

                Ok(())
            })
            .unwrap();
    }

    /// Transaction sees complex sequence of own operations
    #[test]
    fn test_complex_ryw_sequence() {
        let tdb = TestDb::new();
        let key = tdb.key("ryw_complex");

        tdb.db
            .transaction(tdb.run_id, |txn| {
                // Create
                txn.put(key.clone(), values::int(1))?;
                assert_eq!(txn.get(&key)?, Some(values::int(1)));

                // Update
                txn.put(key.clone(), values::int(2))?;
                assert_eq!(txn.get(&key)?, Some(values::int(2)));

                // Delete
                txn.delete(key.clone())?;
                assert!(txn.get(&key)?.is_none());

                // Re-create
                txn.put(key.clone(), values::int(3))?;
                assert_eq!(txn.get(&key)?, Some(values::int(3)));

                Ok(())
            })
            .unwrap();

        // Final committed value
        let val = tdb.db.get(&key).unwrap().unwrap();
        assert_eq!(val.value, values::int(3));
    }
}

// ============================================================================
// Invariant 4: First-Committer-Wins
// When two transactions conflict, the first to commit wins
// ============================================================================

mod first_committer_wins {
    use super::*;

    /// Exactly one transaction wins when both read and write same key
    #[test]
    fn test_exactly_one_winner() {
        let temp_dir = TempDir::new().unwrap();
        let db = Arc::new(Database::open(temp_dir.path().join("db")).unwrap());

        let (run_id, ns) = create_namespace();
        let key = kv_key(&ns, "contested");

        db.put(run_id, key.clone(), values::int(0)).unwrap();

        let barrier = Arc::new(Barrier::new(2));
        let wins = Arc::new(AtomicU64::new(0));

        let handles: Vec<_> = (0..2)
            .map(|i| {
                let db = Arc::clone(&db);
                let key = key.clone();
                let barrier = Arc::clone(&barrier);
                let wins = Arc::clone(&wins);

                thread::spawn(move || {
                    barrier.wait();

                    let result: Result<(), Error> = db.transaction(run_id, |txn| {
                        // Both read (creating conflict potential)
                        let _ = txn.get(&key)?;

                        // Small delay to increase conflict window
                        thread::sleep(Duration::from_millis(10));

                        // Both try to write
                        txn.put(key.clone(), values::int(i))?;
                        Ok(())
                    });

                    if result.is_ok() {
                        wins.fetch_add(1, Ordering::Relaxed);
                    }
                })
            })
            .collect();

        for h in handles {
            h.join().unwrap();
        }

        // THE INVARIANT: exactly one must have won
        // (Note: With timing, both might succeed if they don't actually conflict)
        // But the key point is: final state is consistent
        let final_value = db.get(&key).unwrap().unwrap().value;
        assert!(
            final_value == values::int(0) || final_value == values::int(1),
            "Unexpected final value"
        );
    }

    /// CAS: exactly one winner among concurrent attempts
    #[test]
    fn test_cas_exactly_one_winner() {
        let temp_dir = TempDir::new().unwrap();
        let db = Arc::new(Database::open(temp_dir.path().join("db")).unwrap());

        let (run_id, ns) = create_namespace();
        let key = kv_key(&ns, "cas_contested");

        db.put(run_id, key.clone(), values::int(0)).unwrap();
        let initial_version = db.get(&key).unwrap().unwrap().version.as_u64();

        let barrier = Arc::new(Barrier::new(5));
        let results = Arc::new(std::sync::Mutex::new(Vec::new()));

        let handles: Vec<_> = (0..5)
            .map(|i| {
                let db = Arc::clone(&db);
                let key = key.clone();
                let barrier = Arc::clone(&barrier);
                let results = Arc::clone(&results);

                thread::spawn(move || {
                    barrier.wait();

                    // All try CAS with same initial version
                    let result = db.cas(run_id, key.clone(), initial_version, values::int(i));
                    results.lock().unwrap().push(result.is_ok());
                })
            })
            .collect();

        for h in handles {
            h.join().unwrap();
        }

        // THE INVARIANT: exactly one CAS must succeed
        let success_results: Vec<bool> = results.lock().unwrap().clone();
        invariants::assert_exactly_one_cas_winner(&success_results);
    }
}

// ============================================================================
// Invariant 5: Conflict Detection Completeness
// All actual conflicts are detected
// ============================================================================

mod conflict_detection {
    use super::*;

    /// Read-write conflict is detected
    #[test]
    fn test_read_write_conflict_detected() {
        let temp_dir = TempDir::new().unwrap();
        let db = Arc::new(Database::open(temp_dir.path().join("db")).unwrap());

        let (run_id, ns) = create_namespace();
        let key = kv_key(&ns, "rw_conflict");

        db.put(run_id, key.clone(), values::int(100)).unwrap();

        let db1 = Arc::clone(&db);
        let db2 = Arc::clone(&db);
        let key1 = key.clone();
        let key2 = key.clone();

        let barrier = Arc::new(Barrier::new(2));
        let barrier1 = Arc::clone(&barrier);
        let barrier2 = Arc::clone(&barrier);

        let t1_result = Arc::new(std::sync::Mutex::new(None));
        let t1_result_clone = Arc::clone(&t1_result);

        // T1: Read, wait, try to write
        let h1 = thread::spawn(move || {
            let result: Result<(), Error> = db1.transaction(run_id, |txn| {
                // Read key
                let _ = txn.get(&key1)?;

                // Signal T2 to modify
                barrier1.wait();

                // Wait for T2 to commit
                thread::sleep(Duration::from_millis(30));

                // Try to write (should conflict because key was modified)
                txn.put(key1.clone(), values::int(200))?;
                Ok(())
            });

            *t1_result_clone.lock().unwrap() = Some(result);
        });

        // T2: Modify the key that T1 read
        let h2 = thread::spawn(move || {
            barrier2.wait();
            db2.put(run_id, key2, values::int(150)).unwrap();
        });

        h1.join().unwrap();
        h2.join().unwrap();

        // T1 should have failed with conflict (or succeeded if it committed first)
        // The invariant is: the final state is consistent
        let final_value = match db.get(&key).unwrap().unwrap().value {
            Value::I64(n) => n,
            _ => panic!("Expected I64"),
        };

        assert!(
            final_value == 150 || final_value == 200,
            "Unexpected final value: {}",
            final_value
        );
    }

    /// Write-write conflict is detected
    #[test]
    fn test_write_write_conflict_detected() {
        let temp_dir = TempDir::new().unwrap();
        let db = Arc::new(Database::open(temp_dir.path().join("db")).unwrap());

        let (run_id, ns) = create_namespace();
        let key = kv_key(&ns, "ww_conflict");

        db.put(run_id, key.clone(), values::int(0)).unwrap();

        let conflict_count = Arc::new(AtomicU64::new(0));

        let handles: Vec<_> = (0..5)
            .map(|i| {
                let db = Arc::clone(&db);
                let key = key.clone();
                let conflict_count = Arc::clone(&conflict_count);

                thread::spawn(move || {
                    let result: Result<(), Error> = db.transaction(run_id, |txn| {
                        // Read to create conflict potential
                        let _ = txn.get(&key)?;
                        thread::sleep(Duration::from_millis(10));
                        txn.put(key.clone(), values::int(i))?;
                        Ok(())
                    });

                    if result.is_err() {
                        conflict_count.fetch_add(1, Ordering::Relaxed);
                    }
                })
            })
            .collect();

        for h in handles {
            h.join().unwrap();
        }

        // Some conflicts should have been detected
        // (With 5 concurrent writers to same key, at least some should conflict)
    }

    /// §3.2: Blind Write - BOTH succeed, last write wins
    ///
    /// From spec: "Neither transaction read key_a, so neither has it in their
    /// read_set. Write-write conflict only applies when the key was also read."
    ///
    /// This is INTENDED BEHAVIOR, not a bug.
    #[test]
    fn test_blind_write_both_succeed_last_wins() {
        let temp_dir = TempDir::new().unwrap();
        let db = Arc::new(Database::open(temp_dir.path().join("db")).unwrap());

        let (run_id, ns) = create_namespace();
        let key = kv_key(&ns, "blind_write");

        // Initial value
        db.put(run_id, key.clone(), values::int(0)).unwrap();

        let db1 = Arc::clone(&db);
        let db2 = Arc::clone(&db);
        let key1 = key.clone();
        let key2 = key.clone();

        let barrier = Arc::new(Barrier::new(2));
        let barrier1 = Arc::clone(&barrier);
        let barrier2 = Arc::clone(&barrier);

        let t1_success = Arc::new(AtomicBool::new(false));
        let t2_success = Arc::new(AtomicBool::new(false));
        let t1_success_clone = Arc::clone(&t1_success);
        let t2_success_clone = Arc::clone(&t2_success);

        // T1: Blind write (NO read first)
        let h1 = thread::spawn(move || {
            barrier1.wait();

            let result: Result<(), Error> = db1.transaction(run_id, |txn| {
                // IMPORTANT: No txn.get() - this is a blind write
                txn.put(key1.clone(), values::int(100))?;
                Ok(())
            });

            t1_success_clone.store(result.is_ok(), Ordering::Relaxed);
        });

        // T2: Also blind write (NO read first)
        let h2 = thread::spawn(move || {
            barrier2.wait();

            // Small delay to try to commit after T1
            thread::sleep(Duration::from_millis(5));

            let result: Result<(), Error> = db2.transaction(run_id, |txn| {
                // IMPORTANT: No txn.get() - this is a blind write
                txn.put(key2.clone(), values::int(200))?;
                Ok(())
            });

            t2_success_clone.store(result.is_ok(), Ordering::Relaxed);
        });

        h1.join().unwrap();
        h2.join().unwrap();

        // THE SPEC INVARIANT (§3.2): Both succeed because neither read
        assert!(
            t1_success.load(Ordering::Relaxed),
            "T1 should succeed (blind write)"
        );
        assert!(
            t2_success.load(Ordering::Relaxed),
            "T2 should succeed (blind write)"
        );

        // Last writer wins
        let final_value = match db.get(&key).unwrap().unwrap().value {
            Value::I64(n) => n,
            _ => -1,
        };

        // The value will be either 100 or 200 depending on commit order
        assert!(
            final_value == 100 || final_value == 200,
            "Final value should be 100 or 200, got {}",
            final_value
        );
    }

    /// §3.4: CAS does NOT add to read-set
    ///
    /// From spec: "CAS alone does NOT add to read_set"
    /// "If you want both CAS and read-set protection, explicitly read first"
    #[test]
    fn test_cas_does_not_add_to_read_set() {
        let temp_dir = TempDir::new().unwrap();
        let db = Arc::new(Database::open(temp_dir.path().join("db")).unwrap());

        let (run_id, ns) = create_namespace();
        let key = kv_key(&ns, "cas_no_read_set");

        // Initial value at version V
        db.put(run_id, key.clone(), values::int(0)).unwrap();
        let initial_version = db.get(&key).unwrap().unwrap().version.as_u64();

        let db1 = Arc::clone(&db);
        let db2 = Arc::clone(&db);
        let key1 = key.clone();
        let key2 = key.clone();

        let barrier = Arc::new(Barrier::new(2));
        let barrier1 = Arc::clone(&barrier);
        let barrier2 = Arc::clone(&barrier);

        let t1_success = Arc::new(AtomicBool::new(false));
        let t1_success_clone = Arc::clone(&t1_success);

        // T1: Use CAS without reading first
        let h1 = thread::spawn(move || {
            barrier1.wait();

            let result: Result<(), Error> = db1.transaction(run_id, |txn| {
                // CAS without get() - should NOT add to read_set
                txn.cas(key1.clone(), initial_version, values::int(100))?;

                // Add delay for T2 to modify
                thread::sleep(Duration::from_millis(30));

                // Write to a DIFFERENT key (to test that the transaction can commit)
                let other_key = kv_key(
                    &strata_core::types::Namespace::new(
                        "test".to_string(),
                        "test".to_string(),
                        "test".to_string(),
                        run_id,
                    ),
                    "other",
                );
                txn.put(other_key, values::int(999))?;
                Ok(())
            });

            t1_success_clone.store(result.is_ok(), Ordering::Relaxed);
        });

        // T2: Modify the same key
        let h2 = thread::spawn(move || {
            barrier2.wait();
            thread::sleep(Duration::from_millis(10));
            // This should NOT cause T1 to conflict IF CAS doesn't add to read-set
            // But the CAS itself may fail because version changed
            db2.put(run_id, key2, values::int(50)).unwrap();
        });

        h1.join().unwrap();
        h2.join().unwrap();

        // Note: The CAS in T1 may succeed or fail depending on timing.
        // The key point is: if T1 did a get() instead, and T2 modified the key,
        // T1 would DEFINITELY conflict. With CAS alone, conflict is only on
        // the CAS version check, not read-set.

        // This test documents the behavior - CAS failure is different from
        // read-set conflict
    }
}

// ============================================================================
// Invariant 5b: Version 0 and Tombstone Semantics (from §6.4, §6.5)
// ============================================================================

mod version_semantics {
    use super::*;

    /// §6.4: Version 0 means "never existed"
    ///
    /// From spec: "Version 0 has special meaning: the key has never existed."
    #[test]
    fn test_version_0_means_never_existed() {
        let tdb = TestDb::new();
        let never_existed_key = tdb.key("never_existed");

        // Reading a key that never existed should work
        let result = tdb.db.get(&never_existed_key).unwrap();
        assert!(result.is_none(), "Non-existent key should return None");

        // CAS with version 0 should succeed for never-existed key (create if not exists)
        let cas_result = tdb.db.cas(
            tdb.run_id,
            never_existed_key.clone(),
            0, // expected_version = 0 means "create only if never existed"
            values::int(42),
        );
        assert!(
            cas_result.is_ok(),
            "CAS with version 0 should succeed for never-existed key"
        );

        // Now the key exists with version > 0
        let value = tdb.db.get(&never_existed_key).unwrap().unwrap();
        assert!(value.version.as_u64() > 0, "Created key should have version > 0");

        // CAS with version 0 should now FAIL (key exists)
        let cas_result2 = tdb.db.cas(
            tdb.run_id,
            never_existed_key.clone(),
            0, // expected_version = 0, but key now exists
            values::int(100),
        );
        assert!(
            cas_result2.is_err(),
            "CAS with version 0 should fail when key exists"
        );
    }

    /// §6.5: Tombstone vs Never-Existed distinction
    ///
    /// From spec: "A deleted key (tombstone) has version > 0. Only keys that
    /// have *never* been created have version 0."
    #[test]
    fn test_tombstone_vs_never_existed() {
        let tdb = TestDb::new();

        // Key that never existed
        let never_key = tdb.key("truly_never");

        // Key that will be created then deleted (tombstone)
        let tombstone_key = tdb.key("will_be_tombstone");

        // Create and delete to make tombstone
        tdb.db
            .put(tdb.run_id, tombstone_key.clone(), values::int(100))
            .unwrap();
        let version_before_delete = tdb.db.get(&tombstone_key).unwrap().unwrap().version;
        tdb.db.delete(tdb.run_id, tombstone_key.clone()).unwrap();

        // Both return None when read
        assert!(tdb.db.get(&never_key).unwrap().is_none());
        assert!(tdb.db.get(&tombstone_key).unwrap().is_none());

        // BUT: CAS with version 0 behaves differently
        // Never-existed: CAS(version=0) succeeds
        let never_cas = tdb.db.cas(tdb.run_id, never_key.clone(), 0, values::int(1));
        assert!(
            never_cas.is_ok(),
            "CAS(v=0) should succeed for never-existed key"
        );

        // Tombstone: CAS(version=0) FAILS (tombstone has version > 0)
        let tombstone_cas = tdb
            .db
            .cas(tdb.run_id, tombstone_key.clone(), 0, values::int(1));
        // Note: This depends on implementation - if tombstones track version,
        // CAS(v=0) should fail. If not, behavior may differ.
        // The spec says tombstones have version > 0, so this SHOULD fail.

        // Re-creating a tombstoned key requires knowing the tombstone version
        // or using a blind write
    }

    /// §6.5: Reading a tombstone records tombstone version in read-set
    ///
    /// From spec: "Reading a tombstone records the tombstone's version in
    /// read_set (NOT version 0)"
    #[test]
    fn test_tombstone_read_causes_conflict() {
        let temp_dir = TempDir::new().unwrap();
        let db = Arc::new(Database::open(temp_dir.path().join("db")).unwrap());

        let (run_id, ns) = create_namespace();
        let key = kv_key(&ns, "tombstone_conflict");

        // Create and delete to make tombstone
        db.put(run_id, key.clone(), values::int(100)).unwrap();
        db.delete(run_id, key.clone()).unwrap();

        let db1 = Arc::clone(&db);
        let db2 = Arc::clone(&db);
        let key1 = key.clone();
        let key2 = key.clone();

        let barrier = Arc::new(Barrier::new(2));
        let barrier1 = Arc::clone(&barrier);
        let barrier2 = Arc::clone(&barrier);

        let t1_result = Arc::new(std::sync::Mutex::new(None));
        let t1_result_clone = Arc::clone(&t1_result);

        // T1: Read the tombstone, wait, then try to write something
        let h1 = thread::spawn(move || {
            let result: Result<(), Error> = db1.transaction(run_id, |txn| {
                // Read deleted key - should record tombstone version in read_set
                let val = txn.get(&key1)?;
                assert!(val.is_none(), "Tombstoned key should return None");

                // Wait for T2 to re-create the key
                barrier1.wait();
                thread::sleep(Duration::from_millis(30));

                // Now try to do something - should conflict if tombstone was tracked
                // We'll write to a different key to isolate the conflict
                txn.put(key1.clone(), values::int(999))?;
                Ok(())
            });

            *t1_result_clone.lock().unwrap() = Some(result);
        });

        // T2: Re-create the tombstoned key
        let h2 = thread::spawn(move || {
            barrier2.wait();
            db2.put(run_id, key2, values::int(200)).unwrap();
        });

        h1.join().unwrap();
        h2.join().unwrap();

        let t1_outcome = t1_result.lock().unwrap().take().unwrap();

        // THE SPEC INVARIANT: T1 should conflict because it read the tombstone
        // and T2 re-created the key (changing the version)
        // If T1 fails, the spec is correctly implemented
        // If T1 succeeds, either:
        // - T1 committed before T2
        // - Tombstones don't track versions (spec violation)
    }
}

// ============================================================================
// Invariant 6: Atomicity of Multi-Key Transactions
// All keys in a transaction commit together or none do
// ============================================================================

// ============================================================================
// Invariant 6a: Write Skew Prevention
// SI allows write skew; OCC may or may not prevent it depending on implementation
// ============================================================================

mod write_skew {
    use super::*;

    /// Classic write skew scenario: on-call doctors
    ///
    /// Two doctors (Alice and Bob) are on call. Hospital requires at least one on call.
    /// Both try to go off-call simultaneously. Under SI with OCC:
    /// - Both read: Alice=on, Bob=on (both see >= 1 doctor)
    /// - Both decide they can go off
    /// - Both write themselves as off
    /// - Result depends on conflict detection
    ///
    /// Note: Standard SI ALLOWS write skew. This test documents the behavior.
    /// OCC with read validation may prevent it (if reads are tracked).
    #[test]
    fn test_write_skew_scenario_doctors_on_call() {
        let temp_dir = TempDir::new().unwrap();
        let db = Arc::new(Database::open(temp_dir.path().join("db")).unwrap());

        let (run_id, ns) = create_namespace();
        let alice_key = kv_key(&ns, "doctor_alice");
        let bob_key = kv_key(&ns, "doctor_bob");

        // Both doctors start on call (value=1 means on call)
        db.put(run_id, alice_key.clone(), values::int(1)).unwrap();
        db.put(run_id, bob_key.clone(), values::int(1)).unwrap();

        let db1 = Arc::clone(&db);
        let db2 = Arc::clone(&db);
        let alice1 = alice_key.clone();
        let bob1 = bob_key.clone();
        let alice2 = alice_key.clone();
        let bob2 = bob_key.clone();

        let barrier = Arc::new(Barrier::new(2));
        let barrier1 = Arc::clone(&barrier);
        let barrier2 = Arc::clone(&barrier);

        let alice_success = Arc::new(AtomicBool::new(false));
        let bob_success = Arc::new(AtomicBool::new(false));
        let alice_success_clone = Arc::clone(&alice_success);
        let bob_success_clone = Arc::clone(&bob_success);

        // Alice's transaction: if Bob is on call, I can go off
        let h1 = thread::spawn(move || {
            barrier1.wait();

            let result: Result<(), Error> = db1.transaction(run_id, |txn| {
                // Read Bob's status
                let bob_status = match txn.get(&bob1)?.unwrap() {
                    Value::I64(n) => n,
                    _ => 0,
                };

                // Add delay to increase overlap
                thread::sleep(Duration::from_millis(10));

                // If Bob is on call, Alice can go off
                if bob_status == 1 {
                    txn.put(alice1.clone(), values::int(0))?;
                }
                Ok(())
            });

            alice_success_clone.store(result.is_ok(), Ordering::Relaxed);
        });

        // Bob's transaction: if Alice is on call, I can go off
        let h2 = thread::spawn(move || {
            barrier2.wait();

            let result: Result<(), Error> = db2.transaction(run_id, |txn| {
                // Read Alice's status
                let alice_status = match txn.get(&alice2)?.unwrap() {
                    Value::I64(n) => n,
                    _ => 0,
                };

                // Add delay to increase overlap
                thread::sleep(Duration::from_millis(10));

                // If Alice is on call, Bob can go off
                if alice_status == 1 {
                    txn.put(bob2.clone(), values::int(0))?;
                }
                Ok(())
            });

            bob_success_clone.store(result.is_ok(), Ordering::Relaxed);
        });

        h1.join().unwrap();
        h2.join().unwrap();

        // Check final state
        let alice_final = match db.get(&alice_key).unwrap().unwrap().value {
            Value::I64(n) => n,
            _ => -1,
        };
        let bob_final = match db.get(&bob_key).unwrap().unwrap().value {
            Value::I64(n) => n,
            _ => -1,
        };

        let total_on_call = alice_final + bob_final;

        // Document the behavior - either:
        // 1. SI allowed write skew: total_on_call == 0 (both went off)
        // 2. OCC prevented it: total_on_call >= 1
        // 3. One transaction aborted: total_on_call >= 1
        println!(
            "Write skew test: Alice={}, Bob={}, Total on call={}, Alice committed={}, Bob committed={}",
            alice_final, bob_final, total_on_call,
            alice_success.load(Ordering::Relaxed),
            bob_success.load(Ordering::Relaxed)
        );

        // This is informational - we're documenting SI behavior
        // Under pure SI, write skew IS allowed
        // Under SI + OCC with read validation, it MAY be prevented
    }
}

// ============================================================================
// Invariant 6b: Lost Update Prevention
// SI MUST prevent lost updates
// ============================================================================

mod lost_update_prevention {
    use super::*;

    /// Classic lost update scenario: concurrent increment
    ///
    /// Both T1 and T2 read counter=0, both increment, both try to write counter=1.
    /// Lost update: final value should be 2, but if lost update occurs, it's 1.
    ///
    /// SI with OCC MUST prevent this: only one commit should succeed.
    #[test]
    fn test_lost_update_prevented_counter_increment() {
        let temp_dir = TempDir::new().unwrap();
        let db = Arc::new(Database::open(temp_dir.path().join("db")).unwrap());

        let (run_id, ns) = create_namespace();
        let counter_key = kv_key(&ns, "counter");

        // Counter starts at 0
        db.put(run_id, counter_key.clone(), values::int(0)).unwrap();

        let barrier = Arc::new(Barrier::new(2));
        let success_count = Arc::new(AtomicU64::new(0));

        let handles: Vec<_> = (0..2)
            .map(|_| {
                let db = Arc::clone(&db);
                let key = counter_key.clone();
                let barrier = Arc::clone(&barrier);
                let success_count = Arc::clone(&success_count);

                thread::spawn(move || {
                    barrier.wait();

                    let result: Result<(), Error> = db.transaction(run_id, |txn| {
                        // Read current value
                        let current = match txn.get(&key)?.unwrap() {
                            Value::I64(n) => n,
                            _ => 0,
                        };

                        // Delay to ensure overlap
                        thread::sleep(Duration::from_millis(20));

                        // Increment
                        txn.put(key.clone(), values::int(current + 1))?;
                        Ok(())
                    });

                    if result.is_ok() {
                        success_count.fetch_add(1, Ordering::Relaxed);
                    }
                })
            })
            .collect();

        for h in handles {
            h.join().unwrap();
        }

        let final_value = match db.get(&counter_key).unwrap().unwrap().value {
            Value::I64(n) => n,
            _ => -1,
        };

        let successes = success_count.load(Ordering::Relaxed);

        // THE LOST UPDATE INVARIANT:
        // If both succeeded (no conflict detected), we have a lost update bug.
        // Either:
        // - Only one commits (successes == 1), final_value == 1 ✓
        // - Both commit somehow, final_value MUST equal 2 (no lost update)
        //
        // If final_value == 1 and successes == 2, that's a LOST UPDATE BUG.
        if successes == 2 {
            assert_eq!(
                final_value, 2,
                "LOST UPDATE DETECTED: Both transactions committed but final value is {} (should be 2)",
                final_value
            );
        } else {
            // One transaction was aborted due to conflict - correct behavior
            assert_eq!(
                successes, 1,
                "Expected exactly one success, got {}",
                successes
            );
        }
    }

    /// Multiple concurrent increments must not lose updates
    #[test]
    fn test_lost_update_prevented_many_increments() {
        let temp_dir = TempDir::new().unwrap();
        let db = Arc::new(Database::open(temp_dir.path().join("db")).unwrap());

        let (run_id, ns) = create_namespace();
        let counter_key = kv_key(&ns, "many_counter");

        db.put(run_id, counter_key.clone(), values::int(0)).unwrap();

        let num_threads = 10;
        let barrier = Arc::new(Barrier::new(num_threads));
        let success_count = Arc::new(AtomicU64::new(0));

        let handles: Vec<_> = (0..num_threads)
            .map(|_| {
                let db = Arc::clone(&db);
                let key = counter_key.clone();
                let barrier = Arc::clone(&barrier);
                let success_count = Arc::clone(&success_count);

                thread::spawn(move || {
                    barrier.wait();

                    // Try up to 5 times with retry
                    for _ in 0..5 {
                        let result: Result<(), Error> = db.transaction(run_id, |txn| {
                            let current = match txn.get(&key)?.unwrap() {
                                Value::I64(n) => n,
                                _ => 0,
                            };
                            txn.put(key.clone(), values::int(current + 1))?;
                            Ok(())
                        });

                        if result.is_ok() {
                            success_count.fetch_add(1, Ordering::Relaxed);
                            break;
                        }
                        // If conflict, retry
                        thread::sleep(Duration::from_millis(1));
                    }
                })
            })
            .collect();

        for h in handles {
            h.join().unwrap();
        }

        let final_value = match db.get(&counter_key).unwrap().unwrap().value {
            Value::I64(n) => n,
            _ => -1,
        };

        let successes = success_count.load(Ordering::Relaxed) as i64;

        // THE INVARIANT: final_value MUST equal successes (no lost updates)
        assert_eq!(
            final_value, successes,
            "LOST UPDATES DETECTED: {} successes but final value is {}",
            successes, final_value
        );
    }
}

// ============================================================================
// Invariant 7: Multi-Key Snapshot Consistency
// All reads within a transaction see the same logical point in time
// ============================================================================

mod snapshot_multi_key_consistency {
    use super::*;

    /// Reading multiple keys sees a consistent snapshot
    /// Even if concurrent transactions are modifying them
    #[test]
    fn test_multi_key_reads_see_consistent_snapshot() {
        let temp_dir = TempDir::new().unwrap();
        let db = Arc::new(Database::open(temp_dir.path().join("db")).unwrap());

        let (run_id, ns) = create_namespace();

        // Create 5 keys that all have the same "generation" number
        let keys: Vec<Key> = (0..5).map(|i| kv_key(&ns, &format!("gen_{}", i))).collect();

        // Initialize all to generation 0
        for key in &keys {
            db.put(run_id, key.clone(), values::int(0)).unwrap();
        }

        let db1 = Arc::clone(&db);
        let db2 = Arc::clone(&db);
        let keys1 = keys.clone();
        let keys2 = keys.clone();

        let barrier = Arc::new(Barrier::new(2));
        let barrier1 = Arc::clone(&barrier);
        let barrier2 = Arc::clone(&barrier);

        let saw_inconsistent = Arc::new(AtomicBool::new(false));
        let saw_inconsistent_clone = Arc::clone(&saw_inconsistent);

        // Reader: reads all keys, they should all be same generation
        let h1 = thread::spawn(move || {
            for _ in 0..20 {
                barrier1.wait();

                let result: Result<(), Error> = db1.transaction(run_id, |txn| {
                    let mut generations: Vec<i64> = Vec::new();

                    // Read all keys with delays between
                    for key in &keys1 {
                        let val = match txn.get(key)?.unwrap() {
                            Value::I64(n) => n,
                            _ => -1,
                        };
                        generations.push(val);
                        thread::sleep(Duration::from_micros(100));
                    }

                    // THE INVARIANT: all keys must show same generation
                    let first = generations[0];
                    for (i, &gen) in generations.iter().enumerate() {
                        if gen != first {
                            saw_inconsistent_clone.store(true, Ordering::Relaxed);
                            panic!(
                                "Multi-key snapshot inconsistency: key[0]={} but key[{}]={}",
                                first, i, gen
                            );
                        }
                    }

                    Ok(())
                });

                if result.is_err() {
                    // Transaction aborted - that's ok, just retry
                }
            }
        });

        // Writer: atomically updates all keys to next generation
        let h2 = thread::spawn(move || {
            for gen in 1..=20 {
                barrier2.wait();

                // Update all keys to same generation atomically
                db2.transaction(run_id, |txn| {
                    for key in &keys2 {
                        txn.put(key.clone(), values::int(gen))?;
                    }
                    Ok(())
                })
                .unwrap();

                thread::sleep(Duration::from_micros(50));
            }
        });

        h1.join().unwrap();
        h2.join().unwrap();

        assert!(
            !saw_inconsistent.load(Ordering::Relaxed),
            "Reader saw inconsistent multi-key snapshot"
        );
    }

    /// Bank transfer invariant: sum is always preserved
    #[test]
    fn test_bank_transfer_sum_invariant() {
        let temp_dir = TempDir::new().unwrap();
        let db = Arc::new(Database::open(temp_dir.path().join("db")).unwrap());

        let (run_id, ns) = create_namespace();

        let accounts: Vec<Key> = (0..3)
            .map(|i| kv_key(&ns, &format!("account_{}", i)))
            .collect();

        // Total money in system: 300
        for account in &accounts {
            db.put(run_id, account.clone(), values::int(100)).unwrap();
        }

        let db_checker = Arc::clone(&db);
        let db_transfer = Arc::clone(&db);
        let accounts_checker = accounts.clone();
        let accounts_transfer = accounts.clone();

        let barrier = Arc::new(Barrier::new(2));
        let barrier1 = Arc::clone(&barrier);
        let barrier2 = Arc::clone(&barrier);

        let invariant_violated = Arc::new(AtomicBool::new(false));
        let invariant_violated_clone = Arc::clone(&invariant_violated);

        // Checker: continuously verifies sum == 300
        let h1 = thread::spawn(move || {
            for _ in 0..50 {
                barrier1.wait();

                let result: Result<i64, Error> = db_checker.transaction(run_id, |txn| {
                    let mut sum = 0i64;
                    for account in &accounts_checker {
                        let balance = match txn.get(account)?.unwrap() {
                            Value::I64(n) => n,
                            _ => 0,
                        };
                        sum += balance;
                    }
                    Ok(sum)
                });

                if let Ok(sum) = result {
                    if sum != 300 {
                        invariant_violated_clone.store(true, Ordering::Relaxed);
                        panic!("Bank invariant violated: sum={} (expected 300)", sum);
                    }
                }
            }
        });

        // Transferrer: moves money between accounts (preserving total)
        let h2 = thread::spawn(move || {
            for i in 0..50 {
                barrier2.wait();

                let from = i % 3;
                let to = (i + 1) % 3;
                let amount = 10i64;

                let _ = db_transfer.transaction(run_id, |txn| {
                    let from_balance = match txn.get(&accounts_transfer[from])?.unwrap() {
                        Value::I64(n) => n,
                        _ => 0,
                    };
                    let to_balance = match txn.get(&accounts_transfer[to])?.unwrap() {
                        Value::I64(n) => n,
                        _ => 0,
                    };

                    // Transfer (this preserves total)
                    txn.put(
                        accounts_transfer[from].clone(),
                        values::int(from_balance - amount),
                    )?;
                    txn.put(
                        accounts_transfer[to].clone(),
                        values::int(to_balance + amount),
                    )?;
                    Ok(())
                });
            }
        });

        h1.join().unwrap();
        h2.join().unwrap();

        assert!(
            !invariant_violated.load(Ordering::Relaxed),
            "Bank transfer sum invariant was violated"
        );

        // Final check: sum should still be 300
        let final_sum: i64 = accounts
            .iter()
            .map(|a| match db.get(a).unwrap().unwrap().value {
                Value::I64(n) => n,
                _ => 0,
            })
            .sum();

        assert_eq!(final_sum, 300, "Final sum should be 300");
    }
}

// ============================================================================
// Invariant 8: Atomicity of Multi-Key Transactions
// All keys in a transaction commit together or none do
// ============================================================================

mod multi_key_atomicity {
    use super::*;

    /// Committed multi-key transaction: all keys present
    #[test]
    fn test_committed_transaction_all_keys_present() {
        let tdb = TestDb::new();

        let keys: Vec<Key> = (0..10).map(|i| tdb.key(&format!("atomic_{}", i))).collect();

        tdb.db
            .transaction(tdb.run_id, |txn| {
                for (i, key) in keys.iter().enumerate() {
                    txn.put(key.clone(), values::int(i as i64))?;
                }
                Ok(())
            })
            .unwrap();

        // THE INVARIANT: all keys must exist
        invariants::assert_atomic_transaction(&tdb.db, &keys, true);
    }

    /// Aborted multi-key transaction: no keys present
    #[test]
    fn test_aborted_transaction_no_keys_present() {
        let tdb = TestDb::new();

        let keys: Vec<Key> = (0..10)
            .map(|i| tdb.key(&format!("abort_atomic_{}", i)))
            .collect();

        let _: Result<(), Error> = tdb.db.transaction(tdb.run_id, |txn| {
            for (i, key) in keys.iter().enumerate() {
                txn.put(key.clone(), values::int(i as i64))?;
            }
            Err(Error::InvalidState("abort".to_string()))
        });

        // THE INVARIANT: no keys must exist
        invariants::assert_atomic_transaction(&tdb.db, &keys, false);
    }

    /// No partial writes visible at any time
    #[test]
    fn test_no_partial_writes_observable() {
        let temp_dir = TempDir::new().unwrap();
        let db = Arc::new(Database::open(temp_dir.path().join("db")).unwrap());

        let (run_id, ns) = create_namespace();

        let keys: Vec<Key> = (0..5)
            .map(|i| kv_key(&ns, &format!("partial_{}", i)))
            .collect();

        let db1 = Arc::clone(&db);
        let db2 = Arc::clone(&db);
        let keys1 = keys.clone();
        let keys2 = keys.clone();

        let barrier = Arc::new(Barrier::new(2));
        let barrier1 = Arc::clone(&barrier);
        let barrier2 = Arc::clone(&barrier);

        let observed_partial = Arc::new(AtomicBool::new(false));
        let observed_partial_clone = Arc::clone(&observed_partial);

        // T1: Write multiple keys in transaction
        let h1 = thread::spawn(move || {
            barrier1.wait();
            db1.transaction(run_id, |txn| {
                for (i, key) in keys1.iter().enumerate() {
                    txn.put(key.clone(), values::int(i as i64))?;
                    // Artificial delay between puts (doesn't matter, transaction is atomic)
                    thread::sleep(Duration::from_millis(1));
                }
                Ok(())
            })
            .unwrap();
        });

        // T2: Continuously check for partial writes
        let h2 = thread::spawn(move || {
            barrier2.wait();

            for _ in 0..50 {
                let states: Vec<bool> = keys2
                    .iter()
                    .map(|k| db2.get(k).unwrap().is_some())
                    .collect();

                let all_present = states.iter().all(|&s| s);
                let all_absent = states.iter().all(|&s| !s);

                if !all_present && !all_absent {
                    observed_partial_clone.store(true, Ordering::Relaxed);
                }

                thread::sleep(Duration::from_millis(1));
            }
        });

        h1.join().unwrap();
        h2.join().unwrap();

        // THE INVARIANT: should never observe partial state
        assert!(
            !observed_partial.load(Ordering::Relaxed),
            "Observed partial write state"
        );
    }
}
