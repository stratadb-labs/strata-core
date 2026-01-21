# Epic 71: Snapshot System

**Goal**: Implement point-in-time snapshots with crash-safe creation

**Dependencies**: Epic 70 (WAL Infrastructure)

---

## Scope

- Snapshot file format (`snap-NNNNNN.chk`)
- Snapshot serialization for all 7 primitives
- Crash-safe snapshot creation (write-fsync-rename)
- Checkpoint API
- Snapshot metadata and watermark tracking
- Snapshot loading for recovery

---

## User Stories

| Story | Description | Priority |
|-------|-------------|----------|
| #506 | Snapshot File Format | FOUNDATION |
| #507 | Snapshot Serialization (All Primitives) | CRITICAL |
| #508 | Crash-Safe Snapshot Creation | CRITICAL |
| #509 | Checkpoint API | CRITICAL |
| #510 | Snapshot Metadata and Watermark | HIGH |
| #511 | Snapshot Loading | CRITICAL |

---

## Story #506: Snapshot File Format

**File**: `crates/storage/src/format/snapshot.rs` (NEW)

**Deliverable**: Snapshot file format specification and implementation

### Design

Snapshots are named `snap-NNNNNN.chk` where `NNNNNN` is a zero-padded snapshot ID.

```
Snapshot File Layout:
┌────────────────────────────────────┐
│ Snapshot Header (64 bytes)         │
├────────────────────────────────────┤
│ Primitive Section: KV              │
├────────────────────────────────────┤
│ Primitive Section: Event           │
├────────────────────────────────────┤
│ Primitive Section: State           │
├────────────────────────────────────┤
│ Primitive Section: Trace           │
├────────────────────────────────────┤
│ Primitive Section: Run             │
├────────────────────────────────────┤
│ Primitive Section: Json            │
├────────────────────────────────────┤
│ Primitive Section: Vector          │
├────────────────────────────────────┤
│ Footer (checksum)                  │
└────────────────────────────────────┘
```

### Implementation

```rust
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

/// Primitive section header
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

---

## Story #507: Snapshot Serialization (All Primitives)

**File**: `crates/storage/src/format/primitives.rs` (NEW)

**Deliverable**: Serialization formats for all 7 primitives

### Design

Snapshots contain **logical state**, not memory dumps. Each primitive has a defined serialization format.

> **Critical**: Snapshots are logical, not physical. They persist the materialized state, not internal data structures.

### Implementation

```rust
use crate::format::snapshot::{SectionHeader, primitive_tags};

/// Snapshot serializer for all primitives
pub struct SnapshotSerializer {
    codec: Box<dyn StorageCodec>,
}

impl SnapshotSerializer {
    pub fn new(codec: Box<dyn StorageCodec>) -> Self {
        SnapshotSerializer { codec }
    }

    // === KV Serialization ===

    /// Serialize KV entries for a run
    ///
    /// Format per entry:
    /// - key_len (4 bytes)
    /// - key (variable)
    /// - value_len (4 bytes)
    /// - value (variable)
    /// - version (8 bytes)
    /// - timestamp (8 bytes)
    pub fn serialize_kv_section(
        &self,
        entries: impl Iterator<Item = KvEntry>,
    ) -> Vec<u8> {
        let mut data = Vec::new();
        let mut count = 0u32;

        // Reserve space for count
        data.extend_from_slice(&[0u8; 4]);

        for entry in entries {
            // Key
            let key_bytes = entry.key.as_bytes();
            data.extend_from_slice(&(key_bytes.len() as u32).to_le_bytes());
            data.extend_from_slice(key_bytes);

            // Value
            let value_bytes = self.codec.encode(&entry.value);
            data.extend_from_slice(&(value_bytes.len() as u32).to_le_bytes());
            data.extend_from_slice(&value_bytes);

            // Metadata
            data.extend_from_slice(&entry.version.to_le_bytes());
            data.extend_from_slice(&entry.timestamp.to_le_bytes());

            count += 1;
        }

        // Write count at start
        data[0..4].copy_from_slice(&count.to_le_bytes());

        data
    }

    // === Event Serialization ===

    /// Serialize Event entries for a run
    ///
    /// Format per event:
    /// - sequence (8 bytes)
    /// - payload_len (4 bytes)
    /// - payload (variable)
    /// - timestamp (8 bytes)
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

    // === State Serialization ===

    /// Serialize State cells for a run
    ///
    /// Format per cell:
    /// - name_len (4 bytes)
    /// - name (variable)
    /// - value_len (4 bytes)
    /// - value (variable)
    /// - counter (8 bytes) - version counter
    /// - timestamp (8 bytes)
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

