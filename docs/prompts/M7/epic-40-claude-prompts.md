# Epic 40: Snapshot Format & Writer - Implementation Prompts

**Epic Goal**: Implement snapshot format and writing with checksums

**GitHub Issue**: [#338](https://github.com/anibjoshi/in-mem/issues/338)
**Status**: Ready to begin (after Epic 42 complete)
**Dependencies**: Epic 42 (WAL Enhancement)

---

## AUTHORITATIVE SPECIFICATIONS - READ THESE FIRST

**`docs/architecture/M7_ARCHITECTURE.md` is THE AUTHORITATIVE SPEC.**

Before starting ANY story in this epic, read:
1. **Architecture Spec (AUTHORITATIVE)**: `docs/architecture/M7_ARCHITECTURE.md`
2. **Epic Spec**: `docs/milestones/M7/EPIC_40_SNAPSHOT_FORMAT.md`
3. **Prompt Header**: `docs/prompts/M7/M7_PROMPT_HEADER.md` for the 5 architectural rules

**The architecture spec is LAW.** Epic docs provide implementation details but MUST NOT contradict the architecture spec.

---

## Epic 40 Overview

### Scope
- Snapshot envelope format with magic bytes, version, checksums
- SnapshotHeader with metadata (timestamp, WAL offset, tx count)
- Per-primitive serialization for snapshots
- SnapshotWriter with atomic write (temp + rename)
- CRC32 checksum for integrity validation

### Key Rule: Snapshots Are Physical, Not Semantic

> Snapshots compress WAL effects. They are a cache over history, not the history itself.

**NEVER** store semantic history (EventLog data, transaction logs) in snapshots. Snapshots contain **materialized state only**.

### Success Criteria
- [ ] Snapshot envelope: magic "INMEM_SNAP", version, timestamp, wal_offset, payload, crc32
- [ ] SnapshotHeader with all metadata fields
- [ ] Each primitive has serialize() for snapshots
- [ ] SnapshotWriter writes atomically (temp file + rename)
- [ ] CRC32 checksum validates entire snapshot
- [ ] No derived data (indexes) in snapshot

### Component Breakdown
- **Story #292 (GitHub #347)**: Snapshot Envelope Format - FOUNDATION
- **Story #293 (GitHub #348)**: SnapshotHeader Type - FOUNDATION
- **Story #294 (GitHub #349)**: Per-Primitive Serialization - CRITICAL
- **Story #295 (GitHub #350)**: SnapshotWriter Implementation - CRITICAL
- **Story #296 (GitHub #351)**: CRC32 Checksum Integration - CRITICAL
- **Story #297 (GitHub #352)**: Atomic Snapshot Write - HIGH

---

## Dependency Graph

```
Story #347 (Envelope) ────┬──> Story #349 (Per-Primitive)
                          │
Story #348 (Header) ──────┴──> Story #350 (Writer) ──> Story #352 (Atomic)
                                     │
Story #351 (CRC32) ──────────────────┘
```

---

## Parallelization Strategy

### Optimal Execution (2 Claudes)

| Phase | Duration | Claude 1 | Claude 2 |
|-------|----------|----------|----------|
| 1 | 2 hours | #347 Envelope | #348 Header |
| 2 | 3 hours | #349 Per-Primitive + #351 CRC32 | - |
| 3 | 3 hours | #350 Writer + #352 Atomic | - |

**Total Wall Time**: ~8 hours (vs. ~12 hours sequential)

---

## Story #347: Snapshot Envelope Format

**GitHub Issue**: [#347](https://github.com/anibjoshi/in-mem/issues/347)
**Estimated Time**: 2 hours
**Dependencies**: None
**Blocks**: Stories #349, #350

### Start Story

```bash
gh issue view 347
./scripts/start-story.sh 40 347 snapshot-envelope
```

### Implementation Steps

#### Step 1: Create snapshot_types.rs module

Create `crates/durability/src/snapshot_types.rs`:

```rust
//! Snapshot format types for M7 Durability
//!
//! Snapshot file layout:
//!
//! +------------------+
//! | Magic (10 bytes) |  "INMEM_SNAP"
//! +------------------+
//! | Version (4)      |  Format version (1 for M7)
//! +------------------+
//! | Timestamp (8)    |  Microseconds since epoch
//! +------------------+
//! | WAL Offset (8)   |  WAL position covered
//! +------------------+
//! | Tx Count (8)     |  Transactions included
//! +------------------+
//! | Primitive Count  |  Number of primitive sections
//! +------------------+
//! | Primitive 1      |  Type (1) + Length (8) + Data
//! +------------------+
//! | ...              |
//! +------------------+
//! | CRC32 (4)        |  Checksum of everything above
//! +------------------+

pub const SNAPSHOT_MAGIC: &[u8; 10] = b"INMEM_SNAP";
pub const SNAPSHOT_VERSION_1: u32 = 1;

/// Snapshot envelope (parsed representation)
#[derive(Debug, Clone)]
pub struct SnapshotEnvelope {
    /// Format version
    pub version: u32,
    /// When snapshot was taken (microseconds since epoch)
    pub timestamp_micros: u64,
    /// WAL offset this snapshot covers up to
    pub wal_offset: u64,
    /// Number of transactions included
    pub transaction_count: u64,
    /// Primitive sections
    pub sections: Vec<PrimitiveSection>,
    /// CRC32 checksum (of everything before this)
    pub checksum: u32,
}

/// A section of snapshot data for one primitive
#[derive(Debug, Clone)]
pub struct PrimitiveSection {
    /// Primitive type ID
    pub primitive_type: u8,
    /// Serialized data
    pub data: Vec<u8>,
}

/// Primitive type IDs for snapshot sections
pub mod primitive_ids {
    pub const KV: u8 = 1;
    pub const JSON: u8 = 2;
    pub const EVENT: u8 = 3;
    pub const STATE: u8 = 4;
    pub const TRACE: u8 = 5;
    pub const RUN: u8 = 6;
    // Reserved for Vector (M8): 7
}

/// Snapshot errors
#[derive(Debug, thiserror::Error)]
pub enum SnapshotError {
    #[error("Snapshot too short")]
    TooShort,

    #[error("Invalid magic bytes")]
    InvalidMagic,

    #[error("Unsupported version: {0}")]
    UnsupportedVersion(u32),

    #[error("Checksum mismatch: expected {expected}, got {actual}")]
    ChecksumMismatch { expected: u32, actual: u32 },

    #[error("Unknown primitive type: {0}")]
    UnknownPrimitive(u8),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Serialization error: {0}")]
    Serialize(String),
}
```

#### Step 2: Update lib.rs

```rust
pub mod snapshot_types;
pub use snapshot_types::*;
```

### Tests

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_magic_bytes() {
        assert_eq!(SNAPSHOT_MAGIC.len(), 10);
        assert_eq!(SNAPSHOT_MAGIC, b"INMEM_SNAP");
    }

    #[test]
    fn test_primitive_ids_unique() {
        let ids = [
            primitive_ids::KV,
            primitive_ids::JSON,
            primitive_ids::EVENT,
            primitive_ids::STATE,
            primitive_ids::TRACE,
            primitive_ids::RUN,
        ];
        let mut set = std::collections::HashSet::new();
        for id in ids {
            assert!(set.insert(id), "Duplicate primitive ID: {}", id);
        }
    }

    #[test]
    fn test_snapshot_envelope_default() {
        let envelope = SnapshotEnvelope {
            version: SNAPSHOT_VERSION_1,
            timestamp_micros: 0,
            wal_offset: 0,
            transaction_count: 0,
            sections: vec![],
            checksum: 0,
        };
        assert_eq!(envelope.version, 1);
    }
}
```

### Validation

```bash
~/.cargo/bin/cargo test -p in-mem-durability -- snapshot_types
~/.cargo/bin/cargo clippy -p in-mem-durability -- -D warnings
```

### Complete Story

```bash
./scripts/complete-story.sh 347
```

---

## Story #348: SnapshotHeader Type

**GitHub Issue**: [#348](https://github.com/anibjoshi/in-mem/issues/348)
**Estimated Time**: 2 hours
**Dependencies**: Story #347
**Blocks**: Stories #350

### Start Story

```bash
gh issue view 348
./scripts/start-story.sh 40 348 snapshot-header
```

### Implementation

Add to `crates/durability/src/snapshot_types.rs`:

```rust
use std::time::{SystemTime, UNIX_EPOCH};

/// Snapshot header with metadata
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

impl SnapshotHeader {
    /// Create new header with current timestamp
    pub fn new(wal_offset: u64, transaction_count: u64) -> Self {
        SnapshotHeader {
            version: SNAPSHOT_VERSION_1,
            timestamp_micros: now_micros(),
            wal_offset,
            transaction_count,
            db_version: env!("CARGO_PKG_VERSION").to_string(),
        }
    }

    /// Serialize header to bytes (excludes magic, includes version onward)
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(64);
        buf.extend_from_slice(SNAPSHOT_MAGIC);
        buf.extend_from_slice(&self.version.to_le_bytes());
        buf.extend_from_slice(&self.timestamp_micros.to_le_bytes());
        buf.extend_from_slice(&self.wal_offset.to_le_bytes());
        buf.extend_from_slice(&self.transaction_count.to_le_bytes());
        buf
    }

    /// Parse header from bytes (including magic)
    pub fn from_bytes(data: &[u8]) -> Result<Self, SnapshotError> {
        // Header: Magic(10) + Version(4) + Timestamp(8) + Offset(8) + TxCount(8) = 38 bytes
        if data.len() < 38 {
            return Err(SnapshotError::TooShort);
        }

        // Validate magic
        if &data[0..10] != SNAPSHOT_MAGIC {
            return Err(SnapshotError::InvalidMagic);
        }

        let version = u32::from_le_bytes([data[10], data[11], data[12], data[13]]);
        if version != SNAPSHOT_VERSION_1 {
            return Err(SnapshotError::UnsupportedVersion(version));
        }

        let timestamp_micros = u64::from_le_bytes(data[14..22].try_into().unwrap());
        let wal_offset = u64::from_le_bytes(data[22..30].try_into().unwrap());
        let transaction_count = u64::from_le_bytes(data[30..38].try_into().unwrap());

        Ok(SnapshotHeader {
            version,
            timestamp_micros,
            wal_offset,
            transaction_count,
            db_version: String::new(),
        })
    }
}

