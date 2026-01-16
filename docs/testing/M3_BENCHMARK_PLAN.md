# M3 Primitive Benchmarks Plan

## Mission Statement

**We are building a state substrate for agents with Redis-class hot-path performance and strictly stronger semantics: transactions, causal ordering, structured state, and replayability.**

---

## What "Beat Redis" Means

Redis is not one thing. It is:
- Single-threaded hot-path engine
- Extremely optimized in-memory data structures
- Minimal safety overhead
- Networked, not in-process
- Weak transactional semantics

**We aim to beat Redis on:**
- In-process access (no network hop)
- Structured state (typed primitives)
- Atomic multi-key operations (real transactions)
- Causal consistency (EventLog hash chains)
- Durable-by-default semantics (future)

**We accept Redis will win on:**
- Raw single-thread SET/GET (decades of C optimization)
- Simplicity (one data model)
- Mature ecosystem

---

## Redis Baseline Numbers

These are ballpark numbers on a modern machine (single-threaded):

| Operation | Redis Throughput | Redis Latency |
|-----------|------------------|---------------|
| GET | 5-10M ops/sec | ~100-200 ns |
| SET | 3-6M ops/sec | ~200-300 ns |
| INCR | 2-4M ops/sec | ~300-500 ns |
| List push/pop | 1-3M ops/sec | ~400-1000 ns |

**This is extremely fast.** We will not match these at M3.

---

## Performance Philosophy

What matters at M3:
1. **Establish the right asymptote** - architecture must not preclude Redis-class performance
2. **Know exactly what is slow and why** - every microsecond must be explainable
3. **Do not bake in architectural limits** - no 10× overhead from design mistakes

Acceptable performance gaps:
| Gap | Assessment |
|-----|------------|
| 5× slower than Redis | Great |
| 10× slower | Acceptable |
| 20× slower | Concerning |
| 50× slower | Rethink architecture |

---

## Performance Tiers

### Tier A0: Core Data Structure (True Hot Path)

**The absolute floor. No correctness machinery.**

These benchmarks bypass:
- Snapshot acquisition
- Transaction object creation
- Closure dispatch
- Version tracking (unless intrinsic to data structure)
- Any coordination layer

| Operation | Asymptotic Goal | Redis Baseline | Purpose |
|-----------|-----------------|----------------|---------|
| `core_get_hot` | <200 ns | ~100-200 ns | Raw HashMap lookup |
| `core_put_hot` | <300 ns | ~200-300 ns | Raw HashMap insert |
| `core_cas_hot` | <500 ns | ~300 ns | Atomic compare-swap |

**Purpose**: Tells you if your fundamental data layout and access patterns are Redis-class *in principle*.

**M3 Gate**: None. These are observability metrics only. If >1 µs, investigate data structure choice.

### Tier A1: Minimal Correctness Wrapper

**The cost of correctness without durability.**

These benchmarks include:
- Snapshot creation
- Transaction object allocation
- Read/write set wiring
- Commit validation

These benchmarks exclude:
- WAL writes
- fsync
- Disk I/O of any kind

| Operation | M3 Target | Asymptotic Goal | Purpose |
|-----------|-----------|-----------------|---------|
| `engine_get_direct` | <3 µs | <500 ns | Snapshot + lookup |
| `engine_put_direct` | <3 µs | <1 µs | Snapshot + write + commit |
| `engine_cas_direct` | <3 µs | <2 µs | Read + validate + commit |
| `engine_snapshot_acquire` | <1 µs | <200 ns | Snapshot overhead alone |
| `engine_txn_empty_commit` | <2 µs | <500 ns | Transaction overhead alone |

**Purpose**: Tells you the cost of correctness wrappers.

**M3 Hard Gate**: All Tier A1 operations MUST be < 3 µs. Failure blocks launch.

### Tier B: Transactional Operations (Primitive Facades)

**Our differentiation layer. Redis does not have real transactions.**

