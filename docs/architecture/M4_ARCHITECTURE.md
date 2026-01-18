# M4 Architecture Specification: Performance

**Version**: 1.1
**Status**: Planning Phase (Validated)
**Last Updated**: 2026-01-15

---

## Executive Summary

This document specifies the architecture for **Milestone 4 (M4): Performance** of the in-memory agent database. M4 introduces durability modes and targeted optimizations to **remove architectural barriers** to Redis-class latency on hot-path operations.

**M4 Goals**:
- Implement three durability modes: InMemory, Buffered, Strict
- Achieve 250K simple ops/sec in InMemory mode
- Reduce facade tax to acceptable ratios (A1/A0 < 10×, B/A1 < 5×)
- Improve read path latency to < 10µs
- Enable multi-thread scaling for disjoint keys
- Add performance instrumentation for ongoing optimization

**Built on M1-M3**:
- M1 provides: Storage (UnifiedStore), WAL, Recovery
- M2 provides: OCC transactions, Snapshot isolation, Conflict detection
- M3 provides: Five primitives (KVStore, EventLog, StateCell, TraceStore, RunIndex)
- M4 adds: Durability modes, performance optimizations, instrumentation

**Non-Goals for M4**:
- Arena allocators and advanced memory management
- Cache line alignment and SoA transformations
- Contention backoff strategies
- Conflict detection optimization
- Vector Store (M6)
- Network layer (M7)

**Critical Framing**:
> M4 is a **de-blocking milestone**, not a final optimization milestone. M4 removes architectural blockers that would prevent reaching Redis-class latency. It is not expected to reach Redis-class absolute latency—that requires further work in M5+ on data layout, cache behavior, and lock-free structures.

**M4 Philosophy**:
> **M4 does not aim to be fast. M4 aims to be *fastable*.**
>
> M4 is explicitly allowed to be slow relative to Redis. M4 only ensures the architecture *can* be made fast later. Do not rationalize "good enough" at M4 completion.

---

## Current M3 State Analysis

Before implementing M4, the existing codebase was analyzed to validate assumptions. This section documents key findings.

### Key Structure (crates/core/src/types.rs)

```rust
pub struct Key {
    pub namespace: Namespace,  // Contains RunId
    pub type_tag: TypeTag,
    pub user_key: Vec<u8>,     // Heap-allocated
}

pub struct Namespace {
    run_id: RunId,
    components: Vec<String>,   // Heap-allocated
}
```

**Implications**:
- RunId is embedded in Namespace, not Key directly
- Key clones are expensive due to `Vec<u8>` user_key
- Ordering is namespace → type_tag → user_key (BTreeMap compatible)

### RunId Structure (crates/core/src/types.rs)

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RunId(Uuid);  // 16 bytes, Copy trait
```

**Implications**:
- RunId is `Copy` - trivially cheap to pass around
- UUID v4 (random) - good hash distribution for DashMap sharding
- No allocation overhead

### Current Snapshot Implementation (CRITICAL BOTTLENECK)

```rust
// crates/storage/src/unified.rs
pub fn create_snapshot(&self) -> ClonedSnapshotView {
    let data = self.data.read();
    ClonedSnapshotView::new(version, data.clone())  // O(n) DEEP CLONE
}
```

**Problem**: Current snapshot is **O(n) where n = total keys**. Every transaction pays this cost.

**M4 Solution**: Replace with lazy snapshot that references live data via Arc.

### Current TransactionContext (crates/concurrency/src/transaction.rs)

```rust
pub struct TransactionContext {
    pub txn_id: u64,
    pub run_id: RunId,
    pub start_version: u64,
    snapshot: Option<Box<dyn SnapshotView>>,
    pub read_set: HashMap<Key, u64>,      // key → version read
    pub write_set: HashMap<Key, Value>,   // key → new value
    pub delete_set: HashSet<Key>,
    pub cas_set: Vec<CASOperation>,
    pub status: TransactionStatus,
}
```

**Implications**:
- read_set uses `HashMap<Key, u64>` - tracks versions for conflict detection
- write_set and delete_set are separate (not unified)
- Snapshot is trait object (`Box<dyn SnapshotView>`) - dynamic dispatch overhead
- M4 pooling must preserve all these fields and their capacities

### WAL Abstraction (crates/durability/src/wal.rs)

The WAL already has durability mode support:

```rust
pub enum DurabilityMode {
    Strict,                           // fsync every write
    Batched { batch_size, interval }, // periodic fsync
    Async { ... },                    // background fsync
}
```

**Implication**: M4 durability modes can build on existing WAL infrastructure. The main change is exposing this at the Database API level.

---

## Table of Contents

1. [System Overview](#1-system-overview)
2. [Architecture Principles](#2-architecture-principles)
3. [Durability Modes](#3-durability-modes)
4. [Transaction Object Pooling](#4-transaction-object-pooling)
5. [Lock Sharding](#5-lock-sharding)
6. [Read Path Optimization](#6-read-path-optimization)
7. [Performance Instrumentation](#7-performance-instrumentation)
8. [API Changes](#8-api-changes)
9. [Performance Targets](#9-performance-targets)
10. [Testing Strategy](#10-testing-strategy)
11. [Migration from M3](#11-migration-from-m3)
12. [Known Limitations](#12-known-limitations)
13. [Future Extension Points](#13-future-extension-points)

---

## 1. System Overview

### 1.1 M4 Architecture Stack

```
+-------------------------------------------------------------+
|                    Application                               |
|                    (Agent Applications)                      |
+-----------------------------+-------------------------------+
                              |
                              | High-level typed APIs
                              v
+-------------------------------------------------------------+
|                   Primitives Layer (M3)                      |
|                   (Stateless Facades)                        |
|                                                              |
|  +----------+  +----------+  +------------+  +----------+   |
|  | KVStore  |  | EventLog |  | StateCell  |  |  Trace   |   |
|  +----------+  +----------+  +------------+  +----------+   |
|                           |                                  |
|  +----------------------------------------------------+     |
|  |                    Run Index                        |     |
|  +----------------------------------------------------+     |
|                           |                                  |
+---------------------------+----------------------------------+
                            |
                            | Database transaction API
                            v
