# JSONStore Defects and Gaps

> Consolidated from architecture review, primitive vs substrate analysis, and document database best practices.
> Source: `crates/api/src/substrate/json.rs`, `crates/api/src/facade/json.rs`, and `crates/primitives/src/json_store.rs`

## Summary

| Category | Count | Priority |
|----------|-------|----------|
| Stubbed APIs | 1 | P0 |
| Hidden Primitive Features | 4 | P0-P1 |
| Missing Table Stakes APIs | 5 | P1 |
| Layer Inconsistencies | 2 | P1 |
| API Design Issues | 2 | P1 |
| World-Class Features | 4 | P2 |
| **Total Issues** | **18** | |

**Overall Assessment:** JSONStore is more complete than StateCell, TraceStore, or RunIndex. The facade layer has useful Redis-like convenience operations. Main gaps are search exposure, document listing, and batch operations.

---

## What is JSONStore?

JSONStore is a **lightweight document store** for semi-structured JSON data with path-based access.

**Purpose:**
- Store arbitrary JSON documents with UUID identifiers
- Enable partial updates via JSONPath without full read/write cycles
- Maintain document-level versioning for optimistic concurrency
- Provide full-text search for document discovery
- Support RFC 7396 JSON Merge Patch

**vs KVStore:**
| Aspect | JSONStore | KVStore |
|--------|-----------|---------|
| Data Model | Nested JSON documents | Flat key-value pairs |
| Partial Updates | Yes (path-based) | No (full value replace) |
| Querying | Path navigation + search | Key lookup only |
| Use Case | Semi-structured data | Simple values, counters |

**Not a full document database** - no complex queries, no indexing, no schema validation.

---

## Current API Surface

### Primitive (7 methods)
```rust
fn create(run_id, doc_id, value) -> Version;
fn get(run_id, doc_id, path) -> Option<Versioned<JsonValue>>;  // Fast path
fn get_doc(run_id, doc_id) -> Option<Versioned<JsonDoc>>;
fn get_version(run_id, doc_id) -> Option<u64>;
fn exists(run_id, doc_id) -> bool;
fn set(run_id, doc_id, path, value) -> Version;
fn delete_at_path(run_id, doc_id, path) -> Version;
fn destroy(run_id, doc_id) -> bool;
fn search(request: SearchRequest) -> SearchResponse;
```

### Substrate (5 methods, 1 stubbed)
```rust
fn json_set(run, key, path, value) -> Version;
fn json_get(run, key, path) -> Option<Versioned<Value>>;
fn json_delete(run, key, path) -> u64;
fn json_merge(run, key, path, patch) -> Version;  // RFC 7396
fn json_history(run, key, limit?, before?) -> Vec<Versioned<Value>>;  // STUBBED
```

### Facade (12 methods)
```rust
fn json_get(key, path) -> Option<Value>;
fn json_getv(key, path) -> Option<Versioned<Value>>;
fn json_set(key, path, value) -> ();
fn json_del(key, path) -> u64;
fn json_merge(key, path, patch) -> ();
fn json_type(key, path) -> Option<String>;        // Type introspection
fn json_numincrby(key, path, delta) -> f64;       // Atomic increment
fn json_strappend(key, path, suffix) -> usize;    // String append
fn json_arrappend(key, path, values) -> usize;    // Array append
fn json_arrlen(key, path) -> Option<usize>;       // Array length
fn json_objkeys(key, path) -> Option<Vec<String>>; // Object keys
fn json_objlen(key, path) -> Option<usize>;       // Object length
```

---

## Part 1: Stubbed APIs (P0)

### Stub 1: `json_history` - Version History

**Priority:** P0

**Current State:**
```rust
fn json_history(...) -> StrataResult<Vec<Versioned<Value>>> {
    Ok(vec![])  // Not implemented - always returns empty
}
```

**Why Critical:**
- API exists but doesn't work
- Users expect history based on method signature
- Required for `replay()` Magic API

**Fix:** Implement actual version history retrieval from storage layer

---

## Part 2: Hidden Primitive Features (P0-P1)

### Gap 1: `json_search` - Full-Text Search

**Priority:** P0

**What Primitive Has:**
```rust
fn search(&self, request: &SearchRequest) -> Result<SearchResponse>;
// Flattens JSON into "path: value" pairs for searching
// Respects budget constraints
// Supports time range filtering
```

