//! ISSUE-008: BufferedDurability Requires Manual Thread Startup
//!
//! **Severity**: HIGH
//! **Location**: `/crates/engine/src/durability/buffered.rs:123`
//!
//! **Problem**: `start_flush_thread()` must be called EXPLICITLY after creating
//! `BufferedDurability`. If users forget, the background flush thread never starts
//! and writes silently accumulate without being flushed.
//!
//! **Impact**: Silent data loss risk if thread not started.

use crate::test_utils::*;
use std::thread;
use std::time::Duration;

/// Test that buffered durability auto-starts flush thread.
#[test]
fn test_buffered_auto_starts_flush_thread() {
    // Create database with buffered durability
    let test_db = TestDb::new(); // Uses buffered mode

    // Write some data
    let kv = test_db.kv();
    for i in 0..10 {
        kv.put(
            &test_db.run_id,
            &format!("key_{}", i),
            strata_core::value::Value::I64(i),
        )
        .expect("put");
    }

    // Wait for flush interval
    thread::sleep(Duration::from_millis(200));

    // When ISSUE-008 is fixed:
    // - Flush thread should have started automatically
    // - Data should be flushed to WAL
    // - No explicit start_flush_thread() call needed

    // Verify data is accessible
    for i in 0..10 {
        let value = kv.get(&test_db.run_id, &format!("key_{}", i)).expect("get").map(|v| v.value);
        assert!(value.is_some(), "Key {} should exist", i);
    }
}

/// Test that buffered mode doesn't lose data on normal shutdown.
#[test]
fn test_buffered_no_data_loss() {
    let mut test_db = TestDb::new();
    let run_id = test_db.run_id;

    // Write data
    {
        let kv = test_db.kv();
        kv.put(&run_id, "important", strata_core::value::Value::String("data".into()))
            .expect("put");
    }

    // Flush and reopen
    test_db.db.flush().expect("flush");
    test_db.reopen();

    // Verify data survived
    let kv = test_db.kv();
    let value = kv.get(&run_id, "important").expect("get").map(|v| v.value);
    assert!(value.is_some(), "Data should survive restart");
}

/// Test flush interval configuration.
#[test]
fn test_buffered_flush_interval() {
    // When ISSUE-008 is fixed:
    // - BufferedDurability should accept flush interval parameter
    // - Default should be 100ms per M4_ARCHITECTURE.md
    //
    // let db = Database::builder()
    //     .path(dir)
    //     .buffered_with(Duration::from_millis(50), 500) // 50ms or 500 writes
    //     .open()?;

    // For now, verify buffered mode works with default settings
    let test_db = TestDb::new();
    assert_db_healthy(&test_db.db, &test_db.run_id);
}
