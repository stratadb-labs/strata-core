# M4 Epics and User Stories: Performance

**Milestone**: M4 - Performance
**Goal**: Remove architectural blockers to Redis-class latency through durability modes and targeted optimizations
**Estimated Duration**: 1.5 weeks
**Architecture Spec**: v1.1 (Validated)

---

## Critical Implementation Invariants

Before implementing any M4 story, understand these invariants:

### 1. Atomicity Scope
> **Transactions are atomic within a single RunId. Cross-run writes are NOT guaranteed atomic.**

### 2. Snapshot Semantic Invariant
> **Fast-path reads must be observationally equivalent to a snapshot-based transaction.**

No dirty reads, no torn reads, no stale reads, no mixing versions. This is NON-NEGOTIABLE.

### 3. Required Dependencies
```toml
dashmap = "5"
rustc-hash = "1.1"  # Use this, NOT fxhash
parking_lot = "0.12"
```

### 4. Current M3 State (Verified)
- **RunId**: `Copy` trait, 16 bytes (UUID v4) - trivially cheap
- **Key**: Contains `Namespace` with RunId; `user_key: Vec<u8>` is heap-allocated
- **Snapshot**: Currently O(n) deep clone - M4 MUST fix this
- **WAL**: Already has DurabilityMode abstraction - extend, don't rewrite

---

## Overview

M4 is a **de-blocking milestone** that removes architectural barriers preventing Redis-class performance:

1. **Durability Modes** - InMemory (<3µs), Buffered (<30µs), Strict (~2ms)
2. **Sharded Storage** - DashMap + FxHashMap replaces RwLock + BTreeMap
3. **Transaction Pooling** - Zero allocations on hot path
4. **Read Path Optimization** - Bypass transaction overhead for reads
5. **Performance Instrumentation** - Visibility for ongoing optimization

**Philosophy**: M4 does not aim to be fast. M4 aims to be *fastable*.

---

## Epic 20: Performance Foundation (GitHub #211)

**Goal**: Core infrastructure for performance work

### Scope
- Tag M3 performance baseline
- Benchmark infrastructure for M4
- Feature flags for instrumentation
- DurabilityMode type definition
- Database builder pattern

### Critical First Story
**Story #197: Tag M3 Baseline & Benchmark Infrastructure**
- **BLOCKS ALL M4 IMPLEMENTATION**
- Creates baseline tag, benchmark setup, feature flags
- Must be completed before any optimization work

### Success Criteria
- [ ] `m3_baseline_perf` git tag created
- [ ] `perf-trace` feature flag working
- [ ] `cargo bench --bench m4_performance` runs
- [ ] DurabilityMode enum defined with all three variants
- [ ] DatabaseBuilder pattern implemented
- [ ] Baseline benchmark results recorded

### Dependencies
- M3 complete (all primitives working)

### Estimated Effort
1 day with 2 Claudes in parallel

### Risks
- **Risk**: Baseline numbers not representative
- **Mitigation**: Run benchmarks multiple times, document hardware

### User Stories
- **#197**: Tag M3 Baseline & Benchmark Infrastructure (3 hours) 🔴 FOUNDATION
- **#198**: DurabilityMode Type Definition (3 hours)
- **#199**: Performance Instrumentation Infrastructure (4 hours)
- **#200**: Database Builder Pattern (4 hours)

### Parallelization
After #197, stories #198-200 can run in parallel (3 Claudes)

---

## Epic 21: Durability Modes (GitHub #212)

**Goal**: Implement three durability modes trading latency vs durability

### Scope
- Durability trait abstraction
- InMemory mode (no WAL, no fsync)
- Buffered mode (WAL append, async fsync)
- Strict mode (WAL append + sync fsync)
- Per-operation durability override
- Graceful shutdown

### Success Criteria
- [ ] Durability trait defined and implemented by all modes
- [ ] InMemory: `engine/put_direct` < 3µs
- [ ] InMemory: No WAL file created
- [ ] Buffered: `kvstore/put` < 30µs
- [ ] Buffered: Background flush thread working
- [ ] Buffered: Flush on interval and batch size thresholds
- [ ] Buffered: **Thread lifecycle managed** (shutdown flag + join handle)
- [ ] Buffered: **Drop impl signals shutdown and joins thread**
- [ ] Strict: Identical behavior to M3
- [ ] Per-operation override: `transaction_with_durability()` works
- [ ] Graceful shutdown flushes Buffered mode
- [ ] Drop handler calls shutdown
- [ ] All modes provide same ACI guarantees
- [ ] **Atomicity scope documented**: Per-RunId only