| Operation | M3 Target | Stretch Goal | Notes |
|-----------|-----------|--------------|-------|
| `kvstore_get` | <5 µs | <2 µs | Facade + snapshot + deserialize |
| `kvstore_put` | <8 µs | <3 µs | Facade + serialize + commit |
| `eventlog_append` | <10 µs | <5 µs | Hash chain + metadata CAS + commit |
| `statecell_read` | <5 µs | <2 µs | Versioned read |
| `statecell_cas` | <10 µs | <4 µs | Read + validate + commit |
| `statecell_transition` | <15 µs | <7 µs | Atomic RMW with retry |
| `cross_txn_kv_event` | <20 µs | <10 µs | 2-primitive atomic |
| `cross_txn_all_primitives` | <40 µs | <20 µs | 4-primitive atomic |

**M3 Gate**: Warning if > target. Investigate if > 50 µs. No hard block.

### Tier C: Indexed Operations

**Redis doesn't do this natively. Our structural advantage.**

| Operation | M3 Target | Stretch Goal | Notes |
|-----------|-----------|--------------|-------|
| `tracestore_record_minimal` | <20 µs | <10 µs | 2 indices (type, time) |
| `tracestore_record_3_tags` | <40 µs | <20 µs | 5 indices |
| `tracestore_query_by_type` | <200 µs | <100 µs | Index scan + fetch |
| `tracestore_get_tree` (13 nodes) | <150 µs | <70 µs | Recursive fetch |
| `runindex_create` | <15 µs | <7 µs | Status index |
| `runindex_transition` | <20 µs | <10 µs | Index update |

**M3 Gate**: Warning if > target. No hard block.

### Tier D: Contention Behavior

**This is where most systems collapse. Redis avoids it by being single-threaded. We must survive it.**

#### Same-Key Contention (Relative Scaling)

| Threads | Minimum Throughput | Notes |
|---------|-------------------|-------|
| 4 | ≥25% of 1-thread | Contention overhead acceptable |
| 8 | ≥15% of 1-thread | Further degradation acceptable |

Additional requirements:
- p99 < 10× mean (M3: warning, M4+: hard gate)
- No starvation (no thread < 10% of mean)
- No logical failures (invariants pass)

#### Disjoint-Key Scaling (Must Scale)

| Threads | Minimum Speedup | Notes |
|---------|-----------------|-------|
| 2 | ≥1.8× | Near-linear expected |
| 4 | ≥3.2× | Sub-linear acceptable |
| 8 | ≥6.0× | If less, lock contention problem |

**M3 Gate**: Disjoint scaling MUST meet minimums. Same-key contention is warning only.

---

## Benchmark Validity vs Performance

**These are fundamentally different concerns. Never confuse them.**

### Benchmark Validity (Correctness)

Any of these is a **correctness failure**, not a performance issue:

| Failure | Meaning | Action |
|---------|---------|--------|
| Lost updates | StateCell final ≠ transitions | **Block merge, fail CI** |
| Lost events | EventLog len ≠ appends | **Block merge, fail CI** |
| Duplicate sequences | EventLog serialization broken | **Block merge, fail CI** |
| Starvation | Any thread 0 successes | **Block merge, fail CI** |
| Thread panic | System instability | **Block merge, fail CI** |
| Invariant violation | Any documented invariant | **Block merge, fail CI** |

**Never trade correctness for speed. Throughput numbers are meaningless if invariants fail.**

### Performance (Speed)

| Concern | M3 Action | M4+ Action |
|---------|-----------|------------|
| Tier A1 > 3 µs | **Block launch** | **Block merge** |
| Tier B > target | Warning, investigate | Warning, investigate |
| Tier C > target | Warning | Warning |
| p99 > 10× mean | Warning, investigate | **Block merge** |
| Disjoint scaling < minimum | **Block merge** | **Block merge** |
| Same-key < 25% single-thread | Warning | Warning |

