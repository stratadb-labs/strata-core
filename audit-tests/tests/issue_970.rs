//! Audit test for issue #970: Read operations trigger WAL writes
//!
//! Read-only operations (kv get, kv scan, event read, json list, etc.) should
//! never write to the WAL. Previously, all operations wrapped in
//! `db.transaction()` unconditionally committed to the WAL, even when the
//! transaction had no writes. This produced a 47-49 byte metadata record per
//! read, adding ~6ms of fsync latency in Strict mode.
//!
//! The fix: skip WAL append in `TransactionManager::commit()` when the
//! transaction is read-only (empty write_set, delete_set, cas_set, json_writes).

use std::sync::Arc;
use strata_core::types::{BranchId, Key, Namespace};
use strata_core::Value;
use strata_engine::Database;
use tempfile::TempDir;

/// Helper: create a strict-mode database with WAL counters available.
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

/// Helper: get current WAL append count.
fn wal_appends(db: &Database) -> u64 {
    db.durability_counters()
        .map(|c| c.wal_appends)
        .unwrap_or(0)
}

#[test]
fn read_only_kv_get_produces_no_wal_writes() {
    let (db, branch, _dir) = strict_db();
    let ns = Namespace::for_branch(branch);

    // Setup: write a key
    db.transaction(branch, |txn| {
        txn.put(Key::new_kv(ns.clone(), "key1"), Value::String("value1".into()))?;
        Ok(())
    })
    .unwrap();

    let before = wal_appends(&db);

    // Read-only: kv get
    db.transaction(branch, |txn| {
        let _ = txn.get(&Key::new_kv(ns.clone(), "key1"))?;
        Ok(())
    })
    .unwrap();

    let after = wal_appends(&db);
    assert_eq!(
        after, before,
        "kv get should produce zero WAL appends, but produced {}",
        after - before
    );
}

#[test]
fn read_only_scan_produces_no_wal_writes() {
    let (db, branch, _dir) = strict_db();
    let ns = Namespace::for_branch(branch);

    // Setup: write some keys
    for i in 0..10 {
        db.transaction(branch, |txn| {
            txn.put(
                Key::new_kv(ns.clone(), &format!("key_{}", i)),
                Value::Int(i),
            )?;
            Ok(())
        })
        .unwrap();
    }

    let before = wal_appends(&db);

    // Read-only: prefix scan
    db.transaction(branch, |txn| {
        let prefix = Key::new_kv(ns.clone(), "key_");
        let results = txn.scan_prefix(&prefix)?;
        assert!(!results.is_empty(), "scan should return results");
        Ok(())
    })
    .unwrap();

    let after = wal_appends(&db);
    assert_eq!(
        after, before,
        "prefix scan should produce zero WAL appends, but produced {}",
        after - before
    );
}

#[test]
fn write_transaction_still_produces_wal_writes() {
    let (db, branch, _dir) = strict_db();
    let ns = Namespace::for_branch(branch);

    let before = wal_appends(&db);

    // Write transaction
    db.transaction(branch, |txn| {
        txn.put(
            Key::new_kv(ns.clone(), "write_key"),
            Value::String("write_value".into()),
        )?;
        Ok(())
    })
    .unwrap();

    let after = wal_appends(&db);
    assert!(
        after > before,
        "write transaction should produce WAL appends (before={}, after={})",
        before, after
    );
}

#[test]
fn mixed_reads_then_write_only_writes_wal_for_mutations() {
    let (db, branch, _dir) = strict_db();
    let ns = Namespace::for_branch(branch);

    // Setup: seed data
    for i in 0..5 {
        db.transaction(branch, |txn| {
            txn.put(
                Key::new_kv(ns.clone(), &format!("item_{}", i)),
                Value::Int(i),
            )?;
            Ok(())
        })
        .unwrap();
    }

    let baseline = wal_appends(&db);

    // 5 read-only transactions
    for i in 0..5 {
        db.transaction(branch, |txn| {
            let _ = txn.get(&Key::new_kv(ns.clone(), &format!("item_{}", i)))?;
            Ok(())
        })
        .unwrap();
    }

    let after_reads = wal_appends(&db);
    assert_eq!(
        after_reads, baseline,
        "5 read-only transactions should produce zero WAL appends, but produced {}",
        after_reads - baseline
    );

    // 1 write transaction
    db.transaction(branch, |txn| {
        txn.put(
            Key::new_kv(ns.clone(), "new_item"),
            Value::String("new_value".into()),
        )?;
        Ok(())
    })
    .unwrap();

    let after_write = wal_appends(&db);
    assert_eq!(
        after_write - after_reads,
        1,
        "1 write transaction should produce exactly 1 WAL append, but produced {}",
        after_write - after_reads
    );
}