### Dependencies
- Epic 20 complete (DurabilityMode type, builder pattern)

### Estimated Effort
1.5 days with 3 Claudes in parallel

### Risks
- **Risk**: Buffered mode background thread complexity
- **Mitigation**: Start simple (interval-based), add batch threshold later
- **Risk**: Data loss window miscommunicated
- **Mitigation**: Document clearly in API and error messages

### User Stories
- **#201**: Durability Trait Abstraction (3 hours) 🔴 FOUNDATION
- **#202**: InMemory Durability Implementation (3 hours)
- **#203**: Strict Durability Implementation (3 hours)
- **#204**: Buffered Durability Implementation (5 hours)
- **#205**: Per-Operation Durability Override (3 hours)
- **#206**: Graceful Shutdown (3 hours)

### Parallelization
After #201, stories #202-203 can run in parallel. #204 depends on #201-203 for testing. #205-206 can run in parallel after #204.

---

## Epic 22: Sharded Storage (GitHub #213)

**Goal**: Replace RwLock + BTreeMap with DashMap + HashMap for better concurrency

### Scope
- ShardedStore structure with DashMap
- Per-RunId sharding
- FxHash for fast hashing
- Get/Put/Delete operations
- List operations (with sort)
- Snapshot fast path (allocation-free)
- Migration from UnifiedStore

### Success Criteria
- [ ] ShardedStore implemented with DashMap<RunId, Shard>
- [ ] Shard contains FxHashMap<Key, VersionedValue>
- [ ] get() is lock-free via DashMap
- [ ] put() only locks target shard
- [ ] list() returns sorted results (BTreeMap compatibility)
- [ ] Snapshot acquisition < 500ns
- [ ] Snapshot is allocation-free (Arc bump only)
- [ ] Different runs never contend
- [ ] Disjoint scaling ≥ 1.8× at 2 threads
- [ ] Disjoint scaling ≥ 3.2× at 4 threads
- [ ] Storage trait abstracts implementations
- [ ] UnifiedStore deprecated but kept for reference

### Dependencies
- Epic 20 complete

### Estimated Effort
1.5 days with 3 Claudes in parallel

### Risks
- **Risk**: DashMap contention under write-heavy load
- **Mitigation**: Benchmark early, fall back to explicit sharding if needed
- **Risk**: list() performance regression
- **Mitigation**: list() is not hot path; document tradeoff

### User Stories
- **#207**: ShardedStore Structure (4 hours) 🔴 FOUNDATION
- **#208**: ShardedStore Get/Put Operations (4 hours)
- **#209**: ShardedStore List Operations (3 hours)
- **#210**: Snapshot Acquisition (Fast Path) (4 hours)
- **#211**: Storage Migration Path (3 hours)

### Parallelization
After #207, stories #208-209 can run in parallel. #210 depends on #208. #211 runs last.

---

## Epic 23: Transaction Pooling (GitHub #214)

**Goal**: Eliminate allocation overhead on transaction hot path

### Scope
- TransactionContext reset method
- Thread-local transaction pool
- Pooled begin/end transaction API
- Zero-allocation verification

### Success Criteria
- [ ] `reset()` method clears state without deallocating
- [ ] `reset()` preserves HashMap capacity
- [ ] Thread-local pool with max 8 contexts per thread
- [ ] `acquire()` returns pooled context or allocates
- [ ] `release()` returns context to pool
- [ ] `begin_transaction()` uses pool
- [ ] `end_transaction()` returns to pool
- [ ] Pool is thread-local (no cross-thread sharing)
- [ ] Zero allocations on hot path after warmup
- [ ] Benchmark proves allocation-free operation

### Dependencies
- Epic 20 complete

### Estimated Effort
1 day with 2 Claudes in parallel

### Risks
- **Risk**: Pool contention
- **Mitigation**: Thread-local pools eliminate contention entirely
- **Risk**: Memory leak from pooled objects
- **Mitigation**: Pool caps at MAX_POOL_SIZE, excess dropped

### User Stories
- **#212**: TransactionContext Reset Method (3 hours)
- **#213**: Thread-Local Transaction Pool (4 hours) 🔴 FOUNDATION
- **#214**: Pooled Transaction API (3 hours)
- **#215**: Zero-Allocation Verification (3 hours)

### Parallelization
Stories #212-213 can run in parallel. #214 depends on #213. #215 runs last.

---

## Epic 24: Read Path Optimization (GitHub #215)

**Goal**: Bypass transaction overhead for read-only operations

**CRITICAL INVARIANT**: Fast-path reads must be observationally equivalent to snapshot-based transactions. Breaking this breaks agent reasoning and makes replay non-deterministic.

