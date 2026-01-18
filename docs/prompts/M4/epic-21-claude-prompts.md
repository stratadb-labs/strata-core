# Epic 21: Durability Modes - Implementation Prompts

**Epic Goal**: Implement three durability modes trading latency vs durability

**GitHub Issue**: [#212](https://github.com/anibjoshi/in-mem/issues/212)
**Status**: Ready after Epic 20
**Dependencies**: Epic 20 complete (DurabilityMode type, builder pattern)

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

## Epic 21 Overview

### Reference: Critical Invariants

See `docs/prompts/M4_PROMPT_HEADER.md` for critical invariants. **Especially important for this epic:**
- **Thread Lifecycle (Buffered mode)**: Background flush thread MUST have proper shutdown handling with `AtomicBool` and `JoinHandle`.

### Scope
- Durability trait abstraction
- InMemory mode (no WAL, no fsync)
- Buffered mode (WAL append, async fsync) - **CRITICAL: Thread lifecycle management required**
- Strict mode (WAL append + sync fsync)
- Per-operation durability override
- Graceful shutdown

### Success Criteria
- [ ] Durability trait defined and implemented by all modes
- [ ] InMemory: `engine/put_direct` < 3µs
- [ ] InMemory: No WAL file created
- [ ] Buffered: `kvstore/put` < 30µs
- [ ] Buffered: Background flush thread working
- [ ] Strict: Identical behavior to M3
- [ ] Per-operation override works
- [ ] Graceful shutdown flushes Buffered mode

### Component Breakdown
- **Story #201 (GitHub #221)**: Durability Trait Abstraction - FOUNDATION
- **Story #202 (GitHub #222)**: InMemory Durability Implementation
- **Story #203 (GitHub #223)**: Strict Durability Implementation
- **Story #204 (GitHub #224)**: Buffered Durability Implementation
- **Story #205 (GitHub #225)**: Per-Operation Durability Override
- **Story #206 (GitHub #226)**: Graceful Shutdown

---

## Dependency Graph

```
Story #221 (Durability Trait) ──┬──> Story #222 (InMemory)
                               └──> Story #223 (Strict)
                                         └──> Story #224 (Buffered)
                                                  ├──> Story #225 (Override)
                                                  └──> Story #226 (Shutdown)
```

---

## Parallelization Strategy

| Phase | Duration | Claude 1 | Claude 2 |
|-------|----------|----------|----------|
| 1 | 3 hours | #221 Durability Trait | - |
| 2 | 3 hours | #222 InMemory | #223 Strict |
| 3 | 5 hours | #224 Buffered | - |
| 4 | 3 hours | #225 Override | #226 Shutdown |

**Total Wall Time**: ~14 hours (vs. ~20 hours sequential)

---

## Story #221: Durability Trait Abstraction

**GitHub Issue**: [#221](https://github.com/anibjoshi/in-mem/issues/221)
**Estimated Time**: 3 hours
**Dependencies**: Epic 20 complete
**Blocks**: Stories #222-224

### Start Story

```bash
gh issue view 221
./scripts/start-story.sh 21 221 durability-trait
```

### Implementation

Create `crates/engine/src/durability/trait.rs`:

```rust
//! Durability abstraction for M4 modes
//!
//! Each mode implements this trait differently.

use crate::WriteSet;
use in_mem_core::Result;

/// Durability behavior abstraction
///
/// All three durability modes implement this trait:
/// - InMemory: No persistence
/// - Buffered: Async persistence
/// - Strict: Sync persistence
pub trait Durability: Send + Sync {
    /// Commit a write set with this durability level
    ///
    /// # Contract
    /// - InMemory: Apply to storage only
    /// - Buffered: Append to WAL buffer, apply to storage
    /// - Strict: Append to WAL, fsync, apply to storage
    fn commit(&self, write_set: &WriteSet) -> Result<()>;

    /// Graceful shutdown - flush any pending data
    ///
    /// # Contract
    /// - InMemory: No-op
    /// - Buffered: Flush all pending writes, fsync
    /// - Strict: No-op (already synced)
    fn shutdown(&self) -> Result<()>;

    /// Check if this durability mode persists data
    fn is_persistent(&self) -> bool;

    /// Get human-readable mode name
    fn mode_name(&self) -> &'static str;
}
```

Update `crates/engine/src/durability/mod.rs`:

```rust
//! Durability modes for M4 performance optimization

pub mod modes;
mod r#trait;
mod inmemory;
mod buffered;
mod strict;

pub use modes::DurabilityMode;
pub use r#trait::Durability;
pub use inmemory::InMemoryDurability;
pub use buffered::BufferedDurability;
pub use strict::StrictDurability;
```

### Validation

```bash
~/.cargo/bin/cargo build -p in-mem-engine
~/.cargo/bin/cargo test -p in-mem-engine durability
```

### Complete Story

```bash
./scripts/complete-story.sh 221
```

---

## Story #222: InMemory Durability Implementation

**GitHub Issue**: [#222](https://github.com/anibjoshi/in-mem/issues/222)
**Estimated Time**: 3 hours
**Dependencies**: Story #221

### Start Story

```bash
gh issue view 222
./scripts/start-story.sh 21 222 inmemory-durability
```

### Implementation

Create `crates/engine/src/durability/inmemory.rs`:

```rust
//! InMemory durability mode
//!
//! No WAL, no fsync. All data lost on crash.
//! Fastest mode - target <3µs for engine/put_direct.

use std::sync::Arc;
use super::Durability;
use crate::storage::Storage;
use crate::WriteSet;
use in_mem_core::Result;

/// InMemory durability - no persistence
///
/// # Performance Contract
/// - commit() < 3µs (excluding storage apply time)
/// - No syscalls
/// - No allocations on hot path
pub struct InMemoryDurability<S: Storage> {
    storage: Arc<S>,
}

impl<S: Storage> InMemoryDurability<S> {
    /// Create new InMemory durability
    pub fn new(storage: Arc<S>) -> Self {
        Self { storage }
    }
}

impl<S: Storage + Send + Sync> Durability for InMemoryDurability<S> {
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

    fn mode_name(&self) -> &'static str {
        "InMemory"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_inmemory_not_persistent() {
        // Test setup needed
    }

    #[test]
    fn test_inmemory_shutdown_succeeds() {
        // Test setup needed
    }
}
```

### Validation

```bash
~/.cargo/bin/cargo test -p in-mem-engine inmemory
~/.cargo/bin/cargo bench --bench m4_performance -- inmemory
```

### Complete Story

```bash
./scripts/complete-story.sh 222
```

---

## Story #223: Strict Durability Implementation

**GitHub Issue**: [#223](https://github.com/anibjoshi/in-mem/issues/223)
**Estimated Time**: 3 hours
**Dependencies**: Story #221

### Start Story

```bash
gh issue view 223
./scripts/start-story.sh 21 223 strict-durability
```

### Implementation

Create `crates/engine/src/durability/strict.rs`:

```rust
//! Strict durability mode
//!
//! WAL append + immediate fsync. Zero data loss.
//! Slowest mode - ~2ms for kvstore/put due to fsync.
//! This is the M3 default behavior.

use std::sync::Arc;
use super::Durability;
use crate::storage::Storage;
use crate::wal::WriteAheadLog;
use crate::WriteSet;
use in_mem_core::Result;

/// Strict durability - fsync on every write
///
/// # Guarantees
/// - Zero data loss on crash
/// - WAL always consistent
/// - Identical to M3 behavior
pub struct StrictDurability<S: Storage> {
    storage: Arc<S>,
    wal: Arc<WriteAheadLog>,
}

impl<S: Storage> StrictDurability<S> {
    /// Create new Strict durability
    pub fn new(storage: Arc<S>, wal: Arc<WriteAheadLog>) -> Self {
        Self { storage, wal }
    }
}

impl<S: Storage + Send + Sync> Durability for StrictDurability<S> {
    fn commit(&self, write_set: &WriteSet) -> Result<()> {
        // 1. Append to WAL
        self.wal.append(write_set)?;

        // 2. fsync immediately - this is the slow part (~2ms)
        self.wal.fsync()?;

        // 3. Apply to storage
        self.storage.apply(write_set)
    }

    fn shutdown(&self) -> Result<()> {
        // Already synced on every write - just ensure WAL is closed cleanly
        self.wal.fsync()
    }

    fn is_persistent(&self) -> bool {
        true
    }

    fn mode_name(&self) -> &'static str {
        "Strict"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_strict_is_persistent() {
        // Test setup needed
    }

    #[test]
    fn test_strict_survives_crash() {
        // Test setup needed
    }
}
```

### Validation

```bash
~/.cargo/bin/cargo test -p in-mem-engine strict
~/.cargo/bin/cargo bench --bench m4_performance -- strict
```

### Complete Story

```bash
./scripts/complete-story.sh 223
```

---

## Story #224: Buffered Durability Implementation

**GitHub Issue**: [#224](https://github.com/anibjoshi/in-mem/issues/224)
**Estimated Time**: 5 hours
**Dependencies**: Stories #221, #222, #223

### CRITICAL: Thread Lifecycle Management

> **The background flush thread MUST have proper lifecycle management to avoid resource leaks and ensure clean shutdown.**

Required fields in `BufferedDurability`:
- `shutdown: AtomicBool` - Shutdown signal
- `flush_thread: Option<JoinHandle<()>>` - Thread handle for join

Required `Drop` implementation:
```rust
impl Drop for BufferedDurability {
    fn drop(&mut self) {
        // Signal shutdown to background thread
        self.shutdown.store(true, Ordering::Release);
        self.flush_signal.notify_all();

        // Wait for thread to finish (join)
        if let Some(handle) = self.flush_thread.take() {
            let _ = handle.join();
        }
    }
}
```

**Why this matters:**
- Without proper shutdown, thread may continue running after struct dropped
- Without join, tests may fail intermittently due to lingering threads
- Resource leaks in production under repeated create/drop cycles

### Start Story

```bash
gh issue view 224
./scripts/start-story.sh 21 224 buffered-durability
```

### Implementation

Create `crates/engine/src/durability/buffered.rs`:

```rust
//! Buffered durability mode
//!
//! WAL append without immediate fsync.
//! Periodic flush based on interval or batch size.
//! Balanced mode - target <30µs for kvstore/put.

use std::sync::{Arc, atomic::{AtomicBool, AtomicUsize, Ordering}};
use std::time::{Duration, Instant};
use std::thread::{self, JoinHandle};
use parking_lot::{Mutex, Condvar};
use super::Durability;
use crate::storage::Storage;
use crate::wal::WriteAheadLog;
use crate::WriteSet;
use in_mem_core::Result;

/// Buffered durability - async fsync
///
/// # Performance Contract
/// - commit() < 30µs
/// - Bounded data loss window
pub struct BufferedDurability<S: Storage> {
    storage: Arc<S>,
    wal: Arc<WriteAheadLog>,

    // Flush configuration
    flush_interval: Duration,
    max_pending_writes: usize,

    // State tracking
    pending_writes: AtomicUsize,
    last_flush: Mutex<Instant>,

    // Shutdown coordination
    shutdown_flag: AtomicBool,
    flush_signal: Arc<(Mutex<bool>, Condvar)>,
}

impl<S: Storage + Send + Sync + 'static> BufferedDurability<S> {
    /// Create new Buffered durability
    pub fn new(
        storage: Arc<S>,
        wal: Arc<WriteAheadLog>,
        flush_interval_ms: u64,
        max_pending_writes: usize,
    ) -> Arc<Self> {
        Arc::new(Self {
            storage,
            wal,
            flush_interval: Duration::from_millis(flush_interval_ms),
            max_pending_writes,
            pending_writes: AtomicUsize::new(0),
            last_flush: Mutex::new(Instant::now()),
            shutdown_flag: AtomicBool::new(false),
            flush_signal: Arc::new((Mutex::new(false), Condvar::new())),
        })
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
    fn trigger_flush(&self) {
        let (lock, cvar) = &*self.flush_signal;
        let mut pending = lock.lock();
        *pending = true;
        cvar.notify_one();
    }

    /// Synchronous flush (for shutdown)
    pub fn flush_sync(&self) -> Result<()> {
        self.wal.fsync()?;
        self.pending_writes.store(0, Ordering::Relaxed);
        *self.last_flush.lock() = Instant::now();
        Ok(())
    }

    /// Start background flush thread
    pub fn start_flush_thread(self: &Arc<Self>) -> JoinHandle<()> {
        let durability = Arc::clone(self);
        thread::spawn(move || {
            loop {
                // Wait for signal or timeout
                let (lock, cvar) = &*durability.flush_signal;
                let mut pending = lock.lock();
                let _ = cvar.wait_for(&mut pending, durability.flush_interval);

                // Check if shutdown requested
                if durability.shutdown_flag.load(Ordering::SeqCst) {
                    // Final flush before exit
                    let _ = durability.flush_sync();
                    break;
                }

                // Perform flush
                if let Err(e) = durability.flush_sync() {
                    eprintln!("Buffered flush error: {}", e);
                }
            }
        })
    }
}

impl<S: Storage + Send + Sync> Durability for BufferedDurability<S> {
    fn commit(&self, write_set: &WriteSet) -> Result<()> {
        // 1. Append to WAL buffer (no fsync - fast!)
        self.wal.append(write_set)?;

        // 2. Apply to storage
        self.storage.apply(write_set)?;

        // 3. Track pending writes
        self.pending_writes.fetch_add(1, Ordering::Relaxed);

        // 4. Check if flush needed
        if self.should_flush() {
            self.trigger_flush();
        }

        Ok(())
    }

    fn shutdown(&self) -> Result<()> {
        // Signal shutdown
        self.shutdown_flag.store(true, Ordering::SeqCst);
        self.trigger_flush();

        // Synchronously flush all pending writes
        self.flush_sync()
    }

    fn is_persistent(&self) -> bool {
        true // Eventually persistent (after flush)
    }

    fn mode_name(&self) -> &'static str {
        "Buffered"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_buffered_flushes_on_interval() {
        // Test setup needed
    }

    #[test]
    fn test_buffered_flushes_on_max_pending() {
        // Test setup needed
    }

    #[test]
    fn test_buffered_shutdown_flushes() {
        // Test setup needed
    }
}
```

### Validation

```bash
~/.cargo/bin/cargo test -p in-mem-engine buffered
~/.cargo/bin/cargo bench --bench m4_performance -- buffered
```

### Complete Story

```bash
./scripts/complete-story.sh 224
```

---

## Story #225: Per-Operation Durability Override

**GitHub Issue**: [#225](https://github.com/anibjoshi/in-mem/issues/225)
**Estimated Time**: 3 hours
**Dependencies**: Story #224

### Start Story

```bash
gh issue view 225
./scripts/start-story.sh 21 225 durability-override
```

### Implementation

Add to Database:

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
        let mut txn = self.begin_transaction(run_id)?;
        let result = f(&mut txn)?;
        self.commit_with_durability(&mut txn, durability)?;
        Ok(result)
    }
}
```

### Validation

```bash
~/.cargo/bin/cargo test -p in-mem-engine override
```

### Complete Story

```bash
./scripts/complete-story.sh 225
```

---

## Story #226: Graceful Shutdown

**GitHub Issue**: [#226](https://github.com/anibjoshi/in-mem/issues/226)
**Estimated Time**: 3 hours
**Dependencies**: Story #224

### Start Story

```bash
gh issue view 226
./scripts/start-story.sh 21 226 graceful-shutdown
```

### Implementation

Add to Database:

```rust
impl Database {
    /// Graceful shutdown - ensures all data is persisted
    pub fn shutdown(&self) -> Result<()> {
        // Stop accepting new transactions
        self.accepting_transactions.store(false, Ordering::SeqCst);

        // Flush based on mode
        self.durability.shutdown()
    }

    pub fn is_open(&self) -> bool {
        self.accepting_transactions.load(Ordering::SeqCst)
    }
}

impl Drop for Database {
    fn drop(&mut self) {
        if let Err(e) = self.shutdown() {
            eprintln!("Warning: Error during database shutdown: {}", e);
        }
    }
}
```

### Validation

```bash
~/.cargo/bin/cargo test -p in-mem-engine shutdown
```

### Complete Story

```bash
./scripts/complete-story.sh 226
```

---

## Epic 21 Completion Checklist

### 1. Final Validation

```bash
~/.cargo/bin/cargo test --workspace
~/.cargo/bin/cargo bench --bench m4_performance
~/.cargo/bin/cargo clippy --workspace -- -D warnings
```

### 2. Verify Deliverables

- [ ] Durability trait defined
- [ ] InMemory mode: < 3µs
- [ ] Buffered mode: < 30µs
- [ ] Strict mode: Same as M3
- [ ] Per-operation override works
- [ ] Graceful shutdown flushes Buffered

### 3. Merge to Develop

```bash
git checkout develop
git merge --no-ff epic-21-durability-modes -m "Epic 21: Durability Modes complete"
git push origin develop
gh issue close 212 --comment "Epic 21 complete. All 6 stories delivered."
```
