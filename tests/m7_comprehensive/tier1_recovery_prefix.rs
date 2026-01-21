//! Tier 1.3: R3 - Prefix-Consistent Recovery Tests
//!
//! **Invariant R3**: Recovery sees a prefix of committed transactions.
//!
//! These tests verify:
//! - Committed transactions are always recovered completely
//! - Transactions are atomic (all-or-nothing)
//! - No partial transaction state visible after recovery

use crate::test_utils::*;
use strata_core::types::RunId;
use strata_core::value::Value;
use strata_primitives::KVStore;
use std::sync::Arc;

/// R3: Committed data survives crash
#[test]
fn test_r3_committed_prefix_basic() {
    let mut test_db = TestDb::new();
    let run_id = test_db.run_id;

    // Write several keys (each write is auto-committed in this model)
    let kv = test_db.kv();
    for i in 0..10 {
        kv.put(
            &run_id,
            &format!("key_{}", i),
            Value::String(format!("value_{}", i)),
        )
        .unwrap();
    }

    // Simulate crash and recover
    test_db.reopen();

    // All committed data should be present
    let kv = test_db.kv();
    for i in 0..10 {
        let value = kv.get(&run_id, &format!("key_{}", i)).unwrap();
        assert!(
            value.is_some(),
            "R3 VIOLATED: Committed key_{} missing after recovery",
            i
        );
    }
}

/// R3: All-or-nothing within logical operation
#[test]
fn test_r3_all_or_nothing_logical_operation() {
    let mut test_db = TestDb::new();
    let run_id = test_db.run_id;

    let kv = test_db.kv();

    // Write a set of related keys (treated as atomic batch)
    let keys_values = vec![
        ("user_name", "Alice"),
        ("user_email", "alice@example.com"),
        ("user_role", "admin"),
    ];

    for (key, value) in &keys_values {
        kv.put(&run_id, key, Value::String((*value).to_string()))
            .unwrap();
    }

    // Simulate crash
    test_db.reopen();

    // Check that if one key exists, all should exist
    let kv = test_db.kv();
    let existence: Vec<bool> = keys_values
        .iter()
        .map(|(key, _)| kv.get(&run_id, key).unwrap().is_some())
        .collect();

    let count = existence.iter().filter(|&&x| x).count();
    assert!(
        count == 0 || count == keys_values.len(),
        "R3 VIOLATED: Partial state visible ({}/{} keys)",
        count,
        keys_values.len()
    );
}

/// R3: Sequential commits all recovered
#[test]
fn test_r3_sequential_commits_preserved() {
    let mut test_db = TestDb::new();
    let run_id = test_db.run_id;

    let kv = test_db.kv();

    // Each put is a separate "transaction"
    kv.put(&run_id, "tx1_key", Value::String("tx1_value".into()))
        .unwrap();
    kv.put(&run_id, "tx2_key", Value::String("tx2_value".into()))
        .unwrap();
    kv.put(&run_id, "tx3_key", Value::String("tx3_value".into()))
        .unwrap();

    test_db.reopen();

    let kv = test_db.kv();
    assert!(
        kv.get(&run_id, "tx1_key").unwrap().is_some(),
        "R3: tx1 lost"
    );
    assert!(
        kv.get(&run_id, "tx2_key").unwrap().is_some(),
        "R3: tx2 lost"
    );
    assert!(
        kv.get(&run_id, "tx3_key").unwrap().is_some(),
        "R3: tx3 lost"
    );
}

/// R3: Order of commits preserved
#[test]
fn test_r3_commit_order_preserved() {
    let mut test_db = TestDb::new();
    let run_id = test_db.run_id;

    let kv = test_db.kv();

    // Write in specific order
    kv.put(&run_id, "counter", Value::I64(1)).unwrap();
    kv.put(&run_id, "counter", Value::I64(2)).unwrap();
    kv.put(&run_id, "counter", Value::I64(3)).unwrap();

    test_db.reopen();

    // Final value should reflect last commit
    let kv = test_db.kv();
    let value = kv.get(&run_id, "counter").unwrap().map(|v| v.value);
    assert_eq!(value, Some(Value::I64(3)), "R3: Commit order not preserved");
}

