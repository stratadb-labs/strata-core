# Epic 42: WAL Enhancement

**Goal**: Enhance WAL with checksums and transaction framing

**Dependencies**: M6 complete

---

## Scope

- WAL entry envelope with CRC32 checksum
- Transaction framing with commit markers
- WAL entry type registry for extensibility
- WAL truncation after successful snapshot
- Corruption detection and handling

---

## User Stories

| Story | Description | Priority |
|-------|-------------|----------|
| #305 | WAL Entry Envelope with CRC32 | CRITICAL |
| #306 | Transaction Framing (Commit Markers) | CRITICAL |
| #307 | WAL Entry Type Registry | FOUNDATION |
| #308 | WAL Truncation After Snapshot | HIGH |
| #309 | WAL Corruption Detection | HIGH |

---

## Story #305: WAL Entry Envelope with CRC32

**File**: `crates/durability/src/wal_types.rs`

**Deliverable**: Self-validating WAL entry format

### Implementation

```rust
/// WAL entry envelope format:
///
/// +----------------+
/// | Length (u32)   |  Total bytes after this field
/// +----------------+
/// | Type (u8)      |  Entry type from registry
/// +----------------+
/// | Version (u8)   |  Format version for this entry type
/// +----------------+
/// | TxId (16)      |  Transaction ID (optional, 0 = none)
/// +----------------+
/// | Payload        |  Type-specific data
/// +----------------+
/// | CRC32 (u32)    |  Checksum of Type + Version + TxId + Payload
/// +----------------+

use uuid::Uuid;

/// Transaction ID
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TxId(Uuid);

impl TxId {
    pub fn new() -> Self {
        TxId(Uuid::new_v4())
    }

    pub fn nil() -> Self {
        TxId(Uuid::nil())
    }

    pub fn is_nil(&self) -> bool {
        self.0.is_nil()
    }

    pub fn to_bytes(&self) -> [u8; 16] {
        *self.0.as_bytes()
    }

    pub fn from_bytes(bytes: [u8; 16]) -> Self {
        TxId(Uuid::from_bytes(bytes))
    }
}

/// WAL entry with envelope
#[derive(Debug, Clone)]
pub struct WalEntry {
    /// Entry type
    pub entry_type: WalEntryType,
    /// Format version for this entry type
    pub version: u8,
    /// Transaction ID (nil for non-transactional entries)
    pub tx_id: TxId,
    /// Payload (type-specific)
    pub payload: Vec<u8>,
}

impl WalEntry {
    /// Serialize entry with envelope and checksum
    pub fn serialize(&self) -> Vec<u8> {
        // Calculate payload
        let mut content = Vec::new();
        content.push(self.entry_type as u8);
        content.push(self.version);
        content.extend_from_slice(&self.tx_id.to_bytes());
        content.extend_from_slice(&self.payload);

        // Compute CRC32
        let mut hasher = crc32fast::Hasher::new();
        hasher.update(&content);
        let crc = hasher.finalize();

        // Build final buffer
        let total_len = content.len() + 4;  // +4 for CRC
        let mut buf = Vec::with_capacity(4 + total_len);
        buf.extend_from_slice(&(total_len as u32).to_le_bytes());
        buf.extend_from_slice(&content);
        buf.extend_from_slice(&crc.to_le_bytes());

        buf
    }

    /// Deserialize and validate entry
    pub fn deserialize(data: &[u8]) -> Result<Self, WalError> {
        // Minimum: length(4) + type(1) + version(1) + tx_id(16) + crc(4) = 26
        if data.len() < 26 {
            return Err(WalError::TooShort);
        }

        // Parse length
        let len = u32::from_le_bytes([data[0], data[1], data[2], data[3]]) as usize;
        if data.len() < 4 + len {
            return Err(WalError::TooShort);
        }

        // Extract content and CRC
        let content = &data[4..4 + len - 4];
        let crc_offset = 4 + len - 4;
        let stored_crc = u32::from_le_bytes([
            data[crc_offset],
            data[crc_offset + 1],
            data[crc_offset + 2],
            data[crc_offset + 3],
        ]);

        // Validate CRC
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
        let tx_id = TxId::from_bytes(content[2..18].try_into().unwrap());
        let payload = content[18..].to_vec();

        Ok(WalEntry {
            entry_type,
            version,
            tx_id,
            payload,
        })
    }
}
```

