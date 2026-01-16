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

**Example**:
```rust
let run_id = db.begin_run();
```

##### `end_run`

Ends a run and releases its resources.

```rust
pub fn end_run(&self, run_id: RunId) -> Result<()>
```

**Parameters**:
- `run_id`: Run to end

**Returns**: `Result<()>`

**Example**:
```rust
db.end_run(run_id)?;
```

#### Transactions

##### `transaction`

Executes a closure within a transaction with automatic commit/rollback.

```rust
pub fn transaction<F, T>(&self, run_id: &RunId, f: F) -> Result<T>
where
    F: FnOnce(&mut TransactionContext) -> Result<T>
```

**Parameters**:
- `run_id`: Run ID for this transaction
- `f`: Closure that performs operations within the transaction

**Returns**: `Result<T>` - The closure's return value on success

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

Unique identifier for an agent run.

```rust
pub struct RunId(Uuid);
```

**Properties**:
- Globally unique (UUID v4)
- Serializable
- Cloneable

**Example**:
```rust
let run_id = db.begin_run();
println!("Run ID: {}", run_id); // Prints UUID
```

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

**Example**:
```rust
let v1 = Value::String("hello".to_string());
let v2 = Value::I64(42);
let v3 = Value::Array(vec![Value::I64(1), Value::I64(2)]);
let v4 = Value::Map(BTreeMap::from([
    ("name".to_string(), Value::String("Alice".to_string())),
    ("age".to_string(), Value::I64(30)),
]));
```

---

## Primitives

All primitives are stateless facades over the database engine. They do not hold any state themselves.

### KVStore

Key-value store primitive with type-safe value encoding.

```rust
pub struct KVStore {
    db: Arc<Database>
}
```

#### Construction

```rust
impl KVStore {
    pub fn new(db: Arc<Database>) -> Self
}
```

**Example**:
```rust
let kv = KVStore::new(db.clone());
```

#### Single-Key Operations

##### `get` (Fast Path)

Retrieves a value by key. Uses direct snapshot read for optimal performance.

```rust
pub fn get(&self, run_id: &RunId, key: &str) -> Result<Option<Value>>
```

**Parameters**:
- `run_id`: Run ID
- `key`: String key

**Returns**: `Result<Option<Value>>` - Value if exists, None if not found

**Performance**: <10µs (fast path, no transaction overhead)

**Example**:
```rust
let value = kv.get(&run_id, "user:123:name")?;
match value {
    Some(Value::String(name)) => println!("Name: {}", name),
    Some(_) => println!("Unexpected type"),
    None => println!("Not found"),
}
```

##### `put`

Stores a value. Creates or overwrites existing value.

```rust
pub fn put(&self, run_id: &RunId, key: &str, value: Value) -> Result<()>
```

**Parameters**:
- `run_id`: Run ID
- `key`: String key
- `value`: Value to store

**Returns**: `Result<()>`

**Example**:
```rust
kv.put(&run_id, "user:123:name", Value::String("Alice".into()))?;
kv.put(&run_id, "user:123:age", Value::I64(30))?;
```

##### `put_with_ttl`

Stores a value with time-to-live metadata.

```rust
pub fn put_with_ttl(
    &self,
    run_id: &RunId,
    key: &str,
    value: Value,
    ttl: Duration
) -> Result<()>
```

**Parameters**:
- `run_id`: Run ID
- `key`: String key
- `value`: Value to store
- `ttl`: Time-to-live duration

**Returns**: `Result<()>`

**Example**:
```rust
use std::time::Duration;

kv.put_with_ttl(
    &run_id,
    "session:abc",
    Value::String("session-data".into()),
    Duration::from_secs(3600)
)?;
```

##### `delete`

Deletes a key-value pair.

```rust
pub fn delete(&self, run_id: &RunId, key: &str) -> Result<bool>
```

**Parameters**:
- `run_id`: Run ID
- `key`: String key