+-------------------------------------------------------------+
|                    Engine Layer (M1-M2)                      |
|                                                              |
|  +-------------------------------------------------------+  |
|  |                      Database                          |  |
|  |                                                        |  |
|  | - transaction(run_id, closure)                        |  |
|  | - DurabilityMode selection          (M4 NEW)          |  |
|  +-------------------------------------------------------+  |
|                           |                                  |
+---------------------------+----------------------------------+
                            |
          +-----------------+-----------------+
          |                 |                 |
          v                 v                 v
+-----------------+ +-----------------+ +------------------+
| Storage (M1)    | | Durability (M4) | | Concurrency (M4) |
|                 | |     UPDATED     | |     UPDATED      |
| - UnifiedStore  | |                 | |                  |
| - BTreeMap      | | - InMemory mode | | - Lock sharding  |
| - Versioning    | | - Buffered mode | | - Txn pooling    |
|                 | | - Strict mode   | | - Read-path opt  |
+-----------------+ +-----------------+ +------------------+
          |                 |                 |
          +-----------------+-----------------+
                            |
                            v
+-------------------------------------------------------------+
|                    Core Types (M1)                           |
+-------------------------------------------------------------+
```

### 1.2 What's New in M4

| Component | M3 Behavior | M4 Behavior |
|-----------|-------------|-------------|
| **Durability** | Always fsync (Strict) | Selectable: InMemory, Buffered, Strict |
| **WAL** | Sync write on every op | Mode-dependent: None, Async, Sync |
| **Transactions** | Allocate on every begin | Pooled, reusable transaction objects |
| **Locking** | Single global RwLock | Sharded by RunId |
| **Read path** | Full transaction overhead | Optimized bypass where safe |

### 1.3 Performance Gap Analysis

From M3 benchmarks:

| Layer | M3 Latency | M4 Target | Gap |
|-------|------------|-----------|-----|
| `core/get_hot` | 33 ns | 33 ns | ✅ Already optimal |
| `core/put_hot_prealloc` | 887 ns | 887 ns | ✅ Already optimal |
| `engine/put_direct` | 2.1 ms | <3 µs | **700×** (fsync) |
| `kvstore/put` | 2.2 ms | <8 µs | **275×** (fsync + facade) |
| `kvstore/get` | 139 µs | <5 µs | **28×** (overhead) |

**Root cause**: The raw data structure is already faster than Redis. The 700× slowdown is entirely from fsync on every write.

---

## 2. Architecture Principles

### 2.1 M4-Specific Principles

1. **Durability is a Spectrum**
   - Different use cases need different durability guarantees
   - Performance and durability are traded off explicitly
   - Users choose their position on the spectrum

2. **Hot Path Purity (Syscall-Free)**
   - Tier A0, A1, and B hot paths must NOT:
     - Perform syscalls (including `time()`, `rand()`)
     - Touch filesystem
     - Perform logging
     - Allocate heap memory
     - Use trait objects or dynamic dispatch
     - Trigger page faults
   - This is enforced by code review and benchmark validation

3. **Measure Before Optimize**
   - All optimizations validated by benchmarks
   - Per-layer instrumentation for visibility
   - Baseline tagged for comparison

4. **Preserve Semantics (ACID Clarity)**
   - All durability modes provide same ACI guarantees
   - Only D (Durability) differs by mode:

   | Property | InMemory | Buffered | Strict |
   |----------|----------|----------|--------|
   | **A**tomicity | ✓ | ✓ | ✓ |
   | **C**onsistency | ✓ | ✓ | ✓ |
   | **I**solation | ✓ | ✓ | ✓ |
   | **D**urability | ✗ | Bounded | ✓ |

   - OCC, snapshot isolation, conflict detection unchanged across all modes

5. **Atomicity Scope (CRITICAL)**

   > **Transactions are atomic within a single RunId. Cross-run writes are NOT guaranteed to be atomic unless explicitly coordinated by the caller.**

   This is intentional:
   - RunId is the primary isolation domain
   - Most operations are per-run
   - Cross-run atomicity would require global locking, killing scalability
   - If cross-run atomicity is needed, caller must coordinate explicitly

   **Implications for `apply()`**: When a write set spans multiple runs, operations are applied one-by-one. Observers may see partial state during application. This is acceptable because cross-run operations are administrative, not hot-path.

6. **Snapshot Semantic Invariant (NON-NEGOTIABLE)**

   > **Fast-path reads must be observationally equivalent to a snapshot-based transaction.**

   This means:
   - No dirty reads (uncommitted data)
   - No torn reads (partial write sets)
   - No stale reads (older than snapshot version)
   - No mixing versions (key A at version X, key B at version Y where Y > X)

   **Why this matters**:
   - Breaking this breaks agent reasoning guarantees
   - Breaking this makes replay non-deterministic
   - Breaking this prevents promoting fast-path to transaction later

   "Latest committed at snapshot acquisition" is the correct definition. Fast-path reads return the same value a read-only transaction started at that moment would return.

7. **Backwards Compatibility**
   - M3 code works unchanged (defaults to Strict mode)
   - New APIs are additive
   - No breaking changes to primitive APIs

### 2.2 Optimization Scope

**In Scope for M4:**
- Durability mode selection
- Transaction object pooling
- Lock sharding by RunId
- Read path optimization
- Performance instrumentation

**Explicitly Out of Scope (Deferred, Not Abandoned):**

The following are **required for Redis-class performance** but deferred to M5+:

| Item | Why Deferred | Why Required |
|------|--------------|--------------|
| Arena allocators | Requires significant refactoring | Eliminates malloc overhead |
| Cache line alignment | Requires struct redesign | Prevents false sharing, improves locality |
| Structure of Arrays (SoA) | Requires data model changes | Enables vectorized operations |
| Prefetching | Only useful after layout is fixed | Hides memory latency |
| Contention backoff | Optimization, not blocker | Reduces retry storms |
| Conflict detection optimization | Optimization, not blocker | Reduces validation overhead |

**WARNING**: M4 is a de-blocking milestone, not the end state. If M4 is treated as "good enough", we will never reach Redis parity. These deferred items are tracked in `PERFORMANCE_OPTIMIZATION_REFERENCE.md` and **must be addressed in M5+** if we want true Redis-class performance.

### 2.3 M4 Storage Architecture

**M4 changes the storage layer** to address the RwLock+BTreeMap bottleneck.

**Problem with M3 approach:**

| Issue | Impact |
|-------|--------|
| `BTreeMap` is pointer-heavy | Cache misses on every traversal |
| `BTreeMap` is branch-heavy | Branch mispredictions |
| `RwLock` is kernel-heavy | Syscalls on contention |
| `RwLock` is poor under write load | Writers starve readers |

**M4 storage approach:**

```rust
// M4: Use DashMap with FxHash for sharding
// NOTE: Use rustc-hash crate (not fxhash) - it's more maintained
use dashmap::DashMap;
use rustc_hash::{FxHashMap, FxBuildHasher};