    // === Trace Serialization ===

    /// Serialize Trace entries for a run
    ///
    /// Format per trace:
    /// - trace_id (16 bytes - UUID)
    /// - parent_trace_id (16 bytes - UUID, all zeros if none)
    /// - span_count (4 bytes)
    /// - spans (variable)
    pub fn serialize_trace_section(
        &self,
        traces: impl Iterator<Item = TraceEntry>,
    ) -> Vec<u8> {
        let mut data = Vec::new();
        let mut count = 0u32;

        data.extend_from_slice(&[0u8; 4]);

        for trace in traces {
            data.extend_from_slice(trace.trace_id.as_bytes());
            data.extend_from_slice(
                trace.parent_trace_id
                    .as_ref()
                    .map(|id| id.as_bytes())
                    .unwrap_or(&[0u8; 16])
            );

            data.extend_from_slice(&(trace.spans.len() as u32).to_le_bytes());
            for span in &trace.spans {
                let span_bytes = self.serialize_span(span);
                data.extend_from_slice(&span_bytes);
            }

            count += 1;
        }

        data[0..4].copy_from_slice(&count.to_le_bytes());
        data
    }

    fn serialize_span(&self, span: &Span) -> Vec<u8> {
        // Span serialization implementation
        todo!("Implement based on Span structure")
    }

    // === Run Serialization ===

    /// Serialize Run metadata
    ///
    /// Format per run:
    /// - run_id (16 bytes - UUID)
    /// - name_len (4 bytes)
    /// - name (variable)
    /// - created_at (8 bytes)
    /// - metadata_len (4 bytes)
    /// - metadata (variable - JSON)
    pub fn serialize_run_section(
        &self,
        runs: impl Iterator<Item = RunMetadata>,
    ) -> Vec<u8> {
        let mut data = Vec::new();
        let mut count = 0u32;

        data.extend_from_slice(&[0u8; 4]);

        for run in runs {
            data.extend_from_slice(run.run_id.as_bytes());

            let name_bytes = run.name.as_bytes();
            data.extend_from_slice(&(name_bytes.len() as u32).to_le_bytes());
            data.extend_from_slice(name_bytes);

            data.extend_from_slice(&run.created_at.to_le_bytes());

            let metadata_bytes = serde_json::to_vec(&run.metadata)
                .unwrap_or_else(|_| b"{}".to_vec());
            data.extend_from_slice(&(metadata_bytes.len() as u32).to_le_bytes());
            data.extend_from_slice(&metadata_bytes);

            count += 1;
        }

        data[0..4].copy_from_slice(&count.to_le_bytes());
        data
    }

    // === JSON Serialization ===

    /// Serialize JSON documents for a run
    ///
    /// Format per document:
    /// - doc_id (16 bytes - UUID)
    /// - content_len (4 bytes)
    /// - content (variable - JSON)
    /// - version (8 bytes)
    /// - timestamp (8 bytes)
    pub fn serialize_json_section(
        &self,
        docs: impl Iterator<Item = JsonDocument>,
    ) -> Vec<u8> {
        let mut data = Vec::new();
        let mut count = 0u32;

        data.extend_from_slice(&[0u8; 4]);

        for doc in docs {
            data.extend_from_slice(doc.doc_id.as_bytes());

            let content_bytes = self.codec.encode(
                &serde_json::to_vec(&doc.content).unwrap_or_default()
            );
            data.extend_from_slice(&(content_bytes.len() as u32).to_le_bytes());
            data.extend_from_slice(&content_bytes);

            data.extend_from_slice(&doc.version.to_le_bytes());
            data.extend_from_slice(&doc.timestamp.to_le_bytes());

            count += 1;
        }

        data[0..4].copy_from_slice(&count.to_le_bytes());
        data
    }

    // === Vector Serialization ===

