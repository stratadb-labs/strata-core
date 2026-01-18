# M5 Architecture Specification: JSON Primitive

**Version**: 2.0
**Status**: Implementation Ready
**Last Updated**: 2026-01-16

---

## Executive Summary

This document specifies the architecture for **Milestone 5 (M5): JSON Primitive** of the in-memory agent database. M5 introduces a native JSON primitive that **locks in mutation semantics** before durability formats stabilize.

**THIS DOCUMENT IS AUTHORITATIVE.** All M5 implementation must conform to this specification.

**Related Documents**:
- [M5 Implementation Plan](../milestones/M5/M5_IMPLEMENTATION_PLAN.md) - Epic/Story breakdown
- [M5 Integration Analysis](../milestones/M5/M5_INTEGRATION_ANALYSIS.md) - M1-M4 alignment analysis

**M5 Philosophy**:
> JSON is not a value type. It defines **mutation semantics**.
>
> M5 freezes the semantic model. M6+ optimizes the implementation.

**M5 Goals** (Semantic Lock-In):
- Define JSON as a first-class primitive with path-level operations
- Establish region-based conflict detection semantics
- Commit to patch-based WAL format
- Enable cross-primitive atomic transactions
- Maintain M4 performance on existing primitives

**M5 Non-Goals** (Deferred):
- Structural storage representation (M6+)
- Per-node versioning / subtree MVCC (M6+)
- Structural sharing (M6+)
- Efficient subtree snapshots (M6+)
- Diff operations (M7+)
- Indexes and queries (M11+)

**Critical Constraint**:
> M5 is a semantic milestone, not a performance milestone. JSON operations may be slower than optimal. That is acceptable. Correctness and semantic clarity matter more than speed. We can optimize later once semantics are frozen.

**Built on M1-M4**:
- M1 provides: Storage (UnifiedStore), WAL, Recovery
- M2 provides: OCC transactions, Snapshot isolation, Conflict detection
- M3 provides: Five primitives (KVStore, EventLog, StateCell, TraceStore, RunIndex)
- M4 provides: Durability modes (InMemory/Buffered/Strict), performance optimizations, ShardedStore
- M5 adds: JsonStore primitive with path-level mutation semantics

---

## Table of Contents

