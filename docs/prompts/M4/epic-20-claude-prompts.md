# Epic 20: Performance Foundation - Implementation Prompts

**Epic Goal**: Core infrastructure for M4 performance work

**GitHub Issue**: [#211](https://github.com/anibjoshi/in-mem/issues/211)
**Status**: Ready to begin
**Dependencies**: M3 complete

---

## AUTHORITATIVE SPECIFICATIONS - READ THESE FIRST

**`docs/architecture/M4_ARCHITECTURE.md` is the GOSPEL for ALL M4 implementation.**

Before starting ANY story in this epic:
```bash
cat docs/architecture/M4_ARCHITECTURE.md
cat docs/milestones/M4_IMPLEMENTATION_PLAN.md
```

See `docs/prompts/M4_PROMPT_HEADER.md` for complete guidelines.

---

## Epic 20 Overview

### Reference: Critical Invariants

See `docs/prompts/M4_PROMPT_HEADER.md` for critical invariants that apply to ALL M4 epics:
1. Atomicity Scope (per-RunId only)
2. Snapshot Semantic Invariant (observational equivalence)
3. Thread Lifecycle (Buffered mode)
4. Required Dependencies (rustc-hash, not fxhash)

### Scope
- Tag M3 performance baseline
- Benchmark infrastructure for M4
- Feature flags for instrumentation
- DurabilityMode type definition
- Database builder pattern

### Success Criteria
- [ ] `m3_baseline_perf` git tag created
- [ ] `perf-trace` feature flag working
- [ ] `cargo bench --bench m4_performance` runs
- [ ] DurabilityMode enum defined with all three variants
- [ ] DatabaseBuilder pattern implemented
- [ ] Baseline benchmark results recorded

### Component Breakdown
- **Story #197 (GitHub #217)**: Tag M3 Baseline & Benchmark Infrastructure - BLOCKS ALL M4
- **Story #198 (GitHub #218)**: DurabilityMode Type Definition
- **Story #199 (GitHub #219)**: Performance Instrumentation Infrastructure
- **Story #200 (GitHub #220)**: Database Builder Pattern

---

## Dependency Graph

```
Phase 1 (Sequential - CRITICAL):
  Story #217 (Baseline Tag)
    └─> BLOCKS #218, #219, #220, AND ALL OTHER M4 STORIES

Phase 2 (Parallel - 3 Claudes after #217):
  Story #218 (DurabilityMode)
  Story #219 (Instrumentation)
  Story #220 (Builder Pattern)
    └─> All depend on #217
    └─> Independent of each other
```

---

## Parallelization Strategy

### Optimal Execution (3 Claudes)

| Phase | Duration | Claude 1 | Claude 2 | Claude 3 |
|-------|----------|----------|----------|----------|
| 1 | 3 hours | #217 Baseline | - | - |
| 2 | 3-4 hours | #218 DurabilityMode | #219 Instrumentation | #220 Builder |

**Total Wall Time**: ~7 hours (vs. ~14 hours sequential)

---

## Story #217: Tag M3 Baseline & Benchmark Infrastructure

**GitHub Issue**: [#217](https://github.com/anibjoshi/in-mem/issues/217)
**Estimated Time**: 3 hours
**Dependencies**: M3 complete
**Blocks**: ALL other M4 stories

### PREREQUISITE: Read the Architecture Spec

Before writing ANY code, read these sections of `docs/architecture/M4_ARCHITECTURE.md`:
- Section 1: Philosophy
- Section 2: Key Design Decisions
- Section 7: Red Flag Thresholds

### Start Story

```bash
gh issue view 217
./scripts/start-story.sh 20 217 baseline-benchmark
```

### Implementation Steps

#### Step 1: Tag the M3 baseline

```bash
# Ensure on develop branch with M3 complete
git checkout develop
git pull origin develop

# Create annotated tag
git tag -a m3_baseline_perf -m "M3 performance baseline for M4 comparison

M3 complete with:
- 5 primitives (KV, EventLog, StateCell, TraceStore, RunIndex)
- OCC transactions with snapshot isolation
- WAL + fsync durability

This tag marks the performance baseline before M4 optimizations."

# Push tag
git push origin m3_baseline_perf
```

#### Step 2: Add feature flags to Cargo.toml

Add to workspace `Cargo.toml`:

```toml
[features]
default = []
perf-trace = []  # Enable per-layer timing instrumentation
```

#### Step 3: Create benchmark infrastructure

Create `benches/m4_performance.rs`:

```rust
//! M4 Performance Benchmarks
//!
//! Run with: cargo bench --bench m4_performance
//! Compare to baseline: checkout m3_baseline_perf tag

use criterion::{criterion_group, criterion_main, Criterion, BenchmarkId};
use std::time::Duration;

fn placeholder_benchmarks(c: &mut Criterion) {
    let mut group = c.benchmark_group("m4_placeholder");
    group.measurement_time(Duration::from_secs(5));

    // Placeholder - actual benchmarks added as features implemented
    group.bench_function("noop", |b| {
        b.iter(|| {
            // Will be replaced with actual benchmarks
            std::hint::black_box(42)
        });
    });

    group.finish();
}

fn durability_mode_benchmarks(c: &mut Criterion) {
    let mut group = c.benchmark_group("durability_modes");

    // Placeholder for durability mode benchmarks
    // Filled in by Epic 21

    group.bench_function("placeholder", |b| {
        b.iter(|| std::hint::black_box(0))
    });

    group.finish();
}

fn storage_benchmarks(c: &mut Criterion) {
    let mut group = c.benchmark_group("storage");

    // Placeholder for sharded storage benchmarks
    // Filled in by Epic 22

    group.bench_function("placeholder", |b| {
        b.iter(|| std::hint::black_box(0))
    });

    group.finish();
}

fn snapshot_benchmarks(c: &mut Criterion) {
    let mut group = c.benchmark_group("snapshot");

    // Placeholder for snapshot benchmarks
    // Critical: < 500ns target, < 2µs red flag

    group.bench_function("placeholder", |b| {
        b.iter(|| std::hint::black_box(0))
    });

    group.finish();
}

criterion_group!(
    name = m4_benchmarks;
    config = Criterion::default().sample_size(100);
    targets = placeholder_benchmarks, durability_mode_benchmarks, storage_benchmarks, snapshot_benchmarks
);

criterion_main!(m4_benchmarks);
```

#### Step 4: Update workspace Cargo.toml for benchmarks

Add to workspace `Cargo.toml`:

```toml
[[bench]]
name = "m4_performance"
harness = false

[dev-dependencies]
criterion = { version = "0.5", features = ["html_reports"] }
```

#### Step 5: Record baseline benchmark results

```bash
# Run baseline benchmarks on M3
~/.cargo/bin/cargo bench --bench m4_performance

# Save results
mkdir -p docs/benchmarks
cp -r target/criterion docs/benchmarks/m3_baseline_$(date +%Y%m%d)

# Create baseline record
cat > docs/benchmarks/M3_BASELINE.md << 'EOF'
# M3 Performance Baseline

**Date**: $(date +%Y-%m-%d)
**Tag**: m3_baseline_perf
**Hardware**: [Document your hardware]

## Benchmark Results

| Operation | Mean | Std Dev |
|-----------|------|---------|
| [To be filled after running] | | |

## Notes

This baseline will be used to measure M4 improvements.
EOF
```

### Validation

```bash
# Verify tag exists
git tag -l m3_baseline_perf

# Verify feature flag compiles
~/.cargo/bin/cargo build --features perf-trace

# Run benchmarks
~/.cargo/bin/cargo bench --bench m4_performance

# Check all builds
~/.cargo/bin/cargo build --workspace
~/.cargo/bin/cargo clippy --workspace -- -D warnings
~/.cargo/bin/cargo fmt --check
```

### Complete Story

```bash
./scripts/complete-story.sh 217
```

---

## Story #218: DurabilityMode Type Definition

**GitHub Issue**: [#218](https://github.com/anibjoshi/in-mem/issues/218)
**Estimated Time**: 3 hours
**Dependencies**: Story #217

### Start Story

```bash
gh issue view 218
./scripts/start-story.sh 20 218 durability-mode
```

### Implementation Steps

#### Step 1: Create durability module structure

```bash
mkdir -p crates/engine/src/durability
```

#### Step 2: Create modes.rs

Create `crates/engine/src/durability/modes.rs`:

```rust
//! Durability mode definitions for M4 performance optimization
//!
//! Three modes trading off latency vs durability:
//! - InMemory: Fastest, no persistence
//! - Buffered: Balanced, async fsync
//! - Strict: Safest, sync fsync (M3 default)

use std::time::Duration;
use serde::{Deserialize, Serialize};

/// Durability mode for database operations
///
/// # Modes
///
/// ## InMemory
/// No persistence. All data lost on crash.
/// Fastest mode - no WAL, no fsync.
/// Target latency: <3µs for engine/put_direct
/// Use case: Caches, ephemeral data, tests
///
/// ## Buffered
/// WAL append without immediate fsync.
/// Periodic flush based on interval or batch size.
/// Target latency: <30µs for kvstore/put
/// Data loss window: max(flush_interval, pending_writes)
/// Use case: Production default
///
/// ## Strict
/// fsync on every write.
/// Zero data loss but slowest.
/// Target latency: ~2ms for kvstore/put
/// Use case: Checkpoints, metadata, audit logs
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DurabilityMode {
    /// No persistence. All data lost on crash.
    InMemory,

    /// WAL append without immediate fsync.
    Buffered {
        /// Flush interval in milliseconds
        flush_interval_ms: u64,
        /// Maximum pending writes before flush
        max_pending_writes: usize,
    },

    /// fsync on every write. Zero data loss.
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

    /// Human-readable description
    pub fn description(&self) -> &'static str {
        match self {
            DurabilityMode::InMemory => "No persistence (fastest)",
            DurabilityMode::Buffered { .. } => "Async fsync (balanced)",
            DurabilityMode::Strict => "Sync fsync (safest)",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_is_strict() {
        assert_eq!(DurabilityMode::default(), DurabilityMode::Strict);
    }

    #[test]
    fn test_buffered_default() {
        let mode = DurabilityMode::buffered_default();
        match mode {
            DurabilityMode::Buffered { flush_interval_ms, max_pending_writes } => {
                assert_eq!(flush_interval_ms, 100);
                assert_eq!(max_pending_writes, 1000);
            }
            _ => panic!("Expected Buffered mode"),
        }
    }

    #[test]
    fn test_requires_wal() {
        assert!(!DurabilityMode::InMemory.requires_wal());
        assert!(DurabilityMode::buffered_default().requires_wal());
        assert!(DurabilityMode::Strict.requires_wal());
    }

    #[test]
    fn test_requires_immediate_fsync() {
        assert!(!DurabilityMode::InMemory.requires_immediate_fsync());
        assert!(!DurabilityMode::buffered_default().requires_immediate_fsync());
        assert!(DurabilityMode::Strict.requires_immediate_fsync());
    }

    #[test]
    fn test_flush_interval() {
        assert_eq!(DurabilityMode::InMemory.flush_interval(), None);
        assert_eq!(
            DurabilityMode::buffered_default().flush_interval(),
            Some(Duration::from_millis(100))
        );
        assert_eq!(DurabilityMode::Strict.flush_interval(), None);
    }

    #[test]
    fn test_max_pending_writes() {
        assert_eq!(DurabilityMode::InMemory.max_pending_writes(), None);
        assert_eq!(DurabilityMode::buffered_default().max_pending_writes(), Some(1000));
        assert_eq!(DurabilityMode::Strict.max_pending_writes(), None);
    }

    #[test]
    fn test_description() {
        assert!(!DurabilityMode::InMemory.description().is_empty());
        assert!(!DurabilityMode::buffered_default().description().is_empty());
        assert!(!DurabilityMode::Strict.description().is_empty());
    }

    #[test]
    fn test_serialization() {
        let modes = vec![
            DurabilityMode::InMemory,
            DurabilityMode::buffered_default(),
            DurabilityMode::Strict,
        ];

        for mode in modes {
            let json = serde_json::to_string(&mode).unwrap();
            let deserialized: DurabilityMode = serde_json::from_str(&json).unwrap();
            assert_eq!(mode, deserialized);
        }
    }
}
```

#### Step 3: Create durability mod.rs

Create `crates/engine/src/durability/mod.rs`:

```rust
//! Durability modes for M4 performance optimization
//!
//! This module defines the three durability modes and will contain
//! their implementations (added in Epic 21).

pub mod modes;

pub use modes::DurabilityMode;
```

#### Step 4: Export from engine crate

Update `crates/engine/src/lib.rs` to include:

```rust
pub mod durability;

pub use durability::DurabilityMode;
```

### Validation

```bash
~/.cargo/bin/cargo build -p in-mem-engine
~/.cargo/bin/cargo test -p in-mem-engine durability
~/.cargo/bin/cargo clippy --workspace -- -D warnings
~/.cargo/bin/cargo fmt --check
```

### Complete Story

```bash
./scripts/complete-story.sh 218
```

---

## Story #219: Performance Instrumentation Infrastructure

**GitHub Issue**: [#219](https://github.com/anibjoshi/in-mem/issues/219)
**Estimated Time**: 4 hours
**Dependencies**: Story #217

### Start Story

```bash
gh issue view 219
./scripts/start-story.sh 20 219 instrumentation
```

### Implementation Steps

#### Step 1: Create instrumentation module

Create `crates/engine/src/instrumentation.rs`:

```rust
//! Performance instrumentation for M4 optimization
//!
//! Feature-gated to avoid overhead in production.
//! Enable with: cargo build --features perf-trace

/// Per-operation performance trace
///
/// When `perf-trace` feature is enabled, this struct captures
/// timing information for each phase of an operation.
#[cfg(feature = "perf-trace")]
#[derive(Debug, Default, Clone)]
pub struct PerfTrace {
    /// Time to acquire snapshot (ns)
    pub snapshot_acquire_ns: u64,
    /// Time to validate read set (ns)
    pub read_set_validate_ns: u64,
    /// Time to apply write set (ns)
    pub write_set_apply_ns: u64,
    /// Time to append to WAL (ns)
    pub wal_append_ns: u64,
    /// Time to fsync (ns)
    pub fsync_ns: u64,
    /// Total commit time (ns)
    pub commit_total_ns: u64,
    /// Number of keys read
    pub keys_read: usize,
    /// Number of keys written
    pub keys_written: usize,
}

#[cfg(feature = "perf-trace")]
impl PerfTrace {
    /// Create new empty trace
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a timed section
    pub fn time<F, T>(f: F) -> (T, u64)
    where
        F: FnOnce() -> T,
    {
        let start = std::time::Instant::now();
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

    /// Calculate percentage breakdown
    pub fn breakdown(&self) -> PerfBreakdown {
        let total = self.commit_total_ns.max(1) as f64;
        PerfBreakdown {
            snapshot_pct: self.snapshot_acquire_ns as f64 / total * 100.0,
            validate_pct: self.read_set_validate_ns as f64 / total * 100.0,
            apply_pct: self.write_set_apply_ns as f64 / total * 100.0,
            wal_pct: self.wal_append_ns as f64 / total * 100.0,
            fsync_pct: self.fsync_ns as f64 / total * 100.0,
        }
    }
}

#[cfg(feature = "perf-trace")]
#[derive(Debug, Clone)]
pub struct PerfBreakdown {
    pub snapshot_pct: f64,
    pub validate_pct: f64,
    pub apply_pct: f64,
    pub wal_pct: f64,
    pub fsync_pct: f64,
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
///
/// When `perf-trace` is enabled, times the expression and stores in trace.
/// When disabled, just evaluates the expression with zero overhead.
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

/// Aggregate performance statistics
#[cfg(feature = "perf-trace")]
#[derive(Debug, Default)]
pub struct PerfStats {
    traces: Vec<PerfTrace>,
}

#[cfg(feature = "perf-trace")]
impl PerfStats {
    pub fn new() -> Self {
        Self { traces: Vec::new() }
    }

    pub fn record(&mut self, trace: PerfTrace) {
        self.traces.push(trace);
    }

    pub fn count(&self) -> usize {
        self.traces.len()
    }

    pub fn mean_commit_ns(&self) -> f64 {
        if self.traces.is_empty() {
            return 0.0;
        }
        let sum: u64 = self.traces.iter().map(|t| t.commit_total_ns).sum();
        sum as f64 / self.traces.len() as f64
    }

    pub fn p99_commit_ns(&self) -> u64 {
        if self.traces.is_empty() {
            return 0;
        }
        let mut sorted: Vec<_> = self.traces.iter().map(|t| t.commit_total_ns).collect();
        sorted.sort();
        let idx = (sorted.len() as f64 * 0.99) as usize;
        sorted[idx.min(sorted.len() - 1)]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_perf_trace_creation() {
        let trace = PerfTrace::new();
        let _ = trace.summary();
    }

    #[cfg(feature = "perf-trace")]
    #[test]
    fn test_perf_time_macro() {
        let mut trace = PerfTrace::new();
        let result = perf_time!(trace, snapshot_acquire_ns, {
            std::thread::sleep(std::time::Duration::from_micros(10));
            42
        });
        assert_eq!(result, 42);
        assert!(trace.snapshot_acquire_ns > 0);
    }

    #[cfg(feature = "perf-trace")]
    #[test]
    fn test_perf_breakdown() {
        let mut trace = PerfTrace::new();
        trace.commit_total_ns = 1000;
        trace.snapshot_acquire_ns = 100;
        trace.fsync_ns = 500;

        let breakdown = trace.breakdown();
        assert!((breakdown.snapshot_pct - 10.0).abs() < 0.1);
        assert!((breakdown.fsync_pct - 50.0).abs() < 0.1);
    }

    #[cfg(feature = "perf-trace")]
    #[test]
    fn test_perf_stats() {
        let mut stats = PerfStats::new();

        for i in 1..=100 {
            let mut trace = PerfTrace::new();
            trace.commit_total_ns = i * 1000;
            stats.record(trace);
        }

        assert_eq!(stats.count(), 100);
        assert!(stats.mean_commit_ns() > 0.0);
        assert!(stats.p99_commit_ns() >= 99000);
    }
}
```

#### Step 2: Export from engine crate

Update `crates/engine/src/lib.rs`:

```rust
pub mod instrumentation;

pub use instrumentation::PerfTrace;
#[cfg(feature = "perf-trace")]
pub use instrumentation::PerfStats;
```

### Validation

```bash
# Build without feature
~/.cargo/bin/cargo build -p in-mem-engine

# Build with feature
~/.cargo/bin/cargo build -p in-mem-engine --features perf-trace

# Test without feature
~/.cargo/bin/cargo test -p in-mem-engine instrumentation

# Test with feature
~/.cargo/bin/cargo test -p in-mem-engine instrumentation --features perf-trace

~/.cargo/bin/cargo clippy --workspace -- -D warnings
~/.cargo/bin/cargo fmt --check
```

### Complete Story

```bash
./scripts/complete-story.sh 219
```

---

## Story #220: Database Builder Pattern

**GitHub Issue**: [#220](https://github.com/anibjoshi/in-mem/issues/220)
**Estimated Time**: 4 hours
**Dependencies**: Story #217, Story #218

### Start Story

```bash
gh issue view 220
./scripts/start-story.sh 20 220 database-builder
```

### Implementation Steps

#### Step 1: Create DatabaseBuilder

Add to `crates/engine/src/database.rs` (or create new file):

```rust
use std::path::PathBuf;
use crate::durability::DurabilityMode;

/// Builder for Database configuration
///
/// # Example
///
/// ```rust
/// use in_mem_engine::{Database, DurabilityMode};
///
/// // InMemory mode for tests
/// let db = Database::builder()
///     .in_memory()
///     .open_temp()?;
///
/// // Buffered mode for production
/// let db = Database::builder()
///     .path("/var/data/mydb")
///     .buffered()
///     .open()?;
///
/// // Strict mode with custom config
/// let db = Database::builder()
///     .path("/var/data/mydb")
///     .durability(DurabilityMode::Strict)
///     .open()?;
/// ```
#[derive(Debug, Clone)]
pub struct DatabaseBuilder {
    path: Option<PathBuf>,
    durability: DurabilityMode,
}

impl DatabaseBuilder {
    /// Create new builder with defaults
    pub fn new() -> Self {
        Self {
            path: None,
            durability: DurabilityMode::default(), // Strict
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

    /// Use InMemory mode (convenience)
    pub fn in_memory(mut self) -> Self {
        self.durability = DurabilityMode::InMemory;
        self
    }

    /// Use Buffered mode with defaults (convenience)
    pub fn buffered(mut self) -> Self {
        self.durability = DurabilityMode::buffered_default();
        self
    }

    /// Use Buffered mode with custom parameters
    pub fn buffered_with(mut self, flush_interval_ms: u64, max_pending_writes: usize) -> Self {
        self.durability = DurabilityMode::Buffered {
            flush_interval_ms,
            max_pending_writes,
        };
        self
    }

    /// Use Strict mode (convenience, same as default)
    pub fn strict(mut self) -> Self {
        self.durability = DurabilityMode::Strict;
        self
    }

    /// Get configured path
    pub fn get_path(&self) -> Option<&PathBuf> {
        self.path.as_ref()
    }

    /// Get configured durability mode
    pub fn get_durability(&self) -> DurabilityMode {
        self.durability
    }

    /// Open the database
    ///
    /// If no path is set, generates a temporary path.
    pub fn open(self) -> Result<Database> {
        let path = self.path.unwrap_or_else(|| {
            std::env::temp_dir().join(format!("inmem-{}", uuid::Uuid::new_v4()))
        });

        Database::open_with_mode(path, self.durability)
    }

    /// Open a temporary database (for tests)
    ///
    /// Always generates a unique temporary path.
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

// Add to Database impl
impl Database {
    /// Create a new database builder
    pub fn builder() -> DatabaseBuilder {
        DatabaseBuilder::new()
    }

    /// Open database with specific durability mode
    ///
    /// Note: This is called by DatabaseBuilder. Most users should use
    /// `Database::builder()` instead.
    pub fn open_with_mode(path: PathBuf, durability: DurabilityMode) -> Result<Self> {
        // TODO: Implement in Epic 21
        // For now, delegate to existing open() and store mode
        let mut db = Self::open(&path)?;
        db.durability_mode = durability;
        Ok(db)
    }

    /// Get current durability mode
    pub fn durability_mode(&self) -> DurabilityMode {
        self.durability_mode
    }
}
```

#### Step 2: Add durability_mode field to Database

Update Database struct to include:

```rust
pub struct Database {
    // ... existing fields
    durability_mode: DurabilityMode,
}
```

#### Step 3: Write tests

```rust
#[cfg(test)]
mod builder_tests {
    use super::*;

    #[test]
    fn test_builder_default() {
        let builder = DatabaseBuilder::new();
        assert_eq!(builder.get_durability(), DurabilityMode::Strict);
        assert!(builder.get_path().is_none());
    }

    #[test]
    fn test_builder_in_memory() {
        let builder = DatabaseBuilder::new().in_memory();
        assert_eq!(builder.get_durability(), DurabilityMode::InMemory);
    }

    #[test]
    fn test_builder_buffered() {
        let builder = DatabaseBuilder::new().buffered();
        match builder.get_durability() {
            DurabilityMode::Buffered { .. } => {}
            _ => panic!("Expected Buffered mode"),
        }
    }

    #[test]
    fn test_builder_buffered_custom() {
        let builder = DatabaseBuilder::new().buffered_with(50, 500);
        match builder.get_durability() {
            DurabilityMode::Buffered { flush_interval_ms, max_pending_writes } => {
                assert_eq!(flush_interval_ms, 50);
                assert_eq!(max_pending_writes, 500);
            }
            _ => panic!("Expected Buffered mode"),
        }
    }

    #[test]
    fn test_builder_path() {
        let builder = DatabaseBuilder::new().path("/tmp/test");
        assert_eq!(builder.get_path(), Some(&PathBuf::from("/tmp/test")));
    }

    #[test]
    fn test_builder_chaining() {
        let builder = DatabaseBuilder::new()
            .path("/tmp/test")
            .in_memory()
            .buffered()  // Overrides in_memory
            .strict();   // Overrides buffered

        assert_eq!(builder.get_durability(), DurabilityMode::Strict);
    }

    #[test]
    fn test_database_builder_convenience() {
        let builder = Database::builder();
        assert_eq!(builder.get_durability(), DurabilityMode::Strict);
    }

    #[test]
    fn test_open_temp() {
        let db = Database::builder()
            .in_memory()
            .open_temp()
            .expect("Should open temp database");

        assert_eq!(db.durability_mode(), DurabilityMode::InMemory);
    }
}
```

### Validation

```bash
~/.cargo/bin/cargo build -p in-mem-engine
~/.cargo/bin/cargo test -p in-mem-engine builder
~/.cargo/bin/cargo clippy --workspace -- -D warnings
~/.cargo/bin/cargo fmt --check
```

### Complete Story

```bash
./scripts/complete-story.sh 220
```

---

## Epic 20 Completion Checklist

Once ALL 4 stories are complete and merged to `epic-20-performance-foundation`:

### 1. Final Validation

```bash
# All tests pass
~/.cargo/bin/cargo test --workspace

# Release build clean
~/.cargo/bin/cargo build --release --workspace

# No clippy warnings
~/.cargo/bin/cargo clippy --workspace -- -D warnings

# Formatting clean
~/.cargo/bin/cargo fmt --check

# Benchmarks run
~/.cargo/bin/cargo bench --bench m4_performance
```

### 2. Verify Deliverables

- [ ] `m3_baseline_perf` git tag exists
- [ ] `perf-trace` feature flag compiles
- [ ] DurabilityMode enum has all three variants
- [ ] DatabaseBuilder pattern works
- [ ] `cargo bench --bench m4_performance` runs
- [ ] All unit tests pass

### 3. Merge to Develop

```bash
git checkout develop
git merge --no-ff epic-20-performance-foundation -m "Epic 20: Performance Foundation

Complete:
- M3 baseline tag (m3_baseline_perf)
- Benchmark infrastructure for M4
- DurabilityMode type definition
- Performance instrumentation (feature-gated)
- Database builder pattern

Stories:
- #217: Tag M3 Baseline & Benchmark Infrastructure
- #218: DurabilityMode Type Definition
- #219: Performance Instrumentation Infrastructure
- #220: Database Builder Pattern

This unblocks Epics 21-24 for parallel implementation.
"

git push origin develop
```

### 4. Close Epic Issue

```bash
gh issue close 211 --comment "Epic 20: Performance Foundation - COMPLETE

All 4 stories completed:
- #217: Tag M3 Baseline & Benchmark Infrastructure
- #218: DurabilityMode Type Definition
- #219: Performance Instrumentation Infrastructure
- #220: Database Builder Pattern

Epics 21-23 are now unblocked for parallel implementation.
"
```

---

## Critical Notes

### This Epic Blocks Everything

Story #217 MUST be completed before ANY other M4 work can begin. It establishes:
- The performance baseline tag
- Benchmark infrastructure
- Foundation for measuring improvements

### DurabilityMode is Foundational

The DurabilityMode enum defined in #218 is used throughout M4:
- Epic 21 implements the three modes
- Epic 25 benchmarks compare modes
- Default must be Strict for backwards compatibility

---

## Summary

Epic 20 establishes the foundation for all M4 performance work:
- Tags M3 baseline for comparison
- Sets up benchmark infrastructure
- Defines DurabilityMode enum
- Creates performance instrumentation
- Implements Database builder pattern

**After Epic 20**: Epics 21-23 can begin in parallel, with Epic 24 following Epic 22.