pub struct ShardedStore {
    /// Per-run shards using DashMap (sharded internally)
    shards: DashMap<RunId, Shard, FxBuildHasher>,
}

struct Shard {
    /// Use FxHashMap instead of BTreeMap for O(1) access
    data: FxHashMap<Key, VersionedValue>,
}
```

**Required Cargo dependencies:**
```toml
[dependencies]
dashmap = "5"
rustc-hash = "1.1"
```

**Why this is better:**
- `DashMap`: Lock-free reads, sharded writes
- `FxHashMap`: O(1) lookups, no pointer chasing
- `FxHash` (from rustc-hash): Fast, non-cryptographic hash (fine for in-process use)

**Trade-off**: We lose ordered iteration (BTreeMap → HashMap). This affects `list()` operations which now require a sort. Acceptable because:
- `list()` is not on hot path
- `get/put` are 10-100× more frequent

**Still provisional:**
- DashMap still has internal locks
- HashMap still allocates on growth
- Values still cloned on read

These require M5+ work for true Redis parity.

> **WARNING**: DashMap + HashMap is a **tactical improvement**, not a final architecture. It removes global lock contention and pointer-heavy trees, but it does not solve memory layout, cache friendliness, or allocator behavior. **Treat this as scaffolding, not a destination.**

---

## 3. Durability Modes

### 3.1 Overview

Three modes, user-selectable at database open or per-operation:

| Mode | WAL | fsync | Target Latency | Data Loss Window | Use Case |
|------|-----|-------|----------------|------------------|----------|
| **InMemory** | None | None | <3-10 µs | All (on crash) | Redis competitor, caches, ephemeral |
| **Buffered** | Append | Periodic | <20-50 µs | Bounded (configurable) | Production default |
| **Strict** | Append + fsync | Every write | ~2 ms | Zero | Checkpoints, metadata, audit |

### 3.2 API Design

```rust
/// Durability mode for database operations
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DurabilityMode {
    /// No persistence. All data lost on crash.
    /// Fastest mode - no WAL, no fsync.
    InMemory,

    /// WAL append without immediate fsync.
    /// Periodic flush based on interval or batch size.
    /// Bounded data loss window.
    Buffered {
        /// Flush interval in milliseconds
        flush_interval_ms: u64,
        /// Maximum pending writes before flush
        max_pending_writes: usize,
    },

    /// fsync on every write.
    /// Zero data loss but slowest.
    Strict,
}

impl Default for DurabilityMode {
    fn default() -> Self {
        // Default to Strict for backwards compatibility
        DurabilityMode::Strict
    }
}
```

### 3.3 Database Builder API

```rust
impl Database {
    /// Create a new database builder
    pub fn builder() -> DatabaseBuilder {
        DatabaseBuilder::new()
    }
}

pub struct DatabaseBuilder {
    path: Option<PathBuf>,
    durability: DurabilityMode,
    // ... other options
}

impl DatabaseBuilder {
    /// Set durability mode
    pub fn durability(mut self, mode: DurabilityMode) -> Self {
        self.durability = mode;
        self
    }

    /// Open the database
    pub fn open(self) -> Result<Database> {
        // ...
    }
}

// Usage examples
let db = Database::builder()
    .durability(DurabilityMode::InMemory)
    .open()?;

let db = Database::builder()
    .durability(DurabilityMode::Buffered {
        flush_interval_ms: 100,
        max_pending_writes: 1000,
    })
    .open()?;

let db = Database::builder()
    .durability(DurabilityMode::Strict)
    .open()?;
```

### 3.4 Per-Operation Override

For critical writes in non-strict mode:

```rust
impl Database {
    /// Execute transaction with durability override
    pub fn transaction_with_durability<F, T>(
        &self,
        run_id: RunId,
        durability: DurabilityMode,
        f: F,
    ) -> Result<T>
    where
        F: FnOnce(&mut TransactionContext) -> Result<T>;
}

// Usage: Force fsync for critical metadata even in Buffered mode
db.transaction_with_durability(
    run_id,
    DurabilityMode::Strict,
    |txn| {
        txn.put(critical_key, critical_value)?;
        Ok(())
    }
)?;
```

### 3.5 Implementation Details

#### InMemory Mode

```rust
impl InMemoryDurability {
    fn commit(&self, write_set: &WriteSet) -> Result<()> {
        // No WAL append
        // No fsync
        // Just apply to storage
        self.storage.apply(write_set)
    }
}
```

**Characteristics:**
- Zero disk I/O
- All data in memory only
- Lost on crash/restart
- Fastest possible mode

#### Buffered Mode

```rust
impl BufferedDurability {
    fn commit(&self, write_set: &WriteSet) -> Result<()> {
        // Append to WAL buffer (no syscall)
        self.wal_buffer.append(write_set)?;

        // Apply to storage
        self.storage.apply(write_set)?;

        // Check if flush needed
        if self.should_flush() {
            self.flush_async();
        }

        Ok(())
    }

    fn should_flush(&self) -> bool {
        self.pending_writes >= self.max_pending_writes
            || self.time_since_last_flush() >= self.flush_interval
    }

    fn flush_async(&self) {
        // Signal background thread to fsync
        self.flush_signal.notify_one();
    }
}

// Background flush thread with shutdown support
fn flush_thread(durability: Arc<BufferedDurability>) {
    loop {
        // Wait for flush signal or shutdown
        let result = durability.flush_signal.wait_timeout(
            Duration::from_millis(durability.flush_interval_ms)
        );

        // Check shutdown flag before flushing
        if durability.shutdown.load(Ordering::Acquire) {
            // Final flush before exit
            let _ = durability.wal.fsync();
            return;
        }

        // Perform the flush
        if let Err(e) = durability.wal.fsync() {
            // Log error but don't crash - retry on next interval
            eprintln!("WAL fsync failed: {}", e);
        }
    }
}

