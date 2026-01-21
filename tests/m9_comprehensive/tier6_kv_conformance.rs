//! KVStore Conformance Tests
//!
//! This module verifies that the KVStore primitive conforms to all 7 invariants:
//!
//! 1. Addressable - KV entries have stable identity via EntityRef
//! 2. Versioned - Reads return Versioned<Value>, writes return Version
//! 3. Transactional - KV operations participate in transactions
//! 4. Lifecycle - KV supports full CRUD (create, read, update, delete)
//! 5. Run-scoped - KV entries are isolated by run
//! 6. Introspectable - KV has exists() check
//! 7. Read/Write - Reads don't modify, writes produce versions
//!
//! # Story #492: Invariant 1-2 Conformance Tests (KV portion)
//! # Story #493: Invariant 3-4 Conformance Tests (KV portion)
//! # Story #494: Invariant 5-6 Conformance Tests (KV portion)
//! # Story #495: Invariant 7 Conformance Tests (KV portion)

use crate::test_utils::test_run_id;
use strata_concurrency::snapshot::ClonedSnapshotView;
use strata_concurrency::TransactionContext;
use strata_core::types::{Namespace, RunId};
use strata_core::{EntityRef, PrimitiveType, Value};
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
fn kv_invariant1_has_stable_identity() {
    let run_id = test_run_id();

    // KV entry can be addressed via EntityRef
    let entity_ref = EntityRef::kv(run_id, "my-key");

    // EntityRef identifies the primitive type
    assert_eq!(entity_ref.primitive_type(), PrimitiveType::Kv);

    // EntityRef provides the run context
    assert_eq!(entity_ref.run_id(), run_id);

    // EntityRef provides the key
    assert_eq!(entity_ref.kv_key(), Some("my-key"));
}

#[test]
fn kv_invariant1_identity_stable_across_updates() {
    let run_id = test_run_id();
    let ns = create_namespace(run_id);
    let mut ctx = create_context(&ns);
    let mut txn = Transaction::new(&mut ctx, ns.clone());

    // Create the entity reference
    let entity_ref = EntityRef::kv(run_id, "key");

    // Perform an update
    txn.kv_put("key", Value::String("v1".to_string())).unwrap();
    txn.kv_put("key", Value::String("v2".to_string())).unwrap();

    // The EntityRef is still the same (identity is stable)
    let entity_ref2 = EntityRef::kv(run_id, "key");
    assert_eq!(entity_ref, entity_ref2);
}

#[test]
fn kv_invariant1_identity_can_be_stored() {
    let run_id = test_run_id();

    // EntityRef can be used as a key in collections
    let mut store: HashMap<EntityRef, String> = HashMap::new();

    let ref1 = EntityRef::kv(run_id, "key1");
    let ref2 = EntityRef::kv(run_id, "key2");

    store.insert(ref1.clone(), "value1".to_string());
    store.insert(ref2.clone(), "value2".to_string());

    // Can retrieve using the same identity
    assert_eq!(store.get(&ref1), Some(&"value1".to_string()));
    assert_eq!(store.get(&ref2), Some(&"value2".to_string()));
}

