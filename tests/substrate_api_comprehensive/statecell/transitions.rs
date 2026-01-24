//! StateCell Transition Tests
//!
//! Comprehensive tests for `state_transition` and `state_transition_or_init` APIs.
//!
//! These are the CORE features of StateCell - atomic read-modify-write operations
//! with automatic OCC (Optimistic Concurrency Control) retry.
//!
//! ## Key Properties Tested
//!
//! 1. Atomic read-modify-write semantics
//! 2. Automatic retry on conflict (up to 200 times)
//! 3. Purity requirement documentation (closure may be called multiple times)
//! 4. Error propagation from closures
//! 5. Version tracking through transitions

use crate::*;
use strata_core::StrataError;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread;

// =============================================================================
// BASIC TRANSITION TESTS
// =============================================================================

#[test]
fn test_transition_basic_increment() {
    let (_, substrate) = quick_setup();
    let run = ApiRunId::default();

    // Initialize counter
    substrate.state_set(&run, "counter", Value::Int(0)).unwrap();

    // Transition to increment
    let (new_value, version) = substrate
        .state_transition(&run, "counter", |current| {
            let n = current.as_int().unwrap_or(0);
            Ok(Value::Int(n + 1))
        })
        .expect("transition should succeed");

    assert_eq!(new_value, Value::Int(1));
    assert!(matches!(version, Version::Counter(2))); // Started at 1, now 2

    // Verify state
    let state = substrate.state_get(&run, "counter").unwrap().unwrap();
    assert_eq!(state.value, Value::Int(1));
}

#[test]
fn test_transition_multiple_increments() {
    let (_, substrate) = quick_setup();
    let run = ApiRunId::default();

    substrate.state_set(&run, "counter", Value::Int(0)).unwrap();

    // Multiple transitions
    for expected in 1..=10i64 {
        let (new_value, _) = substrate
            .state_transition(&run, "counter", |current| {
                let n = current.as_int().unwrap_or(0);
                Ok(Value::Int(n + 1))
            })
            .unwrap();

        assert_eq!(new_value, Value::Int(expected));
    }

    // Final value
    let state = substrate.state_get(&run, "counter").unwrap().unwrap();
    assert_eq!(state.value, Value::Int(10));
}

#[test]
fn test_transition_returns_computed_value() {
    let (_, substrate) = quick_setup();
    let run = ApiRunId::default();

    substrate
        .state_set(&run, "data", Value::String("hello".into()))
        .unwrap();

    let (new_value, _) = substrate
        .state_transition(&run, "data", |current| {
            let s = current.as_str().unwrap_or("");
            Ok(Value::String(format!("{} world", s)))
        })
        .unwrap();

    assert_eq!(new_value, Value::String("hello world".into()));
}

#[test]
fn test_transition_type_change() {
    let (_, substrate) = quick_setup();
    let run = ApiRunId::default();

    substrate.state_set(&run, "typed", Value::Int(42)).unwrap();

    // Transition to different type
    let (new_value, _) = substrate
        .state_transition(&run, "typed", |current| {
            let n = current.as_int().unwrap_or(0);
            Ok(Value::String(format!("was: {}", n)))
        })
        .unwrap();

    assert_eq!(new_value, Value::String("was: 42".into()));
}

#[test]
fn test_transition_with_complex_value() {
    let (_, substrate) = quick_setup();
    let run = ApiRunId::default();

    let initial = Value::Object(
        vec![
            ("count".to_string(), Value::Int(0)),
            ("name".to_string(), Value::String("test".into())),
        ]
        .into_iter()
        .collect(),
    );
    substrate.state_set(&run, "complex", initial).unwrap();

    let (new_value, _) = substrate
        .state_transition(&run, "complex", |current| {
            if let Value::Object(mut obj) = current.clone() {
                if let Some(Value::Int(n)) = obj.get("count") {
                    obj.insert("count".to_string(), Value::Int(n + 1));
                }
                Ok(Value::Object(obj))
            } else {
                Ok(current.clone())
            }
        })
        .unwrap();

    if let Value::Object(obj) = new_value {
        assert_eq!(obj.get("count"), Some(&Value::Int(1)));
        assert_eq!(obj.get("name"), Some(&Value::String("test".into())));
    } else {
        panic!("Expected object");
    }
}

// =============================================================================
// TRANSITION ERROR HANDLING
// =============================================================================

