# M4 Performance Architecture Plan

## Mission

Achieve Redis-competitive latency on hot-path in-process operations while maintaining stronger semantics (transactions, causal ordering, structured state, replayability).

## Performance Targets

| Metric | Target | Current | Gap |
|--------|--------|---------|-----|
| Simple ops throughput | **250K ops/sec** | ~475 ops/sec | 500× |
| `engine/put_direct` | <3µs | 2.1ms | 700× |
| `kvstore/put` | <8µs | 2.2ms | 275× |
| `eventlog/append` | <10µs | 3.0ms | 300× |
| `kvstore/get` | <5µs | 139µs | 28× |
| p50 latency | <5µs | - | - |
| p99 latency | <50µs | - | - |

## Root Cause Analysis

From M3 benchmarks:

| Layer | Latency | Notes |
|-------|---------|-------|
| `core/get_hot` | 33 ns | ✅ Faster than Redis |
| `core/put_hot_prealloc` | 887 ns | ✅ Raw storage is fast |
| `engine/put_direct` | 2.1 ms | ❌ **2,400× slowdown from WAL/fsync** |

**The bottleneck is fsync.** Everything else is noise until we fix durability modes.

---

## Durability Modes (Core M4 Feature)

### Three Modes

| Mode | WAL | fsync | Target Latency | Loss Window | Use Case |
|------|-----|-------|----------------|-------------|----------|
| **In-Memory** | None | None | <3-10µs | All (on crash) | Redis competitor, caches, ephemeral |
| **Buffered** | Append | Every N ms/ops | <10-50µs | Bounded (configurable) | Production default |
| **Strict** | Append + fsync | Every write | ~2ms | Zero | Checkpoints, metadata, audit |

### API

```rust
// At database open
let db = Database::open_with_mode(path, DurabilityMode::InMemory);
let db = Database::open_with_mode(path, DurabilityMode::Buffered {
    interval_ms: 100,
    batch_size: 1000
});
let db = Database::open_with_mode(path, DurabilityMode::Strict);

// Per-operation override (for critical writes in non-strict mode)
db.put_durable(key, value);  // Force fsync regardless of mode
```

### Target Performance by Mode

| Operation | In-Memory | Buffered | Strict |
|-----------|-----------|----------|--------|
| `engine/put_direct` | <3µs | <20µs | ~2ms |
| `kvstore/put` | <8µs | <30µs | ~2ms |
| `eventlog/append` | <10µs | <40µs | ~3ms |
| **Throughput** | **250K+ ops/s** | **50-100K ops/s** | ~500 ops/s |

---

## Optimization Phases

### Phase 0: Instrumentation and Ground Truth

Before optimizing, we must see.

#### 0.1 Add perf-guided profiling

Set up:
- Linux `perf` integration
- Flamegraph generation
- Optional: callgrind, cachegrind

Metrics to capture:
- Instruction counts
- Branch misses
- L1/L2/L3 cache misses
- IPC (instructions per cycle)
- Stall breakdown

#### 0.2 Add per-layer timing

Internal breakdown instrumentation:

| Component | Measurement |
|-----------|-------------|
| Snapshot creation | Time to acquire snapshot |
| Txn object creation | Allocation + init time |
| Read-set validation | Version check time |
| Write-set validation | Conflict detection time |
| Commit path | Lock acquire + apply time |
| Index updates | Secondary index maintenance |
| Hash chaining | EventLog chain computation |
| Serialization | WAL encode/decode time |

Each must be independently measurable via feature flag.

#### 0.3 Freeze M3 baselines

```bash
git tag m3_baseline_perf
```

All M4 work measured relative to this commit.

---

### Phase 1: Eliminate Accidental Overhead

Remove overhead unrelated to semantics.

#### 1.1 Abstraction removal on hot paths

Audit for:
- Trait object calls (virtual dispatch)
- Dynamic dispatch
- Iterator chains (`.map().filter().collect()`)
- Closures capturing environment
- Generics that don't inline
- Result/Option propagation in tight loops

Replace with:
- Monomorphic functions
- Manual loops
- `#[inline(always)]` on hot functions
- Simple control flow

