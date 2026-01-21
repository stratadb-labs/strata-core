//! Tier 1: StateCell CAS Tests (M3.12-M3.15)
//!
//! These tests verify StateCell invariants around version monotonicity,
//! CAS atomicity, and transition semantics.
//!
//! ## Invariants Tested
//!
//! - M3.12: Version Monotonicity - Versions always increase
//! - M3.13: CAS Atomicity - Only one concurrent CAS succeeds per version
//! - M3.14: Init Uniqueness - init() fails if cell already exists
//! - M3.15: Transition Speculative Execution - Closure may be re-executed

use super::test_utils::*;
use strata_core::error::Error;
use strata_core::types::RunId;
use strata_core::value::Value;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

// ============================================================================
// M3.12: Version Monotonicity
// ============================================================================
// Versions always increase (1, 2, 3, ...).
// CAS cannot set lower version.
// set() increments version atomically.
//
// What breaks if this fails?
// ABA problem. Old version can mask intervening writes.

mod version_monotonicity {
    use super::*;

    #[test]
    fn test_init_creates_version_one() {
        let tp = TestPrimitives::new();
        let run_id = tp.run_id;

        tp.state_cell.init(&run_id, "cell", values::int(0)).unwrap();

        let state = tp.state_cell.read(&run_id, "cell").unwrap().unwrap();
        assert_eq!(state.value.version, 1, "Initial version should be 1");
    }

    #[test]
    fn test_set_increments_version() {
        let tp = TestPrimitives::new();
        let run_id = tp.run_id;

        tp.state_cell.init(&run_id, "cell", values::int(0)).unwrap();

        let mut expected_version = 1u64;
        for i in 1..=10 {
            let new_ver = tp.state_cell.set(&run_id, "cell", values::int(i)).unwrap().value;
            expected_version += 1;
            assert_eq!(new_ver, expected_version, "set() should increment version");
        }
    }

    #[test]
    fn test_cas_increments_version() {
        let tp = TestPrimitives::new();
        let run_id = tp.run_id;

        tp.state_cell.init(&run_id, "cell", values::int(0)).unwrap();

        // CAS from v1 to v2
        let v2 = tp
            .state_cell
            .cas(&run_id, "cell", 1, values::int(1))
            .unwrap()
            .value;
        assert_eq!(v2, 2);

        // CAS from v2 to v3
        let v3 = tp
            .state_cell
            .cas(&run_id, "cell", 2, values::int(2))
            .unwrap()
            .value;
        assert_eq!(v3, 3);
    }

    #[test]
    fn test_versions_never_decrease() {
        let tp = TestPrimitives::new();
        let run_id = tp.run_id;

        tp.state_cell.init(&run_id, "cell", values::int(0)).unwrap();

        let mut versions = vec![1u64]; // Initial version

        // Perform multiple operations
        for i in 1..=20 {
            let ver = tp.state_cell.set(&run_id, "cell", values::int(i)).unwrap().value;
            versions.push(ver);
        }

        // Verify strict monotonicity
        invariants::assert_version_monotonic(&versions);
    }

    #[test]
    fn test_versions_independent_per_cell() {
        let tp = TestPrimitives::new();
        let run_id = tp.run_id;

        // Create two cells
        tp.state_cell
            .init(&run_id, "cell_a", values::int(0))
            .unwrap();
        tp.state_cell
            .init(&run_id, "cell_b", values::int(0))
            .unwrap();

        // Update cell_a multiple times
        for i in 1..=5 {
            tp.state_cell
                .set(&run_id, "cell_a", values::int(i))
                .unwrap();
        }

        // cell_b should still be at version 1
        let state_b = tp.state_cell.read(&run_id, "cell_b").unwrap().unwrap();
        assert_eq!(state_b.value.version, 1, "cell_b version should be unchanged");
    }

    #[test]
    fn test_version_monotonicity_survives_recovery() {
        let ptp = PersistentTestPrimitives::new();
        let run_id = ptp.run_id;

        let version_before;
        {
            let p = ptp.open_strict();
            p.state_cell.init(&run_id, "cell", values::int(0)).unwrap();

            for i in 1..=10 {
                p.state_cell.set(&run_id, "cell", values::int(i)).unwrap();
            }

            version_before = p.state_cell.read(&run_id, "cell").unwrap().unwrap().value.version;
        }

        // Recover and continue
        {
            let p = ptp.open();
            let state = p.state_cell.read(&run_id, "cell").unwrap().unwrap();
            assert_eq!(
                state.value.version, version_before,
                "Version changed after recovery"
            );

            // New operations should continue monotonically
            let new_ver = p.state_cell.set(&run_id, "cell", values::int(100)).unwrap().value;
            assert!(
                new_ver > version_before,
                "New version {} not greater than pre-recovery {}",
                new_ver,
                version_before
            );
        }
    }
}

