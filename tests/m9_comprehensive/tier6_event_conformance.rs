//! EventLog Conformance Tests
//!
//! This module verifies that the EventLog primitive conforms to all 7 invariants:
//!
//! 1. Addressable - Events have stable identity via EntityRef (run_id + sequence)
//! 2. Versioned - Reads return Versioned<Event>, writes return Version::Sequence
//! 3. Transactional - Event operations participate in transactions
//! 4. Lifecycle - Events are append-only (Create + Read, no Update/Delete)
//! 5. Run-scoped - Events are isolated by run
//! 6. Introspectable - Event existence can be checked via read
//! 7. Read/Write - Reads don't modify, writes (appends) produce sequence versions
//!
//! # Story #492: Invariant 1-2 Conformance Tests (Event portion)
//! # Story #493: Invariant 3-4 Conformance Tests (Event portion)
//! # Story #494: Invariant 5-6 Conformance Tests (Event portion)
//! # Story #495: Invariant 7 Conformance Tests (Event portion)

use crate::test_utils::test_run_id;
use strata_concurrency::snapshot::ClonedSnapshotView;
use strata_concurrency::TransactionContext;
use strata_core::types::{Namespace, RunId};
use strata_core::{EntityRef, PrimitiveType, Value, Version};
use strata_engine::transaction::Transaction;
use strata_engine::transaction_ops::TransactionOps;
use std::collections::{HashMap, HashSet};

/// Create a test namespace for a run
fn create_namespace(run_id: RunId) -> Namespace {
    Namespace::new(
        "test-tenant".to_string(),
        "test-app".to_string(),
        "test-agent".to_string(),
        run_id,
    )
}

/// Create a transaction context for testing
fn create_context(ns: &Namespace) -> TransactionContext {
    let snapshot = Box::new(ClonedSnapshotView::empty(100));
    TransactionContext::with_snapshot(1, ns.run_id, snapshot)
}

// ============================================================================
// Invariant 1: Everything is Addressable
// ============================================================================

#[test]
fn event_invariant1_has_stable_identity() {
    let run_id = test_run_id();

    // Event can be addressed via EntityRef (run_id + sequence)
    let entity_ref = EntityRef::event(run_id, 42);

    // EntityRef identifies the primitive type
    assert_eq!(entity_ref.primitive_type(), PrimitiveType::Event);

    // EntityRef provides the run context
    assert_eq!(entity_ref.run_id(), run_id);

    // EntityRef provides the sequence
    assert_eq!(entity_ref.event_sequence(), Some(42));
}

#[test]
fn event_invariant1_identity_is_immutable() {
    let run_id = test_run_id();
    let ns = create_namespace(run_id);
    let mut ctx = create_context(&ns);
    let mut txn = Transaction::new(&mut ctx, ns.clone());

    // Append an event
    let version = txn.event_append("test", Value::String("payload".to_string())).unwrap();
    let sequence = version.as_u64();

    // Create entity reference
    let entity_ref = EntityRef::event(run_id, sequence);

    // Read the event
    let event = txn.event_read(sequence).unwrap().unwrap();

    // Identity is immutable - event at sequence 0 will always be that event
    assert_eq!(event.value.sequence, sequence);
    assert_eq!(entity_ref.event_sequence(), Some(sequence));
}

#[test]
fn event_invariant1_identity_can_be_stored() {
    let run_id = test_run_id();

    // EntityRef can be used as a key in collections
    let mut store: HashMap<EntityRef, String> = HashMap::new();

    let ref1 = EntityRef::event(run_id, 0);
    let ref2 = EntityRef::event(run_id, 1);

    store.insert(ref1.clone(), "first event".to_string());
    store.insert(ref2.clone(), "second event".to_string());

    // Can retrieve using the same identity
    assert_eq!(store.get(&ref1), Some(&"first event".to_string()));
    assert_eq!(store.get(&ref2), Some(&"second event".to_string()));
}

#[test]
fn event_invariant1_identity_survives_serialization() {
    let run_id = test_run_id();
    let entity_ref = EntityRef::event(run_id, 99);

    // Serialize
    let json = serde_json::to_string(&entity_ref).expect("serialize");

    // Deserialize
    let restored: EntityRef = serde_json::from_str(&json).expect("deserialize");

    // Identity preserved
    assert_eq!(entity_ref, restored);
}

