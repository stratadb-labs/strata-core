//! WAL Semantic Invariant Tests
//!
//! These tests enforce the fundamental WAL contracts, not mechanics.
//!
//! ## Core WAL Invariants
//!
//! 1. **Recovery Completeness**: State after recovery == State before crash
//! 2. **No Partial Writes**: Incomplete transactions do not appear after recovery
//! 3. **Order Preservation**: Writes appear in the order they were committed
//! 4. **Idempotence**: Replaying WAL twice produces the same state
//! 5. **Prefix Consistency**: Replaying prefix of WAL reconstructs prefix state

use super::test_utils::*;
use strata_core::types::Key;
use strata_core::value::Value;
use strata_engine::Database;
use std::collections::HashMap;
use tempfile::TempDir;

// ============================================================================
// Invariant 1: Recovery Completeness
// State after recovery must be identical to state before crash
// ============================================================================

mod recovery_completeness {
    use super::*;

    /// THE fundamental M1 test: full state comparison before/after recovery
    #[test]
    fn test_full_state_preserved_after_recovery() {
        let pdb = PersistentTestDb::new();

        // Build up significant state
        let state_before = {
            let db = pdb.open();

            // Write various keys
            for i in 0..50 {
                let key = pdb.key(&format!("key_{}", i));
                db.put(pdb.run_id, key, values::int(i * 10)).unwrap();
            }

            // Delete some
            for i in [5, 15, 25, 35, 45] {
                let key = pdb.key(&format!("key_{}", i));
                db.delete(pdb.run_id, key).unwrap();
            }

            // Overwrite some
            for i in [0, 10, 20, 30, 40] {
                let key = pdb.key(&format!("key_{}", i));
                db.put(pdb.run_id, key, values::int(i * 100)).unwrap();
            }

            // Capture full state
            DatabaseStateSnapshot::capture(&db, &pdb.ns)
        };

        // Simulate crash by just closing (drop)

        // Recover and compare
        let state_after = {
            let db = pdb.open();
            DatabaseStateSnapshot::capture(&db, &pdb.ns)
        };

        // THE INVARIANT: state must be identical
        invariants::assert_recovery_preserves_state(&state_before, &state_after);
    }

    /// Multiple cycles of write-crash-recover must all preserve state
    #[test]
    fn test_repeated_crash_recovery_preserves_state() {
        let pdb = PersistentTestDb::new();

        for cycle in 0..5 {
            let state_before = {
                let db = pdb.open();

                // Add new data each cycle
                for i in 0..10 {
                    let key = pdb.key(&format!("cycle_{}_key_{}", cycle, i));
                    db.put(pdb.run_id, key, values::int((cycle * 100 + i) as i64))
                        .unwrap();
                }

                // Modify some existing data
                if cycle > 0 {
                    let key = pdb.key(&format!("cycle_{}_key_0", cycle - 1));
                    db.put(pdb.run_id, key, values::int(9999)).unwrap();
                }

                DatabaseStateSnapshot::capture(&db, &pdb.ns)
            };

            // Crash

            let state_after = {
                let db = pdb.open();
                DatabaseStateSnapshot::capture(&db, &pdb.ns)
            };

            invariants::assert_recovery_preserves_state(&state_before, &state_after);
        }
    }

    /// Large state must be fully preserved
    #[test]
    fn test_large_state_fully_preserved() {
        let pdb = PersistentTestDb::new();

        let state_before = {
            let db = pdb.open();

            // 1000 keys
            for i in 0..1000 {
                let key = pdb.key(&format!("large_{}", i));
                db.put(pdb.run_id, key, values::int(i)).unwrap();
            }

            DatabaseStateSnapshot::capture(&db, &pdb.ns)
        };

        let state_after = {
            let db = pdb.open();
            DatabaseStateSnapshot::capture(&db, &pdb.ns)
        };

        // Must have exactly 1000 entries, all identical
        assert_eq!(state_before.count, 1000);
        assert_eq!(state_after.count, 1000);
        invariants::assert_recovery_preserves_state(&state_before, &state_after);
    }