**What Substrate/Facade Expose:** Nothing

**Why Critical:**
- Cannot find documents by content
- Search is implemented (M6) but hidden
- Common use case: "Find all documents mentioning X"

**Proposed Substrate API:**
```rust
fn json_search(&self, run: &ApiRunId, query: &str, limit: Option<u64>)
    -> StrataResult<Vec<(String, Versioned<Value>)>>;  // (doc_id, doc)
```

---

### Gap 2: `json_exists` - Existence Check

**Priority:** P1

**What Primitive Has:**
```rust
fn exists(&self, run_id: &RunId, doc_id: &JsonDocId) -> Result<bool>;
```

**What Substrate/Facade Expose:** Nothing (must do full get)

**Why Important:**
- Efficient existence check without reading document
- Common pattern: "Does this document exist?"

**Proposed API:**
```rust
fn json_exists(&self, run: &ApiRunId, key: &str) -> StrataResult<bool>;
```

---

### Gap 3: `json_get_version` - Version Check

**Priority:** P1

**What Primitive Has:**
```rust
fn get_version(&self, run_id: &RunId, doc_id: &JsonDocId) -> Result<Option<u64>>;
```

**What Substrate/Facade Expose:** Nothing

**Why Important:**
- Check version without reading full document
- Useful for conditional updates, cache invalidation

**Proposed API:**
```rust
fn json_get_version(&self, run: &ApiRunId, key: &str) -> StrataResult<Option<u64>>;
```

---

### Gap 4: `json_get_doc` - Full Document with Metadata

**Priority:** P1

**What Primitive Has:**
```rust
fn get_doc(&self, run_id: &RunId, doc_id: &JsonDocId) -> Result<Option<Versioned<JsonDoc>>>;
// Returns: id, value, version, created_at, updated_at
```

**What Substrate/Facade Expose:** Only value (via json_get at root path)

**Why Important:**
- Cannot get document metadata (created_at, updated_at)
- Cannot get document ID back from a read

**Proposed API:**
```rust
struct JsonDocInfo {
    key: String,
    version: u64,
    created_at: u64,
    updated_at: u64,
}

fn json_get_info(&self, run: &ApiRunId, key: &str) -> StrataResult<Option<JsonDocInfo>>;
```

---

## Part 3: Missing Table Stakes APIs (P1)

### Gap 5: `json_list` / `json_keys` - Document Enumeration

**Priority:** P0

**What Exists:** Nothing at any layer

**Why Critical:**
- Cannot list documents in a run
- Cannot enumerate keys
- Same problem as KVStore's missing `kv_keys`

**Proposed API:**
```rust
fn json_keys(&self, run: &ApiRunId, prefix: Option<&str>, limit: Option<u64>)
    -> StrataResult<Vec<String>>;

fn json_list(&self, run: &ApiRunId, limit: Option<u64>)
    -> StrataResult<Vec<JsonDocInfo>>;
```

---

### Gap 6: `json_create` - Explicit Document Creation

**Priority:** P1

**What Primitive Has:**
```rust
fn create(&self, run_id: &RunId, doc_id: &JsonDocId, value: JsonValue) -> Result<Version>;
// Fails if document already exists
```

**What Substrate/Facade Expose:** Nothing (json_set creates implicitly)

**Why Important:**
- Cannot distinguish "create new" from "update existing"
- Cannot fail if document already exists
- Common pattern: "Create only if not exists"

**Proposed API:**
```rust
fn json_create(&self, run: &ApiRunId, key: &str, value: Value)
    -> StrataResult<Version>;  // Fails if exists
```

---

### Gap 7: `json_destroy` - Full Document Deletion

**Priority:** P1

**What Primitive Has:**
```rust
fn destroy(&self, run_id: &RunId, doc_id: &JsonDocId) -> Result<bool>;
```

**What Substrate/Facade Expose:** Only `json_delete` at path (not full document)

**Clarification:** `json_delete(key, "$")` at root path may work, but there's no explicit "delete entire document" operation.

**Proposed API:**
```rust
fn json_destroy(&self, run: &ApiRunId, key: &str) -> StrataResult<bool>;
```

---

### Gap 8: Batch Operations - Multi-Document Read/Write