**Rule:** If it shows up in a flamegraph and isn't core logic, it's a bug.

#### 1.2 Allocation elimination

From benchmarks: `core/put_hot` (with allocation) is 2× slower than `core/put_hot_prealloc`.

Actions:
- Identify all allocations in Tier A1 and Tier B hot paths
- Eliminate all of them

Techniques:
- Stack allocation for small objects
- Preallocated buffers
- Scratch arenas (per-thread)
- Struct reuse
- Avoid `Vec` growth on hot paths

**Targets:**
- Tier A1: **zero allocations**
- Tier B: **amortized zero allocations**

#### 1.3 Serialization minimization

If serialization is in hot paths:
- Use binary, fixed-layout formats
- Avoid Serde on hot paths
- Inline encode/decode
- Avoid map-like representations (JSON, MessagePack maps)

**Rule:** Hot path data must be trivially serializable (memcpy-able where possible).

#### 1.4 Remove debug-mode safety overhead

Compile out or feature-gate:
- Logging on hot paths
- Assertions (use `debug_assert!`)
- Debug counters
- Diagnostic collection

```rust
#[cfg(feature = "perf-diagnostics")]
fn record_timing(...) { ... }

#[cfg(not(feature = "perf-diagnostics"))]
#[inline(always)]
fn record_timing(...) {}
```

---

### Phase 2: Data Layout Optimization

This is where 5-10× speedups live.

#### 2.1 Flatten structures

Audit hot-path structs:
- Remove nested pointers
- Inline small structs
- Use contiguous arrays
- Avoid `Rc`, `Arc`, `Box` on hot paths

**Goal:** One cache line, one operation.

Current sizes (from benchmarks):
```
Key size: 120 bytes          // Too large - spans 2 cache lines
VersionedValue size: 88 bytes
Value size: 56 bytes
Namespace size: 88 bytes
```

Target: Key < 64 bytes for L1 cache line fit.

#### 2.2 Replace HashMaps where possible

`HashMap` is convenient, not fast.

Consider:
- `Vec`-backed arenas with index lookup
- Slot arrays
- Robin Hood hashing (`hashbrown` with custom hasher)
- Perfect hashing (for static/known key sets)
- Index-based addressing

If keeping `HashMap`:
- Use `FxHash` or `AHash` (not default `SipHash`)
- Pre-size to avoid rehashing

#### 2.3 Structure of Arrays (SoA)

For scanning workloads, separate hot fields from cold:

```rust
// ❌ Array of Structs (AoS) - cache unfriendly for scans
struct Entry { version: u64, value: Value, metadata: Metadata }
Vec<Entry>

// ✅ Structure of Arrays (SoA) - cache friendly
struct Entries {
    versions: Vec<u64>,      // Hot for conflict detection
    values: Vec<Value>,      // Accessed on read
    metadata: Vec<Metadata>, // Cold, rarely accessed
}
```

#### 2.4 Cache line alignment

```rust
#[repr(align(64))]
struct HotStruct {
    // Frequently accessed together
    version: u64,
    flags: u32,
    // ... pad to 64 bytes
}
```

- Align hot structs to 64 bytes
- Avoid false sharing in concurrent code
- Group frequently co-accessed fields

---

### Phase 3: Cache Behavior Engineering

Act on cache benchmark results.

#### 3.1 Make hot path fit in L1

From `cache/working_set` benchmarks:
- 1 key: 69 ns
- 8 keys: 159 ns
- 64 keys: 168 ns
- 10K keys: 283 ns

**Targets:**
- Hot metadata in L1 (32KB per core)
- Transaction state in L2 (1MB)
- Indices in L3 (96MB on 7800X3D)

#### 3.2 Reduce working set

- Eliminate bloated structs
- Remove unused fields
- Compact metadata representations
- Use smaller integer types where safe (`u32` vs `u64`)

#### 3.3 Prefetching (later phase)

Only after layout is correct:
- Manual `prefetch` intrinsics
- Stride-friendly access patterns
- Batch operations to amortize misses

---

### Phase 4: Branch Predictability

