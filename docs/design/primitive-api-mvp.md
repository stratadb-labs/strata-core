# Primitive API: MVP Scope

## Overview

The current primitive API has **152 public methods** across 6 primitives. This is excessive for an MVP. This document defines the minimal API surface needed for launch.

**Target: ~37 methods** (76% reduction)

## Current Issues

1. **Inconsistent data access**: ~50% of methods bypass `Database.transaction()` and directly access `db.storage()`
2. **API bloat**: Many convenience methods, batch operations, and advanced features
3. **No run validation**: Methods don't verify the run exists before operating

## MVP Principles

1. **Everything is scoped to a Run**: All data operations require a valid RunId. The run must exist before data can be written. This is enforced at the Database.transaction() level.
2. **One way to do things**: Remove duplicate methods (e.g., `get` vs `get_in_transaction`)
3. **No batch operations**: Users can loop; batch ops add complexity
4. **No search in primitives**: Search belongs in the intelligence layer
5. **Two-tier versioning**: `get()` for latest value, `getv()` for full history
6. **All operations through transactions**: Consistent data access path, enables run validation

---

## KVStore

**Current: 25 methods**

### Keep (6 methods)
| Method | Signature | Rationale |
|--------|-----------|-----------|
| `new` | `new(db: Arc<Database>) -> Self` | Constructor |
| `get` | `get(run_id, key) -> Option<Value>` | Latest value (fast path) |
| `getv` | `getv(run_id, key) -> Option<VersionedHistory<Value>>` | Full history, indexable |
| `put` | `put(run_id, key, value) -> Version` | Core write |
| `delete` | `delete(run_id, key) -> bool` | Core delete |
| `list` | `list(run_id, prefix) -> Vec<String>` | List keys |

### Remove
| Method | Reason |
|--------|--------|
| `database` | Internal implementation detail |
| `get_value` | Deprecated, use `get()` |
| `get_in_transaction` | Duplicate of `get` |
| `get_at` | Use `getv()[n]` instead |
| `history` | Replaced by `getv()` |
| `put_no_version` | Deprecated, use `put` |
| `put_with_ttl` | TTL - not MVP |
| `exists` | Use `get().is_some()` |
| `get_many` | Batch - not MVP |
| `get_many_map` | Batch - not MVP |
| `contains` | Duplicate of `exists` |
| `list_with_values` | Use `list` + `get` |
| `keys` | Duplicate of `list` |
| `scan` | Cursor pagination - not MVP |
| `search` | Belongs in intelligence layer |
| `transaction` | Users use Database.transaction() directly |
| `KVTransaction::*` | Internal to transaction |

---

## EventLog

**Current: 20 methods**

### Keep (5 methods)
| Method | Signature | Rationale |
|--------|-----------|-----------|
| `new` | `new(db: Arc<Database>) -> Self` | Constructor |
| `append` | `append(run_id, event_type, payload) -> u64` | Core write |
| `read` | `read(run_id, sequence) -> Option<Event>` | Read by sequence |
| `read_by_type` | `read_by_type(run_id, event_type) -> Vec<Event>` | Read stream |
| `len` | `len(run_id) -> u64` | Total count |

### Remove
| Method | Reason |
|--------|--------|
| `database` | Internal |
| `append_batch` | Batch - not MVP |
| `read_in_transaction` | Duplicate |
| `read_range` | Use `read_by_type` |
| `read_range_reverse` | Convenience - not MVP |
| `head` | Use `read_by_type` and take last |
| `is_empty` | Use `len() == 0` |
| `verify_chain` | Advanced integrity check - not MVP |
| `len_by_type` | Use `read_by_type().len()` |
| `latest_sequence_by_type` | Convenience - not MVP |
| `stream_info` | Metadata - not MVP |
| `head_by_type` | Convenience - not MVP |
| `stream_names` | Use application-level tracking |
| `event_types` | Duplicate of `stream_names` |
| `search` | Belongs in intelligence layer |

---

## StateCell

