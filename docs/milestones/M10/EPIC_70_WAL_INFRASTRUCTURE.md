# Epic 70: WAL Infrastructure

**Goal**: Implement append-only, segmented WAL with durability modes

**Dependencies**: M9 complete

---

## Scope

- WAL segment file format (`wal-NNNNNN.seg`)
- WAL record structure with checksums
- Append with durability modes (InMemory, Buffered, Strict)
- Segment rotation when size exceeds limit
- Writeset serialization
- Codec seam integration

---

## User Stories

| Story | Description | Priority |
|-------|-------------|----------|
| #498 | WAL Segment File Format | FOUNDATION |
| #499 | WAL Record Structure and Serialization | FOUNDATION |
| #500 | WAL Append with Durability Modes | CRITICAL |
| #501 | WAL Segment Rotation | CRITICAL |
| #502 | Writeset Serialization | CRITICAL |
| #503 | WAL Configuration (Segment Size, etc.) | HIGH |
| #504 | Codec Seam Integration | HIGH |

---

## Story #498: WAL Segment File Format

**File**: `crates/storage/src/format/wal_record.rs` (NEW)

**Deliverable**: WAL segment file format specification and implementation

### Design

WAL segments are named `wal-NNNNNN.seg` where `NNNNNN` is a zero-padded segment number.

```
WAL Segment File Layout:
┌────────────────────────────────────┐
│ Segment Header (32 bytes)          │
├────────────────────────────────────┤
│ Record 1                           │
├────────────────────────────────────┤
│ Record 2                           │
├────────────────────────────────────┤
│ ...                                │
├────────────────────────────────────┤
│ Record N                           │
└────────────────────────────────────┘
```

**Segment Header**:
```rust
/// WAL segment header (32 bytes)
#[repr(C)]
pub struct SegmentHeader {
    /// Magic bytes: "STRA" (0x53545241)
    pub magic: [u8; 4],

    /// Format version for forward compatibility
    pub format_version: u32,

    /// Segment number
    pub segment_number: u64,

    /// Database UUID (for integrity checking)
    pub database_uuid: [u8; 16],
}
```

### Implementation