    /// Transaction state must be fully preserved
    #[test]
    fn test_transaction_state_preserved() {
        let pdb = PersistentTestDb::new();

        let state_before = {
            let db = pdb.open();

            // Multi-key transaction
            db.transaction(pdb.run_id, |txn| {
                for i in 0..20 {
                    let key = kv_key(&pdb.ns, &format!("txn_key_{}", i));
                    txn.put(key, values::int(i))?;
                }
                Ok(())
            })
            .unwrap();

            DatabaseStateSnapshot::capture(&db, &pdb.ns)
        };

        let state_after = {
            let db = pdb.open();
            DatabaseStateSnapshot::capture(&db, &pdb.ns)
        };

        invariants::assert_recovery_preserves_state(&state_before, &state_after);
    }
}

// ============================================================================
// Invariant 2: No Partial Writes
// Incomplete or aborted transactions must not appear after recovery
// ============================================================================

mod no_partial_writes {
    use super::*;
    use strata_core::error::Error;

    /// Aborted transaction must leave no trace
    #[test]
    fn test_aborted_transaction_invisible_after_recovery() {
        let pdb = PersistentTestDb::new();

        let committed_keys: Vec<Key> = (0..5)
            .map(|i| pdb.key(&format!("committed_{}", i)))
            .collect();
        let aborted_keys: Vec<Key> = (0..5).map(|i| pdb.key(&format!("aborted_{}", i))).collect();

        {
            let db = pdb.open();

            // Committed transaction
            db.transaction(pdb.run_id, |txn| {
                for (i, key) in committed_keys.iter().enumerate() {
                    txn.put(key.clone(), values::int(i as i64))?;
                }
                Ok(())
            })
            .unwrap();

            // Aborted transaction
            let _: Result<(), Error> = db.transaction(pdb.run_id, |txn| {
                for (i, key) in aborted_keys.iter().enumerate() {
                    txn.put(key.clone(), values::int(i as i64))?;
                }
                Err(Error::InvalidState("abort".to_string()))
            });
        }

        // Recover
        {
            let db = pdb.open();

            // Committed keys must all exist
            invariants::assert_atomic_transaction(&db, &committed_keys, true);

            // Aborted keys must all be absent
            invariants::assert_atomic_transaction(&db, &aborted_keys, false);
        }
    }

    /// Transaction with error mid-way leaves no partial state
    #[test]
    fn test_mid_transaction_error_no_partial_state() {
        let pdb = PersistentTestDb::new();

        let keys: Vec<Key> = (0..10)
            .map(|i| pdb.key(&format!("mid_error_{}", i)))
            .collect();

        {
            let db = pdb.open();

            let _: Result<(), Error> = db.transaction(pdb.run_id, |txn| {
                for (i, key) in keys.iter().enumerate() {
                    txn.put(key.clone(), values::int(i as i64))?;
                    if i == 5 {
                        return Err(Error::InvalidState("mid-transaction error".to_string()));
                    }
                }
                Ok(())
            });
        }

        // Recover
        {
            let db = pdb.open();

            // NONE of the keys should exist - not even the first 5
            for key in &keys {
                assert!(
                    db.get(key).unwrap().is_none(),
                    "Partial write detected for key {:?}",
                    key
                );
            }
        }
    }

    /// Multiple aborted transactions leave no traces
    #[test]
    fn test_many_aborted_transactions_no_traces() {
        let pdb = PersistentTestDb::new();

        {
            let db = pdb.open();

            // One successful transaction
            let good_key = pdb.key("good_key");
            db.put(pdb.run_id, good_key.clone(), values::int(42))
                .unwrap();

            // Many aborted transactions
            for batch in 0..10 {
                let _: Result<(), Error> = db.transaction(pdb.run_id, |txn| {
                    for i in 0..5 {
                        let key = kv_key(&pdb.ns, &format!("bad_{}_{}", batch, i));
                        txn.put(key, values::int(999))?;
                    }
                    Err(Error::InvalidState("abort".to_string()))
                });
            }
        }

        // Recover
        {
            let db = pdb.open();
            let state = DatabaseStateSnapshot::capture(&db, &pdb.ns);

            // Only 1 key should exist
            assert_eq!(
                state.count, 1,
                "Expected only 1 key, found {}. Aborted transactions leaked.",
                state.count
            );
        }
    }
}

