//! Concurrent Primitive Stress Tests (Tier 3)
//!
//! These tests are NOT run by default (marked with #[ignore]).
//! Run with: cargo test --test m3_comprehensive stress -- --ignored
//!
//! Focus areas:
//! - High-concurrency KV operations
//! - High-concurrency EventLog appends
//! - High-concurrency StateCell CAS
//! - Cross-primitive stress

use crate::test_utils::{concurrent, values, TestPrimitives};
use strata_core::contract::Version;
use strata_core::value::Value;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

// =============================================================================
// KVStore Stress
// =============================================================================

mod kvstore_stress {
    use super::*;

    #[test]
    #[ignore]
    fn stress_kv_concurrent_writes_100_threads() {
        let tp = Arc::new(TestPrimitives::new());
        let run_id = tp.run_id;
        let num_threads = 100;
        let writes_per_thread = 100;

        let results = concurrent::run_with_shared(
            num_threads,
            (tp.clone(), run_id),
            move |i, (tp, run_id)| {
                let mut successes = 0;
                for j in 0..writes_per_thread {
                    let key = format!("key_{}_{}", i, j);
                    if tp
                        .kv
                        .put(run_id, &key, values::int((i * 1000 + j) as i64))
                        .is_ok()
                    {
                        successes += 1;
                    }
                }
                successes
            },
        );

        // All writes should succeed
        let total: usize = results.iter().sum();
        assert_eq!(total, num_threads * writes_per_thread);

        // Verify all keys present
        let keys = tp.kv.list(&run_id, None).unwrap();
        assert_eq!(keys.len(), num_threads * writes_per_thread);
    }

    #[test]
    #[ignore]
    fn stress_kv_concurrent_reads_while_writing() {
        let tp = Arc::new(TestPrimitives::new());
        let run_id = tp.run_id;

        // Pre-populate some data
        for i in 0..100 {
            tp.kv
                .put(&run_id, &format!("key_{}", i), values::int(i))
                .unwrap();
        }

        let num_threads = 50;
        let ops_per_thread = 100;

        // Half threads read, half threads write
        let results = concurrent::run_with_shared(
            num_threads,
            (tp.clone(), run_id),
            move |i, (tp, run_id)| {
                let mut successes = 0;
                for j in 0..ops_per_thread {
                    if i % 2 == 0 {
                        // Reader
                        let key = format!("key_{}", j % 100);
                        if tp.kv.get(run_id, &key).is_ok() {
                            successes += 1;
                        }
                    } else {
                        // Writer
                        let key = format!("new_key_{}_{}", i, j);
                        if tp
                            .kv
                            .put(run_id, &key, values::int((i * 1000 + j) as i64))
                            .is_ok()
                        {
                            successes += 1;
                        }
                    }
                }
                successes
            },
        );

        let total: usize = results.iter().sum();
        assert_eq!(total, num_threads * ops_per_thread);
    }

    #[test]
    #[ignore]
    fn stress_kv_high_contention_single_key() {
        let tp = Arc::new(TestPrimitives::new());
        let run_id = tp.run_id;
        let num_threads = 100;
        let writes_per_thread = 50;

        // All threads write to same key
        let results = concurrent::run_with_shared(
            num_threads,
            (tp.clone(), run_id),
            move |i, (tp, run_id)| {
                let mut successes = 0;
                for j in 0..writes_per_thread {
                    if tp
                        .kv
                        .put(run_id, "single_key", values::int((i * 1000 + j) as i64))
                        .is_ok()
                    {
                        successes += 1;
                    }
                }
                successes
            },
        );

        // All should succeed (last writer wins)
        let total: usize = results.iter().sum();
        assert_eq!(total, num_threads * writes_per_thread);

        // Key should exist
        assert!(tp.kv.get(&run_id, "single_key").unwrap().is_some());
    }
}

// =============================================================================
// EventLog Stress
// =============================================================================

mod eventlog_stress {
    use super::*;

