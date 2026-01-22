# M10 Architecture Specification: Storage Backend, Retention, and Compaction

**Version**: 1.1
**Status**: Implementation Ready
**Last Updated**: 2026-01-20

---

## Executive Summary

This document specifies the architecture for **Milestone 10 (M10): Storage Backend, Retention, and Compaction** of the Strata database. M10 makes Strata durable and portable without changing substrate semantics, delivering a disk-backed storage layer with WAL, snapshots, user-configurable retention, and deterministic compaction.

**THIS DOCUMENT IS AUTHORITATIVE.** All M10 implementation must conform to this specification.


**M10 Philosophy**:
> Storage is infrastructure, not semantics. The user interacts with the same seven primitives through the same API. They do not know (and should not care) whether state lives in memory or on disk. M10 adds durability and portability. It does not change what the database *means*.
>
> Retention and compaction are user-controlled operations. The system provides tools; the user decides policy. No background magic. No surprising deletions. Deterministic, observable, controllable.

**Critical Architectural Warning**:
> **M10 does NOT make Strata a "disk-first" database.** Disk is a persistence layer, not the primary interface. The engine remains the source of truth for semantics.
>
> This distinction prevents:
> - "Just read from disk" shortcuts that bypass the engine
> - Storage details leaking into APIs
> - Performance hacks that break invariants
> - Strata becoming a file format instead of a substrate

**M10 Goals**:
- Persist all committed transactions to disk (durability modes supported)
- Recover correctly after crash by replaying WAL over the latest snapshot
- Support database growth beyond RAM (storage is authoritative, memory is cache)
- Support user-configurable retention policies with safe defaults
- Support compaction as a deterministic, user-triggerable operation
- Produce portable database artifacts suitable for local embedding and offline transfer
- Introduce a storage "codec seam" to allow future encryption-at-rest without redesign

**M10 Non-Goals** (Deferred):
- Encryption implementation details (beyond the codec seam)
- Background compaction tuning / adaptive heuristics
- Incremental snapshots
- Online defragmentation
- Multi-node replication
- Sharding across processes
- Tiered storage (S3, object stores, etc.)

**Critical Constraint**:
> M10 is an infrastructure milestone, not a feature milestone. It adds durability without changing semantics. If a change affects user-visible behavior beyond durability guarantees, it is out of scope.

**Built on M1-M9**:
- M1 provides: In-memory storage, basic WAL infrastructure
- M2 provides: OCC transactions, Snapshot isolation, Conflict detection
- M3 provides: Five primitives (KVStore, EventLog, StateCell, TraceStore, RunIndex)
- M4 provides: Durability modes, ShardedStore
- M5 provides: JsonStore primitive
- M6 provides: Retrieval surface with search
- M7 provides: Snapshots, crash recovery concepts, deterministic replay
- M8 provides: Vector primitive
- M9 provides: Universal Protocol, API stabilization, Versioned<T>
- M10 adds: Disk-backed storage, retention policies, compaction, portable artifacts

---

## Table of Contents