    /// Serialize Vector collections and entries for a run
    ///
    /// Format:
    /// - collection_count (4 bytes)
    /// - For each collection:
    ///   - name_len (4 bytes)
    ///   - name (variable)
    ///   - config (VectorConfig serialized)
    ///   - vector_count (4 bytes)
    ///   - For each vector:
    ///     - vector_id (8 bytes)
    ///     - key_len (4 bytes)
    ///     - key (variable)
    ///     - embedding_len (4 bytes) - number of f32s
    ///     - embedding (variable - f32 array)
    ///     - metadata_len (4 bytes)
    ///     - metadata (variable - JSON, 0 if none)
    ///     - version (8 bytes)
    pub fn serialize_vector_section(
        &self,
        collections: impl Iterator<Item = VectorCollection>,
    ) -> Vec<u8> {
        let mut data = Vec::new();
        let mut collection_count = 0u32;

        data.extend_from_slice(&[0u8; 4]);

        for collection in collections {
            let name_bytes = collection.name.as_bytes();
            data.extend_from_slice(&(name_bytes.len() as u32).to_le_bytes());
            data.extend_from_slice(name_bytes);

            // Serialize config
            let config_bytes = self.serialize_vector_config(&collection.config);
            data.extend_from_slice(&config_bytes);

            // Vector entries
            data.extend_from_slice(&(collection.vectors.len() as u32).to_le_bytes());
            for vector in &collection.vectors {
                data.extend_from_slice(&vector.vector_id.to_le_bytes());

                let key_bytes = vector.key.as_bytes();
                data.extend_from_slice(&(key_bytes.len() as u32).to_le_bytes());
                data.extend_from_slice(key_bytes);

                // Embedding
                data.extend_from_slice(&(vector.embedding.len() as u32).to_le_bytes());
                for &val in &vector.embedding {
                    data.extend_from_slice(&val.to_le_bytes());
                }

                // Metadata
                if let Some(ref meta) = vector.metadata {
                    let meta_bytes = serde_json::to_vec(meta).unwrap_or_default();
                    data.extend_from_slice(&(meta_bytes.len() as u32).to_le_bytes());
                    data.extend_from_slice(&meta_bytes);
                } else {
                    data.extend_from_slice(&0u32.to_le_bytes());
                }

                data.extend_from_slice(&vector.version.to_le_bytes());
            }

            collection_count += 1;
        }

        data[0..4].copy_from_slice(&collection_count.to_le_bytes());
        data
    }

    fn serialize_vector_config(&self, config: &VectorConfig) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(16);
        bytes.extend_from_slice(&(config.dimension as u32).to_le_bytes());
        bytes.push(config.metric.to_byte());
        bytes.push(0); // storage_dtype (F32 = 0)
        bytes.extend_from_slice(&[0u8; 10]); // reserved
        bytes
    }
}

// Placeholder types (actual types come from primitives)
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

pub struct TraceEntry {
    pub trace_id: [u8; 16],
    pub parent_trace_id: Option<[u8; 16]>,
    pub spans: Vec<Span>,
}

pub struct Span {
    // Span fields
}

pub struct RunMetadata {
    pub run_id: [u8; 16],
    pub name: String,
    pub created_at: u64,
    pub metadata: serde_json::Value,
}

pub struct JsonDocument {
    pub doc_id: [u8; 16],
    pub content: serde_json::Value,
    pub version: u64,
    pub timestamp: u64,
}

pub struct VectorCollection {
    pub name: String,
    pub config: VectorConfig,
    pub vectors: Vec<VectorSnapshot>,
}

