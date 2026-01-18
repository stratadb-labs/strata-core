# M1 Architecture Specification

**Version**: 1.0
**Status**: Foundation Milestone
**Last Updated**: 2026-01-10

---

## Executive Summary

This document specifies the architecture for **Milestone 1 (M1): Foundation** of the in-memory agent database. M1 establishes the core infrastructure that all future milestones build upon: storage layer, durability (WAL), recovery, and basic operations.

**M1 Goals**:
- Provide durable, versioned storage for agent data
- Enable automatic crash recovery via Write-Ahead Log
- Establish clean layer boundaries for future extension
- Support run-scoped operations (fundamental to agent workflows)
- Deliver working KV primitive as primitive pattern exemplar

**Non-Goals for M1**:
- Full OCC transactions (M2)
- Remaining primitives: Event Log, State Machine, Trace (M3)
- Snapshots and WAL rotation (M4)
- Deterministic replay (M5)
- Vector store (M6)
- Network layer (M7)

---

## Table of Contents

1. [System Overview](#system-overview)
2. [Architecture Principles](#architecture-principles)
3. [Component Architecture](#component-architecture)
4. [Data Models](#data-models)
5. [Layer Boundaries](#layer-boundaries)
6. [Concurrency Model](#concurrency-model)
7. [Durability Strategy](#durability-strategy)
8. [Recovery Protocol](#recovery-protocol)
9. [API Design](#api-design)
10. [Error Handling](#error-handling)
11. [Performance Characteristics](#performance-characteristics)
12. [Testing Strategy](#testing-strategy)
13. [Known Limitations](#known-limitations)
14. [Future Extension Points](#future-extension-points)

---

## 1. System Overview

### 1.1 High-Level Architecture

```
┌─────────────────────────────────────────────────────────┐
│                    Application                          │
└────────────────────────┬────────────────────────────────┘
                         │
┌────────────────────────▼────────────────────────────────┐
│                  Primitives Layer                       │
│  ┌──────────┐  ┌──────────┐  ┌──────────┐             │
│  │ KVStore  │  │EventLog* │  │ Trace*   │   (Stateless│
│  │          │  │          │  │          │    Facades) │
│  └──────────┘  └──────────┘  └──────────┘             │
└────────────────────────┬────────────────────────────────┘
                         │
┌────────────────────────▼────────────────────────────────┐
│                   Engine Layer                          │
│  ┌──────────────────────────────────────────────┐      │
│  │  Database                                     │      │
│  │  - Run Lifecycle (begin_run, end_run)        │      │
│  │  - Operation Orchestration (put, get, delete)│      │
│  │  - Recovery Coordination                     │      │
│  └──────────────────────────────────────────────┘      │
└────┬────────────────────────────────┬───────────────────┘
     │                                │
     ▼                                ▼
┌────────────────────┐      ┌─────────────────────────────┐
│  Storage Layer     │      │   Durability Layer          │
│                    │      │                             │
│  ┌──────────────┐ │      │  ┌──────────┐ ┌──────────┐ │
│  │UnifiedStore  │ │      │  │   WAL    │ │ Recovery │ │
│  │(BTreeMap)    │ │      │  │          │ │          │ │
│  └──────────────┘ │      │  └──────────┘ └──────────┘ │
│  ┌──────────────┐ │      │  ┌──────────┐              │
│  │  Indices     │ │      │  │ Encoding │              │
│  │- run_index   │ │      │  │          │              │
│  │- type_index  │ │      │  └──────────┘              │
│  │- ttl_index   │ │      │                             │
│  └──────────────┘ │      └─────────────────────────────┘
└────────────────────┘
         │
         ▼
┌────────────────────────────────────────────────────────┐
│                   Core Types                           │
│  RunId, Namespace, Key, TypeTag, Value, Error         │
└────────────────────────────────────────────────────────┘

* EventLog and Trace are deferred to M3
```

### 1.2 Component Responsibilities

| Component | Responsibility | Owns State? |
|-----------|---------------|-------------|
| **Core Types** | Type definitions, traits, error types | No |
| **UnifiedStore** | In-memory storage, versioning, secondary indices | Yes (BTreeMap) |
| **WAL** | Append-only logging, durability modes, file I/O | Yes (file) |
| **Recovery** | WAL replay, incomplete transaction handling | No (stateless) |
| **Database** | Orchestration, run lifecycle, operation coordination | Yes (active runs) |
| **KVStore** | KV primitive API facade | No (delegates) |

---

## 2. Architecture Principles

### 2.1 Core Principles

1. **Simplicity Over Perfection**
   - Accept MVP limitations (RwLock contention, snapshot cloning)
   - Use well-understood patterns (BTreeMap, fsync, bincode)
   - Optimize later with measurement, not speculation

2. **Trait-Based Abstraction**
   - `Storage` trait: enables swapping BTreeMap for sharded/lock-free implementations
   - `SnapshotView` trait: enables lazy snapshots without API changes
   - Future-proof interfaces, simple implementations

3. **Fail-Safe Recovery**
   - Conservative: discard incomplete transactions, don't guess intent
   - Explicit validation: check transaction boundaries, CRC integrity
   - Stop at first corruption: don't propagate bad data

4. **Run-Scoped Everything**
   - All operations tagged with `run_id`
   - Enables replay, diff, lineage tracking
   - Fundamental to agent workflow model

5. **Layer Isolation**
   - Primitives don't know about storage or WAL
   - Storage doesn't know about primitives
   - Clean dependency flow: primitives → engine → storage/durability → core

6. **Testability First**
   - Each layer testable in isolation
   - Mock-friendly interfaces (traits)
   - Crash simulation tests for durability
   - Property-based testing for recovery

### 2.2 Design Patterns

**Pattern: Stateless Facades**
- Primitives (KVStore, etc.) are facades over Database
- No state ownership, just API sugar
- Cheap to clone (Arc clone)

**Pattern: Unified Storage with Type Tags**
- Single BTreeMap for all data types
- TypeTag discriminates primitives
- Simplifies indexing and queries

**Pattern: Implicit Single-Operation Transactions**
- M1: each put/get/delete is a transaction
- M2: will add explicit multi-operation transactions
- WAL always has transaction boundaries (BeginTxn → Op → CommitTxn)

**Pattern: Background Tasks via Threads**
- TTL cleanup: background thread with transactional deletes
- Async fsync: background thread for durability mode
- No task scheduler needed for MVP

---

## 3. Component Architecture

### 3.1 Core Types (`crates/core`)

**Purpose**: Foundation types used by all layers.

**Key Types**:

```rust
// Unique run identifier
pub struct RunId(Uuid);

// Hierarchical namespace: tenant → app → agent → run
pub struct Namespace {
    pub tenant: String,
    pub app: String,
    pub agent: String,
    pub run_id: RunId,
}

// Type discriminator for unified storage
pub enum TypeTag {
    KV = 0,
    Event = 1,
    StateMachine = 2,
    Trace = 3,
    RunMetadata = 4,
}

// Composite key: namespace + type + user_key
pub struct Key {
    pub namespace: Namespace,
    pub type_tag: TypeTag,
    pub user_key: Vec<u8>,
}

// Versioned value wrapper
pub struct VersionedValue {
    pub value: Value,
    pub version: u64,        // Monotonically increasing
    pub timestamp: Timestamp,
    pub ttl: Option<Duration>,
}

// Unified value enum
pub enum Value {
    Bytes(Vec<u8>),                    // KV data
    Event(EventEntry),                 // Event log entries
    StateMachineRecord(StateMachineEntry),
    Trace(TraceEntry),
    RunMetadata(RunMetadataEntry),     // Run index entries
}
```

**Traits**:

```rust
// Storage abstraction (enables future optimization)
pub trait Storage: Send + Sync {
    fn get(&self, key: &Key) -> Result<Option<VersionedValue>>;
    fn get_versioned(&self, key: &Key, max_version: u64) -> Result<Option<VersionedValue>>;
    fn put(&self, key: Key, value: Value, ttl: Option<Duration>) -> Result<u64>;
    fn delete(&self, key: &Key) -> Result<Option<VersionedValue>>;
    fn scan_prefix(&self, prefix: &Key, max_version: u64) -> Result<Vec<(Key, VersionedValue)>>;
    fn scan_by_run(&self, run_id: RunId, max_version: u64) -> Result<Vec<(Key, VersionedValue)>>;
    fn current_version(&self) -> u64;
}

// Snapshot abstraction (prevents API ossification)
pub trait SnapshotView: Send + Sync {
    fn get(&self, key: &Key) -> Result<Option<VersionedValue>>;
    fn scan_prefix(&self, prefix: &Key) -> Result<Vec<(Key, VersionedValue)>>;
    fn version(&self) -> u64;
}
```

**Error Hierarchy**:

```rust
pub enum Error {
    Storage(StorageError),
    Concurrency(ConcurrencyError),
    Durability(DurabilityError),
    Primitive(PrimitiveError),
}

// Storage errors: NotFound, Expired, SerializationError
// Durability errors: WALError, CorruptionError, IOError
// Context included: key, version, offset, message
```

**Design Rationale**:
- **Namespace hierarchy**: Enables multi-tenancy without separate databases
- **TypeTag enum**: Unified storage simpler than separate stores per primitive
- **Traits for abstraction**: Storage and SnapshotView enable optimization without breaking API
- **Version in value**: Every write gets unique, increasing version (critical for snapshots)

---

### 3.2 Storage Layer (`crates/storage`)

**Purpose**: In-memory storage with versioning and indexing.

**Architecture**:

```rust
pub struct UnifiedStore {
    // Main storage: sorted by Key (namespace → type_tag → user_key)
    data: Arc<RwLock<BTreeMap<Key, VersionedValue>>>,

    // Secondary indices for efficient queries
    run_index: Arc<RwLock<HashMap<RunId, HashSet<Key>>>>,
    type_index: Arc<RwLock<HashMap<TypeTag, HashSet<Key>>>>,
    ttl_index: Arc<RwLock<BTreeMap<Instant, HashSet<Key>>>>,

    // Global version counter
    global_version: AtomicU64,
}
```

**Operations**:

```rust
impl Storage for UnifiedStore {
    fn put(&self, key: Key, value: Value, ttl: Option<Duration>) -> Result<u64> {
        // 1. Allocate version (fetch_add on AtomicU64)
        // 2. Create VersionedValue with version + timestamp + ttl
        // 3. Acquire write lock on data, run_index, type_index, ttl_index
        // 4. Insert into all indices atomically
        // 5. Release locks, return version
    }

    fn get(&self, key: &Key) -> Result<Option<VersionedValue>> {
        // 1. Acquire read lock on data
        // 2. Lookup key in BTreeMap
        // 3. Check is_expired() - return None if expired
        // 4. Return value
    }

    fn scan_by_run(&self, run_id: RunId, max_version: u64) -> Result<Vec<(Key, VersionedValue)>> {
        // 1. Acquire read lock on run_index
        // 2. Get HashSet<Key> for run_id (O(1) lookup)
        // 3. For each key: get from data, filter by version and expiration
        // 4. Return filtered results
        // O(run size) not O(total data) - critical for replay
    }
}
```

**Indexing Strategy**:

| Index | Key | Value | Purpose |
|-------|-----|-------|---------|
| **Primary** | Key (namespace+type+user_key) | VersionedValue | Main storage, sorted |
| **run_index** | RunId | HashSet\<Key\> | Fast run-scoped queries (replay) |
| **type_index** | TypeTag | HashSet\<Key\> | Fast type-scoped queries |
| **ttl_index** | Instant (expiry time) | HashSet\<Key\> | Fast TTL cleanup |

**TTL Cleanup**:

```rust
pub struct TTLCleaner {
    store: Arc<dyn Storage>,
    check_interval: Duration,
}

// Background thread that:
// 1. Periodically calls store.find_expired_keys(now)
// 2. Deletes each via store.delete() (transactional)
// 3. Does NOT directly mutate storage (avoids races)
```

**Known Limitations**:
- **RwLock contention**: Writers block all readers (acceptable for MVP)
- **Global lock**: Single RwLock will bottleneck under high concurrency
- **No version history**: Overwrites discard old versions (acceptable for M1)

**Mitigation**: Storage trait allows replacing with sharded/lock-free implementation in M4+.

---

### 3.3 Durability Layer (`crates/durability`)

**Purpose**: Write-Ahead Log for crash recovery.

#### 3.3.1 WAL Entry Types

```rust
pub enum WALEntry {
    BeginTxn {
        txn_id: u64,
        run_id: RunId,      // CRITICAL: enables run-scoped replay
        timestamp: Timestamp,
    },
    Write {
        run_id: RunId,
        key: Key,
        value: Value,
        version: u64,
    },
    Delete {
        run_id: RunId,
        key: Key,
        version: u64,
    },
    CommitTxn {
        txn_id: u64,
        run_id: RunId,
    },
    AbortTxn {
        txn_id: u64,
        run_id: RunId,
    },
    Checkpoint {
        snapshot_id: Uuid,
        version: u64,
        active_runs: Vec<RunId>,
    },
}
```

**Why run_id in every entry**:
- Enables filtering WAL by run for replay
- Supports run diffing (compare WAL entries for two runs)
- Audit trails are run-scoped
- Future: partial replay of specific runs

#### 3.3.2 WAL File Format

**Entry Format**: `[length: u32][type: u8][payload: bytes][crc32: u32]`

| Field | Size | Description |
|-------|------|-------------|
| Length | 4 bytes | Total entry size (type + payload + CRC) |
| Type tag | 1 byte | Entry type (BeginTxn=1, Write=2, etc.) |
| Payload | Variable | bincode-serialized entry |
| CRC32 | 4 bytes | Checksum over type + payload |

**Design Rationale**:
- **Length prefix**: Enables skipping unknown entry types (forward compatibility)
- **Type tag**: Enables versioning (can skip unknown types)
- **CRC32**: Detects corruption (bit flips, partial writes)
- **bincode**: Fast, deterministic, compact serialization

#### 3.3.3 Durability Modes

```rust
pub enum DurabilityMode {
    Strict,                              // fsync after every commit
    Batched { interval_ms: u64, batch_size: usize }, // fsync periodically
    Async { interval_ms: u64 },          // background fsync
}

impl Default for DurabilityMode {
    fn default() -> Self {
        // DEFAULT: Batched with 100ms or 1000 commits
        DurabilityMode::Batched {
            interval_ms: 100,
            batch_size: 1000,
        }
    }
}
```

**Mode Comparison**:

| Mode | Latency | Durability | Use Case |
|------|---------|------------|----------|
| **Strict** | ~10ms/write | Maximum | Critical data, infrequent writes |
| **Batched** (default) | <1ms/write | Good (100ms loss window) | Agent workflows (balanced) |
| **Async** | <0.1ms/write | Weak (up to interval loss) | Scratch data, high throughput |

**Default Rationale**: Agents prefer speed over perfect durability. Losing 100ms of writes is acceptable; blocking 10ms per write is not.

#### 3.3.4 WAL Operations

```rust
impl WAL {
    pub fn open(path: &Path, durability_mode: DurabilityMode) -> Result<Self> {
        // 1. Open file (create if doesn't exist, append mode)
        // 2. Get file size (current offset)
        // 3. Spawn background fsync thread if Async mode
        // 4. Return WAL ready for appends
    }

    pub fn append(&mut self, entry: &WALEntry) -> Result<u64> {
        // 1. Encode entry (length + type + payload + CRC)
        // 2. Write to buffered writer
        // 3. Update current_offset
        // 4. Handle durability mode:
        //    - Strict: flush() + fsync() immediately
        //    - Batched: increment counter, fsync if threshold reached
        //    - Async: no-op (background thread handles)
        // 5. Return entry offset
    }

    pub fn read_entries(&self, start_offset: u64) -> Result<Vec<WALEntry>> {
        // 1. Open separate read handle (don't interfere with writer)
        // 2. Seek to start_offset
        // 3. Read entries sequentially:
        //    - Read length
        //    - Read type + payload + CRC
        //    - Verify CRC
        //    - Decode entry
        //    - Add to results
        // 4. Stop at: EOF, corruption, or incomplete entry
        // 5. Return entries
    }
}
```

**Corruption Handling**:
- **CRC mismatch**: Return CorruptionError with offset
- **Truncated entry**: Return entries read so far (graceful)
- **Invalid type tag**: Skip unknown types (forward compatibility)
- **Mid-file corruption**: Stop at first error (fail-safe)

---

### 3.4 Recovery Layer (`crates/durability/recovery`)

**Purpose**: Restore database state from WAL on startup.

#### 3.4.1 Recovery Protocol

```
┌──────────────────────────────────────────────────────────┐
│ Recovery Flow (Database::open)                           │
├──────────────────────────────────────────────────────────┤
│ 1. Open WAL file                                         │
│ 2. Create empty UnifiedStore                             │
│ 3. Call replay_wal(wal, storage, offset=0)              │
│    ├─ Scan WAL entries                                   │
│    ├─ Group by txn_id                                    │
│    ├─ Validate transaction boundaries                    │
│    ├─ Apply committed transactions only                  │
│    └─ Discard incomplete transactions                    │
│ 4. Return Database with restored state                   │
└──────────────────────────────────────────────────────────┘
```

#### 3.4.2 Replay Algorithm

```rust
pub fn replay_wal(
    wal: &WAL,
    storage: &dyn Storage,
    start_offset: u64,
) -> Result<ReplayStats> {
    let entries = wal.read_entries(start_offset)?;

    // Phase 1: Validate
    let validation = validate_transactions(&entries);
    validation.log_warnings();

    // Phase 2: Group by transaction
    let mut transactions: HashMap<u64, Transaction> = HashMap::new();

    for entry in entries {
        match entry {
            BeginTxn { txn_id, run_id, .. } => {
                transactions.insert(txn_id, Transaction {
                    txn_id,
                    run_id,
                    entries: vec![entry],
                    committed: false,
                });
            }
            Write { .. } | Delete { .. } => {
                if let Some(txn) = transactions.get_mut(&txn_id) {
                    txn.entries.push(entry);
                }
                // Orphaned entries (no BeginTxn) are skipped
            }
            CommitTxn { txn_id, .. } => {
                if let Some(txn) = transactions.get_mut(&txn_id) {
                    txn.committed = true;
                }
            }
            AbortTxn { txn_id, .. } => {
                transactions.remove(&txn_id);
            }
        }
    }

    // Phase 3: Apply committed transactions only
    let committed = transactions.values().filter(|t| t.committed);

    for txn in committed {
        for entry in &txn.entries {
            match entry {
                Write { key, value, .. } => storage.put(key, value, None)?,
                Delete { key, .. } => storage.delete(key)?,
                _ => {}
            }
        }
    }

    // Return stats
}
```

#### 3.4.3 Validation Rules

```rust
pub fn validate_transactions(entries: &[WALEntry]) -> ValidationResult {
    // Check 1: All writes belong to a transaction (have BeginTxn)
    // Check 2: No duplicate BeginTxn for same txn_id
    // Check 3: CommitTxn/AbortTxn have matching BeginTxn
    // Check 4: Identify incomplete transactions (BeginTxn without CommitTxn)

    // Returns:
    // - incomplete_txns: Vec<u64>
    // - orphaned_entries: usize
    // - warnings: Vec<ValidationWarning>
}
```

**Incomplete Transaction Handling**:
- **Definition**: BeginTxn + writes but NO CommitTxn
- **Cause**: Crash, kill -9, power loss during transaction
- **Action**: Discard all writes from incomplete transactions
- **Rationale**: Conservative (fail-safe), no guessing intent

**Orphaned Entry Handling**:
- **Definition**: Write/Delete without matching BeginTxn
- **Cause**: Corrupted WAL, missing entries
- **Action**: Skip with warning logged
- **Rationale**: Don't apply writes outside transaction boundaries

#### 3.4.4 Recovery Performance

**Optimization**: O(run size) replay via run_index (not O(WAL size))

Future (M5) optimization:
```rust
// Instead of scanning entire WAL:
fn replay_run(run_id: RunId) {
    let metadata = run_index.get_run(run_id)?;
    let entries = wal.scan_range(
        metadata.wal_start_offset..metadata.wal_end_offset
    )?;
    // Only replay entries for this run
}
```

**Performance Targets**:
- M1: > 2000 txns/sec recovery throughput
- M1: < 5 seconds for 10K transactions
- M1: Recovery uses < 100MB memory for 10K transactions

---

### 3.5 Engine Layer (`crates/engine`)

**Purpose**: Orchestrate all components, provide public API.

#### 3.5.1 Database Struct

```rust
pub struct Database {
    data_dir: PathBuf,
    storage: Arc<UnifiedStore>,
    wal: Arc<Mutex<WAL>>,
    durability_mode: DurabilityMode,
    run_tracker: Arc<RunTracker>,
    next_txn_id: AtomicU64,
}
```

**Responsibilities**:
- **Initialization**: Open WAL, create storage, trigger recovery
- **Run lifecycle**: begin_run(), end_run(), track active runs
- **Operations**: put(), get(), delete(), list() (simple, non-transactional in M1)
- **Coordination**: Atomically update storage + WAL
- **Shutdown**: Flush WAL, cleanup resources

#### 3.5.2 Run Tracking

```rust
pub struct RunTracker {
    active_runs: RwLock<HashMap<RunId, RunMetadataEntry>>,
}

pub struct RunMetadataEntry {
    pub run_id: RunId,
    pub parent_run_id: Option<RunId>,
    pub status: String,              // "running", "completed", "failed"
    pub created_at: Timestamp,
    pub completed_at: Option<Timestamp>,
    pub first_version: u64,          // Version when run started
    pub last_version: u64,           // Version when run ended
    pub tags: Vec<(String, String)>, // User metadata
}
```

**Run Lifecycle**:

```rust
impl Database {
    pub fn begin_run(&self, run_id: RunId, tags: Vec<(String, String)>) -> Result<()> {
        // 1. Create RunMetadataEntry with current version
        // 2. Store in storage (TypeTag::RunMetadata)
        // 3. Add to active_runs tracker
        // 4. Log to WAL (future: BeginRun entry type)
    }

    pub fn end_run(&self, run_id: RunId) -> Result<()> {
        // 1. Get metadata from active_runs
        // 2. Update: completed_at, last_version, status="completed"
        // 3. Write updated metadata to storage
        // 4. Remove from active_runs
        // 5. Log to WAL (future: EndRun entry type)
    }
}
```

**Why Run Metadata Persists**:
- Enables queries: "Show all completed runs"
- Enables lineage tracking (parent_run_id)
- Enables replay: find run boundaries in WAL
- Enables cleanup policies (TTL, retention)

#### 3.5.3 Basic Operations (M1)

```rust
impl Database {
    pub fn put(&self, run_id: RunId, key: &[u8], value: Value) -> Result<u64> {
        let txn_id = self.next_txn_id();

        // Acquire WAL lock (ensures atomicity)
        let mut wal = self.wal.lock().unwrap();

        // Write to WAL: BeginTxn → Write → CommitTxn
        wal.append(&WALEntry::BeginTxn { txn_id, run_id, .. })?;

        // Write to storage
        let key = Key::new_kv(namespace_for_run(run_id), key);
        let version = self.storage.put(key.clone(), value.clone(), None)?;

        // Log write
        wal.append(&WALEntry::Write { run_id, key, value, version })?;

        // Commit
        wal.append(&WALEntry::CommitTxn { txn_id, run_id })?;

        Ok(version)
    }

    pub fn get(&self, run_id: RunId, key: &[u8]) -> Result<Option<Value>> {
        let key = Key::new_kv(namespace_for_run(run_id), key);
        self.storage.get(&key).map(|opt| opt.map(|v| v.value))
    }
}
```

**Design Note**: Each operation is an implicit single-operation transaction in M1. M2 will add explicit multi-operation transactions.

---

### 3.6 Primitives Layer (`crates/primitives`)

**Purpose**: Domain-specific APIs as stateless facades over Database.

#### 3.6.1 KVStore Primitive

```rust
#[derive(Clone)]
pub struct KVStore {
    db: Arc<Database>,
}

impl KVStore {
    pub fn new(db: Arc<Database>) -> Self {
        Self { db }
    }

    pub fn get(&self, run_id: RunId, key: &[u8]) -> Result<Option<Vec<u8>>> {
        // Delegate to db.get(), extract bytes from Value::Bytes
    }

    pub fn put(&self, run_id: RunId, key: &[u8], value: Vec<u8>) -> Result<u64> {
        // Delegate to db.put() with Value::Bytes
    }

    pub fn delete(&self, run_id: RunId, key: &[u8]) -> Result<Option<Vec<u8>>> {
        // Delegate to db.delete(), extract bytes
    }

    pub fn list(&self, run_id: RunId, prefix: &[u8]) -> Result<Vec<(Vec<u8>, Vec<u8>)>> {
        // Delegate to db.list(), extract bytes from values
    }
}
```

**Primitive Pattern**:
- **Stateless**: No storage, no WAL, no indices (just Arc<Database>)
- **Facade**: Thin layer over Database methods
- **Domain API**: Provides type-specific interface (bytes for KV)
- **Cheap Clone**: Arc clone, not deep copy

**Future Primitives** (M3):
- EventLog: append(), read(), scan_chain()
- StateMachine: read_state(), cas()
- TraceStore: record(), query()
- RunIndex: create_run(), query_runs(), fork_run()

---

## 4. Data Models

### 4.1 Key Structure

```
Key = Namespace + TypeTag + user_key

Namespace = (tenant, app, agent, run_id)
TypeTag = {KV, Event, StateMachine, Trace, RunMetadata}
user_key = Vec<u8>
```

**Ordering**: `namespace → type_tag → user_key`

**Example Keys**:
```
("acme", "chatbot", "agent-42", run_123, KV, "session_state")
("acme", "chatbot", "agent-42", run_123, Event, "\x00\x00\x00\x01") // seq=1
("system", "in-mem", "run-tracker", run_123, RunMetadata, <run_id bytes>)
```

**Design Rationale**:
- Hierarchical namespace enables multi-tenancy
- TypeTag enables unified storage (one BTreeMap)
- Ordering enables efficient range scans
- user_key supports arbitrary keys (strings, binary, composite)

### 4.2 Value Structure

```rust
pub enum Value {
    Bytes(Vec<u8>),           // KV primitive
    Event(EventEntry),        // Event log primitive (M3)
    StateMachineRecord(StateMachineEntry), // State machine primitive (M3)
    Trace(TraceEntry),        // Trace primitive (M3)
    RunMetadata(RunMetadataEntry), // Run index primitive
}

pub struct VersionedValue {
    value: Value,
    version: u64,             // Assigned by storage
    timestamp: Timestamp,     // Write time
    ttl: Option<Duration>,    // Expiration
}
```

**Version Assignment**:
- Monotonically increasing (AtomicU64)
- Assigned at put() time
- Never reused
- Critical for snapshot isolation (M2)

### 4.3 WAL Transaction Structure

```
Transaction := BeginTxn → Write* → Delete* → (CommitTxn | AbortTxn)

Valid:   BeginTxn(1) → Write → Write → CommitTxn(1)
Invalid: BeginTxn(1) → Write → [crash, no CommitTxn]
Valid:   BeginTxn(1) → Write → AbortTxn(1)
```

**Invariants**:
- Every Write/Delete must have matching BeginTxn (same txn_id)
- Transactions either committed or incomplete (no partial commits)
- txn_id unique within WAL (never reused)

---

## 5. Layer Boundaries

### 5.1 Dependency Graph

```
Core Types
    ↑
    ├── Storage ────────┐
    │                   │
    ├── Durability ─────┤
    │                   │
    └── Engine ─────────┤
            ↑           │
        Primitives ─────┘
```

**Rules**:
- Core types have no dependencies
- Storage and Durability depend only on Core
- Engine depends on Storage, Durability, Core
- Primitives depend only on Engine, Core

### 5.2 Interface Contracts

**Storage Layer**:
- **Provides**: CRUD operations, versioning, indexing
- **Does NOT know about**: WAL, transactions, primitives, recovery
- **Contract**: `Storage` trait (enables replacement)

**Durability Layer**:
- **Provides**: WAL append, WAL read, encoding/decoding
- **Does NOT know about**: Storage, primitives, engine
- **Contract**: WAL file format (enables compatibility)

**Engine Layer**:
- **Provides**: Orchestration, run lifecycle, operation APIs
- **Does NOT know about**: Primitive-specific logic
- **Contract**: Public API (begin_run, put, get, etc.)

**Primitives Layer**:
- **Provides**: Domain-specific APIs
- **Does NOT know about**: Storage, WAL, other primitives
- **Contract**: Each primitive API (KVStore, EventLog, etc.)

### 5.3 Anti-Patterns (DO NOT DO)

❌ **Primitives calling Storage directly**
```rust
// WRONG
impl KVStore {
    fn get(&self, key: &[u8]) -> Result<Value> {
        self.storage.get(&key) // Bypasses engine
    }
}
```

❌ **Storage knowing about primitives**
```rust
// WRONG
impl UnifiedStore {
    fn put_event(&self, event: Event) { ... } // Storage shouldn't know Event
}
```

❌ **Primitives calling each other**
```rust
// WRONG
impl StateMachine {
    fn cas(&self, ...) {
        self.trace_store.record(...); // Cross-primitive dependency
    }
}
```

✅ **Correct: Primitives delegate to Engine**
```rust
impl KVStore {
    fn get(&self, run_id: RunId, key: &[u8]) -> Result<Option<Vec<u8>>> {
        self.db.get(run_id, key) // Delegate to engine
            .map(|opt| opt.and_then(|v| match v { Value::Bytes(b) => Some(b), _ => None }))
    }
}
```

---

## 6. Concurrency Model

### 6.1 M1 Concurrency (Simple)

**Locking Strategy**:
```
UnifiedStore:
  data: Arc<RwLock<BTreeMap>>        // Many readers OR one writer
  run_index: Arc<RwLock<...>>        // Separate lock (can parallelize)
  type_index: Arc<RwLock<...>>       // Separate lock
  ttl_index: Arc<RwLock<...>>        // Separate lock

WAL:
  writer: Arc<Mutex<WAL>>            // Only one writer at a time

Database:
  storage: Arc<UnifiedStore>         // Shared across threads
  wal: Arc<Mutex<WAL>>               // Shared across threads
```

**Thread Safety**:
- Database is `Send + Sync` (can be shared with Arc)
- Storage trait requires `Send + Sync`
- Operations are atomic (storage + WAL updated together via locks)

**Known Bottlenecks**:
- RwLock on data: writers block all readers
- WAL Mutex: serializes all writes
- Global version counter: AtomicU64 contention

**Acceptable for M1**: Agents typically don't have high write concurrency.

### 6.2 M2 Concurrency (OCC)

Future (M2): Optimistic Concurrency Control
- Snapshot isolation (ClonedSnapshotView)
- Read set tracking
- Conflict detection at commit
- Retry on conflict

### 6.3 Background Tasks

**TTL Cleanup Thread**:
```rust
loop {
    sleep(check_interval);
    let expired = storage.find_expired_keys(now);
    for key in expired {
        storage.delete(&key); // Transactional delete
    }
}
```

**Async Fsync Thread** (if DurabilityMode::Async):
```rust
loop {
    sleep(interval);
    wal.flush();
    wal.fsync();
}
```

**Design**: Simple dedicated threads, no task scheduler needed for MVP.

---

## 7. Durability Strategy

### 7.1 Durability Guarantees

| Mode | Guarantee | Loss Window | Use Case |
|------|-----------|-------------|----------|
| **Strict** | Every committed transaction durable | None | Critical data |
| **Batched** (default) | Transactions fsynced every 100ms or 1000 commits | ≤ 100ms | Agent workflows |
| **Async** | Background fsync every interval | ≤ interval | Scratch data |

**Trade-off**: Speed vs. Durability
- Strict: 100% durable, ~10ms latency/write
- Batched: 99.9% durable, <1ms latency/write
- Async: Variable durability, <0.1ms latency/write

**Default Rationale**: Agents value throughput over perfect durability. Losing 100ms of writes (e.g., 10-100 operations) is acceptable trade-off for 10x speed improvement.

### 7.2 Crash Scenarios

**Scenario 1: Clean Shutdown**
- Drop handler calls wal.flush() + wal.fsync()
- All committed transactions durable
- Recovery: replay WAL, all transactions committed

**Scenario 2: Kill -9 (Strict Mode)**
- Every CommitTxn fsynced before returning
- All committed transactions durable
- Recovery: replay WAL, all committed transactions present

**Scenario 3: Kill -9 (Batched Mode)**
- Recent transactions (last 100ms) may be lost
- Transactions before last fsync are durable
- Recovery: replay WAL, discard incomplete transactions

**Scenario 4: Power Loss Mid-Write**
- Partial WAL entry written (no CRC or truncated)
- Recovery: stop at incomplete entry, graceful
- Data before incomplete entry is valid

**Scenario 5: Disk Corruption**
- CRC mismatch detected during read
- Recovery: stop at first corruption, return error
- Data before corruption is valid (fail-safe)

### 7.3 WAL Lifecycle (M1)

**M1 Behavior**:
- Single WAL file: `<data_dir>/wal/current.wal`
- Append-only, grows unbounded
- No rotation, no truncation
- Recovery always scans from offset 0

**M4 Enhancement** (future):
- Periodic snapshots save full storage state
- WAL truncation after snapshot
- Recovery: load snapshot + replay WAL from checkpoint
- Bounded WAL size

---

## 8. Recovery Protocol

### 8.1 Startup Recovery Flow

```
Database::open(path)
    │
    ├─► 1. Create data directory
    │
    ├─► 2. Open WAL (creates if doesn't exist)
    │       └─► wal_path = <path>/wal/current.wal
    │
    ├─► 3. Create empty UnifiedStore
    │
    ├─► 4. Replay WAL
    │       │
    │       ├─► read_entries(offset=0)
    │       │       └─► Decode entries, verify CRC
    │       │
    │       ├─► validate_transactions(entries)
    │       │       ├─► Check transaction boundaries
    │       │       ├─► Find incomplete transactions
    │       │       └─► Log warnings
    │       │
    │       ├─► Group by txn_id
    │       │       └─► Build Transaction structs
    │       │
    │       └─► Apply committed transactions
    │               ├─► For each committed txn:
    │               │   ├─► Apply Write → storage.put()
    │               │   └─► Apply Delete → storage.delete()
    │               └─► Discard incomplete transactions
    │
    ├─► 5. Log recovery stats
    │       └─► "X txns applied, Y writes, Z deletes, W discarded"
    │
    └─► 6. Return Database (ready for use)
```

### 8.2 Recovery Validation

**Validation Checks**:
1. **Transaction Structure**: Every Write/Delete has matching BeginTxn
2. **Completion**: Transactions have CommitTxn or are incomplete
3. **Orphaned Entries**: Warn about Write/Delete without BeginTxn
4. **Duplicate BeginTxn**: Warn about txn_id reuse

**Validation Output**:
```rust
pub struct ValidationResult {
    pub incomplete_txns: Vec<u64>,     // Txns without CommitTxn
    pub orphaned_entries: usize,       // Entries without BeginTxn
    pub warnings: Vec<ValidationWarning>,
}
```

**Logging**:
```
WARNING: 3 incomplete transactions will be discarded: [12, 45, 67]
WARNING: 2 orphaned entries will be skipped
WARNING [offset 1234]: CommitTxn without BeginTxn for txn_id 89
```

### 8.3 Recovery Correctness

**Invariants Preserved**:
1. **Atomicity**: Only complete transactions applied
2. **Consistency**: Storage state matches WAL committed transactions
3. **Durability**: All fsynced transactions restored
4. **Isolation**: Run-scoped data remains isolated

**Test Strategy**:
- **Crash simulation**: Kill process at various points, verify recovery
- **Corruption simulation**: Flip bits in WAL, verify detection
- **Incomplete transaction**: Write BeginTxn + Write, no CommitTxn, verify discarded
- **Large WAL**: 10K transactions, verify recovery < 5 seconds

---

## 9. API Design

### 9.1 Database API (Public)

```rust
// Initialization
pub fn Database::open(path: impl AsRef<Path>) -> Result<Database>
pub fn Database::open_with_mode(path: impl AsRef<Path>, mode: DurabilityMode) -> Result<Database>

// Run Lifecycle
pub fn begin_run(&self, run_id: RunId, tags: Vec<(String, String)>) -> Result<()>
pub fn end_run(&self, run_id: RunId) -> Result<()>
pub fn get_run(&self, run_id: RunId) -> Result<Option<RunMetadataEntry>>
pub fn list_active_runs(&self) -> Vec<RunId>

// Basic Operations (M1: single-operation transactions)
pub fn put(&self, run_id: RunId, key: &[u8], value: Value) -> Result<u64>
pub fn put_with_ttl(&self, run_id: RunId, key: &[u8], value: Value, ttl: Duration) -> Result<u64>
pub fn get(&self, run_id: RunId, key: &[u8]) -> Result<Option<Value>>
pub fn delete(&self, run_id: RunId, key: &[u8]) -> Result<Option<Value>>
pub fn list(&self, run_id: RunId, prefix: &[u8]) -> Result<Vec<(Vec<u8>, Value)>>

// Maintenance
pub fn flush(&self) -> Result<()>
```

### 9.2 KVStore API

```rust
pub fn KVStore::new(db: Arc<Database>) -> KVStore

// All operations require run_id (run-scoped)
pub fn get(&self, run_id: RunId, key: &[u8]) -> Result<Option<Vec<u8>>>
pub fn put(&self, run_id: RunId, key: &[u8], value: Vec<u8>) -> Result<u64>
pub fn put_with_ttl(&self, run_id: RunId, key: &[u8], value: Vec<u8>, ttl: Duration) -> Result<u64>
pub fn delete(&self, run_id: RunId, key: &[u8]) -> Result<Option<Vec<u8>>>
pub fn list(&self, run_id: RunId, prefix: &[u8]) -> Result<Vec<(Vec<u8>, Vec<u8>)>>
pub fn exists(&self, run_id: RunId, key: &[u8]) -> Result<bool>
```

**Design Principles**:
- **Run-scoped**: All operations take run_id (fundamental to agent model)
- **Type-safe**: KVStore returns `Vec<u8>`, not generic `Value`
- **Stateless**: KVStore is cheap to clone (just Arc)
- **Idiomatic**: Rust conventions (Result, Option, &[u8] for keys)

### 9.3 Error Handling

**Error Philosophy**: Errors are actionable and include context.

**Error Types**:
```rust
pub enum Error {
    Storage(StorageError),       // Storage operations
    Durability(DurabilityError), // WAL, recovery
    Concurrency(ConcurrencyError), // Future: OCC conflicts
    Primitive(PrimitiveError),   // Primitive-specific
}
```

**Example Errors**:
```rust
// Storage
StorageError::NotFound { key }
StorageError::Expired { key, version }

// Durability
DurabilityError::CorruptionError { offset, message: "CRC mismatch" }
DurabilityError::WALError { message: "Failed to fsync" }

// Primitive
PrimitiveError::TypeMismatch { expected: "Bytes", actual: "Event" }
```

**Error Context**: Every error includes relevant context (key, version, offset, message).

---

## 10. Performance Characteristics

### 10.1 Expected Performance (M1 MVP)

| Operation | Latency (Strict) | Latency (Batched) | Throughput |
|-----------|------------------|-------------------|------------|
| **put()** | ~10ms | <1ms | 1K-10K ops/sec |
| **get()** | <0.1ms | <0.1ms | 100K+ ops/sec |
| **delete()** | ~10ms | <1ms | 1K-10K ops/sec |
| **Recovery** | - | - | >2K txns/sec |

**Notes**:
- Strict mode: dominated by fsync (~10ms)
- Batched mode: dominated by RwLock + encoding (~0.1-1ms)
- get() is fast (no WAL, just BTreeMap read)
- Recovery throughput depends on I/O (sequential WAL read)

### 10.2 Bottlenecks (Known)

| Bottleneck | Impact | Mitigation |
|------------|--------|------------|
| **RwLock on data** | Writers block readers | M4: Sharded storage |
| **WAL Mutex** | Serializes writes | M4: Parallel WAL writers |
| **Global version counter** | AtomicU64 contention | M4: Per-namespace versions |
| **fsync** | 10ms latency | Use batched mode |
| **Snapshot cloning** | Memory + CPU | M2: Lazy snapshots |

**Acceptance Criteria**: M1 is fast enough for single-agent workflows. Multi-agent concurrency optimization deferred to M4.

### 10.3 Scalability Limits (M1)

| Dimension | Limit | Reason |
|-----------|-------|--------|
| **Data size** | 10GB | In-memory BTreeMap |
| **Concurrent writes** | 100s/sec | RwLock contention |
| **WAL size** | 100MB-1GB | No rotation (M4 adds snapshots) |
| **Recovery time** | <1 min for 100K txns | Sequential WAL scan |

**Design for Future**:
- M4: Snapshots + WAL rotation (unbounded data)
- M4: Sharded storage (higher write concurrency)
- M5: Incremental replay (faster recovery)

---

## 11. Testing Strategy

### 11.1 Unit Tests

**Per Component**:
- **Core types**: Serialization, ordering, trait implementations
- **Storage**: put/get/delete, versioning, indices, TTL
- **WAL**: Encoding/decoding, durability modes, corruption detection
- **Recovery**: Replay logic, validation, incomplete transactions
- **Engine**: Run lifecycle, operations, coordination
- **Primitives**: API correctness, delegation to engine

**Coverage Target**: >90% line coverage

### 11.2 Integration Tests

**Scenarios**:
1. **End-to-end**: Write via KV → close → reopen → read via KV
2. **Multiple runs**: Write to 3 runs → restart → verify isolation
3. **Large dataset**: 1000 keys → restart → verify all restored
4. **TTL**: Write with short/long TTL → restart → verify expiration
5. **Run metadata**: Create run with tags → restart → verify metadata

**Target**: All M1 components working together correctly.

### 11.3 Crash Simulation Tests

**Scenarios**:
1. **Kill after BeginTxn**: Incomplete transaction discarded
2. **Kill after CommitTxn (strict)**: Transaction restored
3. **Kill mid-batch (batched)**: Some transactions lost (expected)
4. **Multiple incomplete**: All discarded correctly
5. **Mixed committed/incomplete**: Only committed restored

**Method**: Spawn subprocess, write data, kill -9, verify recovery.

### 11.4 Corruption Simulation Tests

**Scenarios**:
1. **Bit flip in payload**: CRC detects corruption
2. **Truncated entry**: Gracefully stop at incomplete
3. **Corrupt length field**: Return error with offset
4. **Multiple corruptions**: Stop at first error

**Method**: Write valid WAL, corrupt bytes at offsets, verify detection.

### 11.5 Performance Tests

**Benchmarks**:
1. **Recovery throughput**: 10K transactions → measure recovery time
2. **Write throughput**: Concurrent writes → measure ops/sec
3. **Read throughput**: Concurrent reads → measure ops/sec
4. **Large WAL**: 100K transactions → measure recovery time

**Targets**:
- Recovery: >2K txns/sec
- Write (batched): >1K ops/sec
- Read: >100K ops/sec

### 11.6 Property-Based Testing

**Properties** (future):
1. **Replay determinism**: Same WAL → same final state
2. **Version monotonicity**: Versions always increase
3. **Transaction atomicity**: All writes or none
4. **Isolation**: Different runs see different data

**Tool**: proptest or quickcheck

---

## 12. Known Limitations

### 12.1 Accepted MVP Limitations

| Limitation | Impact | Mitigation Plan |
|------------|--------|----------------|
| **RwLock contention** | Writers block readers | M4: Sharded storage (Storage trait enables swap) |
| **Global version counter** | AtomicU64 hotspot | M4: Per-namespace versions |
| **Snapshot cloning** | Memory + CPU cost | M2: LazySnapshotView (SnapshotView trait enables swap) |
| **No version history** | Can't query old versions | M2+: If needed, keep version chains |
| **WAL grows unbounded** | Disk usage | M4: Snapshots + WAL truncation |
| **Single WAL writer** | Write serialization | M4: Parallel WAL writers |
| **No query DSL** | Manual filtering | M9: Add query DSL |

**Philosophy**: Ship M1 with known limitations. Optimize in M4+ with measurement, not speculation.

### 12.2 What M1 Does NOT Provide

- ❌ Multi-operation transactions (M2)
- ❌ Event Log primitive (M3)
- ❌ State Machine primitive (M3)
- ❌ Trace primitive (M3)
- ❌ Run Index primitive (M3)
- ❌ Snapshots (M4)
- ❌ WAL rotation (M4)
- ❌ Deterministic replay (M5)
- ❌ Run diffing (M5)
- ❌ Vector store (M6)
- ❌ Network layer (M7)

### 12.3 Future Work (Beyond M1)

**M2: Transactions**
- Optimistic Concurrency Control (OCC)
- Snapshot isolation
- Compare-and-swap (CAS)
- Conflict detection and retry

**M3: Primitives**
- Event Log with simple chaining
- State Machine for coordination
- Trace Store for reasoning logs
- Run Index with metadata queries

**M4: Durability**
- Periodic snapshots
- WAL truncation
- Crash simulation at scale

**M5: Replay**
- Deterministic replay_run(run_id)
- Run diffing (diff_runs)
- Lineage tracking

---

## 13. Future Extension Points

### 13.1 Designed for Evolution

**Storage Trait**:
```rust
pub trait Storage: Send + Sync {
    // MVP: UnifiedStore (BTreeMap + RwLock)
    // M4: ShardedStore (per-namespace BTreeMaps)
    // M6: LockFreeStore (crossbeam SkipMap)
}
```

**SnapshotView Trait**:
```rust
pub trait SnapshotView: Send + Sync {
    // MVP: ClonedSnapshotView (deep clone)
    // M2: LazySnapshotView (version-bounded reads)
}
```

**WAL Format**:
- Type tags enable forward compatibility (skip unknown types)
- Length prefix enables skipping unknown entries
- Versioned format (can add new entry types)

### 13.2 Hooks for Future Features

**Run Forking** (M5):
- RunMetadataEntry has `parent_run_id: Option<RunId>`
- Enables lineage tracking
- Enables diff_runs from fork point

**Incremental Snapshots** (M4):
- Snapshot metadata includes version
- Enables incremental snapshots (diff from last snapshot version)

**Sharded Storage** (M4):
- Storage trait abstraction allows replacing UnifiedStore
- Namespace already structured (tenant → app → agent)
- Can shard by namespace prefix

**Query DSL** (M9):
- Storage already has scan_prefix and indices
- Can build query planner on top
- No storage changes needed

### 13.3 Non-Goals (Explicitly Out of Scope)

- **Distributed consensus**: Single-node only
- **SQL interface**: DSL, not SQL
- **ACID across databases**: Single database only
- **Hot backup**: Cold backups only (copy data dir)
- **Schema enforcement**: Schemaless by design

---

## 14. Appendix

### 14.1 Crate Structure

```
in-mem/
├── Cargo.toml                    # Workspace root
├── crates/
│   ├── core/                     # Foundation types
│   │   ├── src/
│   │   │   ├── types.rs          # RunId, Namespace, Key, TypeTag
│   │   │   ├── value.rs          # Value, VersionedValue
│   │   │   ├── error.rs          # Error hierarchy
│   │   │   └── traits.rs         # Storage, SnapshotView traits
│   │   └── Cargo.toml
│   │
│   ├── storage/                  # Storage layer
│   │   ├── src/
│   │   │   ├── unified.rs        # UnifiedStore
│   │   │   ├── index.rs          # Secondary indices
│   │   │   ├── snapshot.rs       # ClonedSnapshotView
│   │   │   └── ttl.rs            # TTL cleanup
│   │   └── Cargo.toml
│   │
│   ├── durability/               # WAL and recovery
│   │   ├── src/
│   │   │   ├── wal.rs            # WAL implementation
│   │   │   ├── encoding.rs       # Entry encoding/decoding
│   │   │   └── recovery.rs       # Recovery logic
│   │   └── Cargo.toml
│   │
│   ├── engine/                   # Main engine
│   │   ├── src/
│   │   │   ├── database.rs       # Database struct
│   │   │   └── run.rs            # Run tracking
│   │   └── Cargo.toml
│   │
│   └── primitives/               # Primitives layer
│       ├── src/
│       │   └── kv.rs             # KVStore primitive
│       └── Cargo.toml
│
├── tests/                        # Integration tests
│   ├── integration_test.rs
│   ├── crash_simulation_test.rs
│   └── recovery_performance_test.rs
│
└── benches/                      # Benchmarks
    └── storage_bench.rs
```

### 14.2 Dependencies

**External Dependencies** (minimal):
- `uuid`: RunId generation
- `serde`: Serialization
- `bincode`: WAL encoding
- `crc32fast`: Checksums
- `parking_lot`: Efficient RwLock

**Internal Dependencies**:
```
core → (none)
storage → core
durability → core
engine → core, storage, durability
primitives → core, engine
```

### 14.3 File Naming Conventions

- `types.rs`: Type definitions
- `traits.rs`: Trait definitions
- `error.rs`: Error types
- `*_test.rs`: Unit tests (in src/)
- `integration_test.rs`: Integration tests (in tests/)

### 14.4 Glossary

| Term | Definition |
|------|------------|
| **Run** | Labeled execution of an agent (unique RunId) |
| **Namespace** | Hierarchical identifier (tenant/app/agent/run) |
| **TypeTag** | Discriminator for primitive types in unified storage |
| **VersionedValue** | Value with version, timestamp, TTL metadata |
| **WAL** | Write-Ahead Log (append-only durability log) |
| **Recovery** | Restoring database state from WAL on startup |
| **Primitive** | Domain-specific API (KVStore, EventLog, etc.) |
| **Snapshot** | Version-bounded read view of storage |
| **OCC** | Optimistic Concurrency Control (M2) |

---

## Conclusion

M1 establishes the **foundation** for the agent database:
- ✅ Durable storage with versioning
- ✅ Crash recovery via WAL
- ✅ Run-scoped operations
- ✅ Clean layer boundaries
- ✅ Extension points for M2+

**Success Criteria**:
- All 27 user stories implemented
- Integration test passes (write → restart → read)
- Recovery handles crashes correctly
- Performance meets targets (>2K txns/sec recovery)

**Next**: M2 adds transactions, M3 adds remaining primitives, M4 adds snapshots, M5 adds replay.

---

**Document Version**: 1.0
**Approved By**: [Engineering Lead]
**Date**: 2026-01-10