### Scope
- KVStore fast path get()
- Batch get_many() operation
- Other primitive fast paths (EventLog, StateCell, TraceStore)
- Observational equivalence verification

### Success Criteria
- [ ] KVStore.get() bypasses full transaction
- [ ] KVStore.get() uses direct snapshot read
- [ ] KVStore.get() < 10µs (target: <5µs)
- [ ] get_many() uses single snapshot for batch
- [ ] EventLog.read() and len() have fast paths
- [ ] StateCell.read() has fast path
- [ ] TraceStore.get() has fast path
- [ ] **INVARIANT**: All fast paths observationally equivalent to transaction reads
- [ ] **INVARIANT**: No dirty reads, stale reads, or torn reads
- [ ] **INVARIANT**: No mixing versions across keys in same snapshot
- [ ] Snapshot consistency verified under concurrent modification

### Dependencies
- Epic 22 complete (ShardedStore with snapshot)

### Estimated Effort
1 day with 2 Claudes in parallel

### Risks
- **Risk**: Fast path breaks isolation guarantees
- **Mitigation**: Explicit observational equivalence tests
- **Risk**: Inconsistent behavior between fast path and transaction
- **Mitigation**: Both paths use same snapshot mechanism

### User Stories
- **#216**: KVStore Fast Path Get (3 hours) 🔴 FOUNDATION
- **#217**: KVStore Fast Path Batch Get (3 hours)
- **#218**: Other Primitive Fast Paths (4 hours)
- **#219**: Observational Equivalence Tests (3 hours)

### Parallelization
After #216, stories #217-218 can run in parallel. #219 runs last.

---

## Epic 25: Validation & Red Flags (GitHub #216)

**Goal**: Verify M4 meets targets and check red flag thresholds

### Scope
- Full M4 benchmark suite
- Red flag validation tests
- Facade tax measurement
- Contention scaling verification
- Success criteria checklist

### Success Criteria
- [ ] All latency benchmarks running
- [ ] All throughput benchmarks running
- [ ] Red flag tests implemented and passing:
  - Snapshot acquisition ≤ 2µs
  - A1/A0 ≤ 20×
  - B/A1 ≤ 8×
  - Disjoint scaling (4T) ≥ 2.5×
  - p99 ≤ 20× mean
  - Zero hot-path allocations
- [ ] Facade tax measured and documented
- [ ] Scaling targets verified:
  - 2 threads ≥ 1.8×
  - 4 threads ≥ 3.2×
  - 8 threads ≥ 6.0×
- [ ] M4 completion checklist created and verified
- [ ] All gates pass
- [ ] No red flags triggered

### Dependencies
- All other M4 epics complete

### Estimated Effort
1.5 days with 3 Claudes in parallel

### Risks
- **Risk**: Red flag triggered
- **Mitigation**: This is the purpose of the test—stop and redesign if triggered
- **Risk**: Benchmarks inconsistent across runs
- **Mitigation**: Multiple runs, statistical analysis, document variance

### User Stories
- **#220**: M4 Benchmark Suite (4 hours)
- **#221**: Red Flag Validation (4 hours) 🔴 CRITICAL
- **#222**: Facade Tax Measurement (3 hours)
- **#223**: Contention Scaling Verification (3 hours)
- **#224**: Success Criteria Checklist (3 hours)

### Parallelization
Stories #220-223 can run in parallel (4 Claudes). #224 runs last after all results collected.

---

## Story Dependency Graph

```
Epic 20: Foundation (GitHub #211)
  #197 (baseline) ──┬──> #198 (DurabilityMode)
                    ├──> #199 (instrumentation)
                    └──> #200 (builder)

Epic 21: Durability Modes (GitHub #212)
  #201 (trait) ──┬──> #202 (InMemory)
                 └──> #203 (Strict)
                      └──> #204 (Buffered) ──┬──> #205 (override)
                                             └──> #206 (shutdown)

Epic 22: Sharded Storage (GitHub #213)
  #207 (structure) ──┬──> #208 (get/put)
                     └──> #209 (list)
                          └──> #210 (snapshot) ──> #211 (migration)

Epic 23: Transaction Pooling (GitHub #214)
  #212 (reset) ──┬
                 └──> #213 (pool) ──> #214 (pooled API) ──> #215 (verification)

Epic 24: Read Path Optimization (GitHub #215)
  #216 (KV fast path) ──┬──> #217 (batch get)
                        └──> #218 (other primitives)
                             └──> #219 (equivalence tests)

Epic 25: Validation (GitHub #216)
  All Epics ──> #220, #221, #222, #223 (parallel) ──> #224 (checklist)
```

