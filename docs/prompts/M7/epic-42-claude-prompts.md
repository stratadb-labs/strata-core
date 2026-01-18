# Epic 42: WAL Enhancement - Implementation Prompts

**Epic Goal**: Enhance WAL with checksums and transaction framing

**GitHub Issue**: [#340](https://github.com/anibjoshi/in-mem/issues/340)
**Status**: CRITICAL FOUNDATION - Start first
**Dependencies**: M6 complete

---

## AUTHORITATIVE SPECIFICATIONS - READ THESE FIRST

**`docs/architecture/M7_ARCHITECTURE.md` is THE AUTHORITATIVE SPEC.**

Before starting ANY story in this epic, read:
1. **Architecture Spec (AUTHORITATIVE)**: `docs/architecture/M7_ARCHITECTURE.md`
2. **Epic Spec**: `docs/milestones/M7/EPIC_42_WAL_ENHANCEMENT.md`
3. **Prompt Header**: `docs/prompts/M7/M7_PROMPT_HEADER.md` for the 5 architectural rules

---

## Epic 42 Overview

### Why This Epic Is CRITICAL

Epic 42 establishes the WAL format that enables:
- **Corruption detection** via CRC32 on every entry
- **Atomic transactions** via commit markers
- **Prefix-consistent recovery** via transaction framing
- **Extensibility** via entry type registry

**ALL other M7 epics depend on this work.**

### Scope
- WAL entry envelope with CRC32 checksums
- Transaction framing with commit markers
- WAL entry type registry (0x00-0xFF allocations)
- WAL truncation after snapshot
- Corruption detection and handling

### Key Rules

1. **Every WAL entry MUST have CRC32 checksum**
2. **Every data mutation MUST be in a transaction with tx_id**
3. **Transactions MUST have commit marker to be visible after recovery**
4. **Entry type registry MUST be extensible**

### Success Criteria
- [ ] Every WAL entry: length, type, version, tx_id, payload, crc32
- [ ] Transaction entries share tx_id, commit marker required
- [ ] Entry types 0x00-0x0F reserved for core, 0x10+ for primitives
- [ ] WAL truncation after successful snapshot (atomic)
- [ ] Corrupt entry detected by CRC mismatch, skipped gracefully

### Component Breakdown
- **Story #305 (GitHub #360)**: WAL Entry Envelope with CRC32 - CRITICAL
- **Story #306 (GitHub #361)**: Transaction Framing (Commit Markers) - CRITICAL
- **Story #307 (GitHub #362)**: WAL Entry Type Registry - FOUNDATION
- **Story #308 (GitHub #363)**: WAL Truncation After Snapshot - HIGH
- **Story #309 (GitHub #364)**: WAL Corruption Detection - HIGH

---

## Dependency Graph

```
Story #362 (Registry) ──> Story #360 (Envelope) ──> Story #361 (Framing)
                                    │
                                    v
                         Story #364 (Corruption)
                                    │
                                    v
                         Story #363 (Truncation)
```

---

## Story #360: WAL Entry Envelope with CRC32

**GitHub Issue**: [#360](https://github.com/anibjoshi/in-mem/issues/360)
**Estimated Time**: 3 hours
**Dependencies**: Story #362
**Blocks**: Story #361, #363, #364

### Start Story

```bash
gh issue view 360
./scripts/start-story.sh 42 360 wal-envelope
```

### Implementation

Modify `crates/durability/src/wal_types.rs`:

```rust
//! WAL entry format for M7
//!
//! Entry format:
//! +----------------+
//! | Length (u32)   |  Total bytes after this field
//! +----------------+
//! | Type (u8)      |  Entry type from registry
//! +----------------+
//! | Version (u8)   |  Format version for this entry type
//! +----------------+
//! | TxId (16)      |  Transaction ID (UUID)
//! +----------------+
//! | Payload        |  Type-specific data
//! +----------------+
//! | CRC32 (u32)    |  Checksum of Type + Version + TxId + Payload
//! +----------------+

use crate::wal_entry_types::WalEntryType;
use uuid::Uuid;

/// Transaction ID
pub type TxId = Uuid;

/// WAL entry with envelope
#[derive(Debug, Clone)]
pub struct WalEntry {
    /// Entry type
    pub entry_type: WalEntryType,
    /// Format version
    pub version: u8,
    /// Transaction ID
    pub tx_id: Option<TxId>,
    /// Payload (type-specific)
    pub payload: Vec<u8>,
}

impl WalEntry {
    /// Serialize entry with envelope and checksum
    pub fn serialize(&self) -> Vec<u8> {
        let mut content = Vec::new();

        // Type
        content.push(self.entry_type as u8);

        // Version
        content.push(self.version);

        // TxId (16 bytes, or zeros if None)
        match self.tx_id {
            Some(id) => content.extend_from_slice(id.as_bytes()),
            None => content.extend_from_slice(&[0u8; 16]),
        }

        // Payload
        content.extend_from_slice(&self.payload);

        // Compute CRC32 of content
        let mut hasher = crc32fast::Hasher::new();
        hasher.update(&content);
        let crc = hasher.finalize();

        // Build final buffer
        let total_len = content.len() + 4; // +4 for CRC
        let mut buf = Vec::with_capacity(4 + total_len);

        // Length (total after length field)
        buf.extend_from_slice(&(total_len as u32).to_le_bytes());

        // Content
        buf.extend_from_slice(&content);

        // CRC32
        buf.extend_from_slice(&crc.to_le_bytes());

        buf
    }

    /// Deserialize and validate entry
    pub fn deserialize(data: &[u8]) -> Result<Self, WalError> {
        // Minimum: length(4) + type(1) + version(1) + txid(16) + crc(4) = 26
        if data.len() < 26 {
            return Err(WalError::TooShort);
        }

        let len = u32::from_le_bytes([data[0], data[1], data[2], data[3]]) as usize;
        if data.len() < 4 + len {
            return Err(WalError::TooShort);
        }

        // Content is from byte 4 to (4 + len - 4) = end - 4 bytes (before CRC)
        let content_end = 4 + len - 4;
        let content = &data[4..content_end];

        // Validate CRC
        let stored_crc = u32::from_le_bytes([
            data[content_end],
            data[content_end + 1],
            data[content_end + 2],
            data[content_end + 3],
        ]);
        let mut hasher = crc32fast::Hasher::new();
        hasher.update(content);
        let computed_crc = hasher.finalize();

        if stored_crc != computed_crc {
            return Err(WalError::ChecksumMismatch {
                expected: stored_crc,
                actual: computed_crc,
            });
        }

        // Parse content
        let entry_type = WalEntryType::try_from(content[0])?;
        let version = content[1];

        let tx_id_bytes: [u8; 16] = content[2..18].try_into().unwrap();
        let tx_id = if tx_id_bytes == [0u8; 16] {
            None
        } else {
            Some(Uuid::from_bytes(tx_id_bytes))
        };

        let payload = content[18..].to_vec();

        Ok(WalEntry {
            entry_type,
            version,
            tx_id,
            payload,
        })
    }
}

/// WAL errors
#[derive(Debug, thiserror::Error)]
pub enum WalError {
    #[error("Entry too short")]
    TooShort,

    #[error("Checksum mismatch: expected {expected}, got {actual}")]
    ChecksumMismatch { expected: u32, actual: u32 },

    #[error("Unknown entry type: {0}")]
    UnknownEntryType(u8),

    #[error("Entry too large: {0}")]
    EntryTooLarge(usize),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}
```

### Tests

```rust
#[test]
fn test_wal_entry_roundtrip() {
    let entry = WalEntry {
        entry_type: WalEntryType::KvPut,
        version: 1,
        tx_id: Some(Uuid::new_v4()),
        payload: vec![1, 2, 3, 4],
    };

    let serialized = entry.serialize();
    let deserialized = WalEntry::deserialize(&serialized).unwrap();

    assert_eq!(entry.entry_type, deserialized.entry_type);
    assert_eq!(entry.version, deserialized.version);
    assert_eq!(entry.tx_id, deserialized.tx_id);
    assert_eq!(entry.payload, deserialized.payload);
}

#[test]
fn test_wal_entry_detects_corruption() {
    let entry = WalEntry {
        entry_type: WalEntryType::KvPut,
        version: 1,
        tx_id: Some(Uuid::new_v4()),
        payload: vec![1, 2, 3, 4],
    };

    let mut serialized = entry.serialize();
    serialized[10] ^= 0xFF; // Corrupt a byte

    let result = WalEntry::deserialize(&serialized);
    assert!(matches!(result, Err(WalError::ChecksumMismatch { .. })));
}
```

### Acceptance Criteria

- [ ] Entry format: length + type + version + txid + payload + crc32
- [ ] CRC32 computed over type + version + txid + payload
- [ ] Corruption detected by CRC mismatch
- [ ] Roundtrip serialization works

### Complete Story

```bash
./scripts/complete-story.sh 360
```

---

## Story #361: Transaction Framing (Commit Markers)

**GitHub Issue**: [#361](https://github.com/anibjoshi/in-mem/issues/361)
**Estimated Time**: 3 hours
**Dependencies**: Story #360

### Start Story

```bash
gh issue view 361
./scripts/start-story.sh 42 361 tx-framing
```

### Implementation

```rust
/// Transaction in WAL:
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
    /// Write a transaction with commit marker
    ///
    /// All entries share the same tx_id.
    /// Commit marker written at end.
    pub fn write_transaction(&mut self, tx: &Transaction) -> Result<(), WalError> {
        let tx_id = tx.id();

        // Write all entries with tx_id
        for entry in tx.entries() {
            let wal_entry = WalEntry {
                entry_type: entry.wal_entry_type(),
                version: 1,
                tx_id: Some(tx_id),
                payload: entry.serialize(),
            };
            self.write_entry(&wal_entry)?;
        }

        // Write commit marker
        let commit = WalEntry {
            entry_type: WalEntryType::TransactionCommit,
            version: 1,
            tx_id: Some(tx_id),
            payload: vec![],
        };
        self.write_entry(&commit)?;

        // Sync based on durability mode
        self.sync_if_required()?;

        Ok(())
    }

    /// Write abort marker (for explicit rollback)
    pub fn write_abort(&mut self, tx_id: TxId) -> Result<(), WalError> {
        let abort = WalEntry {
            entry_type: WalEntryType::TransactionAbort,
            version: 1,
            tx_id: Some(tx_id),
            payload: vec![],
        };
        self.write_entry(&abort)?;
        Ok(())
    }
}
```

### Acceptance Criteria

- [ ] All entries in transaction share tx_id
- [ ] Commit marker written at end
- [ ] No entries visible without commit marker
- [ ] Abort marker supported for explicit rollback

### Complete Story

```bash
./scripts/complete-story.sh 361
```

---

## Story #362: WAL Entry Type Registry

**GitHub Issue**: [#362](https://github.com/anibjoshi/in-mem/issues/362)
**Estimated Time**: 2 hours
**Dependencies**: None
**Blocks**: All other Epic 42 stories

### Start Story

```bash
gh issue view 362
./scripts/start-story.sh 42 362 entry-registry
```

### Implementation

Create `crates/durability/src/wal_entry_types.rs`:

```rust
//! WAL entry type registry
//!
//! Ranges:
//! - 0x00-0x0F: Core (transaction control)
//! - 0x10-0x1F: KV primitive
//! - 0x20-0x2F: JSON primitive
//! - 0x30-0x3F: Event primitive
//! - 0x40-0x4F: State primitive
//! - 0x50-0x5F: Trace primitive
//! - 0x60-0x6F: Run primitive
//! - 0x70-0x7F: Reserved for Vector (M8)
//! - 0x80-0xFF: Reserved for future primitives

/// WAL entry types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum WalEntryType {
    // Core (0x00-0x0F)
    TransactionCommit = 0x00,
    TransactionAbort = 0x01,
    SnapshotMarker = 0x02,

    // KV (0x10-0x1F)
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
}

impl TryFrom<u8> for WalEntryType {
    type Error = WalError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0x00 => Ok(WalEntryType::TransactionCommit),
            0x01 => Ok(WalEntryType::TransactionAbort),
            0x02 => Ok(WalEntryType::SnapshotMarker),

            0x10 => Ok(WalEntryType::KvPut),
            0x11 => Ok(WalEntryType::KvDelete),

            0x20 => Ok(WalEntryType::JsonCreate),
            0x21 => Ok(WalEntryType::JsonSet),
            0x22 => Ok(WalEntryType::JsonDelete),
            0x23 => Ok(WalEntryType::JsonPatch),

            0x30 => Ok(WalEntryType::EventAppend),

            0x40 => Ok(WalEntryType::StateInit),
            0x41 => Ok(WalEntryType::StateSet),
            0x42 => Ok(WalEntryType::StateTransition),

            0x50 => Ok(WalEntryType::TraceRecord),

            0x60 => Ok(WalEntryType::RunCreate),
            0x61 => Ok(WalEntryType::RunUpdate),
            0x62 => Ok(WalEntryType::RunEnd),
            0x63 => Ok(WalEntryType::RunBegin),

            _ => Err(WalError::UnknownEntryType(value)),
        }
    }
}

impl WalEntryType {
    /// Check if this is a control entry (commit, abort, snapshot)
    pub fn is_control(&self) -> bool {
        matches!(
            self,
            WalEntryType::TransactionCommit
                | WalEntryType::TransactionAbort
                | WalEntryType::SnapshotMarker
        )
    }

    /// Get the primitive this entry type belongs to
    pub fn primitive(&self) -> Option<PrimitiveKind> {
        match *self as u8 {
            0x00..=0x0F => None, // Core
            0x10..=0x1F => Some(PrimitiveKind::Kv),
            0x20..=0x2F => Some(PrimitiveKind::Json),
            0x30..=0x3F => Some(PrimitiveKind::Event),
            0x40..=0x4F => Some(PrimitiveKind::State),
            0x50..=0x5F => Some(PrimitiveKind::Trace),
            0x60..=0x6F => Some(PrimitiveKind::Run),
            _ => None,
        }
    }
}
```

### Acceptance Criteria

- [ ] All entry types defined with correct values
- [ ] 0x00-0x0F reserved for core
- [ ] 0x70-0x7F reserved for Vector (M8)
- [ ] TryFrom<u8> implemented
- [ ] is_control() and primitive() helpers

### Complete Story

```bash
./scripts/complete-story.sh 362
```

---

## Story #363: WAL Truncation After Snapshot

**GitHub Issue**: [#363](https://github.com/anibjoshi/in-mem/issues/363)
**Estimated Time**: 3 hours
**Dependencies**: Story #360

### Start Story

```bash
gh issue view 363
./scripts/start-story.sh 42 363 wal-truncation
```

### Implementation

```rust
impl WalManager {
    /// Truncate WAL after successful snapshot
    ///
    /// Creates new WAL with only entries after the snapshot offset.
    /// Uses atomic rename for safety.
    pub fn truncate_to(&self, offset: u64) -> Result<(), WalError> {
        // Keep small buffer before offset for safety
        let safe_offset = offset.saturating_sub(1024);

        // Create temp file
        let temp_path = self.wal_path.with_extension("tmp");
        let mut temp_file = File::create(&temp_path)?;

        // Copy entries after offset
        let mut reader = self.open_reader()?;
        reader.seek_to(safe_offset)?;

        while let Some(entry) = reader.next_entry()? {
            temp_file.write_all(&entry.serialize())?;
        }

        temp_file.sync_all()?;

        // Atomic replace
        std::fs::rename(&temp_path, &self.wal_path)?;

        // Update base offset
        self.base_offset.store(safe_offset, Ordering::Release);

        Ok(())
    }

    /// Get current WAL size in bytes
    pub fn size(&self) -> Result<u64, WalError> {
        let metadata = std::fs::metadata(&self.wal_path)?;
        Ok(metadata.len())
    }
}
```

### Acceptance Criteria

- [ ] Creates temp file with entries after offset
- [ ] Atomic rename for safety
- [ ] Updates base offset tracking
- [ ] Cleans up temp on failure

### Complete Story

```bash
./scripts/complete-story.sh 363
```

---

## Story #364: WAL Corruption Detection

**GitHub Issue**: [#364](https://github.com/anibjoshi/in-mem/issues/364)
**Estimated Time**: 2 hours
**Dependencies**: Story #360

### Start Story

```bash
gh issue view 364
./scripts/start-story.sh 42 364 corruption-detection
```

### Implementation

```rust
impl WalReader {
    /// Read next entry, handling corruption gracefully
    pub fn next_entry(&mut self) -> Result<Option<WalEntry>, WalError> {
        // Try to read length
        let mut len_bytes = [0u8; 4];
        match self.file.read_exact(&mut len_bytes) {
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
                return Ok(None); // End of WAL
            }
            Err(e) => return Err(WalError::Io(e)),
        }

        let len = u32::from_le_bytes(len_bytes) as usize;

        // Sanity check on length
        const MAX_WAL_ENTRY_SIZE: usize = 16 * 1024 * 1024; // 16 MB
        if len > MAX_WAL_ENTRY_SIZE {
            return Err(WalError::EntryTooLarge(len));
        }

        // Read entry data
        let mut data = vec![0u8; len];
        self.file.read_exact(&mut data)?;

        // Prepend length for deserialization
        let mut full_data = Vec::with_capacity(4 + len);
        full_data.extend_from_slice(&len_bytes);
        full_data.extend_from_slice(&data);

        WalEntry::deserialize(&full_data).map(Some)
    }

    /// Scan WAL and report corruption statistics
    pub fn scan_for_corruption(&mut self) -> WalScanResult {
        let mut result = WalScanResult::default();

        while let Some(entry_result) = self.try_next_entry() {
            match entry_result {
                Ok(_) => result.valid_entries += 1,
                Err(WalError::ChecksumMismatch { .. }) => result.corrupt_entries += 1,
                Err(_) => result.other_errors += 1,
            }
        }

        result
    }
}

#[derive(Default, Debug)]
pub struct WalScanResult {
    pub valid_entries: u64,
    pub corrupt_entries: u64,
    pub other_errors: u64,
}
```

### Acceptance Criteria

- [ ] CRC mismatch returns ChecksumMismatch error
- [ ] Entry too large detected
- [ ] End of file handled gracefully
- [ ] Scan function for diagnostics

### Complete Story

```bash
./scripts/complete-story.sh 364
```

---

## Epic 42 Completion Checklist

### 1. Final Validation

```bash
~/.cargo/bin/cargo test -p in-mem-durability -- wal
~/.cargo/bin/cargo test --workspace
~/.cargo/bin/cargo clippy --workspace -- -D warnings
~/.cargo/bin/cargo fmt --check
```

### 2. Verify Deliverables

- [ ] WAL entry envelope: length + type + version + txid + payload + crc32
- [ ] Transaction framing with commit markers
- [ ] Entry type registry (all primitives)
- [ ] WAL truncation after snapshot
- [ ] Corruption detection via CRC

### 3. Merge to Develop

```bash
git checkout develop
git merge --no-ff epic-42-wal-enhancement -m "Epic 42: WAL Enhancement complete

Delivered:
- WAL entry envelope with CRC32
- Transaction framing with commit markers
- WAL entry type registry
- WAL truncation after snapshot
- Corruption detection

Stories: #360, #361, #362, #363, #364
"
git push origin develop
gh issue close 340 --comment "Epic 42: WAL Enhancement - COMPLETE"
```

---

## Summary

Epic 42 establishes the WAL format that enables corruption detection and prefix-consistent recovery. Every entry is checksummed, every transaction is framed with commit markers, and the entry type registry is extensible for future primitives.
