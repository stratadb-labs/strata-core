//! Multi-Threaded OCC Conflict Tests
//!
//! Story #106: Validates optimistic concurrency control behavior
//! with 2-thread scenarios per M2_TRANSACTION_SEMANTICS.md.

use strata_core::types::{Key, Namespace, RunId};
use strata_core::value::Value;
use strata_engine::Database;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Barrier};
use std::thread;
use tempfile::TempDir;

fn create_ns(run_id: RunId) -> Namespace {
    Namespace::new(
        "tenant".to_string(),
        "app".to_string(),
        "agent".to_string(),
        run_id,
    )
}

// ============================================================================
// Read-Write Conflict Tests (Deterministic)
// ============================================================================

/// Test: T1 reads key, T2 writes key before T1 commits -> T1 should abort
/// Uses manual transaction control for precise ordering.
#[test]
fn test_concurrent_read_write_conflict() {
    let temp_dir = TempDir::new().unwrap();
    let db = Database::open(temp_dir.path().join("db")).unwrap();

    let run_id = RunId::new();
    let ns = create_ns(run_id);
    let key = Key::new_kv(ns.clone(), "contested_key");

    // Pre-populate
    db.put(run_id, key.clone(), Value::I64(100)).unwrap();

    // Phase 1: T1 begins and reads (but doesn't commit yet)
    let mut txn1 = db.begin_transaction(run_id);
    let _read_value = txn1.get(&key).unwrap(); // Adds to read_set

    // Phase 2: T2 modifies the same key and commits
    db.put(run_id, key.clone(), Value::I64(200)).unwrap();

    // Phase 3: T1 tries to write something and commit
    let other_key = Key::new_kv(ns, "other");
    txn1.put(other_key, Value::I64(999)).unwrap();

    // T1's commit should fail due to read-write conflict
    let result = db.commit_transaction(&mut txn1);

    assert!(
        result.is_err(),
        "T1 should abort due to read-write conflict"
    );

    // Final value should be T2's write
    let final_val = db.get(&key).unwrap().unwrap();
    assert_eq!(final_val.value, Value::I64(200));
}

// ============================================================================
// Write-Write Conflict Tests (Deterministic)
// ============================================================================

/// Test: T1 and T2 both read then write same key -> second to commit aborts
#[test]
fn test_concurrent_write_write_conflict_with_read() {
    let temp_dir = TempDir::new().unwrap();
    let db = Database::open(temp_dir.path().join("db")).unwrap();

    let run_id = RunId::new();
    let ns = create_ns(run_id);
    let key = Key::new_kv(ns, "ww_key");

    // Pre-populate
    db.put(run_id, key.clone(), Value::I64(0)).unwrap();

    // T1: Begin and read (but don't commit yet)
    let mut txn1 = db.begin_transaction(run_id);
    let _val1 = txn1.get(&key).unwrap();
    txn1.put(key.clone(), Value::I64(1)).unwrap();

    // T2: Begin, read, write, and commit FIRST
    let mut txn2 = db.begin_transaction(run_id);
    let _val2 = txn2.get(&key).unwrap();
    txn2.put(key.clone(), Value::I64(2)).unwrap();

    // T2 commits first
    let result2 = db.commit_transaction(&mut txn2);
    assert!(result2.is_ok(), "T2 (first committer) should succeed");

    // T1 tries to commit second - should fail due to conflict
    let result1 = db.commit_transaction(&mut txn1);
    assert!(
        result1.is_err(),
        "T1 should abort due to read-write conflict"
    );

    // Verify T2's value persisted
    let final_val = db.get(&key).unwrap().unwrap();
    assert_eq!(final_val.value, Value::I64(2));
}

/// Test: Blind writes (no read) - both can commit, last write wins
#[test]
fn test_blind_writes_no_conflict() {
    let temp_dir = TempDir::new().unwrap();
    let db = Arc::new(Database::open(temp_dir.path().join("db")).unwrap());

    let run_id = RunId::new();
    let ns = create_ns(run_id);
    let key = Key::new_kv(ns, "blind_key");

    let db1 = Arc::clone(&db);
    let db2 = Arc::clone(&db);
    let key1 = key.clone();
    let key2 = key.clone();

    let barrier = Arc::new(Barrier::new(2));
    let barrier1 = Arc::clone(&barrier);
    let barrier2 = Arc::clone(&barrier);

    // T1: Blind write (no read first)
    let h1 = thread::spawn(move || {
        db1.transaction(run_id, |txn| {
            barrier1.wait();
            txn.put(key1.clone(), Value::I64(1))?;
            Ok(())
        })
    });

    // T2: Blind write (no read first)
    let h2 = thread::spawn(move || {
        db2.transaction(run_id, |txn| {
            barrier2.wait();
            txn.put(key2.clone(), Value::I64(2))?;
            Ok(())
        })
    });

    let r1 = h1.join().unwrap();
    let r2 = h2.join().unwrap();

    // Both should succeed (blind writes don't conflict per M2 spec)
    assert!(r1.is_ok(), "Blind write T1 should succeed");
    assert!(r2.is_ok(), "Blind write T2 should succeed");
}

