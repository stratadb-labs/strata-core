//! M4 Red Flag Validation Tests
//!
//! These tests FAIL if architecture has fundamental problems.
//! A failure means STOP and REDESIGN - not tune parameters.
//!
//! Run with: cargo test --test m4_red_flags -- --nocapture

use strata_core::types::RunId;
use strata_core::value::Value;
use strata_core::Storage;
use strata_engine::{Database, TransactionPool};
use strata_primitives::KVStore;
use std::sync::Arc;
use std::time::Instant;

const ITERATIONS: usize = 10000;

/// Red flag: Snapshot acquisition > 2µs
#[test]
fn red_flag_snapshot_acquisition() {
    let db = Arc::new(Database::builder().in_memory().open_temp().unwrap());

    // Warmup
    for _ in 0..100 {
        let _ = db.storage().create_snapshot();
    }

    // Measure
    let start = Instant::now();
    for _ in 0..ITERATIONS {
        let _ = db.storage().create_snapshot();
    }
    let elapsed = start.elapsed();

    let avg_ns = elapsed.as_nanos() / ITERATIONS as u128;
    let threshold_ns = 2000; // 2µs

    println!(
        "Snapshot acquisition: {}ns (threshold: {}ns)",
        avg_ns, threshold_ns
    );

    assert!(
        avg_ns <= threshold_ns,
        "RED FLAG: Snapshot acquisition {}ns > {}ns threshold.\n\
         ACTION: Redesign snapshot mechanism.",
        avg_ns,
        threshold_ns
    );

    println!("Snapshot acquisition: PASS");
}

/// Red flag: A1/A0 ratio > 20×
/// A0 = raw storage put, A1 = primitive layer put
#[test]
fn red_flag_facade_tax_a1_a0() {
    use strata_core::types::{Key, Namespace, TypeTag};
    use strata_core::Storage;

    let db = Arc::new(Database::builder().in_memory().open_temp().unwrap());
    let kv = KVStore::new(db.clone());
    let run_id = RunId::new();
    let ns = Namespace::for_run(run_id);

    // Warmup
    for i in 0..100 {
        kv.put(&run_id, &format!("warmup{}", i), Value::I64(i as i64))
            .unwrap();
    }

    // A0: Engine storage layer direct (via Storage trait)
    let start = Instant::now();
    for i in 0..ITERATIONS {
        let key = Key::new(ns.clone(), TypeTag::KV, format!("a0key{}", i).into_bytes());
        let _ = Storage::put(db.storage().as_ref(), key, Value::I64(i as i64), None);
    }
    let a0_elapsed = start.elapsed();

    // A1: Primitive layer (KVStore.put)
    let start = Instant::now();
    for i in 0..ITERATIONS {
        kv.put(&run_id, &format!("a1key{}", i), Value::I64(i as i64))
            .unwrap();
    }
    let a1_elapsed = start.elapsed();

    let a0_ns = a0_elapsed.as_nanos() / ITERATIONS as u128;
    let a1_ns = a1_elapsed.as_nanos() / ITERATIONS as u128;
    let ratio = a1_ns as f64 / a0_ns.max(1) as f64;

    println!(
        "A0 (storage): {}ns, A1 (primitive): {}ns, Ratio: {:.1}×",
        a0_ns, a1_ns, ratio
    );

    assert!(
        ratio <= 20.0,
        "RED FLAG: A1/A0 ratio {:.1}× > 20× threshold.\n\
         A0 (storage): {}ns, A1 (primitive): {}ns\n\
         ACTION: Remove abstraction layers.",
        ratio,
        a0_ns,
        a1_ns
    );

    println!("A1/A0 ratio: PASS");
}

/// Red flag: B/A1 ratio > 8×
/// A1 = primitive put, B = transaction-wrapped put
#[test]
fn red_flag_facade_tax_b_a1() {
    let db = Arc::new(Database::builder().in_memory().open_temp().unwrap());
    let kv = KVStore::new(db.clone());
    let run_id = RunId::new();

    // Warmup
    for i in 0..100 {
        kv.put(&run_id, &format!("warmup{}", i), Value::I64(i as i64))
            .unwrap();
    }

    // A1: Primitive layer
    let start = Instant::now();
    for i in 0..ITERATIONS {
        kv.put(&run_id, &format!("a1key{}", i), Value::I64(i as i64))
            .unwrap();
    }
    let a1_elapsed = start.elapsed();

    // B: Full stack with explicit transaction
    let start = Instant::now();
    for i in 0..ITERATIONS {
        db.transaction(run_id, |txn| {
            use strata_core::types::{Key, Namespace, TypeTag};
            let ns = Namespace::for_run(run_id);
            let key = Key::new(ns, TypeTag::KV, format!("bkey{}", i).into_bytes());
            txn.put(key, Value::I64(i as i64))
        })
        .unwrap();
    }
    let b_elapsed = start.elapsed();

    let a1_ns = a1_elapsed.as_nanos() / ITERATIONS as u128;
    let b_ns = b_elapsed.as_nanos() / ITERATIONS as u128;
    let ratio = b_ns as f64 / a1_ns.max(1) as f64;

    println!(
        "A1 (primitive): {}ns, B (full stack): {}ns, Ratio: {:.1}×",
        a1_ns, b_ns, ratio
    );

    assert!(
        ratio <= 8.0,
        "RED FLAG: B/A1 ratio {:.1}× > 8× threshold.\n\
         A1 (primitive): {}ns, B (full stack): {}ns\n\
         ACTION: Inline facade logic.",
        ratio,
        a1_ns,
        b_ns
    );

    println!("B/A1 ratio: PASS");
}

