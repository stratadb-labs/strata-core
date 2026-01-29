# Strata API Structure

## Overview

This document defines the layered API structure for Strata, the data substrate for AI agents. It establishes the mental model, concurrency semantics, and API design patterns.

## Mental Model: Git Analogy

| Git Concept | Strata Equivalent | Description |
|-------------|-------------------|-------------|
| Repository | `Database` | Shared storage, thread-safe, opened once |
| Working Directory | `Strata` | Per-agent instance with current run context |
| Branch | `Run` | Isolated namespace for data |
| HEAD | `current_run` | The run that operations target |
| `main` branch | Default run | Auto-created, used when no run specified |

```
┌─────────────────────────────────────────────────────────────────┐
│  Database (shared, Arc-wrapped, thread-safe)                    │
│                                                                 │
│  ┌─────────────────┐  ┌─────────────────┐  ┌─────────────────┐ │
│  │ Strata (Agent1) │  │ Strata (Agent2) │  │ Strata (Agent3) │ │
│  │ current: run-1  │  │ current: run-2  │  │ current: run-1  │ │
│  └─────────────────┘  └─────────────────┘  └─────────────────┘ │
│           │                   │                   │             │
│           ▼                   ▼                   ▼             │
│  ┌─────────────────────────────────────────────────────────────┐│
│  │                    Runs (isolated namespaces)               ││
│  │  ┌─────────┐  ┌─────────┐  ┌─────────┐  ┌─────────┐        ││
│  │  │ default │  │  run-1  │  │  run-2  │  │  run-3  │  ...   ││
│  │  │  (main) │  │         │  │         │  │         │        ││
│  │  └─────────┘  └─────────┘  └─────────┘  └─────────┘        ││
│  └─────────────────────────────────────────────────────────────┘│
└─────────────────────────────────────────────────────────────────┘
```

## Architecture Layers

### Layer 1: Database (strata-engine)

The `Database` is the shared storage layer. It is:
- **Opened once** per path (singleton pattern with global registry)
- **Thread-safe** (uses `DashMap`, atomics, internal locking)
- **Shared via `Arc<Database>`** across all agents

```rust
// Database is opened once and shared
let database = Database::open("/path/to/data")?;  // Returns Arc<Database>

// Multiple opens of same path return same instance
let db1 = Database::open("/path/to/data")?;  // Same Arc
let db2 = Database::open("/path/to/data")?;  // Same Arc
assert!(Arc::ptr_eq(&db1, &db2));
```

**Implementation: Global Registry**

```rust
use std::sync::{Mutex, Weak};
use std::collections::HashMap;
use std::path::PathBuf;
use once_cell::sync::Lazy;

static OPEN_DATABASES: Lazy<Mutex<HashMap<PathBuf, Weak<Database>>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

impl Database {
    pub fn open(path: impl AsRef<Path>) -> Result<Arc<Database>> {
        let path = path.as_ref().canonicalize()?;
        let mut registry = OPEN_DATABASES.lock().unwrap();

        // Return existing instance if available
        if let Some(weak) = registry.get(&path) {
            if let Some(db) = weak.upgrade() {
                return Ok(db);
            }
        }

        // Create new instance
        let db = Arc::new(Self::open_internal(&path)?);
        registry.insert(path, Arc::downgrade(&db));
        Ok(db)
    }
}
```

### Layer 2: Strata (strata-executor)

`Strata` is the user-facing API. Each agent gets their own instance, like a git working directory.

```rust
pub struct Strata {
    executor: Executor,           // Wraps Arc<Database>
    current_run: Option<RunId>,   // Per-instance context (like HEAD)
}
```

Key properties:
- **Per-agent instance**: Each agent creates their own `Strata`
- **Independent run context**: `set_run()` only affects that instance
- **Not shared across threads**: Use separate instances per thread/agent

### Layer 3: Primitives

Primitives (KV, State, Event, JSON, Vector) are accessed through `Strata`:

