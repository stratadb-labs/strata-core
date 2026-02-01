//! Audit test for issue #969: Standard mode shows same latency as Always
//!
//! In Standard mode, writes should return quickly (~microseconds)
//! without waiting for fsync. A background thread handles periodic fsync.
//! Previously, every write in Standard mode triggered an inline fsync check
//! that often resulted in a synchronous ~6ms fsync, making it as slow as
//! Always mode.
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

/// Helper: create a standard-mode database.
fn standard_db() -> (Arc<Database>, BranchId, TempDir) {
    let dir = TempDir::new().expect("tempdir");
    let db = Database::builder()
        .path(dir.path())
        .standard()
        .open()
        .expect("open db");
    let branch = BranchId::new();
    (db, branch, dir)
}

/// Helper: create an always-mode database.
fn always_db() -> (Arc<Database>, BranchId, TempDir) {
    let dir = TempDir::new().expect("tempdir");
    let db = Database::builder()
        .path(dir.path())
        .always()
        .open()
        .expect("open db");
    let branch = BranchId::new();
    (db, branch, dir)
}

#[test]
fn standard_mode_writes_are_fast() {
    let (db, branch, _dir) = standard_db();
    let ns = Namespace::for_branch(branch);

    // Warm up
    db.transaction(branch, |txn| {
        txn.put(Key::new_kv(ns.clone(), "warmup"), Value::String("warmup".into()))?;
        Ok(())
    })
    .unwrap();

    // Time 100 sequential writes in standard mode
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
    let standard_elapsed = start.elapsed();

    // 100 writes in standard mode should take well under 100ms.
    // In Always mode this would take ~600ms (6ms per fsync).
    // With the fix, standard mode should be ~1ms total (no fsyncs).
    assert!(
        standard_elapsed < Duration::from_millis(100),
        "100 standard writes took {:?}, expected < 100ms (always would be ~600ms)",
        standard_elapsed
    );
}

#[test]
fn standard_mode_much_faster_than_always() {
    let (standard_db, b_branch, _b_dir) = standard_db();
    let (always_db, s_branch, _s_dir) = always_db();
    let b_ns = Namespace::for_branch(b_branch);
    let s_ns = Namespace::for_branch(s_branch);

    let n = 10;

    // Time writes in always mode
    let start = Instant::now();
    for i in 0..n {
        always_db
            .transaction(s_branch, |txn| {
                txn.put(
                    Key::new_kv(s_ns.clone(), &format!("key_{}", i)),
                    Value::String(format!("value_{}", i).into()),
                )?;
                Ok(())
            })
            .unwrap();
    }
    let always_elapsed = start.elapsed();

    // Time writes in standard mode
    let start = Instant::now();
    for i in 0..n {
        standard_db
            .transaction(b_branch, |txn| {
                txn.put(
                    Key::new_kv(b_ns.clone(), &format!("key_{}", i)),
                    Value::String(format!("value_{}", i).into()),
                )?;
                Ok(())
            })
            .unwrap();
    }
    let standard_elapsed = start.elapsed();

    // Standard should be at least 10x faster than always
    let speedup = always_elapsed.as_nanos() as f64 / standard_elapsed.as_nanos() as f64;
    assert!(
        speedup > 10.0,
        "Standard mode should be >10x faster than always, but was only {:.1}x faster \
        (standard: {:?}, always: {:?})",
        speedup,
        standard_elapsed,
        always_elapsed
    );
}

#[test]
fn standard_mode_data_is_readable_immediately() {
    let (db, branch, _dir) = standard_db();
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
fn standard_mode_syncs_eventually() {
    let (db, branch, _dir) = standard_db();
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
