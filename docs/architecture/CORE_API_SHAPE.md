# Strata Core API Shape

> **Status**: Design Document
> **Stability**: May evolve based on usage patterns
> **Scope**: How the Primitive Contract is expressed in code

---

## Purpose

This document defines the **shape** of the core API. It describes how the invariants from the Primitive Contract are expressed programmatically.

This is not the final API. It is the structural pattern that the API must follow.

---

## Relationship to Deployment Modes

**Strata is an embedded library.** This document defines the embedded API.

The server deployment mode (M10) exposes this same API over a wire protocol. The server adds no new semantics—it is a thin transport layer. If you can do it through the server, you can do it through the embedded API, and vice versa.

This means:
- The embedded API is the canonical form
- The wire protocol is a serialization of these operations
- Client libraries (Python, etc.) implement this same API shape

---

## Relationship to Primitive Contract

The Primitive Contract defines **what must be true**.
This document defines **how to express it**.

| Contract Invariant | API Expression |
|-------------------|----------------|
| Everything is addressable | `EntityRef` type |
| Everything is versioned | `Versioned<T>` wrapper |
| Everything is transactional | `Transaction` trait |
| Everything has a lifecycle | CRUD method patterns |
| Everything is run-scoped | `RunId` parameter or context |
| Everything is introspectable | `exists()`, `get()` methods |
| Reads/writes are consistent | Immutable borrows vs mutable borrows |

---

## Core Types

### EntityRef: The Universal Address

Every entity has a reference that can identify it.

```rust
/// Reference to any entity in Strata
///
/// This is the programmatic expression of "everything is addressable."
/// The exact representation may vary (enum, URI string, struct), but
/// the capability must exist.
pub enum EntityRef {
    Kv { run_id: RunId, key: String },
    Event { run_id: RunId, sequence: u64 },
    State { run_id: RunId, name: String },
    Trace { run_id: RunId, trace_id: TraceId },
    Run { run_id: RunId },
    Json { run_id: RunId, doc_id: JsonDocId },
    Vector { run_id: RunId, collection_id: CollectionId, vector_id: VectorId },
}
```

**Open questions** (to be resolved by usage):
- Should this be an enum or a trait?
- Should there be a string serialization (URI)?
- Should references be typed (`KvRef`, `EventRef`) or unified?

### Versioned<T>: The Universal Read Result

Every read returns version information.

```rust
/// Wrapper for any value read from Strata
///
/// This is the programmatic expression of "everything is versioned."
pub struct Versioned<T> {
    /// The actual value
    pub value: T,

    /// Version identifier
    pub version: Version,

    /// When this version was created
    pub timestamp: Timestamp,
}
```

**The key insight**: The `Versioned<T>` wrapper is how we ensure that version information is never "lost" or "optional." If you read something, you get its version.

**Open questions**:
- Should `Versioned<T>` include the `EntityRef`?
- Should `timestamp` be optional?
- Should there be a `VersionedOption<T>` for reads that might not exist?

### Version: The Universal Version Type

```rust
/// Version identifier
///
/// Versions are comparable within the same entity.
/// Versions may not be comparable across entities or across primitives.
pub enum Version {
    /// Transaction-based version (KV, Trace, Run, Vector)
    TxnId(u64),

    /// Sequence-based version (EventLog)
    Sequence(u64),

    /// Counter-based version (StateCell, JsonStore)
    Counter(u64),
}
```

**Note**: This is one possible representation. The invariant requires that versions exist and are ordered. It does not require this specific enum.

### RunId: The Universal Scope

```rust
/// Identifier for a run (execution context)
///
/// All operations are scoped to a run.
pub struct RunId(pub String);
```

---

## The Transaction Pattern

### Shape

Transactions follow this pattern:

```rust
impl Database {
    /// Execute operations atomically
    fn transaction<F, T>(&self, run_id: &RunId, f: F) -> Result<T>
    where
        F: FnOnce(&mut Transaction) -> Result<T>;
}
```

**The key insight**: The closure receives a `&mut Transaction`. All primitive operations within the closure go through this transaction handle.

### Transaction Trait

```rust
/// Operations available within a transaction
///
/// Each primitive's operations are accessible through this trait.
/// The exact method signatures may vary, but the pattern is:
/// - Reads take &self
/// - Writes take &mut self
/// - All return Result<T>
pub trait TransactionOps {
    // KV
    fn kv_get(&self, key: &str) -> Result<Option<Versioned<Value>>>;
    fn kv_put(&mut self, key: &str, value: Value) -> Result<()>;
    fn kv_delete(&mut self, key: &str) -> Result<bool>;

    // Event
    fn event_append(&mut self, event_type: &str, payload: Value) -> Result<Versioned<u64>>;
    fn event_read(&self, sequence: u64) -> Result<Option<Versioned<Event>>>;

    // State
    fn state_read(&self, name: &str) -> Result<Option<Versioned<State>>>;
    fn state_set(&mut self, name: &str, value: Value) -> Result<Version>;
    fn state_cas(&mut self, name: &str, expected: Version, value: Value) -> Result<Version>;

    // Trace
    fn trace_record(&mut self, trace_type: TraceType, tags: Vec<String>) -> Result<Versioned<TraceId>>;
    fn trace_read(&self, trace_id: &TraceId) -> Result<Option<Versioned<Trace>>>;

    // Json
    fn json_create(&mut self, doc_id: &JsonDocId, value: JsonValue) -> Result<Version>;
    fn json_get(&self, doc_id: &JsonDocId, path: &JsonPath) -> Result<Option<Versioned<JsonValue>>>;
    fn json_set(&mut self, doc_id: &JsonDocId, path: &JsonPath, value: JsonValue) -> Result<Version>;

    // Vector
    fn vector_upsert(&mut self, collection: &CollectionId, entries: Vec<VectorEntry>) -> Result<usize>;
    fn vector_get(&self, collection: &CollectionId, id: &VectorId) -> Result<Option<Versioned<VectorEntry>>>;
    fn vector_search(&self, collection: &CollectionId, query: Vec<f32>, k: usize) -> Result<Vec<VectorMatch>>;
}
```