**Current: 14 methods**

### Keep (6 methods)
| Method | Signature | Rationale |
|--------|-----------|-----------|
| `new` | `new(db: Arc<Database>) -> Self` | Constructor |
| `init` | `init(run_id, name, value) -> Version` | Create cell |
| `read` | `read(run_id, name) -> Option<Value>` | Latest state (fast path) |
| `readv` | `readv(run_id, name) -> Option<VersionedHistory<Value>>` | Full history, indexable |
| `set` | `set(run_id, name, value) -> Version` | Core write |
| `cas` | `cas(run_id, name, expected, value) -> Version` | Compare-and-swap |

### Remove
| Method | Reason |
|--------|--------|
| `database` | Internal |
| `read_in_transaction` | Duplicate |
| `delete` | Cells are permanent in MVP |
| `exists` | Use `read().is_some()` |
| `list` | Track cells at application level |
| `history` | Replaced by `readv()` |
| `transition` | Convenience wrapper around `cas` |
| `transition_or_init` | Convenience - not MVP |
| `search` | Belongs in intelligence layer |

---

## JsonStore

**Current: 24 methods**

### Keep (7 methods)
| Method | Signature | Rationale |
|--------|-----------|-----------|
| `new` | `new(db: Arc<Database>) -> Self` | Constructor |
| `create` | `create(run_id, doc_id, value) -> Version` | Create document |
| `get` | `get(run_id, doc_id, path) -> Option<JsonValue>` | Latest value at path |
| `getv` | `getv(run_id, doc_id) -> Option<VersionedHistory<JsonValue>>` | Full doc history |
| `set` | `set(run_id, doc_id, path, value) -> Version` | Update (with path) |
| `delete` | `delete(run_id, doc_id) -> bool` | Delete document |
| `list` | `list(run_id, prefix, limit) -> Vec<String>` | List documents |

### Remove
| Method | Reason |
|--------|--------|
| `database` | Internal |
| `get_doc` | Duplicate of `get` with root path |
| `get_version` | Use `get` and extract version |
| `exists` | Use `get().is_some()` |
| `history` | Replaced by `getv()` |
| `delete_at_path` | Use `set` with null |
| `destroy` | Rename confusion, use `delete` |
| `merge` | Advanced JSON operation - not MVP |
| `cas` | Use StateCell for CAS patterns |
| `count` | Use `list().len()` |
| `batch_get` | Batch - not MVP |
| `batch_create` | Batch - not MVP |
| `array_push` | Convenience - not MVP |
| `increment` | Convenience - not MVP |
| `array_pop` | Convenience - not MVP |
| `query` | Belongs in intelligence layer |
| `search` | Belongs in intelligence layer |
| `touch` | Internal |

---

## VectorStore

**Current: 35 methods**

### Keep (7 methods)
| Method | Signature | Rationale |
|--------|-----------|-----------|
| `new` | `new(db: Arc<Database>) -> Self` | Constructor |
| `create_collection` | `create_collection(run_id, name, config) -> ()` | Create collection |
| `delete_collection` | `delete_collection(run_id, name) -> ()` | Delete collection |
| `list_collections` | `list_collections(run_id) -> Vec<CollectionInfo>` | List collections |
| `insert` | `insert(run_id, collection, key, embedding, metadata) -> ()` | Upsert vector |
| `get` | `get(run_id, collection, key) -> Option<VectorEntry>` | Get vector |
| `delete` | `delete(run_id, collection, key) -> bool` | Delete vector |
| `search` | `search(run_id, collection, query, k, filter) -> Vec<Match>` | KNN search |