```rust
// All primitives accessed through db, operate on current run
db.kv_put("key", value)?;
db.state_set("cell", value)?;
db.event_append("stream", payload)?;
```

## API Design

### Run Management Architecture: Layered API

Run management uses a **layered API** design that balances simplicity with power:

```
┌─────────────────────────────────────────────────────────────────┐
│  User-Facing API (Strata)                                       │
│                                                                 │
│  Simple Run Context (90% of users)    Power API (10% of users) │
│  ┌─────────────────────────────┐     ┌─────────────────────────┐│
│  │ db.create_run("name")       │     │ db.runs().fork(...)     ││
│  │ db.set_run("name")          │     │ db.runs().diff(...)     ││
│  │ db.current_run()            │     │ db.runs().add_tags(...) ││
│  │ db.list_runs()              │     │ db.runs().pause(...)    ││
│  │ db.delete_run("name")       │     │ db.runs().query_by_tag()││
│  │ db.fork_run("dest")         │     │                         ││
│  └─────────────────────────────┘     └─────────────────────────┘│
│                    │                            │                │
│                    └──────────┬─────────────────┘                │
│                               ▼                                  │
│  ┌─────────────────────────────────────────────────────────────┐│
│  │  RunIndex Primitive (internal plumbing)                     ││
│  │  - Not directly exposed to users                            ││
│  │  - Handles storage, indices, lifecycle                      ││
│  └─────────────────────────────────────────────────────────────┘│
└─────────────────────────────────────────────────────────────────┘
```

**Design Principles:**

1. **RunIndex is internal plumbing** - Users don't interact with it directly
2. **Simple API on `db`** - Context management for everyday use
3. **Power API via `db.runs()`** - Advanced operations for power users
4. **Progressive disclosure** - Start simple, access power when needed

### Simple Run Context API (MVP)

These methods live directly on `Strata` for everyday run management:

| Git Command | Strata API | Description |
|-------------|------------|-------------|
| `git branch <name>` | `db.create_run("name")` | Create a new blank run (stay on current) |
| `git switch <name>` | `db.set_run("name")` | Switch to existing run |
| (implicit) | `db.current_run()` | Get current run name |
| `git branch` | `db.list_runs()` | List all runs |
| `git branch -d <name>` | `db.delete_run("name")` | Delete a run |
| `git checkout -b <name>` | `db.create_run()` + `db.set_run()` | Create and switch |
| `git worktree add` (future) | `db.fork_run("dest")` | Copy current run to new run |

```rust
// Simple usage - most users only need this
let mut db = Strata::open("/data")?;

db.create_run("agent-session")?;     // Create a new blank run
db.set_run("agent-session")?;        // Switch to it
db.kv_put("key", value)?;            // Operates on current run

db.set_run("default")?;              // Switch to existing run
let runs = db.list_runs()?;          // See all available runs
db.delete_run("old-run")?;           // Clean up
```

### Power Run API (via db.runs())

Advanced operations available via `db.runs()` accessor:

```rust
// Power user API - advanced run management
db.runs().list()?;                             // List all runs
db.runs().exists("run-name")?;                 // Check if run exists
db.runs().create("new-run")?;                  // Create a new run
db.runs().delete("old-run")?;                  // Delete a run
db.runs().fork("destination")?;                // Copy current run to new run (future)
db.runs().diff("run-a", "run-b")?;             // Compare two runs (future)
```

**Why this split?**

- **Context methods (`create_run`, `set_run`, `current_run`)** answer: "Where am I working?"
- **Management methods (`db.runs()`)** answer: "How do I manage runs themselves?"

This mirrors git's porcelain (user-facing) vs plumbing (internal) distinction.

### MVP Scope

For MVP, the following run context API is implemented:

| Method | Status | Description |
|--------|--------|-------------|
| `create_run(&self, name)` | MVP | Create a new blank run |
| `set_run(&mut self, name)` | MVP | Switch to existing run (errors if not found) |
| `current_run(&self)` | MVP | Get current run name |
| `list_runs(&self)` | MVP | List all runs |
| `delete_run(&self, name)` | MVP | Delete a run |
| `fork_run(&self, dest)` | Stub | Fork current run (returns NotImplemented) |
| `db.runs().list/create/delete/exists` | MVP | Power API accessor |
| `db.runs().fork/diff` | Stub | Returns NotImplemented |

### Core API

```rust
impl Strata {
    // === Construction ===

    /// Open database and create Strata in one call.
    pub fn open(path: impl AsRef<Path>) -> Result<Self>;

    /// Open a temporary in-memory database (for testing).
    pub fn open_temp() -> Result<Self>;

    /// Create from an existing database handle.
    pub fn from_database(db: Arc<Database>) -> Result<Self>;

    // === Run Context (MVP) ===

    /// Create a new blank run (stays on current run).
    pub fn create_run(&self, name: &str) -> Result<()>;

    /// Switch to an existing run. Returns error if run doesn't exist.
    pub fn set_run(&mut self, name: &str) -> Result<()>;

    /// Get the current run name. Returns "default" initially.
    pub fn current_run(&self) -> &str;

    /// List all runs.
    pub fn list_runs(&self) -> Result<Vec<String>>;

    /// Delete a run. Cannot delete the current run or default run.
    pub fn delete_run(&self, name: &str) -> Result<()>;

    /// Fork current run to a new destination (NOT YET IMPLEMENTED).
    pub fn fork_run(&self, destination: &str) -> Result<()>;

    // === Power Run API ===

    /// Access advanced run management operations.
    /// Returns a handle for list, create, delete, exists, fork, diff.
    pub fn runs(&self) -> Runs;

    // === Primitives (operate on current run) ===

    // KV
    pub fn kv_get(&self, key: &str) -> Result<Option<Value>>;
    pub fn kv_put(&self, key: &str, value: Value) -> Result<u64>;
    pub fn kv_delete(&self, key: &str) -> Result<bool>;
    pub fn kv_list(&self, prefix: Option<&str>) -> Result<Vec<String>>;

    // State
    pub fn state_read(&self, name: &str) -> Result<Option<VersionedValue>>;
    pub fn state_set(&self, name: &str, value: Value) -> Result<u64>;
    pub fn state_cas(&self, name: &str, expected: u64, value: Value) -> Result<u64>;

    // Event
    pub fn event_append(&self, stream: &str, payload: Value) -> Result<u64>;
    pub fn event_read(&self, seq: u64) -> Result<Option<Event>>;
    pub fn event_range(&self, stream: Option<&str>, start: Option<u64>, end: Option<u64>, limit: Option<u64>) -> Result<Vec<Event>>;

    // JSON
    pub fn json_get(&self, doc_id: &str, path: &str) -> Result<Option<JsonValue>>;
    pub fn json_set(&self, doc_id: &str, path: &str, value: JsonValue) -> Result<u64>;
    pub fn json_delete(&self, doc_id: &str, path: &str) -> Result<bool>;

    // Vector
    pub fn vector_create_collection(&self, name: &str, dimension: u64, metric: DistanceMetric) -> Result<()>;
    pub fn vector_upsert(&self, collection: &str, key: &str, embedding: Vec<f32>, metadata: Option<Value>) -> Result<()>;
    pub fn vector_search(&self, collection: &str, query: Vec<f32>, k: u64) -> Result<Vec<VectorMatch>>;
    pub fn vector_delete(&self, collection: &str, key: &str) -> Result<bool>;
}
```

## Concurrency Model

### Multiple Agents, Same Database