**Returns**: `Result<bool>` - true if key existed, false if not found

**Example**:
```rust
let deleted = kv.delete(&run_id, "user:123:name")?;
```

##### `exists` (Fast Path)

Checks if a key exists. Uses direct snapshot read for optimal performance.

```rust
pub fn exists(&self, run_id: &RunId, key: &str) -> Result<bool>
```

**Performance**: <10µs (fast path)

**Example**:
```rust
if kv.exists(&run_id, "user:123")? {
    println!("User exists");
}
```

#### Batch Operations (Fast Path)

##### `get_many`

Retrieves multiple values in a single snapshot read.

```rust
pub fn get_many(&self, run_id: &RunId, keys: &[&str]) -> Result<Vec<Option<Value>>>
```

**Returns**: Vector of Option<Value> in the same order as keys

**Example**:
```rust
let values = kv.get_many(&run_id, &["key1", "key2", "key3"])?;
```

##### `get_many_map`

Retrieves multiple values as a HashMap (missing keys omitted).

```rust
pub fn get_many_map(&self, run_id: &RunId, keys: &[&str]) -> Result<HashMap<String, Value>>
```

**Example**:
```rust
let map = kv.get_many_map(&run_id, &["key1", "key2", "key3"])?;
if let Some(value) = map.get("key1") {
    println!("key1 = {:?}", value);
}
```

#### List Operations

##### `list`

Lists all keys with an optional prefix.

```rust
pub fn list(&self, run_id: &RunId, prefix: Option<&str>) -> Result<Vec<String>>
```

**Parameters**:
- `run_id`: Run ID
- `prefix`: Optional key prefix to filter by

**Returns**: `Result<Vec<String>>` - Vector of matching keys

**Example**:
```rust
// List all keys
let all_keys = kv.list(&run_id, None)?;

// List keys with prefix
let user_keys = kv.list(&run_id, Some("user:"))?;
```

##### `list_with_values`

Lists all key-value pairs with an optional prefix.

```rust
pub fn list_with_values(
    &self,
    run_id: &RunId,
    prefix: Option<&str>
) -> Result<Vec<(String, Value)>>
```

**Example**:
```rust
let users = kv.list_with_values(&run_id, Some("user:"))?;
for (key, value) in users {
    println!("{}: {:?}", key, value);
}
```

#### Explicit Transactions

##### `transaction`

Executes multiple operations in a single transaction.

```rust
pub fn transaction<F, T>(&self, run_id: &RunId, f: F) -> Result<T>
where
    F: FnOnce(&mut KVTransaction<'_>) -> Result<T>
```

**KVTransaction Methods**:
- `get(&mut self, key: &str) -> Result<Option<Value>>`
- `put(&mut self, key: &str, value: Value) -> Result<()>`
- `delete(&mut self, key: &str) -> Result<bool>`
- `list(&mut self, prefix: Option<&str>) -> Result<Vec<String>>`

**Example**:
```rust
kv.transaction(&run_id, |txn| {
    let balance = txn.get("account:balance")?
        .and_then(|v| match v { Value::I64(n) => Some(n), _ => None })
        .unwrap_or(0);

    txn.put("account:balance", Value::I64(balance + 100))?;
    txn.put("account:last_updated", Value::I64(now_ms()))?;

    Ok(balance + 100)
})?;
```

---

### EventLog

Append-only event log with hash chaining for integrity verification.

```rust
pub struct EventLog {
    db: Arc<Database>
}
```

#### Types

```rust
pub struct Event {
    pub sequence: u64,              // Auto-assigned, monotonic per run
    pub event_type: String,         // User-defined category
    pub payload: Value,             // Arbitrary data
    pub timestamp: i64,             // Milliseconds since epoch
    pub prev_hash: [u8; 32],        // Hash of previous event (causal chain)
    pub hash: [u8; 32],             // Hash of this event
}

pub struct ChainVerification {
    pub is_valid: bool,
    pub length: u64,
    pub first_invalid: Option<u64>,
    pub error: Option<String>,
}
```

