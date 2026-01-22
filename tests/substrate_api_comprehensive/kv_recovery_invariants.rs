//! KV Recovery Invariants Tests
//!
//! Tests recovery invariants through the Substrate API layer.
//! These mirror the M7 recovery invariants but test at the API boundary.
//!
//! ## Recovery Invariants (from M7 spec)
//!
//! - **R1**: Deterministic recovery - same WAL produces same state
//! - **R2**: Idempotent recovery - multiple recoveries produce identical state
//! - **R3**: May drop uncommitted - in-flight operations may be lost on crash
//! - **R4**: No drop committed - committed data must survive crash
//! - **R5**: No invent data - recovery cannot create data that was never written
//! - **R6**: Prefix consistency - recovered state is a valid prefix of operations
//!
//! ## History & Versioning Tests
//!
//! - Version history access via kv_get_at
//! - Version history enumeration via kv_history
//!
//! Note: Some tests may fail due to gaps in primitive implementation,
//! particularly around history and versioning features.
//!
//! All test data is loaded from testdata/kv_test_data.jsonl

use super::*;
use crate::test_data::load_kv_test_data;
use std::collections::{HashMap, HashSet};

// =============================================================================
// R1: DETERMINISTIC RECOVERY
// Same WAL produces same state every replay
// =============================================================================

/// R1: Basic deterministic recovery - state matches after reopen
#[test]
fn test_r1_deterministic_recovery_basic() {
    let mut test_db = TestDb::new_buffered();
    let run = ApiRunId::default();
    let test_data = load_kv_test_data();

    // Use entries from testdata
    let entries: Vec<_> = test_data.get_run(0).iter().take(3).collect();
    assert!(entries.len() >= 3, "Need at least 3 entries");

    // Write data through substrate API
    {
        let substrate = test_db.substrate();
        for entry in &entries {
            substrate.kv_put(&run, &entry.key, entry.value.clone()).unwrap();
        }
    }

    // Capture state before crash
    let state_before = capture_kv_state(&test_db.substrate(), &run);

    // Simulate crash and recovery
    test_db.reopen();

    // Capture state after recovery
    let state_after = capture_kv_state(&test_db.substrate(), &run);

    assert_eq!(
        state_before, state_after,
        "R1 VIOLATED: State differs after recovery"
    );
}

/// R1: Deterministic recovery with overwrites
#[test]
fn test_r1_deterministic_with_overwrites() {
    let mut test_db = TestDb::new_buffered();
    let run = ApiRunId::default();

    {
        let substrate = test_db.substrate();
        // Sequence of overwrites
        substrate.kv_put(&run, "key", Value::String("v1".into())).unwrap();
        substrate.kv_put(&run, "key", Value::String("v2".into())).unwrap();
        substrate.kv_put(&run, "key", Value::String("v3".into())).unwrap();
    }

    // Crash and recover
    test_db.reopen();

    // Final value must be "v3" (last write wins)
    let substrate = test_db.substrate();
    let value = substrate.kv_get(&run, "key").unwrap().map(|v| v.value);
    assert_eq!(
        value,
        Some(Value::String("v3".into())),
        "R1 VIOLATED: Operation order not preserved after recovery"
    );
}

/// R1: Deterministic recovery with interleaved puts and deletes
#[test]
fn test_r1_deterministic_interleaved_operations() {
    let mut test_db = TestDb::new_buffered();
    let run = ApiRunId::default();

    {
        let substrate = test_db.substrate();
        substrate.kv_put(&run, "a", Value::Int(1)).unwrap();
        substrate.kv_put(&run, "b", Value::Int(2)).unwrap();
        substrate.kv_delete(&run, "a").unwrap();
        substrate.kv_put(&run, "c", Value::Int(3)).unwrap();
        substrate.kv_put(&run, "a", Value::Int(4)).unwrap(); // Re-create deleted key
    }

    let state_before = capture_kv_state(&test_db.substrate(), &run);

    test_db.reopen();

    let state_after = capture_kv_state(&test_db.substrate(), &run);
    assert_eq!(
        state_before, state_after,
        "R1 VIOLATED: Interleaved operations not preserved"
    );

    // Verify specific values
    let substrate = test_db.substrate();
    assert_eq!(substrate.kv_get(&run, "a").unwrap().map(|v| v.value), Some(Value::Int(4)));
    assert_eq!(substrate.kv_get(&run, "b").unwrap().map(|v| v.value), Some(Value::Int(2)));
    assert_eq!(substrate.kv_get(&run, "c").unwrap().map(|v| v.value), Some(Value::Int(3)));
}

