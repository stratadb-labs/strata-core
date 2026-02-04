# Phase 2b: API Consistency Audit

Date: 2026-02-04
Status: Complete

## Summary

The codebase has **strong architectural consistency** (stateless facades, space-scoped operations, transactional semantics) but exhibits **naming and return type inconsistencies** across primitives that should be addressed for MVP.

**MVP Readiness: CONDITIONAL** — 4 issues identified, 2 are blocking for a clean public API.

---

## 1. Cross-Primitive Operation Matrix

| Operation | KV | JSON | Event | State | Vector |
|-----------|-----|------|-------|-------|--------|
| **Create** | -- | `create()` | -- | `init()` | `create_collection()` |
| **Read/Get** | `get()` | `get()` | `read()` | `read()` | `get()` |
| **Write** | `put()` | `set()` | `append()` | `set()` | `upsert()` |
| **Delete** | `delete()` | `destroy()`/`delete_at_path()` | -- | `delete()` | `delete()` |
| **Exists** | -- | `exists()` | -- | -- | -- |
| **List/Scan** | `list()` | `list()` | `read_by_type()` | `list()` | `list_collections()` |
| **Versioned Read** | `get_versioned()` | `get_versioned()` | -- | `read_versioned()` | -- |
| **History** | `getv()` | `getv()` | -- | `readv()` | -- |
| **Search** | -- | -- | -- | -- | `search()` |

### Key Observations

- KV and JSON are the most complete and consistent pair
- Event lacks delete, exists, versioned read, and history
- Vector lacks versioned read and history
- Only JSON implements `exists()`

---

## 2. Naming Inconsistencies

### Read Operation: `get()` vs `read()`

| Primitive | Method | Convention |
|-----------|--------|------------|
| KV | `get()` | `get` |
| JSON | `get()` | `get` |
| Event | `read()` | `read` |
| State | `read()` | `read` |
| Vector | `get()` | `get` |

**3 use `get()`, 2 use `read()`** — no clear majority convention.

**Locations**:
- `crates/engine/src/primitives/event.rs:415` — uses `read()`
- `crates/engine/src/primitives/state.rs:134` — uses `read()`

### History Operation: `getv()` vs `readv()`

| Primitive | Method |
|-----------|--------|
| KV | `getv()` |
| JSON | `getv()` |
| State | `readv()` |
| Event | -- |
| Vector | -- |

State uses `readv()` while KV/JSON use `getv()`.

---

## 3. Return Type Inconsistencies

### Write Operations

| Primitive | Method | Returns |
|-----------|--------|---------|
| KV | `put()` | `Version` |
| JSON | `create()` | `Version` |
| JSON | `set()` | `Version` |
| Event | `append()` | `Version` |
| State | `init()` | **`Versioned<Version>`** |
| State | `set()` | **`Versioned<Version>`** |
| State | `cas()` | **`Versioned<Version>`** |
| Vector | `upsert()` | `Version` |

**State wraps its return in `Versioned<Version>` while all other primitives return bare `Version`.**

Location: `crates/engine/src/primitives/state.rs:107-320`

### Read Operations

| Primitive | Method | Returns |
|-----------|--------|---------|
| KV | `get()` | `Option<Value>` |
| JSON | `get()` | `Option<JsonValue>` |
| Event | `read()` | `Option<Versioned<Event>>` |
| State | `read()` | `Option<Value>` |
| Vector | `get()` | `Option<VersionedVectorData>` |

Event always returns `Versioned<Event>`, Vector returns `VersionedVectorData`, while KV/JSON/State return unwrapped values by default.

---

## 4. Space-Scoped Operations

**Status: CONSISTENT** — All 5 primitives properly accept `(branch_id, space, ...)` parameters. Example signatures:

```rust
// KV
fn get(&self, branch_id: &BranchId, space: &str, key: &str) -> ...
// JSON
fn create(&self, branch_id: &BranchId, space: &str, doc_id: &str, ...) -> ...
// Event
fn append(&self, branch_id: &BranchId, space: &str, event_type: &str, ...) -> ...
// State
fn init(&self, branch_id: &BranchId, space: &str, name: &str, ...) -> ...
// Vector
fn create_collection(&self, branch_id: BranchId, space: &str, name: &str, ...) -> ...
```

---

## 5. Transaction Extension Traits

File: `crates/engine/src/primitives/extensions.rs`

| Trait | Methods Available | Missing vs Standalone |
|-------|-------------------|----------------------|
| `KVStoreExt` | `kv_get`, `kv_put`, `kv_delete` | Complete |
| `JsonStoreExt` | `json_get`, `json_set`, `json_create` | Missing: `delete_at_path`, `destroy` |
| `EventLogExt` | `event_append`, `event_read` | Missing: `read_by_type` |
| `StateCellExt` | `state_read`, `state_cas`, `state_set` | Missing: `init` |
| `VectorStoreExt` | `vector_get`, `vector_insert` | Missing: collection mgmt, search, delete |

**Impact**: Users cannot perform JSON deletes, Event type queries, State initialization, or Vector management within cross-primitive transactions.

---

## 6. BranchHandle Consistency

File: `crates/engine/src/primitives/branch/handle.rs`

| Handle | Methods |
|--------|---------|
| `KvHandle` | `get()`, `put()`, `delete()`, `exists()` |
| `EventHandle` | `append()`, `read()` |
| `StateHandle` | `read()`, `cas()`, `set()` |
| `JsonHandle` | `create()`, `get()`, `set()` |
| `VectorHandle` | `get()`, `insert()` |

**Observation**: Handles expose a reduced subset of each primitive's operations. This is intentional (simple API), but some notable gaps exist:
- JsonHandle: no `delete`, `list`, `exists`
- EventHandle: no `read_by_type`
- VectorHandle: no collection management, search

---

## 7. Recommendations

### MVP Blocking (Priority 1)

1. **Standardize read naming** — Choose either `get()` or `read()` across all primitives. Given 3-vs-2 split, `get()` is the natural choice. Rename:
   - `EventLog::read()` → `EventLog::get()`
   - `StateCell::read()` → `StateCell::get()`
   - `StateCell::read_versioned()` → `StateCell::get_versioned()`
   - `StateCell::readv()` → `StateCell::getv()`

2. **Fix State return type** — State write operations should return bare `Version` like all other primitives, not `Versioned<Version>`.

### Post-MVP (Priority 2)

3. **Complete extension traits** — Add missing operations to `JsonStoreExt`, `EventLogExt`, `StateCellExt`, `VectorStoreExt` for transaction completeness.

4. **Add missing operations**:
   - `exists()` on KV, Event, State, Vector (currently only JSON has it)
   - `getv()` (history) on Event and Vector
   - `delete()` on Event

5. **Expand BranchHandle methods** — Add `list`, `delete`, `exists` to JsonHandle; `read_by_type` to EventHandle; collection ops to VectorHandle.

---

## Strengths

- All primitives use the stateless `Arc<Database>` facade pattern
- Space-scoped operations are consistent across all primitives
- Branch isolation via key prefix is uniformly enforced
- Transaction extension traits enable cross-primitive atomicity
- BranchHandle pattern provides clean scoped API

---

## Methodology

Read all primitive source files (`kv.rs`, `json.rs`, `event.rs`, `state.rs`, `vector/store.rs`), extension traits (`extensions.rs`), branch handles (`handle.rs`), and public API layer (`executor/src/api/*.rs`). Built complete operation matrix by examining every public method signature.