    #[test]
    #[ignore]
    fn stress_eventlog_concurrent_appends_50_threads() {
        let tp = Arc::new(TestPrimitives::new());
        let run_id = tp.run_id;
        let num_threads = 50;
        let appends_per_thread = 100;

        let results = concurrent::run_with_shared(
            num_threads,
            (tp.clone(), run_id),
            move |i, (tp, run_id)| {
                let mut sequences = Vec::new();
                for _ in 0..appends_per_thread {
                    if let Ok(version) =
                        tp.event_log
                            .append(run_id, &format!("thread_{}", i), values::null())
                    {
                        if let Version::Sequence(seq) = version {
                            sequences.push(seq);
                        }
                    }
                }
                sequences
            },
        );

        // All appends should succeed
        let total: usize = results.iter().map(|v| v.len()).sum();
        assert_eq!(total, num_threads * appends_per_thread);

        // Verify all sequences are contiguous (no gaps)
        let len = tp.event_log.len(&run_id).unwrap();
        assert_eq!(len, (num_threads * appends_per_thread) as u64);

        // Verify chain integrity
        let result = tp.event_log.verify_chain(&run_id).unwrap();
        assert!(result.is_valid);
    }

    #[test]
    #[ignore]
    fn stress_eventlog_long_chain() {
        let tp = TestPrimitives::new();
        let run_id = tp.run_id;
        let num_events = 10_000;

        for i in 0..num_events {
            tp.event_log
                .append(&run_id, "event", values::int(i))
                .unwrap();
        }

        // Verify count
        assert_eq!(tp.event_log.len(&run_id).unwrap(), num_events as u64);

        // Verify chain integrity
        let result = tp.event_log.verify_chain(&run_id).unwrap();
        assert!(result.is_valid);

        // Verify sequences are contiguous
        let events = tp
            .event_log
            .read_range(&run_id, 0, num_events as u64)
            .unwrap();
        for (i, event) in events.iter().enumerate() {
            assert_eq!(event.value.sequence, i as u64);
        }
    }

    #[test]
    #[ignore]
    fn stress_eventlog_read_while_appending() {
        let tp = Arc::new(TestPrimitives::new());
        let run_id = tp.run_id;

        // Pre-populate some events
        for i in 0..100 {
            tp.event_log
                .append(&run_id, "init", values::int(i))
                .unwrap();
        }

        let num_threads = 30;
        let ops_per_thread = 50;

        let results = concurrent::run_with_shared(
            num_threads,
            (tp.clone(), run_id),
            move |i, (tp, run_id)| {
                let mut ops = 0;
                for _ in 0..ops_per_thread {
                    if i % 3 == 0 {
                        // Append
                        if tp.event_log.append(run_id, "new", values::null()).is_ok() {
                            ops += 1;
                        }
                    } else if i % 3 == 1 {
                        // Read
                        if tp.event_log.read(run_id, 0).is_ok() {
                            ops += 1;
                        }
                    } else {
                        // Read range
                        if tp.event_log.read_range(run_id, 0, 10).is_ok() {
                            ops += 1;
                        }
                    }
                }
                ops
            },
        );

        let total: usize = results.iter().sum();
        assert_eq!(total, num_threads * ops_per_thread);

        // Chain should still be valid
        let result = tp.event_log.verify_chain(&run_id).unwrap();
        assert!(result.is_valid);
    }
}

// =============================================================================
// StateCell Stress
// =============================================================================

mod statecell_stress {
    use super::*;

    #[test]
    #[ignore]
    fn stress_statecell_concurrent_cas_100_threads() {
        let tp = Arc::new(TestPrimitives::new());
        let run_id = tp.run_id;

        tp.state_cell
            .init(&run_id, "counter", values::int(0))
            .unwrap();

        let num_threads = 100;
        let increments_per_thread = 50;

        // Each thread tries to increment via transition
        let results = concurrent::run_with_shared(
            num_threads,
            (tp.clone(), run_id),
            move |_, (tp, run_id)| {
                let mut successes = 0;
                for _ in 0..increments_per_thread {
                    let result = tp.state_cell.transition(run_id, "counter", |state| {
                        if let Value::Int(n) = &state.value {
                            Ok((values::int(n + 1), ()))
                        } else {
                            Ok((values::int(1), ()))
                        }
                    });
                    if result.is_ok() {
                        successes += 1;
                    }
                }
                successes
            },
        );

        // All transitions should succeed
        let total: i32 = results.iter().sum();
        assert_eq!(total, (num_threads * increments_per_thread) as i32);

        // Final value should equal total increments (no lost updates)
        let state = tp.state_cell.read(&run_id, "counter").unwrap().unwrap();
        assert_eq!(
            state.value.value,
            values::int((num_threads * increments_per_thread) as i64)
        );
    }