---

## Benchmark Honesty Rules

These are **non-negotiable**. Violating them poisons the benchmark.

### General Rules

| Rule | Rationale |
|------|-----------|
| No allocation inside `b.iter` | Allocator noise dominates nanosecond ops |
| No string formatting inside `b.iter` | `format!` is expensive |
| No serialization unless measuring it | Serde dominates |
| No TempDir inside `b.iter` | Filesystem is milliseconds |
| No thread spawn inside `b.iter` | Thread creation ~100 µs |
| Pre-generate all keys/data | Amortize setup |
| Use `AtomicU64` for unique keys | Avoid format! |
| Separate allocation-free variants | Isolate true engine cost |

### Steady-State Only Rule (Tier A and B)

**All Tier A and Tier B gates apply to steady-state measurements only.**

Cold-start measurements are observability only. They do not gate launches or merges.

| Measurement | Used For | Why |
|-------------|----------|-----|
| Cold-start | Observability | Page faults, allocator growth, cache warming |
| Steady-state | **Gating** | True hot-path performance |

**Rationale**: Failing gates due to first-access costs (page faults, TLB misses, allocator expansion) does not tell you about hot-path performance. Those costs amortize away in real workloads.

**Implementation**: Criterion's warmup phase handles this automatically. Ensure warmup iterations are sufficient (default is usually fine).

### No I/O Rule (Tier A and B)

**Tier A and Tier B benchmarks MUST NOT:**

- Call fsync
- Touch disk
- Use TempDir inside iter
- Perform filesystem syscalls
- Perform WAL flushes
- Perform any durability work

**Rationale**: Redis comparisons are meaningless if you allow I/O. Durability benchmarks come later with explicit `_durable` suffix.

**Enforcement**: Tier A/B benchmarks must use in-memory storage only. Any disk access is a benchmark bug.

---

## Benchmark Categories

### Category 1: Core Data Structure (Tier A0)

**No transaction machinery. Pure data structure access.**

#### `core_get_hot`
- **What**: Raw storage lookup, no wrappers
- **Asymptotic**: <200 ns
- **Why**: Baseline data structure performance

#### `core_put_hot`
- **What**: Raw storage insert, no wrappers
- **Asymptotic**: <300 ns
- **Why**: Baseline write performance

#### `core_cas_hot`
- **What**: Raw compare-and-swap
- **Asymptotic**: <500 ns
- **Why**: Baseline atomic operation

### Category 2: Engine Microbenchmarks (Tier A1)

**Minimal correctness wrapper. No facades.**

#### `engine_get_direct`
- **What**: Snapshot + key lookup
- **M3 Gate**: <3 µs
- **Why**: Cost of snapshot isolation

#### `engine_put_direct`
- **What**: Snapshot + write + commit
- **M3 Gate**: <3 µs
- **Why**: Cost of transactional write

#### `engine_snapshot_acquire`
- **What**: Acquire snapshot only, no operations
- **M3 Gate**: <1 µs
- **Why**: Snapshot overhead in isolation

#### `engine_txn_empty_commit`
- **What**: Begin + commit with no operations
- **M3 Gate**: <2 µs
- **Why**: Transaction overhead in isolation

### Category 3: Allocation-Free Variants

**Isolate engine overhead from memory overhead.**

Reuse:
- Same key (pre-allocated)
- Same value buffer
- Same struct instances

| Benchmark | Target | What It Isolates |
|-----------|--------|------------------|
| `engine_get_noalloc` | <500 ns | True hot-path read |
| `engine_put_noalloc` | <1 µs | True hot-path write |
| `kvstore_get_noalloc` | <3 µs | Facade overhead only |

### Category 4: Cache-Locality Tests

**Redis wins because it is cache-friendly.**

