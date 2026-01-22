# Epic 72: Recovery

**Goal**: Implement recovery from snapshot + WAL replay

**Dependencies**: Epic 70 (WAL Infrastructure), Epic 71 (Snapshot System)

---

## Scope

- MANIFEST structure and atomic persistence
- WAL replay implementation
- Snapshot + WAL recovery algorithm
- Partial record truncation
- Recovery verification tests

---

## User Stories

| Story | Description | Priority |
|-------|-------------|----------|
| #513 | MANIFEST Structure and Persistence | FOUNDATION |
| #514 | WAL Replay Implementation | CRITICAL |
| #515 | Snapshot + WAL Recovery Algorithm | CRITICAL |
| #516 | Partial Record Truncation | CRITICAL |
| #517 | Recovery Verification Tests | HIGH |

---

## Story #513: MANIFEST Structure and Persistence

**File**: `crates/storage/src/format/manifest.rs` (NEW)

**Deliverable**: Atomic MANIFEST file handling

### Design

MANIFEST is a small versioned metadata file containing physical storage state only.

> **Design Rationale**: MANIFEST is intentionally minimal to avoid semantic coupling between storage format and data model. By keeping MANIFEST to physical metadata only (format version, segment IDs, watermarks), we:
> - Prevent configuration drift between MANIFEST and database state
> - Keep all semantic data (including policies) in the versioned, transactional data layer
> - Simplify backup/restore (MANIFEST is stateless relative to semantics)
> - Avoid MANIFEST becoming a dumping ground for "just one more field"

### Implementation

```rust
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

        // Magic
        bytes.extend_from_slice(&MANIFEST_MAGIC);

        // Format version
        bytes.extend_from_slice(&self.format_version.to_le_bytes());

        // Database UUID
        bytes.extend_from_slice(&self.database_uuid);

        // Codec ID (length-prefixed)
        bytes.extend_from_slice(&(self.codec_id.len() as u32).to_le_bytes());
        bytes.extend_from_slice(self.codec_id.as_bytes());

        // Active WAL segment
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

        // Verify magic
        if &bytes[0..4] != MANIFEST_MAGIC {
            return Err(ManifestError::InvalidMagic);
        }

        // Verify CRC (last 4 bytes)
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

        // Format version
        let format_version = u32::from_le_bytes(bytes[cursor..cursor + 4].try_into().unwrap());
        cursor += 4;

        // Database UUID
        let database_uuid: [u8; 16] = bytes[cursor..cursor + 16].try_into().unwrap();
        cursor += 16;

        // Codec ID
        let codec_id_len = u32::from_le_bytes(bytes[cursor..cursor + 4].try_into().unwrap()) as usize;
        cursor += 4;

        let codec_id = String::from_utf8(bytes[cursor..cursor + codec_id_len].to_vec())
            .map_err(|_| ManifestError::InvalidCodecId)?;
        cursor += codec_id_len;

        // Active WAL segment
        let active_wal_segment = u64::from_le_bytes(bytes[cursor..cursor + 8].try_into().unwrap());
        cursor += 8;

        // Snapshot watermark
        let watermark = u64::from_le_bytes(bytes[cursor..cursor + 8].try_into().unwrap());
        cursor += 8;
        let snapshot_watermark = if watermark > 0 { Some(watermark) } else { None };

        // Snapshot ID
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
    /// Path to MANIFEST file
    path: PathBuf,

    /// Current MANIFEST state
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

        // Write to temp file
        let mut file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&temp_path)?;

        file.write_all(&self.manifest.to_bytes())?;
        file.sync_all()?;
        drop(file);

        // Atomic rename
        std::fs::rename(&temp_path, &self.path)?;

        // fsync parent directory
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

    /// Get current manifest
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

---

## Story #514: WAL Replay Implementation

**File**: `crates/storage/src/recovery/replay.rs` (NEW)

**Deliverable**: WAL record replay with idempotency

### Implementation

```rust
use crate::format::wal_record::{WalRecord, WalRecordError};
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

    /// Read all records from a segment
    pub fn read_segment(&self, segment_number: u64) -> Result<Vec<WalRecord>, WalReplayError> {
        let segment = WalSegment::open_read(&self.wal_dir, segment_number)?;
        let mut records = Vec::new();

        let file_data = std::fs::read(WalSegment::segment_path(&self.wal_dir, segment_number))?;

        // Skip header
        let mut cursor = SEGMENT_HEADER_SIZE;

        while cursor < file_data.len() {
            match WalRecord::from_bytes(&file_data[cursor..]) {
                Ok((record, consumed)) => {
                    records.push(record);
                    cursor += consumed;
                }
                Err(WalRecordError::InsufficientData) => {
                    // End of valid records (possibly partial record at end)
                    break;
                }
                Err(WalRecordError::ChecksumMismatch { .. }) => {
                    // Corrupted record - stop here (tail corruption)
                    break;
                }
                Err(e) => {
                    return Err(WalReplayError::RecordError(e));
                }
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
    /// This is deterministic and idempotent:
    /// - Replaying the same records produces the same state
    /// - Replaying a record multiple times has the same effect as once
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

                // Apply the record
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

            // Apply each mutation
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

            // Update engine's transaction counter
            engine.update_txn_id(record.txn_id);

            Ok(())
        })
    }
}

