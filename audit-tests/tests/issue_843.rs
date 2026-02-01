//! Audit test for issue #843: Event hash algorithm diverges between EventLog and Transaction
//! Verdict: CONFIRMED BUG
//!
//! EventLog::append() computes hashes with little-endian byte order and length-prefixed
//! fields, while Transaction::event_append() uses big-endian and no length prefixes.
//! Events appended through different code paths produce different hashes for the
//! same logical event, breaking hash chain integrity.

use std::collections::HashMap;
use strata_core::types::BranchId;
use strata_core::value::Value;
use strata_engine::database::Database;
use strata_engine::primitives::event::EventLog;

/// Helper to create an object payload
fn payload(key: &str, value: Value) -> Value {
    Value::Object(HashMap::from([(key.to_string(), value)]))
}

/// Verify that the EventLog hash chain is consistent when using only EventLog::append.
/// This serves as the baseline -- the EventLog's own hash computation is self-consistent.
#[test]
fn issue_843_eventlog_hash_chain_baseline() {
    let db = Database::cache().unwrap();
    let log = EventLog::new(db.clone());
    let branch_id = BranchId::new();

    // Append several events through EventLog
    log.append(&branch_id, "event_a", payload("seq", Value::Int(1)))
        .unwrap();
    log.append(&branch_id, "event_b", payload("seq", Value::Int(2)))
        .unwrap();
    log.append(&branch_id, "event_c", payload("seq", Value::Int(3)))
        .unwrap();

    // Read all events and verify hash chain
    let event0 = log.read(&branch_id, 0).unwrap().unwrap();
    let event1 = log.read(&branch_id, 1).unwrap().unwrap();
    let event2 = log.read(&branch_id, 2).unwrap().unwrap();

    // First event's prev_hash should be zeros (genesis)
    assert_eq!(
        event0.value.prev_hash, [0u8; 32],
        "First event should chain from zero hash"
    );

    // Chain integrity: each event's prev_hash == previous event's hash
    assert_eq!(
        event1.value.prev_hash, event0.value.hash,
        "Event 1's prev_hash should equal event 0's hash"
    );
    assert_eq!(
        event2.value.prev_hash, event1.value.hash,
        "Event 2's prev_hash should equal event 1's hash"
    );

    // All hashes should be non-zero
    assert_ne!(event0.value.hash, [0u8; 32]);
    assert_ne!(event1.value.hash, [0u8; 32]);
    assert_ne!(event2.value.hash, [0u8; 32]);
}

/// Demonstrate the hash divergence by comparing the hash algorithm behavior.
///
/// The EventLog uses:
///   SHA256(seq_le || type_len_le || type_bytes || timestamp_le || payload_len_le || payload || prev_hash)
///
/// The Transaction uses:
///   SHA256(seq_be || type_bytes || payload || timestamp_be || prev_hash)
///
/// These produce different results for the same input.
#[test]
fn issue_843_hash_algorithm_divergence_demo() {
    use sha2::{Digest, Sha256};

    let sequence: u64 = 42;
    let event_type = "test_event";
    let timestamp: u64 = 1_000_000;
    let prev_hash = [0u8; 32];
    let payload_bytes = b"{}"; // simplified payload

    // EventLog algorithm (event.rs compute_event_hash)
    let eventlog_hash = {
        let mut hasher = Sha256::new();
        hasher.update(&sequence.to_le_bytes()); // little-endian
        hasher.update(&(event_type.len() as u32).to_le_bytes()); // length prefix
        hasher.update(event_type.as_bytes());
        hasher.update(&timestamp.to_le_bytes()); // little-endian, before payload
        hasher.update(&(payload_bytes.len() as u32).to_le_bytes()); // length prefix
        hasher.update(payload_bytes);
        hasher.update(&prev_hash);
        let result: [u8; 32] = hasher.finalize().into();
        result
    };

    // Transaction algorithm (context.rs compute_event_hash)
    let transaction_hash = {
        let mut hasher = Sha256::new();
        hasher.update(sequence.to_be_bytes()); // big-endian (DIFFERENT!)
        hasher.update(event_type.as_bytes()); // no length prefix (DIFFERENT!)
        hasher.update(payload_bytes); // no length prefix (DIFFERENT!)
        hasher.update(timestamp.to_be_bytes()); // big-endian, after payload (DIFFERENT!)
        hasher.update(prev_hash);
        let result: [u8; 32] = hasher.finalize().into();
        result
    };

    // BUG: These two hashes are different for the same logical event
    assert_ne!(
        eventlog_hash,
        transaction_hash,
        "BUG CONFIRMED: EventLog and Transaction compute different hashes \
         for the same event data. EventLog={:x?}, Transaction={:x?}",
        &eventlog_hash[..8],
        &transaction_hash[..8]
    );
}
