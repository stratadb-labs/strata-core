# M7 Scope: Durability, Snapshots, Replay & Storage Stabilization

**Status**: SEALED

---

## Executive Summary

M7 consolidates all durability and persistence concerns into one milestone. The goal is to make the database production-ready from a **data safety** perspective: it survives crashes, recovers efficiently, and enables deterministic replay of agent runs.

**M7 is not about features. M7 is about truth.**

---

## Core Invariants

These invariants are non-negotiable. They define correctness.

### Recovery Invariants

1. **Recovery is deterministic** - Same WAL + Snapshot = Same state
2. **Recovery is idempotent** - Replaying recovery produces identical state
3. **Recovery is prefix-consistent** - No partial transactions visible after recovery
4. **Recovery never invents data** - Only committed data appears
5. **Recovery never drops committed data** - All durable commits survive
6. **Recovery may drop uncommitted data** - Depending on durability mode

### Replay Invariants

1. **Replay is a pure function** over (Snapshot, WAL, EventLog)
2. **Replay is side-effect free** - Does not mutate canonical store
3. **Replay produces a derived view** - Not a new source of truth
4. **Replay does not persist state** unless explicitly materialized
5. **Replay is deterministic** - Same inputs = Same view
6. **Replay is idempotent** - Running twice produces identical view

### Atomic Recovery Invariant

After recovery, the database must correspond to a **prefix of the committed transaction history**.

No partial transactions may be visible. If a transaction spans KV + JSON + Event + State, after crash recovery you must see either all effects or none.

---

## What's Already Done (Context)

### M4 Durability Modes
- **InMemory**: No WAL writes, maximum speed
- **Buffered**: Background WAL writer, batched fsync
- **Strict**: Synchronous fsync on every commit

### Current WAL State
- WAL entries exist for all primitives (KV, Event, State, Trace, Run, JSON)
- JSON has dedicated WAL entries (0x20-0x23)
- WAL replay works for crash recovery
- No WAL truncation or rotation
- No snapshots

### Current Recovery State
- Full WAL replay on startup
- Recovery time = O(total WAL size)
- No partial recovery or checkpoint-based recovery

---

## WAL vs EventLog Responsibility

This separation is critical and must be formalized.

| Layer | Purpose | Stability |
|-------|---------|-----------|
| **WAL** | Physical durability and crash recovery | Implementation may change |
| **EventLog** | Semantic history and replay | Must be stable |
| **Snapshot** | Physical compression of WAL effects | Cache, not truth |
| **Replay** | Semantic reconstruction using EventLog | Derived, not canonical |

**Key Rules:**
- WAL is for crash recovery
- EventLog is for replay and reasoning
- Snapshot compresses WAL effects (it is a cache, not semantic history)
- Replay uses EventLog semantics, accelerated by Snapshot

**A snapshot is a physical checkpoint of materialized state. It is not a semantic history.**

WAL entries may change in representation as long as semantics do not change. EventLog must be stable.

---

## M7 Scope: What We're Building

### 1. Snapshot System

**Goal**: Periodic full-state snapshots that enable bounded recovery time.

**Semantic Clarity**: A snapshot is a **physical compression** of WAL effects. It is not a semantic artifact. It is a byte-level acceleration mechanism, a cache over history.

#### 1.1 Snapshot Format (SEALED)
- [ ] **Single file** - Atomically replaceable, versioned, checksummed
- [ ] Binary format with version header
- [ ] Includes all primitive data (KV, Event, State, Trace, Run, JSON)
- [ ] Snapshot metadata: version, timestamp, WAL offset, CRC32 checksum
- [ ] **No compression** - Correctness milestone, not optimization
- [ ] **No derived data** (indexes rebuilt on recovery)

**Rationale**: Single-file snapshots are simpler to reason about, test, validate, checksum, and atomically replace. Directory-based snapshots only become necessary for incremental snapshots, parallel writes, or multi-terabyte state. None of those apply to M7.

#### 1.2 Snapshot Triggers
- [ ] Size-based: Snapshot when WAL exceeds N bytes (configurable, default 100MB)
- [ ] Time-based: Snapshot every N minutes (configurable, default 30 min)
- [ ] Manual: `db.snapshot()` API for explicit snapshots
- [ ] On shutdown: Optional clean snapshot before exit

#### 1.3 Snapshot Consistency
- [ ] Snapshot is taken at a consistent transaction boundary
- [ ] Uses existing snapshot isolation (ClonedSnapshotView) for consistency
- [ ] No writes blocked during snapshot (copy-on-write semantics)

#### 1.4 WAL Truncation
- [ ] After successful snapshot, truncate WAL entries before snapshot offset
- [ ] Keep configurable number of old snapshots (default 2)
- [ ] Atomic snapshot + truncation (no data loss window)

---

### 2. Crash Recovery

**Goal**: Recover correctly after any crash scenario.

