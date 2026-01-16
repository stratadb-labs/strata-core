# M4 Implementation Plan: Performance

## Response to M4 Architecture Specification

This document provides the complete implementation plan for M4 (Performance), building on the M4 Architecture Specification v1.1 and following the structure established in M2 and M3.

---

## Critical Invariants (NON-NEGOTIABLE)

Before implementing any M4 code, understand these invariants:

### 1. Atomicity Scope

> **Transactions are atomic within a single RunId. Cross-run writes are NOT guaranteed to be atomic.**

This is intentional:
- RunId is the primary isolation domain
- Cross-run atomicity would require global locking
- If cross-run atomicity is needed, caller must coordinate explicitly

**Implementation implication**: `apply()` may be observed mid-execution for cross-run write sets.

### 2. Snapshot Semantic Invariant

> **Fast-path reads must be observationally equivalent to a snapshot-based transaction.**

This means:
- No dirty reads (uncommitted data)
- No torn reads (partial write sets)
- No stale reads (older than snapshot version)
- No mixing versions across keys

**Why this matters**:
- Breaking this breaks agent reasoning guarantees
- Breaking this makes replay non-deterministic
- Breaking this prevents promoting fast-path to transaction later

**Implementation check**: Every optimization must preserve this invariant.

---

## Required Cargo Dependencies

Add to `crates/engine/Cargo.toml`:

```toml
[dependencies]
dashmap = "5"
rustc-hash = "1.1"  # For FxHashMap and FxBuildHasher
parking_lot = "0.12"  # For efficient Mutex/Condvar
```

**Note**: Use `rustc-hash` (not `fxhash`) - it's the more maintained crate.

---

## Key Design Decisions (From Architecture Review)

### 1. ✅ Three Durability Modes (ACCEPTED)

**Decision**: Implement InMemory, Buffered, and Strict durability modes.

**Our Approach**:
- `DurabilityMode` enum with three variants
- Mode selected at database open via builder pattern
- Per-operation override for critical writes
- Backwards compatible (default to Strict)

**Documented Implications**:
```rust
/// M4 Implementation: Durability Modes
///
/// DESIGN PRINCIPLE: Users choose their position on the durability spectrum.
/// All modes provide identical ACI guarantees; only D differs.
///
/// MODES:
/// - InMemory: No WAL, no fsync. <3µs latency. All data lost on crash.
/// - Buffered: WAL append, async fsync. <30µs latency. Bounded loss.
/// - Strict: WAL append + sync fsync. ~2ms latency. Zero loss.
///
/// ACID BY MODE:
/// | Property    | InMemory | Buffered | Strict |
/// |-------------|----------|----------|--------|
/// | Atomicity   |    ✓     |    ✓     |   ✓    |
/// | Consistency |    ✓     |    ✓     |   ✓    |
/// | Isolation   |    ✓     |    ✓     |   ✓    |
/// | Durability  |    ✗     | Bounded  |   ✓    |
pub enum DurabilityMode {
    InMemory,
    Buffered { flush_interval_ms: u64, max_pending_writes: usize },
    Strict,
}
```

---

### 2. ✅ DashMap + HashMap Storage (ACCEPTED)

**Decision**: Replace RwLock + BTreeMap with DashMap + HashMap.

**Our Approach**:
- DashMap for per-RunId sharding (lock-free reads)
- HashMap with FxHash for O(1) lookups within shards
- Trade ordered iteration for performance (list() requires sort)

**Documented Implications**:
```rust
/// M4 Implementation: Sharded Storage
///
/// DESIGN PRINCIPLE: Tactical improvement, not final architecture.
///
/// WHY THIS CHANGE:
/// - RwLock: Global contention, syscalls on contention
/// - BTreeMap: O(log n), pointer-heavy, cache-unfriendly
///
/// WHY DASHMAP + HASHMAP:
/// - DashMap: Lock-free reads, sharded writes (16-way default)
/// - HashMap: O(1) lookups, cache-friendly buckets
/// - FxHash: Fast non-crypto hash (fine for in-process)
///
/// TRADEOFF:
/// - list() now requires sort (not on hot path)
/// - get/put 10-100× more frequent than list
///
/// WARNING: Still provisional. Does not solve:
/// - Memory layout
/// - Cache line alignment
/// - Allocator behavior
/// These require M5+ work for Redis parity.
///
/// DEPENDENCY: rustc-hash crate (not fxhash)
use dashmap::DashMap;
use rustc_hash::{FxHashMap, FxBuildHasher};

pub struct ShardedStore {
    shards: DashMap<RunId, Shard, FxBuildHasher>,
}
```

---

### 3. ✅ Transaction Object Pooling (ACCEPTED)

**Decision**: Thread-local pool of reusable TransactionContext objects.

**Our Approach**:
- Thread-local `Vec<TransactionContext>` pool
- Pool on begin, return on end
- `reset()` method clears state but preserves capacity
- Max pool size of 8 per thread

**Documented Implications**:
```rust
/// M4 Implementation: Transaction Pooling
///
/// DESIGN PRINCIPLE: Zero allocations on hot path.
///
/// HOW IT WORKS:
/// 1. begin_transaction() pops from pool (or allocates if empty)
/// 2. Transaction is used normally
/// 3. end_transaction() pushes back to pool (or drops if full)
/// 4. reset() clears read_set/write_set without deallocating
///
/// MEMORY COST:
/// - ~1KB per context × 8 max = 8KB per thread
/// - Negligible compared to data size
///
/// EXPECTED IMPROVEMENT:
/// - core/put_hot matches core/put_hot_prealloc within 10%
/// - ~2× faster than M3 transaction creation
thread_local! {
    static TXN_POOL: RefCell<Vec<TransactionContext>> = RefCell::new(Vec::new());
}
```

---

### 4. ✅ Read Path Fast Path (ACCEPTED)

**Decision**: Bypass full transaction overhead for read-only operations.

**Our Approach**:
- Direct snapshot acquisition for reads
- No transaction object allocation
- No read-set recording, commit validation, or WAL append
- Preserve snapshot isolation and run isolation

**Documented Implications**:
```rust
/// M4 Implementation: Read Fast Path
///
/// DESIGN PRINCIPLE: Reads don't need write infrastructure.
///
/// WHAT WE SKIP:
/// - Transaction object allocation
/// - Read-set recording
/// - Write-set creation
/// - Commit validation
/// - WAL append
///
/// WHAT WE KEEP:
/// - Snapshot isolation (consistent view)
/// - Run isolation (key prefixing)
///
/// HARD INVARIANT:
/// All fast-path reads must be OBSERVATIONALLY EQUIVALENT to a
/// snapshot-based transaction. No dirty reads, no stale reads,
/// no torn reads. Any optimization that changes visibility,
/// ordering, or consistency is FORBIDDEN.
impl KVStore {
    pub fn get(&self, run_id: RunId, key: &str) -> Result<Option<Value>> {
        let snapshot = self.db.snapshot();  // Fast: Arc bump only
        let storage_key = Key::new_kv(run_id.namespace(), key);
        Ok(snapshot.get(&storage_key).map(|v| v.value.clone()))
    }
}
```

---

### 5. ✅ Lock Sharding by RunId (ACCEPTED)

**Decision**: Shard storage by RunId for multi-thread scaling.

**Our Approach**:
- DashMap keyed by RunId
- Each shard contains HashMap of keys
- Different runs never contend
- Cross-run queries use slower global path

**Documented Implications**:
```rust
/// M4 Implementation: Lock Sharding
///
/// DESIGN PRINCIPLE: Agents naturally partition by run.
///
/// WHY RUNID SHARDING:
/// 1. Natural agent partitioning
/// 2. Cross-run queries are rare (mostly RunIndex)
/// 3. No coordination needed between runs
/// 4. Enables future per-run WAL segments
///
/// SCALING TARGETS:
/// - 2 threads: ≥ 1.8× of 1-thread
/// - 4 threads: ≥ 3.2× of 1-thread
/// - 8 threads: ≥ 6.0× of 1-thread
```

---

### 6. ✅ Syscall-Free Hot Path (ACCEPTED)

**Decision**: Tier A0, A1, and B hot paths must not perform syscalls.

**Our Approach**:
- No `time()`, `rand()`, filesystem, logging on hot path
- No heap allocations (use pooling)
- No trait objects or dynamic dispatch
- Enforced by code review and benchmark validation

**Documented Contract**:
```rust
/// M4 Implementation: Syscall-Free Hot Path
///
/// HOT PATH MUST NOT:
/// - Perform syscalls (including time(), rand())
/// - Touch filesystem
/// - Perform logging
/// - Allocate heap memory
/// - Use trait objects or dynamic dispatch
/// - Trigger page faults
///
/// ENFORCEMENT:
/// - Code review
/// - Benchmark validation
/// - Red flag if violated
```

---

### 7. ✅ Red Flag Thresholds (ACCEPTED)

**Decision**: Define hard stop criteria that require architecture redesign.

**Our Approach**:
- Specific numeric thresholds for each metric
- If any red flag is hit, stop and redesign
- Non-negotiable—no exceptions

**Documented Contract**:
```rust
/// M4 Implementation: Red Flag Thresholds
///
/// IF ANY OF THESE ARE TRUE, STOP AND REDESIGN:
///
/// | Metric                    | Red Flag     | Action              |
/// |---------------------------|--------------|---------------------|
/// | Snapshot acquisition      | > 2µs        | Redesign snapshot   |
/// | A1/A0 ratio               | > 20×        | Remove abstractions |
/// | B/A1 ratio                | > 8×         | Inline facade logic |
/// | Disjoint scaling (4T)     | < 2.5×       | Redesign sharding   |
/// | p99 latency               | > 20× mean   | Fix tail latency    |
/// | Hot-path allocations      | > 0          | Eliminate allocs    |
```