### Remove
| Method | Reason |
|--------|--------|
| `database` | Internal |
| `recover` | Internal recovery |
| `get_collection` | Use `list_collections` |
| `collection_exists` | Use `list_collections` |
| `insert_with_source` | Advanced provenance - not MVP |
| `history` | Version history - not MVP |
| `get_at` | Historical - not MVP |
| `count` | Use application tracking |
| `list_keys` | Use application tracking |
| `scan` | Cursor pagination - not MVP |
| `search_simple` | Duplicate |
| `search_with_sources` | Advanced - not MVP |
| `search_response` | Internal |
| `search_with_budget` | Advanced - not MVP |
| `insert_batch` | Batch - not MVP |
| `insert_batch_with_source` | Batch - not MVP |
| `get_batch` | Batch - not MVP |
| `delete_batch` | Batch - not MVP |
| `get_key_and_metadata` | Use `get` |
| `get_key_metadata_source` | Advanced - not MVP |
| `ensure_collection_loaded` | Internal |
| `replay_*` | Internal recovery |
| `backends` | Internal |
| `db` | Internal |

---

## RunIndex

**Current: 34 methods**

Runs are central to Strata's data model. See [run-api-git-semantics.md](./run-api-git-semantics.md) for the full vision. For MVP, we include core CRUD plus git-like `fork` and `diff` operations.

### Keep (8 methods)
| Method | Signature | Rationale |
|--------|-----------|-----------|
| `new` | `new(db: Arc<Database>) -> Self` | Constructor |
| `create_run` | `create_run(name) -> RunMetadata` | Create run (git init) |
| `get_run` | `get_run(name) -> Option<RunMetadata>` | Get run info (git status) |
| `list_runs` | `list_runs() -> Vec<String>` | List all runs |
| `delete_run` | `delete_run(name) -> ()` | Delete run and ALL its data |
| `exists` | `exists(name) -> bool` | Check run exists (for validation) |
| `fork_run` | `fork_run(source, name) -> RunMetadata` | **NEW**: Copy run with all data (git fork) |
| `diff_runs` | `diff_runs(base, target) -> RunDiff` | **NEW**: Compare two runs (git diff) |

### New Methods to Implement

#### `fork_run` (Critical - fixes #780)
```rust
/// Fork a run, creating a complete copy of all its data.
/// The new run starts with an exact copy of source's KV, State, Events, JSON, Vectors.
/// Subsequent changes to source or fork do not affect each other.
pub fn fork_run(&self, source: &str, name: &str) -> Result<Versioned<RunMetadata>>
```

#### `diff_runs` (High priority)
```rust
/// Compare two runs and return their differences.
/// Returns added, removed, and modified entries across all primitives.
pub fn diff_runs(&self, base: &str, target: &str) -> Result<RunDiff>

pub struct RunDiff {
    pub added: Vec<DiffEntry>,    // In target but not base
    pub removed: Vec<DiffEntry>,  // In base but not target
    pub modified: Vec<DiffEntry>, // Different values
}

pub struct DiffEntry {
    pub primitive: PrimitiveType,  // KV, State, Event, JSON, Vector
    pub key: String,
    pub base_value: Option<Value>,
    pub target_value: Option<Value>,
}
```

### Remove
| Method | Reason |
|--------|--------|
| `database` | Internal |
| `create_run_with_options` | Use `create_run`, metadata not MVP |
| `count` | Use `list_runs().len()` |
| `update_status` | Status lifecycle - not MVP |
| `complete_run` | Status lifecycle - not MVP |
| `fail_run` | Status lifecycle - not MVP |
| `pause_run` | Status lifecycle - not MVP |
| `resume_run` | Status lifecycle - not MVP |
| `cancel_run` | Status lifecycle - not MVP |
| `query_by_status` | Query - not MVP |
| `query_by_tag` | Tags - not MVP |
| `get_child_runs` | Use application tracking |
| `archive_run` | Archival - not MVP |
| `add_tags` | Tags - not MVP |
| `remove_tags` | Tags - not MVP |
| `update_metadata` | Metadata - not MVP |
| `search` | Belongs in intelligence layer |
| `export_run` | Import/export - not MVP |
| `export_run_with_options` | Import/export - not MVP |
| `import_run` | Import/export - not MVP |
| `verify_bundle` | Import/export - not MVP |
| `RunStatus::*` | Status enum - not MVP |
| `RunMetadata::*` | Keep minimal fields only |

