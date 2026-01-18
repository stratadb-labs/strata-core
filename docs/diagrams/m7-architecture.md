# M7 Architecture Diagrams: Durability, Snapshots, Replay & Storage Stabilization

This document contains visual representations of the M7 architecture focused on crash recovery, deterministic replay, WAL enhancement, and storage API freeze.

**Architecture Spec Version**: 1.0

---

## Semantic Invariants (Reference)

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                         M7 SEMANTIC INVARIANTS                               │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│  RECOVERY INVARIANTS (R1-R6)                                                │
│  ───────────────────────────                                                │
│  R1. DETERMINISTIC         Same WAL → same state every replay              │
│  R2. IDEMPOTENT            replay(replay(S,WAL),WAL) = replay(S,WAL)       │
│  R3. PREFIX-CONSISTENT     Recover prefix of committed transactions        │
│  R4. NEVER INVENTS DATA    Only data explicitly written appears            │
│  R5. NEVER DROPS COMMITTED Committed data survives any single crash        │
│  R6. MAY DROP UNCOMMITTED  Incomplete transactions may vanish              │
│                                                                             │
│  REPLAY INVARIANTS (P1-P6)                                                  │
│  ─────────────────────────                                                  │
│  P1. PURE FUNCTION         fn(run_id, event_log) → ReadOnlyView            │
│  P2. SIDE-EFFECT FREE      Does NOT mutate any persistent state            │
│  P3. DERIVED VIEW          Computes view, does NOT reconstruct state       │
│  P4. DOES NOT PERSIST      Result is ephemeral, discarded after use        │
│  P5. DETERMINISTIC         Same inputs → identical view                    │
│  P6. IDEMPOTENT            Safe to call multiple times                     │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

---

## 1. System Architecture Overview (M7)

```
+-------------------------------------------------------------------------+
|                           Application Layer                              |
|                      (Agent Applications using DB)                       |
+-----------------------------------+-------------------------------------+
                                    |
                                    | High-level typed APIs
                                    v
+-------------------------------------------------------------------------+
|                          Primitives Layer (M3-M6)                        |
|                          (Stateless Facades)                             |
|                                                                          |
|  +-------------+  +-------------+  +--------------+  +-------------+    |
|  |  KV Store   |  |  Event Log  |  |  StateCell   |  |Trace Store  |    |
|  +------+------+  +------+------+  +------+-------+  +------+------+    |
|         |                |                |                |            |
|         +----------------+-------+--------+----------------+            |
|                                  |                                      |
|  +---------------------------+   |   +-----------------------------+   |
|  |        Run Index          |   |   |      JSON Store (M5)        |   |
|  +-------------+-------------+   |   +-------------+---------------+   |
|                |                 |                 |                    |
+----------------+-----------------+-----------------+--------------------+
                                   |
                                   | Database transaction API
                                   v
+-------------------------------------------------------------------------+
|                         Engine Layer (M1-M7)                             |
|                   (Orchestration & Coordination)                         |
|                                                                          |
|  +-------------------------------------------------------------------+  |
|  |                          Database                                  |  |
|  |                                                                    |  |
|  |  M7 NEW: Durability & Recovery                                    |  |
|  |  +-------------------------------------------------------------+  |  |
|  |  |                   RecoveryEngine                             |  |  |
|  |  |  - discover_snapshots()                                      |  |  |
|  |  |  - recover_from_crash()                                      |  |  |
|  |  |  - replay_wal_from_snapshot()                                |  |  |
|  |  |  - validate_recovery()                                       |  |  |
|  |  +-------------------------------------------------------------+  |  |
|  |                                                                    |  |
|  |  M7 NEW: Run Replay                                               |  |
|  |  +-------------------------------------------------------------+  |  |
|  |  |                   ReplayEngine                               |  |  |
|  |  |  - replay_run(run_id) → ReadOnlyView                         |  |  |
|  |  |  - diff_runs(run_a, run_b) → RunDiff                         |  |  |
|  |  |  - detect_orphaned_runs()                                    |  |  |
|  |  +-------------------------------------------------------------+  |  |
|  |                                                                    |  |
|  |  M7 NEW: Snapshot System                                          |  |
|  |  +-------------------------------------------------------------+  |  |
|  |  |                 SnapshotManager                              |  |  |
|  |  |  - create_snapshot() → SnapshotHandle                        |  |  |
|  |  |  - load_snapshot(path) → Snapshot                            |  |  |
|  |  |  - validate_snapshot(snapshot)                               |  |  |
|  |  +-------------------------------------------------------------+  |  |
|  |                                                                    |  |
|  +-------------------------------------------------------------------+  |
|                               |                                          |
+----------+-------------------+-------------------+-----------------------+
           |                   |                   |
           v                   v                   v
+------------------+  +-------------------+  +------------------------+
|  Storage (M4+M7) |  | Durability (M4+M7)|  | Concurrency (M4)       |
|                  |  |                   |  |                        |
| M7 NEW:          |  | M7 NEW:           |  | - Transaction Pooling  |
| -PrimitiveStorage|  | - WAL Envelope    |  | - Read Fast Path       |
|   Ext trait      |  | - CRC32 validation|  | - OCC Validation       |
| -Primitive       |  | - Tx framing      |  |                        |
|   Registry       |  | - Entry types     |  |                        |
+------------------+  +-------------------+  +------------------------+
           |                   |                   |
           +-------------------+-------------------+
                               |
                               v
+-------------------------------------------------------------------------+
|                         Core Types Layer (M1 + M7)                       |
|                       (Foundation Definitions)                           |
|                                                                          |
|  M7 NEW Types:                                                           |
|  - SnapshotEnvelope   (magic, version, header, primitive_blobs, CRC32)  |
|  - WalEntry           (length, type, version, tx_id, payload, CRC32)    |
|  - TxId               (run_id, sequence)                                |
|  - RecoveryOptions    (strategy, validation, snapshot_dir)              |
|  - RunStatus          (Active, Completed, Failed, Orphaned)             |
|  - ReadOnlyView       (computed view from replay)                       |
|  - RunDiff            (delta between two run states)                    |
+-------------------------------------------------------------------------+
```