    #[test]
    #[ignore]
    fn stress_statecell_multiple_cells_concurrent() {
        let tp = Arc::new(TestPrimitives::new());
        let run_id = tp.run_id;
        let num_cells = 10;

        // Initialize cells
        for i in 0..num_cells {
            tp.state_cell
                .init(&run_id, &format!("cell_{}", i), values::int(0))
                .unwrap();
        }

        let num_threads = 50;
        let ops_per_thread = 100;

        let results = concurrent::run_with_shared(
            num_threads,
            (tp.clone(), run_id),
            move |i, (tp, run_id)| {
                let mut successes = 0;
                for j in 0..ops_per_thread {
                    let cell_name = format!("cell_{}", (i + j) % num_cells);
                    let result = tp.state_cell.transition(run_id, &cell_name, |state| {
                        if let Value::Int(n) = &state.value {
                            Ok((values::int(n + 1), ()))
                        } else {
                            Ok((values::int(1), ()))
                        }
                    });
                    if result.is_ok() {
                        successes += 1;
                    }
                }
                successes
            },
        );

        let total: i32 = results.iter().sum();
        assert_eq!(total, (num_threads * ops_per_thread) as i32);
    }

    #[test]
    #[ignore]
    fn stress_statecell_version_never_decreases() {
        let tp = Arc::new(TestPrimitives::new());
        let run_id = tp.run_id;

        tp.state_cell.init(&run_id, "cell", values::int(0)).unwrap();

        let num_threads = 50;
        let ops_per_thread = 100;

        // Test per-thread version monotonicity: within each thread, versions should increase
        // Note: Global monotonicity across threads is NOT guaranteed by OCC - threads can
        // return from transition() in different order than they committed due to scheduling
        let results = concurrent::run_with_shared(
            num_threads,
            (tp.clone(), run_id),
            move |_, (tp, run_id)| {
                let mut last_version = 0u64;
                let mut version_decreases = 0;

                for _ in 0..ops_per_thread {
                    let result = tp
                        .state_cell
                        .transition(run_id, "cell", |_| Ok((values::int(1), ())));

                    if let Ok((_, new_version)) = result {
                        // Per-thread: each successful transition should return a higher version
                        // than the previous successful transition by THIS thread
                        if new_version.value <= last_version {
                            version_decreases += 1;
                        }
                        last_version = new_version.value;
                    }
                }
                version_decreases
            },
        );

        // No per-thread version decreases should have been observed
        let total_decreases: i32 = results.iter().sum();
        assert_eq!(
            total_decreases, 0,
            "Per-thread version monotonicity violated!"
        );

        // Also verify final version equals number of successful transitions
        let final_state = tp.state_cell.read(&run_id, "cell").unwrap().unwrap();
        let expected_version = (num_threads * ops_per_thread + 1) as u64; // +1 for init
        assert_eq!(
            final_state.value.version, expected_version,
            "Final version should equal total transitions + 1"
        );
    }
}

// =============================================================================
// Cross-Primitive Stress
// =============================================================================

mod cross_primitive_stress {
    use super::*;

    #[test]
    #[ignore]
    fn stress_cross_primitive_operations() {
        let tp = Arc::new(TestPrimitives::new());
        let run_id = tp.run_id;

        // Initialize state cell
        tp.state_cell
            .init(&run_id, "state", values::int(0))
            .unwrap();

        let num_threads = 50;
        let ops_per_thread = 50;

        let results = concurrent::run_with_shared(
            num_threads,
            (tp.clone(), run_id),
            move |i, (tp, run_id)| {
                let mut successes = 0;
                for j in 0..ops_per_thread {
                    // KV put
                    if tp
                        .kv
                        .put(run_id, &format!("key_{}_{}", i, j), values::int(1))
                        .is_ok()
                    {
                        successes += 1;
                    }

                    // EventLog append
                    if tp
                        .event_log
                        .append(run_id, "op", values::int((i * 100 + j) as i64))
                        .is_ok()
                    {
                        successes += 1;
                    }

                    // StateCell transition
                    if tp
                        .state_cell
                        .transition(run_id, "state", |state| {
                            if let Value::Int(n) = &state.value {
                                Ok((values::int(n + 1), ()))
                            } else {
                                Ok((values::int(1), ()))
                            }
                        })
                        .is_ok()
                    {
                        successes += 1;
                    }
                }
                successes
            },
        );

        // All operations should succeed
        let total: i32 = results.iter().sum();
        assert_eq!(total, (num_threads * ops_per_thread * 3) as i32);

        // Verify counts
        let kv_count = tp.kv.list(&run_id, None).unwrap().len();
        let event_count = tp.event_log.len(&run_id).unwrap() as usize;

        assert_eq!(kv_count, num_threads * ops_per_thread);
        assert_eq!(event_count, num_threads * ops_per_thread);

        // StateCell should equal total transitions
        let state = tp.state_cell.read(&run_id, "state").unwrap().unwrap();
        assert_eq!(
            state.value.value,
            values::int((num_threads * ops_per_thread) as i64)
        );
    }

