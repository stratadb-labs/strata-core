//! KV Durability Tests
//!
//! Tests for durability guarantees:
//! - Crash recovery: data survives database close/reopen
//! - Persistence modes: in_memory vs buffered vs strict
//! - Version/timestamp preservation
//! - Prefix consistency (no gaps after recovery)
//! - Large dataset survival
//! - Stress cycles
//!
//! All test data is loaded from testdata/kv_test_data.jsonl

use super::*;
use crate::test_data::load_kv_test_data;

// =============================================================================
// BASIC CRASH RECOVERY
// =============================================================================

#[test]
fn test_buffered_crash_recovery() {
    let mut test_db = TestDb::new_buffered();
    let run = ApiRunId::default();
    let test_data = load_kv_test_data();

    let entry = test_data.get_type("string").first().expect("Need string entry");

    // Write data
    {
        let substrate = test_db.substrate();
        substrate
            .kv_put(&run, &entry.key, entry.value.clone())
            .expect("Put should succeed");
    }

    // Simulate crash
    test_db.reopen();

    // Verify data survived
    let substrate = test_db.substrate();
    let result = substrate.kv_get(&run, &entry.key).unwrap();
    assert!(result.is_some(), "Key should survive crash in buffered mode");
    assert!(values_equal(&result.unwrap().value, &entry.value));
}

#[test]
fn test_strict_crash_recovery() {
    let mut test_db = TestDb::new_strict();
    let run = ApiRunId::default();
    let test_data = load_kv_test_data();

    let entry = test_data.get_type("string").first().expect("Need string entry");

    // Write data
    {
        let substrate = test_db.substrate();
        substrate
            .kv_put(&run, &entry.key, entry.value.clone())
            .expect("Put should succeed");
    }

    // Simulate crash
    test_db.reopen();

    // Verify data survived
    let substrate = test_db.substrate();
    let result = substrate.kv_get(&run, &entry.key).unwrap();
    assert!(result.is_some(), "Key should survive crash in strict mode");
    assert!(values_equal(&result.unwrap().value, &entry.value));
}

#[test]
fn test_in_memory_no_persistence() {
    let mut test_db = TestDb::new_in_memory();
    let run = ApiRunId::default();
    let test_data = load_kv_test_data();

    let entry = test_data.get_type("int").first().expect("Need int entry");

    // Write data
    {
        let substrate = test_db.substrate();
        substrate
            .kv_put(&run, &entry.key, entry.value.clone())
            .expect("Put should succeed");

        // Verify it exists
        assert!(substrate.kv_get(&run, &entry.key).unwrap().is_some());
    }

    // Simulate crash (creates fresh in-memory db)
    test_db.reopen();

    // Data should be gone (expected for in-memory)
    let substrate = test_db.substrate();
    let result = substrate.kv_get(&run, &entry.key).unwrap();
    assert!(
        result.is_none(),
        "In-memory data should not persist across restart"
    );
}

// =============================================================================
// MULTIPLE CRASHES
// =============================================================================

#[test]
fn test_survives_multiple_crashes_buffered() {
    let mut test_db = TestDb::new_buffered();
    let run = ApiRunId::default();

    // Write initial data
    {
        let substrate = test_db.substrate();
        for i in 0..10 {
            substrate
                .kv_put(&run, &format!("key_{}", i), Value::Int(i as i64))
                .unwrap();
        }
    }

    // Simulate 5 consecutive crashes
    for crash_num in 0..5 {
        test_db.reopen();

        let substrate = test_db.substrate();
        for i in 0..10 {
            let result = substrate.kv_get(&run, &format!("key_{}", i)).unwrap();
            assert!(
                result.is_some(),
                "key_{} should survive crash {}",
                i,
                crash_num
            );
            assert_eq!(
                result.unwrap().value,
                Value::Int(i as i64),
                "key_{} should have correct value after crash {}",
                i,
                crash_num
            );
        }
    }
}

#[test]
fn test_survives_multiple_crashes_strict() {
    let mut test_db = TestDb::new_strict();
    let run = ApiRunId::default();

    // Write initial data
    {
        let substrate = test_db.substrate();
        for i in 0..10 {
            substrate
                .kv_put(&run, &format!("key_{}", i), Value::Int(i as i64))
                .unwrap();
        }
    }

    // Simulate 5 consecutive crashes
    for _ in 0..5 {
        test_db.reopen();

        let substrate = test_db.substrate();
        for i in 0..10 {
            let result = substrate.kv_get(&run, &format!("key_{}", i)).unwrap();
            assert!(result.is_some());
            assert_eq!(result.unwrap().value, Value::Int(i as i64));
        }
    }
}

// =============================================================================
// LARGE DATASET
// =============================================================================