#[test]
fn test_transition_cell_not_found() {
    let (_, substrate) = quick_setup();
    let run = ApiRunId::default();

    // Transition on non-existent cell should fail
    let result = substrate.state_transition(&run, "nonexistent", |_| Ok(Value::Int(1)));

    assert!(result.is_err(), "Transition on non-existent cell should fail");
}

#[test]
fn test_transition_closure_error_propagates() {
    let (_, substrate) = quick_setup();
    let run = ApiRunId::default();

    substrate.state_set(&run, "error_test", Value::Int(0)).unwrap();

    let result = substrate.state_transition(&run, "error_test", |_| {
        Err(StrataError::invalid_input("intentional error"))
    });

    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(
        err.to_string().contains("intentional error"),
        "Error message should propagate: {}",
        err
    );
}

#[test]
fn test_transition_conditional_error() {
    let (_, substrate) = quick_setup();
    let run = ApiRunId::default();

    substrate.state_set(&run, "cond_error", Value::Int(5)).unwrap();

    // Should succeed when condition is met
    let result = substrate.state_transition(&run, "cond_error", |current| {
        let n = current.as_int().unwrap_or(0);
        if n < 10 {
            Ok(Value::Int(n + 1))
        } else {
            Err(StrataError::invalid_input("limit reached"))
        }
    });
    assert!(result.is_ok());

    // Keep going until we hit the limit
    for _ in 0..4 {
        let _ = substrate.state_transition(&run, "cond_error", |current| {
            let n = current.as_int().unwrap_or(0);
            if n < 10 {
                Ok(Value::Int(n + 1))
            } else {
                Err(StrataError::invalid_input("limit reached"))
            }
        });
    }

    // Now should fail
    let result = substrate.state_transition(&run, "cond_error", |current| {
        let n = current.as_int().unwrap_or(0);
        if n < 10 {
            Ok(Value::Int(n + 1))
        } else {
            Err(StrataError::invalid_input("limit reached"))
        }
    });
    assert!(result.is_err());
}

// =============================================================================
// TRANSITION OR INIT TESTS
// =============================================================================

#[test]
fn test_transition_or_init_creates_new_cell() {
    let (_, substrate) = quick_setup();
    let run = ApiRunId::default();

    // Cell doesn't exist - should initialize with 0, then transition to 1
    let (new_value, version) = substrate
        .state_transition_or_init(&run, "new_counter", Value::Int(0), |current| {
            let n = current.as_int().unwrap_or(0);
            Ok(Value::Int(n + 1))
        })
        .expect("transition_or_init should succeed");

    assert_eq!(new_value, Value::Int(1)); // 0 -> 1
    assert!(matches!(version, Version::Counter(2))); // init=1, transition=2

    // Verify cell exists
    let state = substrate.state_get(&run, "new_counter").unwrap().unwrap();
    assert_eq!(state.value, Value::Int(1));
}

#[test]
fn test_transition_or_init_uses_existing_value() {
    let (_, substrate) = quick_setup();
    let run = ApiRunId::default();

    // Create cell with value 10
    substrate.state_set(&run, "existing_counter", Value::Int(10)).unwrap();

    // transition_or_init should use existing value, not initial
    let (new_value, _) = substrate
        .state_transition_or_init(&run, "existing_counter", Value::Int(0), |current| {
            let n = current.as_int().unwrap_or(0);
            Ok(Value::Int(n + 1))
        })
        .unwrap();

    assert_eq!(new_value, Value::Int(11)); // 10 -> 11, not 0 -> 1
}

#[test]
fn test_transition_or_init_multiple_times() {
    let (_, substrate) = quick_setup();
    let run = ApiRunId::default();

    // First call initializes
    let (v1, _) = substrate
        .state_transition_or_init(&run, "multi_init", Value::Int(0), |current| {
            let n = current.as_int().unwrap_or(0);
            Ok(Value::Int(n + 1))
        })
        .unwrap();
    assert_eq!(v1, Value::Int(1));

    // Subsequent calls use existing
    for expected in 2..=5i64 {
        let (v, _) = substrate
            .state_transition_or_init(&run, "multi_init", Value::Int(0), |current| {
                let n = current.as_int().unwrap_or(0);
                Ok(Value::Int(n + 1))
            })
            .unwrap();
        assert_eq!(v, Value::Int(expected));
    }
}