#[test]
fn event_invariant1_sequence_is_unique_identity() {
    let run_id = test_run_id();
    let ns = create_namespace(run_id);
    let mut ctx = create_context(&ns);
    let mut txn = Transaction::new(&mut ctx, ns.clone());

    // Append multiple events
    txn.event_append("e1", Value::I64(1)).unwrap();
    txn.event_append("e2", Value::I64(2)).unwrap();
    txn.event_append("e3", Value::I64(3)).unwrap();

    // Each has a unique identity by sequence
    let ref0 = EntityRef::event(run_id, 0);
    let ref1 = EntityRef::event(run_id, 1);
    let ref2 = EntityRef::event(run_id, 2);

    // All are different
    assert_ne!(ref0, ref1);
    assert_ne!(ref1, ref2);
    assert_ne!(ref0, ref2);
}

// ============================================================================
// Invariant 2: Everything is Versioned
// ============================================================================

#[test]
fn event_invariant2_read_returns_versioned() {
    let run_id = test_run_id();
    let ns = create_namespace(run_id);
    let mut ctx = create_context(&ns);
    let mut txn = Transaction::new(&mut ctx, ns.clone());

    // Append an event
    txn.event_append("test", Value::String("payload".to_string())).unwrap();

    // Read returns Versioned<Event>
    let result = txn.event_read(0).unwrap();
    assert!(result.is_some());

    let versioned = result.unwrap();
    // Has version (Sequence type for events)
    assert!(versioned.version.is_sequence());
    // Has timestamp
    assert!(versioned.timestamp.as_micros() > 0);
    // Has value
    assert_eq!(versioned.value.event_type, "test");
}

#[test]
fn event_invariant2_append_returns_sequence_version() {
    let run_id = test_run_id();
    let ns = create_namespace(run_id);
    let mut ctx = create_context(&ns);
    let mut txn = Transaction::new(&mut ctx, ns.clone());

    // Append returns Version::Sequence
    let v0 = txn.event_append("e0", Value::I64(0)).unwrap();
    let v1 = txn.event_append("e1", Value::I64(1)).unwrap();
    let v2 = txn.event_append("e2", Value::I64(2)).unwrap();

    assert_eq!(v0, Version::Sequence(0));
    assert_eq!(v1, Version::Sequence(1));
    assert_eq!(v2, Version::Sequence(2));
}

#[test]
fn event_invariant2_sequence_is_monotonic() {
    let run_id = test_run_id();
    let ns = create_namespace(run_id);
    let mut ctx = create_context(&ns);
    let mut txn = Transaction::new(&mut ctx, ns.clone());

    // Append multiple events
    let versions: Vec<Version> = (0..10)
        .map(|i| txn.event_append(&format!("e{}", i), Value::I64(i)).unwrap())
        .collect();

    // Sequences should be monotonically increasing
    for i in 1..versions.len() {
        assert!(versions[i].as_u64() > versions[i - 1].as_u64());
    }
}

#[test]
fn event_invariant2_read_version_matches_append() {
    let run_id = test_run_id();
    let ns = create_namespace(run_id);
    let mut ctx = create_context(&ns);
    let mut txn = Transaction::new(&mut ctx, ns.clone());

    // Append
    let append_version = txn.event_append("test", Value::String("data".to_string())).unwrap();

    // Read back - version should match
    let read_result = txn.event_read(0).unwrap().unwrap();
    assert_eq!(read_result.version, append_version);
}

// ============================================================================
// Invariant 3: Everything is Transactional
// ============================================================================

#[test]
fn event_invariant3_participates_in_transaction() {
    let run_id = test_run_id();
    let ns = create_namespace(run_id);
    let mut ctx = create_context(&ns);

    // Event operations happen within a transaction context
    {
        let mut txn = Transaction::new(&mut ctx, ns.clone());
        txn.event_append("test", Value::String("payload".to_string())).unwrap();

        // Read-your-writes within transaction
        let result = txn.event_read(0).unwrap();
        assert!(result.is_some());
    }
}