---

## 2. Recovery Model

```
+-------------------------------------------------------------------------+
|                        Recovery Model (M7)                               |
+-------------------------------------------------------------------------+

Conceptual Framing:
===================

    ┌─────────────────────────────────────────────────────────────────────┐
    │                                                                     │
    │  Recovery reconstructs COMMITTED transaction history.               │
    │                                                                     │
    │  Key Distinction:                                                   │
    │  - WAL is for CRASH RECOVERY (physical, may change format)         │
    │  - EventLog is for SEMANTIC HISTORY (stable, user-visible)         │
    │                                                                     │
    │  Snapshot is a PHYSICAL COMPRESSION of WAL effects.                 │
    │  It is a cache over history, not semantic truth.                   │
    │                                                                     │
    │  If snapshot is corrupt: discard and replay from WAL.              │
    │  If WAL is corrupt: truncate to last valid transaction boundary.   │
    │                                                                     │
    └─────────────────────────────────────────────────────────────────────┘


Recovery Sequence:
==================

    ┌─────────────────────────────────────────────────────────────────────┐
    │                                                                     │
    │  1. DISCOVER      Find snapshot files, validate headers            │
    │  2. LOAD          Load most recent valid snapshot                  │
    │  3. REPLAY        Apply WAL entries from snapshot point            │
    │  4. VALIDATE      Verify invariants hold                           │
    │  5. READY         Database ready for operations                    │
    │                                                                     │
    │  On any step failure: fall back or fail explicitly                 │
    │                                                                     │
    └─────────────────────────────────────────────────────────────────────┘


    Startup                Recovery Engine              Snapshot           WAL
        │                        │                         │               │
        │ recover_from_crash()   │                         │               │
        ├───────────────────────►│                         │               │
        │                        │                         │               │
        │                        │ 1. discover_snapshots() │               │
        │                        ├────────────────────────►│               │
        │                        │◄────────────────────────┤               │
        │                        │   Vec<SnapshotMetadata> │               │
        │                        │                         │               │
        │                        │ 2. validate & load      │               │
        │                        │    most recent valid    │               │
        │                        ├────────────────────────►│               │
        │                        │◄────────────────────────┤               │
        │                        │   Snapshot(wal_offset)  │               │
        │                        │                         │               │
        │                        │ 3. replay from offset   │               │
        │                        ├─────────────────────────────────────────►│
        │                        │◄─────────────────────────────────────────┤
        │                        │   WAL entries           │               │
        │                        │                         │               │
        │                        │ 4. validate invariants  │               │
        │                        │                         │               │
        │◄───────────────────────┤                         │               │
        │  Database ready        │                         │               │


Fallback Cascade:
=================

    ┌─────────────────────────────────────────────────────────────────────┐
    │                                                                     │
    │                    Try most recent snapshot                         │
    │                            │                                        │
    │              ┌─────────────┴─────────────┐                         │
    │              │                           │                          │
    │           SUCCESS                     CORRUPT                       │
    │              │                           │                          │
    │              ▼                           ▼                          │
    │     Replay WAL from              Try older snapshot                │
    │     snapshot offset                      │                          │
    │              │                ┌──────────┴──────────┐              │
    │              │              SUCCESS              NO MORE            │
    │              │                │                     │               │
    │              │                ▼                     ▼               │
    │              │       Replay from older      Full WAL replay         │
    │              │       snapshot               (no snapshot)           │
    │              │                                     │                │
    │              └──────────────┬─────────────────────┘                │
    │                             │                                       │
    │                             ▼                                       │
    │                      Validate state                                │
    │                             │                                       │
    │              ┌──────────────┴──────────────┐                       │
    │            VALID                        INVALID                    │
    │              │                              │                       │
    │              ▼                              ▼                       │
    │         DB Ready                    Truncate to last               │
    │                                     valid tx boundary              │
    │                                                                     │
    └─────────────────────────────────────────────────────────────────────┘
```

---

## 3. Snapshot System