#[test]
fn kv_invariant1_identity_survives_serialization() {
    let run_id = test_run_id();
    let entity_ref = EntityRef::kv(run_id, "my-key");

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
fn kv_invariant2_read_returns_versioned() {
    let run_id = test_run_id();
    let ns = create_namespace(run_id);
    let mut ctx = create_context(&ns);
    let mut txn = Transaction::new(&mut ctx, ns.clone());

    // Write a value
    txn.kv_put("key", Value::String("value".to_string())).unwrap();

    // Read returns Versioned<Value>
    let result = txn.kv_get("key").unwrap();
    assert!(result.is_some());

    let versioned = result.unwrap();
    // Has version
    assert!(versioned.version.is_txn_id());
    // Has timestamp
    assert!(versioned.timestamp.as_micros() > 0);
    // Has value
    assert_eq!(versioned.value, Value::String("value".to_string()));
}

#[test]
fn kv_invariant2_write_returns_version() {
    let run_id = test_run_id();
    let ns = create_namespace(run_id);
    let mut ctx = create_context(&ns);
    let mut txn = Transaction::new(&mut ctx, ns.clone());

    // Write returns Version
    let version = txn.kv_put("key", Value::String("value".to_string())).unwrap();

    // Version is meaningful
    assert!(version.is_txn_id());
    assert!(version.as_u64() > 0);
}

#[test]
fn kv_invariant2_version_from_same_transaction() {
    let run_id = test_run_id();
    let ns = create_namespace(run_id);
    let mut ctx = create_context(&ns);
    let mut txn = Transaction::new(&mut ctx, ns.clone());

    // Multiple writes in same transaction get same txn_id
    let v1 = txn.kv_put("key1", Value::String("v1".to_string())).unwrap();
    let v2 = txn.kv_put("key2", Value::String("v2".to_string())).unwrap();

    // Same transaction = same TxnId (though different keys)
    assert_eq!(v1.as_u64(), v2.as_u64());
}

#[test]
fn kv_invariant2_read_version_matches_write() {
    let run_id = test_run_id();
    let ns = create_namespace(run_id);
    let mut ctx = create_context(&ns);
    let mut txn = Transaction::new(&mut ctx, ns.clone());

    // Write
    let write_version = txn.kv_put("key", Value::String("value".to_string())).unwrap();

    // Read back - version should match
    let read_result = txn.kv_get("key").unwrap().unwrap();
    assert_eq!(read_result.version, write_version);
}

// ============================================================================
// Invariant 3: Everything is Transactional
// ============================================================================

#[test]
fn kv_invariant3_participates_in_transaction() {
    let run_id = test_run_id();
    let ns = create_namespace(run_id);
    let mut ctx = create_context(&ns);

    // KV operations happen within a transaction context
    {
        let mut txn = Transaction::new(&mut ctx, ns.clone());
        txn.kv_put("key", Value::String("value".to_string())).unwrap();

        // Read-your-writes within transaction
        let result = txn.kv_get("key").unwrap();
        assert!(result.is_some());
    }
}

#[test]
fn kv_invariant3_read_your_writes() {
    let run_id = test_run_id();
    let ns = create_namespace(run_id);
    let mut ctx = create_context(&ns);
    let mut txn = Transaction::new(&mut ctx, ns.clone());

    // Write
    txn.kv_put("key", Value::String("original".to_string())).unwrap();

    // Read sees the write
    let v1 = txn.kv_get("key").unwrap().unwrap();
    assert_eq!(v1.value, Value::String("original".to_string()));

    // Update
    txn.kv_put("key", Value::String("updated".to_string())).unwrap();

    // Read sees the update
    let v2 = txn.kv_get("key").unwrap().unwrap();
    assert_eq!(v2.value, Value::String("updated".to_string()));
}

#[test]
fn kv_invariant3_delete_visible_in_transaction() {
    let run_id = test_run_id();
    let ns = create_namespace(run_id);
    let mut ctx = create_context(&ns);
    let mut txn = Transaction::new(&mut ctx, ns.clone());

    // Write then delete
    txn.kv_put("key", Value::String("value".to_string())).unwrap();
    txn.kv_delete("key").unwrap();

    // Read sees None (delete is visible)
    let result = txn.kv_get("key").unwrap();
    assert!(result.is_none());
}

// ============================================================================
// Invariant 4: Everything Has a Lifecycle
// ============================================================================

#[test]
fn kv_invariant4_full_lifecycle_crud() {
    let run_id = test_run_id();
    let ns = create_namespace(run_id);
    let mut ctx = create_context(&ns);
    let mut txn = Transaction::new(&mut ctx, ns.clone());

    // CREATE
    txn.kv_put("key", Value::String("v1".to_string())).unwrap();

    // READ (exists)
    let v = txn.kv_get("key").unwrap();
    assert!(v.is_some());
    assert_eq!(v.unwrap().value, Value::String("v1".to_string()));

    // UPDATE (evolve)
    txn.kv_put("key", Value::String("v2".to_string())).unwrap();
    let v = txn.kv_get("key").unwrap().unwrap();
    assert_eq!(v.value, Value::String("v2".to_string()));

    // DELETE (destroy)
    let deleted = txn.kv_delete("key").unwrap();
    assert!(deleted);

    // Verify destroyed
    assert!(txn.kv_get("key").unwrap().is_none());
}

#[test]
fn kv_invariant4_delete_returns_false_for_nonexistent() {
    let run_id = test_run_id();
    let ns = create_namespace(run_id);
    let mut ctx = create_context(&ns);
    let mut txn = Transaction::new(&mut ctx, ns.clone());

    // Delete non-existent key
    let deleted = txn.kv_delete("nonexistent").unwrap();
    assert!(!deleted);
}

#[test]
fn kv_invariant4_recreate_after_delete() {
    let run_id = test_run_id();
    let ns = create_namespace(run_id);
    let mut ctx = create_context(&ns);
    let mut txn = Transaction::new(&mut ctx, ns.clone());

    // Create, delete, recreate
    txn.kv_put("key", Value::String("first".to_string())).unwrap();
    txn.kv_delete("key").unwrap();
    txn.kv_put("key", Value::String("second".to_string())).unwrap();

    // Should see the new value
    let v = txn.kv_get("key").unwrap().unwrap();
    assert_eq!(v.value, Value::String("second".to_string()));
}

// ============================================================================
// Invariant 5: Everything is Run-Scoped
// ============================================================================

#[test]
fn kv_invariant5_isolated_between_runs() {
    let run1 = test_run_id();
    let run2 = test_run_id();

    let ns1 = create_namespace(run1);
    let ns2 = create_namespace(run2);

    let mut ctx1 = create_context(&ns1);
    let mut ctx2 = create_context(&ns2);

    let mut txn1 = Transaction::new(&mut ctx1, ns1.clone());
    let mut txn2 = Transaction::new(&mut ctx2, ns2.clone());

    // Write to run1
    txn1.kv_put("key", Value::String("run1-value".to_string())).unwrap();

    // Write same key to run2
    txn2.kv_put("key", Value::String("run2-value".to_string())).unwrap();

    // Each run sees its own value
    let v1 = txn1.kv_get("key").unwrap().unwrap();
    let v2 = txn2.kv_get("key").unwrap().unwrap();

    assert_eq!(v1.value, Value::String("run1-value".to_string()));
    assert_eq!(v2.value, Value::String("run2-value".to_string()));
}

#[test]
fn kv_invariant5_entity_ref_includes_run_id() {
    let run1 = test_run_id();
    let run2 = test_run_id();

    let ref1 = EntityRef::kv(run1, "same-key");
    let ref2 = EntityRef::kv(run2, "same-key");

    // Same key in different runs = different entities
    assert_ne!(ref1, ref2);
    assert_ne!(ref1.run_id(), ref2.run_id());
}

#[test]
fn kv_invariant5_list_respects_run_scope() {
    let run_id = test_run_id();
    let ns = create_namespace(run_id);
    let mut ctx = create_context(&ns);
    let mut txn = Transaction::new(&mut ctx, ns.clone());

    // Add keys
    txn.kv_put("key1", Value::String("v1".to_string())).unwrap();
    txn.kv_put("key2", Value::String("v2".to_string())).unwrap();

    // List only returns keys from this run's namespace
    let keys = txn.kv_list(None).unwrap();
    assert_eq!(keys.len(), 2);
}

// ============================================================================
// Invariant 6: Everything is Introspectable
// ============================================================================

#[test]
fn kv_invariant6_has_exists_check() {
    let run_id = test_run_id();
    let ns = create_namespace(run_id);
    let mut ctx = create_context(&ns);
    let mut txn = Transaction::new(&mut ctx, ns.clone());

    // Check existence before creation
    assert!(!txn.kv_exists("key").unwrap());

    // Create
    txn.kv_put("key", Value::String("value".to_string())).unwrap();

    // Check existence after creation
    assert!(txn.kv_exists("key").unwrap());
}

#[test]
fn kv_invariant6_exists_false_after_delete() {
    let run_id = test_run_id();
    let ns = create_namespace(run_id);
    let mut ctx = create_context(&ns);
    let mut txn = Transaction::new(&mut ctx, ns.clone());

    // Create and delete
    txn.kv_put("key", Value::String("value".to_string())).unwrap();
    txn.kv_delete("key").unwrap();

    // Exists should return false
    assert!(!txn.kv_exists("key").unwrap());
}

#[test]
fn kv_invariant6_can_list_keys() {
    let run_id = test_run_id();
    let ns = create_namespace(run_id);
    let mut ctx = create_context(&ns);
    let mut txn = Transaction::new(&mut ctx, ns.clone());

    // Add keys with different prefixes
    txn.kv_put("user:1", Value::String("alice".to_string())).unwrap();
    txn.kv_put("user:2", Value::String("bob".to_string())).unwrap();
    txn.kv_put("config:app", Value::String("settings".to_string())).unwrap();

    // Can introspect all keys
    let all_keys = txn.kv_list(None).unwrap();
    assert_eq!(all_keys.len(), 3);

    // Can introspect keys by prefix
    let user_keys = txn.kv_list(Some("user:")).unwrap();
    assert_eq!(user_keys.len(), 2);
}

// ============================================================================
// Invariant 7: Reads and Writes Have Consistent Semantics
// ============================================================================

#[test]
fn kv_invariant7_read_does_not_modify() {
    let run_id = test_run_id();
    let ns = create_namespace(run_id);
    let mut ctx = create_context(&ns);
    let mut txn = Transaction::new(&mut ctx, ns.clone());

    // Write a value
    txn.kv_put("key", Value::String("value".to_string())).unwrap();

    // Multiple reads
    let v1 = txn.kv_get("key").unwrap().unwrap();
    let v2 = txn.kv_get("key").unwrap().unwrap();
    let v3 = txn.kv_get("key").unwrap().unwrap();

    // All reads return same version (no modification)
    assert_eq!(v1.version, v2.version);
    assert_eq!(v2.version, v3.version);
    assert_eq!(v1.value, v2.value);
    assert_eq!(v2.value, v3.value);
}

#[test]
fn kv_invariant7_write_always_produces_version() {
    let run_id = test_run_id();
    let ns = create_namespace(run_id);
    let mut ctx = create_context(&ns);
    let mut txn = Transaction::new(&mut ctx, ns.clone());

    // Every write produces a version
    let v1 = txn.kv_put("key1", Value::String("v1".to_string())).unwrap();
    let v2 = txn.kv_put("key2", Value::String("v2".to_string())).unwrap();
    let v3 = txn.kv_put("key3", Value::String("v3".to_string())).unwrap();

    // All are valid versions
    assert!(v1.is_txn_id());
    assert!(v2.is_txn_id());
    assert!(v3.is_txn_id());
}

#[test]
fn kv_invariant7_overwrite_produces_same_txn_version() {
    let run_id = test_run_id();
    let ns = create_namespace(run_id);
    let mut ctx = create_context(&ns);
    let mut txn = Transaction::new(&mut ctx, ns.clone());

    // Write and overwrite in same transaction
    let v1 = txn.kv_put("key", Value::String("first".to_string())).unwrap();
    let v2 = txn.kv_put("key", Value::String("second".to_string())).unwrap();

    // Same transaction ID (within one transaction)
    assert_eq!(v1.as_u64(), v2.as_u64());
}

// ============================================================================
// Combined Invariants Test
// ============================================================================

#[test]
fn kv_all_invariants_work_together() {
    let run_id = test_run_id();
    let ns = create_namespace(run_id);
    let mut ctx = create_context(&ns);
    let mut txn = Transaction::new(&mut ctx, ns.clone());

    // Invariant 1: Addressable
    let entity_ref = EntityRef::kv(run_id, "test-key");
    assert_eq!(entity_ref.primitive_type(), PrimitiveType::Kv);

    // Invariant 4: Lifecycle - CREATE
    let version = txn.kv_put("test-key", Value::String("test-value".to_string())).unwrap();

    // Invariant 2: Versioned
    assert!(version.is_txn_id());

    // Invariant 6: Introspectable
    assert!(txn.kv_exists("test-key").unwrap());

    // Invariant 7: Read doesn't modify
    let read1 = txn.kv_get("test-key").unwrap().unwrap();
    let read2 = txn.kv_get("test-key").unwrap().unwrap();
    assert_eq!(read1.version, read2.version);

    // Invariant 3: Transactional (read-your-writes)
    assert_eq!(read1.value, Value::String("test-value".to_string()));

    // Invariant 5: Run-scoped (EntityRef includes run_id)
    assert_eq!(entity_ref.run_id(), run_id);

    // Invariant 4: Lifecycle - DELETE
    txn.kv_delete("test-key").unwrap();
    assert!(!txn.kv_exists("test-key").unwrap());
}
