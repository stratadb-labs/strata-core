//! Tier 9: Stress & Scale Tests
//!
//! Stress tests for large WAL, concurrent operations.
//! These tests are marked with #[ignore] for opt-in execution.

use crate::test_utils::*;
use strata_core::types::RunId;
use strata_core::value::Value;
use strata_primitives::KVStore;
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

/// Large WAL recovery test
#[test]
#[ignore]
fn stress_large_wal_recovery() {
    let mut test_db = TestDb::new();
    let run_id = test_db.run_id;

    let kv = test_db.kv();

    // Write large amount of data
    for i in 0..10_000 {
        kv.put(&run_id, &format!("key_{}", i), Value::I64(i))
            .unwrap();
    }

    let start = Instant::now();
    test_db.reopen();
    let recovery_time = start.elapsed();

    println!("Large WAL recovery took: {:?}", recovery_time);

    // Verify data
    let kv = test_db.kv();
    for i in 0..10_000 {
        let value = kv.get(&run_id, &format!("key_{}", i)).unwrap();
        assert!(value.is_some(), "Key {} missing after recovery", i);
    }
}

/// Concurrent writes stress test
#[test]
#[ignore]
fn stress_concurrent_writes() {
    let test_db = TestDb::new_in_memory();
    let db = test_db.db.clone();

    let handles: Vec<_> = (0..10)
        .map(|thread_id| {
            let db = db.clone();
            thread::spawn(move || {
                let run_id = RunId::new();
                let kv = KVStore::new(db);

                for i in 0..1000 {
                    kv.put(&run_id, &format!("t{}_k{}", thread_id, i), Value::I64(i))
                        .unwrap();
                }
            })
        })
        .collect();

    for handle in handles {
        handle.join().unwrap();
    }
}

/// Concurrent reads stress test
#[test]
#[ignore]
fn stress_concurrent_reads() {
    let test_db = TestDb::new_in_memory();
    let run_id = test_db.run_id;

    let kv = test_db.kv();

    // Populate data
    for i in 0..1000 {
        kv.put(&run_id, &format!("key_{}", i), Value::I64(i))
            .unwrap();
    }

    let db = test_db.db.clone();

    let handles: Vec<_> = (0..20)
        .map(|_| {
            let db = db.clone();
            thread::spawn(move || {
                let kv = KVStore::new(db);
                for _ in 0..1000 {
                    for i in 0..1000 {
                        let _ = kv.get(&run_id, &format!("key_{}", i));
                    }
                }
            })
        })
        .collect();

    for handle in handles {
        handle.join().unwrap();
    }
}

/// Many small writes
#[test]
#[ignore]
fn stress_many_small_writes() {
    let test_db = TestDb::new_in_memory();
    let run_id = test_db.run_id;

    let kv = test_db.kv();

    let start = Instant::now();
    for i in 0..100_000 {
        kv.put(&run_id, &format!("k{}", i), Value::I64(i)).unwrap();
    }
    let write_time = start.elapsed();

    println!("100K writes took: {:?}", write_time);
    println!(
        "Rate: {:.0} writes/sec",
        100_000.0 / write_time.as_secs_f64()
    );
}

/// Large values stress
#[test]
#[ignore]
fn stress_large_values() {
    let test_db = TestDb::new_in_memory();
    let run_id = test_db.run_id;

    let kv = test_db.kv();

    let large_value = "x".repeat(1_000_000); // 1MB

    for i in 0..100 {
        kv.put(
            &run_id,
            &format!("large_{}", i),
            Value::String(large_value.clone()),
        )
        .unwrap();
    }

    // Verify
    for i in 0..100 {
        let value = kv.get(&run_id, &format!("large_{}", i)).unwrap();
        assert!(value.is_some());
    }
}

/// Many runs stress
#[test]
#[ignore]
fn stress_many_runs() {
    let test_db = TestDb::new_in_memory();

    let kv = test_db.kv();

    // Create many runs
    for _ in 0..1000 {
        let run_id = RunId::new();
        for i in 0..10 {
            kv.put(&run_id, &format!("k{}", i), Value::I64(i)).unwrap();
        }
    }
}

/// Mixed operations stress
#[test]
#[ignore]
fn stress_mixed_operations() {
    let test_db = TestDb::new_in_memory();
    let run_id = test_db.run_id;

    let kv = test_db.kv();

    for i in 0..10_000 {
        match i % 3 {
            0 => {
                kv.put(&run_id, &format!("k{}", i % 1000), Value::I64(i))
                    .unwrap();
            }
            1 => {
                let _ = kv.get(&run_id, &format!("k{}", i % 1000));
            }
            2 => {
                kv.delete(&run_id, &format!("k{}", (i + 500) % 1000)).ok();
            }
            _ => {}
        }
    }
}

/// Sustained load over time
#[test]
#[ignore]
fn stress_sustained_load() {
    let test_db = TestDb::new_in_memory();
    let run_id = test_db.run_id;

    let kv = test_db.kv();

    let duration = Duration::from_secs(10);
    let start = Instant::now();
    let mut count = 0;

    while start.elapsed() < duration {
        kv.put(&run_id, &format!("k{}", count), Value::I64(count as i64))
            .unwrap();
        count += 1;
    }

    println!("Sustained load: {} operations in {:?}", count, duration);
    println!("Rate: {:.0} ops/sec", count as f64 / duration.as_secs_f64());
}

/// Recovery after large churn
#[test]
#[ignore]
fn stress_recovery_after_churn() {
    let mut test_db = TestDb::new();
    let run_id = test_db.run_id;

    let kv = test_db.kv();

    // Create lots of churn
    for i in 0..10_000 {
        kv.put(&run_id, &format!("churn_{}", i % 100), Value::I64(i))
            .unwrap();
    }

    let state_before = CapturedState::capture(&test_db.db, &run_id);

    test_db.reopen();

    let state_after = CapturedState::capture(&test_db.db, &run_id);

    assert_states_equal(&state_before, &state_after, "Churn recovery mismatch");
}

/// Concurrent crash simulation
#[test]
#[ignore]
fn stress_concurrent_crash_simulation() {
    for iteration in 0..10 {
        let mut test_db = TestDb::new();
        let run_id = test_db.run_id;

        let kv = test_db.kv();

        for i in 0..1000 {
            kv.put(&run_id, &format!("k{}", i), Value::I64(i)).unwrap();
        }

        test_db.reopen();

        let kv = test_db.kv();
        let present = (0..1000)
            .filter(|i| kv.get(&run_id, &format!("k{}", i)).unwrap().is_some())
            .count();

        println!("Iteration {}: {}/1000 keys recovered", iteration, present);
    }
}
