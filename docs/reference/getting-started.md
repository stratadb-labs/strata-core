# Getting Started with in-mem

**in-mem** is a fast, durable, embedded database designed for AI agent workloads. This guide will help you get started quickly.

**Current Version**: 0.3.0 (M3 Primitives + M4 Performance)

## Installation

Add `in-mem` to your `Cargo.toml`:

```toml
[dependencies]
in-mem = "0.3"
```

**Note**: Currently in development. Not yet published to crates.io.

## Quick Start

### Opening a Database

```rust
use in_mem::{Database, DurabilityMode};
use std::sync::Arc;

// Open with default settings (Strict durability)
let db = Arc::new(Database::open("./my-agent-db")?);

// Or choose a durability mode
let db = Arc::new(Database::open_with_mode(
    "./my-agent-db",
    DurabilityMode::Buffered {
        flush_interval_ms: 100,
        max_pending_writes: 1000,
    }
)?);
```

### Basic KV Operations

```rust
use in_mem::{Database, primitives::KVStore, Value};
use std::sync::Arc;

let db = Arc::new(Database::open("./data")?);
let kv = KVStore::new(db.clone());

// Begin a run (all operations are run-scoped)
let run_id = db.begin_run();

// Put values
kv.put(&run_id, "user:123:name", Value::String("Alice".into()))?;
kv.put(&run_id, "user:123:age", Value::I64(30))?;

// Get values
let name = kv.get(&run_id, "user:123:name")?;
match name {
    Some(Value::String(s)) => println!("Name: {}", s),
    _ => println!("Not found or wrong type"),
}

// Delete
kv.delete(&run_id, "user:123:name")?;

// End the run
db.end_run(run_id)?;
```

## Key Concepts

### Runs

Every operation in **in-mem** is associated with a `RunId`. Runs represent agent execution sessions and enable:

- **Deterministic Replay**: Reconstruct exact agent state from any run
- **Debugging**: Trace what an agent did during a specific run
- **Isolation**: Separate data from different agent executions

```rust
// Create a new run
let run_id = db.begin_run();

// All operations use this run_id
kv.put(&run_id, "key", Value::String("value".into()))?;

// End the run when done
db.end_run(run_id)?;
```

### Durability Modes

Choose the right trade-off between performance and durability:

```rust
use in_mem::{Database, DurabilityMode};

// InMemory: Fastest, no persistence (for tests/caches)
let db = Database::open_with_mode("./data", DurabilityMode::InMemory)?;

// Buffered: Production default (balanced)
let db = Database::open_with_mode(
    "./data",
    DurabilityMode::Buffered {
        flush_interval_ms: 100,   // fsync every 100ms
        max_pending_writes: 1000  // or after 1000 writes
    }
)?;

// Strict: Maximum durability (for critical data)
let db = Database::open_with_mode("./data", DurabilityMode::Strict)?;
```

| Mode | Latency | Data Loss Window | Use Case |
|------|---------|------------------|----------|
| InMemory | <3µs | All | Tests, caches |
| Buffered | <30µs | ~100ms | Production |
| Strict | ~2ms | None | Audit logs |

## Using the Primitives

### KVStore

Key-value storage with batch operations and transactions.

```rust
use in_mem::primitives::KVStore;

let kv = KVStore::new(db.clone());

// Single operations
kv.put(&run_id, "key", Value::I64(42))?;
let value = kv.get(&run_id, "key")?;
kv.delete(&run_id, "key")?;

// Batch reads (efficient, single snapshot)
let values = kv.get_many(&run_id, &["key1", "key2", "key3"])?;

// List keys by prefix
let user_keys = kv.list(&run_id, Some("user:"))?;

// Transactional updates
kv.transaction(&run_id, |txn| {
    let balance = txn.get("account:balance")?
        .and_then(|v| match v { Value::I64(n) => Some(n), _ => None })
        .unwrap_or(0);
    txn.put("account:balance", Value::I64(balance + 100))?;
    Ok(())
})?;
```

### EventLog

Append-only event log with hash chaining.