| Test | Working Set | What It Measures |
|------|-------------|------------------|
| `cache_hot_key` | 1 key | L1 cache hit |
| `cache_working_set_8` | 8 keys | L1 cache |
| `cache_working_set_64` | 64 keys | L2 cache |
| `cache_working_set_512` | 512 keys | L3 cache |
| `cache_uniform_10k` | 10,000 keys | Main memory |

**Expected Pattern:**
- Hot key → 8 keys: ~same (L1)
- 64 keys: slight degradation (L2)
- 512 keys: noticeable degradation (L3)
- 10k keys: memory-bound

**If hot key is not 2×+ faster than 10k uniform, your data structures are not cache-friendly.**

### Category 5: Branch Predictor Tests

**Predictable access should be faster than random.**

| Test | Pattern | Expected |
|------|---------|----------|
| `branch_sequential` | Keys 0,1,2,3... | Fastest |
| `branch_strided` | Keys 0,8,16,24... | Fast |
| `branch_random` | Random keys | Slower |

**If sequential is not 20%+ faster than random, investigate branch prediction issues.**

### Category 6: First-Touch vs Steady-State

**Some costs appear only on first use.**

First-touch costs:
- Page faults
- Allocator growth
- Cache warming

| Benchmark | First-Touch | Steady-State | Purpose |
|-----------|-------------|--------------|---------|
| `engine_get_cold` | Measured | N/A | First access cost |
| `engine_get_warm` | Discarded | Measured | Amortized cost |

**Steady-state is used for gating. First-touch is observability only.**

### Category 7: Memory Overhead

**Redis wins partly because it is memory-efficient.**

| Benchmark | What It Measures |
|-----------|------------------|
| `mem_bytes_per_kv_entry` | KVStore overhead per key |
| `mem_bytes_per_event` | EventLog overhead per event |
| `mem_bytes_per_trace` | TraceStore overhead per trace |
| `mem_bytes_per_run` | RunIndex overhead per run |
| `index_amp_ratio_trace` | Bytes written / payload bytes |
| `index_amp_ratio_run` | Bytes written / metadata bytes |

**M3 Gate**: None. These are observability metrics.

**Purpose**: Prevent building a fast-but-10×-memory system accidentally.

### Category 8: Primitive Facade Benchmarks (Tier B)

Standard M3 primitive operations.

#### EventLog
| Benchmark | M3 Target | Stretch |
|-----------|-----------|---------|
| `eventlog_append` | <10 µs | <5 µs |
| `eventlog_read` | <5 µs | <2 µs |
| `eventlog_read_range/100` | <100 µs | <50 µs |
| `eventlog_verify_chain/1000` | <2 ms | <1 ms |

#### StateCell
| Benchmark | M3 Target | Stretch |
|-----------|-----------|---------|
| `statecell_init` | <10 µs | <5 µs |
| `statecell_read` | <5 µs | <2 µs |
| `statecell_cas` | <10 µs | <4 µs |
| `statecell_transition` | <15 µs | <7 µs |

#### KVStore
| Benchmark | M3 Target | Stretch |
|-----------|-----------|---------|
| `kvstore_put` | <8 µs | <3 µs |
| `kvstore_get` | <5 µs | <2 µs |
| `kvstore_delete` | <8 µs | <3 µs |

#### TraceStore
| Benchmark | M3 Target | Stretch |
|-----------|-----------|---------|
| `tracestore_record_minimal` | <20 µs | <10 µs |
| `tracestore_record_3_tags` | <40 µs | <20 µs |
| `tracestore_query_by_type` | <200 µs | <100 µs |
| `tracestore_get_tree` | <150 µs | <70 µs |

#### RunIndex
| Benchmark | M3 Target | Stretch |
|-----------|-----------|---------|
| `runindex_create` | <15 µs | <7 µs |
| `runindex_transition` | <20 µs | <10 µs |
| `runindex_lifecycle` | <50 µs | <25 µs |

### Category 9: Cross-Primitive Transactions

