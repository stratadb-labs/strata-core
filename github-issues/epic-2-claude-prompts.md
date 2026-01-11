# Epic 2: Storage Layer - Claude Prompts

**Epic Branch**: `epic-2-storage-layer`

**Dependencies**: Epic 1 must be complete and merged to develop ✅

---

## Parallelization Strategy

### Phase 1: Foundation (Sequential) - Story #12
**Story #12 (UnifiedStore)** must complete first - it implements the Storage trait and blocks all other stories.

**Estimated**: 5-6 hours

### Phase 2: Indices and Snapshot (3 Claudes in Parallel) - Stories #13, #14, #15
After #12 merges to epic branch, these can run in parallel:
- Story #13: Secondary indices (run_index, type_index)
- Story #14: TTL index and cleanup
- Story #15: ClonedSnapshotView implementation

**Estimated**: 4-5 hours wall time (parallel)

### Phase 3: Comprehensive Testing (Sequential) - Story #16
After #13-15 merge, final story adds comprehensive storage tests.

**Estimated**: 3-4 hours

**Total Epic 2**: ~13-15 hours sequential, ~9-11 hours with 3 Claudes in parallel

---

## Prompt 1: Story #12 - UnifiedStore (MUST DO FIRST)

### Context
You are implementing Story #12 for the in-mem database project. Epic 1 (Workspace & Core Types) is complete with Storage and SnapshotView traits defined. You are now implementing the MVP storage backend.

### Your Task
Implement UnifiedStore with BTreeMap backend and version management in the `in-mem-storage` crate.

### Getting Started

1. **Clone the repository** (if not already cloned):
   ```bash
   git clone https://github.com/anibjoshi/in-mem.git
   cd in-mem
   ```

2. **Start the story**:
   ```bash
   ./scripts/start-story.sh 2 12 unified-store
   ```

   This automatically:
   - Creates/checks out epic-2-storage-layer branch
   - Creates epic-2-story-12-unified-store branch
   - Sets up remote tracking

3. **Read context**:
   ```bash
   gh issue view 12
   ```

   Also read:
   - `docs/architecture/M1_ARCHITECTURE.md` - Complete M1 specification
   - `docs/development/TDD_METHODOLOGY.md` - Testing approach
   - `crates/core/src/traits.rs` - Storage trait you're implementing
   - `crates/core/src/types.rs` - Key, RunId, Namespace, TypeTag types
   - `crates/core/src/value.rs` - Value and VersionedValue types

### Implementation Steps

#### Step 1: Update Cargo.toml
Edit `crates/storage/Cargo.toml`:
```toml
[dependencies]
in-mem-core = { path = "../core" }
parking_lot = "0.12"  # More efficient RwLock
```