// Thread lifecycle management
pub struct BufferedDurability {
    wal: Arc<WAL>,
    wal_buffer: WalBuffer,
    flush_signal: Condvar,
    flush_interval_ms: u64,
    max_pending_writes: usize,
    pending_writes: AtomicUsize,
    shutdown: AtomicBool,           // Shutdown signal
    flush_thread: Option<JoinHandle<()>>,  // Thread handle for join
}

impl Drop for BufferedDurability {
    fn drop(&mut self) {
        // Signal shutdown
        self.shutdown.store(true, Ordering::Release);
        self.flush_signal.notify_all();

        // Wait for thread to finish
        if let Some(handle) = self.flush_thread.take() {
            let _ = handle.join();
        }
    }
}
```

**Characteristics:**
- WAL append is memory-only (fast)
- Periodic fsync in background thread
- Bounded data loss: max(flush_interval, pending_writes)
- Good balance of performance and durability

#### Strict Mode

```rust
impl StrictDurability {
    fn commit(&self, write_set: &WriteSet) -> Result<()> {
        // Append to WAL
        self.wal.append(write_set)?;

        // fsync immediately
        self.wal.fsync()?;

        // Apply to storage
        self.storage.apply(write_set)?;

        Ok(())
    }
}
```

**Characteristics:**
- fsync on every write
- Zero data loss
- Slowest mode (~2ms per write)
- Use for critical data only

### 3.6 Recovery Behavior

| Mode | On Clean Shutdown | On Crash |
|------|-------------------|----------|
| **InMemory** | Data lost | Data lost |
| **Buffered** | WAL flushed, data safe | Up to flush_interval lost |
| **Strict** | Data safe | Data safe |

```rust
impl Database {
    /// Graceful shutdown - ensures all data is persisted
    pub fn shutdown(&self) -> Result<()> {
        match self.durability_mode {
            DurabilityMode::InMemory => {
                // Nothing to do - data is ephemeral
                Ok(())
            }
            DurabilityMode::Buffered { .. } => {
                // Flush pending writes
                self.durability.flush_sync()?;
                Ok(())
            }
            DurabilityMode::Strict => {
                // Already synced - nothing to do
                Ok(())
            }
        }
    }
}
```

---

## 4. Transaction Object Pooling

### 4.1 Problem

Current M3 behavior allocates a new `TransactionContext` on every `begin_transaction()`:

```rust
// M3: Allocates every time
pub fn begin_transaction(&self, run_id: RunId) -> TransactionContext {
    TransactionContext {
        run_id,
        snapshot: self.storage.snapshot(),  // Clone
        read_set: HashMap::new(),            // Allocate
        write_set: HashMap::new(),           // Allocate
        version: self.next_version(),
    }
}
```

From benchmarks: `core/put_hot` (with allocation) is **2× slower** than `core/put_hot_prealloc`.

### 4.2 Solution

Thread-local pool of reusable transaction contexts:

```rust
thread_local! {
    static TXN_POOL: RefCell<Vec<TransactionContext>> = RefCell::new(Vec::new());
}

impl Database {
    pub fn begin_transaction(&self, run_id: RunId) -> TransactionContext {
        TXN_POOL.with(|pool| {
            match pool.borrow_mut().pop() {
                Some(mut txn) => {
                    // Reuse existing allocation
                    txn.reset(run_id, self.storage.snapshot(), self.next_version());
                    txn
                }
                None => {
                    // Pool empty - allocate new
                    TransactionContext::new(
                        run_id,
                        self.storage.snapshot(),
                        self.next_version(),
                    )
                }
            }
        })
    }

    pub fn end_transaction(&self, txn: TransactionContext) {
        TXN_POOL.with(|pool| {
            let mut pool = pool.borrow_mut();
            if pool.len() < MAX_POOL_SIZE {
                pool.push(txn);  // Return to pool
            }
            // else: drop (pool full)
        });
    }
}

impl TransactionContext {
    /// Reset for reuse without deallocating
    fn reset(&mut self, run_id: RunId, snapshot: Snapshot, version: u64) {
        self.run_id = run_id;
        self.snapshot = snapshot;
        self.read_set.clear();   // Clear but keep capacity
        self.write_set.clear();  // Clear but keep capacity
        self.version = version;
    }
}
```

### 4.3 Pool Configuration

```rust
/// Maximum transaction contexts per thread
const MAX_POOL_SIZE: usize = 8;
```

**Why 8?**
- Typical agent has 1-2 concurrent transactions
- Extra headroom for burst scenarios
- Memory cost: ~1KB per context × 8 = 8KB per thread
- Negligible compared to data size

### 4.4 Success Criteria

- Zero allocations in Tier A1 hot path (measured via benchmarks)
- `core/put_hot` matches `core/put_hot_prealloc` within 10%

---

## 5. Lock Sharding

### 5.1 Problem

Current M3 uses a single global lock:

```rust
// M3: Global lock
pub struct UnifiedStore {
    data: RwLock<BTreeMap<Key, VersionedValue>>,
}
```

From contention benchmarks:
- Same-key: ~45K ops/s ✅
- Disjoint-key: ~45K ops/s ❌ (should scale)

**Problem**: Disjoint keys don't scale because all operations contend on the same lock.

### 5.2 Solution

Shard by RunId using DashMap + FxHashMap (not RwLock + BTreeMap):

```rust
use dashmap::DashMap;
use rustc_hash::{FxHashMap, FxBuildHasher};

pub struct ShardedStore {
    /// Per-run shards using DashMap (lock-free reads, sharded writes)
    shards: DashMap<RunId, Shard, FxBuildHasher>,
}

struct Shard {
    /// FxHashMap for O(1) lookups (not BTreeMap)
    data: FxHashMap<Key, VersionedValue>,
}

impl ShardedStore {
    pub fn get(&self, run_id: &RunId, key: &Key) -> Option<VersionedValue> {
        // Fast path: DashMap.get() is lock-free for reads
        self.shards.get(run_id)
            .and_then(|shard| shard.data.get(key).cloned())
    }