| Benchmark | M3 Target | Stretch | Notes |
|-----------|-----------|---------|-------|
| `cross_txn_kv_event` | <20 µs | <10 µs | Most common pattern |
| `cross_txn_kv_event_state` | <30 µs | <15 µs | Full agent step |
| `cross_txn_all_primitives` | <40 µs | <20 µs | Maximum fanout |
| `cross_snapshot_read` | <15 µs | <7 µs | Multi-primitive read |

### Category 10: Index Amplification (Tier C)

| Benchmark | M3 Target | Notes |
|-----------|-----------|-------|
| `index_amp_trace_0_tags` | <15 µs | Base (2 indices) |
| `index_amp_trace_1_tag` | <18 µs | +1 index |
| `index_amp_trace_3_tags` | <25 µs | +3 indices |
| `index_amp_trace_5_tags` | <35 µs | +5 indices |

**Per-index overhead should be <5 µs. If >10 µs, investigate.**

### Category 11: Contention Benchmarks (Tier D)

#### Same-Key Contention (Relative Scaling)

| Benchmark | Relative Target | Invariant |
|-----------|-----------------|-----------|
| `contention/statecell/4` | ≥25% of 1-thread | final == transitions |
| `contention/statecell/8` | ≥15% of 1-thread | final == transitions |
| `contention/eventlog/4` | ≥25% of 1-thread | len == appends |
| `contention/eventlog/8` | ≥15% of 1-thread | len == appends |

#### Disjoint-Key Scaling (Must Scale)

| Benchmark | Minimum Speedup |
|-----------|-----------------|
| `contention/disjoint/2` | ≥1.8× |
| `contention/disjoint/4` | ≥3.2× |
| `contention/disjoint/8` | ≥6.0× |

---

## Facade Tax Reporting

**Every benchmark group should report the abstraction cost.**

| Layer | Example | Purpose |
|-------|---------|---------|
| A0 | `core_get_hot` | Data structure baseline |
| A1 | `engine_get_direct` | Correctness wrapper cost |
| B | `kvstore_get` | Facade overhead |

### Facade Tax Report Format

```
=== Facade Tax Report ===

GET operation:
  A0 (core_get_hot):      180 ns   (baseline)
  A1 (engine_get_direct): 1.2 µs   (6.7× over A0)
  B  (kvstore_get):       4.5 µs   (3.8× over A1, 25× over A0)

PUT operation:
  A0 (core_put_hot):      250 ns   (baseline)
  A1 (engine_put_direct): 1.8 µs   (7.2× over A0)
  B  (kvstore_put):       7.2 µs   (4.0× over A1, 29× over A0)

Assessment:
  - A1/A0 ratio ~7×: Transaction overhead acceptable
  - B/A1 ratio ~4×: Facade overhead acceptable
  - B/A0 ratio ~27×: Total abstraction cost is high but explainable
```

**If B/A1 > 10×, your facade has a problem.**
**If A1/A0 > 20×, your transaction layer has a problem.**

---

## Latency Distribution

### Required Percentiles
- Mean
- p95
- p99
- Max

**Use Criterion's built-in estimates.** Do not hand-roll.

### p99 Rules (Phase-Gated)

| Condition | M3 Action | M4+ Action |
|-----------|-----------|------------|
| p99 > 10× mean | Warning + investigate | **Block merge** |
| Max > 100× mean | Warning + investigate | Warning + investigate |
| Mean attempts > 3 | Warning + investigate | Warning + investigate |
| Retry rate > 60% | Warning + investigate | **Block merge** |

**Rationale**: Early systems suffer from allocator behavior, OS scheduling, paging. Track now, gate later.

---

## Contention Benchmark Protocol

### Timing Structure
```
|--warmup--|--measurement--|--cooldown--|
   500ms        2000ms         500ms
```

### Required Metrics
| Metric | Definition |
|--------|------------|
| Throughput | ops/sec per thread |
| Mean attempts | total_attempts / successes |
| Max attempts | Worst single operation |
| Retry rate | (attempts - successes) / attempts |
| p99 latency | 99th percentile |

