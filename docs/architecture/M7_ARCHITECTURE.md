# M7 Architecture Specification: Durability, Snapshots, Replay & Storage Stabilization

**Version**: 1.0
**Status**: Implementation Ready
**Last Updated**: 2026-01-17

---

## Executive Summary

This document specifies the architecture for **Milestone 7 (M7): Durability, Snapshots, Replay & Storage Stabilization** of the in-memory agent database. M7 consolidates all durability and persistence concerns into one milestone, making the database production-ready from a **data safety** perspective.

**THIS DOCUMENT IS AUTHORITATIVE.** All M7 implementation must conform to this specification.

**Related Documents**:
- [M7 Scope](./M7_SCOPE.md) - Sealed design decisions
- [M6 Architecture](./M6_ARCHITECTURE.md) - Previous milestone
- [MILESTONES.md](../milestones/MILESTONES.md) - Project milestone tracking

**M7 Philosophy**:
> M7 is not about features. M7 is about truth.
>
> After crash recovery, the database must correspond to a **prefix of the committed transaction history**. No partial transactions may be visible. If a transaction spans KV + JSON + Event + State, after crash recovery you must see either all effects or none.

**M7 Goals** (Truth Guarantees):
- Periodic snapshots enable bounded recovery time
- Crash recovery is deterministic, idempotent, and prefix-consistent
- Deterministic replay reconstructs agent run state
- Storage APIs frozen for future primitives (Vector in M8)

**M7 Non-Goals** (Deferred):
- Vector primitive (M8)
- HNSW index (M8/M9)
- Incremental snapshots (Post-MVP)
- Point-in-time recovery (Post-MVP)
- Encryption at rest (M11)

**Critical Constraint**:
> M7 is a correctness milestone, not an optimization milestone. Recovery may be slower than optimal. Snapshots may be larger than necessary. That is acceptable. Correctness matters more than performance. We can optimize later.

**Built on M1-M6**:
- M1 provides: Storage (UnifiedStore), WAL, Recovery basics
- M2 provides: OCC transactions, Snapshot isolation, Conflict detection
- M3 provides: Five primitives (KVStore, EventLog, StateCell, TraceStore, RunIndex)
- M4 provides: Durability modes (InMemory, Buffered, Strict), ShardedStore
- M5 provides: JsonStore primitive with path-level mutations
- M6 provides: Retrieval surface with search
- M7 adds: Snapshots, bounded recovery, deterministic replay, storage stabilization

---

## Table of Contents