#### 2.1 Recovery Sequence
- [ ] On startup: Find latest valid snapshot
- [ ] Load snapshot into memory
- [ ] Replay WAL entries from snapshot offset to end
- [ ] Validate recovered state via checksums

#### 2.2 WAL Entry Format (SEALED)
Each WAL entry must be **self-validating**:
- [ ] Length (u32)
- [ ] Type (u8)
- [ ] Version (u8)
- [ ] Payload
- [ ] CRC32 checksum

**Rationale**: WAL is the last line of defense against disk corruption, partial writes, torn writes, filesystem bugs, bad SSDs, kernel bugs, and power loss. Without checksums, you replay garbage silently.

#### 2.3 Crash Scenarios to Test
- [ ] Crash during normal operation (WAL has uncommitted entries)
- [ ] Crash during snapshot write (partial snapshot)
- [ ] Crash during WAL truncation (partial truncation)
- [ ] Corrupted WAL entries (checksum validation, skip corrupt entry)
- [ ] Corrupted snapshot (checksum validation, fall back to older snapshot)

#### 2.4 Recovery Time Bounds
- [ ] Recovery time = O(snapshot load) + O(WAL since snapshot)
- [ ] Target: <1 second for typical agent databases (<1GB)
- [ ] Bounded by snapshot frequency configuration

#### 2.5 Durability Mode Integration
- [ ] InMemory mode: No snapshots, no recovery (ephemeral)
- [ ] Buffered mode: Snapshots work, some recent data may be lost
- [ ] Strict mode: Full durability, no data loss

---

### 3. JSON & Cross-Primitive Recovery

**Goal**: All primitives recover correctly, including JSON with its complex patch semantics.

#### 3.1 JSON Recovery
- [ ] JSON documents recovered from snapshot
- [ ] JSON patches (WAL entries 0x20-0x23) replayed in order
- [ ] Patch application is idempotent (same result if replayed twice)
- [ ] Document versions recovered correctly

#### 3.2 Cross-Primitive Atomicity
- [ ] Transactions spanning multiple primitives recover atomically
- [ ] Either all effects visible or none (after recovery)
- [ ] Transaction boundaries respected in WAL replay
- [ ] WAL commit markers or transaction framing required

#### 3.3 Search Index Recovery (SEALED)
- [ ] **Rebuild indexes from recovered data** (not snapshotted)

**Rationale**: Indexes are derived data. Putting derived data into snapshots bloats snapshot size, couples snapshot format to index internals, makes recovery more fragile, and makes schema evolution harder. Startup time can be slower. That is acceptable in M7.

---

### 4. Deterministic Replay

**Goal**: Replay an agent run and get the exact same state.

**Critical Clarification**: Replay is **interpretation**, not mutation. Replay must not write into the canonical store by default. Replay must not become a second source of truth.

#### 4.1 Run-Scoped Replay (SEALED)
- [ ] `db.replay_run(run_id) -> ReadOnlyView`
- [ ] Returns a **read-only derived view**, not a diff
- [ ] Reconstructs all state for that run
- [ ] Returns the state at run completion (or current state if run ongoing)
- [ ] Does not mutate canonical store

**Rationale**: Replay produces a state view. Diff compares two views. Two separate operations. Combining them leads to architectural confusion.

#### 4.2 Efficient Replay
- [ ] Run Index maps runs to **semantic events** (WAL offsets as acceleration)
- [ ] Replay time = O(run size), not O(total WAL size)
- [ ] Skip WAL entries for other runs

#### 4.3 Run Diff (SEALED)
- [ ] `db.diff_runs(run_a, run_b) -> Diff`
- [ ] **Key-level granularity** (not JSON path-level)
- [ ] Shows what changed between two runs
- [ ] Useful for debugging agent behavior

**Rationale**: JSON path-level diff is complex, requires canonicalization, requires stable patch semantics, and explodes surface area. Key-level diff gives simplicity, determinism, performance, and clear semantics. Path-level can be added later as a feature.

#### 4.4 Run Lifecycle
- [ ] `db.begin_run(run_id)` marks run start in WAL/EventLog
- [ ] `db.end_run(run_id)` marks run completion in WAL/EventLog
- [ ] Orphaned runs (no end marker) detected and reported

#### 4.5 Materialization (Future API, Named Now)
Eventually we may want:
- `replay_run(run_id) -> ReadOnlyView`
- `materialize(view) -> canonical state`

This concept is named but **not built in M7**. This avoids confusion between replay and restore.

#### 4.6 What Replay Is NOT

Replay is not:
- Backup
- Point-in-time recovery (PITR)
- Restore
- Migration
- Replication

**It is interpretation.**

#### 4.7 Timestamp-based Replay (DEFERRED)
- **Not in M7**
- Can be derived later using WAL metadata
- Introduces clock semantics, wall clock vs logical time, ambiguity, drift, ordering questions
- No concrete use case yet

---

### 5. Storage Stabilization