**Priority:** P1

**What Exists:** Nothing at any layer

**Why Important:**
- Cannot efficiently read multiple documents
- Cannot atomically write multiple documents
- Reduces round trips

**Proposed API:**
```rust
fn json_mget(&self, run: &ApiRunId, keys: &[&str])
    -> StrataResult<Vec<Option<Versioned<Value>>>>;

fn json_mset(&self, run: &ApiRunId, entries: &[(&str, Value)])
    -> StrataResult<Vec<Version>>;
```

---

### Gap 9: `json_cas` - Compare-and-Set

**Priority:** P1

**What Exists:** Nothing at any layer

**Why Important:**
- Cannot do optimistic concurrency control
- Document versioning exists but no CAS operation

**Proposed API:**
```rust
fn json_cas(&self, run: &ApiRunId, key: &str, path: &str,
    expected_version: u64, value: Value) -> StrataResult<Option<Version>>;
```

---

## Part 4: Layer Inconsistencies (P1)

### Issue 1: Facade Has Features Substrate Doesn't

**The Problem:** Facade has convenience operations that aren't in Substrate:

| Operation | Facade | Substrate |
|-----------|--------|-----------|
| `json_type()` | ✅ | ❌ |
| `json_numincrby()` | ✅ | ❌ |
| `json_strappend()` | ✅ | ❌ |
| `json_arrappend()` | ✅ | ❌ |
| `json_arrlen()` | ✅ | ❌ |
| `json_objkeys()` | ✅ | ❌ |
| `json_objlen()` | ✅ | ❌ |

**Why Problem:**
- Substrate users don't get these convenience operations
- Violates the layer model (facade should be subset of substrate)
- Users choosing substrate for runs/versioning lose these features

**Fix:** Add these operations to Substrate trait

---

### Issue 2: Implicit vs Explicit Document Creation

**Facade/Substrate:** `json_set` creates document if not exists
**Primitive:** Has separate `create()` that fails if exists

**Why Problem:**
- No way to say "create only, fail if exists" at substrate/facade
- Accidental overwrites possible

---

## Part 5: API Design Issues (P1)

### Design Issue 1: Path Syntax Not Documented

**Current:** Paths are strings like `"$.user.name"` or `".user.name"`

**Problem:**
- Is `$` required? Optional?
- How to escape special characters?
- What's the exact syntax?

**Fix:** Document path syntax clearly, validate at API boundary

---

### Design Issue 2: Delete Return Value Confusing

**Current:**
```rust
fn json_delete(run, key, path) -> u64;  // Returns... what?
```

**Problem:** Unclear what the return value means (version? count? boolean as u64?)

**Fix:** Either return `Version` or `bool` with clear semantics

---

## Part 6: World-Class Features (P2)

### Gap 10: JSONPath Query Expressions

**Priority:** P2

**Problem:** No filtering queries like `$.items[?(@.price > 100)]`

**Industry Standard:** JSONPath (RFC 9535), JMESPath

**Proposed API:**
```rust
fn json_query(&self, run: &ApiRunId, key: &str, jsonpath: &str)
    -> StrataResult<Vec<Value>>;
```

---

### Gap 11: JSON Patch (RFC 6902)

**Priority:** P2

**Current:** Only RFC 7396 Merge Patch

**Missing:** RFC 6902 JSON Patch with operations:
- `add`, `remove`, `replace`, `move`, `copy`, `test`

**Why Useful:** More precise control over changes, atomic test-and-set

---

### Gap 12: Schema Validation

**Priority:** P2

**Current:** Only size/depth limits (16MB, 100 levels, 256 path segments)

**Missing:** JSON Schema validation

**Proposed API:**
```rust
fn json_set_schema(&self, run: &ApiRunId, key: &str, schema: Value)
    -> StrataResult<()>;

fn json_validate(&self, run: &ApiRunId, key: &str)
    -> StrataResult<ValidationResult>;
```

---

### Gap 13: Field-Level Indexing

**Priority:** P2

**Problem:** No indexing by field values

**Use Case:** "Find all documents where `status == 'active'`"

**Current:** Must use full-text search (imprecise) or scan all documents

---

## Priority Matrix

