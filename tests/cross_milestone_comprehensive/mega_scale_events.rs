//! Mega-Scale Event Tests
//!
//! Tests with many events.

use crate::test_utils::*;
use in_mem_core::value::Value;

/// Test 100K events in single log.
#[test]
fn test_100k_events() {
    let test_db = TestDb::new_in_memory();
    let event = test_db.event();
    let run_id = test_db.run_id;

    for i in 0..100_000 {
        event
            .append(&run_id, "mega_event", Value::I64(i))
            .expect("append");

        if i % 10_000 == 0 {
            eprintln!("Appended {} events", i);
        }
    }

    // Verify events were appended using len()
    let len = event.len(&run_id).expect("len");
    assert!(len >= 100_000, "Should have at least 100K events");

    // Read range should work
    let events = event.read_range(&run_id, 50_000, 50_100).expect("range");
    assert_eq!(events.len(), 100, "Should return 100 events in range");
}

/// Test many event types.
#[test]
fn test_many_event_types() {
    let test_db = TestDb::new_in_memory();
    let event = test_db.event();
    let run_id = test_db.run_id;

    // Create events with 100 different types
    for log in 0..100 {
        let event_type = format!("type_{}", log);
        for i in 0..100 {
            event
                .append(&run_id, &event_type, Value::I64(i))
                .expect("append");
        }
    }

    // Verify events exist
    let len = event.len(&run_id).expect("len");
    assert!(len >= 10_000, "Should have at least 10K events");
}

/// Test event chain integrity at scale.
#[test]
fn test_chain_integrity_at_scale() {
    let test_db = TestDb::new_in_memory();
    let event = test_db.event();
    let run_id = test_db.run_id;

    // Append 1000 events
    for i in 0..1000 {
        event
            .append(&run_id, "chain_type", Value::I64(i))
            .expect("append");
    }

    // Verify chain
    let result = event.verify_chain(&run_id).expect("verify");
    assert!(result.is_valid, "Chain should be valid");
    assert!(result.length >= 1000, "Should verify at least 1000 events");
}