```
+-------------------------------------------------------------------------+
|                       Snapshot System (M7)                               |
+-------------------------------------------------------------------------+

Snapshot File Format:
=====================

    ┌─────────────────────────────────────────────────────────────────────┐
    │                    SNAPSHOT FILE LAYOUT                              │
    ├─────────────────────────────────────────────────────────────────────┤
    │                                                                     │
    │  ┌────────────────────────────────────────────────────────────┐    │
    │  │ MAGIC NUMBER (8 bytes)                                     │    │
    │  │ "INMEMSNP"                                                 │    │
    │  ├────────────────────────────────────────────────────────────┤    │
    │  │ VERSION (4 bytes)                                          │    │
    │  │ Format version for forward compatibility                   │    │
    │  ├────────────────────────────────────────────────────────────┤    │
    │  │ HEADER (variable)                                          │    │
    │  │ - created_at: u64 (microseconds)                          │    │
    │  │ - wal_offset: u64 (replay point)                          │    │
    │  │ - tx_id: TxId (last included transaction)                 │    │
    │  │ - primitive_count: u32                                     │    │
    │  │ - flags: u32                                               │    │
    │  ├────────────────────────────────────────────────────────────┤    │
    │  │ PRIMITIVE BLOBS (variable per primitive)                   │    │
    │  │ ┌──────────────────────────────────────────────────────┐  │    │
    │  │ │ Primitive Type (1 byte): 0x10=KV, 0x20=JSON, etc.   │  │    │
    │  │ │ Blob Length (4 bytes)                                │  │    │
    │  │ │ Serialized Primitive State (variable)                │  │    │
    │  │ │ Blob CRC32 (4 bytes)                                 │  │    │
    │  │ └──────────────────────────────────────────────────────┘  │    │
    │  │ ... repeated for each primitive ...                       │    │
    │  ├────────────────────────────────────────────────────────────┤    │
    │  │ FOOTER CRC32 (4 bytes)                                    │    │
    │  │ Covers magic + version + header + all blobs               │    │
    │  └────────────────────────────────────────────────────────────┘    │
    │                                                                     │
    └─────────────────────────────────────────────────────────────────────┘


Snapshot Semantics:
===================

    ┌─────────────────────────────────────────────────────────────────────┐
    │                                                                     │
    │  SNAPSHOT IS A CACHE                                                │
    │  ─────────────────────                                              │
    │                                                                     │
    │  A snapshot is a PHYSICAL COMPRESSION of WAL effects.               │
    │                                                                     │
    │  It contains the computed state at a point in WAL history.          │
    │  It does NOT contain semantic history or transaction log.           │
    │                                                                     │
    │  Properties:                                                        │
    │  - DISPOSABLE: Can be deleted; WAL can rebuild state                │
    │  - ACCELERATOR: Speeds up recovery by skipping WAL prefix           │
    │  - POINT-IN-TIME: Represents state at specific wal_offset           │
    │  - ATOMIC: Either fully valid or completely discarded               │
    │                                                                     │
    └─────────────────────────────────────────────────────────────────────┘


Atomic Write Protocol:
======================

    ┌─────────────────────────────────────────────────────────────────────┐
    │                                                                     │
    │  1. WRITE TEMP     Write to snapshot.tmp                           │
    │  2. FSYNC          Force to disk                                   │
    │  3. RENAME         Atomic rename to snapshot-{timestamp}.snap      │
    │  4. FSYNC DIR      Sync directory entry                            │
    │                                                                     │
    │  On crash during write: .tmp file is ignored on recovery           │
    │                                                                     │
    └─────────────────────────────────────────────────────────────────────┘

    Timeline:

    T1: Start snapshot write
        ┌─────────────────────┐
        │ snapshot.tmp        │ ← Writing in progress
        └─────────────────────┘

    T2: Crash during write
        ┌─────────────────────┐
        │ snapshot.tmp        │ ← Incomplete, IGNORED on recovery
        └─────────────────────┘

    T3: Successful complete write
        ┌─────────────────────────────────┐
        │ snapshot-1705123456789012.snap  │ ← Valid, used for recovery
        └─────────────────────────────────┘


Snapshot Discovery:
===================

    fn discover_snapshots(dir: &Path) -> Vec<SnapshotMetadata> {
        // 1. List all .snap files
        // 2. Parse metadata from each
        // 3. Sort by wal_offset (descending)
        // 4. Return ordered list
    }

    ┌─────────────────────────────────────────────────────────────────────┐
    │                                                                     │
    │  Directory: /data/snapshots/                                        │
    │                                                                     │
    │  snapshot-1705123400000000.snap  (wal_offset: 1000)                │
    │  snapshot-1705123450000000.snap  (wal_offset: 1500)  ← Try first   │
    │  snapshot-1705123300000000.snap  (wal_offset: 500)                 │
    │  snapshot.tmp                    (ignored)                          │
    │                                                                     │
    └─────────────────────────────────────────────────────────────────────┘
```

---

## 4. WAL Entry Format

