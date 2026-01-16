# M4 Completion Checklist

**Date**: _2026-01-15_
**Signed Off By**: ___________

## Gate 1: Durability Modes

- [x] Three modes implemented: InMemory, Buffered, Strict
- [ ] InMemory mode: `engine/put_direct` < 3us
- [ ] InMemory mode: 250K ops/sec (1-thread)
- [ ] Buffered mode: `kvstore/put` < 30us
- [ ] Buffered mode: 50K ops/sec throughput
- [x] Strict mode: Same behavior as M3 (backwards compatible)
- [ ] Per-operation durability override works

**Measured Values:**
- InMemory put latency: _____ us
- InMemory throughput: _____ ops/sec
- Buffered put latency: _____ us
- Buffered throughput: _____ ops/sec

## Gate 2: Hot Path Optimization

- [x] Transaction pooling: Zero allocations in A1 hot path
- [x] Snapshot acquisition: < 500ns, allocation-free
- [ ] Read optimization: `kvstore/get` < 10us

**Measured Values:**
- Transaction pool: PASS (pool size stable after warmup)
- Snapshot acquisition: 13ns (threshold: 2000ns)
- KVStore get latency: _____ us

## Gate 3: Scaling

- [x] Lock sharding: DashMap + HashMap replaces RwLock + BTreeMap
- [ ] Disjoint scaling >= 1.8x at 2 threads
- [ ] Disjoint scaling >= 3.2x at 4 threads
- [ ] 4-thread disjoint throughput: >= 800K ops/sec

**Measured Values:**
- 2-thread disjoint scaling: _____ x
- 4-thread disjoint scaling: 0.20x (FAILING - needs optimization)
- 4-thread throughput: _____ ops/sec

## Gate 4: Facade Tax

- [ ] A1/A0 < 10x (InMemory mode)
- [x] B/A1 < 5x (measured 3.5x)
- [ ] B/A0 < 30x

**Measured Values:**
- A1/A0 ratio: 1472x (FAILING - KVStore uses transactions per-op)
- B/A1 ratio: 3.5x (PASS)
- B/A0 ratio: _____ x

## Gate 5: Infrastructure

- [x] Baseline tagged: `m3_baseline_perf`
- [x] Per-layer instrumentation working (via perf-trace feature)
- [x] Backwards compatibility: M3 code unchanged
- [x] All M3 tests still pass

## Red Flag Check (must all pass)

| Test | Threshold | Measured | Status |
|------|-----------|----------|--------|
| Snapshot acquisition | <= 2us | 13ns | PASS |
| A1/A0 ratio | <= 20x | 1472x | FAIL |
| B/A1 ratio | <= 8x | 3.5x | PASS |
| Disjoint scaling (4T) | >= 2.5x | 0.20x | FAIL |
| p99/mean latency | <= 20x | 2.1x | PASS |
| Hot path allocations | 0 | 0 | PASS |

**Current Status: 5/7 red flag tests passing, 2 failing**

### Analysis of Failing Tests

#### A1/A0 Ratio (1472x vs 20x threshold)
- **Root Cause**: KVStore.put() wraps every operation in a full transaction
- **A0 (storage.put)**: Direct storage write ~1.6us
- **A1 (KVStore.put)**: Creates transaction + commits ~2.4ms
- **Action Required**: Implement non-transactional fast path for single operations

#### Disjoint Scaling (0.20x vs 2.5x threshold)
- **Root Cause**: Heavy lock contention despite sharding
- **1-thread**: 7.2s for 10K ops
- **4-threads**: 147s for 40K ops (should be ~7.2s with perfect scaling)
- **Action Required**: Profile lock contention, optimize transaction path

## Documentation

- [x] M4_ARCHITECTURE.md complete
- [x] m4-architecture.md diagrams complete
- [x] API docs updated
- [x] Benchmark results recorded

## Final Sign-off

- [ ] All gates pass
- [ ] No red flags triggered
- [x] Code reviewed
- [x] CI passes
- [ ] Ready for M5

---

**APPROVED FOR M5**: [ ] Yes / [x] No (2 red flags failing)

---

## Remediation Plan

The following optimizations are needed before M4 can be signed off:

### Priority 1: Fix A1/A0 Ratio

1. Add non-transactional fast path for KVStore operations
2. Allow direct storage writes for single operations when no transaction context
3. Target: A1/A0 < 10x

### Priority 2: Fix Disjoint Scaling

1. Profile lock contention points
2. Review transaction pool lock usage
3. Consider lock-free data structures for hot paths
4. Target: >= 2.5x scaling at 4 threads

### Estimated Effort

- A1/A0 fix: 4-6 hours
- Disjoint scaling fix: 8-12 hours
- Re-validation: 2 hours

---

## Benchmark Commands

Run these commands to update measured values:

```bash
# Red flag validation
cargo test --test m4_red_flags --release -- --nocapture

# M4 performance benchmarks
cargo bench --bench m4_performance

# Facade tax benchmarks
cargo bench --bench m4_facade_tax

# Contention benchmarks
cargo bench --bench m4_contention
```

---

## Version History

| Date | Author | Changes |
|------|--------|---------|
| 2026-01-15 | Claude | Initial checklist with measured values |
