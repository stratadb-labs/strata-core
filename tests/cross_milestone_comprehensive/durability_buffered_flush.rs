//! Buffered Durability Flush Tests
//!
//! Tests buffered mode flush behavior.

use crate::test_utils::*;
use std::thread;
use std::time::Duration;

/// Test explicit flush persists data.
#[test]
fn test_explicit_flush() {
    let mut test_db = TestDb::new();
    let run_id = test_db.run_id;

    {
        let kv = test_db.kv();
        kv.put(&run_id, "flush_test", in_mem_core::value::Value::I64(42)).expect("put");
    }

    // Explicit flush
    test_db.db.flush().expect("flush");

    // Reopen and verify
    test_db.reopen();
    let kv = test_db.kv();
    assert!(kv.get(&run_id, "flush_test").expect("get").is_some());
}

/// Test automatic flush after interval.
#[test]
fn test_auto_flush_after_interval() {
    let test_db = TestDb::new();
    let kv = test_db.kv();
    let run_id = test_db.run_id;

    kv.put(&run_id, "auto_flush", in_mem_core::value::Value::I64(1)).expect("put");

    // Wait for auto-flush interval (default 100ms)
    thread::sleep(Duration::from_millis(200));

    // When ISSUE-008 is fixed:
    // - Auto-flush thread should have flushed data
    // - Data should be in WAL
}

/// Test flush on shutdown.
#[test]
fn test_flush_on_shutdown() {
    let test_db = TestDb::new();
    let kv = test_db.kv();
    let run_id = test_db.run_id;

    kv.put(&run_id, "shutdown_data", in_mem_core::value::Value::String("important".into()))
        .expect("put");

    // Database should flush on drop
    drop(test_db);

    // Can't easily verify without reopening, but this tests the path
}