### Acceptance Criteria

- [ ] Entry format: length, type, version, tx_id, payload, crc32
- [ ] CRC32 covers type + version + tx_id + payload
- [ ] serialize() produces valid format
- [ ] deserialize() validates checksum
- [ ] Returns ChecksumMismatch on corruption

---

## Story #306: Transaction Framing (Commit Markers)

**File**: `crates/durability/src/wal.rs`

**Deliverable**: Transaction framing with commit markers

### Implementation

```rust
impl WalWriter {
    /// Begin a transaction
    ///
    /// Returns a TxId to use for subsequent writes.
    pub fn begin_transaction(&self) -> TxId {
        TxId::new()
    }

    /// Write entry as part of a transaction
    pub fn write_tx_entry(
        &self,
        tx_id: TxId,
        entry_type: WalEntryType,
        payload: Vec<u8>,
    ) -> Result<(), WalError> {
        let entry = WalEntry {
            entry_type,
            version: 1,
            tx_id,
            payload,
        };
        self.write_entry(&entry)
    }

    /// Commit a transaction
    ///
    /// Writes commit marker and syncs if in Strict mode.
    pub fn commit_transaction(&self, tx_id: TxId) -> Result<(), WalError> {
        let entry = WalEntry {
            entry_type: WalEntryType::TransactionCommit,
            version: 1,
            tx_id,
            payload: vec![],
        };
        self.write_entry(&entry)?;

        // Sync based on durability mode
        if self.durability_mode == DurabilityMode::Strict {
            self.sync()?;
        }

        Ok(())
    }

    /// Abort a transaction
    ///
    /// Writes abort marker (optional, for explicit cleanup).
    pub fn abort_transaction(&self, tx_id: TxId) -> Result<(), WalError> {
        let entry = WalEntry {
            entry_type: WalEntryType::TransactionAbort,
            version: 1,
            tx_id,
            payload: vec![],
        };
        self.write_entry(&entry)
    }

    /// Write a complete transaction atomically
    ///
    /// Writes all entries followed by commit marker.
    pub fn write_transaction(&self, entries: Vec<(WalEntryType, Vec<u8>)>) -> Result<TxId, WalError> {
        let tx_id = self.begin_transaction();

        for (entry_type, payload) in entries {
            self.write_tx_entry(tx_id, entry_type, payload)?;
        }

        self.commit_transaction(tx_id)?;

        Ok(tx_id)
    }
}
```

### Acceptance Criteria

- [ ] begin_transaction() creates unique TxId
- [ ] write_tx_entry() includes TxId in entry
- [ ] commit_transaction() writes commit marker
- [ ] abort_transaction() writes abort marker
- [ ] write_transaction() handles complete transaction
- [ ] Sync on commit in Strict mode

---

## Story #307: WAL Entry Type Registry

**File**: `crates/durability/src/wal_types.rs`

**Deliverable**: Extensible entry type registry

### Implementation

```rust
/// WAL entry types
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
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
    /// Get the primitive this entry type belongs to
    pub fn primitive_kind(&self) -> Option<PrimitiveKind> {
        match self {
            WalEntryType::KvPut | WalEntryType::KvDelete => Some(PrimitiveKind::Kv),
            WalEntryType::JsonCreate | WalEntryType::JsonSet |
            WalEntryType::JsonDelete | WalEntryType::JsonPatch => Some(PrimitiveKind::Json),
            WalEntryType::EventAppend => Some(PrimitiveKind::Event),
            WalEntryType::StateInit | WalEntryType::StateSet |
            WalEntryType::StateTransition => Some(PrimitiveKind::State),
            WalEntryType::TraceRecord => Some(PrimitiveKind::Trace),
            WalEntryType::RunCreate | WalEntryType::RunUpdate |
            WalEntryType::RunEnd | WalEntryType::RunBegin => Some(PrimitiveKind::Run),
            _ => None,  // Core entries
        }
    }
}
```

### Acceptance Criteria

- [ ] All existing entry types defined
- [ ] Clear range allocation documented
- [ ] 0x70-0x7F reserved for Vector (M8)
- [ ] TryFrom<u8> for parsing
- [ ] primitive_kind() returns associated primitive