#### Construction

```rust
impl EventLog {
    pub fn new(db: Arc<Database>) -> Self
}
```

#### Append Operations

##### `append`

Appends a new event to the log. Returns sequence number and hash.

```rust
pub fn append(
    &self,
    run_id: &RunId,
    event_type: &str,
    payload: Value
) -> Result<(u64, [u8; 32])>
```

**Parameters**:
- `run_id`: Run ID
- `event_type`: User-defined event category
- `payload`: Event data

**Returns**: `Result<(sequence, hash)>`

**Concurrency**: Uses high retry count (200 retries) with exponential backoff for contention

**Example**:
```rust
let (seq, hash) = event_log.append(
    &run_id,
    "user_action",
    Value::Map(BTreeMap::from([
        ("action".into(), Value::String("login".into())),
        ("user_id".into(), Value::String("123".into())),
    ]))
)?;
println!("Event {} with hash {:?}", seq, hash);
```

#### Read Operations

##### `read` (Fast Path)

Reads a single event by sequence number.

```rust
pub fn read(&self, run_id: &RunId, sequence: u64) -> Result<Option<Event>>
```

**Performance**: <10µs (fast path)

**Example**:
```rust
if let Some(event) = event_log.read(&run_id, 0)? {
    println!("First event: {} - {:?}", event.event_type, event.payload);
}
```

##### `read_range`

Reads a range of events [start, end).

```rust
pub fn read_range(
    &self,
    run_id: &RunId,
    start: u64,
    end: u64
) -> Result<Vec<Event>>
```

**Example**:
```rust
let events = event_log.read_range(&run_id, 0, 100)?;
for event in events {
    println!("{}: {}", event.sequence, event.event_type);
}
```

##### `head`

Returns the most recent event.

```rust
pub fn head(&self, run_id: &RunId) -> Result<Option<Event>>
```

##### `len` (Fast Path)

Returns the number of events in the log.

```rust
pub fn len(&self, run_id: &RunId) -> Result<u64>
```

**Performance**: <10µs (fast path)

##### `is_empty`

Returns true if the log has no events.

```rust
pub fn is_empty(&self, run_id: &RunId) -> Result<bool>
```

#### Query Operations

##### `read_by_type`

Returns all events of a specific type.

```rust
pub fn read_by_type(&self, run_id: &RunId, event_type: &str) -> Result<Vec<Event>>
```

**Example**:
```rust
let login_events = event_log.read_by_type(&run_id, "user_login")?;
```

##### `event_types`

Returns all distinct event types in the log.

```rust
pub fn event_types(&self, run_id: &RunId) -> Result<Vec<String>>
```

#### Chain Verification

##### `verify_chain`

Validates the hash chain integrity of the entire log.

```rust
pub fn verify_chain(&self, run_id: &RunId) -> Result<ChainVerification>
```

**Example**:
```rust
let verification = event_log.verify_chain(&run_id)?;
if verification.is_valid {
    println!("Chain valid with {} events", verification.length);
} else {
    println!("Chain invalid at event {}", verification.first_invalid.unwrap());
}
```

---

### StateCell

Named state cells with compare-and-swap (CAS) operations for safe concurrent updates.

```rust
pub struct StateCell {
    db: Arc<Database>
}
```

#### Types

```rust
pub struct State {
    pub value: Value,
    pub version: u64,           // Monotonically increasing
    pub updated_at: i64,        // Milliseconds since epoch
}
```

#### Construction

```rust
impl StateCell {
    pub fn new(db: Arc<Database>) -> Self
}
```

#### Basic Operations

##### `init`

Initializes a state cell only if it doesn't exist.

```rust
pub fn init(&self, run_id: &RunId, name: &str, value: Value) -> Result<u64>
```

**Returns**: Version number (0 if created, error if exists)