/// R1: Deterministic recovery with all value types
#[test]
fn test_r1_deterministic_all_value_types() {
    let mut test_db = TestDb::new_buffered();
    let run = ApiRunId::default();
    let test_data = load_kv_test_data();

    // Get one entry of each type from test data
    let value_types = ["null", "bool", "int", "float", "string", "bytes", "array", "object"];

    {
        let substrate = test_db.substrate();
        for value_type in value_types {
            if let Some(entry) = test_data.get_type(value_type).first() {
                substrate.kv_put(&run, &entry.key, entry.value.clone()).unwrap();
            }
        }
    }

    let state_before = capture_kv_state(&test_db.substrate(), &run);

    test_db.reopen();

    let state_after = capture_kv_state(&test_db.substrate(), &run);

    // Compare with special handling for floats
    assert_kv_states_equal(&state_before, &state_after, "R1 VIOLATED: Value types differ");
}

/// R1: Large dataset determinism
#[test]
fn test_r1_deterministic_large_dataset() {
    let mut test_db = TestDb::new_buffered();
    let run = ApiRunId::default();

    {
        let substrate = test_db.substrate();
        for i in 0..500 {
            substrate
                .kv_put(&run, &format!("key_{:04}", i), Value::Int(i as i64))
                .unwrap();
        }
    }

    let state_before = capture_kv_state(&test_db.substrate(), &run);

    test_db.reopen();

    let state_after = capture_kv_state(&test_db.substrate(), &run);
    assert_eq!(
        state_before.len(),
        state_after.len(),
        "R1 VIOLATED: Entry count differs"
    );
    assert_eq!(
        state_before, state_after,
        "R1 VIOLATED: Large dataset state differs"
    );
}

// =============================================================================
// R2: IDEMPOTENT RECOVERY
// Multiple recoveries produce identical state
// =============================================================================

/// R2: Multiple consecutive recoveries produce identical state
#[test]
fn test_r2_idempotent_multiple_recoveries() {
    let mut test_db = TestDb::new_buffered();
    let run = ApiRunId::default();

    {
        let substrate = test_db.substrate();
        for i in 0..50 {
            substrate
                .kv_put(&run, &format!("key_{}", i), Value::Int(i as i64))
                .unwrap();
        }
    }

    // Capture original state
    let original_state = capture_kv_state(&test_db.substrate(), &run);

    // Perform 10 consecutive recoveries
    for recovery_num in 0..10 {
        test_db.reopen();
        let recovered_state = capture_kv_state(&test_db.substrate(), &run);

        assert_eq!(
            original_state, recovered_state,
            "R2 VIOLATED: State differs after recovery #{}",
            recovery_num
        );
    }
}

/// R2: Recovery is idempotent for mixed operations
#[test]
fn test_r2_idempotent_mixed_operations() {
    let mut test_db = TestDb::new_buffered();
    let run = ApiRunId::default();

    {
        let substrate = test_db.substrate();
        // Mix of operations
        substrate.kv_put(&run, "counter", Value::Int(0)).unwrap();
        substrate.kv_incr(&run, "counter", 10).unwrap();
        substrate.kv_incr(&run, "counter", 5).unwrap();

        let entries: Vec<(&str, Value)> = vec![
            ("batch_a", Value::Int(1)),
            ("batch_b", Value::Int(2)),
        ];
        substrate.kv_mput(&run, &entries).unwrap();

        substrate.kv_delete(&run, "batch_a").unwrap();
    }

    let original_state = capture_kv_state(&test_db.substrate(), &run);

    // Multiple recoveries
    for _ in 0..5 {
        test_db.reopen();
        let state = capture_kv_state(&test_db.substrate(), &run);
        assert_eq!(original_state, state, "R2 VIOLATED: Mixed operations not idempotent");
    }
}