1. [Scope Boundaries](#1-scope-boundaries)
2. [THE FIVE ARCHITECTURAL RULES](#2-the-five-architectural-rules-non-negotiable)
3. [Core Invariants](#3-core-invariants)
4. [Architecture Principles](#4-architecture-principles)
5. [Interface Invariants](#5-interface-invariants)
6. [Core Types](#6-core-types)
7. [Snapshot System](#7-snapshot-system)
8. [Crash Recovery](#8-crash-recovery)
9. [WAL Format](#9-wal-format)
10. [Deterministic Replay](#10-deterministic-replay)
11. [Cross-Primitive Atomicity](#11-cross-primitive-atomicity)
12. [Storage Stabilization](#12-storage-stabilization)
13. [API Design](#13-api-design)
14. [Performance Characteristics](#14-performance-characteristics)
15. [Testing Strategy](#15-testing-strategy)
16. [Known Limitations](#16-known-limitations)
17. [Future Extension Points](#17-future-extension-points)
18. [Appendix](#18-appendix)

---

## 1. Scope Boundaries

### 1.1 What M7 IS

M7 is a **durability correctness milestone**. It defines:

| Aspect | M7 Commits To |
|--------|---------------|
| **Snapshot format** | Single file, versioned, checksummed |
| **Recovery sequence** | Snapshot load + WAL replay |
| **Replay API** | `replay_run(run_id) -> ReadOnlyView` |
| **Diff API** | `diff_runs(a, b) -> Diff` (key-level) |
| **WAL format** | Self-validating entries with CRC32 |
| **Storage extension** | Clear patterns for adding primitives |

### 1.2 What M7 is NOT

M7 is **not** an optimization milestone. These are explicitly deferred:

| Deferred Item | Why Deferred | Target Milestone |
|---------------|--------------|------------------|
| Vector primitive | Separate milestone | M8 |
| HNSW index | Depends on Vector | M8/M9 |
| Incremental snapshots | Optimization | Post-MVP |
| Point-in-time recovery | Complexity, no use case yet | Post-MVP |
| Online backup | Enterprise feature | Post-MVP |
| Snapshot compression | Optimization | M9 |
| Encryption at rest | Security milestone | M11 |
| Timestamp-based replay | Clock semantics complexity | Future |

### 1.3 WAL vs EventLog Separation

This separation is critical and must be understood:

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

### 1.4 The Risk We Are Avoiding

Without proper durability guarantees:
- Crashes can lose committed data
- Recovery can invent data that was never committed
- Partial transactions can become visible
- Replay can produce non-deterministic results
- Adding new primitives can break existing recovery

**We must guarantee truth.** If we don't, agents cannot trust their memory.

M7 builds the foundation that ensures:
- Committed data survives crashes
- Uncommitted data is never visible after recovery
- Replay is deterministic and repeatable
- Future primitives integrate without breaking recovery

### 1.5 Evolution Warnings

**These are explicit warnings about M7 design decisions that must not ossify:**

#### A. Snapshot Format Must Support Versioning

The snapshot format will evolve. M7 uses a simple format without compression. Future versions will add:
- Compression (M9)
- Incremental snapshots (future)
- Multi-file snapshots for large databases (future)

The version header exists to enable this evolution. Do NOT assume M7's format is final.

#### B. WAL Entry Types Will Expand

The WAL entry type registry (0x00-0x1F reserved for core, 0x20+ for primitives) exists for extension. When M8 adds Vector primitive, it will add WAL entry types. The recovery engine must handle unknown entry types gracefully (skip with warning).

#### C. Replay Is Interpretation, Not Mutation

Replay must NEVER write to the canonical store. This invariant must not weaken over time. If future features want to "materialize" replay results, that is a separate operation with a separate API.

#### D. Index Recovery Is Rebuild, Not Restore

Indexes are derived data. They are rebuilt from recovered data, not snapshotted. This makes snapshots smaller and recovery more flexible, but means startup time scales with data size. This tradeoff is acceptable for M7.

---

## 2. THE FIVE ARCHITECTURAL RULES (NON-NEGOTIABLE)

**These rules MUST be followed in ALL M7 implementation. Violating any of these is a blocking issue.**

### Rule 1: Recovery Is Deterministic

> **Same WAL + Snapshot = Same state. Always.**

```rust
// CORRECT: Deterministic recovery
fn recover(snapshot: &Snapshot, wal: &WalReader) -> Database {
    let mut state = snapshot.load()?;
    for entry in wal.entries_from(snapshot.wal_offset)? {
        state.apply(entry)?;  // Deterministic application
    }
    state
}

// WRONG: Non-deterministic recovery
fn recover(snapshot: &Snapshot, wal: &WalReader) -> Database {
    let mut state = snapshot.load()?;
    for entry in wal.entries_from(snapshot.wal_offset)? {
        if rand::random::<bool>() {  // NEVER DO THIS
            state.apply(entry)?;
        }
    }
    state
}
```

**Why**: Determinism enables testing, debugging, and reasoning about recovery.

### Rule 2: Recovery Is Prefix-Consistent

> **After recovery, you see a prefix of the committed transaction history. No partial transactions visible.**

```rust
// CORRECT: Atomic transaction boundaries
impl WalWriter {
    fn commit_transaction(&self, tx: &Transaction) -> Result<()> {
        // Write all entries for this transaction
        for entry in tx.entries() {
            self.write_entry(entry)?;
        }
        // Write commit marker
        self.write_commit_marker(tx.id())?;
        self.sync()?;
        Ok(())
    }
}

// Recovery only includes transactions with commit markers
impl WalReader {
    fn committed_entries(&self) -> impl Iterator<Item = WalEntry> {
        // Skip entries from transactions without commit markers
    }
}

// WRONG: No transaction boundaries
impl WalWriter {
    fn write(&self, entry: WalEntry) -> Result<()> {
        self.file.write(&entry.serialize())?;  // Individual entries without grouping
    }
}
```

**Why**: Agents cannot reason about partial state. Either all effects of a transaction are visible, or none.

### Rule 3: Replay Is Side-Effect Free

> **Replay produces a derived view. It does NOT mutate the canonical store.**

```rust
// CORRECT: Replay returns read-only view
pub fn replay_run(db: &Database, run_id: RunId) -> Result<ReadOnlyView> {
    let events = db.event_log.get_run_events(run_id)?;
    let view = ReplayEngine::replay(events)?;
    Ok(view)  // Read-only view, not mutable state
}

// WRONG: Replay mutates canonical store
pub fn replay_run(db: &Database, run_id: RunId) -> Result<()> {
    let events = db.event_log.get_run_events(run_id)?;
    for event in events {
        db.apply(event)?;  // NEVER DO THIS - mutates canonical store
    }
    Ok(())
}
```

**Why**: Replay is interpretation. If replay mutates state, you have two sources of truth.

### Rule 4: Snapshots Are Physical, Not Semantic

> **Snapshots compress WAL effects. They are a cache over history, not the history itself.**

```rust
// CORRECT: Snapshot is byte-level materialized state
pub struct Snapshot {
    version: u32,
    timestamp: u64,
    wal_offset: u64,
    kv_data: Vec<u8>,      // Serialized KV state
    json_data: Vec<u8>,    // Serialized JSON state
    event_data: Vec<u8>,   // Serialized Event state
    // ... other primitives
    checksum: u32,
}

// WRONG: Snapshot stores semantic history
pub struct Snapshot {
    transactions: Vec<Transaction>,  // NEVER DO THIS - that's what WAL is for
    event_history: Vec<Event>,       // NEVER DO THIS - that's what EventLog is for
}
```

**Why**: Semantic history belongs in EventLog. Snapshots are for fast recovery, not reasoning.

### Rule 5: Storage APIs Must Be Stable After M7

> **Adding a primitive must NOT require changes to WAL core format, Snapshot core format, Recovery engine, or Replay engine. Only extension points.**

```rust
// CORRECT: Primitive registry for extension
pub trait PrimitiveStorage {
    fn wal_entry_types(&self) -> &[u8];          // Entry types this primitive uses
    fn serialize(&self) -> Result<Vec<u8>>;      // For snapshots
    fn deserialize(&mut self, data: &[u8]) -> Result<()>;  // From snapshots
    fn apply_wal_entry(&mut self, entry: &WalEntry) -> Result<()>;
}

// New primitive (M8 Vector) implements the trait
impl PrimitiveStorage for VectorStore {
    fn wal_entry_types(&self) -> &[u8] { &[0x30, 0x31, 0x32] }
    fn serialize(&self) -> Result<Vec<u8>> { /* ... */ }
    fn deserialize(&mut self, data: &[u8]) -> Result<()> { /* ... */ }
    fn apply_wal_entry(&mut self, entry: &WalEntry) -> Result<()> { /* ... */ }
}

// WRONG: Hardcoded primitive list in recovery
fn recover() {
    match entry_type {
        0x01 => kv_apply(entry),
        0x02 => json_apply(entry),
        // Can't add Vector without modifying this match
    }
}
```

**Why**: If adding a primitive requires engine changes, we'll break existing deployments.

---

## 3. Core Invariants

### 3.1 Recovery Invariants

These invariants define recovery correctness. They are non-negotiable.

| # | Invariant | Meaning |
|---|-----------|---------|
| R1 | Recovery is deterministic | Same WAL + Snapshot = Same state |
| R2 | Recovery is idempotent | Replaying recovery produces identical state |
| R3 | Recovery is prefix-consistent | No partial transactions visible after recovery |
| R4 | Recovery never invents data | Only committed data appears |
| R5 | Recovery never drops committed data | All durable commits survive |
| R6 | Recovery may drop uncommitted data | Depending on durability mode |

### 3.2 Replay Invariants

These invariants define replay correctness.

| # | Invariant | Meaning |
|---|-----------|---------|
| P1 | Replay is a pure function | Over (Snapshot, WAL, EventLog) |
| P2 | Replay is side-effect free | Does not mutate canonical store |
| P3 | Replay produces a derived view | Not a new source of truth |
| P4 | Replay does not persist state | Unless explicitly materialized |
| P5 | Replay is deterministic | Same inputs = Same view |
| P6 | Replay is idempotent | Running twice produces identical view |

### 3.3 Atomic Recovery Invariant

After recovery, the database must correspond to a **prefix of the committed transaction history**.

```
Committed history: [T1, T2, T3, T4, T5]
                            ^
                         crash here

Valid recovered states:
- [T1, T2, T3, T4]  (Buffered mode - may lose T5)
- [T1, T2, T3, T4, T5]  (Strict mode - all committed data)

Invalid recovered states:
- [T1, T2, T4, T5]  (Skipped T3 - NEVER)
- [T1, T2, T3, partial T4]  (Partial transaction - NEVER)
- [T1, T2, T3, T4, T5, T6]  (T6 was never committed - NEVER)
```

---

## 4. Architecture Principles

### 4.1 M7-Specific Principles

1. **Correctness Over Performance**
   - Recovery may be slower than optimal. That is acceptable.
   - Snapshots may be larger than necessary. That is acceptable.
   - Optimization is future work (M9).

2. **Simplicity Over Features**
   - Single-file snapshots, not directory-based.
   - No compression, no encryption.
   - Key-level diff, not path-level.

3. **Stability Over Flexibility**
   - Freeze WAL format with version header.
   - Freeze snapshot format with version header.
   - Document extension points, not extension mechanisms.

4. **Explicitness Over Convenience**
   - Replay returns a read-only view, not a diff.
   - Materialization is a separate explicit operation (future).
   - Run lifecycle has explicit begin/end markers.

5. **Defense in Depth**
   - CRC32 on every WAL entry.
   - Checksum on every snapshot.
   - Multiple snapshot retention (fall back to older on corruption).

### 4.2 What Replay Is NOT

| Misconception | Reality |
|---------------|---------|
| "Replay is backup" | Replay reconstructs state for a run, not the whole database |
| "Replay is PITR" | Replay is run-scoped, not time-scoped |
| "Replay is restore" | Replay produces a view, not a restored database |
| "Replay is migration" | Replay doesn't change data format |
| "Replay is replication" | Replay is local interpretation, not distributed |

**Replay is interpretation.** Nothing more.

---

## 5. Interface Invariants (Never Change)

This section defines interface invariants that **MUST hold for all future milestones**.

### 5.1 Snapshot Format Envelope

Every snapshot has this envelope:

```rust
pub struct SnapshotEnvelope {
    /// Magic bytes: "INMEM_SNAP"
    pub magic: [u8; 10],
    /// Format version (for future evolution)
    pub version: u32,
    /// Timestamp when snapshot was taken (microseconds)
    pub timestamp_micros: u64,
    /// WAL offset this snapshot includes up to
    pub wal_offset: u64,
    /// Payload length
    pub payload_length: u64,
    /// Payload (version-specific format)
    pub payload: Vec<u8>,
    /// CRC32 of everything above
    pub checksum: u32,
}
```

**This envelope structure must not change.** Payload format is version-dependent.

### 5.2 WAL Entry Envelope

Every WAL entry has this envelope:

```rust
pub struct WalEntryEnvelope {
    /// Entry length (u32)
    pub length: u32,
    /// Entry type (u8)
    pub entry_type: u8,
    /// Format version (u8)
    pub version: u8,
    /// Payload
    pub payload: Vec<u8>,
    /// CRC32 of everything above
    pub checksum: u32,
}
```

**This envelope structure must not change.** Payload format is entry-type-specific.

### 5.3 Run Lifecycle API

Run lifecycle is explicit:

```rust
impl Database {
    /// Begin a new run
    pub fn begin_run(&self, run_id: RunId) -> Result<()>;

    /// End a run
    pub fn end_run(&self, run_id: RunId) -> Result<()>;

    /// Get run status
    pub fn run_status(&self, run_id: RunId) -> Result<RunStatus>;
}

pub enum RunStatus {
    /// Run is active
    Active,
    /// Run completed normally
    Completed,
    /// Run was never ended (orphaned)
    Orphaned,
    /// Run doesn't exist
    NotFound,
}
```

**This API must not change.**

### 5.4 Replay API

Replay returns a read-only view:

```rust
impl Database {
    /// Replay a run and return read-only view
    pub fn replay_run(&self, run_id: RunId) -> Result<ReadOnlyView>;

    /// Diff two runs (key-level)
    pub fn diff_runs(&self, run_a: RunId, run_b: RunId) -> Result<RunDiff>;
}
```

**This API must not change.**

---

## 6. Core Types

### 6.1 Snapshot Types

```rust
/// Snapshot file format (v1)
#[derive(Debug)]
pub struct SnapshotV1 {
    /// Header with metadata
    pub header: SnapshotHeader,
    /// KV store data
    pub kv_data: KVSnapshotData,
    /// JSON store data
    pub json_data: JsonSnapshotData,
    /// Event log data
    pub event_data: EventSnapshotData,
    /// State cell data
    pub state_data: StateSnapshotData,
    /// Trace store data
    pub trace_data: TraceSnapshotData,
    /// Run index data
    pub run_data: RunSnapshotData,
}

#[derive(Debug, Clone)]
pub struct SnapshotHeader {
    /// Format version
    pub version: u32,
    /// When snapshot was taken (microseconds since epoch)
    pub timestamp_micros: u64,
    /// WAL offset this snapshot covers up to
    pub wal_offset: u64,
    /// Number of transactions included
    pub transaction_count: u64,
    /// Database version that created this snapshot
    pub db_version: String,
}

/// Per-primitive snapshot data
#[derive(Debug)]
pub struct KVSnapshotData {
    /// Number of entries
    pub entry_count: u64,
    /// Serialized entries (key-value pairs)
    pub data: Vec<u8>,
}

#[derive(Debug)]
pub struct JsonSnapshotData {
    /// Number of documents
    pub doc_count: u64,
    /// Serialized documents
    pub data: Vec<u8>,
}

// Similar for other primitives...
```

### 6.2 WAL Types

```rust
/// WAL entry types (registry)
#[repr(u8)]
pub enum WalEntryType {
    // Core (0x00-0x1F reserved)
    TransactionCommit = 0x00,
    TransactionAbort = 0x01,
    SnapshotMarker = 0x02,

    // KV (0x01-0x0F)
    KvPut = 0x10,
    KvDelete = 0x11,

    // JSON (0x20-0x2F)
    JsonCreate = 0x20,
    JsonSet = 0x21,
    JsonDelete = 0x22,
    JsonPatch = 0x23,

    // Event (0x30-0x3F)
    EventAppend = 0x30,

    // State (0x40-0x4F)
    StateInit = 0x40,
    StateSet = 0x41,
    StateTransition = 0x42,

    // Trace (0x50-0x5F)
    TraceRecord = 0x50,

    // Run (0x60-0x6F)
    RunCreate = 0x60,
    RunUpdate = 0x61,
    RunEnd = 0x62,
    RunBegin = 0x63,

    // Reserved for Vector (M8): 0x70-0x7F
    // Reserved for future: 0x80-0xFF
}

/// WAL entry with envelope
#[derive(Debug, Clone)]
pub struct WalEntry {
    /// Entry type
    pub entry_type: WalEntryType,
    /// Format version
    pub version: u8,
    /// Transaction ID (for grouping)
    pub tx_id: Option<TxId>,
    /// Payload (type-specific)
    pub payload: Vec<u8>,
}

impl WalEntry {
    /// Serialize entry with envelope and checksum
    pub fn serialize(&self) -> Vec<u8> {
        let mut buf = Vec::new();
        // Length placeholder (filled in at end)
        buf.extend_from_slice(&[0u8; 4]);
        // Entry type
        buf.push(self.entry_type as u8);
        // Version
        buf.push(self.version);
        // Payload
        buf.extend_from_slice(&self.payload);
        // Calculate and append CRC32
        let crc = crc32(&buf[4..]);
        buf.extend_from_slice(&crc.to_le_bytes());
        // Fill in length
        let len = (buf.len() - 4) as u32;
        buf[0..4].copy_from_slice(&len.to_le_bytes());
        buf
    }

    /// Deserialize and validate entry
    pub fn deserialize(data: &[u8]) -> Result<Self, WalError> {
        if data.len() < 10 {  // min: length(4) + type(1) + version(1) + crc(4)
            return Err(WalError::TooShort);
        }

        let len = u32::from_le_bytes([data[0], data[1], data[2], data[3]]) as usize;
        if data.len() < 4 + len {
            return Err(WalError::TooShort);
        }

        // Validate CRC
        let payload_end = 4 + len - 4;  // Exclude CRC
        let stored_crc = u32::from_le_bytes([
            data[payload_end], data[payload_end + 1],
            data[payload_end + 2], data[payload_end + 3]
        ]);
        let computed_crc = crc32(&data[4..payload_end]);
        if stored_crc != computed_crc {
            return Err(WalError::ChecksumMismatch);
        }

        Ok(WalEntry {
            entry_type: WalEntryType::try_from(data[4])?,
            version: data[5],
            tx_id: None,  // Extracted from payload
            payload: data[6..payload_end].to_vec(),
        })
    }
}
```

### 6.3 Recovery Types

```rust
/// Recovery result
#[derive(Debug)]
pub struct RecoveryResult {
    /// Snapshot used (if any)
    pub snapshot_used: Option<SnapshotInfo>,
    /// WAL entries replayed
    pub wal_entries_replayed: u64,
    /// Transactions recovered
    pub transactions_recovered: u64,
    /// Orphaned transactions (no commit marker) skipped
    pub orphaned_transactions: u64,
    /// Corrupt entries skipped
    pub corrupt_entries_skipped: u64,
    /// Recovery time (microseconds)
    pub recovery_time_micros: u64,
}

#[derive(Debug, Clone)]
pub struct SnapshotInfo {
    /// Snapshot file path
    pub path: PathBuf,
    /// Snapshot timestamp
    pub timestamp_micros: u64,
    /// WAL offset
    pub wal_offset: u64,
}

/// Recovery options
#[derive(Debug, Clone)]
pub struct RecoveryOptions {
    /// Maximum corrupt entries to tolerate before failing
    pub max_corrupt_entries: usize,
    /// Whether to verify all checksums (slower but safer)
    pub verify_all_checksums: bool,
    /// Whether to rebuild indexes after recovery
    pub rebuild_indexes: bool,
}

impl Default for RecoveryOptions {
    fn default() -> Self {
        RecoveryOptions {
            max_corrupt_entries: 10,
            verify_all_checksums: true,
            rebuild_indexes: true,
        }
    }
}
```

### 6.4 Replay Types

```rust
/// Read-only view from replay
pub struct ReadOnlyView {
    /// Run this view is for
    pub run_id: RunId,
    /// State at run completion (or current if ongoing)
    kv_state: HashMap<Key, Value>,
    json_state: HashMap<Key, JsonDoc>,
    event_state: Vec<Event>,
    state_state: HashMap<Key, StateValue>,
    trace_state: Vec<Span>,
}

impl ReadOnlyView {
    /// Get KV value
    pub fn get_kv(&self, key: &Key) -> Option<&Value> {
        self.kv_state.get(key)
    }

    /// Get JSON document
    pub fn get_json(&self, key: &Key) -> Option<&JsonDoc> {
        self.json_state.get(key)
    }

    /// Get events
    pub fn events(&self) -> &[Event] {
        &self.event_state
    }

    /// Get state value
    pub fn get_state(&self, key: &Key) -> Option<&StateValue> {
        self.state_state.get(key)
    }

    /// Get traces
    pub fn traces(&self) -> &[Span] {
        &self.trace_state
    }

    /// List all keys in this view
    pub fn keys(&self) -> impl Iterator<Item = &Key> {
        self.kv_state.keys()
            .chain(self.json_state.keys())
            .chain(self.state_state.keys())
    }
}

/// Diff between two runs (key-level)
#[derive(Debug)]
pub struct RunDiff {
    /// Run A
    pub run_a: RunId,
    /// Run B
    pub run_b: RunId,
    /// Keys added in B (not in A)
    pub added: Vec<DiffEntry>,
    /// Keys removed in B (in A but not B)
    pub removed: Vec<DiffEntry>,
    /// Keys modified (different values)
    pub modified: Vec<DiffEntry>,
}

#[derive(Debug)]
pub struct DiffEntry {
    /// Key that changed
    pub key: Key,
    /// Primitive type
    pub primitive: PrimitiveKind,
    /// Value in run A (if present)
    pub value_a: Option<String>,  // Stringified for display
    /// Value in run B (if present)
    pub value_b: Option<String>,  // Stringified for display
}
```

### 6.5 Snapshot Triggers

```rust
/// Snapshot trigger configuration
#[derive(Debug, Clone)]
pub struct SnapshotConfig {
    /// Trigger snapshot when WAL exceeds this size (bytes)
    pub wal_size_threshold: u64,
    /// Trigger snapshot every N minutes
    pub time_interval_minutes: u32,
    /// Number of old snapshots to retain
    pub retention_count: usize,
    /// Whether to snapshot on clean shutdown
    pub snapshot_on_shutdown: bool,
}

impl Default for SnapshotConfig {
    fn default() -> Self {
        SnapshotConfig {
            wal_size_threshold: 100 * 1024 * 1024,  // 100 MB
            time_interval_minutes: 30,
            retention_count: 2,
            snapshot_on_shutdown: true,
        }
    }
}
```

---

## 7. Snapshot System

### 7.1 Snapshot Format (v1)

```rust
/// Snapshot file layout:
///
/// +------------------+
/// | Magic (10 bytes) |  "INMEM_SNAP"
/// +------------------+
/// | Version (4)      |  Format version (1 for M7)
/// +------------------+
/// | Timestamp (8)    |  Microseconds since epoch
/// +------------------+
/// | WAL Offset (8)   |  WAL position covered
/// +------------------+
/// | Tx Count (8)     |  Transactions included
/// +------------------+
/// | Primitive Count  |  Number of primitive sections
/// +------------------+
/// | Primitive 1      |  Type (1) + Length (8) + Data
/// +------------------+
/// | Primitive 2      |
/// +------------------+
/// | ...              |
/// +------------------+
/// | CRC32 (4)        |  Checksum of everything above
/// +------------------+

impl SnapshotWriter {
    pub fn write(&self, db: &Database, path: &Path) -> Result<SnapshotInfo> {
        // Take snapshot at consistent transaction boundary
        let snapshot_view = db.snapshot();  // Uses existing snapshot isolation

        let mut file = File::create(path)?;
        let mut hasher = Crc32::new();

        // Write magic
        let magic = b"INMEM_SNAP";
        file.write_all(magic)?;
        hasher.update(magic);

        // Write version
        let version: u32 = 1;
        let version_bytes = version.to_le_bytes();
        file.write_all(&version_bytes)?;
        hasher.update(&version_bytes);

        // Write timestamp
        let timestamp = now_micros();
        let ts_bytes = timestamp.to_le_bytes();
        file.write_all(&ts_bytes)?;
        hasher.update(&ts_bytes);

        // Write WAL offset
        let wal_offset = db.wal_offset();
        let offset_bytes = wal_offset.to_le_bytes();
        file.write_all(&offset_bytes)?;
        hasher.update(&offset_bytes);

        // Write each primitive's data
        for primitive in &[
            PrimitiveKind::Kv,
            PrimitiveKind::Json,
            PrimitiveKind::Event,
            PrimitiveKind::State,
            PrimitiveKind::Trace,
            PrimitiveKind::Run,
        ] {
            let data = self.serialize_primitive(&snapshot_view, *primitive)?;
            let type_byte = *primitive as u8;
            file.write_all(&[type_byte])?;
            hasher.update(&[type_byte]);

            let len_bytes = (data.len() as u64).to_le_bytes();
            file.write_all(&len_bytes)?;
            hasher.update(&len_bytes);

            file.write_all(&data)?;
            hasher.update(&data);
        }

        // Write CRC32
        let checksum = hasher.finish();
        file.write_all(&checksum.to_le_bytes())?;

        // Sync to ensure durability
        file.sync_all()?;

        Ok(SnapshotInfo {
            path: path.to_path_buf(),
            timestamp_micros: timestamp,
            wal_offset,
        })
    }
}
```

### 7.2 Snapshot Consistency

Snapshots are taken at transaction boundaries using existing snapshot isolation:

```rust
impl Database {
    /// Create a snapshot (manual trigger)
    pub fn snapshot_to_file(&self) -> Result<SnapshotInfo> {
        // Use existing snapshot isolation for consistency
        // This ensures no writes blocked during snapshot
        let snapshot_view = self.snapshot();

        // Determine snapshot path
        let timestamp = now_micros();
        let path = self.snapshot_dir().join(format!("snapshot_{}.dat", timestamp));

        // Write snapshot
        let writer = SnapshotWriter::new();
        let info = writer.write_from_view(&snapshot_view, &path)?;

        // Record snapshot in metadata
        self.record_snapshot(&info)?;

        // Trigger WAL truncation (async)
        self.schedule_wal_truncation(info.wal_offset);

        Ok(info)
    }
}
```

### 7.3 Automatic Snapshot Triggers

```rust
impl SnapshotManager {
    /// Check if snapshot should be triggered
    fn should_snapshot(&self, db: &Database) -> bool {
        let config = db.snapshot_config();

        // Size trigger
        let wal_size = db.wal_size();
        if wal_size >= config.wal_size_threshold {
            return true;
        }

        // Time trigger
        let last_snapshot = self.last_snapshot_time();
        let elapsed = now_micros() - last_snapshot;
        let threshold = config.time_interval_minutes as u64 * 60 * 1_000_000;
        if elapsed >= threshold {
            return true;
        }

        false
    }

    /// Background snapshot loop
    pub fn run(&self, db: Arc<Database>) {
        loop {
            std::thread::sleep(Duration::from_secs(60));  // Check every minute

            if self.should_snapshot(&db) {
                match db.snapshot_to_file() {
                    Ok(info) => {
                        tracing::info!("Snapshot created: {:?}", info.path);
                    }
                    Err(e) => {
                        tracing::error!("Snapshot failed: {}", e);
                    }
                }
            }
        }
    }
}
```

### 7.4 WAL Truncation

```rust
impl WalManager {
    /// Truncate WAL after successful snapshot
    pub fn truncate_to(&self, offset: u64) -> Result<()> {
        // Ensure we keep some buffer
        let safe_offset = offset.saturating_sub(1024);

        // Create new WAL file with entries after offset
        let temp_path = self.wal_path().with_extension("tmp");
        let mut temp_file = File::create(&temp_path)?;

        let mut reader = self.open_reader()?;
        reader.seek_to(safe_offset)?;

        // Copy remaining entries
        while let Some(entry) = reader.next_entry()? {
            temp_file.write_all(&entry.serialize())?;
        }

        temp_file.sync_all()?;

        // Atomic replace
        std::fs::rename(&temp_path, self.wal_path())?;

        // Update offset tracking
        self.base_offset.store(safe_offset, Ordering::Release);

        Ok(())
    }
}
```

### 7.5 Snapshot Retention

```rust
impl SnapshotManager {
    /// Clean up old snapshots, keeping retention_count
    pub fn cleanup_old_snapshots(&self, db: &Database) -> Result<()> {
        let config = db.snapshot_config();
        let mut snapshots = self.list_snapshots()?;

        // Sort by timestamp descending (newest first)
        snapshots.sort_by(|a, b| b.timestamp_micros.cmp(&a.timestamp_micros));

        // Keep retention_count, delete the rest
        for snapshot in snapshots.iter().skip(config.retention_count) {
            std::fs::remove_file(&snapshot.path)?;
            tracing::info!("Deleted old snapshot: {:?}", snapshot.path);
        }

        Ok(())
    }
}
```

---

## 8. Crash Recovery

### 8.1 Recovery Sequence

```rust
impl Database {
    /// Recover from crash
    pub fn recover(
        data_dir: &Path,
        options: RecoveryOptions,
    ) -> Result<(Database, RecoveryResult)> {
        let start = Instant::now();
        let mut result = RecoveryResult::default();

        // 1. Find latest valid snapshot
        let snapshot = Self::find_latest_valid_snapshot(data_dir)?;

        // 2. Load snapshot (if exists)
        let mut db = if let Some(ref snap_info) = snapshot {
            result.snapshot_used = Some(snap_info.clone());
            Self::load_from_snapshot(&snap_info.path)?
        } else {
            Database::empty()
        };

        // 3. Determine WAL replay start point
        let replay_from = snapshot
            .map(|s| s.wal_offset)
            .unwrap_or(0);

        // 4. Replay WAL entries
        let wal_reader = WalReader::open(data_dir.join("wal.dat"))?;
        let mut current_tx: Option<TxId> = None;
        let mut tx_entries: Vec<WalEntry> = Vec::new();

        for entry_result in wal_reader.entries_from(replay_from) {
            let entry = match entry_result {
                Ok(e) => e,
                Err(WalError::ChecksumMismatch) => {
                    result.corrupt_entries_skipped += 1;
                    if result.corrupt_entries_skipped > options.max_corrupt_entries {
                        return Err(RecoveryError::TooManyCorruptEntries);
                    }
                    continue;
                }
                Err(e) => return Err(e.into()),
            };

            result.wal_entries_replayed += 1;

            match entry.entry_type {
                WalEntryType::TransactionCommit => {
                    // Apply all entries for this transaction
                    for e in tx_entries.drain(..) {
                        db.apply_wal_entry(&e)?;
                    }
                    result.transactions_recovered += 1;
                    current_tx = None;
                }
                WalEntryType::TransactionAbort => {
                    // Discard transaction entries
                    tx_entries.clear();
                    result.orphaned_transactions += 1;
                    current_tx = None;
                }
                _ => {
                    // Buffer entry for transaction
                    if entry.tx_id.is_some() {
                        tx_entries.push(entry);
                    } else {
                        // Entry without transaction (shouldn't happen in normal operation)
                        db.apply_wal_entry(&entry)?;
                    }
                }
            }
        }

        // 5. Handle orphaned transaction (no commit marker)
        if !tx_entries.is_empty() {
            result.orphaned_transactions += 1;
            // Don't apply - transaction was in progress during crash
        }

        // 6. Rebuild indexes if configured
        if options.rebuild_indexes {
            db.rebuild_all_indexes()?;
        }

        result.recovery_time_micros = start.elapsed().as_micros() as u64;

        Ok((db, result))
    }
}
```

### 8.2 Snapshot Validation

```rust
impl SnapshotReader {
    /// Validate snapshot integrity
    pub fn validate(path: &Path) -> Result<SnapshotInfo, SnapshotError> {
        let mut file = File::open(path)?;
        let mut hasher = Crc32::new();

        // Read all but last 4 bytes (checksum)
        let file_len = file.metadata()?.len();
        if file_len < 14 {  // Magic(10) + CRC(4)
            return Err(SnapshotError::TooShort);
        }

        let mut data = vec![0u8; (file_len - 4) as usize];
        file.read_exact(&mut data)?;
        hasher.update(&data);

        // Read and verify checksum
        let mut crc_bytes = [0u8; 4];
        file.read_exact(&mut crc_bytes)?;
        let stored_crc = u32::from_le_bytes(crc_bytes);
        let computed_crc = hasher.finish();

        if stored_crc != computed_crc {
            return Err(SnapshotError::ChecksumMismatch);
        }

        // Parse header
        if &data[0..10] != b"INMEM_SNAP" {
            return Err(SnapshotError::InvalidMagic);
        }

        let version = u32::from_le_bytes([data[10], data[11], data[12], data[13]]);
        if version != 1 {
            return Err(SnapshotError::UnsupportedVersion(version));
        }

        let timestamp = u64::from_le_bytes([
            data[14], data[15], data[16], data[17],
            data[18], data[19], data[20], data[21],
        ]);

        let wal_offset = u64::from_le_bytes([
            data[22], data[23], data[24], data[25],
            data[26], data[27], data[28], data[29],
        ]);

        Ok(SnapshotInfo {
            path: path.to_path_buf(),
            timestamp_micros: timestamp,
            wal_offset,
        })
    }
}
```

### 8.3 Fallback to Older Snapshot

```rust
impl Database {
    fn find_latest_valid_snapshot(data_dir: &Path) -> Result<Option<SnapshotInfo>> {
        let snapshot_dir = data_dir.join("snapshots");
        if !snapshot_dir.exists() {
            return Ok(None);
        }

        let mut snapshots = Vec::new();
        for entry in std::fs::read_dir(&snapshot_dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension() == Some("dat".as_ref()) {
                snapshots.push(path);
            }
        }

        // Sort by name (which includes timestamp) descending
        snapshots.sort();
        snapshots.reverse();

        // Try each snapshot until we find a valid one
        for path in snapshots {
            match SnapshotReader::validate(&path) {
                Ok(info) => {
                    tracing::info!("Using snapshot: {:?}", path);
                    return Ok(Some(info));
                }
                Err(e) => {
                    tracing::warn!("Snapshot {:?} is invalid: {}, trying older...", path, e);
                    continue;
                }
            }
        }

        tracing::warn!("No valid snapshots found, will replay full WAL");
        Ok(None)
    }
}
```

### 8.4 Durability Mode Integration

```rust
impl Database {
    /// Apply durability mode rules to recovery expectations
    pub fn recovery_expectations(&self) -> RecoveryExpectations {
        match self.durability_mode() {
            DurabilityMode::InMemory => RecoveryExpectations {
                // No recovery - data is ephemeral
                snapshots_exist: false,
                wal_exists: false,
                data_survives_crash: false,
            },
            DurabilityMode::Buffered => RecoveryExpectations {
                // Snapshots and WAL exist, but some recent data may be lost
                snapshots_exist: true,
                wal_exists: true,
                data_survives_crash: true,  // Except buffered writes in flight
            },
            DurabilityMode::Strict => RecoveryExpectations {
                // Full durability - all committed data survives
                snapshots_exist: true,
                wal_exists: true,
                data_survives_crash: true,
            },
        }
    }
}
```

---

## 9. WAL Format

### 9.1 Entry Format

Every WAL entry follows this format:

```
+----------------+
| Length (u32)   |  Total bytes after this field
+----------------+
| Type (u8)      |  Entry type from registry
+----------------+
| Version (u8)   |  Format version for this entry type
+----------------+
| Payload        |  Type-specific data
+----------------+
| CRC32 (u32)    |  Checksum of Type + Version + Payload
+----------------+
```

### 9.2 Entry Type Registry

```rust
/// WAL entry type registry
///
/// Ranges:
/// - 0x00-0x0F: Core (transaction control)
/// - 0x10-0x1F: KV primitive
/// - 0x20-0x2F: JSON primitive
/// - 0x30-0x3F: Event primitive
/// - 0x40-0x4F: State primitive
/// - 0x50-0x5F: Trace primitive
/// - 0x60-0x6F: Run primitive
/// - 0x70-0x7F: Reserved for Vector (M8)
/// - 0x80-0xFF: Reserved for future primitives
```

### 9.3 Transaction Framing

Transactions are framed with commit markers:

```rust
/// Transaction in WAL looks like:
///
/// [Entry 1 with tx_id=T1]
/// [Entry 2 with tx_id=T1]
/// [Entry 3 with tx_id=T1]
/// [TransactionCommit with tx_id=T1]  <- Commit marker
///
/// On recovery:
/// - Entries without commit marker are discarded
/// - Entries with commit marker are applied atomically

impl WalWriter {
    pub fn commit(&self, tx: &Transaction) -> Result<()> {
        let tx_id = tx.id();

        // Write all entries with tx_id
        for entry in tx.entries() {
            let mut wal_entry = entry.to_wal_entry();
            wal_entry.tx_id = Some(tx_id);
            self.write_entry(&wal_entry)?;
        }

        // Write commit marker
        let commit = WalEntry {
            entry_type: WalEntryType::TransactionCommit,
            version: 1,
            tx_id: Some(tx_id),
            payload: tx_id.to_bytes().to_vec(),
        };
        self.write_entry(&commit)?;

        // Sync based on durability mode
        self.sync_if_strict()?;

        Ok(())
    }
}
```

### 9.4 Corruption Handling

```rust
impl WalReader {
    /// Read next entry, handling corruption gracefully
    pub fn next_entry(&mut self) -> Result<Option<WalEntry>, WalError> {
        // Try to read length
        let mut len_bytes = [0u8; 4];
        match self.file.read_exact(&mut len_bytes) {
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
                return Ok(None);  // End of WAL
            }
            Err(e) => return Err(WalError::Io(e)),
        }

        let len = u32::from_le_bytes(len_bytes) as usize;
        if len > MAX_WAL_ENTRY_SIZE {
            // Likely corruption - skip forward and try to resync
            return Err(WalError::EntryTooLarge(len));
        }

        // Read entry data
        let mut data = vec![0u8; len];
        self.file.read_exact(&mut data)?;

        // Validate CRC
        let payload_len = len - 4;  // Exclude CRC
        let stored_crc = u32::from_le_bytes([
            data[payload_len], data[payload_len + 1],
            data[payload_len + 2], data[payload_len + 3],
        ]);
        let computed_crc = crc32(&data[..payload_len]);

        if stored_crc != computed_crc {
            return Err(WalError::ChecksumMismatch);
        }

        // Parse entry
        WalEntry::from_bytes(&data[..payload_len])
            .map(Some)
    }
}
```

---

## 10. Deterministic Replay

### 10.1 Replay API

```rust
impl Database {
    /// Replay a run and return read-only view
    ///
    /// IMPORTANT: This does NOT mutate the canonical store.
    /// The returned view is derived, not authoritative.
    pub fn replay_run(&self, run_id: RunId) -> Result<ReadOnlyView> {
        // Check run exists
        let run_status = self.run_status(run_id)?;
        if run_status == RunStatus::NotFound {
            return Err(ReplayError::RunNotFound(run_id));
        }

        // Get run's events from EventLog (semantic history)
        let events = self.event_log.get_run_events(run_id)?;

        // Build state by replaying events
        let mut view = ReadOnlyView::new(run_id);

        for event in events {
            match event {
                RunEvent::KvPut { key, value } => {
                    view.kv_state.insert(key, value);
                }
                RunEvent::KvDelete { key } => {
                    view.kv_state.remove(&key);
                }
                RunEvent::JsonSet { key, doc } => {
                    view.json_state.insert(key, doc);
                }
                RunEvent::JsonDelete { key } => {
                    view.json_state.remove(&key);
                }
                RunEvent::JsonPatch { key, patch } => {
                    if let Some(doc) = view.json_state.get_mut(&key) {
                        doc.apply_patch(&patch)?;
                    }
                }
                RunEvent::StateSet { key, value } => {
                    view.state_state.insert(key, value);
                }
                RunEvent::StateTransition { key, from, to } => {
                    view.state_state.insert(key, to);
                }
                RunEvent::EventAppend { event } => {
                    view.event_state.push(event);
                }
                RunEvent::TraceSpan { span } => {
                    view.trace_state.push(span);
                }
            }
        }

        Ok(view)
    }
}
```

### 10.2 Run Index

```rust
/// Run index maps runs to their semantic events
pub struct RunIndex {
    /// Run metadata
    runs: HashMap<RunId, RunMetadata>,
    /// Run -> EventLog offsets (for fast replay)
    run_events: HashMap<RunId, Vec<u64>>,
}

#[derive(Debug, Clone)]
pub struct RunMetadata {
    pub run_id: RunId,
    pub status: RunStatus,
    pub started_at: u64,
    pub ended_at: Option<u64>,
    pub event_count: u64,
}

impl RunIndex {
    /// Get all event offsets for a run (for O(run size) replay)
    pub fn get_run_event_offsets(&self, run_id: RunId) -> Option<&[u64]> {
        self.run_events.get(&run_id).map(|v| v.as_slice())
    }

    /// Detect orphaned runs (no end marker)
    pub fn orphaned_runs(&self) -> Vec<RunId> {
        self.runs
            .iter()
            .filter(|(_, meta)| meta.status == RunStatus::Active)
            .map(|(id, _)| *id)
            .collect()
    }
}
```

### 10.3 Run Diff

```rust
impl Database {
    /// Compare two runs at key level
    pub fn diff_runs(&self, run_a: RunId, run_b: RunId) -> Result<RunDiff> {
        // Replay both runs
        let view_a = self.replay_run(run_a)?;
        let view_b = self.replay_run(run_b)?;

        let mut diff = RunDiff {
            run_a,
            run_b,
            added: Vec::new(),
            removed: Vec::new(),
            modified: Vec::new(),
        };

        // Compare KV state
        Self::diff_maps(
            &view_a.kv_state,
            &view_b.kv_state,
            PrimitiveKind::Kv,
            &mut diff,
        );

        // Compare JSON state
        Self::diff_maps(
            &view_a.json_state,
            &view_b.json_state,
            PrimitiveKind::Json,
            &mut diff,
        );

        // Compare State state
        Self::diff_maps(
            &view_a.state_state,
            &view_b.state_state,
            PrimitiveKind::State,
            &mut diff,
        );

        Ok(diff)
    }

    fn diff_maps<V: std::fmt::Debug + PartialEq>(
        map_a: &HashMap<Key, V>,
        map_b: &HashMap<Key, V>,
        primitive: PrimitiveKind,
        diff: &mut RunDiff,
    ) {
        // Keys in B but not A (added)
        for (key, value_b) in map_b {
            if !map_a.contains_key(key) {
                diff.added.push(DiffEntry {
                    key: key.clone(),
                    primitive,
                    value_a: None,
                    value_b: Some(format!("{:?}", value_b)),
                });
            }
        }

        // Keys in A but not B (removed)
        for (key, value_a) in map_a {
            if !map_b.contains_key(key) {
                diff.removed.push(DiffEntry {
                    key: key.clone(),
                    primitive,
                    value_a: Some(format!("{:?}", value_a)),
                    value_b: None,
                });
            }
        }

        // Keys in both but different values (modified)
        for (key, value_a) in map_a {
            if let Some(value_b) = map_b.get(key) {
                if value_a != value_b {
                    diff.modified.push(DiffEntry {
                        key: key.clone(),
                        primitive,
                        value_a: Some(format!("{:?}", value_a)),
                        value_b: Some(format!("{:?}", value_b)),
                    });
                }
            }
        }
    }
}
```

### 10.4 Run Lifecycle

```rust
impl Database {
    /// Begin a new run
    pub fn begin_run(&self, run_id: RunId) -> Result<()> {
        // Check run doesn't already exist
        if self.run_index.exists(run_id) {
            return Err(RunError::AlreadyExists(run_id));
        }

        // Write WAL entry
        let entry = WalEntry {
            entry_type: WalEntryType::RunBegin,
            version: 1,
            tx_id: None,  // Run lifecycle is not transactional
            payload: run_id.to_bytes().to_vec(),
        };
        self.wal.write_entry(&entry)?;

        // Update run index
        self.run_index.insert(run_id, RunMetadata {
            run_id,
            status: RunStatus::Active,
            started_at: now_micros(),
            ended_at: None,
            event_count: 0,
        });

        // Write to EventLog (semantic history)
        self.event_log.append(RunEvent::RunStarted { run_id })?;

        Ok(())
    }

    /// End a run
    pub fn end_run(&self, run_id: RunId) -> Result<()> {
        // Check run exists and is active
        let meta = self.run_index.get(run_id)?;
        if meta.status != RunStatus::Active {
            return Err(RunError::NotActive(run_id));
        }

        // Write WAL entry
        let entry = WalEntry {
            entry_type: WalEntryType::RunEnd,
            version: 1,
            tx_id: None,
            payload: run_id.to_bytes().to_vec(),
        };
        self.wal.write_entry(&entry)?;

        // Update run index
        self.run_index.update(run_id, |meta| {
            meta.status = RunStatus::Completed;
            meta.ended_at = Some(now_micros());
        })?;

        // Write to EventLog
        self.event_log.append(RunEvent::RunEnded { run_id })?;

        Ok(())
    }
}
```

---

## 11. Cross-Primitive Atomicity

### 11.1 Transaction Spans Primitives

```rust
impl Transaction {
    /// Transaction can include operations on multiple primitives
    pub fn new() -> Self {
        Transaction {
            id: TxId::new(),
            entries: Vec::new(),
        }
    }

    pub fn kv_put(&mut self, key: Key, value: Value) {
        self.entries.push(TxEntry::KvPut { key, value });
    }

    pub fn json_set(&mut self, key: Key, doc: JsonDoc) {
        self.entries.push(TxEntry::JsonSet { key, doc });
    }

    pub fn event_append(&mut self, event: Event) {
        self.entries.push(TxEntry::EventAppend { event });
    }

    pub fn state_transition(&mut self, key: Key, from: State, to: State) {
        self.entries.push(TxEntry::StateTransition { key, from, to });
    }
}

impl Database {
    pub fn commit(&self, tx: Transaction) -> Result<()> {
        // All entries get same tx_id
        // All entries written to WAL
        // Commit marker written at end
        // Either all visible after recovery, or none
        self.wal.commit(&tx)?;

        // Apply to in-memory state
        for entry in tx.entries {
            self.apply_entry(&entry)?;
        }

        Ok(())
    }
}
```

### 11.2 Atomic Recovery

```rust
impl Database {
    /// Apply WAL entry during recovery
    ///
    /// CRITICAL: This is only called for entries with commit markers.
    /// Entries without commit markers (orphaned transactions) are never applied.
    fn apply_wal_entry(&mut self, entry: &WalEntry) -> Result<()> {
        match entry.entry_type {
            WalEntryType::KvPut => {
                let (key, value) = KvPut::from_bytes(&entry.payload)?;
                self.kv.put_raw(key, value)?;
            }
            WalEntryType::KvDelete => {
                let key = Key::from_bytes(&entry.payload)?;
                self.kv.delete_raw(key)?;
            }
            WalEntryType::JsonCreate | WalEntryType::JsonSet => {
                let (key, doc) = JsonDoc::from_bytes(&entry.payload)?;
                self.json.set_raw(key, doc)?;
            }
            WalEntryType::JsonDelete => {
                let key = Key::from_bytes(&entry.payload)?;
                self.json.delete_raw(key)?;
            }
            WalEntryType::JsonPatch => {
                let (key, patch) = JsonPatch::from_bytes(&entry.payload)?;
                self.json.apply_patch_raw(key, patch)?;
            }
            WalEntryType::EventAppend => {
                let event = Event::from_bytes(&entry.payload)?;
                self.event.append_raw(event)?;
            }
            WalEntryType::StateInit | WalEntryType::StateSet => {
                let (key, state) = State::from_bytes(&entry.payload)?;
                self.state.set_raw(key, state)?;
            }
            WalEntryType::StateTransition => {
                let (key, from, to) = StateTransition::from_bytes(&entry.payload)?;
                self.state.transition_raw(key, from, to)?;
            }
            WalEntryType::TraceRecord => {
                let span = Span::from_bytes(&entry.payload)?;
                self.trace.record_raw(span)?;
            }
            WalEntryType::RunCreate | WalEntryType::RunUpdate => {
                let run = Run::from_bytes(&entry.payload)?;
                self.run_index.update_raw(run)?;
            }
            WalEntryType::RunBegin | WalEntryType::RunEnd => {
                // Run lifecycle entries (handled separately)
            }
            WalEntryType::TransactionCommit | WalEntryType::TransactionAbort => {
                // Control entries (handled in recovery loop)
            }
            WalEntryType::SnapshotMarker => {
                // Metadata entry
            }
            _ => {
                // Unknown entry type - skip with warning
                tracing::warn!("Unknown WAL entry type: {:?}", entry.entry_type);
            }
        }
        Ok(())
    }
}
```

---

## 12. Storage Stabilization

### 12.1 Extension Points

After M7, adding a primitive requires implementing these extension points only:

```rust
/// Trait that new primitives must implement for storage integration
pub trait PrimitiveStorageExt {
    /// WAL entry types this primitive uses (from its allocated range)
    fn wal_entry_types(&self) -> &'static [u8];

    /// Serialize primitive state for snapshot
    fn snapshot_serialize(&self) -> Result<Vec<u8>>;

    /// Deserialize primitive state from snapshot
    fn snapshot_deserialize(&mut self, data: &[u8]) -> Result<()>;

    /// Apply a WAL entry during recovery
    fn apply_wal_entry(&mut self, entry: &WalEntry) -> Result<()>;

    /// Create a WAL entry for an operation
    fn to_wal_entry(&self, op: &Self::Operation) -> WalEntry;

    /// Primitive type ID (for snapshot sections)
    fn primitive_type_id(&self) -> u8;
}

// Example: Vector primitive (M8) implementation
impl PrimitiveStorageExt for VectorStore {
    fn wal_entry_types(&self) -> &'static [u8] {
        &[0x70, 0x71, 0x72]  // VectorInsert, VectorDelete, VectorUpdate
    }

    fn snapshot_serialize(&self) -> Result<Vec<u8>> {
        // Serialize vectors and index state
        bincode::serialize(&self.vectors)
    }

    fn snapshot_deserialize(&mut self, data: &[u8]) -> Result<()> {
        self.vectors = bincode::deserialize(data)?;
        // Index will be rebuilt by recovery engine
        Ok(())
    }

    fn apply_wal_entry(&mut self, entry: &WalEntry) -> Result<()> {
        match entry.entry_type {
            0x70 => {  // VectorInsert
                let (id, vec) = Vector::from_bytes(&entry.payload)?;
                self.insert_raw(id, vec)?;
            }
            0x71 => {  // VectorDelete
                let id = VectorId::from_bytes(&entry.payload)?;
                self.delete_raw(id)?;
            }
            _ => {}
        }
        Ok(())
    }

    fn primitive_type_id(&self) -> u8 {
        7  // After existing 6 primitives
    }
}
```

### 12.2 Primitive Registry

```rust
/// Registry of primitives for recovery/snapshot
pub struct PrimitiveRegistry {
    primitives: HashMap<u8, Box<dyn PrimitiveStorageExt>>,
}

impl PrimitiveRegistry {
    pub fn new() -> Self {
        let mut registry = PrimitiveRegistry {
            primitives: HashMap::new(),
        };

        // Register built-in primitives
        registry.register(1, Box::new(KvStorageExt::new()));
        registry.register(2, Box::new(JsonStorageExt::new()));
        registry.register(3, Box::new(EventStorageExt::new()));
        registry.register(4, Box::new(StateStorageExt::new()));
        registry.register(5, Box::new(TraceStorageExt::new()));
        registry.register(6, Box::new(RunStorageExt::new()));

        registry
    }

    /// Register a new primitive (for M8 Vector, etc.)
    pub fn register(&mut self, type_id: u8, primitive: Box<dyn PrimitiveStorageExt>) {
        self.primitives.insert(type_id, primitive);
    }

    /// Get primitive by type ID
    pub fn get(&self, type_id: u8) -> Option<&dyn PrimitiveStorageExt> {
        self.primitives.get(&type_id).map(|p| p.as_ref())
    }
}
```

### 12.3 API Freeze

**After M7, these MUST NOT change:**

| API | Status |
|-----|--------|
| WAL entry envelope format | Frozen |
| Snapshot envelope format | Frozen |
| `PrimitiveStorageExt` trait | Frozen |
| `Database::recover()` signature | Frozen |
| `Database::replay_run()` signature | Frozen |
| `Database::diff_runs()` signature | Frozen |
| `Database::begin_run()` / `end_run()` signatures | Frozen |
| Recovery invariants (R1-R6) | Frozen |
| Replay invariants (P1-P6) | Frozen |

---

## 13. API Design

### 13.1 Snapshot API

```rust
impl Database {
    /// Create snapshot (manual trigger)
    pub fn snapshot(&self) -> Result<SnapshotInfo>;

    /// Configure automatic snapshots
    pub fn configure_snapshots(&self, config: SnapshotConfig);

    /// Get snapshot configuration
    pub fn snapshot_config(&self) -> &SnapshotConfig;

    /// List available snapshots
    pub fn list_snapshots(&self) -> Result<Vec<SnapshotInfo>>;

    /// Delete a specific snapshot
    pub fn delete_snapshot(&self, info: &SnapshotInfo) -> Result<()>;
}
```

### 13.2 Recovery API

```rust
impl Database {
    /// Open database with recovery
    pub fn open(path: &Path) -> Result<Database> {
        Self::open_with_options(path, RecoveryOptions::default())
    }

    /// Open database with custom recovery options
    pub fn open_with_options(
        path: &Path,
        options: RecoveryOptions,
    ) -> Result<Database>;

    /// Get last recovery result
    pub fn last_recovery_result(&self) -> Option<&RecoveryResult>;
}
```

### 13.3 Replay API

```rust
impl Database {
    /// Replay a run and return read-only view
    pub fn replay_run(&self, run_id: RunId) -> Result<ReadOnlyView>;

    /// Diff two runs (key-level)
    pub fn diff_runs(&self, run_a: RunId, run_b: RunId) -> Result<RunDiff>;

    /// List all runs
    pub fn list_runs(&self) -> Result<Vec<RunMetadata>>;

    /// Get run status
    pub fn run_status(&self, run_id: RunId) -> Result<RunStatus>;

    /// List orphaned runs (no end marker)
    pub fn orphaned_runs(&self) -> Result<Vec<RunId>>;
}
```

### 13.4 Run Lifecycle API

```rust
impl Database {
    /// Begin a new run
    pub fn begin_run(&self, run_id: RunId) -> Result<()>;

    /// End a run
    pub fn end_run(&self, run_id: RunId) -> Result<()>;

    /// Abort a run (mark as failed)
    pub fn abort_run(&self, run_id: RunId, reason: &str) -> Result<()>;
}
```

### 13.5 Usage Example

```rust
// Open database (recovers from crash if needed)
let db = Database::open("./data")?;

// Check recovery result
if let Some(result) = db.last_recovery_result() {
    println!("Recovered {} transactions", result.transactions_recovered);
    if result.corrupt_entries_skipped > 0 {
        println!("WARNING: {} corrupt entries skipped", result.corrupt_entries_skipped);
    }
}

// Begin a new agent run
let run_id = RunId::new();
db.begin_run(run_id)?;

// Do work within the run
db.kv.put(run_id, "key1", "value1")?;
db.json.set(run_id, "doc1", json!({"field": "value"}))?;
db.event.append(run_id, Event::new("task_started"))?;

// End the run
db.end_run(run_id)?;

// Later: replay the run to see what happened
let view = db.replay_run(run_id)?;
println!("Run had {} KV entries", view.kv_state.len());
println!("Run had {} events", view.events().len());

// Compare two runs
let diff = db.diff_runs(run_id_1, run_id_2)?;
println!("Added keys: {:?}", diff.added);
println!("Modified keys: {:?}", diff.modified);

// Manual snapshot
let snapshot_info = db.snapshot()?;
println!("Snapshot created at {:?}", snapshot_info.path);
```

---

## 14. Performance Characteristics

### 14.1 M7 Performance Expectations

**M7 prioritizes correctness over speed.**

| Operation | Target | Notes |
|-----------|--------|-------|
| Snapshot write (100MB state) | < 5 seconds | No compression |
| Snapshot load (100MB state) | < 3 seconds | No compression |
| WAL replay (10K entries) | < 1 second | Sequential read |
| Full recovery (100MB snap + 10K WAL) | < 5 seconds | Typical agent |
| Index rebuild (10K docs) | < 2 seconds | During recovery |
| Replay run (1K events) | < 100 ms | O(run size) |
| Diff runs (1K keys each) | < 200 ms | Key-level comparison |

### 14.2 Memory Overhead

| Component | Overhead |
|-----------|----------|
| WAL writer buffer | 64 KB default |
| Snapshot during write | 2x state size (copy-on-write) |
| Run index | ~100 bytes per run |
| Event offset index | ~8 bytes per event |

### 14.3 Disk Space

| Component | Size |
|-----------|------|
| WAL entry overhead | 10 bytes per entry (envelope) |
| Snapshot overhead | ~50 bytes (header) |
| Snapshot retention | 2x snapshot size (default retention) |

---

## 15. Testing Strategy

### 15.1 Recovery Correctness Tests

```rust
#[test]
fn test_recovery_deterministic() {
    let db1 = setup_test_db();
    populate_test_data(&db1);

    // Snapshot
    db1.snapshot()?;

    // Recover twice
    let (db2, _) = Database::recover(&db1.data_dir(), Default::default())?;
    let (db3, _) = Database::recover(&db1.data_dir(), Default::default())?;

    // Must be identical
    assert_eq!(db2.kv.list_all()?, db3.kv.list_all()?);
    assert_eq!(db2.json.list_all()?, db3.json.list_all()?);
}

#[test]
fn test_recovery_prefix_consistent() {
    let db = setup_test_db();

    // Create transaction spanning KV + JSON
    let mut tx = Transaction::new();
    tx.kv_put("key1".into(), "value1".into());
    tx.json_set("doc1".into(), json!({"field": "value"}));

    // Simulate crash before commit (don't write commit marker)
    db.wal.write_entries(&tx.entries)?;  // No commit marker

    // Recover
    let (recovered, result) = Database::recover(&db.data_dir(), Default::default())?;

    // Transaction should not be visible
    assert_eq!(result.orphaned_transactions, 1);
    assert!(recovered.kv.get("key1")?.is_none());
    assert!(recovered.json.get("doc1")?.is_none());
}

#[test]
fn test_recovery_never_loses_committed() {
    let db = setup_test_db();

    // Commit 100 transactions
    for i in 0..100 {
        let mut tx = Transaction::new();
        tx.kv_put(format!("key{}", i).into(), format!("value{}", i).into());
        db.commit(tx)?;
    }

    // Recover
    let (recovered, result) = Database::recover(&db.data_dir(), Default::default())?;

    // All 100 must be present
    assert_eq!(result.transactions_recovered, 100);
    for i in 0..100 {
        let value = recovered.kv.get(&format!("key{}", i))?;
        assert_eq!(value, Some(format!("value{}", i).into()));
    }
}
```

### 15.2 Crash Scenario Tests

```rust
#[test]
fn test_crash_during_snapshot() {
    let db = setup_test_db();
    populate_test_data(&db);

    // Simulate crash during snapshot write (partial file)
    let snapshot_path = db.snapshot_dir().join("snapshot_partial.dat");
    {
        let mut file = File::create(&snapshot_path)?;
        file.write_all(b"INMEM_SNAP")?;  // Write magic only
        // Don't write rest - simulates crash
    }

    // Recovery should use older snapshot or full WAL replay
    let (recovered, result) = Database::recover(&db.data_dir(), Default::default())?;

    // Data should be intact
    assert_data_intact(&db, &recovered);
}

#[test]
fn test_crash_during_wal_truncation() {
    let db = setup_test_db();
    populate_test_data(&db);

    // Create snapshot
    db.snapshot()?;

    // Simulate crash during WAL truncation (both old and new WAL exist)
    // Recovery should handle this

    let (recovered, _) = Database::recover(&db.data_dir(), Default::default())?;
    assert_data_intact(&db, &recovered);
}

#[test]
fn test_corrupt_wal_entries() {
    let db = setup_test_db();

    // Write some valid entries
    for i in 0..10 {
        let mut tx = Transaction::new();
        tx.kv_put(format!("key{}", i).into(), format!("value{}", i).into());
        db.commit(tx)?;
    }

    // Corrupt one entry in the middle (flip a bit in checksum)
    corrupt_wal_entry(&db.wal_path(), 5)?;

    // Recovery should skip corrupt entry
    let (recovered, result) = Database::recover(&db.data_dir(), Default::default())?;

    assert_eq!(result.corrupt_entries_skipped, 1);
    // Other entries should be recovered
}

#[test]
fn test_corrupt_snapshot() {
    let db = setup_test_db();
    populate_test_data(&db);

    // Create two snapshots
    db.snapshot()?;
    std::thread::sleep(Duration::from_millis(10));
    db.snapshot()?;

    // Corrupt newest snapshot
    let snapshots = db.list_snapshots()?;
    corrupt_snapshot(&snapshots[0].path)?;  // Newest

    // Recovery should fall back to older snapshot
    let (recovered, result) = Database::recover(&db.data_dir(), Default::default())?;

    assert_eq!(result.snapshot_used, Some(snapshots[1].clone()));
    assert_data_intact(&db, &recovered);
}
```

### 15.3 Replay Tests

```rust
#[test]
fn test_replay_deterministic() {
    let db = setup_test_db();

    let run_id = RunId::new();
    db.begin_run(run_id)?;
    db.kv.put(run_id, "key1", "value1")?;
    db.kv.put(run_id, "key2", "value2")?;
    db.end_run(run_id)?;

    // Replay twice
    let view1 = db.replay_run(run_id)?;
    let view2 = db.replay_run(run_id)?;

    // Must be identical
    assert_eq!(view1.kv_state, view2.kv_state);
}

#[test]
fn test_replay_side_effect_free() {
    let db = setup_test_db();

    let run_id = RunId::new();
    db.begin_run(run_id)?;
    db.kv.put(run_id, "key1", "value1")?;
    db.end_run(run_id)?;

    // Get canonical state
    let canonical_before = db.kv.list_all()?;

    // Replay
    let _view = db.replay_run(run_id)?;

    // Canonical state unchanged
    let canonical_after = db.kv.list_all()?;
    assert_eq!(canonical_before, canonical_after);
}

#[test]
fn test_diff_runs() {
    let db = setup_test_db();

    // Run A: keys 1, 2, 3
    let run_a = RunId::new();
    db.begin_run(run_a)?;
    db.kv.put(run_a, "key1", "value1")?;
    db.kv.put(run_a, "key2", "value2")?;
    db.kv.put(run_a, "key3", "value3")?;
    db.end_run(run_a)?;

    // Run B: keys 2, 3, 4 (key1 removed, key4 added, key3 modified)
    let run_b = RunId::new();
    db.begin_run(run_b)?;
    db.kv.put(run_b, "key2", "value2")?;
    db.kv.put(run_b, "key3", "value3_modified")?;
    db.kv.put(run_b, "key4", "value4")?;
    db.end_run(run_b)?;

    let diff = db.diff_runs(run_a, run_b)?;

    assert_eq!(diff.added.len(), 1);  // key4
    assert_eq!(diff.removed.len(), 1);  // key1
    assert_eq!(diff.modified.len(), 1);  // key3
}
```

### 15.4 Stress Tests

```rust
#[test]
fn test_many_small_transactions() {
    let db = setup_test_db();

    // 10K small transactions
    for i in 0..10_000 {
        let mut tx = Transaction::new();
        tx.kv_put(format!("key{}", i).into(), format!("value{}", i).into());
        db.commit(tx)?;
    }

    // Snapshot and recover
    db.snapshot()?;
    let (recovered, result) = Database::recover(&db.data_dir(), Default::default())?;

    assert_eq!(result.transactions_recovered, 10_000);
    assert!(result.recovery_time_micros < 5_000_000);  // < 5 seconds
}

#[test]
fn test_large_values() {
    let db = setup_test_db();

    // 100 large values (1 MB each)
    for i in 0..100 {
        let value = "x".repeat(1_000_000);
        let mut tx = Transaction::new();
        tx.kv_put(format!("key{}", i).into(), value.into());
        db.commit(tx)?;
    }

    // Snapshot and recover
    db.snapshot()?;
    let (recovered, _) = Database::recover(&db.data_dir(), Default::default())?;

    for i in 0..100 {
        let value = recovered.kv.get(&format!("key{}", i))?;
        assert_eq!(value.map(|v| v.len()), Some(1_000_000));
    }
}
```

---

## 16. Known Limitations

### 16.1 M7 Limitations (Intentional)

| Limitation | Impact | Mitigation |
|------------|--------|------------|
| **No compression** | Larger snapshots | M9 adds compression |
| **No encryption** | Data at rest unprotected | M11 adds encryption |
| **Key-level diff only** | No JSON path-level diff | Future enhancement |
| **No PITR** | Can't recover to arbitrary point | Use run-based replay |
| **No incremental snapshots** | Full snapshot every time | Future optimization |
| **Index rebuild on recovery** | Slower startup | Acceptable for M7 |

### 16.2 What M7 Explicitly Does NOT Provide

- Point-in-time recovery (PITR)
- Timestamp-based replay
- Path-level JSON diff
- Incremental snapshots
- Parallel snapshot write
- Online backup
- Encryption at rest
- Compression

These are all **intentionally deferred**, not forgotten.

---

## 17. Future Extension Points

### 17.1 M8: Vector Primitive

Vector primitive will integrate using `PrimitiveStorageExt`:

```rust
// M8 adds:
impl PrimitiveStorageExt for VectorStore {
    fn wal_entry_types(&self) -> &'static [u8] {
        &[0x70, 0x71, 0x72]
    }
    // ...
}

// Recovery engine handles it automatically
```

### 17.2 M9: Compression

```rust
// M9 adds snapshot compression:
pub struct SnapshotV2 {
    // Same envelope as V1
    // Payload is compressed with zstd
    pub compression: CompressionType,
}
```

### 17.3 M11: Encryption

```rust
// M11 adds encryption:
pub struct SnapshotV3 {
    // Envelope includes encryption metadata
    pub encryption: EncryptionMetadata,
}
```

### 17.4 Future: Materialization API

```rust
// Named concept for future:
impl Database {
    /// Materialize a replay view into canonical state
    ///
    /// WARNING: This creates a new source of truth.
    /// Use with extreme caution.
    pub fn materialize(&self, view: ReadOnlyView) -> Result<()> {
        // NOT implemented in M7
        // Named here to prevent API confusion
        unimplemented!()
    }
}
```

---

## 18. Appendix

### 18.1 WAL Entry Type Registry

```
Core (0x00-0x0F):
  0x00 - TransactionCommit
  0x01 - TransactionAbort
  0x02 - SnapshotMarker

KV (0x10-0x1F):
  0x10 - KvPut
  0x11 - KvDelete

JSON (0x20-0x2F):
  0x20 - JsonCreate
  0x21 - JsonSet
  0x22 - JsonDelete
  0x23 - JsonPatch

Event (0x30-0x3F):
  0x30 - EventAppend

State (0x40-0x4F):
  0x40 - StateInit
  0x41 - StateSet
  0x42 - StateTransition

Trace (0x50-0x5F):
  0x50 - TraceRecord

Run (0x60-0x6F):
  0x60 - RunCreate
  0x61 - RunUpdate
  0x62 - RunEnd
  0x63 - RunBegin

Reserved for Vector (M8): 0x70-0x7F
Reserved for future: 0x80-0xFF
```

### 18.2 Success Criteria Checklist

**Gate 1: Snapshot System**
- [ ] `db.snapshot()` creates valid snapshot
- [ ] Periodic snapshots trigger correctly (size and time)
- [ ] WAL truncation works after snapshot
- [ ] Snapshot + truncation is atomic (no data loss window)
- [ ] Multiple snapshot retention works
- [ ] Snapshot on shutdown works (when configured)

**Gate 2: Crash Recovery**
- [ ] Recovery loads latest valid snapshot
- [ ] Recovery replays WAL from snapshot offset
- [ ] Recovery skips uncommitted transactions
- [ ] Recovery handles corrupt WAL entries gracefully
- [ ] Recovery falls back to older snapshot on corruption
- [ ] Recovery is deterministic (same inputs = same state)
- [ ] Recovery is idempotent
- [ ] Recovery respects durability mode expectations

**Gate 3: WAL Format**
- [ ] WAL entries have CRC32 checksums
- [ ] WAL entries have version headers
- [ ] Transaction framing with commit markers works
- [ ] Entry type registry supports extension

**Gate 4: Deterministic Replay**
- [ ] `replay_run()` returns correct state
- [ ] Replay is O(run size), not O(total WAL)
- [ ] Replay is side-effect free (doesn't mutate canonical store)
- [ ] Replay is deterministic (same run = same view)
- [ ] `diff_runs()` works at key level
- [ ] Run lifecycle (begin/end) works
- [ ] Orphaned runs detected and reported

**Gate 5: Cross-Primitive Atomicity**
- [ ] Transactions spanning primitives are atomic
- [ ] After recovery, either all effects visible or none
- [ ] No partial transactions visible

**Gate 6: Storage Stabilization**
- [ ] `PrimitiveStorageExt` trait documented
- [ ] WAL entry type registry documented
- [ ] Snapshot format documented
- [ ] Extension points clear for M8 Vector
- [ ] Performance baselines documented

---

## Conclusion

M7 is a **durability correctness milestone**.

It defines:
- Snapshot system for bounded recovery time
- Crash recovery that is deterministic, idempotent, prefix-consistent
- Deterministic replay for agent run reconstruction
- Storage APIs frozen for future primitives

It does NOT attempt to optimize. That is intentional.

**M7 builds truth. Future milestones build speed.**

After M7, the database survives crashes correctly, recovers efficiently, and enables deterministic replay of agent runs. This is the foundation for production deployment.

---

**Document Version**: 1.0
**Status**: Implementation Ready
**Date**: 2026-01-17