#[test]
fn event_invariant3_read_your_writes() {
    let run_id = test_run_id();
    let ns = create_namespace(run_id);
    let mut ctx = create_context(&ns);
    let mut txn = Transaction::new(&mut ctx, ns.clone());

    // Append events and read them back immediately
    txn.event_append("first", Value::I64(100)).unwrap();
    txn.event_append("second", Value::I64(200)).unwrap();

    // Read sees the writes
    let first = txn.event_read(0).unwrap().unwrap();
    let second = txn.event_read(1).unwrap().unwrap();

    assert_eq!(first.value.event_type, "first");
    assert_eq!(first.value.payload, Value::I64(100));
    assert_eq!(second.value.event_type, "second");
    assert_eq!(second.value.payload, Value::I64(200));
}

#[test]
fn event_invariant3_event_len_reflects_pending() {
    let run_id = test_run_id();
    let ns = create_namespace(run_id);
    let mut ctx = create_context(&ns);
    let mut txn = Transaction::new(&mut ctx, ns.clone());

    // Initial length
    assert_eq!(txn.event_len().unwrap(), 0);

    // Append events
    txn.event_append("e1", Value::I64(1)).unwrap();
    assert_eq!(txn.event_len().unwrap(), 1);

    txn.event_append("e2", Value::I64(2)).unwrap();
    assert_eq!(txn.event_len().unwrap(), 2);
}

#[test]
fn event_invariant3_range_includes_pending() {
    let run_id = test_run_id();
    let ns = create_namespace(run_id);
    let mut ctx = create_context(&ns);
    let mut txn = Transaction::new(&mut ctx, ns.clone());

    // Append several events
    for i in 0..5 {
        txn.event_append(&format!("event_{}", i), Value::I64(i)).unwrap();
    }

    // Range sees all pending events
    let events = txn.event_range(0, 5).unwrap();
    assert_eq!(events.len(), 5);
}

// ============================================================================
// Invariant 4: Everything Has a Lifecycle
// ============================================================================

#[test]
fn event_invariant4_append_only_lifecycle() {
    let run_id = test_run_id();
    let ns = create_namespace(run_id);
    let mut ctx = create_context(&ns);
    let mut txn = Transaction::new(&mut ctx, ns.clone());

    // CREATE (via append)
    let version = txn.event_append("user_created", Value::String("alice".to_string())).unwrap();
    assert_eq!(version, Version::Sequence(0));

    // READ (exists)
    let event = txn.event_read(0).unwrap();
    assert!(event.is_some());
    assert_eq!(event.unwrap().value.event_type, "user_created");

    // Events are IMMUTABLE - no UPDATE
    // Events are IMMUTABLE - no DELETE

    // Can only append more
    let version2 = txn.event_append("user_deleted", Value::String("alice".to_string())).unwrap();
    assert_eq!(version2, Version::Sequence(1));
}

#[test]
fn event_invariant4_events_are_immutable() {
    let run_id = test_run_id();
    let ns = create_namespace(run_id);
    let mut ctx = create_context(&ns);
    let mut txn = Transaction::new(&mut ctx, ns.clone());

    // Append an event
    txn.event_append("original", Value::String("data".to_string())).unwrap();

    // Read it
    let event1 = txn.event_read(0).unwrap().unwrap();

    // Append another event (cannot update the first one)
    txn.event_append("another", Value::String("more data".to_string())).unwrap();

    // First event is unchanged
    let event1_again = txn.event_read(0).unwrap().unwrap();
    assert_eq!(event1.value.event_type, event1_again.value.event_type);
    assert_eq!(event1.value.payload, event1_again.value.payload);
    assert_eq!(event1.value.sequence, event1_again.value.sequence);
}

#[test]
fn event_invariant4_sequence_never_reused() {
    let run_id = test_run_id();
    let ns = create_namespace(run_id);
    let mut ctx = create_context(&ns);
    let mut txn = Transaction::new(&mut ctx, ns.clone());

    // Append events
    let v1 = txn.event_append("e1", Value::I64(1)).unwrap();
    let v2 = txn.event_append("e2", Value::I64(2)).unwrap();
    let v3 = txn.event_append("e3", Value::I64(3)).unwrap();

    // Sequences are unique and never reused
    let sequences: HashSet<u64> = vec![v1.as_u64(), v2.as_u64(), v3.as_u64()]
        .into_iter()
        .collect();
    assert_eq!(sequences.len(), 3);
}

// ============================================================================
// Invariant 5: Everything is Run-Scoped
// ============================================================================