// =============================================================================
// R3: MAY DROP UNCOMMITTED
// In-flight operations may be lost on crash (this is acceptable)
// =============================================================================

// Note: Testing R3 properly requires the ability to crash during a write,
// which is difficult to simulate without internal hooks. These tests document
// the expected behavior.

/// R3: Documented behavior - uncommitted writes may be lost
#[test]
fn test_r3_uncommitted_may_be_lost_documented() {
    // This test documents R3 behavior rather than testing it directly.
    // In a real crash scenario:
    // 1. A write is initiated
    // 2. Crash occurs before WAL entry is fsynced
    // 3. On recovery, that write may not be present
    //
    // This is acceptable per R3 - we only guarantee committed data survives.
    //
    // The actual invariant is tested at the engine level in m7_comprehensive.

    let mut test_db = TestDb::new_strict(); // strict mode = fsync on every write
    let run = ApiRunId::default();

    {
        let substrate = test_db.substrate();
        substrate.kv_put(&run, "committed_key", Value::Int(42)).unwrap();
        // In strict mode, this write is committed (fsynced)
    }

    test_db.reopen();

    // Committed data should survive
    let substrate = test_db.substrate();
    let value = substrate.kv_get(&run, "committed_key").unwrap();
    assert!(value.is_some(), "R3: Committed data should survive in strict mode");
}

// =============================================================================
// R4: NO DROP COMMITTED
// Committed data must survive crash
// =============================================================================

/// R4: Single committed write survives crash
#[test]
fn test_r4_committed_survives_crash() {
    let mut test_db = TestDb::new_buffered();
    let run = ApiRunId::default();

    {
        let substrate = test_db.substrate();
        substrate.kv_put(&run, "important_data", Value::String("must survive".into())).unwrap();
    }

    test_db.reopen();

    let substrate = test_db.substrate();
    let value = substrate.kv_get(&run, "important_data").unwrap();
    assert!(value.is_some(), "R4 VIOLATED: Committed data lost");
    assert_eq!(
        value.unwrap().value,
        Value::String("must survive".into()),
        "R4 VIOLATED: Committed data corrupted"
    );
}

/// R4: All committed writes survive crash
#[test]
fn test_r4_all_committed_survive() {
    let mut test_db = TestDb::new_buffered();
    let run = ApiRunId::default();

    let keys: Vec<String> = (0..100).map(|i| format!("key_{}", i)).collect();

    {
        let substrate = test_db.substrate();
        for (i, key) in keys.iter().enumerate() {
            substrate.kv_put(&run, key, Value::Int(i as i64)).unwrap();
        }
    }

    test_db.reopen();

    let substrate = test_db.substrate();
    let mut missing = Vec::new();
    for (i, key) in keys.iter().enumerate() {
        match substrate.kv_get(&run, key).unwrap() {
            Some(v) if v.value == Value::Int(i as i64) => {}
            Some(v) => missing.push(format!("{}: wrong value {:?}", key, v.value)),
            None => missing.push(format!("{}: missing", key)),
        }
    }

    assert!(
        missing.is_empty(),
        "R4 VIOLATED: {} committed keys lost or corrupted: {:?}",
        missing.len(),
        &missing[..missing.len().min(10)]
    );
}

/// R4: Committed deletes survive crash
#[test]
fn test_r4_committed_deletes_survive() {
    let mut test_db = TestDb::new_buffered();
    let run = ApiRunId::default();

    {
        let substrate = test_db.substrate();
        substrate.kv_put(&run, "to_delete", Value::Int(1)).unwrap();
        substrate.kv_put(&run, "to_keep", Value::Int(2)).unwrap();
        substrate.kv_delete(&run, "to_delete").unwrap();
    }

    test_db.reopen();

    let substrate = test_db.substrate();
    assert!(
        substrate.kv_get(&run, "to_delete").unwrap().is_none(),
        "R4 VIOLATED: Committed delete not preserved"
    );
    assert!(
        substrate.kv_get(&run, "to_keep").unwrap().is_some(),
        "R4 VIOLATED: Non-deleted key missing"
    );
}