---

## Story #308: WAL Truncation After Snapshot

**File**: `crates/durability/src/wal.rs`

**Deliverable**: WAL truncation after successful snapshot

### Implementation

```rust
impl WalManager {
    /// Truncate WAL to remove entries before offset
    ///
    /// This should only be called after a successful snapshot.
    pub fn truncate_to(&self, offset: u64) -> Result<(), WalError> {
        // Safety buffer: keep a few entries before offset
        let safe_offset = offset.saturating_sub(1024);

        let wal_path = &self.wal_path;
        let temp_path = wal_path.with_extension("tmp");

        // Read current WAL
        let mut reader = self.open_reader()?;

        // Create new WAL with only entries after offset
        let mut temp_file = std::fs::File::create(&temp_path)?;

        reader.seek_to(safe_offset)?;
        while let Some(entry) = reader.next_entry_raw()? {
            temp_file.write_all(&entry)?;
        }

        // Sync temp file
        temp_file.sync_all()?;

        // Atomic rename
        std::fs::rename(&temp_path, wal_path)?;

        // Update base offset
        self.base_offset.store(safe_offset, std::sync::atomic::Ordering::Release);

        tracing::info!(
            "WAL truncated: removed entries before offset {}",
            safe_offset
        );

        Ok(())
    }

    /// Get current WAL size in bytes
    pub fn size(&self) -> Result<u64, WalError> {
        let metadata = std::fs::metadata(&self.wal_path)?;
        Ok(metadata.len())
    }

    /// Get current WAL offset (for snapshot)
    pub fn current_offset(&self) -> u64 {
        self.writer.position()
    }
}
```

### Acceptance Criteria

- [ ] Truncation removes entries before offset
- [ ] Safety buffer keeps some entries
- [ ] Atomic temp + rename pattern
- [ ] Updates base offset tracking
- [ ] size() returns current WAL size

---

## Story #309: WAL Corruption Detection

**File**: `crates/durability/src/wal.rs`

**Deliverable**: Robust corruption detection in WAL reader

### Implementation

```rust
impl WalReader {
    /// Read next entry with corruption detection
    pub fn next_entry(&mut self) -> Result<Option<WalEntry>, WalError> {
        loop {
            match self.try_read_entry() {
                Ok(Some(entry)) => return Ok(Some(entry)),
                Ok(None) => return Ok(None),  // EOF
                Err(WalError::ChecksumMismatch { .. }) => {
                    // Try to resync
                    if self.try_resync()? {
                        continue;  // Retry after resync
                    } else {
                        return Err(WalError::UnrecoverableCorruption);
                    }
                }
                Err(e) => return Err(e),
            }
        }
    }

    fn try_read_entry(&mut self) -> Result<Option<WalEntry>, WalError> {
        // Read length
        let mut len_bytes = [0u8; 4];
        match self.file.read_exact(&mut len_bytes) {
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
                return Ok(None);
            }
            Err(e) => return Err(WalError::Io(e)),
        }

        let len = u32::from_le_bytes(len_bytes) as usize;

        // Sanity check length
        if len > MAX_WAL_ENTRY_SIZE {
            return Err(WalError::EntryTooLarge(len));
        }

        // Read entry
        let mut data = vec![0u8; 4 + len];
        data[0..4].copy_from_slice(&len_bytes);
        self.file.read_exact(&mut data[4..])?;

        // Parse and validate
        WalEntry::deserialize(&data).map(Some)
    }

    /// Try to resync after corruption
    ///
    /// Scans forward looking for valid entry length prefix.
    fn try_resync(&mut self) -> Result<bool, WalError> {
        const RESYNC_WINDOW: usize = 4096;
        let mut buf = [0u8; RESYNC_WINDOW];

        let bytes_read = self.file.read(&mut buf)?;
        if bytes_read == 0 {
            return Ok(false);  // EOF
        }

        // Look for plausible length prefix
        for i in 0..bytes_read.saturating_sub(4) {
            let potential_len = u32::from_le_bytes([buf[i], buf[i + 1], buf[i + 2], buf[i + 3]]) as usize;

            // Sanity check
            if potential_len > 18 && potential_len < MAX_WAL_ENTRY_SIZE {
                // Try to seek here and read
                let new_pos = self.file.stream_position()? - (bytes_read - i) as u64;
                self.file.seek(std::io::SeekFrom::Start(new_pos))?;
                return Ok(true);
            }
        }

        Ok(false)
    }
}

/// WAL errors
#[derive(Debug, thiserror::Error)]
pub enum WalError {
    #[error("Checksum mismatch: expected {expected}, got {actual}")]
    ChecksumMismatch { expected: u32, actual: u32 },

    #[error("Entry too short")]
    TooShort,

    #[error("Entry too large: {0} bytes")]
    EntryTooLarge(usize),

    #[error("Unknown entry type: {0}")]
    UnknownEntryType(u8),

    #[error("Unrecoverable corruption")]
    UnrecoverableCorruption,

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

const MAX_WAL_ENTRY_SIZE: usize = 16 * 1024 * 1024;  // 16 MB
```