**Goal**: Freeze storage APIs for future primitives (Vector in M8).

This is one of the most important parts of M7. If we don't enforce this, M8 will corrupt M7.

#### 5.1 API Freeze (CRITICAL)
- [ ] Document storage engine extension points
- [ ] WAL entry format stable (version header for future changes)
- [ ] Snapshot format stable (version header for future changes)

**After M7, adding a primitive must NOT require changes to:**
- WAL core format
- Snapshot core format
- Recovery engine
- Replay engine

**Only extension points.**

#### 5.2 Extension Points
- [ ] Clear pattern for adding new primitive types
- [ ] WAL entry type registry (0x00-0x1F reserved for core, 0x20+ for primitives)
- [ ] Snapshot section format for new primitives
- [ ] Primitive registry in code for cleaner extension

#### 5.3 Performance Baseline
- [ ] Document M7 performance as baseline
- [ ] Snapshot write time benchmarks
- [ ] Recovery time benchmarks
- [ ] Replay time benchmarks

---

## What's NOT in M7 (Explicitly Out of Scope)

1. **Vector Primitive** - M8
2. **HNSW Index** - M8/M9
3. **Secondary Indexes** - M9
4. **Incremental Snapshots** - Post-MVP
5. **Distributed Snapshots** - Far future
6. **Point-in-Time Recovery (PITR)** - Post-MVP (we have replay instead)
7. **Online Backup** - Post-MVP
8. **Encryption at Rest** - M11

---

## Success Criteria Checklist

### Gate 1: Snapshot System
- [ ] `db.snapshot()` creates valid snapshot
- [ ] Periodic snapshots trigger correctly
- [ ] WAL truncation works after snapshot
- [ ] Snapshot + truncation is atomic

### Gate 2: Crash Recovery
- [ ] All crash scenarios pass tests
- [ ] Recovery time bounded by configuration
- [ ] No data loss in Strict mode

### Gate 3: JSON & Cross-Primitive Recovery
- [ ] JSON documents recover correctly
- [ ] JSON patches replay idempotently
- [ ] Cross-primitive transactions atomic

### Gate 4: Deterministic Replay
- [ ] `replay_run()` returns correct state
- [ ] Replay is O(run size)
- [ ] `diff_runs()` works
- [ ] Run lifecycle fully working

### Gate 5: Storage Stabilization
- [ ] APIs documented and frozen
- [ ] Extension points clear
- [ ] Performance baselines documented

---

## Risks and Mitigations

| Risk | Likelihood | Impact | Mitigation |
|------|------------|--------|------------|
| Snapshot corruption | Medium | High | Checksums, multiple snapshot retention |
| WAL corruption | Low | High | Entry checksums, graceful degradation |
| Replay non-determinism | Medium | Medium | Property-based tests, fuzzing |
| Performance regression | Low | Medium | Benchmarks before/after |
| Scope creep | Medium | Medium | Strict "not in scope" list |

---

## Implementation Order (Suggested)

1. **Snapshot format and write** - Foundation for everything else
2. **Snapshot load and recovery** - Enables testing
3. **WAL truncation** - Enables bounded storage
4. **Crash simulation tests** - Validates correctness
5. **Run replay** - Builds on recovery infrastructure
6. **Run diff** - Nice-to-have, lower priority
7. **Storage API documentation** - Cleanup at the end

---

## Sealed Decisions

All open questions have been resolved:

| Question | Decision | Rationale |
|----------|----------|-----------|
| Snapshot format | **Single file** | Simpler to reason about, test, validate, checksum, atomically replace |
| Compression | **None for M7** | Correctness milestone, not optimization. Add in M9. |
| WAL checksums | **Per-entry CRC32** | WAL is last defense against corruption. Worth the overhead. |
| Index recovery | **Rebuild from data** | Never snapshot derived structures. Indexes are derived. |
| Replay output | **Read-only view** | Replay produces state. Diff compares states. Two operations. |
| Diff granularity | **Key-level** | Path-level is complex, can add later as feature |
| Replay by timestamp | **Defer** | No concrete use case, introduces clock semantics complexity |

---

## Appendix: Current WAL Entry Types

```
0x01 - KV Put
0x02 - KV Delete
0x03 - Event Append
0x04 - State Init
0x05 - State Set
0x06 - State Transition
0x07 - Trace Record
0x08 - Run Create
0x09 - Run Update
0x0A - Run End

0x20 - JSON Create
0x21 - JSON Set
0x22 - JSON Delete
0x23 - JSON Patch
```

---

## Document History

| Version | Date | Changes |
|---------|------|---------|
| 1.0 | 2026-01-17 | Initial draft with open questions |
| 2.0 | 2026-01-17 | **SEALED**: All decisions finalized. Added Core Invariants, WAL vs EventLog separation, Replay Invariants, sealed all open questions. |

---

**Status: SEALED**

This scope document is finalized. Implementation may begin.