```rust
use std::fs::{File, OpenOptions};
use std::io::{Read, Write, Seek, SeekFrom};
use std::path::{Path, PathBuf};

pub const SEGMENT_MAGIC: [u8; 4] = *b"STRA";
pub const SEGMENT_FORMAT_VERSION: u32 = 1;
pub const SEGMENT_HEADER_SIZE: usize = 32;

/// WAL segment file handle
pub struct WalSegment {
    /// File handle
    file: File,

    /// Segment number
    segment_number: u64,

    /// Current write position (bytes from start)
    write_position: u64,

    /// Path to segment file
    path: PathBuf,

    /// Whether this segment is closed (immutable)
    closed: bool,
}

impl WalSegment {
    /// Create a new WAL segment
    pub fn create(
        dir: &Path,
        segment_number: u64,
        database_uuid: [u8; 16],
    ) -> std::io::Result<Self> {
        let path = Self::segment_path(dir, segment_number);

        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .read(true)
            .open(&path)?;

        // Write header
        let header = SegmentHeader {
            magic: SEGMENT_MAGIC,
            format_version: SEGMENT_FORMAT_VERSION,
            segment_number,
            database_uuid,
        };
        file.write_all(&header.to_bytes())?;

        Ok(WalSegment {
            file,
            segment_number,
            write_position: SEGMENT_HEADER_SIZE as u64,
            path,
            closed: false,
        })
    }

    /// Open an existing WAL segment for reading
    pub fn open_read(dir: &Path, segment_number: u64) -> std::io::Result<Self> {
        let path = Self::segment_path(dir, segment_number);

        let mut file = OpenOptions::new()
            .read(true)
            .open(&path)?;

        // Validate header
        let mut header_bytes = [0u8; SEGMENT_HEADER_SIZE];
        file.read_exact(&mut header_bytes)?;

        let header = SegmentHeader::from_bytes(&header_bytes)
            .ok_or_else(|| std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Invalid segment header",
            ))?;

        if header.magic != SEGMENT_MAGIC {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Invalid segment magic bytes",
            ));
        }

        let write_position = file.seek(SeekFrom::End(0))?;

        Ok(WalSegment {
            file,
            segment_number: header.segment_number,
            write_position,
            path,
            closed: true, // Opened for reading = treat as closed
        })
    }

    /// Generate segment file path
    pub fn segment_path(dir: &Path, segment_number: u64) -> PathBuf {
        dir.join(format!("wal-{:06}.seg", segment_number))
    }

    /// Get current segment size in bytes
    pub fn size(&self) -> u64 {
        self.write_position
    }

    /// Mark segment as closed (immutable)
    pub fn close(&mut self) -> std::io::Result<()> {
        if !self.closed {
            self.file.sync_all()?;
            self.closed = true;
        }
        Ok(())
    }

    /// Check if segment is closed
    pub fn is_closed(&self) -> bool {
        self.closed
    }
}

impl SegmentHeader {
    pub fn to_bytes(&self) -> [u8; SEGMENT_HEADER_SIZE] {
        let mut bytes = [0u8; SEGMENT_HEADER_SIZE];
        bytes[0..4].copy_from_slice(&self.magic);
        bytes[4..8].copy_from_slice(&self.format_version.to_le_bytes());
        bytes[8..16].copy_from_slice(&self.segment_number.to_le_bytes());
        bytes[16..32].copy_from_slice(&self.database_uuid);
        bytes
    }

    pub fn from_bytes(bytes: &[u8; SEGMENT_HEADER_SIZE]) -> Option<Self> {
        Some(SegmentHeader {
            magic: bytes[0..4].try_into().ok()?,
            format_version: u32::from_le_bytes(bytes[4..8].try_into().ok()?),
            segment_number: u64::from_le_bytes(bytes[8..16].try_into().ok()?),
            database_uuid: bytes[16..32].try_into().ok()?,
        })
    }
}
```

### Acceptance Criteria

- [ ] Segment file naming: `wal-NNNNNN.seg` (zero-padded)
- [ ] 32-byte header with magic, format_version, segment_number, database_uuid
- [ ] Magic bytes: `STRA` (0x53545241)
- [ ] `create()` initializes new segment with header
- [ ] `open_read()` validates header and magic bytes
- [ ] `close()` marks segment as immutable and fsyncs
- [ ] `size()` returns current byte count

---

## Story #499: WAL Record Structure and Serialization

**File**: `crates/storage/src/format/wal_record.rs`

**Deliverable**: Self-delimiting WAL record format with checksums

### Design

Each WAL record is self-delimiting with length prefix and checksum:

```
WAL Record Layout:
┌─────────────────┬──────────────────┬─────────────────────────┬──────────┐
│ Length (4 bytes)│ Format Ver (1)   │ Payload (variable)      │ CRC32 (4)│
└─────────────────┴──────────────────┴─────────────────────────┴──────────┘

Payload:
┌──────────────┬──────────────┬──────────────┬─────────────────────────────┐
│ TxnId (8)    │ RunId (16)   │ Timestamp (8)│ Writeset (variable)         │
└──────────────┴──────────────┴──────────────┴─────────────────────────────┘
```

### Implementation