```
+-------------------------------------------------------------------------+
|                       WAL Entry Format (M7)                              |
+-------------------------------------------------------------------------+

Entry Structure:
================

    ┌─────────────────────────────────────────────────────────────────────┐
    │                     WAL ENTRY LAYOUT                                 │
    ├─────────────────────────────────────────────────────────────────────┤
    │                                                                     │
    │  ┌────────────────────────────────────────────────────────────┐    │
    │  │ Length (4 bytes, u32)                                      │    │
    │  │ Total length of entry including this field                 │    │
    │  ├────────────────────────────────────────────────────────────┤    │
    │  │ Type (1 byte, u8)                                          │    │
    │  │ Entry type from registry                                   │    │
    │  ├────────────────────────────────────────────────────────────┤    │
    │  │ Version (1 byte, u8)                                       │    │
    │  │ Entry format version                                       │    │
    │  ├────────────────────────────────────────────────────────────┤    │
    │  │ TxId (16 bytes)                                            │    │
    │  │ - run_id: u64                                              │    │
    │  │ - sequence: u64                                            │    │
    │  ├────────────────────────────────────────────────────────────┤    │
    │  │ Payload (variable)                                         │    │
    │  │ Operation-specific data                                    │    │
    │  ├────────────────────────────────────────────────────────────┤    │
    │  │ CRC32 (4 bytes)                                            │    │
    │  │ Covers type + version + tx_id + payload                    │    │
    │  └────────────────────────────────────────────────────────────┘    │
    │                                                                     │
    │  Total: 4 + 1 + 1 + 16 + payload_len + 4 = 26 + payload_len        │
    │                                                                     │
    └─────────────────────────────────────────────────────────────────────┘


Entry Type Registry:
====================

    ┌─────────────────────────────────────────────────────────────────────┐
    │                     WAL ENTRY TYPE REGISTRY                          │
    ├─────────────────────────────────────────────────────────────────────┤
    │                                                                     │
    │  Range         Primitive        Types                              │
    │  ─────────────────────────────────────────────────────────────     │
    │  0x00-0x0F     Core             TxBegin, TxCommit, TxAbort,        │
    │                                 Checkpoint, Noop                    │
    │                                                                     │
    │  0x10-0x1F     KV Store         KvPut, KvDelete, KvClear           │
    │                                                                     │
    │  0x20-0x2F     JSON Store       JsonCreate, JsonSet, JsonDelete,   │
    │                                 JsonPatch                           │
    │                                                                     │
    │  0x30-0x3F     Event Log        EventAppend, EventTruncate         │
    │                                                                     │
    │  0x40-0x4F     StateCell        StateInit, StateSet, StateCas      │
    │                                                                     │
    │  0x50-0x5F     Trace Store      TraceRecord, TraceEndSpan          │
    │                                                                     │
    │  0x60-0x6F     Run Index        RunBegin, RunEnd, RunUpdate        │
    │                                                                     │
    │  0x70-0x7F     Vector (M8)      RESERVED                           │
    │                                                                     │
    │  0x80-0xFF     Future           RESERVED for new primitives        │
    │                                                                     │
    └─────────────────────────────────────────────────────────────────────┘


Transaction Framing:
====================

    ┌─────────────────────────────────────────────────────────────────────┐
    │                                                                     │
    │  Every transaction is framed:                                       │
    │                                                                     │
    │  ┌──────────┐  ┌──────────┐  ┌──────────┐  ┌──────────┐           │
    │  │ TxBegin  │──│ Op1      │──│ Op2      │──│ TxCommit │           │
    │  │ (0x01)   │  │ (KvPut)  │  │ (KvPut)  │  │ (0x02)   │           │
    │  └──────────┘  └──────────┘  └──────────┘  └──────────┘           │
    │       │                                          │                  │
    │       └────────── Same TxId ─────────────────────┘                 │
    │                                                                     │
    │                                                                     │
    │  On recovery:                                                       │
    │  - TxBegin without TxCommit → transaction is DISCARDED             │
    │  - TxCommit without TxBegin → CORRUPT, truncate WAL                │
    │  - Complete TxBegin...TxCommit → transaction is REPLAYED           │
    │                                                                     │
    └─────────────────────────────────────────────────────────────────────┘


CRC32 Validation:
=================

    ┌─────────────────────────────────────────────────────────────────────┐
    │                                                                     │
    │  EVERY entry is self-validating:                                   │
    │                                                                     │
    │  fn validate_entry(entry: &WalEntry) -> Result<()> {               │
    │      let computed = crc32(&entry.type_version_txid_payload);       │
    │      if computed != entry.crc32 {                                  │
    │          return Err(WalError::CorruptEntry);                       │
    │      }                                                              │
    │      Ok(())                                                         │
    │  }                                                                  │
    │                                                                     │
    │  On corrupt entry during recovery:                                  │
    │  → Truncate WAL to previous valid transaction boundary             │
    │  → Log warning                                                      │
    │  → Continue recovery                                                │
    │                                                                     │
    └─────────────────────────────────────────────────────────────────────┘
```

---

## 5. Run Lifecycle & Replay

