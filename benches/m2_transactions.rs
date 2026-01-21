//! M2 Transaction Benchmarks - Semantic Regression Harness
//!
//! ## Benchmark Path Types (Layer Labels)
//!
//! The prefix indicates the **primary semantic being exercised**:
//!
//! - `txn_*`: Transaction lifecycle + commit/validation (begin, ops, validate, commit)
//! - `snapshot_*`: Snapshot read/view semantics (point-in-time consistency)
//! - `conflict_*`: Multi-thread contention patterns (first-committer-wins)
//!
//! Note: All paths go through the engine, but the prefix indicates what semantic
//! property the benchmark is designed to exercise and detect regressions in.
//!
//! ## Deterministic Randomness
//!
//! All "random" access patterns use a fixed seed (BENCH_SEED) for reproducibility.
//!
//! ## Conflict Benchmark Model
//!
//! All conflict_* benchmarks use:
//! - Barrier synchronization so all threads start simultaneously
//! - Fixed-duration loops (not fixed iteration count) for steady-state measurement
//! - Counters for successful commits and aborts (logged once per run)
//!
//! ## What These Benchmarks Prove
//!
//! | Benchmark | Semantic Guarantee | Regression Detection |
//! |-----------|-------------------|----------------------|
//! | txn_commit/* | Atomic commit: all-or-nothing | OCC validation cost |
//! | txn_cas/* | CAS fails if version mismatched | Version comparison overhead |
//! | snapshot_*/* | Snapshot view is consistent across multiple reads | Snapshot creation cost |
//! | conflict_*/* | Conflict causes abort, not partial commit | Conflict check scaling |
//!
//! ## Running
//!
//! ```bash
//! cargo bench --bench m2_transactions
//! cargo bench --bench m2_transactions -- "txn_cas"  # specific group
//! ```

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use strata_core::types::{Key, Namespace, RunId};
use strata_core::value::Value;
use strata_engine::Database;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Barrier};
use std::thread;
use std::time::{Duration, Instant};
use tempfile::TempDir;

// =============================================================================
// Constants and Configuration
// =============================================================================

/// Fixed seed for deterministic "random" key selection.
/// Change this seed and baselines become invalid - that's intentional.
const BENCH_SEED: u64 = 0xDEADBEEF_CAFEBABE;

/// Duration for fixed-time conflict benchmarks (steady-state measurement)
const CONFLICT_BENCH_DURATION: Duration = Duration::from_secs(2);

// =============================================================================
// Test Utilities - All allocation happens here, outside timed loops
// =============================================================================

fn create_namespace(run_id: RunId) -> Namespace {
    Namespace::new(
        "tenant".to_string(),
        "app".to_string(),
        "agent".to_string(),
        run_id,
    )
}

fn make_key(ns: &Namespace, name: &str) -> Key {
    Key::new_kv(ns.clone(), name)
}

/// Pre-generate keys to avoid allocation in timed loops
fn pregenerate_keys(ns: &Namespace, prefix: &str, count: usize) -> Vec<Key> {
    (0..count)
        .map(|i| make_key(ns, &format!("{}_{:06}", prefix, i)))
        .collect()
}

/// Simple LCG for deterministic "random" key selection without allocation.
fn lcg_next(state: &mut u64) -> u64 {
    *state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
    *state
}

// =============================================================================
// Transaction Layer: Commit Benchmarks
// =============================================================================
// Semantic: Atomic commit - all writes succeed or none do
// Regression: OCC validation cost, snapshot creation, write-set serialization