---

## Revised Epic Structure

### Overview: 6 Epics, 28 Stories

| Epic | Name | Stories | Duration | Parallelization |
|------|------|---------|----------|-----------------|
| **Epic 20** | Performance Foundation | 4 | 1 day | Blocks all M4 |
| **Epic 21** | Durability Modes | 6 | 1.5 days | After Epic 20 |
| **Epic 22** | Sharded Storage | 5 | 1.5 days | After Epic 20 |
| **Epic 23** | Transaction Pooling | 4 | 1 day | After Epic 20 |
| **Epic 24** | Read Path Optimization | 4 | 1 day | After Epic 22 |
| **Epic 25** | Validation & Red Flags | 5 | 1.5 days | After all others |

**Total**: 6 epics, 28 stories, ~8-9 days with parallel execution

---

## Epic 20: Performance Foundation (4 stories, 1 day)

**Goal**: Core infrastructure for performance work

**Dependencies**: M3 complete

**Deliverables**:
- Performance baseline tag
- Benchmark infrastructure updates
- Feature flags for instrumentation
- DurabilityMode type definition

### Story #197: Tag M3 Baseline & Benchmark Infrastructure (3 hours) 🔴 FOUNDATION
**Blocks**: All M4 stories

**Files**:
- Git tag `m3_baseline_perf`
- `benches/m4_performance.rs`
- `Cargo.toml` (feature flags)

**Deliverable**: Tagged baseline and benchmark setup

**Implementation**:
```bash
# Tag the baseline
git tag -a m3_baseline_perf -m "M3 performance baseline for M4 comparison"
```

```toml
# Cargo.toml - root
[features]
default = []
perf-trace = []  # Enable per-layer timing instrumentation
```

```rust
// benches/m4_performance.rs
//! M4 Performance Benchmarks
//!
//! Compares M4 implementations against m3_baseline_perf tag.
//! Run with: cargo bench --bench m4_performance

use criterion::{criterion_group, criterion_main, Criterion, BenchmarkId};

fn durability_mode_benchmarks(c: &mut Criterion) {
    let mut group = c.benchmark_group("durability_modes");

    // InMemory mode
    group.bench_function("inmemory/put", |b| {
        // Setup InMemory database
        b.iter(|| {
            // Benchmark put operation
        });
    });

    // Buffered mode
    group.bench_function("buffered/put", |b| {
        // Setup Buffered database
        b.iter(|| {
            // Benchmark put operation
        });
    });

    // Strict mode (M3 baseline)
    group.bench_function("strict/put", |b| {
        // Setup Strict database
        b.iter(|| {
            // Benchmark put operation
        });
    });

    group.finish();
}

criterion_group!(durability, durability_mode_benchmarks);
criterion_main!(durability);
```

**Acceptance Criteria**:
- [ ] `m3_baseline_perf` tag exists
- [ ] `perf-trace` feature flag compiles
- [ ] `cargo bench --bench m4_performance` runs
- [ ] Baseline numbers recorded in benchmark output

---

### Story #198: DurabilityMode Type Definition (3 hours)
**File**: `crates/engine/src/durability/modes.rs`

**Deliverable**: DurabilityMode enum and associated types

**Implementation**:
```rust
//! Durability mode definitions for M4 performance optimization
//!
//! Three modes trading off latency vs durability:
//! - InMemory: Fastest, no persistence
//! - Buffered: Balanced, async fsync
//! - Strict: Safest, sync fsync (M3 default)

use std::time::Duration;

/// Durability mode for database operations
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DurabilityMode {
    /// No persistence. All data lost on crash.
    /// Fastest mode - no WAL, no fsync.
    ///
    /// Target latency: <3µs for engine/put_direct
    /// Use case: Caches, ephemeral data, tests
    InMemory,

    /// WAL append without immediate fsync.
    /// Periodic flush based on interval or batch size.
    ///
    /// Target latency: <30µs for kvstore/put
    /// Data loss window: max(flush_interval, pending_writes)
    /// Use case: Production default
    Buffered {
        /// Flush interval in milliseconds
        flush_interval_ms: u64,
        /// Maximum pending writes before flush
        max_pending_writes: usize,
    },

    /// fsync on every write.
    /// Zero data loss but slowest.
    ///
    /// Target latency: ~2ms for kvstore/put
    /// Use case: Checkpoints, metadata, audit logs
    Strict,
}

impl Default for DurabilityMode {
    fn default() -> Self {
        // Default to Strict for backwards compatibility with M3
        DurabilityMode::Strict
    }
}

impl DurabilityMode {
    /// Create Buffered mode with recommended production defaults
    pub fn buffered_default() -> Self {
        DurabilityMode::Buffered {
            flush_interval_ms: 100,
            max_pending_writes: 1000,
        }
    }

    /// Check if this mode requires WAL
    pub fn requires_wal(&self) -> bool {
        match self {
            DurabilityMode::InMemory => false,
            DurabilityMode::Buffered { .. } => true,
            DurabilityMode::Strict => true,
        }
    }

    /// Check if this mode requires immediate fsync
    pub fn requires_immediate_fsync(&self) -> bool {
        match self {
            DurabilityMode::InMemory => false,
            DurabilityMode::Buffered { .. } => false,
            DurabilityMode::Strict => true,
        }
    }

    /// Get flush interval for Buffered mode (None for others)
    pub fn flush_interval(&self) -> Option<Duration> {
        match self {
            DurabilityMode::Buffered { flush_interval_ms, .. } => {
                Some(Duration::from_millis(*flush_interval_ms))
            }
            _ => None,
        }
    }

    /// Get max pending writes for Buffered mode (None for others)
    pub fn max_pending_writes(&self) -> Option<usize> {
        match self {
            DurabilityMode::Buffered { max_pending_writes, .. } => Some(*max_pending_writes),
            _ => None,
        }
    }
}
```

**Tests**:
- [ ] Default is Strict
- [ ] buffered_default() returns sensible values
- [ ] requires_wal() correct for all modes
- [ ] requires_immediate_fsync() correct for all modes
- [ ] flush_interval() and max_pending_writes() correct

---

### Story #199: Performance Instrumentation Infrastructure (4 hours)
**File**: `crates/engine/src/instrumentation.rs`

**Deliverable**: Feature-gated per-layer timing infrastructure

**Implementation**:
```rust
//! Performance instrumentation for M4 optimization
//!
//! Feature-gated to avoid overhead in production.
//! Enable with: cargo build --features perf-trace

#[cfg(feature = "perf-trace")]
use std::time::Instant;

/// Per-operation performance trace
#[cfg(feature = "perf-trace")]
#[derive(Debug, Default, Clone)]
pub struct PerfTrace {
    /// Time to acquire snapshot
    pub snapshot_acquire_ns: u64,
    /// Time to validate read set
    pub read_set_validate_ns: u64,
    /// Time to apply write set
    pub write_set_apply_ns: u64,
    /// Time to append to WAL
    pub wal_append_ns: u64,
    /// Time to fsync
    pub fsync_ns: u64,
    /// Total commit time
    pub commit_total_ns: u64,
    /// Number of keys read
    pub keys_read: usize,
    /// Number of keys written
    pub keys_written: usize,
}

#[cfg(feature = "perf-trace")]
impl PerfTrace {
    /// Create new trace
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a timed section
    pub fn time<F, T>(f: F) -> (T, u64)
    where
        F: FnOnce() -> T,
    {
        let start = Instant::now();
        let result = f();
        let elapsed = start.elapsed().as_nanos() as u64;
        (result, elapsed)
    }

    /// Format as human-readable string
    pub fn summary(&self) -> String {
        format!(
            "snapshot: {}ns, validate: {}ns, apply: {}ns, wal: {}ns, fsync: {}ns, total: {}ns ({} reads, {} writes)",
            self.snapshot_acquire_ns,
            self.read_set_validate_ns,
            self.write_set_apply_ns,
            self.wal_append_ns,
            self.fsync_ns,
            self.commit_total_ns,
            self.keys_read,
            self.keys_written,
        )
    }
}

/// No-op trace for production builds
#[cfg(not(feature = "perf-trace"))]
#[derive(Debug, Default, Clone, Copy)]
pub struct PerfTrace;

#[cfg(not(feature = "perf-trace"))]
impl PerfTrace {
    pub fn new() -> Self { Self }
    pub fn summary(&self) -> &'static str { "perf-trace disabled" }
}

/// Macro for conditional timing
#[cfg(feature = "perf-trace")]
#[macro_export]
macro_rules! perf_time {
    ($trace:expr, $field:ident, $expr:expr) => {{
        let start = std::time::Instant::now();
        let result = $expr;
        $trace.$field = start.elapsed().as_nanos() as u64;
        result
    }};
}

#[cfg(not(feature = "perf-trace"))]
#[macro_export]
macro_rules! perf_time {
    ($trace:expr, $field:ident, $expr:expr) => {
        $expr
    };
}
```

**Tests**:
- [ ] PerfTrace compiles with feature enabled
- [ ] PerfTrace compiles with feature disabled
- [ ] perf_time! macro works with feature enabled
- [ ] perf_time! macro is no-op with feature disabled
- [ ] summary() returns readable string

---

### Story #200: Database Builder Pattern (4 hours)
**File**: `crates/engine/src/database.rs`

**Deliverable**: DatabaseBuilder for configuration including durability mode