```
+-------------------------------------------------------------------------+
|                     Run Lifecycle & Replay (M7)                          |
+-------------------------------------------------------------------------+

Run States:
===========

    ┌─────────────────────────────────────────────────────────────────────┐
    │                                                                     │
    │                        ┌──────────┐                                │
    │                        │  Active  │                                │
    │                        └────┬─────┘                                │
    │                             │                                       │
    │              ┌──────────────┼──────────────┐                       │
    │              │              │              │                        │
    │              ▼              ▼              ▼                        │
    │        ┌──────────┐  ┌──────────┐  ┌──────────┐                   │
    │        │Completed │  │  Failed  │  │ Orphaned │                   │
    │        └──────────┘  └──────────┘  └──────────┘                   │
    │                                                                     │
    │  Active:    Run is in progress, accepting operations               │
    │  Completed: Run ended successfully via end_run()                   │
    │  Failed:    Run ended with error via end_run(err)                  │
    │  Orphaned:  Run was Active when crash occurred (detected on        │
    │             recovery)                                               │
    │                                                                     │
    └─────────────────────────────────────────────────────────────────────┘


Run Operations:
===============

    Application              Run Index                    WAL
        │                        │                         │
        │ begin_run(metadata)    │                         │
        ├───────────────────────►│                         │
        │                        │ Write RunBegin entry    │
        │                        ├────────────────────────►│
        │◄───────────────────────┤                         │
        │   RunId                │                         │
        │                        │                         │
        │ ... operations ...     │                         │
        │                        │                         │
        │ end_run(run_id, status)│                         │
        ├───────────────────────►│                         │
        │                        │ Write RunEnd entry      │
        │                        ├────────────────────────►│
        │◄───────────────────────┤                         │
        │   Ok(())               │                         │


Replay (Pure Function):
=======================

    ┌─────────────────────────────────────────────────────────────────────┐
    │                                                                     │
    │  fn replay_run(run_id: RunId, event_log: &EventLog) -> ReadOnlyView│
    │                                                                     │
    │  This is a PURE FUNCTION:                                          │
    │  - Takes run_id and event log                                      │
    │  - Returns a read-only computed view                               │
    │  - Does NOT mutate any state                                       │
    │  - Does NOT persist anything                                       │
    │  - Result is ephemeral                                             │
    │                                                                     │
    └─────────────────────────────────────────────────────────────────────┘

    Replay Flow:
    ============

    Application              Replay Engine              Event Log
        │                        │                         │
        │ replay_run(run_id)     │                         │
        ├───────────────────────►│                         │
        │                        │                         │
        │                        │ Filter events by run_id │
        │                        ├────────────────────────►│
        │                        │◄────────────────────────┤
        │                        │   Events for run        │
        │                        │                         │
        │                        │ Apply events to         │
        │                        │ in-memory view          │
        │                        │ (NO writes)             │
        │                        │                         │
        │◄───────────────────────┤                         │
        │   ReadOnlyView         │                         │
        │   (ephemeral)          │                         │


    ┌─────────────────────────────────────────────────────────────────────┐
    │                                                                     │
    │  ReadOnlyView {                                                     │
    │    run_id: RunId,                                                  │
    │    kv_state: HashMap<Key, Value>,      // Computed KV state        │
    │    json_state: HashMap<Key, JsonDoc>,  // Computed JSON state      │
    │    state_cells: HashMap<Key, State>,   // Computed StateCell state │
    │    events: Vec<Event>,                 // Events in this run       │
    │    traces: Vec<Span>,                  // Traces in this run       │
    │  }                                                                  │
    │                                                                     │
    │  WARNING: This view is READ-ONLY and EPHEMERAL.                    │
    │  It exists only in memory during the replay_run() call.            │
    │  Once returned, it can be queried but NOT persisted.               │
    │                                                                     │
    └─────────────────────────────────────────────────────────────────────┘


Diff Runs:
==========

    ┌─────────────────────────────────────────────────────────────────────┐
    │                                                                     │
    │  fn diff_runs(run_a: RunId, run_b: RunId, event_log: &EventLog)    │
    │      -> RunDiff                                                     │
    │                                                                     │
    │  Compares two runs by replaying both and computing delta:          │
    │                                                                     │
    │  RunDiff {                                                          │
    │    added_keys: Vec<Key>,                                           │
    │    removed_keys: Vec<Key>,                                         │
    │    modified_keys: Vec<(Key, OldValue, NewValue)>,                  │
    │    event_count_a: usize,                                           │
    │    event_count_b: usize,                                           │
    │  }                                                                  │
    │                                                                     │
    └─────────────────────────────────────────────────────────────────────┘


Orphaned Run Detection:
=======================

    ┌─────────────────────────────────────────────────────────────────────┐
    │                                                                     │
    │  On recovery, detect runs that were Active when crash occurred:    │
    │                                                                     │
    │  fn detect_orphaned_runs() -> Vec<RunId> {                         │
    │      run_index.iter()                                              │
    │          .filter(|r| r.status == RunStatus::Active)                │
    │          .filter(|r| r.start_time < crash_time)                    │
    │          .map(|r| r.run_id)                                        │
    │          .collect()                                                 │
    │  }                                                                  │
    │                                                                     │
    │  Orphaned runs are marked as Orphaned (not Failed) because:        │
    │  - They may have been successful but just not ended                │
    │  - Application can decide how to handle them                       │
    │  - Replay can still reconstruct their state                        │
    │                                                                     │
    └─────────────────────────────────────────────────────────────────────┘
```

---

## 6. Cross-Primitive Atomicity

```
+-------------------------------------------------------------------------+
|                   Cross-Primitive Atomicity (M7)                         |
+-------------------------------------------------------------------------+

Transaction Grouping:
=====================

    ┌─────────────────────────────────────────────────────────────────────┐
    │                                                                     │
    │  A single transaction may write to MULTIPLE primitives.             │
    │                                                                     │
    │  All writes share the SAME TxId:                                   │
    │                                                                     │
    │  ┌──────────┐                                                      │
    │  │ TxBegin  │  TxId = (run_123, seq_456)                          │
    │  └────┬─────┘                                                      │
    │       │                                                             │
    │       ▼                                                             │
    │  ┌──────────┐  ┌──────────┐  ┌──────────┐  ┌──────────┐           │
    │  │ KvPut    │  │ JsonSet  │  │ EventApp │  │StateInit │           │
    │  │ key1=v1  │  │ doc1={}  │  │ event1   │  │ cell1=0  │           │
    │  └────┬─────┘  └────┬─────┘  └────┬─────┘  └────┬─────┘           │
    │       │              │              │              │                │
    │       └──────────────┴──────────────┴──────────────┘               │
    │                              │                                      │
    │                              ▼                                      │
    │                        ┌──────────┐                                │
    │                        │ TxCommit │                                │
    │                        └──────────┘                                │
    │                                                                     │
    │  All or nothing: Either ALL writes commit or NONE do.              │
    │                                                                     │
    └─────────────────────────────────────────────────────────────────────┘


Atomic Commit:
==============

    ┌─────────────────────────────────────────────────────────────────────┐
    │                                                                     │
    │  fn commit_transaction(tx: Transaction) -> Result<()> {            │
    │      // 1. Write TxBegin                                           │
    │      wal.append(WalEntry::TxBegin { tx_id })?;                     │
    │                                                                     │
    │      // 2. Write all primitive operations (same tx_id)             │
    │      for op in tx.operations {                                     │
    │          wal.append(op.to_wal_entry(tx_id))?;                      │
    │      }                                                              │
    │                                                                     │
    │      // 3. Write TxCommit (commit point)                           │
    │      wal.append(WalEntry::TxCommit { tx_id })?;                    │
    │                                                                     │
    │      // 4. fsync (durability point)                                │
    │      wal.fsync()?;                                                  │
    │                                                                     │
    │      // 5. Apply to in-memory state                                │
    │      for op in tx.operations {                                     │
    │          self.apply_to_memory(op)?;                                │
    │      }                                                              │
    │                                                                     │
    │      Ok(())                                                         │
    │  }                                                                  │
    │                                                                     │
    └─────────────────────────────────────────────────────────────────────┘


Recovery Boundaries:
====================

    ┌─────────────────────────────────────────────────────────────────────┐
    │                                                                     │
    │  During recovery, transaction boundaries determine visibility:     │
    │                                                                     │
    │  WAL contents:                                                      │
    │  ┌─────────────────────────────────────────────────────────────┐  │
    │  │ TxBegin(1) │ KvPut │ JsonSet │ TxCommit(1) │               │  │
    │  │            └───────────────────────────────│ COMMITTED     │  │
    │  │                                                             │  │
    │  │ TxBegin(2) │ KvPut │ StateInit │ TxCommit(2) │             │  │
    │  │            └─────────────────────────────────│ COMMITTED   │  │
    │  │                                                             │  │
    │  │ TxBegin(3) │ KvPut │ EventApp │ [CRASH - no commit]       │  │
    │  │            └──────────────────│ DISCARDED                  │  │
    │  └─────────────────────────────────────────────────────────────┘  │
    │                                                                     │
    │  Tx1 and Tx2: Complete, replayed                                   │
    │  Tx3: Incomplete, discarded (invariant R6)                         │
    │                                                                     │
    └─────────────────────────────────────────────────────────────────────┘
```

