//! ISSUE-009: ReadOnlyView Incomplete - Not Derived from EventLog
//!
//! **Severity**: HIGH
//! **Location**: `/crates/engine/src/replay.rs:192-274`
//!
//! **Problem**: ReadOnlyView captures state but:
//! 1. No EventLog integration
//! 2. No WAL replay integration
//! 3. No Snapshot loading code
//! 4. No actual replay_run() implementation that reconstructs state
//!
//! **Impact**: Replay invariants P1-P3 not fully implemented.

use crate::test_utils::*;
use strata_core::value::Value;

/// Test ReadOnlyView construction from EventLog.
#[test]
fn test_readonly_view_from_eventlog() {
    let test_db = TestDb::new_strict();
    let run_id = test_db.run_id;

    // Create events
    let event = test_db.event();
    event.append(&run_id, "type1", Value::I64(1))
        .expect("append");
    event.append(&run_id, "type2", Value::I64(2))
        .expect("append");

    test_db.db.flush().expect("flush");

    // When ISSUE-009 is fixed:
    // - ReadOnlyView should include EventLog data
    // - replay(run_id) = f(Snapshot, WAL, EventLog) per spec

    // For now, verify events exist in the log
    let events = event.read_range(&run_id, 0, 100).expect("read_range");
    assert_eq!(events.len(), 2, "Should have 2 events");
}

/// Test ReadOnlyView includes WAL data.
#[test]
fn test_readonly_view_includes_wal() {
    let test_db = TestDb::new_strict();
    let run_id = test_db.run_id;

    // Write data to WAL
    let kv = test_db.kv();
    kv.put(&run_id, "wal_data", strata_core::value::Value::I64(42))
        .expect("put");

    test_db.db.flush().expect("flush");

    // When ISSUE-009 is fixed:
    // let view = test_db.db.replay_run(run_id)?;
    // let value = view.kv_get("wal_data");
    // assert_eq!(value, Some(Value::I64(42)));
}

/// Test ReadOnlyView includes snapshot data.
#[test]
fn test_readonly_view_includes_snapshot() {
    let test_db = TestDb::new_strict();
    let run_id = test_db.run_id;

    // Create data that will be in snapshot
    let kv = test_db.kv();
    for i in 0..100 {
        kv.put(&run_id, &format!("snap_key_{}", i), strata_core::value::Value::I64(i))
            .expect("put");
    }

    test_db.db.flush().expect("flush");

    // When ISSUE-009 is fixed:
    // - Snapshot should be created
    // - ReadOnlyView should load from snapshot + WAL
}

/// Test replay produces same result regardless of intermediate state.
#[test]
fn test_replay_determinism() {
    let test_db = TestDb::new_strict();
    let run_id = test_db.run_id;

    let kv = test_db.kv();

    // Write, delete, write again
    kv.put(&run_id, "key", strata_core::value::Value::I64(1)).expect("put");
    kv.delete(&run_id, "key").expect("delete");
    kv.put(&run_id, "key", strata_core::value::Value::I64(2)).expect("put");

    test_db.db.flush().expect("flush");

    // Final state should be value=2 regardless of intermediate states
    let value = kv.get(&run_id, "key").expect("get").map(|v| v.value);
    assert_eq!(value, Some(strata_core::value::Value::I64(2)));

    // When ISSUE-009 is fixed:
    // let view = test_db.db.replay_run(run_id)?;
    // assert_eq!(view.kv_get("key"), Some(Value::I64(2)));
}
