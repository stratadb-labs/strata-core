# Epic 40: Snapshot Format & Writer

**Goal**: Implement snapshot format and writing with checksums

**Dependencies**: M6 complete

---

## Scope

- Snapshot envelope format with magic bytes, version, checksums
- SnapshotHeader with metadata (timestamp, WAL offset, tx count)
- Per-primitive serialization for snapshots
- SnapshotWriter with atomic write (temp + rename)
- CRC32 checksum for integrity validation

---

## User Stories

| Story | Description | Priority |
|-------|-------------|----------|
| #292 | Snapshot Envelope Format | FOUNDATION |
| #293 | SnapshotHeader Type | FOUNDATION |
| #294 | Per-Primitive Serialization | CRITICAL |
| #295 | SnapshotWriter Implementation | CRITICAL |
| #296 | CRC32 Checksum Integration | CRITICAL |
| #297 | Atomic Snapshot Write | HIGH |

---

## Story #292: Snapshot Envelope Format

**File**: `crates/durability/src/snapshot_types.rs` (NEW)

**Deliverable**: Snapshot envelope format definition

### Implementation

```rust
/// Snapshot file layout:
///
/// +------------------+
/// | Magic (10 bytes) |  "INMEM_SNAP"
/// +------------------+
/// | Version (4)      |  Format version (1 for M7)
/// +------------------+
/// | Timestamp (8)    |  Microseconds since epoch
/// +------------------+
/// | WAL Offset (8)   |  WAL position covered
/// +------------------+
/// | Tx Count (8)     |  Transactions included
/// +------------------+
/// | Primitive Count  |  Number of primitive sections
/// +------------------+
/// | Primitive 1      |  Type (1) + Length (8) + Data
/// +------------------+
/// | ...              |
/// +------------------+
/// | CRC32 (4)        |  Checksum of everything above
/// +------------------+

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
```

### Acceptance Criteria

- [ ] Magic bytes: "INMEM_SNAP" (10 bytes)
- [ ] Version field for future format evolution
- [ ] Timestamp in microseconds
- [ ] WAL offset for replay starting point
- [ ] CRC32 at end covers all preceding data

---

## Story #293: SnapshotHeader Type

**File**: `crates/durability/src/snapshot_types.rs`

**Deliverable**: SnapshotHeader for metadata

### Implementation

```rust
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
    pub fn new(wal_offset: u64, transaction_count: u64) -> Self {
        SnapshotHeader {
            version: SNAPSHOT_VERSION_1,
            timestamp_micros: now_micros(),
            wal_offset,
            transaction_count,
            db_version: env!("CARGO_PKG_VERSION").to_string(),
        }
    }

    /// Serialize header to bytes
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(64);
        buf.extend_from_slice(SNAPSHOT_MAGIC);
        buf.extend_from_slice(&self.version.to_le_bytes());
        buf.extend_from_slice(&self.timestamp_micros.to_le_bytes());
        buf.extend_from_slice(&self.wal_offset.to_le_bytes());
        buf.extend_from_slice(&self.transaction_count.to_le_bytes());
        buf
    }

    /// Parse header from bytes
    pub fn from_bytes(data: &[u8]) -> Result<Self, SnapshotError> {
        if data.len() < 38 {  // Magic(10) + Version(4) + Timestamp(8) + Offset(8) + TxCount(8)
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
            db_version: String::new(),  // Not stored in binary format
        })
    }
}
```

### Acceptance Criteria

- [ ] All metadata fields present
- [ ] Serialization roundtrips correctly
- [ ] Version validation on parse
- [ ] Magic validation on parse

---

## Story #294: Per-Primitive Serialization

**File**: `crates/durability/src/snapshot.rs` (NEW)

**Deliverable**: Serialization methods for each primitive

### Implementation

