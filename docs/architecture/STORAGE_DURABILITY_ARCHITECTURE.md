# Strata Storage & Durability Architecture

## 1. Context

Strata has four crates addressing persistence:

| Crate | Status | LOC | Tests |
|-------|--------|-----|-------|
| `storage` (active) | ShardedStore (in-memory) + 6 orphaned persistence modules | ~4,700 + ~2,500 orphaned | ~120 + 205 orphaned |
| `durability` (active) | Simple WAL + snapshots + recovery + RunBundle | ~8,400 | ~200 |
| `storage-unified-archive` | Complete store + persistence (UnifiedStore, ShardedStore, WAL, recovery, compaction, etc.) | ~19,500 | ~520 |
| `durability-archive` | Mature durability with transaction framing, recovery manager, RunBundle | ~15,000 | ~380 |

This happened because two parallel architectures were built (Architecture A:
self-contained storage+persistence, Architecture B: separate crates) and
neither was fully completed before the other was started. The result is ~47,600
lines of persistence code spread across four crates with duplicated concepts,
incompatible wiring, and orphaned modules.

### Goal

Consolidate into two clean crates with clear boundaries:

- **`storage`** — In-memory data structures. No disk I/O.
- **`durability`** — Everything disk. WAL, snapshots, recovery, compaction,
  retention, codec, RunBundle.

The engine coordinates between them via well-defined trait boundaries. The
archive crates become unnecessary.

---

## 2. Design Principles

1. **Separation of concerns.** Storage knows nothing about files. Durability
   knows nothing about in-memory data structures. The `Storage` trait
   (defined in `strata-core`) is the boundary.

2. **Segmented WAL.** Production databases use segmented WALs (Postgres,
   SQLite, RocksDB). Segments enable: bounded file sizes, per-segment
   compaction (no file rewriting), parallel recovery, and clean rotation
   points for fsync batching. The storage-unified-archive already implements
   this.

3. **Transaction framing.** Every WAL record is wrapped in an envelope with
   transaction ID, entry type tag, format version, and CRC32 checksum. This
   comes from the durability-archive's design and enables: atomic commit
   semantics during recovery, forward-compatible format evolution, and
   corruption detection per record.

4. **Crash-safe snapshots.** Snapshots use the write-to-temp, fsync, atomic
   rename, fsync-parent pattern from the storage-unified-archive. This is
   standard practice (Postgres, SQLite, LevelDB).

5. **Codec abstraction.** All bytes that touch disk pass through a
   `StorageCodec` trait. The default is `IdentityCodec` (passthrough). This
   is the extension point for encryption-at-rest or compression without
   touching any other module.

6. **Callback-based recovery.** The durability crate does not import or
   depend on any specific `Storage` implementation. Recovery uses callbacks:
   `on_snapshot(LoadedSnapshot)` and `on_record(WalRecord)`. The engine
   provides these callbacks and applies recovered data to ShardedStore.

7. **Version preservation.** Recovery replays use `put_with_version()` and
   `delete_with_version()` to restore exact version numbers from the WAL.
   The durability crate never assigns versions — that is the concurrency
   layer's job.

8. **No backward compatibility.** This is a clean break. Existing WAL files
   and snapshots from the old format are not supported. The database
   directory is re-initialized.

---

## 3. Target Architecture

```
┌──────────────────────────────────────────────────────────┐
│                        Engine                             │
│  Database, TransactionCoordinator                        │
│  Orchestrates: begin → validate → WAL write → apply      │
│  Orchestrates: recovery, checkpoints, compaction          │
└───────┬──────────────────┬──────────────────┬────────────┘
        │                  │                  │
        │ Storage trait    │ WAL API          │ Concurrency
        │ (in-memory)      │ (disk)           │ (transactions)
        ▼                  ▼                  ▼
┌───────────────┐  ┌───────────────┐  ┌──────────────────┐
│   Storage     │  │  Durability   │  │   Concurrency    │
│               │  │               │  │                  │
│ ShardedStore  │  │ WAL (segment) │  │ TransactionMgr   │
│ VersionChain  │  │ Snapshots     │  │ Validation       │
│ ShardedSnap   │  │ Manifest      │  │ WAL Writer       │
│ Indices       │  │ Recovery      │  │ Recovery Coord   │
│ TTL           │  │ Compaction    │  │                  │
│ Registry      │  │ Retention     │  │                  │
│               │  │ Codec         │  │                  │
│               │  │ RunBundle     │  │                  │
└───────────────┘  └───────────────┘  └──────────────────┘
        │                  │
        └───────┬──────────┘
                │
        ┌───────▼───────┐
        │     Core      │
        │ Storage trait │
        │ SnapshotView  │
        │ Key, Value    │
        └───────────────┘
```

