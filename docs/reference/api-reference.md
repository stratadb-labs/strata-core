# API Reference

Complete API reference for **in-mem** v0.3.0 (M3 Primitives + M4 Performance).

## Table of Contents

- [Core Types](#core-types)
- [Primitives](#primitives)
  - [KVStore](#kvstore)
  - [EventLog](#eventlog)
  - [StateCell](#statecell)
  - [TraceStore](#tracestore)
  - [RunIndex](#runindex)
- [Transactions](#transactions)
- [Durability Modes](#durability-modes)
- [Error Types](#error-types)
- [Performance Characteristics](#performance-characteristics)

---

## Core Types

### `Database`

Main entry point for interacting with **in-mem**.

```rust
pub struct Database {
    // Internal fields (opaque)
}
```

#### Construction

##### `open`

Opens or creates a database at the specified path with default settings (Strict durability).

```rust
pub fn open<P: AsRef<Path>>(path: P) -> Result<Database>
```

**Parameters**:
- `path`: Directory path for database storage (created if doesn't exist)

**Returns**: `Result<Database>`

**Example**:
```rust
let db = Database::open("./my-database")?;
```

##### `open_with_mode`

Opens a database with a specific durability mode.

```rust
pub fn open_with_mode<P: AsRef<Path>>(
    path: P,
    mode: DurabilityMode
) -> Result<Database>
```

**Parameters**:
- `path`: Directory path for database storage
- `mode`: Durability mode (InMemory, Buffered, or Strict)

**Returns**: `Result<Database>`

**Example**:
```rust
use in_mem::{Database, DurabilityMode};

// InMemory mode for tests (fastest, no persistence)
let db = Database::open_with_mode("./data", DurabilityMode::InMemory)?;

// Buffered mode for production (balanced)
let db = Database::open_with_mode(
    "./data",
    DurabilityMode::Buffered {
        flush_interval_ms: 100,
        max_pending_writes: 1000
    }
)?;

// Strict mode for critical data (safest)
let db = Database::open_with_mode("./data", DurabilityMode::Strict)?;
```

#### Run Lifecycle

##### `begin_run`

Creates a new run and returns its ID.

```rust
pub fn begin_run(&self) -> RunId
```

**Returns**: `RunId` - Unique identifier for this run

##### `end_run`

Ends a run and releases its resources.

```rust
pub fn end_run(&self, run_id: RunId) -> Result<()>
```

#### Transactions

##### `transaction`

Executes a closure within a transaction with automatic commit/rollback.

```rust
pub fn transaction<F, T>(&self, run_id: &RunId, f: F) -> Result<T>
where
    F: FnOnce(&mut TransactionContext) -> Result<T>
```

**Example**:
```rust
let result = db.transaction(&run_id, |txn| {
    txn.kv_put("key1", Value::String("value1".into()))?;
    txn.kv_put("key2", Value::I64(42))?;
    Ok("success")
})?;
```

---

### `RunId`

Unique identifier for an agent run (UUID v4).

### `Value`

Flexible value type supporting multiple primitives.

```rust
pub enum Value {
    Null,
    Bool(bool),
    I64(i64),
    F64(f64),
    String(String),
    Bytes(Vec<u8>),
    Array(Vec<Value>),
    Map(BTreeMap<String, Value>),
}
```

---

## Primitives

All primitives are stateless facades over the database engine.

### KVStore

Key-value store primitive with batch operations and transactions.

```rust
pub struct KVStore {
    db: Arc<Database>
}
```

#### Single-Key Operations

##### `get` (Fast Path)

Retrieves a value by key. Uses direct snapshot read for optimal performance.

```rust
pub fn get(&self, run_id: &RunId, key: &str) -> Result<Option<Value>>
```

**Performance**: <10µs (fast path, no transaction overhead)

##### `put`

Stores a value. Creates or overwrites existing value.

```rust
pub fn put(&self, run_id: &RunId, key: &str, value: Value) -> Result<()>
```

##### `put_with_ttl`

Stores a value with time-to-live metadata.

```rust
pub fn put_with_ttl(&self, run_id: &RunId, key: &str, value: Value, ttl: Duration) -> Result<()>
```

##### `delete`

Deletes a key-value pair.

```rust
pub fn delete(&self, run_id: &RunId, key: &str) -> Result<bool>
```

##### `exists` (Fast Path)

Checks if a key exists.

```rust
pub fn exists(&self, run_id: &RunId, key: &str) -> Result<bool>
```

#### Batch Operations (Fast Path)

##### `get_many`

Retrieves multiple values in a single snapshot read.

```rust
pub fn get_many(&self, run_id: &RunId, keys: &[&str]) -> Result<Vec<Option<Value>>>
```

##### `get_many_map`

Retrieves multiple values as a HashMap.

```rust
pub fn get_many_map(&self, run_id: &RunId, keys: &[&str]) -> Result<HashMap<String, Value>>
```

#### List Operations

##### `list`

Lists all keys with an optional prefix.

```rust
pub fn list(&self, run_id: &RunId, prefix: Option<&str>) -> Result<Vec<String>>
```

##### `list_with_values`

Lists all key-value pairs with an optional prefix.

```rust
pub fn list_with_values(&self, run_id: &RunId, prefix: Option<&str>) -> Result<Vec<(String, Value)>>
```

#### Explicit Transactions

##### `transaction`

Executes multiple operations in a single transaction.

```rust
pub fn transaction<F, T>(&self, run_id: &RunId, f: F) -> Result<T>
where
    F: FnOnce(&mut KVTransaction<'_>) -> Result<T>
```

---

### EventLog

Append-only event log with hash chaining for integrity verification.

#### Types

```rust
pub struct Event {
    pub sequence: u64,
    pub event_type: String,
    pub payload: Value,
    pub timestamp: i64,
    pub prev_hash: [u8; 32],
    pub hash: [u8; 32],
}

pub struct ChainVerification {
    pub is_valid: bool,
    pub length: u64,
    pub first_invalid: Option<u64>,
    pub error: Option<String>,
}
```

#### Methods

##### `append`

Appends a new event to the log.

```rust
pub fn append(&self, run_id: &RunId, event_type: &str, payload: Value) -> Result<(u64, [u8; 32])>
```

##### `read` (Fast Path)

Reads a single event by sequence number.

```rust
pub fn read(&self, run_id: &RunId, sequence: u64) -> Result<Option<Event>>
```

##### `read_range`

Reads a range of events [start, end).

```rust
pub fn read_range(&self, run_id: &RunId, start: u64, end: u64) -> Result<Vec<Event>>
```

##### `head`

Returns the most recent event.

##### `len` (Fast Path)

Returns the number of events in the log.

##### `read_by_type`

Returns all events of a specific type.

##### `verify_chain`

Validates the hash chain integrity.

---

### StateCell

Named state cells with compare-and-swap (CAS) operations.

#### Types

```rust
pub struct State {
    pub value: Value,
    pub version: u64,
    pub updated_at: i64,
}
```

#### Methods

##### `init`

Initializes a state cell only if it doesn't exist.

```rust
pub fn init(&self, run_id: &RunId, name: &str, value: Value) -> Result<u64>
```

##### `read` (Fast Path)

Reads the current state of a cell.

```rust
pub fn read(&self, run_id: &RunId, name: &str) -> Result<Option<State>>
```

##### `set`

Unconditionally sets the value.

```rust
pub fn set(&self, run_id: &RunId, name: &str, value: Value) -> Result<u64>
```

##### `cas`

Compare-and-swap: updates only if version matches.

```rust
pub fn cas(&self, run_id: &RunId, name: &str, expected_version: u64, new_value: Value) -> Result<u64>
```

##### `transition`

Applies a pure function with automatic retry on conflict.

```rust
pub fn transition<F, T>(&self, run_id: &RunId, name: &str, f: F) -> Result<(T, u64)>
where
    F: Fn(&State) -> Result<(Value, T)>
```

**Important**: The closure must be pure (no I/O) as it may be called multiple times.

##### `transition_or_init`

Like `transition`, but initializes if cell doesn't exist.

---

### TraceStore

Records agent reasoning traces for debugging and auditing.

#### Types

```rust
pub enum TraceType {
    ToolCall { tool_name: String, arguments: Value, result: Option<Value>, duration_ms: Option<u64> },
    Decision { question: String, options: Vec<String>, chosen: String, reasoning: Option<String> },
    Query { query_type: String, query: String, results_count: Option<u32> },
    Thought { content: String, confidence: Option<f64> },
    Error { error_type: String, message: String, recoverable: bool },
    Custom { name: String, data: Value },
}

pub struct Trace {
    pub id: String,
    pub parent_id: Option<String>,
    pub trace_type: TraceType,
    pub timestamp: i64,
    pub tags: Vec<String>,
    pub metadata: Value,
}

pub struct TraceTree {
    pub trace: Trace,
    pub children: Vec<TraceTree>,
}
```

#### Methods

##### `record`

Records a new trace entry.

```rust
pub fn record(&self, run_id: &RunId, trace_type: TraceType, tags: Vec<String>, metadata: Value) -> Result<String>
```

##### `record_child`

Records a trace as a child of an existing trace.

##### `get` (Fast Path)

Retrieves a trace by ID.

##### `query_by_type`

Returns all traces of a specific type.

##### `query_by_tag`

Returns all traces with a specific tag.

##### `get_tree`

Builds a recursive tree structure from a root trace.

##### `get_roots`

Returns all traces without parents.

---

### RunIndex

First-class run management with status tracking.

#### Types

```rust
pub enum RunStatus {
    Active,
    Completed,
    Failed,
    Cancelled,
    Paused,
    Archived,
}

pub struct RunMetadata {
    pub name: String,
    pub run_id: String,
    pub parent_run: Option<String>,
    pub status: RunStatus,
    pub created_at: i64,
    pub updated_at: i64,
    pub completed_at: Option<i64>,
    pub tags: Vec<String>,
    pub metadata: Value,
    pub error: Option<String>,
}
```

**Valid Status Transitions**:
- `Active` → Completed, Failed, Cancelled, Paused, Archived
- `Paused` → Active, Cancelled, Archived
- `Completed/Failed/Cancelled` → Archived
- `Archived` → (terminal)

#### Methods

##### `create_run`

Creates a new run entry.

##### `create_run_with_options`

Creates a run with parent, tags, and metadata.

##### `get_run`

Retrieves run metadata.

##### `update_status`

Updates run status with transition validation.

##### `complete_run`, `fail_run`, `pause_run`, `resume_run`, `cancel_run`, `archive_run`

Convenience methods for status updates.

##### `query_by_status`

Returns all runs with a specific status.

##### `query_by_tag`

Returns all runs with a specific tag.

##### `delete_run`

**Hard delete**: Removes the run and ALL associated data.

---

## Transactions

### Cross-Primitive Transactions

Use extension traits for atomic operations across multiple primitives.

```rust
use in_mem::primitives::{KVStoreExt, EventLogExt, StateCellExt, TraceStoreExt};

db.transaction(&run_id, |txn| {
    txn.kv_put("key", Value::String("value".into()))?;
    txn.event_append("my_event", Value::Null)?;
    txn.state_set("counter", Value::I64(1))?;
    txn.trace_record("operation", Value::Null)?;
    Ok(())
})?;
```

### Extension Traits

```rust
pub trait KVStoreExt {
    fn kv_get(&mut self, key: &str) -> Result<Option<Value>>;
    fn kv_put(&mut self, key: &str, value: Value) -> Result<()>;
    fn kv_delete(&mut self, key: &str) -> Result<()>;
}

pub trait EventLogExt {
    fn event_append(&mut self, event_type: &str, payload: Value) -> Result<u64>;
    fn event_read(&mut self, sequence: u64) -> Result<Option<Value>>;
}

pub trait StateCellExt {
    fn state_read(&mut self, name: &str) -> Result<Option<Value>>;
    fn state_cas(&mut self, name: &str, expected_version: u64, new_value: Value) -> Result<u64>;
    fn state_set(&mut self, name: &str, value: Value) -> Result<u64>;
}

pub trait TraceStoreExt {
    fn trace_record(&mut self, trace_type: &str, metadata: Value) -> Result<String>;
    fn trace_record_child(&mut self, parent_id: &str, trace_type: &str, metadata: Value) -> Result<String>;
}
```

---

## Durability Modes

```rust
pub enum DurabilityMode {
    InMemory,
    Buffered { flush_interval_ms: u64, max_pending_writes: usize },
    Strict,
}
```

### InMemory

No persistence. Data is lost on crash.

| Property | Value |
|----------|-------|
| Latency | <3µs |
| Throughput | 250K+ ops/sec |
| Data Loss | All |

### Buffered

Background thread fsyncs periodically.

| Property | Value |
|----------|-------|
| Latency | <30µs |
| Throughput | 50K+ ops/sec |
| Data Loss | ~100ms |

### Strict

Synchronous fsync after every commit.

| Property | Value |
|----------|-------|
| Latency | ~2ms |
| Throughput | ~500 ops/sec |
| Data Loss | None |

---

## Error Types

```rust
pub enum Error {
    Io(std::io::Error),
    Serialization(String),
    Corruption(String),
    KeyNotFound(String),
    InvalidOperation(String),
    TransactionConflict(String),
    InvalidStatusTransition { from: String, to: String },
    VersionMismatch { expected: u64, actual: u64 },
}
```

---

## Performance Characteristics

### Fast Path Operations

| Operation | Target Latency |
|-----------|----------------|
| `KVStore::get` | <10µs |
| `KVStore::exists` | <10µs |
| `KVStore::get_many` | <10µs + O(n) |
| `EventLog::read` | <10µs |
| `EventLog::len` | <10µs |
| `StateCell::read` | <10µs |
| `TraceStore::get` | <10µs |

### Throughput Targets

| Mode | Target |
|------|--------|
| InMemory (1 thread) | 250K ops/sec |
| InMemory (4 threads) | 800K+ ops/sec |
| Buffered | 50K ops/sec |
| Strict | ~500 ops/sec |

### Scaling

| Threads | Disjoint Scaling |
|---------|------------------|
| 2 | ≥1.8× |
| 4 | ≥3.2× |

---

## Version History

### v0.3.0 (M3 Primitives + M4 Performance)

**M3 Features**:
- KVStore with batch operations and transactions
- EventLog with hash chaining and type queries
- StateCell with CAS and transition closures
- TraceStore with hierarchical traces
- RunIndex with status management
- Cross-primitive transaction support

**M4 Features**:
- Three durability modes (InMemory, Buffered, Strict)
- Fast path API for read operations
- OCC transactions with snapshot isolation
- 250K+ ops/sec in InMemory mode

### v0.2.0 (M2 Transactions)

- OCC with snapshot isolation
- Multi-key transactions
- CAS operations

### v0.1.0 (M1 Foundation)

- Basic KV operations
- Write-ahead logging
- Crash recovery

---

## See Also

- [Getting Started Guide](getting-started.md)
- [Architecture Overview](architecture.md)
- [Milestones](../milestones/MILESTONES.md)
