# Architecture Overview

Learn how **Strata** works internally and why it's designed the way it is.

**Current Version**: 0.10.0 (M10 Storage Backend, Retention & Compaction)

## Design Philosophy

1. **Run-First Design**: Every operation is scoped to a run for deterministic replay
2. **Layered Performance**: Fast paths for common operations, full transactions when needed
3. **Storage is Infrastructure**: Disk is a persistence layer, not the primary interface
4. **Correctness Over Performance**: A correct implementation can be optimized; an incorrect one destroys trust
5. **Accept MVP Limitations, Design for Evolution**: Simple implementations now, trait abstractions for future optimization

## System Architecture

```
┌─────────────────────────────────────────────────────────┐
│              API Layer (embedded/rpc/mcp)               │
└───────────────────────────┬─────────────────────────────┘
                            │
┌───────────────────────────▼─────────────────────────────┐
│  Primitives (KV, EventLog, StateCell, Trace, RunIndex,  │  ← Stateless facades
│              JsonStore, VectorStore)                    │
└───────────────────────────┬─────────────────────────────┘
                            │
┌───────────────────────────▼─────────────────────────────┐
│  Search Layer (HybridSearch, BM25, Vector, RRF Fusion)  │  ← Retrieval surfaces
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
│   Storage Backend (M10)                                 │
│   (WAL Segments, Snapshots, MANIFEST, Retention)        │
└───────────────────────────┬─────────────────────────────┘
                            │
┌───────────────────────────▼─────────────────────────────┐
│      Core Types (RunId, Key, Value, Versioned<T>)       │
└─────────────────────────────────────────────────────────┘
```

## Versioned API (M9)

All read operations return `Versioned<T>`:

```rust
pub struct Versioned<T> {
    pub value: T,
    pub version: Version,
    pub timestamp: u64,  // microseconds since epoch
}

pub enum Version {
    Txn(u64),       // KV, JSON, Vector, Run
    Sequence(u64),  // Events
    Counter(u64),   // StateCell
}
```

### Seven Invariants

Every primitive conforms to:

1. **Everything is Addressable**: `EntityRef` for universal addressing
2. **Everything is Versioned**: All reads return version info
3. **Everything is Transactional**: Atomic cross-primitive operations
4. **Everything Has a Lifecycle**: Begin, modify, end states
5. **Everything Exists Within a Run**: Run scoping for replay
6. **Everything is Introspectable**: Metadata and history access
7. **Reads and Writes Have Consistent Semantics**: Symmetric API shapes

## Concurrency Model

### Optimistic Concurrency Control (OCC)

**Strata** uses OCC with first-committer-wins conflict detection:

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

All seven primitives are stateless facades:

```rust
pub struct Primitive {
    db: Arc<Database>
}
```

**Seven Primitives**:
- **KVStore**: Key-value storage with batch operations
- **EventLog**: Append-only log with hash chaining
- **StateCell**: Named cells with CAS operations
- **TraceStore**: Hierarchical trace recording
- **RunIndex**: Run lifecycle management
- **JsonStore** (M5): JSON documents with path mutations
- **VectorStore** (M8): Embeddings with similarity search

### Fast Path vs Transaction Path

**Fast Path** (for read-only operations):
- Direct snapshot read
- No transaction overhead
- <10µs latency

**Transaction Path** (for writes):
- Full OCC with conflict detection
- WAL persistence (based on durability mode)

## Vector Architecture (M8)

### VectorStore

Semantic search for AI agent workloads:

```
insert(key, embedding, metadata) → Versioned<()>
search(query, k, metric, filter) → Vec<SearchResult>
```

### Similarity Metrics

| Metric | Formula | Use Case |
|--------|---------|----------|
| Cosine | 1 - (a·b)/(‖a‖‖b‖) | Normalized embeddings |
| Euclidean | ‖a-b‖₂ | Absolute distances |
| DotProduct | a·b | Pre-normalized vectors |

### Index Support

- **Brute Force**: O(n) scan, always correct
- **HNSW**: O(log n) approximate, configurable recall

## Search Architecture (M6)

### Hybrid Search

**Strata** provides unified search across all primitives:

```
SearchRequest → HybridSearch → [BM25 + Vector] → RRF Fusion → SearchResponse
```

### Components

**BM25Lite**: Lightweight keyword scoring
- Tokenization with lowercase normalization
- TF-IDF weighting with BM25 formula
- Title boost for structured documents

**Vector Search**: Semantic similarity
- Cosine/Euclidean/DotProduct metrics
- Metadata filtering
- HNSW acceleration (optional)