**Example**:
```rust
state.init(&run_id, "counter", Value::I64(0))?;
```

##### `read` (Fast Path)

Reads the current state of a cell.

```rust
pub fn read(&self, run_id: &RunId, name: &str) -> Result<Option<State>>
```

**Performance**: <10µs (fast path)

**Example**:
```rust
if let Some(state) = state_cell.read(&run_id, "counter")? {
    println!("Counter: {:?} (version {})", state.value, state.version);
}
```

##### `exists` (Fast Path)

Checks if a state cell exists.

```rust
pub fn exists(&self, run_id: &RunId, name: &str) -> Result<bool>
```

##### `delete`

Deletes a state cell.

```rust
pub fn delete(&self, run_id: &RunId, name: &str) -> Result<bool>
```

##### `list`

Lists all state cell names.

```rust
pub fn list(&self, run_id: &RunId) -> Result<Vec<String>>
```

#### Update Operations

##### `set`

Unconditionally sets the value (creates if needed).

```rust
pub fn set(&self, run_id: &RunId, name: &str, value: Value) -> Result<u64>
```

**Returns**: New version number

**Example**:
```rust
let version = state_cell.set(&run_id, "status", Value::String("active".into()))?;
```

##### `cas`

Compare-and-swap: updates only if the current version matches expected.

```rust
pub fn cas(
    &self,
    run_id: &RunId,
    name: &str,
    expected_version: u64,
    new_value: Value
) -> Result<u64>
```

**Parameters**:
- `expected_version`: The version you expect the cell to have
- `new_value`: The new value to set

**Returns**: New version number on success, error if version mismatch

**Example**:
```rust
// Read current state
let state = state_cell.read(&run_id, "counter")?.unwrap();
let current = match state.value {
    Value::I64(n) => n,
    _ => 0,
};

// Try to increment
match state_cell.cas(&run_id, "counter", state.version, Value::I64(current + 1)) {
    Ok(new_version) => println!("Updated to version {}", new_version),
    Err(_) => println!("Conflict - retry needed"),
}
```

##### `transition`

Applies a pure function to the state with automatic retry on conflict.

```rust
pub fn transition<F, T>(
    &self,
    run_id: &RunId,
    name: &str,
    f: F
) -> Result<(T, u64)>
where
    F: Fn(&State) -> Result<(Value, T)>
```

**Parameters**:
- `f`: Pure function that takes current state and returns (new_value, user_result)

**Returns**: `(user_result, new_version)`

**Concurrency**: 200 retries with 1-50ms exponential backoff

**Important**: The closure must be pure (no I/O, no external mutations) as it may be called multiple times on conflict.

**Example**:
```rust
let (old_value, new_version) = state_cell.transition(&run_id, "counter", |state| {
    let current = match &state.value {
        Value::I64(n) => *n,
        _ => 0,
    };
    Ok((Value::I64(current + 1), current))
})?;
println!("Incremented from {} to version {}", old_value, new_version);
```

##### `transition_or_init`

Like `transition`, but initializes the cell if it doesn't exist.

```rust
pub fn transition_or_init<F, T>(
    &self,
    run_id: &RunId,
    name: &str,
    initial: Value,
    f: F
) -> Result<(T, u64)>
where
    F: Fn(&State) -> Result<(Value, T)>
```

**Example**:
```rust
let (old, version) = state_cell.transition_or_init(
    &run_id,
    "counter",
    Value::I64(0),  // Initial value if not exists
    |state| {
        let n = match &state.value { Value::I64(n) => *n, _ => 0 };
        Ok((Value::I64(n + 1), n))
    }
)?;
```

---

### TraceStore

Records agent reasoning traces for debugging and auditing.

```rust
pub struct TraceStore {
    db: Arc<Database>
}
```

#### Types