pub struct VectorSnapshot {
    pub vector_id: u64,
    pub key: String,
    pub embedding: Vec<f32>,
    pub metadata: Option<serde_json::Value>,
    pub version: u64,
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

---

## Story #508: Crash-Safe Snapshot Creation

**File**: `crates/storage/src/snapshot/writer.rs` (NEW)

**Deliverable**: Crash-safe snapshot write using write-fsync-rename pattern

### Implementation

```rust
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

/// Snapshot writer with crash-safe semantics
pub struct SnapshotWriter {
    /// Snapshots directory
    snapshots_dir: PathBuf,

    /// Codec for encoding data
    codec: Box<dyn StorageCodec>,

    /// Database UUID
    database_uuid: [u8; 16],
}

impl SnapshotWriter {
    pub fn new(
        snapshots_dir: PathBuf,
        codec: Box<dyn StorageCodec>,
        database_uuid: [u8; 16],
    ) -> std::io::Result<Self> {
        std::fs::create_dir_all(&snapshots_dir)?;
        Ok(SnapshotWriter {
            snapshots_dir,
            codec,
            database_uuid,
        })
    }

    /// Create a snapshot at the given watermark
    ///
    /// Uses crash-safe write pattern:
    /// 1. Write to temporary file
    /// 2. fsync temporary file
    /// 3. Atomic rename to final path
    ///
    /// If crash occurs at any point:
    /// - Before rename: temp file is ignored/cleaned up
    /// - After rename: snapshot is complete and valid
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

        // Write header
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

        // Write codec ID
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

        // Write footer (CRC32 of everything)
        let file_crc = self.compute_file_crc(&temp_path)?;
        file.write_all(&file_crc.to_le_bytes())?;

        // Step 2: fsync to ensure durability
        file.sync_all()?;
        drop(file);

        // Step 3: Atomic rename
        std::fs::rename(&temp_path, &final_path)?;

        // fsync parent directory to ensure rename is durable
        let dir = File::open(&self.snapshots_dir)?;
        dir.sync_all()?;

        Ok(SnapshotInfo {
            snapshot_id,
            watermark_txn,
            timestamp: created_at,
            path: final_path,
        })
    }

    /// Compute CRC32 of file contents
    fn compute_file_crc(&self, path: &Path) -> std::io::Result<u32> {
        let data = std::fs::read(path)?;
        let mut hasher = crc32fast::Hasher::new();
        hasher.update(&data);
        Ok(hasher.finalize())
    }

    /// Clean up any incomplete temporary files
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

---

## Story #509: Checkpoint API

**File**: `crates/storage/src/snapshot/mod.rs`

**Deliverable**: User-facing checkpoint API

### Implementation

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
    /// This captures a point-in-time view of all committed transactions
    /// up to the current moment. After checkpoint:
    /// - Snapshot file contains all state at watermark
    /// - WAL entries > watermark are still needed for recovery
    /// - WAL entries <= watermark can be removed by compaction
    ///
    /// # Example
    /// ```
    /// let checkpoint = db.checkpoint()?;
    /// println!("Checkpointed at txn {}", checkpoint.watermark_txn);
    /// ```
    pub fn checkpoint(&self) -> Result<CheckpointInfo, StorageError> {
        // Get current transaction watermark
        let watermark_txn = self.engine.current_txn_id();

        // Generate snapshot ID
        let snapshot_id = self.next_snapshot_id();

        // Collect all primitive state
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
            SnapshotSection {
                primitive_type: primitive_tags::STATE,
                data: serializer.serialize_state_section(self.state_entries()),
            },
            SnapshotSection {
                primitive_type: primitive_tags::TRACE,
                data: serializer.serialize_trace_section(self.trace_entries()),
            },
            SnapshotSection {
                primitive_type: primitive_tags::RUN,
                data: serializer.serialize_run_section(self.run_entries()),
            },
            SnapshotSection {
                primitive_type: primitive_tags::JSON,
                data: serializer.serialize_json_section(self.json_entries()),
            },
            SnapshotSection {
                primitive_type: primitive_tags::VECTOR,
                data: serializer.serialize_vector_section(self.vector_entries()),
            },
        ];

        // Write snapshot
        let snapshot_info = self.snapshot_writer.create_snapshot(
            snapshot_id,
            watermark_txn,
            sections,
        )?;

        // Update MANIFEST
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

---

## Story #510: Snapshot Metadata and Watermark

**File**: `crates/storage/src/format/manifest.rs`

**Deliverable**: MANIFEST tracking of snapshot watermark

### Implementation

```rust
/// MANIFEST file structure
#[derive(Debug, Clone)]
pub struct Manifest {
    /// Format version for forward compatibility
    pub format_version: u32,

    /// Unique database identifier
    pub database_uuid: [u8; 16],

    /// Codec identifier
    pub codec_id: String,

    /// Current active WAL segment number
    pub active_wal_segment: u64,

    /// Latest snapshot watermark (if any)
    pub snapshot_watermark: Option<u64>,

    /// Latest snapshot identifier (if any)
    pub snapshot_id: Option<u64>,
}

impl Manifest {
    /// Set snapshot watermark
    pub fn set_snapshot_watermark(
        &mut self,
        snapshot_id: u64,
        watermark_txn: u64,
    ) {
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

---

## Story #511: Snapshot Loading

**File**: `crates/storage/src/snapshot/reader.rs` (NEW)

**Deliverable**: Snapshot deserialization for recovery

### Implementation

```rust
use std::fs::File;
use std::io::{Read, BufReader};

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
                    // Check if this might be the footer (4 bytes CRC)
                    // Section headers are 9 bytes, footer is 4 bytes
                    // We'll detect end by primitive_type == 0
                    let section_header = SectionHeader::from_bytes(&section_header_bytes);

                    if section_header.primitive_type == 0 {
                        // This is likely the footer CRC misread, stop
                        break;
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

        Ok(LoadedSnapshot {
            header,
            codec_id,
            sections,
        })
    }