---

## 7. Storage Stabilization

```
+-------------------------------------------------------------------------+
|                    Storage Stabilization (M7)                            |
+-------------------------------------------------------------------------+

PrimitiveStorageExt Trait:
==========================

    ┌─────────────────────────────────────────────────────────────────────┐
    │                                                                     │
    │  /// Extension trait for primitive storage operations.              │
    │  /// Implemented by each primitive to enable WAL/snapshot support. │
    │  ///                                                                │
    │  /// STABILITY: This trait is FROZEN after M7.                     │
    │  /// New primitives MUST implement this trait.                     │
    │  /// Existing implementations MUST NOT change.                     │
    │                                                                     │
    │  trait PrimitiveStorageExt {                                        │
    │      /// Type tag for WAL entry type registry                      │
    │      const TYPE_TAG: u8;                                            │
    │                                                                     │
    │      /// Convert operations to WAL entries                         │
    │      fn to_wal_entries(&self, ops: &[Op]) -> Vec<WalEntry>;        │
    │                                                                     │
    │      /// Apply WAL entries during replay                           │
    │      fn apply_wal_entry(&mut self, entry: &WalEntry) -> Result<()>;│
    │                                                                     │
    │      /// Serialize state for snapshot                              │
    │      fn to_snapshot_blob(&self) -> Vec<u8>;                        │
    │                                                                     │
    │      /// Restore state from snapshot blob                          │
    │      fn from_snapshot_blob(blob: &[u8]) -> Result<Self>;           │
    │  }                                                                  │
    │                                                                     │
    └─────────────────────────────────────────────────────────────────────┘


Primitive Registry:
===================

    ┌─────────────────────────────────────────────────────────────────────┐
    │                                                                     │
    │  struct PrimitiveRegistry {                                         │
    │      primitives: HashMap<u8, Box<dyn PrimitiveStorageExt>>,        │
    │  }                                                                  │
    │                                                                     │
    │  impl PrimitiveRegistry {                                           │
    │      fn register<P: PrimitiveStorageExt>(&mut self) {              │
    │          self.primitives.insert(P::TYPE_TAG, Box::new(P::default()));
    │      }                                                              │
    │                                                                     │
    │      fn get(&self, type_tag: u8) -> Option<&dyn PrimitiveStorageExt>│
    │      fn get_mut(&mut self, type_tag: u8) -> Option<&mut dyn ...>   │
    │  }                                                                  │
    │                                                                     │
    └─────────────────────────────────────────────────────────────────────┘


Extension Points for New Primitives (M8+):
==========================================

    ┌─────────────────────────────────────────────────────────────────────┐
    │                                                                     │
    │  To add a new primitive (e.g., VectorStore in M8):                 │
    │                                                                     │
    │  1. ALLOCATE TYPE TAG                                              │
    │     const TYPE_TAG: u8 = 0x70;  // From reserved range             │
    │                                                                     │
    │  2. IMPLEMENT PrimitiveStorageExt                                  │
    │     impl PrimitiveStorageExt for VectorStore {                     │
    │         const TYPE_TAG: u8 = 0x70;                                 │
    │         fn to_wal_entries(...) { ... }                             │
    │         fn apply_wal_entry(...) { ... }                            │
    │         fn to_snapshot_blob(...) { ... }                           │
    │         fn from_snapshot_blob(...) { ... }                         │
    │     }                                                               │
    │                                                                     │
    │  3. REGISTER IN DATABASE INIT                                      │
    │     registry.register::<VectorStore>();                            │
    │                                                                     │
    │  4. DOCUMENT WAL ENTRY TYPES                                       │
    │     0x70 = VectorInsert                                            │
    │     0x71 = VectorDelete                                            │
    │     0x72 = VectorUpdate                                            │
    │                                                                     │
    │  No changes to RecoveryEngine or SnapshotManager required!         │
    │                                                                     │
    └─────────────────────────────────────────────────────────────────────┘


API Freeze After M7:
====================

    ┌─────────────────────────────────────────────────────────────────────┐
    │                                                                     │
    │  FROZEN AFTER M7:                                                  │
    │  ─────────────────                                                 │
    │  ✓ PrimitiveStorageExt trait signature                             │
    │  ✓ WAL entry envelope format (length, type, version, tx_id, crc)   │
    │  ✓ Snapshot envelope format (magic, version, header, blobs, crc)   │
    │  ✓ Recovery sequence (discover → load → replay → validate)         │
    │  ✓ Type tag allocation scheme                                      │
    │                                                                     │
    │  EXTENSIBLE AFTER M7:                                              │
    │  ─────────────────────                                             │
    │  → New primitives can be added via PrimitiveStorageExt             │
    │  → New WAL entry types within allocated ranges                     │
    │  → New snapshot blob formats for new primitives                    │
    │                                                                     │
    │  NEVER CHANGE:                                                      │
    │  ─────────────                                                      │
    │  ✗ Existing WAL entry format                                        │
    │  ✗ Existing snapshot format                                         │
    │  ✗ PrimitiveStorageExt method signatures                           │
    │  ✗ Recovery invariants (R1-R6)                                     │
    │                                                                     │
    └─────────────────────────────────────────────────────────────────────┘
```