```rust
use crc32fast::Hasher;

pub const WAL_RECORD_FORMAT_VERSION: u8 = 1;

/// WAL record for a committed transaction
#[derive(Debug, Clone)]
pub struct WalRecord {
    /// Transaction ID (assigned by engine)
    pub txn_id: u64,

    /// Run this transaction belongs to
    pub run_id: [u8; 16],

    /// Commit timestamp (microseconds since epoch)
    pub timestamp: u64,

    /// Serialized writeset
    pub writeset: Vec<u8>,
}

impl WalRecord {
    /// Serialize record to bytes (for writing to WAL)
    ///
    /// Format: length (4) + format_version (1) + payload + crc32 (4)
    pub fn to_bytes(&self) -> Vec<u8> {
        // Build payload
        let mut payload = Vec::with_capacity(33 + self.writeset.len());
        payload.push(WAL_RECORD_FORMAT_VERSION);
        payload.extend_from_slice(&self.txn_id.to_le_bytes());
        payload.extend_from_slice(&self.run_id);
        payload.extend_from_slice(&self.timestamp.to_le_bytes());
        payload.extend_from_slice(&self.writeset);

        // Calculate CRC32 of payload
        let crc = Self::compute_crc(&payload);

        // Build final record: length + payload + crc
        let total_len = payload.len() + 4; // payload + crc
        let mut record = Vec::with_capacity(4 + total_len);
        record.extend_from_slice(&(total_len as u32).to_le_bytes());
        record.extend_from_slice(&payload);
        record.extend_from_slice(&crc.to_le_bytes());

        record
    }

    /// Deserialize record from bytes
    ///
    /// Returns (record, bytes_consumed) or error
    pub fn from_bytes(bytes: &[u8]) -> Result<(Self, usize), WalRecordError> {
        if bytes.len() < 4 {
            return Err(WalRecordError::InsufficientData);
        }

        // Read length
        let length = u32::from_le_bytes(bytes[0..4].try_into().unwrap()) as usize;

        if bytes.len() < 4 + length {
            return Err(WalRecordError::InsufficientData);
        }

        let payload_with_crc = &bytes[4..4 + length];

        if length < 5 {
            return Err(WalRecordError::InvalidFormat);
        }

        // Split payload and CRC
        let payload = &payload_with_crc[..length - 4];
        let stored_crc = u32::from_le_bytes(
            payload_with_crc[length - 4..].try_into().unwrap()
        );

        // Verify CRC
        let computed_crc = Self::compute_crc(payload);
        if computed_crc != stored_crc {
            return Err(WalRecordError::ChecksumMismatch {
                expected: stored_crc,
                computed: computed_crc,
            });
        }

        // Parse payload
        if payload.len() < 33 {
            return Err(WalRecordError::InvalidFormat);
        }

        let format_version = payload[0];
        if format_version != WAL_RECORD_FORMAT_VERSION {
            return Err(WalRecordError::UnsupportedVersion(format_version));
        }

        let txn_id = u64::from_le_bytes(payload[1..9].try_into().unwrap());
        let run_id: [u8; 16] = payload[9..25].try_into().unwrap();
        let timestamp = u64::from_le_bytes(payload[25..33].try_into().unwrap());
        let writeset = payload[33..].to_vec();

        let record = WalRecord {
            txn_id,
            run_id,
            timestamp,
            writeset,
        };

        Ok((record, 4 + length))
    }

    /// Compute CRC32 checksum
    fn compute_crc(data: &[u8]) -> u32 {
        let mut hasher = Hasher::new();
        hasher.update(data);
        hasher.finalize()
    }
}

/// WAL record parsing errors
#[derive(Debug, thiserror::Error)]
pub enum WalRecordError {
    #[error("Insufficient data to parse record")]
    InsufficientData,

    #[error("Invalid record format")]
    InvalidFormat,

    #[error("Checksum mismatch: expected {expected:08x}, computed {computed:08x}")]
    ChecksumMismatch { expected: u32, computed: u32 },

    #[error("Unsupported format version: {0}")]
    UnsupportedVersion(u8),
}
```

### Acceptance Criteria

- [ ] WAL record with txn_id, run_id, timestamp, writeset
- [ ] Self-delimiting: length prefix allows independent parsing
- [ ] CRC32 checksum for integrity verification
- [ ] `to_bytes()` serializes to wire format
- [ ] `from_bytes()` deserializes and verifies checksum
- [ ] Returns bytes consumed for streaming reads
- [ ] Error on checksum mismatch
- [ ] Error on insufficient data (partial record)

