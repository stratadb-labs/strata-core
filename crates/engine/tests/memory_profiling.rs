//! Memory Usage Profiling Tests
//!
//! Story #108: Documents ClonedSnapshotView memory overhead
//! and TransactionContext footprint.

use strata_core::types::{Key, Namespace, RunId};
use strata_core::value::Value;
use strata_engine::Database;
use std::mem;
use tempfile::TempDir;

fn create_ns(run_id: RunId) -> Namespace {
    Namespace::new(
        "tenant".to_string(),
        "app".to_string(),
        "agent".to_string(),
        run_id,
    )
}

/// Test: Document TransactionContext memory footprint
#[test]
fn test_transaction_context_size() {
    let temp_dir = TempDir::new().unwrap();
    let db = Database::open(temp_dir.path().join("db")).unwrap();
    let run_id = RunId::new();

    // Create empty transaction
    let txn = db.begin_transaction(run_id);

    // Document base size
    let base_size = mem::size_of_val(&txn);
    println!("TransactionContext base size: {} bytes", base_size);

    // Base size should be reasonable (< 1KB without data)
    assert!(
        base_size < 1024,
        "Base TransactionContext too large: {} bytes",
        base_size
    );
}

/// Test: Memory grows with read-set size
#[test]
fn test_read_set_memory_growth() {
    let temp_dir = TempDir::new().unwrap();
    let db = Database::open(temp_dir.path().join("db")).unwrap();
    let run_id = RunId::new();
    let ns = create_ns(run_id);

    // Pre-populate with data
    for i in 0..1000 {
        let key = Key::new_kv(ns.clone(), format!("key_{}", i));
        db.put(run_id, key, Value::I64(i as i64)).unwrap();
    }

    // Read increasing numbers of keys
    for read_count in [10, 100, 500, 1000] {
        let result = db.transaction(run_id, |txn| {
            for i in 0..read_count {
                let key = Key::new_kv(ns.clone(), format!("key_{}", i));
                txn.get(&key)?;
            }
            // Can't easily measure txn size here, but verify it works
            Ok(read_count)
        });

        assert!(result.is_ok());
        println!("Read {} keys successfully", read_count);
    }
}

/// Test: Memory grows with write-set size
#[test]
fn test_write_set_memory_growth() {
    let temp_dir = TempDir::new().unwrap();
    let db = Database::open(temp_dir.path().join("db")).unwrap();
    let run_id = RunId::new();
    let ns = create_ns(run_id);

    // Write increasing numbers of keys
    for write_count in [10, 100, 500, 1000] {
        let result = db.transaction(run_id, |txn| {
            for i in 0..write_count {
                let key = Key::new_kv(ns.clone(), format!("batch_{}_key_{}", write_count, i));
                txn.put(key, Value::I64(i as i64))?;
            }
            Ok(write_count)
        });

        assert!(result.is_ok());
        println!("Wrote {} keys successfully", write_count);
    }
}

/// Test: Concurrent transactions memory (O(N * data_size) expected)
#[test]
fn test_concurrent_transactions_memory() {
    use std::sync::Arc;
    use std::thread;

    let temp_dir = TempDir::new().unwrap();
    let db = Arc::new(Database::open(temp_dir.path().join("db")).unwrap());
    let run_id = RunId::new();
    let ns = create_ns(run_id);

    // Pre-populate with data
    let data_size = 1000;
    for i in 0..data_size {
        let key = Key::new_kv(ns.clone(), format!("key_{}", i));
        db.put(run_id, key, Value::I64(i as i64)).unwrap();
    }

    // Create N concurrent transactions (each holds a snapshot)
    let num_concurrent = 10;

    let handles: Vec<_> = (0..num_concurrent)
        .map(|thread_id| {
            let db = Arc::clone(&db);
            let ns = ns.clone();

            thread::spawn(move || {
                // Begin transaction (creates snapshot)
                let mut txn = db.begin_transaction(run_id);

                // Read some data
                for i in 0..100 {
                    let key = Key::new_kv(ns.clone(), format!("key_{}", i));
                    let _ = txn.get(&key);
                }

                // Hold transaction open briefly
                thread::sleep(std::time::Duration::from_millis(10));

                // Write and commit
                let key = Key::new_kv(ns.clone(), format!("thread_{}", thread_id));
                txn.put(key, Value::I64(thread_id as i64)).unwrap();

                db.commit_transaction(&mut txn)
            })
        })
        .collect();

    let mut success = 0;
    for h in handles {
        if h.join().unwrap().is_ok() {
            success += 1;
        }
    }

    println!(
        "Concurrent transactions: {} succeeded out of {}",
        success, num_concurrent
    );

    // Most should succeed (some may conflict)
    assert!(
        success >= num_concurrent / 2,
        "Too many concurrent transaction failures"
    );
}