```rust
use in_mem::primitives::EventLog;

let events = EventLog::new(db.clone());

// Append events
let (seq, hash) = events.append(
    &run_id,
    "user_action",
    Value::Map(BTreeMap::from([
        ("action".into(), Value::String("login".into())),
        ("user_id".into(), Value::String("123".into())),
    ]))
)?;

// Read events
let event = events.read(&run_id, seq)?;
let all_events = events.read_range(&run_id, 0, 100)?;
let login_events = events.read_by_type(&run_id, "user_action")?;

// Verify chain integrity
let verification = events.verify_chain(&run_id)?;
assert!(verification.is_valid);
```

### StateCell

Named state cells with compare-and-swap (CAS) for safe concurrent updates.

```rust
use in_mem::primitives::StateCell;

let state = StateCell::new(db.clone());

// Initialize (only if not exists)
state.init(&run_id, "counter", Value::I64(0))?;

// Read current state
if let Some(s) = state.read(&run_id, "counter")? {
    println!("Counter: {:?} (version {})", s.value, s.version);
}

// Unconditional set
state.set(&run_id, "status", Value::String("active".into()))?;

// Compare-and-swap (atomic update)
let current = state.read(&run_id, "counter")?.unwrap();
state.cas(&run_id, "counter", current.version, Value::I64(1))?;

// Transition with automatic retry (recommended for counters)
let (old_value, new_version) = state.transition(&run_id, "counter", |s| {
    let n = match &s.value { Value::I64(n) => *n, _ => 0 };
    Ok((Value::I64(n + 1), n))  // Returns (new_value, user_result)
})?;
```

### TraceStore

Record agent reasoning traces for debugging.

```rust
use in_mem::primitives::{TraceStore, TraceType};

let traces = TraceStore::new(db.clone());

// Record a tool call
let trace_id = traces.record(
    &run_id,
    TraceType::ToolCall {
        tool_name: "search".into(),
        arguments: Value::String("weather query".into()),
        result: Some(Value::String("sunny".into())),
        duration_ms: Some(150),
    },
    vec!["search".into(), "weather".into()],
    Value::Null
)?;

// Record a decision
let decision_id = traces.record(
    &run_id,
    TraceType::Decision {
        question: "Which API to use?".into(),
        options: vec!["OpenWeather".into(), "WeatherAPI".into()],
        chosen: "OpenWeather".into(),
        reasoning: Some("Better free tier".into()),
    },
    vec!["api-choice".into()],
    Value::Null
)?;

// Record child traces (hierarchical)
let child_id = traces.record_child(
    &run_id,
    &decision_id,
    TraceType::Thought {
        content: "Comparing API pricing...".into(),
        confidence: Some(0.85),
    },
    vec![],
    Value::Null
)?;

// Query traces
let tool_calls = traces.query_by_type(&run_id, "ToolCall")?;
let search_traces = traces.query_by_tag(&run_id, "search")?;

// Build trace tree
if let Some(tree) = traces.get_tree(&run_id, &decision_id)? {
    // tree.children contains child traces
}
```

### RunIndex

First-class run management with status tracking.

```rust
use in_mem::primitives::{RunIndex, RunStatus};

let runs = RunIndex::new(db.clone());

// Create a run entry
let run = runs.create_run_with_options(
    &run_id.to_string(),
    None,  // parent_run
    vec!["batch-job".into(), "high-priority".into()],
    Value::Map(BTreeMap::from([
        ("source".into(), Value::String("api".into())),
    ]))
)?;

// Update status
runs.complete_run(&run_id.to_string())?;
// Or: runs.fail_run(&run_id.to_string(), "Error message")?;

// Query runs
let active = runs.query_by_status(RunStatus::Active)?;
let batch_jobs = runs.query_by_tag("batch-job")?;

// Archive (soft delete)
runs.archive_run(&run_id.to_string())?;

// Hard delete (cascading - removes all data!)
// runs.delete_run(&run_id.to_string())?;
```

## Cross-Primitive Transactions

Use extension traits for atomic operations across multiple primitives:

```rust
use in_mem::primitives::{KVStoreExt, EventLogExt, StateCellExt, TraceStoreExt};

db.transaction(&run_id, |txn| {
    // KV operations
    txn.kv_put("user:123:name", Value::String("Alice".into()))?;

    // Event operations
    let seq = txn.event_append("user_created", Value::String("123".into()))?;

    // State operations
    txn.state_set("user_count", Value::I64(1))?;

    // Trace operations
    txn.trace_record("operation", Value::Null)?;

    // All operations commit atomically
    Ok(())
})?;
```