---

## Story #500: WAL Append with Durability Modes

**File**: `crates/storage/src/wal/writer.rs` (NEW)

**Deliverable**: WAL append respecting durability modes

### Implementation

```rust
use std::io::Write;

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
    /// Current active segment
    segment: WalSegment,

    /// Durability mode
    durability: DurabilityMode,

    /// WAL directory
    wal_dir: PathBuf,

    /// Database UUID
    database_uuid: [u8; 16],

    /// Configuration
    config: WalConfig,

    /// Bytes written since last fsync (for Buffered mode)
    bytes_since_sync: u64,
}

impl WalWriter {
    /// Create a new WAL writer
    pub fn new(
        wal_dir: PathBuf,
        database_uuid: [u8; 16],
        durability: DurabilityMode,
        config: WalConfig,
    ) -> std::io::Result<Self> {
        std::fs::create_dir_all(&wal_dir)?;

        // Find or create active segment
        let segment_number = Self::find_latest_segment(&wal_dir)
            .map(|n| n + 1)
            .unwrap_or(1);

        let segment = WalSegment::create(&wal_dir, segment_number, database_uuid)?;

        Ok(WalWriter {
            segment,
            durability,
            wal_dir,
            database_uuid,
            config,
            bytes_since_sync: 0,
        })
    }

    /// Append a record to the WAL
    ///
    /// Respects durability mode:
    /// - InMemory: no-op (returns immediately)
    /// - Buffered: write, fsync periodically
    /// - Strict: write + fsync
    pub fn append(&mut self, record: &WalRecord) -> std::io::Result<()> {
        match self.durability {
            DurabilityMode::InMemory => {
                // No persistence in InMemory mode
                return Ok(());
            }
            DurabilityMode::Buffered | DurabilityMode::Strict => {
                self.write_record(record)?;
            }
        }

        // Handle sync based on durability mode
        match self.durability {
            DurabilityMode::Strict => {
                self.segment.file.sync_all()?;
                self.bytes_since_sync = 0;
            }
            DurabilityMode::Buffered => {
                // Sync if we've written enough
                if self.bytes_since_sync >= self.config.buffered_sync_bytes {
                    self.segment.file.sync_all()?;
                    self.bytes_since_sync = 0;
                }
            }
            DurabilityMode::InMemory => unreachable!(),
        }

        Ok(())
    }

    /// Write record to current segment
    fn write_record(&mut self, record: &WalRecord) -> std::io::Result<()> {
        let bytes = record.to_bytes();

        // Check if we need to rotate
        if self.segment.size() + bytes.len() as u64 > self.config.segment_size {
            self.rotate_segment()?;
        }

        self.segment.file.write_all(&bytes)?;
        self.segment.write_position += bytes.len() as u64;
        self.bytes_since_sync += bytes.len() as u64;

        Ok(())
    }

    /// Rotate to a new segment
    fn rotate_segment(&mut self) -> std::io::Result<()> {
        // Close current segment
        self.segment.close()?;

        // Create new segment
        let new_segment_number = self.segment.segment_number + 1;
        self.segment = WalSegment::create(
            &self.wal_dir,
            new_segment_number,
            self.database_uuid,
        )?;

        Ok(())
    }

    /// Find the latest segment number in the WAL directory
    fn find_latest_segment(dir: &Path) -> Option<u64> {
        std::fs::read_dir(dir)
            .ok()?
            .filter_map(|e| e.ok())
            .filter_map(|e| {
                let name = e.file_name().to_string_lossy().to_string();
                if name.starts_with("wal-") && name.ends_with(".seg") {
                    let num_str = &name[4..10];
                    num_str.parse::<u64>().ok()
                } else {
                    None
                }
            })
            .max()
    }

    /// Flush any buffered data
    pub fn flush(&mut self) -> std::io::Result<()> {
        if self.durability != DurabilityMode::InMemory {
            self.segment.file.sync_all()?;
            self.bytes_since_sync = 0;
        }
        Ok(())
    }

    /// Get current segment number
    pub fn current_segment(&self) -> u64 {
        self.segment.segment_number
    }
}
```

