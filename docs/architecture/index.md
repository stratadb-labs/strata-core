# Architecture Overview

StrataDB is a layered embedded database built as a Rust workspace of 7 crates. This section describes the system from the top down.

## High-Level Diagram

```
+-----------------------------------------------------------+
|  Strata API                                                |
|  (KV, Event, State, JSON, Vector, Run)                     |
+-----------------------------------------------------------+
|  Executor (Command dispatch)                               |
|  Session (Transaction lifecycle + read-your-writes)         |
+-----------------------------------------------------------+
|  Engine                                                    |
|  (Database, Primitives, Transaction coordination)          |
+-----+-----------------------+-----------------------------+
      |                       |
+-----v-------+  +------------v----------+  +--------------+
| Concurrency |  |  Durability           |  | Intelligence |
| OCC, CAS    |  |  WAL, Snapshots       |  | BM25, RRF    |
| Validation  |  |  Recovery, RunBundle  |  | Hybrid Search|
+------+------+  +----------+------------+  +------+-------+
       |                     |                      |
       +----------+----------+----------------------+
                  |
        +---------v---------+
        |  Storage          |
        |  ShardedStore     |
        |  DashMap-based    |
        +-------------------+
        |  Core             |
        |  Value, Types     |
        +-------------------+
```

## Key Design Decisions

### Unified Storage

All six primitives store their data in a single `ShardedStore` (DashMap-based). Keys are prefixed with the run ID and primitive type. This enables:

- **Atomic multi-primitive transactions** — a single OCC validation covers KV, State, Event, and JSON
- **Simple storage layer** — one sorted map, no separate data files per primitive
- **Run deletion** — scan and delete by prefix

### Run-Tagged Keys

Every key in storage includes the run ID: `{run_id}:{primitive}:{user_key}`. This makes:

- **Run isolation** automatic — no filtering needed
- **Run replay** O(run size) instead of O(total database size)
- **Run deletion** a prefix scan

### Optimistic Concurrency Control

Transactions use OCC rather than locks:

- Begin: take a snapshot version
- Execute: read from snapshot, buffer writes
- Validate: check that reads haven't been modified by concurrent commits
- Commit: apply writes atomically

This works well for AI agents because they rarely conflict (different keys, different runs).

### Stateless Primitives

The `Strata` struct is a thin wrapper around an `Executor`. Primitives don't hold state — they just translate method calls into `Command` enums and dispatch them. This means:

- Multiple `Strata` instances can safely share a `Database`
- No warm-up or initialization state
- Idempotent retry works correctly

## Deep Dives

- [Crate Structure](crate-structure.md) — the 7 crates and their responsibilities
- [Storage Engine](storage-engine.md) — ShardedStore, MVCC, key structure
- [Durability and Recovery](durability-and-recovery.md) — WAL, snapshots, recovery flow
- [Concurrency Model](concurrency-model.md) — OCC lifecycle, conflict detection
