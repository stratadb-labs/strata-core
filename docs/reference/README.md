# in-mem Reference Documentation

**in-mem** - a fast, durable, embedded database for AI agent workloads.

**Current Version**: 0.7.0 (M7 Durability, Snapshots & Replay)

## Quick Links

- [Getting Started](getting-started.md) - Installation and quick start
- [API Reference](api-reference.md) - Complete API documentation
- [Architecture](architecture.md) - How in-mem works internally
- [Milestones](../milestones/MILESTONES.md) - Project roadmap

## Features

- **Six Primitives**: KVStore, EventLog, StateCell, TraceStore, RunIndex, JsonStore
- **Hybrid Search**: BM25 + semantic search with RRF fusion
- **Three Durability Modes**: InMemory (<3µs), Buffered (<30µs), Strict (~2ms)
- **OCC Transactions**: Optimistic concurrency with snapshot isolation
- **Run-Scoped Operations**: Every operation tagged with RunId for replay
- **Periodic Snapshots**: Bounded recovery time with automatic WAL truncation
- **Crash Recovery**: Deterministic, idempotent, prefix-consistent recovery
- **Deterministic Replay**: Side-effect free reconstruction of agent run state

## Current Status

| Milestone | Status |
|-----------|--------|
| M1 Foundation | ✅ |
| M2 Transactions | ✅ |
| M3 Primitives | ✅ |
| M4 Performance | ✅ |
| M5 JSON | ✅ |
| M6 Retrieval | ✅ |
| M7 Durability | ✅ |
| M8 Vector | Next |

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

**Version**: 0.7.0
**Last Updated**: 2026-01-17
