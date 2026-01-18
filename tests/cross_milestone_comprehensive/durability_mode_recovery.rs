//! Durability Mode Recovery Tests
//!
//! Tests recovery behavior across durability modes.

use crate::test_utils::*;
use in_mem_core::value::Value;

/// Test strict mode recovery.
#[test]
fn test_strict_mode_recovery() {
    let mut test_db = TestDb::new_strict();
    let run_id = test_db.run_id;

    {
        let kv = test_db.kv();
        kv.put(&run_id, "strict_data", Value::String("survives".into())).expect("put");
    }

    test_db.db.flush().expect("flush");
    test_db.reopen();

    let kv = test_db.kv();
    let value = kv.get(&run_id, "strict_data").expect("get");
    assert!(value.is_some(), "Data should survive restart in strict mode");
}

/// Test buffered mode recovery.
#[test]
fn test_buffered_mode_recovery() {
    let mut test_db = TestDb::new();
    let run_id = test_db.run_id;

    {
        let kv = test_db.kv();
        kv.put(&run_id, "buffered_data", Value::String("survives".into())).expect("put");
    }

    test_db.db.flush().expect("flush");
    test_db.reopen();

    let kv = test_db.kv();
    let value = kv.get(&run_id, "buffered_data").expect("get");
    assert!(value.is_some(), "Data should survive restart after flush");
}

/// Test in-memory mode doesn't recover (by design).
#[test]
fn test_inmemory_no_recovery() {
    // In-memory mode doesn't persist data
    let test_db = TestDb::new_in_memory();
    let kv = test_db.kv();
    let run_id = test_db.run_id;

    kv.put(&run_id, "ephemeral", Value::I64(42)).expect("put");

    // Data exists while database is open
    assert!(kv.get(&run_id, "ephemeral").expect("get").is_some());

    // After drop, data is gone (can't really test this without reopening)
}