    pub fn put(&self, run_id: &RunId, key: Key, value: VersionedValue) {
        // Get or create shard (sharded lock, not global)
        self.shards
            .entry(*run_id)
            .or_insert_with(|| Shard { data: FxHashMap::default() })
            .data
            .insert(key, value);
    }

    pub fn list(&self, run_id: &RunId, prefix: &[u8]) -> Vec<(Key, VersionedValue)> {
        // Slower than BTreeMap range scan - requires filter + sort
        // But list() is not on hot path
        self.shards.get(run_id)
            .map(|shard| {
                let mut results: Vec<_> = shard.data.iter()
                    .filter(|(k, _)| k.as_bytes().starts_with(prefix))
                    .map(|(k, v)| (k.clone(), v.clone()))
                    .collect();
                results.sort_by(|(a, _), (b, _)| a.cmp(b));
                results
            })
            .unwrap_or_default()
    }
}
```

**Why DashMap + HashMap instead of RwLock + BTreeMap:**

| Aspect | RwLock + BTreeMap | DashMap + HashMap |
|--------|-------------------|-------------------|
| Read locking | Global RwLock | Lock-free reads |
| Write locking | Global RwLock | Per-shard (16-way by default) |
| Lookup | O(log n) + pointer chasing | O(1) + single probe |
| Cache behavior | Poor (tree traversal) | Good (hash bucket) |
| Ordered iteration | Native | Requires sort |

### 5.3 Why RunId Sharding?

| Sharding Strategy | Pros | Cons |
|-------------------|------|------|
| **By RunId** | Natural agent partitioning, no coordination between runs | Cross-run queries slower |
| By Key Hash | Even distribution | Hot keys still contend, no semantic meaning |
| By TypeTag | Primitive isolation | Doesn't help multi-run scaling |

**RunId sharding wins** because:
1. Agents naturally partition by run
2. Cross-run queries are rare (mostly RunIndex)
3. No coordination needed between runs
4. Enables future per-run WAL segments

### 5.4 Cross-Run Operations

Operations that span runs use the slower global path:

| Operation | Path | Performance |
|-----------|------|-------------|
| `kv.get(run_id, key)` | Per-run shard | Fast |
| `kv.put(run_id, key, value)` | Per-run shard | Fast |
| `run_index.query_runs(status)` | Global scan | Slower |
| `run_index.list_runs()` | Global scan | Slower |

This is acceptable because:
- Per-run operations are the hot path (>99% of operations)
- Cross-run queries are administrative (infrequent)

### 5.5 Success Criteria

| Threads | Disjoint Key Scaling Target |
|---------|----------------------------|
| 2 | ≥ 1.8× of 1-thread |
| 4 | ≥ 3.2× of 1-thread |
| 8 | ≥ 6.0× of 1-thread |

---

## 6. Read Path Optimization

### 6.1 Problem

`kvstore/get` is 139µs but target is <5µs (28× gap).

Analysis of read path:
1. Transaction begin (snapshot creation)
2. Key lookup in storage
3. Read-set recording
4. Transaction commit (validation)
5. WAL append (unnecessary for reads!)

**Insight**: Reads don't need WAL or commit validation.

### 6.2 Solution

Fast path for read-only operations:

```rust
impl KVStore {
    /// Optimized read - bypasses full transaction overhead
    pub fn get(&self, run_id: RunId, key: &str) -> Result<Option<Value>> {
        // Fast path: direct snapshot read
        let snapshot = self.db.snapshot();
        let storage_key = Key::new_kv(run_id.namespace(), key);

        match snapshot.get(&storage_key) {
            Some(versioned) => Ok(Some(versioned.value.clone())),
            None => Ok(None),
        }
    }
}
```

**What we skip:**
- Transaction object allocation
- Read-set recording (not needed for single read)
- Write-set creation
- Commit validation
- WAL append

**What we keep:**
- Snapshot isolation (consistent view)
- Run isolation (key prefixing)

### 6.3 When Fast Path is Safe

| Operation | Fast Path Safe? | Reason |
|-----------|-----------------|--------|
| Single-key read | ✅ Yes | No write-write conflicts possible |
| Multi-key read | ✅ Yes | Snapshot provides consistency |
| Read-then-write | ❌ No | Need transaction for atomicity |
| CAS | ❌ No | Need version tracking |

**Hard Invariant**:
> All fast-path reads must be **observationally equivalent** to a snapshot-based transaction. Any optimization that changes visibility, ordering, or consistency is forbidden. No dirty reads, no stale reads, no torn reads.

### 6.4 Implementation

```rust
impl Database {
    /// Get a consistent snapshot for read operations
    pub fn snapshot(&self) -> Snapshot {
        self.storage.snapshot()
    }
}

impl KVStore {
    /// Fast read - no transaction overhead
    pub fn get(&self, run_id: RunId, key: &str) -> Result<Option<Value>> {
        let snapshot = self.db.snapshot();
        let storage_key = Key::new_kv(run_id.namespace(), key);
        Ok(snapshot.get(&storage_key).map(|v| v.value.clone()))
    }

    /// Batch read - single snapshot for consistency
    pub fn get_many(&self, run_id: RunId, keys: &[&str]) -> Result<Vec<Option<Value>>> {
        let snapshot = self.db.snapshot();
        let namespace = run_id.namespace();

        keys.iter()
            .map(|key| {
                let storage_key = Key::new_kv(namespace.clone(), key);
                Ok(snapshot.get(&storage_key).map(|v| v.value.clone()))
            })
            .collect()
    }
}
```

### 6.5 Snapshot Fast Path Requirements

**Hard Requirements** (non-negotiable for M4):

| Requirement | Rationale |
|-------------|-----------|
| Snapshot acquisition must be **allocation-free** | Allocations are ~50-100ns each, kills throughput |
| Snapshot acquisition must **not acquire global locks** | Lock contention destroys scaling |
| Snapshot acquisition must **not scan data structures** | O(n) snapshot = unusable at scale |
| Snapshot acquisition must **not touch WAL** | WAL is for writes only |

**Implementation approach:**
```rust
// GOOD: Atomic version bump, no allocation
pub fn snapshot(&self) -> Snapshot {
    Snapshot {
        version: self.version.load(Ordering::Acquire),
        store: Arc::clone(&self.store),  // Arc bump only
    }
}

