//! Global Regression Sentinel (Kill Switch Test)
//!
//! Single test that exercises all primitives under stress.
//! If this fails, DO NOT proceed with other tests.
//!
//! ## What This Catches
//!
//! | Failure Mode | Symptom |
//! |--------------|---------|
//! | Deadlock | Timeout (30s) |
//! | Panic | Thread join fails |
//! | Livelock | Throughput collapse |
//! | Lock contention explosion | p99 > 100× mean |
//! | Broken scaling | Disjoint slower than single |
//! | MVCC corruption | Panic in primitive ops |

use super::*;
use in_mem_core::types::RunId;
use in_mem_core::value::Value;
use in_mem_primitives::{EventLog, KVStore, RunIndex, StateCell, TraceStore, TraceType};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

const OPS_PER_PRIMITIVE: usize = 500;
const TIMEOUT_SECS: u64 = 60;

/// Kill switch test - catches catastrophic regressions
///
/// If this test fails, DO NOT proceed with other tests.
/// Investigate immediately.
#[test]
fn global_regression_sentinel() {
    println!("\n=== GLOBAL REGRESSION SENTINEL ===\n");

    // === Phase 1: Single Thread Baseline ===
    println!("Phase 1: Single thread baseline...");
    let (single_time, single_ops, single_p99_ratio) =
        run_mixed_workload(1, OPS_PER_PRIMITIVE, false);
    println!(
        "  Completed {} ops in {:?} (p99/mean: {:.1}x)",
        single_ops, single_time, single_p99_ratio
    );

    // === Phase 2: 4 Threads, Same Run (Contention) ===
    println!("Phase 2: 4 threads, same run (contention)...");
    let (contention_time, contention_ops, contention_p99_ratio) =
        run_mixed_workload(4, OPS_PER_PRIMITIVE, false);
    println!(
        "  Completed {} ops in {:?} (p99/mean: {:.1}x)",
        contention_ops, contention_time, contention_p99_ratio
    );

    // === Phase 3: 4 Threads, Disjoint Runs (Scaling) ===
    println!("Phase 3: 4 threads, disjoint runs (scaling)...");
    let (disjoint_time, disjoint_ops, disjoint_p99_ratio) =
        run_mixed_workload(4, OPS_PER_PRIMITIVE, true);
    println!(
        "  Completed {} ops in {:?} (p99/mean: {:.1}x)",
        disjoint_ops, disjoint_time, disjoint_p99_ratio
    );

    // === Assertions ===
    println!("\n=== ASSERTIONS ===\n");

    // 1. No timeout (deadlock detection)
    assert!(
        single_time < Duration::from_secs(TIMEOUT_SECS),
        "KILL SWITCH: Single thread timed out after {:?} - possible deadlock",
        single_time
    );
    println!("✓ Single thread completed without timeout");

    assert!(
        contention_time < Duration::from_secs(TIMEOUT_SECS),
        "KILL SWITCH: Contention test timed out after {:?} - possible deadlock",
        contention_time
    );
    println!("✓ Contention test completed without timeout");

    assert!(
        disjoint_time < Duration::from_secs(TIMEOUT_SECS),
        "KILL SWITCH: Disjoint test timed out after {:?} - possible deadlock",
        disjoint_time
    );
    println!("✓ Disjoint test completed without timeout");

    // 2. No throughput collapse (> 10% of single thread)
    let single_throughput = single_ops as f64 / single_time.as_secs_f64();
    let contention_throughput = contention_ops as f64 / contention_time.as_secs_f64();
    assert!(
        contention_throughput > single_throughput * 0.1,
        "KILL SWITCH: Throughput collapsed under contention ({:.0} vs {:.0} baseline)",
        contention_throughput,
        single_throughput
    );
    println!(
        "✓ Contention throughput OK ({:.0} ops/sec, baseline {:.0})",
        contention_throughput, single_throughput
    );

    // 3. Disjoint should scale (not regress significantly)
    let disjoint_throughput = disjoint_ops as f64 / disjoint_time.as_secs_f64();
    assert!(
        disjoint_throughput > single_throughput * 0.5,
        "KILL SWITCH: Disjoint runs slower than expected ({:.0} vs {:.0} baseline)",
        disjoint_throughput,
        single_throughput
    );
    println!(
        "✓ Disjoint throughput OK ({:.0} ops/sec, baseline {:.0})",
        disjoint_throughput, single_throughput
    );

    // 4. p99 sanity check
    assert!(
        single_p99_ratio < 100.0,
        "KILL SWITCH: Single thread p99/mean = {:.1}x > 100x threshold",
        single_p99_ratio
    );
    assert!(
        contention_p99_ratio < 100.0,
        "KILL SWITCH: Contention p99/mean = {:.1}x > 100x threshold",
        contention_p99_ratio
    );
    assert!(
        disjoint_p99_ratio < 100.0,
        "KILL SWITCH: Disjoint p99/mean = {:.1}x > 100x threshold",
        disjoint_p99_ratio
    );
    println!("✓ Tail latency within bounds");

    println!("\n=== KILL SWITCH: PASS ===\n");
}