// ============================================================================
// Invariant 3: Order Preservation
// Writes appear in the order they were committed
// ============================================================================

mod order_preservation {
    use super::*;

    /// Version numbers must be strictly increasing across commits
    #[test]
    fn test_version_order_preserved_after_recovery() {
        let pdb = PersistentTestDb::new();

        let mut expected_order: Vec<(Key, i64)> = Vec::new();

        {
            let db = pdb.open();

            // Write keys in specific order
            for i in 0..20 {
                let key = pdb.key(&format!("ordered_{}", i));
                db.put(pdb.run_id, key.clone(), values::int(i)).unwrap();
                expected_order.push((key, i));
            }
        }

        // Recover
        {
            let db = pdb.open();

            // Collect versions in order of original writes
            let mut versions: Vec<u64> = Vec::new();
            for (key, expected_value) in &expected_order {
                let val = db.get(key).unwrap().expect("Key should exist");
                assert_eq!(val.value, values::int(*expected_value));
                versions.push(val.version.as_u64());
            }

            // Versions must be strictly increasing
            invariants::assert_monotonic_versions(&versions);
        }
    }

    /// Overwrites maintain proper version ordering
    #[test]
    fn test_overwrite_order_preserved() {
        let pdb = PersistentTestDb::new();
        let key = pdb.key("overwritten");

        let mut version_history: Vec<u64> = Vec::new();

        {
            let db = pdb.open();

            // Write same key multiple times
            for i in 0..10 {
                db.put(pdb.run_id, key.clone(), values::int(i)).unwrap();
                version_history.push(db.get(&key).unwrap().unwrap().version.as_u64());
            }
        }

        // Versions during writes must have been monotonic
        invariants::assert_monotonic_versions(&version_history);

        // After recovery, should have the last value with the last version
        {
            let db = pdb.open();
            let val = db.get(&key).unwrap().unwrap();
            assert_eq!(val.value, values::int(9));
            assert_eq!(val.version.as_u64(), *version_history.last().unwrap());
        }
    }

    /// Interleaved writes to different keys maintain global order
    #[test]
    fn test_interleaved_writes_global_order() {
        let pdb = PersistentTestDb::new();

        let mut write_order: Vec<(Key, u64)> = Vec::new();

        {
            let db = pdb.open();

            // Interleave writes to 3 different keys
            for round in 0..10 {
                for key_id in 0..3 {
                    let key = pdb.key(&format!("interleave_{}", key_id));
                    db.put(pdb.run_id, key.clone(), values::int(round * 10 + key_id))
                        .unwrap();
                    let version = db.get(&key).unwrap().unwrap().version.as_u64();
                    write_order.push((key, version));
                }
            }
        }

        // All versions across all keys must be globally monotonic
        let versions: Vec<u64> = write_order.iter().map(|(_, v)| *v).collect();
        invariants::assert_monotonic_versions(&versions);
    }
}

// ============================================================================
// Invariant 4: Idempotence
// Replaying WAL twice produces the same state
// ============================================================================

mod idempotence {
    use super::*;

