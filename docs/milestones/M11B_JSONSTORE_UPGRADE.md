# M11B: JSONStore Upgrade Plan

## Executive Summary

This document outlines the comprehensive upgrade plan for Strata's JSONStore primitive and substrate layers. The goal is to transform JSONStore from a basic document store into a credible, production-ready JSON document database that covers essential operations users expect.

**Scope**: Primitive layer fixes, substrate API additions, facade alignment
**Priority**: P0 (blocking for M11 completion)
**Estimated Effort**: Medium-Large

---

## Table of Contents

1. [Current State Analysis](#1-current-state-analysis)
2. [Issues to Fix](#2-issues-to-fix)
3. [Features to Add](#3-features-to-add)
4. [API Specifications](#4-api-specifications)
5. [Implementation Plan](#5-implementation-plan)
6. [Migration Notes](#6-migration-notes)

---

## 1. Current State Analysis

### 1.1 Primitive Layer (`crates/primitives/src/json_store.rs`)

**What We Have:**
| Method | Status | Notes |
|--------|--------|-------|
| `create` | ✅ Working | Creates document with version 1 |
| `get` | ✅ Working | Fast-path via SnapshotView |
| `get_doc` | ✅ Working | Returns full JsonDoc with metadata |
| `get_version` | ✅ Working | Returns document version |
| `exists` | ✅ Working | Check document existence |
| `set` | ✅ Working | Set value at path (transactional) |
| `delete_at_path` | ✅ Working | Delete value at path |
| `destroy` | ✅ Working | Delete entire document |
| `search` | ✅ Working | Full-text search |

**What's Missing:**
- `list` - Document enumeration
- `merge` - Atomic RFC 7396 merge
- `cas` - Compare-and-swap for optimistic concurrency
- `query` - Find documents by field value
- `count` - Count documents
- `batch_get` / `batch_create` - Bulk operations
- `array_push` - Atomic array append
- `increment` - Atomic numeric increment

### 1.2 Substrate Layer (`crates/api/src/substrate/json.rs`)

**What We Have:**
| Method | Status | Notes |
|--------|--------|-------|
| `json_set` | ✅ Working | Creates if not exists (semantic difference from primitive) |
| `json_get` | ✅ Working | Calls primitive |
| `json_delete` | ✅ Working | Returns count, not Version |
| `json_merge` | ⚠️ Non-atomic | Read-modify-write implementation |
| `json_exists` | ✅ Working | Calls primitive |
| `json_get_version` | ✅ Working | Calls primitive |
| `json_search` | ✅ Working | Calls primitive |
| `json_history` | ❌ **Stubbed** | Returns `Ok(vec![])` |

### 1.3 Facade Layer (`crates/api/src/facade/json.rs`)

The facade has RedisJSON-style helpers that duplicate substrate logic:
- `json_merge` - Re-implements merge instead of calling substrate
- `json_numincrby` - Read-modify-write (not atomic)
- `json_strappend` - Read-modify-write (not atomic)
- `json_arrappend` - Read-modify-write (not atomic)

### 1.4 Architecture Violations

1. **Facade duplicates substrate logic** - `json_merge` is implemented in both layers
2. **Substrate has features primitive doesn't** - `json_merge` exists in substrate but not primitive
3. **Non-atomic operations** - Multiple operations that should be atomic are read-modify-write

---

## 2. Issues to Fix

### 2.1 [P0] `json_history` is Stubbed

**Location**: `crates/api/src/substrate/json.rs:369-378`

**Current Code**:
```rust
fn json_history(
    &self,
    _run: &ApiRunId,
    _key: &str,
    _limit: Option<u64>,
    _before: Option<Version>,
) -> StrataResult<Vec<Versioned<Value>>> {
    // History not yet implemented
    Ok(vec![])
}
```

**Problem**: Returns empty vector instead of actual history. Users calling this API get no data and no error - silent failure.

**Resolution Options**:
1. **Option A**: Implement via WAL replay (requires WAL infrastructure)
2. **Option B**: Remove from API and document as not supported
3. **Option C**: Store version history in document metadata (storage overhead)

**Recommendation**: Option B for now. Remove `json_history` from the substrate trait. Document that JSON documents don't retain history. Users who need history should use EventLog to record changes.

### 2.2 [P0] `json_merge` is Non-Atomic

**Location**: `crates/api/src/substrate/json.rs:323-367`

**Current Code**:
```rust
fn json_merge(&self, run, key, path, patch) -> StrataResult<Version> {
    let current = self.json().get(...)?;        // Read
    let merged = json_merge_patch(&mut base, &patch);  // Modify
    self.json().set(..., merged)?;              // Write - RACE CONDITION
}
```

**Problem**: Between read and write, another client can modify the document. Those changes are silently lost.

**Resolution**: Move merge logic into primitive layer inside a transaction:
```rust
// In primitive
fn merge(&self, run_id, doc_id, path, patch) -> Result<Version> {
    self.db.transaction(*run_id, |txn| {
        let mut doc = load_doc(txn, &key)?;
        apply_merge_patch(&mut doc.value, path, &patch);
        doc.touch();
        txn.put(key, serialize(&doc)?)?;
        Ok(Version::counter(doc.version))
    })
}
```

### 2.3 [P1] Facade Duplicates Substrate Merge Logic

**Location**: `crates/api/src/facade/json.rs:161-168`

**Current Code**:
```rust
fn json_merge(&self, key: &str, path: &str, patch: Value) -> StrataResult<()> {
    let current = self.substrate().json_get(...)?;
    let merged = merge_values(v.value, patch);  // Duplicate merge logic!
    self.substrate().json_set(..., merged)?;
}
```

**Problem**: Facade should call `substrate.json_merge()`, not re-implement merge.

**Resolution**: Change to:
```rust
fn json_merge(&self, key: &str, path: &str, patch: Value) -> StrataResult<()> {
    self.substrate().json_merge(self.default_run(), key, path, patch)?;
    Ok(())
}
```

### 2.4 [P1] Facade Atomic Operations are Non-Atomic

**Affected Methods**:
- `json_numincrby` - Increment number
- `json_strappend` - Append to string
- `json_arrappend` - Append to array

**Problem**: All use read-modify-write pattern which has race conditions.

**Resolution**: Add atomic operations to primitive, expose through substrate, call from facade.

### 2.5 [P2] Versioning Semantics

**Current**: Primitive uses `Version::counter(doc.version)` where `doc.version` is a per-document counter (1, 2, 3...).

**Contract Says**: JSON should use transaction-based versioning (`Version::Txn`).

**Analysis**: Per-document counter is actually appropriate for documents:
- Intuitive for users ("version 5 of this document")
- Natural for CAS operations
- Matches MongoDB/CouchDB behavior

**Resolution**: Keep `Version::counter()` but document this clearly. The counter represents document revision, not global transaction ID.

---

## 3. Features to Add

### 3.1 Tier 1: Essential (Must Have)

#### 3.1.1 Document Listing (`list`)

**Why Essential**: Users cannot enumerate documents in a run. This is fundamental to any document store.

**Use Cases**:
- Admin dashboard showing all documents
- Backup/export functionality
- Cleanup scripts
- Debugging

**API**:
```rust
// Primitive
fn list(
    &self,
    run_id: &RunId,
    prefix: Option<&str>,     // Optional key prefix filter
    cursor: Option<&str>,     // Pagination cursor
    limit: usize,             // Max results
) -> Result<ListResult>;

struct ListResult {
    doc_ids: Vec<JsonDocId>,
    next_cursor: Option<String>,
}

// Substrate
fn json_list(
    &self,
    run: &ApiRunId,
    prefix: Option<&str>,
    cursor: Option<&str>,
    limit: u64,
) -> StrataResult<JsonListResult>;
```

#### 3.1.2 Compare-and-Swap (`cas`)

**Why Essential**: Without CAS, there's no safe way to do concurrent updates. Two clients reading version 5 and both writing will result in lost updates.

**Use Cases**:
- Optimistic concurrency control
- Preventing lost updates
- Implementing higher-level transactions

**API**:
```rust
// Primitive
fn cas(
    &self,
    run_id: &RunId,
    doc_id: &JsonDocId,
    expected_version: u64,
    path: &JsonPath,
    value: JsonValue,
) -> Result<Version>;  // Returns Err(VersionMismatch) if version doesn't match

// Substrate
fn json_cas(
    &self,
    run: &ApiRunId,
    key: &str,
    expected_version: u64,
    path: &str,
    value: Value,
) -> StrataResult<Version>;
```

**Error Handling**:
```rust
pub enum JsonError {
    VersionMismatch { expected: u64, actual: u64 },
    // ... other errors
}
```

#### 3.1.3 Atomic Merge (`merge` at primitive level)

**Why Essential**: Current non-atomic merge loses concurrent updates.

**API**: Already defined in substrate, needs primitive implementation.

```rust
// Primitive (NEW)
fn merge(
    &self,
    run_id: &RunId,
    doc_id: &JsonDocId,
    path: &JsonPath,
    patch: JsonValue,
) -> Result<Version>;
```

#### 3.1.4 Query by Field Value (`query`)

**Why Essential**: Search is for fuzzy text matching. Query is for exact field matching. Applications need both.

**Difference**:
- Search: "Find documents containing 'error'" → fuzzy, ranked
- Query: "Find documents where status='failed'" → exact, boolean

**Use Cases**:
- Find all orders with status "pending"
- Find all users in organization "acme"
- Find all documents created by user "alice"

**API**:
```rust
// Primitive
fn query(
    &self,
    run_id: &RunId,
    path: &JsonPath,
    value: &JsonValue,
    limit: usize,
) -> Result<Vec<JsonDocId>>;

// Substrate
fn json_query(
    &self,
    run: &ApiRunId,
    path: &str,
    value: Value,
    limit: u64,
) -> StrataResult<Vec<String>>;
```

**Implementation Note**: Initial implementation will scan all documents. Future optimization can add secondary indexes.

### 3.2 Tier 2: Important (Should Have)

#### 3.2.1 Document Count (`count`)

**Why Important**: Basic analytics, progress tracking, capacity planning.

**API**:
```rust
// Primitive
fn count(&self, run_id: &RunId) -> Result<u64>;

// Substrate
fn json_count(&self, run: &ApiRunId) -> StrataResult<u64>;
```

#### 3.2.2 Batch Get (`batch_get`)

**Why Important**: Getting N documents one-by-one requires N round trips. Batch get is O(1) round trips.

**API**:
```rust
// Primitive
fn batch_get(
    &self,
    run_id: &RunId,
    doc_ids: &[JsonDocId],
) -> Result<Vec<Option<Versioned<JsonDoc>>>>;

// Substrate
fn json_batch_get(
    &self,
    run: &ApiRunId,
    keys: Vec<&str>,
) -> StrataResult<Vec<Option<Versioned<Value>>>>;
```

#### 3.2.3 Batch Create (`batch_create`)

**Why Important**: Creating N documents atomically. All succeed or all fail.

**API**:
```rust
// Primitive
fn batch_create(
    &self,
    run_id: &RunId,
    docs: Vec<(JsonDocId, JsonValue)>,
) -> Result<Vec<Version>>;

// Substrate
fn json_batch_create(
    &self,
    run: &ApiRunId,
    docs: Vec<(&str, Value)>,
) -> StrataResult<Vec<Version>>;
```

### 3.3 Tier 3: Valuable (Nice to Have)

#### 3.3.1 Atomic Array Push (`array_push`)

**Why Valuable**: Appending to arrays is common. Current read-modify-write loses concurrent appends.

**API**:
```rust
// Primitive
fn array_push(
    &self,
    run_id: &RunId,
    doc_id: &JsonDocId,
    path: &JsonPath,
    values: Vec<JsonValue>,
) -> Result<(Version, usize)>;  // Returns (new_version, new_length)

// Substrate
fn json_array_push(
    &self,
    run: &ApiRunId,
    key: &str,
    path: &str,
    values: Vec<Value>,
) -> StrataResult<usize>;
```

#### 3.3.2 Atomic Increment (`increment`)

**Why Valuable**: Counters, metrics, rate limiting. Current read-modify-write loses concurrent increments.

**API**:
```rust
// Primitive
fn increment(
    &self,
    run_id: &RunId,
    doc_id: &JsonDocId,
    path: &JsonPath,
    delta: f64,
) -> Result<(Version, f64)>;  // Returns (new_version, new_value)

// Substrate
fn json_increment(
    &self,
    run: &ApiRunId,
    key: &str,
    path: &str,
    delta: f64,
) -> StrataResult<f64>;
```

#### 3.3.3 Array Pop (`array_pop`)

**Why Valuable**: Queue-like operations, stack operations.

**API**:
```rust
// Primitive
fn array_pop(
    &self,
    run_id: &RunId,
    doc_id: &JsonDocId,
    path: &JsonPath,
) -> Result<(Version, Option<JsonValue>)>;

// Substrate
fn json_array_pop(
    &self,
    run: &ApiRunId,
    key: &str,
    path: &str,
) -> StrataResult<Option<Value>>;
```

---

## 4. API Specifications

### 4.1 Updated Primitive Trait

```rust
impl JsonStore {
    // ============================================================
    // EXISTING (keep as-is)
    // ============================================================

    pub fn create(&self, run_id: &RunId, doc_id: &JsonDocId, value: JsonValue) -> Result<Version>;
    pub fn get(&self, run_id: &RunId, doc_id: &JsonDocId, path: &JsonPath) -> Result<Option<Versioned<JsonValue>>>;
    pub fn get_doc(&self, run_id: &RunId, doc_id: &JsonDocId) -> Result<Option<Versioned<JsonDoc>>>;
    pub fn get_version(&self, run_id: &RunId, doc_id: &JsonDocId) -> Result<Option<u64>>;
    pub fn exists(&self, run_id: &RunId, doc_id: &JsonDocId) -> Result<bool>;
    pub fn set(&self, run_id: &RunId, doc_id: &JsonDocId, path: &JsonPath, value: JsonValue) -> Result<Version>;
    pub fn delete_at_path(&self, run_id: &RunId, doc_id: &JsonDocId, path: &JsonPath) -> Result<Version>;
    pub fn destroy(&self, run_id: &RunId, doc_id: &JsonDocId) -> Result<bool>;
    pub fn search(&self, req: &SearchRequest) -> Result<SearchResponse>;

    // ============================================================
    // TIER 1: ESSENTIAL (add)
    // ============================================================

    /// List documents with cursor-based pagination
    pub fn list(
        &self,
        run_id: &RunId,
        prefix: Option<&str>,
        cursor: Option<&str>,
        limit: usize,
    ) -> Result<JsonListResult>;

    /// Compare-and-swap update - fails if version doesn't match
    pub fn cas(
        &self,
        run_id: &RunId,
        doc_id: &JsonDocId,
        expected_version: u64,
        path: &JsonPath,
        value: JsonValue,
    ) -> Result<Version>;

    /// Atomic RFC 7396 merge
    pub fn merge(
        &self,
        run_id: &RunId,
        doc_id: &JsonDocId,
        path: &JsonPath,
        patch: JsonValue,
    ) -> Result<Version>;

    /// Query documents by field value (exact match)
    pub fn query(
        &self,
        run_id: &RunId,
        path: &JsonPath,
        value: &JsonValue,
        limit: usize,
    ) -> Result<Vec<JsonDocId>>;

    // ============================================================
    // TIER 2: IMPORTANT (add)
    // ============================================================

    /// Count documents in run
    pub fn count(&self, run_id: &RunId) -> Result<u64>;

    /// Get multiple documents
    pub fn batch_get(
        &self,
        run_id: &RunId,
        doc_ids: &[JsonDocId],
    ) -> Result<Vec<Option<Versioned<JsonDoc>>>>;

    /// Create multiple documents atomically
    pub fn batch_create(
        &self,
        run_id: &RunId,
        docs: Vec<(JsonDocId, JsonValue)>,
    ) -> Result<Vec<Version>>;

    // ============================================================
    // TIER 3: VALUABLE (add)
    // ============================================================

    /// Atomic array push - returns new length
    pub fn array_push(
        &self,
        run_id: &RunId,
        doc_id: &JsonDocId,
        path: &JsonPath,
        values: Vec<JsonValue>,
    ) -> Result<(Version, usize)>;

    /// Atomic numeric increment - returns new value
    pub fn increment(
        &self,
        run_id: &RunId,
        doc_id: &JsonDocId,
        path: &JsonPath,
        delta: f64,
    ) -> Result<(Version, f64)>;

    /// Atomic array pop - returns removed element
    pub fn array_pop(
        &self,
        run_id: &RunId,
        doc_id: &JsonDocId,
        path: &JsonPath,
    ) -> Result<(Version, Option<JsonValue>)>;
}

/// Result of list operation
#[derive(Debug, Clone)]
pub struct JsonListResult {
    pub doc_ids: Vec<JsonDocId>,
    pub next_cursor: Option<String>,
}
```

### 4.2 Updated Substrate Trait

```rust
pub trait JsonStore {
    // ============================================================
    // EXISTING (keep)
    // ============================================================

    fn json_set(&self, run: &ApiRunId, key: &str, path: &str, value: Value) -> StrataResult<Version>;
    fn json_get(&self, run: &ApiRunId, key: &str, path: &str) -> StrataResult<Option<Versioned<Value>>>;
    fn json_delete(&self, run: &ApiRunId, key: &str, path: &str) -> StrataResult<u64>;
    fn json_merge(&self, run: &ApiRunId, key: &str, path: &str, patch: Value) -> StrataResult<Version>;
    fn json_exists(&self, run: &ApiRunId, key: &str) -> StrataResult<bool>;
    fn json_get_version(&self, run: &ApiRunId, key: &str) -> StrataResult<Option<u64>>;
    fn json_search(&self, run: &ApiRunId, query: &str, k: u64) -> StrataResult<Vec<JsonSearchHit>>;

    // REMOVE: json_history (was stubbed, not implementable without WAL changes)

    // ============================================================
    // TIER 1: ESSENTIAL (add)
    // ============================================================

    /// List document keys with pagination
    fn json_list(
        &self,
        run: &ApiRunId,
        prefix: Option<&str>,
        cursor: Option<&str>,
        limit: u64,
    ) -> StrataResult<JsonListResult>;

    /// Compare-and-swap update
    fn json_cas(
        &self,
        run: &ApiRunId,
        key: &str,
        expected_version: u64,
        path: &str,
        value: Value,
    ) -> StrataResult<Version>;

    /// Query by field value
    fn json_query(
        &self,
        run: &ApiRunId,
        path: &str,
        value: Value,
        limit: u64,
    ) -> StrataResult<Vec<String>>;

    // ============================================================
    // TIER 2: IMPORTANT (add)
    // ============================================================

    /// Count documents
    fn json_count(&self, run: &ApiRunId) -> StrataResult<u64>;

    /// Batch get
    fn json_batch_get(
        &self,
        run: &ApiRunId,
        keys: Vec<&str>,
    ) -> StrataResult<Vec<Option<Versioned<Value>>>>;

    /// Batch create
    fn json_batch_create(
        &self,
        run: &ApiRunId,
        docs: Vec<(&str, Value)>,
    ) -> StrataResult<Vec<Version>>;

    // ============================================================
    // TIER 3: VALUABLE (add)
    // ============================================================

    /// Atomic array push
    fn json_array_push(
        &self,
        run: &ApiRunId,
        key: &str,
        path: &str,
        values: Vec<Value>,
    ) -> StrataResult<usize>;

    /// Atomic increment
    fn json_increment(
        &self,
        run: &ApiRunId,
        key: &str,
        path: &str,
        delta: f64,
    ) -> StrataResult<f64>;

    /// Atomic array pop
    fn json_array_pop(
        &self,
        run: &ApiRunId,
        key: &str,
        path: &str,
    ) -> StrataResult<Option<Value>>;
}

/// List operation result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonListResult {
    pub keys: Vec<String>,
    pub next_cursor: Option<String>,
}
```

### 4.3 Updated Facade Trait

```rust
pub trait JsonFacade {
    // ============================================================
    // EXISTING (keep, but fix implementations)
    // ============================================================

    fn json_get(&self, key: &str, path: &str) -> StrataResult<Option<Value>>;
    fn json_getv(&self, key: &str, path: &str) -> StrataResult<Option<Versioned<Value>>>;
    fn json_set(&self, key: &str, path: &str, value: Value) -> StrataResult<()>;
    fn json_del(&self, key: &str, path: &str) -> StrataResult<u64>;
    fn json_merge(&self, key: &str, path: &str, patch: Value) -> StrataResult<()>;  // FIX: call substrate
    fn json_type(&self, key: &str, path: &str) -> StrataResult<Option<String>>;
    fn json_numincrby(&self, key: &str, path: &str, delta: f64) -> StrataResult<f64>;  // FIX: call substrate
    fn json_strappend(&self, key: &str, path: &str, suffix: &str) -> StrataResult<usize>;
    fn json_arrappend(&self, key: &str, path: &str, values: Vec<Value>) -> StrataResult<usize>;  // FIX: call substrate
    fn json_arrlen(&self, key: &str, path: &str) -> StrataResult<Option<usize>>;
    fn json_objkeys(&self, key: &str, path: &str) -> StrataResult<Option<Vec<String>>>;
    fn json_objlen(&self, key: &str, path: &str) -> StrataResult<Option<usize>>;

    // ============================================================
    // NEW (add)
    // ============================================================

    /// List document keys
    fn json_keys(&self, prefix: Option<&str>, limit: u64) -> StrataResult<Vec<String>>;

    /// Count documents
    fn json_count(&self) -> StrataResult<u64>;

    /// Query by field
    fn json_query(&self, path: &str, value: Value, limit: u64) -> StrataResult<Vec<String>>;

    /// Pop from array
    fn json_arrpop(&self, key: &str, path: &str) -> StrataResult<Option<Value>>;
}
```

---

## 5. Implementation Plan

### Phase 1: Bug Fixes (P0)

**Estimated Effort**: 1-2 days

| Task | File | Description |
|------|------|-------------|
| 1.1 | `substrate/json.rs` | Remove `json_history` from trait (or mark deprecated) |
| 1.2 | `primitives/json_store.rs` | Add `merge()` method with atomic implementation |
| 1.3 | `substrate/json.rs` | Update `json_merge` to call primitive `merge()` |
| 1.4 | `facade/json.rs` | Fix `json_merge` to call `substrate.json_merge()` |
| 1.5 | `facade/json.rs` | Fix `json_numincrby` to use atomic increment (after Phase 2) |
| 1.6 | `facade/json.rs` | Fix `json_arrappend` to use atomic push (after Phase 2) |

### Phase 2: Tier 1 Features (P0)

**Estimated Effort**: 3-4 days

| Task | File | Description |
|------|------|-------------|
| 2.1 | `primitives/json_store.rs` | Add `list()` method |
| 2.2 | `substrate/json.rs` | Add `json_list()` method |
| 2.3 | `primitives/json_store.rs` | Add `cas()` method |
| 2.4 | `substrate/json.rs` | Add `json_cas()` method |
| 2.5 | `primitives/json_store.rs` | Add `query()` method |
| 2.6 | `substrate/json.rs` | Add `json_query()` method |
| 2.7 | Tests | Comprehensive tests for all Tier 1 features |

### Phase 3: Tier 2 Features (P1)

**Estimated Effort**: 2-3 days

| Task | File | Description |
|------|------|-------------|
| 3.1 | `primitives/json_store.rs` | Add `count()` method |
| 3.2 | `substrate/json.rs` | Add `json_count()` method |
| 3.3 | `primitives/json_store.rs` | Add `batch_get()` method |
| 3.4 | `substrate/json.rs` | Add `json_batch_get()` method |
| 3.5 | `primitives/json_store.rs` | Add `batch_create()` method |
| 3.6 | `substrate/json.rs` | Add `json_batch_create()` method |
| 3.7 | Tests | Comprehensive tests for all Tier 2 features |

### Phase 4: Tier 3 Features (P2)

**Estimated Effort**: 2 days

| Task | File | Description |
|------|------|-------------|
| 4.1 | `primitives/json_store.rs` | Add `array_push()` method |
| 4.2 | `primitives/json_store.rs` | Add `increment()` method |
| 4.3 | `primitives/json_store.rs` | Add `array_pop()` method |
| 4.4 | `substrate/json.rs` | Add substrate wrappers for all Tier 3 |
| 4.5 | `facade/json.rs` | Update facade to use atomic operations |
| 4.6 | Tests | Comprehensive tests for all Tier 3 features |

### Phase 5: Documentation & Polish (P1)

**Estimated Effort**: 1 day

| Task | Description |
|------|-------------|
| 5.1 | Update `M11_CONTRACT.md` with new API surface |
| 5.2 | Update `JSONSTORE_TRANSLATION.md` |
| 5.3 | Add usage examples to doc comments |
| 5.4 | Update defects tracking |

---

## 6. Migration Notes

### 6.1 Breaking Changes

1. **`json_history` removal**: Clients calling this will get a compile error (if using trait) or runtime error (if using dynamic dispatch). Migration: Remove calls, use EventLog for audit trails.

### 6.2 Behavioral Changes

1. **`json_merge` atomicity**: Previously, concurrent merges could lose updates. Now they're atomic. This is a correctness fix, not a breaking change.

2. **`json_numincrby` atomicity**: Same as above.

3. **`json_arrappend` atomicity**: Same as above.

### 6.3 New Error Types

```rust
pub enum JsonError {
    /// CAS operation failed due to version mismatch
    VersionMismatch {
        expected: u64,
        actual: u64,
    },

    /// Path targets wrong type (e.g., array_push on non-array)
    TypeMismatch {
        path: String,
        expected: &'static str,
        actual: String,
    },

    /// Query path is not indexable
    InvalidQueryPath {
        path: String,
        reason: String,
    },
}
```

---

## Appendix A: Implementation Sketches

### A.1 `list` Implementation

```rust
pub fn list(
    &self,
    run_id: &RunId,
    prefix: Option<&str>,
    cursor: Option<&str>,
    limit: usize,
) -> Result<JsonListResult> {
    let snapshot = self.db.storage().create_snapshot();
    let ns = self.namespace_for_run(run_id);
    let scan_prefix = Key::new_json_prefix(ns);

    let mut doc_ids = Vec::new();
    let mut seen_cursor = cursor.is_none();
    let mut last_id: Option<JsonDocId> = None;

    for (key, _value) in snapshot.scan_prefix(&scan_prefix)? {
        // Extract doc_id from key
        let doc_id = extract_doc_id_from_key(&key)?;

        // Handle cursor-based pagination
        if !seen_cursor {
            if doc_id.to_string() == cursor.unwrap() {
                seen_cursor = true;
            }
            continue;
        }

        // Handle prefix filter
        if let Some(p) = prefix {
            if !doc_id.to_string().starts_with(p) {
                continue;
            }
        }

        doc_ids.push(doc_id);
        last_id = Some(doc_id);

        if doc_ids.len() >= limit {
            break;
        }
    }

    let next_cursor = if doc_ids.len() == limit {
        last_id.map(|id| id.to_string())
    } else {
        None
    };

    Ok(JsonListResult { doc_ids, next_cursor })
}
```

### A.2 `cas` Implementation

```rust
pub fn cas(
    &self,
    run_id: &RunId,
    doc_id: &JsonDocId,
    expected_version: u64,
    path: &JsonPath,
    value: JsonValue,
) -> Result<Version> {
    let key = self.key_for(run_id, doc_id);

    self.db.transaction(*run_id, |txn| {
        let stored = txn.get(&key)?.ok_or_else(|| {
            Error::NotFound(format!("JSON document {} not found", doc_id))
        })?;
        let mut doc = Self::deserialize_doc(&stored)?;

        // Version check
        if doc.version != expected_version {
            return Err(Error::VersionMismatch {
                expected: expected_version,
                actual: doc.version,
            });
        }

        // Apply update
        set_at_path(&mut doc.value, path, value)
            .map_err(|e| Error::InvalidOperation(format!("Path error: {}", e)))?;
        doc.touch();

        let serialized = Self::serialize_doc(&doc)?;
        txn.put(key.clone(), serialized)?;

        Ok(Version::counter(doc.version))
    })
}
```

### A.3 `merge` Implementation (Atomic)

```rust
pub fn merge(
    &self,
    run_id: &RunId,
    doc_id: &JsonDocId,
    path: &JsonPath,
    patch: JsonValue,
) -> Result<Version> {
    let key = self.key_for(run_id, doc_id);

    self.db.transaction(*run_id, |txn| {
        // Load or create document
        let mut doc = match txn.get(&key)? {
            Some(stored) => Self::deserialize_doc(&stored)?,
            None => JsonDoc::new(*doc_id, JsonValue::object()),
        };

        // Get current value at path
        let current = get_at_path(&doc.value, path).cloned();

        // Apply RFC 7396 merge
        let merged = match current {
            Some(mut base) => {
                json_merge_patch(base.as_inner_mut(), patch.as_inner());
                base
            }
            None => patch,
        };

        // Set merged value
        if path.is_root() {
            doc.value = merged;
        } else {
            set_at_path(&mut doc.value, path, merged)
                .map_err(|e| Error::InvalidOperation(format!("Path error: {}", e)))?;
        }
        doc.touch();

        let serialized = Self::serialize_doc(&doc)?;
        txn.put(key.clone(), serialized)?;

        Ok(Version::counter(doc.version))
    })
}
```

### A.4 `query` Implementation

```rust
pub fn query(
    &self,
    run_id: &RunId,
    path: &JsonPath,
    value: &JsonValue,
    limit: usize,
) -> Result<Vec<JsonDocId>> {
    let snapshot = self.db.storage().create_snapshot();
    let ns = self.namespace_for_run(run_id);
    let scan_prefix = Key::new_json_prefix(ns);

    let mut results = Vec::new();

    for (_key, versioned_value) in snapshot.scan_prefix(&scan_prefix)? {
        if results.len() >= limit {
            break;
        }

        let doc = match Self::deserialize_doc(&versioned_value.value) {
            Ok(d) => d,
            Err(_) => continue,
        };

        // Check if path matches value
        if let Some(field_value) = get_at_path(&doc.value, path) {
            if field_value == value {
                results.push(doc.id);
            }
        }
    }

    Ok(results)
}
```

---

## Appendix B: Test Plan

### B.1 Unit Tests (per method)

Each new method needs:
1. Happy path test
2. Error case tests (not found, wrong type, etc.)
3. Edge cases (empty, large, unicode, etc.)
4. Run isolation test

### B.2 Integration Tests

1. **Concurrent CAS**: Multiple threads doing CAS on same document
2. **Concurrent merge**: Multiple threads merging same document
3. **List pagination**: Large number of documents, verify pagination works
4. **Query correctness**: Verify query returns exactly matching documents
5. **Batch atomicity**: Verify batch operations are all-or-nothing

### B.3 Stress Tests

1. **1000 concurrent merges**: Verify no lost updates
2. **10000 documents list**: Verify pagination handles scale
3. **Query on 10000 documents**: Verify acceptable performance

---

## Revision History

| Date | Version | Author | Changes |
|------|---------|--------|---------|
| 2024-01-24 | 1.0 | Claude | Initial document |