---

## Parallelization Strategy

### Phase 1: Foundation (Day 1)
- **Claude 1**: Story #197 (baseline tag)
- After #197: Claude 1 → #198, Claude 2 → #199, Claude 3 → #200

### Phase 2: Core Optimizations (Days 2-4)
After Epic 20 complete, Epics 21-23 can start in parallel:

- **Claude 1**: Epic 21 (Durability) - #201 → #202, #203 → #204 → #205, #206
- **Claude 2**: Epic 22 (Sharded Storage) - #207 → #208, #209 → #210 → #211
- **Claude 3**: Epic 23 (Transaction Pooling) - #212, #213 → #214 → #215

### Phase 3: Read Optimization (Day 5)
After Epic 22 complete:

- **Claude 1**: Story #216 (KV fast path)
- **Claude 2**: Story #217 (batch get)
- **Claude 3**: Story #218 (other primitives)
- After above: Story #219 (equivalence tests)

### Phase 4: Validation (Days 6-7)
After all optimizations:

- **Claude 1**: Story #220 (benchmark suite)
- **Claude 2**: Story #221 (red flag validation)
- **Claude 3**: Story #222 (facade tax)
- **Claude 4**: Story #223 (scaling verification)
- **All**: Story #224 (completion checklist)

---

## Story Count Summary

| Epic | Stories | Effort |
|------|---------|--------|
| Epic 20: Foundation | 4 | 14 hours |
| Epic 21: Durability Modes | 6 | 20 hours |
| Epic 22: Sharded Storage | 5 | 18 hours |
| Epic 23: Transaction Pooling | 4 | 13 hours |
| Epic 24: Read Path Optimization | 4 | 13 hours |
| Epic 25: Validation | 5 | 17 hours |
| **Total** | **28** | **95 hours** |

With 4 Claudes working in parallel: ~24 hours elapsed = ~3 working days
Realistic with dependencies: ~6-7 working days

---

## Red Flag Thresholds (Hard Stops)

If ANY of these fail, **STOP AND REDESIGN**:

| Metric | Threshold | Action if Exceeded |
|--------|-----------|-------------------|
| Snapshot acquisition | > 2µs | Redesign snapshot mechanism |
| A1/A0 ratio | > 20× | Remove abstraction layers |
| B/A1 ratio | > 8× | Inline facade logic |
| Disjoint scaling (4T) | < 2.5× | Redesign sharding |
| p99 latency | > 20× mean | Fix tail latency source |
| Hot-path allocations | > 0 | Eliminate allocations |

---

## Performance Targets

| Metric | Target | Red Flag |
|--------|--------|----------|
| `engine/put_direct` (InMemory) | < 3µs | > 10µs |
| `kvstore/put` (InMemory) | < 8µs | > 20µs |
| `kvstore/get` | < 5µs | > 10µs |
| Throughput (1-thread InMemory) | 250K ops/sec | < 100K ops/sec |
| Throughput (4-thread disjoint) | 800K ops/sec | < 400K ops/sec |
| Snapshot acquisition | < 500ns | > 2µs |
| A1/A0 | < 10× | > 20× |
| B/A1 | < 5× | > 8× |

---

## Success Criteria (M4 Complete)

### Gate 1: Durability Modes
- [ ] Three modes implemented: InMemory, Buffered, Strict
- [ ] InMemory mode: `engine/put_direct` < 3µs
- [ ] InMemory mode: 250K ops/sec (1-thread)
- [ ] Buffered mode: `kvstore/put` < 30µs
- [ ] Strict mode: Same behavior as M3

### Gate 2: Hot Path Optimization
- [ ] Transaction pooling: Zero allocations in A1 hot path
- [ ] Snapshot acquisition: < 500ns, allocation-free
- [ ] Read optimization: `kvstore/get` < 10µs

### Gate 3: Scaling
- [ ] Lock sharding: DashMap + HashMap in use
- [ ] Disjoint scaling ≥ 3.2× at 4 threads
- [ ] 4-thread disjoint throughput: ≥ 800K ops/sec

### Gate 4: Facade Tax
- [ ] A1/A0 < 10× (InMemory mode)
- [ ] B/A1 < 5×
- [ ] B/A0 < 30×

### Gate 5: Infrastructure
- [ ] Baseline tagged: `m3_baseline_perf`
- [ ] Backwards compatibility: M3 code unchanged
- [ ] All M3 tests still pass

### Red Flag Check
- [ ] All red flag tests pass
- [ ] No architectural redesign required

---

**Document Version**: 1.0
**Created**: 2026-01-15
**Status**: Planning