    /// THE fundamental idempotence test: Replaying WAL twice produces identical state
    /// This tests that recovery is a pure function: f(f(state)) == f(state)
    #[test]
    fn test_replay_twice_produces_identical_state() {
        let pdb = PersistentTestDb::new();

        // Write diverse initial state
        {
            let db = pdb.open();

            // Simple puts
            for i in 0..30 {
                let key = pdb.key(&format!("replay_key_{}", i));
                db.put(pdb.run_id, key, values::int(i)).unwrap();
            }

            // Deletes
            for i in [5, 10, 15, 20, 25] {
                let key = pdb.key(&format!("replay_key_{}", i));
                db.delete(pdb.run_id, key).unwrap();
            }

            // Overwrites
            for i in [0, 1, 2] {
                let key = pdb.key(&format!("replay_key_{}", i));
                db.put(pdb.run_id, key, values::int(i * 1000)).unwrap();
            }

            // Transaction
            db.transaction(pdb.run_id, |txn| {
                for i in 50..60 {
                    let key = kv_key(&pdb.ns, &format!("replay_txn_{}", i));
                    txn.put(key, values::int(i))?;
                }
                Ok(())
            })
            .unwrap();
        }

        // First replay (recovery)
        let state_replay_1 = {
            let db = pdb.open();
            // DO NOT write anything - just recover and capture state
            DatabaseStateSnapshot::capture(&db, &pdb.ns)
        };

        // Second replay (recovery again without any writes)
        let state_replay_2 = {
            let db = pdb.open();
            // Again, DO NOT write anything - just recover and capture state
            DatabaseStateSnapshot::capture(&db, &pdb.ns)
        };

        // THE IDEMPOTENCE INVARIANT: replay(replay(wal)) == replay(wal)
        // Both states must be byte-for-byte identical
        assert_eq!(
            state_replay_1.count, state_replay_2.count,
            "Replay idempotence violated: count mismatch ({} vs {})",
            state_replay_1.count, state_replay_2.count
        );
        assert_eq!(
            state_replay_1.checksum, state_replay_2.checksum,
            "Replay idempotence violated: checksum mismatch"
        );
        invariants::assert_recovery_preserves_state(&state_replay_1, &state_replay_2);
    }

    /// Opening database twice without changes produces identical state
    #[test]
    fn test_double_recovery_same_state() {
        let pdb = PersistentTestDb::new();

        // Write initial state
        {
            let db = pdb.open();
            for i in 0..50 {
                let key = pdb.key(&format!("idem_{}", i));
                db.put(pdb.run_id, key, values::int(i)).unwrap();
            }
        }

        // First recovery
        let state_after_first = {
            let db = pdb.open();
            DatabaseStateSnapshot::capture(&db, &pdb.ns)
        };

        // Second recovery (no changes between)
        let state_after_second = {
            let db = pdb.open();
            DatabaseStateSnapshot::capture(&db, &pdb.ns)
        };

        // Must be identical
        invariants::assert_recovery_preserves_state(&state_after_first, &state_after_second);
    }

    /// Multiple recovery cycles produce identical state
    #[test]
    fn test_multiple_recovery_cycles_identical() {
        let pdb = PersistentTestDb::new();

        // Write state once
        {
            let db = pdb.open();
            for i in 0..100 {
                let key = pdb.key(&format!("multi_idem_{}", i));
                db.put(pdb.run_id, key, values::int(i * i)).unwrap();
            }
        }

        // Capture reference state
        let reference_state = {
            let db = pdb.open();
            DatabaseStateSnapshot::capture(&db, &pdb.ns)
        };

        // Multiple recovery cycles
        for cycle in 0..5 {
            let state = {
                let db = pdb.open();
                DatabaseStateSnapshot::capture(&db, &pdb.ns)
            };

            assert!(
                reference_state == state,
                "Recovery cycle {} produced different state",
                cycle
            );
        }
    }

    /// Recovery is idempotent even with complex write patterns
    #[test]
    fn test_idempotence_with_complex_patterns() {
        let pdb = PersistentTestDb::new();

        // Complex write pattern
        {
            let db = pdb.open();

            // Creates
            for i in 0..30 {
                let key = pdb.key(&format!("complex_{}", i));
                db.put(pdb.run_id, key, values::int(i)).unwrap();
            }

            // Deletes
            for i in [5, 10, 15, 20, 25] {
                let key = pdb.key(&format!("complex_{}", i));
                db.delete(pdb.run_id, key).unwrap();
            }

            // Overwrites
            for i in [0, 1, 2, 3, 4] {
                let key = pdb.key(&format!("complex_{}", i));
                db.put(pdb.run_id, key, values::int(i * 1000)).unwrap();
            }

            // Transaction
            db.transaction(pdb.run_id, |txn| {
                for i in 30..40 {
                    let key = kv_key(&pdb.ns, &format!("complex_{}", i));
                    txn.put(key, values::int(i))?;
                }
                Ok(())
            })
            .unwrap();
        }

        let state_1 = {
            let db = pdb.open();
            DatabaseStateSnapshot::capture(&db, &pdb.ns)
        };

        let state_2 = {
            let db = pdb.open();
            DatabaseStateSnapshot::capture(&db, &pdb.ns)
        };

        invariants::assert_recovery_preserves_state(&state_1, &state_2);
    }
}