// ============================================================================
// M3.13: CAS Atomicity
// ============================================================================
// Only one concurrent CAS succeeds per version.
// Losing CAS sees correct version for retry.
// No lost updates.
//
// What breaks if this fails?
// Lost updates. Two concurrent increments both succeed, counter only goes up by 1.

mod cas_atomicity {
    use super::*;

    #[test]
    fn test_cas_succeeds_with_correct_version() {
        let tp = TestPrimitives::new();
        let run_id = tp.run_id;

        tp.state_cell.init(&run_id, "cell", values::int(0)).unwrap();

        // CAS with correct version succeeds
        let result = tp.state_cell.cas(&run_id, "cell", 1, values::int(1));
        assert!(result.is_ok(), "CAS with correct version should succeed");
    }

    #[test]
    fn test_cas_fails_with_wrong_version() {
        let tp = TestPrimitives::new();
        let run_id = tp.run_id;

        tp.state_cell.init(&run_id, "cell", values::int(0)).unwrap();

        // CAS with wrong version fails
        let result = tp.state_cell.cas(&run_id, "cell", 99, values::int(1));
        assert!(result.is_err(), "CAS with wrong version should fail");
    }

    #[test]
    fn test_exactly_one_cas_winner() {
        let tp = TestPrimitives::new();
        let run_id = tp.run_id;
        let sc = tp.state_cell.clone();

        tp.state_cell.init(&run_id, "cell", values::int(0)).unwrap();

        // Multiple threads try CAS from version 1
        let results = concurrent::run_with_shared(10, (sc, run_id), |i, (sc, run_id)| {
            sc.cas(run_id, "cell", 1, values::int(i as i64 + 100))
        });

        // Count winners
        let winners: usize = results.iter().filter(|r| r.is_ok()).count();
        assert_eq!(winners, 1, "Expected exactly 1 CAS winner, got {}", winners);
    }

    #[test]
    fn test_cas_lost_update_prevention() {
        let tp = TestPrimitives::new();
        let run_id = tp.run_id;
        let sc = tp.state_cell.clone();
        let num_threads = 10;

        // Initialize counter
        tp.state_cell
            .init(&run_id, "counter", values::int(0))
            .unwrap();

        // Each thread tries to increment via CAS with retry
        let success_count = Arc::new(AtomicU64::new(0));

        let _ = concurrent::run_with_shared(
            num_threads,
            (sc.clone(), run_id, success_count.clone()),
            |_, (sc, run_id, success_count)| {
                // Retry loop
                for _ in 0..100 {
                    let state = sc.read(run_id, "counter").unwrap().unwrap();
                    let current = if let Value::I64(v) = state.value.value {
                        v
                    } else {
                        panic!()
                    };

                    if sc
                        .cas(run_id, "counter", state.value.version, Value::I64(current + 1))
                        .is_ok()
                    {
                        success_count.fetch_add(1, Ordering::Relaxed);
                        return true;
                    }
                }
                false
            },
        );

        // Final value should equal number of successful increments
        let final_state = tp.state_cell.read(&run_id, "counter").unwrap().unwrap();
        let final_value = if let Value::I64(v) = final_state.value.value {
            v
        } else {
            panic!()
        };
        let total_success = success_count.load(Ordering::Relaxed);

        assert_eq!(
            final_value as u64, total_success,
            "Lost updates: final={}, success_count={}",
            final_value, total_success
        );
    }

    #[test]
    fn test_failed_cas_returns_current_version() {
        let tp = TestPrimitives::new();
        let run_id = tp.run_id;

        tp.state_cell.init(&run_id, "cell", values::int(0)).unwrap();

        // Update to v2
        tp.state_cell.set(&run_id, "cell", values::int(1)).unwrap();

        // CAS with old version should fail
        let result = tp.state_cell.cas(&run_id, "cell", 1, values::int(2));
        assert!(result.is_err());

        // Can read current version for retry
        let state = tp.state_cell.read(&run_id, "cell").unwrap().unwrap();
        assert_eq!(state.value.version, 2, "Should be able to read current version");
    }
}

// ============================================================================
// M3.14: Init Uniqueness
// ============================================================================
// init() fails if cell already exists.
// Second init() returns error, not overwrite.
// Use CAS for updates after init.
//
// What breaks if this fails?
// Silent overwrites. init() clobbers existing state.

mod init_uniqueness {
    use super::*;

    #[test]
    fn test_init_succeeds_for_new_cell() {
        let tp = TestPrimitives::new();
        let run_id = tp.run_id;

        let result = tp.state_cell.init(&run_id, "new_cell", values::int(0));
        assert!(result.is_ok(), "init() should succeed for new cell");
    }

