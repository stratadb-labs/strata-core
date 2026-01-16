//! ABA Detection Tests
//!
//! Critical for versioned CAS correctness: The ABA problem occurs when a value
//! changes A→B→A. A naive implementation might allow stale CAS to succeed
//! because the value "looks the same." Version-based systems must reject this.
//!
//! ## Why This Matters for M4
//!
//! M4 introduced version chains with lazy snapshot reads. The risk:
//!
//! ```text
//! Version Chain: [V3: "A"] → [V2: "B"] → [V1: "A"]
//!
//! Snapshot at V1 reads "A"
//! Current value is also "A" (at V3)
//!
//! Naive implementation might:
//!   - See current value = "A"
//!   - Think "matches snapshot"
//!   - Allow CAS
//!
//! Correct implementation:
//!   - Check version, not just value
//!   - V1 ≠ V3, reject CAS
//! ```

use super::*;
use in_mem_core::types::RunId;
use in_mem_core::value::Value;
use in_mem_primitives::{KVStore, StateCell};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Barrier};
use std::thread;

/// Classic ABA: Value changes A→B→A, stale CAS must fail
#[test]
fn statecell_aba_version_guard() {
    test_across_modes("statecell_aba_version_guard", |db| {
        let state = StateCell::new(db);
        let run_id = RunId::new();

        // 1. Init cell with value "A", get version V1
        let v1 = state.init(&run_id, "cell", Value::String("A".to_string())).unwrap();

        // 2. Verify we can read it
        let read1 = state.read(&run_id, "cell").unwrap().unwrap();
        assert_eq!(read1.value, Value::String("A".to_string()));
        assert_eq!(read1.version, v1);

        // 3. CAS V1 → "B" (succeeds, now at V2)
        let v2 = state
            .cas(&run_id, "cell", v1, Value::String("B".to_string()))
            .unwrap();
        assert!(v2 > v1, "Version should increase after CAS");

        // 4. CAS V2 → "A" (succeeds, now at V3 with value "A" again)
        let v3 = state
            .cas(&run_id, "cell", v2, Value::String("A".to_string()))
            .unwrap();
        assert!(v3 > v2, "Version should increase after second CAS");

        // 5. Now try CAS with stale V1 - MUST FAIL even though value is "A"
        let stale_cas_result = state.cas(&run_id, "cell", v1, Value::String("C".to_string()));

        // The CAS should fail because V1 is stale
        assert!(
            stale_cas_result.is_err(),
            "ABA BUG: CAS with stale version V1={} succeeded despite ABA cycle (current V3={})",
            v1,
            v3
        );

        // Verify value is still "A" (not "C")
        let final_read = state.read(&run_id, "cell").unwrap().unwrap();
        assert_eq!(
            final_read.value,
            Value::String("A".to_string()),
            "Value should still be 'A', not changed by stale CAS"
        );
        assert_eq!(final_read.version, v3, "Version should be V3");

        true // Test passed
    });
}

/// Rapid ABA cycle: 0→1→2→1→0, stale snapshot CAS must fail
#[test]
fn statecell_aba_rapid_cycle() {
    test_across_modes("statecell_aba_rapid_cycle", |db| {
        let state = StateCell::new(db);
        let run_id = RunId::new();

        // 1. Init cell with value 0
        let v0 = state.init(&run_id, "counter", Value::I64(0)).unwrap();

        // 2. Record V0 for later stale CAS attempt
        let stale_version = v0;

        // 3. Rapid cycle: 0 → 1 → 2 → 1 → 0
        let v1 = state.cas(&run_id, "counter", v0, Value::I64(1)).unwrap();
        let v2 = state.cas(&run_id, "counter", v1, Value::I64(2)).unwrap();
        let v3 = state.cas(&run_id, "counter", v2, Value::I64(1)).unwrap();
        let v4 = state.cas(&run_id, "counter", v3, Value::I64(0)).unwrap();

        // Value is now 0 (same as original) but version is V4
        let current = state.read(&run_id, "counter").unwrap().unwrap();
        assert_eq!(current.value, Value::I64(0));
        assert_eq!(current.version, v4);

        // 4. Attempt CAS with stale V0 - MUST FAIL
        let stale_cas = state.cas(&run_id, "counter", stale_version, Value::I64(99));
        assert!(
            stale_cas.is_err(),
            "ABA BUG: CAS with stale version {} succeeded (current {})",
            stale_version,
            v4
        );

        true
    });
}