```rust
pub enum TraceType {
    ToolCall {
        tool_name: String,
        arguments: Value,
        result: Option<Value>,
        duration_ms: Option<u64>,
    },
    Decision {
        question: String,
        options: Vec<String>,
        chosen: String,
        reasoning: Option<String>,
    },
    Query {
        query_type: String,
        query: String,
        results_count: Option<u32>,
    },
    Thought {
        content: String,
        confidence: Option<f64>,
    },
    Error {
        error_type: String,
        message: String,
        recoverable: bool,
    },
    Custom {
        name: String,
        data: Value,
    },
}

pub struct Trace {
    pub id: String,                 // "trace-{uuid}"
    pub parent_id: Option<String>,  // For hierarchical tracing
    pub trace_type: TraceType,
    pub timestamp: i64,             // Milliseconds since epoch
    pub tags: Vec<String>,
    pub metadata: Value,
}

pub struct TraceTree {
    pub trace: Trace,
    pub children: Vec<TraceTree>,   // Recursive structure
}
```

#### Construction

```rust
impl TraceStore {
    pub fn new(db: Arc<Database>) -> Self
}
```

#### Record Operations

##### `record`

Records a new trace entry.

```rust
pub fn record(
    &self,
    run_id: &RunId,
    trace_type: TraceType,
    tags: Vec<String>,
    metadata: Value
) -> Result<String>
```

**Returns**: Trace ID ("trace-{uuid}")

**Example**:
```rust
let trace_id = trace.record(
    &run_id,
    TraceType::ToolCall {
        tool_name: "search".into(),
        arguments: Value::String("query".into()),
        result: Some(Value::Array(vec![])),
        duration_ms: Some(150),
    },
    vec!["search".into(), "external-api".into()],
    Value::Null
)?;
```

##### `record_child`

Records a trace as a child of an existing trace.

```rust
pub fn record_child(
    &self,
    run_id: &RunId,
    parent_id: &str,
    trace_type: TraceType,
    tags: Vec<String>,
    metadata: Value
) -> Result<String>
```

**Example**:
```rust
let parent = trace.record(&run_id, TraceType::Decision { ... }, vec![], Value::Null)?;
let child = trace.record_child(&run_id, &parent, TraceType::ToolCall { ... }, vec![], Value::Null)?;
```

#### Read Operations

##### `get` (Fast Path)

Retrieves a trace by ID.

```rust
pub fn get(&self, run_id: &RunId, trace_id: &str) -> Result<Option<Trace>>
```

**Performance**: <10µs (fast path)

##### `exists` (Fast Path)

Checks if a trace exists.

```rust
pub fn exists(&self, run_id: &RunId, trace_id: &str) -> Result<bool>
```

##### `list`

Lists all traces for a run.

```rust
pub fn list(&self, run_id: &RunId) -> Result<Vec<Trace>>
```

##### `count`

Returns the number of traces.

```rust
pub fn count(&self, run_id: &RunId) -> Result<usize>
```

#### Query Operations

##### `query_by_type`

Returns all traces of a specific type.

```rust
pub fn query_by_type(&self, run_id: &RunId, type_name: &str) -> Result<Vec<Trace>>
```

**Example**:
```rust
let tool_calls = trace.query_by_type(&run_id, "ToolCall")?;
```

##### `query_by_tag`

Returns all traces with a specific tag.

```rust
pub fn query_by_tag(&self, run_id: &RunId, tag: &str) -> Result<Vec<Trace>>
```

##### `query_by_time`

Returns all traces within a time range.

```rust
pub fn query_by_time(
    &self,
    run_id: &RunId,
    start_ms: i64,
    end_ms: i64
) -> Result<Vec<Trace>>
```

##### `get_children`

Returns all direct children of a trace.

```rust
pub fn get_children(&self, run_id: &RunId, parent_id: &str) -> Result<Vec<Trace>>
```

#### Tree Operations

##### `get_tree`

Builds a recursive tree structure from a root trace.