/// Get current time in microseconds since epoch
pub fn now_micros() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("Time went backwards")
        .as_micros() as u64
}
```

### Tests

```rust
#[test]
fn test_snapshot_header_roundtrip() {
    let header = SnapshotHeader::new(12345, 100);
    let bytes = header.to_bytes();

    let parsed = SnapshotHeader::from_bytes(&bytes).unwrap();
    assert_eq!(parsed.version, header.version);
    assert_eq!(parsed.wal_offset, header.wal_offset);
    assert_eq!(parsed.transaction_count, header.transaction_count);
}

#[test]
fn test_snapshot_header_invalid_magic() {
    let mut data = vec![0u8; 38];
    data[0..10].copy_from_slice(b"WRONGMAGIC");

    let result = SnapshotHeader::from_bytes(&data);
    assert!(matches!(result, Err(SnapshotError::InvalidMagic)));
}

#[test]
fn test_snapshot_header_unsupported_version() {
    let mut data = vec![0u8; 38];
    data[0..10].copy_from_slice(SNAPSHOT_MAGIC);
    data[10..14].copy_from_slice(&99u32.to_le_bytes()); // Invalid version

    let result = SnapshotHeader::from_bytes(&data);
    assert!(matches!(result, Err(SnapshotError::UnsupportedVersion(99))));
}
```

### Complete Story

```bash
./scripts/complete-story.sh 348
```

---

## Story #349: Per-Primitive Serialization

**GitHub Issue**: [#349](https://github.com/anibjoshi/in-mem/issues/349)
**Estimated Time**: 3 hours
**Dependencies**: Story #347
**Blocks**: Story #350

### Start Story

```bash
gh issue view 349
./scripts/start-story.sh 40 349 primitive-serialization
```

### Implementation

Create `crates/durability/src/snapshot.rs`:

```rust
//! Snapshot serialization for primitives