/// R4: Committed batch operations survive crash
#[test]
fn test_r4_committed_batch_survives() {
    let mut test_db = TestDb::new_buffered();
    let run = ApiRunId::default();

    {
        let substrate = test_db.substrate();
        let entries: Vec<(&str, Value)> = vec![
            ("batch_1", Value::Int(100)),
            ("batch_2", Value::Int(200)),
            ("batch_3", Value::Int(300)),
        ];
        substrate.kv_mput(&run, &entries).unwrap();
    }

    test_db.reopen();

    let substrate = test_db.substrate();
    let keys = ["batch_1", "batch_2", "batch_3"];
    let results = substrate.kv_mget(&run, &keys).unwrap();

    assert!(
        results.iter().all(|r| r.is_some()),
        "R4 VIOLATED: Committed batch partially lost"
    );
}

/// R4: Committed incr survives crash
#[test]
fn test_r4_committed_incr_survives() {
    let mut test_db = TestDb::new_buffered();
    let run = ApiRunId::default();

    {
        let substrate = test_db.substrate();
        substrate.kv_incr(&run, "counter", 10).unwrap();
        substrate.kv_incr(&run, "counter", 20).unwrap();
        substrate.kv_incr(&run, "counter", 30).unwrap();
    }

    test_db.reopen();

    let substrate = test_db.substrate();
    let value = substrate.kv_get(&run, "counter").unwrap();
    assert_eq!(
        value.map(|v| v.value),
        Some(Value::Int(60)),
        "R4 VIOLATED: Committed incr value lost"
    );
}

// =============================================================================
// R5: NO INVENT DATA
// Recovery cannot create data that was never written
// =============================================================================

/// R5: Empty database stays empty after recovery
#[test]
fn test_r5_empty_stays_empty() {
    let mut test_db = TestDb::new_buffered();
    let run = ApiRunId::default();

    // Don't write anything

    test_db.reopen();

    // Check that no spurious keys exist
    let substrate = test_db.substrate();
    let test_keys = ["key", "data", "value", "test", "spurious"];
    for key in test_keys {
        assert!(
            substrate.kv_get(&run, key).unwrap().is_none(),
            "R5 VIOLATED: Spurious key '{}' appeared after recovery",
            key
        );
    }
}

/// R5: Only written keys exist after recovery
#[test]
fn test_r5_only_written_keys_exist() {
    let mut test_db = TestDb::new_buffered();
    let run = ApiRunId::default();

    let written_keys: HashSet<String> = (0..20).map(|i| format!("written_{}", i)).collect();

    {
        let substrate = test_db.substrate();
        for key in &written_keys {
            substrate.kv_put(&run, key, Value::Int(1)).unwrap();
        }
    }

    test_db.reopen();

    let substrate = test_db.substrate();

    // All written keys should exist
    for key in &written_keys {
        assert!(
            substrate.kv_get(&run, key).unwrap().is_some(),
            "R5: Written key '{}' should exist",
            key
        );
    }

    // Unwritten keys should not exist
    let unwritten_keys = ["never_written", "invented", "spurious_123"];
    for key in unwritten_keys {
        assert!(
            substrate.kv_get(&run, key).unwrap().is_none(),
            "R5 VIOLATED: Unwritten key '{}' exists after recovery",
            key
        );
    }
}

/// R5: Deleted keys don't reappear after recovery
#[test]
fn test_r5_deleted_keys_stay_deleted() {
    let mut test_db = TestDb::new_buffered();
    let run = ApiRunId::default();

    {
        let substrate = test_db.substrate();
        substrate.kv_put(&run, "ghost_key", Value::String("boo".into())).unwrap();
        substrate.kv_delete(&run, "ghost_key").unwrap();
    }

    // Multiple recoveries
    for _ in 0..5 {
        test_db.reopen();

        let substrate = test_db.substrate();
        assert!(
            substrate.kv_get(&run, "ghost_key").unwrap().is_none(),
            "R5 VIOLATED: Deleted key reappeared after recovery"
        );
    }
}

