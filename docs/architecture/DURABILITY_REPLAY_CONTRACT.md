# M7 Contract: Durability + Replay + Storage Semantics

**Status**: STABLE - This contract is frozen. Future milestones must not violate these guarantees.

**Purpose**: This document defines the semantic guarantees that all in-mem primitives (current and future) must honor. It answers what "durable" means, what "replay" guarantees, and what API surface is now stable.

---

## Table of Contents

1. [Durability Semantics](#durability-semantics)
2. [Recovery Invariants](#recovery-invariants)
3. [Replay Invariants](#replay-invariants)
4. [Determinism Guarantees](#determinism-guarantees)
5. [Stable API Surface](#stable-api-surface)
6. [Extension Contract](#extension-contract)
7. [Testing Requirements](#testing-requirements)

---

## Durability Semantics

### What Does "Durable" Mean?

A write is **durable** when it will survive a process crash and be visible after recovery. The definition varies by mode:

### InMemory Mode

| Property | Guarantee |
|----------|-----------|
| Durability | **None** - all data lost on crash |
| Visibility | Immediate after operation returns |
| Use case | Tests, ephemeral workloads |
| Performance | <3µs latency, 250K+ ops/sec |

**Contract**: InMemory mode makes NO durability promises. Applications using InMemory mode accept total data loss on crash.

### Buffered Mode

| Property | Guarantee |
|----------|-----------|
| Durability | **Eventual** - durable within flush interval |
| Flush interval | Configurable (default: 100ms) |
| Visibility | Immediate after operation returns |
| Crash loss | Up to one flush interval of data |
| Use case | Production workloads accepting bounded loss |
| Performance | <30µs latency, 50K+ ops/sec |

**Contract**: In Buffered mode, a write becomes durable when:
1. The flush interval elapses and fsync completes, OR
2. An explicit `flush()` is called and returns, OR
3. A snapshot is taken and completes

**What can be lost**: Writes within the last flush interval before crash. This is bounded and predictable.

### Strict Mode

| Property | Guarantee |
|----------|-----------|
| Durability | **Immediate** - durable when operation returns |
| Visibility | Immediate after operation returns |
| Crash loss | None (for completed operations) |
| Use case | Financial, audit, zero-loss requirements |
| Performance | ~2ms latency, ~500 ops/sec |

**Contract**: In Strict mode, when an operation returns successfully:
1. The data is in the WAL
2. The WAL has been fsynced to disk
3. A crash immediately after will recover this write

---

## Recovery Invariants

These six invariants define what recovery MUST guarantee. They are non-negotiable.

### R1: Deterministic

```
Same (Snapshot + WAL) → Same recovered state
```

Recovery is a pure function over its inputs. Given identical snapshot and WAL files, recovery produces byte-identical database state. No randomness, no timestamps, no external input.

**Test**: Run recovery twice on same files, compare resulting state hash.

### R2: Idempotent

```
recover(recover(state)) = recover(state)
```

Running recovery multiple times produces identical results. Recovery can be safely retried.

**Test**: Recover, snapshot, recover from snapshot, compare states.

### R3: Prefix-Consistent

```
Recovered state = some valid prefix of committed operations
```

Recovery never produces a state that could not have existed. If operations A, B, C were committed in order, recovery produces state after A, after B, or after C—never a state with C but not B.

**Test**: Verify no partial transactions visible after recovery.

### R4: Never Invents Data

```
∀ key in recovered_state: key was written by a committed transaction
```

Recovery never creates data that wasn't explicitly written. No default values, no synthesized entries, no placeholders.

**Test**: Recover empty WAL, verify empty state.

### R5: Never Drops Committed Data

```
∀ committed transaction T: effects of T are in recovered state
```

If a transaction committed (returned success in Strict mode, or was flushed in Buffered mode), its effects survive recovery.

**Test**: Write in Strict mode, crash, verify data present.

### R6: May Drop Uncommitted Data

```
Uncommitted transactions may or may not survive
```

This is explicitly permitted, not a bug. In Buffered mode, recent unflushed writes may be lost. Applications must handle this.

**Test**: Write in Buffered mode, crash before flush, accept either outcome.

---

## Replay Invariants

Replay reconstructs the state of a specific run from the EventLog. These six invariants define replay semantics.

### P1: Pure Function

```
replay(run_id) = f(Snapshot, WAL, EventLog)
```

Replay is a pure function over stored data. No external input, no side effects during computation.

### P2: Side-Effect Free

```
replay(run_id) does NOT mutate canonical store
```

Replay is read-only. It constructs a view but never writes to the database. The canonical store is unchanged after replay.

**Critical**: This is the key difference from recovery. Recovery mutates state; replay does not.

### P3: Derived View

```
replay(run_id) is a derived view, not a source of truth
```

The replayed view is computed from EventLog. It is not persisted. It is not authoritative. The EventLog is the source of truth.

### P4: Does Not Persist

```
replay(run_id) produces ephemeral ReadOnlyView
```

The result of replay is a transient in-memory structure. It is not written to disk unless explicitly materialized by the application.

### P5: Deterministic

```
Same inputs → Same replayed view
```

Replaying the same run twice produces identical views. No randomness, no timestamps in replay logic.

### P6: Idempotent

```
replay(replay_inputs) = replay(replay_inputs)
```

Replay can be called multiple times with identical results.

---

## Determinism Guarantees

### What IS Deterministic

| Operation | Determinism | Notes |
|-----------|-------------|-------|
| Recovery | **Yes** | Same inputs → same state |
| Replay | **Yes** | Same run → same view |
| Transaction commit order | **Yes** | WAL order is authoritative |
| WAL entry ordering | **Yes** | Sequential, no reordering |
| Snapshot contents | **Yes** | Point-in-time consistent |
| CRC32 checksums | **Yes** | Computed identically |
| Key ordering | **Yes** | Lexicographic, stable |

### What is NOT Deterministic

| Operation | Why | Implication |
|-----------|-----|-------------|
| `RunId::new()` | Uses UUID/random | Store run_id if you need to reference it later |
| `TxId::new()` | Uses UUID/random | Internal use only, not exposed |
| Timestamps | Wall clock | Use for display, not ordering |
| Crash timing | External | Buffered mode may lose recent writes |
| Flush timing | Background thread | Don't depend on exact flush moment |
| Snapshot timing | Configurable triggers | Don't depend on exact snapshot moment |

### Determinism Boundary

```
┌─────────────────────────────────────────────────────────┐
│                    DETERMINISTIC                        │
│  ┌─────────────────────────────────────────────────┐   │
│  │  Recovery: f(Snapshot, WAL) → State             │   │
│  │  Replay: f(EventLog) → View                     │   │
│  │  Transaction: f(Operations) → Commit/Abort      │   │
│  └─────────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────┘
                          │
                          │ ID generation, timestamps,
                          │ crash timing, flush timing
                          ▼
┌─────────────────────────────────────────────────────────┐
│                  NON-DETERMINISTIC                      │
│  ┌─────────────────────────────────────────────────┐   │
│  │  RunId::new(), TxId::new()                      │   │
│  │  SystemTime::now()                               │   │
│  │  Crash occurrence                                │   │
│  │  Background flush timing                         │   │
│  └─────────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────┘
```

---

## Stable API Surface

These APIs are **frozen**. Future milestones must not change their signatures or semantics.

### Core Types (STABLE)

```rust
// These types are stable and must not change
pub struct RunId { /* opaque */ }
pub struct TxId { /* opaque */ }
pub enum Value { Null, Bool(bool), I64(i64), F64(f64), String(String), Bytes(Vec<u8>) }
pub struct Key { /* opaque */ }
```

### Database Lifecycle (STABLE)

```rust
impl Database {
    // Opening
    pub fn builder() -> DatabaseBuilder;
    pub fn open(path: &Path) -> Result<Database>;
    pub fn open_in_memory() -> Result<Database>;

    // Run lifecycle
    pub fn begin_run(&self, run_id: RunId) -> Result<()>;
    pub fn end_run(&self, run_id: RunId) -> Result<()>;
    pub fn abort_run(&self, run_id: RunId, reason: &str) -> Result<()>;
    pub fn run_status(&self, run_id: RunId) -> Result<RunStatus>;
    pub fn orphaned_runs(&self) -> Result<Vec<RunId>>;

    // Replay
    pub fn replay_run(&self, run_id: RunId) -> Result<ReadOnlyView>;
    pub fn diff_runs(&self, run_a: RunId, run_b: RunId) -> Result<RunDiff>;

    // Snapshots
    pub fn snapshot(&self) -> Result<SnapshotInfo>;
    pub fn configure_snapshots(&self, config: SnapshotConfig);

    // Recovery info
    pub fn last_recovery_result(&self) -> Option<&RecoveryResult>;
}
```

### Primitive Operations (STABLE)

```rust
// KVStore
pub fn get(&self, run_id: &RunId, key: &str) -> Result<Option<Value>>;
pub fn put(&self, run_id: &RunId, key: &str, value: Value) -> Result<()>;
pub fn delete(&self, run_id: &RunId, key: &str) -> Result<()>;
pub fn list(&self, run_id: &RunId, prefix: Option<&str>) -> Result<Vec<String>>;

// EventLog
pub fn append(&self, run_id: &RunId, event_type: &str, payload: Value) -> Result<u64>;
pub fn read(&self, run_id: &RunId, sequence: u64) -> Result<Option<Event>>;
pub fn range(&self, run_id: &RunId, start: u64, end: u64) -> Result<Vec<Event>>;

// StateCell
pub fn get(&self, run_id: &RunId, name: &str) -> Result<Option<StateValue>>;
pub fn set(&self, run_id: &RunId, name: &str, value: Value) -> Result<u64>;
pub fn cas(&self, run_id: &RunId, name: &str, expected: u64, value: Value) -> Result<u64>;

// TraceStore
pub fn record(&self, run_id: &RunId, trace_type: &str, metadata: Value) -> Result<String>;
pub fn get(&self, run_id: &RunId, trace_id: &str) -> Result<Option<Span>>;

// JsonStore
pub fn create(&self, run_id: &RunId, key: &str, doc: JsonValue) -> Result<()>;
pub fn get(&self, run_id: &RunId, key: &str) -> Result<Option<JsonDoc>>;
pub fn set(&self, run_id: &RunId, key: &str, path: &str, value: JsonValue) -> Result<()>;
pub fn delete(&self, run_id: &RunId, key: &str) -> Result<()>;
```

### WAL Entry Types (STABLE)

```rust
#[repr(u8)]
pub enum WalEntryType {
    // Core (0x00-0x0F) - FROZEN
    TransactionCommit = 0x00,
    TransactionAbort = 0x01,
    SnapshotMarker = 0x02,

    // KV (0x10-0x1F) - FROZEN
    KvPut = 0x10,
    KvDelete = 0x11,

    // JSON (0x20-0x2F) - FROZEN
    JsonCreate = 0x20,
    JsonSet = 0x21,
    JsonDelete = 0x22,
    JsonPatch = 0x23,

    // Event (0x30-0x3F) - FROZEN
    EventAppend = 0x30,

    // State (0x40-0x4F) - FROZEN
    StateInit = 0x40,
    StateSet = 0x41,
    StateTransition = 0x42,

    // Trace (0x50-0x5F) - FROZEN
    TraceRecord = 0x50,

    // Run (0x60-0x6F) - FROZEN
    RunCreate = 0x60,
    RunUpdate = 0x61,
    RunEnd = 0x62,
    RunBegin = 0x63,

    // Vector (0x70-0x7F) - RESERVED for M8
    // Future (0x80-0xFF) - RESERVED
}
```

### Snapshot Format (STABLE)

```
+------------------+
| Magic (10 bytes) |  "INMEM_SNAP" - FROZEN
+------------------+
| Version (4)      |  Format version - FROZEN structure
+------------------+
| Timestamp (8)    |  Microseconds since epoch
+------------------+
| WAL Offset (8)   |  WAL position covered
+------------------+
| Primitive Data   |  Per-primitive sections
+------------------+
| CRC32 (4)        |  Checksum
+------------------+
```

---

## Extension Contract

Future primitives (Vector in M8, others later) must follow this contract.

### Requirements for New Primitives

1. **WAL Entry Types**: Use assigned range (Vector: 0x70-0x7F)
2. **Implement PrimitiveStorageExt**:
   ```rust
   pub trait PrimitiveStorageExt {
       fn wal_entry_types(&self) -> &'static [u8];
       fn snapshot_serialize(&self) -> Result<Vec<u8>>;
       fn snapshot_deserialize(&mut self, data: &[u8]) -> Result<()>;
       fn apply_wal_entry(&mut self, entry: &WalEntry) -> Result<()>;
       fn primitive_type_id(&self) -> u8;
   }
   ```
3. **Recovery Compliance**: Must honor R1-R6 invariants
4. **Replay Compliance**: Must honor P1-P6 invariants if participating in replay
5. **Transaction Compliance**: Must integrate with OCC if transactional

### What New Primitives Must NOT Do

- Change existing WAL entry type values
- Change snapshot magic bytes or header format
- Break recovery of existing primitives
- Violate determinism guarantees
- Add non-deterministic behavior to replay

### What New Primitives MAY Do

- Add new WAL entry types in their assigned range
- Add new snapshot sections (with new primitive_type_id)
- Add new API methods
- Add new configuration options

---

## Testing Requirements

Every primitive (existing and future) must pass these test categories:

### Recovery Tests (Mandatory)

```
□ R1: Recovery is deterministic (same inputs → same state)
□ R2: Recovery is idempotent (recover twice → same result)
□ R3: Recovery is prefix-consistent (no partial transactions)
□ R4: Recovery never invents data
□ R5: Recovery never drops committed data (Strict mode)
□ R6: Recovery may drop uncommitted data (Buffered mode)
```

### Replay Tests (If Applicable)

```
□ P1: Replay is pure function
□ P2: Replay is side-effect free
□ P3: Replay produces derived view
□ P4: Replay does not persist
□ P5: Replay is deterministic
□ P6: Replay is idempotent
```

### Crash Tests (Mandatory)

```
□ Crash during write recovers correctly
□ Crash during transaction recovers atomically
□ Crash during snapshot recovers correctly
□ Corrupt WAL entry detected and handled
□ Truncated WAL handled gracefully
```

### Cross-Primitive Tests (If Transactional)

```
□ Atomic commit with other primitives
□ Atomic abort with other primitives
□ Recovery respects transaction boundaries
```

---

## Summary

This contract establishes:

1. **Durability**: Defined per mode (InMemory/Buffered/Strict)
2. **Recovery**: Six invariants (R1-R6) that are non-negotiable
3. **Replay**: Six invariants (P1-P6) for run reconstruction
4. **Determinism**: Clear boundary between deterministic and non-deterministic
5. **Stability**: Frozen API surface that must not break
6. **Extension**: Contract for adding new primitives

**This document is the foundation. Every future primitive depends on these guarantees.**

---

## Document History

| Version | Date | Changes |
|---------|------|---------|
| 1.0 | 2026-01-17 | Initial contract established after M7 completion |