// ============================================================================
// Invariant 5: Prefix Consistency
// Replaying prefix of WAL reconstructs prefix state
// ============================================================================

mod prefix_consistency {
    use super::*;

    /// WAL Prefix Replay Equivalence:
    /// If we commit transactions T1, T2, T3 and capture state after each,
    /// then after recovery the state equals state after T3.
    /// More importantly: replaying up to any commit point N produces
    /// the state that existed at commit N.
    ///
    /// We can only test this indirectly since we can't truncate the WAL,
    /// but we verify that checkpoints produce consistent states.
    #[test]
    fn test_prefix_states_are_reconstructible() {
        let pdb = PersistentTestDb::new();

        // Capture state at each "checkpoint" (after each batch of operations)
        let mut checkpoint_states: Vec<DatabaseStateSnapshot> = Vec::new();

        // Checkpoint 0: Empty state
        {
            let db = pdb.open();
            checkpoint_states.push(DatabaseStateSnapshot::capture(&db, &pdb.ns));
        }

        // Checkpoint 1: After first batch
        {
            let db = pdb.open();
            for i in 0..10 {
                let key = pdb.key(&format!("batch1_{}", i));
                db.put(pdb.run_id, key, values::int(i)).unwrap();
            }
            checkpoint_states.push(DatabaseStateSnapshot::capture(&db, &pdb.ns));
        }

        // Checkpoint 2: After second batch (includes first batch data)
        {
            let db = pdb.open();
            for i in 0..10 {
                let key = pdb.key(&format!("batch2_{}", i));
                db.put(pdb.run_id, key, values::int(i * 10)).unwrap();
            }
            checkpoint_states.push(DatabaseStateSnapshot::capture(&db, &pdb.ns));
        }

        // Checkpoint 3: After third batch with deletes
        {
            let db = pdb.open();
            // Delete some from batch1
            for i in [0, 2, 4] {
                let key = pdb.key(&format!("batch1_{}", i));
                db.delete(pdb.run_id, key).unwrap();
            }
            checkpoint_states.push(DatabaseStateSnapshot::capture(&db, &pdb.ns));
        }

        // Final state after recovery should match last checkpoint
        let final_state = {
            let db = pdb.open();
            DatabaseStateSnapshot::capture(&db, &pdb.ns)
        };

        // THE PREFIX INVARIANT: final state == last checkpoint state
        invariants::assert_recovery_preserves_state(
            checkpoint_states.last().unwrap(),
            &final_state,
        );

        // Additional invariant: checkpoint states should show monotonic growth
        // (or decreases from deletes), but always consistent
        assert_eq!(
            checkpoint_states[0].count, 0,
            "Checkpoint 0 should be empty"
        );
        assert_eq!(
            checkpoint_states[1].count, 10,
            "Checkpoint 1 should have 10 entries"
        );
        assert_eq!(
            checkpoint_states[2].count, 20,
            "Checkpoint 2 should have 20 entries"
        );
        assert_eq!(
            checkpoint_states[3].count, 17,
            "Checkpoint 3 should have 17 entries (20 - 3 deleted)"
        );
    }

    /// State at intermediate point is consistent
    /// (We can only test this indirectly by checking that committed
    /// transactions appear atomically)
    #[test]
    fn test_transactions_appear_atomically() {
        let pdb = PersistentTestDb::new();

        let txn_keys: Vec<Vec<Key>> = (0..5)
            .map(|txn_id| {
                (0..3)
                    .map(|key_id| pdb.key(&format!("txn_{}_key_{}", txn_id, key_id)))
                    .collect()
            })
            .collect();

        {
            let db = pdb.open();

            // Commit transactions one by one
            for (txn_id, keys) in txn_keys.iter().enumerate() {
                db.transaction(pdb.run_id, |txn| {
                    for (key_id, key) in keys.iter().enumerate() {
                        txn.put(key.clone(), values::int((txn_id * 10 + key_id) as i64))?;
                    }
                    Ok(())
                })
                .unwrap();
            }
        }

        // After recovery, each transaction's keys must be all-or-nothing
        {
            let db = pdb.open();

            for keys in &txn_keys {
                invariants::assert_atomic_transaction(&db, keys, true);
            }
        }
    }