```rust
use bincode;

/// Trait for primitives to implement snapshot serialization
pub trait SnapshotSerializable {
    /// Serialize primitive state for snapshot
    fn snapshot_serialize(&self) -> Result<Vec<u8>, SnapshotError>;

    /// Deserialize primitive state from snapshot
    fn snapshot_deserialize(&mut self, data: &[u8]) -> Result<(), SnapshotError>;

    /// Primitive type ID (for snapshot sections)
    fn primitive_type_id(&self) -> u8;
}

/// Primitive type IDs
pub mod primitive_ids {
    pub const KV: u8 = 1;
    pub const JSON: u8 = 2;
    pub const EVENT: u8 = 3;
    pub const STATE: u8 = 4;
    pub const TRACE: u8 = 5;
    pub const RUN: u8 = 6;
    // Reserved for Vector (M8): 7
}

impl SnapshotWriter {
    fn serialize_primitive(
        &self,
        snapshot_view: &SnapshotView,
        primitive_type: u8,
    ) -> Result<Vec<u8>, SnapshotError> {
        match primitive_type {
            primitive_ids::KV => self.serialize_kv(snapshot_view),
            primitive_ids::JSON => self.serialize_json(snapshot_view),
            primitive_ids::EVENT => self.serialize_event(snapshot_view),
            primitive_ids::STATE => self.serialize_state(snapshot_view),
            primitive_ids::TRACE => self.serialize_trace(snapshot_view),
            primitive_ids::RUN => self.serialize_run(snapshot_view),
            _ => Err(SnapshotError::UnknownPrimitive(primitive_type)),
        }
    }

    fn serialize_kv(&self, view: &SnapshotView) -> Result<Vec<u8>, SnapshotError> {
        let entries: Vec<(Key, Value)> = view.scan_kv_all()?;
        Ok(bincode::serialize(&entries)?)
    }

    fn serialize_json(&self, view: &SnapshotView) -> Result<Vec<u8>, SnapshotError> {
        let docs: Vec<(Key, JsonDoc)> = view.scan_json_all()?;
        Ok(bincode::serialize(&docs)?)
    }

    // Similar for other primitives...
}
```

### Acceptance Criteria

- [ ] KV serialization includes all key-value pairs
- [ ] JSON serialization includes all documents with versions
- [ ] Event serialization includes all events
- [ ] State serialization includes all state cells
- [ ] Trace serialization includes all spans
- [ ] Run serialization includes all run metadata
- [ ] No derived data (indexes) serialized

---

## Story #295: SnapshotWriter Implementation

**File**: `crates/durability/src/snapshot.rs`

**Deliverable**: SnapshotWriter that creates snapshot files

### Implementation

```rust
use std::fs::File;
use std::io::Write;
use std::path::Path;

pub struct SnapshotWriter {
    /// CRC32 hasher
    hasher: crc32fast::Hasher,
}

impl SnapshotWriter {
    pub fn new() -> Self {
        SnapshotWriter {
            hasher: crc32fast::Hasher::new(),
        }
    }

    /// Write snapshot to file
    ///
    /// Uses atomic write: write to temp file, then rename.
    pub fn write(
        &mut self,
        db: &Database,
        path: &Path,
    ) -> Result<SnapshotInfo, SnapshotError> {
        // Take consistent snapshot view
        let snapshot_view = db.snapshot();
        let wal_offset = db.wal_offset();
        let tx_count = db.transaction_count();

        // Create header
        let header = SnapshotHeader::new(wal_offset, tx_count);

        // Create temp file
        let temp_path = path.with_extension("tmp");
        let mut file = File::create(&temp_path)?;

        // Write header
        let header_bytes = header.to_bytes();
        file.write_all(&header_bytes)?;
        self.hasher.update(&header_bytes);

        // Write primitive count
        let primitive_count: u8 = 6;
        file.write_all(&[primitive_count])?;
        self.hasher.update(&[primitive_count]);

        // Write each primitive
        for primitive_id in 1..=6u8 {
            let data = self.serialize_primitive(&snapshot_view, primitive_id)?;

            // Write type
            file.write_all(&[primitive_id])?;
            self.hasher.update(&[primitive_id]);

            // Write length
            let len_bytes = (data.len() as u64).to_le_bytes();
            file.write_all(&len_bytes)?;
            self.hasher.update(&len_bytes);

            // Write data
            file.write_all(&data)?;
            self.hasher.update(&data);
        }

        // Write CRC32
        let checksum = self.hasher.clone().finalize();
        file.write_all(&checksum.to_le_bytes())?;

        // Sync to disk
        file.sync_all()?;

        // Atomic rename
        std::fs::rename(&temp_path, path)?;

        Ok(SnapshotInfo {
            path: path.to_path_buf(),
            timestamp_micros: header.timestamp_micros,
            wal_offset,
        })
    }
}
```

### Acceptance Criteria

- [ ] Takes consistent snapshot view at transaction boundary
- [ ] Writes header, primitives, checksum in order
- [ ] Uses temp file + rename for atomicity
- [ ] Syncs to disk before rename
- [ ] Returns SnapshotInfo with metadata

---

## Story #296: CRC32 Checksum Integration

**File**: `crates/durability/src/snapshot.rs`

**Deliverable**: CRC32 checksum for snapshot integrity

### Implementation