**RRF Fusion**: Reciprocal Rank Fusion
- Combines keyword and semantic scores
- Default k=60 for rank normalization
- Preserves relative ordering from both sources

### Budget Semantics

Search operations respect time budgets:
- `budget_ms`: Maximum search time
- Graceful degradation on timeout
- Partial results returned with budget metadata

## Storage Backend (M10)

M10 adds production-ready disk-backed storage.

### Directory Structure

```
strata.db/
├── MANIFEST              # Database metadata
├── WAL/
│   ├── wal-000001.seg    # WAL segment files
│   ├── wal-000002.seg
│   └── ...
├── SNAPSHOTS/
│   ├── snap-000010.chk   # Snapshot checkpoint files
│   └── ...
└── DATA/                 # Optional: materialized data
```

### WAL Architecture

**WAL Segments**:
- Append-only, segmented files
- Format: `wal-NNNNNN.seg`
- Default size limit: 64 MB
- Closed segments are **immutable**

**WAL Record Format**:
```
+------------------+
| Length (u32)     |  Total bytes
+------------------+
| Format Ver (u8)  |  Version
+------------------+
| TxnId (u64)      |  Transaction ID
+------------------+
| RunId (16 bytes) |  UUID
+------------------+
| Timestamp (u64)  |  Microseconds
+------------------+
| Writeset         |  Mutations
+------------------+
| CRC32 (u32)      |  Checksum
+------------------+
```

### Storage Invariants

| # | Invariant | Meaning |
|---|-----------|---------|
| S1 | WAL append-only | File size only grows |
| S2 | Segments immutable | Closed segments never change |
| S3 | Self-delimiting | Records parseable independently |
| S4 | Consistent snapshots | Point-in-time capture |
| S5 | Storage never assigns versions | Engine assigns, storage persists |
| S6 | Durability mode respected | fsync semantics honored |

### Checkpoint

```rust
checkpoint() → CheckpointInfo { watermark_txn, snapshot_id, timestamp }
```

Creates a stable boundary for:
- Safe database copying
- WAL truncation
- Crash recovery point

### Retention Policies

User-configurable data retention:

```rust
pub enum RetentionPolicy {
    KeepAll,              // Default - never delete
    KeepLast(u64),        // Keep N most recent versions
    KeepFor(Duration),    // Keep versions within time window
    Composite(Vec<...>),  // Union of policies
}
```

**Retention Invariants**:
- Version ordering preserved
- No silent fallback to nearest version
- Explicit `HistoryTrimmed` error with metadata

### Compaction

User-triggered space reclamation:

```rust
compact(mode) → CompactInfo { reclaimed_bytes, wal_segments_removed, versions_removed }
```

**Modes**:
- `WALOnly`: Remove WAL segments covered by snapshot
- `Full`: WAL + retention enforcement

**Compaction Invariants**:
- Read equivalence (retained reads unchanged)
- Version IDs never change
- History order preserved
- No semantic changes

## Snapshot System (M7)

Periodic snapshots enable bounded recovery time.

### Snapshot Format

```
+------------------+
| Magic (10 bytes) |  "STRATA_SNP"
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

## Crash Recovery (M7/M10)

Recovery is deterministic, idempotent, and prefix-consistent.

### Recovery Sequence

```
1. Read MANIFEST to find latest snapshot
2. Load snapshot state (fall back to older if corrupt)
3. Replay WAL from snapshot watermark
4. Skip entries without commit markers (orphaned transactions)
5. Truncate partial records at WAL tail
6. Rebuild indexes
7. Update MANIFEST
```

### Recovery Invariants

| # | Invariant | Meaning |
|---|-----------|---------|
| R1 | Deterministic | Same WAL + Snapshot = Same state |
| R2 | Idempotent | Replaying recovery produces identical state |
| R3 | Prefix-consistent | No partial transactions visible |
| R4 | Never invents | Only committed data appears |
| R5 | Never drops committed | All durable commits survive (Strict mode) |
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
| Vector search (brute force, 10K vectors) | <100ms |
| Vector search (HNSW, 100K vectors) | <10ms |
| Snapshot write (100MB) | < 5 seconds |
| Full recovery (100MB + 10K WAL) | < 5 seconds |
| Replay run (1K events) | < 100 ms |
| Diff runs (1K keys) | < 200 ms |
| Compaction (WAL only) | O(segments) |
| Compaction (full) | O(retained versions) |

## See Also

- [API Reference](api-reference.md)
- [Getting Started Guide](getting-started.md)
- [Milestones](../milestones/MILESTONES.md)