use crate::snapshot_types::*;

/// Trait for primitives to implement snapshot serialization
pub trait SnapshotSerializable {
    /// Serialize primitive state for snapshot
    fn snapshot_serialize(&self) -> Result<Vec<u8>, SnapshotError>;

    /// Deserialize primitive state from snapshot
    fn snapshot_deserialize(&mut self, data: &[u8]) -> Result<(), SnapshotError>;

    /// Primitive type ID (for snapshot sections)
    fn primitive_type_id(&self) -> u8;
}

/// Serialize all primitives for snapshot
pub fn serialize_all_primitives(
    kv: &impl SnapshotSerializable,
    json: &impl SnapshotSerializable,
    event: &impl SnapshotSerializable,
    state: &impl SnapshotSerializable,
    trace: &impl SnapshotSerializable,
    run: &impl SnapshotSerializable,
) -> Result<Vec<PrimitiveSection>, SnapshotError> {
    let mut sections = Vec::with_capacity(6);

    // Serialize each primitive
    sections.push(PrimitiveSection {
        primitive_type: kv.primitive_type_id(),
        data: kv.snapshot_serialize()?,
    });

    sections.push(PrimitiveSection {
        primitive_type: json.primitive_type_id(),
        data: json.snapshot_serialize()?,
    });

    sections.push(PrimitiveSection {
        primitive_type: event.primitive_type_id(),
        data: event.snapshot_serialize()?,
    });

    sections.push(PrimitiveSection {
        primitive_type: state.primitive_type_id(),
        data: state.snapshot_serialize()?,
    });

    sections.push(PrimitiveSection {
        primitive_type: trace.primitive_type_id(),
        data: trace.snapshot_serialize()?,
    });

    sections.push(PrimitiveSection {
        primitive_type: run.primitive_type_id(),
        data: run.snapshot_serialize()?,
    });

    Ok(sections)
}

