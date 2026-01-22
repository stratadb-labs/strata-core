# Getting Started with Strata

**Strata** is a fast, durable, embedded database designed for AI agent workloads.

**Current Version**: 0.7.0 (M7 Durability, Snapshots & Replay)

## Installation

```toml
[dependencies]
strata = "0.7"
```

## Quick Start

```rust
use strata::{Database, DurabilityMode, primitives::KVStore, Value};
use std::sync::Arc;

// Open database
let db = Arc::new(Database::open_with_mode(
    "./my-agent-db",
    DurabilityMode::Buffered {
        flush_interval_ms: 100,
        max_pending_writes: 1000,
    }
)?);

// Create KVStore
let kv = KVStore::new(db.clone());

// Begin a run
let run_id = db.begin_run();

// Store and retrieve data
kv.put(&run_id, "key", Value::String("value".into()))?;
let value = kv.get(&run_id, "key")?;

// End run
db.end_run(run_id)?;
```

## Durability Modes

| Mode | Latency | Data Loss | Use Case |
|------|---------|-----------|----------|
| InMemory | <3µs | All | Tests, caches |
| Buffered | <30µs | ~100ms | Production |
| Strict | ~2ms | None | Audit logs |

## Using the Primitives

### KVStore

```rust
let kv = KVStore::new(db.clone());
kv.put(&run_id, "key", Value::I64(42))?;
let value = kv.get(&run_id, "key")?;

// Batch reads
let values = kv.get_many(&run_id, &["key1", "key2"])?;
```

### EventLog

```rust
let events = EventLog::new(db.clone());
let (seq, hash) = events.append(&run_id, "user_action", Value::String("login".into()))?;
let event = events.read(&run_id, seq)?;
```

### StateCell

```rust
let state = StateCell::new(db.clone());
state.init(&run_id, "counter", Value::I64(0))?;

// Atomic increment with retry
let (old, version) = state.transition(&run_id, "counter", |s| {
    let n = match &s.value { Value::I64(n) => *n, _ => 0 };
    Ok((Value::I64(n + 1), n))
})?;
```

### TraceStore

```rust
let traces = TraceStore::new(db.clone());
let trace_id = traces.record(
    &run_id,
    TraceType::ToolCall {
        tool_name: "search".into(),
        arguments: Value::String("query".into()),
        result: None,
        duration_ms: Some(150),
    },
    vec!["search".into()],
    Value::Null
)?;
```

### RunIndex

```rust
let runs = RunIndex::new(db.clone());
runs.create_run(&run_id.to_string())?;
runs.complete_run(&run_id.to_string())?;
let active = runs.query_by_status(RunStatus::Active)?;
```

### JsonStore (M5)

```rust
use strata::primitives::JsonStore;
use serde_json::json;

let json = JsonStore::new(db.clone());

// Store JSON document
json.put(&run_id, "config", json!({
    "model": "gpt-4",
    "temperature": 0.7,
    "max_tokens": 1000
}))?;

// Path-level mutations
json.set_path(&run_id, "config", "$.temperature", json!(0.9))?;
let temp = json.get_path(&run_id, "config", "$.temperature")?;

// Array operations
json.array_push(&run_id, "config", "$.history", json!({"role": "user"}))?;
```

### Hybrid Search (M6)

```rust
use strata::search::{SearchRequest, HybridSearch};

let search = HybridSearch::new(db.clone());

// Search across all primitives
let request = SearchRequest::new("find error logs")
    .with_limit(10)
    .with_budget_ms(50);

let response = search.search(&run_id, request)?;

for result in response.results {
    println!("Found: {:?} (score: {})", result.doc_ref, result.score);
}
```

### Snapshots (M7)

```rust
use strata::SnapshotConfig;

// Configure automatic snapshots
db.configure_snapshots(SnapshotConfig {
    wal_size_threshold: 50 * 1024 * 1024,  // 50 MB
    time_interval_minutes: 15,
    retention_count: 3,
    snapshot_on_shutdown: true,
});

// Manual snapshot
let info = db.snapshot()?;
println!("Snapshot created at {:?}", info.path);
```

### Recovery (M7)

```rust
use strata::{Database, RecoveryOptions};

// Open with custom recovery options
let db = Database::open_with_options(
    "./data",
    RecoveryOptions {
        max_corrupt_entries: 5,
        verify_all_checksums: true,
        rebuild_indexes: true,
    }
)?;

// Check recovery result
if let Some(result) = db.last_recovery_result() {
    println!("Recovered {} transactions", result.transactions_recovered);
    if result.corrupt_entries_skipped > 0 {
        eprintln!("Warning: {} corrupt entries skipped", result.corrupt_entries_skipped);
    }
}
```

### Run Lifecycle (M7)

```rust
// Explicit run lifecycle
let run_id = RunId::new();
db.begin_run(run_id)?;

// Do work within the run
db.kv.put(&run_id, "step", Value::String("started".into()))?;
db.event.append(&run_id, "task_begun", Value::Null)?;

// End run normally
db.end_run(run_id)?;

// Check for orphaned runs after restart
for orphan in db.orphaned_runs()? {
    println!("Orphaned run: {:?}", orphan);
}
```

### Replay (M7)

```rust
// Replay a completed run (read-only, side-effect free)
let view = db.replay_run(run_id)?;
println!("Run had {} KV entries", view.keys().count());
println!("Run had {} events", view.events().len());

// Compare two runs
let diff = db.diff_runs(run_a, run_b)?;
for entry in &diff.added {
    println!("Added: {:?}", entry.key);
}
for entry in &diff.modified {
    println!("Modified: {:?} ({:?} -> {:?})", entry.key, entry.value_a, entry.value_b);
}
```

## Cross-Primitive Transactions

```rust
use strata::primitives::{KVStoreExt, EventLogExt, StateCellExt};

db.transaction(&run_id, |txn| {
    txn.kv_put("key", Value::String("value".into()))?;
    txn.event_append("event", Value::Null)?;
    txn.state_set("counter", Value::I64(1))?;
    Ok(())
})?;
```

## Performance Tips

1. **Use fast path operations** for reads: `kv.get()`, `kv.exists()`, `kv.get_many()`
2. **Choose appropriate durability**: InMemory for tests, Buffered for production
3. **Use `transition()` for counters**: Handles retries automatically
4. **Set search budgets**: Use `with_budget_ms()` to control search latency
5. **Enable indexing selectively**: Inverted index is opt-in for memory efficiency
6. **Configure snapshot triggers**: Tune `wal_size_threshold` based on recovery time requirements
7. **Use explicit run lifecycle**: `begin_run()`/`end_run()` enables orphan detection
8. **Replay is cheap**: O(run size), not O(database size)

## See Also

- [API Reference](api-reference.md)
- [Architecture](architecture.md)
- [Milestones](../milestones/MILESTONES.md)
