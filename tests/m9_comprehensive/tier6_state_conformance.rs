//! StateCell Conformance Tests
//!
//! This module verifies that the StateCell primitive conforms to all 7 invariants:
//!
//! 1. Addressable - State cells have stable identity via EntityRef
//! 2. Versioned - Reads return Versioned<State>, writes return Version::Counter
//! 3. Transactional - State operations participate in transactions
//! 4. Lifecycle - State supports init, read, CAS update, delete
//! 5. Run-scoped - State cells are isolated by run
//! 6. Introspectable - State has exists() check
//! 7. Read/Write - Reads don't modify, writes produce versions
//!
//! # Story #492: Invariant 1-2 Conformance Tests (State portion)
//! # Story #493: Invariant 3-4 Conformance Tests (State portion)
//! # Story #494: Invariant 5-6 Conformance Tests (State portion)
//! # Story #495: Invariant 7 Conformance Tests (State portion)

use crate::test_utils::test_run_id;
use strata_concurrency::snapshot::ClonedSnapshotView;
use strata_concurrency::TransactionContext;
use strata_core::types::{Namespace, RunId};
use strata_core::{EntityRef, PrimitiveType, Value, Version};
use strata_engine::transaction::Transaction;
use strata_engine::transaction_ops::TransactionOps;
use std::collections::HashMap;

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
fn state_invariant1_has_stable_identity() {
    let run_id = test_run_id();

    // State cell can be addressed via EntityRef
    let entity_ref = EntityRef::state(run_id, "counter");

    // EntityRef identifies the primitive type
    assert_eq!(entity_ref.primitive_type(), PrimitiveType::State);

    // EntityRef provides the run context
    assert_eq!(entity_ref.run_id(), run_id);

    // EntityRef provides the state name
    assert_eq!(entity_ref.state_name(), Some("counter"));
}

#[test]
fn state_invariant1_identity_stable_across_updates() {
    let run_id = test_run_id();
    let ns = create_namespace(run_id);
    let mut ctx = create_context(&ns);
    let mut txn = Transaction::new(&mut ctx, ns.clone());

    // Create the entity reference
    let entity_ref = EntityRef::state(run_id, "counter");

    // Perform an update
    txn.state_init("counter", Value::I64(0)).unwrap();
    txn.state_cas("counter", 1, Value::I64(1)).unwrap();

    // The EntityRef is still the same (identity is stable)
    let entity_ref2 = EntityRef::state(run_id, "counter");
    assert_eq!(entity_ref, entity_ref2);
}

#[test]
fn state_invariant1_identity_can_be_stored() {
    let run_id = test_run_id();

    // EntityRef can be used as a key in collections
    let mut store: HashMap<EntityRef, String> = HashMap::new();

    let ref1 = EntityRef::state(run_id, "counter1");
    let ref2 = EntityRef::state(run_id, "counter2");

    store.insert(ref1.clone(), "state1".to_string());
    store.insert(ref2.clone(), "state2".to_string());

    // Can retrieve using the same identity
    assert_eq!(store.get(&ref1), Some(&"state1".to_string()));
    assert_eq!(store.get(&ref2), Some(&"state2".to_string()));
}

#[test]
fn state_invariant1_identity_survives_serialization() {
    let run_id = test_run_id();
    let entity_ref = EntityRef::state(run_id, "my-counter");

    // Serialize
    let json = serde_json::to_string(&entity_ref).expect("serialize");

    // Deserialize
    let restored: EntityRef = serde_json::from_str(&json).expect("deserialize");

    // Identity preserved
    assert_eq!(entity_ref, restored);
}

// ============================================================================
// Invariant 2: Everything is Versioned
// ============================================================================