**Open questions**:
- Should each primitive have its own sub-trait?
- Should there be a builder pattern instead of direct methods?
- How should error types be structured?

---

## Primitive Handle Pattern

Outside of transactions, primitives can be accessed through handles:

```rust
/// Handle to the KV primitive for a specific run
pub struct KvHandle<'a> {
    db: &'a Database,
    run_id: RunId,
}

impl<'a> KvHandle<'a> {
    /// Read (fast path, outside transaction)
    pub fn get(&self, key: &str) -> Result<Option<Versioned<Value>>>;

    /// Write (implicit single-operation transaction)
    pub fn put(&self, key: &str, value: Value) -> Result<()>;

    /// Existence check (fast path)
    pub fn exists(&self, key: &str) -> Result<bool>;
}
```

**The key insight**: Simple operations don't require explicit transaction syntax. The handle provides a convenient API that internally creates single-operation transactions.

**Open questions**:
- Should handles be stateful (caching, batching)?
- Should there be async versions?
- How should handles relate to the transaction API?

---

## The Run Handle Pattern

Runs are the entry point:

```rust
impl Database {
    /// Get a handle for an existing run
    fn run(&self, run_id: &str) -> RunHandle;

    /// Create a new run
    fn create_run(&self, run_id: &str) -> Result<RunHandle>;
}

impl RunHandle {
    /// Access KV primitive
    fn kv(&self) -> KvHandle;

    /// Access Event primitive
    fn events(&self) -> EventHandle;

    /// Access State primitive
    fn state(&self) -> StateHandle;

    /// Access Trace primitive
    fn traces(&self) -> TraceHandle;

    /// Access Json primitive
    fn json(&self) -> JsonHandle;

    /// Access Vector primitive
    fn vectors(&self) -> VectorHandle;

    /// Execute a transaction
    fn transaction<F, T>(&self, f: F) -> Result<T>
    where
        F: FnOnce(&mut Transaction) -> Result<T>;
}
```

---

## Lifecycle Method Patterns

Each primitive follows this pattern for lifecycle operations:

| Operation | Method Pattern | Returns |
|-----------|---------------|---------|
| Create | `create()`, `init()`, `put()` | `Result<Version>` |
| Read | `get()`, `read()` | `Result<Option<Versioned<T>>>` |
| Update | `set()`, `put()`, `cas()` | `Result<Version>` |
| Delete | `delete()`, `destroy()` | `Result<bool>` |
| Exists | `exists()` | `Result<bool>` |
| List | `list()`, `keys()` | `Result<Vec<...>>` |

**The key insight**: The return types are consistent. Creates/updates return versions. Reads return versioned values. Deletes return success/failure. Lists return collections.

---

## Error Handling Pattern

```rust
/// Errors from Strata operations
pub enum StrataError {
    /// Entity not found
    NotFound { entity_ref: EntityRef },

    /// Version conflict (CAS failure, OCC conflict)
    VersionConflict {
        entity_ref: EntityRef,
        expected: Version,
        actual: Version,
    },

    /// Transaction aborted (conflict, timeout, etc.)
    TransactionAborted { reason: String },

    /// Invalid operation for entity state
    InvalidOperation { entity_ref: EntityRef, reason: String },

    /// Storage error
    StorageError { source: Box<dyn std::error::Error> },
}
```

**Open questions**:
- Should errors include the EntityRef?
- Should there be primitive-specific error variants?
- How should errors compose across primitives in a transaction?

---

## What This Document Does NOT Cover

- **Specific method signatures**: The exact parameters and return types will be refined
- **Async API**: Whether operations are sync or async
- **Builder patterns**: Fluent APIs for complex operations
- **Search API**: Discovery is a separate feature layer
- **History/Diff API**: Introspection beyond basic reads
- **Wire protocol**: Serialization format
- **SDK ergonomics**: Language-specific conveniences

---

## Design Principles

### 1. Reads are Pure

Read operations (`&self`) never modify state. They can be called multiple times with the same result (within a transaction snapshot).

### 2. Writes Return Versions

Write operations (`&mut self`) always return the new version. This makes it possible to track what happened.

### 3. Version Information is Not Optional

There is no "read without version" API. If you read, you get version information. You can ignore it, but it's always there.

### 4. Run Scope is Explicit

The run is always known. Either it's in the handle, or it's a parameter. There is no "ambient" run context.

### 5. Transactions are Explicit Boundaries

The transaction boundary is visible in the code (`transaction(|txn| { ... })`). There is no hidden transaction scope.

### 6. Single-Operation Shortcuts Exist

For simple operations, you don't need explicit transactions. But under the hood, they are transactions.

---

## Migration from Current API

The current API already follows most of these patterns. Key changes:

1. **Wrap reads in `Versioned<T>`**: Currently some reads return raw values
2. **Unify `EntityRef`**: Currently identity is per-primitive
3. **Standardize error types**: Currently errors are ad-hoc

These changes are incremental. The existing API is close to this shape.

---

## Document History

| Version | Date | Changes |
|---------|------|---------|
| 1.0 | 2026-01-19 | Initial API shape extracted from Universal Primitive Protocol |