### Dependency Direction

```
engine → storage, durability, concurrency, core
concurrency → durability, storage, core
durability → core (NOT storage)
storage → core
```

**Critical: `durability` does NOT depend on `storage`.** The durability crate
defines its own data types for what goes on disk (WalRecord, Writeset,
SnapshotSection). The engine translates between storage types and durability
types. This keeps the crates independently testable and avoids circular
dependencies.

---

## 4. Storage Crate (`strata-storage`)

### Scope

Pure in-memory data structures. No file I/O. No `std::fs`. No `std::path`.

### Contents (kept from current storage crate)

| Module | Source | Purpose |
|--------|--------|---------|
| `sharded.rs` | Current storage | ShardedStore, Shard, VersionChain, ShardedSnapshot |
| `stored_value.rs` | Current storage | StoredValue (VersionedValue + TTL wrapper) |
| `index.rs` | Current storage | RunIndex, TypeIndex (secondary indices) |
| `ttl.rs` | Current storage | TTLIndex (expiration tracking) |
| `primitive_ext.rs` | Current storage | PrimitiveStorageExt trait, WAL type ranges |
| `registry.rs` | Current storage | PrimitiveRegistry (dynamic primitive lookup) |

### Contents removed (moved to durability)

| Module | Destination |
|--------|-------------|
| `format/` | `durability/format/` |
| `disk_snapshot/` | `durability/snapshot/` |
| `compaction/` | `durability/compaction/` |
| `retention/` | `durability/retention/` |
| `codec/` | `durability/codec/` |
| `testing/` | `durability/testing/` |

### Public API (after cleanup)

```rust
// strata-storage/src/lib.rs

// In-memory storage
pub use sharded::{Shard, ShardedSnapshot, ShardedStore, VersionChain};
pub use stored_value::StoredValue;
pub use index::{RunIndex, TypeIndex};
pub use ttl::TTLIndex;

// Primitive extension system
pub use primitive_ext::{PrimitiveStorageExt, PrimitiveExtError};
pub use primitive_ext::{primitive_type_ids, wal_ranges, primitive_for_wal_type};
pub use registry::PrimitiveRegistry;
```

### What stays the same

ShardedStore's implementation, MVCC version chains, DashMap sharding, snapshot
creation — all unchanged. The Storage trait in strata-core is unchanged. No
engine or concurrency code needs to change for the storage crate.

---

## 5. Durability Crate (`strata-durability`)

### Scope

Everything that touches disk. WAL, snapshots, manifest, recovery,
compaction, retention, codec, RunBundle.

### Module Structure

```
strata-durability/src/
├── lib.rs                    # Public API and re-exports
│
├── wal/                      # Write-Ahead Log (segmented)
│   ├── mod.rs                # WalManager: segment lifecycle
│   ├── writer.rs             # WalWriter: append with CRC32
│   ├── reader.rs             # WalReader: scan segments
│   ├── record.rs             # WalRecord, SegmentHeader format
│   ├── entry_types.rs        # Entry type registry (0x00-0x7F)
│   └── config.rs             # WalConfig, DurabilityMode
│
├── snapshot/                 # Crash-safe snapshots
│   ├── mod.rs                # Public snapshot types
│   ├── writer.rs             # SnapshotWriter (write-fsync-rename)
│   ├── reader.rs             # SnapshotReader (validate + load)
│   ├── checkpoint.rs         # CheckpointCoordinator
│   └── format.rs             # SnapshotHeader, SectionHeader
│
├── manifest/                 # Database metadata
│   ├── mod.rs                # Manifest, ManifestManager
│   └── format.rs             # Binary format (magic, version, UUID)
│
├── recovery/                 # Crash recovery
│   ├── mod.rs                # RecoveryCoordinator
│   └── replay.rs             # WalReplayer (watermark-filtered)
│
├── compaction/               # WAL segment cleanup
│   ├── mod.rs                # CompactMode (WALOnly, Full)
│   ├── wal_only.rs           # WalOnlyCompactor
│   └── tombstone.rs          # TombstoneIndex
│
├── retention/                # Version retention policies
│   ├── mod.rs                # System namespace utilities
│   └── policy.rs             # RetentionPolicy (KeepAll, KeepLast, etc.)
│
├── codec/                    # Storage codec abstraction
│   ├── mod.rs                # get_codec() factory
│   ├── traits.rs             # StorageCodec trait
│   └── identity.rs           # IdentityCodec (passthrough)
│
├── format/                   # Shared binary format primitives
│   ├── mod.rs
│   ├── writeset.rs           # Mutation, EntityRef, Writeset
│   ├── primitives.rs         # Per-primitive snapshot entry types
│   └── watermark.rs          # SnapshotWatermark, CheckpointInfo
│
├── run_bundle/               # Portable execution artifacts
│   ├── mod.rs
│   ├── writer.rs             # RunBundleWriter (tar+zstd)
│   ├── reader.rs             # RunBundleReader
│   ├── wal_log.rs            # Run-filtered WAL operations
│   └── types.rs              # BundleManifest, ExportOptions
│
├── testing/                  # Test infrastructure
│   ├── mod.rs
│   ├── crash_harness.rs      # CrashConfig, CrashPoint, DataState
│   └── reference_model.rs    # ReferenceModel for verification
│
└── database/                 # Database lifecycle coordination
    ├── mod.rs                # DatabaseHandle
    ├── config.rs             # DatabaseConfig
    └── paths.rs              # DatabasePaths (directory structure)
```