```rust
use crc32fast::Hasher;

impl SnapshotWriter {
    /// Compute CRC32 of data
    fn compute_checksum(&self, data: &[u8]) -> u32 {
        let mut hasher = Hasher::new();
        hasher.update(data);
        hasher.finalize()
    }
}

impl SnapshotReader {
    /// Validate snapshot checksum
    pub fn validate_checksum(path: &Path) -> Result<(), SnapshotError> {
        let data = std::fs::read(path)?;

        if data.len() < 4 {
            return Err(SnapshotError::TooShort);
        }

        // Split data and checksum
        let (content, checksum_bytes) = data.split_at(data.len() - 4);

        // Parse stored checksum
        let stored = u32::from_le_bytes([
            checksum_bytes[0],
            checksum_bytes[1],
            checksum_bytes[2],
            checksum_bytes[3],
        ]);

        // Compute checksum
        let mut hasher = Hasher::new();
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
}
```

### Acceptance Criteria

- [ ] CRC32 computed over all data before checksum
- [ ] Checksum stored as last 4 bytes
- [ ] Validation compares stored vs computed
- [ ] ChecksumMismatch error on mismatch
- [ ] Uses crc32fast for performance

---

## Story #297: Atomic Snapshot Write

**File**: `crates/durability/src/snapshot.rs`

**Deliverable**: Atomic snapshot write with rollback on failure

### Implementation

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
        db: &Database,
        path: &Path,
    ) -> Result<SnapshotInfo, SnapshotError> {
        let temp_path = path.with_extension("tmp");

        // Clean up temp file if it exists (from previous failed attempt)
        let _ = std::fs::remove_file(&temp_path);

        // Write to temp
        let result = self.write_to_path(db, &temp_path);

        match result {
            Ok(info) => {
                // Atomic rename
                match std::fs::rename(&temp_path, path) {
                    Ok(()) => Ok(SnapshotInfo {
                        path: path.to_path_buf(),
                        ..info
                    }),
                    Err(e) => {
                        // Clean up temp on rename failure
                        let _ = std::fs::remove_file(&temp_path);
                        Err(SnapshotError::Io(e))
                    }
                }
            }
            Err(e) => {
                // Clean up temp on write failure
                let _ = std::fs::remove_file(&temp_path);
                Err(e)
            }
        }
    }
}

/// Snapshot info returned after successful write
#[derive(Debug, Clone)]
pub struct SnapshotInfo {
    /// Path to snapshot file
    pub path: PathBuf,
    /// Timestamp when snapshot was taken
    pub timestamp_micros: u64,
    /// WAL offset covered by this snapshot
    pub wal_offset: u64,
}
```

### Acceptance Criteria

- [ ] Write to temp file first
- [ ] Sync temp file before rename
- [ ] Atomic rename on POSIX systems
- [ ] Clean up temp file on any failure
- [ ] Clean up stale temp files on start

---

## Testing

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_snapshot_roundtrip() {
        let db = test_db();
        populate_test_data(&db);

        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path().join("snapshot.dat");

        // Write snapshot
        let mut writer = SnapshotWriter::new();
        let info = writer.write(&db, &path).unwrap();

        // Validate checksum
        SnapshotReader::validate_checksum(&path).unwrap();

        // Read snapshot
        let reader = SnapshotReader::new();
        let header = reader.read_header(&path).unwrap();

        assert_eq!(header.version, SNAPSHOT_VERSION_1);
        assert_eq!(header.wal_offset, info.wal_offset);
    }

    #[test]
    fn test_corrupt_snapshot_detected() {
        let db = test_db();
        populate_test_data(&db);

        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path().join("snapshot.dat");

        // Write snapshot
        let mut writer = SnapshotWriter::new();
        writer.write(&db, &path).unwrap();

        // Corrupt the file
        let mut data = std::fs::read(&path).unwrap();
        data[50] ^= 0xFF;  // Flip bits
        std::fs::write(&path, &data).unwrap();

        // Validation should fail
        let result = SnapshotReader::validate_checksum(&path);
        assert!(matches!(result, Err(SnapshotError::ChecksumMismatch { .. })));
    }

    #[test]
    fn test_atomic_write_cleanup() {
        let db = test_db();
        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path().join("snapshot.dat");
        let temp_path = path.with_extension("tmp");

        // Create stale temp file
        std::fs::write(&temp_path, b"stale").unwrap();

        // Write should succeed and clean up stale temp
        let mut writer = SnapshotWriter::new();
        writer.write_atomic(&db, &path).unwrap();

        assert!(!temp_path.exists());
        assert!(path.exists());
    }
}
```

---

## Files Modified/Created

| File | Action |
|------|--------|
| `crates/durability/src/snapshot_types.rs` | CREATE - Envelope, header types |
| `crates/durability/src/snapshot.rs` | CREATE - Writer implementation |
| `crates/durability/src/lib.rs` | MODIFY - Export snapshot modules |
| `crates/durability/Cargo.toml` | MODIFY - Add crc32fast dependency |
