//! Tier 1.5: R5 - Never Drops Committed Data Tests
//!
//! **Invariant R5**: Committed data survives any single crash.
//!
//! These tests verify:
//! - Once committed, data survives crashes
//! - fsync guarantees durability
//! - All primitives' committed data survives

use crate::test_utils::*;
use strata_core::types::RunId;
use strata_core::value::Value;
use strata_primitives::KVStore;
use std::sync::Arc;

/// R5: Committed data survives single crash
#[test]
fn test_r5_committed_survives_crash_basic() {
    let mut test_db = TestDb::new();
    let run_id = test_db.run_id;

    let kv = test_db.kv();
    kv.put(
        &run_id,
        "committed_key",
        Value::String("committed_value".into()),
    )
    .unwrap();

    // Simulate crash
    test_db.reopen();

    // Committed data must be present
    let kv = test_db.kv();
    let value = kv.get(&run_id, "committed_key").unwrap();
    assert!(
        value.is_some(),
        "R5 VIOLATED: Committed key disappeared after crash"
    );
    assert_eq!(
        value.unwrap().value,
        Value::String("committed_value".into()),
        "R5 VIOLATED: Committed value changed after crash"
    );
}

/// R5: Committed data survives multiple crashes
#[test]
fn test_r5_committed_survives_multiple_crashes() {
    let mut test_db = TestDb::new();
    let run_id = test_db.run_id;

    let kv = test_db.kv();
    for i in 0..10 {
        kv.put(
            &run_id,
            &format!("key_{}", i),
            Value::String(format!("value_{}", i)),
        )
        .unwrap();
    }

    // Simulate 5 consecutive crashes
    for crash_num in 0..5 {
        test_db.reopen();

        // All committed data must still be present
        let kv = test_db.kv();
        for i in 0..10 {
            let value = kv.get(&run_id, &format!("key_{}", i)).unwrap();
            assert!(
                value.is_some(),
                "R5 VIOLATED: Committed key_{} disappeared after crash {}",
                i,
                crash_num
            );
        }
    }
}

/// R5: Large committed dataset survives
#[test]
fn test_r5_large_committed_survives() {
    let mut test_db = TestDb::new();
    let run_id = test_db.run_id;

    let kv = test_db.kv();

    // Write large dataset
    for i in 0..500 {
        kv.put(
            &run_id,
            &format!("key_{:04}", i),
            Value::String(format!("value_{:04}", i)),
        )
        .unwrap();
    }

    test_db.reopen();

    // All must survive
    let kv = test_db.kv();
    let mut missing = 0;
    for i in 0..500 {
        if kv.get(&run_id, &format!("key_{:04}", i)).unwrap().is_none() {
            missing += 1;
        }
    }

    assert_eq!(
        missing, 0,
        "R5 VIOLATED: {} committed entries disappeared",
        missing
    );
}

/// R5: Committed values integrity after crash
#[test]
fn test_r5_committed_value_integrity() {
    let mut test_db = TestDb::new();
    let run_id = test_db.run_id;

    let kv = test_db.kv();

    // Write various value types
    kv.put(&run_id, "str", Value::String("test_string".into()))
        .unwrap();
    kv.put(&run_id, "int", Value::I64(42)).unwrap();
    kv.put(&run_id, "float", Value::F64(3.14)).unwrap();
    kv.put(&run_id, "bool", Value::Bool(true)).unwrap();

    test_db.reopen();

    let kv = test_db.kv();

    // Verify values are exactly as committed
    assert_eq!(
        kv.get(&run_id, "str").unwrap().map(|v| v.value),
        Some(Value::String("test_string".into())),
        "R5: String value corrupted"
    );
    assert_eq!(
        kv.get(&run_id, "int").unwrap().map(|v| v.value),
        Some(Value::I64(42)),
        "R5: Int value corrupted"
    );
    assert_eq!(
        kv.get(&run_id, "float").unwrap().map(|v| v.value),
        Some(Value::F64(3.14)),
        "R5: Float value corrupted"
    );
    assert_eq!(
        kv.get(&run_id, "bool").unwrap().map(|v| v.value),
        Some(Value::Bool(true)),
        "R5: Bool value corrupted"
    );
}

