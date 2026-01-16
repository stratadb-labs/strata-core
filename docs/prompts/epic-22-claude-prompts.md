# Epic 22: Sharded Storage - Implementation Prompts

**Epic Goal**: Replace RwLock + BTreeMap with DashMap + HashMap for better concurrency

**GitHub Issue**: [#213](https://github.com/anibjoshi/in-mem/issues/213)
**Status**: Ready after Epic 20
**Dependencies**: Epic 20 complete

---

## AUTHORITATIVE SPECIFICATIONS - READ THESE FIRST

**`docs/architecture/M4_ARCHITECTURE.md` is the GOSPEL for ALL M4 implementation.**

See `docs/prompts/M4_PROMPT_HEADER.md` for complete guidelines.

---

## Epic 22 Overview

### Reference: Critical Invariants

See `docs/prompts/M4_PROMPT_HEADER.md` for critical invariants. **Especially important for this epic:**
- **Required Dependencies**: Use `rustc-hash` crate, NOT `fxhash`
- **Snapshot Semantic Invariant**: Snapshots must be observationally equivalent to transaction reads

### CRITICAL: Dependency Note

> **Use `rustc-hash` crate (NOT `fxhash`).**

```toml
[dependencies]
dashmap = "5"
rustc-hash = "1.1"    # NOT fxhash - provides FxHashMap
parking_lot = "0.12"
```

Import as: `use rustc_hash::{FxHashMap, FxBuildHasher};`

### Scope
- ShardedStore structure with DashMap
- Per-RunId sharding
- FxHash for fast hashing (from rustc-hash crate)
- Get/Put/Delete operations
- List operations (with sort)
- Snapshot fast path (allocation-free)
- Migration from UnifiedStore

### Success Criteria
- [ ] ShardedStore with DashMap<RunId, Shard>
- [ ] Shard contains FxHashMap<Key, VersionedValue>
- [ ] get() is lock-free via DashMap
- [ ] put() only locks target shard
- [ ] Snapshot acquisition < 500ns
- [ ] Different runs never contend
- [ ] Disjoint scaling ≥ 1.8× at 2 threads

### Component Breakdown
- **Story #207 (GitHub #227)**: ShardedStore Structure - FOUNDATION
- **Story #208 (GitHub #228)**: ShardedStore Get/Put Operations
- **Story #209 (GitHub #229)**: ShardedStore List Operations
- **Story #210 (GitHub #230)**: Snapshot Acquisition (Fast Path)
- **Story #211 (GitHub #231)**: Storage Migration Path

---

## Dependency Graph

```
Story #227 (Structure) ──┬──> Story #228 (Get/Put)
                        └──> Story #229 (List)
                                  └──> Story #230 (Snapshot) ──> Story #231 (Migration)
```

---

## Story #227: ShardedStore Structure

**GitHub Issue**: [#227](https://github.com/anibjoshi/in-mem/issues/227)
**Estimated Time**: 4 hours
**Blocks**: Stories #228-231

### Start Story

```bash
gh issue view 227
./scripts/start-story.sh 22 227 sharded-store-structure
```

### Implementation

Add dependencies to `Cargo.toml`:

```toml
[dependencies]
dashmap = "5"
rustc-hash = "1.1"    # NOT fxhash - provides FxHashMap
parking_lot = "0.12"
```

**IMPORTANT**: Use `rustc-hash` crate, NOT `fxhash`. They both provide `FxHashMap` but are different crates.

Create `crates/engine/src/storage/sharded.rs`:

```rust
//! Sharded storage for M4 performance
//!
//! Replaces RwLock + BTreeMap with DashMap + HashMap.
//! Lock-free reads, sharded writes, O(1) lookups.

use dashmap::DashMap;
use rustc_hash::FxHashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use in_mem_core::{RunId, Key, VersionedValue};

/// Per-run shard containing run's data
pub struct Shard {
    /// HashMap with FxHash for O(1) lookups
    pub(crate) data: FxHashMap<Key, VersionedValue>,
}

impl Shard {
    pub fn new() -> Self {
        Self {
            data: FxHashMap::default(),
        }
    }

    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            data: FxHashMap::with_capacity_and_hasher(capacity, Default::default()),
        }
    }
}

impl Default for Shard {
    fn default() -> Self {
        Self::new()
    }
}

/// Sharded storage - DashMap by RunId, HashMap within
///
/// # Design
/// - DashMap: 16-way sharded by default, lock-free reads
/// - FxHashMap: O(1) lookups, fast non-crypto hash
/// - Per-RunId: Natural agent partitioning, no cross-run contention
pub struct ShardedStore {
    /// Per-run shards using DashMap
    shards: DashMap<RunId, Shard>,
    /// Global version for snapshots
    version: AtomicU64,
}

impl ShardedStore {
    /// Create new sharded store
    pub fn new() -> Self {
        Self {
            shards: DashMap::new(),
            version: AtomicU64::new(0),
        }
    }

    /// Create with expected number of runs
    pub fn with_capacity(num_runs: usize) -> Self {
        Self {
            shards: DashMap::with_capacity(num_runs),
            version: AtomicU64::new(0),
        }
    }

    /// Get current version
    pub fn version(&self) -> u64 {
        self.version.load(Ordering::Acquire)
    }

    /// Increment version and return new value
    pub fn next_version(&self) -> u64 {
        self.version.fetch_add(1, Ordering::AcqRel) + 1
    }

    /// Get number of shards (runs)
    pub fn shard_count(&self) -> usize {
        self.shards.len()
    }
}

impl Default for ShardedStore {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sharded_store_creation() {
        let store = ShardedStore::new();
        assert_eq!(store.shard_count(), 0);
        assert_eq!(store.version(), 0);
    }

    #[test]
    fn test_version_increment() {
        let store = ShardedStore::new();
        assert_eq!(store.next_version(), 1);
        assert_eq!(store.next_version(), 2);
        assert_eq!(store.version(), 2);
    }

    #[test]
    fn test_version_thread_safety() {
        use std::thread;
        let store = Arc::new(ShardedStore::new());
        let handles: Vec<_> = (0..10)
            .map(|_| {
                let store = Arc::clone(&store);
                thread::spawn(move || {
                    for _ in 0..100 {
                        store.next_version();
                    }
                })
            })
            .collect();

        for h in handles {
            h.join().unwrap();
        }

        assert_eq!(store.version(), 1000);
    }
}
```

### Validation

```bash
~/.cargo/bin/cargo build -p in-mem-engine
~/.cargo/bin/cargo test -p in-mem-engine sharded
```

### Complete Story

```bash
./scripts/complete-story.sh 227
```

---

## Story #228: ShardedStore Get/Put Operations

**GitHub Issue**: [#228](https://github.com/anibjoshi/in-mem/issues/228)
**Estimated Time**: 4 hours
**Dependencies**: Story #227

### Implementation

Add to `crates/engine/src/storage/sharded.rs`:

```rust
impl ShardedStore {
    /// Get a value by run_id and key
    ///
    /// Lock-free read via DashMap.
    pub fn get(&self, run_id: &RunId, key: &Key) -> Option<VersionedValue> {
        self.shards
            .get(run_id)
            .and_then(|shard| shard.data.get(key).cloned())
    }

    /// Put a value for run_id and key
    ///
    /// Sharded write - only locks this run's shard.
    pub fn put(&self, run_id: RunId, key: Key, value: VersionedValue) {
        self.shards
            .entry(run_id)
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
    pub fn apply(&self, write_set: &WriteSet) -> Result<()> {
        for (key, value) in write_set.writes() {
            let run_id = key.run_id();
            match value {
                Some(v) => self.put(run_id, key.clone(), v.clone()),
                None => { self.delete(&run_id, key); }
            }
        }
        Ok(())
    }
}
```

### Validation

```bash
~/.cargo/bin/cargo test -p in-mem-engine sharded::tests::test_get_put
```

### Complete Story

```bash
./scripts/complete-story.sh 228
```

---

## Story #229: ShardedStore List Operations

**GitHub Issue**: [#229](https://github.com/anibjoshi/in-mem/issues/229)
**Estimated Time**: 3 hours
**Dependencies**: Story #227

### Implementation

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

                // Sort for consistent ordering
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
}
```

### Complete Story

```bash
./scripts/complete-story.sh 229
```

---

## Story #230: Snapshot Acquisition (Fast Path)

**GitHub Issue**: [#230](https://github.com/anibjoshi/in-mem/issues/230)
**Estimated Time**: 4 hours
**Dependencies**: Story #228

### Implementation

```rust
/// Snapshot of storage at a point in time
///
/// CRITICAL: Snapshot acquisition must be:
/// - Allocation-free (Arc bump only)
/// - Lock-free (atomic version load)
/// - O(1) (no data structure scan)
/// - < 500ns (RED FLAG if > 2µs)
pub struct Snapshot {
    version: u64,
    store: Arc<ShardedStore>,
}

impl Snapshot {
    pub fn get(&self, run_id: &RunId, key: &Key) -> Option<VersionedValue> {
        self.store.get(run_id, key)
    }

    pub fn version(&self) -> u64 {
        self.version
    }
}

impl ShardedStore {
    /// Create a snapshot
    ///
    /// FAST PATH: This must be < 500ns!
    pub fn snapshot(self: &Arc<Self>) -> Snapshot {
        Snapshot {
            version: self.version.load(Ordering::Acquire),
            store: Arc::clone(self),
        }
    }
}
```

### Validation

```bash
# Must verify < 500ns
~/.cargo/bin/cargo bench --bench m4_performance -- snapshot
```

### Complete Story

```bash
./scripts/complete-story.sh 230
```

---

## Story #231: Storage Migration Path

**GitHub Issue**: [#231](https://github.com/anibjoshi/in-mem/issues/231)
**Estimated Time**: 3 hours
**Dependencies**: Story #230

### Implementation

Create `crates/engine/src/storage/mod.rs`:

```rust
//! Storage layer for M4

mod sharded;

pub use sharded::{ShardedStore, Shard, Snapshot};

/// Storage trait for abstracting implementations
pub trait Storage: Send + Sync {
    fn get(&self, run_id: &RunId, key: &Key) -> Option<VersionedValue>;
    fn put(&self, run_id: &RunId, key: Key, value: VersionedValue);
    fn delete(&self, run_id: &RunId, key: &Key) -> Option<VersionedValue>;
    fn list(&self, run_id: &RunId, prefix: &[u8]) -> Vec<(Key, VersionedValue)>;
    fn apply(&self, write_set: &WriteSet) -> Result<()>;
}

impl Storage for ShardedStore {
    // Implement trait methods delegating to ShardedStore
}
```

### Complete Story

```bash
./scripts/complete-story.sh 231
```

---

## Epic 22 Completion Checklist

### 1. Final Validation

```bash
~/.cargo/bin/cargo test --workspace
~/.cargo/bin/cargo bench --bench m4_performance -- storage snapshot
```

### 2. Verify Deliverables

- [ ] ShardedStore with DashMap
- [ ] Get/Put/Delete operations
- [ ] List with sorting
- [ ] Snapshot < 500ns
- [ ] Different runs don't contend

### 3. Merge to Develop

```bash
git checkout develop
git merge --no-ff epic-22-sharded-storage -m "Epic 22: Sharded Storage complete"
git push origin develop
gh issue close 213 --comment "Epic 22 complete. All 5 stories delivered."
```