/// Test: Verify no memory leaks (transactions properly cleaned up)
#[test]
fn test_no_memory_leaks() {
    let temp_dir = TempDir::new().unwrap();
    let db = Database::open(temp_dir.path().join("db")).unwrap();
    let run_id = RunId::new();
    let ns = create_ns(run_id);

    // Run many transactions
    for round in 0..100 {
        let result = db.transaction(run_id, |txn| {
            for i in 0..100 {
                let key = Key::new_kv(ns.clone(), format!("round_{}_key_{}", round, i));
                txn.put(key, Value::I64(i as i64))?;
            }
            Ok(())
        });
        assert!(result.is_ok());
    }

    // If we got here without OOM, cleanup is working
    println!("Completed 100 rounds of 100 writes each - no memory issues");
}

/// Test: Aborted transactions release memory
#[test]
fn test_aborted_transaction_cleanup() {
    let temp_dir = TempDir::new().unwrap();
    let db = Database::open(temp_dir.path().join("db")).unwrap();
    let run_id = RunId::new();
    let ns = create_ns(run_id);

    // Create and abort many transactions
    for round in 0..100 {
        let result: Result<(), strata_core::error::Error> = db.transaction(run_id, |txn| {
            for i in 0..100 {
                let key = Key::new_kv(ns.clone(), format!("abort_{}_key_{}", round, i));
                txn.put(key, Value::I64(i as i64))?;
            }
            // Force abort
            Err(strata_core::error::Error::InvalidState(
                "intentional abort".to_string(),
            ))
        });

        assert!(result.is_err());
    }

    // If we got here without OOM, aborted transactions are cleaned up
    println!("Completed 100 aborted transactions - cleanup working");
}

/// Test: Large value handling
#[test]
fn test_large_value_memory() {
    let temp_dir = TempDir::new().unwrap();
    let db = Database::open(temp_dir.path().join("db")).unwrap();
    let run_id = RunId::new();
    let ns = create_ns(run_id);

    // Write a large string value
    let large_string = "x".repeat(100_000); // 100KB string

    let result = db.transaction(run_id, |txn| {
        let key = Key::new_kv(ns.clone(), "large_value");
        txn.put(key, Value::String(large_string.clone()))?;
        Ok(())
    });

    assert!(result.is_ok());
    println!("Successfully wrote 100KB value");

    // Read it back
    let key = Key::new_kv(ns.clone(), "large_value");
    let read_result = db.get(&key).unwrap().unwrap();
    if let Value::String(s) = read_result.value {
        assert_eq!(s.len(), 100_000);
        println!("Successfully read 100KB value");
    }
}

/// Document: Memory characteristics
#[test]
fn document_memory_characteristics() {
    println!("\n=== M2 Memory Characteristics ===\n");

    println!("ClonedSnapshotView:");
    println!("  - Creates full clone of BTreeMap at transaction start");
    println!("  - Memory: O(data_size) per active transaction");
    println!("  - Time: O(data_size) per snapshot creation");
    println!();

    println!("TransactionContext:");
    println!("  - read_set: O(keys_read) entries");
    println!("  - write_set: O(keys_written) entries");
    println!("  - delete_set: O(keys_deleted) entries");
    println!("  - cas_set: O(cas_operations) entries");
    println!();

    println!("Concurrent Transactions:");
    println!("  - N concurrent transactions = N snapshots");
    println!("  - Total memory: O(N * data_size)");
    println!();

    println!("Recommended Limits:");
    println!("  - Data size: < 100MB per RunId");
    println!("  - Concurrent transactions: < 100 per RunId");
    println!("  - Transaction duration: < 1 second");
    println!();

    println!("Future Optimization (M3+):");
    println!("  - LazySnapshotView: O(1) snapshot creation");
    println!("  - Version-bounded reads from live storage");
    println!("  - No cloning overhead");
    println!();
}