1. [Scope Boundaries](#1-scope-boundaries)
2. [THE SIX ARCHITECTURAL RULES](#2-the-six-architectural-rules-non-negotiable)
3. [Architecture Principles](#3-architecture-principles)
4. [Semantic Invariants](#4-semantic-invariants)
5. [JSON Document Model](#5-json-document-model)
6. [Path Semantics](#6-path-semantics)
7. [Array Mutation Rules](#7-array-mutation-rules)
8. [Conflict Detection](#8-conflict-detection)
9. [Versioning Model](#9-versioning-model)
10. [Snapshot Semantics](#10-snapshot-semantics)
11. [WAL Integration](#11-wal-integration)
12. [Patch Ordering Semantics](#12-patch-ordering-semantics)
13. [Transaction Integration](#13-transaction-integration)
14. [API Design](#14-api-design)
15. [Storage Layout](#15-storage-layout)
16. [Performance Characteristics](#16-performance-characteristics)
17. [Testing Strategy](#17-testing-strategy)
18. [Known Limitations](#18-known-limitations)
19. [Future Extension Points](#19-future-extension-points)
20. [Appendix](#20-appendix)

---

## 1. Scope Boundaries

### 1.1 What M5 IS

M5 is a **semantic lock-in milestone**. It defines:

| Aspect | M5 Commits To |
|--------|---------------|
| **Mutation model** | Path-level get/set/delete |
| **Conflict semantics** | Region-based (path overlap) |
| **WAL format** | Patch-based entries |
| **Transaction integration** | JSON participates in OCC |
| **Isolation** | Separate TypeTag, no KV interference |

### 1.2 What M5 is NOT

M5 is **not** an optimization milestone. These are explicitly deferred:

| Deferred Item | Why Deferred | Target Milestone |
|---------------|--------------|------------------|
| Structural tree representation | Optimization, not semantics | M6 |
| Per-node versioning | Requires MVCC infrastructure | M6+ |
| Structural sharing | Optimization | M6+ |
| Lazy subtree snapshots | Requires structural representation | M6+ |
| Diff operations | Convenience, not core semantics | M7 |
| Hybrid small/large format | Optimization | M6+ |
| Indexes | Advanced feature | M11 |
| Query language | Advanced feature | M11 |

### 1.3 The Risk We Are Avoiding

The previous draft attempted to design three systems simultaneously:
1. A JSON mutation model
2. A structural storage engine
3. A partial MVCC system

Each of those is a milestone on its own. M5 focuses on **(1) only**.

**Rule**: If a feature requires structural representation or per-node versioning, it is out of scope for M5.

---

## 2. THE SIX ARCHITECTURAL RULES (NON-NEGOTIABLE)

**These rules MUST be followed in ALL M5 implementation. Violating any of these is a blocking issue.**

These rules ensure M5 integrates properly with the existing M1-M4 architecture.

### Rule 1: JSON Lives Inside ShardedStore

> **Documents stored via `Key::new_json()` in existing ShardedStore. NO separate DashMap.**

```rust
// CORRECT: Use existing storage with JSON type tag
let key = Key::new_json(Namespace::for_run(run_id), &doc_id);
self.db.storage().put(key, serialized_doc)?;

// WRONG: Separate DashMap for JSON
struct JsonStore {
    documents: DashMap<JsonDocId, JsonDoc>,  // NEVER DO THIS
}
```

**Why**: This gives us sharding, versioning, snapshots, WAL, and recovery FOR FREE.

### Rule 2: JsonStore Is a Stateless Facade

> **JsonStore holds ONLY `Arc<Database>`. No internal state, no maps, no locks.**

```rust
// CORRECT: Stateless facade
#[derive(Clone)]
pub struct JsonStore {
    db: Arc<Database>,  // ONLY state
}

// WRONG: Holding additional state
pub struct JsonStore {
    db: Arc<Database>,
    cache: DashMap<Key, JsonDoc>,  // NEVER DO THIS
}
```

**Why**: Same pattern as KVStore, EventLog, StateCell, TraceStore, RunIndex. No cache invalidation complexity.

### Rule 3: JSON Extends TransactionContext

> **Add `JsonStoreExt` trait to TransactionContext. NO separate JsonTransaction type.**

```rust
// CORRECT: Extension trait on existing TransactionContext
pub trait JsonStoreExt {
    fn json_get(&self, key: &Key, path: &JsonPath) -> Result<Option<JsonValue>>;
    fn json_set(&mut self, key: &Key, path: &JsonPath, value: JsonValue) -> Result<()>;
}

impl JsonStoreExt for TransactionContext {
    // Implementation...
}

// WRONG: Separate transaction type
pub struct JsonTransaction {
    inner: TransactionContext,
}
```

**Why**: Enables cross-primitive atomic transactions "just works" without additional coordination.

### Rule 4: Path-Level Semantics in Validation, Not Storage

> **Storage sees whole documents. Path logic lives in JsonStoreExt methods.**

```rust
// CORRECT: Storage stores whole documents
storage.put(key, serialize_whole_doc(&doc))?;

// Path operations happen at the API layer
fn json_set(key, path, value) {
    let mut doc = self.load_doc(key)?;
    set_at_path(&mut doc.value, path, value)?;  // Path logic here
    self.store_doc(key, doc)?;
}

// WRONG: Storage understanding paths
storage.put_at_path(key, path, value)?;  // NEVER DO THIS
```

**Why**: Storage remains simple. Path semantics are isolated in the API layer.

### Rule 5: WAL Remains Unified

> **Add JSON entry types (0x20-0x23) to existing WALEntry enum. NO separate JSON WAL.**

```rust
// CORRECT: Extend existing WALEntry enum
pub enum WALEntry {
    // Existing entries...
    Put { key: Key, value: Value },
    Delete { key: Key },

    // NEW: JSON entries (0x20-0x23)
    JsonCreate { key: Key, doc: JsonDoc },           // 0x20
    JsonSet { key: Key, path: JsonPath, value: JsonValue, version: u64 }, // 0x21
    JsonDelete { key: Key, path: JsonPath, version: u64 },  // 0x22
    JsonDestroy { key: Key },                        // 0x23
}

// WRONG: Separate WAL
struct JsonWAL {
    entries: Vec<JsonWALEntry>,  // NEVER DO THIS
}
```

**Why**: Unified WAL maintains transaction atomicity across all primitives.

### Rule 6: JSON API Feels Like Other Primitives

> **Same patterns as KVStore, EventLog, etc.**

```rust
// CORRECT: Follows existing primitive patterns
let json = JsonStore::new(db.clone());
json.create(&run_id, &doc_id, initial_value)?;
json.set(&run_id, &doc_id, &path, new_value)?;
let value = json.get(&run_id, &doc_id, &path)?;

// Also works in transactions
db.transaction(run_id, |txn| {
    txn.json_set(&key, &path, value)?;
    txn.kv_put("related", related_value)?;  // Cross-primitive atomic
    Ok(())
})?;
```

**Why**: Consistent API reduces cognitive load and ensures proper integration.

---

## 3. Architecture Principles

### 3.1 M5-Specific Principles

1. **Semantics Over Speed**
   - M5 may be slow. That is acceptable.
   - Correctness and semantic clarity come first.
   - Performance optimization happens after semantics are frozen.

2. **Document-Granular Versioning**
   - Each document has ONE version number.
   - No per-path or per-node versions in M5.
   - Simplifies snapshot and conflict reasoning.

3. **Path-Granular Conflict Detection**
   - Reads and writes are tracked at path level.
   - Conflicts are detected based on path overlap.
   - But versioning remains document-level.

4. **Patch-Based Persistence**
   - WAL records patches (path + operation), not full documents.
   - This semantic decision is locked in M5.
   - Implementation can be optimized later.

5. **Weak Snapshot Isolation**
   - M5 provides snapshot consistency for **unmodified documents**.
   - If a document is modified after snapshot, reads fail.
   - This is explicit and documented, not hidden.
   - Full snapshot isolation (reading historical versions) is M6+.

6. **Idempotent Replay**
   - WAL patches must be safely replayable if applied twice.
   - Critical for recovery and partial replays.

7. **Backwards Compatible**
   - Existing primitives unchanged.
   - No performance regression on M4 benchmarks.
   - JSON tracking is lazy (only activated when JSON ops occur).

### 3.2 What JSON Is NOT

| Misconception | Reality |
|---------------|---------|
| "JSON replaces KV" | JSON is a separate primitive with different semantics |
| "JSON is for large documents" | JSON is for **structured mutable state** |
| "JSON needs a query language" | M5 provides path access only |
| "JSON values are opaque blobs" | JSON values have structure that affects conflict detection |
| "JSON has full snapshot isolation" | M5 has weak snapshots; full MVCC is M6+ |

---

## 4. Semantic Invariants (Never Change)

This section defines semantic invariants that **MUST hold for all future milestones** (M6+). These invariants define the meaning of the JSON primitive. Implementations may change, but these rules must not.

### 3.1 Path Semantics Are Positional, Not Identity-Based

Paths refer to **positions** within a JSON structure, not to stable identities.

- `$.items[0]` refers to "the element at index 0," not "the same logical object forever"
- Insertions and removals may change what a path refers to
- Paths are views, not references

**This implies**:
- Structural modifications may invalidate previously read paths
- Conflict detection must treat paths conservatively
- Stable identity semantics (node IDs) are a future extension (M6+)

**This invariant must not change.**

### 3.2 JSON Mutations Are Path-Based, Not Value-Based

All JSON writes are defined as mutations to paths, not replacements of opaque values.

Even if the internal implementation uses blobs, the semantic model is:
- Set value at path
- Delete value at path
- Replace subtree at path

**This enables**:
- Patch-based WAL
- Region-based conflict detection
- Structural replay
- Future subtree MVCC

**Future optimizations must preserve this semantic model.**

### 3.3 Conflict Detection Is Region-Based

Two JSON operations conflict if and only if:
1. They target the same document, AND
2. Their paths overlap (ancestor, descendant, or equal)

This is **independent of**:
- Storage format
- Versioning strategy
- Indexing strategy
- Snapshot implementation

**This invariant must not change.**

### 3.4 WAL Is Patch-Based, Not Value-Based

WAL entries for JSON **MUST** describe mutations, not full-document overwrites.

Even if the storage layer writes full blobs, the WAL format must remain patch-based.

**This invariant enables**:
- Efficient replication
- Streaming replays
- Partial recovery
- Structural evolution

**This must not change.**

### 3.5 JSON Must Participate in Cross-Primitive Atomicity

JSON operations must obey the same atomicity, isolation, and rollback semantics as other primitives.

This includes:
- Atomic commit
- Atomic rollback
- Conflict-driven aborts
- Read-your-writes

**This invariant must not change.**

### 3.6 JsonPath Tracking Is Solely for Conflict Detection (M5)

**In M5, JsonPath tracking exists only to support region-based conflict detection. It is not used for snapshot versioning or historical reads.**

This prevents accidental semantic drift. Specifically:

- **NOT for snapshot versioning**: M5 uses document-level versions, not per-path versions
- **NOT for historical reads**: M5 does not support point-in-time queries at path granularity
- **NOT for change tracking**: Path history is not maintained beyond conflict detection scope
- **NOT persisted to WAL**: Conflict detection is transient (in-memory during transaction); rejected transactions are rolled back and not written to WAL. Only committed operations are persisted.

**Why this matters**:
- Keeps M5 implementation simple and focused
- Prevents premature complexity
- Makes the M5→M6 upgrade path clear (M6+ may add per-path versioning)
- Avoids confusion about what JsonPath tracking guarantees

**This is an M5-specific constraint, not a permanent invariant.** Future milestones (M6+) may extend JsonPath tracking for additional purposes such as subtree MVCC or path-level historical queries.

---

## 5. JSON Document Model

### 4.1 Document Identity

```rust
/// Unique identifier for a JSON document within a run
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct JsonDocId(Uuid);

impl JsonDocId {
    pub fn new() -> Self {
        JsonDocId(Uuid::new_v4())
    }
}
```

**Document Lifecycle**:
1. `create()` - Allocates new JsonDocId, stores initial value
2. `get/set/delete` - Operate on existing document
3. `delete_document()` - Removes entire document

### 4.2 Supported JSON Types

```rust
/// JSON value types supported by JsonStore
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum JsonValue {
    Null,
    Bool(bool),
    Number(JsonNumber),
    String(String),
    Array(Vec<JsonValue>),
    Object(IndexMap<String, JsonValue>),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum JsonNumber {
    Int(i64),
    Float(f64),
}
```

We use `IndexMap` to preserve key insertion order (matching JSON semantics).

### 4.3 M5 Storage Format

**M5 uses a simple blob format**:

```rust
/// M5 JSON document storage (simple, correct, not optimized)
struct JsonDoc {
    /// Document ID
    doc_id: JsonDocId,
    /// Serialized JSON (msgpack)
    data: Vec<u8>,
    /// Document version (increments on ANY change)
    version: u64,
    /// Creation timestamp
    created_at: Timestamp,
    /// Last modified timestamp
    modified_at: Timestamp,
}
```

**Operations**:
- `get(path)`: Deserialize → traverse → return value
- `set(path, value)`: Deserialize → modify → serialize → store
- `delete(path)`: Deserialize → remove → serialize → store

**This is intentionally simple**. Structural representation is M6+.

### 4.4 Document Size Limits

| Limit | Value | Rationale |
|-------|-------|-----------|
| Max document size | 16 MB | Prevents memory issues |
| Max nesting depth | 100 levels | Prevents stack overflow |
| Max path length | 256 segments | Practical limit |
| Max array size | 1M elements | Practical limit |

---

## 6. Path Semantics

### 5.1 Path Representation

```rust
/// A path into a JSON document
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct JsonPath {
    segments: Vec<PathSegment>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum PathSegment {
    /// Object key: `.foo`
    Key(String),
    /// Array index: `[0]`
    Index(usize),
}
```

### 5.2 Path Syntax (M5 Subset)

M5 supports simple path syntax only:

| Syntax | Meaning | Example |
|--------|---------|---------|
| `.key` | Object property | `.user` |
| `[n]` | Array index | `[0]` |
| `.key1.key2` | Nested property | `.user.name` |
| `.key[n]` | Property then index | `.items[0]` |
| (empty) | Root | `` |

**Not supported in M5**:
- Wildcards (`.*`, `[*]`)
- Filters (`[?(@.price < 10)]`)
- Slices (`[0:5]`)
- Recursive descent (`..`)

### 5.3 Critical: Paths Are Logical Views, Not Physical Identities

**This is the most important semantic decision in M5.**

Paths like `$.items[0]` refer to **positions**, not **identities**.

If array `$.items` is `[A, B, C]`:
- `$.items[0]` refers to `A`
- Insert `X` at index 0 → array becomes `[X, A, B, C]`
- Now `$.items[0]` refers to `X`, `$.items[1]` refers to `A`

**Paths are unstable under structural modification.**

This has critical implications for conflict detection (Section 7).

### 5.4 Path Operations

```rust
impl JsonPath {
    /// Root path (empty)
    pub fn root() -> Self;

    /// Parse from string: "foo.bar[0].baz"
    pub fn parse(s: &str) -> Result<Self, PathParseError>;

    /// Append a key segment
    pub fn key(self, key: impl Into<String>) -> Self;

    /// Append an index segment
    pub fn index(self, idx: usize) -> Self;

    /// Check if this path is an ancestor of another (or equal)
    pub fn is_ancestor_of(&self, other: &JsonPath) -> bool;

    /// Check if this path is a descendant of another (or equal)
    pub fn is_descendant_of(&self, other: &JsonPath) -> bool;

    /// Check if two paths overlap (one is ancestor/descendant of other)
    pub fn overlaps(&self, other: &JsonPath) -> bool {
        self.is_ancestor_of(other) || self.is_descendant_of(other)
    }

    /// Get parent path (None if root)
    pub fn parent(&self) -> Option<JsonPath>;
}
```

---

## 7. Array Mutation Rules (M5)

### 6.1 Arrays Are Positional

Array elements in M5 have **no stable identity**.

Paths like `$.items[0]` refer to a **position**, not an object.

### 6.2 Consequence: Structural Instability

Any mutation that changes array shape may invalidate all positional paths under it.

Examples of shape-changing mutations:
- Insert
- Remove
- Shift
- Reorder

### 6.3 M5 Restriction: No Structural Array Mutations

To avoid ambiguous semantics, M5 does **not** provide structural array mutation APIs.

**M5 does NOT provide**:
- `array_insert`
- `array_remove`
- `array_push`
- `array_pop`
- `array_move`

**Instead**, arrays must be modified via read-modify-write:

```rust
let arr = json.get(run_id, doc_id, &path("$.items"))?;
let mut arr = arr.as_array_mut().unwrap();
arr.push(new_value);
json.set(run_id, doc_id, &path("$.items"), arr)?;
```

**This restriction is intentional.**

### 6.4 Future Direction (M6+)

M6+ may introduce:
- Stable element identities (NodeId)
- Structural array mutations
- Index-safe path semantics

But those features will **not change the M5 semantic contract**.

---

## 8. Conflict Detection

### 7.1 Region-Based Conflict Detection

JSON uses **region-based** conflict detection based on path overlap.

**Core Rule**: Two operations conflict if their paths overlap (one is ancestor/descendant of the other, or they are the same path).

```rust
/// Check if two JSON operations conflict
fn operations_conflict(op1: &JsonOp, op2: &JsonOp) -> bool {
    // Different documents never conflict
    if op1.doc_id != op2.doc_id {
        return false;
    }

    // Same document: check path overlap
    op1.path.overlaps(&op2.path)
}
```

### 7.2 Conflict Matrix

| Write A | Write B | Conflict? | Reason |
|---------|---------|-----------|--------|
| `$.a.b` | `$.a.c` | **No** | Siblings, no overlap |
| `$.a.b` | `$.a.b` | **Yes** | Same path |
| `$.a` | `$.a.b` | **Yes** | A is ancestor of B |
| `$.a.b` | `$.a` | **Yes** | B is ancestor of A |
| `$.x` | `$.y` | **No** | Different subtrees |
| `$` (root) | `$.anything` | **Yes** | Root overlaps everything |

### 7.3 Array Operations and Structural Shifts

**Critical**: Array mutations cause structural shifts that affect path stability.

**Rule**: Any array mutation (insert, remove) conflicts with ALL paths that traverse through that array.

| Operation A | Operation B | Conflict? | Reason |
|-------------|-------------|-----------|--------|
| `insert($.items, 0, X)` | `set($.items[1].price, 10)` | **Yes** | Insert shifts indices |
| `remove($.items, 0)` | `get($.items[0])` | **Yes** | Remove shifts indices |
| `insert($.items, 0, X)` | `set($.other, Y)` | **No** | Different subtrees |
| `push($.items, X)` | `set($.items[0], Y)` | **Yes** | Array mutation |

**Implementation**:

```rust
/// Array mutation affects all paths through that array
fn array_mutation_conflicts(
    array_path: &JsonPath,
    other_op: &JsonOp,
) -> bool {
    // Array mutation conflicts with any path that:
    // 1. Is the array itself
    // 2. Is a descendant of the array (traverses through it)
    // 3. Is an ancestor of the array
    array_path.overlaps(&other_op.path)
}
```

**Why this conservative rule?**

Because paths are positional, not identity-based. `$.items[1]` before an insert at index 0 refers to a different element than `$.items[1]` after. Without stable node identities (M6+), we cannot safely allow concurrent array mutations and element access.

### 7.4 Read-Write Conflict Detection

For OCC validation, reads at a path conflict with writes to overlapping paths:

```rust
/// JSON read set entry
struct JsonReadEntry {
    doc_id: JsonDocId,
    path: JsonPath,
    doc_version: u64,  // Document version when read
}

/// Check read-write conflict
fn read_write_conflicts(
    read: &JsonReadEntry,
    write: &JsonWriteEntry,
) -> bool {
    if read.doc_id != write.doc_id {
        return false;
    }
    read.path.overlaps(&write.path)
}
```

### 7.5 Document Version Check

In addition to path-level conflict detection, M5 performs a **document version check**:

```rust
/// Validate JSON reads at commit time
fn validate_json_reads(
    read_set: &[JsonReadEntry],
    storage: &Storage,
) -> Result<(), ConflictError> {
    for read in read_set {
        let current_version = storage.get_json_doc_version(&read.doc_id)?;
        if current_version != read.doc_version {
            // Document was modified - check if our paths overlap
            // with the modification
            //
            // M5 LIMITATION: We don't track which paths were modified,
            // only that the document changed. So we conservatively
            // fail if the document version changed.
            //
            // M6+ with per-path versioning can be more precise.
            return Err(ConflictError::JsonDocumentModified {
                doc_id: read.doc_id,
                read_version: read.doc_version,
                current_version,
            });
        }
    }
    Ok(())
}
```

**M5 Limitation**: Because we don't have per-path versions, any document modification fails all reads from that document. This is conservative but correct. M6+ can be more precise.

---

## 9. Versioning Model

### 8.1 Document-Granular Versioning

**M5 uses document-level versioning only.**

```rust
struct JsonDoc {
    doc_id: JsonDocId,
    data: Vec<u8>,
    version: u64,  // Single version for entire document
}
```

- Version increments on ANY change to the document
- No per-path or per-node versions
- Simplifies reasoning about snapshots and conflicts

### 8.2 Version Allocation

Versions are allocated from the global version counter:

```rust
impl JsonStore {
    fn allocate_version(&self) -> u64 {
        self.db.next_version()  // Same counter as KV, Event, etc.
    }
}
```

This ensures global ordering across all primitives.

### 8.3 Why Not Per-Path Versioning in M5?

Per-path versioning requires:
- Version propagation rules (which ancestors get updated?)
- Version dominance rules (which version wins?)
- Snapshot read rules (how to find version at snapshot time?)
- Partial ordering semantics
- Read-your-writes across version boundaries

Each of these is a research-grade problem. M5 avoids this complexity by using document-level versions.

**Trade-off**: More false conflicts (document changed = all reads conflict), but simpler and provably correct.

---

## 10. Snapshot Semantics

### 9.1 M5 Snapshot Guarantee

**M5 provides weak snapshot isolation for JSON:**

> Reads see a consistent view of documents **that have not been modified since the snapshot was taken**.

If a document is modified after the transaction's snapshot, reads from that document **fail** rather than returning stale data.

### 9.2 Why Weak Snapshots?

Full snapshot isolation requires one of:
- Version chains (MVCC)
- Copy-on-write subtrees
- Structural persistence

These require structural representation, which is M6+.

M5's weak snapshots are:
- Simple to implement
- Provably correct (no stale reads, no torn reads)
- Explicit about limitations

### 9.3 Snapshot Read Behavior

```rust
impl JsonSnapshot {
    pub fn get(
        &self,
        doc_id: &JsonDocId,
        path: &JsonPath,
    ) -> Result<Option<JsonValue>, JsonError> {
        // Get current document
        let doc = self.storage.get_json_doc(doc_id)?;

        match doc {
            None => Ok(None),
            Some(doc) => {
                // Check version
                if doc.version > self.snapshot_version {
                    // Document modified after snapshot
                    return Err(JsonError::DocumentModifiedAfterSnapshot {
                        doc_id: *doc_id,
                        snapshot_version: self.snapshot_version,
                        current_version: doc.version,
                    });
                }

                // Safe to read
                let value = deserialize_and_traverse(&doc.data, path)?;
                Ok(value)
            }
        }
    }
}
```

### 9.4 Handling Snapshot Failures

When a snapshot read fails due to concurrent modification:

```rust
// Application code
let result = db.transaction(run_id, |txn| {
    let value = txn.json_get(doc_id, &path)?;
    // ... use value ...
    Ok(value)
});

match result {
    Ok(v) => { /* success */ }
    Err(Error::Conflict(_)) => {
        // Retry transaction
    }
    Err(e) => { /* other error */ }
}
```

This is standard OCC behavior. The application retries on conflict.

### 9.5 Explicit Documentation

The API documentation must clearly state:

```rust
/// Get value at path within a transaction.
///
/// # Snapshot Behavior (M5)
///
/// M5 provides weak snapshot isolation. If the document has been
/// modified by another transaction after this transaction's snapshot
/// was taken, this method returns `Err(JsonError::DocumentModifiedAfterSnapshot)`.
///
/// This is different from full MVCC where you would see the old value.
/// Full snapshot isolation is planned for M6+.
///
/// # Conflict Behavior
///
/// This read is tracked for conflict detection. If another transaction
/// modifies any overlapping path before this transaction commits,
/// commit will fail with a conflict error.
pub fn json_get(&mut self, doc_id: &JsonDocId, path: &JsonPath) -> Result<Option<JsonValue>>;
```

---

## 11. WAL Integration

### 10.1 Patch-Based WAL Entries

**This is a non-negotiable semantic decision.**

WAL entries record patches, not full documents:

```rust
/// WAL entries for JSON operations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum JsonWalEntry {
    /// Create new document
    Create {
        run_id: RunId,
        doc_id: JsonDocId,
        value: JsonValue,
        version: u64,
        timestamp: Timestamp,
    },

    /// Set value at path
    Set {
        run_id: RunId,
        doc_id: JsonDocId,
        path: JsonPath,
        value: JsonValue,
        version: u64,
    },

    /// Delete value at path
    Delete {
        run_id: RunId,
        doc_id: JsonDocId,
        path: JsonPath,
        version: u64,
    },

    /// Delete entire document
    DeleteDoc {
        run_id: RunId,
        doc_id: JsonDocId,
        version: u64,
    },
}
```

### 10.2 Why Patch-Based?

Once you commit to JSON as a primitive, full-document WAL entries are a dead end:

| Approach | WAL Size for 1MB Doc + 1 Field Change |
|----------|---------------------------------------|
| Full document | ~1 MB |
| Patch | ~100 bytes |

Patch-based WAL is 10,000× more efficient for typical workloads.

### 10.3 Idempotent Replay

**Critical requirement**: Patches must be safely replayable if applied twice.

```rust
/// Replay a JSON WAL entry
fn replay_json_entry(
    storage: &mut Storage,
    entry: &JsonWalEntry,
) -> Result<()> {
    match entry {
        JsonWalEntry::Create { doc_id, value, version, .. } => {
            // Idempotent: if doc exists with same version, skip
            if let Some(existing) = storage.get_json_doc(doc_id)? {
                if existing.version >= *version {
                    return Ok(()); // Already applied
                }
            }
            storage.json_create(doc_id, value, *version)?;
        }

        JsonWalEntry::Set { doc_id, path, value, version, .. } => {
            // Idempotent: if doc version >= entry version, skip
            if let Some(existing) = storage.get_json_doc(doc_id)? {
                if existing.version >= *version {
                    return Ok(()); // Already applied
                }
            }
            storage.json_set(doc_id, path, value, *version)?;
        }

        // ... similar for Delete, DeleteDoc
    }
    Ok(())
}
```

### 10.4 WAL Entry Type Tags

```rust
// Existing WAL entry type tags
const WAL_BEGIN_TXN: u8 = 0x01;
const WAL_WRITE: u8 = 0x02;
const WAL_DELETE: u8 = 0x03;
const WAL_COMMIT_TXN: u8 = 0x04;
const WAL_ABORT_TXN: u8 = 0x05;
const WAL_CHECKPOINT: u8 = 0x06;

// New: JSON operations (0x20 range)
const WAL_JSON_CREATE: u8 = 0x20;
const WAL_JSON_SET: u8 = 0x21;
const WAL_JSON_DELETE: u8 = 0x22;
const WAL_JSON_DELETE_DOC: u8 = 0x23;
```

### 10.5 Integration with Existing WAL

JSON WAL entries extend the existing `WALEntry` enum:

```rust
pub enum WALEntry {
    // Existing entries (unchanged)
    BeginTxn { ... },
    Write { ... },
    Delete { ... },
    CommitTxn { ... },
    AbortTxn { ... },
    Checkpoint { ... },

    // New: JSON operations
    Json(JsonWalEntry),
}
```

---

## 12. Patch Ordering Semantics

Within a single transaction, JSON patches are **ordered** and **sequentially applied**.

### 11.1 Sequential Application

Given a patch list:

```
[P1, P2, P3]
```

The semantic meaning is:
1. Apply P1
2. Then apply P2 on top of the result of P1
3. Then apply P3 on top of the result of P2

Each patch sees the effects of all prior patches in the same transaction.

### 11.2 Intra-Transaction Patch Conflicts

If two patches in the same transaction conflict, the transaction is **invalid**.

| Patch A | Patch B | Allowed? | Reason |
|---------|---------|----------|--------|
| `Set($.a.b, 1)` | `Set($.a.c, 2)` | **Yes** | Disjoint paths |
| `Set($.a, {...})` | `Set($.a.b, 1)` | **No** | Overlapping paths |
| `Delete($.a)` | `Set($.a.b, 1)` | **No** | Overlapping paths |
| `Set($.a.b, 1)` | `Delete($.a.b)` | **No** | Same path |

**Rule**: If any two patches in the same transaction overlap, the transaction must fail.

This prevents ambiguous semantics.

### 11.3 Ordering Matters

Patch ordering is **semantically meaningful**.

These are NOT equivalent:

```rust
// Transaction A
json.set(doc_id, &path("$.a"), json!({}))?;
json.set(doc_id, &path("$.a.b"), json!(1))?;
// Result: $.a = { "b": 1 }
```

vs

```rust
// Transaction B
json.set(doc_id, &path("$.a.b"), json!(1))?;
json.set(doc_id, &path("$.a"), json!({}))?;
// Result: $.a = {} (the second set overwrites)
```

**Therefore**: JSON patch lists are not commutative. They are **ordered programs**.

### 11.4 WAL Ordering and Replay Guarantees

**WAL Is Order-Dependent**

JSON WAL entries are **not commutative**. They must be replayed in the exact order they were written.

**Idempotence, Not Reordering**

JSON WAL entries are required to be:
- **Idempotent**: Safe to apply twice
- **Not reorderable**: Order matters

| Property | Required |
|----------|----------|
| Apply twice | Yes |
| Apply out of order | **No** |
| Skip if already applied | Yes |

**WAL Replay Contract**

Replay semantics:
1. WAL entries must be replayed in **strict order**
2. Each entry must be applied **at most once**
3. If an entry's version has already been applied, it must be **skipped**
4. Replay must be **deterministic**

---

## 13. Transaction Integration

### 12.1 Transaction Context Extension

```rust
impl TransactionContext {
    // Existing fields (unchanged)...

    // New: JSON tracking (lazy - only allocated when needed)
    json_read_set: Option<Vec<JsonReadEntry>>,
    json_write_set: Option<Vec<JsonWriteEntry>>,
}

struct JsonReadEntry {
    doc_id: JsonDocId,
    path: JsonPath,
    doc_version: u64,
}

struct JsonWriteEntry {
    doc_id: JsonDocId,
    path: JsonPath,
    operation: JsonWriteOp,
}

enum JsonWriteOp {
    Set(JsonValue),
    Delete,
}
```

### 12.2 Lazy Activation

**Critical for non-regression**: JSON tracking is only allocated when JSON operations occur.

```rust
impl TransactionContext {
    fn ensure_json_tracking(&mut self) {
        if self.json_read_set.is_none() {
            self.json_read_set = Some(Vec::new());
            self.json_write_set = Some(Vec::new());
        }
    }

    pub fn json_get(&mut self, doc_id: &JsonDocId, path: &JsonPath) -> Result<...> {
        self.ensure_json_tracking();  // Lazy init
        // ...
    }
}
```

Non-JSON transactions pay zero overhead for JSON support.

### 12.3 Read-Your-Writes

```rust
impl TransactionContext {
    pub fn json_get(
        &mut self,
        doc_id: &JsonDocId,
        path: &JsonPath,
    ) -> Result<Option<JsonValue>> {
        self.ensure_json_tracking();

        // Check write set first (read-your-writes)
        if let Some(write_set) = &self.json_write_set {
            // Find most recent write that affects this path
            for entry in write_set.iter().rev() {
                if entry.doc_id == *doc_id && entry.path.is_ancestor_of(path) {
                    return self.resolve_from_write(entry, path);
                }
            }
        }

        // Read from snapshot
        let result = self.snapshot.json_get(doc_id, path)?;

        // Track read
        if let Some(read_set) = &mut self.json_read_set {
            let doc_version = self.snapshot.get_json_doc_version(doc_id)?;
            read_set.push(JsonReadEntry {
                doc_id: *doc_id,
                path: path.clone(),
                doc_version,
            });
        }

        Ok(result)
    }
}
```

### 12.4 Commit Validation

```rust
impl TransactionContext {
    fn validate_json(&self, storage: &Storage) -> Result<(), ConflictError> {
        let Some(read_set) = &self.json_read_set else {
            return Ok(()); // No JSON operations
        };

        for read in read_set {
            let current_version = storage.get_json_doc_version(&read.doc_id)?;

            if current_version != read.doc_version {
                // Document modified - conflict
                // (M5 is conservative: any doc change = conflict)
                return Err(ConflictError::JsonConflict {
                    doc_id: read.doc_id,
                    path: read.path.clone(),
                    read_version: read.doc_version,
                    current_version,
                });
            }
        }

        Ok(())
    }
}
```

### 12.5 Cross-Primitive Transactions

JSON operations can be combined with other primitives:

```rust
db.transaction(run_id, |txn| {
    // KV operation
    txn.kv_put("config", Value::String("enabled".into()))?;

    // JSON operation
    let doc_id = txn.json_create(json!({ "status": "active" }))?;
    txn.json_set(doc_id, &path("$.count"), json!(0))?;

    // Event operation
    txn.event_append("doc_created", json!({ "doc_id": doc_id }))?;

    Ok(doc_id)
})?;
// All commit atomically or none do
```

---

## 14. API Design

### 13.1 Core M5 API

**Minimal API that locks in semantics:**

```rust
/// JSON document store primitive
pub struct JsonStore {
    db: Arc<Database>,
}

impl JsonStore {
    /// Create a new JsonStore instance
    pub fn new(db: Arc<Database>) -> Self;

    // ========== Document Lifecycle ==========

    /// Create a new JSON document
    pub fn create(
        &self,
        run_id: RunId,
        value: JsonValue,
    ) -> Result<JsonDocId>;

    /// Delete an entire document
    pub fn delete_document(
        &self,
        run_id: RunId,
        doc_id: JsonDocId,
    ) -> Result<()>;

    /// Check if document exists
    pub fn exists(
        &self,
        run_id: RunId,
        doc_id: JsonDocId,
    ) -> Result<bool>;

    // ========== Path Operations ==========

    /// Get value at path
    pub fn get(
        &self,
        run_id: RunId,
        doc_id: JsonDocId,
        path: &JsonPath,
    ) -> Result<Option<JsonValue>>;

    /// Set value at path
    pub fn set(
        &self,
        run_id: RunId,
        doc_id: JsonDocId,
        path: &JsonPath,
        value: JsonValue,
    ) -> Result<()>;

    /// Delete value at path
    pub fn delete(
        &self,
        run_id: RunId,
        doc_id: JsonDocId,
        path: &JsonPath,
    ) -> Result<()>;

    // ========== Batch Operations ==========

    /// Apply multiple patches atomically
    pub fn patch(
        &self,
        run_id: RunId,
        doc_id: JsonDocId,
        patches: Vec<JsonPatch>,
    ) -> Result<()>;

    // ========== CAS ==========

    /// Compare-and-swap at path (document-level version)
    pub fn cas(
        &self,
        run_id: RunId,
        doc_id: JsonDocId,
        path: &JsonPath,
        expected_version: u64,
        new_value: JsonValue,
    ) -> Result<CasResult>;

    /// Get current document version
    pub fn version(
        &self,
        run_id: RunId,
        doc_id: JsonDocId,
    ) -> Result<u64>;
}
```

### 13.2 What Is NOT in M5 API

| Excluded | Reason | Target |
|----------|--------|--------|
| `array_insert()` | Requires stable identity handling | M6+ |
| `array_remove()` | Requires stable identity handling | M6+ |
| `array_push()` | Can use `set(path, array_with_new_element)` | M6+ convenience |
| `diff()` | Requires structural representation | M7 |
| `metadata()` | Convenience, not core semantic | M6+ |
| `list_documents()` | Convenience, not core semantic | M6+ |
| `list_keys()` | Convenience, not core semantic | M6+ |

**Note on arrays**: M5 supports arrays in JSON values, but dedicated array mutation operations (insert/remove at index) are deferred because they interact with path stability in complex ways. In M5, modify arrays by reading the full array, modifying it, and writing it back.

### 13.3 JsonPatch Type

```rust
/// A patch operation on a JSON document
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum JsonPatch {
    /// Set value at path
    Set { path: JsonPath, value: JsonValue },
    /// Delete value at path
    Delete { path: JsonPath },
}

impl JsonPatch {
    pub fn path(&self) -> &JsonPath;

    pub fn conflicts_with(&self, other: &JsonPatch) -> bool {
        self.path().overlaps(other.path())
    }
}
```

### 13.4 Error Types

```rust
#[derive(Debug, thiserror::Error)]
pub enum JsonError {
    #[error("document not found: {0}")]
    DocumentNotFound(JsonDocId),

    #[error("path not found: {0}")]
    PathNotFound(String),

    #[error("type mismatch at path: expected {expected}, found {found}")]
    TypeMismatch { path: String, expected: String, found: String },

    #[error("invalid path syntax: {0}")]
    InvalidPath(String),

    #[error("document too large: {size} bytes (max {max})")]
    DocumentTooLarge { size: u64, max: u64 },

    #[error("nesting too deep: {depth} levels (max {max})")]
    NestingTooDeep { depth: usize, max: usize },

    #[error("CAS failed: expected version {expected}, found {found}")]
    CasFailed { expected: u64, found: u64 },

    #[error("document modified after snapshot: snapshot={snapshot}, current={current}")]
    DocumentModifiedAfterSnapshot { doc_id: JsonDocId, snapshot: u64, current: u64 },

    #[error("document already exists: {0}")]
    DocumentExists(JsonDocId),
}
```

---

## 15. Storage Layout

### 15.1 TypeTag

```rust
pub enum TypeTag {
    KV = 0x01,
    Event = 0x02,
    State = 0x03,
    Trace = 0x04,
    Run = 0x05,
    // 0x06-0x0F reserved for future core primitives
    Vector = 0x10,    // Reserved for M8
    Json = 0x11,      // NEW in M5
}
```

**Important**: JSON uses `0x11` (not `0x06`) to leave room for future core primitive types in the `0x06-0x0F` range.

### 15.2 Storage Keys

| Key Pattern | Purpose | Value |
|-------------|---------|-------|
| `<ns>:Json:<doc_id>` | Document data + metadata | Serialized `JsonDoc` |
| `<ns>:Json:__index__` | Document listing (optional) | `Vec<JsonDocId>` |

### 15.3 Serialization

M5 uses MessagePack for JSON document serialization:
- Compact binary format
- Preserves JSON semantics
- Fast serialization/deserialization

```rust
fn serialize_json_doc(doc: &JsonDoc) -> Vec<u8> {
    rmp_serde::to_vec(doc).expect("serialization should not fail")
}

fn deserialize_json_doc(bytes: &[u8]) -> Result<JsonDoc, JsonError> {
    rmp_serde::from_slice(bytes).map_err(|e| JsonError::DeserializationFailed(e))
}
```

---

## 16. Performance Characteristics

### 15.1 M5 Performance Expectations

**M5 prioritizes correctness over speed.**

| Operation | Expected Latency | Notes |
|-----------|------------------|-------|
| `create()` | 50-100 µs | Serialize + store |
| `get(shallow path)` | 30-50 µs | Deserialize + traverse |
| `get(deep path)` | 50-100 µs | More traversal |
| `set(any path)` | 100-200 µs | Deserialize + modify + serialize + store |
| `patch(n ops)` | 100 + 50n µs | Multiple modifications |

These are **acceptable for M5**. Optimization is M6+.

### 15.2 Non-Regression Requirement

**Critical**: M5 must NOT degrade performance of existing primitives.

| Primitive | M4 Target | M5 Requirement |
|-----------|-----------|----------------|
| KVStore get | < 5 µs | < 5 µs |
| KVStore put | < 8 µs | < 8 µs |
| EventLog append | < 10 µs | < 10 µs |
| StateCell cas | < 10 µs | < 10 µs |

**How achieved**:
- Separate TypeTag (no key collision)
- Separate code paths (no hot-path impact)
- Lazy JSON tracking (zero overhead for non-JSON transactions)
- Additive WAL entries (no existing entry changes)

### 15.3 Benchmark Requirements

```rust
#[test]
fn test_json_does_not_regress_kv() {
    let db = test_db();
    let kv = KVStore::new(db.clone());
    let json = JsonStore::new(db.clone());

    // Baseline KV performance
    let kv_before = benchmark(|| kv.put(run_id, "k", value.clone()));

    // Create some JSON documents
    for _ in 0..1000 {
        json.create(run_id, json!({"x": 1}))?;
    }

    // KV performance after JSON operations
    let kv_after = benchmark(|| kv.put(run_id, "k2", value.clone()));

    // Must be within 5%
    assert!(kv_after < kv_before * 1.05);
}
```

---

## 17. Testing Strategy

### 16.1 Unit Tests

```rust
// Document lifecycle
#[test] fn test_create_and_get() { ... }
#[test] fn test_delete_document() { ... }
#[test] fn test_document_not_found() { ... }

// Path operations
#[test] fn test_get_shallow_path() { ... }
#[test] fn test_get_deep_path() { ... }
#[test] fn test_set_creates_intermediate() { ... }
#[test] fn test_delete_path() { ... }
#[test] fn test_path_not_found() { ... }
#[test] fn test_type_mismatch() { ... }

// CAS
#[test] fn test_cas_success() { ... }
#[test] fn test_cas_version_mismatch() { ... }

// Patch
#[test] fn test_patch_multiple_ops() { ... }
#[test] fn test_patch_atomic() { ... }
```

### 16.2 Conflict Detection Tests

```rust
#[test]
fn test_sibling_paths_no_conflict() {
    // $.a.b and $.a.c should not conflict
}

#[test]
fn test_ancestor_descendant_conflict() {
    // $.a and $.a.b should conflict
}

#[test]
fn test_same_path_conflict() {
    // $.a.b and $.a.b should conflict
}

#[test]
fn test_different_documents_no_conflict() {
    // doc1.$.a and doc2.$.a should not conflict
}

#[test]
fn test_root_conflicts_with_everything() {
    // $ and $.anything should conflict
}
```

### 16.3 Snapshot Tests

```rust
#[test]
fn test_snapshot_sees_committed_state() { ... }

#[test]
fn test_snapshot_fails_on_concurrent_modification() {
    // Start txn1, read doc
    // Commit txn2 that modifies doc
    // txn1 read should fail (weak snapshot)
}

#[test]
fn test_read_your_writes() {
    // Within same txn, reads see uncommitted writes
}
```

### 16.4 WAL Tests

```rust
#[test]
fn test_wal_replay_creates_doc() { ... }

#[test]
fn test_wal_replay_sets_path() { ... }

#[test]
fn test_wal_replay_idempotent() {
    // Applying same entry twice should be safe
}

#[test]
fn test_wal_recovery_after_crash() {
    // Create doc, set paths, "crash", recover, verify state
}
```

### 16.5 Cross-Primitive Tests

```rust
#[test]
fn test_json_and_kv_atomic() {
    // Both commit or both rollback
}

#[test]
fn test_json_conflict_rolls_back_kv_too() {
    // If JSON conflicts, KV writes in same txn also roll back
}
```

---

## 18. Known Limitations

### 17.1 M5 Limitations (Intentional)

| Limitation | Impact | Mitigation |
|------------|--------|------------|
| **Weak snapshots** | Reads fail on concurrent modification | Retry transaction |
| **Document-level versioning** | More false conflicts | M6+ per-path versioning |
| **No array mutations** | Must read/modify/write whole array | M6+ array operations |
| **Blob storage** | Full deserialize on every operation | M6+ structural storage |
| **No diff** | Can't see what changed | M7 diff support |
| **Conservative array conflict** | Array access conflicts with any array change | M6+ stable identities |
| **Subset of RFC 6902** | Only Set/Delete operations | Compose complex ops from primitives |

### 17.2 RFC 6902 Subset Support

M5 implements a **subset** of RFC 6902 JSON Patch operations, not full compliance.

**Supported Operations:**
- `Set` - Replace/create value at path (similar to RFC 6902 `replace`)
- `Delete` - Remove value at path (RFC 6902 `remove`)

**NOT Supported in M5:**
- `add` - Insert at path (differs from `replace` for arrays and missing keys)
- `test` - Conditional patch execution (verify value before applying)
- `move` - Move value from one path to another
- `copy` - Copy value from one path to another

**Rationale:** M5 prioritizes semantic lock-in over feature completeness. The `Set` and `Delete` operations cover the core mutation semantics. Complex transformations can be composed via read-modify-write patterns. Full RFC 6902 support may be added in M6+ if needed. WAL entry type 0x24 is reserved for future `JsonPatch` (RFC 6902) support.

### 17.3 What M5 Explicitly Does NOT Provide

- Per-path versioning
- Structural storage
- Structural sharing
- Subtree MVCC
- Full snapshot isolation for modified documents
- Array insert/remove operations
- Diff operations
- Query language
- Indexes
- Full RFC 6902 JSON Patch compliance (only Set/Delete supported)

These are all **intentionally deferred**, not forgotten.

---

## 19. Future Extension Points

### 18.1 M6: Structural Representation

```rust
// M6 will add:
struct JsonNode {
    id: NodeId,
    kind: NodeKind,
    version: u64,  // Per-node version
}

// Enabling:
// - Structural sharing
// - Per-path versioning
// - Efficient subtree reads
// - Array insert/remove with stable identities
```

### 18.2 M6+: Full Snapshot Isolation

```rust
// M6+ will add version chains or COW subtrees
// Enabling reads of historical versions
impl JsonSnapshot {
    fn get_at_version(&self, doc_id, path, version) -> Result<JsonValue>;
}
```

### 18.3 M7: Diff

```rust
// M7 will add:
impl JsonStore {
    fn diff(
        &self,
        run_id: RunId,
        doc_id: JsonDocId,
        from_version: u64,
        to_version: u64,
    ) -> Result<Vec<JsonPatch>>;
}
```

### 18.4 M11: Query & Index

```rust
// M11 will add:
impl JsonStore {
    fn query(&self, run_id, doc_id, query: &str) -> Result<Vec<JsonValue>>;
    fn create_index(&self, run_id, doc_id, path: &JsonPath) -> Result<IndexId>;
}
```

### 18.5 Extension Hooks

M5 code is designed for extension:

```rust
// Storage trait allows swapping implementations
trait JsonStorage {
    fn get(&self, doc_id: &JsonDocId, path: &JsonPath) -> Result<...>;
    fn set(&mut self, doc_id: &JsonDocId, path: &JsonPath, value: JsonValue) -> Result<...>;
}

// M5: BlobJsonStorage (simple)
// M6+: StructuralJsonStorage (optimized)
```

---

## 20. Appendix

### 19.1 Dependency Changes

**New dependencies for M5**:
- `indexmap`: Ordered map for JSON objects (preserves insertion order)
- `rmp-serde`: MessagePack serialization (likely already present)

### 19.2 Crate Structure

```
in-mem/
├── crates/
│   ├── core/
│   │   └── src/
│   │       ├── types.rs          # +JsonDocId
│   │       └── json.rs           # JsonValue, JsonPath, JsonPatch (NEW)
│   ├── durability/
│   │   └── src/
│   │       └── wal.rs            # +JsonWalEntry
│   ├── concurrency/
│   │   └── src/
│   │       └── transaction.rs    # +json_read_set, json_write_set (lazy)
│   └── primitives/
│       └── src/
│           ├── lib.rs            # +json module
│           └── json.rs           # JsonStore (NEW)
```

### 19.3 Success Criteria Checklist

**Gate 1: Core Semantics**
- [ ] JsonStore::create() works
- [ ] JsonStore::get(path) works
- [ ] JsonStore::set(path) works
- [ ] JsonStore::delete(path) works
- [ ] JsonStore::cas() works with document version
- [ ] JsonStore::patch() applies multiple operations atomically

**Gate 2: Conflict Detection**
- [ ] Sibling paths do not conflict
- [ ] Ancestor/descendant paths conflict
- [ ] Same path conflicts
- [ ] Different documents do not conflict
- [ ] Root path conflicts with all paths

**Gate 3: WAL**
- [ ] JSON WAL entries written correctly
- [ ] WAL replay is deterministic
- [ ] WAL replay is idempotent
- [ ] Recovery works after simulated crash

**Gate 4: Transactions**
- [ ] JSON participates in transactions
- [ ] Read-your-writes works
- [ ] Cross-primitive atomicity works
- [ ] Conflict detection fails transaction correctly

**Gate 5: Non-Regression**
- [ ] KV performance unchanged
- [ ] Event performance unchanged
- [ ] State performance unchanged
- [ ] Trace performance unchanged
- [ ] Non-JSON transactions have zero overhead

---

## Conclusion

M5 is a **semantic lock-in milestone**.

It defines:
- Path-level mutation semantics
- Region-based conflict detection
- Patch-based WAL format
- Weak snapshot isolation (explicit limitation)
- Transaction integration

It does NOT attempt to be fast or feature-complete. That is intentional.

**M5 freezes semantics. M6+ optimizes.**

The simple blob-based implementation in M5 is correct and testable. Once semantics are validated, M6+ can add structural representation, per-path versioning, and full MVCC without changing the API contract.

---

**Document Version**: 1.1
**Status**: Planning Phase
**Date**: 2026-01-16