    #[test]
    #[ignore]
    fn stress_many_runs_concurrent() {
        let tp = Arc::new(TestPrimitives::new());
        let num_threads = 30;
        let runs_per_thread = 10;
        let ops_per_run = 10;

        let results = concurrent::run_with_shared(num_threads, tp.clone(), move |i, tp| {
            let mut total_ops = 0;
            for _ in 0..runs_per_thread {
                let run = tp.new_run();
                for j in 0..ops_per_run {
                    if tp
                        .kv
                        .put(&run, &format!("key_{}", j), values::int(i as i64))
                        .is_ok()
                    {
                        total_ops += 1;
                    }
                    if tp.event_log.append(&run, "event", values::null()).is_ok() {
                        total_ops += 1;
                    }
                }
            }
            total_ops
        });

        let total: i32 = results.iter().sum();
        assert_eq!(
            total,
            (num_threads * runs_per_thread * ops_per_run * 2) as i32
        );
    }
}

// =============================================================================
// Run Lifecycle Stress
// =============================================================================

mod run_lifecycle_stress {
    use super::*;
    use strata_primitives::RunStatus;

    #[test]
    #[ignore]
    fn stress_create_many_runs() {
        let tp = TestPrimitives::new();
        let num_runs = 1000;

        let mut run_names = Vec::with_capacity(num_runs);
        for i in 0..num_runs {
            let meta = tp.run_index.create_run(&format!("run-{}", i)).unwrap();
            run_names.push(meta.value.name);
        }

        // All runs should exist
        for run_name in &run_names {
            assert!(tp.run_index.get_run(run_name).unwrap().is_some());
        }

        // Count should reflect
        let count = tp.run_index.count().unwrap();
        assert!(count >= num_runs);
    }

    #[test]
    #[ignore]
    fn stress_rapid_status_transitions() {
        let tp = TestPrimitives::new();
        let num_runs = 100;

        let run_names: Vec<_> = (0..num_runs)
            .map(|i| tp.run_index.create_run(&format!("run-{}", i)).unwrap().value.name)
            .collect();

        // Rapidly transition each run through lifecycle
        for run_name in &run_names {
            // Active -> Paused
            tp.run_index
                .update_status(run_name, RunStatus::Paused)
                .unwrap();
            // Paused -> Active
            tp.run_index
                .update_status(run_name, RunStatus::Active)
                .unwrap();
            // Active -> Completed
            tp.run_index.complete_run(run_name).unwrap();
            // Completed -> Archived
            tp.run_index.archive_run(run_name).unwrap();
        }

        // All should be archived
        for run_name in &run_names {
            let run = tp.run_index.get_run(run_name).unwrap().unwrap();
            assert_eq!(run.value.status, RunStatus::Archived);
        }
    }

    #[test]
    #[ignore]
    fn stress_cascading_delete_large_run() {
        let tp = TestPrimitives::new();
        let run_id = tp.new_run();
        let run_meta = tp.run_index.create_run("large-run").unwrap();

        // Create large amount of data
        let num_kv = 10_000;
        let num_events = 1_000;
        for i in 0..num_kv {
            tp.kv
                .put(&run_id, &format!("key_{}", i), values::int(i as i64))
                .unwrap();
        }

        for _ in 0..num_events {
            tp.event_log
                .append(&run_id, "event", values::null())
                .unwrap();
        }

        // Delete the run from index
        tp.run_index.delete_run(&run_meta.value.name).unwrap();

        // Note: Deleting from RunIndex removes metadata, but not primitive data
        // Data deletion would be a separate cleanup operation
        // This test just verifies the delete doesn't error with large data present
    }
}