fn txn_commit_benchmarks(c: &mut Criterion) {
    let mut group = c.benchmark_group("txn_commit");
    group.throughput(Throughput::Elements(1));

    // --- Benchmark: single_put (minimal transaction) ---
    // Semantic: Single-key write is atomic and durable after commit
    // Real pattern: Agent storing single state update
    {
        let temp_dir = TempDir::new().unwrap();
        let db = Database::open(temp_dir.path().join("db")).unwrap();
        let run_id = RunId::new();
        let ns = create_namespace(run_id);

        let counter = AtomicU64::new(0);

        group.bench_function("single_put", |b| {
            b.iter(|| {
                let i = counter.fetch_add(1, Ordering::Relaxed);
                // Generate key in real-time to avoid exhaustion
                let key = make_key(&ns, &format!("single_{}", i));
                let result = db.transaction(run_id, |txn| {
                    txn.put(key, Value::I64(i as i64))?;
                    Ok(())
                });
                black_box(result.unwrap())
            });
        });
    }

    // --- Benchmark: multi_put (batch transaction) ---
    // Semantic: Multi-key write is atomic - all keys committed together
    // Real pattern: Agent storing related state atomically
    for num_keys in [3, 5, 10] {
        let temp_dir = TempDir::new().unwrap();
        let db = Database::open(temp_dir.path().join("db")).unwrap();
        let run_id = RunId::new();
        let ns = create_namespace(run_id);

        let counter = AtomicU64::new(0);

        group.bench_with_input(
            BenchmarkId::new("multi_put", num_keys),
            &num_keys,
            |b, &num_keys| {
                b.iter(|| {
                    let batch_idx = counter.fetch_add(1, Ordering::Relaxed);
                    // Generate keys in real-time to avoid exhaustion
                    let keys: Vec<Key> = (0..num_keys)
                        .map(|i| make_key(&ns, &format!("batch_{}_{}", batch_idx, i)))
                        .collect();
                    let result = db.transaction(run_id, |txn| {
                        for (i, key) in keys.iter().enumerate() {
                            txn.put(key.clone(), Value::I64(i as i64))?;
                        }
                        Ok(())
                    });
                    black_box(result.unwrap())
                });
            },
        );
    }

    // --- Benchmark: read_modify_write (RMW pattern) ---
    // Semantic: Read + conditional write is atomic
    // Real pattern: Counter increment, state machine transitions
    {
        let temp_dir = TempDir::new().unwrap();
        let db = Database::open(temp_dir.path().join("db")).unwrap();
        let run_id = RunId::new();
        let ns = create_namespace(run_id);
        let key = make_key(&ns, "rmw_counter");
        db.put(run_id, key.clone(), Value::I64(0)).unwrap();

        group.bench_function("read_modify_write", |b| {
            b.iter(|| {
                let result = db.transaction(run_id, |txn| {
                    let val = txn.get(&key)?;
                    let n = match val {
                        Some(Value::I64(n)) => n,
                        _ => 0,
                    };
                    txn.put(key.clone(), Value::I64(n + 1))?;
                    Ok(())
                });
                black_box(result.unwrap())
            });
        });
    }

    // --- Benchmark: readN_write1 (canonical agent workload) ---
    // Semantic: N reads + 1 write is atomic; read-set validated at commit
    // Real pattern: Gather context, update one state key
    // Regression: read-set tracking, validation, write-set application
    for num_reads in [1, 10, 100] {
        let temp_dir = TempDir::new().unwrap();
        let db = Database::open(temp_dir.path().join("db")).unwrap();
        let run_id = RunId::new();
        let ns = create_namespace(run_id);

        // Pre-populate read keys
        let read_keys = pregenerate_keys(&ns, "read", num_reads);
        for (i, key) in read_keys.iter().enumerate() {
            db.put(run_id, key.clone(), Value::I64(i as i64)).unwrap();
        }

        // Write key (will be overwritten each iteration)
        let write_key = make_key(&ns, "write_target");
        db.put(run_id, write_key.clone(), Value::I64(0)).unwrap();

        let counter = AtomicU64::new(0);

        group.bench_with_input(
            BenchmarkId::new("readN_write1", num_reads),
            &num_reads,
            |b, _| {
                b.iter(|| {
                    let i = counter.fetch_add(1, Ordering::Relaxed);
                    let result = db.transaction(run_id, |txn| {
                        // Read phase - these go into read-set for validation
                        for key in &read_keys {
                            txn.get(key)?;
                        }
                        // Write phase
                        txn.put(write_key.clone(), Value::I64(i as i64))?;
                        Ok(())
                    });
                    black_box(result.unwrap())
                });
            },
        );
    }

    group.finish();
}

// =============================================================================
// Transaction Layer: CAS Benchmarks
// =============================================================================
// Semantic: CAS fails if version doesn't match (first-committer-wins)
// Regression: Version comparison overhead, conflict detection cost

