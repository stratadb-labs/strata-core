# Epic 70: WAL Infrastructure - Implementation Prompts

**Epic Goal**: Implement append-only, segmented WAL with durability modes

**GitHub Issue**: [#498](https://github.com/anibjoshi/in-mem/issues/498)
**Status**: Ready to begin
**Dependencies**: M9 complete
**Phase**: 1 (WAL Foundation)

---

## NAMING CONVENTION - CRITICAL

> **NEVER use "M10" or "Strata" in the actual codebase or comments.**
>
> - "M10" is an internal milestone tracker only - do not use it in code, comments, or user-facing text
> - All existing crates refer to the database as "in-mem" - use this name consistently
> - Do not use "Strata" anywhere in the codebase
> - This applies to: code, comments, docstrings, error messages, log messages, test names
>
> **CORRECT**: `//! Write-ahead log segment file handling`
> **WRONG**: `//! M10 WAL segment for Strata database`

---

## AUTHORITATIVE SPECIFICATIONS - READ THESE FIRST

**`docs/architecture/M10_ARCHITECTURE.md` is THE AUTHORITATIVE SPEC.**

Before starting ANY story in this epic, read:
1. **Architecture Spec (AUTHORITATIVE)**: `docs/architecture/M10_ARCHITECTURE.md`
2. **Implementation Plan**: `docs/milestones/M10/M10_IMPLEMENTATION_PLAN.md`
3. **Epic Spec**: `docs/milestones/M10/EPIC_70_WAL_INFRASTRUCTURE.md`
4. **Prompt Header**: `docs/prompts/M10/M10_PROMPT_HEADER.md` for the 8 architectural rules

**The architecture spec is LAW.** Epic docs provide implementation details but MUST NOT contradict the architecture spec.

---

## Epic 70 Overview

### Scope
- WAL segment file format (`wal-NNNNNN.seg`)
- WAL record structure with checksums (CRC32)
- Append with durability modes (InMemory, Buffered, Strict)
- Segment rotation when size exceeds limit (default 64MB)
- Writeset serialization
- Codec seam integration (identity codec for MVP)

### Key Rules for Epic 70

1. **WAL is append-only** - Never modify records after writing
2. **Segments are immutable once closed** - Only active segment is writable
3. **Records are self-delimiting** - Length prefix + CRC32 checksum
4. **Durability mode determines fsync behavior** - Strict always fsyncs

### Success Criteria
- [ ] WAL segment file format implemented (`wal-NNNNNN.seg`)
- [ ] 32-byte segment header with magic `STRA` (0x53545241)
- [ ] Self-delimiting WAL records with CRC32
- [ ] `DurabilityMode` enum (InMemory, Buffered, Strict)
- [ ] Automatic segment rotation at size limit
- [ ] `Writeset` with Put, Delete, Append mutations
- [ ] `StorageCodec` trait with identity implementation
- [ ] All tests passing

### Component Breakdown
- **Story #499**: WAL Segment File Format - FOUNDATION
- **Story #500**: WAL Record Structure and Serialization - FOUNDATION
- **Story #501**: WAL Append with Durability Modes - CRITICAL
- **Story #502**: WAL Segment Rotation - CRITICAL
- **Story #503**: Writeset Serialization - CRITICAL
- **Story #504**: WAL Configuration - HIGH
- **Story #505**: Codec Seam Integration - HIGH

---

## File Organization

### Directory Structure

Create this structure FIRST before implementing stories:

```bash
mkdir -p crates/storage/src/format
mkdir -p crates/storage/src/wal
mkdir -p crates/storage/src/codec
touch crates/storage/src/lib.rs
```

**Target structure**:
```
crates/storage/src/
├── lib.rs                    # Crate entry point
├── format/                   # Binary format definitions
│   ├── mod.rs
│   ├── wal_record.rs         # WAL record format
│   └── writeset.rs           # Writeset serialization
├── wal/                      # WAL operational logic
│   ├── mod.rs
│   ├── segment.rs            # Segment file handling
│   ├── writer.rs             # WAL writer
│   └── config.rs             # WAL configuration
└── codec/                    # Codec abstraction
    ├── mod.rs
    └── identity.rs           # Identity codec
```

---

## Dependency Graph

```
Story #499 (Segment Format) ──┬──> Story #501 (Append)
                              │
Story #500 (Record Format) ───┼──> Story #501 (Append)
                              │
Story #503 (Writeset) ────────┘

Story #501 (Append) ──────────> Story #502 (Rotation)

Story #504 (Config) ──────────> Story #501 (Append)

Story #505 (Codec) ───────────> All other stories
```

**Recommended Order**: #505 (Codec) → #504 (Config) → #499 (Segment) → #500 (Record) → #503 (Writeset) → #501 (Append) → #502 (Rotation)

---

## Story #499: WAL Segment File Format

**GitHub Issue**: [#499](https://github.com/anibjoshi/in-mem/issues/499)
**Estimated Time**: 2 hours
**Dependencies**: None
**Blocks**: Story #501

### Start Story

```bash
gh issue view 499
./scripts/start-story.sh 70 499 wal-segment-format
```

### Implementation

Create `crates/storage/src/format/wal_record.rs`:

```rust
//! WAL segment file format
//!
//! Segments are named `wal-NNNNNN.seg` where NNNNNN is zero-padded.
//! Each segment has a 32-byte header followed by WAL records.

pub const SEGMENT_MAGIC: [u8; 4] = *b"STRA";
pub const SEGMENT_FORMAT_VERSION: u32 = 1;
pub const SEGMENT_HEADER_SIZE: usize = 32;

/// WAL segment header (32 bytes)
#[repr(C)]
pub struct SegmentHeader {
    /// Magic bytes: "STRA" (0x53545241)
    pub magic: [u8; 4],
    /// Format version
    pub format_version: u32,
    /// Segment number
    pub segment_number: u64,
    /// Database UUID
    pub database_uuid: [u8; 16],
}
```

### Acceptance Criteria

- [ ] Segment file naming: `wal-NNNNNN.seg` (zero-padded)
- [ ] 32-byte header with magic, format_version, segment_number, database_uuid
- [ ] Magic bytes: `STRA` (0x53545241)
- [ ] `SegmentHeader::to_bytes()` / `from_bytes()` serialization
- [ ] `WalSegment::create()` initializes new segment
- [ ] `WalSegment::open_read()` validates header

### Complete Story

```bash
./scripts/complete-story.sh 499
```

---

## Story #500: WAL Record Structure and Serialization

**GitHub Issue**: [#500](https://github.com/anibjoshi/in-mem/issues/500)
**Estimated Time**: 3 hours
**Dependencies**: None
**Blocks**: Story #501

### Start Story

```bash
gh issue view 500
./scripts/start-story.sh 70 500 wal-record-format
```

### Implementation

Add to `crates/storage/src/format/wal_record.rs`:

```rust
//! WAL record format
//!
//! Record Layout:
//! ┌─────────────────┬──────────────────┬─────────────────────────┬──────────┐
//! │ Length (4 bytes)│ Format Ver (1)   │ Payload (variable)      │ CRC32 (4)│
//! └─────────────────┴──────────────────┴─────────────────────────┴──────────┘

pub const WAL_RECORD_FORMAT_VERSION: u8 = 1;

/// WAL record for a committed transaction
#[derive(Debug, Clone)]
pub struct WalRecord {
    pub txn_id: u64,
    pub run_id: [u8; 16],
    pub timestamp: u64,
    pub writeset: Vec<u8>,
}

impl WalRecord {
    /// Serialize record to bytes with length prefix and CRC32
    pub fn to_bytes(&self) -> Vec<u8> { ... }

    /// Deserialize record, returns (record, bytes_consumed)
    pub fn from_bytes(bytes: &[u8]) -> Result<(Self, usize), WalRecordError> { ... }
}
```

### Acceptance Criteria

- [ ] Self-delimiting format with length prefix
- [ ] CRC32 checksum for integrity
- [ ] `to_bytes()` serializes deterministically
- [ ] `from_bytes()` validates checksum
- [ ] Error on checksum mismatch
- [ ] Error on insufficient data (partial record)
- [ ] Returns bytes consumed for streaming reads

### Complete Story

```bash
./scripts/complete-story.sh 500
```

---

## Story #501: WAL Append with Durability Modes

**GitHub Issue**: [#501](https://github.com/anibjoshi/in-mem/issues/501)
**Estimated Time**: 4 hours
**Dependencies**: Stories #499, #500, #503, #504
**Blocks**: Story #502

### Start Story

```bash
gh issue view 501
./scripts/start-story.sh 70 501 wal-append
```

### Implementation

Create `crates/storage/src/wal/writer.rs`:

```rust
/// Durability mode for WAL writes
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DurabilityMode {
    /// No WAL persistence - data lost on crash
    InMemory,
    /// Buffered writes - fsync on coarse boundary
    Buffered,
    /// Strict durability - fsync after every commit
    Strict,
}

/// WAL writer with durability mode support
pub struct WalWriter {
    segment: WalSegment,
    durability: DurabilityMode,
    // ...
}

impl WalWriter {
    /// Append a record respecting durability mode
    pub fn append(&mut self, record: &WalRecord) -> std::io::Result<()> {
        match self.durability {
            DurabilityMode::InMemory => Ok(()), // No-op
            DurabilityMode::Buffered => { ... }
            DurabilityMode::Strict => {
                self.write_record(record)?;
                self.segment.file.sync_all()?;
                Ok(())
            }
        }
    }
}
```

### Acceptance Criteria

- [ ] `DurabilityMode` enum with InMemory, Buffered, Strict
- [ ] InMemory: no-op, immediate return
- [ ] Buffered: write, periodic fsync based on config
- [ ] Strict: write + fsync before returning
- [ ] `flush()` forces fsync for Buffered mode
- [ ] Error handling for I/O failures

### Complete Story

```bash
./scripts/complete-story.sh 501
```

---

## Story #502: WAL Segment Rotation

**GitHub Issue**: [#502](https://github.com/anibjoshi/in-mem/issues/502)
**Estimated Time**: 2 hours
**Dependencies**: Story #501
**Blocks**: None

### Start Story

```bash
gh issue view 502
./scripts/start-story.sh 70 502 wal-rotation
```

### Acceptance Criteria

- [ ] Rotation when segment size exceeds `config.segment_size`
- [ ] Closed segments are immutable
- [ ] New segment gets incremented segment number
- [ ] Closed segment fsynced before creating new one
- [ ] Segment boundary is not transaction boundary

### Complete Story

```bash
./scripts/complete-story.sh 502
```

---

## Story #503: Writeset Serialization

**GitHub Issue**: [#503](https://github.com/anibjoshi/in-mem/issues/503)
**Estimated Time**: 3 hours
**Dependencies**: None
**Blocks**: Story #501

### Start Story

```bash
gh issue view 503
./scripts/start-story.sh 70 503 writeset-format
```

### Implementation

Create `crates/storage/src/format/writeset.rs`:

```rust
/// A mutation within a transaction writeset
#[derive(Debug, Clone)]
pub enum Mutation {
    Put { entity_ref: EntityRef, value: Vec<u8>, version: u64 },
    Delete { entity_ref: EntityRef },
    Append { entity_ref: EntityRef, value: Vec<u8>, version: u64 },
}

/// Transaction writeset
#[derive(Debug, Clone, Default)]
pub struct Writeset {
    pub mutations: Vec<Mutation>,
}

impl Writeset {
    pub fn to_bytes(&self) -> Vec<u8> { ... }
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, WritesetError> { ... }
}
```

### Acceptance Criteria

- [ ] `Mutation` enum with Put, Delete, Append
- [ ] Version included in Put/Append (assigned by engine)
- [ ] `to_bytes()` serializes deterministically
- [ ] `from_bytes()` deserializes and validates
- [ ] EntityRef serialization integrated

### Complete Story

```bash
./scripts/complete-story.sh 503
```

---

## Story #504: WAL Configuration

**GitHub Issue**: [#504](https://github.com/anibjoshi/in-mem/issues/504)
**Estimated Time**: 1 hour
**Dependencies**: None
**Blocks**: Story #501

### Start Story

```bash
gh issue view 504
./scripts/start-story.sh 70 504 wal-config
```

### Implementation

Create `crates/storage/src/wal/config.rs`:

```rust
/// WAL configuration
#[derive(Debug, Clone)]
pub struct WalConfig {
    /// Maximum segment size in bytes (default: 64MB)
    pub segment_size: u64,
    /// Bytes between fsyncs in Buffered mode (default: 4MB)
    pub buffered_sync_bytes: u64,
}

impl Default for WalConfig {
    fn default() -> Self {
        WalConfig {
            segment_size: 64 * 1024 * 1024,
            buffered_sync_bytes: 4 * 1024 * 1024,
        }
    }
}
```

### Acceptance Criteria

- [ ] `segment_size` default 64MB
- [ ] `buffered_sync_bytes` default 4MB
- [ ] Builder pattern (`with_segment_size()`, etc.)
- [ ] `validate()` checks constraints
- [ ] Configurable at database open time

### Complete Story

```bash
./scripts/complete-story.sh 504
```

---

## Story #505: Codec Seam Integration

**GitHub Issue**: [#505](https://github.com/anibjoshi/in-mem/issues/505)
**Estimated Time**: 2 hours
**Dependencies**: None
**Blocks**: All other stories

### Start Story

```bash
gh issue view 505
./scripts/start-story.sh 70 505 codec-seam
```

### Implementation

Create `crates/storage/src/codec/mod.rs`:

```rust
/// Storage codec trait
///
/// All bytes passing through the storage layer go through the codec.
/// M10 uses IdentityCodec (no transformation).
pub trait StorageCodec: Send + Sync {
    fn encode(&self, data: &[u8]) -> Vec<u8>;
    fn decode(&self, data: &[u8]) -> Result<Vec<u8>, CodecError>;
    fn codec_id(&self) -> &str;
}

/// Identity codec (no transformation)
#[derive(Debug, Clone, Default)]
pub struct IdentityCodec;

impl StorageCodec for IdentityCodec {
    fn encode(&self, data: &[u8]) -> Vec<u8> { data.to_vec() }
    fn decode(&self, data: &[u8]) -> Result<Vec<u8>, CodecError> { Ok(data.to_vec()) }
    fn codec_id(&self) -> &str { "identity" }
}
```

### Acceptance Criteria

- [ ] `StorageCodec` trait with encode/decode
- [ ] `codec_id()` for MANIFEST tracking
- [ ] `IdentityCodec` implementation
- [ ] `get_codec(codec_id)` factory function
- [ ] All WAL/snapshot bytes pass through codec

### Complete Story

```bash
./scripts/complete-story.sh 505
```

---

## Epic 70 Completion Checklist

### 1. Final Validation

```bash
~/.cargo/bin/cargo build --workspace
~/.cargo/bin/cargo test --workspace
~/.cargo/bin/cargo clippy --workspace -- -D warnings
~/.cargo/bin/cargo fmt --check
```

### 2. Verify Deliverables

- [ ] `WalSegment` with create/open/close
- [ ] `WalRecord` with to_bytes/from_bytes
- [ ] `WalWriter` with durability modes
- [ ] `DurabilityMode` enum
- [ ] `Writeset` with mutations
- [ ] `WalConfig` with defaults
- [ ] `StorageCodec` trait and identity impl

### 3. Run Epic-End Validation

See `docs/prompts/EPIC_END_VALIDATION.md`

### 4. Merge to Develop

```bash
git checkout develop
git merge --no-ff epic-70-wal-infrastructure -m "Epic 70: WAL Infrastructure complete

Delivered:
- WAL segment file format
- WAL record structure with CRC32
- Durability modes (InMemory, Buffered, Strict)
- Segment rotation
- Writeset serialization
- Codec seam (identity codec)

Stories: #499, #500, #501, #502, #503, #504, #505
"
git push origin develop
gh issue close 498 --comment "Epic 70: WAL Infrastructure - COMPLETE"
```

---

## Summary

Epic 70 establishes the foundational WAL infrastructure:

- **WAL Segments** provide the append-only storage unit
- **WAL Records** are self-delimiting with checksums
- **Durability Modes** control fsync behavior
- **Writesets** capture transaction mutations
- **Codec Seam** prepares for future encryption

This foundation enables Epic 71 (Snapshot System) and Epic 72 (Recovery).