    /// Deserialize KV section
    pub fn deserialize_kv_section(&self, data: &[u8]) -> Result<Vec<KvEntry>, SnapshotError> {
        let mut cursor = 0;

        let count = u32::from_le_bytes(data[0..4].try_into().unwrap()) as usize;
        cursor += 4;

        let mut entries = Vec::with_capacity(count);

        for _ in 0..count {
            let key_len = u32::from_le_bytes(
                data[cursor..cursor + 4].try_into().unwrap()
            ) as usize;
            cursor += 4;

            let key = String::from_utf8(data[cursor..cursor + key_len].to_vec())
                .map_err(|_| SnapshotError::InvalidData)?;
            cursor += key_len;

            let value_len = u32::from_le_bytes(
                data[cursor..cursor + 4].try_into().unwrap()
            ) as usize;
            cursor += 4;

            let value = self.codec.decode(&data[cursor..cursor + value_len])?;
            cursor += value_len;

            let version = u64::from_le_bytes(
                data[cursor..cursor + 8].try_into().unwrap()
            );
            cursor += 8;

            let timestamp = u64::from_le_bytes(
                data[cursor..cursor + 8].try_into().unwrap()
            );
            cursor += 8;

            entries.push(KvEntry {
                key,
                value,
                version,
                timestamp,
            });
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

---

## Testing

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_snapshot_header_roundtrip() {
        let header = SnapshotHeader {
            magic: SNAPSHOT_MAGIC,
            format_version: SNAPSHOT_FORMAT_VERSION,
            snapshot_id: 42,
            watermark_txn: 1000,
            created_at: 1234567890,
            database_uuid: [0xAB; 16],
            codec_id_len: 8,
            reserved: [0; 15],
        };

        let bytes = header.to_bytes();
        let parsed = SnapshotHeader::from_bytes(&bytes).unwrap();

        assert_eq!(parsed.magic, header.magic);
        assert_eq!(parsed.snapshot_id, header.snapshot_id);
        assert_eq!(parsed.watermark_txn, header.watermark_txn);
    }

    #[test]
    fn test_crash_safe_snapshot() {
        let dir = tempfile::tempdir().unwrap();
        let writer = SnapshotWriter::new(
            dir.path().to_path_buf(),
            Box::new(IdentityCodec),
            [0u8; 16],
        ).unwrap();

        let sections = vec![
            SnapshotSection {
                primitive_type: primitive_tags::KV,
                data: vec![0, 0, 0, 0], // Empty KV section
            },
        ];

        let info = writer.create_snapshot(1, 100, sections).unwrap();

        assert_eq!(info.snapshot_id, 1);
        assert_eq!(info.watermark_txn, 100);
        assert!(info.path.exists());

        // No temp files should remain
        for entry in std::fs::read_dir(dir.path()).unwrap() {
            let name = entry.unwrap().file_name().to_string_lossy().to_string();
            assert!(!name.ends_with(".tmp"));
        }
    }

    #[test]
    fn test_kv_serialization_roundtrip() {
        let serializer = SnapshotSerializer::new(Box::new(IdentityCodec));

        let entries = vec![
            KvEntry {
                key: "key1".to_string(),
                value: vec![1, 2, 3],
                version: 10,
                timestamp: 1000,
            },
            KvEntry {
                key: "key2".to_string(),
                value: vec![4, 5, 6],
                version: 20,
                timestamp: 2000,
            },
        ];

        let data = serializer.serialize_kv_section(entries.into_iter());

        let reader = SnapshotReader::new(Box::new(IdentityCodec));
        let parsed = reader.deserialize_kv_section(&data).unwrap();

        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0].key, "key1");
        assert_eq!(parsed[0].value, vec![1, 2, 3]);
        assert_eq!(parsed[1].key, "key2");
    }

    #[test]
    fn test_snapshot_load_validates_magic() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("bad.chk");

        // Write invalid snapshot
        std::fs::write(&path, b"BAAD").unwrap();

        let reader = SnapshotReader::new(Box::new(IdentityCodec));
        let result = reader.load(&path);

        assert!(matches!(result, Err(SnapshotError::InvalidHeader | SnapshotError::InvalidMagic)));
    }
}
```

---

## Files Modified/Created

| File | Action |
|------|--------|
| `crates/storage/src/format/snapshot.rs` | CREATE - Snapshot format |
| `crates/storage/src/format/primitives.rs` | CREATE - Primitive serialization |
| `crates/storage/src/snapshot/mod.rs` | CREATE - Snapshot module |
| `crates/storage/src/snapshot/writer.rs` | CREATE - Snapshot writer |
| `crates/storage/src/snapshot/reader.rs` | CREATE - Snapshot reader |
| `crates/storage/src/format/manifest.rs` | MODIFY - Add snapshot tracking |