### Acceptance Criteria

- [ ] `append()` respects DurabilityMode
- [ ] InMemory: no-op, immediate return
- [ ] Buffered: write, periodic fsync based on config
- [ ] Strict: write + fsync before returning
- [ ] Automatic segment rotation when size exceeded
- [ ] `flush()` forces fsync for Buffered mode
- [ ] Error handling for I/O failures

---

## Story #501: WAL Segment Rotation

**File**: `crates/storage/src/wal/writer.rs`

**Deliverable**: Segment rotation when size exceeds configured limit

### Implementation

(Included in Story #403)

### Acceptance Criteria

- [ ] Rotation when segment size exceeds `config.segment_size`
- [ ] Closed segments are immutable (never modified)
- [ ] New segment gets incremented segment number
- [ ] Closed segment is fsynced before opening new one
- [ ] Segment boundary is not transaction boundary (records can span)

---

## Story #502: Writeset Serialization

**File**: `crates/storage/src/format/writeset.rs` (NEW)

**Deliverable**: Serialization format for transaction writesets

### Implementation

```rust
use crate::contract::EntityRef;

/// A mutation within a transaction writeset
#[derive(Debug, Clone)]
pub enum Mutation {
    /// Put a value (create or update)
    Put {
        entity_ref: EntityRef,
        value: Vec<u8>,
        version: u64,
    },

    /// Delete an entity
    Delete {
        entity_ref: EntityRef,
    },

    /// Append to a log-like entity (Event)
    Append {
        entity_ref: EntityRef,
        value: Vec<u8>,
        version: u64,
    },
}

/// Transaction writeset
#[derive(Debug, Clone, Default)]
pub struct Writeset {
    pub mutations: Vec<Mutation>,
}

impl Writeset {
    pub fn new() -> Self {
        Writeset { mutations: Vec::new() }
    }

    pub fn put(&mut self, entity_ref: EntityRef, value: Vec<u8>, version: u64) {
        self.mutations.push(Mutation::Put { entity_ref, value, version });
    }

    pub fn delete(&mut self, entity_ref: EntityRef) {
        self.mutations.push(Mutation::Delete { entity_ref });
    }

    pub fn append(&mut self, entity_ref: EntityRef, value: Vec<u8>, version: u64) {
        self.mutations.push(Mutation::Append { entity_ref, value, version });
    }

    /// Serialize writeset to bytes
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::new();

        // Number of mutations
        bytes.extend_from_slice(&(self.mutations.len() as u32).to_le_bytes());

        for mutation in &self.mutations {
            match mutation {
                Mutation::Put { entity_ref, value, version } => {
                    bytes.push(0x01); // Put tag
                    bytes.extend_from_slice(&entity_ref_to_bytes(entity_ref));
                    bytes.extend_from_slice(&version.to_le_bytes());
                    bytes.extend_from_slice(&(value.len() as u32).to_le_bytes());
                    bytes.extend_from_slice(value);
                }
                Mutation::Delete { entity_ref } => {
                    bytes.push(0x02); // Delete tag
                    bytes.extend_from_slice(&entity_ref_to_bytes(entity_ref));
                }
                Mutation::Append { entity_ref, value, version } => {
                    bytes.push(0x03); // Append tag
                    bytes.extend_from_slice(&entity_ref_to_bytes(entity_ref));
                    bytes.extend_from_slice(&version.to_le_bytes());
                    bytes.extend_from_slice(&(value.len() as u32).to_le_bytes());
                    bytes.extend_from_slice(value);
                }
            }
        }

        bytes
    }

    /// Deserialize writeset from bytes
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, WritesetError> {
        let mut cursor = 0;

        if bytes.len() < 4 {
            return Err(WritesetError::InsufficientData);
        }

        let count = u32::from_le_bytes(bytes[0..4].try_into().unwrap()) as usize;
        cursor += 4;

        let mut mutations = Vec::with_capacity(count);

        for _ in 0..count {
            if cursor >= bytes.len() {
                return Err(WritesetError::InsufficientData);
            }

            let tag = bytes[cursor];
            cursor += 1;

            match tag {
                0x01 => {
                    // Put
                    let (entity_ref, consumed) = entity_ref_from_bytes(&bytes[cursor..])?;
                    cursor += consumed;

                    let version = u64::from_le_bytes(
                        bytes[cursor..cursor + 8].try_into()
                            .map_err(|_| WritesetError::InsufficientData)?
                    );
                    cursor += 8;

                    let value_len = u32::from_le_bytes(
                        bytes[cursor..cursor + 4].try_into()
                            .map_err(|_| WritesetError::InsufficientData)?
                    ) as usize;
                    cursor += 4;

                    let value = bytes[cursor..cursor + value_len].to_vec();
                    cursor += value_len;

                    mutations.push(Mutation::Put { entity_ref, value, version });
                }
                0x02 => {
                    // Delete
                    let (entity_ref, consumed) = entity_ref_from_bytes(&bytes[cursor..])?;
                    cursor += consumed;

                    mutations.push(Mutation::Delete { entity_ref });
                }
                0x03 => {
                    // Append
                    let (entity_ref, consumed) = entity_ref_from_bytes(&bytes[cursor..])?;
                    cursor += consumed;

                    let version = u64::from_le_bytes(
                        bytes[cursor..cursor + 8].try_into()
                            .map_err(|_| WritesetError::InsufficientData)?
                    );
                    cursor += 8;

                    let value_len = u32::from_le_bytes(
                        bytes[cursor..cursor + 4].try_into()
                            .map_err(|_| WritesetError::InsufficientData)?
                    ) as usize;
                    cursor += 4;

                    let value = bytes[cursor..cursor + value_len].to_vec();
                    cursor += value_len;

                    mutations.push(Mutation::Append { entity_ref, value, version });
                }
                _ => return Err(WritesetError::InvalidTag(tag)),
            }
        }

        Ok(Writeset { mutations })
    }
}

#[derive(Debug, thiserror::Error)]
pub enum WritesetError {
    #[error("Insufficient data")]
    InsufficientData,

    #[error("Invalid mutation tag: {0}")]
    InvalidTag(u8),

    #[error("Invalid entity ref")]
    InvalidEntityRef,
}

// Helper functions for EntityRef serialization
fn entity_ref_to_bytes(entity_ref: &EntityRef) -> Vec<u8> {
    // Implementation depends on EntityRef structure from M9
    todo!("Implement based on M9 EntityRef")
}

fn entity_ref_from_bytes(bytes: &[u8]) -> Result<(EntityRef, usize), WritesetError> {
    // Implementation depends on EntityRef structure from M9
    todo!("Implement based on M9 EntityRef")
}
```

### Acceptance Criteria

- [ ] Mutation enum with Put, Delete, Append variants
- [ ] Writeset struct containing mutations
- [ ] `to_bytes()` serializes deterministically
- [ ] `from_bytes()` deserializes and validates
- [ ] EntityRef serialization integrated
- [ ] Version included in Put/Append (assigned by engine)

---

## Story #503: WAL Configuration

**File**: `crates/storage/src/wal/config.rs` (NEW)

**Deliverable**: WAL configuration with sensible defaults

### Implementation

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
            segment_size: 64 * 1024 * 1024,        // 64MB
            buffered_sync_bytes: 4 * 1024 * 1024,  // 4MB
        }
    }
}

