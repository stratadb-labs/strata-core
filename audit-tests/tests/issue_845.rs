//! Audit test for issue #845: from_stored_value silently resets metadata on deserialization failure
//! Verdict: CONFIRMED BUG (for EventLogMeta specifically)
//!
//! When EventLogMeta fails to deserialize, callers use .unwrap_or_else(|_| EventLogMeta::default())
//! which silently resets next_sequence to 0 and head_hash to zeros, causing event overwrites
//! and hash chain corruption.

use std::collections::HashMap;
use strata_core::types::BranchId;
use strata_core::value::Value;
use strata_engine::database::Database;
use strata_engine::primitives::event::EventLog;

fn payload(key: &str, value: Value) -> Value {
    Value::Object(HashMap::from([(key.to_string(), value)]))
}

/// Demonstrates that EventLog correctly handles normal operations.
/// This baseline test confirms the happy path works.
#[test]
fn issue_845_baseline_event_metadata_correct() {
    let db = Database::cache().unwrap();
    let log = EventLog::new(db.clone());
    let branch_id = BranchId::new();

    // Append several events
    log.append(&branch_id, "event1", payload("seq", Value::Int(1)))
        .unwrap();
    log.append(&branch_id, "event2", payload("seq", Value::Int(2)))
        .unwrap();
    log.append(&branch_id, "event3", payload("seq", Value::Int(3)))
        .unwrap();

    // Verify sequence is correct
    let len = log.len(&branch_id).unwrap();
    assert_eq!(len, 3, "Should have 3 events");

    // Read each event
    let event0 = log.read(&branch_id, 0).unwrap().unwrap();
    let event1 = log.read(&branch_id, 1).unwrap().unwrap();
    let event2 = log.read(&branch_id, 2).unwrap().unwrap();

    assert_eq!(event0.value.sequence, 0);
    assert_eq!(event1.value.sequence, 1);
    assert_eq!(event2.value.sequence, 2);

    // Hash chain should be intact
    assert_eq!(event1.value.prev_hash, event0.value.hash);
    assert_eq!(event2.value.prev_hash, event1.value.hash);
}

/// The bug pattern: EventLogMeta deserialization failure causes silent reset.
///
/// In event.rs, the EventLogMeta is read with:
///   from_stored_value(&v).unwrap_or_else(|_| EventLogMeta::default())
///
/// If the stored metadata is corrupted (not valid JSON), this silently resets:
///   - next_sequence to 0 (new events overwrite old ones)
///   - head_hash to [0u8; 32] (hash chain broken)
///   - streams to empty (per-stream metadata lost)
///
/// This test documents that the fallback to default() exists in the code.
/// In practice, metadata corruption could happen due to:
///   - Issue #838 (to_stored_value returning Null on serialization failure)
///   - Disk corruption during recovery
///   - Race conditions during concurrent writes
#[test]
fn issue_845_metadata_corruption_leads_to_sequence_reset() {
    let db = Database::cache().unwrap();
    let log = EventLog::new(db.clone());
    let branch_id = BranchId::new();

    // Append 5 events to establish a sequence
    for i in 0..5 {
        log.append(&branch_id, "test_event", payload("index", Value::Int(i)))
            .unwrap();
    }

    assert_eq!(log.len(&branch_id).unwrap(), 5);

    // If the EventLogMeta were corrupted at this point (e.g., stored as Value::Null
    // due to issue #838), the next call to len() or append() would silently reset
    // to default, making next_sequence=0 and enabling event overwrite.
    //
    // We cannot directly corrupt the metadata in this test without accessing
    // internal storage, but we document the code pattern:
    //
    // In event.rs line 328:
    //   let mut meta: EventLogMeta = match txn.get(&meta_key)? {
    //       Some(v) => from_stored_value(&v).unwrap_or_else(|_| EventLogMeta::default()),
    //       None => EventLogMeta::default(),
    //   };
    //
    // The .unwrap_or_else(|_| EventLogMeta::default()) silently swallows the error.
    // The correct behavior would be to return an error to the caller.

    // Verify current state is still correct
    let event4 = log.read(&branch_id, 4).unwrap().unwrap();
    assert_eq!(event4.value.sequence, 4);
    assert_eq!(event4.value.event_type, "test_event");
}
