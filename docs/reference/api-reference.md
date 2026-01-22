# API Reference

Complete API reference for **Strata** v0.7.0 (M7 Durability, Snapshots & Replay).

## Table of Contents

- [Core Types](#core-types)
- [Primitives](#primitives)
  - [KVStore](#kvstore)
  - [EventLog](#eventlog)
  - [StateCell](#statecell)
  - [TraceStore](#tracestore)
  - [RunIndex](#runindex)
  - [JsonStore](#jsonstore)
- [Search](#search)
  - [SearchRequest](#searchrequest)
  - [SearchResponse](#searchresponse)
  - [HybridSearch](#hybridsearch)
  - [InvertedIndex](#invertedindex)
- [Snapshots](#snapshots)
  - [SnapshotConfig](#snapshotconfig)
  - [SnapshotInfo](#snapshotinfo)
- [Recovery](#recovery)
  - [RecoveryOptions](#recoveryoptions)
  - [RecoveryResult](#recoveryresult)
- [Replay](#replay)
  - [ReadOnlyView](#readonlyview)
  - [RunDiff](#rundiff)
- [Run Lifecycle](#run-lifecycle)
- [WAL Types](#wal-types)
- [Storage Extension](#storage-extension)
- [Transactions](#transactions)
- [Durability Modes](#durability-modes)
- [Error Types](#error-types)
- [Performance Characteristics](#performance-characteristics)

---

## Core Types

### `Database`

Main entry point for interacting with **Strata**.

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
use strata::{Database, DurabilityMode};

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

### JsonStore

JSON document storage with path-level mutations and region-based conflict detection.

```rust
pub struct JsonStore {
    db: Arc<Database>
}
```

#### Document Operations

##### `put`

Stores a JSON document.

```rust
pub fn put(&self, run_id: &RunId, key: &str, value: serde_json::Value) -> Result<()>
```

##### `get` (Fast Path)

Retrieves a JSON document.

```rust
pub fn get(&self, run_id: &RunId, key: &str) -> Result<Option<serde_json::Value>>
```

##### `delete`

Deletes a JSON document.

```rust
pub fn delete(&self, run_id: &RunId, key: &str) -> Result<bool>
```

#### Path Operations

##### `get_path`

Retrieves a value at a JSONPath.

```rust
pub fn get_path(&self, run_id: &RunId, key: &str, path: &str) -> Result<Option<serde_json::Value>>
```

##### `set_path`

Sets a value at a JSONPath.

```rust
pub fn set_path(&self, run_id: &RunId, key: &str, path: &str, value: serde_json::Value) -> Result<()>
```

##### `delete_path`

Deletes a value at a JSONPath.

```rust
pub fn delete_path(&self, run_id: &RunId, key: &str, path: &str) -> Result<bool>
```

#### Array Operations

##### `array_push`

Appends a value to an array at a path.

```rust
pub fn array_push(&self, run_id: &RunId, key: &str, path: &str, value: serde_json::Value) -> Result<()>
```

##### `array_pop`

Removes and returns the last element of an array.

```rust
pub fn array_pop(&self, run_id: &RunId, key: &str, path: &str) -> Result<Option<serde_json::Value>>
```

#### Conflict Detection

JsonStore uses region-based conflict detection:
- Path mutations only conflict if they touch overlapping regions
- Parent-child path conflicts are detected
- Sibling paths can be mutated concurrently

---

## Search

### SearchRequest

Configuration for search queries.

```rust
pub struct SearchRequest {
    pub query: String,
    pub limit: usize,
    pub offset: usize,
    pub budget_ms: Option<u64>,
    pub primitives: Option<Vec<PrimitiveType>>,
    pub run_scope: Option<RunId>,
}
```

#### Construction

```rust
let request = SearchRequest::new("search query")
    .with_limit(10)
    .with_offset(0)
    .with_budget_ms(50)
    .with_primitives(vec![PrimitiveType::Kv, PrimitiveType::Event]);
```

---

### SearchResponse

Results from a search operation.

```rust
pub struct SearchResponse {
    pub results: Vec<SearchResult>,
    pub total_matches: usize,
    pub budget_exhausted: bool,
    pub search_time_ms: u64,
}

pub struct SearchResult {
    pub doc_ref: DocRef,
    pub score: f64,
    pub highlights: Vec<String>,
    pub metadata: Value,
}
```

---

### DocRef

Reference to a document in any primitive.

```rust
pub enum DocRef {
    Kv { key: Key },
    Event { sequence: u64 },
    State { name: String },
    Trace { id: String },
    Run { run_id: String },
    Json { key: Key },
}
```

---

### HybridSearch

Unified search across all primitives.

```rust
pub struct HybridSearch {
    db: Arc<Database>
}
```

#### Methods

##### `search`

Executes a hybrid search combining keyword and semantic results.

```rust
pub fn search(&self, run_id: &RunId, request: SearchRequest) -> Result<SearchResponse>
```

##### `search_kv`

Searches only in KVStore.

##### `search_events`

Searches only in EventLog.

##### `search_json`

Searches only in JsonStore.

---

### InvertedIndex

Optional full-text index for improved search performance.

```rust
pub struct InvertedIndex { /* internal */ }
```

#### Methods

##### `new`

Creates a new index (disabled by default).

```rust
pub fn new() -> InvertedIndex
```

##### `enable` / `disable`

Enables or disables the index.

##### `index_document`

Adds a document to the index.

```rust
pub fn index_document(&self, doc_ref: &DocRef, content: &str, title: Option<&str>)
```

##### `remove_document`

Removes a document from the index.

##### `lookup`

Looks up documents containing a term.

```rust
pub fn lookup(&self, term: &str) -> Option<Vec<DocRef>>
```

##### `compute_idf`

Computes inverse document frequency for a term.

##### `total_docs` / `avg_doc_len` / `doc_freq`

Index statistics methods.

##### `version` / `wait_for_version`

Version tracking for cache invalidation.

---

## Snapshots

Periodic snapshots enable bounded recovery time by capturing consistent database state.

### SnapshotConfig

Configuration for automatic snapshot triggers.

```rust
pub struct SnapshotConfig {
    /// Trigger snapshot when WAL exceeds this size (bytes)
    pub wal_size_threshold: u64,
    /// Trigger snapshot every N minutes
    pub time_interval_minutes: u32,
    /// Number of old snapshots to retain
    pub retention_count: usize,
    /// Whether to snapshot on clean shutdown
    pub snapshot_on_shutdown: bool,
}

impl Default for SnapshotConfig {
    fn default() -> Self {
        SnapshotConfig {
            wal_size_threshold: 100 * 1024 * 1024,  // 100 MB
            time_interval_minutes: 30,
            retention_count: 2,
            snapshot_on_shutdown: true,
        }
    }
}
```

### SnapshotInfo

Information about a created snapshot.

```rust
pub struct SnapshotInfo {
    /// Snapshot file path
    pub path: PathBuf,
    /// Snapshot timestamp (microseconds since epoch)
    pub timestamp_micros: u64,
    /// WAL offset this snapshot covers up to
    pub wal_offset: u64,
}
```

### Snapshot Methods

##### `snapshot`

Creates a snapshot manually.

```rust
impl Database {
    pub fn snapshot(&self) -> Result<SnapshotInfo>
}
```

##### `configure_snapshots`

Configures automatic snapshot triggers.

```rust
impl Database {
    pub fn configure_snapshots(&self, config: SnapshotConfig)
}
```

##### `list_snapshots`

Lists all available snapshots.

```rust
impl Database {
    pub fn list_snapshots(&self) -> Result<Vec<SnapshotInfo>>
}
```

##### `delete_snapshot`

Deletes a specific snapshot.

```rust
impl Database {
    pub fn delete_snapshot(&self, info: &SnapshotInfo) -> Result<()>
}
```

---

## Recovery

Crash recovery restores database state from snapshots and WAL replay.

### RecoveryOptions

Options for controlling recovery behavior.

```rust
pub struct RecoveryOptions {
    /// Maximum corrupt entries to tolerate before failing
    pub max_corrupt_entries: usize,
    /// Whether to verify all checksums (slower but safer)
    pub verify_all_checksums: bool,
    /// Whether to rebuild indexes after recovery
    pub rebuild_indexes: bool,
}

impl Default for RecoveryOptions {
    fn default() -> Self {
        RecoveryOptions {
            max_corrupt_entries: 10,
            verify_all_checksums: true,
            rebuild_indexes: true,
        }
    }
}
```

### RecoveryResult

Information about a completed recovery.

```rust
pub struct RecoveryResult {
    /// Snapshot used (if any)
    pub snapshot_used: Option<SnapshotInfo>,
    /// WAL entries replayed
    pub wal_entries_replayed: u64,
    /// Transactions recovered
    pub transactions_recovered: u64,
    /// Orphaned transactions skipped
    pub orphaned_transactions: u64,
    /// Corrupt entries skipped
    pub corrupt_entries_skipped: u64,
    /// Recovery time (microseconds)
    pub recovery_time_micros: u64,
}
```

### Recovery Methods

##### `open_with_options`

Opens a database with custom recovery options.

```rust
impl Database {
    pub fn open_with_options(
        path: &Path,
        options: RecoveryOptions,
    ) -> Result<Database>
}
```

##### `last_recovery_result`

Gets the result from the most recent recovery.

```rust
impl Database {
    pub fn last_recovery_result(&self) -> Option<&RecoveryResult>
}
```

**Example**:
```rust
let db = Database::open("./data")?;

if let Some(result) = db.last_recovery_result() {
    println!("Recovered {} transactions", result.transactions_recovered);
    if result.corrupt_entries_skipped > 0 {
        println!("WARNING: {} corrupt entries skipped", result.corrupt_entries_skipped);
    }
}
```

---

## Replay

Deterministic replay reconstructs agent run state from EventLog.

### ReadOnlyView

Read-only view of a run's state from replay.

```rust
pub struct ReadOnlyView {
    /// Run this view is for
    pub run_id: RunId,
    // Internal state maps (KV, JSON, Event, State, Trace)
}

impl ReadOnlyView {
    /// Get KV value
    pub fn get_kv(&self, key: &Key) -> Option<&Value>;

    /// Get JSON document
    pub fn get_json(&self, key: &Key) -> Option<&JsonDoc>;

    /// Get all events
    pub fn events(&self) -> &[Event];

    /// Get state value
    pub fn get_state(&self, key: &Key) -> Option<&StateValue>;

    /// Get all traces
    pub fn traces(&self) -> &[Span];

    /// List all keys in this view
    pub fn keys(&self) -> impl Iterator<Item = &Key>;
}
```

### RunDiff

Difference between two runs at key level.

```rust
pub struct RunDiff {
    pub run_a: RunId,
    pub run_b: RunId,
    /// Keys added in B (not in A)
    pub added: Vec<DiffEntry>,
    /// Keys removed in B (in A but not B)
    pub removed: Vec<DiffEntry>,
    /// Keys modified (different values)
    pub modified: Vec<DiffEntry>,
}

pub struct DiffEntry {
    pub key: Key,
    pub primitive: PrimitiveKind,
    pub value_a: Option<String>,
    pub value_b: Option<String>,
}
```

### Replay Methods

##### `replay_run`

Replays a run and returns a read-only view.

```rust
impl Database {
    pub fn replay_run(&self, run_id: RunId) -> Result<ReadOnlyView>
}
```

**Important**: Replay is side-effect free. It does NOT mutate the canonical store.

##### `diff_runs`

Compares two runs at key level.

```rust
impl Database {
    pub fn diff_runs(&self, run_a: RunId, run_b: RunId) -> Result<RunDiff>
}
```

**Example**:
```rust
// Replay a completed run
let view = db.replay_run(run_id)?;
println!("Run had {} KV entries", view.keys().count());
println!("Run had {} events", view.events().len());

// Compare two runs
let diff = db.diff_runs(run_a, run_b)?;
println!("Added: {:?}", diff.added.len());
println!("Removed: {:?}", diff.removed.len());
println!("Modified: {:?}", diff.modified.len());
```

---

## Run Lifecycle

Explicit run lifecycle management with status tracking.

### RunStatus

```rust
pub enum RunStatus {
    Active,      // Run is in progress
    Completed,   // Run ended normally
    Orphaned,    // Run was never ended (crash detected)
    NotFound,    // Run doesn't exist
}
```

### Run Lifecycle Methods

##### `begin_run`

Begins a new run.

```rust
impl Database {
    pub fn begin_run(&self, run_id: RunId) -> Result<()>
}
```

##### `end_run`

Ends a run normally.

```rust
impl Database {
    pub fn end_run(&self, run_id: RunId) -> Result<()>
}
```

##### `abort_run`

Aborts a run with a failure reason.

```rust
impl Database {
    pub fn abort_run(&self, run_id: RunId, reason: &str) -> Result<()>
}
```

##### `run_status`

Gets the status of a run.

```rust
impl Database {
    pub fn run_status(&self, run_id: RunId) -> Result<RunStatus>
}
```

##### `orphaned_runs`

Lists runs that were never ended (detected after crash).

```rust
impl Database {
    pub fn orphaned_runs(&self) -> Result<Vec<RunId>>
}
```

**Example**:
```rust
let run_id = RunId::new();
db.begin_run(run_id)?;

// Do work
db.kv.put(&run_id, "key", Value::String("value".into()))?;

// End run
db.end_run(run_id)?;

// Check for orphaned runs after restart
for orphan in db.orphaned_runs()? {
    println!("Orphaned run detected: {:?}", orphan);
}
```

---

## WAL Types

Write-ahead log types for durability and recovery.

### WalEntryType

Registry of WAL entry types.

```rust
#[repr(u8)]
pub enum WalEntryType {
    // Core (0x00-0x0F)
    TransactionCommit = 0x00,
    TransactionAbort = 0x01,
    SnapshotMarker = 0x02,

    // KV (0x10-0x1F)
    KvPut = 0x10,
    KvDelete = 0x11,

    // JSON (0x20-0x2F)
    JsonCreate = 0x20,
    JsonSet = 0x21,
    JsonDelete = 0x22,
    JsonPatch = 0x23,

    // Event (0x30-0x3F)
    EventAppend = 0x30,

    // State (0x40-0x4F)
    StateInit = 0x40,
    StateSet = 0x41,
    StateTransition = 0x42,

    // Trace (0x50-0x5F)
    TraceRecord = 0x50,

    // Run (0x60-0x6F)
    RunCreate = 0x60,
    RunUpdate = 0x61,
    RunEnd = 0x62,
    RunBegin = 0x63,

    // Reserved for Vector (M8): 0x70-0x7F
    // Reserved for future: 0x80-0xFF
}
```

### WAL Entry Format

Every WAL entry has this envelope:

```
+----------------+
| Length (u32)   |  Total bytes after this field
+----------------+
| Type (u8)      |  Entry type from registry
+----------------+
| Version (u8)   |  Format version for this entry type
+----------------+
| Payload        |  Type-specific data
+----------------+
| CRC32 (u32)    |  Checksum of Type + Version + Payload
+----------------+
```

---

## Storage Extension

Trait for adding new primitives to the storage system.

### PrimitiveStorageExt

```rust
pub trait PrimitiveStorageExt {
    /// WAL entry types this primitive uses
    fn wal_entry_types(&self) -> &'static [u8];

    /// Serialize primitive state for snapshot
    fn snapshot_serialize(&self) -> Result<Vec<u8>>;

    /// Deserialize primitive state from snapshot
    fn snapshot_deserialize(&mut self, data: &[u8]) -> Result<()>;

    /// Apply a WAL entry during recovery
    fn apply_wal_entry(&mut self, entry: &WalEntry) -> Result<()>;

    /// Primitive type ID (for snapshot sections)
    fn primitive_type_id(&self) -> u8;
}
```

**Example** (future Vector primitive):
```rust
impl PrimitiveStorageExt for VectorStore {
    fn wal_entry_types(&self) -> &'static [u8] {
        &[0x70, 0x71, 0x72]  // VectorInsert, VectorDelete, VectorUpdate
    }

    fn primitive_type_id(&self) -> u8 {
        7  // After existing 6 primitives
    }

    // ... other methods
}
```

---

## Transactions

### Cross-Primitive Transactions

Use extension traits for atomic operations across multiple primitives.

```rust
use strata::primitives::{KVStoreExt, EventLogExt, StateCellExt, TraceStoreExt};

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

### Recovery Performance (M7)

| Operation | Target |
|-----------|--------|
| Snapshot write (100MB state) | < 5 seconds |
| Snapshot load (100MB state) | < 3 seconds |
| WAL replay (10K entries) | < 1 second |
| Full recovery (100MB snap + 10K WAL) | < 5 seconds |
| Index rebuild (10K docs) | < 2 seconds |
| Replay run (1K events) | < 100 ms |
| Diff runs (1K keys each) | < 200 ms |

---

## Version History

### v0.7.0 (M7 Durability, Snapshots & Replay)

**Snapshot System**:
- Periodic snapshots for bounded recovery time
- SnapshotConfig for automatic triggers (size, time, shutdown)
- Multiple snapshot retention with automatic cleanup
- WAL truncation after successful snapshot

**Crash Recovery**:
- RecoveryOptions for controlling recovery behavior
- RecoveryResult with detailed recovery statistics
- Fallback to older snapshots on corruption
- CRC32 validation on all WAL entries

**Deterministic Replay**:
- `replay_run()` returns read-only view of run state
- `diff_runs()` compares two runs at key level
- Side-effect free replay (does not mutate canonical store)
- O(run size) replay performance

**Run Lifecycle**:
- `begin_run()` / `end_run()` for explicit lifecycle
- Orphaned run detection after crash recovery
- RunStatus tracking (Active, Completed, Orphaned)

**Storage Stabilization**:
- PrimitiveStorageExt trait for adding new primitives
- WAL entry type registry (0x00-0xFF)
- Frozen API surface for future extension
- Clear extension points for M8 Vector primitive

### v0.5.0 (M5 JSON + M6 Retrieval)

**M5 Features**:
- JsonStore primitive for JSON documents
- Path-level mutations with JSONPath
- Array operations (push, pop)
- Region-based conflict detection

**M6 Features**:
- SearchRequest/SearchResponse types
- HybridSearch with RRF fusion (k=60)
- BM25Lite keyword scoring
- InvertedIndex (opt-in)
- Budget semantics for search operations
- DocRef for cross-primitive document references

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