1. [Scope Boundaries](#1-scope-boundaries)
2. [THE FIVE ARCHITECTURAL RULES](#2-the-five-architectural-rules-non-negotiable)
3. [Core Invariants](#3-core-invariants)
4. [Storage Artifact Format](#4-storage-artifact-format)
5. [WAL Contract](#5-wal-contract)
6. [Snapshot and Checkpoint](#6-snapshot-and-checkpoint)
7. [Recovery Algorithm](#7-recovery-algorithm)
8. [Retention Policy](#8-retention-policy)
9. [Compaction](#9-compaction)
10. [Codec Seam](#10-codec-seam)
11. [Public API Surface](#11-public-api-surface)
12. [Database Open/Close](#12-database-openclose)
13. [Testing Strategy](#13-testing-strategy)
14. [Known Limitations](#14-known-limitations)
15. [Future Extension Points](#15-future-extension-points)
16. [Success Criteria Checklist](#16-success-criteria-checklist)

---

## 1. Scope Boundaries

### 1.1 What M10 IS

M10 is an **infrastructure milestone**. It defines:

| Aspect | M10 Commits To |
|--------|---------------|
| **Storage Format** | Portable `strata.db/` directory with MANIFEST, WAL, SNAPSHOTS |
| **WAL** | Append-only, segmented, deterministic replay, durability modes |
| **Snapshots** | Point-in-time materialization, crash-safe creation |
| **Recovery** | Snapshot + WAL replay, idempotent, correct ordering |
| **Retention** | KeepAll, KeepLast(N), KeepFor(Duration) as database entries |
| **Compaction** | WALOnly and Full modes, user-triggered, deterministic |
| **Codec Seam** | Identity codec, extensible for future encryption |
| **Portability** | Copy directory = clone database |

### 1.2 What M10 is NOT

M10 is **not** a feature milestone. These are explicitly deferred:

| Deferred Item | Why Deferred | Target Milestone |
|---------------|--------------|------------------|
| Encryption implementation | Complexity | Post-MVP |
| Background compaction | Policy complexity | Post-MVP |
| Incremental snapshots | Optimization | Post-MVP |
| Multi-node replication | Far future | Post-MVP |
| Tiered storage (S3, etc.) | Far future | Post-MVP |
| Online defragmentation | Optimization | Post-MVP |

### 1.3 The Risk We Are Avoiding

Without proper storage architecture:
- Data is lost on crash
- Database cannot grow beyond RAM
- No portability (cannot backup, transfer, or clone)
- No control over disk usage growth
- No path to encryption-at-rest

**M10 provides the foundation.** All future durability, portability, and security features build on these primitives.

### 1.4 Evolution Warnings

**These are explicit warnings about M10 design decisions:**

#### A. MANIFEST Is Physical Metadata Only

The MANIFEST file contains only physical storage metadata:
- Format version, database UUID, codec ID
- Active WAL segment, latest snapshot watermark

**Retention policies are NOT in MANIFEST.** They are first-class database entries, stored through the same primitives as user data. This keeps MANIFEST minimal and makes policies versioned, transactional, and introspectable.

#### B. Checkpoint + Copy Is Canonical

`export()` and `import()` are convenience wrappers. The canonical mechanism is:
1. `checkpoint()` creates a stable boundary
2. Copy the `strata.db/` directory
3. Open the copy as a new database

The convenience API exists for user experience but delegates to checkpoint internally.

#### C. Retention Policies Are Data, Not Config

Retention policies are stored as regular database entries:
- Versioned (tracked like any other data)
- Transactional (can be updated atomically with other changes)
- Introspectable (can be read, audited, modified through normal APIs)
- Recoverable (part of WAL replay, part of snapshots)

Bootstrap default is in code, not MANIFEST.

#### D. WAL Segment Size Is Configurable

WAL segment size is configurable with a default (64MB recommended). Users who know their workload can tune this. Most users use the default.

---

## 2. THE FIVE ARCHITECTURAL RULES (NON-NEGOTIABLE)

**These rules MUST be followed in ALL M10 implementation. Violating any of these is a blocking issue.**

### Rule 1: Storage Is Logically Invisible

> **The storage layer must not change user-visible semantics. Before and after M10, the seven primitives behave identically.**

```rust
// CORRECT: Same API, durable storage
let db = Database::open("./strata.db", config)?;
let kv = db.kv();
kv.put(run_id, "key", value)?;  // Persisted according to durability mode

// WRONG: Storage-specific API leak
let db = Database::open("./strata.db", config)?;
let kv = db.kv();
kv.put_to_wal(run_id, "key", value)?;  // Storage details leaked to API
```

**Why**: Users interact with primitives, not storage. Storage is an implementation detail.

### Rule 2: Durability Mode Determines Commit Semantics

> **Transaction commit semantics depend on durability mode. Storage must respect this.**

```rust
// CORRECT: Durability mode controls fsync behavior
pub enum DurabilityMode {
    InMemory,   // No WAL persistence
    Buffered,   // WAL appended, fsync on coarse boundary
    Strict,     // fsync before acknowledging commit
}

// WRONG: Ignoring durability mode
fn commit(&self, txn: Transaction) -> Result<Version> {
    self.wal.append(txn.writeset())?;
    self.wal.fsync()?;  // Always fsync - ignores durability mode
    Ok(version)
}
```

**Why**: Users choose durability vs performance tradeoffs. Storage must honor their choice.

### Rule 3: Recovery Is Deterministic and Idempotent

> **Replaying the same WAL produces identical state. Replaying a record twice produces the same result as replaying once.**

```rust
// CORRECT: Deterministic replay
fn replay_record(&mut self, record: WalRecord) -> Result<()> {
    // Apply writeset - produces same state regardless of how many times called
    for mutation in record.writeset {
        self.apply_mutation(mutation)?;
    }
    Ok(())
}

// WRONG: Non-idempotent replay
fn replay_record(&mut self, record: WalRecord) -> Result<()> {
    self.version_counter += 1;  // Counter changes on each replay!
    // ...
}
```

**Why**: Crash recovery may replay records multiple times (crash during recovery). Idempotence ensures correctness.

### Rule 4: Compaction Is Logically Invisible

> **Compaction must not change the result of reading any retained version. It reclaims space, nothing more.**

```rust
// CORRECT: Compaction only removes unreachable data
fn compact(&mut self, mode: CompactMode) -> Result<CompactInfo> {
    let watermark = self.latest_snapshot_watermark();

    // Remove WAL segments fully covered by snapshot
    let removed = self.remove_wal_segments_before(watermark)?;

    // If Full mode, also apply retention policy
    if mode == CompactMode::Full {
        self.apply_retention_policy()?;
    }

    Ok(CompactInfo { ... })
}

// WRONG: Compaction changes visible state
fn compact(&mut self) -> Result<()> {
    self.rewrite_all_keys();  // Might change version numbers!
}
```

**Why**: Compaction is disk management. It must not affect read results or transaction semantics.

### Rule 5: Retention Policies Are Database Entries

> **Retention policies are stored as first-class database entries, not configuration files.**

```rust
// CORRECT: Retention policy as database entry
pub fn set_retention_policy(&self, run_id: RunId, policy: RetentionPolicy) -> Result<Version> {
    // Store as a special KV entry, versioned and transactional
    self.kv.put(run_id, RETENTION_POLICY_KEY, policy.to_value())?
}

pub fn get_retention_policy(&self, run_id: RunId) -> Result<Option<Versioned<RetentionPolicy>>> {
    // Read like any other entry
    self.kv.get(run_id, RETENTION_POLICY_KEY)
}

// WRONG: Retention in MANIFEST or config file
fn load_retention_policy(&self) -> RetentionPolicy {
    self.manifest.retention_policy.clone()  // Not versioned, not transactional
}
```

**Why**: Policies should be auditable, versioned, and recoverable through normal database mechanisms.

---

## 3. Core Invariants

### 3.1 Storage Invariants

| # | Invariant | Meaning |
|---|-----------|---------|
| S1 | WAL is append-only | Records can only be appended, never modified in place |
| S2 | WAL segments immutable once closed | Only active segment is writable; closed segments never change |
| S3 | WAL records are self-delimiting | Each record contains its length and checksum |
| S4 | Snapshots are consistent | Snapshot represents a single logical point in time |
| S5 | Snapshots are logical | Snapshots persist materialized state, not physical memory |
| S6 | Watermark ordering | Snapshot watermark ≤ all WAL records after it |
| S7 | MANIFEST atomicity | MANIFEST updates are atomic (write-fsync-rename) |
| S8 | Codec pass-through | All persisted bytes pass through codec boundary |
| S9 | Storage never assigns versions | Versions come from engine; storage only persists them |

### 3.2 Recovery Invariants

| # | Invariant | Meaning |
|---|-----------|---------|
| R1 | No committed txn lost | In Strict mode, committed transactions survive crash |
| R2 | Order preservation | WAL replay preserves transaction order |
| R3 | Idempotent replay | Replaying a record multiple times = replaying once |
| R4 | Snapshot-WAL equivalence | Snapshot + WAL replay = pure WAL replay |
| R5 | Partial record truncation | Incomplete records at WAL tail are safely truncated |

### 3.3 Retention Invariants

| # | Invariant | Meaning |
|---|-----------|---------|
| RT1 | Version ordering preserved | Retained versions maintain their relative order |
| RT2 | No silent fallback | Reads don't silently return nearest available version |
| RT3 | Explicit unavailability | Trimmed versions return HistoryTrimmed error |
| RT4 | Policy is versioned | Retention policy changes are tracked like data |

### 3.4 Compaction Invariants

| # | Invariant | Meaning |
|---|-----------|---------|
| C1 | Read equivalence | Before/after compaction, retained reads match exactly |
| C2 | No semantic change | Compaction doesn't affect transaction semantics |
| C3 | No reordering | Compaction doesn't reorder visible history |
| C4 | Safe boundaries | Compaction only removes data below snapshot watermark |
| C5 | Version identity | Compaction never rewrites, renumbers, or reinterprets versions |

---

## 4. Storage Artifact Format

### 4.1 Database Directory Structure

MVP database artifact is a portable directory (SQLite-like portability by copy):

```
strata.db/
├── MANIFEST                    # Physical metadata
├── WAL/
│   ├── wal-000001.seg         # WAL segment files
│   ├── wal-000002.seg
│   └── ...
├── SNAPSHOTS/
│   ├── snap-000010.chk        # Snapshot checkpoint files
│   └── ...
└── DATA/                       # Optional: materialized data segments
    └── ...
```

### 4.2 Portability Guarantees

**Copy Semantics**:
- If Strata is closed cleanly, copying `strata.db/` produces a valid clone
- `checkpoint()` creates a stable boundary for safe copying while database is open
- Copied database opens independently with identical observable state

**Path Configuration**:
- Database path is configurable: user-provided or system default
- Opening with path creates directory if it doesn't exist
- Opening existing path validates MANIFEST and recovers if needed

```rust
// User-provided path
let db = Database::open("./my-data/strata.db", config)?;

// Or with default path
let db = Database::open_default(config)?;  // Uses platform default location
```

### 4.3 MANIFEST Structure

MANIFEST is a small versioned metadata file containing physical storage state only.

**Design Rationale**:
> MANIFEST is intentionally minimal to avoid semantic coupling between storage format and data model.

By keeping MANIFEST to physical metadata only (format version, segment IDs, watermarks), we:
- Prevent configuration drift between MANIFEST and database state
- Keep all semantic data (including policies) in the versioned, transactional data layer
- Simplify backup/restore (MANIFEST is stateless relative to semantics)
- Avoid MANIFEST becoming a dumping ground for "just one more field"

```rust
/// MANIFEST file contents - physical metadata only
#[derive(Debug, Serialize, Deserialize)]
pub struct Manifest {
    /// Format version for forward compatibility
    pub format_version: u32,

    /// Unique database identifier
    pub database_uuid: Uuid,

    /// Codec identifier (for future encryption)
    pub codec_id: String,

    /// Active WAL segment number
    pub active_wal_segment: u64,

    /// Latest snapshot watermark (TxnId)
    pub snapshot_watermark: Option<u64>,

    /// Latest snapshot ID
    pub snapshot_id: Option<u64>,
}
```

**What MANIFEST does NOT contain**:
- Retention policies (stored as database entries)
- Durability mode defaults (passed at open time)
- User configuration (passed at open time)

**MANIFEST Update Protocol**:
1. Write new MANIFEST to `MANIFEST.new`
2. `fsync(MANIFEST.new)`
3. Atomic rename `MANIFEST.new` → `MANIFEST`
4. `fsync(directory)`

---

## 5. WAL Contract

### 5.1 WAL Semantics

**Core Properties**:
- WAL is append-only and segmented (`wal-N.seg`)
- A transaction is durable if its WAL record is persisted according to durability mode
- WAL replay is deterministic and idempotent
- Records are self-delimiting and checksummed

**Segment Immutability**:
> **WAL segments are immutable once closed.** The active segment is the only writable file.

This invariant:
- Prevents accidental rewrites
- Simplifies recovery (no need to verify segment integrity beyond checksum)
- Simplifies checksums (whole-segment checksums become viable)
- Enables future replication (segments can be shipped as-is)
- Enables safe memory-mapping

**Segment Size**:
- Configurable with default (64MB recommended)
- New segment when current exceeds size limit
- Segment boundary is not transaction boundary (records can span)

```rust
/// WAL configuration
pub struct WalConfig {
    /// Segment size in bytes (default: 64MB)
    pub segment_size: usize,

    /// Durability mode
    pub durability_mode: DurabilityMode,
}

impl Default for WalConfig {
    fn default() -> Self {
        WalConfig {
            segment_size: 64 * 1024 * 1024,  // 64MB
            durability_mode: DurabilityMode::Strict,
        }
    }
}
```

### 5.2 WAL Record Format

Each WAL record contains sufficient information for deterministic replay:

```rust
/// WAL record (logical structure)
#[derive(Debug, Serialize, Deserialize)]
pub struct WalRecord {
    /// Record format version
    pub format_version: u8,

    /// Transaction ID
    pub txn_id: u64,

    /// Run ID
    pub run_id: RunId,

    /// Commit timestamp (microseconds since epoch)
    pub commit_timestamp: u64,

    /// Canonical writeset
    pub writeset: Writeset,

    /// CRC32 checksum
    pub checksum: u32,
}
```

**Physical Layout**:
```
┌─────────────────────────────────────────────────────┐
│ Record Length (4 bytes, little-endian)              │
├─────────────────────────────────────────────────────┤
│ Format Version (1 byte)                             │
├─────────────────────────────────────────────────────┤
│ TxnId (8 bytes, little-endian)                      │
├─────────────────────────────────────────────────────┤
│ RunId (16 bytes, UUID)                              │
├─────────────────────────────────────────────────────┤
│ Commit Timestamp (8 bytes, little-endian)           │
├─────────────────────────────────────────────────────┤
│ Writeset (variable length, codec-encoded)           │
├─────────────────────────────────────────────────────┤
│ Checksum (4 bytes, CRC32)                           │
└─────────────────────────────────────────────────────┘
```

### 5.3 Writeset Representation

The WAL persists writesets in a primitive-agnostic representation:

```rust
/// Canonical writeset representation
#[derive(Debug, Serialize, Deserialize)]
pub struct Writeset {
    pub mutations: Vec<Mutation>,
}

/// Single mutation in a writeset
#[derive(Debug, Serialize, Deserialize)]
pub enum Mutation {
    /// Put a value (KV, State, JSON, Vector metadata)
    Put {
        entity_ref: EntityRef,
        value: Vec<u8>,  // Codec-encoded value
        version: Version,
    },

    /// Delete an entity
    Delete {
        entity_ref: EntityRef,
    },

    /// Append to append-only primitive (Event, Trace)
    Append {
        entity_ref: EntityRef,
        value: Vec<u8>,  // Codec-encoded value
        version: Version,
    },
}
```

**Key Properties**:
- Each mutation references an entity via `EntityRef` (from M9)
- Version assignment is done by the engine, not storage
- Storage persists enough metadata to reconstruct committed state exactly
- Storage must not invent versions

**Version Assignment Rule (Non-Negotiable)**:
> **Storage must never assign or modify versions.** Versions are assigned by the engine before persistence.

The WAL record contains versions that were already assigned by the transaction engine. Storage's job is to persist these versions faithfully and replay them exactly. Storage must not:
- Generate new version numbers
- Increment version counters during replay
- Modify version values during compaction
- Interpret version semantics

### 5.4 Durability Modes

M10 supports the durability modes defined in earlier milestones:

```rust
pub enum DurabilityMode {
    /// No WAL persistence - data lost on crash
    InMemory,

    /// WAL appended, fsync on coarse boundary (e.g., checkpoint)
    /// Data may be lost on crash, but faster writes
    Buffered,

    /// fsync before acknowledging commit
    /// No committed data lost on crash
    Strict,
}
```

**Durability Mode Enforcement**:

| Mode | WAL Append | fsync Timing | Data Loss on Crash |
|------|------------|--------------|-------------------|
| InMemory | No | Never | All uncommitted |
| Buffered | Yes | Checkpoint/periodic | Recent commits possible |
| Strict | Yes | Per commit | None (for committed) |

---

## 6. Snapshot and Checkpoint

### 6.1 Snapshot Definition

A snapshot materializes the database state at watermark W such that:
- All committed transactions with `txn_id <= W` are included
- Recovery can start from the snapshot and replay WAL entries with `txn_id > W`

**Snapshot Content Specification**:
> Snapshots persist the **fully materialized logical state** of all primitives at watermark W, independent of in-memory representation.

This means:
- Snapshots are logical, not physical (not a memory dump)
- Snapshot format may differ from in-memory data structures
- Recovery reconstructs equivalent logical state, not byte-identical memory
- Future optimizations can change snapshot format without breaking recovery

The snapshot contains the **result** of applying all transactions up to W, not the transactions themselves.

```rust
/// Snapshot metadata
#[derive(Debug)]
pub struct SnapshotInfo {
    /// Snapshot identifier
    pub snapshot_id: u64,

    /// Watermark TxnId
    pub watermark: u64,

    /// Creation timestamp
    pub created_at: u64,

    /// Size in bytes
    pub size_bytes: u64,
}
```

### 6.2 Checkpoint Operation

`checkpoint()` creates a new snapshot and updates the MANIFEST watermark:

```rust
/// Checkpoint result
pub struct CheckpointInfo {
    /// Watermark TxnId included in snapshot
    pub watermark_txn: u64,

    /// Snapshot identifier
    pub snapshot_id: u64,

    /// Creation timestamp
    pub timestamp: u64,
}

impl Database {
    /// Create a checkpoint (snapshot) for crash recovery and portability
    pub fn checkpoint(&self) -> Result<CheckpointInfo> {
        // 1. Determine current committed watermark
        let watermark = self.current_watermark();

        // 2. Serialize state to snapshot file
        let snapshot_id = self.next_snapshot_id();
        let temp_path = format!("SNAPSHOTS/snap-{:06}.chk.tmp", snapshot_id);
        let final_path = format!("SNAPSHOTS/snap-{:06}.chk", snapshot_id);

        self.serialize_snapshot(&temp_path, watermark)?;

        // 3. fsync the snapshot file
        fsync_file(&temp_path)?;

        // 4. Atomic rename to final name
        rename(&temp_path, &final_path)?;

        // 5. Update MANIFEST atomically
        self.update_manifest(|m| {
            m.snapshot_watermark = Some(watermark);
            m.snapshot_id = Some(snapshot_id);
        })?;

        Ok(CheckpointInfo {
            watermark_txn: watermark,
            snapshot_id,
            timestamp: Timestamp::now().0,
        })
    }
}
```

### 6.3 Crash-Safe Snapshot Creation

Snapshot creation protocol ensures crash safety:

1. Write snapshot to temporary name (`snap-N.chk.tmp`)
2. `fsync(snapshot file)`
3. Atomic rename to final name (`snap-N.chk`)
4. Update MANIFEST atomically
5. `fsync(directory)`

If crash occurs at any step, recovery finds consistent state:
- If temp file exists without final, ignore temp
- If final exists but MANIFEST not updated, MANIFEST points to previous snapshot
- If MANIFEST updated, new snapshot is active

### 6.4 Snapshot Frequency

M10 does not require automatic snapshot scheduling. The system supports:
- Manual checkpoint trigger via API
- Simple heuristics (optional): "checkpoint if WAL exceeds X MB"

Automatic checkpointing is a future optimization, not M10 scope.

---

## 7. Recovery Algorithm

### 7.1 Startup Recovery

On database open:

```rust
impl Database {
    pub fn open(path: &Path, config: DatabaseConfig) -> Result<Database> {
        // 1. Read MANIFEST
        let manifest = Manifest::read(path.join("MANIFEST"))?;

        // 2. Initialize state
        let mut db = Database::new_empty(manifest.database_uuid);

        // 3. Load latest snapshot (if any)
        let watermark = if let Some(snapshot_id) = manifest.snapshot_id {
            let snapshot_path = path.join(format!("SNAPSHOTS/snap-{:06}.chk", snapshot_id));
            db.load_snapshot(&snapshot_path)?;
            manifest.snapshot_watermark.unwrap_or(0)
        } else {
            0
        };

        // 4. Scan WAL segments and replay records > watermark
        for segment in db.list_wal_segments()? {
            for record in db.read_wal_segment(&segment)? {
                if record.txn_id > watermark {
                    db.replay_record(record)?;
                }
            }
        }

        // 5. Truncate any partial/corrupt tail record
        db.truncate_partial_wal_tail()?;

        Ok(db)
    }
}
```

### 7.2 WAL Replay

```rust
impl Database {
    fn replay_record(&mut self, record: WalRecord) -> Result<()> {
        // Verify checksum
        if !record.verify_checksum() {
            return Err(StorageError::CorruptWalRecord);
        }

        // Apply each mutation
        for mutation in record.writeset.mutations {
            self.apply_mutation(mutation)?;
        }

        // Update internal watermark
        self.update_watermark(record.txn_id);

        Ok(())
    }

    fn apply_mutation(&mut self, mutation: Mutation) -> Result<()> {
        match mutation {
            Mutation::Put { entity_ref, value, version } => {
                self.put_entity(&entity_ref, &value, version)?;
            }
            Mutation::Delete { entity_ref } => {
                self.delete_entity(&entity_ref)?;
            }
            Mutation::Append { entity_ref, value, version } => {
                self.append_entity(&entity_ref, &value, version)?;
            }
        }
        Ok(())
    }
}
```

### 7.3 Partial Record Handling

If a crash occurs mid-write, the WAL may have a partial record at the end:

```rust
impl Database {
    fn truncate_partial_wal_tail(&mut self) -> Result<()> {
        let last_segment = self.get_last_wal_segment()?;

        // Read records until we hit incomplete/corrupt data
        let mut valid_end = 0;
        for record_result in self.read_wal_segment_records(&last_segment) {
            match record_result {
                Ok(record) => {
                    valid_end = record.end_offset;
                }
                Err(StorageError::IncompleteRecord | StorageError::ChecksumMismatch) => {
                    // Truncate at last valid record
                    break;
                }
                Err(e) => return Err(e),
            }
        }

        // Truncate file to valid_end
        self.truncate_file(&last_segment, valid_end)?;

        Ok(())
    }
}
```

### 7.4 Recovery Invariants Verification

After recovery:
- No committed transaction (in Strict mode) is lost
- No transaction is applied out of order
- State is identical to pre-crash state for all committed transactions
- Reapplying any WAL record is idempotent

---

## 8. Retention Policy

### 8.1 Retention Model

Retention is configured hierarchically:
- Global defaults (bootstrap, in code)
- Per-run overrides (stored as database entries)
- Per-primitive overrides (stored as database entries)

Retention governs which historical versions are eligible for deletion during compaction.

### 8.2 Retention Policy Types

```rust
/// Retention policy configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RetentionPolicy {
    /// Keep all historical versions forever
    KeepAll,

    /// Keep only the last N versions (for mutable entities)
    KeepLast(usize),

    /// Keep versions for the specified duration
    KeepFor(Duration),

    /// Composite: different policies for different entity types
    Composite {
        default: Box<RetentionPolicy>,
        overrides: HashMap<PrimitiveType, Box<RetentionPolicy>>,
    },
}
```

### 8.3 Storage as Database Entries

Retention policies are stored as first-class database entries in a **system namespace**:

**Storage Location**:
> Retention policies are stored in the system namespace (`_strata/`) as versioned entities, not in user-visible KV space.

This design:
- Prevents users from accidentally deleting retention policies
- Keeps system metadata separate from user data
- Allows system namespace to have different access controls (future)
- Makes policies discoverable via standard introspection

```rust
// Reserved key prefix for system namespace
const SYSTEM_NAMESPACE: &str = "_strata/";

// Reserved key for retention policy per run
const RETENTION_POLICY_KEY: &str = "_strata/retention_policy";

impl Database {
    /// Set retention policy for a run
    pub fn set_retention_policy(
        &self,
        run_id: RunId,
        policy: RetentionPolicy
    ) -> Result<Version> {
        // Store as versioned KV entry
        let value = serde_json::to_value(&policy)?;
        self.kv().put(run_id, RETENTION_POLICY_KEY, value)
    }

    /// Get current retention policy for a run
    pub fn get_retention_policy(
        &self,
        run_id: RunId
    ) -> Result<Option<Versioned<RetentionPolicy>>> {
        match self.kv().get(run_id, RETENTION_POLICY_KEY)? {
            Some(versioned) => {
                let policy: RetentionPolicy = serde_json::from_value(versioned.value.clone())?;
                Ok(Some(Versioned {
                    value: policy,
                    version: versioned.version,
                    timestamp: versioned.timestamp,
                    ttl: versioned.ttl,
                }))
            }
            None => Ok(None),
        }
    }
}
```

### 8.4 Bootstrap Default

The bootstrap default (before any policy is set) is:

```rust
impl RetentionPolicy {
    /// Default policy for new databases
    pub fn bootstrap_default() -> Self {
        RetentionPolicy::KeepAll
    }
}
```

This ensures no data loss by default. Users explicitly opt into retention limits.

### 8.5 Safety Rules

Retention must not violate substrate semantics:

1. **Version ordering preserved**: Remaining versions maintain their relative order
2. **No silent fallback**: Reads don't return nearest available version when requested version is trimmed
3. **Explicit unavailability**: Trimmed versions return specific error

```rust
#[derive(Debug, thiserror::Error)]
pub enum StorageError {
    /// Requested version was trimmed by retention policy
    #[error("version {requested} was trimmed; earliest retained: {earliest_retained}")]
    HistoryTrimmed {
        requested: Version,
        earliest_retained: Version,
    },

    // ... other errors
}
```

---

## 9. Compaction

### 9.1 Compaction Definition

Compaction reclaims disk space by removing data made unreachable by:
- Snapshots (WAL segments older than watermark)
- Retention policy (old versions outside retention window)

### 9.2 Compaction Modes

```rust
/// Compaction mode
pub enum CompactMode {
    /// Remove only WAL segments covered by snapshot
    WALOnly,

    /// Full compaction: WAL + retention enforcement
    Full,
}

/// Compaction result
pub struct CompactInfo {
    /// Bytes reclaimed
    pub reclaimed_bytes: u64,

    /// WAL segments removed
    pub wal_segments_removed: usize,

    /// Historical versions removed (Full mode only)
    pub versions_removed: usize,
}
```

### 9.3 Compaction Operation

```rust
impl Database {
    /// Perform compaction
    pub fn compact(&self, mode: CompactMode) -> Result<CompactInfo> {
        let mut info = CompactInfo::default();

        // Phase 1: Remove WAL segments covered by snapshot
        let watermark = self.manifest().snapshot_watermark.unwrap_or(0);
        for segment in self.wal_segments_before(watermark)? {
            let size = segment.size();
            self.remove_wal_segment(&segment)?;
            info.reclaimed_bytes += size;
            info.wal_segments_removed += 1;
        }

        // Phase 2: Apply retention policy (Full mode only)
        if mode == CompactMode::Full {
            let removed = self.apply_retention_to_data()?;
            info.versions_removed = removed;
        }

        Ok(info)
    }
}
```

### 9.4 Compaction Correctness

**Before/after compaction invariant**: For any retained version V, reading V before compaction and after compaction returns identical results.

**Version Identity Invariant (Non-Negotiable)**:
> **Compaction must not rewrite, renumber, or reinterpret version identifiers.**

This is critical because:
- Version numbers are semantic identifiers referenced by users and external systems
- Changing versions would break CAS operations, external caches, and audit trails
- Version stability is part of the substrate contract (M9 Invariant 2)

Compaction may remove data, but it must never change the identity of retained data.

**No implicit compaction**: M10 does not perform background compaction. Users explicitly trigger compaction when ready.

### 9.5 Tombstones

For correctness during retention-based compaction, storage may write tombstones for deleted entries:

```rust
/// Tombstone marker for deleted entries
#[derive(Debug, Serialize, Deserialize)]
struct Tombstone {
    /// Entity that was deleted
    entity_ref: EntityRef,

    /// When deleted
    deleted_at: u64,

    /// Deletion version
    version: Version,
}
```

Tombstones are internal implementation details, not exposed to users.

---

## 10. Codec Seam

### 10.1 Codec Interface

All bytes persisted to disk pass through a codec boundary:

```rust
/// Codec trait for storage encoding/decoding
pub trait StorageCodec: Send + Sync {
    /// Encode data for persistence
    fn encode(&self, data: &[u8]) -> Result<Vec<u8>>;

    /// Decode data from persistence
    fn decode(&self, data: &[u8]) -> Result<Vec<u8>>;

    /// Codec identifier for MANIFEST
    fn codec_id(&self) -> &str;
}
```

### 10.2 Identity Codec (MVP)

M10 uses identity codec (no transformation):

```rust
/// Identity codec - no encryption, no compression
pub struct IdentityCodec;

impl StorageCodec for IdentityCodec {
    fn encode(&self, data: &[u8]) -> Result<Vec<u8>> {
        Ok(data.to_vec())
    }

    fn decode(&self, data: &[u8]) -> Result<Vec<u8>> {
        Ok(data.to_vec())
    }

    fn codec_id(&self) -> &str {
        "identity"
    }
}
```

### 10.3 Future Extension

The codec seam enables future encryption-at-rest:

```rust
// Future: encryption codec
pub struct AesGcmCodec {
    key: [u8; 32],
}

impl StorageCodec for AesGcmCodec {
    fn encode(&self, data: &[u8]) -> Result<Vec<u8>> {
        // AES-GCM encryption
    }

    fn decode(&self, data: &[u8]) -> Result<Vec<u8>> {
        // AES-GCM decryption
    }

    fn codec_id(&self) -> &str {
        "aes-gcm-256"
    }
}
```

The codec ID is stored in MANIFEST to ensure correct decoder is used on open.

---

## 11. Public API Surface

### 11.1 Database Open/Close

```rust
impl Database {
    /// Open database at specified path
    pub fn open(path: impl AsRef<Path>, config: DatabaseConfig) -> Result<Database>;

    /// Open database with platform default path
    pub fn open_default(config: DatabaseConfig) -> Result<Database>;

    /// Close database cleanly
    pub fn close(self) -> Result<()>;
}

/// Database configuration
pub struct DatabaseConfig {
    /// Durability mode
    pub durability_mode: DurabilityMode,

    /// WAL segment size (bytes)
    pub wal_segment_size: usize,

    /// Storage codec
    pub codec: Box<dyn StorageCodec>,
}

impl Default for DatabaseConfig {
    fn default() -> Self {
        DatabaseConfig {
            durability_mode: DurabilityMode::Strict,
            wal_segment_size: 64 * 1024 * 1024,  // 64MB
            codec: Box::new(IdentityCodec),
        }
    }
}
```

### 11.2 Checkpoint and Compaction

```rust
impl Database {
    /// Create checkpoint (snapshot) for crash recovery and portability
    pub fn checkpoint(&self) -> Result<CheckpointInfo>;

    /// Perform compaction to reclaim disk space
    pub fn compact(&self, mode: CompactMode) -> Result<CompactInfo>;
}
```

### 11.3 Export and Import (Convenience)

```rust
impl Database {
    /// Export database to specified path
    ///
    /// Internally performs checkpoint, then copies artifact
    pub fn export(&self, path: impl AsRef<Path>) -> Result<()> {
        // 1. Checkpoint to ensure consistent state
        self.checkpoint()?;

        // 2. Copy strata.db/ directory to destination
        copy_directory(&self.path, path.as_ref())?;

        Ok(())
    }
}

/// Import database from exported artifact
pub fn import(path: impl AsRef<Path>, config: DatabaseConfig) -> Result<Database> {
    // Simply open the exported directory
    Database::open(path, config)
}
```

**Note**: `export()` calls `checkpoint()` internally. The canonical portable mechanism is "checkpoint + copy directory".

### 11.4 Retention API

```rust
impl Database {
    /// Set retention policy for a run
    pub fn set_retention_policy(&self, run_id: RunId, policy: RetentionPolicy) -> Result<Version>;

    /// Get current retention policy for a run
    pub fn get_retention_policy(&self, run_id: RunId) -> Result<Option<Versioned<RetentionPolicy>>>;
}
```

---

## 12. Database Open/Close

### 12.1 Open Behavior

```rust
impl Database {
    pub fn open(path: impl AsRef<Path>, config: DatabaseConfig) -> Result<Database> {
        let path = path.as_ref();

        if path.exists() {
            // Existing database: validate and recover
            Self::open_existing(path, config)
        } else {
            // New database: create and initialize
            Self::create_new(path, config)
        }
    }

    fn create_new(path: &Path, config: DatabaseConfig) -> Result<Database> {
        // Create directory structure
        std::fs::create_dir_all(path)?;
        std::fs::create_dir(path.join("WAL"))?;
        std::fs::create_dir(path.join("SNAPSHOTS"))?;
        std::fs::create_dir(path.join("DATA"))?;

        // Create initial MANIFEST
        let manifest = Manifest {
            format_version: 1,
            database_uuid: Uuid::new_v4(),
            codec_id: config.codec.codec_id().to_string(),
            active_wal_segment: 1,
            snapshot_watermark: None,
            snapshot_id: None,
        };
        manifest.write(&path.join("MANIFEST"))?;

        // Initialize empty database
        let db = Database::new(path, manifest, config);

        Ok(db)
    }

    fn open_existing(path: &Path, config: DatabaseConfig) -> Result<Database> {
        // Read and validate MANIFEST
        let manifest = Manifest::read(&path.join("MANIFEST"))?;

        // Verify codec compatibility
        if manifest.codec_id != config.codec.codec_id() {
            return Err(StorageError::CodecMismatch {
                expected: manifest.codec_id.clone(),
                got: config.codec.codec_id().to_string(),
            });
        }

        // Initialize database and recover
        let mut db = Database::new(path, manifest, config);
        db.recover()?;

        Ok(db)
    }
}
```

### 12.2 Close Behavior

```rust
impl Database {
    /// Close database cleanly
    pub fn close(self) -> Result<()> {
        // Flush any buffered WAL data
        self.wal.flush()?;

        // Update MANIFEST with final state
        self.write_manifest()?;

        // fsync everything
        self.fsync_all()?;

        Ok(())
    }
}
```

---

## 13. Testing Strategy

### 13.1 Crash Recovery Tests

```rust
#[test]
fn test_committed_txn_survives_crash() {
    // 1. Open database in Strict mode
    // 2. Commit transaction
    // 3. Simulate crash (drop without close)
    // 4. Reopen database
    // 5. Verify committed data exists
}

#[test]
fn test_uncommitted_txn_lost_on_crash() {
    // 1. Open database
    // 2. Start transaction, write data, don't commit
    // 3. Simulate crash
    // 4. Reopen database
    // 5. Verify uncommitted data does not exist
}

#[test]
fn test_partial_wal_record_truncated() {
    // 1. Write partial WAL record (simulate mid-write crash)
    // 2. Open database
    // 3. Verify recovery succeeds
    // 4. Verify partial record was truncated
}
```

### 13.2 Checkpoint Correctness Tests

```rust
#[test]
fn test_checkpoint_produces_consistent_snapshot() {
    // 1. Commit multiple transactions
    // 2. Checkpoint
    // 3. Verify snapshot contains all committed data
}

#[test]
fn test_recovery_from_snapshot_plus_wal() {
    // 1. Commit transactions T1..T10
    // 2. Checkpoint at T5
    // 3. Commit transactions T6..T10
    // 4. Simulate crash
    // 5. Recover
    // 6. Verify all T1..T10 present
}

#[test]
fn test_checkpoint_is_crash_safe() {
    // 1. Start checkpoint
    // 2. Crash at various points (temp file, after fsync, before manifest update)
    // 3. Verify recovery is correct at each crash point
}
```

### 13.3 Retention Enforcement Tests

```rust
#[test]
fn test_retention_policy_stored_as_entry() {
    // 1. Set retention policy
    // 2. Verify can read it back as versioned entry
    // 3. Update policy
    // 4. Verify version increased
}

#[test]
fn test_trimmed_version_returns_error() {
    // 1. Set KeepLast(1) policy
    // 2. Write 5 versions
    // 3. Compact
    // 4. Try to read old versions
    // 5. Verify HistoryTrimmed error
}

#[test]
fn test_retention_does_not_affect_current_version() {
    // 1. Set KeepLast(1) policy
    // 2. Write multiple versions
    // 3. Compact
    // 4. Current version still readable
}
```

### 13.4 Compaction Invariance Tests

```rust
#[test]
fn test_compaction_read_equivalence() {
    // 1. Write data
    // 2. Checkpoint
    // 3. Read all retained data
    // 4. Compact
    // 5. Read all retained data again
    // 6. Verify identical results
}

#[test]
fn test_wal_compaction_reclaims_space() {
    // 1. Write data filling multiple WAL segments
    // 2. Checkpoint
    // 3. Note disk usage
    // 4. Compact(WALOnly)
    // 5. Verify disk usage decreased
}
```

### 13.5 Portability Tests

```rust
#[test]
fn test_copy_after_checkpoint_produces_clone() {
    // 1. Write data
    // 2. Checkpoint
    // 3. Copy directory
    // 4. Open copy as new database
    // 5. Verify identical observable state
}

#[test]
fn test_export_import_roundtrip() {
    // 1. Create database with data
    // 2. Export to path
    // 3. Import from path
    // 4. Verify identical observable state
}
```

---

## 14. Known Limitations

### 14.1 No Background Operations

- No automatic checkpointing
- No automatic compaction
- Users must explicitly trigger these operations

### 14.2 Single-Node Only

- No replication
- No distributed transactions
- No multi-process access to same database

### 14.3 No Incremental Snapshots

- Snapshots are full database state
- Large databases produce large snapshots
- Incremental snapshots are future optimization

### 14.4 Codec Cannot Change

- Database created with codec X must always open with codec X
- No codec migration path in M10
- Future work may add codec migration

### 14.5 WAL Cannot Shrink Mid-Segment

- WAL segments are append-only
- Compaction removes whole segments
- Partial segment reclamation not supported

---

## 15. Future Extension Points

### 15.1 For M11+ (Performance)

- Background checkpoint scheduling
- Parallel WAL replay
- Memory-mapped snapshots
- Compressed WAL segments

### 15.2 For Post-MVP (Features)

- Encryption-at-rest via codec
- Incremental snapshots
- WAL shipping for replication
- Tiered storage (hot/cold)
- Online defragmentation

### 15.3 For Wire Protocol

- Snapshot streaming for backup
- WAL streaming for replication
- Checkpoint coordination across nodes

---

## 16. Success Criteria Checklist

### Gate 1: WAL and Recovery

- [ ] WAL append works with all durability modes
- [ ] WAL records are self-delimiting and checksummed
- [ ] Recovery replays WAL correctly
- [ ] Partial records are safely truncated
- [ ] Committed transactions survive crash (Strict mode)

### Gate 2: Snapshots and Checkpoint

- [ ] Checkpoint creates consistent snapshot
- [ ] Snapshot creation is crash-safe
- [ ] Recovery from snapshot + WAL works
- [ ] Snapshot watermark is correctly maintained

### Gate 3: Retention

- [ ] Retention policies stored as database entries
- [ ] KeepAll, KeepLast(N), KeepFor(Duration) work
- [ ] Trimmed versions return HistoryTrimmed error
- [ ] Retention policy is versioned and transactional

### Gate 4: Compaction

- [ ] WALOnly compaction removes old segments
- [ ] Full compaction enforces retention
- [ ] Compaction is logically invisible
- [ ] Reclaimed space is measurable

### Gate 5: Portability

- [ ] Database directory is portable by copy
- [ ] Checkpoint + copy produces valid clone
- [ ] export() and import() work as convenience wrappers

### Gate 6: Codec Seam

- [ ] All persistence goes through codec
- [ ] Identity codec works
- [ ] Codec ID stored in MANIFEST
- [ ] Codec mismatch detected on open

### Gate 7: Testing

- [ ] Crash recovery tests pass
- [ ] Checkpoint correctness tests pass
- [ ] Retention enforcement tests pass
- [ ] Compaction invariance tests pass
- [ ] Portability tests pass

---

## Document History

| Version | Date | Changes |
|---------|------|---------|
| 1.0 | 2026-01-20 | Initial M10 architecture specification |
| 1.1 | 2026-01-20 | Added tightening clarifications: WAL segment immutability, version assignment rule, snapshot content spec, system namespace for retention, compaction version identity invariant, MANIFEST design rationale, disk-first architectural warning |