---

## 8. Recovery Invariant Verification

```
+-------------------------------------------------------------------------+
|                 Recovery Invariant Verification (M7)                     |
+-------------------------------------------------------------------------+

Invariant Test Matrix:
======================

    +-------------+----------------------------------------+------------------+
    | Invariant   | Test Strategy                          | Verification     |
    +-------------+----------------------------------------+------------------+
    | R1 DETERM   | Replay same WAL twice, compare states  | Byte equality    |
    | R2 IDEMPOT  | replay(replay(S,WAL),WAL) = replay(S,WAL)| Hash equality  |
    | R3 PREFIX   | Commit N tx, crash, recover, count     | tx_count <= N    |
    | R4 NO INVENT| Write known set, recover, check extras | Set equality     |
    | R5 NO DROP  | Commit tx, crash, recover, verify      | All present      |
    | R6 MAY DROP | Start tx, crash before commit, recover | Tx absent OK     |
    +-------------+----------------------------------------+------------------+


Crash Simulation Tests:
=======================

    ┌─────────────────────────────────────────────────────────────────────┐
    │                                                                     │
    │  Test: crash_during_wal_write                                      │
    │  ─────────────────────────────                                     │
    │                                                                     │
    │  1. Begin transaction                                              │
    │  2. Write partial WAL entry                                        │
    │  3. Simulate crash (don't write CRC)                               │
    │  4. Recover                                                        │
    │  5. Verify: partial entry discarded, prior state intact            │
    │                                                                     │
    │                                                                     │
    │  Test: crash_between_commit_and_fsync                              │
    │  ────────────────────────────────────                              │
    │                                                                     │
    │  1. Write TxCommit entry                                           │
    │  2. Simulate crash before fsync                                    │
    │  3. Recover                                                        │
    │  4. Verify: transaction may or may not be present (both valid)     │
    │                                                                     │
    │                                                                     │
    │  Test: crash_during_snapshot_write                                 │
    │  ───────────────────────────────                                   │
    │                                                                     │
    │  1. Begin snapshot write                                           │
    │  2. Simulate crash mid-write                                       │
    │  3. Recover                                                        │
    │  4. Verify: partial .tmp ignored, older snapshot used              │
    │                                                                     │
    └─────────────────────────────────────────────────────────────────────┘


Determinism Verification:
=========================

    ┌─────────────────────────────────────────────────────────────────────┐
    │                                                                     │
    │  fn test_replay_determinism() {                                    │
    │      // Setup: Write known operations                              │
    │      let ops = vec![                                               │
    │          KvPut("k1", "v1"),                                        │
    │          JsonSet("doc1", json!({"a": 1})),                         │
    │          EventAppend("log1", Event::new("test")),                  │
    │      ];                                                             │
    │                                                                     │
    │      // Execute and capture WAL                                    │
    │      let wal = execute_and_capture(ops);                           │
    │                                                                     │
    │      // Replay 100 times                                           │
    │      let mut states = Vec::new();                                  │
    │      for _ in 0..100 {                                             │
    │          let state = replay_wal(&wal);                             │
    │          states.push(hash(&state));                                │
    │      }                                                              │
    │                                                                     │
    │      // ALL hashes must be identical                               │
    │      assert!(states.windows(2).all(|w| w[0] == w[1]));             │
    │  }                                                                  │
    │                                                                     │
    └─────────────────────────────────────────────────────────────────────┘
```

---

## 9. Performance Characteristics

```
+-------------------------------------------------------------------------+
|                  Performance Characteristics (M7)                        |
+-------------------------------------------------------------------------+

Recovery Time Targets:
======================

    +----------------------------------+------------------+
    |            Scenario              |     Target       |
    +----------------------------------+------------------+
    | Cold start (no snapshot)         |   < 5 seconds    |
    |   - 10K WAL entries              |                  |
    +----------------------------------+------------------+
    | Warm start (with snapshot)       |  < 500 ms        |
    |   - Load snapshot + 1K WAL       |                  |
    +----------------------------------+------------------+
    | Snapshot creation                |   < 1 second     |
    |   - 100K entries                 |                  |
    +----------------------------------+------------------+
    | WAL entry write                  |   < 100 µs       |
    |   - Single entry with fsync      |                  |
    +----------------------------------+------------------+


Non-Regression Requirements:
============================

    ┌─────────────────────────────────────────────────────────────────────┐
    │                                                                     │
    │  CRITICAL: M7 must NOT degrade normal operation performance.       │
    │                                                                     │
    │  +-------------------+------------------+------------------+        │
    │  |    Operation      |   M6 Target      |  M7 Requirement  |        │
    │  +-------------------+------------------+------------------+        │
    │  | KVStore get       |      < 5 µs      |      < 5 µs      |        │
    │  | KVStore put       |      < 8 µs      |     < 10 µs      |        │
    │  |                   |                  |  (+ WAL write)   |        │
    │  | JsonStore get     |    30-50 µs      |    30-50 µs      |        │
    │  | JsonStore set     |   100-200 µs     |   100-250 µs     |        │
    │  |                   |                  |  (+ WAL write)   |        │
    │  | EventLog append   |     < 10 µs      |    < 15 µs       |        │
    │  |                   |                  |  (+ WAL write)   |        │
    │  +-------------------+------------------+------------------+        │
    │                                                                     │
    │  WAL overhead must be minimal for in-memory mode operations.        │
    │                                                                     │
    └─────────────────────────────────────────────────────────────────────┘


Replay Performance:
===================

    ┌─────────────────────────────────────────────────────────────────────┐
    │                                                                     │
    │  replay_run() Performance Targets:                                 │
    │                                                                     │
    │  +----------------------------------+------------------+            │
    │  |     Run Size (events)            |     Target       |            │
    │  +----------------------------------+------------------+            │
    │  | 100 events                       |    < 10 ms       |            │
    │  | 1,000 events                     |    < 50 ms       |            │
    │  | 10,000 events                    |   < 500 ms       |            │
    │  +----------------------------------+------------------+            │
    │                                                                     │
    │  diff_runs() adds ~20% overhead over two replays.                  │
    │                                                                     │
    └─────────────────────────────────────────────────────────────────────┘
```