#### Step 2: Write Tests First (TDD)
Create tests in `crates/storage/src/unified.rs` (#[cfg(test)] module):

1. `test_store_creation` - empty store, current_version=0
2. `test_put_and_get` - basic write and read
3. `test_version_monotonicity` - versions increase 1,2,3...
4. `test_get_versioned` - respects max_version parameter
5. `test_delete` - removes key correctly
6. `test_ttl_expiration` - expired values return None
7. `test_scan_prefix` - BTreeMap range query
8. `test_scan_by_run` - filters by run_id
9. `test_concurrent_writes` - 10 threads × 100 writes = 1000 versions

#### Step 3: Implement UnifiedStore
Create `crates/storage/src/unified.rs`:

```rust
use in_mem_core::{
    error::Result,
    traits::Storage,
    types::{Key, RunId},
    value::{Value, VersionedValue},
};
use parking_lot::RwLock;
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

pub struct UnifiedStore {
    data: Arc<RwLock<BTreeMap<Key, VersionedValue>>>,
    global_version: AtomicU64,
}

impl UnifiedStore {
    pub fn new() -> Self {
        Self {
            data: Arc::new(RwLock::new(BTreeMap::new())),
            global_version: AtomicU64::new(1),
        }
    }

    fn next_version(&self) -> u64 {
        self.global_version.fetch_add(1, Ordering::SeqCst)
    }

    fn is_expired(value: &VersionedValue) -> bool {
        value.is_expired()
    }
}

impl Storage for UnifiedStore {
    // Implement all trait methods
    // See GitHub issue #12 for complete implementation
}

#[cfg(test)]
mod tests {
    // Add all 9 tests here
}
```

#### Step 4: Update lib.rs
Edit `crates/storage/src/lib.rs`:
```rust
pub mod unified;

pub use unified::UnifiedStore;
```

#### Step 5: Verify Locally
```bash
# Build
cargo build -p in-mem-storage

# Test
cargo test -p in-mem-storage

# Clippy
cargo clippy -p in-mem-storage -- -D warnings

# Format
cargo fmt -p in-mem-storage
```

### When Complete

Run the completion script:
```bash
./scripts/complete-story.sh 12
```

This automatically:
- Runs all quality checks (build, test, clippy, format)
- Pushes your branch
- Creates PR to epic-2-storage-layer
- Generates PR description with changes

Then comment on issue #12 with your PR link and notify other Claudes that Story #12 is ready for stories #13-15 to begin.

### Critical Requirements

- [ ] All 9 unit tests pass
- [ ] Version numbers monotonically increase (1, 2, 3, ...)
- [ ] TTL expiration works
- [ ] scan_prefix returns only matching keys
- [ ] scan_by_run filters by run_id
- [ ] Concurrent writes work (10 threads test)
- [ ] 100% test coverage for unified.rs
- [ ] No clippy warnings
- [ ] Code formatted with cargo fmt

### Notes

- **Known limitation**: Overwrites old versions (no version history). Acceptable for MVP.
- **Known bottleneck**: RwLock will contend under high load. Storage trait allows future replacement.
- Use `parking_lot::RwLock` (more efficient than std)
- TTL expiration is logical (filtered at read time)

---

## Prompt 2: Story #13 - Secondary Indices (WAIT FOR #12)

### Context
You are implementing Story #13 for the in-mem database project. Story #12 (UnifiedStore) is complete. You are now adding secondary indices for efficient queries.

### Your Task
Add run_index and type_index secondary indices to UnifiedStore for efficient run-scoped and type-scoped queries.

### Getting Started

1. **WAIT FOR STORY #12 TO MERGE** to epic-2-storage-layer branch

2. **Start your story**:
   ```bash
   ./scripts/start-story.sh 2 13 secondary-indices
   ```

3. **Read context**:
   ```bash
   gh issue view 13
   ```

   Also read:
   - `crates/storage/src/unified.rs` - UnifiedStore from #12
   - `crates/core/src/types.rs` - RunId, TypeTag types

### Implementation Steps

#### Step 1: Create index.rs
Create `crates/storage/src/index.rs`:

```rust
use in_mem_core::types::{Key, RunId, TypeTag};
use std::collections::{HashMap, HashSet};

pub struct RunIndex {
    index: HashMap<RunId, HashSet<Key>>,
}

impl RunIndex {
    pub fn new() -> Self { /* ... */ }
    pub fn insert(&mut self, run_id: RunId, key: Key) { /* ... */ }
    pub fn remove(&mut self, run_id: RunId, key: &Key) { /* ... */ }
    pub fn get_keys(&self, run_id: &RunId) -> Option<&HashSet<Key>> { /* ... */ }
}

pub struct TypeIndex {
    index: HashMap<TypeTag, HashSet<Key>>,
}

impl TypeIndex {
    pub fn new() -> Self { /* ... */ }
    pub fn insert(&mut self, type_tag: TypeTag, key: Key) { /* ... */ }
    pub fn remove(&mut self, type_tag: TypeTag, key: &Key) { /* ... */ }
    pub fn get_keys(&self, type_tag: &TypeTag) -> Option<&HashSet<Key>> { /* ... */ }
}
```

#### Step 2: Modify UnifiedStore
Update `crates/storage/src/unified.rs`:
- Add `run_index: RunIndex` field
- Add `type_index: TypeIndex` field
- Update `put()` to insert into both indices
- Update `delete()` to remove from both indices
- Update `scan_by_run()` to use run_index (faster)
- Add `scan_by_type()` method

#### Step 3: Write Tests
Add to `index.rs`:
1. `test_run_index_insert_and_get`
2. `test_run_index_remove`
3. `test_type_index_insert_and_get`
4. `test_type_index_remove`

Add to `unified.rs`:
5. `test_scan_by_run_uses_index`
6. `test_scan_by_type`
7. `test_indices_stay_consistent`

#### Step 4: Update lib.rs
```rust
pub mod unified;
pub mod index;

pub use unified::UnifiedStore;
pub use index::{RunIndex, TypeIndex};
```

#### Step 5: Verify
```bash
cargo test -p in-mem-storage
cargo clippy -p in-mem-storage -- -D warnings
cargo fmt -p in-mem-storage
```

### When Complete
```bash
./scripts/complete-story.sh 13
```

### Critical Requirements

- [ ] RunIndex and TypeIndex implemented
- [ ] put() updates both indices atomically
- [ ] delete() removes from both indices
- [ ] scan_by_run() uses index (O(run size) not O(total))
- [ ] scan_by_type() works
- [ ] All 7 tests pass
- [ ] Indices stay consistent

---

## Prompt 3: Story #14 - TTL Index (WAIT FOR #12)

### Context
You are implementing Story #14 for the in-mem database project. Story #12 (UnifiedStore) is complete. You are now adding TTL index for efficient cleanup.

### Your Task
Add TTL index to UnifiedStore for efficient TTL expiration cleanup.

### Getting Started

1. **WAIT FOR STORY #12 TO MERGE**

2. **Start your story**:
   ```bash
   ./scripts/start-story.sh 2 14 ttl-index
   ```

3. **Read context**:
   ```bash
   gh issue view 14
   ```

### Implementation Steps

#### Step 1: Create ttl.rs
Create `crates/storage/src/ttl.rs`:

```rust
use in_mem_core::types::Key;
use std::collections::{BTreeMap, HashSet};
use std::time::Instant;

pub struct TTLIndex {
    index: BTreeMap<Instant, HashSet<Key>>,
}

impl TTLIndex {
    pub fn new() -> Self { /* ... */ }
    pub fn insert(&mut self, expiry: Instant, key: Key) { /* ... */ }
    pub fn remove(&mut self, expiry: Instant, key: &Key) { /* ... */ }
    pub fn find_expired(&self, now: Instant) -> Vec<Key> { /* ... */ }
}
```

#### Step 2: Modify UnifiedStore
Update `crates/storage/src/unified.rs`:
- Add `ttl_index: TTLIndex` field
- Update `put()` with TTL to insert into ttl_index
- Update `delete()` to remove from ttl_index
- Update `find_expired_keys()` to use index

#### Step 3: Add TTLCleaner
Create `crates/storage/src/cleaner.rs`:
```rust
use crate::unified::UnifiedStore;
use std::sync::Arc;
use std::time::Duration;

pub struct TTLCleaner {
    store: Arc<UnifiedStore>,
    check_interval: Duration,
}

impl TTLCleaner {
    pub fn start(store: Arc<UnifiedStore>) -> std::thread::JoinHandle<()> {
        // Background thread that calls find_expired_keys() and deletes
    }
}
```

#### Step 4: Write Tests
1. `test_ttl_index_insert_and_find_expired`
2. `test_ttl_index_remove`
3. `test_find_expired_keys_uses_index`
4. `test_ttl_cleaner_deletes_expired`

#### Step 5: Update lib.rs
```rust
pub mod unified;
pub mod ttl;
pub mod cleaner;

pub use unified::UnifiedStore;
pub use ttl::TTLIndex;
pub use cleaner::TTLCleaner;
```

#### Step 6: Verify
```bash
cargo test -p in-mem-storage
cargo clippy -p in-mem-storage -- -D warnings
cargo fmt -p in-mem-storage
```

### When Complete
```bash
./scripts/complete-story.sh 14
```

### Critical Requirements

- [ ] TTLIndex using BTreeMap<Instant, HashSet<Key>>
- [ ] find_expired_keys() uses index (O(expired) not O(total))
- [ ] TTLCleaner background task works
- [ ] Cleanup uses transactions (not direct mutation)
- [ ] All tests pass

---

## Prompt 4: Story #15 - ClonedSnapshotView (WAIT FOR #12)

### Context
You are implementing Story #15 for the in-mem database project. Story #12 (UnifiedStore) is complete. You are now implementing the MVP snapshot mechanism.

### Your Task
Implement ClonedSnapshotView that creates version-bounded views of storage for transactions.

### Getting Started

1. **WAIT FOR STORY #12 TO MERGE**

2. **Start your story**:
   ```bash
   ./scripts/start-story.sh 2 15 snapshot-view
   ```

3. **Read context**:
   ```bash
   gh issue view 15
   ```

   Also read:
   - `crates/core/src/traits.rs` - SnapshotView trait
   - `crates/storage/src/unified.rs` - UnifiedStore

### Implementation Steps

#### Step 1: Create snapshot.rs
Create `crates/storage/src/snapshot.rs`:

```rust
use in_mem_core::{
    traits::SnapshotView,
    types::Key,
    value::VersionedValue,
};
use std::collections::BTreeMap;
use std::sync::Arc;

pub struct ClonedSnapshotView {
    version: u64,
    data: Arc<BTreeMap<Key, VersionedValue>>,
}

impl ClonedSnapshotView {
    pub fn new(version: u64, data: BTreeMap<Key, VersionedValue>) -> Self {
        Self {
            version,
            data: Arc::new(data),
        }
    }
}

impl SnapshotView for ClonedSnapshotView {
    fn get(&self, key: &Key) -> Option<VersionedValue> { /* ... */ }
    fn scan_prefix(&self, prefix: &Key) -> Vec<(Key, VersionedValue)> { /* ... */ }
    fn version(&self) -> u64 { self.version }
}
```

#### Step 2: Add create_snapshot to UnifiedStore
Update `crates/storage/src/unified.rs`:

```rust
impl UnifiedStore {
    pub fn create_snapshot(&self) -> ClonedSnapshotView {
        let data = self.data.read();
        let version = self.current_version();
        ClonedSnapshotView::new(version, data.clone())
    }
}
```

#### Step 3: Write Tests
Add to `snapshot.rs`:
1. `test_snapshot_creation`
2. `test_snapshot_get`
3. `test_snapshot_isolation` - writes after snapshot don't appear
4. `test_snapshot_scan_prefix`
5. `test_snapshot_is_immutable`

#### Step 4: Update lib.rs
```rust
pub mod unified;
pub mod snapshot;

pub use unified::UnifiedStore;
pub use snapshot::ClonedSnapshotView;
```

#### Step 5: Verify
```bash
cargo test -p in-mem-storage
cargo clippy -p in-mem-storage -- -D warnings
cargo fmt -p in-mem-storage
```

### When Complete
```bash
./scripts/complete-story.sh 15
```

### Critical Requirements

- [ ] ClonedSnapshotView implements SnapshotView trait
- [ ] create_snapshot() clones BTreeMap at specific version
- [ ] Snapshots are isolated (writes don't appear)
- [ ] All 5 tests pass
- [ ] No clippy warnings

---

## Prompt 5: Story #16 - Comprehensive Storage Tests (DO LAST)

### Context
You are implementing Story #16 for the in-mem database project. Stories #12-15 are complete. You are now adding comprehensive integration tests for the storage layer.

### Your Task
Add comprehensive storage integration tests covering all edge cases, concurrent access, and stress scenarios.

### Getting Started

1. **WAIT FOR STORIES #12, #13, #14, #15 TO MERGE**

2. **Start your story**:
   ```bash
   ./scripts/start-story.sh 2 16 storage-tests
   ```

3. **Read context**:
   ```bash
   gh issue view 16
   ```

   Also review all prior implementations (#12-15)

### Implementation Steps

#### Step 1: Create Integration Test File
Create `crates/storage/tests/integration_tests.rs`:

**Edge Cases**:
- Empty keys, empty values
- Very large values (MB-sized)
- Unicode keys, binary keys
- Maximum version number (u64::MAX)

**Concurrent Access**:
- 100 threads × 1000 writes
- Read-heavy workload (90% reads, 10% writes)
- Write-heavy workload (10% reads, 90% writes)
- Mixed workload with deletes

**TTL and Expiration**:
- Expired values don't appear in scans
- find_expired_keys is efficient
- TTL cleanup doesn't race with writes

**Snapshot Isolation**:
- Snapshots don't see later writes
- Multiple concurrent snapshots work
- Large snapshot doesn't crash

**Index Consistency**:
- After 10000 random operations, indices match main storage
- Scan via index matches scan via full iteration
- Delete removes from all indices

**Version Ordering**:
- Versions are globally monotonic
- No version collisions under heavy concurrency
- current_version() is always accurate

#### Step 2: Create Stress Tests
Create `crates/storage/tests/stress_tests.rs`:
- Insert 1 million keys
- Scan with 100000 results
- Concurrent snapshot creation under load

#### Step 3: Verify Coverage
```bash
cargo tarpaulin -p in-mem-storage --out Html
open tarpaulin-report.html
# Ensure ≥85% coverage
```

#### Step 4: Run All Tests
```bash
cargo test -p in-mem-storage --all
cargo test -p in-mem-storage --all --release
cargo clippy -p in-mem-storage -- -D warnings
cargo fmt -p in-mem-storage
```

### When Complete
```bash
./scripts/complete-story.sh 16
```

### Critical Requirements

- [ ] ≥85% test coverage for storage layer
- [ ] All edge cases tested
- [ ] Concurrent access tests pass (no data races)
- [ ] TTL expiration tests pass
- [ ] Snapshot isolation tests pass
- [ ] Index consistency tests pass
- [ ] Stress tests pass (1M keys, 100K scan results)
- [ ] All tests pass in release mode
- [ ] No clippy warnings

---

## Coordination Notes

### For Claude Working on Story #12
- You are **blocking** stories #13, #14, #15
- **Prioritize completion** - get your PR merged ASAP
- After running `./scripts/complete-story.sh 12`, comment on issues #13, #14, #15: "Story #12 merged, you can start"

### For Claudes Working on Stories #13, #14, #15
- **Wait for story #12 PR to merge** to epic branch
- You can work in **parallel** with each other (different files)
- Use `./scripts/start-story.sh 2 <story-num> <description>` to begin
- Use `./scripts/complete-story.sh <story-num>` when done
- All three must merge before #16 can start

### For Claude Working on Story #16
- **Wait for stories #12, #13, #14, #15 to merge**
- Your tests should cover all prior implementations
- Focus on integration tests (multiple components together)
- Use `./scripts/start-story.sh 2 16 storage-tests` to begin
- Use `./scripts/complete-story.sh 16` when done

---

## Epic 2 Completion

After all 5 stories merge to `epic-2-storage-layer`:

1. Run epic review:
   ```bash
   ./scripts/review-epic.sh 2
   ```

2. Fill out `docs/milestones/EPIC_2_REVIEW.md`

3. If approved, merge to develop:
   ```bash
   git checkout develop
   git merge epic-2-storage-layer --no-ff
   git push origin develop
   ```

4. Tag release:
   ```bash
   git tag epic-2-complete
   git push origin epic-2-complete
   ```

5. Close epic issue:
   ```bash
   gh issue close 2
   ```

---

**Repository**: https://github.com/anibjoshi/in-mem
**Epic Branch**: `epic-2-storage-layer`
**Epic Issue**: #2