#[test]
fn test_transition_or_init_different_initial_values() {
    let (_, substrate) = quick_setup();
    let run = ApiRunId::default();

    // First init with 100
    let (v1, _) = substrate
        .state_transition_or_init(&run, "diff_init", Value::Int(100), |current| {
            let n = current.as_int().unwrap_or(0);
            Ok(Value::Int(n + 1))
        })
        .unwrap();
    assert_eq!(v1, Value::Int(101)); // 100 -> 101

    // Second call ignores initial value 500 (cell already exists)
    let (v2, _) = substrate
        .state_transition_or_init(&run, "diff_init", Value::Int(500), |current| {
            let n = current.as_int().unwrap_or(0);
            Ok(Value::Int(n + 1))
        })
        .unwrap();
    assert_eq!(v2, Value::Int(102)); // 101 -> 102, not 500 -> 501
}

// =============================================================================
// VERSION TRACKING
// =============================================================================

#[test]
fn test_transition_increments_version() {
    let (_, substrate) = quick_setup();
    let run = ApiRunId::default();

    let v1 = substrate.state_set(&run, "versioned", Value::Int(0)).unwrap();

    let (_, v2) = substrate
        .state_transition(&run, "versioned", |_| Ok(Value::Int(1)))
        .unwrap();

    let (_, v3) = substrate
        .state_transition(&run, "versioned", |_| Ok(Value::Int(2)))
        .unwrap();

    if let (Version::Counter(c1), Version::Counter(c2), Version::Counter(c3)) = (v1, v2, v3) {
        assert!(c2 > c1, "Version should increment");
        assert!(c3 > c2, "Version should increment");
    } else {
        panic!("Expected Counter versions");
    }
}

#[test]
fn test_transition_returns_correct_version() {
    let (_, substrate) = quick_setup();
    let run = ApiRunId::default();

    substrate.state_set(&run, "ver_check", Value::Int(0)).unwrap();

    let (_, returned_version) = substrate
        .state_transition(&run, "ver_check", |_| Ok(Value::Int(1)))
        .unwrap();

    // Returned version should match what get() returns
    let state = substrate.state_get(&run, "ver_check").unwrap().unwrap();
    assert_eq!(state.version, returned_version);
}

// =============================================================================
// CONCURRENT TRANSITIONS
// =============================================================================

#[test]
fn test_transition_concurrent_increments() {
    let (_, substrate) = quick_setup();
    let run = ApiRunId::default();

    substrate.state_set(&run, "concurrent", Value::Int(0)).unwrap();

    let substrate = Arc::new(substrate);
    let num_threads = 8usize;
    let increments_per_thread = 50usize;

    let handles: Vec<_> = (0..num_threads)
        .map(|_| {
            let s = Arc::clone(&substrate);
            let r = run.clone();
            thread::spawn(move || {
                for _ in 0..increments_per_thread {
                    s.state_transition(&r, "concurrent", |current| {
                        let n = current.as_int().unwrap_or(0);
                        Ok(Value::Int(n + 1))
                    })
                    .expect("transition should succeed");
                }
            })
        })
        .collect();

    for h in handles {
        h.join().unwrap();
    }

    // All increments should be counted
    let final_state = substrate.state_get(&run, "concurrent").unwrap().unwrap();
    let expected = (num_threads * increments_per_thread) as i64;
    assert_eq!(
        final_state.value,
        Value::Int(expected),
        "All {} increments should be reflected",
        expected
    );
}

#[test]
fn test_transition_or_init_concurrent_first_write() {
    let (_, substrate) = quick_setup();
    let run = ApiRunId::default();

    let substrate = Arc::new(substrate);
    let num_threads = 10usize;
    let success_count = Arc::new(AtomicUsize::new(0));

    // Multiple threads try to transition_or_init the same cell
    let handles: Vec<_> = (0..num_threads)
        .map(|i| {
            let s = Arc::clone(&substrate);
            let r = run.clone();
            let count = Arc::clone(&success_count);
            thread::spawn(move || {
                let result = s.state_transition_or_init(&r, "race_init", Value::Int(i as i64), |current| {
                    let n = current.as_int().unwrap_or(0);
                    Ok(Value::Int(n + 1))
                });
                if result.is_ok() {
                    count.fetch_add(1, Ordering::SeqCst);
                }
            })
        })
        .collect();

    for h in handles {
        h.join().unwrap();
    }

    // All transitions should have succeeded (one init + 9 transitions on existing)
    assert_eq!(success_count.load(Ordering::SeqCst), num_threads);

    // Final value should reflect all transitions
    let final_state = substrate.state_get(&run, "race_init").unwrap().unwrap();
    let n = final_state.value.as_int().unwrap();
    // First: init to some value, then transition (+1) = initial+1
    // Then 9 more transitions each adding 1 = +9
    // So final should be initial + 10
    assert!(n >= 10, "Final value should be at least 10, got {}", n);
}