```rust
pub fn get_tree(&self, run_id: &RunId, root_id: &str) -> Result<Option<TraceTree>>
```

**Example**:
```rust
if let Some(tree) = trace.get_tree(&run_id, &root_trace_id)? {
    print_tree(&tree, 0);
}

fn print_tree(tree: &TraceTree, depth: usize) {
    println!("{}{}", "  ".repeat(depth), tree.trace.id);
    for child in &tree.children {
        print_tree(child, depth + 1);
    }
}
```

##### `get_roots`

Returns all traces without parents (root traces).

```rust
pub fn get_roots(&self, run_id: &RunId) -> Result<Vec<Trace>>
```

---

### RunIndex

First-class run management with status tracking and querying.

```rust
pub struct RunIndex {
    db: Arc<Database>
}
```

#### Types

```rust
pub enum RunStatus {
    Active,
    Completed,
    Failed,
    Cancelled,
    Paused,
    Archived,   // Terminal state
}

impl RunStatus {
    pub fn is_terminal(&self) -> bool       // Only Archived
    pub fn is_finished(&self) -> bool       // Completed, Failed, Cancelled
    pub fn can_transition_to(&self, target: RunStatus) -> bool
    pub fn as_str(&self) -> &'static str
}

pub struct RunMetadata {
    pub name: String,
    pub run_id: String,             // UUID
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
- `Completed` → Archived
- `Failed` → Archived
- `Cancelled` → Archived
- `Archived` → (terminal, no transitions)

#### Construction

```rust
impl RunIndex {
    pub fn new(db: Arc<Database>) -> Self
}
```

#### Create & Get Operations

##### `create_run`

Creates a new run entry.

```rust
pub fn create_run(&self, run_id: &str) -> Result<RunMetadata>
```

##### `create_run_with_options`

Creates a new run with additional options.

```rust
pub fn create_run_with_options(
    &self,
    run_id: &str,
    parent_run: Option<String>,
    tags: Vec<String>,
    metadata: Value
) -> Result<RunMetadata>
```

**Example**:
```rust
let run = run_index.create_run_with_options(
    &run_id.to_string(),
    Some(parent_run_id.to_string()),
    vec!["batch-job".into(), "priority-high".into()],
    Value::Map(BTreeMap::from([
        ("source".into(), Value::String("api".into())),
    ]))
)?;
```

##### `get_run`

Retrieves run metadata.

```rust
pub fn get_run(&self, run_id: &str) -> Result<Option<RunMetadata>>
```

##### `exists`

Checks if a run exists.

```rust
pub fn exists(&self, run_id: &str) -> Result<bool>
```

##### `list_runs`

Lists all run IDs.

```rust
pub fn list_runs(&self) -> Result<Vec<String>>
```

##### `count`

Returns the number of runs.

```rust
pub fn count(&self) -> Result<usize>
```

#### Status Updates

##### `update_status`

Updates run status with transition validation.

```rust
pub fn update_status(&self, run_id: &str, new_status: RunStatus) -> Result<RunMetadata>
```

**Convenience Methods**:

```rust
pub fn complete_run(&self, run_id: &str) -> Result<RunMetadata>
pub fn fail_run(&self, run_id: &str, error: &str) -> Result<RunMetadata>
pub fn pause_run(&self, run_id: &str) -> Result<RunMetadata>
pub fn resume_run(&self, run_id: &str) -> Result<RunMetadata>
pub fn cancel_run(&self, run_id: &str) -> Result<RunMetadata>
pub fn archive_run(&self, run_id: &str) -> Result<RunMetadata>
```

**Example**:
```rust
// Mark as completed
run_index.complete_run(&run_id.to_string())?;