// ============================================================================
// CAS Conflict Tests (Deterministic)
// ============================================================================

/// Test: T1 and T2 both CAS same key -> second one aborts
#[test]
fn test_concurrent_cas_conflict() {
    let temp_dir = TempDir::new().unwrap();
    let db = Database::open(temp_dir.path().join("db")).unwrap();

    let run_id = RunId::new();
    let ns = create_ns(run_id);
    let key = Key::new_kv(ns, "cas_key");

    // Pre-populate with known version
    db.put(run_id, key.clone(), Value::I64(0)).unwrap();
    let initial_version = db.get(&key).unwrap().unwrap().version.as_u64();

    // T1: Begin and CAS
    let mut txn1 = db.begin_transaction(run_id);
    txn1.cas(key.clone(), initial_version, Value::I64(1))
        .unwrap();

    // T2: Begin and CAS with same version
    let mut txn2 = db.begin_transaction(run_id);
    txn2.cas(key.clone(), initial_version, Value::I64(2))
        .unwrap();

    // T1 commits first
    let result1 = db.commit_transaction(&mut txn1);
    assert!(result1.is_ok(), "T1 (first CAS) should succeed");

    // T2 tries to commit - should fail due to version mismatch
    let result2 = db.commit_transaction(&mut txn2);
    assert!(result2.is_err(), "T2 should abort due to CAS conflict");

    // Verify T1's value persisted
    let final_val = db.get(&key).unwrap().unwrap();
    assert_eq!(final_val.value, Value::I64(1));
}

// ============================================================================
// No Conflict Tests
// ============================================================================

/// Test: T1 and T2 write different keys -> both commit
#[test]
fn test_no_conflict_different_keys() {
    let temp_dir = TempDir::new().unwrap();
    let db = Arc::new(Database::open(temp_dir.path().join("db")).unwrap());

    let run_id = RunId::new();
    let ns = create_ns(run_id);
    let key_a = Key::new_kv(ns.clone(), "key_a");
    let key_b = Key::new_kv(ns, "key_b");

    let db1 = Arc::clone(&db);
    let db2 = Arc::clone(&db);

    let barrier = Arc::new(Barrier::new(2));
    let barrier1 = Arc::clone(&barrier);
    let barrier2 = Arc::clone(&barrier);

    // T1: Write key_a
    let h1 = thread::spawn(move || {
        db1.transaction(run_id, |txn| {
            barrier1.wait();
            txn.put(key_a.clone(), Value::I64(1))?;
            Ok(())
        })
    });

    // T2: Write key_b
    let h2 = thread::spawn(move || {
        db2.transaction(run_id, |txn| {
            barrier2.wait();
            txn.put(key_b.clone(), Value::I64(2))?;
            Ok(())
        })
    });

    let r1 = h1.join().unwrap();
    let r2 = h2.join().unwrap();

    // Both should succeed
    assert!(r1.is_ok(), "T1 should succeed");
    assert!(r2.is_ok(), "T2 should succeed");
}

// ============================================================================
// First-Committer-Wins Verification (Deterministic)
// ============================================================================

/// Test: Verify first-committer-wins semantics with controlled ordering
#[test]
fn test_first_committer_wins() {
    let temp_dir = TempDir::new().unwrap();
    let db = Database::open(temp_dir.path().join("db")).unwrap();

    let run_id = RunId::new();
    let ns = create_ns(run_id);
    let key = Key::new_kv(ns, "fcw_key");

    // Pre-populate
    db.put(run_id, key.clone(), Value::I64(0)).unwrap();

    // Run 10 rounds, each round has two transactions competing
    for round in 0..10 {
        // Both transactions begin and read the same key
        let mut txn1 = db.begin_transaction(run_id);
        let mut txn2 = db.begin_transaction(run_id);

        let _val1 = txn1.get(&key).unwrap();
        let _val2 = txn2.get(&key).unwrap();

        txn1.put(key.clone(), Value::I64(round * 2)).unwrap();
        txn2.put(key.clone(), Value::I64(round * 2 + 1)).unwrap();

        // T1 commits first, T2 commits second
        let r1 = db.commit_transaction(&mut txn1);
        let r2 = db.commit_transaction(&mut txn2);

        // Exactly one should succeed (first-committer-wins)
        let successes = r1.is_ok() as i32 + r2.is_ok() as i32;
        assert_eq!(
            successes, 1,
            "Round {}: Exactly one transaction should commit, got {}",
            round, successes
        );
    }
}