### Post-MVP Run Operations
| Method | Priority | Description |
|--------|----------|-------------|
| `merge_runs` | Medium | Combine changes from two runs |
| `reset_run` | Medium | Rollback run to earlier state |
| `export_run` | Low | Export to portable bundle |
| `import_run` | Low | Import from bundle |

---

## Summary

### MVP API Surface

| Primitive | MVP Methods | Removed | Notes |
|-----------|-------------|---------|-------|
| KVStore | 6 | 19 | Core CRUD + list + `getv` |
| EventLog | 5 | 15 | Core CRUD + read stream |
| StateCell | 6 | 8 | Core CRUD + CAS + `readv` |
| JsonStore | 7 | 17 | Core CRUD + path access + `getv` |
| VectorStore | 8 | 27 | Core CRUD + search |
| RunIndex | 8 | 26 | Core CRUD + **fork + diff** |
| **Total** | **40** | **112** | ~74% reduction |

### Architectural Changes Required

1. **All methods must go through `Database.transaction()`**
   - Remove direct `db.storage()` access in primitives
   - Provides single enforcement point for run validation

2. **Run validation in transaction**
   - `Database.transaction(run_id, ...)` checks run exists
   - Cache of valid runs for performance
   - Default run created on database open

3. **Remove `database()` accessor from primitives**
   - Prevents users from bypassing the API

4. **Move search methods to intelligence layer**
   - Primitives do CRUD only
   - Intelligence layer handles search across primitives

5. **Versioning API**

   Two methods for reading data:

   | Method | Returns | Use Case |
   |--------|---------|----------|
   | `get()` | Latest value only (`Value`) | Fast reads, most common |
   | `getv()` | Full history, indexable (`VersionedHistory<Value>`) | Need previous versions |

   ```rust
   let val = db.kv.get("key")?;      // Just the value
   let hist = db.kv.getv("key")?;    // hist[0] = latest, hist[1] = previous
   ```

   See [versioning-api.md](./versioning-api.md) for full design.

---

## Transaction Enforcement Audit

All 40 MVP methods must go through `Database.transaction()`. Currently ~15 methods bypass transactions by calling `db.storage()` directly.

### Current State by Primitive

#### KVStore (6 methods)
| Method | Current | Required Change |
|--------|---------|-----------------|
| `get` | `db.storage().create_snapshot()` | Use `db.transaction()` |
| `getv` | `db.storage().get_history()` | Use `db.transaction()` |
| `put` | `db.transaction()` | None |
| `delete` | `db.transaction()` | None |
| `list` | `db.transaction()` | None |

#### StateCell (6 methods)
| Method | Current | Required Change |
|--------|---------|-----------------|
| `init` | `db.transaction()` | None |
| `read` | `db.storage().create_snapshot()` | Use `db.transaction()` |
| `readv` | `db.storage().get_history()` | Use `db.transaction()` |
| `set` | `db.transaction()` | None |
| `cas` | `db.transaction()` | None |

#### JsonStore (7 methods)
| Method | Current | Required Change |
|--------|---------|-----------------|
| `create` | `db.transaction()` | None |
| `get` | `db.storage().create_snapshot()` | Use `db.transaction()` |
| `getv` | `db.storage().get_history()` | Use `db.transaction()` |
| `set` | `db.transaction()` | None |
| `delete` | `db.transaction()` | None |
| `list` | `db.transaction()` | None |

#### EventLog (5 methods)
| Method | Current | Required Change |
|--------|---------|-----------------|
| `append` | `db.transaction()` | None |
| `read` | `db.storage().create_snapshot()` | Use `db.transaction()` |
| `read_by_type` | `db.storage().create_snapshot()` | Use `db.transaction()` |
| `len` | `db.storage().create_snapshot()` | Use `db.transaction()` |

