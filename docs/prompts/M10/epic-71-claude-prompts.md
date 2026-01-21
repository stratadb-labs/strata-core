# Epic 71: Snapshot System - Implementation Prompts

**Epic Goal**: Implement point-in-time snapshots with crash-safe creation

**GitHub Issue**: [#506](https://github.com/anibjoshi/in-mem/issues/506)
**Status**: Ready to begin
**Dependencies**: Epic 70 (WAL Infrastructure)
**Phase**: 2 (Snapshot System)

---

## NAMING CONVENTION - CRITICAL

> **NEVER use "M10" or "Strata" in the actual codebase or comments.**
>
> - "M10" is an internal milestone tracker only - do not use it in code, comments, or user-facing text
> - All existing crates refer to the database as "in-mem" - use this name consistently
> - Do not use "Strata" anywhere in the codebase
> - This applies to: code, comments, docstrings, error messages, log messages, test names
>
> **CORRECT**: `//! Point-in-time snapshot for in-mem database`
> **WRONG**: `//! M10 Snapshot for Strata database`

---

## AUTHORITATIVE SPECIFICATIONS - READ THESE FIRST

**`docs/architecture/M10_ARCHITECTURE.md` is THE AUTHORITATIVE SPEC.**

Before starting ANY story in this epic, read:
1. **Architecture Spec (AUTHORITATIVE)**: `docs/architecture/M10_ARCHITECTURE.md`
2. **Implementation Plan**: `docs/milestones/M10/M10_IMPLEMENTATION_PLAN.md`
3. **Epic Spec**: `docs/milestones/M10/EPIC_71_SNAPSHOT_SYSTEM.md`
4. **Prompt Header**: `docs/prompts/M10/M10_PROMPT_HEADER.md` for the 8 architectural rules

**The architecture spec is LAW.** Epic docs provide implementation details but MUST NOT contradict the architecture spec.

---

## Epic 71 Overview

### Scope
- Snapshot file format (`snap-NNNNNN.chk`)
- Snapshot serialization for all 7 primitives
- Crash-safe snapshot creation (write-fsync-rename)
- Checkpoint API
- Snapshot metadata and watermark tracking
- Snapshot loading for recovery

### Key Rules for Epic 71

1. **Snapshots are logical** - Materialized state, not memory dumps
2. **Crash-safe creation** - write-fsync-rename pattern
3. **All primitives included** - KV, Event, State, Trace, Run, Json, Vector
4. **Watermark tracking** - Snapshot knows its transaction watermark

### Success Criteria
- [ ] Snapshot file format implemented (`snap-NNNNNN.chk`)
- [ ] 64-byte snapshot header with magic `SNAP` (0x534E4150)
- [ ] Serialization for all 7 primitives
- [ ] Crash-safe snapshot creation (write-fsync-rename)
- [ ] Checkpoint API returns CheckpointInfo
- [ ] Snapshot loading for recovery
- [ ] All tests passing

### Component Breakdown
- **Story #506**: Snapshot File Format - FOUNDATION
- **Story #507**: Snapshot Serialization (All Primitives) - CRITICAL
- **Story #508**: Crash-Safe Snapshot Creation - CRITICAL
- **Story #509**: Checkpoint API - CRITICAL
- **Story #510**: Snapshot Metadata and Watermark - HIGH
- **Story #511**: Snapshot Loading - CRITICAL

---

## File Organization

### Directory Structure

Extend the structure from Epic 70:

```bash
mkdir -p crates/storage/src/snapshot
```

**Target structure**:
```
crates/storage/src/
├── lib.rs
├── format/
│   ├── mod.rs
│   ├── wal_record.rs         # From Epic 70
│   ├── writeset.rs           # From Epic 70
│   ├── snapshot.rs           # NEW - Snapshot format
│   └── primitives.rs         # NEW - Primitive serialization
├── wal/                      # From Epic 70
│   └── ...
├── snapshot/                 # NEW
│   ├── mod.rs
│   ├── writer.rs             # Crash-safe writer
│   └── reader.rs             # Snapshot reader
└── codec/                    # From Epic 70
    └── ...
```

---

## Dependency Graph

```
Story #506 (Snapshot Format) ──> Story #508 (Crash-Safe Creation)
                              │
Story #507 (Serialization) ───┘
                              │
                              └──> Story #509 (Checkpoint API)
                                        │
Story #510 (Watermark) ─────────────────┘
                                        │
                              └──> Story #511 (Loading)
```

**Recommended Order**: #506 (Format) → #507 (Serialization) → #508 (Crash-Safe) → #510 (Watermark) → #509 (Checkpoint) → #511 (Loading)

---

## Story #506: Snapshot File Format

**GitHub Issue**: [#506](https://github.com/anibjoshi/in-mem/issues/506)
**Estimated Time**: 2 hours
**Dependencies**: None
**Blocks**: Stories #507, #508

### Start Story

```bash
gh issue view 506
./scripts/start-story.sh 71 506 snapshot-format
```

### Implementation

Create `crates/storage/src/format/snapshot.rs`:

```rust
//! Snapshot file format
//!
//! Snapshots are named `snap-NNNNNN.chk` where NNNNNN is zero-padded.
//! Each snapshot has a 64-byte header followed by primitive sections.

use std::path::{Path, PathBuf};

pub const SNAPSHOT_MAGIC: [u8; 4] = *b"SNAP";
pub const SNAPSHOT_FORMAT_VERSION: u32 = 1;
pub const SNAPSHOT_HEADER_SIZE: usize = 64;

/// Snapshot header (64 bytes)
#[repr(C)]
pub struct SnapshotHeader {
    /// Magic bytes: "SNAP" (0x534E4150)
    pub magic: [u8; 4],
    /// Format version for forward compatibility
    pub format_version: u32,
    /// Snapshot identifier
    pub snapshot_id: u64,
    /// Watermark transaction ID
    pub watermark_txn: u64,
    /// Creation timestamp (microseconds since epoch)
    pub created_at: u64,
    /// Database UUID
    pub database_uuid: [u8; 16],
    /// Codec ID length (followed by codec ID string in body)
    pub codec_id_len: u8,
    /// Reserved for future use
    pub reserved: [u8; 15],
}

impl SnapshotHeader {
    pub fn to_bytes(&self) -> [u8; SNAPSHOT_HEADER_SIZE] {
        let mut bytes = [0u8; SNAPSHOT_HEADER_SIZE];
        bytes[0..4].copy_from_slice(&self.magic);
        bytes[4..8].copy_from_slice(&self.format_version.to_le_bytes());
        bytes[8..16].copy_from_slice(&self.snapshot_id.to_le_bytes());
        bytes[16..24].copy_from_slice(&self.watermark_txn.to_le_bytes());
        bytes[24..32].copy_from_slice(&self.created_at.to_le_bytes());
        bytes[32..48].copy_from_slice(&self.database_uuid);
        bytes[48] = self.codec_id_len;
        bytes[49..64].copy_from_slice(&self.reserved);
        bytes
    }

    pub fn from_bytes(bytes: &[u8; SNAPSHOT_HEADER_SIZE]) -> Option<Self> {
        Some(SnapshotHeader {
            magic: bytes[0..4].try_into().ok()?,
            format_version: u32::from_le_bytes(bytes[4..8].try_into().ok()?),
            snapshot_id: u64::from_le_bytes(bytes[8..16].try_into().ok()?),
            watermark_txn: u64::from_le_bytes(bytes[16..24].try_into().ok()?),
            created_at: u64::from_le_bytes(bytes[24..32].try_into().ok()?),
            database_uuid: bytes[32..48].try_into().ok()?,
            codec_id_len: bytes[48],
            reserved: bytes[49..64].try_into().ok()?,
        })
    }
}

/// Section header for each primitive type
#[derive(Debug, Clone)]
pub struct SectionHeader {
    /// Primitive type tag
    pub primitive_type: u8,
    /// Section data length (bytes)
    pub data_len: u64,
}

impl SectionHeader {
    pub const SIZE: usize = 9;

    pub fn to_bytes(&self) -> [u8; Self::SIZE] {
        let mut bytes = [0u8; Self::SIZE];
        bytes[0] = self.primitive_type;
        bytes[1..9].copy_from_slice(&self.data_len.to_le_bytes());
        bytes
    }

    pub fn from_bytes(bytes: &[u8; Self::SIZE]) -> Self {
        SectionHeader {
            primitive_type: bytes[0],
            data_len: u64::from_le_bytes(bytes[1..9].try_into().unwrap()),
        }
    }
}

/// Primitive type tags for snapshot sections
pub mod primitive_tags {
    pub const KV: u8 = 0x01;
    pub const EVENT: u8 = 0x02;
    pub const STATE: u8 = 0x03;
    pub const TRACE: u8 = 0x04;
    pub const RUN: u8 = 0x05;
    pub const JSON: u8 = 0x06;
    pub const VECTOR: u8 = 0x07;
}

/// Generate snapshot file path
pub fn snapshot_path(dir: &Path, snapshot_id: u64) -> PathBuf {
    dir.join(format!("snap-{:06}.chk", snapshot_id))
}
```

### Acceptance Criteria

- [ ] Snapshot file naming: `snap-NNNNNN.chk` (zero-padded)
- [ ] 64-byte header with magic, format_version, snapshot_id, watermark_txn, created_at, database_uuid
- [ ] Magic bytes: `SNAP` (0x534E4150)
- [ ] Section headers for each primitive type
- [ ] Primitive type tags defined
- [ ] File path generation helper

### Complete Story

```bash
./scripts/complete-story.sh 506
```

---

## Story #507: Snapshot Serialization (All Primitives)

**GitHub Issue**: [#507](https://github.com/anibjoshi/in-mem/issues/507)
**Estimated Time**: 4 hours
**Dependencies**: Story #506
**Blocks**: Story #508

### Start Story

```bash
gh issue view 507
./scripts/start-story.sh 71 507 snapshot-serialization
```

### Implementation

Create `crates/storage/src/format/primitives.rs`:

```rust
//! Primitive serialization for snapshots
//!
//! Each primitive has a defined binary format for snapshot storage.
//! All values pass through the codec for encoding/decoding.

use crate::codec::StorageCodec;
use crate::format::snapshot::{SectionHeader, primitive_tags};

/// Snapshot serializer for all primitives
pub struct SnapshotSerializer {
    codec: Box<dyn StorageCodec>,
}

impl SnapshotSerializer {
    pub fn new(codec: Box<dyn StorageCodec>) -> Self {
        SnapshotSerializer { codec }
    }

    /// Serialize KV entries
    /// Format per entry: key_len(4) + key + value_len(4) + value + version(8) + timestamp(8)
    pub fn serialize_kv_section(
        &self,
        entries: impl Iterator<Item = KvEntry>,
    ) -> Vec<u8> {
        let mut data = Vec::new();
        let mut count = 0u32;
        data.extend_from_slice(&[0u8; 4]); // Reserve for count

        for entry in entries {
            let key_bytes = entry.key.as_bytes();
            data.extend_from_slice(&(key_bytes.len() as u32).to_le_bytes());
            data.extend_from_slice(key_bytes);

            let value_bytes = self.codec.encode(&entry.value);
            data.extend_from_slice(&(value_bytes.len() as u32).to_le_bytes());
            data.extend_from_slice(&value_bytes);

            data.extend_from_slice(&entry.version.to_le_bytes());
            data.extend_from_slice(&entry.timestamp.to_le_bytes());

            count += 1;
        }

        data[0..4].copy_from_slice(&count.to_le_bytes());
        data
    }

    /// Serialize Event entries
    /// Format per event: sequence(8) + payload_len(4) + payload + timestamp(8)
    pub fn serialize_event_section(
        &self,
        events: impl Iterator<Item = EventEntry>,
    ) -> Vec<u8> {
        let mut data = Vec::new();
        let mut count = 0u32;
        data.extend_from_slice(&[0u8; 4]);

        for event in events {
            data.extend_from_slice(&event.sequence.to_le_bytes());

            let payload_bytes = self.codec.encode(&event.payload);
            data.extend_from_slice(&(payload_bytes.len() as u32).to_le_bytes());
            data.extend_from_slice(&payload_bytes);

            data.extend_from_slice(&event.timestamp.to_le_bytes());
            count += 1;
        }

        data[0..4].copy_from_slice(&count.to_le_bytes());
        data
    }

    /// Serialize State cells
    /// Format per cell: name_len(4) + name + value_len(4) + value + counter(8) + timestamp(8)
    pub fn serialize_state_section(
        &self,
        cells: impl Iterator<Item = StateCell>,
    ) -> Vec<u8> {
        let mut data = Vec::new();
        let mut count = 0u32;
        data.extend_from_slice(&[0u8; 4]);

        for cell in cells {
            let name_bytes = cell.name.as_bytes();
            data.extend_from_slice(&(name_bytes.len() as u32).to_le_bytes());
            data.extend_from_slice(name_bytes);

            let value_bytes = self.codec.encode(&cell.value);
            data.extend_from_slice(&(value_bytes.len() as u32).to_le_bytes());
            data.extend_from_slice(&value_bytes);

            data.extend_from_slice(&cell.counter.to_le_bytes());
            data.extend_from_slice(&cell.timestamp.to_le_bytes());
            count += 1;
        }

        data[0..4].copy_from_slice(&count.to_le_bytes());
        data
    }

    // Similar methods for Trace, Run, Json, Vector...
}

// Placeholder entry types (integrate with actual primitive types)
pub struct KvEntry {
    pub key: String,
    pub value: Vec<u8>,
    pub version: u64,
    pub timestamp: u64,
}

pub struct EventEntry {
    pub sequence: u64,
    pub payload: Vec<u8>,
    pub timestamp: u64,
}

pub struct StateCell {
    pub name: String,
    pub value: Vec<u8>,
    pub counter: u64,
    pub timestamp: u64,
}
```

### Acceptance Criteria

- [ ] KV serialization: key, value, version, timestamp
- [ ] Event serialization: sequence, payload, timestamp
- [ ] State serialization: name, value, counter, timestamp
- [ ] Trace serialization: trace_id, parent, spans
- [ ] Run serialization: run_id, name, created_at, metadata
- [ ] Json serialization: doc_id, content, version, timestamp
- [ ] Vector serialization: collection config, vectors with embeddings
- [ ] All values pass through codec
- [ ] Logical state only (no internal data structures)

### Complete Story

```bash
./scripts/complete-story.sh 507
```

---

## Story #508: Crash-Safe Snapshot Creation

**GitHub Issue**: [#508](https://github.com/anibjoshi/in-mem/issues/508)
**Estimated Time**: 3 hours
**Dependencies**: Stories #506, #507
**Blocks**: Story #509

### Start Story

```bash
gh issue view 508
./scripts/start-story.sh 71 508 crash-safe-snapshot
```

### Implementation

Create `crates/storage/src/snapshot/writer.rs`:

```rust
//! Crash-safe snapshot writer
//!
//! Uses write-fsync-rename pattern for atomic snapshot creation.

use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use crate::format::snapshot::*;
use crate::codec::StorageCodec;

/// Snapshot writer with crash-safe semantics
pub struct SnapshotWriter {
    snapshots_dir: PathBuf,
    codec: Box<dyn StorageCodec>,
    database_uuid: [u8; 16],
}

impl SnapshotWriter {
    pub fn new(
        snapshots_dir: PathBuf,
        codec: Box<dyn StorageCodec>,
        database_uuid: [u8; 16],
    ) -> std::io::Result<Self> {
        std::fs::create_dir_all(&snapshots_dir)?;
        Ok(SnapshotWriter { snapshots_dir, codec, database_uuid })
    }

    /// Create a snapshot using crash-safe write pattern:
    /// 1. Write to temporary file
    /// 2. fsync temporary file
    /// 3. Atomic rename to final path
    pub fn create_snapshot(
        &self,
        snapshot_id: u64,
        watermark_txn: u64,
        sections: Vec<SnapshotSection>,
    ) -> std::io::Result<SnapshotInfo> {
        let final_path = snapshot_path(&self.snapshots_dir, snapshot_id);
        let temp_path = self.snapshots_dir.join(format!(".snap-{:06}.tmp", snapshot_id));

        // Step 1: Write to temporary file
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temp_path)?;

        let created_at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_micros() as u64;

        let header = SnapshotHeader {
            magic: SNAPSHOT_MAGIC,
            format_version: SNAPSHOT_FORMAT_VERSION,
            snapshot_id,
            watermark_txn,
            created_at,
            database_uuid: self.database_uuid,
            codec_id_len: self.codec.codec_id().len() as u8,
            reserved: [0u8; 15],
        };
        file.write_all(&header.to_bytes())?;
        file.write_all(self.codec.codec_id().as_bytes())?;

        // Write sections
        for section in &sections {
            let section_header = SectionHeader {
                primitive_type: section.primitive_type,
                data_len: section.data.len() as u64,
            };
            file.write_all(&section_header.to_bytes())?;
            file.write_all(&section.data)?;
        }

        // Write footer CRC32
        let file_crc = self.compute_file_crc(&temp_path)?;
        file.write_all(&file_crc.to_le_bytes())?;

        // Step 2: fsync
        file.sync_all()?;
        drop(file);

        // Step 3: Atomic rename
        std::fs::rename(&temp_path, &final_path)?;

        // fsync parent directory
        let dir = File::open(&self.snapshots_dir)?;
        dir.sync_all()?;

        Ok(SnapshotInfo {
            snapshot_id,
            watermark_txn,
            timestamp: created_at,
            path: final_path,
        })
    }

    fn compute_file_crc(&self, path: &Path) -> std::io::Result<u32> {
        let data = std::fs::read(path)?;
        let mut hasher = crc32fast::Hasher::new();
        hasher.update(&data);
        Ok(hasher.finalize())
    }

    /// Clean up incomplete temporary files
    pub fn cleanup_temp_files(&self) -> std::io::Result<()> {
        for entry in std::fs::read_dir(&self.snapshots_dir)? {
            let entry = entry?;
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with(".snap-") && name.ends_with(".tmp") {
                std::fs::remove_file(entry.path())?;
            }
        }
        Ok(())
    }
}

/// Snapshot section data
pub struct SnapshotSection {
    pub primitive_type: u8,
    pub data: Vec<u8>,
}

/// Information about a created snapshot
#[derive(Debug, Clone)]
pub struct SnapshotInfo {
    pub snapshot_id: u64,
    pub watermark_txn: u64,
    pub timestamp: u64,
    pub path: PathBuf,
}
```

### Acceptance Criteria

- [ ] Write-fsync-rename pattern for crash safety
- [ ] Temporary file has `.tmp` extension
- [ ] fsync before rename
- [ ] fsync parent directory after rename
- [ ] Returns SnapshotInfo with metadata
- [ ] `cleanup_temp_files()` removes incomplete snapshots
- [ ] Footer contains CRC32 of file contents

### Complete Story

```bash
./scripts/complete-story.sh 508
```

---

## Story #509: Checkpoint API

**GitHub Issue**: [#509](https://github.com/anibjoshi/in-mem/issues/509)
**Estimated Time**: 2 hours
**Dependencies**: Stories #508, #510
**Blocks**: None

### Start Story

```bash
gh issue view 509
./scripts/start-story.sh 71 509 checkpoint-api
```

### Implementation

Add to `crates/storage/src/snapshot/mod.rs`:

```rust
/// Checkpoint result
#[derive(Debug, Clone)]
pub struct CheckpointInfo {
    /// Transaction ID at checkpoint
    pub watermark_txn: u64,
    /// Snapshot identifier
    pub snapshot_id: u64,
    /// Timestamp of checkpoint
    pub timestamp: u64,
}

impl Database {
    /// Create a checkpoint (snapshot) of the current database state
    ///
    /// Captures a point-in-time view of all committed transactions.
    /// After checkpoint:
    /// - Snapshot file contains all state at watermark
    /// - WAL entries > watermark are still needed for recovery
    /// - WAL entries <= watermark can be removed by compaction
    pub fn checkpoint(&self) -> Result<CheckpointInfo, StorageError> {
        let watermark_txn = self.engine.current_txn_id();
        let snapshot_id = self.next_snapshot_id();

        let serializer = SnapshotSerializer::new(self.codec.clone());

        let sections = vec![
            SnapshotSection {
                primitive_type: primitive_tags::KV,
                data: serializer.serialize_kv_section(self.kv_entries()),
            },
            SnapshotSection {
                primitive_type: primitive_tags::EVENT,
                data: serializer.serialize_event_section(self.event_entries()),
            },
            // ... all 7 primitives
        ];

        let snapshot_info = self.snapshot_writer.create_snapshot(
            snapshot_id,
            watermark_txn,
            sections,
        )?;

        self.manifest.set_snapshot_watermark(snapshot_id, watermark_txn)?;

        Ok(CheckpointInfo {
            watermark_txn,
            snapshot_id,
            timestamp: snapshot_info.timestamp,
        })
    }

    fn next_snapshot_id(&self) -> u64 {
        self.manifest.snapshot_id().map(|id| id + 1).unwrap_or(1)
    }
}
```

### Acceptance Criteria

- [ ] `checkpoint()` creates snapshot and returns CheckpointInfo
- [ ] Returns watermark_txn, snapshot_id, timestamp
- [ ] Updates MANIFEST with new snapshot watermark
- [ ] Serializes all 7 primitives
- [ ] Can be called multiple times (incremental snapshots)

### Complete Story

```bash
./scripts/complete-story.sh 509
```

---

## Story #510: Snapshot Metadata and Watermark

**GitHub Issue**: [#510](https://github.com/anibjoshi/in-mem/issues/510)
**Estimated Time**: 1 hour
**Dependencies**: Story #506
**Blocks**: Story #509

### Start Story

```bash
gh issue view 510
./scripts/start-story.sh 71 510 snapshot-watermark
```

### Implementation

Update `crates/storage/src/format/manifest.rs`:

```rust
impl Manifest {
    /// Set snapshot watermark
    pub fn set_snapshot_watermark(&mut self, snapshot_id: u64, watermark_txn: u64) {
        self.snapshot_id = Some(snapshot_id);
        self.snapshot_watermark = Some(watermark_txn);
    }

    /// Get snapshot watermark
    pub fn snapshot_watermark(&self) -> Option<u64> {
        self.snapshot_watermark
    }

    /// Get snapshot ID
    pub fn snapshot_id(&self) -> Option<u64> {
        self.snapshot_id
    }
}
```

### Acceptance Criteria

- [ ] MANIFEST tracks snapshot_id and snapshot_watermark
- [ ] `set_snapshot_watermark()` updates both fields
- [ ] Recovery uses watermark to determine WAL replay start point

### Complete Story

```bash
./scripts/complete-story.sh 510
```

---

## Story #511: Snapshot Loading

**GitHub Issue**: [#511](https://github.com/anibjoshi/in-mem/issues/511)
**Estimated Time**: 3 hours
**Dependencies**: Stories #506, #507
**Blocks**: None

### Start Story

```bash
gh issue view 511
./scripts/start-story.sh 71 511 snapshot-loading
```

### Implementation

Create `crates/storage/src/snapshot/reader.rs`:

```rust
//! Snapshot reader for recovery

use std::fs::File;
use std::io::{Read, BufReader};
use std::path::Path;
use crate::format::snapshot::*;
use crate::codec::{StorageCodec, CodecError};

/// Snapshot reader for recovery
pub struct SnapshotReader {
    codec: Box<dyn StorageCodec>,
}

impl SnapshotReader {
    pub fn new(codec: Box<dyn StorageCodec>) -> Self {
        SnapshotReader { codec }
    }

    /// Load snapshot from file
    pub fn load(&self, path: &Path) -> Result<LoadedSnapshot, SnapshotError> {
        let file = File::open(path)?;
        let mut reader = BufReader::new(file);

        // Read and validate header
        let mut header_bytes = [0u8; SNAPSHOT_HEADER_SIZE];
        reader.read_exact(&mut header_bytes)?;

        let header = SnapshotHeader::from_bytes(&header_bytes)
            .ok_or(SnapshotError::InvalidHeader)?;

        if header.magic != SNAPSHOT_MAGIC {
            return Err(SnapshotError::InvalidMagic);
        }

        // Read codec ID
        let mut codec_id = vec![0u8; header.codec_id_len as usize];
        reader.read_exact(&mut codec_id)?;
        let codec_id = String::from_utf8(codec_id)
            .map_err(|_| SnapshotError::InvalidCodecId)?;

        if codec_id != self.codec.codec_id() {
            return Err(SnapshotError::CodecMismatch {
                expected: codec_id,
                actual: self.codec.codec_id().to_string(),
            });
        }

        // Read sections
        let mut sections = Vec::new();
        loop {
            let mut section_header_bytes = [0u8; SectionHeader::SIZE];
            match reader.read_exact(&mut section_header_bytes) {
                Ok(_) => {
                    let section_header = SectionHeader::from_bytes(&section_header_bytes);
                    if section_header.primitive_type == 0 {
                        break; // Footer
                    }

                    let mut data = vec![0u8; section_header.data_len as usize];
                    reader.read_exact(&mut data)?;

                    sections.push(LoadedSection {
                        primitive_type: section_header.primitive_type,
                        data,
                    });
                }
                Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
                Err(e) => return Err(e.into()),
            }
        }

        Ok(LoadedSnapshot { header, codec_id, sections })
    }

    /// Deserialize KV section
    pub fn deserialize_kv_section(&self, data: &[u8]) -> Result<Vec<KvEntry>, SnapshotError> {
        // Implementation matches serialization format
        let mut cursor = 0;
        let count = u32::from_le_bytes(data[0..4].try_into().unwrap()) as usize;
        cursor += 4;

        let mut entries = Vec::with_capacity(count);
        for _ in 0..count {
            // Parse key, value, version, timestamp
            // ...
        }
        Ok(entries)
    }

    // Similar deserialize methods for other primitives...
}

/// Loaded snapshot data
pub struct LoadedSnapshot {
    pub header: SnapshotHeader,
    pub codec_id: String,
    pub sections: Vec<LoadedSection>,
}

pub struct LoadedSection {
    pub primitive_type: u8,
    pub data: Vec<u8>,
}

#[derive(Debug, thiserror::Error)]
pub enum SnapshotError {
    #[error("Invalid snapshot header")]
    InvalidHeader,
    #[error("Invalid magic bytes")]
    InvalidMagic,
    #[error("Invalid codec ID")]
    InvalidCodecId,
    #[error("Codec mismatch: expected {expected}, got {actual}")]
    CodecMismatch { expected: String, actual: String },
    #[error("Invalid data")]
    InvalidData,
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Codec error: {0}")]
    Codec(#[from] CodecError),
}
```

### Acceptance Criteria

- [ ] `load()` reads and validates snapshot file
- [ ] Validates magic bytes and format version
- [ ] Validates codec ID matches
- [ ] Returns LoadedSnapshot with header and sections
- [ ] Deserialize methods for each primitive type
- [ ] Error on corrupted/invalid snapshots

### Complete Story

```bash
./scripts/complete-story.sh 511
```

---

## Epic 71 Completion Checklist

### 1. Final Validation

```bash
~/.cargo/bin/cargo build --workspace
~/.cargo/bin/cargo test --workspace
~/.cargo/bin/cargo clippy --workspace -- -D warnings
~/.cargo/bin/cargo fmt --check
```

### 2. Verify Deliverables

- [ ] `SnapshotHeader` with 64-byte format
- [ ] `SnapshotWriter` with crash-safe creation
- [ ] `SnapshotReader` with validation
- [ ] `SnapshotSerializer` for all 7 primitives
- [ ] `checkpoint()` API with CheckpointInfo
- [ ] MANIFEST watermark tracking

### 3. Run Epic-End Validation

See `docs/prompts/EPIC_END_VALIDATION.md`

### 4. Merge to Develop

```bash
git checkout develop
git merge --no-ff epic-71-snapshot-system -m "Epic 71: Snapshot System complete

Delivered:
- Snapshot file format (snap-NNNNNN.chk)
- Crash-safe snapshot creation (write-fsync-rename)
- Serialization for all 7 primitives
- Checkpoint API
- Snapshot metadata and watermark tracking
- Snapshot loading for recovery

Stories: #506, #507, #508, #509, #510, #511
"
git push origin develop
gh issue close 506 --comment "Epic 71: Snapshot System - COMPLETE"
```

---

## Summary

Epic 71 establishes the snapshot system:

- **Snapshot Format** provides point-in-time persistence
- **Crash-Safe Creation** ensures atomicity via write-fsync-rename
- **Primitive Serialization** captures all 7 primitives
- **Checkpoint API** exposes user-facing functionality
- **Watermark Tracking** enables efficient WAL compaction

This foundation enables Epic 72 (Recovery) and Epic 74 (Compaction).
