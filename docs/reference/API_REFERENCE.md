# StrataDB API Reference

**Production-grade embedded database for AI agents**

StrataDB provides a unified API for storing and querying agent state across multiple primitives: key-value, JSON documents, event streams, state cells, and vector embeddings.

---

## Table of Contents

1. [Quick Start](#quick-start)
2. [Database Lifecycle](#database-lifecycle)
3. [Value Types](#value-types)
4. [Key-Value Store (KV)](#key-value-store-kv)
5. [JSON Documents](#json-documents)
6. [Event Streams](#event-streams)
7. [State Cells](#state-cells)
8. [Vector Store](#vector-store)
9. [Run Management](#run-management)
10. [Versioning](#versioning)
11. [Error Handling](#error-handling)
12. [Complete Example](#complete-example)

---

## Quick Start

```rust
use stratadb::prelude::*;

fn main() -> Result<()> {
    // Open a database
    let db = Strata::open("./my-db")?;

    // Key-value operations
    db.kv.set("user:name", "Alice")?;
    let name = db.kv.get("user:name")?;

    // JSON documents - use json! macro
    db.json.set("profile:1", json!({
        "name": "Alice",
        "age": 30
    }))?;

    // Event streams
    db.events.append("activity", json!({
        "action": "login"
    }))?;

    // Graceful shutdown
    db.close()?;
    Ok(())
}
```

---

## Database Lifecycle

### Opening a Database

#### `Strata::open(path)`

Opens a database at the specified path with default settings.

```rust
let db = Strata::open("./my-db")?;
```

| Parameter | Type | Description |
|-----------|------|-------------|
| `path` | `impl AsRef<Path>` | Directory path for database files |

**Returns:** `Result<Strata>`

---

#### `Strata::ephemeral()`

Creates a truly in-memory database with **no disk I/O at all**.

```rust
let db = Strata::ephemeral()?;
assert!(db.is_ephemeral());

// All operations work normally
db.kv.set("key", "value")?;

// Data is lost when db is dropped
drop(db);
```

Use for:
- Unit tests requiring maximum isolation and speed
- Caching scenarios
- Temporary computations

---

#### `StrataBuilder`

Builder pattern for advanced configuration.

```rust
// Production: buffered writes (default)
let db = StrataBuilder::new()
    .path("./my-db")
    .buffered()
    .open()?;

// Integration testing: temp directory, no durability
let db = StrataBuilder::new()
    .no_durability()
    .open_temp()?;

// Critical data: strict durability
let db = StrataBuilder::new()
    .path("./audit-db")
    .strict()
    .open()?;

// Unit testing: truly ephemeral (preferred)
let db = Strata::ephemeral()?;
```

##### Builder Methods

| Method | Description |
|--------|-------------|
| `new()` | Create a new builder with default settings |
| `path(path)` | Set the database directory path |
| `no_durability()` | No WAL sync (files created, but no fsync) |
| `buffered()` | Use buffered mode (default, recommended) |
| `buffered_with(interval_ms, max_writes)` | Custom buffered parameters |
| `strict()` | Use strict mode (fsync every commit) |
| `open()` | Open the database |
| `open_temp()` | Open a temporary database |

> **Note:** The `.in_memory()` method is deprecated. Use `.no_durability()` for disk-backed
> databases without fsync, or `Strata::ephemeral()` for truly file-free operation.

##### Persistence vs Durability

| Constructor/Method | Disk Files | WAL Sync | Recovery | Use Case |
|-------------------|------------|----------|----------|----------|
| `Strata::ephemeral()` | None | N/A | No | Unit tests, caching |
| `.no_durability().open_temp()` | Temp dir | No | Yes | Integration tests |
| `.buffered().open(path)` | User dir | Periodic | Yes | Production (default) |
| `.strict().open(path)` | User dir | Immediate | Yes | Critical data |

---

### Database Operations

#### `db.flush()`

Forces all pending writes to disk.

```rust
db.flush()?;
```

---

#### `db.close()`

Gracefully closes the database, flushing writes and releasing resources.

```rust
db.close()?;
```

---

#### `db.path()`

Returns the database directory path.

```rust
let path = db.path();
println!("Database at: {:?}", path);
```

---

#### `db.is_ephemeral()`

Returns `true` if this is an ephemeral (no-disk) database.

```rust
let ephemeral_db = Strata::ephemeral()?;
assert!(ephemeral_db.is_ephemeral());

let disk_db = Strata::open("./data")?;
assert!(!disk_db.is_ephemeral());
```

---

#### `db.metrics()`

Returns database metrics.

```rust
let metrics = db.metrics();
println!("Operations: {}", metrics.operations);
println!("Commit rate: {:.2}%", metrics.commit_rate * 100.0);
```

**Returns:** `DatabaseMetrics`

| Field | Type | Description |
|-------|------|-------------|
| `transactions_committed` | `u64` | Total committed transactions |
| `transactions_aborted` | `u64` | Total aborted transactions |
| `transactions_active` | `u64` | Currently active transactions |
| `commit_rate` | `f64` | Commit success rate (0.0 - 1.0) |
| `operations` | `u64` | Total operations |

---

## Value Types

StrataDB uses a canonical 8-type value model that maps cleanly to JSON.

### `Value` Enum

```rust
pub enum Value {
    Null,                           // null
    Bool(bool),                     // true, false
    Int(i64),                       // 64-bit signed integer
    Float(f64),                     // 64-bit IEEE-754 float
    String(String),                 // UTF-8 string
    Bytes(Vec<u8>),                 // Raw bytes
    Array(Vec<Value>),              // Array of values
    Object(HashMap<String, Value>), // Object with string keys
}
```

### Type Rules

1. **Eight types only** - No implicit type extensions
2. **No implicit coercions** - Types are not automatically converted
3. **Strict equality** - `Int(1) != Float(1.0)` (different types are never equal)
4. **Bytes vs String** - `Bytes` and `String` are distinct types
5. **IEEE-754 floats** - `NaN != NaN`, `-0.0 == 0.0`

### Ergonomic Conversions

Values can be created from common Rust types using `From`:

```rust
// These all work automatically
db.kv.set("name", "Alice")?;           // &str -> Value::String
db.kv.set("count", 42i64)?;            // i64 -> Value::Int
db.kv.set("price", 19.99f64)?;         // f64 -> Value::Float
db.kv.set("active", true)?;            // bool -> Value::Bool
```

### Type Checking Methods

| Method | Returns | Description |
|--------|---------|-------------|
| `is_null()` | `bool` | Check if null |
| `is_bool()` | `bool` | Check if boolean |
| `is_int()` | `bool` | Check if integer |
| `is_float()` | `bool` | Check if float |
| `is_string()` | `bool` | Check if string |
| `is_bytes()` | `bool` | Check if bytes |
| `is_array()` | `bool` | Check if array |
| `is_object()` | `bool` | Check if object |

### Value Extraction Methods

| Method | Returns | Description |
|--------|---------|-------------|
| `as_bool()` | `Option<bool>` | Get as bool |
| `as_int()` | `Option<i64>` | Get as i64 |
| `as_float()` | `Option<f64>` | Get as f64 |
| `as_str()` | `Option<&str>` | Get as string slice |
| `as_bytes()` | `Option<&[u8]>` | Get as byte slice |
| `as_array()` | `Option<&[Value]>` | Get as array slice |
| `as_object()` | `Option<&HashMap<String, Value>>` | Get as object |

---

## Key-Value Store (KV)

The KV primitive provides simple key-value storage with versioning, history, and atomic operations.

Access via `db.kv`.

### Progressive Disclosure

StrataDB follows a progressive disclosure pattern:

```rust
// Level 1: Simple (default run)
db.kv.set("key", "value")?;
db.kv.get("key")?;

// Level 2: Run-scoped
db.kv.set_in(&run, "key", "value")?;
db.kv.get_in(&run, "key")?;

// Level 3: Full control (returns version)
let version = db.kv.put(&run, "key", "value")?;
```

---

### Basic Operations

#### `db.kv.set(key, value)`

Sets a value in the default run.

```rust
db.kv.set("name", "Alice")?;
db.kv.set("age", 30i64)?;
db.kv.set("active", true)?;
```

| Parameter | Type | Description |
|-----------|------|-------------|
| `key` | `&str` | Key name |
| `value` | `impl Into<Value>` | Value to store |

**Returns:** `Result<()>`

---

#### `db.kv.get(key)`

Gets a value from the default run.

```rust
if let Some(versioned) = db.kv.get("name")? {
    // Convenience: access value directly via delegation
    println!("Name: {:?}", versioned.as_str());
    println!("Version: {:?}", versioned.version);
}
```

| Parameter | Type | Description |
|-----------|------|-------------|
| `key` | `&str` | Key name |

**Returns:** `Result<Option<Versioned<Value>>>`

---

#### `db.kv.exists(key)`

Checks if a key exists.

```rust
if db.kv.exists("name")? {
    println!("Key exists");
}
```

**Returns:** `Result<bool>`

---

#### `db.kv.delete(key)`

Deletes a key. Returns `true` if the key existed.

```rust
let existed = db.kv.delete("name")?;
```

**Returns:** `Result<bool>`

---

### Run-Scoped Operations

#### `db.kv.set_in(run, key, value)`

Sets a value in a specific run.

```rust
let run = db.runs.create(None)?;
db.kv.set_in(&run, "session:token", "abc123")?;
```

---

#### `db.kv.get_in(run, key)`

Gets a value from a specific run.

```rust
let value = db.kv.get_in(&run, "session:token")?;
```

---

#### `db.kv.exists_in(run, key)`

Checks if a key exists in a specific run.

```rust
let exists = db.kv.exists_in(&run, "session:token")?;
```

---

#### `db.kv.delete_in(run, key)`

Deletes a key from a specific run.

```rust
let existed = db.kv.delete_in(&run, "session:token")?;
```

---

### Full Control Operations

#### `db.kv.put(run, key, value)`

Sets a value and returns the version.

```rust
let version = db.kv.put(&run, "counter", 0i64)?;
println!("Written at version: {:?}", version);
```

**Returns:** `Result<Version>`

---

#### `db.kv.get_at(run, key, version)`

Gets a value at a specific version (historical read).

```rust
let old_value = db.kv.get_at(&run, "counter", version)?;
```

**Returns:** `Result<Versioned<Value>>`

---

#### `db.kv.history(run, key, limit, before)`

Gets version history for a key, newest first.

```rust
let history = db.kv.history(&run, "counter", Some(10), None)?;
for entry in history {
    println!("{:?} at {:?}", entry.value, entry.version);
}
```

| Parameter | Type | Description |
|-----------|------|-------------|
| `run` | `&RunId` | Run identifier |
| `key` | `&str` | Key name |
| `limit` | `Option<u64>` | Maximum versions to return |
| `before` | `Option<Version>` | Pagination cursor |

**Returns:** `Result<Vec<Versioned<Value>>>`

---

### Atomic Operations

#### `db.kv.incr(run, key, delta)`

Atomically increments an integer value.

```rust
let new_value = db.kv.incr(&run, "counter", 1)?;
println!("Counter is now: {}", new_value);
```

| Parameter | Type | Description |
|-----------|------|-------------|
| `run` | `&RunId` | Run identifier |
| `key` | `&str` | Key name |
| `delta` | `i64` | Amount to increment (negative for decrement) |

**Returns:** `Result<i64>` - The new value

---

#### `db.kv.cas(run, key, expected, value)`

Compare-and-swap by version. Sets the value only if the current version matches.

```rust
// Get current version
let current = db.kv.get_in(&run, "config")?;
let expected = current.map(|v| v.version);

// Try to update atomically
if db.kv.cas(&run, "config", expected, "new-value")? {
    println!("Updated successfully");
} else {
    println!("Concurrent modification detected");
}
```

| Parameter | Type | Description |
|-----------|------|-------------|
| `run` | `&RunId` | Run identifier |
| `key` | `&str` | Key name |
| `expected` | `Option<Version>` | Expected version (`None` for key must not exist) |
| `value` | `impl Into<Value>` | New value |

**Returns:** `Result<bool>` - `true` if swap succeeded

---

### Batch Operations

#### `db.kv.mget(run, keys)`

Gets multiple values atomically.

```rust
let values = db.kv.mget(&run, &["key1", "key2", "key3"])?;
for (i, val) in values.iter().enumerate() {
    println!("key{}: {:?}", i + 1, val);
}
```

**Returns:** `Result<Vec<Option<Versioned<Value>>>>`

---

#### `db.kv.mset(run, entries)`

Sets multiple values atomically in one transaction.

```rust
db.kv.mset(&run, &[
    ("name", Value::from("Alice")),
    ("age", Value::from(30i64)),
])?;
```

**Returns:** `Result<Version>`

---

#### `db.kv.mdelete(run, keys)`

Deletes multiple keys atomically.

```rust
let deleted_count = db.kv.mdelete(&run, &["key1", "key2"])?;
```

**Returns:** `Result<u64>` - Count of keys that existed

---

### Key Listing

#### `db.kv.keys(run, prefix, limit)`

Lists keys with optional prefix filter.

```rust
// List all keys
let all_keys = db.kv.keys(&run, "", None)?;

// List keys with prefix
let user_keys = db.kv.keys(&run, "user:", Some(100))?;
```

| Parameter | Type | Description |
|-----------|------|-------------|
| `run` | `&RunId` | Run identifier |
| `prefix` | `&str` | Key prefix filter (empty for all) |
| `limit` | `Option<usize>` | Maximum keys to return |

**Returns:** `Result<Vec<String>>`

---

## JSON Documents

The JSON primitive provides structured document storage with path-level operations.

Access via `db.json`.

### Basic Operations

#### `db.json.set(key, value)`

Stores a JSON document.

```rust
// Idiomatic way - use json! macro
db.json.set("user:1", json!({
    "name": "Alice",
    "age": 30,
    "active": true
}))?;

// Or with HashMap if you need programmatic construction
let mut doc = std::collections::HashMap::new();
doc.insert("name".to_string(), Value::from("Alice"));
doc.insert("age".to_string(), Value::from(30i64));
db.json.set("user:2", doc)?;
```

**Returns:** `Result<Version>`

---

#### `db.json.get(key)`

Retrieves a JSON document.

```rust
if let Some(versioned) = db.json.get("user:1")? {
    // Convenience: as_object() delegates to inner value
    let doc = versioned.as_object().unwrap();
    println!("Name: {:?}", doc.get("name"));
}
```

**Returns:** `Result<Option<Versioned<Value>>>`

---

### Run-Scoped Operations

#### `db.json.set_in(run, key, value)`

Stores a document in a specific run.

```rust
db.json.set_in(&run, "config", config_doc)?;
```

---

#### `db.json.get_in(run, key)`

Retrieves a document from a specific run.

```rust
let doc = db.json.get_in(&run, "config")?;
```

---

### Path Operations

JSON paths use JSONPath syntax (`$` for root, `$.field` for nested access).

#### `db.json.get_path(run, key, path)`

Gets a value at a specific path within a document.

```rust
let name = db.json.get_path(&run, "user:1", "$.name")?;
let city = db.json.get_path(&run, "user:1", "$.address.city")?;
```

---

#### `db.json.set_path(run, key, path, value)`

Sets a value at a specific path.

```rust
db.json.set_path(&run, "user:1", "$.name", "Bob")?;
db.json.set_path(&run, "user:1", "$.address.city", "NYC")?;
```

---

#### `db.json.delete_path(run, key, path)`

Deletes a path within a document.

```rust
db.json.delete_path(&run, "user:1", "$.temporary_field")?;
```

**Returns:** `Result<u64>` - Number of paths deleted

---

#### `db.json.merge(run, key, path, patch)`

Merges a value using JSON Merge Patch (RFC 7396).

```rust
// Partial update - only modifies specified fields
db.json.merge(&run, "user:1", "$", json!({"age": 31}))?;
```

---

#### `db.json.exists(run, key)`

Checks if a document exists.

```rust
if db.json.exists(&run, "user:1")? {
    println!("Document exists");
}
```

---

## Event Streams

The Events primitive provides append-only event streams for logging and event sourcing.

Access via `db.events`.

### Basic Operations

#### `db.events.append(stream, payload)`

Appends an event to a stream.

```rust
// Use json! macro for clean event creation
db.events.append("activity", json!({
    "action": "login",
    "user_id": "user:1"
}))?;
```

| Parameter | Type | Description |
|-----------|------|-------------|
| `stream` | `&str` | Stream name |
| `payload` | `impl Into<Value>` | Event payload (must be an Object) |

**Returns:** `Result<Version>`

---

#### `db.events.read(stream, limit)`

Reads events from a stream.

```rust
let events = db.events.read("activity", 100)?;
for event in events {
    println!("{:?}: {:?}", event.version, event.value);
}
```

**Returns:** `Result<Vec<Versioned<Value>>>`

---

### Run-Scoped Operations

#### `db.events.append_in(run, stream, payload)`

Appends an event in a specific run.

```rust
db.events.append_in(&run, "trace", event)?;
```

---

#### `db.events.read_in(run, stream, limit)`

Reads events from a specific run.

```rust
let events = db.events.read_in(&run, "trace", 50)?;
```

---

### Range Queries

#### `db.events.range(run, stream, start, end)`

Reads events in a sequence range.

```rust
let events = db.events.range(&run, "activity", 10, 20)?;
```

| Parameter | Type | Description |
|-----------|------|-------------|
| `run` | `&RunId` | Run identifier |
| `stream` | `&str` | Stream name |
| `start` | `u64` | Starting sequence (inclusive) |
| `end` | `u64` | Ending sequence (inclusive) |

---

#### `db.events.head(run, stream)`

Gets the latest event in a stream.

```rust
if let Some(latest) = db.events.head(&run, "activity")? {
    println!("Latest event: {:?}", latest.value);
}
```

---

#### `db.events.count(run, stream)`

Gets the count of events in a stream.

```rust
let count = db.events.count(&run, "activity")?;
println!("Total events: {}", count);
```

---

#### `db.events.streams(run)`

Lists all stream names in a run.

```rust
let streams = db.events.streams(&run)?;
for name in streams {
    println!("Stream: {}", name);
}
```

---

## State Cells

The State primitive provides compare-and-swap (CAS) cells for coordination and locks.

Access via `db.state`.

### Basic Operations

#### `db.state.set(key, value)`

Sets a state cell value.

```rust
db.state.set("task:status", "running")?;
db.state.set("task:progress", 50i64)?;
```

**Returns:** `Result<Version>`

---

#### `db.state.get(key)`

Gets a state cell value.

```rust
if let Some(state) = db.state.get("task:status")? {
    // Convenience: as_str() delegates to inner value
    println!("Status: {:?}", state.as_str());
}
```

**Returns:** `Result<Option<Versioned<Value>>>`

---

### Run-Scoped Operations

#### `db.state.set_in(run, key, value)`

Sets a state cell in a specific run.

```rust
db.state.set_in(&run, "status", "active")?;
```

---

#### `db.state.get_in(run, key)`

Gets a state cell from a specific run.

```rust
let state = db.state.get_in(&run, "status")?;
```

---

### Compare-and-Swap

#### `db.state.cas(run, key, expected_counter, value)`

Atomic compare-and-swap by counter.

```rust
// Get current counter
let current = db.state.get_in(&run, "lock")?;
let counter = current.map(|v| {
    match v.version {
        Version::Counter(c) => c,
        _ => 0,
    }
});

// Try to acquire lock
if let Some(version) = db.state.cas(&run, "lock", counter, "acquired")? {
    println!("Lock acquired at {:?}", version);
} else {
    println!("Lock contention - retry");
}
```

| Parameter | Type | Description |
|-----------|------|-------------|
| `run` | `&RunId` | Run identifier |
| `key` | `&str` | Cell name |
| `expected_counter` | `Option<u64>` | Expected counter (`None` for cell must not exist) |
| `value` | `impl Into<Value>` | New value |

**Returns:** `Result<Option<Version>>` - `Some(version)` if successful, `None` if CAS failed

---

### Additional Operations

#### `db.state.delete(run, key)`

Deletes a state cell.

```rust
db.state.delete(&run, "lock")?;
```

---

#### `db.state.exists(run, key)`

Checks if a state cell exists.

```rust
let exists = db.state.exists(&run, "lock")?;
```

---

#### `db.state.history(run, key, limit)`

Gets version history for a state cell.

```rust
let history = db.state.history(&run, "status", Some(10))?;
```

---

## Vector Store

The Vectors primitive provides vector embeddings storage with similarity search.

Access via `db.vectors`.

### Collection Management

#### `db.vectors.create_collection(run, name, dimension, metric)`

Creates a vector collection.

```rust
// DistanceMetric is included in prelude
db.vectors.create_collection(
    &run,
    "embeddings",
    384,  // dimension
    DistanceMetric::Cosine,
)?;
```

| Parameter | Type | Description |
|-----------|------|-------------|
| `run` | `&RunId` | Run identifier |
| `name` | `&str` | Collection name |
| `dimension` | `usize` | Vector dimension (fixed for collection) |
| `metric` | `DistanceMetric` | Distance metric for similarity |

##### Distance Metrics

| Metric | Description | Use Case |
|--------|-------------|----------|
| `Cosine` | Cosine similarity | Text embeddings, normalized vectors |
| `Euclidean` | L2 distance | General purpose |
| `DotProduct` | Dot product | When vectors are pre-normalized |

---

#### `db.vectors.delete_collection(run, collection)`

Deletes a collection and all its vectors.

```rust
db.vectors.delete_collection(&run, "old_embeddings")?;
```

**Returns:** `Result<bool>` - `true` if collection existed

---

#### `db.vectors.list_collections(run)`

Lists all collections in a run.

```rust
let collections = db.vectors.list_collections(&run)?;
for col in collections {
    println!("{}: {} dimensions, {} vectors", col.name, col.dimension, col.count);
}
```

---

### Vector Operations

#### `db.vectors.upsert(run, collection, key, vector, metadata)`

Inserts or updates a vector.

```rust
let embedding = vec![0.1, 0.2, 0.3, /* ... */ 0.384];

// Use json! for metadata
db.vectors.upsert(
    &run,
    "embeddings",
    "doc:1",
    &embedding,
    Some(json!({"title": "Hello World"}).into()),
)?;
```

| Parameter | Type | Description |
|-----------|------|-------------|
| `run` | `&RunId` | Run identifier |
| `collection` | `&str` | Collection name |
| `key` | `&str` | Vector key (unique identifier) |
| `vector` | `&[f32]` | Vector data (must match collection dimension) |
| `metadata` | `Option<Value>` | Optional metadata (Object) |

---

#### `db.vectors.get(run, collection, key)`

Gets a vector by key.

```rust
if let Some(data) = db.vectors.get(&run, "embeddings", "doc:1")? {
    println!("Vector: {:?}", data.value.vector);
    println!("Metadata: {:?}", data.value.metadata);
}
```

**Returns:** `Result<Option<Versioned<VectorData>>>`

---

#### `db.vectors.delete(run, collection, key)`

Deletes a vector.

```rust
db.vectors.delete(&run, "embeddings", "doc:1")?;
```

---

### Similarity Search

#### `db.vectors.search(run, collection, query, k, filter)`

Searches for similar vectors.

```rust
let query_embedding = vec![0.1, 0.2, 0.3, /* ... */];

let results = db.vectors.search(
    &run,
    "embeddings",
    &query_embedding,
    10,   // k - number of results
    None, // optional filter
)?;

for result in results {
    println!("{}: score={:.4}", result.key, result.score);
}
```

| Parameter | Type | Description |
|-----------|------|-------------|
| `run` | `&RunId` | Run identifier |
| `collection` | `&str` | Collection name |
| `query` | `&[f32]` | Query vector |
| `k` | `usize` | Number of results to return |
| `filter` | `Option<SearchFilter>` | Optional metadata filter |

**Returns:** `Result<Vec<VectorMatch>>`

##### `VectorMatch` Fields

| Field | Type | Description |
|-------|------|-------------|
| `key` | `String` | Vector key |
| `score` | `f32` | Similarity score (higher is better for cosine) |
| `vector` | `Option<Vec<f32>>` | Vector data (if requested) |
| `metadata` | `Option<Value>` | Metadata (if present) |

---

#### `db.vectors.search_with_threshold(run, collection, query, k, threshold, filter)`

Searches with a minimum score threshold.

```rust
let results = db.vectors.search_with_threshold(
    &run,
    "embeddings",
    &query,
    10,
    0.7,  // minimum score
    None,
)?;
```

---

#### `db.vectors.count(run, collection)`

Gets the count of vectors in a collection.

```rust
let count = db.vectors.count(&run, "embeddings")?;
```

---

#### `db.vectors.collection_info(run, collection)`

Gets detailed collection information.

```rust
if let Some(info) = db.vectors.collection_info(&run, "embeddings")? {
    println!("Name: {}", info.name);
    println!("Dimension: {}", info.dimension);
    println!("Count: {}", info.count);
    println!("Metric: {:?}", info.metric);
}
```

---

## Run Management

Runs are isolated namespaces for organizing data. They enable multi-tenancy, experimentation, and lifecycle management.

Access via `db.runs`.

### Lifecycle Operations

#### `db.runs.create(metadata)`

Creates a new run.

```rust
// Simple run
let run = db.runs.create(None)?;

// Run with metadata - use json! macro
let run = db.runs.create(Some(json!({
    "name": "experiment-1",
    "user": "alice"
}).into()))?;
```

**Returns:** `Result<RunId>`

---

#### `db.runs.get(run)`

Gets information about a run.

```rust
if let Some(info) = db.runs.get(&run)? {
    println!("Run ID: {}", info.value.run_id);
    println!("State: {:?}", info.value.state);
    println!("Created: {}", info.value.created_at);
}
```

**Returns:** `Result<Option<Versioned<RunInfo>>>`

---

#### `db.runs.list(state, limit)`

Lists runs, optionally filtered by state.

```rust
// List all runs
let all = db.runs.list(None, None)?;

// List active runs
let active = db.runs.list(Some(RunState::Active), Some(100))?;
```

---

#### `db.runs.exists(run)`

Checks if a run exists.

```rust
if db.runs.exists(&run)? {
    println!("Run exists");
}
```

---

#### `db.runs.is_active(run)`

Checks if a run exists and is active.

```rust
if db.runs.is_active(&run)? {
    // Safe to write
}
```

---

### State Transitions

#### Run States

| State | Description | Accepts Writes |
|-------|-------------|----------------|
| `Active` | Running, accepting operations | Yes |
| `Paused` | Temporarily paused (can resume) | No |
| `Completed` | Successfully finished | No |
| `Failed` | Finished with error | No |
| `Cancelled` | User cancelled | No |
| `Archived` | Terminal state (soft delete) | No |

---

#### `db.runs.close(run)`

Closes a run (marks as completed).

```rust
db.runs.close(&run)?;
```

---

#### `db.runs.pause(run)`

Pauses a run (can be resumed later).

```rust
db.runs.pause(&run)?;
```

---

#### `db.runs.resume(run)`

Resumes a paused run.

```rust
db.runs.resume(&run)?;
```

---

#### `db.runs.fail(run, error)`

Marks a run as failed with an error message.

```rust
db.runs.fail(&run, "Connection timeout")?;
```

---

#### `db.runs.cancel(run)`

Cancels a run.

```rust
db.runs.cancel(&run)?;
```

---

#### `db.runs.archive(run)`

Archives a run (terminal state).

```rust
db.runs.archive(&run)?;
```

---

### Metadata

#### `db.runs.update_metadata(run, metadata)`

Updates run metadata.

```rust
db.runs.update_metadata(&run, json!({"status": "processing"}).into())?;
```

---

### Retention Policy

#### `db.runs.set_retention(run, policy)`

Sets version history retention policy.

```rust
use std::time::Duration;

// Keep last 100 versions
db.runs.set_retention(&run, RetentionPolicy::KeepLast(100))?;

// Keep 7 days of history
db.runs.set_retention(&run, RetentionPolicy::KeepFor(Duration::from_secs(7 * 24 * 60 * 60)))?;

// Keep all history (default)
db.runs.set_retention(&run, RetentionPolicy::KeepAll)?;
```

---

#### `db.runs.get_retention(run)`

Gets the current retention policy.

```rust
let policy = db.runs.get_retention(&run)?;
```

---

### Default Run

#### `db.runs.default_run()`

Gets the default run ID. The default run is automatically available.

```rust
let default = db.runs.default_run();
```

---

## Versioning

Every mutation produces a version. Every read returns version information.

### `Version` Enum

```rust
pub enum Version {
    Txn(u64),      // Transaction-based (KV, JSON, Vector, Run)
    Sequence(u64), // Position-based (Events)
    Counter(u64),  // Per-entity counter (State)
}
```

| Variant | Used By | Description |
|---------|---------|-------------|
| `Txn` | KV, JSON, Vectors, Runs | Global transaction ID |
| `Sequence` | Events | Position in append-only log |
| `Counter` | State | Per-entity mutation counter |

### Version Methods

| Method | Returns | Description |
|--------|---------|-------------|
| `as_u64()` | `u64` | Get numeric value |
| `is_txn()` | `bool` | Check if transaction-based |
| `is_sequence()` | `bool` | Check if sequence-based |
| `is_counter()` | `bool` | Check if counter-based |
| `is_zero()` | `bool` | Check if version is zero |

---

### `Versioned<T>` Wrapper

All read operations return `Versioned<T>`:

```rust
pub struct Versioned<T> {
    pub value: T,           // The actual data
    pub version: Version,   // Version identifier
    pub timestamp: Timestamp, // Creation timestamp (microseconds)
}
```

### Versioned Methods

| Method | Returns | Description |
|--------|---------|-------------|
| `value()` | `&T` | Reference to inner value |
| `into_value()` | `T` | Consume and return value |
| `version()` | `Version` | Get version |
| `timestamp()` | `Timestamp` | Get timestamp |
| `age()` | `Option<Duration>` | Get age of this version |
| `is_older_than(dur)` | `bool` | Check if older than duration |

### Convenience Methods for `Versioned<Value>`

These methods delegate to the inner `Value`, reducing verbosity:

| Method | Returns | Description |
|--------|---------|-------------|
| `as_str()` | `Option<&str>` | Get inner value as string |
| `as_int()` | `Option<i64>` | Get inner value as integer |
| `as_float()` | `Option<f64>` | Get inner value as float |
| `as_bool()` | `Option<bool>` | Get inner value as boolean |
| `as_array()` | `Option<&[Value]>` | Get inner value as array |
| `as_object()` | `Option<&HashMap<String, Value>>` | Get inner value as object |
| `is_null()` | `bool` | Check if inner value is null |
| `is_string()` | `bool` | Check if inner value is string |

```rust
// Before (verbose)
let name = db.kv.get("name")?.unwrap().value.as_str();

// After (clean)
let name = db.kv.get("name")?.unwrap().as_str();
```

---

## Error Handling

### `Error` Enum

```rust
pub enum Error {
    NotFound(String),           // Entity not found
    WrongType { expected, actual }, // Type mismatch
    InvalidKey(String),         // Invalid key format
    InvalidPath(String),        // Invalid JSON path
    Conflict(String),           // Version conflict, CAS failure
    ConstraintViolation(String), // Invalid input, limits exceeded
    RunError(String),           // Run closed or not found
    Io(std::io::Error),         // I/O error
    Serialization(String),      // Serialization error
    Storage(String),            // Storage error
    Internal(String),           // Internal error (bug)
}
```

### Error Methods

| Method | Returns | Description |
|--------|---------|-------------|
| `is_retryable()` | `bool` | Can retry (conflicts) |
| `is_not_found()` | `bool` | Entity not found |
| `is_conflict()` | `bool` | Version/write conflict |
| `is_serious()` | `bool` | Unrecoverable error |

### Error Handling Pattern

```rust
use stratadb::prelude::*;

fn handle_cas_retry(db: &Strata, run: &RunId) -> Result<()> {
    loop {
        let current = db.kv.get_in(run, "counter")?;
        let expected = current.as_ref().map(|v| v.version);
        let new_val = current.map(|v| v.value.as_int().unwrap_or(0) + 1).unwrap_or(1);

        match db.kv.cas(run, "counter", expected, new_val) {
            Ok(true) => return Ok(()),
            Ok(false) => continue, // Retry on conflict
            Err(e) if e.is_retryable() => continue,
            Err(e) => return Err(e),
        }
    }
}
```

---

## Complete Example

```rust
use stratadb::prelude::*;

fn main() -> Result<()> {
    // Open database
    let db = StrataBuilder::new()
        .path("./agent-memory")
        .buffered()
        .open()?;

    // Create a conversation run with metadata
    let conversation = db.runs.create(Some(json!({
        "user": "alice",
        "topic": "weather"
    }).into()))?;

    // Store conversation context (KV)
    db.kv.set_in(&conversation, "user:name", "Alice")?;
    db.kv.set_in(&conversation, "user:location", "NYC")?;

    // Store user profile (JSON) - use json! macro for ergonomic syntax
    db.json.set_in(&conversation, "user:profile", json!({
        "preferences": {
            "units": "metric",
            "theme": "dark"
        }
    }))?;

    // Log agent actions (Events) - json! macro makes this clean
    db.events.append_in(&conversation, "trace", json!({
        "type": "user_message",
        "content": "What's the weather?"
    }))?;

    db.events.append_in(&conversation, "trace", json!({
        "type": "tool_call",
        "tool": "weather_api"
    }))?;

    // Track current state (State)
    db.state.set_in(&conversation, "agent:status", "thinking")?;

    // Store embeddings (Vectors)
    db.vectors.create_collection(&conversation, "memory", 4, DistanceMetric::Cosine)?;
    db.vectors.upsert(
        &conversation,
        "memory",
        "mem:1",
        &[0.1, 0.2, 0.3, 0.4],
        Some(Value::from("User asked about weather")),
    )?;

    // Read everything back
    let name = db.kv.get_in(&conversation, "user:name")?;
    println!("User: {:?}", name.map(|v| v.value));

    let events = db.events.read_in(&conversation, "trace", 10)?;
    println!("Events: {}", events.len());

    let similar = db.vectors.search(&conversation, "memory", &[0.1, 0.2, 0.3, 0.4], 5, None)?;
    println!("Similar memories: {}", similar.len());

    // Close the conversation
    db.runs.close(&conversation)?;

    // Flush and close
    db.flush()?;
    db.close()?;

    Ok(())
}
```

---

## Prelude

For convenience, import everything you need with:

```rust
use stratadb::prelude::*;
```

This imports:

- `Strata`, `StrataBuilder` - Database entry points
- `Error`, `Result` - Error handling
- `KV`, `Json`, `Events`, `State`, `Vectors`, `Runs` - Primitives
- `Value`, `Version`, `Versioned`, `RunId`, `Timestamp` - Core types
- `DistanceMetric` - Vector similarity metrics
- `RunState`, `RetentionPolicy` - Run management types
- `json!` macro - JSON construction

---

## Version

This documentation is for StrataDB v0.1.0.
