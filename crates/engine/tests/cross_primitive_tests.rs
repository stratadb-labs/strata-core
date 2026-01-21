//! Cross-Primitive Transaction Tests
//!
//! Per M2_REVISED_PLAN.md Story #54 and GitHub Issue #99:
//! Validates that transactions atomically operate across different
//! Key types (KV and Event) in a single transaction.

use strata_core::error::Error;
use strata_core::types::{Key, Namespace, RunId};
use strata_core::value::Value;
use strata_engine::Database;
use tempfile::TempDir;

fn create_ns(run_id: RunId) -> Namespace {
    Namespace::new(
        "tenant".to_string(),
        "app".to_string(),
        "agent".to_string(),
        run_id,
    )
}

// ============================================================================
// Cross-Primitive Atomic Write Tests
// ============================================================================

#[test]
fn test_atomic_kv_and_event_write() {
    let temp_dir = TempDir::new().unwrap();
    let db = Database::open(temp_dir.path().join("db")).unwrap();

    let run_id = RunId::new();
    let ns = create_ns(run_id);

    // Create keys for both KV and Event types
    let kv_key = Key::new_kv(ns.clone(), "user_state");
    let event_key = Key::new_event(ns.clone(), 1); // Event with sequence 1

    // Transaction writes to BOTH KV and Event in single transaction
    db.transaction(run_id, |txn| {
        txn.put(kv_key.clone(), Value::String("active".to_string()))?;
        txn.put(
            event_key.clone(),
            Value::String("user_logged_in".to_string()),
        )?;
        Ok(())
    })
    .unwrap();

    // Verify BOTH were committed atomically
    let kv_result = db.get(&kv_key).unwrap().unwrap();
    assert_eq!(kv_result.value, Value::String("active".to_string()));

    let event_result = db.get(&event_key).unwrap().unwrap();
    assert_eq!(
        event_result.value,
        Value::String("user_logged_in".to_string())
    );

    // Both should have the SAME version (atomically committed)
    assert_eq!(kv_result.version, event_result.version);
}

#[test]
fn test_atomic_kv_and_event_with_multiple_events() {
    let temp_dir = TempDir::new().unwrap();
    let db = Database::open(temp_dir.path().join("db")).unwrap();

    let run_id = RunId::new();
    let ns = create_ns(run_id);

    let kv_key = Key::new_kv(ns.clone(), "counter");
    let event1 = Key::new_event(ns.clone(), 1);
    let event2 = Key::new_event(ns.clone(), 2);
    let event3 = Key::new_event(ns.clone(), 3);

    // Write KV state + 3 events atomically
    db.transaction(run_id, |txn| {
        txn.put(kv_key.clone(), Value::I64(100))?;
        txn.put(event1.clone(), Value::String("event_1".to_string()))?;
        txn.put(event2.clone(), Value::String("event_2".to_string()))?;
        txn.put(event3.clone(), Value::String("event_3".to_string()))?;
        Ok(())
    })
    .unwrap();

    // All should be committed with same version
    let kv = db.get(&kv_key).unwrap().unwrap();
    let e1 = db.get(&event1).unwrap().unwrap();
    let e2 = db.get(&event2).unwrap().unwrap();
    let e3 = db.get(&event3).unwrap().unwrap();

    assert_eq!(kv.version, e1.version);
    assert_eq!(e1.version, e2.version);
    assert_eq!(e2.version, e3.version);
}

// ============================================================================
// Cross-Primitive Rollback Tests
// ============================================================================

#[test]
fn test_cross_primitive_rollback_on_error() {
    let temp_dir = TempDir::new().unwrap();
    let db = Database::open(temp_dir.path().join("db")).unwrap();

    let run_id = RunId::new();
    let ns = create_ns(run_id);

    let kv_key = Key::new_kv(ns.clone(), "state");
    let event_key = Key::new_event(ns.clone(), 1);

    // Pre-populate KV key
    db.put(run_id, kv_key.clone(), Value::I64(0)).unwrap();

    // Transaction that writes to both but then fails
    let result: Result<(), Error> = db.transaction(run_id, |txn| {
        txn.put(kv_key.clone(), Value::I64(999))?;
        txn.put(
            event_key.clone(),
            Value::String("should_rollback".to_string()),
        )?;

        // Force abort
        Err(Error::InvalidState("intentional failure".to_string()))
    });

    assert!(result.is_err());

    // BOTH writes should be rolled back
    // KV should still have original value
    let kv = db.get(&kv_key).unwrap().unwrap();
    assert_eq!(kv.value, Value::I64(0)); // Original value preserved

    // Event should NOT exist
    assert!(db.get(&event_key).unwrap().is_none());
}

