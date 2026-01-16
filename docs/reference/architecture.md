# Architecture Overview

Learn how **in-mem** works internally and why it's designed the way it is.

**Current Version**: 0.3.0 (M3 Primitives + M4 Performance)

## Design Philosophy

1. **Run-First Design**: Every operation is scoped to a run for deterministic replay
2. **Layered Performance**: Fast paths for common operations, full transactions when needed
3. **Accept MVP Limitations, Design for Evolution**: Simple implementations now, trait abstractions for future optimization

## System Architecture

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

## Concurrency Model

### Optimistic Concurrency Control (OCC)

**in-mem** uses OCC with first-committer-wins conflict detection:

1. **BEGIN**: Acquire snapshot (current version)
2. **EXECUTE**: Read from snapshot, buffer writes
3. **VALIDATE**: Check read_set versions unchanged
4. **COMMIT**: Allocate version, write to WAL, apply to storage

### Read-Your-Writes Semantics

Within a transaction, reads see uncommitted writes:
1. Check `write_set` (uncommitted write)
2. Check `delete_set` (uncommitted delete → return None)
3. Check snapshot (committed data)

## Durability Modes (M4)

### InMemory Mode

```
write → apply to storage → return
```

- Latency: <3µs
- Throughput: 250K+ ops/sec
- Data Loss: All (on crash)

### Buffered Mode (Production Default)

```
write → log to WAL buffer → apply to storage → return
                 ↓
      background thread fsyncs periodically
```

- Latency: <30µs
- Throughput: 50K+ ops/sec
- Data Loss: Bounded (~100ms)

### Strict Mode

```
write → log to WAL → fsync → apply to storage → return
```

- Latency: ~2ms
- Throughput: ~500 ops/sec
- Data Loss: Zero

## Primitives Architecture

All primitives are stateless facades:

```rust
pub struct Primitive {
    db: Arc<Database>
}
```

### Fast Path vs Transaction Path

**Fast Path** (for read-only operations):
- Direct snapshot read
- No transaction overhead
- <10µs latency

**Transaction Path** (for writes):
- Full OCC with conflict detection
- WAL persistence (based on durability mode)

## Performance Characteristics

| Metric | Target |
|--------|--------|
| InMemory put | <3µs |
| InMemory throughput (1 thread) | 250K ops/sec |
| Buffered put | <30µs |
| Buffered throughput | 50K ops/sec |
| Fast path read | <10µs |
| Disjoint scaling (4 threads) | ≥3.2× |

## See Also

- [API Reference](api-reference.md)
- [Getting Started Guide](getting-started.md)
- [Milestones](../milestones/MILESTONES.md)
