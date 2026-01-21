//! StateCell Semantic Invariant Tests
//!
//! StateCell is highest risk due to CAS semantics interacting with MVCC.
//! These tests verify that CAS, init, set, and transition behave correctly
//! across all durability modes.

use super::*;
use strata_core::types::RunId;
use strata_core::value::Value;
use strata_primitives::StateCell;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Barrier};
use std::thread;

/// CAS semantics: Success with correct version, fail with wrong version
#[test]
fn statecell_cas_basic_semantics() {
    test_across_modes("statecell_cas_basic_semantics", |db| {
        let state = StateCell::new(db);
        let run_id = RunId::new();

        // Init cell
        let v1 = state.init(&run_id, "cell", Value::I64(0)).unwrap().value;

        // CAS with correct version should succeed
        let v2 = state.cas(&run_id, "cell", v1, Value::I64(1)).unwrap().value;
        assert!(v2 > v1, "Version should increase after CAS");

        // CAS with wrong (old) version should fail
        let stale_result = state.cas(&run_id, "cell", v1, Value::I64(999));
        assert!(stale_result.is_err(), "CAS with stale version should fail");

        // Value should still be 1
        let current = state.read(&run_id, "cell").unwrap().unwrap();
        assert_eq!(current.value.value, Value::I64(1));
        assert_eq!(current.value.version, v2);

        true
    });
}

/// CAS with correct version succeeds
#[test]
fn statecell_cas_succeeds_with_correct_version() {
    test_across_modes("statecell_cas_succeeds_with_correct_version", |db| {
        let state = StateCell::new(db);
        let run_id = RunId::new();

        let v1 = state
            .init(&run_id, "x", Value::String("A".to_string()))
            .unwrap()
            .value;
        let read = state.read(&run_id, "x").unwrap().unwrap();
        assert_eq!(read.value.version, v1);

        let v2 = state
            .cas(&run_id, "x", v1, Value::String("B".to_string()))
            .unwrap()
            .value;
        assert!(v2 > v1);

        let read2 = state.read(&run_id, "x").unwrap().unwrap();
        assert_eq!(read2.value.value, Value::String("B".to_string()));
        assert_eq!(read2.value.version, v2);

        true
    });
}

/// CAS fails with wrong version
#[test]
fn statecell_cas_fails_with_wrong_version() {
    test_across_modes("statecell_cas_fails_with_wrong_version", |db| {
        let state = StateCell::new(db);
        let run_id = RunId::new();

        let v1 = state.init(&run_id, "x", Value::I64(0)).unwrap().value;
        let _v2 = state.cas(&run_id, "x", v1, Value::I64(1)).unwrap();

        // Try with old version
        let result = state.cas(&run_id, "x", v1, Value::I64(999));
        result.is_err()
    });
}

/// Failed CAS preserves current value
#[test]
fn statecell_failed_cas_preserves_value() {
    test_across_modes("statecell_failed_cas_preserves_value", |db| {
        let state = StateCell::new(db);
        let run_id = RunId::new();

        let v1 = state.init(&run_id, "x", Value::I64(100)).unwrap().value;
        let v2 = state.cas(&run_id, "x", v1, Value::I64(200)).unwrap().value;

        // Fail a CAS with stale version
        let _ = state.cas(&run_id, "x", v1, Value::I64(999));

        // Value should still be 200
        let current = state.read(&run_id, "x").unwrap().unwrap();
        assert_eq!(current.value.value, Value::I64(200));
        assert_eq!(current.value.version, v2);

        true
    });
}

/// Init uniqueness: Init succeeds for new cell, fails for existing
#[test]
fn statecell_init_uniqueness() {
    test_across_modes("statecell_init_uniqueness", |db| {
        let state = StateCell::new(db);
        let run_id = RunId::new();

        // First init succeeds
        let v1 = state.init(&run_id, "unique", Value::I64(1)).unwrap().value;
        assert!(v1 > 0);

        // Second init fails (cell exists)
        let second_init = state.init(&run_id, "unique", Value::I64(2));
        assert!(second_init.is_err(), "Second init should fail");

        // Value should still be original
        let current = state.read(&run_id, "unique").unwrap().unwrap();
        assert_eq!(current.value.value, Value::I64(1));

        true
    });
}