```rust
let database = Database::open("/data")?;

// Agent 1 - operates on agent-1-session run
let mut agent1_db = Strata::from_database(database.clone())?;
agent1_db.create_run("agent-1-session")?;
agent1_db.set_run("agent-1-session")?;

// Agent 2 - operates on agent-2-session run
let mut agent2_db = Strata::from_database(database.clone())?;
agent2_db.create_run("agent-2-session")?;
agent2_db.set_run("agent-2-session")?;

// Concurrent operations are safe - different runs are isolated
std::thread::spawn(move || {
    agent1_db.kv_put("status", "working")?;
});
std::thread::spawn(move || {
    agent2_db.kv_put("status", "also working")?;
});
```

### Multiple Agents, Same Run

Multiple agents can operate on the same run. Transaction isolation ensures correctness:

```rust
let database = Database::open("/data")?;

let mut agent1_db = Strata::new(database.clone());
agent1_db.set_run("shared-run")?;

let mut agent2_db = Strata::new(database.clone());
agent2_db.set_run("shared-run")?;  // Same run as agent1

// Both agents writing to same run - transactions prevent conflicts
agent1_db.kv_put("counter", 1.into())?;
agent2_db.kv_put("counter", 2.into())?;  // Last write wins (or use CAS for coordination)
```

### Thread Safety Rules

1. **`Database`**: Thread-safe, share via `Arc<Database>`
2. **`Strata`**: NOT thread-safe, create one per thread/agent
3. **Runs**: Isolated namespaces, safe for concurrent access from different `Strata` instances
4. **Same path, multiple opens**: Returns same `Arc<Database>` (safe)

## Implementation Status

### 1. Database: Global Registry ✅

File: `crates/engine/src/database.rs`

- ✅ Added static `OPEN_DATABASES` registry
- ✅ `Database::open()` returns `Arc<Database>`
- ✅ Same path returns same instance (singleton)
- ✅ Fixed TOCTOU race condition
- ✅ Singleton tests added

### 2. Strata: Run Context ✅

File: `crates/executor/src/api/mod.rs`

- ✅ Added `current_run: RunId` field (defaults to "default")
- ✅ Added run context methods:
  - `create_run(&self, name: &str)` - create a new blank run
  - `set_run(&mut self, name: &str)` - switch to existing run
  - `current_run(&self)` - get current run name
  - `list_runs(&self)` - list all runs
  - `delete_run(&self, name: &str)` - delete a run
  - `fork_run(&self, dest: &str)` - stub (NotImplemented)
- ✅ Added `db.runs()` power API accessor
- ✅ All primitive methods use `self.current_run.clone()`

### 3. Commands: No Change Needed

The command structure already has `run: Option<RunId>`. Strata fills in `run: self.current_run.clone()` when building commands.

### 4. Hide RunIndex ✅

- ✅ RunIndex is internal plumbing
- ✅ Users access run management through Strata methods
- ✅ `db.runs()` accessor for power API

```rust
// User sees this:
db.create_run("my-session")?;
db.set_run("my-session")?;
db.kv_put("key", "value")?;

// NOT this:
let run_index = RunIndex::new(database);
run_index.create_run(...)?;
```

## Migration Path

### Phase 1: KV MVP API ✅
- Simplified KV from ~25 methods to 4 MVP methods
- PR #785 merged

### Phase 2: Database Thread Safety ✅
- Implemented global registry for `Database::open()`
- Returns `Arc<Database>`, same path returns same instance
- Fixed TOCTOU race condition
- PR #786 merged

### Phase 3: Run Context ✅
- ✅ Added `current_run` field to Strata
- ✅ Added `create_run()`, `set_run()`, `current_run()`, `list_runs()`, `delete_run()`, `fork_run()` (stub)
- ✅ Modified all primitive methods to use `current_run`
- ✅ Hide RunIndex as internal plumbing
- ✅ Added `db.runs()` power API accessor

### Phase 4: Simplify Other Primitive APIs
- Apply MVP pattern to Event, State, JSON, Vector
- Make primitive constructors `pub(crate)`

### Phase 5: Power Run API (Future)
- Implement `fork_run()` (copy data to new run)
- Implement `diff_runs()` (compare two runs)
- Expose tags, lifecycle, genealogy via power API