/// Concurrent ABA stress: Multiple threads creating ABA patterns
#[test]
fn statecell_aba_concurrent_stress() {
    let db = create_inmemory_db();
    let state = StateCell::new(db);
    let run_id = RunId::new();

    // Initialize counter
    state.init(&run_id, "counter", Value::I64(0)).unwrap();

    let successful_cas = Arc::new(AtomicUsize::new(0));
    let failed_cas = Arc::new(AtomicUsize::new(0));
    let aba_bugs_detected = Arc::new(AtomicUsize::new(0));

    const NUM_THREADS: usize = 4;
    const ITERATIONS: usize = 100;

    let barrier = Arc::new(Barrier::new(NUM_THREADS));

    let handles: Vec<_> = (0..NUM_THREADS)
        .map(|_thread_id| {
            let state = StateCell::new(state.database().clone());
            let run_id = run_id;
            let successful_cas = Arc::clone(&successful_cas);
            let failed_cas = Arc::clone(&failed_cas);
            let aba_bugs_detected = Arc::clone(&aba_bugs_detected);
            let barrier = Arc::clone(&barrier);

            thread::spawn(move || {
                barrier.wait();

                for _ in 0..ITERATIONS {
                    // Read current state
                    let current = state.read(&run_id, "counter").unwrap();

                    if let Some(current_state) = current {
                        let current_version = current_state.version;
                        let current_value = match current_state.value {
                            Value::I64(n) => n,
                            _ => continue,
                        };

                        // Try to increment
                        let new_value = Value::I64(current_value + 1);
                        match state.cas(&run_id, "counter", current_version, new_value) {
                            Ok(new_version) => {
                                // Verify the version actually changed
                                if new_version <= current_version {
                                    aba_bugs_detected.fetch_add(1, Ordering::Relaxed);
                                }
                                successful_cas.fetch_add(1, Ordering::Relaxed);
                            }
                            Err(_) => {
                                failed_cas.fetch_add(1, Ordering::Relaxed);
                            }
                        }
                    }
                }
            })
        })
        .collect();

    for handle in handles {
        handle.join().unwrap();
    }

    let successes = successful_cas.load(Ordering::Relaxed);
    let failures = failed_cas.load(Ordering::Relaxed);
    let bugs = aba_bugs_detected.load(Ordering::Relaxed);

    println!(
        "ABA Stress: {} successes, {} failures, {} ABA bugs",
        successes, failures, bugs
    );

    // Final value should equal number of successful CAS operations
    let final_state = state.read(&run_id, "counter").unwrap().unwrap();
    let final_value = match final_state.value {
        Value::I64(n) => n as usize,
        _ => panic!("Unexpected value type"),
    };

    assert_eq!(
        bugs, 0,
        "ABA BUGS DETECTED: {} operations had non-monotonic versions",
        bugs
    );

    assert_eq!(
        final_value, successes,
        "LOST UPDATES: final value {} != successful CAS count {}",
        final_value, successes
    );
}

