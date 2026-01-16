# Architecture Overview

Learn how **in-mem** works internally and why it's designed the way it is.

**Current Version**: 0.3.0 (M3 Primitives + M4 Performance)

## Design Philosophy

**in-mem** is built around three core principles:

1. **Run-First Design**: Every operation is scoped to a run, enabling deterministic replay and debugging
2. **Accept MVP Limitations, Design for Evolution**: Simple implementations now, trait abstractions for future optimization
3. **Layered Performance**: Fast paths for common operations, full transactions when needed

## System Architecture

### Layered Design

```
┌─────────────────────────────────────────────────────────┐
│              API Layer (embedded/rpc/mcp)               │
└───────────────────────────┬─────────────────────────────┘
                            │
┌───────────────────────────▼─────────────────────────────┐
│  Primitives (KV, EventLog, StateCell, Trace, RunIndex)  │  ← Stateless facades
└───────────────────────────┬─────────────────────────────┘
                            │
┌───────────────────────────▼─────────────────────────────┐
│       Engine (Database, Run Lifecycle, Coordinator)     │  ← Orchestration
└───────┬───────────────────────────────────────┬─────────┘
        │                                       │
┌───────▼───────────────┐         ┌─────────────▼─────────┐
│     Concurrency       │         │      Durability       │
│  (OCC/Transactions)   │         │  (InMemory/Buffered/  │
│                       │         │       Strict)         │
└───────────┬───────────┘         └───────────┬───────────┘
            │                                 │
┌───────────▼─────────────────────────────────▼───────────┐
│         Storage (UnifiedStore + Snapshots)              │
└───────────────────────────┬─────────────────────────────┘
                            │
┌───────────────────────────▼─────────────────────────────┐
│      Core Types (RunId, Key, Value, TypeTag)            │
└─────────────────────────────────────────────────────────┘
```

### Layer Responsibilities

**Core Types**: Foundation data structures (RunId, Key, Value, TypeTag)

**Storage**: Unified BTreeMap with snapshots and version tracking

**Durability**: Write-ahead log (WAL) with three modes: InMemory, Buffered, Strict

**Concurrency**: Optimistic Concurrency Control (OCC) with snapshot isolation

**Engine**: Orchestrates run lifecycle, transactions, recovery

**Primitives**: High-level APIs (KV, EventLog, StateCell, TraceStore, RunIndex)

**API**: Embedded library interface (network layer planned for future)

## Data Model

### Keys

Every key in **in-mem** has three components:

```rust
pub struct Key {
    namespace: Namespace,  // tenant/app/agent/run hierarchy
    type_tag: TypeTag,     // KV, Event, State, Trace, RunIndex
    user_key: Vec<u8>,     // your application key
}
```

**Key Ordering**: Keys are ordered by namespace → type_tag → user_key

This enables:
- Efficient prefix scans (list all keys for a run)
- Cross-primitive queries (get all events and KV for a run)
- Namespace isolation (tenant separation)

### Type Tags

```rust
pub enum TypeTag {
    KV,         // Key-value pairs
    Event,      // Event log entries
    State,      // StateCell values
    Trace,      // Trace records
    RunIndex,   // Run metadata
    Index,      // Secondary indices
}
```

### Values

Values are versioned with metadata:

```rust
pub struct VersionedValue {
    value: Value,              // The actual data
    version: u64,              // Monotonically increasing
    timestamp: Timestamp,      // When written
    ttl: Option<Duration>,     // Expiration (optional)
}
```

**Version Numbers**: Global, monotonically increasing counter. Enables:
- Snapshot isolation (read as-of version V)
- Conflict detection (version changed during transaction)
- Replay (apply operations in version order)

## Concurrency Model

### Optimistic Concurrency Control (OCC)

**in-mem** uses OCC with first-committer-wins conflict detection:

```
┌─────────────────────────────────────────────────────────┐
│                    Transaction Flow                     │
├─────────────────────────────────────────────────────────┤
│                                                         │
│  1. BEGIN                                               │
│     ├─ Acquire snapshot (current version)              │
│     └─ Initialize read/write/delete sets               │
│                                                         │
│  2. EXECUTE                                             │
│     ├─ Reads: Check write_set → delete_set → snapshot  │
│     ├─ Writes: Buffer in write_set                     │
│     └─ Deletes: Buffer in delete_set                   │
│                                                         │
│  3. VALIDATE                                            │
│     ├─ Check read_set versions unchanged               │
│     └─ If conflict → ABORT, else continue              │
│                                                         │
│  4. COMMIT                                              │
│     ├─ Allocate commit version                         │
│     ├─ Write to WAL (durability mode determines sync)  │
│     └─ Apply to storage                                │
│                                                         │
└─────────────────────────────────────────────────────────┘
```