#[test]
fn test_large_dataset_survives_buffered() {
    let mut test_db = TestDb::new_buffered();
    let run = ApiRunId::default();

    const ENTRY_COUNT: usize = 500;

    // Write large dataset
    {
        let substrate = test_db.substrate();
        for i in 0..ENTRY_COUNT {
            substrate
                .kv_put(
                    &run,
                    &format!("large_key_{:04}", i),
                    Value::String(format!("large_value_{:04}", i)),
                )
                .unwrap();
        }
    }

    // Crash
    test_db.reopen();

    // Verify all survived
    let substrate = test_db.substrate();
    let mut missing = 0;
    for i in 0..ENTRY_COUNT {
        if substrate
            .kv_get(&run, &format!("large_key_{:04}", i))
            .unwrap()
            .is_none()
        {
            missing += 1;
        }
    }

    assert_eq!(
        missing, 0,
        "All {} entries should survive crash",
        ENTRY_COUNT
    );
}

// =============================================================================
// VALUE INTEGRITY
// =============================================================================

#[test]
fn test_all_value_types_survive_crash() {
    let test_data = load_kv_test_data();

    for mode in ["buffered", "strict"] {
        let mut test_db = if mode == "buffered" {
            TestDb::new_buffered()
        } else {
            TestDb::new_strict()
        };
        let run = ApiRunId::default();

        // Get one entry of each type from test data
        let value_types = ["null", "bool", "int", "float", "string", "bytes", "array", "object"];
        let entries: Vec<_> = value_types.iter()
            .filter_map(|t| test_data.get_type(t).first())
            .collect();

        // Write all values
        {
            let substrate = test_db.substrate();
            for entry in &entries {
                substrate.kv_put(&run, &entry.key, entry.value.clone()).unwrap();
            }
        }

        // Crash
        test_db.reopen();

        // Verify all values
        let substrate = test_db.substrate();
        for entry in &entries {
            let result = substrate.kv_get(&run, &entry.key).unwrap();
            assert!(result.is_some(), "{}: '{}' should survive crash", mode, entry.key);

            let actual = result.unwrap().value;
            assert!(
                values_equal(&actual, &entry.value),
                "{}: '{}' value mismatch: expected {:?}, got {:?}",
                mode,
                entry.key,
                entry.value,
                actual
            );
        }
    }
}

// =============================================================================
// DELETE PERSISTENCE
// =============================================================================

#[test]
fn test_delete_persists_after_crash() {
    let test_data = load_kv_test_data();
    let entries: Vec<_> = test_data.get_run(0).iter().take(2).collect();
    assert!(entries.len() >= 2, "Need at least 2 entries");

    for mode in ["buffered", "strict"] {
        let mut test_db = if mode == "buffered" {
            TestDb::new_buffered()
        } else {
            TestDb::new_strict()
        };
        let run = ApiRunId::default();

        // Create then delete
        {
            let substrate = test_db.substrate();
            substrate
                .kv_put(&run, &entries[0].key, entries[0].value.clone())
                .unwrap();
            substrate
                .kv_put(&run, &entries[1].key, entries[1].value.clone())
                .unwrap();
            substrate.kv_delete(&run, &entries[0].key).unwrap();
        }

        // Crash
        test_db.reopen();

        // Verify delete was preserved
        let substrate = test_db.substrate();
        assert!(
            substrate.kv_get(&run, &entries[0].key).unwrap().is_none(),
            "{}: deleted key should stay deleted",
            mode
        );
        assert!(
            substrate.kv_get(&run, &entries[1].key).unwrap().is_some(),
            "{}: kept key should still exist",
            mode
        );
    }
}

// =============================================================================
// OVERWRITE PERSISTENCE
// =============================================================================

#[test]
fn test_final_overwrite_survives_crash() {
    let test_data = load_kv_test_data();
    let int_entries: Vec<_> = test_data.get_type("int").iter().take(3).collect();
    let string_entry = test_data.get_type("string").first().expect("Need string entry");
    assert!(int_entries.len() >= 3, "Need at least 3 int entries");

    for mode in ["buffered", "strict"] {
        let mut test_db = if mode == "buffered" {
            TestDb::new_buffered()
        } else {
            TestDb::new_strict()
        };
        let run = ApiRunId::default();

        // Multiple overwrites using different values from test data
        {
            let substrate = test_db.substrate();
            substrate
                .kv_put(&run, "durability_overwrite_key", int_entries[0].value.clone())
                .unwrap();
            substrate
                .kv_put(&run, "durability_overwrite_key", int_entries[1].value.clone())
                .unwrap();
            substrate
                .kv_put(&run, "durability_overwrite_key", int_entries[2].value.clone())
                .unwrap();
            substrate
                .kv_put(&run, "durability_overwrite_key", string_entry.value.clone())
                .unwrap();
        }

        // Crash
        test_db.reopen();

        // Verify final value
        let substrate = test_db.substrate();
        let value = substrate
            .kv_get(&run, "durability_overwrite_key")
            .unwrap()
            .unwrap()
            .value;
        assert!(
            values_equal(&value, &string_entry.value),
            "{}: final overwritten value should survive",
            mode
        );
    }
}