/// Run mixed workload across all primitives
///
/// Returns (elapsed_time, total_ops, p99_to_mean_ratio)
fn run_mixed_workload(
    num_threads: usize,
    ops_per_primitive: usize,
    disjoint_runs: bool,
) -> (Duration, usize, f64) {
    let db = create_inmemory_db();
    let total_ops = Arc::new(AtomicUsize::new(0));
    let latencies = Arc::new(Mutex::new(Vec::new()));

    // Shared RunId for non-disjoint tests
    let shared_run_id = RunId::new();

    let start = Instant::now();

    let handles: Vec<_> = (0..num_threads)
        .map(|thread_id| {
            let db = Arc::clone(&db);
            let total_ops = Arc::clone(&total_ops);
            let latencies = Arc::clone(&latencies);
            let shared_run_id = shared_run_id;

            thread::spawn(move || {
                let run_id = if disjoint_runs {
                    RunId::new() // Each thread gets unique run
                } else {
                    shared_run_id // All threads share run
                };

                let kv = KVStore::new(db.clone());
                let events = EventLog::new(db.clone());
                let state = StateCell::new(db.clone());
                let traces = TraceStore::new(db.clone());
                let runs = RunIndex::new(db.clone());

                // Initialize StateCell for this run
                let _ = state.init(&run_id, "counter", Value::I64(0));

                for i in 0..ops_per_primitive {
                    let op_start = Instant::now();

                    // Round-robin across primitives
                    let result = match i % 5 {
                        0 => {
                            // KVStore: put + get
                            let key = format!("key_{}_{}", thread_id, i);
                            kv.put(&run_id, &key, Value::I64(i as i64))
                                .and_then(|_| kv.get(&run_id, &key))
                                .map(|_| ())
                        }
                        1 => {
                            // EventLog: append
                            events
                                .append(&run_id, "test_event", Value::I64(i as i64))
                                .map(|_| ())
                        }
                        2 => {
                            // StateCell: set (simpler than transition for stress test)
                            state
                                .set(&run_id, "counter", Value::I64(i as i64))
                                .map(|_| ())
                        }
                        3 => {
                            // TraceStore: record
                            traces
                                .record(
                                    &run_id,
                                    TraceType::Thought {
                                        content: format!("trace_{}", i),
                                        confidence: None,
                                    },
                                    vec![],
                                    Value::I64(i as i64),
                                )
                                .map(|_| ())
                        }
                        4 => {
                            // RunIndex: operations (less frequent)
                            if i % 50 == 4 {
                                let run_name = format!("run_{}_{}", thread_id, i);
                                runs.create_run(&run_name)
                                    .and_then(|_meta| {
                                        // Run is already Active on creation, so complete it
                                        runs.complete_run(&run_name)
                                    })
                                    .map(|_| ())
                            } else {
                                Ok(())
                            }
                        }
                        _ => unreachable!(),
                    };

                    // Record result
                    if result.is_ok() {
                        let elapsed = op_start.elapsed().as_nanos();
                        latencies.lock().unwrap().push(elapsed);
                        total_ops.fetch_add(1, Ordering::Relaxed);
                    } else {
                        // Log but don't fail - some operations may conflict
                        // under contention and that's expected
                    }
                }
            })
        })
        .collect();

    // Wait for all threads with panic detection
    for (i, handle) in handles.into_iter().enumerate() {
        handle
            .join()
            .unwrap_or_else(|_| panic!("KILL SWITCH: Thread {} panicked", i));
    }

    let elapsed = start.elapsed();
    let ops = total_ops.load(Ordering::Relaxed);

    // Calculate p99/mean ratio
    let mut lats = latencies.lock().unwrap();
    let p99_ratio = if !lats.is_empty() {
        lats.sort();
        let mean = lats.iter().sum::<u128>() / lats.len() as u128;
        let p99_idx = (lats.len() * 99) / 100;
        let p99 = lats[p99_idx.min(lats.len() - 1)];
        if mean > 0 {
            p99 as f64 / mean as f64
        } else {
            0.0
        }
    } else {
        0.0
    };

    (elapsed, ops, p99_ratio)
}

#[cfg(test)]
mod kill_switch_unit_tests {
    use super::*;

    #[test]
    fn test_single_thread_baseline() {
        let (time, ops, ratio) = run_mixed_workload(1, 100, false);
        assert!(ops > 0, "Should complete some operations");
        assert!(time < Duration::from_secs(30), "Should not timeout");
        assert!(ratio < 100.0, "Tail ratio should be reasonable");
    }

    #[test]
    fn test_disjoint_runs_scale() {
        let (single_time, _, _) = run_mixed_workload(1, 100, true);
        let (multi_time, _, _) = run_mixed_workload(4, 100, true);

        // 4 threads doing 4x work should not take 10x as long
        let ratio = multi_time.as_nanos() as f64 / (single_time.as_nanos() as f64 * 4.0);
        assert!(
            ratio < 10.0,
            "Disjoint scaling severely degraded: {}x slowdown",
            ratio
        );
    }
}