### Read-Your-Writes Semantics

Within a transaction, reads see uncommitted writes:

1. Check `write_set` (uncommitted write)
2. Check `delete_set` (uncommitted delete → return None)
3. Check snapshot (committed data, tracked in `read_set`)

### Conflict Detection

At commit time:
- For each key in `read_set`, check if current version > version at read time
- If any conflict detected → transaction aborts
- First committer wins (no blocking)

### Snapshot Isolation

```rust
pub struct ClonedSnapshotView {
    version: u64,
    data: Arc<BTreeMap<Key, VersionedValue>>,
}
```

**Current Implementation**: Deep clone of data at snapshot time (M2)
- Simple and correct
- O(data_size) memory and creation time
- Acceptable for agent workloads with small working sets

**Future Optimization**: LazySnapshotView with version bounds
- O(1) creation time
- Read from live storage with version filtering

## Durability

### Write-Ahead Log (WAL)

Every write is logged before applying to storage:

```rust
pub enum WALEntry {
    BeginTxn { txn_id: u64, run_id: RunId, timestamp: Timestamp },
    Write { run_id: RunId, key: Key, value: Value, version: u64 },
    Delete { run_id: RunId, key: Key, version: u64 },
    CommitTxn { txn_id: u64, run_id: RunId },
    AbortTxn { txn_id: u64, run_id: RunId },
}
```

**Entry Format**: `[Length][Type][Payload][CRC32]`

**CRC Protection**: Every entry has CRC32 checksum. Corrupted entries stop recovery (fail-safe).

### Durability Modes (M4)

**in-mem** provides three durability modes to match different workload requirements:

#### InMemory Mode

```
write → apply to storage → return
```

| Property | Value |
|----------|-------|
| WAL | None |
| fsync | None |
| Latency | <3µs |
| Throughput | 250K+ ops/sec |
| Data Loss | All (on crash) |

**Use Cases**: Tests, caches, ephemeral data, benchmarks

#### Buffered Mode (Production Default)

```
write → log to WAL buffer → apply to storage → return
                 ↓
      background thread fsyncs periodically
```

| Property | Value |
|----------|-------|
| WAL | Append (buffered) |
| fsync | Every 100ms or 1000 writes |
| Latency | <30µs |
| Throughput | 50K+ ops/sec |
| Data Loss | Bounded (~100ms) |

**Background Thread**:
- Wakes on timer (flush_interval_ms) or threshold (max_pending_writes)
- Graceful shutdown with final sync
- Thread lifecycle properly managed

**Use Cases**: Production workloads, agent workflows, general use

#### Strict Mode

```
write → log to WAL → fsync → apply to storage → return
```

| Property | Value |
|----------|-------|
| WAL | Append (sync) |
| fsync | Every write |
| Latency | ~2ms |
| Throughput | ~500 ops/sec |
| Data Loss | Zero |

**Use Cases**: Audit logs, compliance, critical metadata

### Recovery

On database open:

1. Scan WAL from beginning
2. Validate each entry (CRC check)
3. Replay committed transactions
4. Discard incomplete transactions (no CommitTxn = rollback)
5. Rebuild secondary indices
6. Resume normal operation