/// Deserialize primitives from snapshot sections
pub fn deserialize_primitives(
    sections: &[PrimitiveSection],
    kv: &mut impl SnapshotSerializable,
    json: &mut impl SnapshotSerializable,
    event: &mut impl SnapshotSerializable,
    state: &mut impl SnapshotSerializable,
    trace: &mut impl SnapshotSerializable,
    run: &mut impl SnapshotSerializable,
) -> Result<(), SnapshotError> {
    for section in sections {
        match section.primitive_type {
            primitive_ids::KV => kv.snapshot_deserialize(&section.data)?,
            primitive_ids::JSON => json.snapshot_deserialize(&section.data)?,
            primitive_ids::EVENT => event.snapshot_deserialize(&section.data)?,
            primitive_ids::STATE => state.snapshot_deserialize(&section.data)?,
            primitive_ids::TRACE => trace.snapshot_deserialize(&section.data)?,
            primitive_ids::RUN => run.snapshot_deserialize(&section.data)?,
            _ => {
                // Unknown primitive - log warning but continue
                // This allows forward compatibility with newer snapshots
                tracing::warn!("Unknown primitive type in snapshot: {}", section.primitive_type);
            }
        }
    }
    Ok(())
}
```

### Acceptance Criteria

- [ ] SnapshotSerializable trait defined
- [ ] All 6 primitives can be serialized
- [ ] Unknown primitives logged but not fatal (forward compat)
- [ ] No derived data (indexes) serialized

### Complete Story

```bash
./scripts/complete-story.sh 349
```

---

## Story #350: SnapshotWriter Implementation

**GitHub Issue**: [#350](https://github.com/anibjoshi/in-mem/issues/350)
**Estimated Time**: 3 hours
**Dependencies**: Stories #348, #349, #351
**Blocks**: Story #352

### Start Story

```bash
gh issue view 350
./scripts/start-story.sh 40 350 snapshot-writer
```

### Implementation

Add to `crates/durability/src/snapshot.rs`:

```rust
use std::fs::File;
use std::io::Write;
use std::path::Path;

/// Snapshot writer
pub struct SnapshotWriter {
    hasher: crc32fast::Hasher,
}

impl SnapshotWriter {
    pub fn new() -> Self {
        SnapshotWriter {
            hasher: crc32fast::Hasher::new(),
        }
    }

    /// Write snapshot to file
    pub fn write(
        &mut self,
        header: &SnapshotHeader,
        sections: &[PrimitiveSection],
        path: &Path,
    ) -> Result<SnapshotInfo, SnapshotError> {
        let mut file = File::create(path)?;
        self.hasher = crc32fast::Hasher::new();

        // Write header
        let header_bytes = header.to_bytes();
        file.write_all(&header_bytes)?;
        self.hasher.update(&header_bytes);

        // Write primitive count
        let count = sections.len() as u8;
        file.write_all(&[count])?;
        self.hasher.update(&[count]);

        // Write each section
        for section in sections {
            // Type
            file.write_all(&[section.primitive_type])?;
            self.hasher.update(&[section.primitive_type]);

            // Length
            let len_bytes = (section.data.len() as u64).to_le_bytes();
            file.write_all(&len_bytes)?;
            self.hasher.update(&len_bytes);

            // Data
            file.write_all(&section.data)?;
            self.hasher.update(&section.data);
        }

        // Write CRC32
        let checksum = self.hasher.clone().finalize();
        file.write_all(&checksum.to_le_bytes())?;

        // Sync to disk
        file.sync_all()?;

        Ok(SnapshotInfo {
            path: path.to_path_buf(),
            timestamp_micros: header.timestamp_micros,
            wal_offset: header.wal_offset,
        })
    }
}