#[test]
fn event_invariant5_isolated_between_runs() {
    let run1 = test_run_id();
    let run2 = test_run_id();

    let ns1 = create_namespace(run1);
    let ns2 = create_namespace(run2);

    let mut ctx1 = create_context(&ns1);
    let mut ctx2 = create_context(&ns2);

    let mut txn1 = Transaction::new(&mut ctx1, ns1.clone());
    let mut txn2 = Transaction::new(&mut ctx2, ns2.clone());

    // Append to run1
    txn1.event_append("run1_event", Value::I64(1)).unwrap();

    // Append to run2 (gets its own sequence starting at 0)
    let v = txn2.event_append("run2_event", Value::I64(2)).unwrap();
    assert_eq!(v, Version::Sequence(0)); // Independent sequence

    // Run2 doesn't see run1's event
    // Note: We can't easily test cross-run isolation at this level since
    // each Transaction is scoped to one namespace. The isolation is
    // enforced by the namespace in the key structure.
    let run2_event = txn2.event_read(0).unwrap().unwrap();
    assert_eq!(run2_event.value.event_type, "run2_event");
}

#[test]
fn event_invariant5_entity_ref_includes_run_id() {
    let run1 = test_run_id();
    let run2 = test_run_id();

    let ref1 = EntityRef::event(run1, 0);
    let ref2 = EntityRef::event(run2, 0);

    // Same sequence in different runs = different entities
    assert_ne!(ref1, ref2);
    assert_ne!(ref1.run_id(), ref2.run_id());
}

#[test]
fn event_invariant5_each_run_has_independent_sequence() {
    let run1 = test_run_id();
    let run2 = test_run_id();

    let ns1 = create_namespace(run1);
    let ns2 = create_namespace(run2);

    let mut ctx1 = create_context(&ns1);
    let mut ctx2 = create_context(&ns2);

    let mut txn1 = Transaction::new(&mut ctx1, ns1.clone());
    let mut txn2 = Transaction::new(&mut ctx2, ns2.clone());

    // Both runs start at sequence 0
    let v1 = txn1.event_append("e", Value::I64(1)).unwrap();
    let v2 = txn2.event_append("e", Value::I64(2)).unwrap();

    assert_eq!(v1, Version::Sequence(0));
    assert_eq!(v2, Version::Sequence(0));
}

// ============================================================================
// Invariant 6: Everything is Introspectable
// ============================================================================

#[test]
fn event_invariant6_existence_via_read() {
    let run_id = test_run_id();
    let ns = create_namespace(run_id);
    let mut ctx = create_context(&ns);
    let mut txn = Transaction::new(&mut ctx, ns.clone());

    // No event at sequence 0 yet
    assert!(txn.event_read(0).unwrap().is_none());

    // Append
    txn.event_append("test", Value::I64(1)).unwrap();

    // Now exists
    assert!(txn.event_read(0).unwrap().is_some());
}

#[test]
fn event_invariant6_can_get_length() {
    let run_id = test_run_id();
    let ns = create_namespace(run_id);
    let mut ctx = create_context(&ns);
    let mut txn = Transaction::new(&mut ctx, ns.clone());

    // Can check how many events exist
    assert_eq!(txn.event_len().unwrap(), 0);

    txn.event_append("e1", Value::I64(1)).unwrap();
    txn.event_append("e2", Value::I64(2)).unwrap();
    txn.event_append("e3", Value::I64(3)).unwrap();

    assert_eq!(txn.event_len().unwrap(), 3);
}

#[test]
fn event_invariant6_can_read_range() {
    let run_id = test_run_id();
    let ns = create_namespace(run_id);
    let mut ctx = create_context(&ns);
    let mut txn = Transaction::new(&mut ctx, ns.clone());

    // Append events
    for i in 0..10 {
        txn.event_append(&format!("e{}", i), Value::I64(i)).unwrap();
    }

    // Can introspect a range
    let middle = txn.event_range(3, 7).unwrap();
    assert_eq!(middle.len(), 4);
    assert_eq!(middle[0].value.event_type, "e3");
    assert_eq!(middle[3].value.event_type, "e6");
}

// ============================================================================
// Invariant 7: Reads and Writes Have Consistent Semantics
// ============================================================================