### Invariants (MUST PASS - Correctness)

| Invariant | Scope | Action |
|-----------|-------|--------|
| Final value == total successes | StateCell | **Block merge, fail CI** |
| Log length == total appends | EventLog | **Block merge, fail CI** |
| No duplicate sequences | EventLog | **Block merge, fail CI** |
| No thread panics | All | **Block merge, fail CI** |

### Starvation Detection

| Invariant | Threshold | Action |
|-----------|-----------|--------|
| Per-thread success > 0% | Any thread | **Block merge** |
| No thread < 10% mean | Fairness | Warning + investigate |
| Max attempts < 200 | Retry limit | **Block merge** |

---

## Environment Capture

**Every benchmark run MUST capture:**

```
=== Environment ===
OS:           Linux (Ubuntu 22.04.3 LTS)
CPU:          AMD Ryzen 7 5800X3D (8 cores, 16 threads)
Memory:       64 GB DDR5
Governor:     performance
Rust:         1.75.0
Build:        release (opt-level=3, lto=true)
Features:     default
Threads:      16 (available)
Core pinning: Yes (contention tests)
Timestamp:    2024-01-15T10:30:00Z
Git commit:   abc123f
```

**Purpose**: Makes Redis-style claims reproducible. Prevents meaningless cross-machine regressions.

---

## Benchmark Platform Policy

**All performance gates and Redis comparisons are valid only on the reference platform.**

### Reference Platform

| Component | Requirement |
|-----------|-------------|
| **OS** | Linux (Ubuntu 22.04+) |
| **Mode** | Bare metal (no VMs, no containers) |
| **CPU** | AMD Ryzen 7 5800X3D (or equivalent) |
| **Memory** | 64 GB DDR5 |
| **Governor** | `performance` (not `powersave`) |
| **Build** | Release, LTO enabled |
| **Background** | No background workloads |
| **Cores** | Pinned cores for contention tests |

### Platform Setup Checklist

```bash
# Set CPU governor to performance
sudo cpupower frequency-set -g performance

# Verify governor
cat /sys/devices/system/cpu/cpu*/cpufreq/scaling_governor

# Disable turbo boost for consistency (optional)
echo 0 | sudo tee /sys/devices/system/cpu/cpufreq/boost

# Pin benchmark to specific cores (for contention tests)
taskset -c 0-7 cargo bench --bench m3_primitives -- "contention/"
```

### macOS Policy

macOS is allowed **only** for:

| Use Case | Allowed |
|----------|---------|
| Functional testing | ✓ |
| Relative micro-optimization experiments | ✓ |
| Development iteration | ✓ |
| **Performance gate validation** | ✗ |
| **Redis comparison claims** | ✗ |
| **PR performance impact reports** | ✗ |

**macOS numbers must NEVER be used in:**
- Launch gate decisions
- Redis competitiveness claims
- Performance regression reports
- Benchmark comparison documentation

**Rationale**: macOS has different scheduler behavior, memory allocator, and lacks the kernel tuning options needed for reproducible nanosecond-level measurements. Use Linux for all official measurements.

---

## Assessment Criteria

### M3 Launch Gates

| Tier | Requirement | If Failed |
|------|-------------|-----------|
| A0 | Observability only | No gate |
| A1 | ALL < 3 µs | **Block launch** |
| B | Warning if > target | Investigate |
| C | Warning if > target | No gate |
| D (disjoint) | Must meet scaling minimums | **Block launch** |
| D (contention) | Invariants must pass | **Block launch** |

### Future Milestones

| Tier | M4 Target | M5 Target |
|------|-----------|-----------|
| A0 | <200 ns | <150 ns |
| A1 | <1 µs | <500 ns |
| B | <10 µs | <5 µs |
| p99 gate | Hard | Hard |

---