/// R5: All committed transactions survive
#[test]
fn test_r5_all_transactions_survive() {
    let mut test_db = TestDb::new();
    let run_id = test_db.run_id;

    let kv = test_db.kv();

    // Multiple separate "transactions"
    for batch in 0..10 {
        for i in 0..5 {
            kv.put(
                &run_id,
                &format!("batch_{}_key_{}", batch, i),
                Value::I64((batch * 5 + i) as i64),
            )
            .unwrap();
        }
    }

    test_db.reopen();

    // All batches must survive
    let kv = test_db.kv();
    for batch in 0..10 {
        for i in 0..5 {
            let key = format!("batch_{}_key_{}", batch, i);
            let value = kv.get(&run_id, &key).unwrap();
            assert!(value.is_some(), "R5 VIOLATED: {} disappeared", key);
        }
    }
}

/// R5: Later writes don't affect earlier committed data
#[test]
fn test_r5_earlier_commits_survive_later_operations() {
    let mut test_db = TestDb::new();
    let run_id = test_db.run_id;

    let kv = test_db.kv();

    // First batch - committed
    kv.put(&run_id, "early_key", Value::String("early_value".into()))
        .unwrap();

    // Many more operations
    for i in 0..100 {
        kv.put(&run_id, &format!("later_{}", i), Value::I64(i))
            .unwrap();
    }

    test_db.reopen();

    // Early commit must survive
    let kv = test_db.kv();
    let early = kv.get(&run_id, "early_key").unwrap().map(|v| v.value);
    assert_eq!(
        early,
        Some(Value::String("early_value".into())),
        "R5 VIOLATED: Early committed data lost"
    );
}

/// R5: Committed deletes are preserved
#[test]
fn test_r5_committed_deletes_preserved() {
    let mut test_db = TestDb::new();
    let run_id = test_db.run_id;

    let kv = test_db.kv();

    // Create then delete
    kv.put(&run_id, "to_delete", Value::String("original".into()))
        .unwrap();
    kv.delete(&run_id, "to_delete").unwrap();
    kv.put(&run_id, "kept", Value::String("kept_value".into()))
        .unwrap();

    test_db.reopen();

    let kv = test_db.kv();

    // Delete is committed - key should not exist
    assert!(
        kv.get(&run_id, "to_delete").unwrap().is_none(),
        "R5 VIOLATED: Committed delete not preserved"
    );

    // Kept key should exist
    assert!(
        kv.get(&run_id, "kept").unwrap().is_some(),
        "R5 VIOLATED: Kept key disappeared"
    );
}

/// R5: Sequential overwrites - final value survives
#[test]
fn test_r5_final_committed_value_survives() {
    let mut test_db = TestDb::new();
    let run_id = test_db.run_id;

    let kv = test_db.kv();

    // Multiple overwrites
    kv.put(&run_id, "counter", Value::I64(1)).unwrap();
    kv.put(&run_id, "counter", Value::I64(2)).unwrap();
    kv.put(&run_id, "counter", Value::I64(3)).unwrap();

    test_db.reopen();

    let kv = test_db.kv();
    let value = kv.get(&run_id, "counter").unwrap().map(|v| v.value);
    assert_eq!(
        value,
        Some(Value::I64(3)),
        "R5 VIOLATED: Final committed value not preserved"
    );
}

/// R5: Committed data survives stress cycles
#[test]
fn test_r5_survives_stress_cycles() {
    let mut test_db = TestDb::new();
    let run_id = test_db.run_id;

    let kv = test_db.kv();

    // Initial committed data
    for i in 0..50 {
        kv.put(&run_id, &format!("stable_{}", i), Value::I64(i))
            .unwrap();
    }

    // Stress: add more data, crash, repeat
    for cycle in 0..5 {
        // Add more data
        for i in 0..20 {
            kv.put(
                &run_id,
                &format!("cycle_{}_{}", cycle, i),
                Value::I64((cycle * 20 + i) as i64),
            )
            .unwrap();
        }

        test_db.reopen();

        // Original data must survive
        let kv = test_db.kv();
        for i in 0..50 {
            assert!(
                kv.get(&run_id, &format!("stable_{}", i)).unwrap().is_some(),
                "R5 VIOLATED: stable_{} lost at cycle {}",
                i,
                cycle
            );
        }
    }
}