// =============================================================================
// R6: PREFIX CONSISTENCY
// Recovered state is a valid prefix of operations
// =============================================================================

/// R6: No gaps in sequential writes after recovery
#[test]
fn test_r6_prefix_no_gaps() {
    let mut test_db = TestDb::new_buffered();
    let run = ApiRunId::default();

    {
        let substrate = test_db.substrate();
        for i in 0..100 {
            substrate
                .kv_put(&run, &format!("seq_{:03}", i), Value::Int(i as i64))
                .unwrap();
        }
    }

    test_db.reopen();

    let substrate = test_db.substrate();

    // Find first missing key
    let mut first_missing: Option<usize> = None;
    let mut found_after_missing = Vec::new();

    for i in 0..100 {
        let exists = substrate
            .kv_get(&run, &format!("seq_{:03}", i))
            .unwrap()
            .is_some();

        if !exists && first_missing.is_none() {
            first_missing = Some(i);
        } else if exists && first_missing.is_some() {
            found_after_missing.push(i);
        }
    }

    assert!(
        found_after_missing.is_empty(),
        "R6 VIOLATED: Gap detected - missing starts at {:?}, but found keys at {:?}",
        first_missing,
        found_after_missing
    );
}

/// R6: Prefix consistency with mixed operations
#[test]
fn test_r6_prefix_mixed_operations() {
    let mut test_db = TestDb::new_buffered();
    let run = ApiRunId::default();

    {
        let substrate = test_db.substrate();

        // Sequence of operations with dependencies
        substrate.kv_put(&run, "step_1", Value::Int(1)).unwrap();
        substrate.kv_put(&run, "step_2", Value::Int(2)).unwrap();
        substrate.kv_incr(&run, "counter", 10).unwrap();
        substrate.kv_put(&run, "step_3", Value::Int(3)).unwrap();
        substrate.kv_incr(&run, "counter", 20).unwrap();
        substrate.kv_put(&run, "step_4", Value::Int(4)).unwrap();
    }

    test_db.reopen();

    let substrate = test_db.substrate();

    // Check for consistent prefix - if step_N exists, step_N-1 must exist
    let steps = [
        ("step_1", None),
        ("step_2", Some("step_1")),
        ("step_3", Some("step_2")),
        ("step_4", Some("step_3")),
    ];

    for (step, prerequisite) in steps {
        let step_exists = substrate.kv_get(&run, step).unwrap().is_some();
        if let Some(prereq) = prerequisite {
            let prereq_exists = substrate.kv_get(&run, prereq).unwrap().is_some();
            if step_exists {
                assert!(
                    prereq_exists,
                    "R6 VIOLATED: {} exists but prerequisite {} does not",
                    step,
                    prereq
                );
            }
        }
    }
}

// =============================================================================
// VERSION HISTORY TESTS
// These test kv_get_at and kv_history - may fail due to implementation gaps
// =============================================================================

/// Version history: kv_get_at retrieves value at specific version
#[test]
fn test_version_history_get_at() {
    let (_, substrate) = quick_setup();
    let run = ApiRunId::default();

    // Write sequence of values, capturing versions
    let v1 = substrate.kv_put(&run, "versioned_key", Value::Int(1)).unwrap();
    let v2 = substrate.kv_put(&run, "versioned_key", Value::Int(2)).unwrap();
    let v3 = substrate.kv_put(&run, "versioned_key", Value::Int(3)).unwrap();

    // Get value at each version
    let at_v1 = substrate.kv_get_at(&run, "versioned_key", v1);
    let at_v2 = substrate.kv_get_at(&run, "versioned_key", v2);
    let at_v3 = substrate.kv_get_at(&run, "versioned_key", v3);

    // Verify each version returns correct value
    assert!(at_v1.is_ok(), "kv_get_at(v1) should succeed");
    assert_eq!(at_v1.unwrap().value, Value::Int(1), "v1 should return Int(1)");

    assert!(at_v2.is_ok(), "kv_get_at(v2) should succeed");
    assert_eq!(at_v2.unwrap().value, Value::Int(2), "v2 should return Int(2)");

    assert!(at_v3.is_ok(), "kv_get_at(v3) should succeed");
    assert_eq!(at_v3.unwrap().value, Value::Int(3), "v3 should return Int(3)");
}