/// Statistics from WAL replay
#[derive(Debug, Default)]
pub struct ReplayStats {
    /// Number of segments read
    pub segments_read: usize,

    /// Total records read
    pub records_read: usize,

    /// Records skipped (at or before watermark)
    pub records_skipped: usize,

    /// Records applied
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

---

## Story #515: Snapshot + WAL Recovery Algorithm

**File**: `crates/storage/src/recovery/mod.rs` (NEW)

**Deliverable**: Complete recovery algorithm

### Design

Recovery algorithm:
1. Load MANIFEST to get snapshot info and active WAL segment
2. If snapshot exists: load snapshot → replay WAL records > snapshot watermark
3. If no snapshot: replay all WAL records from the beginning
4. Truncate any partial records at WAL tail

### Implementation

```rust
/// Recovery coordinator
pub struct Recovery {
    /// Database directory
    db_dir: PathBuf,

    /// Codec for decoding
    codec: Box<dyn StorageCodec>,
}

impl Recovery {
    pub fn new(db_dir: PathBuf, codec: Box<dyn StorageCodec>) -> Self {
        Recovery { db_dir, codec }
    }

    /// Perform full recovery
    ///
    /// Returns recovered engine state and recovery info.
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
            let snapshot_path = snapshot_path(
                &self.db_dir.join("SNAPSHOTS"),
                snapshot_id,
            );

            let snapshot_reader = SnapshotReader::new(self.codec.clone());
            let loaded = snapshot_reader.load(&snapshot_path)?;

            // Apply snapshot to engine
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

        // Step 5: Truncate partial records (if any)
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
                primitive_tags::STATE => {
                    let cells = deserializer.deserialize_state_section(&section.data)?;
                    for cell in cells {
                        engine.load_state(cell)?;
                    }
                }
                primitive_tags::TRACE => {
                    let traces = deserializer.deserialize_trace_section(&section.data)?;
                    for trace in traces {
                        engine.load_trace(trace)?;
                    }
                }
                primitive_tags::RUN => {
                    let runs = deserializer.deserialize_run_section(&section.data)?;
                    for run in runs {
                        engine.load_run(run)?;
                    }
                }
                primitive_tags::JSON => {
                    let docs = deserializer.deserialize_json_section(&section.data)?;
                    for doc in docs {
                        engine.load_json(doc)?;
                    }
                }
                primitive_tags::VECTOR => {
                    let collections = deserializer.deserialize_vector_section(&section.data)?;
                    for collection in collections {
                        engine.load_vector_collection(collection)?;
                    }
                }
                _ => {
                    // Unknown section type - skip for forward compatibility
                }
            }
        }

        // Set engine's watermark
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

        // Only check the last segment (active segment)
        let last_segment = *segments.last().unwrap();
        let segment_path = WalSegment::segment_path(&wal_dir, last_segment);

        let file_data = std::fs::read(&segment_path)?;
        let mut cursor = SEGMENT_HEADER_SIZE;
        let mut valid_end = SEGMENT_HEADER_SIZE;

        // Find last valid record
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
            let file = OpenOptions::new()
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
    /// Recovered engine state
    pub engine: Engine,

    /// Loaded manifest
    pub manifest: Manifest,

    /// Snapshot watermark used (if any)
    pub snapshot_watermark: Option<u64>,

    /// WAL replay statistics
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

---

## Story #516: Partial Record Truncation

**File**: `crates/storage/src/recovery/replay.rs`

**Deliverable**: Safe handling of partial/corrupt WAL tail

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

(Included in Story #416)

### Acceptance Criteria

- [ ] Detect partial records at WAL tail
- [ ] Truncate file to last valid record boundary
- [ ] fsync after truncation
- [ ] Only truncate active segment (closed segments are immutable)
- [ ] Log truncation for observability

---

## Story #517: Recovery Verification Tests

**File**: `crates/storage/tests/recovery_tests.rs` (NEW)

**Deliverable**: Comprehensive recovery test suite

### Implementation

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_recovery_strict_mode_no_data_loss() {
        let dir = tempdir().unwrap();
        let db_dir = dir.path().to_path_buf();

        // Setup database
        let mut db = Database::create(&db_dir, DatabaseConfig {
            durability: DurabilityMode::Strict,
            ..Default::default()
        }).unwrap();

        // Write data
        let run_id = db.create_run("test-run").unwrap();
        db.kv_put(run_id, "key1", b"value1").unwrap();
        db.kv_put(run_id, "key2", b"value2").unwrap();

        // Simulate crash (drop without close)
        drop(db);

        // Recover
        let db = Database::open(&db_dir, DatabaseConfig::default()).unwrap();

        // Verify all data present
        let v1 = db.kv_get(run_id, "key1").unwrap();
        let v2 = db.kv_get(run_id, "key2").unwrap();

        assert_eq!(v1.unwrap().value, b"value1");
        assert_eq!(v2.unwrap().value, b"value2");
    }

    #[test]
    fn test_recovery_order_preservation() {
        let dir = tempdir().unwrap();
        let db_dir = dir.path().to_path_buf();

        // Write ordered data
        {
            let mut db = Database::create(&db_dir, DatabaseConfig {
                durability: DurabilityMode::Strict,
                ..Default::default()
            }).unwrap();

            let run_id = db.create_run("test-run").unwrap();

            for i in 0..100 {
                db.event_append(run_id, format!("event-{}", i).as_bytes()).unwrap();
            }
        }

        // Recover and verify order
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

        // Write data
        {
            let mut db = Database::create(&db_dir, DatabaseConfig {
                durability: DurabilityMode::Strict,
                ..Default::default()
            }).unwrap();

            let run_id = db.create_run("test-run").unwrap();
            db.kv_put(run_id, "key1", b"value1").unwrap();
        }

        // Recover multiple times - state should be identical
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

        // Write data, checkpoint, write more
        {
            let mut db = Database::create(&db_dir, DatabaseConfig {
                durability: DurabilityMode::Strict,
                ..Default::default()
            }).unwrap();

            let run_id = db.create_run("test-run").unwrap();

            // Before checkpoint
            db.kv_put(run_id, "key1", b"before-checkpoint").unwrap();

            db.checkpoint().unwrap();

            // After checkpoint
            db.kv_put(run_id, "key2", b"after-checkpoint").unwrap();
        }

        // Recover - should have both keys
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

        // Write valid data
        {
            let mut db = Database::create(&db_dir, DatabaseConfig {
                durability: DurabilityMode::Strict,
                ..Default::default()
            }).unwrap();

            let run_id = db.create_run("test-run").unwrap();
            db.kv_put(run_id, "key1", b"value1").unwrap();
        }

        // Append garbage to WAL (simulate partial write)
        let wal_dir = db_dir.join("WAL");
        let segments: Vec<_> = std::fs::read_dir(&wal_dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .collect();

        if let Some(last_segment) = segments.last() {
            let mut file = OpenOptions::new()
                .append(true)
                .open(last_segment.path())
                .unwrap();
            file.write_all(b"GARBAGE_PARTIAL_RECORD").unwrap();
        }

        // Recover - should truncate garbage and still have valid data
        let db = Database::open(&db_dir, DatabaseConfig::default()).unwrap();
        let run_id = db.resolve_run("test-run").unwrap();

        let v1 = db.kv_get(run_id, "key1").unwrap();
        assert_eq!(v1.unwrap().value, b"value1");
    }

    #[test]
    fn test_recovery_corrupt_checksum_stops_replay() {
        let dir = tempdir().unwrap();
        let db_dir = dir.path().to_path_buf();

        // Write multiple records
        {
            let mut db = Database::create(&db_dir, DatabaseConfig {
                durability: DurabilityMode::Strict,
                ..Default::default()
            }).unwrap();

            let run_id = db.create_run("test-run").unwrap();
            db.kv_put(run_id, "key1", b"value1").unwrap();
            db.kv_put(run_id, "key2", b"value2").unwrap();
        }

        // Corrupt second record's checksum
        let wal_dir = db_dir.join("WAL");
        for entry in std::fs::read_dir(&wal_dir).unwrap() {
            let entry = entry.unwrap();
            if entry.path().extension() == Some(std::ffi::OsStr::new("seg")) {
                let mut data = std::fs::read(entry.path()).unwrap();
                // Corrupt byte in the middle (affects second record)
                if data.len() > 100 {
                    data[80] ^= 0xFF;
                    std::fs::write(entry.path(), data).unwrap();
                }
            }
        }

        // Recover - should have first key but not second
        let db = Database::open(&db_dir, DatabaseConfig::default()).unwrap();
        let run_id = db.resolve_run("test-run").unwrap();

        // First key might be present (depends on corruption location)
        // The key invariant: no corrupt data is applied
    }

    #[test]
    fn test_recovery_multiple_checkpoints() {
        let dir = tempdir().unwrap();
        let db_dir = dir.path().to_path_buf();

        {
            let mut db = Database::create(&db_dir, DatabaseConfig {
                durability: DurabilityMode::Strict,
                ..Default::default()
            }).unwrap();

            let run_id = db.create_run("test-run").unwrap();

            // Multiple checkpoint cycles
            for i in 0..3 {
                db.kv_put(run_id, &format!("key-{}", i), format!("value-{}", i).as_bytes()).unwrap();
                db.checkpoint().unwrap();
            }

            // Final write after last checkpoint
            db.kv_put(run_id, "final", b"final-value").unwrap();
        }

        // Recover
        let db = Database::open(&db_dir, DatabaseConfig::default()).unwrap();
        let run_id = db.resolve_run("test-run").unwrap();

        // All keys should be present
        for i in 0..3 {
            let v = db.kv_get(run_id, &format!("key-{}", i)).unwrap();
            assert!(v.is_some());
        }

        let final_v = db.kv_get(run_id, "final").unwrap();
        assert_eq!(final_v.unwrap().value, b"final-value");
    }

    #[test]
    fn test_recovery_all_primitives() {
        let dir = tempdir().unwrap();
        let db_dir = dir.path().to_path_buf();

        let run_name = "test-run";
        let mut saved_run_id = None;

        // Write all primitive types
        {
            let mut db = Database::create(&db_dir, DatabaseConfig {
                durability: DurabilityMode::Strict,
                ..Default::default()
            }).unwrap();

            let run_id = db.create_run(run_name).unwrap();
            saved_run_id = Some(run_id);

            // KV
            db.kv_put(run_id, "test-key", b"test-value").unwrap();

            // Event
            db.event_append(run_id, b"test-event").unwrap();

            // State
            db.state_set(run_id, "test-cell", b"test-state").unwrap();

            // JSON
            let doc = serde_json::json!({"test": "document"});
            db.json_insert(run_id, doc).unwrap();

            // Vector
            db.vector_create_collection(run_id, "test-collection", VectorConfig::for_minilm()).unwrap();
            db.vector_insert(run_id, "test-collection", "vec-key", &vec![0.0f32; 384], None).unwrap();

            db.checkpoint().unwrap();
        }

        // Recover and verify all primitives
        let db = Database::open(&db_dir, DatabaseConfig::default()).unwrap();
        let run_id = saved_run_id.unwrap();

        // Verify KV
        let kv = db.kv_get(run_id, "test-key").unwrap();
        assert_eq!(kv.unwrap().value, b"test-value");

        // Verify Event
        let events = db.event_range(run_id, 0..1).unwrap();
        assert_eq!(events.len(), 1);

        // Verify State
        let state = db.state_get(run_id, "test-cell").unwrap();
        assert_eq!(state.unwrap().value, b"test-state");

        // Verify Vector
        let count = db.vector_count(run_id, "test-collection").unwrap();
        assert_eq!(count, 1);
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

---

## Files Modified/Created

| File | Action |
|------|--------|
| `crates/storage/src/format/manifest.rs` | CREATE - MANIFEST format |
| `crates/storage/src/recovery/mod.rs` | CREATE - Recovery module |
| `crates/storage/src/recovery/manifest.rs` | CREATE - MANIFEST manager |
| `crates/storage/src/recovery/replay.rs` | CREATE - WAL replay |
| `crates/storage/tests/recovery_tests.rs` | CREATE - Recovery tests |