### Acceptance Criteria

- [ ] Detects checksum mismatches
- [ ] Attempts resync after corruption
- [ ] MAX_WAL_ENTRY_SIZE prevents bad length reads
- [ ] UnrecoverableCorruption if can't resync
- [ ] Clear error types for different failures

---

## Testing

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_entry_roundtrip() {
        let entry = WalEntry {
            entry_type: WalEntryType::KvPut,
            version: 1,
            tx_id: TxId::new(),
            payload: b"test payload".to_vec(),
        };

        let serialized = entry.serialize();
        let deserialized = WalEntry::deserialize(&serialized).unwrap();

        assert_eq!(deserialized.entry_type, entry.entry_type);
        assert_eq!(deserialized.tx_id, entry.tx_id);
        assert_eq!(deserialized.payload, entry.payload);
    }

    #[test]
    fn test_checksum_detects_corruption() {
        let entry = WalEntry {
            entry_type: WalEntryType::KvPut,
            version: 1,
            tx_id: TxId::new(),
            payload: b"test".to_vec(),
        };

        let mut serialized = entry.serialize();
        serialized[10] ^= 0xFF;  // Corrupt a byte

        let result = WalEntry::deserialize(&serialized);
        assert!(matches!(result, Err(WalError::ChecksumMismatch { .. })));
    }

    #[test]
    fn test_transaction_framing() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let wal_path = temp_dir.path().join("test.wal");

        let writer = WalWriter::new(&wal_path, DurabilityMode::Buffered).unwrap();

        // Write transaction
        let tx_id = writer.write_transaction(vec![
            (WalEntryType::KvPut, b"key1=value1".to_vec()),
            (WalEntryType::KvPut, b"key2=value2".to_vec()),
        ]).unwrap();

        // Read back
        let mut reader = WalReader::open(&wal_path).unwrap();
        let mut entries = Vec::new();
        while let Some(entry) = reader.next_entry().unwrap() {
            entries.push(entry);
        }

        assert_eq!(entries.len(), 3);  // 2 puts + 1 commit
        assert_eq!(entries[0].tx_id, tx_id);
        assert_eq!(entries[1].tx_id, tx_id);
        assert_eq!(entries[2].entry_type, WalEntryType::TransactionCommit);
    }

    #[test]
    fn test_wal_truncation() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let wal_path = temp_dir.path().join("test.wal");

        let writer = WalWriter::new(&wal_path, DurabilityMode::Buffered).unwrap();

        // Write many entries
        for i in 0..100 {
            writer.write_transaction(vec![
                (WalEntryType::KvPut, format!("key{}=value{}", i, i).into_bytes()),
            ]).unwrap();
        }

        let original_size = std::fs::metadata(&wal_path).unwrap().len();

        // Truncate
        let manager = WalManager::new(&wal_path).unwrap();
        let offset = original_size / 2;
        manager.truncate_to(offset).unwrap();

        let new_size = std::fs::metadata(&wal_path).unwrap().len();
        assert!(new_size < original_size);
    }
}
```

---

## Files Modified/Created

| File | Action |
|------|--------|
| `crates/durability/src/wal_types.rs` | MODIFY - Add TxId, entry envelope |
| `crates/durability/src/wal.rs` | MODIFY - Add transaction framing, truncation |
| `crates/durability/Cargo.toml` | MODIFY - Add crc32fast |