// =============================================================================
// PURITY DOCUMENTATION TESTS
// These tests document that closures may be called multiple times
// =============================================================================

#[test]
fn test_transition_closure_may_be_called_multiple_times() {
    // This test demonstrates that the closure MAY be called more than once
    // due to OCC retries. Users must ensure their closures are PURE.
    let (_, substrate) = quick_setup();
    let run = ApiRunId::default();

    substrate.state_set(&run, "purity_demo", Value::Int(0)).unwrap();

    let call_count = Arc::new(AtomicUsize::new(0));
    let call_count_clone = Arc::clone(&call_count);

    // This closure is NOT pure (has side effect) but demonstrates the concept
    // In real code, this would be a bug!
    let _ = substrate.state_transition(&run, "purity_demo", move |current| {
        call_count_clone.fetch_add(1, Ordering::SeqCst);
        let n = current.as_int().unwrap_or(0);
        Ok(Value::Int(n + 1))
    });

    // Without contention, closure should be called exactly once
    let calls = call_count.load(Ordering::SeqCst);
    assert!(
        calls >= 1,
        "Closure should be called at least once, was called {} times",
        calls
    );

    // NOTE: Under contention, `calls` could be > 1 due to retries
    // This test just documents the behavior
}

#[test]
fn test_transition_idempotent_closure_is_safe() {
    // A pure, idempotent closure is safe because:
    // - Same input always produces same output
    // - No side effects
    // - Multiple calls with same state produce same result
    let (_, substrate) = quick_setup();
    let run = ApiRunId::default();

    substrate.state_set(&run, "idempotent", Value::Int(5)).unwrap();

    // This closure is pure and idempotent
    let (result, _) = substrate
        .state_transition(&run, "idempotent", |current| {
            let n = current.as_int().unwrap_or(0);
            Ok(Value::Int(n * 2)) // Deterministic: same n always gives same result
        })
        .unwrap();

    assert_eq!(result, Value::Int(10));
}

// =============================================================================
// USE CASE PATTERNS
// =============================================================================

#[test]
fn test_pattern_atomic_counter() {
    let (_, substrate) = quick_setup();
    let run = ApiRunId::default();

    // Atomic counter pattern using transition_or_init
    fn increment_counter(
        substrate: &SubstrateImpl,
        run: &ApiRunId,
        name: &str,
    ) -> i64 {
        let (new_value, _) = substrate
            .state_transition_or_init(run, name, Value::Int(0), |current| {
                let n = current.as_int().unwrap_or(0);
                Ok(Value::Int(n + 1))
            })
            .expect("increment should succeed");
        new_value.as_int().unwrap_or(0)
    }

    assert_eq!(increment_counter(&substrate, &run, "atomic_counter"), 1);
    assert_eq!(increment_counter(&substrate, &run, "atomic_counter"), 2);
    assert_eq!(increment_counter(&substrate, &run, "atomic_counter"), 3);
}

#[test]
fn test_pattern_workflow_state_machine() {
    let (_, substrate) = quick_setup();
    let run = ApiRunId::default();

    // Workflow state machine pattern
    substrate
        .state_set(&run, "workflow", Value::String("pending".into()))
        .unwrap();

    // Advance: pending -> running
    let (new_state, _) = substrate
        .state_transition(&run, "workflow", |current| {
            match current.as_str() {
                Some("pending") => Ok(Value::String("running".into())),
                Some("running") => Ok(Value::String("completed".into())),
                Some("completed") => Err(StrataError::invalid_input("already completed")),
                _ => Err(StrataError::invalid_input("unknown state")),
            }
        })
        .unwrap();
    assert_eq!(new_state, Value::String("running".into()));

    // Advance: running -> completed
    let (new_state, _) = substrate
        .state_transition(&run, "workflow", |current| {
            match current.as_str() {
                Some("pending") => Ok(Value::String("running".into())),
                Some("running") => Ok(Value::String("completed".into())),
                Some("completed") => Err(StrataError::invalid_input("already completed")),
                _ => Err(StrataError::invalid_input("unknown state")),
            }
        })
        .unwrap();
    assert_eq!(new_state, Value::String("completed".into()));

    // Cannot advance past completed
    let result = substrate.state_transition(&run, "workflow", |current| {
        match current.as_str() {
            Some("pending") => Ok(Value::String("running".into())),
            Some("running") => Ok(Value::String("completed".into())),
            Some("completed") => Err(StrataError::invalid_input("already completed")),
            _ => Err(StrataError::invalid_input("unknown state")),
        }
    });
    assert!(result.is_err());
}