**Implementation**:
```rust
use std::path::PathBuf;
use crate::durability::DurabilityMode;

/// Builder for Database configuration
pub struct DatabaseBuilder {
    path: Option<PathBuf>,
    durability: DurabilityMode,
    // Future: other config options
}

impl DatabaseBuilder {
    /// Create new builder with defaults
    pub fn new() -> Self {
        Self {
            path: None,
            durability: DurabilityMode::default(),  // Strict for backwards compat
        }
    }

    /// Set database path
    pub fn path<P: Into<PathBuf>>(mut self, path: P) -> Self {
        self.path = Some(path.into());
        self
    }

    /// Set durability mode
    pub fn durability(mut self, mode: DurabilityMode) -> Self {
        self.durability = mode;
        self
    }

    /// Open database with InMemory mode (convenience)
    pub fn in_memory(mut self) -> Self {
        self.durability = DurabilityMode::InMemory;
        self
    }

    /// Open database with Buffered mode (convenience)
    pub fn buffered(mut self) -> Self {
        self.durability = DurabilityMode::buffered_default();
        self
    }

    /// Open the database
    pub fn open(self) -> Result<Database> {
        let path = self.path.unwrap_or_else(|| {
            // Generate temp path for InMemory mode
            std::env::temp_dir().join(format!("inmem-{}", uuid::Uuid::new_v4()))
        });

        Database::open_with_mode(path, self.durability)
    }

    /// Open a temporary database (for tests)
    pub fn open_temp(self) -> Result<Database> {
        let path = std::env::temp_dir().join(format!("inmem-test-{}", uuid::Uuid::new_v4()));
        Database::open_with_mode(path, self.durability)
    }
}

impl Default for DatabaseBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl Database {
    /// Create a new database builder
    pub fn builder() -> DatabaseBuilder {
        DatabaseBuilder::new()
    }

    /// Get current durability mode
    pub fn durability_mode(&self) -> DurabilityMode {
        self.durability_mode
    }
}
```

**Tests**:
- [ ] DatabaseBuilder::new() creates default config
- [ ] .durability() sets mode
- [ ] .in_memory() sets InMemory mode
- [ ] .buffered() sets Buffered with defaults
- [ ] .open() creates database with configured mode
- [ ] Database::builder() returns builder
- [ ] Default mode is Strict (backwards compat)

---

## Epic 21: Durability Modes (6 stories, 1.5 days)

**Goal**: Implement three durability modes

**Dependencies**: Epic 20 complete

**Deliverables**:
- InMemory durability (no WAL)
- Buffered durability (async fsync)
- Strict durability (sync fsync, M3 behavior)
- Per-operation durability override
- Background flush thread for Buffered mode

### Story #201: Durability Trait Abstraction (3 hours) 🔴 FOUNDATION
**File**: `crates/engine/src/durability/mod.rs`

**Deliverable**: Durability trait for mode implementations

**Implementation**:
```rust
//! Durability abstraction for M4 modes
//!
//! Each mode implements this trait differently.

use crate::WriteSet;
use in_mem_core::Result;

/// Durability behavior abstraction
pub trait Durability: Send + Sync {
    /// Commit a write set with this durability level
    fn commit(&self, write_set: &WriteSet) -> Result<()>;

    /// Graceful shutdown - flush any pending data
    fn shutdown(&self) -> Result<()>;

    /// Check if this durability mode persists data
    fn is_persistent(&self) -> bool;
}

pub mod modes;
mod inmemory;
mod buffered;
mod strict;

pub use modes::DurabilityMode;
pub use inmemory::InMemoryDurability;
pub use buffered::BufferedDurability;
pub use strict::StrictDurability;
```

**Tests**:
- [ ] Durability trait compiles
- [ ] All three implementations implement trait

---

### Story #202: InMemory Durability Implementation (3 hours)
**File**: `crates/engine/src/durability/inmemory.rs`

**Deliverable**: InMemory durability - no WAL, no fsync

**Implementation**:
```rust
//! InMemory durability mode
//!
//! No WAL, no fsync. All data lost on crash.
//! Fastest mode - target <3µs for engine/put_direct.

use std::sync::Arc;
use super::Durability;
use crate::storage::ShardedStore;
use in_mem_core::Result;

/// InMemory durability - no persistence
pub struct InMemoryDurability {
    storage: Arc<ShardedStore>,
}

impl InMemoryDurability {
    /// Create new InMemory durability
    pub fn new(storage: Arc<ShardedStore>) -> Self {
        Self { storage }
    }
}

impl Durability for InMemoryDurability {
    fn commit(&self, write_set: &WriteSet) -> Result<()> {
        // Hot path - no WAL, no fsync, just apply
        //
        // CRITICAL: This must be syscall-free!
        // - No logging
        // - No time() calls
        // - No allocations (write_set already allocated)
        self.storage.apply(write_set)
    }

    fn shutdown(&self) -> Result<()> {
        // Nothing to flush - data is ephemeral
        Ok(())
    }

    fn is_persistent(&self) -> bool {
        false
    }
}
```

**Tests**:
- [ ] commit() applies write set to storage
- [ ] commit() does not create WAL entries
- [ ] shutdown() succeeds (no-op)
- [ ] is_persistent() returns false
- [ ] Benchmark: commit() < 3µs (excluding storage apply)

---

### Story #203: Strict Durability Implementation (3 hours)
**File**: `crates/engine/src/durability/strict.rs`

**Deliverable**: Strict durability - WAL + immediate fsync (M3 behavior)

**Implementation**:
```rust
//! Strict durability mode
//!
//! WAL append + immediate fsync. Zero data loss.
//! Slowest mode - ~2ms for kvstore/put due to fsync.
//! This is the M3 default behavior.

use std::sync::Arc;
use super::Durability;
use crate::storage::ShardedStore;
use crate::wal::WriteAheadLog;
use in_mem_core::Result;

/// Strict durability - fsync on every write
pub struct StrictDurability {
    storage: Arc<ShardedStore>,
    wal: Arc<WriteAheadLog>,
}

impl StrictDurability {
    /// Create new Strict durability
    pub fn new(storage: Arc<ShardedStore>, wal: Arc<WriteAheadLog>) -> Self {
        Self { storage, wal }
    }
}

impl Durability for StrictDurability {
    fn commit(&self, write_set: &WriteSet) -> Result<()> {
        // 1. Append to WAL
        self.wal.append(write_set)?;

        // 2. fsync immediately - this is the slow part (~2ms)
        self.wal.fsync()?;

        // 3. Apply to storage
        self.storage.apply(write_set)
    }

    fn shutdown(&self) -> Result<()> {
        // Already synced - nothing to do
        self.wal.fsync()
    }

    fn is_persistent(&self) -> bool {
        true
    }
}
```

**Tests**:
- [ ] commit() appends to WAL
- [ ] commit() calls fsync
- [ ] commit() applies to storage
- [ ] Order: WAL → fsync → storage apply
- [ ] shutdown() fsyncs
- [ ] is_persistent() returns true
- [ ] Recovery: data survives crash

---

### Story #204: Buffered Durability Implementation (5 hours)
**File**: `crates/engine/src/durability/buffered.rs`

**Deliverable**: Buffered durability - WAL append with async fsync

**Implementation**:
```rust
//! Buffered durability mode
//!
//! WAL append without immediate fsync.
//! Periodic flush based on interval or batch size.
//! Balanced mode - target <30µs for kvstore/put.

use std::sync::{Arc, atomic::{AtomicUsize, AtomicU64, Ordering}};
use std::time::{Duration, Instant};
use parking_lot::Mutex;
use super::Durability;
use crate::storage::ShardedStore;
use crate::wal::WriteAheadLog;
use in_mem_core::Result;

/// Buffered durability - async fsync
pub struct BufferedDurability {
    storage: Arc<ShardedStore>,
    wal: Arc<WriteAheadLog>,

    // Flush configuration
    flush_interval: Duration,
    max_pending_writes: usize,

    // State tracking
    pending_writes: AtomicUsize,
    last_flush: Mutex<Instant>,

    // Flush signaling
    flush_signal: Arc<std::sync::Condvar>,
    flush_mutex: Arc<Mutex<bool>>,

    // Shutdown handling (CRITICAL for clean teardown)
    shutdown: AtomicBool,
    flush_thread: Mutex<Option<std::thread::JoinHandle<()>>>,
}

impl BufferedDurability {
    /// Create new Buffered durability
    pub fn new(
        storage: Arc<ShardedStore>,
        wal: Arc<WriteAheadLog>,
        flush_interval_ms: u64,
        max_pending_writes: usize,
    ) -> Self {
        Self {
            storage,
            wal,
            flush_interval: Duration::from_millis(flush_interval_ms),
            max_pending_writes,
            pending_writes: AtomicUsize::new(0),
            last_flush: Mutex::new(Instant::now()),
            flush_signal: Arc::new(std::sync::Condvar::new()),
            flush_mutex: Arc::new(Mutex::new(false)),
        }
    }

    /// Check if flush is needed
    fn should_flush(&self) -> bool {
        let pending = self.pending_writes.load(Ordering::Relaxed);
        if pending >= self.max_pending_writes {
            return true;
        }

        let last = self.last_flush.lock();
        last.elapsed() >= self.flush_interval
    }

    /// Trigger async flush
    fn flush_async(&self) {
        let mut flush = self.flush_mutex.lock();
        *flush = true;
        self.flush_signal.notify_one();
    }

    /// Synchronous flush (for shutdown)
    pub fn flush_sync(&self) -> Result<()> {
        self.wal.fsync()?;
        self.pending_writes.store(0, Ordering::Relaxed);
        *self.last_flush.lock() = Instant::now();
        Ok(())
    }

    /// Start background flush thread
    pub fn start_flush_thread(self: &Arc<Self>) {
        let durability = Arc::clone(self);
        let handle = std::thread::spawn(move || {
            loop {
                // Wait for signal or timeout
                let mut flush = durability.flush_mutex.lock();
                let _ = durability.flush_signal.wait_timeout(
                    &mut flush,
                    durability.flush_interval,
                );

                // Check if shutdown requested
                if durability.shutdown.load(Ordering::Acquire) {
                    // Final flush before exit
                    let _ = durability.flush_sync();
                    return;
                }

                // Perform flush
                if let Err(e) = durability.flush_sync() {
                    eprintln!("Buffered flush error: {}", e);
                }
            }
        });

        // Store thread handle for join on shutdown
        *self.flush_thread.lock() = Some(handle);
    }
}

impl Drop for BufferedDurability {
    fn drop(&mut self) {
        // Signal shutdown
        self.shutdown.store(true, Ordering::Release);
        self.flush_signal.notify_all();

        // Wait for thread to finish
        if let Some(handle) = self.flush_thread.lock().take() {
            let _ = handle.join();
        }
    }
}

impl Durability for BufferedDurability {
    fn commit(&self, write_set: &WriteSet) -> Result<()> {
        // 1. Append to WAL buffer (no fsync - fast!)
        self.wal.append(write_set)?;

        // 2. Apply to storage
        self.storage.apply(write_set)?;

        // 3. Track pending writes
        self.pending_writes.fetch_add(1, Ordering::Relaxed);

        // 4. Check if flush needed
        if self.should_flush() {
            self.flush_async();
        }

        Ok(())
    }

    fn shutdown(&self) -> Result<()> {
        // Synchronously flush all pending writes
        self.flush_sync()
    }

    fn is_persistent(&self) -> bool {
        true  // Eventually persistent (after flush)
    }
}
```