#### VectorStore (8 methods)
| Method | Current | Required Change |
|--------|---------|-----------------|
| `create_collection` | Mixed | Audit needed |
| `delete_collection` | Mixed | Audit needed |
| `list_collections` | `db.storage().create_snapshot()` | Use `db.transaction()` |
| `insert` | Mixed | Audit needed |
| `get` | `db.storage().create_snapshot()` | Use `db.transaction()` |
| `delete` | Mixed | Audit needed |
| `search` | `db.storage().create_snapshot()` | Use `db.transaction()` |

#### RunIndex (8 methods)
| Method | Current | Required Change |
|--------|---------|-----------------|
| `create_run` | `db.transaction()` | None |
| `get_run` | Mixed | Audit needed |
| `list_runs` | Mixed | Audit needed |
| `delete_run` | `db.transaction()` | None |
| `exists` | `db.storage().create_snapshot()` | Use `db.transaction()` |
| `fork_run` | New method | Implement with `db.transaction()` |
| `diff_runs` | New method | Implement with `db.transaction()` |

### Why Reads Bypassed Transactions

The bypasses were introduced as "fast path" optimizations:
- Reads don't need write-set tracking
- Reads don't need WAL append
- Direct snapshot access is faster

However, this breaks:
1. **Run validation** - No enforcement that the run exists
2. **Consistency** - Different code paths for reads vs writes
3. **Future extensibility** - Can't add read hooks, auditing, etc.

### Fix Strategy

Replace `db.storage().create_snapshot()` with `db.transaction()` for all reads:

```rust
// Before (bypasses transaction)
pub fn get(&self, run_id: &RunId, key: &str) -> Result<Option<Value>> {
    let snapshot = self.db.storage().create_snapshot();
    let storage_key = self.key_for(run_id, key);
    Ok(snapshot.get(&storage_key)?.map(|v| v.value))
}

// After (uses transaction)
pub fn get(&self, run_id: &RunId, key: &str) -> Result<Option<Value>> {
    self.db.transaction(*run_id, |txn| {
        let storage_key = self.key_for(run_id, key);
        txn.get(&storage_key)
    })
}
```

### Read-Only Transaction Optimization

For performance, add a read-only transaction mode:

```rust
impl Database {
    /// Read-only transaction - no write-set, no WAL, but still validates run
    pub fn read_transaction<T, F>(&self, run_id: RunId, f: F) -> Result<T>
    where
        F: FnOnce(&ReadOnlyContext) -> Result<T>,
    {
        // 1. Validate run exists
        self.validate_run(&run_id)?;

        // 2. Create snapshot
        let snapshot = self.storage.create_snapshot();

        // 3. Execute read-only closure
        let ctx = ReadOnlyContext::new(run_id, snapshot);
        f(&ctx)
    }
}
```

This preserves the performance benefits while enforcing the run validation invariant.

---

## Transaction System Audit

Traced `db.transaction()` to verify it enforces run validation.

### Current Call Chain

```
db.transaction(run_id, closure)
    ├── check_accepting()           ✓ Checks if DB is open
    ├── begin_transaction(run_id)   ✗ NO run validation
    │   ├── coordinator.next_txn_id()
    │   ├── storage.create_snapshot()
    │   └── TransactionPool::acquire()
    ├── closure(&mut txn)           User code runs here
    ├── run_single_attempt()
    │   └── commit_internal()       ✗ NO run validation
    │       ├── acquire commit_lock per run
    │       ├── validate (conflict detection only)
    │       ├── allocate commit version
    │       ├── write to WAL
    │       └── apply to storage
    └── end_transaction()
```

### Finding: Run Validation is Missing

| Check | Location | Status |
|-------|----------|--------|
| Database accepting transactions | `check_accepting()` | ✓ Implemented |
| **Run exists** | nowhere | ✗ **NOT IMPLEMENTED** |
| Conflict detection | `validate_transaction()` | ✓ Implemented |
| WAL write | `commit_internal()` | ✓ Implemented |
| Storage apply | `commit_internal()` | ✓ Implemented |