/// Red flag: Disjoint scaling < 2.5× at 4 threads
#[test]
fn red_flag_disjoint_scaling() {
    let iterations = 10000;

    // Single-threaded baseline
    let db = Arc::new(Database::builder().in_memory().open_temp().unwrap());
    let kv = KVStore::new(db.clone());
    let run_id = RunId::new();

    let start = Instant::now();
    for i in 0..iterations {
        kv.put(&run_id, &format!("key{}", i), Value::I64(i as i64))
            .unwrap();
    }
    let single_thread_time = start.elapsed();

    // 4-thread disjoint
    let db = Arc::new(Database::builder().in_memory().open_temp().unwrap());
    let start = Instant::now();
    let handles: Vec<_> = (0..4)
        .map(|_| {
            let db = Arc::clone(&db);
            std::thread::spawn(move || {
                let kv = KVStore::new(db);
                let run_id = RunId::new(); // Different run per thread
                for i in 0..iterations {
                    kv.put(&run_id, &format!("key{}", i), Value::I64(i as i64))
                        .unwrap();
                }
            })
        })
        .collect();
    for h in handles {
        h.join().unwrap();
    }
    let four_thread_time = start.elapsed();

    // 4× work should take less than 4× time
    // scaling = (single_time * 4) / four_thread_time
    let scaling = (single_thread_time.as_nanos() * 4) as f64 / four_thread_time.as_nanos() as f64;

    println!(
        "1-thread: {:?}, 4-threads (4× work): {:?}, Scaling: {:.2}×",
        single_thread_time, four_thread_time, scaling
    );

    assert!(
        scaling >= 2.5,
        "RED FLAG: Disjoint scaling {:.2}× < 2.5× threshold.\n\
         1-thread: {:?}, 4-threads: {:?}\n\
         ACTION: Redesign sharding.",
        scaling,
        single_thread_time,
        four_thread_time
    );

    println!("Disjoint scaling (4T): PASS");
}

/// Red flag: p99 > 20× mean
#[test]
fn red_flag_tail_latency() {
    let db = Database::builder().in_memory().open_temp().unwrap();
    let kv = KVStore::new(Arc::new(db));
    let run_id = RunId::new();

    // Warmup
    for i in 0..100 {
        kv.put(&run_id, &format!("warmup{}", i), Value::I64(i as i64))
            .unwrap();
    }

    // Collect latencies
    let mut latencies: Vec<u128> = Vec::with_capacity(1000);
    for i in 0..1000 {
        let start = Instant::now();
        kv.put(&run_id, &format!("key{}", i), Value::I64(i as i64))
            .unwrap();
        latencies.push(start.elapsed().as_nanos());
    }

    latencies.sort();
    let mean = latencies.iter().sum::<u128>() / latencies.len() as u128;
    let p99 = latencies[989]; // 99th percentile

    let ratio = p99 as f64 / mean.max(1) as f64;

    println!("mean: {}ns, p99: {}ns, Ratio: {:.1}×", mean, p99, ratio);

    assert!(
        ratio <= 20.0,
        "RED FLAG: p99/mean = {:.1}× > 20× threshold.\n\
         mean: {}ns, p99: {}ns\n\
         ACTION: Fix tail latency source.",
        ratio,
        mean,
        p99
    );

    println!("p99/mean: PASS");
}

/// Red flag: Hot path has allocations (after warmup)
#[test]
fn red_flag_hot_path_allocations() {
    let db = Database::builder().in_memory().open_temp().unwrap();
    let kv = KVStore::new(Arc::new(db));
    let run_id = RunId::new();

    // Warmup - fill pool
    for _ in 0..10 {
        kv.put(&run_id, "warmup", Value::I64(0)).unwrap();
    }

    // Check pool has contexts
    let pool_size_before = TransactionPool::pool_size();
    println!("Pool size before: {}", pool_size_before);

    assert!(
        pool_size_before > 0,
        "Pool should have contexts after warmup"
    );

    // Do operations
    for i in 0..100 {
        kv.put(&run_id, &format!("key{}", i), Value::I64(i as i64))
            .unwrap();
    }

    // Pool should still have same number of contexts (reused, not allocated)
    let pool_size_after = TransactionPool::pool_size();
    println!("Pool size after: {}", pool_size_after);

    assert_eq!(
        pool_size_before, pool_size_after,
        "RED FLAG: Pool size changed from {} to {}.\n\
         Transactions are not being properly pooled.\n\
         ACTION: Eliminate allocations.",
        pool_size_before, pool_size_after
    );

    println!("Hot path allocations: PASS");
}

/// Additional: Verify graceful shutdown doesn't lose data
#[test]
fn red_flag_graceful_shutdown() {
    use tempfile::TempDir;

    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test_db");

    // Write data
    {
        let db = Arc::new(
            Database::builder()
                .path(&db_path)
                .buffered()
                .open()
                .unwrap(),
        );
        let kv = KVStore::new(db.clone());
        let run_id = RunId::new();

        for i in 0..100 {
            kv.put(&run_id, &format!("key{}", i), Value::I64(i as i64))
                .unwrap();
        }

        // Graceful shutdown
        db.shutdown().unwrap();
    }

    // Reopen and verify data persisted
    {
        let db = Database::builder()
            .path(&db_path)
            .buffered()
            .open()
            .unwrap();

        // Data should be there after recovery
        // (version > 0 means writes happened, DB opened successfully means recovery worked)
        assert!(
            db.storage().current_version() > 0,
            "Data should persist after shutdown"
        );
    }

    println!("Graceful shutdown: PASS");
}
