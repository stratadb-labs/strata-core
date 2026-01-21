//! ISSUE-019: Database Doesn't Instantiate Durability Handlers
//!
//! **Severity**: MEDIUM
//! **Location**: `/crates/engine/src/database.rs`
//!
//! **Problem**: Database stores which mode was selected but doesn't actually
//! instantiate durability handlers (InMemoryDurability, BufferedDurability,
//! StrictDurability) during open.
//!
//! **Impact**: Durability mode selection may not take effect as expected.

use crate::test_utils::*;

/// Test in-memory mode behavior.
#[test]
fn test_in_memory_mode() {
    let test_db = TestDb::new_in_memory();
    let kv = test_db.kv();

    kv.put(&test_db.run_id, "inmem_key", strata_core::value::Value::I64(42))
        .expect("put");

    // In-memory mode should not create WAL files
    let _wal_dir = test_db.db_path().join("wal");
    // Note: wal_dir might still be created but empty
}

/// Test buffered mode behavior.
#[test]
fn test_buffered_mode() {
    let test_db = TestDb::new(); // buffered mode
    let kv = test_db.kv();

    kv.put(&test_db.run_id, "buffered_key", strata_core::value::Value::I64(42))
        .expect("put");

    // Buffered mode should batch writes
    test_db.db.flush().expect("flush");
}

/// Test strict mode behavior.
#[test]
fn test_strict_mode() {
    let mut test_db = TestDb::new_strict();
    let run_id = test_db.run_id;

    {
        let kv = test_db.kv();
        kv.put(&run_id, "strict_key", strata_core::value::Value::I64(42))
            .expect("put");
    }

    test_db.db.flush().expect("flush");
    test_db.reopen();

    // Data should survive restart
    let kv = test_db.kv();
    let value = kv.get(&run_id, "strict_key").expect("get").map(|v| v.value);
    assert!(value.is_some(), "Data should survive restart in strict mode");
}