The `run_id` is currently used only for:
- Commit lock namespacing (`commit_locks: DashMap<RunId, Mutex>`)
- WAL recording

**Any arbitrary `RunId` is accepted without validation.**

### Required Changes to Database

#### 1. Add valid_runs cache

```rust
pub struct Database {
    // ... existing fields ...

    /// Cache of valid run IDs for O(1) validation
    valid_runs: DashSet<RunId>,
}
```

#### 2. Add validate_run method

```rust
impl Database {
    /// Validate that a run exists
    fn validate_run(&self, run_id: &RunId) -> Result<()> {
        // Allow the global run (used by RunIndex itself)
        if *run_id == global_run_id() {
            return Ok(());
        }

        if !self.valid_runs.contains(run_id) {
            return Err(StrataError::invalid_input(
                format!("Run {:?} does not exist", run_id)
            ));
        }
        Ok(())
    }
}
```

#### 3. Call validate_run in transaction methods

```rust
pub fn transaction<F, T>(&self, run_id: RunId, f: F) -> Result<T> {
    self.check_accepting()?;
    self.validate_run(&run_id)?;  // ADD THIS
    let mut txn = self.begin_transaction(run_id);
    // ... rest unchanged
}

pub fn transaction_with_version<F, T>(&self, run_id: RunId, f: F) -> Result<(T, u64)> {
    self.check_accepting()?;
    self.validate_run(&run_id)?;  // ADD THIS
    // ... rest unchanged
}

pub fn read_transaction<F, T>(&self, run_id: RunId, f: F) -> Result<T> {
    self.validate_run(&run_id)?;  // ADD THIS
    // ... rest unchanged
}
```

#### 4. RunIndex maintains the cache

```rust
// In RunIndex::create_run()
pub fn create_run(&self, name: &str) -> Result<Versioned<RunMetadata>> {
    let run_id = RunId::new();
    // ... create run in storage ...

    // Add to valid_runs cache
    self.db.valid_runs.insert(run_id);

    Ok(metadata)
}

// In RunIndex::delete_run()
pub fn delete_run(&self, name: &str) -> Result<()> {
    let run_id = self.get_run_id(name)?;
    // ... delete from storage ...

    // Remove from valid_runs cache
    self.db.valid_runs.remove(&run_id);

    Ok(())
}
```

#### 5. Recovery populates the cache

```rust
// During WAL replay or snapshot recovery
fn recover_run(&self, run_id: RunId) {
    self.valid_runs.insert(run_id);
}
```

### Default Run

Per the MVP principles, a default run should be created on database open:

```rust
impl Database {
    pub fn open(path: &Path) -> Result<Self> {
        let db = Self::open_internal(path)?;

        // Create default run if it doesn't exist
        let run_index = RunIndex::new(Arc::new(db.clone()));
        if !run_index.exists("default")? {
            run_index.create_run("default")?;
        }

        Ok(db)
    }
}
```

---

## Migration Path

### Phase 1: Deprecate
- Mark removed methods as `#[deprecated]`
- Point to replacement patterns in deprecation message

### Phase 2: Make private
- Change `pub fn` to `pub(crate) fn` for internal methods
- Keep deprecated public methods as thin wrappers

### Phase 3: Remove
- Delete deprecated methods
- Clean up internal-only code

---

## Post-MVP Candidates

Methods that could return after MVP based on user feedback:

### High Priority (likely soon after MVP)
1. **Run merge** - Combine changes from multiple runs (git merge)
2. **Run reset** - Rollback to earlier state (git reset)
3. **Batch operations** - If single-item performance is insufficient

### Medium Priority
4. **Run lifecycle states** - If workflow management is needed
5. **Tags and metadata** - If run organization is needed

### Lower Priority
6. **Import/export** - If data portability is needed
7. **TTL** - If automatic expiration is needed
8. **Advanced queries** - query_by_status, query_by_tag

See [run-api-git-semantics.md](./run-api-git-semantics.md) for the full git-like Run API vision.