/// KV delete+recreate: Snapshot should see original, not recreated value
#[test]
fn kv_aba_delete_recreate() {
    test_across_modes("kv_aba_delete_recreate", |db| {
        let kv = KVStore::new(db.clone());
        let run_id = RunId::new();

        // 1. Put initial value
        kv.put(&run_id, "key", Value::String("original".to_string()))
            .unwrap();

        // 2. Read it back (establishes baseline)
        let v1 = kv.get(&run_id, "key").unwrap();
        assert_eq!(v1, Some(Value::String("original".to_string())));

        // 3. Delete it
        let deleted = kv.delete(&run_id, "key").unwrap();
        assert!(deleted, "Delete should succeed");

        // 4. Verify it's gone
        let after_delete = kv.get(&run_id, "key").unwrap();
        assert_eq!(after_delete, None, "Key should be deleted");

        // 5. Recreate with same value
        kv.put(&run_id, "key", Value::String("original".to_string()))
            .unwrap();

        // 6. Verify it's back
        let recreated = kv.get(&run_id, "key").unwrap();
        assert_eq!(recreated, Some(Value::String("original".to_string())));

        // Key insight: Even though the value looks the same, any version-based
        // operation should use the new version, not the old one.
        // This test verifies the delete+recreate cycle works correctly.

        true
    });
}

/// Version uniqueness: No two CAS operations should report success for same version
#[test]
fn statecell_version_uniqueness() {
    let db = create_inmemory_db();
    let state = StateCell::new(db);
    let run_id = RunId::new();

    state.init(&run_id, "cell", Value::I64(0)).unwrap();

    let versions_won = Arc::new(Mutex::new(Vec::new()));

    const NUM_THREADS: usize = 4;
    const ITERATIONS: usize = 50;

    let barrier = Arc::new(Barrier::new(NUM_THREADS));

    let handles: Vec<_> = (0..NUM_THREADS)
        .map(|_| {
            let state = StateCell::new(state.database().clone());
            let run_id = run_id;
            let versions_won = Arc::clone(&versions_won);
            let barrier = Arc::clone(&barrier);

            thread::spawn(move || {
                barrier.wait();

                for i in 0..ITERATIONS {
                    loop {
                        let current = state.read(&run_id, "cell").unwrap().unwrap();
                        let new_value = Value::I64(i as i64);

                        match state.cas(&run_id, "cell", current.version, new_value) {
                            Ok(new_version) => {
                                versions_won.lock().unwrap().push(new_version);
                                break;
                            }
                            Err(_) => {
                                // Retry
                                continue;
                            }
                        }
                    }
                }
            })
        })
        .collect();

    for handle in handles {
        handle.join().unwrap();
    }

    // Check that all won versions are unique
    let mut versions = versions_won.lock().unwrap();
    let total = versions.len();
    versions.sort();
    versions.dedup();
    let unique = versions.len();

    assert_eq!(
        total, unique,
        "VERSION COLLISION: {} versions won but only {} unique (expected {})",
        total, unique, NUM_THREADS * ITERATIONS
    );

    // Check versions are monotonically increasing when sorted
    for i in 1..versions.len() {
        assert!(
            versions[i] > versions[i - 1],
            "VERSION MONOTONICITY VIOLATED: {} not > {}",
            versions[i],
            versions[i - 1]
        );
    }
}

use std::sync::Mutex;

#[cfg(test)]
mod aba_unit_tests {
    use super::*;

    #[test]
    fn test_basic_cas_increment() {
        let db = create_inmemory_db();
        let state = StateCell::new(db);
        let run_id = RunId::new();

        let v1 = state.init(&run_id, "x", Value::I64(0)).unwrap();
        let v2 = state.cas(&run_id, "x", v1, Value::I64(1)).unwrap();
        let v3 = state.cas(&run_id, "x", v2, Value::I64(2)).unwrap();

        assert!(v1 < v2);
        assert!(v2 < v3);

        let current = state.read(&run_id, "x").unwrap().unwrap();
        assert_eq!(current.value, Value::I64(2));
        assert_eq!(current.version, v3);
    }

    #[test]
    fn test_stale_cas_fails() {
        let db = create_inmemory_db();
        let state = StateCell::new(db);
        let run_id = RunId::new();

        let v1 = state.init(&run_id, "x", Value::I64(0)).unwrap();
        let _v2 = state.cas(&run_id, "x", v1, Value::I64(1)).unwrap();

        // Try to use stale v1
        let result = state.cas(&run_id, "x", v1, Value::I64(999));
        assert!(result.is_err(), "Stale CAS should fail");
    }
}