**Tests**:
- [ ] commit() appends to WAL without fsync
- [ ] commit() applies to storage
- [ ] should_flush() triggers on max_pending_writes
- [ ] should_flush() triggers on interval
- [ ] flush_async() signals background thread
- [ ] flush_sync() forces immediate fsync
- [ ] shutdown() flushes all pending
- [ ] Benchmark: commit() < 30µs (excluding storage)

---

### Story #205: Per-Operation Durability Override (3 hours)
**File**: `crates/engine/src/database.rs`

**Deliverable**: transaction_with_durability() for critical writes

**Implementation**:
```rust
impl Database {
    /// Execute transaction with durability override
    ///
    /// Use this for critical writes in non-strict mode.
    /// Example: Force fsync for metadata even in Buffered mode.
    pub fn transaction_with_durability<F, T>(
        &self,
        run_id: RunId,
        durability: DurabilityMode,
        f: F,
    ) -> Result<T>
    where
        F: FnOnce(&mut TransactionContext) -> Result<T>,
    {
        // Create transaction context
        let mut txn = self.begin_transaction(run_id)?;

        // Execute closure
        let result = f(&mut txn)?;

        // Commit with specified durability
        self.commit_with_durability(&mut txn, durability)?;

        Ok(result)
    }

    /// Internal: commit with specific durability mode
    fn commit_with_durability(
        &self,
        txn: &mut TransactionContext,
        durability: DurabilityMode,
    ) -> Result<()> {
        // Validate transaction
        self.validate_transaction(txn)?;

        // Get durability implementation for this mode
        let durability_impl = self.get_durability_for_mode(durability);

        // Commit with specified durability
        durability_impl.commit(&txn.write_set)
    }

    /// Get or create durability implementation for mode
    fn get_durability_for_mode(&self, mode: DurabilityMode) -> Arc<dyn Durability> {
        match mode {
            DurabilityMode::InMemory => {
                Arc::new(InMemoryDurability::new(Arc::clone(&self.storage)))
            }
            DurabilityMode::Buffered { .. } => {
                // Use existing buffered or default
                self.buffered_durability.clone().unwrap_or_else(|| {
                    Arc::new(BufferedDurability::new(
                        Arc::clone(&self.storage),
                        Arc::clone(&self.wal),
                        mode.flush_interval().map(|d| d.as_millis() as u64).unwrap_or(100),
                        mode.max_pending_writes().unwrap_or(1000),
                    ))
                })
            }
            DurabilityMode::Strict => {
                Arc::new(StrictDurability::new(
                    Arc::clone(&self.storage),
                    Arc::clone(&self.wal),
                ))
            }
        }
    }
}
```

**Usage Example**:
```rust
// Normal operation in Buffered mode
db.transaction(run_id, |txn| {
    txn.put(key, value)?;
    Ok(())
})?;

// Critical metadata - force Strict even in Buffered mode
db.transaction_with_durability(run_id, DurabilityMode::Strict, |txn| {
    txn.put(critical_metadata_key, value)?;
    Ok(())
})?;
```

**Tests**:
- [ ] Override to Strict in Buffered database causes fsync
- [ ] Override to InMemory in Strict database skips fsync
- [ ] Override does not affect default mode
- [ ] Override works for any mode combination

---

### Story #206: Graceful Shutdown (3 hours)
**File**: `crates/engine/src/database.rs`

**Deliverable**: Database::shutdown() ensures data safety

**Implementation**:
```rust
impl Database {
    /// Graceful shutdown - ensures all data is persisted
    ///
    /// Behavior by mode:
    /// - InMemory: No-op (data is ephemeral)
    /// - Buffered: Flush pending writes
    /// - Strict: No-op (already synced)
    pub fn shutdown(&self) -> Result<()> {
        // Stop accepting new transactions
        self.accepting_transactions.store(false, Ordering::SeqCst);

        // Wait for in-flight transactions to complete
        // (implementation depends on transaction tracking)

        // Flush based on mode
        self.durability.shutdown()
    }

    /// Check if database is accepting transactions
    pub fn is_open(&self) -> bool {
        self.accepting_transactions.load(Ordering::SeqCst)
    }
}

impl Drop for Database {
    fn drop(&mut self) {
        // Best-effort shutdown on drop
        if let Err(e) = self.shutdown() {
            eprintln!("Warning: Error during database shutdown: {}", e);
        }
    }
}
```

**Tests**:
- [ ] shutdown() in InMemory mode succeeds immediately
- [ ] shutdown() in Buffered mode flushes pending
- [ ] shutdown() in Strict mode succeeds immediately
- [ ] Drop triggers shutdown
- [ ] Transactions fail after shutdown

---

## Epic 22: Sharded Storage (5 stories, 1.5 days)

**Goal**: Replace RwLock + BTreeMap with DashMap + HashMap

**Dependencies**: Epic 20 complete

**Deliverables**:
- ShardedStore implementation
- Per-RunId sharding
- FxHash for fast hashing
- Migration from UnifiedStore

### Story #207: ShardedStore Structure (4 hours) 🔴 FOUNDATION
**File**: `crates/engine/src/storage/sharded.rs`

**Deliverable**: ShardedStore with DashMap + HashMap

**Implementation**:
```rust
//! Sharded storage for M4 performance
//!
//! Replaces RwLock + BTreeMap with DashMap + HashMap.
//! Lock-free reads, sharded writes, O(1) lookups.

use dashmap::DashMap;
use rustc_hash::FxHashMap;
use fxhash::FxBuildHasher;
use std::sync::Arc;
use in_mem_core::{RunId, Key, VersionedValue, Result};

/// Per-run shard containing run's data
struct Shard {
    /// HashMap with FxHash for O(1) lookups
    data: FxHashMap<Key, VersionedValue>,
}

impl Shard {
    fn new() -> Self {
        Self {
            data: FxHashMap::default(),
        }
    }
}

/// Sharded storage - DashMap by RunId, HashMap within
///
/// DESIGN: Lock-free reads, sharded writes.
///
/// WHY THIS STRUCTURE:
/// - DashMap: 16-way sharded by default, lock-free reads
/// - FxHashMap: O(1) lookups, fast non-crypto hash
/// - Per-RunId: Natural agent partitioning, no cross-run contention
///
/// WARNING: Provisional for M4. Does not solve memory layout,
/// cache alignment, or allocator behavior. M5+ required for Redis parity.
pub struct ShardedStore {
    /// Per-run shards using DashMap
    shards: DashMap<RunId, Shard, FxBuildHasher>,

    /// Global version for snapshots
    version: std::sync::atomic::AtomicU64,
}

impl ShardedStore {
    /// Create new sharded store
    pub fn new() -> Self {
        Self {
            shards: DashMap::with_hasher(FxBuildHasher::default()),
            version: std::sync::atomic::AtomicU64::new(0),
        }
    }

    /// Get current version
    pub fn version(&self) -> u64 {
        self.version.load(std::sync::atomic::Ordering::Acquire)
    }

    /// Increment version and return new value
    pub fn next_version(&self) -> u64 {
        self.version.fetch_add(1, std::sync::atomic::Ordering::AcqRel) + 1
    }
}

impl Default for ShardedStore {
    fn default() -> Self {
        Self::new()
    }
}
```

**Tests**:
- [ ] ShardedStore::new() creates empty store
- [ ] version() returns current version
- [ ] next_version() increments atomically
- [ ] Multiple threads can call next_version() safely

---

### Story #208: ShardedStore Get/Put Operations (4 hours)
**File**: `crates/engine/src/storage/sharded.rs`

**Deliverable**: get() and put() with per-RunId sharding

