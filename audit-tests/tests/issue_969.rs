//! Audit test for issue #969: Buffered mode shows same latency as Strict
//!
//! In Buffered/Batched mode, writes should return quickly (~microseconds)
//! without waiting for fsync. A background thread handles periodic fsync.
//! Previously, every write in Batched mode triggered an inline fsync check
//! that often resulted in a synchronous ~6ms fsync, making it as slow as
//! Strict mode.
//!
//! The fix: defer fsync to a background flush thread. The inline write path
//! only appends to the OS buffer (fast), and the background thread calls
//! fsync() periodically.

use std::sync::Arc;
use std::time::{Duration, Instant};
use strata_core::types::{BranchId, Key, Namespace};
use strata_core::Value;
use strata_engine::Database;
use tempfile::TempDir;

/// Helper: create a buffered-mode database.
fn buffered_db() -> (Arc<Database>, BranchId, TempDir) {
    let dir = TempDir::new().expect("tempdir");
    let db = Database::builder()
        .path(dir.path())
        .buffered()
        .open()
        .expect("open db");
    let branch = BranchId::new();
    (db, branch, dir)
}

/// Helper: create a strict-mode database.
fn strict_db() -> (Arc<Database>, BranchId, TempDir) {
    let dir = TempDir::new().expect("tempdir");
    let db = Database::builder()
        .path(dir.path())
        .strict()
        .open()
        .expect("open db");
    let branch = BranchId::new();
    (db, branch, dir)
}

#[test]
fn buffered_mode_writes_are_fast() {
    let (db, branch, _dir) = buffered_db();
    let ns = Namespace::for_branch(branch);

    // Warm up
    db.transaction(branch, |txn| {
        txn.put(Key::new_kv(ns.clone(), "warmup"), Value::String("warmup".into()))?;
        Ok(())
    })
    .unwrap();

    // Time 100 sequential writes in buffered mode
    let start = Instant::now();
    for i in 0..100 {
        db.transaction(branch, |txn| {
            txn.put(
                Key::new_kv(ns.clone(), &format!("key_{}", i)),
                Value::String(format!("value_{}", i).into()),
            )?;
            Ok(())
        })
        .unwrap();
    }
    let buffered_elapsed = start.elapsed();

    // 100 writes in buffered mode should take well under 100ms.
    // In Strict mode this would take ~600ms (6ms per fsync).
    // With the fix, buffered mode should be ~1ms total (no fsyncs).
    assert!(
        buffered_elapsed < Duration::from_millis(100),
        "100 buffered writes took {:?}, expected < 100ms (strict would be ~600ms)",
        buffered_elapsed
    );
}

#[test]
fn buffered_mode_much_faster_than_strict() {
    let (buffered_db, b_branch, _b_dir) = buffered_db();
    let (strict_db, s_branch, _s_dir) = strict_db();
    let b_ns = Namespace::for_branch(b_branch);
    let s_ns = Namespace::for_branch(s_branch);

    let n = 10;

    // Time writes in strict mode
    let start = Instant::now();
    for i in 0..n {
        strict_db
            .transaction(s_branch, |txn| {
                txn.put(
                    Key::new_kv(s_ns.clone(), &format!("key_{}", i)),
                    Value::String(format!("value_{}", i).into()),
                )?;
                Ok(())
            })
            .unwrap();
    }
    let strict_elapsed = start.elapsed();

    // Time writes in buffered mode
    let start = Instant::now();
    for i in 0..n {
        buffered_db
            .transaction(b_branch, |txn| {
                txn.put(
                    Key::new_kv(b_ns.clone(), &format!("key_{}", i)),
                    Value::String(format!("value_{}", i).into()),
                )?;
                Ok(())
            })
            .unwrap();
    }
    let buffered_elapsed = start.elapsed();

    // Buffered should be at least 10x faster than strict
    let speedup = strict_elapsed.as_nanos() as f64 / buffered_elapsed.as_nanos() as f64;
    assert!(
        speedup > 10.0,
        "Buffered mode should be >10x faster than strict, but was only {:.1}x faster \
        (buffered: {:?}, strict: {:?})",
        speedup,
        buffered_elapsed,
        strict_elapsed
    );
}

#[test]
fn buffered_mode_data_is_readable_immediately() {
    let (db, branch, _dir) = buffered_db();
    let ns = Namespace::for_branch(branch);

    // Write data
    db.transaction(branch, |txn| {
        txn.put(
            Key::new_kv(ns.clone(), "immediate"),
            Value::String("hello".into()),
        )?;
        Ok(())
    })
    .unwrap();

    // Read it back immediately (should be available in memory even if not yet fsynced)
    let result = db
        .transaction(branch, |txn| {
            let val = txn.get(&Key::new_kv(ns.clone(), "immediate"))?;
            Ok(val)
        })
        .unwrap();

    assert!(result.is_some(), "Data should be readable immediately");
    assert_eq!(result.unwrap(), Value::String("hello".into()));
}

#[test]
fn buffered_mode_syncs_eventually() {
    let (db, branch, _dir) = buffered_db();
    let ns = Namespace::for_branch(branch);

    // Write data
    db.transaction(branch, |txn| {
        txn.put(
            Key::new_kv(ns.clone(), "eventual"),
            Value::String("sync_me".into()),
        )?;
        Ok(())
    })
    .unwrap();

    // Wait for background flush thread to run (interval_ms default = 100ms)
    std::thread::sleep(Duration::from_millis(300));

    // Check that sync happened (sync_calls > 0)
    let counters = db.durability_counters().expect("should have counters");
    assert!(
        counters.sync_calls > 0,
        "Background flush thread should have synced at least once, but sync_calls={}",
        counters.sync_calls
    );
}