## Common Patterns

### Time-to-Live (TTL)

Store temporary data that expires automatically:

```rust
use std::time::Duration;

kv.put_with_ttl(
    &run_id,
    "session:abc123",
    Value::String("session-data".into()),
    Duration::from_secs(3600)  // 1 hour
)?;
```

### Counter Pattern

Use StateCell's `transition` for safe concurrent increments:

```rust
// Initialize or increment atomically
let (old, version) = state.transition_or_init(
    &run_id,
    "page_views",
    Value::I64(0),  // Initial value
    |s| {
        let n = match &s.value { Value::I64(n) => *n, _ => 0 };
        Ok((Value::I64(n + 1), n))
    }
)?;
```

### Audit Trail Pattern

Use EventLog for append-only audit trails:

```rust
events.append(&run_id, "audit", Value::Map(BTreeMap::from([
    ("action".into(), Value::String("user_delete".into())),
    ("actor".into(), Value::String("admin".into())),
    ("target".into(), Value::String("user:456".into())),
    ("timestamp".into(), Value::I64(now_ms())),
])))?;

// Later: verify integrity
let verification = events.verify_chain(&run_id)?;
```

### Run Guard Pattern

Ensure runs are properly ended:

```rust
struct RunGuard {
    db: Arc<Database>,
    run_id: RunId,
}

impl Drop for RunGuard {
    fn drop(&mut self) {
        let _ = self.db.end_run(self.run_id.clone());
    }
}

fn do_work(db: Arc<Database>) -> Result<()> {
    let run_id = db.begin_run();
    let _guard = RunGuard { db: db.clone(), run_id: run_id.clone() };

    // Work here...
    // Run is automatically ended when guard is dropped

    Ok(())
}
```

## Performance Tips

### Use Fast Path Operations

For read-heavy workloads, use fast path methods that bypass transaction overhead:

```rust
// Fast (direct snapshot read, <10µs)
let value = kv.get(&run_id, "key")?;
let exists = kv.exists(&run_id, "key")?;

// Also fast (single snapshot for multiple reads)
let values = kv.get_many(&run_id, &["key1", "key2", "key3"])?;
```

### Choose the Right Durability Mode

```rust
// For tests: InMemory (250K+ ops/sec)
DurabilityMode::InMemory

// For production: Buffered (50K+ ops/sec)
DurabilityMode::Buffered { flush_interval_ms: 100, max_pending_writes: 1000 }

// For critical data only: Strict (~500 ops/sec)
DurabilityMode::Strict
```

### Batch Operations

Use batch reads instead of individual reads:

```rust
// Slower: Multiple round trips
for key in keys {
    let v = kv.get(&run_id, key)?;
}

// Faster: Single snapshot
let values = kv.get_many(&run_id, &keys)?;
```

## Troubleshooting

### Database won't open

```rust
// Check the error for details
match Database::open("./data") {
    Ok(db) => println!("Opened successfully"),
    Err(e) => eprintln!("Failed: {:?}", e),
}
```

Common causes:
- Permissions issue (check directory permissions)
- Corrupted WAL (check for recovery errors)
- Disk full

### Transaction conflicts

If using CAS or transactions, conflicts are normal under contention:

```rust
// Use transition() for automatic retry
state.transition(&run_id, "counter", |s| {
    // This closure may be called multiple times
    let n = match &s.value { Value::I64(n) => *n, _ => 0 };
    Ok((Value::I64(n + 1), n))
})?;
```

### High memory usage

- Check for large values being stored
- Verify runs are being ended (`db.end_run()`)
- Consider using TTLs for temporary data

### Slow writes

- Check durability mode (use Buffered or InMemory for better performance)
- Batch writes in transactions when possible
- Monitor disk I/O

## Next Steps

- [API Reference](api-reference.md) - Complete API documentation
- [Architecture](architecture.md) - How in-mem works internally
- [Milestones](../milestones/MILESTONES.md) - Project roadmap

## Support

- **GitHub Issues**: [anibjoshi/in-mem/issues](https://github.com/anibjoshi/in-mem/issues)

---

**Current Version**: 0.3.0 (M3 Primitives + M4 Performance)
**Status**: Production-ready embedded database