// Mark as failed with error
run_index.fail_run(&run_id.to_string(), "Connection timeout")?;
```

#### Query Operations

##### `query_by_status`

Returns all runs with a specific status.

```rust
pub fn query_by_status(&self, status: RunStatus) -> Result<Vec<RunMetadata>>
```

**Example**:
```rust
let active_runs = run_index.query_by_status(RunStatus::Active)?;
let failed_runs = run_index.query_by_status(RunStatus::Failed)?;
```

##### `query_by_tag`

Returns all runs with a specific tag.

```rust
pub fn query_by_tag(&self, tag: &str) -> Result<Vec<RunMetadata>>
```

##### `get_child_runs`

Returns all child runs of a parent.

```rust
pub fn get_child_runs(&self, parent_id: &str) -> Result<Vec<RunMetadata>>
```

#### Tag & Metadata Operations

##### `add_tags`

Adds tags to a run.

```rust
pub fn add_tags(&self, run_id: &str, new_tags: Vec<String>) -> Result<RunMetadata>
```

##### `remove_tags`

Removes tags from a run.

```rust
pub fn remove_tags(&self, run_id: &str, tags_to_remove: Vec<String>) -> Result<RunMetadata>
```

##### `update_metadata`

Updates run metadata.

```rust
pub fn update_metadata(&self, run_id: &str, metadata: Value) -> Result<RunMetadata>
```

#### Delete Operations

##### `delete_run`

**Hard delete**: Removes the run and ALL associated data (KV, Events, States, Traces).

```rust
pub fn delete_run(&self, run_id: &str) -> Result<()>
```

**Warning**: This is a cascading delete. Use `archive_run` for soft delete.

---

## Transactions

### Cross-Primitive Transactions

Use extension traits for atomic operations across multiple primitives.

```rust
use in_mem::primitives::{KVStoreExt, EventLogExt, StateCellExt, TraceStoreExt};