    /// Committed state at each checkpoint is recoverable
    #[test]
    fn test_checkpoint_states_consistent() {
        let pdb = PersistentTestDb::new();

        // Build up state in phases, verifying each phase
        for phase in 0..3 {
            // Add phase data
            {
                let db = pdb.open();
                for i in 0..10 {
                    let key = pdb.key(&format!("phase_{}_key_{}", phase, i));
                    db.put(pdb.run_id, key, values::int(phase * 100 + i))
                        .unwrap();
                }
            }

            // Verify phase is complete after recovery
            {
                let db = pdb.open();

                // All keys up to this phase should exist
                for p in 0..=phase {
                    for i in 0..10 {
                        let key = pdb.key(&format!("phase_{}_key_{}", p, i));
                        let val = db.get(&key).unwrap().expect("Key should exist");
                        assert_eq!(val.value, values::int(p * 100 + i));
                    }
                }
            }
        }
    }
}

// ============================================================================
// Additional WAL Invariants
// ============================================================================

// ============================================================================
// Invariant 6: Incomplete Transaction Handling (from §5.5)
// ============================================================================

mod incomplete_transaction_handling {
    use super::*;

    /// §5.5: Incomplete transactions are discarded on recovery
    ///
    /// From spec: "If a crash occurs during commit... Recovery identifies
    /// incomplete transactions by missing CommitTxn... Action: DISCARD all
    /// entries for incomplete transactions"
    ///
    /// Note: We can't directly simulate a crash mid-commit in user-space tests,
    /// but we can verify that aborted transactions leave no trace.
    #[test]
    fn test_aborted_transaction_leaves_no_trace_after_recovery() {
        let pdb = PersistentTestDb::new();

        // Keys for committed and aborted transactions
        let committed_keys: Vec<Key> = (0..5)
            .map(|i| pdb.key(&format!("committed_{}", i)))
            .collect();
        let aborted_keys: Vec<Key> = (0..5).map(|i| pdb.key(&format!("aborted_{}", i))).collect();

        // Capture state before any transactions
        let state_before = {
            let db = pdb.open();

            // First: commit a transaction
            db.transaction(pdb.run_id, |txn| {
                for (i, key) in committed_keys.iter().enumerate() {
                    txn.put(key.clone(), values::int(i as i64))?;
                }
                Ok(())
            })
            .unwrap();

            // Second: abort a transaction (simulates incomplete)
            let _: Result<(), strata_core::error::Error> = db.transaction(pdb.run_id, |txn| {
                for (i, key) in aborted_keys.iter().enumerate() {
                    txn.put(key.clone(), values::int(i as i64))?;
                }
                Err(strata_core::error::Error::InvalidState("abort".to_string()))
            });

            DatabaseStateSnapshot::capture(&db, &pdb.ns)
        };

        // Recovery
        let state_after = {
            let db = pdb.open();
            DatabaseStateSnapshot::capture(&db, &pdb.ns)
        };

        // THE SPEC INVARIANT: Only committed transaction data survives
        invariants::assert_recovery_preserves_state(&state_before, &state_after);

        // Verify committed keys exist
        {
            let db = pdb.open();
            for key in &committed_keys {
                assert!(
                    db.get(key).unwrap().is_some(),
                    "Committed key should survive recovery"
                );
            }

            // Verify aborted keys do NOT exist
            for key in &aborted_keys {
                assert!(
                    db.get(key).unwrap().is_none(),
                    "Aborted transaction key should NOT survive recovery"
                );
            }
        }
    }