**Implementation**:
```rust
impl ShardedStore {
    /// Get a value by run_id and key
    ///
    /// Lock-free read via DashMap.
    pub fn get(&self, run_id: &RunId, key: &Key) -> Option<VersionedValue> {
        // DashMap.get() is lock-free for reads
        self.shards
            .get(run_id)
            .and_then(|shard| shard.data.get(key).cloned())
    }

    /// Put a value for run_id and key
    ///
    /// Sharded write - only locks this run's shard.
    pub fn put(&self, run_id: &RunId, key: Key, value: VersionedValue) {
        self.shards
            .entry(*run_id)
            .or_insert_with(Shard::new)
            .data
            .insert(key, value);
    }

    /// Delete a key
    pub fn delete(&self, run_id: &RunId, key: &Key) -> Option<VersionedValue> {
        self.shards
            .get_mut(run_id)
            .and_then(|mut shard| shard.data.remove(key))
    }

    /// Check if key exists
    pub fn contains(&self, run_id: &RunId, key: &Key) -> bool {
        self.shards
            .get(run_id)
            .map(|shard| shard.data.contains_key(key))
            .unwrap_or(false)
    }

    /// Apply a write set atomically
    ///
    /// All writes in the set are applied together.
    pub fn apply(&self, write_set: &WriteSet) -> Result<()> {
        for (key, value) in write_set.writes() {
            let run_id = key.run_id();
            match value {
                Some(v) => self.put(&run_id, key.clone(), v.clone()),
                None => { self.delete(&run_id, key); }
            }
        }
        Ok(())
    }
}
```

**Tests**:
- [ ] get() returns None for missing key
- [ ] put() stores value
- [ ] get() returns stored value
- [ ] delete() removes key
- [ ] contains() returns correct result
- [ ] Different runs don't interfere
- [ ] apply() processes entire write set

---

### Story #209: ShardedStore List Operations (3 hours)
**File**: `crates/engine/src/storage/sharded.rs`

**Deliverable**: list() with prefix filtering (requires sort)

**Implementation**:
```rust
impl ShardedStore {
    /// List keys with prefix filter
    ///
    /// NOTE: Slower than BTreeMap range scan. Requires filter + sort.
    /// This is acceptable because list() is NOT on hot path.
    pub fn list(&self, run_id: &RunId, prefix: &[u8]) -> Vec<(Key, VersionedValue)> {
        self.shards
            .get(run_id)
            .map(|shard| {
                let mut results: Vec<_> = shard.data
                    .iter()
                    .filter(|(k, _)| k.as_bytes().starts_with(prefix))
                    .map(|(k, v)| (k.clone(), v.clone()))
                    .collect();

                // Sort for consistent ordering (BTreeMap gave this for free)
                results.sort_by(|(a, _), (b, _)| a.cmp(b));
                results
            })
            .unwrap_or_default()
    }

    /// List all keys for a run
    pub fn list_all(&self, run_id: &RunId) -> Vec<(Key, VersionedValue)> {
        self.list(run_id, &[])
    }

    /// Count keys for a run
    pub fn count(&self, run_id: &RunId) -> usize {
        self.shards
            .get(run_id)
            .map(|shard| shard.data.len())
            .unwrap_or(0)
    }

    /// Count all keys across all runs
    pub fn total_count(&self) -> usize {
        self.shards
            .iter()
            .map(|entry| entry.data.len())
            .sum()
    }
}
```

**Tests**:
- [ ] list() returns empty for missing run
- [ ] list() filters by prefix correctly
- [ ] list() returns sorted results
- [ ] list_all() returns all keys for run
- [ ] count() returns correct count
- [ ] total_count() sums all runs

---

### Story #210: Snapshot Acquisition (Fast Path) (4 hours)
**File**: `crates/engine/src/storage/sharded.rs`

**Deliverable**: Allocation-free snapshot acquisition

**Implementation**:
```rust
use std::sync::Arc;

/// Snapshot of storage at a point in time
///
/// CRITICAL: Snapshot acquisition must be:
/// - Allocation-free (Arc bump only)
/// - Lock-free (atomic version load)
/// - O(1) (no data structure scan)
pub struct Snapshot {
    /// Version at snapshot time
    version: u64,
    /// Reference to storage (Arc bump, not clone)
    store: Arc<ShardedStore>,
}

impl Snapshot {
    /// Get a value from snapshot
    pub fn get(&self, run_id: &RunId, key: &Key) -> Option<VersionedValue> {
        self.store.get(run_id, key)
    }

    /// Get snapshot version
    pub fn version(&self) -> u64 {
        self.version
    }
}

impl ShardedStore {
    /// Create a snapshot
    ///
    /// FAST PATH: This must be < 500ns!
    /// - Atomic version load: ~10ns
    /// - Arc clone: ~20ns
    /// - Total: ~30ns
    ///
    /// NO allocations, NO locks, NO scans.
    pub fn snapshot(self: &Arc<Self>) -> Snapshot {
        Snapshot {
            version: self.version.load(std::sync::atomic::Ordering::Acquire),
            store: Arc::clone(self),
        }
    }
}
```

**Tests**:
- [ ] snapshot() returns consistent version
- [ ] Snapshot::get() returns value from store
- [ ] snapshot() is allocation-free (verify with allocator tracking)
- [ ] Benchmark: snapshot() < 500ns

---

### Story #211: Storage Migration Path (3 hours)
**File**: `crates/engine/src/storage/mod.rs`

**Deliverable**: Migration from UnifiedStore to ShardedStore

**Implementation**:
```rust
//! Storage layer for M4
//!
//! M3 used UnifiedStore (RwLock + BTreeMap).
//! M4 uses ShardedStore (DashMap + HashMap).
//!
//! This module provides the migration path.

mod unified;   // M3 implementation (kept for reference)
mod sharded;   // M4 implementation

pub use sharded::{ShardedStore, Snapshot};

// Re-export UnifiedStore for backwards compatibility during migration
#[deprecated(since = "0.4.0", note = "Use ShardedStore instead")]
pub use unified::UnifiedStore;

/// Storage trait for abstracting implementations
pub trait Storage: Send + Sync {
    fn get(&self, run_id: &RunId, key: &Key) -> Option<VersionedValue>;
    fn put(&self, run_id: &RunId, key: Key, value: VersionedValue);
    fn delete(&self, run_id: &RunId, key: &Key) -> Option<VersionedValue>;
    fn list(&self, run_id: &RunId, prefix: &[u8]) -> Vec<(Key, VersionedValue)>;
}

impl Storage for ShardedStore {
    fn get(&self, run_id: &RunId, key: &Key) -> Option<VersionedValue> {
        ShardedStore::get(self, run_id, key)
    }

    fn put(&self, run_id: &RunId, key: Key, value: VersionedValue) {
        ShardedStore::put(self, run_id, key, value)
    }

    fn delete(&self, run_id: &RunId, key: &Key) -> Option<VersionedValue> {
        ShardedStore::delete(self, run_id, key)
    }

    fn list(&self, run_id: &RunId, prefix: &[u8]) -> Vec<(Key, VersionedValue)> {
        ShardedStore::list(self, run_id, prefix)
    }
}
```

**Tests**:
- [ ] ShardedStore implements Storage trait
- [ ] UnifiedStore deprecated warning appears
- [ ] Migration does not break existing tests

---

## Epic 23: Transaction Pooling (4 stories, 1 day)

**Goal**: Eliminate allocation overhead on hot path

**Dependencies**: Epic 20 complete

**Deliverables**:
- Thread-local transaction pool
- TransactionContext reset method
- Pooled begin/end transaction
- Zero-allocation verification

### Story #212: TransactionContext Reset Method (3 hours)
**File**: `crates/engine/src/transaction/context.rs`

**Deliverable**: reset() method for context reuse

**Implementation**:
```rust
impl TransactionContext {
    /// Reset context for reuse
    ///
    /// Clears state without deallocating.
    /// HashMap::clear() preserves capacity.
    pub fn reset(&mut self, run_id: RunId, snapshot: Snapshot, version: u64) {
        self.run_id = run_id;
        self.snapshot = snapshot;
        self.version = version;

        // Clear but keep capacity
        self.read_set.clear();
        self.write_set.clear();
    }

    /// Get current capacity (for debugging)
    pub fn capacity(&self) -> (usize, usize) {
        (self.read_set.capacity(), self.write_set.capacity())
    }
}
```

**Tests**:
- [ ] reset() clears read_set
- [ ] reset() clears write_set
- [ ] reset() preserves capacity
- [ ] reset() updates run_id, snapshot, version

---

### Story #213: Thread-Local Transaction Pool (4 hours)
**File**: `crates/engine/src/transaction/pool.rs`

**Deliverable**: Thread-local pool for transaction contexts

**Implementation**:
```rust
//! Thread-local transaction pool for M4
//!
//! Eliminates allocation overhead by reusing TransactionContext objects.

use std::cell::RefCell;
use super::TransactionContext;
use crate::storage::Snapshot;
use in_mem_core::RunId;

/// Maximum contexts per thread
const MAX_POOL_SIZE: usize = 8;

thread_local! {
    /// Thread-local pool of reusable contexts
    static TXN_POOL: RefCell<Vec<TransactionContext>> = RefCell::new(Vec::with_capacity(MAX_POOL_SIZE));
}

/// Transaction pool operations
pub struct TransactionPool;

impl TransactionPool {
    /// Acquire a transaction context
    ///
    /// Returns pooled context if available, allocates if pool empty.
    pub fn acquire(run_id: RunId, snapshot: Snapshot, version: u64) -> TransactionContext {
        TXN_POOL.with(|pool| {
            match pool.borrow_mut().pop() {
                Some(mut ctx) => {
                    // Reuse existing allocation
                    ctx.reset(run_id, snapshot, version);
                    ctx
                }
                None => {
                    // Pool empty - allocate new
                    TransactionContext::new(run_id, snapshot, version)
                }
            }
        })
    }

    /// Return a transaction context to the pool
    ///
    /// Context is returned if pool has room, dropped otherwise.
    pub fn release(ctx: TransactionContext) {
        TXN_POOL.with(|pool| {
            let mut pool = pool.borrow_mut();
            if pool.len() < MAX_POOL_SIZE {
                pool.push(ctx);
            }
            // else: drop (pool full)
        });
    }

    /// Get current pool size (for debugging)
    pub fn pool_size() -> usize {
        TXN_POOL.with(|pool| pool.borrow().len())
    }

    /// Clear the pool (for testing)
    #[cfg(test)]
    pub fn clear() {
        TXN_POOL.with(|pool| pool.borrow_mut().clear());
    }
}
```