db.transaction(&run_id, |txn| {
    // KV operations
    txn.kv_put("key", Value::String("value".into()))?;
    let value = txn.kv_get("key")?;

    // Event operations
    let seq = txn.event_append("my_event", Value::Null)?;

    // State operations
    txn.state_set("counter", Value::I64(1))?;
    let state = txn.state_read("counter")?;

    // Trace operations
    let trace_id = txn.trace_record("operation", Value::Null)?;

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

Control how writes are persisted to disk.

```rust
pub enum DurabilityMode {
    InMemory,
    Buffered { flush_interval_ms: u64, max_pending_writes: usize },
    Strict,
}
```

### InMemory

No persistence. Data is lost on crash or restart.

```rust
DurabilityMode::InMemory
```

| Property | Value |
|----------|-------|
| WAL | None |
| fsync | None |
| Latency | <3µs |
| Data Loss | All (on crash) |

**Use Cases**: Tests, caches, ephemeral data

### Buffered

Background thread fsyncs periodically or on threshold.

```rust
DurabilityMode::Buffered {
    flush_interval_ms: 100,    // fsync every 100ms
    max_pending_writes: 1000   // or after 1000 writes
}
```

| Property | Value |
|----------|-------|
| WAL | Append |
| fsync | Periodic |
| Latency | <30µs |
| Data Loss | Bounded (~100ms) |

**Use Cases**: Production default, high-throughput workloads

### Strict

Synchronous fsync after every commit.

```rust
DurabilityMode::Strict
```

| Property | Value |
|----------|-------|
| WAL | Append |
| fsync | Every write |
| Latency | ~2ms |
| Data Loss | Zero |

**Use Cases**: Audit logs, compliance, critical metadata

### Comparison Table

| Mode | Latency | Throughput | Data Loss Window |
|------|---------|------------|------------------|
| InMemory | <3µs | 250K+ ops/sec | All |
| Buffered | <30µs | 50K+ ops/sec | ~100ms |
| Strict | ~2ms | ~500 ops/sec | None |

---

## Error Types

### `Error`

All errors in **in-mem** use this type.

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
    // ... other variants
}
```

**Common Errors**:

- `Error::Io`: File system errors
- `Error::Serialization`: Value encoding/decoding failed
- `Error::Corruption`: WAL/data corruption detected
- `Error::KeyNotFound`: Key doesn't exist
- `Error::InvalidOperation`: Invalid operation for current state
- `Error::TransactionConflict`: OCC conflict (retry with fresh data)
- `Error::InvalidStatusTransition`: Invalid run status change
- `Error::VersionMismatch`: CAS version doesn't match

**Example**:
```rust
match state_cell.cas(&run_id, "counter", expected_ver, new_value) {
    Ok(version) => println!("Updated to version {}", version),
    Err(Error::VersionMismatch { expected, actual }) => {
        println!("Conflict: expected {} but was {}", expected, actual);
    }
    Err(e) => return Err(e),
}
```

---

## Performance Characteristics

### Fast Path Operations

These operations bypass transaction overhead for optimal performance:

| Operation | Target Latency | Notes |
|-----------|----------------|-------|
| `KVStore::get` | <10µs | Direct snapshot read |
| `KVStore::exists` | <10µs | Direct snapshot read |
| `KVStore::get_many` | <10µs + O(n) | Single snapshot, multiple keys |
| `EventLog::read` | <10µs | Direct snapshot read |
| `EventLog::len` | <10µs | Direct snapshot read |
| `StateCell::read` | <10µs | Direct snapshot read |
| `StateCell::exists` | <10µs | Direct snapshot read |
| `TraceStore::get` | <10µs | Direct snapshot read |

### Throughput Targets (M4)

| Mode | Target |
|------|--------|
| InMemory (1 thread) | 250K ops/sec |
| InMemory (4 threads, disjoint) | 800K+ ops/sec |
| Buffered | 50K ops/sec |
| Strict | ~500 ops/sec |

### Scaling

| Threads | Disjoint Scaling |
|---------|------------------|
| 2 | ≥1.8× |
| 4 | ≥3.2× |

---

## Thread Safety

All types are `Send` and `Sync`. The database can be safely shared across threads:

```rust
use std::sync::Arc;
use std::thread;

let db = Arc::new(Database::open("./data")?);

let handles: Vec<_> = (0..4).map(|i| {
    let db = Arc::clone(&db);
    thread::spawn(move || {
        let run_id = db.begin_run();
        let kv = KVStore::new(db.clone());

        for j in 0..1000 {
            kv.put(&run_id, &format!("key-{}-{}", i, j), Value::I64(j))?;
        }

        db.end_run(run_id)
    })
}).collect();

for handle in handles {
    handle.join().unwrap()?;
}
```

**Concurrency Model**: Optimistic Concurrency Control (OCC) with first-committer-wins conflict resolution.

---

## Version History

### v0.3.0 (M3 Primitives + M4 Performance) - 2026-01-16

**M3 Features**:
- ✅ KVStore with batch operations and transactions
- ✅ EventLog with hash chaining and type queries
- ✅ StateCell with CAS and transition closures
- ✅ TraceStore with hierarchical traces and queries
- ✅ RunIndex with status management and cascading delete
- ✅ Cross-primitive transaction support via extension traits

**M4 Features**:
- ✅ Three durability modes (InMemory, Buffered, Strict)
- ✅ Fast path API for read operations (<10µs)
- ✅ OCC transactions with snapshot isolation
- ✅ 250K+ ops/sec in InMemory mode
- ✅ Near-linear scaling for disjoint workloads

### v0.2.0 (M2 Transactions)

- ✅ OCC with snapshot isolation
- ✅ Conflict detection and retry
- ✅ Multi-key transactions

### v0.1.0 (M1 Foundation)

- ✅ Basic KV operations
- ✅ Write-ahead logging (WAL)
- ✅ Crash recovery
- ✅ Run-scoped operations

---

## See Also

- [Getting Started Guide](getting-started.md)
- [Architecture Overview](architecture.md)
- [Milestones](../milestones/MILESTONES.md)
- [GitHub Repository](https://github.com/anibjoshi/in-mem)
