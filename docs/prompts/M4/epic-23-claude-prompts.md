# Epic 23: Transaction Pooling - Implementation Prompts

**Epic Goal**: Eliminate allocation overhead on transaction hot path

**GitHub Issue**: [#214](https://github.com/anibjoshi/in-mem/issues/214)
**Status**: Ready after Epic 20
**Dependencies**: Epic 20 complete

---

## AUTHORITATIVE SPECIFICATIONS - READ THESE FIRST

**`docs/architecture/M4_ARCHITECTURE.md` is the GOSPEL for ALL M4 implementation.**

See `docs/prompts/M4_PROMPT_HEADER.md` for complete guidelines.

---

## Epic 23 Overview

### Reference: Critical Invariants

See `docs/prompts/M4_PROMPT_HEADER.md` for critical invariants. **Especially important for this epic:**
- **Atomicity Scope**: Transactions are atomic within a single RunId ONLY
- **Zero Allocations**: Hot path must have zero allocations after warmup (red flag check)

### Scope
- TransactionContext reset method
- Thread-local transaction pool
- Pooled begin/end transaction API
- Zero-allocation verification

### Success Criteria
- [ ] `reset()` method clears state without deallocating
- [ ] `reset()` preserves HashMap capacity
- [ ] Thread-local pool with max 8 contexts per thread
- [ ] `begin_transaction()` uses pool
- [ ] `end_transaction()` returns to pool
- [ ] Zero allocations on hot path after warmup

### Component Breakdown
- **Story #212 (GitHub #232)**: TransactionContext Reset Method
- **Story #213 (GitHub #233)**: Thread-Local Transaction Pool - FOUNDATION
- **Story #214 (GitHub #234)**: Pooled Transaction API
- **Story #215 (GitHub #235)**: Zero-Allocation Verification

---

## Dependency Graph

```
Story #232 (Reset) ──┐
                     └──> Story #233 (Pool) ──> Story #234 (API) ──> Story #235 (Verify)
```

---

## Story #232: TransactionContext Reset Method

**GitHub Issue**: [#232](https://github.com/anibjoshi/in-mem/issues/232)
**Estimated Time**: 3 hours
**Dependencies**: Epic 20 complete

### Start Story

```bash
gh issue view 232
./scripts/start-story.sh 23 232 txn-reset
```

### Implementation

Add to TransactionContext:

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

        // Clear but keep capacity - this is the key optimization!
        self.read_set.clear();
        self.write_set.clear();
    }

    /// Get current capacity (for debugging/testing)
    pub fn capacity(&self) -> (usize, usize) {
        (self.read_set.capacity(), self.write_set.capacity())
    }
}
```

### Tests

```rust
#[test]
fn test_reset_preserves_capacity() {
    let mut ctx = TransactionContext::new(run_id, snapshot, 1);

    // Fill with data
    for i in 0..100 {
        ctx.read_set.insert(format!("key{}", i), version);
        ctx.write_set.insert(format!("key{}", i), value);
    }

    let (read_cap, write_cap) = ctx.capacity();
    assert!(read_cap >= 100);
    assert!(write_cap >= 100);

    // Reset
    ctx.reset(new_run_id, new_snapshot, 2);

    // Capacity preserved
    let (new_read_cap, new_write_cap) = ctx.capacity();
    assert_eq!(new_read_cap, read_cap);
    assert_eq!(new_write_cap, write_cap);

    // But data cleared
    assert!(ctx.read_set.is_empty());
    assert!(ctx.write_set.is_empty());
}
```

### Complete Story

```bash
./scripts/complete-story.sh 232
```

---

## Story #233: Thread-Local Transaction Pool

**GitHub Issue**: [#233](https://github.com/anibjoshi/in-mem/issues/233)
**Estimated Time**: 4 hours
**Dependencies**: Story #232

### Start Story

```bash
gh issue view 233
./scripts/start-story.sh 23 233 txn-pool
```

### Implementation

Create `crates/engine/src/transaction/pool.rs`:

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_acquire_from_empty_pool() {
        TransactionPool::clear();
        assert_eq!(TransactionPool::pool_size(), 0);

        let ctx = TransactionPool::acquire(run_id, snapshot, 1);
        assert_eq!(TransactionPool::pool_size(), 0); // Still empty
    }

    #[test]
    fn test_release_adds_to_pool() {
        TransactionPool::clear();
        let ctx = TransactionPool::acquire(run_id, snapshot, 1);
        TransactionPool::release(ctx);
        assert_eq!(TransactionPool::pool_size(), 1);
    }

    #[test]
    fn test_acquire_reuses_pooled() {
        TransactionPool::clear();

        // Create and release a context with capacity
        let mut ctx = TransactionPool::acquire(run_id, snapshot, 1);
        for i in 0..100 {
            ctx.read_set.insert(format!("key{}", i), 1);
        }
        let original_cap = ctx.capacity();
        TransactionPool::release(ctx);

        // Acquire again - should reuse
        let ctx2 = TransactionPool::acquire(run_id, snapshot, 2);
        assert_eq!(ctx2.capacity(), original_cap);
        assert!(ctx2.read_set.is_empty()); // But cleared
    }

    #[test]
    fn test_pool_caps_at_max_size() {
        TransactionPool::clear();

        // Release more than MAX_POOL_SIZE
        for _ in 0..MAX_POOL_SIZE + 5 {
            let ctx = TransactionPool::acquire(run_id, snapshot, 1);
            TransactionPool::release(ctx);
        }

        assert_eq!(TransactionPool::pool_size(), MAX_POOL_SIZE);
    }

    #[test]
    fn test_pool_is_thread_local() {
        use std::thread;

        TransactionPool::clear();
        let ctx = TransactionPool::acquire(run_id, snapshot, 1);
        TransactionPool::release(ctx);
        assert_eq!(TransactionPool::pool_size(), 1);

        // Other thread has its own pool
        let handle = thread::spawn(|| {
            assert_eq!(TransactionPool::pool_size(), 0);
        });
        handle.join().unwrap();

        // Our pool unchanged
        assert_eq!(TransactionPool::pool_size(), 1);
    }
}
```