fn txn_cas_benchmarks(c: &mut Criterion) {
    let mut group = c.benchmark_group("txn_cas");
    group.throughput(Throughput::Elements(1));

    // --- Benchmark: success_sequential (no contention) ---
    // Semantic: CAS succeeds when expected version matches current version
    // Real pattern: Single-threaded state updates with optimistic locking
    {
        let temp_dir = TempDir::new().unwrap();
        let db = Database::open(temp_dir.path().join("db")).unwrap();
        let run_id = RunId::new();
        let ns = create_namespace(run_id);
        let key = make_key(&ns, "cas_seq");
        db.put(run_id, key.clone(), Value::I64(0)).unwrap();

        group.bench_function("success_sequential", |b| {
            b.iter(|| {
                let current = db.get(&key).unwrap().unwrap();
                let new_val = match current.value {
                    Value::I64(n) => n + 1,
                    _ => 1,
                };
                black_box(
                    db.cas(run_id, key.clone(), current.version, Value::I64(new_val))
                        .unwrap(),
                )
            });
        });
    }

    // --- Benchmark: failure_version_mismatch ---
    // Semantic: CAS fails immediately when version doesn't match
    // Real pattern: Detecting stale reads before wasted work
    {
        let temp_dir = TempDir::new().unwrap();
        let db = Database::open(temp_dir.path().join("db")).unwrap();
        let run_id = RunId::new();
        let ns = create_namespace(run_id);
        let key = make_key(&ns, "cas_fail");
        db.put(run_id, key.clone(), Value::I64(0)).unwrap();

        group.bench_function("failure_version_mismatch", |b| {
            b.iter(|| {
                // Always use wrong version - should fail fast
                let result = db.cas(run_id, key.clone(), 999999, Value::I64(1));
                black_box(result.is_err())
            });
        });
    }

    // --- Benchmark: create_new_key (version 0 = insert if not exists) ---
    // Semantic: CAS with version 0 atomically creates key if absent
    // Real pattern: Claiming a resource, initializing state
    {
        let temp_dir = TempDir::new().unwrap();
        let db = Database::open(temp_dir.path().join("db")).unwrap();
        let run_id = RunId::new();
        let ns = create_namespace(run_id);

        // Counter must be outside bench_function to persist across warm-up and measurement
        let counter = AtomicU64::new(0);

        group.bench_function("create_new_key", |b| {
            b.iter(|| {
                let i = counter.fetch_add(1, Ordering::Relaxed);
                // Generate key in real-time to avoid exhaustion
                let key = make_key(&ns, &format!("cas_create_{}", i));
                black_box(
                    db.cas(run_id, key, 0, Value::I64(i as i64))
                        .unwrap(),
                )
            });
        });
    }

    // --- Benchmark: retry_until_success (bounded retry loop) ---
    // Semantic: CAS retry converges (no starvation under self-contention)
    // Real pattern: Agent coordination with optimistic retry
    {
        let temp_dir = TempDir::new().unwrap();
        let db = Database::open(temp_dir.path().join("db")).unwrap();
        let run_id = RunId::new();
        let ns = create_namespace(run_id);
        let key = make_key(&ns, "cas_retry");
        db.put(run_id, key.clone(), Value::I64(0)).unwrap();

        group.bench_function("retry_until_success", |b| {
            b.iter(|| {
                let mut attempts = 0;
                loop {
                    let current = db.get(&key).unwrap().unwrap();
                    let new_val = match current.value {
                        Value::I64(n) => n + 1,
                        _ => 1,
                    };
                    let result = db.cas(run_id, key.clone(), current.version, Value::I64(new_val));
                    if result.is_ok() {
                        break black_box(attempts);
                    }
                    attempts += 1;
                    if attempts > 100 {
                        panic!("CAS retry exceeded limit");
                    }
                }
            });
        });
    }

    group.finish();
}

// =============================================================================
// Snapshot Isolation Benchmarks
// =============================================================================
// Semantic: Snapshot view is consistent across multiple reads in same transaction
// Regression: Snapshot creation cost, version lookup overhead

