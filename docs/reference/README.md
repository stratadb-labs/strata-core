# in-mem Reference Documentation

**in-mem** - a fast, durable, embedded database for AI agent workloads.

**Current Version**: 0.3.0 (M3 Primitives + M4 Performance)

## Quick Links

- [Getting Started](getting-started.md) - Installation and quick start
- [API Reference](api-reference.md) - Complete API documentation
- [Architecture](architecture.md) - How in-mem works internally
- [Milestones](../milestones/MILESTONES.md) - Project roadmap

## Features

- **Five Primitives**: KVStore, EventLog, StateCell, TraceStore, RunIndex
- **Three Durability Modes**: InMemory (<3µs), Buffered (<30µs), Strict (~2ms)
- **OCC Transactions**: Optimistic concurrency with snapshot isolation
- **Run-Scoped Operations**: Every operation tagged with RunId for replay

## Current Status

| Milestone | Status |
|-----------|--------|
| M1 Foundation | ✅ |
| M2 Transactions | ✅ |
| M3 Primitives | ✅ |
| M4 Performance | ✅ |
| M5 JSON | Next |

## Quick Start

```rust
use in_mem::{Database, DurabilityMode, primitives::KVStore, Value};
use std::sync::Arc;

let db = Arc::new(Database::open_with_mode(
    "./my-agent-db",
    DurabilityMode::Buffered { flush_interval_ms: 100, max_pending_writes: 1000 }
)?);

let kv = KVStore::new(db.clone());
let run_id = db.begin_run();

kv.put(&run_id, "key", Value::String("value".into()))?;
let value = kv.get(&run_id, "key")?;

db.end_run(run_id)?;
```

## Performance

| Mode | Latency | Throughput |
|------|---------|------------|
| InMemory | <3µs | 250K+ ops/sec |
| Buffered | <30µs | 50K+ ops/sec |
| Strict | ~2ms | ~500 ops/sec |

---

**Version**: 0.3.0
**Last Updated**: 2026-01-16
