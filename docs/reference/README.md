# in-mem Reference Documentation

Complete reference documentation for **in-mem** - a fast, durable, embedded database for AI agent workloads.

**Current Version**: 0.3.0 (M3 Primitives + M4 Performance)

## Quick Links

### For Users

- **[Getting Started](getting-started.md)** - Installation, quick start, common patterns
- **[API Reference](api-reference.md)** - Complete API documentation
- **[Architecture Overview](architecture.md)** - How in-mem works internally

### For Developers

- **[M4 Architecture](../architecture/M4_ARCHITECTURE.md)** - Performance architecture
- **[Development Workflow](../development/DEVELOPMENT_WORKFLOW.md)** - Git workflow
- **[TDD Methodology](../development/TDD_METHODOLOGY.md)** - Testing approach

### Project Information

- **[Milestones](../milestones/MILESTONES.md)** - Roadmap M1-M7
- **[GitHub Repository](https://github.com/anibjoshi/in-mem)** - Source code

## What is in-mem?

**in-mem** is an embedded database designed specifically for AI agent workloads. It provides:

- **Run-Scoped Operations**: Every operation tagged with a RunId for deterministic replay
- **Five Primitives**: KVStore, EventLog, StateCell, TraceStore, RunIndex
- **Three Durability Modes**: InMemory (<3µs), Buffered (<30µs), Strict (~2ms)
- **OCC Transactions**: Optimistic concurrency with snapshot isolation
- **Embedded Library**: Zero-copy in-process API

## Current Status

### Completed Milestones

| Milestone | Status | Description |
|-----------|--------|-------------|
| M1 Foundation | ✅ | Basic storage, WAL, recovery |
| M2 Transactions | ✅ | OCC with snapshot isolation |
| M3 Primitives | ✅ | 5 primitives (KV, Events, State, Trace, Run) |
| M4 Performance | ✅ | Durability modes, fast paths, 250K ops/sec |

### Next Up

| Milestone | Description |
|-----------|-------------|
| M5 JSON | Native JSON with path-level atomicity |
| M6 Durability | Snapshots, WAL truncation |
| M7 Replay | Deterministic replay, run diffing |

## Quick Start

```rust
use in_mem::{Database, DurabilityMode, primitives::KVStore, Value};
use std::sync::Arc;

// Open database with Buffered durability
let db = Arc::new(Database::open_with_mode(
    "./my-agent-db",
    DurabilityMode::Buffered {
        flush_interval_ms: 100,
        max_pending_writes: 1000,
    }
)?);

// Create KVStore primitive
let kv = KVStore::new(db.clone());

// Begin a run
let run_id = db.begin_run();

// Store and retrieve data
kv.put(&run_id, "key", Value::String("value".into()))?;
let value = kv.get(&run_id, "key")?;

// End run
db.end_run(run_id)?;
```

See [Getting Started](getting-started.md) for the full guide.

## Performance

| Mode | Latency | Throughput | Data Loss |
|------|---------|------------|-----------|
| InMemory | <3µs | 250K+ ops/sec | All |
| Buffered | <30µs | 50K+ ops/sec | ~100ms |
| Strict | ~2ms | ~500 ops/sec | None |

### Scaling

| Threads | Disjoint Scaling |
|---------|------------------|
| 2 | ≥1.8× |
| 4 | ≥3.2× |

## Primitives

| Primitive | Purpose |
|-----------|---------|
| **KVStore** | Key-value storage with batch operations |
| **EventLog** | Append-only log with hash chaining |
| **StateCell** | Named cells with CAS operations |
| **TraceStore** | Hierarchical agent reasoning traces |
| **RunIndex** | Run lifecycle management |

## Documentation Structure

```
docs/
├── reference/              # User-facing reference docs
│   ├── getting-started.md  # Quick start guide
│   ├── api-reference.md    # Complete API reference
│   ├── architecture.md     # Architecture overview
│   └── README.md           # This file
│
├── architecture/           # Technical specifications
│   └── M4_ARCHITECTURE.md  # Performance architecture
│
├── development/            # Developer guides
│   └── DEVELOPMENT_WORKFLOW.md
│
└── milestones/             # Project management
    └── MILESTONES.md       # Roadmap
```

## Support

- **Issues**: [GitHub Issues](https://github.com/anibjoshi/in-mem/issues)
- **Documentation**: This site

## License

[MIT License](https://github.com/anibjoshi/in-mem/blob/main/LICENSE)

---

**Version**: 0.3.0 (M3 Primitives + M4 Performance)
**Last Updated**: 2026-01-16