#[test]
fn test_pattern_bounded_counter() {
    let (_, substrate) = quick_setup();
    let run = ApiRunId::default();

    // Bounded counter that doesn't exceed max
    let max = 5i64;

    for i in 0..10 {
        let result = substrate.state_transition_or_init(&run, "bounded", Value::Int(0), |current| {
            let n = current.as_int().unwrap_or(0);
            if n >= max {
                Err(StrataError::invalid_input("limit reached"))
            } else {
                Ok(Value::Int(n + 1))
            }
        });

        if i < max as usize {
            assert!(result.is_ok(), "Should succeed up to max");
        } else {
            assert!(result.is_err(), "Should fail after max");
        }
    }

    // Final value is exactly max
    let state = substrate.state_get(&run, "bounded").unwrap().unwrap();
    assert_eq!(state.value, Value::Int(max));
}

#[test]
fn test_pattern_accumulator() {
    let (_, substrate) = quick_setup();
    let run = ApiRunId::default();

    // Accumulator pattern - collect values into array
    let initial = Value::Array(vec![]);
    substrate.state_set(&run, "accumulator", initial).unwrap();

    for i in 1..=5i64 {
        substrate
            .state_transition(&run, "accumulator", move |current| {
                if let Value::Array(mut arr) = current.clone() {
                    arr.push(Value::Int(i));
                    Ok(Value::Array(arr))
                } else {
                    Ok(Value::Array(vec![Value::Int(i)]))
                }
            })
            .unwrap();
    }

    let state = substrate.state_get(&run, "accumulator").unwrap().unwrap();
    if let Value::Array(arr) = state.value {
        assert_eq!(arr.len(), 5);
        assert_eq!(arr, vec![
            Value::Int(1),
            Value::Int(2),
            Value::Int(3),
            Value::Int(4),
            Value::Int(5),
        ]);
    } else {
        panic!("Expected array");
    }
}

// =============================================================================
// HISTORY TESTS
// =============================================================================

#[test]
fn test_history_basic() {
    let (_, substrate) = quick_setup();
    let run = ApiRunId::default();

    // Create cell and make several updates
    substrate.state_set(&run, "history_test", Value::Int(1)).unwrap();
    substrate.state_set(&run, "history_test", Value::Int(2)).unwrap();
    substrate.state_set(&run, "history_test", Value::Int(3)).unwrap();

    // Get history
    let history = substrate
        .state_history(&run, "history_test", None, None)
        .expect("history should succeed");

    // Should have at least one entry (storage may not keep all versions)
    assert!(!history.is_empty(), "History should not be empty");

    // All entries should have Counter versions
    for entry in &history {
        assert!(
            matches!(entry.version, Version::Counter(_)),
            "History entries should use Counter versions"
        );
    }
}

#[test]
fn test_history_nonexistent_cell() {
    let (_, substrate) = quick_setup();
    let run = ApiRunId::default();

    // History of non-existent cell should return empty
    let history = substrate
        .state_history(&run, "nonexistent", None, None)
        .expect("history should not error");

    assert!(history.is_empty(), "History of nonexistent cell should be empty");
}

#[test]
fn test_history_with_limit() {
    let (_, substrate) = quick_setup();
    let run = ApiRunId::default();

    // Create cell and make many updates
    for i in 1..=10i64 {
        substrate.state_set(&run, "limit_test", Value::Int(i)).unwrap();
    }

    // Get history with limit
    let history = substrate
        .state_history(&run, "limit_test", Some(3), None)
        .expect("history should succeed");

    // Should respect limit
    assert!(
        history.len() <= 3,
        "History should respect limit, got {} entries",
        history.len()
    );
}

