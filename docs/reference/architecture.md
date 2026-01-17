# Architecture Overview

Learn how **in-mem** works internally and why it's designed the way it is.

**Current Version**: 0.7.0 (M7 Durability, Snapshots & Replay)

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
│  Primitives (KV, EventLog, StateCell, Trace, RunIndex,  │  ← Stateless facades
│              JsonStore)                                 │
└───────────────────────────┬─────────────────────────────┘
                            │
┌───────────────────────────▼─────────────────────────────┐
│  Search Layer (HybridSearch, BM25, InvertedIndex, RRF)  │  ← Retrieval surfaces
└───────────────────────────┬─────────────────────────────┘
                            │
┌───────────────────────────▼─────────────────────────────┐
│       Engine (Database, Run Lifecycle, Coordinator)     │  ← Orchestration
└───────┬───────────────────┬───────────────────┬─────────┘
        │                   │                   │
┌───────▼───────┐   ┌───────▼───────┐   ┌───────▼───────┐
│  Concurrency  │   │   Durability  │   │    Replay     │
│(OCC/Txn/CAS)  │   │(WAL/Snapshot) │   │(RunView/Diff) │
└───────┬───────┘   └───────┬───────┘   └───────┬───────┘
        │                   │                   │
┌───────▼───────────────────▼───────────────────▼─────────┐
│      Storage (UnifiedStore + Snapshots + WAL)           │
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

All six primitives are stateless facades:

```rust
pub struct Primitive {
    db: Arc<Database>
}
```

**Six Primitives**:
- **KVStore**: Key-value storage with batch operations
- **EventLog**: Append-only log with hash chaining
- **StateCell**: Named cells with CAS operations
- **TraceStore**: Hierarchical trace recording
- **RunIndex**: Run lifecycle management
- **JsonStore** (M5): JSON documents with path mutations

### Fast Path vs Transaction Path

**Fast Path** (for read-only operations):
- Direct snapshot read
- No transaction overhead
- <10µs latency

**Transaction Path** (for writes):
- Full OCC with conflict detection
- WAL persistence (based on durability mode)

## Search Architecture (M6)

### Hybrid Search

**in-mem** provides unified search across all primitives:

```
SearchRequest → HybridSearch → [BM25 + Semantic] → RRF Fusion → SearchResponse
```

### Components

**BM25Lite**: Lightweight keyword scoring
- Tokenization with lowercase normalization
- TF-IDF weighting with BM25 formula
- Title boost for structured documents

**InvertedIndex**: Optional full-text index
- Disabled by default (opt-in)
- Tracks document frequency and term positions
- Version-based cache invalidation

**RRF Fusion**: Reciprocal Rank Fusion
- Combines keyword and semantic scores
- Default k=60 for rank normalization
- Preserves relative ordering from both sources

### Budget Semantics

Search operations respect time budgets:
- `budget_ms`: Maximum search time
- Graceful degradation on timeout
- Partial results returned with budget metadata

## Snapshot System (M7)

Periodic snapshots enable bounded recovery time.

### Snapshot Format

```
+------------------+
| Magic (10 bytes) |  "INMEM_SNAP"
+------------------+
| Version (4)      |  Format version
+------------------+
| Timestamp (8)    |  Microseconds since epoch
+------------------+
| WAL Offset (8)   |  WAL position covered
+------------------+
| Primitive Data   |  Serialized state per primitive
+------------------+
| CRC32 (4)        |  Checksum
+------------------+
```

### Snapshot Triggers

Snapshots are triggered automatically based on:
- WAL size threshold (default: 100 MB)
- Time interval (default: 30 minutes)
- Clean shutdown (configurable)

### WAL Truncation

After a successful snapshot:
1. New WAL entries go to truncated file
2. Old WAL data before snapshot offset is removed
3. Atomic rename ensures consistency

## Crash Recovery (M7)

Recovery is deterministic, idempotent, and prefix-consistent.

### Recovery Sequence

```
1. Find latest valid snapshot (fallback to older if corrupt)
2. Load snapshot state
3. Replay WAL from snapshot offset
4. Skip entries without commit markers (orphaned transactions)
5. Rebuild indexes
```

### Recovery Invariants

| # | Invariant | Meaning |
|---|-----------|---------|
| R1 | Deterministic | Same WAL + Snapshot = Same state |
| R2 | Idempotent | Replaying recovery produces identical state |
| R3 | Prefix-consistent | No partial transactions visible |
| R4 | Never invents | Only committed data appears |
| R5 | Never drops committed | All durable commits survive |
| R6 | May drop uncommitted | Depending on durability mode |

### Transaction Framing

```
[Entry 1 with tx_id=T1]
[Entry 2 with tx_id=T1]
[Entry 3 with tx_id=T1]
[TransactionCommit with tx_id=T1]  ← Commit marker
```

Entries without commit markers are discarded during recovery.

## Deterministic Replay (M7)

Replay reconstructs agent run state from EventLog.

### Replay vs Recovery

| Aspect | Recovery | Replay |
|--------|----------|--------|
| Purpose | Restore database after crash | Reconstruct run state |
| Scope | Entire database | Single run |
| Source | WAL + Snapshot | EventLog |
| Mutates | Yes (canonical store) | No (read-only view) |
| Speed | O(WAL size) | O(run size) |

### Replay Invariants

| # | Invariant | Meaning |
|---|-----------|---------|
| P1 | Pure function | Over (Snapshot, WAL, EventLog) |
| P2 | Side-effect free | Does not mutate canonical store |
| P3 | Derived view | Not a new source of truth |
| P4 | Does not persist | Unless explicitly materialized |
| P5 | Deterministic | Same inputs = Same view |
| P6 | Idempotent | Running twice produces identical view |

### Run Lifecycle

```
begin_run(run_id)
    │
    ▼
  Active ──────────────────────────┐
    │                              │
    ▼                              ▼
end_run() ──► Completed      (crash) ──► Orphaned
```

## Performance Characteristics

| Metric | Target |
|--------|--------|
| InMemory put | <3µs |
| InMemory throughput (1 thread) | 250K ops/sec |
| Buffered put | <30µs |
| Buffered throughput | 50K ops/sec |
| Fast path read | <10µs |
| Disjoint scaling (4 threads) | ≥3.2× |
| Search (no index) | O(n) scan |
| Search (with index) | O(log n) lookup |
| Snapshot write (100MB) | < 5 seconds |
| Full recovery (100MB + 10K WAL) | < 5 seconds |
| Replay run (1K events) | < 100 ms |
| Diff runs (1K keys) | < 200 ms |

## See Also

- [API Reference](api-reference.md)
- [Getting Started Guide](getting-started.md)
- [Milestones](../milestones/MILESTONES.md)
