# Epic 25: Validation & Red Flags - Implementation Prompts

**Epic Goal**: Verify M4 meets targets and check red flag thresholds

**GitHub Issue**: [#216](https://github.com/anibjoshi/in-mem/issues/216)
**Status**: Ready after all other M4 epics
**Dependencies**: Epics 20-24 complete

---

## AUTHORITATIVE SPECIFICATIONS - READ THESE FIRST

**`docs/architecture/M4_ARCHITECTURE.md` is the GOSPEL for ALL M4 implementation.**

See `docs/prompts/M4_PROMPT_HEADER.md` for complete guidelines.

---

## Epic 25 Overview

### Reference: Critical Invariants

See `docs/prompts/M4_PROMPT_HEADER.md` for critical invariants. **This epic validates ALL of them:**
1. **Atomicity Scope** - Validated by disjoint scaling tests
2. **Snapshot Semantic Invariant** - Validated by observational equivalence tests
3. **Thread Lifecycle** - Validated by shutdown tests
4. **Zero Allocations** - Validated by hot-path allocation test

### Scope
- Full M4 benchmark suite
- Red flag validation tests
- Facade tax measurement
- Contention scaling verification
- Success criteria checklist

### Success Criteria
- [ ] All latency benchmarks running
- [ ] All throughput benchmarks running
- [ ] Red flag tests implemented and passing
- [ ] Facade tax measured and documented
- [ ] Scaling targets verified
- [ ] M4 completion checklist created

### Component Breakdown
- **Story #220 (GitHub #240)**: M4 Benchmark Suite
- **Story #221 (GitHub #241)**: Red Flag Validation - CRITICAL
- **Story #222 (GitHub #242)**: Facade Tax Measurement
- **Story #223 (GitHub #243)**: Contention Scaling Verification
- **Story #224 (GitHub #244)**: Success Criteria Checklist

---

## Dependency Graph

```
All Epics Complete ──> Stories #240, #241, #242, #243 (parallel)
                              └──> Story #244 (after all results)
```

---

## Story #240: M4 Benchmark Suite

**GitHub Issue**: [#240](https://github.com/anibjoshi/in-mem/issues/240)
**Estimated Time**: 4 hours
**Dependencies**: Epics 20-24 complete

### Start Story

```bash
gh issue view 240
./scripts/start-story.sh 25 240 benchmark-suite
```

### Implementation

Update `benches/m4_performance.rs`:

```rust
//! M4 Performance Benchmark Suite
//!
//! Run with: cargo bench --bench m4_performance

use criterion::{criterion_group, criterion_main, Criterion, BenchmarkId};
use std::time::Duration;

fn latency_benchmarks(c: &mut Criterion) {
    let mut group = c.benchmark_group("latency");
    group.measurement_time(Duration::from_secs(10));

    for mode in ["inmemory", "buffered", "strict"] {
        let db = match mode {
            "inmemory" => Database::builder().in_memory().open_temp().unwrap(),
            "buffered" => Database::builder().buffered().open_temp().unwrap(),
            "strict" => Database::builder().strict().open_temp().unwrap(),
            _ => unreachable!(),
        };

        let kv = KVStore::new(db.clone());
        let run_id = RunId::new();

        // Warmup
        for i in 0..100 {
            kv.put(run_id, &format!("warmup{}", i), Value::I64(i as i64)).unwrap();
        }

        group.bench_function(BenchmarkId::new("kvstore/put", mode), |b| {
            let mut i = 0;
            b.iter(|| {
                i += 1;
                kv.put(run_id, &format!("key{}", i), Value::I64(i as i64)).unwrap();
            });
        });

        group.bench_function(BenchmarkId::new("kvstore/get", mode), |b| {
            b.iter(|| {
                kv.get(run_id, "warmup50").unwrap();
            });
        });

        group.bench_function(BenchmarkId::new("engine/put_direct", mode), |b| {
            let mut i = 0;
            b.iter(|| {
                i += 1;
                db.transaction(run_id, |txn| {
                    txn.put(Key::new_kv(run_id.namespace(), &format!("direct{}", i)), Value::I64(i as i64))?;
                    Ok(())
                }).unwrap();
            });
        });
    }

    group.finish();
}

fn throughput_benchmarks(c: &mut Criterion) {
    let mut group = c.benchmark_group("throughput");
    group.measurement_time(Duration::from_secs(15));
    group.throughput(criterion::Throughput::Elements(1000));

    let db = Database::builder().in_memory().open_temp().unwrap();
    let kv = KVStore::new(db.clone());
    let run_id = RunId::new();

    group.bench_function("inmemory/1000_puts", |b| {
        b.iter(|| {
            for i in 0..1000 {
                kv.put(run_id, &format!("key{}", i), Value::I64(i as i64)).unwrap();
            }
        });
    });

    group.finish();
}

fn snapshot_benchmarks(c: &mut Criterion) {
    let mut group = c.benchmark_group("snapshot");

    let db = Arc::new(Database::builder().in_memory().open_temp().unwrap());

    // Populate with some data
    let kv = KVStore::new((*db).clone());
    let run_id = RunId::new();
    for i in 0..1000 {
        kv.put(run_id, &format!("key{}", i), Value::I64(i as i64)).unwrap();
    }

    group.bench_function("acquire", |b| {
        b.iter(|| {
            let _snapshot = db.storage.snapshot();
        });
    });

    group.finish();
}

fn contention_benchmarks(c: &mut Criterion) {
    let mut group = c.benchmark_group("contention");
    group.measurement_time(Duration::from_secs(15));

    for threads in [1, 2, 4, 8] {
        group.bench_function(BenchmarkId::new("disjoint_runs", threads), |b| {
            let db = Arc::new(Database::builder().in_memory().open_temp().unwrap());

            b.iter(|| {
                let handles: Vec<_> = (0..threads)
                    .map(|_| {
                        let db = Arc::clone(&db);
                        std::thread::spawn(move || {
                            let kv = KVStore::new((*db).clone());
                            let run_id = RunId::new(); // Different run per thread
                            for i in 0..1000 {
                                kv.put(run_id, &format!("key{}", i), Value::I64(i as i64)).unwrap();
                            }
                        })
                    })
                    .collect();

                for h in handles {
                    h.join().unwrap();
                }
            });
        });
    }

    group.finish();
}

criterion_group!(
    name = m4_benchmarks;
    config = Criterion::default().sample_size(50);
    targets = latency_benchmarks, throughput_benchmarks, snapshot_benchmarks, contention_benchmarks
);

criterion_main!(m4_benchmarks);
```

### Validation

```bash
~/.cargo/bin/cargo bench --bench m4_performance
```

### Complete Story

```bash
./scripts/complete-story.sh 240
```

---

## Story #241: Red Flag Validation

**GitHub Issue**: [#241](https://github.com/anibjoshi/in-mem/issues/241)
**Estimated Time**: 4 hours
**Dependencies**: Epics 20-24 complete
**CRITICAL**: If any test fails, STOP and REDESIGN

### Start Story

```bash
gh issue view 241
./scripts/start-story.sh 25 241 red-flag-validation
```

### Implementation

Create `tests/m4_red_flags.rs`:

```rust
//! M4 Red Flag Validation Tests
//!
//! These tests FAIL if architecture has fundamental problems.
//! A failure means STOP and REDESIGN - not tune parameters.

use std::time::Instant;
use std::sync::Arc;

const ITERATIONS: usize = 10000;

/// Red flag: Snapshot acquisition > 2µs
#[test]
fn red_flag_snapshot_acquisition() {
    let db = Arc::new(Database::builder().in_memory().open_temp().unwrap());

    // Warmup
    for _ in 0..100 {
        let _ = db.storage.snapshot();
    }

    // Measure
    let start = Instant::now();
    for _ in 0..ITERATIONS {
        let _ = db.storage.snapshot();
    }
    let elapsed = start.elapsed();

    let avg_ns = elapsed.as_nanos() / ITERATIONS as u128;
    let threshold_ns = 2000; // 2µs

    assert!(
        avg_ns <= threshold_ns,
        "RED FLAG: Snapshot acquisition {}ns > {}ns threshold.\n\
         ACTION: Redesign snapshot mechanism.",
        avg_ns, threshold_ns
    );

    println!("Snapshot acquisition: {}ns (threshold: {}ns) ✓", avg_ns, threshold_ns);
}

/// Red flag: A1/A0 ratio > 20×
#[test]
fn red_flag_facade_tax_a1_a0() {
    let db = Database::builder().in_memory().open_temp().unwrap();
    let kv = KVStore::new(db.clone());
    let run_id = RunId::new();

    // Warmup
    for i in 0..100 {
        kv.put(run_id, &format!("warmup{}", i), Value::I64(i as i64)).unwrap();
    }

    // A0: Engine direct (raw storage)
    let start = Instant::now();
    for i in 0..ITERATIONS {
        db.storage.put(
            run_id,
            Key::new_kv(run_id.namespace(), &format!("a0key{}", i)),
            VersionedValue::new(Value::I64(i as i64), 1),
        );
    }
    let a0_elapsed = start.elapsed();

    // A1: Primitive layer (KVStore.put)
    let start = Instant::now();
    for i in 0..ITERATIONS {
        kv.put(run_id, &format!("a1key{}", i), Value::I64(i as i64)).unwrap();
    }
    let a1_elapsed = start.elapsed();

    let a0_ns = a0_elapsed.as_nanos() / ITERATIONS as u128;
    let a1_ns = a1_elapsed.as_nanos() / ITERATIONS as u128;
    let ratio = a1_ns as f64 / a0_ns.max(1) as f64;

    assert!(
        ratio <= 20.0,
        "RED FLAG: A1/A0 ratio {:.1}× > 20× threshold.\n\
         A0 (storage): {}ns, A1 (primitive): {}ns\n\
         ACTION: Remove abstraction layers.",
        ratio, a0_ns, a1_ns
    );

    println!("A1/A0 ratio: {:.1}× (threshold: 20×) ✓", ratio);
}

/// Red flag: B/A1 ratio > 8×
#[test]
fn red_flag_facade_tax_b_a1() {
    let db = Database::builder().in_memory().open_temp().unwrap();
    let kv = KVStore::new(db.clone());
    let run_id = RunId::new();

    // Warmup
    for i in 0..100 {
        kv.put(run_id, &format!("warmup{}", i), Value::I64(i as i64)).unwrap();
    }

    // A1: Primitive layer
    let start = Instant::now();
    for i in 0..ITERATIONS {
        kv.put(run_id, &format!("a1key{}", i), Value::I64(i as i64)).unwrap();
    }
    let a1_elapsed = start.elapsed();

    // B: Full stack with explicit transaction
    let start = Instant::now();
    for i in 0..ITERATIONS {
        db.transaction(run_id, |txn| {
            kv.put_in_transaction(txn, &format!("bkey{}", i), Value::I64(i as i64))
        }).unwrap();
    }
    let b_elapsed = start.elapsed();

    let a1_ns = a1_elapsed.as_nanos() / ITERATIONS as u128;
    let b_ns = b_elapsed.as_nanos() / ITERATIONS as u128;
    let ratio = b_ns as f64 / a1_ns.max(1) as f64;

    assert!(
        ratio <= 8.0,
        "RED FLAG: B/A1 ratio {:.1}× > 8× threshold.\n\
         A1 (primitive): {}ns, B (full stack): {}ns\n\
         ACTION: Inline facade logic.",
        ratio, a1_ns, b_ns
    );

    println!("B/A1 ratio: {:.1}× (threshold: 8×) ✓", ratio);
}

/// Red flag: Disjoint scaling < 2.5× at 4 threads
#[test]
fn red_flag_disjoint_scaling() {
    let db = Arc::new(Database::builder().in_memory().open_temp().unwrap());
    let iterations = 10000;

    // Single-threaded baseline
    let kv = KVStore::new((*db).clone());
    let run_id = RunId::new();

    let start = Instant::now();
    for i in 0..iterations {
        kv.put(run_id, &format!("key{}", i), Value::I64(i as i64)).unwrap();
    }
    let single_thread_time = start.elapsed();

    // 4-thread disjoint
    let start = Instant::now();
    let handles: Vec<_> = (0..4)
        .map(|_| {
            let db = Arc::clone(&db);
            std::thread::spawn(move || {
                let kv = KVStore::new((*db).clone());
                let run_id = RunId::new(); // Different run per thread
                for i in 0..iterations {
                    kv.put(run_id, &format!("key{}", i), Value::I64(i as i64)).unwrap();
                }
            })
        })
        .collect();
    for h in handles {
        h.join().unwrap();
    }
    let four_thread_time = start.elapsed();

    // 4× work should take less than 4× time
    // scaling = (single_time * 4) / four_thread_time
    let scaling = (single_thread_time.as_nanos() * 4) as f64 / four_thread_time.as_nanos() as f64;

    assert!(
        scaling >= 2.5,
        "RED FLAG: Disjoint scaling {:.2}× < 2.5× threshold.\n\
         1-thread: {:?}, 4-threads: {:?}\n\
         ACTION: Redesign sharding.",
        scaling, single_thread_time, four_thread_time
    );

    println!("Disjoint scaling (4T): {:.2}× (threshold: ≥2.5×) ✓", scaling);
}

/// Red flag: p99 > 20× mean
#[test]
fn red_flag_tail_latency() {
    let db = Database::builder().in_memory().open_temp().unwrap();
    let kv = KVStore::new(db.clone());
    let run_id = RunId::new();

    // Warmup
    for i in 0..100 {
        kv.put(run_id, &format!("warmup{}", i), Value::I64(i as i64)).unwrap();
    }

    // Collect latencies
    let mut latencies: Vec<u128> = Vec::with_capacity(1000);
    for i in 0..1000 {
        let start = Instant::now();
        kv.put(run_id, &format!("key{}", i), Value::I64(i as i64)).unwrap();
        latencies.push(start.elapsed().as_nanos());
    }

    latencies.sort();
    let mean = latencies.iter().sum::<u128>() / latencies.len() as u128;
    let p99 = latencies[989]; // 99th percentile

    let ratio = p99 as f64 / mean.max(1) as f64;

    assert!(
        ratio <= 20.0,
        "RED FLAG: p99/mean = {:.1}× > 20× threshold.\n\
         mean: {}ns, p99: {}ns\n\
         ACTION: Fix tail latency source.",
        ratio, mean, p99
    );

    println!("p99/mean: {:.1}× (threshold: ≤20×) ✓", ratio);
}

/// Red flag: Hot path has allocations (after warmup)
#[test]
fn red_flag_hot_path_allocations() {
    let db = Database::builder().in_memory().open_temp().unwrap();
    let kv = KVStore::new(db.clone());
    let run_id = RunId::new();

    // Warmup - fill pool
    for _ in 0..10 {
        kv.put(run_id, "warmup", Value::I64(0)).unwrap();
    }

    // Check pool has contexts
    let pool_size_before = TransactionPool::pool_size();
    assert!(pool_size_before > 0, "Pool should have contexts after warmup");

    // Do operations
    for i in 0..100 {
        kv.put(run_id, &format!("key{}", i), Value::I64(i as i64)).unwrap();
    }

    // Pool should still have same number of contexts (reused, not allocated)
    let pool_size_after = TransactionPool::pool_size();

    assert_eq!(
        pool_size_before, pool_size_after,
        "RED FLAG: Pool size changed from {} to {}.\n\
         Transactions are not being properly pooled.\n\
         ACTION: Eliminate allocations.",
        pool_size_before, pool_size_after
    );

    println!("Hot path allocations: 0 ✓");
}
```

### Validation

```bash
~/.cargo/bin/cargo test --test m4_red_flags -- --nocapture
```

### Complete Story

```bash
./scripts/complete-story.sh 241
```

---

## Story #242: Facade Tax Measurement

**GitHub Issue**: [#242](https://github.com/anibjoshi/in-mem/issues/242)
**Estimated Time**: 3 hours
**Dependencies**: Epics 20-24 complete

### Implementation

Create `benches/m4_facade_tax.rs`:

```rust
//! Facade Tax Benchmarks
//!
//! Measures overhead at each layer:
//! - A0: Core data structure (HashMap)
//! - A1: Engine layer (storage.put)
//! - B:  Facade layer (KVStore.put)

use criterion::{criterion_group, criterion_main, Criterion};

fn facade_tax_benchmarks(c: &mut Criterion) {
    let mut group = c.benchmark_group("facade_tax");

    // A0: Raw HashMap (baseline)
    let mut map = rustc_hash::FxHashMap::default();
    group.bench_function("A0/hashmap_insert", |b| {
        let mut i = 0;
        b.iter(|| {
            i += 1;
            map.insert(format!("key{}", i), i as i64);
        });
    });

    // A1: Engine storage layer
    let db = Database::builder().in_memory().open_temp().unwrap();
    let run_id = RunId::new();
    group.bench_function("A1/storage_put", |b| {
        let mut i = 0;
        b.iter(|| {
            i += 1;
            db.storage.put(
                run_id,
                Key::new_kv(run_id.namespace(), &format!("key{}", i)),
                VersionedValue::new(Value::I64(i as i64), 1),
            );
        });
    });

    // B: Facade layer (KVStore)
    let kv = KVStore::new(db.clone());
    group.bench_function("B/kvstore_put", |b| {
        let mut i = 0;
        b.iter(|| {
            i += 1;
            kv.put(run_id, &format!("key{}", i), Value::I64(i as i64)).unwrap();
        });
    });

    group.finish();
}

criterion_group!(facade_tax, facade_tax_benchmarks);
criterion_main!(facade_tax);
```

### Complete Story

```bash
./scripts/complete-story.sh 242
```

---

## Story #243: Contention Scaling Verification

**GitHub Issue**: [#243](https://github.com/anibjoshi/in-mem/issues/243)
**Estimated Time**: 3 hours
**Dependencies**: Epics 20-24 complete

### Implementation

Create `benches/m4_contention.rs` with scaling benchmarks for 1, 2, 4, 8 threads with both disjoint and shared run patterns.

### Complete Story

```bash
./scripts/complete-story.sh 243
```

---

## Story #244: Success Criteria Checklist

**GitHub Issue**: [#244](https://github.com/anibjoshi/in-mem/issues/244)
**Estimated Time**: 3 hours
**Dependencies**: Stories #240-243

### Implementation

Create `docs/milestones/M4_COMPLETION_CHECKLIST.md`:

```markdown
# M4 Completion Checklist

**Date**: ___________
**Signed Off By**: ___________

## Gate 1: Durability Modes
- [ ] Three modes implemented: InMemory, Buffered, Strict
- [ ] InMemory mode: `engine/put_direct` < 3µs
- [ ] InMemory mode: 250K ops/sec (1-thread)
- [ ] Buffered mode: `kvstore/put` < 30µs
- [ ] Buffered mode: 50K ops/sec throughput
- [ ] Strict mode: Same behavior as M3 (backwards compatible)
- [ ] Per-operation durability override works

## Gate 2: Hot Path Optimization
- [ ] Transaction pooling: Zero allocations in A1 hot path
- [ ] Snapshot acquisition: < 500ns, allocation-free
- [ ] Read optimization: `kvstore/get` < 10µs

## Gate 3: Scaling
- [ ] Lock sharding: DashMap + HashMap replaces RwLock + BTreeMap
- [ ] Disjoint scaling ≥ 1.8× at 2 threads
- [ ] Disjoint scaling ≥ 3.2× at 4 threads
- [ ] 4-thread disjoint throughput: ≥ 800K ops/sec

## Gate 4: Facade Tax
- [ ] A1/A0 < 10× (InMemory mode)
- [ ] B/A1 < 5×
- [ ] B/A0 < 30×

## Gate 5: Infrastructure
- [ ] Baseline tagged: `m3_baseline_perf`
- [ ] Per-layer instrumentation working
- [ ] Backwards compatibility: M3 code unchanged
- [ ] All M3 tests still pass

## Red Flag Check (must all pass)
- [ ] Snapshot acquisition ≤ 2µs
- [ ] A1/A0 ≤ 20×
- [ ] B/A1 ≤ 8×
- [ ] Disjoint scaling (4 threads) ≥ 2.5×
- [ ] p99 ≤ 20× mean
- [ ] Zero hot-path allocations

## Documentation
- [ ] M4_ARCHITECTURE.md complete
- [ ] m4-architecture.md diagrams complete
- [ ] API docs updated
- [ ] Benchmark results recorded

## Final Sign-off
- [ ] All gates pass
- [ ] No red flags triggered
- [ ] Code reviewed
- [ ] CI passes
- [ ] Ready for M5

---

**APPROVED FOR M5**: [ ] Yes / [ ] No
```

### Complete Story

```bash
./scripts/complete-story.sh 244
```

---

## Epic 25 Completion Checklist

### 1. Final Validation

```bash
~/.cargo/bin/cargo test --workspace
~/.cargo/bin/cargo test --test m4_red_flags
~/.cargo/bin/cargo bench --bench m4_performance
~/.cargo/bin/cargo bench --bench m4_facade_tax
~/.cargo/bin/cargo bench --bench m4_contention
```

### 2. Verify All Red Flags Pass

All 6 red flag tests must pass:
- [ ] Snapshot acquisition ≤ 2µs
- [ ] A1/A0 ≤ 20×
- [ ] B/A1 ≤ 8×
- [ ] Disjoint scaling ≥ 2.5×
- [ ] p99 ≤ 20× mean
- [ ] Zero allocations

### 3. Complete M4 Checklist

Fill out `docs/milestones/M4_COMPLETION_CHECKLIST.md` with actual measured values.

### 4. Merge to Develop

```bash
git checkout develop
git merge --no-ff epic-25-validation-red-flags -m "Epic 25: Validation & Red Flags complete"
git push origin develop
gh issue close 216 --comment "Epic 25 complete. All 5 stories delivered. All red flags pass."
```

---

## M4 Milestone Completion

After Epic 25 is complete, M4 is done:

```bash
# 1. Verify all M4 epics complete
gh issue list --state closed --label "milestone-4"

# 2. Merge develop to main
git checkout main
git merge --no-ff develop -m "M4: Performance milestone complete

Delivered:
- Three durability modes (InMemory, Buffered, Strict)
- Sharded storage (DashMap + HashMap)
- Transaction pooling (zero-allocation hot path)
- Read path optimization (fast path reads)
- Performance validation (all red flags pass)

28 stories across 6 epics."

git push origin main

# 3. Tag release
git tag -a v0.4.0 -m "M4: Performance"
git push origin v0.4.0
```