// BAD: Allocation on every snapshot
pub fn snapshot(&self) -> Snapshot {
    Snapshot {
        data: self.store.read().clone(),  // Full clone!
    }
}
```

### 6.6 Success Criteria

- `kvstore/get` < 10µs (stretch: <5µs)
- `engine/get_direct` < 1µs
- `snapshot_acquire` < 500ns (hard requirement)

---

## 7. Performance Instrumentation

### 7.1 Baseline Tagging

```bash
# Tag M3 baseline before any M4 changes
git tag m3_baseline_perf
```

All M4 optimizations measured relative to this tag.

### 7.2 Per-Layer Timing

Feature-gated instrumentation for debugging:

```rust
#[cfg(feature = "perf-trace")]
pub struct PerfTrace {
    pub snapshot_acquire_ns: u64,
    pub read_set_validate_ns: u64,
    pub write_set_apply_ns: u64,
    pub wal_append_ns: u64,
    pub fsync_ns: u64,
    pub commit_total_ns: u64,
}

#[cfg(feature = "perf-trace")]
impl TransactionContext {
    pub fn commit_with_trace(&mut self) -> Result<PerfTrace> {
        let mut trace = PerfTrace::default();

        let start = Instant::now();
        self.validate_read_set()?;
        trace.read_set_validate_ns = start.elapsed().as_nanos() as u64;

        let start = Instant::now();
        self.apply_write_set()?;
        trace.write_set_apply_ns = start.elapsed().as_nanos() as u64;

        // ... etc

        Ok(trace)
    }
}
```

### 7.3 Benchmark Integration

```rust
// In benchmarks, optionally collect traces
#[cfg(feature = "perf-trace")]
fn bench_with_trace(db: &Database, run_id: RunId) -> PerfTrace {
    db.transaction_with_trace(run_id, |txn| {
        txn.put(key, value)?;
        Ok(())
    })
}
```

### 7.4 Feature Flags

```toml
# Cargo.toml
[features]
default = []
perf-trace = []  # Enable per-layer timing
perf-alloc = []  # Track allocations (future)
```

**Usage:**
```bash
# Normal benchmarks
cargo bench

# With instrumentation
cargo bench --features perf-trace
```

### 7.5 Perf-Guided Development Loop

**Every M4 optimization must follow this loop:**

```
1. IDENTIFY    → Find hot function via `perf record` / flamegraph
2. HYPOTHESIZE → State what you expect to improve and by how much
3. IMPLEMENT   → Make the change
4. BENCHMARK   → Run targeted benchmark (not full suite)
5. COMPARE     → Compare to m3_baseline_perf tag
6. DECIDE      → Keep if improved, revert if not
```

**Rules:**
- No speculative optimizations ("this should be faster")
- No premature abstractions ("we might need this later")
- No changes without before/after numbers
- All changes must be justified by data

**Example:**
```
Hypothesis: "Removing clone() in read path will reduce kvstore/get by 50%"
Before: 139µs
After: 68µs
Improvement: 51%
Decision: KEEP
```

---

## 8. API Changes

### 8.1 New Types

```rust
/// Durability mode selection
pub enum DurabilityMode {
    InMemory,
    Buffered { flush_interval_ms: u64, max_pending_writes: usize },
    Strict,
}

/// Database builder for configuration
pub struct DatabaseBuilder { ... }
```

### 8.2 New Database Methods

```rust
impl Database {
    /// Create builder for database configuration
    pub fn builder() -> DatabaseBuilder;

    /// Get current durability mode
    pub fn durability_mode(&self) -> DurabilityMode;

    /// Execute transaction with durability override
    pub fn transaction_with_durability<F, T>(
        &self,
        run_id: RunId,
        durability: DurabilityMode,
        f: F,
    ) -> Result<T>;

    /// Get a read-only snapshot
    pub fn snapshot(&self) -> Snapshot;

    /// Graceful shutdown (flushes pending writes)
    pub fn shutdown(&self) -> Result<()>;
}
```

### 8.3 Unchanged APIs

All M3 primitive APIs remain unchanged:
- `KVStore::get`, `put`, `delete`, `list`
- `EventLog::append`, `read`, `verify_chain`
- `StateCell::read`, `cas`, `transition`
- `TraceStore::record`, `query_by_type`
- `RunIndex::create_run`, `update_status`

M3 code works unchanged (defaults to Strict mode).

---

## 9. Performance Targets

### 9.1 Latency Targets

| Operation | InMemory | Buffered | Strict |
|-----------|----------|----------|--------|
| `engine/get_direct` | <500 ns | <500 ns | <500 ns |
| `engine/put_direct` | **<3 µs** | <20 µs | ~2 ms |
| `kvstore/get` | <5 µs | <5 µs | <5 µs |
| `kvstore/put` | **<8 µs** | <30 µs | ~2 ms |
| `eventlog/append` | **<10 µs** | <40 µs | ~3 ms |

### 9.2 Throughput Targets

**Overall targets:**

| Mode | Target | M3 Baseline |
|------|--------|-------------|
| InMemory | **250K ops/sec** | ~475 ops/sec |
| Buffered | 50K ops/sec | ~475 ops/sec |
| Strict | ~500 ops/sec | ~475 ops/sec |

**Throughput by scenario (InMemory mode):**

| Scenario | Target | Notes |
|----------|--------|-------|
| 1-thread, hot key | ≥ 250K ops/sec | Baseline single-threaded |
| 1-thread, uniform random | ≥ 200K ops/sec | Cache miss impact |
| 4-thread, disjoint keys | ≥ 800K ops/sec | ~3.2× scaling |
| 4-thread, same key | ≥ 25% of 1-thread | Contention penalty |
| 8-thread, disjoint keys | ≥ 1.4M ops/sec | ~5.6× scaling |

**Why these numbers?**
- Redis over TCP: ~100K-200K ops/sec (network-bound)
- Redis internal loop (in-process): **millions of ops/sec**
- We target 250K as "removes blockers", not "achieves parity"
- True Redis parity (millions ops/sec) requires M5+ data layout work

### 9.3 Facade Tax Targets

| Ratio | Target | M3 Measured | Notes |
|-------|--------|-------------|-------|
| A1/A0 | <10× | ~2400× (Strict) | Fixed by InMemory mode |
| B/A1 | <5× | ~10× | Improved by read optimization |
| B/A0 | <30× | ~4000× | Fixed by above |

**Enforcement Rule**:
> Facade tax must be **justified**, not just measured. Any layer with B/A1 > 5× must provide a written justification in the PR explaining why the overhead is necessary and what would be required to reduce it.

### 9.4 Contention Scaling Targets

| Threads | Disjoint Key Scaling |
|---------|---------------------|
| 1 | Baseline |
| 2 | ≥ 1.8× |
| 4 | ≥ 3.2× |
| 8 | ≥ 6.0× |

---

## 10. Testing Strategy

### 10.1 Unit Tests

```rust
#[cfg(test)]
mod tests {
    #[test]
    fn test_inmemory_mode_no_wal() {
        let db = Database::builder()
            .durability(DurabilityMode::InMemory)
            .open_temp()?;

        db.transaction(run_id, |txn| {
            txn.put(key, value)?;
            Ok(())
        })?;

        // Verify no WAL file created
        assert!(!db.wal_path().exists());
    }

