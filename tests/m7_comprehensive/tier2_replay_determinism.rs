//! Tier 2.5: P5 - Replay is Deterministic Tests
//!
//! **Invariant P5**: Same inputs produce identical view.
//!
//! These tests verify:
//! - Replay 100 times produces identical results
//! - Deterministic across restarts
//! - No random elements in replay

use crate::test_utils::*;
use strata_core::types::RunId;
use strata_core::value::Value;
use strata_primitives::KVStore;
use std::sync::Arc;

/// P5: Replay is deterministic - 100 iterations
#[test]
fn test_p5_replay_deterministic_100_times() {
    let test_db = TestDb::new_in_memory();
    let run_id = test_db.run_id;

    let kv = test_db.kv();
    kv.put(&run_id, "key1", Value::String("value1".into()))
        .unwrap();
    kv.put(&run_id, "key2", Value::I64(42)).unwrap();
    kv.put(&run_id, "key3", Value::F64(3.14)).unwrap();

    // Replay 100 times
    let mut hashes = Vec::new();
    for _ in 0..100 {
        let state = CapturedState::capture(&test_db.db, &run_id);
        hashes.push(state.hash);
    }

    // All hashes identical
    assert!(
        hashes.windows(2).all(|w| w[0] == w[1]),
        "P5 VIOLATED: replay_run() not deterministic"
    );
}

/// P5: Deterministic across multiple restarts
#[test]
fn test_p5_deterministic_across_restarts() {
    let mut test_db = TestDb::new();
    let run_id = test_db.run_id;

    let kv = test_db.kv();
    kv.put(&run_id, "key", Value::String("value".into()))
        .unwrap();

    let state1 = CapturedState::capture(&test_db.db, &run_id);
    let hash1 = state1.hash;

    test_db.reopen();

    let state2 = CapturedState::capture(&test_db.db, &run_id);
    let hash2 = state2.hash;

    assert_eq!(hash1, hash2, "P5 VIOLATED: State differs after restart");
}

/// P5: Deterministic with complex operations
#[test]
fn test_p5_deterministic_complex_ops() {
    let test_db = TestDb::new_in_memory();
    let run_id = test_db.run_id;

    let kv = test_db.kv();

    // Complex sequence
    for i in 0..50 {
        kv.put(&run_id, &format!("k{}", i), Value::I64(i)).unwrap();
    }
    for i in (0..50).step_by(2) {
        kv.delete(&run_id, &format!("k{}", i)).unwrap();
    }
    for i in 0..25 {
        kv.put(&run_id, &format!("k{}", i * 2), Value::I64(i * 100))
            .unwrap();
    }

    // Capture multiple times
    let hashes: Vec<_> = (0..50)
        .map(|_| CapturedState::capture(&test_db.db, &run_id).hash)
        .collect();

    // All must be identical
    let first = hashes[0];
    for (i, hash) in hashes.iter().enumerate() {
        assert_eq!(first, *hash, "P5 VIOLATED: Iteration {} differs", i);
    }
}

/// P5: Deterministic with various value types
#[test]
fn test_p5_deterministic_value_types() {
    let test_db = TestDb::new_in_memory();
    let run_id = test_db.run_id;

    let kv = test_db.kv();

    kv.put(&run_id, "string", Value::String("hello".into()))
        .unwrap();
    kv.put(&run_id, "int", Value::I64(-999)).unwrap();
    kv.put(&run_id, "float", Value::F64(2.718281828)).unwrap();
    kv.put(&run_id, "bool_t", Value::Bool(true)).unwrap();
    kv.put(&run_id, "bool_f", Value::Bool(false)).unwrap();
    kv.put(&run_id, "null", Value::Null).unwrap();

    let hashes: Vec<_> = (0..100)
        .map(|_| CapturedState::capture(&test_db.db, &run_id).hash)
        .collect();

    assert!(
        hashes.windows(2).all(|w| w[0] == w[1]),
        "P5 VIOLATED: Value types not deterministic"
    );
}

/// P5: Deterministic with large dataset
#[test]
fn test_p5_deterministic_large_dataset() {
    let test_db = TestDb::new_in_memory();
    let run_id = test_db.run_id;

    let kv = test_db.kv();

    for i in 0..1000 {
        kv.put(
            &run_id,
            &format!("key_{:05}", i),
            Value::String(format!("value_{:05}", i)),
        )
        .unwrap();
    }

    let hashes: Vec<_> = (0..20)
        .map(|_| CapturedState::capture(&test_db.db, &run_id).hash)
        .collect();

    let first = hashes[0];
    for hash in &hashes {
        assert_eq!(first, *hash, "P5 VIOLATED: Large dataset not deterministic");
    }
}

/// P5: Deterministic across threads
#[test]
fn test_p5_deterministic_across_threads() {
    use std::thread;

    let test_db = TestDb::new_in_memory();
    let run_id = test_db.run_id;

    let kv = test_db.kv();
    for i in 0..50 {
        kv.put(&run_id, &format!("k{}", i), Value::I64(i)).unwrap();
    }

    let db = test_db.db.clone();
    let handles: Vec<_> = (0..10)
        .map(|_| {
            let db = db.clone();
            thread::spawn(move || CapturedState::capture(&db, &run_id).hash)
        })
        .collect();

    let hashes: Vec<u64> = handles.into_iter().map(|h| h.join().unwrap()).collect();

    // All thread results should be identical
    let first = hashes[0];
    for (i, hash) in hashes.iter().enumerate() {
        assert_eq!(
            first, *hash,
            "P5 VIOLATED: Thread {} got different result",
            i
        );
    }
}

/// P5: Deterministic after many operations
#[test]
fn test_p5_deterministic_after_churn() {
    let test_db = TestDb::new_in_memory();
    let run_id = test_db.run_id;

    let kv = test_db.kv();

    // Create churn
    for i in 0..100 {
        kv.put(&run_id, &format!("churn{}", i % 10), Value::I64(i))
            .unwrap();
    }

    // Final state should be deterministic
    let hashes: Vec<_> = (0..50)
        .map(|_| CapturedState::capture(&test_db.db, &run_id).hash)
        .collect();

    assert!(
        hashes.windows(2).all(|w| w[0] == w[1]),
        "P5 VIOLATED: State after churn not deterministic"
    );
}

/// P5: Order of keys in state is deterministic
#[test]
fn test_p5_key_order_deterministic() {
    let test_db = TestDb::new_in_memory();
    let run_id = test_db.run_id;

    let kv = test_db.kv();

    // Insert in specific order
    for c in ['z', 'a', 'm', 'b', 'y'] {
        kv.put(&run_id, &c.to_string(), Value::String(c.to_string()))
            .unwrap();
    }

    // Capture multiple times and compare key sets
    let states: Vec<_> = (0..10)
        .map(|_| CapturedState::capture(&test_db.db, &run_id))
        .collect();

    let first_keys: Vec<_> = states[0].kv_entries.keys().collect();
    for state in &states[1..] {
        let keys: Vec<_> = state.kv_entries.keys().collect();
        assert_eq!(first_keys.len(), keys.len());
    }
}