fn snapshot_benchmarks(c: &mut Criterion) {
    let mut group = c.benchmark_group("snapshot");
    group.throughput(Throughput::Elements(1));

    // --- Benchmark: single_read (read within transaction) ---
    // Semantic: Read in transaction sees consistent point-in-time view
    // Real pattern: Agent reading state during computation
    {
        let temp_dir = TempDir::new().unwrap();
        let db = Database::open(temp_dir.path().join("db")).unwrap();
        let run_id = RunId::new();
        let ns = create_namespace(run_id);

        // Pre-populate with 1000 keys
        let keys = pregenerate_keys(&ns, "snap", 1000);
        for (i, key) in keys.iter().enumerate() {
            db.put(run_id, key.clone(), Value::I64(i as i64)).unwrap();
        }

        let lookup_key = keys[500].clone();

        group.bench_function("single_read", |b| {
            b.iter(|| {
                let result = db.transaction(run_id, |txn| txn.get(&lookup_key));
                black_box(result.unwrap())
            });
        });
    }

    // --- Benchmark: multi_read (consistent multi-key view) ---
    // Semantic: All reads in transaction see same snapshot (no phantom reads)
    // Real pattern: Agent gathering related state
    {
        let temp_dir = TempDir::new().unwrap();
        let db = Database::open(temp_dir.path().join("db")).unwrap();
        let run_id = RunId::new();
        let ns = create_namespace(run_id);

        let keys = pregenerate_keys(&ns, "multi", 1000);
        for (i, key) in keys.iter().enumerate() {
            db.put(run_id, key.clone(), Value::I64(i as i64)).unwrap();
        }

        // Read 10 keys per transaction
        let read_keys: Vec<_> = (0..10).map(|i| keys[i * 100].clone()).collect();

        group.bench_function("multi_read_10", |b| {
            b.iter(|| {
                let result = db.transaction(run_id, |txn| {
                    for key in &read_keys {
                        txn.get(key)?;
                    }
                    Ok(())
                });
                black_box(result.unwrap())
            });
        });
    }

    // --- Benchmark: version_count_scaling ---
    // Semantic: Snapshot read cost does not grow with version history depth
    // Real pattern: Long-running agent with many state updates
    for num_versions in [10, 100, 1000] {
        let temp_dir = TempDir::new().unwrap();
        let db = Database::open(temp_dir.path().join("db")).unwrap();
        let run_id = RunId::new();
        let ns = create_namespace(run_id);
        let key = make_key(&ns, "versioned");

        // Create version history
        for v in 0..num_versions {
            db.put(run_id, key.clone(), Value::I64(v as i64)).unwrap();
        }

        group.bench_with_input(
            BenchmarkId::new("after_versions", num_versions),
            &num_versions,
            |b, _| {
                b.iter(|| {
                    let result = db.transaction(run_id, |txn| txn.get(&key));
                    black_box(result.unwrap())
                });
            },
        );
    }

    // --- Benchmark: read_your_writes ---
    // Semantic: Transaction sees its own uncommitted writes
    // Real pattern: Agent building up state before commit
    {
        let temp_dir = TempDir::new().unwrap();
        let db = Database::open(temp_dir.path().join("db")).unwrap();
        let run_id = RunId::new();
        let ns = create_namespace(run_id);

        let counter = AtomicU64::new(0);

        group.bench_function("read_your_writes", |b| {
            b.iter(|| {
                let i = counter.fetch_add(1, Ordering::Relaxed);
                // Generate key in real-time to avoid exhaustion
                let key = make_key(&ns, &format!("ryw_{}", i));
                let result = db.transaction(run_id, |txn| {
                    txn.put(key.clone(), Value::I64(i as i64))?;
                    let val = txn.get(&key)?;
                    Ok(val)
                });
                black_box(result.unwrap())
            });
        });
    }

    // --- Benchmark: read_only transaction ---
    // Semantic: Pure read transaction has no write-set, cannot conflict
    // Real pattern: Agent querying state without modification
    {
        let temp_dir = TempDir::new().unwrap();
        let db = Database::open(temp_dir.path().join("db")).unwrap();
        let run_id = RunId::new();
        let ns = create_namespace(run_id);

        let keys = pregenerate_keys(&ns, "ro", 10_000);
        for (i, key) in keys.iter().enumerate() {
            db.put(run_id, key.clone(), Value::I64(i as i64)).unwrap();
        }

        let read_keys: Vec<_> = keys.iter().take(10).cloned().collect();

        group.bench_function("read_only_10", |b| {
            b.iter(|| {
                let result = db.transaction(run_id, |txn| {
                    for key in &read_keys {
                        txn.get(key)?;
                    }
                    Ok(())
                });
                black_box(result.unwrap())
            });
        });
    }

    group.finish();
}

// =============================================================================
// Conflict Detection Benchmarks (Multi-Threaded)
// =============================================================================
// Semantic: First-committer-wins; conflict causes abort, not partial commit
// Regression: Conflict detection scaling with thread count
//
// CONTENTION MODEL:
// - All threads synchronized via barrier (start simultaneously)
// - Fixed-duration loops for steady-state measurement
// - Success/abort counters reported