    #[test]
    fn test_init_fails_for_existing_cell() {
        let tp = TestPrimitives::new();
        let run_id = tp.run_id;

        // First init succeeds
        tp.state_cell.init(&run_id, "cell", values::int(0)).unwrap();

        // Second init fails
        let result = tp.state_cell.init(&run_id, "cell", values::int(1));
        assert!(result.is_err(), "init() should fail for existing cell");
    }

    #[test]
    fn test_init_does_not_overwrite_value() {
        let tp = TestPrimitives::new();
        let run_id = tp.run_id;

        tp.state_cell
            .init(&run_id, "cell", values::int(42))
            .unwrap();

        // Try to init with different value
        let _ = tp.state_cell.init(&run_id, "cell", values::int(99));

        // Original value preserved
        let state = tp.state_cell.read(&run_id, "cell").unwrap().unwrap();
        assert_eq!(state.value.value, values::int(42), "init() should not overwrite");
    }

    #[test]
    fn test_init_does_not_reset_version() {
        let tp = TestPrimitives::new();
        let run_id = tp.run_id;

        tp.state_cell.init(&run_id, "cell", values::int(0)).unwrap();

        // Update several times
        for i in 1..=5 {
            tp.state_cell.set(&run_id, "cell", values::int(i)).unwrap();
        }

        let version_before = tp
            .state_cell
            .read(&run_id, "cell")
            .unwrap()
            .unwrap()
            .value
            .version;

        // Try init again
        let _ = tp.state_cell.init(&run_id, "cell", values::int(0));

        // Version not reset
        let version_after = tp
            .state_cell
            .read(&run_id, "cell")
            .unwrap()
            .unwrap()
            .value
            .version;
        assert_eq!(
            version_before, version_after,
            "init() should not reset version"
        );
    }

    #[test]
    fn test_concurrent_init_exactly_one_wins() {
        let tp = TestPrimitives::new();
        let run_id = tp.run_id;
        let sc = tp.state_cell.clone();

        // Multiple threads try to init same cell
        let results = concurrent::run_with_shared(10, (sc, run_id), |i, (sc, run_id)| {
            sc.init(run_id, "contested", values::int(i as i64)).is_ok()
        });

        let winners: usize = results.iter().filter(|&&won| won).count();
        assert_eq!(winners, 1, "Exactly one init() should succeed");
    }

    #[test]
    fn test_init_independent_per_run() {
        let tp = TestPrimitives::new();
        let run1 = tp.run_id;
        let run2 = RunId::new();

        // Same cell name, different runs - both should succeed
        tp.state_cell.init(&run1, "cell", values::int(1)).unwrap();
        tp.state_cell.init(&run2, "cell", values::int(2)).unwrap();

        let state1 = tp.state_cell.read(&run1, "cell").unwrap().unwrap();
        let state2 = tp.state_cell.read(&run2, "cell").unwrap().unwrap();

        assert_eq!(state1.value.value, values::int(1));
        assert_eq!(state2.value.value, values::int(2));
    }
}

// ============================================================================
// M3.15: Transition Speculative Execution
// ============================================================================
// transition() closure may be re-executed on OCC conflict.
// The system does NOT guarantee single invocation.
// Closure must be treated as pure (side effects will be multiplied).
//
// What breaks if this fails?
// Side effects multiply. HTTP request sent N times.

mod transition_speculative_execution {
    use super::*;

    #[test]
    fn test_transition_basic_operation() {
        let tp = TestPrimitives::new();
        let run_id = tp.run_id;

        tp.state_cell
            .init(&run_id, "counter", values::int(0))
            .unwrap();

        // Simple transition: increment
        let (result, _version) = tp
            .state_cell
            .transition(&run_id, "counter", |state| {
                let current = if let Value::I64(v) = &state.value {
                    *v
                } else {
                    0
                };
                Ok((Value::I64(current + 1), current + 1))
            })
            .unwrap();

        assert_eq!(result, 1);

        let state = tp.state_cell.read(&run_id, "counter").unwrap().unwrap();
        assert_eq!(state.value.value, values::int(1));
    }

    #[test]
    fn test_transition_sees_current_state() {
        let tp = TestPrimitives::new();
        let run_id = tp.run_id;

        tp.state_cell
            .init(&run_id, "cell", values::int(42))
            .unwrap();

        tp.state_cell
            .transition(&run_id, "cell", |state| {
                // Inside transition, state is the State struct directly
                assert_eq!(state.value, values::int(42));
                assert_eq!(state.version, 1);
                Ok((values::int(43), ()))
            })
            .unwrap();
    }