**Tests**:
- [ ] acquire() returns context
- [ ] release() returns context to pool
- [ ] acquire() reuses pooled context
- [ ] Pool caps at MAX_POOL_SIZE
- [ ] Pool is thread-local (different threads have different pools)
- [ ] pool_size() returns correct count

---

### Story #214: Pooled Transaction API (3 hours)
**File**: `crates/engine/src/database.rs`

**Deliverable**: Database methods using transaction pool

**Implementation**:
```rust
use crate::transaction::pool::TransactionPool;

impl Database {
    /// Begin a transaction (pooled)
    ///
    /// Uses thread-local pool to avoid allocation.
    pub fn begin_transaction(&self, run_id: RunId) -> Result<TransactionContext> {
        if !self.is_open() {
            return Err(Error::DatabaseClosed);
        }

        let snapshot = self.storage.snapshot();
        let version = self.storage.next_version();

        Ok(TransactionPool::acquire(run_id, snapshot, version))
    }

    /// End a transaction (returns to pool)
    ///
    /// Call after commit or abort to return context to pool.
    pub fn end_transaction(&self, ctx: TransactionContext) {
        TransactionPool::release(ctx);
    }

    /// Execute a transaction with automatic pooling
    pub fn transaction<F, T>(&self, run_id: RunId, f: F) -> Result<T>
    where
        F: FnOnce(&mut TransactionContext) -> Result<T>,
    {
        let mut ctx = self.begin_transaction(run_id)?;

        let result = f(&mut ctx);

        match result {
            Ok(value) => {
                self.commit_transaction(&mut ctx)?;
                self.end_transaction(ctx);
                Ok(value)
            }
            Err(e) => {
                // Abort - still return to pool
                self.end_transaction(ctx);
                Err(e)
            }
        }
    }
}
```

**Tests**:
- [ ] transaction() uses pooled context
- [ ] Context returned to pool on success
- [ ] Context returned to pool on error
- [ ] Repeated transactions reuse contexts

---

### Story #215: Zero-Allocation Verification (3 hours)
**File**: `benches/m4_performance.rs`

**Deliverable**: Benchmark verifying zero allocations on hot path

**Implementation**:
```rust
//! Allocation tracking benchmarks
//!
//! Verifies that hot path has zero allocations.

use criterion::{criterion_group, criterion_main, Criterion};
use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering};

/// Allocation-counting allocator
struct CountingAllocator {
    alloc_count: AtomicUsize,
}

static COUNTER: CountingAllocator = CountingAllocator {
    alloc_count: AtomicUsize::new(0),
};

unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        self.alloc_count.fetch_add(1, Ordering::Relaxed);
        System.alloc(layout)
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        System.dealloc(ptr, layout)
    }
}

fn zero_allocation_benchmarks(c: &mut Criterion) {
    let mut group = c.benchmark_group("zero_alloc");

    // Setup: warm up pool with one transaction
    let db = Database::builder().in_memory().open_temp().unwrap();
    let run_id = RunId::new();
    db.transaction(run_id, |txn| {
        txn.put(Key::new_kv(run_id.namespace(), "warmup"), Value::I64(0))?;
        Ok(())
    }).unwrap();

    group.bench_function("hot_path_put", |b| {
        b.iter(|| {
            // Reset counter
            COUNTER.alloc_count.store(0, Ordering::Relaxed);

            // Hot path operation
            db.transaction(run_id, |txn| {
                txn.put(Key::new_kv(run_id.namespace(), "key"), Value::I64(42))?;
                Ok(())
            }).unwrap();

            // Verify zero allocations
            let allocs = COUNTER.alloc_count.load(Ordering::Relaxed);
            assert_eq!(allocs, 0, "Hot path should have zero allocations");
        });
    });

    group.finish();
}

criterion_group!(zero_alloc, zero_allocation_benchmarks);
```

**Tests**:
- [ ] Hot path put has zero allocations (after warmup)
- [ ] Hot path get has zero allocations
- [ ] Pooled transaction has zero allocations

---

## Epic 24: Read Path Optimization (4 stories, 1 day)

**Goal**: Bypass transaction overhead for reads

**Dependencies**: Epic 22 complete (ShardedStore with snapshot)

**Deliverables**:
- Fast path get() for KVStore
- Fast path batch get_many()
- Snapshot-based reads
- Observational equivalence verification

### Story #216: KVStore Fast Path Get (3 hours)
**File**: `crates/primitives/src/kv.rs`

**Deliverable**: Optimized get() bypassing full transaction

**Implementation**:
```rust
impl KVStore {
    /// Get a value by key (FAST PATH)
    ///
    /// Bypasses full transaction overhead:
    /// - No transaction object allocation
    /// - No read-set recording
    /// - No commit validation
    /// - No WAL append
    ///
    /// PRESERVES:
    /// - Snapshot isolation (consistent view)
    /// - Run isolation (key prefixing)
    ///
    /// INVARIANT: Observationally equivalent to transaction-based read.
    pub fn get(&self, run_id: RunId, key: &str) -> Result<Option<Value>> {
        // Fast path: direct snapshot read
        let snapshot = self.db.snapshot();
        let storage_key = Key::new_kv(run_id.namespace(), key);

        Ok(snapshot.get(&run_id, &storage_key).map(|v| v.value.clone()))
    }

    /// Get with full transaction (for comparison/fallback)
    #[doc(hidden)]
    pub fn get_with_transaction(&self, run_id: RunId, key: &str) -> Result<Option<Value>> {
        self.db.transaction(run_id, |txn| {
            let storage_key = Key::new_kv(run_id.namespace(), key);
            txn.get(&storage_key)
        })
    }
}
```

**Tests**:
- [ ] get() returns correct value
- [ ] get() returns None for missing key
- [ ] get() is observationally equivalent to get_with_transaction()
- [ ] Benchmark: get() < 10µs (target: <5µs)

---

### Story #217: KVStore Fast Path Batch Get (3 hours)
**File**: `crates/primitives/src/kv.rs`

**Deliverable**: get_many() for efficient batch reads

**Implementation**:
```rust
impl KVStore {
    /// Get multiple values in a single snapshot (FAST PATH)
    ///
    /// Single snapshot acquisition for all keys.
    /// More efficient than multiple get() calls.
    pub fn get_many(&self, run_id: RunId, keys: &[&str]) -> Result<Vec<Option<Value>>> {
        // Single snapshot for consistency
        let snapshot = self.db.snapshot();
        let namespace = run_id.namespace();

        keys.iter()
            .map(|key| {
                let storage_key = Key::new_kv(namespace.clone(), key);
                Ok(snapshot.get(&run_id, &storage_key).map(|v| v.value.clone()))
            })
            .collect()
    }

    /// Check if key exists (FAST PATH)
    pub fn contains(&self, run_id: RunId, key: &str) -> Result<bool> {
        let snapshot = self.db.snapshot();
        let storage_key = Key::new_kv(run_id.namespace(), key);
        Ok(snapshot.get(&run_id, &storage_key).is_some())
    }
}
```

**Tests**:
- [ ] get_many() returns correct values
- [ ] get_many() uses single snapshot
- [ ] contains() returns correct result
- [ ] Benchmark: get_many(10) < 50µs

---

### Story #218: Other Primitive Fast Paths (4 hours)
**Files**:
- `crates/primitives/src/event_log.rs`
- `crates/primitives/src/state_cell.rs`
- `crates/primitives/src/trace.rs`

**Deliverable**: Fast path reads for EventLog, StateCell, TraceStore

**Implementation**:
```rust
// event_log.rs
impl EventLog {
    /// Read event by sequence (FAST PATH)
    pub fn read(&self, run_id: RunId, sequence: u64) -> Result<Option<Event>> {
        let snapshot = self.db.snapshot();
        let ns = Namespace::for_run(run_id);
        let event_key = Key::new_event(ns, sequence);

        match snapshot.get(&run_id, &event_key) {
            Some(v) => Ok(Some(serde_json::from_value(v.value.into_json()?)?)),
            None => Ok(None),
        }
    }

    /// Get log length (FAST PATH)
    pub fn len(&self, run_id: RunId) -> Result<u64> {
        let snapshot = self.db.snapshot();
        let ns = Namespace::for_run(run_id);
        let meta_key = Key::new_event_meta(ns);

        match snapshot.get(&run_id, &meta_key) {
            Some(v) => {
                let meta: EventLogMeta = serde_json::from_value(v.value.into_json()?)?;
                Ok(meta.next_sequence)
            }
            None => Ok(0),
        }
    }
}

// state_cell.rs
impl StateCell {
    /// Read state (FAST PATH)
    pub fn read(&self, run_id: RunId, name: &str) -> Result<Option<State>> {
        let snapshot = self.db.snapshot();
        let ns = Namespace::for_run(run_id);
        let state_key = Key::new_state(ns, name);

        match snapshot.get(&run_id, &state_key) {
            Some(v) => Ok(Some(serde_json::from_value(v.value.into_json()?)?)),
            None => Ok(None),
        }
    }
}

// trace.rs
impl TraceStore {
    /// Get trace by ID (FAST PATH)
    pub fn get(&self, run_id: RunId, trace_id: &str) -> Result<Option<Trace>> {
        let snapshot = self.db.snapshot();
        let ns = Namespace::for_run(run_id);
        let trace_key = Key::new_trace(ns, trace_id);

        match snapshot.get(&run_id, &trace_key) {
            Some(v) => Ok(Some(serde_json::from_value(v.value.into_json()?)?)),
            None => Ok(None),
        }
    }
}
```