// =============================================================================
// VERSION PRESERVATION
// =============================================================================

#[test]
fn test_version_preserved_after_crash() {
    let test_data = load_kv_test_data();
    let entry = test_data.get_type("int").first().expect("Need int entry");

    for mode in ["buffered", "strict"] {
        let mut test_db = if mode == "buffered" {
            TestDb::new_buffered()
        } else {
            TestDb::new_strict()
        };
        let run = ApiRunId::default();

        // Write and capture version
        let version_before = {
            let substrate = test_db.substrate();
            substrate
                .kv_put(&run, &entry.key, entry.value.clone())
                .expect("Put should succeed")
        };

        // Crash
        test_db.reopen();

        // Verify version preserved
        let substrate = test_db.substrate();
        let versioned = substrate
            .kv_get(&run, &entry.key)
            .unwrap()
            .unwrap();

        assert_eq!(
            versioned.version, version_before,
            "{}: version should be preserved after crash",
            mode
        );
        assert!(values_equal(&versioned.value, &entry.value));
    }
}

// =============================================================================
// BATCH OPERATIONS SURVIVE CRASH
// =============================================================================

#[test]
fn test_mput_survives_crash() {
    let test_data = load_kv_test_data();
    let entries: Vec<_> = test_data.get_run(0).iter().take(3).collect();
    assert!(entries.len() >= 3, "Need at least 3 entries");

    for mode in ["buffered", "strict"] {
        let mut test_db = if mode == "buffered" {
            TestDb::new_buffered()
        } else {
            TestDb::new_strict()
        };
        let run = ApiRunId::default();

        // Batch write
        {
            let substrate = test_db.substrate();
            let mput_entries: Vec<(&str, Value)> = entries
                .iter()
                .map(|e| (e.key.as_str(), e.value.clone()))
                .collect();
            substrate.kv_mput(&run, &mput_entries).unwrap();
        }

        // Crash
        test_db.reopen();

        // Verify all batch entries survived
        let substrate = test_db.substrate();
        let keys: Vec<&str> = entries.iter().map(|e| e.key.as_str()).collect();
        let results = substrate.kv_mget(&run, &keys).unwrap();

        assert_eq!(
            results.iter().filter(|r| r.is_some()).count(),
            3,
            "{}: all mput entries should survive crash",
            mode
        );
    }
}

// =============================================================================
// INCR SURVIVES CRASH
// =============================================================================

#[test]
fn test_incr_survives_crash() {
    for mode in ["buffered", "strict"] {
        let mut test_db = if mode == "buffered" {
            TestDb::new_buffered()
        } else {
            TestDb::new_strict()
        };
        let run = ApiRunId::default();

        // Increment counter multiple times
        {
            let substrate = test_db.substrate();
            substrate.kv_incr(&run, "counter", 10).unwrap();
            substrate.kv_incr(&run, "counter", 20).unwrap();
            substrate.kv_incr(&run, "counter", 30).unwrap();
        }

        // Crash
        test_db.reopen();

        // Verify counter value
        let substrate = test_db.substrate();
        let value = substrate.kv_get(&run, "counter").unwrap().unwrap().value;
        assert_eq!(
            value,
            Value::Int(60),
            "{}: incr counter should survive crash",
            mode
        );
    }
}

// =============================================================================
// RUN ISOLATION SURVIVES CRASH
// =============================================================================

#[test]
fn test_run_isolation_survives_crash() {
    let test_data = load_kv_test_data();
    let int_entries: Vec<_> = test_data.get_type("int").iter().take(2).collect();
    assert!(int_entries.len() >= 2, "Need at least 2 int entries");

    for mode in ["buffered", "strict"] {
        let mut test_db = if mode == "buffered" {
            TestDb::new_buffered()
        } else {
            TestDb::new_strict()
        };
        let run1 = ApiRunId::default();
        let run2 = ApiRunId::new();

        // Write different values to same key in different runs
        {
            let substrate = test_db.substrate();
            substrate
                .kv_put(&run1, "durability_shared_key", int_entries[0].value.clone())
                .unwrap();
            substrate
                .kv_put(&run2, "durability_shared_key", int_entries[1].value.clone())
                .unwrap();
        }

        // Crash
        test_db.reopen();

        // Verify isolation preserved
        let substrate = test_db.substrate();
        let v1 = substrate.kv_get(&run1, "durability_shared_key").unwrap().unwrap().value;
        let v2 = substrate.kv_get(&run2, "durability_shared_key").unwrap().unwrap().value;

        assert!(values_equal(&v1, &int_entries[0].value), "{}: run1 should have its value", mode);
        assert!(values_equal(&v2, &int_entries[1].value), "{}: run2 should have its value", mode);
    }
}