---

## 10. M7 Philosophy

```
+-------------------------------------------------------------------------+
|                           M7 Philosophy                                  |
+-------------------------------------------------------------------------+

    ┌─────────────────────────────────────────────────────────────────────┐
    │                                                                     │
    │     M7 builds DURABILITY, not FEATURES.                             │
    │                                                                     │
    │     The storage layer must survive crashes and enable               │
    │     deterministic replay without losing committed data.             │
    │                                                                     │
    │     We stabilize the storage API to enable future primitives        │
    │     without changing the core recovery infrastructure.              │
    │                                                                     │
    └─────────────────────────────────────────────────────────────────────┘


What M7 Locks In:
=================

    ┌─────────────────────────────────────────────────────────────────────┐
    │                                                                     │
    │  ✓ WAL entry format (envelope with CRC32)                          │
    │  ✓ Snapshot format (envelope with per-primitive blobs)             │
    │  ✓ Recovery sequence (discover → load → replay → validate)         │
    │  ✓ PrimitiveStorageExt trait for primitive extensions              │
    │  ✓ Run lifecycle (begin_run, end_run, orphan detection)            │
    │  ✓ replay_run() as pure function returning ReadOnlyView            │
    │  ✓ Recovery invariants (R1-R6) and replay invariants (P1-P6)       │
    │                                                                     │
    └─────────────────────────────────────────────────────────────────────┘


What M7 Explicitly Defers:
==========================

    ┌─────────────────────────────────────────────────────────────────────┐
    │                                                                     │
    │  → M8:  Vector store primitive (uses extension point)              │
    │  → M9:  Compression for WAL and snapshots                          │
    │  → M10: WAL archival and retention policies                        │
    │  → Future: Distributed recovery                                     │
    │  → Future: Point-in-time recovery                                   │
    │  → Future: Incremental snapshots                                    │
    │                                                                     │
    └─────────────────────────────────────────────────────────────────────┘


Key Distinctions:
=================

    ┌─────────────────────────────────────────────────────────────────────┐
    │                                                                     │
    │  WAL vs EventLog                                                   │
    │  ───────────────                                                   │
    │                                                                     │
    │  WAL (Write-Ahead Log):                                            │
    │  - Physical, low-level                                             │
    │  - Format may change                                               │
    │  - Used for crash recovery                                         │
    │  - Internal implementation detail                                  │
    │                                                                     │
    │  EventLog:                                                          │
    │  - Semantic, high-level                                            │
    │  - Format is stable                                                │
    │  - Used for replay and audit                                       │
    │  - User-visible API                                                │
    │                                                                     │
    │                                                                     │
    │  Snapshot vs State                                                  │
    │  ─────────────────                                                 │
    │                                                                     │
    │  Snapshot:                                                          │
    │  - Physical compression of WAL effects                             │
    │  - Cache, can be rebuilt from WAL                                  │
    │  - Disposable                                                       │
    │                                                                     │
    │  State:                                                             │
    │  - Semantic truth                                                  │
    │  - Result of committed transactions                                │
    │  - Cannot be discarded                                             │
    │                                                                     │
    └─────────────────────────────────────────────────────────────────────┘


M7 Success Criteria:
====================

    ┌─────────────────────────────────────────────────────────────────────┐
    │                                                                     │
    │  ✓ Recovery invariants (R1-R6) hold under all crash scenarios      │
    │  ✓ Replay invariants (P1-P6) hold for all runs                     │
    │  ✓ Snapshots accelerate recovery without affecting correctness     │
    │  ✓ WAL entries are self-validating with CRC32                      │
    │  ✓ Transactions are atomic across primitives                       │
    │  ✓ Orphaned runs are detected and reported                         │
    │  ✓ PrimitiveStorageExt enables new primitives without core changes │
    │  ✓ No regression in M6 performance baselines                       │
    │                                                                     │
    └─────────────────────────────────────────────────────────────────────┘
```

---

These diagrams illustrate the key architectural components and flows for M7's Durability, Snapshots, Replay & Storage Stabilization milestone. M7 builds upon M6's retrieval surfaces while adding crash recovery, deterministic replay, and storage API stabilization.

**Key Design Points Reflected in These Diagrams**:
- Recovery reconstructs committed transaction history (not semantic history)
- WAL is for crash recovery; EventLog is for semantic history
- Snapshots are physical compression of WAL effects (cache, not truth)
- All WAL entries are self-validating with CRC32
- Transactions are atomic across primitives (all or nothing)
- replay_run() is a pure function returning ephemeral ReadOnlyView
- PrimitiveStorageExt trait enables new primitives without changing core
- Recovery invariants (R1-R6) and replay invariants (P1-P6) are non-negotiable

**M7 Philosophy**: M7 builds durability, not features. The storage layer must survive crashes and enable deterministic replay. Storage API is frozen after M7 to enable future primitive extensions.