### Source Material Map

Each module draws from specific sources in the existing crates:

| Module | Primary Source | Secondary Source | Notes |
|--------|---------------|-----------------|-------|
| `wal/record.rs` | storage-archive `format/wal_record.rs` | — | Segmented format with CRC32 |
| `wal/writer.rs` | storage-archive `wal/writer.rs` | durability-archive `wal_writer.rs` | Segment rotation + transaction framing |
| `wal/reader.rs` | storage-archive `wal/reader.rs` | — | Multi-segment scanning |
| `wal/entry_types.rs` | current durability `wal.rs` (WALEntry enum) | durability-archive `wal_entry_types.rs` | Keep current enum + add type registry |
| `wal/config.rs` | storage-archive `wal/config.rs` | current durability `wal.rs` (DurabilityMode) | Merge configs |
| `snapshot/writer.rs` | storage-archive `disk_snapshot/writer.rs` | — | Crash-safe write pattern |
| `snapshot/reader.rs` | storage-archive `disk_snapshot/reader.rs` | — | CRC validation |
| `snapshot/checkpoint.rs` | storage-archive `disk_snapshot/checkpoint.rs` | — | Watermark tracking |
| `snapshot/format.rs` | storage-archive `format/snapshot.rs` | — | Fixed 64-byte header |
| `manifest/` | storage-archive `format/manifest.rs` | — | ManifestManager |
| `recovery/` | storage-archive `recovery/` | durability-archive `recovery_manager.rs` | Coordinator + replay |
| `compaction/` | current storage `compaction/` | — | Already well-tested |
| `retention/` | current storage `retention/` | — | Already well-tested |
| `codec/` | current storage `codec/` | — | Already well-tested |
| `format/writeset.rs` | current storage `format/writeset.rs` | — | Already well-tested |
| `format/primitives.rs` | current storage `format/primitives.rs` | — | Already well-tested |
| `format/watermark.rs` | current storage `format/watermark.rs` | — | Already well-tested |
| `run_bundle/` | current durability `run_bundle/` | — | Already working |
| `testing/` | current storage `testing/` | — | Already well-tested |
| `database/` | storage-archive `database/` | — | DatabaseHandle, Config, Paths |

### Key Design Decisions

#### 5.1 Segmented WAL

```
wal/
├── wal_00000001.seg    # Segment 1 (closed, immutable)
├── wal_00000002.seg    # Segment 2 (closed, immutable)
└── wal_00000003.seg    # Segment 3 (active, appending)
```

Each segment file:
```
[SegmentHeader: 32 bytes]
  magic: "STRA" (4)
  format_version: u8 (1)
  segment_number: u32 (4)
  creation_timestamp: u64 (8)
  database_uuid: [u8; 16] (16)

[WalRecord 1]
  length: u32 (4)
  txn_id: u64 (8)
  run_id: RunId (16)
  timestamp: u64 (8)
  writeset_bytes: [u8] (variable)
  crc32: u32 (4)

[WalRecord 2]
...
```

From: `storage-unified-archive/src/format/wal_record.rs`

Segment rotation occurs when the active segment exceeds a configured size
(default: 64MB). Closed segments are immutable and can be removed by
compaction.