/// Version history: kv_get_at survives crash
#[test]
fn test_version_history_survives_crash() {
    let mut test_db = TestDb::new_buffered();
    let run = ApiRunId::default();

    let (v1, v2, v3) = {
        let substrate = test_db.substrate();
        let v1 = substrate.kv_put(&run, "versioned", Value::Int(100)).unwrap();
        let v2 = substrate.kv_put(&run, "versioned", Value::Int(200)).unwrap();
        let v3 = substrate.kv_put(&run, "versioned", Value::Int(300)).unwrap();
        (v1, v2, v3)
    };

    test_db.reopen();

    let substrate = test_db.substrate();

    // All versions should be accessible after crash
    let at_v1 = substrate.kv_get_at(&run, "versioned", v1);
    let at_v2 = substrate.kv_get_at(&run, "versioned", v2);
    let at_v3 = substrate.kv_get_at(&run, "versioned", v3);

    assert!(at_v1.is_ok(), "Version history should survive crash (v1)");
    assert!(at_v2.is_ok(), "Version history should survive crash (v2)");
    assert!(at_v3.is_ok(), "Version history should survive crash (v3)");
}

/// Version history: kv_history enumerates all versions
#[test]
fn test_version_history_enumeration() {
    let (_, substrate) = quick_setup();
    let run = ApiRunId::default();

    // Write multiple versions
    substrate.kv_put(&run, "history_key", Value::Int(1)).unwrap();
    substrate.kv_put(&run, "history_key", Value::Int(2)).unwrap();
    substrate.kv_put(&run, "history_key", Value::Int(3)).unwrap();

    // Get history
    let history = substrate.kv_history(&run, "history_key", None, None);

    assert!(history.is_ok(), "kv_history should succeed");
    let entries = history.unwrap();

    assert_eq!(entries.len(), 3, "Should have 3 history entries");

    // Verify values in order (oldest to newest or newest to oldest)
    let values: Vec<i64> = entries
        .iter()
        .filter_map(|v| match &v.value {
            Value::Int(n) => Some(*n),
            _ => None,
        })
        .collect();

    // Either ascending or descending order is acceptable
    assert!(
        values == vec![1, 2, 3] || values == vec![3, 2, 1],
        "History should be in order, got: {:?}",
        values
    );
}

// =============================================================================
// RUN ISOLATION RECOVERY TESTS
// =============================================================================

/// Run isolation survives crash
#[test]
fn test_run_isolation_survives_crash() {
    let mut test_db = TestDb::new_buffered();
    let run1 = ApiRunId::default();
    let run2 = ApiRunId::new();

    {
        let substrate = test_db.substrate();
        substrate.kv_put(&run1, "shared_key", Value::Int(111)).unwrap();
        substrate.kv_put(&run2, "shared_key", Value::Int(222)).unwrap();
    }

    test_db.reopen();

    let substrate = test_db.substrate();
    let v1 = substrate.kv_get(&run1, "shared_key").unwrap().map(|v| v.value);
    let v2 = substrate.kv_get(&run2, "shared_key").unwrap().map(|v| v.value);

    assert_eq!(v1, Some(Value::Int(111)), "Run1 isolation should survive crash");
    assert_eq!(v2, Some(Value::Int(222)), "Run2 isolation should survive crash");
}