// ============================================================================
// Multi-threaded Contention Tests
// ============================================================================

/// Test: Multi-threaded contention - validates database handles concurrent access
#[test]
fn test_multi_threaded_contention() {
    let temp_dir = TempDir::new().unwrap();
    let db = Arc::new(Database::open(temp_dir.path().join("db")).unwrap());

    let run_id = RunId::new();
    let ns = create_ns(run_id);

    let num_threads = 4;
    let ops_per_thread = 10;
    let completed_ops = Arc::new(AtomicU64::new(0));

    // Each thread writes to its own key (no contention)
    let handles: Vec<_> = (0..num_threads)
        .map(|thread_id| {
            let db = Arc::clone(&db);
            let ns = ns.clone();
            let completed = Arc::clone(&completed_ops);

            thread::spawn(move || {
                for op in 0..ops_per_thread {
                    let key = Key::new_kv(ns.clone(), format!("t{}_op{}", thread_id, op));
                    let result = db.transaction(run_id, |txn| {
                        txn.put(key.clone(), Value::I64((thread_id * 100 + op) as i64))?;
                        Ok(())
                    });

                    if result.is_ok() {
                        completed.fetch_add(1, Ordering::Relaxed);
                    }
                }
            })
        })
        .collect();

    for h in handles {
        h.join().unwrap();
    }

    let completed = completed_ops.load(Ordering::Relaxed);

    // All operations on disjoint keys should succeed
    assert_eq!(
        completed,
        (num_threads * ops_per_thread) as u64,
        "All operations should complete successfully"
    );

    // Verify some values are readable
    let key = Key::new_kv(ns.clone(), "t0_op0");
    let val = db.get(&key).unwrap().unwrap();
    assert_eq!(val.value, Value::I64(0));
}

/// Test: Read-only transactions never conflict
#[test]
fn test_read_only_transactions_no_conflict() {
    let temp_dir = TempDir::new().unwrap();
    let db = Arc::new(Database::open(temp_dir.path().join("db")).unwrap());

    let run_id = RunId::new();
    let ns = create_ns(run_id);

    // Pre-populate with data
    for i in 0..10 {
        let key = Key::new_kv(ns.clone(), format!("key_{}", i));
        db.put(run_id, key, Value::I64(i)).unwrap();
    }

    // Run multiple read-only transactions concurrently
    let num_threads = 10;
    let success_count = Arc::new(AtomicU64::new(0));

    let handles: Vec<_> = (0..num_threads)
        .map(|_| {
            let db = Arc::clone(&db);
            let ns = ns.clone();
            let success = Arc::clone(&success_count);

            thread::spawn(move || {
                let result = db.transaction(run_id, |txn| {
                    // Just read, no writes
                    for i in 0..10 {
                        let key = Key::new_kv(ns.clone(), format!("key_{}", i));
                        let _val = txn.get(&key)?;
                    }
                    Ok(())
                });

                if result.is_ok() {
                    success.fetch_add(1, Ordering::Relaxed);
                }
            })
        })
        .collect();

    for h in handles {
        h.join().unwrap();
    }

    // All read-only transactions should succeed
    assert_eq!(
        success_count.load(Ordering::Relaxed),
        num_threads as u64,
        "All read-only transactions should succeed"
    );
}

/// Test: Concurrent transactions on different keys succeed
#[test]
fn test_concurrent_disjoint_transactions() {
    let temp_dir = TempDir::new().unwrap();
    let db = Arc::new(Database::open(temp_dir.path().join("db")).unwrap());

    let run_id = RunId::new();
    let ns = create_ns(run_id);

    let num_threads = 8;
    let success_count = Arc::new(AtomicU64::new(0));

    let handles: Vec<_> = (0..num_threads)
        .map(|thread_id| {
            let db = Arc::clone(&db);
            let ns = ns.clone();
            let success = Arc::clone(&success_count);

            thread::spawn(move || {
                // Each thread works on its own set of keys
                let result = db.transaction(run_id, |txn| {
                    for i in 0..5 {
                        let key = Key::new_kv(ns.clone(), format!("t{}_{}", thread_id, i));
                        txn.put(key, Value::I64((thread_id * 10 + i) as i64))?;
                    }
                    Ok(())
                });

                if result.is_ok() {
                    success.fetch_add(1, Ordering::Relaxed);
                }
            })
        })
        .collect();

    for h in handles {
        h.join().unwrap();
    }

    // All transactions should succeed (no conflicts on disjoint keys)
    assert_eq!(
        success_count.load(Ordering::Relaxed),
        num_threads as u64,
        "All disjoint transactions should succeed"
    );
}