    #[test]
    fn test_transition_closure_may_be_called_multiple_times() {
        let tp = TestPrimitives::new();
        let run_id = tp.run_id;
        let sc = tp.state_cell.clone();

        tp.state_cell
            .init(&run_id, "counter", values::int(0))
            .unwrap();

        // Track call count
        let call_count = Arc::new(AtomicU64::new(0));
        let cc = call_count.clone();

        // Under contention, closure may be retried
        let _ = concurrent::run_with_shared(
            5,
            (sc.clone(), run_id, cc.clone()),
            |_, (sc, run_id, cc)| {
                for _ in 0..10 {
                    let _ = sc.transition(run_id, "counter", |state| {
                        cc.fetch_add(1, Ordering::Relaxed);
                        let current = if let Value::I64(v) = &state.value {
                            *v
                        } else {
                            0
                        };
                        Ok((Value::I64(current + 1), ()))
                    });
                }
            },
        );

        // call_count >= number of successful transitions
        // (may be greater due to retries)
        let final_state = tp.state_cell.read(&run_id, "counter").unwrap().unwrap();
        let final_value = if let Value::I64(v) = final_state.value.value {
            v
        } else {
            0
        };
        let total_calls = call_count.load(Ordering::Relaxed);

        assert!(
            total_calls >= final_value as u64,
            "Calls ({}) should be >= successful transitions ({})",
            total_calls,
            final_value
        );
    }

    #[test]
    fn test_transition_result_is_correct_despite_retries() {
        let tp = TestPrimitives::new();
        let run_id = tp.run_id;
        let sc = tp.state_cell.clone();
        let num_threads = 10;
        let ops_per_thread = 10;

        tp.state_cell
            .init(&run_id, "counter", values::int(0))
            .unwrap();

        // Each thread increments multiple times
        let _ = concurrent::run_with_shared(
            num_threads,
            (sc.clone(), run_id),
            move |_, (sc, run_id)| {
                for _ in 0..ops_per_thread {
                    let _ = sc.transition(run_id, "counter", |state| {
                        let current = if let Value::I64(v) = &state.value {
                            *v
                        } else {
                            0
                        };
                        Ok((Value::I64(current + 1), ()))
                    });
                }
            },
        );

        // Final value should be exactly num_threads * ops_per_thread
        let final_state = tp.state_cell.read(&run_id, "counter").unwrap().unwrap();
        let final_value = if let Value::I64(v) = final_state.value.value {
            v
        } else {
            0
        };

        assert_eq!(
            final_value,
            (num_threads * ops_per_thread) as i64,
            "Final value should be {}",
            num_threads * ops_per_thread
        );
    }

    #[test]
    fn test_transition_failure_rolls_back() {
        let tp = TestPrimitives::new();
        let run_id = tp.run_id;

        tp.state_cell.init(&run_id, "cell", values::int(0)).unwrap();

        // Transition that returns error
        let result =
            tp.state_cell.transition(&run_id, "cell", |_state| -> Result<(Value, ()), Error> {
                Err(Error::InvalidState("intentional failure".to_string()))
            });

        assert!(result.is_err());

        // Value should be unchanged
        let state = tp.state_cell.read(&run_id, "cell").unwrap().unwrap();
        assert_eq!(state.value.value, values::int(0));
        assert_eq!(state.value.version, 1);
    }

    #[test]
    fn test_transition_or_init() {
        let tp = TestPrimitives::new();
        let run_id = tp.run_id;

        // Use transition_or_init on non-existent cell
        let (result, _version) = tp
            .state_cell
            .transition_or_init(&run_id, "new_cell", values::int(0), |state| {
                let current = if let Value::I64(v) = &state.value {
                    *v
                } else {
                    0
                };
                Ok((Value::I64(current + 1), current + 1))
            })
            .unwrap();

        // Should have initialized to 0, then incremented to 1
        assert_eq!(result, 1);

        // Verify state
        let state = tp.state_cell.read(&run_id, "new_cell").unwrap().unwrap();
        assert_eq!(state.value.value, values::int(1));
    }

    #[test]
    fn test_delete_cell() {
        let tp = TestPrimitives::new();
        let run_id = tp.run_id;

        tp.state_cell
            .init(&run_id, "cell", values::int(42))
            .unwrap();

        // Delete returns true for existing cell
        assert!(tp.state_cell.delete(&run_id, "cell").unwrap());

        // Cell no longer exists
        assert!(tp.state_cell.read(&run_id, "cell").unwrap().is_none());

        // Delete returns false for non-existent cell
        assert!(!tp.state_cell.delete(&run_id, "cell").unwrap());
    }

    #[test]
    fn test_exists() {
        let tp = TestPrimitives::new();
        let run_id = tp.run_id;

        assert!(!tp.state_cell.exists(&run_id, "cell").unwrap());

        tp.state_cell.init(&run_id, "cell", values::int(0)).unwrap();

        assert!(tp.state_cell.exists(&run_id, "cell").unwrap());
    }
}