**Tests**:
- [ ] EventLog::read() fast path works
- [ ] EventLog::len() fast path works
- [ ] StateCell::read() fast path works
- [ ] TraceStore::get() fast path works
- [ ] All fast paths are observationally equivalent

---

### Story #219: Observational Equivalence Tests (3 hours)
**File**: `tests/m4_fast_path_equivalence.rs`

**Deliverable**: Tests proving fast path = transaction path

**Implementation**:
```rust
//! Tests verifying fast path reads are observationally equivalent
//! to transaction-based reads.

use in_mem_primitives::{KVStore, EventLog, StateCell, TraceStore};
use in_mem_core::{Value, RunId};

#[test]
fn kv_fast_path_equivalent() {
    let db = Database::builder().in_memory().open_temp().unwrap();
    let kv = KVStore::new(db.clone());
    let run_id = RunId::new();

    // Write some data
    kv.put(run_id, "key1", Value::I64(42)).unwrap();
    kv.put(run_id, "key2", Value::String("hello".into())).unwrap();

    // Fast path reads
    let fast1 = kv.get(run_id, "key1").unwrap();
    let fast2 = kv.get(run_id, "key2").unwrap();
    let fast_missing = kv.get(run_id, "missing").unwrap();

    // Transaction reads
    let txn1 = kv.get_with_transaction(run_id, "key1").unwrap();
    let txn2 = kv.get_with_transaction(run_id, "key2").unwrap();
    let txn_missing = kv.get_with_transaction(run_id, "missing").unwrap();

    // Must be identical
    assert_eq!(fast1, txn1);
    assert_eq!(fast2, txn2);
    assert_eq!(fast_missing, txn_missing);
}

#[test]
fn kv_fast_path_snapshot_consistency() {
    // Test that batch reads see consistent view
    let db = Database::builder().in_memory().open_temp().unwrap();
    let kv = KVStore::new(db.clone());
    let run_id = RunId::new();

    // Write initial data
    kv.put(run_id, "a", Value::I64(1)).unwrap();
    kv.put(run_id, "b", Value::I64(2)).unwrap();

    // Concurrent modification (in another thread)
    let kv2 = kv.clone();
    let run_id2 = run_id;
    let handle = std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(10));
        kv2.put(run_id2, "a", Value::I64(100)).unwrap();
        kv2.put(run_id2, "b", Value::I64(200)).unwrap();
    });

    // Batch read should see consistent view (either all old or all new)
    let results = kv.get_many(run_id, &["a", "b"]).unwrap();

    // Either both old or both new (snapshot consistency)
    let a = results[0].as_ref().unwrap().as_i64().unwrap();
    let b = results[1].as_ref().unwrap().as_i64().unwrap();

    assert!(
        (a == 1 && b == 2) || (a == 100 && b == 200),
        "Snapshot should be consistent: a={}, b={}",
        a, b
    );

    handle.join().unwrap();
}
```

**Tests**:
- [ ] KV fast path = transaction path
- [ ] EventLog fast path = transaction path
- [ ] StateCell fast path = transaction path
- [ ] TraceStore fast path = transaction path
- [ ] Batch reads see consistent snapshot

---

## Epic 25: Validation & Red Flags (5 stories, 1.5 days)

**Goal**: Verify M4 meets targets and check red flags

**Dependencies**: All other epics complete

**Deliverables**:
- Full benchmark suite for M4
- Red flag validation
- Facade tax measurement
- Contention scaling verification
- Success criteria checklist

### Story #220: M4 Benchmark Suite (4 hours)
**File**: `benches/m4_performance.rs`

**Deliverable**: Comprehensive benchmarks for all M4 targets

**Implementation**:
```rust
//! M4 Performance Benchmark Suite
//!
//! Run with: cargo bench --bench m4_performance
//! Compare to baseline: compare against m3_baseline_perf tag

use criterion::{criterion_group, criterion_main, Criterion, BenchmarkId};
use std::time::Duration;

fn latency_benchmarks(c: &mut Criterion) {
    let mut group = c.benchmark_group("latency");
    group.measurement_time(Duration::from_secs(10));

    // InMemory mode benchmarks
    for mode in ["inmemory", "buffered", "strict"] {
        let db = match mode {
            "inmemory" => Database::builder().in_memory().open_temp().unwrap(),
            "buffered" => Database::builder().buffered().open_temp().unwrap(),
            "strict" => Database::builder().open_temp().unwrap(),
            _ => unreachable!(),
        };

        let kv = KVStore::new(db.clone());
        let run_id = RunId::new();

        group.bench_function(BenchmarkId::new("kvstore/put", mode), |b| {
            b.iter(|| {
                kv.put(run_id, "key", Value::I64(42)).unwrap();
            });
        });

        group.bench_function(BenchmarkId::new("kvstore/get", mode), |b| {
            kv.put(run_id, "key", Value::I64(42)).unwrap();
            b.iter(|| {
                kv.get(run_id, "key").unwrap();
            });
        });

        group.bench_function(BenchmarkId::new("engine/put_direct", mode), |b| {
            b.iter(|| {
                db.transaction(run_id, |txn| {
                    txn.put(Key::new_kv(run_id.namespace(), "key"), Value::I64(42))?;
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

    let db = Database::builder().in_memory().open_temp().unwrap();
    let kv = KVStore::new(db.clone());
    let run_id = RunId::new();

    group.bench_function("inmemory/1thread", |b| {
        b.iter(|| {
            for i in 0..1000 {
                kv.put(run_id, &format!("key{}", i), Value::I64(i)).unwrap();
            }
        });
    });

    group.finish();
}

fn contention_benchmarks(c: &mut Criterion) {
    let mut group = c.benchmark_group("contention");
    group.measurement_time(Duration::from_secs(15));

    // Test scaling with disjoint keys
    for threads in [1, 2, 4, 8] {
        group.bench_function(BenchmarkId::new("disjoint", threads), |b| {
            let db = Database::builder().in_memory().open_temp().unwrap();

            b.iter(|| {
                let handles: Vec<_> = (0..threads)
                    .map(|t| {
                        let db = db.clone();
                        std::thread::spawn(move || {
                            let kv = KVStore::new(db);
                            let run_id = RunId::new(); // Different run per thread
                            for i in 0..1000 {
                                kv.put(run_id, &format!("key{}", i), Value::I64(i)).unwrap();
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

fn snapshot_benchmarks(c: &mut Criterion) {
    let mut group = c.benchmark_group("snapshot");

    let db = Database::builder().in_memory().open_temp().unwrap();

    group.bench_function("acquire", |b| {
        b.iter(|| {
            let _snapshot = db.snapshot();
        });
    });

    group.finish();
}

criterion_group!(
    name = m4_benchmarks;
    config = Criterion::default().sample_size(100);
    targets = latency_benchmarks, throughput_benchmarks, contention_benchmarks, snapshot_benchmarks
);

criterion_main!(m4_benchmarks);
```

**Tests**:
- [ ] All benchmarks run successfully
- [ ] Results output includes comparison to baseline

---

### Story #221: Red Flag Validation (4 hours)
**File**: `tests/m4_red_flags.rs`

**Deliverable**: Tests that fail if red flag thresholds exceeded

**Implementation**:
```rust
//! M4 Red Flag Validation Tests
//!
//! These tests FAIL if architecture has fundamental problems.
//! A failure means STOP and REDESIGN - not tune parameters.

use std::time::{Instant, Duration};

/// Red flag: Snapshot acquisition > 2µs
#[test]
fn red_flag_snapshot_acquisition() {
    let db = Database::builder().in_memory().open_temp().unwrap();

    // Warm up
    for _ in 0..100 {
        let _ = db.snapshot();
    }

    // Measure
    let iterations = 10000;
    let start = Instant::now();
    for _ in 0..iterations {
        let _ = db.snapshot();
    }
    let elapsed = start.elapsed();

    let avg_ns = elapsed.as_nanos() / iterations as u128;
    let threshold_ns = 2000; // 2µs

    assert!(
        avg_ns <= threshold_ns,
        "RED FLAG: Snapshot acquisition {}ns > {}ns threshold. REDESIGN REQUIRED.",
        avg_ns, threshold_ns
    );
}

/// Red flag: Disjoint scaling < 2.5× at 4 threads
#[test]
fn red_flag_disjoint_scaling() {
    let db = Database::builder().in_memory().open_temp().unwrap();
    let iterations = 10000;

    // Single-threaded baseline
    let start = Instant::now();
    let run_id = RunId::new();
    let kv = KVStore::new(db.clone());
    for i in 0..iterations {
        kv.put(run_id, &format!("key{}", i), Value::I64(i)).unwrap();
    }
    let single_thread_time = start.elapsed();

    // 4-thread disjoint
    let start = Instant::now();
    let handles: Vec<_> = (0..4)
        .map(|_| {
            let db = db.clone();
            std::thread::spawn(move || {
                let kv = KVStore::new(db);
                let run_id = RunId::new(); // Different run
                for i in 0..iterations {
                    kv.put(run_id, &format!("key{}", i), Value::I64(i)).unwrap();
                }
            })
        })
        .collect();
    for h in handles {
        h.join().unwrap();
    }
    let four_thread_time = start.elapsed();

    // 4× work should take < 4× time (ideally ~1× with perfect scaling)
    // We require at least 2.5× speedup
    let scaling = (single_thread_time.as_nanos() * 4) as f64 / four_thread_time.as_nanos() as f64;
    let threshold = 2.5;

    assert!(
        scaling >= threshold,
        "RED FLAG: Disjoint scaling {:.2}× < {:.1}× threshold. REDESIGN SHARDING.",
        scaling, threshold
    );
}

/// Red flag: p99 > 20× mean
#[test]
fn red_flag_tail_latency() {
    let db = Database::builder().in_memory().open_temp().unwrap();
    let kv = KVStore::new(db.clone());
    let run_id = RunId::new();

    // Collect latencies
    let mut latencies: Vec<u128> = Vec::with_capacity(1000);
    for i in 0..1000 {
        let start = Instant::now();
        kv.put(run_id, &format!("key{}", i), Value::I64(i)).unwrap();
        latencies.push(start.elapsed().as_nanos());
    }

    latencies.sort();
    let mean = latencies.iter().sum::<u128>() / latencies.len() as u128;
    let p99 = latencies[989]; // 99th percentile

    let ratio = p99 as f64 / mean as f64;
    let threshold = 20.0;

    assert!(
        ratio <= threshold,
        "RED FLAG: p99/mean = {:.1}× > {:.0}× threshold. FIX TAIL LATENCY.",
        ratio, threshold
    );
}

/// Red flag: Hot path has allocations
#[test]
fn red_flag_hot_path_allocations() {
    // This test requires custom allocator tracking
    // Simplified version: check pool is being used

    let db = Database::builder().in_memory().open_temp().unwrap();
    let kv = KVStore::new(db.clone());
    let run_id = RunId::new();

    // Warm up pool
    kv.put(run_id, "warmup", Value::I64(0)).unwrap();

    // Check pool has contexts
    let pool_size_before = TransactionPool::pool_size();
    assert!(pool_size_before > 0, "Pool should have contexts after warmup");

    // Do operation
    kv.put(run_id, "key", Value::I64(42)).unwrap();

    // Pool should still have contexts (reused, not allocated)
    let pool_size_after = TransactionPool::pool_size();
    assert_eq!(
        pool_size_before, pool_size_after,
        "RED FLAG: Pool size changed - transactions not being reused"
    );
}
```