    #[test]
    fn test_buffered_mode_periodic_flush() {
        let db = Database::builder()
            .durability(DurabilityMode::Buffered {
                flush_interval_ms: 100,
                max_pending_writes: 10,
            })
            .open_temp()?;

        // Write 5 records (below threshold)
        for i in 0..5 {
            db.transaction(run_id, |txn| txn.put(key, value))?;
        }

        // WAL should not be fsynced yet
        // (hard to test directly, but can verify data is in buffer)

        // Write 5 more (triggers flush at 10)
        for i in 0..5 {
            db.transaction(run_id, |txn| txn.put(key, value))?;
        }

        // Now WAL should be fsynced
    }

    #[test]
    fn test_strict_mode_immediate_fsync() {
        let db = Database::builder()
            .durability(DurabilityMode::Strict)
            .open_temp()?;

        db.transaction(run_id, |txn| {
            txn.put(key, value)?;
            Ok(())
        })?;

        // Simulate crash and recover
        drop(db);
        let db = Database::open(path)?;

        // Data should survive
        let value = db.transaction(run_id, |txn| txn.get(&key))?;
        assert!(value.is_some());
    }
}
```

### 10.2 Benchmark Tests

```rust
#[test]
fn test_inmemory_meets_latency_target() {
    let db = Database::builder()
        .durability(DurabilityMode::InMemory)
        .open_temp()?;

    let kv = KVStore::new(db.clone());

    let start = Instant::now();
    for _ in 0..1000 {
        kv.put(run_id, "key", value.clone())?;
    }
    let elapsed = start.elapsed();

    let avg_ns = elapsed.as_nanos() / 1000;
    assert!(avg_ns < 8_000, "kvstore/put should be <8µs, was {}ns", avg_ns);
}

#[test]
fn test_contention_scaling() {
    let db = Database::builder()
        .durability(DurabilityMode::InMemory)
        .open_temp()?;

    // Single-threaded baseline
    let single = measure_throughput(&db, 1);

    // Multi-threaded (disjoint keys)
    let dual = measure_throughput(&db, 2);
    let quad = measure_throughput(&db, 4);

    assert!(dual >= single * 1.8, "2 threads should be ≥1.8x");
    assert!(quad >= single * 3.2, "4 threads should be ≥3.2x");
}
```

### 10.3 Recovery Tests

```rust
#[test]
fn test_buffered_recovery_bounded_loss() {
    let db = Database::builder()
        .durability(DurabilityMode::Buffered {
            flush_interval_ms: 1000,  // 1 second
            max_pending_writes: 100,
        })
        .open_temp()?;

    // Write 50 records
    for i in 0..50 {
        db.transaction(run_id, |txn| txn.put(key(i), value))?;
    }

    // Force crash (no graceful shutdown)
    std::mem::forget(db);

    // Recover
    let db = Database::open(path)?;

    // Some records may be lost (up to 100 pending)
    // But at least the flushed ones survive
    let recovered = count_records(&db, run_id);
    // Can't guarantee exact count, but should be in bounds
    assert!(recovered <= 50);
}
```

### 10.4 Compatibility Tests

```rust
#[test]
fn test_m3_code_works_unchanged() {
    // M3 code - no durability mode specified
    let db = Database::open(path)?;  // Defaults to Strict

    let kv = KVStore::new(db.clone());
    kv.put(run_id, "key", value)?;

    let result = kv.get(run_id, "key")?;
    assert_eq!(result, Some(value));
}
```

---

## 11. Migration from M3

### 11.1 Backwards Compatibility

M3 code works unchanged:

```rust
// M3 code (still works - defaults to Strict mode)
let db = Database::open(path)?;
let kv = KVStore::new(db.clone());
kv.put(run_id, "key", value)?;
```

### 11.2 Opting into InMemory Mode

```rust
// M4 code - explicit InMemory mode
let db = Database::builder()
    .durability(DurabilityMode::InMemory)
    .open()?;