## Examples

### Simple Usage (Default Run)

```rust
let db = Strata::open("/path/to/data")?;

// All operations go to default run
db.kv_put("config", json!({"debug": true}))?;
let config = db.kv_get("config")?;
```

### Multi-Agent Usage

```rust
let database = Database::open("/shared/data")?;

// Agent 1 - customer support context
let mut db1 = Strata::from_database(database.clone())?;
db1.create_run("customer-support-session")?;
db1.set_run("customer-support-session")?;
db1.kv_put("context", customer_data)?;
db1.event_append("actions", json!({"action": "lookup"}))?;

// Agent 2 - research context (different run, isolated)
let mut db2 = Strata::from_database(database.clone())?;
db2.create_run("research-session")?;
db2.set_run("research-session")?;
db2.kv_put("context", research_query)?;

// Agent 3 - same run as Agent 1 (shared context)
let mut db3 = Strata::from_database(database.clone())?;
db3.set_run("customer-support-session")?;  // Join existing run
let context = db3.kv_get("context")?;       // Sees Agent 1's data
```

### Session/Transaction Usage

```rust
let mut db = Strata::open("/data")?;
db.create_run("my-run")?;
db.set_run("my-run")?;

// Multi-operation transaction
db.transaction(|txn| {
    let counter = txn.kv_get("counter")?.unwrap_or(0);
    txn.kv_put("counter", counter + 1)?;
    txn.event_append("increments", json!({"new_value": counter + 1}))?;
    Ok(())
})?;
```

## Future Features (Post-MVP)

### fork_run: Copy Data Between Runs

```rust
// Create "experiment-v2" with all data copied from the current run
db.set_run("experiment-v1")?;
db.runs().fork("experiment-v2")?;  // Fork current run to experiment-v2
// OR use the convenience method:
db.fork_run("experiment-v2")?;
```

**Behavior:**
- Creates target run with `parent_id` pointing to current run (for genealogy)
- Copies all data: KV pairs, events, state cells, JSON docs, vector collections
- New run starts in `Active` status with fresh timestamps
- Stays on the current run (use `set_run()` to switch after)

**Use cases:**
- A/B testing different approaches
- Checkpointing before risky operations
- Agent handoff with full context

### diff_runs: Compare Two Runs

```rust
let diff = db.runs().diff("baseline", "experiment")?;
println!("Modified keys: {:?}", diff.kv.modified);
println!("New events: {:?}", diff.events.added);
```

**Returns:**
```rust
pub struct RunDiff {
    pub kv: KvDiff,        // Added, removed, modified keys
    pub events: EventDiff,  // Event count differences per stream
    pub state: StateDiff,   // Changed state cells
    pub json: JsonDiff,     // Changed documents
    pub vectors: VectorDiff, // Changed embeddings
}
```

**Use cases:**
- Compare experiment results
- Audit what an agent changed
- Selective merge between runs

### Run Genealogy

```rust
// Get full ancestry
let lineage = db.runs().get_lineage("run-id")?;
// Returns: [run-id, parent, grandparent, ...]

// Get all descendants
let children = db.runs().get_children("run-id")?;
```

## Open Questions (Resolved)

1. **Default run name**: Should it be "default", "main", or something else?
   - ✅ Decision: Uses "default"

2. **Auto-create on set_run?**: Should `set_run("foo")` create the run if it doesn't exist?
   - ✅ Decision: No. Use `create_run()` + `set_run()` for explicit semantics.
   - `set_run()` returns error if run doesn't exist (explicit is better).

3. **Run persistence**: Should "current run" persist across restarts?
   - ✅ Decision: No. Always start at default run.
   - Rationale: Strata instances are per-agent, not persisted entities.

4. **Strata cloning**: Should `Strata` be `Clone`?
   - ✅ Decision: No. Each agent should create their own instance.
   - Rationale: Cloning would create confusion about shared vs independent context.