### Complete Story

```bash
./scripts/complete-story.sh 233
```

---

## Story #234: Pooled Transaction API

**GitHub Issue**: [#234](https://github.com/anibjoshi/in-mem/issues/234)
**Estimated Time**: 3 hours
**Dependencies**: Story #233

### Implementation

Update Database:

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
    pub fn end_transaction(&self, ctx: TransactionContext) {
        TransactionPool::release(ctx);
    }

    /// Execute a transaction with automatic pooling
    pub fn transaction<F, T>(&self, run_id: RunId, f: F) -> Result<T>
    where
        F: FnOnce(&mut TransactionContext) -> Result<T>,
    {
        let mut ctx = self.begin_transaction(run_id)?;

        match f(&mut ctx) {
            Ok(value) => {
                self.commit_transaction(&mut ctx)?;
                self.end_transaction(ctx);
                Ok(value)
            }
            Err(e) => {
                self.end_transaction(ctx);
                Err(e)
            }
        }
    }
}
```

### Complete Story

```bash
./scripts/complete-story.sh 234
```

---

## Story #235: Zero-Allocation Verification

**GitHub Issue**: [#235](https://github.com/anibjoshi/in-mem/issues/235)
**Estimated Time**: 3 hours
**Dependencies**: Story #234

### Implementation

Create `tests/m4_zero_allocation.rs`:

```rust
//! Zero-allocation verification tests

#[test]
fn test_pool_reuses_contexts() {
    let db = Database::builder().in_memory().open_temp().unwrap();
    let run_id = RunId::new();

    // Warmup - fill pool
    for _ in 0..8 {
        db.transaction(run_id, |_| Ok(())).unwrap();
    }

    let pool_size_before = TransactionPool::pool_size();

    // Operations should reuse from pool
    for _ in 0..100 {
        db.transaction(run_id, |txn| {
            txn.put(key, value)?;
            Ok(())
        }).unwrap();
    }

    let pool_size_after = TransactionPool::pool_size();
    assert_eq!(pool_size_before, pool_size_after, "Pool size should be stable");
}
```

### Complete Story

```bash
./scripts/complete-story.sh 235
```

---

## Epic 23 Completion Checklist

### Verify Deliverables

- [ ] reset() preserves capacity
- [ ] Thread-local pool working
- [ ] begin/end use pool
- [ ] Zero allocations after warmup

### Merge to Develop

```bash
git checkout develop
git merge --no-ff epic-23-transaction-pooling -m "Epic 23: Transaction Pooling complete"
git push origin develop
gh issue close 214 --comment "Epic 23 complete. All 4 stories delivered."
```