#[test]
fn event_invariant7_read_does_not_modify() {
    let run_id = test_run_id();
    let ns = create_namespace(run_id);
    let mut ctx = create_context(&ns);
    let mut txn = Transaction::new(&mut ctx, ns.clone());

    // Append an event
    txn.event_append("test", Value::String("payload".to_string())).unwrap();

    // Multiple reads
    let e1 = txn.event_read(0).unwrap().unwrap();
    let e2 = txn.event_read(0).unwrap().unwrap();
    let e3 = txn.event_read(0).unwrap().unwrap();

    // All reads return same version and value (no modification)
    assert_eq!(e1.version, e2.version);
    assert_eq!(e2.version, e3.version);
    assert_eq!(e1.value.event_type, e2.value.event_type);
    assert_eq!(e2.value.event_type, e3.value.event_type);
}

#[test]
fn event_invariant7_append_always_produces_version() {
    let run_id = test_run_id();
    let ns = create_namespace(run_id);
    let mut ctx = create_context(&ns);
    let mut txn = Transaction::new(&mut ctx, ns.clone());

    // Every append produces a version
    let v1 = txn.event_append("e1", Value::I64(1)).unwrap();
    let v2 = txn.event_append("e2", Value::I64(2)).unwrap();
    let v3 = txn.event_append("e3", Value::I64(3)).unwrap();

    // All are valid sequence versions
    assert!(v1.is_sequence());
    assert!(v2.is_sequence());
    assert!(v3.is_sequence());
}

#[test]
fn event_invariant7_length_read_does_not_modify() {
    let run_id = test_run_id();
    let ns = create_namespace(run_id);
    let mut ctx = create_context(&ns);
    let mut txn = Transaction::new(&mut ctx, ns.clone());

    txn.event_append("test", Value::I64(1)).unwrap();

    // Multiple length reads
    let len1 = txn.event_len().unwrap();
    let len2 = txn.event_len().unwrap();
    let len3 = txn.event_len().unwrap();

    // All return same value
    assert_eq!(len1, len2);
    assert_eq!(len2, len3);
}

// ============================================================================
// Event-Specific Invariant: Hash Chaining
// ============================================================================

#[test]
fn event_hash_chain_integrity() {
    let run_id = test_run_id();
    let ns = create_namespace(run_id);
    let mut ctx = create_context(&ns);
    let mut txn = Transaction::new(&mut ctx, ns.clone());

    // Append events
    txn.event_append("first", Value::I64(1)).unwrap();
    txn.event_append("second", Value::I64(2)).unwrap();
    txn.event_append("third", Value::I64(3)).unwrap();

    let e0 = txn.event_read(0).unwrap().unwrap();
    let e1 = txn.event_read(1).unwrap().unwrap();
    let e2 = txn.event_read(2).unwrap().unwrap();

    // First event's prev_hash should be zeros (genesis)
    assert_eq!(e0.value.prev_hash, [0u8; 32]);

    // Each subsequent event chains from the previous
    assert_eq!(e1.value.prev_hash, e0.value.hash);
    assert_eq!(e2.value.prev_hash, e1.value.hash);

    // All events have non-zero hashes
    assert_ne!(e0.value.hash, [0u8; 32]);
    assert_ne!(e1.value.hash, [0u8; 32]);
    assert_ne!(e2.value.hash, [0u8; 32]);
}

// ============================================================================
// Combined Invariants Test
// ============================================================================

#[test]
fn event_all_invariants_work_together() {
    let run_id = test_run_id();
    let ns = create_namespace(run_id);
    let mut ctx = create_context(&ns);
    let mut txn = Transaction::new(&mut ctx, ns.clone());

    // Invariant 4: Lifecycle - CREATE (append-only)
    let version = txn.event_append("test_event", Value::String("test payload".to_string())).unwrap();

    // Invariant 2: Versioned (returns sequence version)
    assert!(version.is_sequence());
    assert_eq!(version, Version::Sequence(0));

    // Invariant 1: Addressable
    let entity_ref = EntityRef::event(run_id, version.as_u64());
    assert_eq!(entity_ref.primitive_type(), PrimitiveType::Event);

    // Invariant 6: Introspectable
    let event = txn.event_read(0).unwrap();
    assert!(event.is_some());
    let event = event.unwrap();

    // Invariant 7: Read doesn't modify
    let event2 = txn.event_read(0).unwrap().unwrap();
    assert_eq!(event.version, event2.version);

    // Invariant 3: Transactional (read-your-writes)
    assert_eq!(event.value.event_type, "test_event");

    // Invariant 5: Run-scoped (EntityRef includes run_id)
    assert_eq!(entity_ref.run_id(), run_id);
}