| ID | Issue | Priority | Effort | Category |
|----|-------|----------|--------|----------|
| Stub 1 | History stubbed | P0 | Medium | Stubbed |
| Gap 1 | Search hidden | P0 | Low | Hidden |
| Gap 5 | Document listing | P0 | Medium | Missing API |
| Gap 2 | Exists hidden | P1 | Low | Hidden |
| Gap 3 | Get version hidden | P1 | Low | Hidden |
| Gap 4 | Get doc metadata | P1 | Low | Hidden |
| Gap 6 | Explicit create | P1 | Low | Missing API |
| Gap 7 | Full delete | P1 | Low | Missing API |
| Gap 8 | Batch operations | P1 | Medium | Missing API |
| Gap 9 | CAS operation | P1 | Low | Missing API |
| Issue 1 | Facade > Substrate | P1 | Medium | Inconsistency |
| Issue 2 | Implicit create | P1 | Low | Inconsistency |
| Design 1 | Path syntax | P1 | Low | Design |
| Design 2 | Delete return | P1 | Low | Design |
| Gap 10 | JSONPath queries | P2 | High | World-Class |
| Gap 11 | JSON Patch | P2 | Medium | World-Class |
| Gap 12 | Schema validation | P2 | High | World-Class |
| Gap 13 | Field indexing | P2 | High | World-Class |

---

## Recommended Fix Order

### Phase 1: Quick Wins (Low Effort)
1. Expose `json_search` (Gap 1) - already implemented
2. Expose `json_exists` (Gap 2) - already implemented
3. Expose `json_get_version` (Gap 3) - already implemented
4. Expose `json_get_info` (Gap 4) - already implemented
5. Add `json_create` (Gap 6)
6. Add `json_destroy` (Gap 7)
7. Document path syntax (Design 1)
8. Fix delete return value (Design 2)

### Phase 2: Core Features (Medium Effort)
9. Implement `json_history` (Stub 1)
10. Add `json_keys` / `json_list` (Gap 5)
11. Add batch operations (Gap 8)
12. Add `json_cas` (Gap 9)
13. Promote facade operations to substrate (Issue 1)

### Phase 3: World-Class (High Effort)
14. JSONPath query expressions (Gap 10)
15. JSON Patch RFC 6902 (Gap 11)
16. Schema validation (Gap 12)
17. Field-level indexing (Gap 13)

---

## Comparison with Industry Standards

| Feature | Strata JSONStore | Redis JSON | MongoDB | CouchDB |
|---------|------------------|------------|---------|---------|
| Path-based get/set | ✅ | ✅ | ✅ | ✅ |
| Merge patch | ✅ | ✅ | ✅ | ✅ |
| **JSON Patch** | ❌ | ✅ | ✅ | ✅ |
| **Full-text search** | ❌ (hidden) | ❌ | ✅ | ✅ |
| Document listing | ❌ | ✅ | ✅ | ✅ |
| Batch ops | ❌ | ✅ | ✅ | ✅ |
| CAS/versioning | ❌ (no CAS) | ❌ | ✅ | ✅ |
| Type introspection | ✅ (facade) | ✅ | ✅ | ❌ |
| Atomic increment | ✅ (facade) | ✅ | ✅ | ❌ |
| Array operations | ✅ (facade) | ✅ | ✅ | ❌ |
| **JSONPath queries** | ❌ | ✅ | ✅ | ✅ |
| **Schema validation** | ❌ | ❌ | ✅ | ❌ |
| **Field indexing** | ❌ | ✅ | ✅ | ✅ |

**Strata's Strengths:**
- Good facade with Redis JSON-like convenience operations
- Full-text search implemented (just hidden)
- Document versioning built-in

**Strata's Gaps:**
- Document enumeration (critical)
- Search not exposed (quick fix)
- CAS for optimistic concurrency
- JSONPath filtering queries

---

## Key Finding

**JSONStore is more complete than other primitives** - the facade layer has useful convenience operations (type introspection, atomic increment, array append). However:

1. **Search is hidden** - Full-text search exists at primitive but not exposed
2. **Cannot list documents** - Same problem as KVStore's missing `kv_keys`
3. **Facade has more than Substrate** - Violates expected layer model
4. **No CAS** - Document versioning exists but no compare-and-set operation

**Priority:** Expose search, add document listing, promote facade ops to substrate.