impl WalConfig {
    /// Create with custom segment size
    pub fn with_segment_size(mut self, size: u64) -> Self {
        self.segment_size = size;
        self
    }

    /// Create with custom buffered sync threshold
    pub fn with_buffered_sync_bytes(mut self, bytes: u64) -> Self {
        self.buffered_sync_bytes = bytes;
        self
    }

    /// Validate configuration
    pub fn validate(&self) -> Result<(), ConfigError> {
        if self.segment_size < 1024 {
            return Err(ConfigError::SegmentSizeTooSmall);
        }
        if self.buffered_sync_bytes > self.segment_size {
            return Err(ConfigError::BufferedSyncExceedsSegment);
        }
        Ok(())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("Segment size must be at least 1KB")]
    SegmentSizeTooSmall,

    #[error("Buffered sync threshold cannot exceed segment size")]
    BufferedSyncExceedsSegment,
}
```

### Acceptance Criteria

- [ ] `segment_size` default 64MB
- [ ] `buffered_sync_bytes` default 4MB
- [ ] Builder pattern for configuration
- [ ] `validate()` checks constraints
- [ ] Configurable at database open time

---

## Story #504: Codec Seam Integration

**File**: `crates/storage/src/codec/mod.rs` (NEW)

**Deliverable**: Codec abstraction for future encryption-at-rest

### Implementation

```rust
/// Storage codec trait
///
/// All bytes passing through the storage layer go through the codec.
/// M10 uses IdentityCodec (no transformation).
/// Future milestones may implement encryption codecs.
pub trait StorageCodec: Send + Sync {
    /// Encode bytes for storage
    fn encode(&self, data: &[u8]) -> Vec<u8>;