#[test]
fn state_invariant2_read_returns_versioned() {
    let run_id = test_run_id();
    let ns = create_namespace(run_id);
    let mut ctx = create_context(&ns);
    let mut txn = Transaction::new(&mut ctx, ns.clone());

    // Initialize a state cell
    txn.state_init("counter", Value::I64(42)).unwrap();

    // Read returns Versioned<State>
    let result = txn.state_read("counter").unwrap();
    assert!(result.is_some());

    let versioned = result.unwrap();
    // Has version (Counter type for state)
    assert!(versioned.version.is_counter());
    // Has timestamp
    assert!(versioned.timestamp.as_micros() > 0);
    // Has value
    assert_eq!(versioned.value.value, Value::I64(42));
}

#[test]
fn state_invariant2_init_returns_version() {
    let run_id = test_run_id();
    let ns = create_namespace(run_id);
    let mut ctx = create_context(&ns);
    let mut txn = Transaction::new(&mut ctx, ns.clone());

    // Init returns Version::Counter(1) for new state
    let version = txn.state_init("counter", Value::I64(0)).unwrap();
    assert!(version.is_counter());
    assert_eq!(version, Version::counter(1));
}

#[test]
fn state_invariant2_cas_returns_incremented_version() {
    let run_id = test_run_id();
    let ns = create_namespace(run_id);
    let mut ctx = create_context(&ns);
    let mut txn = Transaction::new(&mut ctx, ns.clone());

    // Initialize
    let v1 = txn.state_init("counter", Value::I64(0)).unwrap();
    assert_eq!(v1, Version::counter(1));

    // CAS returns incremented version
    let v2 = txn.state_cas("counter", 1, Value::I64(1)).unwrap();
    assert_eq!(v2, Version::counter(2));

    let v3 = txn.state_cas("counter", 2, Value::I64(2)).unwrap();
    assert_eq!(v3, Version::counter(3));
}

#[test]
fn state_invariant2_version_monotonically_increasing() {
    let run_id = test_run_id();
    let ns = create_namespace(run_id);
    let mut ctx = create_context(&ns);
    let mut txn = Transaction::new(&mut ctx, ns.clone());

    // Track versions
    let v1 = txn.state_init("counter", Value::I64(0)).unwrap();

    let mut prev_version = v1.as_u64();
    for i in 1..5 {
        let v = txn.state_cas("counter", prev_version, Value::I64(i)).unwrap();
        assert!(v.as_u64() > prev_version, "Version should be monotonically increasing");
        prev_version = v.as_u64();
    }
}

// ============================================================================
// Invariant 3: Everything is Transactional
// ============================================================================

#[test]
fn state_invariant3_operations_in_transaction() {
    let run_id = test_run_id();
    let ns = create_namespace(run_id);
    let mut ctx = create_context(&ns);
    let mut txn = Transaction::new(&mut ctx, ns.clone());

    // All state operations happen within a transaction context
    txn.state_init("counter", Value::I64(0)).unwrap();
    txn.state_cas("counter", 1, Value::I64(1)).unwrap();
    let _ = txn.state_read("counter").unwrap();
    let _ = txn.state_exists("counter").unwrap();
    txn.state_delete("counter").unwrap();

    // Transaction tracks all changes
    assert!(!ctx.write_set.is_empty() || !ctx.delete_set.is_empty());
}

#[test]
fn state_invariant3_read_your_writes() {
    let run_id = test_run_id();
    let ns = create_namespace(run_id);
    let mut ctx = create_context(&ns);
    let mut txn = Transaction::new(&mut ctx, ns.clone());

    // Write within transaction
    txn.state_init("counter", Value::I64(100)).unwrap();

    // Read sees uncommitted write (read-your-writes)
    let result = txn.state_read("counter").unwrap().unwrap();
    assert_eq!(result.value.value, Value::I64(100));
}

#[test]
fn state_invariant3_cas_atomic() {
    let run_id = test_run_id();
    let ns = create_namespace(run_id);
    let mut ctx = create_context(&ns);
    let mut txn = Transaction::new(&mut ctx, ns.clone());

    // Initialize
    txn.state_init("counter", Value::I64(0)).unwrap();

    // CAS with wrong version fails atomically
    let result = txn.state_cas("counter", 99, Value::I64(100));
    assert!(result.is_err());

    // Original value unchanged
    let current = txn.state_read("counter").unwrap().unwrap();
    assert_eq!(current.value.value, Value::I64(0));
    assert_eq!(current.value.version, 1);
}