#### 5.2 Transaction Framing

The current durability crate uses explicit `BeginTxn`/`CommitTxn`/`AbortTxn`
WAL entries for transaction boundaries. The segmented WAL uses `WalRecord`
which contains a `Writeset` (batch of mutations).

For the consolidated design, we use **both**:

- **`BeginTxn` entry**: Written at transaction start (WAL append). Contains
  txn_id, run_id, timestamp.
- **Writeset entries**: Written during commit. Each WalRecord contains the
  full writeset for the transaction (puts, deletes, appends).
- **`CommitTxn` entry**: Written after writeset. This is the durability
  point. Only transactions with a CommitTxn marker are recovered.

Recovery groups entries by txn_id and discards any transaction without a
CommitTxn marker (crashed mid-commit).

The WALEntry enum from the current durability crate is the **logical**
format (used by the concurrency layer's TransactionWALWriter). The
WalRecord from the storage-archive is the **physical** format (used by
the WAL writer for disk I/O). The durability crate translates between them.

#### 5.3 Crash-Safe Snapshots

```
snapshots/
├── snapshot_00000001.chk   # Older snapshot
└── snapshot_00000002.chk   # Latest snapshot
```

Each snapshot file:
```
[SnapshotHeader: 64 bytes]
  magic: "STRS" (4)
  format_version: u8 (1)
  snapshot_id: u64 (8)
  watermark_txn: u64 (8)
  creation_timestamp: u64 (8)
  database_uuid: [u8; 16] (16)
  codec_id: [u8; 8] (8)
  reserved: [u8; 11] (11)

[SectionHeader + Data] per primitive
  primitive_type: u8 (1)
  data_length: u64 (8)
  data: [u8] (variable)
  section_crc32: u32 (4)

...more sections...
```

Write pattern (from `storage-unified-archive/src/disk_snapshot/writer.rs`):
1. Write to `snapshot_NNNN.chk.tmp`
2. `fsync()` the temp file
3. `rename()` temp to final path (atomic on POSIX)
4. `fsync()` the parent directory

From: `storage-unified-archive/src/format/snapshot.rs`

#### 5.4 Manifest

Single `MANIFEST` file in the database directory:

```
[ManifestHeader]
  magic: "STRM" (4)
  format_version: u8 (1)
  database_uuid: [u8; 16] (16)
  codec_id: [u8; 8] (8)
  active_wal_segment: u32 (4)
  snapshot_watermark: u64 (8)
  latest_snapshot_id: u64 (8)
  crc32: u32 (4)
```

Updated atomically using the same write-fsync-rename pattern. The manifest
is the root of truth for recovery — it tells the recovery coordinator where
to find the latest snapshot and which WAL segments to replay.

From: `storage-unified-archive/src/format/manifest.rs`

#### 5.5 Recovery Flow

```
RecoveryCoordinator::recover(database_path)
  │
  ├─ 1. Load MANIFEST
  │     └─ Validates magic, format version, codec ID, CRC32
  │     └─ Extracts: database_uuid, snapshot_watermark, active_wal_segment
  │
  ├─ 2. Load latest snapshot (if exists)
  │     └─ SnapshotReader validates header, CRC32, codec
  │     └─ Returns LoadedSnapshot with per-primitive sections
  │     └─ Calls on_snapshot(loaded_snapshot) callback
  │
  ├─ 3. Replay WAL segments after watermark
  │     └─ WalReplayer scans segments > watermark
  │     └─ For each WalRecord with txn_id > watermark:
  │         └─ Groups by txn_id
  │         └─ Only applies committed transactions
  │         └─ Calls on_record(wal_record) callback
  │
  ├─ 4. Truncate partial records at WAL tail
  │     └─ Removes incomplete entries from active segment
  │
  └─ Returns RecoveryResult
        ├─ final_version: u64 (highest version recovered)
        ├─ max_txn_id: u64 (highest txn_id recovered)
        └─ stats: ReplayStats (counters for diagnostics)
```

The engine provides the callbacks:
- `on_snapshot`: Deserializes each primitive section into ShardedStore
- `on_record`: Applies each committed write via `put_with_version()`

From: `storage-unified-archive/src/recovery/`

#### 5.6 Compaction

```
CompactMode::WALOnly
  │
  ├─ Reads WAL segments
  ├─ Finds max txn_id per segment
  ├─ Removes segments where max_txn_id <= snapshot_watermark
  ├─ Never removes the active segment
  └─ Reports: bytes reclaimed, segments removed

CompactMode::Full
  │
  ├─ WALOnly compaction (above)
  └─ Apply retention policies to prune old versions
```

From: current storage `compaction/`

#### 5.7 Retention Policies

```rust
pub enum RetentionPolicy {
    KeepAll,                    // Never discard (default)
    KeepLast(usize),           // Keep N most recent versions
    KeepFor(Duration),         // Keep versions newer than threshold
    Composite {                 // Per-primitive-type overrides
        default: Box<RetentionPolicy>,
        overrides: HashMap<PrimitiveType, RetentionPolicy>,
    },
}
```

From: current storage `retention/`

#### 5.8 DatabaseHandle

The top-level coordinator that owns all disk resources:

```rust
pub struct DatabaseHandle {
    paths: DatabasePaths,
    manifest: ManifestManager,
    wal: WalManager,
    checkpoint: CheckpointCoordinator,
    codec: Box<dyn StorageCodec>,
    config: DatabaseConfig,
    database_uuid: [u8; 16],
}

impl DatabaseHandle {
    pub fn create(path: &Path, config: DatabaseConfig) -> Result<Self>;
    pub fn open(path: &Path, config: DatabaseConfig) -> Result<Self>;
    pub fn open_or_create(path: &Path, config: DatabaseConfig) -> Result<Self>;

    pub fn recover(&self, on_snapshot: F1, on_record: F2) -> Result<RecoveryResult>;
    pub fn append_wal(&self, record: &WalRecord) -> Result<()>;
    pub fn checkpoint(&self, data: CheckpointData) -> Result<()>;
    pub fn compact(&self, mode: CompactMode) -> Result<CompactInfo>;
    pub fn close(&self) -> Result<()>;
}
```

From: `storage-unified-archive/src/database/handle.rs`

### Public API (after consolidation)

```rust
// strata-durability/src/lib.rs

// WAL
pub use wal::{WalManager, WalWriter, WalReader, WalRecord};
pub use wal::{DurabilityMode, WalConfig, SegmentHeader};
pub use wal::entry_types::WALEntry;  // Logical entry types

// Snapshots
pub use snapshot::{SnapshotWriter, SnapshotReader, CheckpointCoordinator};
pub use snapshot::{CheckpointData, LoadedSnapshot, LoadedSection};

// Manifest
pub use manifest::{Manifest, ManifestManager};

// Recovery
pub use recovery::{RecoveryCoordinator, RecoveryResult, ReplayStats};

// Compaction
pub use compaction::{CompactMode, CompactInfo, WalOnlyCompactor};
pub use compaction::{Tombstone, TombstoneIndex};

// Retention
pub use retention::{RetentionPolicy, CompositeBuilder};

// Codec
pub use codec::{StorageCodec, IdentityCodec, get_codec};

// Format types
pub use format::{Writeset, Mutation, EntityRef};
pub use format::{SnapshotWatermark, CheckpointInfo};
pub use format::primitives::*;

// Database lifecycle
pub use database::{DatabaseHandle, DatabaseConfig, DatabasePaths};

// RunBundle
pub use run_bundle::{RunBundleWriter, RunBundleReader};
pub use run_bundle::{BundleManifest, ExportOptions};

// Testing utilities
pub use testing::{CrashConfig, CrashPoint, ReferenceModel};
```

---

## 6. Integration Contracts

### 6.1 Transaction Commit Flow

```
Engine::commit_internal(txn, durability_mode)
  │
  ├─ 1. Lock WAL (if durability.requires_wal())
  │     └─ db_handle.wal() → WalManager (locked)
  │
  ├─ 2. Validate transaction
  │     └─ TransactionManager::commit(txn, store, wal)
  │         ├─ Acquire per-run commit lock
  │         ├─ Validate read-set against storage
  │         └─ Allocate commit version
  │
  ├─ 3. Write to WAL
  │     └─ TransactionWALWriter::write_begin()   → WALEntry::BeginTxn
  │     └─ TransactionWALWriter::write_put()     → WALEntry::Write
  │     └─ TransactionWALWriter::write_delete()  → WALEntry::Delete
  │     └─ TransactionWALWriter::write_commit()  → WALEntry::CommitTxn
  │     └─ WalManager handles encoding + CRC32 + segment append + fsync
  │
  ├─ 4. Apply to storage
  │     └─ store.put_with_version(key, value, commit_version)
  │     └─ store.delete_with_version(key, commit_version)
  │
  └─ 5. Return commit_version
```

### 6.2 Recovery Flow

```
Engine::open(path, config)
  │
  ├─ 1. Open DatabaseHandle
  │     └─ DatabaseHandle::open(path, config)
  │
  ├─ 2. Create empty ShardedStore
  │
  ├─ 3. Recover
  │     └─ db_handle.recover(
  │         on_snapshot: |loaded| {
  │             for section in loaded.sections {
  │                 primitive_registry.get(section.type_id)
  │                     .snapshot_deserialize(section.data);
  │             }
  │         },
  │         on_record: |record| {
  │             for (entity, mutation) in record.writeset {
  │                 match mutation {
  │                     Put(key, value) => store.put_with_version(key, value, record.version),
  │                     Delete(key) => store.delete_with_version(key, record.version),
  │                 }
  │             }
  │         },
  │     )
  │
  ├─ 4. Initialize TransactionManager with recovered state
  │     └─ TransactionManager::with_txn_id(result.final_version, result.max_txn_id)
  │
  └─ 5. Open WAL for appending
```

### 6.3 Checkpoint Flow

```
Engine::checkpoint()
  │
  ├─ 1. Serialize each primitive's state
  │     └─ for primitive in registry.list():
  │         data = primitive.snapshot_serialize()
  │         checkpoint_data.add_section(primitive.type_id(), data)
  │
  ├─ 2. Write snapshot
  │     └─ db_handle.checkpoint(checkpoint_data)
  │         ├─ SnapshotWriter writes crash-safe snapshot file
  │         ├─ ManifestManager updates watermark + snapshot ID
  │         └─ CheckpointCoordinator tracks watermark state
  │
  └─ 3. (Optional) Compact WAL
        └─ db_handle.compact(CompactMode::WALOnly)
```

### 6.4 Trait Boundaries

**`Storage` trait** (in `strata-core`, unchanged):
```rust
pub trait Storage: Send + Sync {
    fn get(&self, key: &Key) -> StrataResult<Option<VersionedValue>>;
    fn get_versioned(&self, key: &Key, max_version: u64) -> ...;
    fn get_history(&self, key: &Key, limit, before_version) -> ...;
    fn put(&self, key: Key, value: Value, ttl: Option<Duration>) -> ...;
    fn put_with_version(&self, key: Key, value: Value, version: u64, ttl) -> ...;
    fn delete(&self, key: &Key) -> ...;
    fn delete_with_version(&self, key: &Key, version: u64) -> ...;
    fn scan_prefix(&self, prefix: &Key, max_version: u64) -> ...;
    fn scan_by_run(&self, run_id: RunId, max_version: u64) -> ...;
    fn current_version(&self) -> u64;
}
```

**`SnapshotView` trait** (in `strata-core`, unchanged):
```rust
pub trait SnapshotView: Send + Sync {
    fn get(&self, key: &Key) -> StrataResult<Option<VersionedValue>>;
    fn scan_prefix(&self, prefix: &Key) -> StrataResult<Vec<(Key, VersionedValue)>>;
    fn version(&self) -> u64;
}
```

**`StorageCodec` trait** (moves to durability):
```rust
pub trait StorageCodec: Send + Sync {
    fn encode(&self, data: &[u8]) -> Result<Vec<u8>>;
    fn decode(&self, data: &[u8]) -> Result<Vec<u8>>;
    fn codec_id(&self) -> &str;
}
```

**`PrimitiveStorageExt` trait** (stays in storage):
```rust
pub trait PrimitiveStorageExt: Send + Sync {
    fn primitive_type_id(&self) -> u8;
    fn wal_entry_types(&self) -> &[u8];
    fn snapshot_serialize(&self) -> Result<Vec<u8>>;
    fn snapshot_deserialize(&mut self, data: &[u8]) -> Result<()>;
    fn apply_wal_entry(&mut self, entry_type: u8, data: &[u8]) -> Result<()>;
    fn primitive_name(&self) -> &str;
}
```

---

## 7. Directory Layout

### Database directory on disk

```
my-database/
├── MANIFEST                    # Database metadata (atomic write)
├── wal/
│   ├── wal_00000001.seg       # WAL segment 1 (closed)
│   ├── wal_00000002.seg       # WAL segment 2 (closed)
│   └── wal_00000003.seg       # WAL segment 3 (active)
└── snapshots/
    ├── snapshot_00000001.chk  # Older snapshot
    └── snapshot_00000002.chk  # Latest snapshot
```

From: `storage-unified-archive/src/database/paths.rs`

---

## 8. Implementation Phases

### Phase 1: Move persistence modules from storage to durability

**What:** Move the 6 orphaned modules from `crates/storage/` to
`crates/durability/`. This is a mechanical move — rename imports, update
Cargo.toml, update lib.rs re-exports.

**Modules moved:**
- `format/` → `durability/format/`
- `disk_snapshot/` → `durability/snapshot/`
- `compaction/` → `durability/compaction/`
- `retention/` → `durability/retention/`
- `codec/` → `durability/codec/`
- `testing/` → `durability/testing/`

**Storage crate changes:**
- Remove the 6 modules from `src/` and `lib.rs`
- Remove disk-related dependencies from `Cargo.toml` (crc32fast, byteorder
  if only used by moved modules)
- Update module doc comment

**Durability crate changes:**
- Add the 6 modules to `src/` and `lib.rs`
- Add their dependencies to `Cargo.toml`
- Update re-exports

**Verification:**
```bash
cargo check --workspace
cargo test -p strata-storage --lib
cargo test -p strata-durability --lib
```

### Phase 2: Add DatabaseHandle, recovery, and WAL management

**What:** Bring in the database lifecycle coordinator, recovery system, and
segmented WAL management from the storage-unified-archive. Adapt to work
with the existing WALEntry types and DurabilityMode.

**New modules:**
- `durability/database/` (from storage-archive `database/`)
  - `handle.rs` — DatabaseHandle
  - `config.rs` — DatabaseConfig
  - `paths.rs` — DatabasePaths
- `durability/recovery/` (from storage-archive `recovery/`)
  - `mod.rs` — RecoveryCoordinator
  - `replay.rs` — WalReplayer
- `durability/wal/` restructured (from storage-archive `wal/`)
  - `mod.rs` — WalManager
  - `writer.rs` — WalWriter (segment-based)
  - `reader.rs` — WalReader (multi-segment)
  - `record.rs` — WalRecord, SegmentHeader
  - `config.rs` — WalConfig, DurabilityMode

**Key integration:** The existing `WALEntry` enum (BeginTxn, Write, Delete,
CommitTxn, etc.) becomes the logical entry type. The `WalRecord` from the
format module becomes the physical disk format. The WAL writer translates
from logical to physical on append, and the WAL reader translates from
physical to logical on read.

**Backward compatibility for the WALEntry enum:** The concurrency crate's
`TransactionWALWriter` currently uses `WAL::append(WALEntry)`. This
interface changes to use the new `WalManager` API, but the `WALEntry` enum
itself and the `TransactionWALWriter` logic remain the same. The change is
in how entries get encoded to disk, not in what entries represent.

**Verification:**
```bash
cargo check --workspace
cargo test -p strata-durability --lib
cargo test -p strata-durability --tests  # integration tests
```

### Phase 3: Wire into engine

**What:** Update the engine to use `DatabaseHandle` for all disk operations.
Update recovery to use the new callback-based flow. Add checkpoint and
compaction support.

**Engine changes:**
- `database/mod.rs`: Replace `WAL` field with `DatabaseHandle`
- `database/mod.rs`: Update `open_with_mode()` to use
  `DatabaseHandle::open_or_create()`
- `database/mod.rs`: Update recovery to use callback-based
  `DatabaseHandle::recover()`
- `database/mod.rs`: Update `commit_internal()` to use
  `DatabaseHandle::append_wal()`
- `database/mod.rs`: Add `checkpoint()` method using
  `DatabaseHandle::checkpoint()`
- `database/mod.rs`: Add `compact()` method using
  `DatabaseHandle::compact()`
- `database/builder.rs`: Update builder to accept `DatabaseConfig`

**Concurrency changes:**
- `wal_writer.rs`: Update to write through `WalManager` instead of `WAL`
- `recovery.rs`: Update `RecoveryCoordinator` to delegate to durability's
  `RecoveryCoordinator`

**Verification:**
```bash
cargo check --workspace
cargo test --workspace --lib
cargo test --workspace --tests
```

### Phase 4: Cleanup

**What:** Remove archive crates, remove old durability code that's been
replaced, update documentation.

- Delete `crates/storage-unified-archive/`
- Delete `crates/durability-archive/`
- Remove old WAL/snapshot code from durability that's been replaced by the
  new modules
- Remove `docs/architecture/STORAGE_PERSISTENCE_MODULES.md` (superseded by
  this document)
- Update workspace `Cargo.toml` to remove archive crates

**Verification:**
```bash
cargo check --workspace
cargo test --workspace --lib
cargo test --workspace --tests
```

---

## 9. Alignment with `strata-core`

The core crate defines the foundational types and traits that all other
crates depend on. The consolidation plan is designed to align with core's
existing contracts.

### Types used across the boundary

| Core Type | Used By Storage | Used By Durability | Notes |
|-----------|----------------|-------------------|-------|
| `Key` | ShardedStore keys | — | Not used in disk format (EntityRef used instead) |
| `Value` | ShardedStore values | Serialized as bytes in Writeset | Durability stores codec-encoded bytes, not raw Value |
| `EntityRef` | — | Writeset serialization (`format/writeset.rs` already imports from core) | Canonical addressing for WAL mutations |
| `RunId` | Shard keys | WAL records, segment headers | 16-byte UUID used throughout |
| `TypeTag` | Key component | Entry type ranges align | Frozen `#[repr(u8)]` enum |
| `Version` | VersionChain stores Version | WAL stores version as `u64` | Variant (Txn/Seq/Counter) determined by TypeTag at recovery |
| `VersionedValue` | Storage trait returns | — | Engine converts between VersionedValue and disk bytes |
| `PrimitiveType` | EntityRef mapping | Snapshot section tags | 6 variants match EntityRef 1:1 |
| `StrataResult` | All Storage methods | All durability operations | Unified error handling |
| `Storage` trait | ShardedStore implements | Recovery uses `put_with_version()` via callbacks | Unchanged |
| `SnapshotView` trait | ShardedSnapshot implements | — | Unchanged |

### Dependency direction verified

```
durability → core (only)
storage → core (only)
engine → storage + durability + concurrency + core
concurrency → durability + core (uses Storage trait generically)
```

The current durability crate has a thin dependency on storage
(`strata_storage::PrimitiveStorageExt` used in a deprecated blanket impl,
plus test code). This dependency is removed during consolidation:

- The deprecated `SnapshotSerializable` trait and its blanket impl are
  deleted
- Recovery and snapshot operations use callbacks instead of
  `PrimitiveStorageExt` directly
- Test code uses mock implementations instead of `ShardedStore`

### `PrimitiveStorageExt` trait location

This trait lives in storage but defines durability-facing methods:
`snapshot_serialize()`, `snapshot_deserialize()`, `apply_wal_entry()`,
`wal_entry_types()`. It's the bridge between primitives (storage-side) and
persistence (durability-side).

For this consolidation, the trait stays in storage. The engine provides
the bridge: it calls `PrimitiveStorageExt` methods on primitives and
passes the resulting bytes to durability's snapshot/recovery APIs. This
keeps the dependency direction clean.

A future refactoring could move this trait to core (since it's
foundational), but that's out of scope for this effort.

---

## 10. What Does NOT Change

- **`strata-core`**: No changes. `Storage` trait, `SnapshotView` trait, all
  types unchanged.
- **`strata-concurrency`**: Minimal changes. `TransactionWALWriter` adapts
  to new WAL API. `TransactionManager`, validation, snapshot isolation all
  unchanged.
- **`strata-engine`**: Database lifecycle changes (DatabaseHandle replaces
  raw WAL). Transaction commit flow is structurally the same. Primitive
  system unchanged.
- **`strata-executor`**: No changes.
- **ShardedStore**: No changes. Same MVCC, same version chains, same
  DashMap sharding.
- **`Storage` trait**: No changes.

---

## 11. Test Strategy

### Unit tests (per module)

Each module carries its own tests. The 205 tests from the orphaned modules
move with them to durability. The existing durability tests are updated to
use the new APIs.

### Integration tests

- **Recovery round-trip**: Write transactions → crash → recover → verify
  state matches
- **Checkpoint round-trip**: Write → checkpoint → write more → recover →
  verify both pre- and post-checkpoint data
- **Compaction correctness**: Checkpoint → compact → recover → verify no
  data loss
- **Corruption detection**: Inject CRC errors → verify recovery detects and
  reports them
- **Crash safety**: Simulate crashes at each point in the checkpoint flow →
  verify recovery succeeds

### Property-based tests (from archives)

The durability-archive has ~5,700 lines of tests covering recovery
invariants, corruption simulation, cross-primitive atomicity, and
adversarial scenarios. These patterns should be adapted for the new
architecture.

### Workspace tests

```bash
cargo test --workspace --lib      # All unit tests
cargo test --workspace --tests    # All integration tests
```
