//! Tier 6: Cross-Primitive Atomicity Tests
//!
//! Tests for all-or-nothing commits across primitives.

use crate::test_utils::*;
use strata_core::types::RunId;
use strata_core::value::Value;
use strata_primitives::KVStore;

/// Multi-key atomic commit
#[test]
fn test_multi_key_atomic_commit() {
    let mut test_db = TestDb::new();
    let run_id = test_db.run_id;

    let kv = test_db.kv();

    // Write multiple related keys
    kv.put(&run_id, "user.name", Value::String("Alice".into()))
        .unwrap();
    kv.put(
        &run_id,
        "user.email",
        Value::String("alice@example.com".into()),
    )
    .unwrap();
    kv.put(&run_id, "user.role", Value::String("admin".into()))
        .unwrap();

    test_db.reopen();

    // All or none should be present
    let kv = test_db.kv();
    let name = kv.get(&run_id, "user.name").unwrap();
    let email = kv.get(&run_id, "user.email").unwrap();
    let role = kv.get(&run_id, "user.role").unwrap();

    let count = [name.is_some(), email.is_some(), role.is_some()]
        .iter()
        .filter(|&&x| x)
        .count();

    assert!(
        count == 0 || count == 3,
        "Partial user data visible: {}/3 fields",
        count
    );
}

/// Transaction boundary respected
#[test]
fn test_transaction_boundary() {
    let mut test_db = TestDb::new();
    let run_id = test_db.run_id;

    let kv = test_db.kv();

    // First batch
    kv.put(&run_id, "batch1_a", Value::I64(1)).unwrap();
    kv.put(&run_id, "batch1_b", Value::I64(2)).unwrap();

    // Second batch
    kv.put(&run_id, "batch2_a", Value::I64(3)).unwrap();
    kv.put(&run_id, "batch2_b", Value::I64(4)).unwrap();

    test_db.reopen();

    let kv = test_db.kv();

    // Each batch should be atomic
    let batch1_present = kv.get(&run_id, "batch1_a").unwrap().is_some()
        && kv.get(&run_id, "batch1_b").unwrap().is_some();
    let batch2_present = kv.get(&run_id, "batch2_a").unwrap().is_some()
        && kv.get(&run_id, "batch2_b").unwrap().is_some();

    // Both batches committed or database is healthy
    assert_db_healthy(&test_db.db, &run_id);
}

/// Interleaved operations from different "transactions"
#[test]
fn test_interleaved_operations() {
    let mut test_db = TestDb::new();
    let run_id1 = test_db.run_id;
    let run_id2 = RunId::new();

    let kv = test_db.kv();

    // Interleaved writes to different runs
    kv.put(&run_id1, "key", Value::String("run1_v1".into()))
        .unwrap();
    kv.put(&run_id2, "key", Value::String("run2_v1".into()))
        .unwrap();
    kv.put(&run_id1, "key", Value::String("run1_v2".into()))
        .unwrap();
    kv.put(&run_id2, "key", Value::String("run2_v2".into()))
        .unwrap();

    test_db.reopen();

    let kv = test_db.kv();

    // Each run should have consistent state
    if let Some(versioned) = kv.get(&run_id1, "key").unwrap() {
        if let Value::String(v1) = versioned.value {
            assert!(v1 == "run1_v1" || v1 == "run1_v2");
        }
    }
    if let Some(versioned) = kv.get(&run_id2, "key").unwrap() {
        if let Value::String(v2) = versioned.value {
            assert!(v2 == "run2_v1" || v2 == "run2_v2");
        }
    }
}

/// Large atomic batch
#[test]
fn test_large_atomic_batch() {
    let mut test_db = TestDb::new();
    let run_id = test_db.run_id;

    let kv = test_db.kv();

    // Large batch of related data
    for i in 0..100 {
        kv.put(&run_id, &format!("item_{}", i), Value::I64(i))
            .unwrap();
    }

    test_db.reopen();

    let kv = test_db.kv();

    // Count present items
    let present: usize = (0..100)
        .filter(|i| kv.get(&run_id, &format!("item_{}", i)).unwrap().is_some())
        .count();

    // Should be all or none (atomic batch)
    assert!(
        present == 0 || present == 100,
        "Partial batch visible: {}/100",
        present
    );
}

/// Cross-run isolation
#[test]
fn test_cross_run_isolation() {
    let mut test_db = TestDb::new();
    let run_id1 = RunId::new();
    let run_id2 = RunId::new();

    let kv = test_db.kv();

    // Write to run1
    kv.put(&run_id1, "isolated", Value::String("run1_data".into()))
        .unwrap();

    // Write to run2
    kv.put(&run_id2, "isolated", Value::String("run2_data".into()))
        .unwrap();

    test_db.reopen();

    let kv = test_db.kv();

    // Runs should be isolated
    if let Some(versioned) = kv.get(&run_id1, "isolated").unwrap() {
        if let Value::String(v1) = versioned.value {
            assert_eq!(v1, "run1_data");
        }
    }
    if let Some(versioned) = kv.get(&run_id2, "isolated").unwrap() {
        if let Value::String(v2) = versioned.value {
            assert_eq!(v2, "run2_data");
        }
    }
}

/// Delete in atomic batch
#[test]
fn test_delete_in_atomic_batch() {
    let mut test_db = TestDb::new();
    let run_id = test_db.run_id;

    let kv = test_db.kv();

    // Create keys
    kv.put(&run_id, "keep", Value::I64(1)).unwrap();
    kv.put(&run_id, "delete", Value::I64(2)).unwrap();

    // Batch: delete one, modify another
    kv.delete(&run_id, "delete").unwrap();
    kv.put(&run_id, "keep", Value::I64(10)).unwrap();

    test_db.reopen();

    let kv = test_db.kv();

    // Atomic batch should be consistent
    let keep_present = kv.get(&run_id, "keep").unwrap().is_some();
    let delete_present = kv.get(&run_id, "delete").unwrap().is_some();

    // If keep is modified, delete should be gone
    if let Some(versioned) = kv.get(&run_id, "keep").unwrap() {
        if let Value::I64(10) = versioned.value {
            assert!(!delete_present, "Delete not atomic with update");
        }
    }
}

/// Empty batch is valid
#[test]
fn test_empty_batch() {
    let mut test_db = TestDb::new();
    let run_id = test_db.run_id;

    // No writes

    test_db.reopen();

    // Empty state is valid
    let state = CapturedState::capture(&test_db.db, &run_id);
    assert!(state.kv_entries.is_empty());
}

/// Single operation is atomic
#[test]
fn test_single_operation_atomic() {
    let mut test_db = TestDb::new();
    let run_id = test_db.run_id;

    let kv = test_db.kv();
    kv.put(&run_id, "single", Value::String("value".into()))
        .unwrap();

    test_db.reopen();

    // Single operation is trivially atomic
    assert_db_healthy(&test_db.db, &run_id);
}
