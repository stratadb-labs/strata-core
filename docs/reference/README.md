# Strata Reference Documentation

**Strata** - a fast, durable, embedded database for AI agent workloads.

**Current Version**: 0.10.0 (M10 Storage Backend, Retention & Compaction)

## Quick Links

- [Getting Started](getting-started.md) - Installation and quick start
- [API Reference](api-reference.md) - Complete API documentation
- [Architecture](architecture.md) - How Strata works internally
- [Milestones](../milestones/MILESTONES.md) - Project roadmap

## Features

- **Seven Primitives**: KVStore, EventLog, StateCell, TraceStore, RunIndex, JsonStore, VectorStore
- **Hybrid Search**: BM25 + semantic (vector) search with RRF fusion
- **Three Durability Modes**: InMemory (<3µs), Buffered (<30µs), Strict (~2ms)
- **OCC Transactions**: Optimistic concurrency with snapshot isolation
- **Run-Scoped Operations**: Every operation tagged with RunId for replay
- **Disk-Backed Storage**: Portable database artifacts with WAL + snapshots
- **Retention Policies**: KeepAll, KeepLast(N), KeepFor(Duration)
- **Compaction**: User-triggered WAL and data compaction
- **Crash Recovery**: Deterministic, idempotent, prefix-consistent recovery
- **Deterministic Replay**: Side-effect free reconstruction of agent run state
- **Versioned API**: All reads return `Versioned<T>` with version and timestamp

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
| M8 Vector | ✅ |
| M9 API Stabilization | ✅ |
| M10 Storage Backend | ✅ |
| M11 Public API Contract | Next |

## Quick Start

```rust
use strata::{Database, DatabaseConfig, DurabilityMode};
use strata::primitives::KVStore;
use strata::Value;
use std::sync::Arc;

// Open database with disk-backed storage
let config = DatabaseConfig {
    durability_mode: DurabilityMode::Buffered {
        flush_interval_ms: 100,
        max_pending_writes: 1000
    },
    ..Default::default()
};

let db = Arc::new(Database::open("./my-agent-db", config)?);

let kv = KVStore::new(db.clone());
let run_id = db.begin_run();

// All operations participate in transactions
kv.put(&run_id, "key", Value::String("value".into()))?;

// Reads return Versioned<T> with version and timestamp
let versioned = kv.get(&run_id, "key")?;
if let Some(v) = versioned {
    println!("Value: {:?}, Version: {:?}", v.value, v.version);
}

// Checkpoint for crash recovery
db.checkpoint()?;

db.end_run(run_id)?;
db.close()?;
```

## Performance

| Mode | Latency | Throughput |
|------|---------|------------|
| InMemory | <3µs | 250K+ ops/sec |
| Buffered | <30µs | 50K+ ops/sec |
| Strict | ~2ms | ~500 ops/sec |

## Storage

Strata uses a portable directory structure:

```
strata.db/
├── MANIFEST              # Database metadata
├── WAL/                  # Write-ahead log segments
│   ├── wal-000001.seg
│   └── ...
├── SNAPSHOTS/            # Point-in-time snapshots
│   └── snap-000010.chk
└── DATA/                 # Optional data segments
```

**Portability**: Copy a closed `strata.db/` directory to create a valid clone.

---

**Version**: 0.10.0
**Last Updated**: 2026-01-21
