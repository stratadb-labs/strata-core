# Getting Started with in-mem

**in-mem** is a fast, durable, embedded database designed for AI agent workloads.

**Current Version**: 0.3.0 (M3 Primitives + M4 Performance)

## Installation

```toml
[dependencies]
in-mem = "0.3"
```

## Quick Start

```rust
use in_mem::{Database, DurabilityMode, primitives::KVStore, Value};
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

## Cross-Primitive Transactions

```rust
use in_mem::primitives::{KVStoreExt, EventLogExt, StateCellExt};

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

## See Also

- [API Reference](api-reference.md)
- [Architecture](architecture.md)
- [Milestones](../milestones/MILESTONES.md)