let kv = KVStore::new(db.clone());
kv.put(run_id, "key", value)?;  // Now ~250K ops/sec
```

### 11.3 Migration Path

1. **Phase 1**: Update to M4, keep Strict mode (default)
   - Zero code changes required
   - Same behavior as M3

2. **Phase 2**: Identify hot paths, switch to InMemory/Buffered
   - Caches, ephemeral data → InMemory
   - Production agent runs → Buffered
   - Checkpoints, audit logs → Strict

3. **Phase 3**: Use per-operation override for critical writes
   ```rust
   db.transaction_with_durability(run_id, DurabilityMode::Strict, |txn| {
       txn.put(critical_metadata_key, value)?;
       Ok(())
   })?;
   ```

---

## 12. Known Limitations

### 12.1 M4 Limitations

| Limitation | Impact | Mitigation |
|------------|--------|------------|
| **InMemory data loss** | All data lost on crash | Use for ephemeral data only |
| **Buffered bounded loss** | Up to flush_interval data lost | Configure based on tolerance |
| **Cross-run query performance** | Global scan for RunIndex queries | Keep run count manageable |
| **No hot-key mitigation** | Hot keys still contend within shard | Application-level sharding |
| **DashMap still has internal locks** | Not fully lock-free | M5+: Epoch-based or RCU |
| **Clone on read** | Allocation per read | Provisional for M4, revisit M5 |

### 12.2 Architectural Red Flags (Hard Stop Criteria)

**If any of these are true after M4 implementation, the architecture must be revisited before proceeding to M5:**

| Metric | Red Flag Threshold | Action |
|--------|-------------------|--------|
| Snapshot acquisition | > 2µs | Redesign snapshot mechanism |
| A1/A0 ratio | > 20× | Remove abstraction layers |
| B/A1 ratio | > 8× | Inline facade logic |
| Disjoint scaling (4 threads) | < 2.5× | Redesign sharding |
| p99 latency | > 20× mean | Find and fix tail latency source |
| Hot-path allocations | > 0 | Eliminate allocations |

**These are not negotiable.** If we hit a red flag, we stop feature work and fix the architecture.

### 12.3 What M4 Does NOT Provide

- Arena allocators (future M5+)
- Cache line alignment (future)
- Contention backoff (future)
- Conflict detection optimization (future)
- Vector Store (M6)
- Network layer (M7)

---

## 13. Future Extension Points

### 13.1 M5: Replay & Polish

- Deterministic replay using EventLog
- Run lifecycle completion
- Performance polish based on M4 learnings

### 13.2 M5+ Performance Architecture (Preview)

These are **not optional nice-to-haves**. They are required for Redis-class performance and must be addressed in M5+:

| Category | Items | Expected Impact |
|----------|-------|-----------------|
| **Memory Management** | Arena allocators, object pools, slab allocators | Eliminate malloc overhead |
| **Data Layout** | Cache-line alignment (64B), SoA transforms, struct flattening | 2-5× from cache locality |
| **Lock-Free Reads** | Epoch-based reclamation, RCU patterns, hazard pointers | Remove read-side locking |
| **False Sharing** | Per-thread buffers, padding, alignment | Fix multi-core scaling |
| **Prefetching** | Manual prefetch, stride-friendly access | Hide memory latency |
| **Conflict Detection** | Bloom filters, bitsets, epoch validation | Reduce validation cost |

**Detailed roadmap in `PERFORMANCE_OPTIMIZATION_REFERENCE.md`.**

This section exists to prevent M4 from becoming the de facto end state. These items are centralized here, not scattered in footnotes.

### 13.3 M6: Vector Store

Vector primitive with HNSW index, benefits from M4 durability modes.

---

## 14. Appendix

### 14.1 Benchmark Commands

```bash
# Tag baseline
git tag m3_baseline_perf

# Run full benchmark suite
./scripts/bench_runner.sh --full

# Run specific benchmark
cargo bench --bench m3_primitives -- kvstore/put

# Run with instrumentation
cargo bench --features perf-trace
```

### 14.2 Configuration Recommendations

| Use Case | Recommended Mode | flush_interval | max_pending |
|----------|------------------|----------------|-------------|
| Unit tests | InMemory | N/A | N/A |
| Development | InMemory | N/A | N/A |
| Agent hot path | Buffered | 100ms | 1000 |
| Production default | Buffered | 50ms | 500 |
| Checkpoints | Strict | N/A | N/A |
| Audit logs | Strict | N/A | N/A |

### 14.3 Success Criteria Checklist

**Gate 1: Durability Modes**
- [ ] InMemory mode: `engine/put_direct` < 3µs
- [ ] InMemory mode: 250K ops/sec (1-thread)
- [ ] Buffered mode: `kvstore/put` < 30µs
- [ ] Buffered mode: 50K ops/sec throughput
- [ ] Strict mode: Same behavior as M3

**Gate 2: Hot Path Optimization**
- [ ] Transaction pooling: Zero allocations in A1 hot path
- [ ] Snapshot acquisition: < 500ns, allocation-free
- [ ] Read optimization: `kvstore/get` < 10µs

**Gate 3: Scaling**
- [ ] Lock sharding: Disjoint scaling ≥ 1.8× at 2 threads
- [ ] Lock sharding: Disjoint scaling ≥ 3.2× at 4 threads
- [ ] 4-thread disjoint throughput: ≥ 800K ops/sec

**Gate 4: Facade Tax**
- [ ] A1/A0 < 10× (InMemory mode)
- [ ] B/A1 < 5×
- [ ] B/A0 < 30×

**Gate 5: Infrastructure**
- [ ] Baseline tagged: `m3_baseline_perf`
- [ ] Per-layer instrumentation working
- [ ] Backwards compatibility: M3 code unchanged

**Red Flag Check (must all pass)**
- [ ] Snapshot acquisition ≤ 2µs
- [ ] A1/A0 ≤ 20×
- [ ] B/A1 ≤ 8×
- [ ] Disjoint scaling (4 threads) ≥ 2.5×
- [ ] p99 ≤ 20× mean
- [ ] Zero hot-path allocations

---

## Conclusion

M4 is a **de-blocking milestone** that removes architectural barriers to Redis-class performance.

**M4 does not aim to be fast. M4 aims to be *fastable*.**

It does not achieve final performance parity—that requires M5+ work on data layout, cache behavior, and lock-free structures. Do not rationalize "good enough" at M4 completion.

**Key Deliverables**:
1. **Durability Modes** - User chooses performance vs durability tradeoff
2. **Transaction Pooling** - Zero-allocation hot path
3. **Lock Sharding** - Multi-thread scaling for disjoint runs
4. **Read Optimization** - Fast path for read-only operations
5. **Instrumentation** - Visibility for ongoing optimization
6. **Facade Tax Validation** - Prove abstractions are cheap

**Performance Transformation**:

| Metric | M3 | M4 (InMemory) | Improvement |
|--------|----|----|-------------|
| `kvstore/put` | 2.2 ms | <8 µs | **275×** |
| `kvstore/get` | 139 µs | <5 µs | **28×** |
| Throughput | 475 ops/s | 250K ops/s | **500×** |

**Next**: M5 adds deterministic replay and run lifecycle polish.

---

**Document Version**: 1.0
**Status**: Planning Phase
**Date**: 2026-01-15