    /// Multiple aborted transactions in sequence leave no traces
    #[test]
    fn test_multiple_aborts_no_accumulation() {
        let pdb = PersistentTestDb::new();

        {
            let db = pdb.open();

            // One good transaction
            let good_key = pdb.key("good");
            db.put(pdb.run_id, good_key, values::int(1)).unwrap();

            // Many aborted transactions
            for batch in 0..10 {
                let _: Result<(), strata_core::error::Error> = db.transaction(pdb.run_id, |txn| {
                    for i in 0..10 {
                        let key = kv_key(&pdb.ns, &format!("abort_{}_{}", batch, i));
                        txn.put(key, values::int(999))?;
                    }
                    Err(strata_core::error::Error::InvalidState("abort".to_string()))
                });
            }
        }

        // After recovery: only 1 key should exist
        {
            let db = pdb.open();
            let state = DatabaseStateSnapshot::capture(&db, &pdb.ns);

            assert_eq!(
                state.count, 1,
                "Only 1 committed key should exist, found {}. Aborted txns leaked.",
                state.count
            );
        }
    }
}

mod additional_invariants {
    use super::*;

    /// Deleted keys stay deleted after recovery
    #[test]
    fn test_deletes_are_durable() {
        let pdb = PersistentTestDb::new();
        let key = pdb.key("deleted_key");

        // Create and delete
        {
            let db = pdb.open();
            db.put(pdb.run_id, key.clone(), values::int(42)).unwrap();
            db.delete(pdb.run_id, key.clone()).unwrap();
        }

        // After recovery, key must not exist
        {
            let db = pdb.open();
            assert!(db.get(&key).unwrap().is_none(), "Deleted key reappeared");
        }

        // Even after multiple recoveries
        for _ in 0..3 {
            let db = pdb.open();
            assert!(db.get(&key).unwrap().is_none(), "Deleted key reappeared");
        }
    }

    /// WAL handles empty database correctly
    #[test]
    fn test_empty_database_recovery() {
        let pdb = PersistentTestDb::new();

        // Just open and close without writing
        {
            let _db = pdb.open();
        }

        // Reopen - should work
        {
            let db = pdb.open();
            let state = DatabaseStateSnapshot::capture(&db, &pdb.ns);
            assert_eq!(state.count, 0, "Empty database should have 0 entries");
        }
    }

    /// Version counter persists across restarts
    #[test]
    fn test_version_counter_persistence() {
        let pdb = PersistentTestDb::new();
        let key = pdb.key("version_test");

        let version_before;
        {
            let db = pdb.open();
            db.put(pdb.run_id, key.clone(), values::int(1)).unwrap();
            version_before = db.get(&key).unwrap().unwrap().version;
        }

        // New write after recovery must have higher version
        let version_after;
        {
            let db = pdb.open();
            let key2 = pdb.key("version_test_2");
            db.put(pdb.run_id, key2.clone(), values::int(2)).unwrap();
            version_after = db.get(&key2).unwrap().unwrap().version;
        }

        assert!(
            version_after > version_before,
            "Version counter not persisted: {} should be > {}",
            version_after,
            version_before
        );
    }

    /// All value types survive recovery correctly
    #[test]
    fn test_all_value_types_survive_recovery() {
        let pdb = PersistentTestDb::new();

        let test_cases: Vec<(&str, Value)> = vec![
            ("null", values::null()),
            ("bool", values::bool_val(true)),
            ("int_max", values::int(i64::MAX)),
            ("int_min", values::int(i64::MIN)),
            ("float", values::float(std::f64::consts::PI)),
            ("string", values::string("hello world")),
            ("bytes", values::bytes(&[0, 1, 2, 255])),
            (
                "array",
                values::array(vec![values::int(1), values::string("a")]),
            ),
            ("map", values::map(vec![("key", values::int(42))])),
            ("large", values::large_bytes(10)), // 10KB
        ];

        // Write all
        {
            let db = pdb.open();
            for (name, value) in &test_cases {
                let key = pdb.key(name);
                db.put(pdb.run_id, key, value.clone()).unwrap();
            }
        }

        // Verify after recovery
        {
            let db = pdb.open();
            for (name, expected_value) in &test_cases {
                let key = pdb.key(name);
                let val = db.get(&key).unwrap().expect("Key should exist");
                assert_eq!(
                    &val.value, expected_value,
                    "Value mismatch for type: {}",
                    name
                );
            }
        }
    }
}