## Redis Comparison Report Format

After benchmarks, generate comparison:

```
=== Redis Competitiveness Report ===
Environment: Ubuntu 22.04, AMD Ryzen 7 5800X3D, 64GB DDR5, Rust 1.75.0, release

Tier A0: Core Data Structure
  core_get_hot:         180 ns  (Redis: ~150 ns, gap: 1.2×) ✓
  core_put_hot:         250 ns  (Redis: ~250 ns, gap: 1.0×) ✓

Tier A1: Correctness Wrapper
  engine_get_direct:    1.2 µs  (M3 gate: <3 µs) ✓
  engine_put_direct:    1.8 µs  (M3 gate: <3 µs) ✓
  Correctness cost:     6-7× over A0

Tier B: Transactions (Redis N/A)
  kvstore_get:          4.5 µs  (target: <5 µs) ✓
  statecell_transition: 12 µs   (target: <15 µs) ✓
  cross_txn_kv_event:   18 µs   (target: <20 µs) ✓

Tier C: Indexed (Redis N/A)
  tracestore_record:    17 µs   (target: <20 µs) ✓
  tracestore_query:     180 µs  (target: <200 µs) ✓

Tier D: Contention (Redis single-threaded)
  statecell/4:          28% of 1-thread  (target: ≥25%) ✓
  eventlog/4:           31% of 1-thread  (target: ≥25%) ✓
  disjoint/4 scaling:   3.4×             (target: ≥3.2×) ✓

Facade Tax:
  GET: A0→A1 = 6.7×, A1→B = 3.8×, total = 25×
  PUT: A0→A1 = 7.2×, A1→B = 4.0×, total = 29×

Overall: 25-30× slower than Redis raw, but with full transactions.
Assessment: ACCEPTABLE for M3.
```

---

## PR Review Checklist

**Any PR touching hot paths must answer:**

| Question | Required |
|----------|----------|
| Which Tier did this affect? | Yes |
| What changed in A0 vs A1 vs B? | Yes |
| Did Facade Tax change? | Yes |
| Did cache-locality regress? | If touching data structures |
| Did p99 behavior change? | Yes |
| Did memory overhead change? | If adding fields/indices |
| Did contention behavior change? | If touching locks/transactions |

**Format:**
```
## Performance Impact

- Tier affected: B (kvstore_put)
- A0 change: None
- A1 change: None
- B change: +0.3 µs (serialize optimization)
- Facade tax: Unchanged
- p99: Unchanged
- Memory: Unchanged
```

---

## Running Benchmarks

```bash
# Full suite
cargo bench --bench m3_primitives

# By tier
cargo bench --bench m3_primitives -- "core_"        # Tier A0
cargo bench --bench m3_primitives -- "engine_"      # Tier A1
cargo bench --bench m3_primitives -- "kvstore_"     # Tier B
cargo bench --bench m3_primitives -- "tracestore_"  # Tier C
cargo bench --bench m3_primitives -- "contention/"  # Tier D

# Special categories
cargo bench --bench m3_primitives -- "cache_"       # Cache locality
cargo bench --bench m3_primitives -- "_noalloc"     # Allocation-free
cargo bench --bench m3_primitives -- "mem_"         # Memory overhead

# Save baseline
cargo bench --bench m3_primitives -- --save-baseline m3_launch

# Compare
cargo bench --bench m3_primitives -- --baseline m3_launch
```

---

## Future Work

### M4: Performance Optimization
- Target Tier A0 stretch (<200 ns)
- Target Tier A1 stretch (<1 µs)
- Target Tier B stretch (<10 µs)
- Profile and eliminate allocations
- Cache-align hot structures
- p99 becomes hard gate

### M5: Durability
- Add `_durable` suffix for WAL benchmarks
- Measure fsync overhead
- Batch commit optimizations

### M6+: Network
- Add `_remote` suffix for network benchmarks
- Compare to Redis over network (our advantage)