// ============================================================================
// Invariant 4: Everything has Lifecycle
// ============================================================================

#[test]
fn state_invariant4_full_lifecycle() {
    let run_id = test_run_id();
    let ns = create_namespace(run_id);
    let mut ctx = create_context(&ns);
    let mut txn = Transaction::new(&mut ctx, ns.clone());

    // Create
    let v1 = txn.state_init("counter", Value::I64(0)).unwrap();
    assert_eq!(v1, Version::counter(1));

    // Read
    let read1 = txn.state_read("counter").unwrap().unwrap();
    assert_eq!(read1.value.value, Value::I64(0));

    // Update (CAS)
    let v2 = txn.state_cas("counter", 1, Value::I64(42)).unwrap();
    assert_eq!(v2, Version::counter(2));

    // Read updated
    let read2 = txn.state_read("counter").unwrap().unwrap();
    assert_eq!(read2.value.value, Value::I64(42));

    // Delete
    let existed = txn.state_delete("counter").unwrap();
    assert!(existed);

    // Read after delete returns None
    let read3 = txn.state_read("counter").unwrap();
    assert!(read3.is_none());
}

#[test]
fn state_invariant4_init_fails_on_existing() {
    let run_id = test_run_id();
    let ns = create_namespace(run_id);
    let mut ctx = create_context(&ns);
    let mut txn = Transaction::new(&mut ctx, ns.clone());

    // Initialize once
    txn.state_init("counter", Value::I64(0)).unwrap();

    // Initialize again should fail
    let result = txn.state_init("counter", Value::I64(1));
    assert!(result.is_err());
}

#[test]
fn state_invariant4_cas_fails_on_nonexistent() {
    let run_id = test_run_id();
    let ns = create_namespace(run_id);
    let mut ctx = create_context(&ns);
    let mut txn = Transaction::new(&mut ctx, ns.clone());

    // CAS on non-existent state should fail
    let result = txn.state_cas("missing", 1, Value::I64(100));
    assert!(result.is_err());
}

#[test]
fn state_invariant4_delete_returns_existed() {
    let run_id = test_run_id();
    let ns = create_namespace(run_id);
    let mut ctx = create_context(&ns);
    let mut txn = Transaction::new(&mut ctx, ns.clone());

    // Delete non-existent returns false
    let existed1 = txn.state_delete("missing").unwrap();
    assert!(!existed1);

    // Create and delete returns true
    txn.state_init("counter", Value::I64(0)).unwrap();
    let existed2 = txn.state_delete("counter").unwrap();
    assert!(existed2);
}

// ============================================================================
// Invariant 5: Everything is Run-Scoped
// ============================================================================

#[test]
fn state_invariant5_isolated_by_run() {
    let run_id1 = test_run_id();
    let run_id2 = test_run_id();

    // Create namespaces for two different runs
    let ns1 = create_namespace(run_id1);
    let ns2 = create_namespace(run_id2);

    let mut ctx1 = create_context(&ns1);
    let mut ctx2 = create_context(&ns2);

    let mut txn1 = Transaction::new(&mut ctx1, ns1.clone());
    let mut txn2 = Transaction::new(&mut ctx2, ns2.clone());

    // Write to run1
    txn1.state_init("counter", Value::I64(100)).unwrap();

    // Write to run2 with same name
    txn2.state_init("counter", Value::I64(200)).unwrap();

    // Each run sees its own value
    let v1 = txn1.state_read("counter").unwrap().unwrap();
    let v2 = txn2.state_read("counter").unwrap().unwrap();

    assert_eq!(v1.value.value, Value::I64(100));
    assert_eq!(v2.value.value, Value::I64(200));
}

