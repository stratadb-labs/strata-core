# Epic 72: Recovery - Implementation Prompts

**Epic Goal**: Implement recovery from snapshot + WAL replay

**GitHub Issue**: [#513](https://github.com/anibjoshi/in-mem/issues/513)
**Status**: Ready to begin
**Dependencies**: Epic 70 (WAL Infrastructure), Epic 71 (Snapshot System)
**Phase**: 3 (Recovery)

---

## NAMING CONVENTION - CRITICAL

> **NEVER use "M10" or "Strata" in the actual codebase or comments.**
>
> - "M10" is an internal milestone tracker only - do not use it in code, comments, or user-facing text
> - All existing crates refer to the database as "in-mem" - use this name consistently
> - Do not use "Strata" anywhere in the codebase
> - This applies to: code, comments, docstrings, error messages, log messages, test names
>
> **CORRECT**: `//! Database recovery from snapshot and WAL`
> **WRONG**: `//! M10 Recovery for Strata database`

---

## AUTHORITATIVE SPECIFICATIONS - READ THESE FIRST

**`docs/architecture/M10_ARCHITECTURE.md` is THE AUTHORITATIVE SPEC.**

Before starting ANY story in this epic, read:
1. **Architecture Spec (AUTHORITATIVE)**: `docs/architecture/M10_ARCHITECTURE.md`
2. **Implementation Plan**: `docs/milestones/M10/M10_IMPLEMENTATION_PLAN.md`
3. **Epic Spec**: `docs/milestones/M10/EPIC_72_RECOVERY.md`
4. **Prompt Header**: `docs/prompts/M10/M10_PROMPT_HEADER.md` for the 8 architectural rules

**The architecture spec is LAW.** Epic docs provide implementation details but MUST NOT contradict the architecture spec.

---

## Epic 72 Overview

### Scope
- MANIFEST structure and atomic persistence
- WAL replay implementation
- Snapshot + WAL recovery algorithm
- Partial record truncation
- Recovery verification tests

### Key Rules for Epic 72

1. **Recovery is idempotent** - Multiple recoveries produce same state
2. **Recovery is deterministic** - Same WAL → same state
3. **MANIFEST is atomic** - Write-fsync-rename pattern
4. **Partial records are truncated** - Not an error condition

### Success Criteria
- [ ] MANIFEST with magic `STRM` (0x5354524D)
- [ ] MANIFEST atomic persistence (write-fsync-rename)
- [ ] WAL replay with watermark filtering
- [ ] Snapshot + WAL recovery algorithm
- [ ] Partial record truncation at WAL tail
- [ ] Recovery verification tests
- [ ] All tests passing

### Component Breakdown
- **Story #513**: MANIFEST Structure and Persistence - FOUNDATION
- **Story #514**: WAL Replay Implementation - CRITICAL
- **Story #515**: Snapshot + WAL Recovery Algorithm - CRITICAL
- **Story #516**: Partial Record Truncation - CRITICAL
- **Story #517**: Recovery Verification Tests - HIGH

---

## File Organization

### Directory Structure

Extend the structure from Epic 71:

```bash
mkdir -p crates/storage/src/recovery
```

**Target structure**:
```
crates/storage/src/
├── lib.rs
├── format/
│   ├── mod.rs
│   ├── wal_record.rs
│   ├── writeset.rs
│   ├── snapshot.rs
│   ├── primitives.rs
│   └── manifest.rs          # NEW - MANIFEST format
├── wal/
│   └── ...
├── snapshot/
│   └── ...
├── recovery/                 # NEW
│   ├── mod.rs
│   ├── manifest.rs           # MANIFEST manager
│   └── replay.rs             # WAL replay
└── codec/
    └── ...
```

---

## Dependency Graph

```
Story #513 (MANIFEST) ──────────> Story #515 (Recovery Algorithm)
                                        │
Story #514 (WAL Replay) ────────────────┘
                                        │
Story #516 (Partial Truncation) ────────┘
                                        │
                              └──> Story #517 (Verification Tests)
```

**Recommended Order**: #513 (MANIFEST) → #514 (WAL Replay) → #516 (Truncation) → #515 (Recovery) → #517 (Tests)

---

## Story #513: MANIFEST Structure and Persistence

**GitHub Issue**: [#513](https://github.com/anibjoshi/in-mem/issues/513)
**Estimated Time**: 3 hours
**Dependencies**: None
**Blocks**: Story #515

### Start Story

```bash
gh issue view 513
./scripts/start-story.sh 72 513 manifest-structure
```

### Implementation

Create `crates/storage/src/format/manifest.rs`:

```rust
//! MANIFEST file format
//!
//! MANIFEST contains physical storage metadata only.
//! It is intentionally minimal to avoid semantic coupling.

use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

pub const MANIFEST_MAGIC: [u8; 4] = *b"STRM";
pub const MANIFEST_FORMAT_VERSION: u32 = 1;

/// MANIFEST file structure
#[derive(Debug, Clone)]
pub struct Manifest {
    /// Format version for forward compatibility
    pub format_version: u32,
    /// Unique database identifier (generated on creation)
    pub database_uuid: [u8; 16],
    /// Codec identifier (e.g., "identity")
    pub codec_id: String,
    /// Current active WAL segment number
    pub active_wal_segment: u64,
    /// Latest snapshot watermark (if any)
    pub snapshot_watermark: Option<u64>,
    /// Latest snapshot identifier (if any)
    pub snapshot_id: Option<u64>,
}

impl Manifest {
    /// Create a new MANIFEST for a fresh database
    pub fn new(database_uuid: [u8; 16], codec_id: String) -> Self {
        Manifest {
            format_version: MANIFEST_FORMAT_VERSION,
            database_uuid,
            codec_id,
            active_wal_segment: 1,
            snapshot_watermark: None,
            snapshot_id: None,
        }
    }

    /// Serialize MANIFEST to bytes
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::new();

        bytes.extend_from_slice(&MANIFEST_MAGIC);
        bytes.extend_from_slice(&self.format_version.to_le_bytes());
        bytes.extend_from_slice(&self.database_uuid);

        // Codec ID (length-prefixed)
        bytes.extend_from_slice(&(self.codec_id.len() as u32).to_le_bytes());
        bytes.extend_from_slice(self.codec_id.as_bytes());

        bytes.extend_from_slice(&self.active_wal_segment.to_le_bytes());

        // Snapshot watermark (0 = none)
        let watermark = self.snapshot_watermark.unwrap_or(0);
        bytes.extend_from_slice(&watermark.to_le_bytes());

        // Snapshot ID (0 = none)
        let snapshot_id = self.snapshot_id.unwrap_or(0);
        bytes.extend_from_slice(&snapshot_id.to_le_bytes());

        // CRC32 of all preceding bytes
        let crc = crc32fast::hash(&bytes);
        bytes.extend_from_slice(&crc.to_le_bytes());

        bytes
    }

    /// Deserialize MANIFEST from bytes
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, ManifestError> {
        if bytes.len() < 48 {
            return Err(ManifestError::TooShort);
        }

        if &bytes[0..4] != MANIFEST_MAGIC {
            return Err(ManifestError::InvalidMagic);
        }

        // Verify CRC
        let data = &bytes[..bytes.len() - 4];
        let stored_crc = u32::from_le_bytes(bytes[bytes.len() - 4..].try_into().unwrap());
        let computed_crc = crc32fast::hash(data);
        if stored_crc != computed_crc {
            return Err(ManifestError::ChecksumMismatch {
                expected: stored_crc,
                computed: computed_crc,
            });
        }

        let mut cursor = 4;

        let format_version = u32::from_le_bytes(bytes[cursor..cursor + 4].try_into().unwrap());
        cursor += 4;

        let database_uuid: [u8; 16] = bytes[cursor..cursor + 16].try_into().unwrap();
        cursor += 16;

        let codec_id_len = u32::from_le_bytes(bytes[cursor..cursor + 4].try_into().unwrap()) as usize;
        cursor += 4;

        let codec_id = String::from_utf8(bytes[cursor..cursor + codec_id_len].to_vec())
            .map_err(|_| ManifestError::InvalidCodecId)?;
        cursor += codec_id_len;

        let active_wal_segment = u64::from_le_bytes(bytes[cursor..cursor + 8].try_into().unwrap());
        cursor += 8;

        let watermark = u64::from_le_bytes(bytes[cursor..cursor + 8].try_into().unwrap());
        cursor += 8;
        let snapshot_watermark = if watermark > 0 { Some(watermark) } else { None };

        let snapshot_id_val = u64::from_le_bytes(bytes[cursor..cursor + 8].try_into().unwrap());
        let snapshot_id = if snapshot_id_val > 0 { Some(snapshot_id_val) } else { None };

        Ok(Manifest {
            format_version,
            database_uuid,
            codec_id,
            active_wal_segment,
            snapshot_watermark,
            snapshot_id,
        })
    }
}

/// MANIFEST persistence manager
pub struct ManifestManager {
    path: PathBuf,
    manifest: Manifest,
}

impl ManifestManager {
    /// Create a new MANIFEST manager (for new database)
    pub fn create(
        path: PathBuf,
        database_uuid: [u8; 16],
        codec_id: String,
    ) -> Result<Self, ManifestError> {
        let manifest = Manifest::new(database_uuid, codec_id);
        let manager = ManifestManager { path, manifest };
        manager.persist()?;
        Ok(manager)
    }

    /// Load existing MANIFEST
    pub fn load(path: PathBuf) -> Result<Self, ManifestError> {
        let bytes = std::fs::read(&path)?;
        let manifest = Manifest::from_bytes(&bytes)?;
        Ok(ManifestManager { path, manifest })
    }

    /// Persist MANIFEST atomically (write-fsync-rename)
    pub fn persist(&self) -> Result<(), ManifestError> {
        let temp_path = self.path.with_extension("tmp");

        let mut file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&temp_path)?;

        file.write_all(&self.manifest.to_bytes())?;
        file.sync_all()?;
        drop(file);

        std::fs::rename(&temp_path, &self.path)?;

        if let Some(parent) = self.path.parent() {
            let dir = File::open(parent)?;
            dir.sync_all()?;
        }

        Ok(())
    }

    /// Update active WAL segment
    pub fn set_active_segment(&mut self, segment: u64) -> Result<(), ManifestError> {
        self.manifest.active_wal_segment = segment;
        self.persist()
    }

    /// Update snapshot watermark
    pub fn set_snapshot_watermark(
        &mut self,
        snapshot_id: u64,
        watermark_txn: u64,
    ) -> Result<(), ManifestError> {
        self.manifest.snapshot_id = Some(snapshot_id);
        self.manifest.snapshot_watermark = Some(watermark_txn);
        self.persist()
    }

    pub fn manifest(&self) -> &Manifest {
        &self.manifest
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ManifestError {
    #[error("MANIFEST too short")]
    TooShort,
    #[error("Invalid magic bytes")]
    InvalidMagic,
    #[error("Invalid codec ID")]
    InvalidCodecId,
    #[error("Checksum mismatch: expected {expected:08x}, computed {computed:08x}")]
    ChecksumMismatch { expected: u32, computed: u32 },
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}
```

### Acceptance Criteria

- [ ] MANIFEST with format_version, database_uuid, codec_id, active_wal_segment, snapshot_watermark, snapshot_id
- [ ] Magic bytes: `STRM` (0x5354524D)
- [ ] CRC32 checksum for integrity
- [ ] `to_bytes()` / `from_bytes()` serialization
- [ ] Atomic persistence using write-fsync-rename
- [ ] `ManifestManager` for state management
- [ ] Error on corrupted/invalid MANIFEST

### Complete Story

```bash
./scripts/complete-story.sh 513
```

---

## Story #514: WAL Replay Implementation

**GitHub Issue**: [#514](https://github.com/anibjoshi/in-mem/issues/514)
**Estimated Time**: 4 hours
**Dependencies**: Epic 70 (WAL)
**Blocks**: Story #515

### Start Story

```bash
gh issue view 514
./scripts/start-story.sh 72 514 wal-replay
```

### Implementation

Create `crates/storage/src/recovery/replay.rs`:

```rust
//! WAL replay for recovery
//!
//! Replays WAL records to reconstruct database state.
//! Replay is deterministic and idempotent.

use std::path::PathBuf;
use crate::format::wal_record::{WalRecord, WalRecordError, SEGMENT_HEADER_SIZE};
use crate::format::writeset::{Writeset, Mutation};
use crate::wal::segment::WalSegment;

/// WAL reader for replay
pub struct WalReader {
    wal_dir: PathBuf,
}

impl WalReader {
    pub fn new(wal_dir: PathBuf) -> Self {
        WalReader { wal_dir }
    }

    /// List all WAL segments in order
    pub fn list_segments(&self) -> std::io::Result<Vec<u64>> {
        let mut segments = Vec::new();

        for entry in std::fs::read_dir(&self.wal_dir)? {
            let entry = entry?;
            let name = entry.file_name().to_string_lossy().to_string();

            if name.starts_with("wal-") && name.ends_with(".seg") {
                if let Ok(num) = name[4..10].parse::<u64>() {
                    segments.push(num);
                }
            }
        }

        segments.sort();
        Ok(segments)
    }

    /// Read all valid records from a segment
    pub fn read_segment(&self, segment_number: u64) -> Result<Vec<WalRecord>, WalReplayError> {
        let segment_path = WalSegment::segment_path(&self.wal_dir, segment_number);
        let file_data = std::fs::read(&segment_path)?;

        let mut records = Vec::new();
        let mut cursor = SEGMENT_HEADER_SIZE;

        while cursor < file_data.len() {
            match WalRecord::from_bytes(&file_data[cursor..]) {
                Ok((record, consumed)) => {
                    records.push(record);
                    cursor += consumed;
                }
                Err(WalRecordError::InsufficientData) => break,
                Err(WalRecordError::ChecksumMismatch { .. }) => break,
                Err(e) => return Err(WalReplayError::RecordError(e)),
            }
        }

        Ok(records)
    }
}

/// WAL replay engine
pub struct WalReplayer {
    reader: WalReader,
}

impl WalReplayer {
    pub fn new(wal_dir: PathBuf) -> Self {
        WalReplayer {
            reader: WalReader::new(wal_dir),
        }
    }

    /// Replay WAL records after a given watermark
    ///
    /// Deterministic and idempotent:
    /// - Same records → same state
    /// - Multiple replays → same result
    pub fn replay_after<F>(
        &self,
        watermark: Option<u64>,
        mut apply_fn: F,
    ) -> Result<ReplayStats, WalReplayError>
    where
        F: FnMut(WalRecord) -> Result<(), WalReplayError>,
    {
        let mut stats = ReplayStats::default();

        let segments = self.reader.list_segments()?;

        for segment_number in segments {
            let records = self.reader.read_segment(segment_number)?;
            stats.segments_read += 1;

            for record in records {
                stats.records_read += 1;

                // Skip records at or before watermark
                if let Some(w) = watermark {
                    if record.txn_id <= w {
                        stats.records_skipped += 1;
                        continue;
                    }
                }

                apply_fn(record)?;
                stats.records_applied += 1;
            }
        }

        Ok(stats)
    }

    /// Replay into engine state
    pub fn replay_into_engine(
        &self,
        engine: &mut Engine,
        watermark: Option<u64>,
    ) -> Result<ReplayStats, WalReplayError> {
        self.replay_after(watermark, |record| {
            let writeset = Writeset::from_bytes(&record.writeset)
                .map_err(|e| WalReplayError::WritesetError(e.to_string()))?;

            for mutation in writeset.mutations {
                match mutation {
                    Mutation::Put { entity_ref, value, version } => {
                        engine.apply_put(entity_ref, value, version, record.timestamp)?;
                    }
                    Mutation::Delete { entity_ref } => {
                        engine.apply_delete(entity_ref)?;
                    }
                    Mutation::Append { entity_ref, value, version } => {
                        engine.apply_append(entity_ref, value, version, record.timestamp)?;
                    }
                }
            }

            engine.update_txn_id(record.txn_id);
            Ok(())
        })
    }
}

/// Statistics from WAL replay
#[derive(Debug, Default)]
pub struct ReplayStats {
    pub segments_read: usize,
    pub records_read: usize,
    pub records_skipped: usize,
    pub records_applied: usize,
}

#[derive(Debug, thiserror::Error)]
pub enum WalReplayError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Record error: {0}")]
    RecordError(#[from] WalRecordError),
    #[error("Writeset error: {0}")]
    WritesetError(String),
    #[error("Apply error: {0}")]
    ApplyError(String),
}
```

### Acceptance Criteria

- [ ] `WalReader` lists and reads segments in order
- [ ] `replay_after(watermark)` skips records <= watermark
- [ ] Replay is deterministic (same records → same state)
- [ ] Replay is idempotent (multiple replays → same result)
- [ ] Stops at corrupted records (tail truncation)
- [ ] Returns `ReplayStats` for observability
- [ ] Applies mutations in order

### Complete Story

```bash
./scripts/complete-story.sh 514
```

---

## Story #515: Snapshot + WAL Recovery Algorithm

**GitHub Issue**: [#515](https://github.com/anibjoshi/in-mem/issues/515)
**Estimated Time**: 4 hours
**Dependencies**: Stories #513, #514, Story #511 (Snapshot Loading)
**Blocks**: Story #517

### Start Story

```bash
gh issue view 515
./scripts/start-story.sh 72 515 recovery-algorithm
```

### Implementation

Create `crates/storage/src/recovery/mod.rs`:

```rust
//! Recovery coordinator
//!
//! Recovery algorithm:
//! 1. Load MANIFEST
//! 2. If snapshot exists: load snapshot → replay WAL > watermark
//! 3. If no snapshot: replay all WAL
//! 4. Truncate partial records at WAL tail

use std::path::PathBuf;
use crate::format::manifest::{ManifestManager, Manifest, ManifestError};
use crate::format::snapshot::{snapshot_path, primitive_tags};
use crate::snapshot::reader::{SnapshotReader, LoadedSnapshot, SnapshotError};
use crate::codec::StorageCodec;

pub mod replay;
use replay::{WalReplayer, WalReplayError, ReplayStats};

/// Recovery coordinator
pub struct Recovery {
    db_dir: PathBuf,
    codec: Box<dyn StorageCodec>,
}

impl Recovery {
    pub fn new(db_dir: PathBuf, codec: Box<dyn StorageCodec>) -> Self {
        Recovery { db_dir, codec }
    }

    /// Perform full recovery
    pub fn recover(&self) -> Result<RecoveryResult, RecoveryError> {
        // Step 1: Load MANIFEST
        let manifest_path = self.db_dir.join("MANIFEST");
        let manifest_manager = ManifestManager::load(manifest_path)?;
        let manifest = manifest_manager.manifest();

        // Validate codec
        if manifest.codec_id != self.codec.codec_id() {
            return Err(RecoveryError::CodecMismatch {
                expected: manifest.codec_id.clone(),
                actual: self.codec.codec_id().to_string(),
            });
        }

        // Step 2: Initialize engine
        let mut engine = Engine::new();

        // Step 3: Load snapshot (if exists)
        let watermark = if let Some(snapshot_id) = manifest.snapshot_id {
            let snap_path = snapshot_path(
                &self.db_dir.join("SNAPSHOTS"),
                snapshot_id,
            );

            let snapshot_reader = SnapshotReader::new(self.codec.clone());
            let loaded = snapshot_reader.load(&snap_path)?;

            self.apply_snapshot(&mut engine, &loaded)?;
            loaded.header.watermark_txn
        } else {
            0
        };

        // Step 4: Replay WAL
        let wal_replayer = WalReplayer::new(self.db_dir.join("WAL"));
        let replay_stats = wal_replayer.replay_into_engine(
            &mut engine,
            if watermark > 0 { Some(watermark) } else { None },
        )?;

        // Step 5: Truncate partial records
        self.truncate_partial_records()?;

        Ok(RecoveryResult {
            engine,
            manifest: manifest.clone(),
            snapshot_watermark: if watermark > 0 { Some(watermark) } else { None },
            replay_stats,
        })
    }

    /// Apply loaded snapshot to engine
    fn apply_snapshot(
        &self,
        engine: &mut Engine,
        snapshot: &LoadedSnapshot,
    ) -> Result<(), RecoveryError> {
        let deserializer = SnapshotReader::new(self.codec.clone());

        for section in &snapshot.sections {
            match section.primitive_type {
                primitive_tags::KV => {
                    let entries = deserializer.deserialize_kv_section(&section.data)?;
                    for entry in entries {
                        engine.load_kv(entry)?;
                    }
                }
                primitive_tags::EVENT => {
                    let entries = deserializer.deserialize_event_section(&section.data)?;
                    for entry in entries {
                        engine.load_event(entry)?;
                    }
                }
                // ... similar for STATE, TRACE, RUN, JSON, VECTOR
                _ => {
                    // Unknown section type - skip for forward compatibility
                }
            }
        }

        engine.set_txn_watermark(snapshot.header.watermark_txn);
        Ok(())
    }

    /// Truncate partial WAL records at tail
    fn truncate_partial_records(&self) -> Result<(), RecoveryError> {
        let wal_dir = self.db_dir.join("WAL");
        let reader = WalReader::new(wal_dir.clone());

        let segments = reader.list_segments()?;
        if segments.is_empty() {
            return Ok(());
        }

        // Only check last (active) segment
        let last_segment = *segments.last().unwrap();
        let segment_path = WalSegment::segment_path(&wal_dir, last_segment);

        let file_data = std::fs::read(&segment_path)?;
        let mut cursor = SEGMENT_HEADER_SIZE;
        let mut valid_end = SEGMENT_HEADER_SIZE;

        while cursor < file_data.len() {
            match WalRecord::from_bytes(&file_data[cursor..]) {
                Ok((_, consumed)) => {
                    cursor += consumed;
                    valid_end = cursor;
                }
                Err(_) => break,
            }
        }

        // Truncate if needed
        if valid_end < file_data.len() {
            let file = std::fs::OpenOptions::new()
                .write(true)
                .open(&segment_path)?;
            file.set_len(valid_end as u64)?;
            file.sync_all()?;
        }

        Ok(())
    }
}

/// Recovery result
pub struct RecoveryResult {
    pub engine: Engine,
    pub manifest: Manifest,
    pub snapshot_watermark: Option<u64>,
    pub replay_stats: ReplayStats,
}

#[derive(Debug, thiserror::Error)]
pub enum RecoveryError {
    #[error("MANIFEST error: {0}")]
    Manifest(#[from] ManifestError),
    #[error("Snapshot error: {0}")]
    Snapshot(#[from] SnapshotError),
    #[error("WAL replay error: {0}")]
    Replay(#[from] WalReplayError),
    #[error("Codec mismatch: expected {expected}, got {actual}")]
    CodecMismatch { expected: String, actual: String },
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Engine error: {0}")]
    Engine(String),
}
```

### Acceptance Criteria

- [ ] Load MANIFEST to determine recovery path
- [ ] Load snapshot if snapshot_id present
- [ ] Replay WAL records > snapshot watermark
- [ ] Replay all WAL if no snapshot
- [ ] Truncate partial records at WAL tail
- [ ] Return complete recovery stats
- [ ] Validate codec matches MANIFEST

### Complete Story

```bash
./scripts/complete-story.sh 515
```

---

## Story #516: Partial Record Truncation

**GitHub Issue**: [#516](https://github.com/anibjoshi/in-mem/issues/516)
**Estimated Time**: 2 hours
**Dependencies**: Story #514
**Blocks**: Story #515

### Start Story

```bash
gh issue view 516
./scripts/start-story.sh 72 516 partial-truncation
```

### Design

Partial records can occur when:
- Crash during WAL append
- Power loss mid-write
- Disk corruption

Recovery must:
1. Detect incomplete records
2. Truncate to last valid record
3. Continue operation

This is safe because:
- Partial records mean transaction wasn't committed (Strict mode would have fsynced)
- Truncating uncommitted data doesn't violate durability guarantees
- In Buffered mode, some data loss is expected on crash

### Implementation

(Included in Story #515 - `truncate_partial_records()`)

### Acceptance Criteria

- [ ] Detect partial records at WAL tail
- [ ] Truncate file to last valid record boundary
- [ ] fsync after truncation
- [ ] Only truncate active segment (closed segments are immutable)
- [ ] Log truncation for observability

### Complete Story

```bash
./scripts/complete-story.sh 516
```

---

## Story #517: Recovery Verification Tests

**GitHub Issue**: [#517](https://github.com/anibjoshi/in-mem/issues/517)
**Estimated Time**: 3 hours
**Dependencies**: Story #515
**Blocks**: None

### Start Story

```bash
gh issue view 517
./scripts/start-story.sh 72 517 recovery-tests
```

### Implementation

Create `crates/storage/tests/recovery_tests.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_recovery_strict_mode_no_data_loss() {
        let dir = tempdir().unwrap();
        let db_dir = dir.path().to_path_buf();

        // Setup and write data
        {
            let mut db = Database::create(&db_dir, DatabaseConfig::strict()).unwrap();
            let run_id = db.create_run("test-run").unwrap();
            db.kv_put(run_id, "key1", b"value1").unwrap();
            db.kv_put(run_id, "key2", b"value2").unwrap();
            drop(db); // Simulate crash
        }

        // Recover
        let db = Database::open(&db_dir, DatabaseConfig::default()).unwrap();
        let run_id = db.resolve_run("test-run").unwrap();

        let v1 = db.kv_get(run_id, "key1").unwrap();
        let v2 = db.kv_get(run_id, "key2").unwrap();

        assert_eq!(v1.unwrap().value, b"value1");
        assert_eq!(v2.unwrap().value, b"value2");
    }

    #[test]
    fn test_recovery_order_preservation() {
        let dir = tempdir().unwrap();
        let db_dir = dir.path().to_path_buf();

        {
            let mut db = Database::create(&db_dir, DatabaseConfig::strict()).unwrap();
            let run_id = db.create_run("test-run").unwrap();

            for i in 0..100 {
                db.event_append(run_id, format!("event-{}", i).as_bytes()).unwrap();
            }
        }

        let db = Database::open(&db_dir, DatabaseConfig::default()).unwrap();
        let run_id = db.resolve_run("test-run").unwrap();

        let events = db.event_range(run_id, 0..100).unwrap();
        assert_eq!(events.len(), 100);

        for (i, event) in events.iter().enumerate() {
            let expected = format!("event-{}", i);
            assert_eq!(event.value, expected.as_bytes());
        }
    }

    #[test]
    fn test_recovery_idempotent() {
        let dir = tempdir().unwrap();
        let db_dir = dir.path().to_path_buf();

        {
            let mut db = Database::create(&db_dir, DatabaseConfig::strict()).unwrap();
            let run_id = db.create_run("test-run").unwrap();
            db.kv_put(run_id, "key1", b"value1").unwrap();
        }

        // Recover multiple times
        let mut states = Vec::new();
        for _ in 0..3 {
            let db = Database::open(&db_dir, DatabaseConfig::default()).unwrap();
            let run_id = db.resolve_run("test-run").unwrap();
            let value = db.kv_get(run_id, "key1").unwrap();
            states.push(value);
            drop(db);
        }

        // All states should be equal
        assert!(states.windows(2).all(|w| w[0] == w[1]));
    }

    #[test]
    fn test_recovery_snapshot_wal_equivalence() {
        let dir = tempdir().unwrap();
        let db_dir = dir.path().to_path_buf();

        {
            let mut db = Database::create(&db_dir, DatabaseConfig::strict()).unwrap();
            let run_id = db.create_run("test-run").unwrap();

            db.kv_put(run_id, "key1", b"before-checkpoint").unwrap();
            db.checkpoint().unwrap();
            db.kv_put(run_id, "key2", b"after-checkpoint").unwrap();
        }

        let db = Database::open(&db_dir, DatabaseConfig::default()).unwrap();
        let run_id = db.resolve_run("test-run").unwrap();

        let v1 = db.kv_get(run_id, "key1").unwrap();
        let v2 = db.kv_get(run_id, "key2").unwrap();

        assert_eq!(v1.unwrap().value, b"before-checkpoint");
        assert_eq!(v2.unwrap().value, b"after-checkpoint");
    }

    #[test]
    fn test_recovery_partial_record_truncation() {
        let dir = tempdir().unwrap();
        let db_dir = dir.path().to_path_buf();

        {
            let mut db = Database::create(&db_dir, DatabaseConfig::strict()).unwrap();
            let run_id = db.create_run("test-run").unwrap();
            db.kv_put(run_id, "key1", b"value1").unwrap();
        }

        // Append garbage to WAL
        let wal_dir = db_dir.join("WAL");
        let segments: Vec<_> = std::fs::read_dir(&wal_dir).unwrap()
            .filter_map(|e| e.ok())
            .collect();

        if let Some(last_segment) = segments.last() {
            let mut file = std::fs::OpenOptions::new()
                .append(true)
                .open(last_segment.path())
                .unwrap();
            std::io::Write::write_all(&mut file, b"GARBAGE_PARTIAL_RECORD").unwrap();
        }

        // Should recover with truncation
        let db = Database::open(&db_dir, DatabaseConfig::default()).unwrap();
        let run_id = db.resolve_run("test-run").unwrap();

        let v1 = db.kv_get(run_id, "key1").unwrap();
        assert_eq!(v1.unwrap().value, b"value1");
    }

    #[test]
    fn test_recovery_all_primitives() {
        let dir = tempdir().unwrap();
        let db_dir = dir.path().to_path_buf();
        let run_name = "test-run";

        {
            let mut db = Database::create(&db_dir, DatabaseConfig::strict()).unwrap();
            let run_id = db.create_run(run_name).unwrap();

            db.kv_put(run_id, "test-key", b"test-value").unwrap();
            db.event_append(run_id, b"test-event").unwrap();
            db.state_set(run_id, "test-cell", b"test-state").unwrap();
            db.checkpoint().unwrap();
        }

        let db = Database::open(&db_dir, DatabaseConfig::default()).unwrap();
        let run_id = db.resolve_run(run_name).unwrap();

        assert!(db.kv_get(run_id, "test-key").unwrap().is_some());
        assert_eq!(db.event_range(run_id, 0..1).unwrap().len(), 1);
        assert!(db.state_get(run_id, "test-cell").unwrap().is_some());
    }
}
```

### Acceptance Criteria

- [ ] Test strict mode: no committed transaction lost
- [ ] Test order preservation during replay
- [ ] Test idempotent replay (multiple recoveries → same state)
- [ ] Test snapshot + WAL equivalence
- [ ] Test partial record truncation
- [ ] Test corrupt checksum stops replay
- [ ] Test multiple checkpoints
- [ ] Test all 7 primitives recovered correctly

### Complete Story

```bash
./scripts/complete-story.sh 517
```

---

## Epic 72 Completion Checklist

### 1. Final Validation

```bash
~/.cargo/bin/cargo build --workspace
~/.cargo/bin/cargo test --workspace
~/.cargo/bin/cargo clippy --workspace -- -D warnings
~/.cargo/bin/cargo fmt --check
```

### 2. Verify Deliverables

- [ ] `Manifest` with atomic persistence
- [ ] `ManifestManager` with write-fsync-rename
- [ ] `WalReader` and `WalReplayer`
- [ ] `Recovery` coordinator
- [ ] Partial record truncation
- [ ] Comprehensive recovery tests

### 3. Run Epic-End Validation

See `docs/prompts/EPIC_END_VALIDATION.md`

### 4. Merge to Develop

```bash
git checkout develop
git merge --no-ff epic-72-recovery -m "Epic 72: Recovery complete

Delivered:
- MANIFEST structure and atomic persistence
- WAL replay implementation
- Snapshot + WAL recovery algorithm
- Partial record truncation
- Recovery verification tests

Stories: #513, #514, #515, #516, #517
"
git push origin develop
gh issue close 513 --comment "Epic 72: Recovery - COMPLETE"
```

---

## Summary

Epic 72 establishes the recovery system:

- **MANIFEST** provides atomic metadata persistence
- **WAL Replay** reconstructs state deterministically
- **Recovery Algorithm** coordinates snapshot + WAL
- **Partial Truncation** handles crash scenarios
- **Verification Tests** ensure correctness

This foundation enables Epic 73 (Retention) and Epic 74 (Compaction).