Pure CPU hygiene.

#### 4.1 Flatten hot paths

Remove from hot paths:
- Deep nesting
- Rare-case checks inside loops
- Polymorphism

Split into:
- **Fast path**: Common case, straight-line code
- **Slow path**: `#[cold]` annotated, out-of-line

```rust
#[inline(always)]
fn get_fast(&self, key: &Key) -> Option<Value> {
    // Fast path - no branches if possible
    self.data.get(key).cloned()
}

#[cold]
#[inline(never)]
fn handle_miss(&self, key: &Key) -> Option<Value> {
    // Slow path - logging, fallback, etc.
}
```

#### 4.2 Separate rare logic

Move out of hot paths:
- Validation failures
- Conflict handling
- Error construction
- Logging

#### 4.3 Remove virtual dispatch

No trait objects in hot loops:

```rust
// ❌ Virtual dispatch
fn process(storage: &dyn Storage) { ... }

// ✅ Monomorphic
fn process<S: Storage>(storage: &S) { ... }

// ✅ Even better - concrete type
fn process(storage: &UnifiedStore) { ... }
```

---

### Phase 5: Synchronization Redesign

Where most systems fail.

#### 5.1 Lock sharding

Replace global `RwLock<UnifiedStore>` with:
- Per-partition locks
- Per-run locks
- Per-TypeTag locks

```rust
struct ShardedStore {
    shards: [RwLock<Shard>; NUM_SHARDS],
}

fn shard_for_key(&self, key: &Key) -> usize {
    // Hash-based or run_id-based sharding
}
```

#### 5.2 Partition by run_id

Agents naturally partition state. Exploit it:
- Per-run shards
- Per-agent arenas
- Per-agent WAL segments

```rust
struct PerRunStore {
    runs: DashMap<RunId, RunShard>,
}

struct RunShard {
    data: BTreeMap<Key, VersionedValue>,
    wal_segment: WalSegment,
}
```

#### 5.3 Read-optimized structures

Consider:
- RCU-like patterns (read-copy-update)
- Copy-on-write for snapshots
- Versioned pointers / epoch-based access
- `crossbeam-epoch` for lock-free reads

#### 5.4 Avoid lock convoys

Monitor for:
- Futex contention (`perf stat`)
- Spin waits
- Excessive CAS retries

From contention benchmarks:
- Same-key contention: ~45K ops/s (acceptable)
- Disjoint-key contention: ~45K ops/s (should scale better)

**Problem:** Disjoint keys don't scale - indicates global lock contention.

---

### Phase 6: Transaction Layer Optimization

Current bottleneck location.

#### 6.1 Remove fsync from hot path

Implement durability modes (see above):
- In-memory mode: No WAL
- Buffered mode: Async WAL flush
- Strict mode: Current behavior

**Critical:** Strict must not be default for hot paths.

#### 6.2 Reduce write amplification

Current: 2,400× overhead from WAL path.

Actions:
- Batch writes before WAL append
- Coalesce metadata updates
- Avoid redundant index writes
- Delta encoding for sequential writes

#### 6.3 Transaction object pooling

Stop allocating transaction objects:

```rust
thread_local! {
    static TXN_POOL: RefCell<Vec<TransactionContext>> = RefCell::new(Vec::new());
}

fn begin_transaction(&self) -> TransactionContext {
    TXN_POOL.with(|pool| {
        pool.borrow_mut().pop().unwrap_or_else(TransactionContext::new)
    })
}

fn end_transaction(&self, txn: TransactionContext) {
    txn.reset();  // Clear state, keep allocations
    TXN_POOL.with(|pool| pool.borrow_mut().push(txn));
}
```

#### 6.4 Conflict detection optimization

Current: Linear scan of read/write sets.

Optimize to:
- O(1) where possible (bloom filters)
- Bitset-based version tracking
- Range-based conflict detection
- Epoch-based validation

---

### Phase 7: Memory Management Strategy

After Phases 1-6.

#### 7.1 Arena allocators

Use arenas for:
- Transaction temporaries
- Serialization buffers
- Batch operation storage