/// Concurrent init: Exactly one winner
#[test]
fn statecell_concurrent_init_one_winner() {
    let db = create_inmemory_db();
    let state = StateCell::new(db);
    let run_id = RunId::new();

    let success_count = Arc::new(AtomicUsize::new(0));
    let barrier = Arc::new(Barrier::new(4));

    let handles: Vec<_> = (0..4)
        .map(|i| {
            let state = StateCell::new(state.database().clone());
            let run_id = run_id;
            let success_count = Arc::clone(&success_count);
            let barrier = Arc::clone(&barrier);

            thread::spawn(move || {
                barrier.wait();
                if state.init(&run_id, "contested", Value::I64(i)).is_ok() {
                    success_count.fetch_add(1, Ordering::Relaxed);
                }
            })
        })
        .collect();

    for h in handles {
        h.join().unwrap();
    }

    let successes = success_count.load(Ordering::Relaxed);
    assert_eq!(
        successes, 1,
        "Exactly one init should succeed, got {}",
        successes
    );
}

/// Transition atomicity: No lost updates
#[test]
fn statecell_transition_atomicity() {
    test_across_modes("statecell_transition_atomicity", |db| {
        let state = StateCell::new(db);
        let run_id = RunId::new();

        state.init(&run_id, "counter", Value::I64(0)).unwrap();

        // 100 increments
        for _ in 0..100 {
            state
                .transition(&run_id, "counter", |s| {
                    if let Value::I64(n) = &s.value {
                        Ok((Value::I64(n + 1), ()))
                    } else {
                        Ok((s.value.clone(), ()))
                    }
                })
                .unwrap();
        }

        let final_val = state.read(&run_id, "counter").unwrap().unwrap();
        match final_val.value.value {
            Value::I64(n) => n == 100,
            _ => false,
        }
    });
}

/// Concurrent transitions: Total equals sum of successful increments
#[test]
fn statecell_concurrent_transitions_no_lost_updates() {
    let db = create_inmemory_db();
    let state = StateCell::new(db);
    let run_id = RunId::new();

    state.init(&run_id, "counter", Value::I64(0)).unwrap();

    const NUM_THREADS: usize = 4;
    const INCREMENTS_PER_THREAD: usize = 100;

    let barrier = Arc::new(Barrier::new(NUM_THREADS));

    let handles: Vec<_> = (0..NUM_THREADS)
        .map(|_| {
            let state = StateCell::new(state.database().clone());
            let run_id = run_id;
            let barrier = Arc::clone(&barrier);

            thread::spawn(move || {
                barrier.wait();
                for _ in 0..INCREMENTS_PER_THREAD {
                    let _ = state.transition(&run_id, "counter", |s| {
                        if let Value::I64(n) = &s.value {
                            Ok((Value::I64(n + 1), ()))
                        } else {
                            Ok((s.value.clone(), ()))
                        }
                    });
                }
            })
        })
        .collect();

    for h in handles {
        h.join().unwrap();
    }

    let final_val = state.read(&run_id, "counter").unwrap().unwrap();
    let expected = (NUM_THREADS * INCREMENTS_PER_THREAD) as i64;

    match final_val.value.value {
        Value::I64(n) => {
            assert_eq!(
                n, expected,
                "LOST UPDATES: expected {}, got {}",
                expected, n
            );
        }
        _ => panic!("Unexpected value type"),
    }
}

/// Set always succeeds (no version check)
#[test]
fn statecell_set_always_succeeds() {
    test_across_modes("statecell_set_always_succeeds", |db| {
        let state = StateCell::new(db);
        let run_id = RunId::new();

        state.init(&run_id, "x", Value::I64(0)).unwrap();

        // Multiple sets should all succeed
        for i in 1..=10 {
            let v = state.set(&run_id, "x", Value::I64(i)).unwrap().value;
            assert!(v > 0);
        }

        let final_val = state.read(&run_id, "x").unwrap().unwrap();
        assert_eq!(final_val.value.value, Value::I64(10));

        true
    });
}