/// Snapshot info returned after successful write
#[derive(Debug, Clone)]
pub struct SnapshotInfo {
    /// Path to snapshot file
    pub path: std::path::PathBuf,
    /// Timestamp when snapshot was taken
    pub timestamp_micros: u64,
    /// WAL offset covered by this snapshot
    pub wal_offset: u64,
}

impl Default for SnapshotWriter {
    fn default() -> Self {
        Self::new()
    }
}
```

### Tests

```rust
#[test]
fn test_snapshot_writer() {
    let temp_dir = tempfile::TempDir::new().unwrap();
    let path = temp_dir.path().join("test.dat");

    let header = SnapshotHeader::new(100, 10);
    let sections = vec![
        PrimitiveSection {
            primitive_type: primitive_ids::KV,
            data: vec![1, 2, 3],
        },
    ];

    let mut writer = SnapshotWriter::new();
    let info = writer.write(&header, &sections, &path).unwrap();

    assert!(path.exists());
    assert_eq!(info.wal_offset, 100);
}
```

### Complete Story

```bash
./scripts/complete-story.sh 350
```

---

## Story #351: CRC32 Checksum Integration

**GitHub Issue**: [#351](https://github.com/anibjoshi/in-mem/issues/351)
**Estimated Time**: 2 hours
**Dependencies**: Story #347
**Blocks**: Story #350

### Start Story

```bash
gh issue view 351
./scripts/start-story.sh 40 351 crc32-checksum
```

### Implementation

Add checksum validation to `crates/durability/src/snapshot.rs`:

```rust
/// Validate snapshot checksum
pub fn validate_snapshot_checksum(path: &Path) -> Result<(), SnapshotError> {
    let data = std::fs::read(path)?;
    validate_checksum_from_bytes(&data)
}

/// Validate checksum from bytes
pub fn validate_checksum_from_bytes(data: &[u8]) -> Result<(), SnapshotError> {
    if data.len() < 4 {
        return Err(SnapshotError::TooShort);
    }

    // Split content and checksum
    let (content, checksum_bytes) = data.split_at(data.len() - 4);

    // Parse stored checksum
    let stored = u32::from_le_bytes([
        checksum_bytes[0],
        checksum_bytes[1],
        checksum_bytes[2],
        checksum_bytes[3],
    ]);

    // Compute checksum
    let mut hasher = crc32fast::Hasher::new();
    hasher.update(content);
    let computed = hasher.finalize();

    if stored != computed {
        return Err(SnapshotError::ChecksumMismatch {
            expected: stored,
            actual: computed,
        });
    }

    Ok(())
}
```

### Tests

```rust
#[test]
fn test_checksum_validation() {
    let temp_dir = tempfile::TempDir::new().unwrap();
    let path = temp_dir.path().join("test.dat");

    // Write valid snapshot
    let header = SnapshotHeader::new(100, 10);
    let sections = vec![];
    let mut writer = SnapshotWriter::new();
    writer.write(&header, &sections, &path).unwrap();

    // Validate should pass
    validate_snapshot_checksum(&path).unwrap();
}