#[test]
fn test_cross_primitive_conflict_rollback() {
    use std::sync::Arc;
    use std::thread;

    let temp_dir = TempDir::new().unwrap();
    let db = Arc::new(Database::open(temp_dir.path().join("db")).unwrap());

    let run_id = RunId::new();
    let ns = create_ns(run_id);

    let kv_key = Key::new_kv(ns.clone(), "contested_kv");
    let event_key = Key::new_event(ns.clone(), 1);

    // Pre-populate with initial value
    db.put(run_id, kv_key.clone(), Value::I64(0)).unwrap();

    let db1 = Arc::clone(&db);
    let db2 = Arc::clone(&db);
    let kv_key1 = kv_key.clone();
    let event_key1 = event_key.clone();

    // Use barriers to control execution order
    use std::sync::Barrier;
    let barrier = Arc::new(Barrier::new(2));
    let barrier1 = Arc::clone(&barrier);
    let barrier2 = Arc::clone(&barrier);

    // T1: Read KV, write Event + KV
    let h1 = thread::spawn(move || {
        db1.transaction(run_id, |txn| {
            // Read KV (adds to read_set)
            let _val = txn.get(&kv_key1)?;

            // Wait for T2 to also start
            barrier1.wait();

            // Write to both primitives
            txn.put(kv_key1.clone(), Value::I64(1))?;
            txn.put(event_key1.clone(), Value::String("from_t1".to_string()))?;

            // Small delay to let T2 commit first (usually)
            thread::sleep(std::time::Duration::from_millis(5));

            Ok(())
        })
    });

    // T2: Just update the KV key (no event)
    let h2 = thread::spawn(move || {
        db2.transaction(run_id, |txn| {
            // Wait for T1 to start
            barrier2.wait();

            // Blind write to KV (should commit quickly)
            txn.put(kv_key.clone(), Value::I64(2))?;
            Ok(())
        })
    });

    let r1 = h1.join().unwrap();
    let r2 = h2.join().unwrap();

    // At least one should succeed
    assert!(r1.is_ok() || r2.is_ok());

    // If T1 failed due to conflict, the event should NOT be written
    // (both KV and Event must roll back together)
    if r1.is_err() {
        // T1 was aborted - event should not exist
        assert!(db.get(&event_key).unwrap().is_none());
    }
}

// ============================================================================
// Cross-Primitive Read Consistency Tests
// ============================================================================

#[test]
fn test_cross_primitive_read_consistency() {
    let temp_dir = TempDir::new().unwrap();
    let db = Database::open(temp_dir.path().join("db")).unwrap();

    let run_id = RunId::new();
    let ns = create_ns(run_id);

    let kv_key = Key::new_kv(ns.clone(), "state");
    let event1 = Key::new_event(ns.clone(), 1);
    let event2 = Key::new_event(ns.clone(), 2);

    // Commit 1: Write initial state
    db.transaction(run_id, |txn| {
        txn.put(kv_key.clone(), Value::I64(1))?;
        txn.put(event1.clone(), Value::String("initial".to_string()))?;
        Ok(())
    })
    .unwrap();

    // Commit 2: Update state and add event
    db.transaction(run_id, |txn| {
        txn.put(kv_key.clone(), Value::I64(2))?;
        txn.put(event2.clone(), Value::String("updated".to_string()))?;
        Ok(())
    })
    .unwrap();

    // A transaction reading both should see consistent state
    db.transaction(run_id, |txn| {
        let kv = txn.get(&kv_key)?.unwrap();
        let e1 = txn.get(&event1)?;
        let e2 = txn.get(&event2)?;

        // Both events should be visible
        assert!(e1.is_some());
        assert!(e2.is_some());

        // KV should be the latest value
        assert_eq!(kv, Value::I64(2));

        Ok(())
    })
    .unwrap();
}

#[test]
fn test_cross_primitive_delete_atomicity() {
    let temp_dir = TempDir::new().unwrap();
    let db = Database::open(temp_dir.path().join("db")).unwrap();

    let run_id = RunId::new();
    let ns = create_ns(run_id);

    let kv_key = Key::new_kv(ns.clone(), "to_delete_kv");
    let event_key = Key::new_event(ns.clone(), 1);

    // Create both
    db.transaction(run_id, |txn| {
        txn.put(kv_key.clone(), Value::I64(100))?;
        txn.put(event_key.clone(), Value::String("event".to_string()))?;
        Ok(())
    })
    .unwrap();

    // Delete both atomically
    db.transaction(run_id, |txn| {
        txn.delete(kv_key.clone())?;
        txn.delete(event_key.clone())?;
        Ok(())
    })
    .unwrap();

    // Both should be deleted
    assert!(db.get(&kv_key).unwrap().is_none());
    assert!(db.get(&event_key).unwrap().is_none());
}

// ============================================================================
// Recovery Tests for Cross-Primitive Transactions
// ============================================================================

#[test]
fn test_cross_primitive_recovery() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("db");
    let run_id = RunId::new();
    let ns = create_ns(run_id);

    let kv_key = Key::new_kv(ns.clone(), "persistent_kv");
    let event_key = Key::new_event(ns.clone(), 42);

    // Write cross-primitive data and close
    {
        let db = Database::open(&db_path).unwrap();
        db.transaction(run_id, |txn| {
            txn.put(kv_key.clone(), Value::String("kv_data".to_string()))?;
            txn.put(event_key.clone(), Value::String("event_data".to_string()))?;
            Ok(())
        })
        .unwrap();
    }

    // Reopen and verify both recovered
    {
        let db = Database::open(&db_path).unwrap();

        let kv = db.get(&kv_key).unwrap().unwrap();
        assert_eq!(kv.value, Value::String("kv_data".to_string()));

        let event = db.get(&event_key).unwrap().unwrap();
        assert_eq!(event.value, Value::String("event_data".to_string()));

        // Should have same version (atomically committed)
        assert_eq!(kv.version, event.version);
    }
}