#[test]
fn state_invariant5_entity_ref_includes_run() {
    let run_id1 = test_run_id();
    let run_id2 = test_run_id();

    let ref1 = EntityRef::state(run_id1, "counter");
    let ref2 = EntityRef::state(run_id2, "counter");

    // Same name, different runs = different entities
    assert_ne!(ref1, ref2);
    assert_eq!(ref1.run_id(), run_id1);
    assert_eq!(ref2.run_id(), run_id2);
}

// ============================================================================
// Invariant 6: Everything is Introspectable
// ============================================================================

#[test]
fn state_invariant6_exists_check() {
    let run_id = test_run_id();
    let ns = create_namespace(run_id);
    let mut ctx = create_context(&ns);
    let mut txn = Transaction::new(&mut ctx, ns.clone());

    // Before creation
    assert!(!txn.state_exists("counter").unwrap());

    // After creation
    txn.state_init("counter", Value::I64(0)).unwrap();
    assert!(txn.state_exists("counter").unwrap());

    // After delete
    txn.state_delete("counter").unwrap();
    assert!(!txn.state_exists("counter").unwrap());
}

#[test]
fn state_invariant6_read_non_existent_returns_none() {
    let run_id = test_run_id();
    let ns = create_namespace(run_id);
    let mut ctx = create_context(&ns);
    let txn = Transaction::new(&mut ctx, ns.clone());

    // Reading non-existent state returns None (not error)
    let result = txn.state_read("missing").unwrap();
    assert!(result.is_none());
}

// ============================================================================
// Invariant 7: Read/Write Semantics
// ============================================================================

#[test]
fn state_invariant7_read_does_not_modify() {
    let run_id = test_run_id();
    let ns = create_namespace(run_id);
    let mut ctx = create_context(&ns);
    let mut txn = Transaction::new(&mut ctx, ns.clone());

    // Initialize
    txn.state_init("counter", Value::I64(42)).unwrap();

    // Multiple reads
    let v1 = txn.state_read("counter").unwrap().unwrap();
    let v2 = txn.state_read("counter").unwrap().unwrap();
    let v3 = txn.state_read("counter").unwrap().unwrap();

    // All reads return same version (read doesn't increment)
    assert_eq!(v1.version, v2.version);
    assert_eq!(v2.version, v3.version);
    assert_eq!(v1.value.version, 1);
}

#[test]
fn state_invariant7_write_produces_version() {
    let run_id = test_run_id();
    let ns = create_namespace(run_id);
    let mut ctx = create_context(&ns);
    let mut txn = Transaction::new(&mut ctx, ns.clone());

    // Each write produces a version
    let v1 = txn.state_init("counter", Value::I64(0)).unwrap();
    assert!(v1.as_u64() > 0);

    let v2 = txn.state_cas("counter", 1, Value::I64(1)).unwrap();
    assert!(v2.as_u64() > v1.as_u64());
}

#[test]
fn state_invariant7_version_is_counter_type() {
    let run_id = test_run_id();
    let ns = create_namespace(run_id);
    let mut ctx = create_context(&ns);
    let mut txn = Transaction::new(&mut ctx, ns.clone());

    // State uses Counter version (per-entity mutation counter)
    let version = txn.state_init("counter", Value::I64(0)).unwrap();
    assert!(version.is_counter(), "State should use Counter version type");
}

#[test]
fn state_invariant7_exists_does_not_modify() {
    let run_id = test_run_id();
    let ns = create_namespace(run_id);
    let mut ctx = create_context(&ns);
    let mut txn = Transaction::new(&mut ctx, ns.clone());

    // Initialize
    txn.state_init("counter", Value::I64(42)).unwrap();

    // Get version before exists checks
    let v1 = txn.state_read("counter").unwrap().unwrap();

    // Multiple exists calls
    let _ = txn.state_exists("counter").unwrap();
    let _ = txn.state_exists("counter").unwrap();
    let _ = txn.state_exists("counter").unwrap();

    // Version unchanged
    let v2 = txn.state_read("counter").unwrap().unwrap();
    assert_eq!(v1.value.version, v2.value.version);
}