#[test]
fn test_checksum_detects_corruption() {
    let temp_dir = tempfile::TempDir::new().unwrap();
    let path = temp_dir.path().join("test.dat");

    // Write valid snapshot
    let header = SnapshotHeader::new(100, 10);
    let sections = vec![];
    let mut writer = SnapshotWriter::new();
    writer.write(&header, &sections, &path).unwrap();

    // Corrupt the file
    let mut data = std::fs::read(&path).unwrap();
    data[20] ^= 0xFF;
    std::fs::write(&path, &data).unwrap();

    // Validate should fail
    let result = validate_snapshot_checksum(&path);
    assert!(matches!(result, Err(SnapshotError::ChecksumMismatch { .. })));
}
```

### Acceptance Criteria

- [ ] CRC32 computed over all data before checksum
- [ ] Uses crc32fast for performance
- [ ] Validation compares stored vs computed
- [ ] ChecksumMismatch error includes both values

### Complete Story

```bash
./scripts/complete-story.sh 351
```

---

## Story #352: Atomic Snapshot Write

**GitHub Issue**: [#352](https://github.com/anibjoshi/in-mem/issues/352)
**Estimated Time**: 2 hours
**Dependencies**: Story #350

### Start Story

```bash
gh issue view 352
./scripts/start-story.sh 40 352 atomic-write
```

### Implementation

Add atomic write wrapper:

```rust
impl SnapshotWriter {
    /// Write snapshot atomically
    ///
    /// 1. Write to temp file
    /// 2. Sync temp file
    /// 3. Rename temp to final (atomic on POSIX)
    ///
    /// If any step fails, temp file is cleaned up.
    pub fn write_atomic(
        &mut self,
        header: &SnapshotHeader,
        sections: &[PrimitiveSection],
        path: &Path,
    ) -> Result<SnapshotInfo, SnapshotError> {
        let temp_path = path.with_extension("tmp");

        // Clean up stale temp file if exists
        let _ = std::fs::remove_file(&temp_path);

        // Write to temp
        let result = self.write(header, sections, &temp_path);

        match result {
            Ok(info) => {
                // Atomic rename
                match std::fs::rename(&temp_path, path) {
                    Ok(()) => Ok(SnapshotInfo {
                        path: path.to_path_buf(),
                        ..info
                    }),
                    Err(e) => {
                        let _ = std::fs::remove_file(&temp_path);
                        Err(SnapshotError::Io(e))
                    }
                }
            }
            Err(e) => {
                let _ = std::fs::remove_file(&temp_path);
                Err(e)
            }
        }
    }
}
```

### Tests

```rust
#[test]
fn test_atomic_write_cleanup_on_success() {
    let temp_dir = tempfile::TempDir::new().unwrap();
    let path = temp_dir.path().join("snapshot.dat");
    let temp_path = path.with_extension("tmp");

    // Create stale temp file
    std::fs::write(&temp_path, b"stale").unwrap();

    let header = SnapshotHeader::new(100, 10);
    let mut writer = SnapshotWriter::new();
    writer.write_atomic(&header, &[], &path).unwrap();

    // Temp should be gone, final should exist
    assert!(!temp_path.exists());
    assert!(path.exists());
}
```

### Acceptance Criteria

- [ ] Write to temp file first
- [ ] Sync before rename
- [ ] Atomic rename on POSIX
- [ ] Clean up temp on failure
- [ ] Clean up stale temp on start

### Complete Story

```bash
./scripts/complete-story.sh 352
```

---

## Epic 40 Completion Checklist

### 1. Final Validation

```bash
~/.cargo/bin/cargo test -p in-mem-durability -- snapshot
~/.cargo/bin/cargo test --workspace
~/.cargo/bin/cargo clippy --workspace -- -D warnings
~/.cargo/bin/cargo fmt --check
```

### 2. Verify Deliverables

- [ ] Snapshot envelope format with magic bytes "INMEM_SNAP"
- [ ] SnapshotHeader with version, timestamp, wal_offset, tx_count
- [ ] Per-primitive serialization (6 primitives)
- [ ] SnapshotWriter with CRC32 checksum
- [ ] Atomic write (temp + rename)
- [ ] No derived data in snapshots

### 3. Merge to Develop

```bash
git checkout develop
git merge --no-ff epic-40-snapshot-format -m "Epic 40: Snapshot Format & Writer complete

Delivered:
- Snapshot envelope format with magic, version, checksum
- SnapshotHeader with metadata
- Per-primitive serialization
- SnapshotWriter with atomic write
- CRC32 checksum integration

Stories: #347, #348, #349, #350, #351, #352
"
git push origin develop
gh issue close 338 --comment "Epic 40: Snapshot Format & Writer - COMPLETE"
```

---

## Summary

Epic 40 establishes the snapshot format that enables bounded recovery time. These snapshots are **physical materialized state** (not semantic history) with integrity protection via CRC32 checksums.