**Conservative Recovery**: Stop at first corrupted entry (don't skip). Ensures no silent data loss.

## Primitives Architecture

All primitives follow the same pattern:

```rust
pub struct Primitive {
    db: Arc<Database>  // Stateless facade
}
```

### Primitive Layer Design

```
┌─────────────────────────────────────────────────────────┐
│                    Primitive Layer                      │
├─────────────────────────────────────────────────────────┤
│                                                         │
│  ┌─────────┐ ┌─────────┐ ┌─────────┐ ┌───────┐ ┌─────┐ │
│  │ KVStore │ │EventLog │ │StateCell│ │ Trace │ │ Run │ │
│  │         │ │         │ │         │ │ Store │ │Index│ │
│  └────┬────┘ └────┬────┘ └────┬────┘ └───┬───┘ └──┬──┘ │
│       │          │          │          │        │     │
│       └──────────┴──────────┴──────────┴────────┘     │
│                          │                            │
│                   Extension Traits                    │
│            (KVStoreExt, EventLogExt, etc.)           │
│                          │                            │
└──────────────────────────┼────────────────────────────┘
                           │
              ┌────────────▼────────────┐
              │   TransactionContext    │
              │   (cross-primitive)     │
              └─────────────────────────┘
```

### Fast Path vs Transaction Path

**Fast Path** (for read-only operations):
```rust
// Direct snapshot read - no transaction overhead
pub fn get(&self, run_id: &RunId, key: &str) -> Result<Option<Value>> {
    let snapshot = self.db.storage().create_snapshot();
    // Read directly from snapshot
}
```

**Transaction Path** (for writes or multi-operation consistency):
```rust
// Full transaction with conflict detection
pub fn put(&self, run_id: &RunId, key: &str, value: Value) -> Result<()> {
    self.db.transaction(run_id, |txn| {
        txn.kv_put(key, value)
    })
}
```

### Secondary Indices

Each primitive maintains its own indices for efficient queries:

**EventLog**:
- `event:meta:{run_id}` → sequence counter, last hash
- `event:{run_id}:{seq}` → event data

**StateCell**:
- `state:{run_id}:{name}` → state value with version

**TraceStore**:
- `trace:{run_id}:{id}` → trace data
- `trace:idx:type:{run_id}:{type}:{id}` → by type
- `trace:idx:tag:{run_id}:{tag}:{id}` → by tag
- `trace:idx:parent:{run_id}:{parent_id}:{id}` → by parent
- `trace:idx:time:{run_id}:{hour_bucket}:{id}` → by time

**RunIndex**:
- `run:{id}` → run metadata
- `run:idx:status:{status}:{id}` → by status
- `run:idx:tag:{tag}:{id}` → by tag
- `run:idx:parent:{parent_id}:{id}` → by parent

## Performance Characteristics

### M4 Targets

| Metric | Target | Notes |
|--------|--------|-------|
| InMemory put | <3µs | No WAL, no syscalls |
| InMemory throughput (1 thread) | 250K ops/sec | |
| Buffered put | <30µs | WAL append, async fsync |
| Buffered throughput | 50K ops/sec | |
| Strict put | ~2ms | WAL append + fsync |
| Fast path read | <10µs | Direct snapshot |
| Disjoint scaling (2 threads) | ≥1.8× | |
| Disjoint scaling (4 threads) | ≥3.2× | |

### Facade Tax

Performance overhead from abstraction layers:

| Layer | Overhead |
|-------|----------|
| A0 (engine/put_direct) | Baseline |
| A1 (engine/transaction) | <10× A0 |
| B (primitive/put) | <5× A1 |
| Total (B/A0) | <30× |

### Hot Path Optimization

The M4 hot path is designed for minimal overhead:

1. **Transaction Pooling**: Reuse transaction objects (planned)
2. **Snapshot Acquisition**: <500ns target, allocation-free (planned)
3. **Fast Path Reads**: Bypass transaction for read-only operations

## Known Limitations

| Limitation | Impact | Mitigation |
|------------|--------|------------|
| ClonedSnapshot | O(n) creation | Acceptable for small working sets; lazy snapshots planned |
| Global version counter | AtomicU64 contention | Sharding planned for M12 |
| BTreeMap storage | Ordered but slower than HashMap | DashMap integration planned |
| No persistent indices | Rebuilt on recovery | Snapshot indices planned for M6 |

## Comparison to Other Databases

### vs. SQLite

| Feature | in-mem | SQLite |
|---------|--------|--------|
| Run-scoped operations | Built-in | Manual |
| Multi-primitive | Yes (5 primitives) | SQL only |
| Agent-optimized | Yes | No |
| SQL queries | No | Yes |
| Storage | In-memory | Disk |

### vs. Redis

| Feature | in-mem | Redis |
|---------|--------|-------|
| Embedded | Yes | No |
| Network overhead | None | ~100µs |
| Run concept | Built-in | Manual |
| Replay/debugging | Built-in | Manual |
| Data structures | 5 primitives | Rich |

### vs. RocksDB

| Feature | in-mem | RocksDB |
|---------|--------|---------|
| Multi-primitive | Yes | KV only |
| Run-scoped | Built-in | Manual |
| Storage | In-memory | LSM tree |
| Simple API | Yes | Complex |

## Future Roadmap

### M5: JSON Primitive
- Native JSON with path-level atomicity
- Patch-based WAL entries
- Structural conflict detection

### M6: Durability
- Periodic snapshots
- WAL truncation
- Full recovery with JSON support

### M7: Replay & Polish
- Deterministic replay
- Run diffing
- Production readiness

### Post-MVP
- Vector store (M8)
- Network layer (M9)
- MCP integration (M10)
- Query DSL (M11)
- Redis parity performance (M12)

## See Also

- [API Reference](api-reference.md) - Complete API documentation
- [Getting Started Guide](getting-started.md) - Quick start
- [Milestones](../milestones/MILESTONES.md) - Project roadmap
- [M4 Architecture](../architecture/M4_ARCHITECTURE.md) - Performance architecture details

---

**Current Version**: 0.3.0 (M3 Primitives + M4 Performance)
**Architecture Status**: Production-ready for embedded use