```rust
// Per-thread arena
thread_local! {
    static ARENA: RefCell<bumpalo::Bump> = RefCell::new(bumpalo::Bump::new());
}
```

#### 7.2 Slab allocators

For fixed-size objects:
- WAL entries
- Index nodes
- Event records

#### 7.3 Object pools

Reuse:
- Transaction contexts
- Snapshot views
- Serialization buffers

#### 7.4 Memory reclamation

For lock-free structures:
- Epoch-based GC (`crossbeam-epoch`)
- Hazard pointers
- Deferred free lists

---

### Phase 8: Contention Behavior Hardening

Make contention predictable.

#### 8.1 Backoff strategies

Avoid retry storms:

```rust
fn cas_with_backoff(&self, key: &Key, expected: u64, new: Value) -> Result<()> {
    let mut backoff = Backoff::new();
    loop {
        match self.try_cas(key, expected, new.clone()) {
            Ok(()) => return Ok(()),
            Err(CasError::Conflict) => {
                if backoff.is_completed() {
                    return Err(Error::TooManyRetries);
                }
                backoff.snooze();  // Exponential backoff with jitter
            }
            Err(e) => return Err(e),
        }
    }
}
```

#### 8.2 Fairness enforcement

Ensure:
- No starvation under contention
- No livelock (all threads making progress)
- Bounded wait times

#### 8.3 Hot-key mitigation

Detect and handle hot keys:
- Runtime detection (access counters)
- Automatic sharding for hot keys
- Throttling / rate limiting

---

### Phase 9: Facade Tax Reduction

Act on facade tax report.

#### Current State

| Ratio | Target | Estimated Current |
|-------|--------|-------------------|
| A1/A0 | <10× | ~70× (get), ~2400× (put) |
| B/A1 | <5× | ~10× (get), ~1× (put) |
| B/A0 | <30× | ~4000× (get), ~2500× (put) |

#### Targets

| Ratio | Target |
|-------|--------|
| A1/A0 (with In-Memory mode) | <10× |
| B/A1 | <5× |
| B/A0 | <30× |

#### Actions if violated

- Inline logic from facades into engine
- Remove unnecessary abstraction layers
- Collapse primitive facades where possible
- Direct engine calls for performance-critical paths

---

## Implementation Priority

### P0 - Must Have (Week 1-2)

1. **Durability Modes** - In-Memory, Buffered, Strict
2. **Baseline tagging** - `m3_baseline_perf`
3. **Per-layer timing instrumentation**

### P1 - High Impact (Week 2-3)

4. **Allocation elimination** on hot paths
5. **Lock sharding** by run_id
6. **Transaction object pooling**

### P2 - Medium Impact (Week 3-4)

7. **Data layout optimization** (Key size reduction)
8. **Serialization bypass** for in-memory mode
9. **Branch flattening** on hot paths

### P3 - Polish (Week 4+)

10. **Cache optimization**
11. **Arena allocators**
12. **Contention hardening**

---

## Success Criteria

### Gate 1: In-Memory Mode

| Metric | Target |
|--------|--------|
| `engine/put_direct` | <3µs |
| `kvstore/put` | <8µs |
| `eventlog/append` | <10µs |
| Throughput | 250K+ ops/sec |

### Gate 2: Buffered Mode

| Metric | Target |
|--------|--------|
| `kvstore/put` | <30µs |
| Throughput | 50K+ ops/sec |
| Loss window | Configurable (default 100ms) |

### Gate 3: Facade Tax

| Ratio | Target |
|-------|--------|
| A1/A0 | <10× |
| B/A1 | <5× |
| B/A0 | <30× |

### Gate 4: Contention Scaling

| Threads | Disjoint Key Scaling |
|---------|---------------------|
| 2 | ≥1.8× of 1-thread |
| 4 | ≥3.2× of 1-thread |
| 8 | ≥6.0× of 1-thread |

---

## References

- M3 Benchmark Results: `target/benchmark-results/bench_output_*.txt`
- Redis Comparison: `target/benchmark-results/redis_comparison_*.txt`
- Current Architecture: `docs/architecture/M3_ARCHITECTURE.md`