    /// Decode bytes from storage
    fn decode(&self, data: &[u8]) -> Result<Vec<u8>, CodecError>;

    /// Codec identifier (for MANIFEST)
    fn codec_id(&self) -> &str;
}

/// Identity codec (no transformation)
///
/// Used in M10 MVP. Bytes pass through unchanged.
#[derive(Debug, Clone, Default)]
pub struct IdentityCodec;

impl StorageCodec for IdentityCodec {
    fn encode(&self, data: &[u8]) -> Vec<u8> {
        data.to_vec()
    }

    fn decode(&self, data: &[u8]) -> Result<Vec<u8>, CodecError> {
        Ok(data.to_vec())
    }

    fn codec_id(&self) -> &str {
        "identity"
    }
}

#[derive(Debug, thiserror::Error)]
pub enum CodecError {
    #[error("Decode error: {0}")]
    DecodeError(String),

    #[error("Unknown codec: {0}")]
    UnknownCodec(String),
}

/// Get codec by ID
pub fn get_codec(codec_id: &str) -> Result<Box<dyn StorageCodec>, CodecError> {
    match codec_id {
        "identity" => Ok(Box::new(IdentityCodec)),
        _ => Err(CodecError::UnknownCodec(codec_id.to_string())),
    }
}
```

### Acceptance Criteria

- [ ] `StorageCodec` trait with encode/decode
- [ ] `codec_id()` for MANIFEST tracking
- [ ] `IdentityCodec` implementation (pass-through)
- [ ] `get_codec()` factory function
- [ ] All WAL/snapshot bytes pass through codec

---

## Testing

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_wal_record_roundtrip() {
        let record = WalRecord {
            txn_id: 42,
            run_id: [1u8; 16],
            timestamp: 1234567890,
            writeset: vec![1, 2, 3, 4, 5],
        };

        let bytes = record.to_bytes();
        let (parsed, consumed) = WalRecord::from_bytes(&bytes).unwrap();

        assert_eq!(parsed.txn_id, record.txn_id);
        assert_eq!(parsed.run_id, record.run_id);
        assert_eq!(parsed.timestamp, record.timestamp);
        assert_eq!(parsed.writeset, record.writeset);
        assert_eq!(consumed, bytes.len());
    }

    #[test]
    fn test_wal_record_checksum_failure() {
        let record = WalRecord {
            txn_id: 42,
            run_id: [1u8; 16],
            timestamp: 1234567890,
            writeset: vec![1, 2, 3],
        };

        let mut bytes = record.to_bytes();

        // Corrupt a byte
        bytes[10] ^= 0xFF;

        let result = WalRecord::from_bytes(&bytes);
        assert!(matches!(result, Err(WalRecordError::ChecksumMismatch { .. })));
    }

    #[test]
    fn test_segment_header_roundtrip() {
        let header = SegmentHeader {
            magic: SEGMENT_MAGIC,
            format_version: SEGMENT_FORMAT_VERSION,
            segment_number: 12345,
            database_uuid: [0xAB; 16],
        };

        let bytes = header.to_bytes();
        let parsed = SegmentHeader::from_bytes(&bytes).unwrap();

        assert_eq!(parsed.magic, header.magic);
        assert_eq!(parsed.format_version, header.format_version);
        assert_eq!(parsed.segment_number, header.segment_number);
        assert_eq!(parsed.database_uuid, header.database_uuid);
    }

    #[test]
    fn test_durability_modes() {
        let dir = tempfile::tempdir().unwrap();
        let uuid = [0u8; 16];

        // InMemory mode - no files created
        {
            let mut writer = WalWriter::new(
                dir.path().join("wal"),
                uuid,
                DurabilityMode::InMemory,
                WalConfig::default(),
            ).unwrap();

            let record = WalRecord {
                txn_id: 1,
                run_id: uuid,
                timestamp: 0,
                writeset: vec![],
            };

            writer.append(&record).unwrap();
            // In InMemory mode, segment only has header
        }

        // Strict mode - file fsynced after each write
        {
            let mut writer = WalWriter::new(
                dir.path().join("wal_strict"),
                uuid,
                DurabilityMode::Strict,
                WalConfig::default(),
            ).unwrap();

            let record = WalRecord {
                txn_id: 1,
                run_id: uuid,
                timestamp: 0,
                writeset: vec![1, 2, 3],
            };

            writer.append(&record).unwrap();

            // File should exist with content
            let segment_path = WalSegment::segment_path(
                &dir.path().join("wal_strict"),
                1,
            );
            assert!(segment_path.exists());
        }
    }

    #[test]
    fn test_writeset_roundtrip() {
        let mut writeset = Writeset::new();
        writeset.put(
            EntityRef::kv(RunId::from_bytes([1u8; 16]), "key1"),
            vec![1, 2, 3],
            42,
        );
        writeset.delete(
            EntityRef::kv(RunId::from_bytes([1u8; 16]), "key2"),
        );

        let bytes = writeset.to_bytes();
        let parsed = Writeset::from_bytes(&bytes).unwrap();

        assert_eq!(parsed.mutations.len(), 2);
    }

    #[test]
    fn test_identity_codec() {
        let codec = IdentityCodec;
        let data = vec![1, 2, 3, 4, 5];

        let encoded = codec.encode(&data);
        let decoded = codec.decode(&encoded).unwrap();

        assert_eq!(data, encoded);
        assert_eq!(data, decoded);
        assert_eq!(codec.codec_id(), "identity");
    }
}
```

---

## Files Modified/Created

| File | Action |
|------|--------|
| `crates/storage/src/lib.rs` | CREATE - Crate entry point |
| `crates/storage/src/format/mod.rs` | CREATE - Format module |
| `crates/storage/src/format/wal_record.rs` | CREATE - WAL record format |
| `crates/storage/src/format/writeset.rs` | CREATE - Writeset format |
| `crates/storage/src/wal/mod.rs` | CREATE - WAL module |
| `crates/storage/src/wal/segment.rs` | CREATE - WAL segment handling |
| `crates/storage/src/wal/writer.rs` | CREATE - WAL writer |
| `crates/storage/src/wal/config.rs` | CREATE - WAL configuration |
| `crates/storage/src/codec/mod.rs` | CREATE - Codec module |
| `crates/storage/src/codec/identity.rs` | CREATE - Identity codec |
| `crates/storage/src/codec/trait.rs` | CREATE - StorageCodec trait |
| `Cargo.toml` | MODIFY - Add storage crate |