/// Version increments on every successful mutation
#[test]
fn statecell_version_increments() {
    test_across_modes("statecell_version_increments", |db| {
        let state = StateCell::new(db);
        let run_id = RunId::new();

        let v1 = state.init(&run_id, "x", Value::I64(0)).unwrap().value;
        let v2 = state.set(&run_id, "x", Value::I64(1)).unwrap().value;
        let v3 = state.cas(&run_id, "x", v2, Value::I64(2)).unwrap().value;
        let (_, v4) = state
            .transition(&run_id, "x", |s| Ok((s.value.clone(), ())))
            .unwrap();
        let v4 = v4.value;

        assert!(v1 < v2, "set should increment version");
        assert!(v2 < v3, "cas should increment version");
        assert!(v3 < v4, "transition should increment version");

        true
    });
}

/// Delete and exists semantics
#[test]
fn statecell_delete_exists_semantics() {
    test_across_modes("statecell_delete_exists_semantics", |db| {
        let state = StateCell::new(db);
        let run_id = RunId::new();

        // Initially doesn't exist
        assert!(!state.exists(&run_id, "cell").unwrap());

        // Create
        state.init(&run_id, "cell", Value::I64(1)).unwrap();
        assert!(state.exists(&run_id, "cell").unwrap());

        // Delete
        let deleted = state.delete(&run_id, "cell").unwrap();
        assert!(deleted);
        assert!(!state.exists(&run_id, "cell").unwrap());

        // Read returns None
        assert!(state.read(&run_id, "cell").unwrap().is_none());

        true
    });
}

/// Cells are independent per run
#[test]
fn statecell_run_isolation() {
    test_across_modes("statecell_run_isolation", |db| {
        let state = StateCell::new(db);
        let run_a = RunId::new();
        let run_b = RunId::new();

        // Same cell name in different runs
        state.init(&run_a, "shared_name", Value::I64(100)).unwrap();
        state.init(&run_b, "shared_name", Value::I64(200)).unwrap();

        let val_a = state.read(&run_a, "shared_name").unwrap().unwrap();
        let val_b = state.read(&run_b, "shared_name").unwrap().unwrap();

        assert_eq!(val_a.value.value, Value::I64(100));
        assert_eq!(val_b.value.value, Value::I64(200));

        // Modifying one doesn't affect other
        state.set(&run_a, "shared_name", Value::I64(999)).unwrap();

        let val_b_after = state.read(&run_b, "shared_name").unwrap().unwrap();
        assert_eq!(val_b_after.value.value, Value::I64(200));

        true
    });
}

#[cfg(test)]
mod statecell_unit_tests {
    use super::*;

    #[test]
    fn test_basic_init_read() {
        let db = create_inmemory_db();
        let state = StateCell::new(db);
        let run_id = RunId::new();

        let v = state.init(&run_id, "test", Value::I64(42)).unwrap().value;
        let read = state.read(&run_id, "test").unwrap().unwrap();

        assert_eq!(read.value.value, Value::I64(42));
        assert_eq!(read.value.version, v);
    }

    #[test]
    fn test_transition_or_init() {
        let db = create_inmemory_db();
        let state = StateCell::new(db);
        let run_id = RunId::new();

        // First call should init
        let (result1, _) = state
            .transition_or_init(&run_id, "toi", Value::I64(10), |s| {
                if let Value::I64(n) = &s.value {
                    Ok((Value::I64(n + 1), Value::I64(n + 1)))
                } else {
                    Ok((s.value.clone(), s.value.clone()))
                }
            })
            .unwrap();
        // After init, transition is applied, so result is the transitioned value
        assert_eq!(result1, Value::I64(11));

        // Second call should transition
        let (result2, _) = state
            .transition_or_init(&run_id, "toi", Value::I64(10), |s| {
                if let Value::I64(n) = &s.value {
                    Ok((Value::I64(n + 1), Value::I64(n + 1)))
                } else {
                    Ok((s.value.clone(), s.value.clone()))
                }
            })
            .unwrap();
        assert_eq!(result2, Value::I64(12)); // Transitioned again
    }
}