**Tests**:
- [ ] Snapshot < 2µs
- [ ] Disjoint scaling ≥ 2.5×
- [ ] p99 ≤ 20× mean
- [ ] Zero hot path allocations

---

### Story #222: Facade Tax Measurement (3 hours)
**File**: `benches/m4_facade_tax.rs`

**Deliverable**: Benchmark measuring A0, A1, B layer costs

**Implementation**:
```rust
//! Facade Tax Benchmarks
//!
//! Measures overhead at each layer:
//! - A0: Core data structure (HashMap)
//! - A1: Engine layer (Database.put_direct)
//! - B:  Facade layer (KVStore.put)
//!
//! Targets:
//! - A1/A0 < 10× (InMemory mode)
//! - B/A1 < 5×
//! - B/A0 < 30×

use criterion::{criterion_group, criterion_main, Criterion};

fn facade_tax_benchmarks(c: &mut Criterion) {
    let mut group = c.benchmark_group("facade_tax");

    // A0: Raw HashMap
    let mut map = rustc_hash::FxHashMap::default();
    group.bench_function("A0/hashmap_insert", |b| {
        b.iter(|| {
            map.insert("key".to_string(), 42i64);
        });
    });

    // A1: Engine layer (InMemory)
    let db = Database::builder().in_memory().open_temp().unwrap();
    let run_id = RunId::new();
    group.bench_function("A1/engine_put_direct", |b| {
        b.iter(|| {
            db.transaction(run_id, |txn| {
                txn.put(Key::new_kv(run_id.namespace(), "key"), Value::I64(42))?;
                Ok(())
            }).unwrap();
        });
    });

    // B: Facade layer
    let kv = KVStore::new(db.clone());
    group.bench_function("B/kvstore_put", |b| {
        b.iter(|| {
            kv.put(run_id, "key", Value::I64(42)).unwrap();
        });
    });

    group.finish();
}

fn calculate_ratios() {
    // After benchmarks, calculate ratios
    // This would be done by a post-processing script
    println!("
    Facade Tax Analysis:
    ====================
    A0 (HashMap):     XXX ns
    A1 (Engine):      XXX ns  (A1/A0 = X.X×, target < 10×)
    B  (KVStore):     XXX ns  (B/A1 = X.X×, target < 5×)

    B/A0 = X.X× (target < 30×)
    ");
}

criterion_group!(facade_tax, facade_tax_benchmarks);
criterion_main!(facade_tax);
```

**Tests**:
- [ ] A1/A0 < 10× (InMemory)
- [ ] B/A1 < 5×
- [ ] B/A0 < 30×

---

### Story #223: Contention Scaling Verification (3 hours)
**File**: `benches/m4_contention.rs`

**Deliverable**: Benchmarks verifying multi-thread scaling targets

**Implementation**:
```rust
//! Contention Scaling Benchmarks
//!
//! Verifies disjoint key scaling targets:
//! - 2 threads: ≥ 1.8× of 1-thread
//! - 4 threads: ≥ 3.2× of 1-thread
//! - 8 threads: ≥ 6.0× of 1-thread

use criterion::{criterion_group, criterion_main, Criterion, BenchmarkId};
use std::sync::Arc;

fn scaling_benchmarks(c: &mut Criterion) {
    let mut group = c.benchmark_group("scaling");
    group.measurement_time(Duration::from_secs(20));

    let iterations = 10000;

    for threads in [1, 2, 4, 8] {
        group.bench_function(BenchmarkId::new("disjoint_runs", threads), |b| {
            let db = Arc::new(Database::builder().in_memory().open_temp().unwrap());

            b.iter(|| {
                let handles: Vec<_> = (0..threads)
                    .map(|_| {
                        let db = Arc::clone(&db);
                        std::thread::spawn(move || {
                            let kv = KVStore::new((*db).clone());
                            let run_id = RunId::new();
                            for i in 0..iterations / threads {
                                kv.put(run_id, &format!("k{}", i), Value::I64(i as i64)).unwrap();
                            }
                        })
                    })
                    .collect();

                for h in handles {
                    h.join().unwrap();
                }
            });
        });

        group.bench_function(BenchmarkId::new("same_run", threads), |b| {
            let db = Arc::new(Database::builder().in_memory().open_temp().unwrap());
            let shared_run_id = RunId::new();

            b.iter(|| {
                let handles: Vec<_> = (0..threads)
                    .map(|t| {
                        let db = Arc::clone(&db);
                        let run_id = shared_run_id;
                        std::thread::spawn(move || {
                            let kv = KVStore::new((*db).clone());
                            for i in 0..iterations / threads {
                                kv.put(run_id, &format!("t{}k{}", t, i), Value::I64(i as i64)).unwrap();
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

criterion_group!(scaling, scaling_benchmarks);
criterion_main!(scaling);
```

**Tests**:
- [ ] 2 threads ≥ 1.8× scaling
- [ ] 4 threads ≥ 3.2× scaling
- [ ] 8 threads ≥ 6.0× scaling

---

### Story #224: Success Criteria Checklist (3 hours)
**File**: `docs/milestones/M4_COMPLETION_CHECKLIST.md`

**Deliverable**: Comprehensive checklist for M4 sign-off

**Implementation**:
```markdown
# M4 Completion Checklist

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

## Sign-off
- [ ] All gates pass
- [ ] No red flags triggered
- [ ] Code reviewed
- [ ] CI passes

Date: ___________
Signed: ___________
```

**Tests**:
- [ ] Checklist covers all success criteria
- [ ] No items missing from architecture doc

---

## Parallelization Strategy

```
Week 1:
├── Epic 20 (Foundation) ─────────────────► [Day 1]
│   └── Stories #197-200
│
├─┬─ Epic 21 (Durability Modes) ──────────► [Day 2-3]
│ │  └── Stories #201-206
│ │
│ ├─ Epic 22 (Sharded Storage) ───────────► [Day 2-3]
│ │  └── Stories #207-211
│ │
│ └─ Epic 23 (Transaction Pooling) ───────► [Day 2]
│    └── Stories #212-215
│
├── Epic 24 (Read Path Optimization) ─────► [Day 4]
│   └── Stories #216-219
│   └── Depends on: Epic 22
│
└── Epic 25 (Validation & Red Flags) ─────► [Day 5]
    └── Stories #220-224
    └── Depends on: All others
```

**Critical Path**: Epic 20 → Epic 22 → Epic 24 → Epic 25

**Parallelizable**: Epic 21, 22, 23 can run in parallel after Epic 20

---

## Risk Mitigation

### High Risk
1. **DashMap contention under write load**
   - Mitigation: Benchmark early, fall back to sharded locks if needed
   - Red flag threshold: Disjoint scaling < 2.5×

2. **Snapshot acquisition too slow**
   - Mitigation: Keep Arc::clone path, avoid any allocation
   - Red flag threshold: > 2µs

### Medium Risk
1. **Buffered mode complexity**
   - Mitigation: Start simple (just interval-based), add batch trigger later
   - Fallback: Ship with InMemory + Strict only

2. **Transaction pool thread safety**
   - Mitigation: Thread-local pools (no cross-thread sharing)

### Low Risk
1. **Backwards compatibility**
   - Mitigation: Default to Strict mode
   - M3 tests run unchanged

---

## Success Metrics

| Metric | Target | Red Flag |
|--------|--------|----------|
| `kvstore/put` (InMemory) | < 8µs | > 20µs |
| `kvstore/get` | < 5µs | > 10µs |
| Throughput (1-thread) | 250K ops/sec | < 100K ops/sec |
| Throughput (4-thread disjoint) | 800K ops/sec | < 400K ops/sec |
| Snapshot acquisition | < 500ns | > 2µs |
| A1/A0 | < 10× | > 20× |
| B/A1 | < 5× | > 8× |

---

## Document History

| Version | Date | Changes |
|---------|------|---------|
| 1.0 | 2026-01-15 | Initial M4 implementation plan |