// =============================================================================
// STRESS CYCLES
// =============================================================================

#[test]
fn test_stress_cycles() {
    for mode in ["buffered", "strict"] {
        let mut test_db = if mode == "buffered" {
            TestDb::new_buffered()
        } else {
            TestDb::new_strict()
        };
        let run = ApiRunId::default();

        // Initial stable data
        {
            let substrate = test_db.substrate();
            for i in 0..50 {
                substrate
                    .kv_put(&run, &format!("stable_{}", i), Value::Int(i as i64))
                    .unwrap();
            }
        }

        // Stress: add more data, crash, verify stable data, repeat
        for cycle in 0..5 {
            // Add cycle-specific data
            {
                let substrate = test_db.substrate();
                for i in 0..20 {
                    substrate
                        .kv_put(
                            &run,
                            &format!("cycle_{}_{}", cycle, i),
                            Value::Int((cycle * 100 + i) as i64),
                        )
                        .unwrap();
                }
            }

            // Crash
            test_db.reopen();

            // Verify stable data still exists
            let substrate = test_db.substrate();
            for i in 0..50 {
                let result = substrate.kv_get(&run, &format!("stable_{}", i)).unwrap();
                assert!(
                    result.is_some(),
                    "{}: stable_{} lost at cycle {}",
                    mode,
                    i,
                    cycle
                );
                assert_eq!(result.unwrap().value, Value::Int(i as i64));
            }
        }
    }
}

// =============================================================================
// PREFIX CONSISTENCY
// =============================================================================

#[test]
fn test_prefix_consistency() {
    for mode in ["buffered", "strict"] {
        let mut test_db = if mode == "buffered" {
            TestDb::new_buffered()
        } else {
            TestDb::new_strict()
        };
        let run = ApiRunId::default();

        // Write a sequence of entries
        {
            let substrate = test_db.substrate();
            for i in 0..100 {
                substrate
                    .kv_put(&run, &format!("seq_{:03}", i), Value::Int(i as i64))
                    .unwrap();
            }
        }

        // Crash
        test_db.reopen();

        // Check for prefix consistency: no gaps in recovered data
        let substrate = test_db.substrate();
        let mut first_missing: Option<usize> = None;
        let mut gaps: Vec<usize> = Vec::new();

        for i in 0..100 {
            match substrate.kv_get(&run, &format!("seq_{:03}", i)).unwrap() {
                Some(v) if v.value == Value::Int(i as i64) => {
                    if first_missing.is_some() {
                        gaps.push(i);
                    }
                }
                _ => {
                    if first_missing.is_none() {
                        first_missing = Some(i);
                    }
                }
            }
        }

        assert!(
            gaps.is_empty(),
            "{}: prefix consistency violated - gaps at {:?}",
            mode,
            gaps
        );
    }
}

// =============================================================================
// CROSS-MODE EQUIVALENCE
// =============================================================================

#[test]
fn test_cross_mode_semantic_equivalence() {
    // Run identical workload on all three modes, verify results match
    fn workload(substrate: &SubstrateImpl, run: &ApiRunId) -> Vec<Option<Value>> {
        substrate.kv_put(run, "a", Value::Int(1)).unwrap();
        substrate.kv_put(run, "b", Value::Int(2)).unwrap();
        substrate.kv_put(run, "c", Value::Int(3)).unwrap();
        substrate.kv_put(run, "a", Value::Int(10)).unwrap(); // overwrite
        substrate.kv_delete(run, "b").unwrap();
        substrate.kv_incr(run, "counter", 5).unwrap();
        substrate.kv_incr(run, "counter", 3).unwrap();

        vec![
            substrate.kv_get(run, "a").unwrap().map(|v| v.value),
            substrate.kv_get(run, "b").unwrap().map(|v| v.value),
            substrate.kv_get(run, "c").unwrap().map(|v| v.value),
            substrate.kv_get(run, "counter").unwrap().map(|v| v.value),
        ]
    }

    let modes = [
        TestDb::new_in_memory(),
        TestDb::new_buffered(),
        TestDb::new_strict(),
    ];

    let results: Vec<_> = modes
        .into_iter()
        .map(|test_db| {
            let substrate = test_db.substrate();
            let run = ApiRunId::default();
            workload(&substrate, &run)
        })
        .collect();

    // All should be identical
    assert_eq!(results[0], results[1], "in_memory vs buffered should match");
    assert_eq!(results[1], results[2], "buffered vs strict should match");
}