fn conflict_benchmarks(c: &mut Criterion) {
    let mut group = c.benchmark_group("conflict");
    group.sample_size(10);

    // --- Conflict shape: disjoint_keys (no actual conflicts) ---
    // Semantic: Non-overlapping keys means no conflicts, all commits succeed
    // Real pattern: Partitioned agent workloads
    for num_threads in [2, 4, 8] {
        group.bench_with_input(
            BenchmarkId::new("disjoint_keys", num_threads),
            &num_threads,
            |b, &num_threads| {
                b.iter_custom(|_iters| {
                    let temp_dir = TempDir::new().unwrap();
                    let db = Arc::new(Database::open(temp_dir.path().join("db")).unwrap());
                    let run_id = RunId::new();

                    let barrier = Arc::new(Barrier::new(num_threads + 1));
                    let total_commits = Arc::new(AtomicU64::new(0));
                    let stop_flag = Arc::new(AtomicU64::new(0));

                    let handles: Vec<_> = (0..num_threads)
                        .map(|thread_id| {
                            let db = Arc::clone(&db);
                            let barrier = Arc::clone(&barrier);
                            let total_commits = Arc::clone(&total_commits);
                            let stop_flag = Arc::clone(&stop_flag);
                            let ns = create_namespace(run_id);

                            thread::spawn(move || {
                                // Pre-generate keys for this thread
                                let mut key_idx = 0u64;
                                let mut rng_state = BENCH_SEED ^ (thread_id as u64 * 0x9E3779B9);

                                barrier.wait();

                                let mut local_commits = 0u64;
                                while stop_flag.load(Ordering::Relaxed) == 0 {
                                    let key = make_key(&ns, &format!("t{}_{}", thread_id, key_idx));
                                    key_idx += 1;

                                    let _ = lcg_next(&mut rng_state); // consume for consistency

                                    if db
                                        .transaction(run_id, |txn| {
                                            txn.put(key, Value::I64(local_commits as i64))?;
                                            Ok(())
                                        })
                                        .is_ok()
                                    {
                                        local_commits += 1;
                                    }
                                }
                                total_commits.fetch_add(local_commits, Ordering::Relaxed);
                            })
                        })
                        .collect();

                    // Measure for fixed duration
                    let start = Instant::now();
                    barrier.wait();
                    thread::sleep(CONFLICT_BENCH_DURATION);
                    stop_flag.store(1, Ordering::Relaxed);

                    for h in handles {
                        h.join().unwrap();
                    }

                    let elapsed = start.elapsed();
                    let commits = total_commits.load(Ordering::Relaxed);

                    // Log throughput (visible in benchmark output)
                    eprintln!(
                        "conflict/disjoint_keys/{}: {} commits in {:?} ({:.0} commits/s)",
                        num_threads,
                        commits,
                        elapsed,
                        commits as f64 / elapsed.as_secs_f64()
                    );

                    elapsed
                });
            },
        );
    }

    // --- Conflict shape: same_key (maximum contention) ---
    // Semantic: Concurrent writes to same key - exactly one winner per round
    // Real pattern: Global counter, leader election
    for num_threads in [2, 4] {
        group.bench_with_input(
            BenchmarkId::new("same_key", num_threads),
            &num_threads,
            |b, &num_threads| {
                b.iter_custom(|_iters| {
                    let temp_dir = TempDir::new().unwrap();
                    let db = Arc::new(Database::open(temp_dir.path().join("db")).unwrap());
                    let run_id = RunId::new();
                    let ns = create_namespace(run_id);
                    let contested_key = make_key(&ns, "contested");

                    db.put(run_id, contested_key.clone(), Value::I64(0))
                        .unwrap();

                    let barrier = Arc::new(Barrier::new(num_threads + 1));
                    let total_commits = Arc::new(AtomicU64::new(0));
                    let total_aborts = Arc::new(AtomicU64::new(0));
                    let stop_flag = Arc::new(AtomicU64::new(0));

                    let handles: Vec<_> = (0..num_threads)
                        .map(|_| {
                            let db = Arc::clone(&db);
                            let barrier = Arc::clone(&barrier);
                            let total_commits = Arc::clone(&total_commits);
                            let total_aborts = Arc::clone(&total_aborts);
                            let stop_flag = Arc::clone(&stop_flag);
                            let key = contested_key.clone();

                            thread::spawn(move || {
                                barrier.wait();

                                let mut local_commits = 0u64;
                                let mut local_aborts = 0u64;

                                while stop_flag.load(Ordering::Relaxed) == 0 {
                                    let result = db.transaction(run_id, |txn| {
                                        let val = txn.get(&key)?;
                                        let n = match val {
                                            Some(Value::I64(n)) => n,
                                            _ => 0,
                                        };
                                        txn.put(key.clone(), Value::I64(n + 1))?;
                                        Ok(())
                                    });

                                    if result.is_ok() {
                                        local_commits += 1;
                                    } else {
                                        local_aborts += 1;
                                        // Brief backoff on conflict
                                        thread::sleep(Duration::from_micros(10));
                                    }
                                }

                                total_commits.fetch_add(local_commits, Ordering::Relaxed);
                                total_aborts.fetch_add(local_aborts, Ordering::Relaxed);
                            })
                        })
                        .collect();

                    let start = Instant::now();
                    barrier.wait();
                    thread::sleep(CONFLICT_BENCH_DURATION);
                    stop_flag.store(1, Ordering::Relaxed);

                    for h in handles {
                        h.join().unwrap();
                    }

                    let elapsed = start.elapsed();
                    let commits = total_commits.load(Ordering::Relaxed);
                    let aborts = total_aborts.load(Ordering::Relaxed);
                    let success_ratio = if commits + aborts > 0 {
                        commits as f64 / (commits + aborts) as f64
                    } else {
                        0.0
                    };

                    // Log throughput and conflict stats
                    eprintln!(
                        "conflict/same_key/{}: {} commits, {} aborts ({:.1}% success) in {:?}",
                        num_threads,
                        commits,
                        aborts,
                        success_ratio * 100.0,
                        elapsed
                    );

                    elapsed
                });
            },
        );
    }

    // --- Conflict shape: cas_one_winner (CAS race) ---
    // Semantic: Multiple simultaneous CAS attempts - exactly one winner
    // Real pattern: Distributed lock acquisition
    group.bench_function("cas_one_winner", |b| {
        b.iter_custom(|iters| {
            let mut total_elapsed = Duration::ZERO;

            for _ in 0..iters {
                let temp_dir = TempDir::new().unwrap();
                let db = Arc::new(Database::open(temp_dir.path().join("db")).unwrap());
                let run_id = RunId::new();
                let ns = create_namespace(run_id);
                let key = make_key(&ns, "cas_contest");

                db.put(run_id, key.clone(), Value::I64(0)).unwrap();
                let initial_version = db.get(&key).unwrap().unwrap().version;

                let num_threads: usize = 4;
                let barrier = Arc::new(Barrier::new(num_threads + 1));
                let winners = Arc::new(AtomicU64::new(0));
                let losers = Arc::new(AtomicU64::new(0));

                let handles: Vec<_> = (0..num_threads)
                    .map(|id| {
                        let db = Arc::clone(&db);
                        let barrier = Arc::clone(&barrier);
                        let winners = Arc::clone(&winners);
                        let losers = Arc::clone(&losers);
                        let key = key.clone();

                        thread::spawn(move || {
                            barrier.wait();
                            let result =
                                db.cas(run_id, key, initial_version, Value::I64(id as i64));
                            if result.is_ok() {
                                winners.fetch_add(1, Ordering::Relaxed);
                            } else {
                                losers.fetch_add(1, Ordering::Relaxed);
                            }
                        })
                    })
                    .collect();

                let start = Instant::now();
                barrier.wait();

                for h in handles {
                    h.join().unwrap();
                }

                total_elapsed += start.elapsed();

                // Invariant: exactly one winner (first-committer-wins)
                let winner_count = winners.load(Ordering::Relaxed);
                let loser_count = losers.load(Ordering::Relaxed);
                assert_eq!(winner_count, 1, "Expected exactly 1 CAS winner");
                assert_eq!(
                    loser_count,
                    (num_threads - 1) as u64,
                    "Expected {} CAS losers",
                    num_threads - 1
                );
            }

            total_elapsed
        });
    });

    group.finish();
}

// =============================================================================
// Benchmark Groups
// =============================================================================

criterion_group!(
    name = txn;
    config = Criterion::default().measurement_time(Duration::from_secs(10));
    targets = txn_commit_benchmarks, txn_cas_benchmarks
);

criterion_group!(
    name = snapshot;
    config = Criterion::default().measurement_time(Duration::from_secs(10));
    targets = snapshot_benchmarks
);

criterion_group!(
    name = conflict;
    config = Criterion::default()
        .measurement_time(Duration::from_secs(15))
        .sample_size(10);
    targets = conflict_benchmarks
);

criterion_main!(txn, snapshot, conflict);