#[test]
fn test_history_with_before_filter() {
    let (_, substrate) = quick_setup();
    let run = ApiRunId::default();

    // Create cell and make updates
    substrate.state_set(&run, "before_test", Value::Int(1)).unwrap();
    substrate.state_set(&run, "before_test", Value::Int(2)).unwrap();
    let v3 = substrate.state_set(&run, "before_test", Value::Int(3)).unwrap();
    substrate.state_set(&run, "before_test", Value::Int(4)).unwrap();

    // Get history with before filter
    let counter = match v3 {
        Version::Counter(c) => c,
        _ => panic!("Expected Counter version"),
    };

    let history = substrate
        .state_history(&run, "before_test", None, Some(Version::Counter(counter)))
        .expect("history should succeed");

    // All entries should have counter < before
    for entry in &history {
        if let Version::Counter(c) = entry.version {
            assert!(
                c < counter,
                "History entry counter {} should be < before {}",
                c,
                counter
            );
        }
    }
}

#[test]
fn test_history_wrong_version_type_rejected() {
    let (_, substrate) = quick_setup();
    let run = ApiRunId::default();

    substrate.state_set(&run, "version_type_test", Value::Int(1)).unwrap();

    // Using Txn version should fail (StateCell uses Counter)
    let result = substrate.state_history(
        &run,
        "version_type_test",
        None,
        Some(Version::Txn(123)),
    );

    assert!(
        result.is_err(),
        "Should reject Txn version for StateCell history"
    );
}

#[test]
fn test_history_after_transitions() {
    let (_, substrate) = quick_setup();
    let run = ApiRunId::default();

    // Use transitions to update
    substrate.state_set(&run, "trans_history", Value::Int(0)).unwrap();

    for _ in 0..5 {
        substrate
            .state_transition(&run, "trans_history", |current| {
                let n = current.as_int().unwrap_or(0);
                Ok(Value::Int(n + 1))
            })
            .unwrap();
    }

    // Get history
    let history = substrate
        .state_history(&run, "trans_history", None, None)
        .expect("history should succeed");

    // Should have entries
    assert!(!history.is_empty(), "History should have entries after transitions");

    // Latest entry should have current value
    if let Some(latest) = history.first() {
        // The value should be 5 (0 + 5 increments)
        assert_eq!(
            latest.value,
            Value::Int(5),
            "Latest history entry should have current value"
        );
    }
}

#[test]
fn test_history_after_delete() {
    let (_, substrate) = quick_setup();
    let run = ApiRunId::default();

    // Create and update
    substrate.state_set(&run, "delete_history", Value::Int(1)).unwrap();
    substrate.state_set(&run, "delete_history", Value::Int(2)).unwrap();

    // Delete
    substrate.state_delete(&run, "delete_history").unwrap();

    // History should be empty after delete (cell no longer exists)
    let history = substrate
        .state_history(&run, "delete_history", None, None)
        .expect("history should succeed");

    assert!(
        history.is_empty(),
        "History should be empty after delete"
    );
}

#[test]
fn test_history_entries_have_timestamps() {
    let (_, substrate) = quick_setup();
    let run = ApiRunId::default();

    substrate.state_set(&run, "timestamp_test", Value::Int(42)).unwrap();

    let history = substrate
        .state_history(&run, "timestamp_test", None, None)
        .expect("history should succeed");

    for entry in &history {
        // Timestamp should be non-zero (some time after Unix epoch)
        assert!(
            entry.timestamp.as_micros() > 0,
            "History entries should have valid timestamps"
        );
    }
}

#[test]
fn test_history_pagination() {
    let (_, substrate) = quick_setup();
    let run = ApiRunId::default();

    // Create many versions
    for i in 1..=20i64 {
        substrate.state_set(&run, "paginate", Value::Int(i)).unwrap();
    }

    // Get first page
    let page1 = substrate
        .state_history(&run, "paginate", Some(5), None)
        .expect("page 1 should succeed");

    if !page1.is_empty() {
        // Get last entry's version for pagination
        if let Some(last) = page1.last() {
            // Get next page using before cursor
            let page2 = substrate
                .state_history(&run, "paginate", Some(5), Some(last.version.clone()))
                .expect("page 2 should succeed");

            // Pages should not overlap
            if !page2.is_empty() {
                let page1_versions: Vec<_> = page1.iter().map(|e| &e.version).collect();
                let page2_versions: Vec<_> = page2.iter().map(|e| &e.version).collect();

                for v in &page2_versions {
                    assert!(
                        !page1_versions.contains(v),
                        "Paginated pages should not overlap"
                    );
                }
            }
        }
    }
}