/// Multiple runs with interleaved operations survive crash
#[test]
fn test_multiple_runs_interleaved_survive_crash() {
    let mut test_db = TestDb::new_buffered();
    let runs: Vec<ApiRunId> = (0..5).map(|i| {
        if i == 0 { ApiRunId::default() } else { ApiRunId::new() }
    }).collect();

    {
        let substrate = test_db.substrate();
        // Interleave operations across runs
        for i in 0..20 {
            let run = &runs[i % runs.len()];
            substrate
                .kv_put(run, &format!("key_{}", i), Value::Int(i as i64))
                .unwrap();
        }
    }

    test_db.reopen();

    let substrate = test_db.substrate();
    for i in 0..20 {
        let run = &runs[i % runs.len()];
        let value = substrate.kv_get(run, &format!("key_{}", i)).unwrap();
        assert!(
            value.is_some(),
            "Run {} key_{} should survive crash",
            i % runs.len(),
            i
        );
        assert_eq!(value.unwrap().value, Value::Int(i as i64));
    }
}

// =============================================================================
// CROSS-MODE RECOVERY EQUIVALENCE
// =============================================================================

/// Recovery produces same results in buffered and strict modes
#[test]
fn test_recovery_cross_mode_equivalence() {
    fn run_workload(test_db: &TestDb, run: &ApiRunId) {
        let substrate = test_db.substrate();
        substrate.kv_put(run, "a", Value::Int(1)).unwrap();
        substrate.kv_put(run, "b", Value::Int(2)).unwrap();
        substrate.kv_put(run, "a", Value::Int(10)).unwrap();
        substrate.kv_delete(run, "b").unwrap();
        substrate.kv_incr(run, "counter", 5).unwrap();
    }

    // Buffered mode
    let mut buffered_db = TestDb::new_buffered();
    let run = ApiRunId::default();
    run_workload(&buffered_db, &run);
    buffered_db.reopen();
    let buffered_state = capture_kv_state(&buffered_db.substrate(), &run);

    // Strict mode
    let mut strict_db = TestDb::new_strict();
    run_workload(&strict_db, &run);
    strict_db.reopen();
    let strict_state = capture_kv_state(&strict_db.substrate(), &run);

    assert_eq!(
        buffered_state, strict_state,
        "Recovery should produce identical state in buffered and strict modes"
    );
}

// =============================================================================
// HELPER FUNCTIONS
// =============================================================================

/// Capture current KV state as a HashMap for comparison
fn capture_kv_state(substrate: &SubstrateImpl, run: &ApiRunId) -> HashMap<String, Value> {
    // Since we can't list keys through substrate API, we'll check known keys
    // In real tests, you'd want a kv_scan or similar operation
    let mut state = HashMap::new();

    // Check various key patterns that tests use
    let key_patterns: Vec<String> = {
        let mut keys = Vec::new();

        // Basic keys
        for key in ["key", "key1", "key2", "key3", "a", "b", "c", "counter"] {
            keys.push(key.to_string());
        }

        // Indexed keys
        for prefix in ["key_", "seq_", "written_", "batch_", "step_"] {
            for i in 0..500 {
                keys.push(format!("{}{:03}", prefix, i));
                keys.push(format!("{}{:04}", prefix, i));
                keys.push(format!("{}{}", prefix, i));
            }
        }

        // Test value keys
        for (key, _) in standard_test_values() {
            keys.push(key.to_string());
        }

        keys
    };

    for key in key_patterns {
        if let Ok(Some(versioned)) = substrate.kv_get(run, &key) {
            state.insert(key, versioned.value);
        }
    }

    state
}

/// Compare two KV states with special handling for float NaN
fn assert_kv_states_equal(
    state1: &HashMap<String, Value>,
    state2: &HashMap<String, Value>,
    message: &str,
) {
    assert_eq!(state1.len(), state2.len(), "{}: entry count differs", message);

    for (key, value1) in state1 {
        let value2 = state2.get(key).expect(&format!("{}: key '{}' missing in state2", message, key));
        assert!(
            values_equal(value1, value2),
            "{}: key '{}' value differs: {:?} vs {:?}",
            message,
            key,
            value1,
            value2
        );
    }
}