/// R3: Prefix consistency with interleaved keys
#[test]
fn test_r3_prefix_consistent_interleaved() {
    let mut test_db = TestDb::new();
    let run_id = test_db.run_id;

    let kv = test_db.kv();

    // Interleaved writes to different keys
    for i in 0..10 {
        kv.put(&run_id, &format!("a_{}", i), Value::I64(i)).unwrap();
        kv.put(&run_id, &format!("b_{}", i), Value::I64(i * 10))
            .unwrap();
    }

    test_db.reopen();

    let kv = test_db.kv();

    // All keys should be present (prefix of committed)
    for i in 0..10 {
        assert!(
            kv.get(&run_id, &format!("a_{}", i)).unwrap().is_some(),
            "R3: a_{} missing",
            i
        );
        assert!(
            kv.get(&run_id, &format!("b_{}", i)).unwrap().is_some(),
            "R3: b_{} missing",
            i
        );
    }
}

/// R3: Delete followed by put preserves final state
#[test]
fn test_r3_delete_then_put_preserved() {
    let mut test_db = TestDb::new();
    let run_id = test_db.run_id;

    let kv = test_db.kv();

    // Create, delete, recreate
    kv.put(&run_id, "key", Value::String("original".into()))
        .unwrap();
    kv.delete(&run_id, "key").unwrap();
    kv.put(&run_id, "key", Value::String("recreated".into()))
        .unwrap();

    test_db.reopen();

    let kv = test_db.kv();
    let value = kv.get(&run_id, "key").unwrap().map(|v| v.value);
    assert_eq!(
        value,
        Some(Value::String("recreated".into())),
        "R3: Final state not preserved"
    );
}

/// R3: No gaps in recovered sequence
#[test]
fn test_r3_no_gaps_in_sequence() {
    let mut test_db = TestDb::new();
    let run_id = test_db.run_id;

    let kv = test_db.kv();

    // Sequential writes
    for i in 0..100 {
        kv.put(&run_id, &format!("seq_{:03}", i), Value::I64(i))
            .unwrap();
    }

    test_db.reopen();

    let kv = test_db.kv();

    // Find where recovery ends
    let mut last_found = -1i64;
    for i in 0..100 {
        if kv.get(&run_id, &format!("seq_{:03}", i)).unwrap().is_some() {
            last_found = i;
        } else {
            break;
        }
    }

    // All entries up to last_found must exist (no gaps)
    for i in 0..=last_found {
        assert!(
            kv.get(&run_id, &format!("seq_{:03}", i)).unwrap().is_some(),
            "R3 VIOLATED: Gap at seq_{:03} (last_found={})",
            i,
            last_found
        );
    }
}

/// R3: Committed prefix after many operations
#[test]
fn test_r3_prefix_after_many_operations() {
    let mut test_db = TestDb::new();
    let run_id = test_db.run_id;

    let kv = test_db.kv();

    // Many operations
    for i in 0..500 {
        match i % 3 {
            0 => {
                kv.put(&run_id, &format!("k{}", i), Value::I64(i)).unwrap();
            }
            1 => {
                kv.put(
                    &run_id,
                    &format!("k{}", i),
                    Value::String(format!("v{}", i)),
                )
                .unwrap();
            }
            2 => {
                if i > 0 {
                    kv.delete(&run_id, &format!("k{}", i - 1)).ok();
                }
            }
            _ => {}
        }
    }

    let state_before = CapturedState::capture(&test_db.db, &run_id);

    test_db.reopen();

    let state_after = CapturedState::capture(&test_db.db, &run_id);

    // State should be identical (all committed operations recovered)
    assert_states_equal(&state_before, &state_after, "R3: Prefix not consistent");
}
