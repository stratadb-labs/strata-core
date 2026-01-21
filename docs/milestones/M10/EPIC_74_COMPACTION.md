# Epic 74: Compaction

**Goal**: Implement deterministic, user-triggered compaction

**Dependencies**: Epic 71 (Snapshot System), Epic 73 (Retention Policies)

---

## Scope

- CompactMode enum (WALOnly, Full)
- WAL-only compaction (remove segments covered by snapshot)
- Full compaction (WAL + retention enforcement)
- Tombstone management
- Compaction correctness verification

---

## User Stories

| Story | Description | Priority |
|-------|-------------|----------|
| #525 | CompactMode Enum and CompactInfo | FOUNDATION |
| #526 | WAL-Only Compaction | CRITICAL |
| #527 | Full Compaction (with Retention) | CRITICAL |
| #528 | Tombstone Management | HIGH |
| #529 | Compaction Correctness Verification | HIGH |
| #530 | Compaction API | CRITICAL |

---

## Story #525: CompactMode Enum and CompactInfo

**File**: `crates/storage/src/compaction/mod.rs` (NEW)

**Deliverable**: Compaction mode and result types

### Design

Compaction is user-triggered and deterministic:
- **WALOnly**: Remove WAL segments covered by snapshot (safe, always works)
- **Full**: WAL removal + retention policy enforcement (removes old versions)

> **Critical**: Compaction must never change version IDs. Versions are semantic identifiers that users and external systems depend on.

### Implementation

```rust
/// Compaction mode
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompactMode {
    /// Remove WAL segments covered by snapshot
    ///
    /// This is the safest compaction mode. It only removes WAL segments
    /// whose transactions are fully captured in a snapshot.
    ///
    /// After WAL-only compaction:
    /// - Snapshot contains all state up to watermark
    /// - WAL contains transactions after watermark
    /// - All version history is preserved
    WALOnly,

    /// Full compaction: WAL + retention policy enforcement
    ///
    /// This mode:
    /// 1. Removes WAL segments covered by snapshot
    /// 2. Applies retention policy to remove old versions
    /// 3. Creates tombstones for deleted entries
    ///
    /// After full compaction:
    /// - Disk space is reclaimed
    /// - Old versions may be permanently removed
    /// - Retained versions are unchanged (including their IDs)
    Full,
}

impl CompactMode {
    pub fn name(&self) -> &'static str {
        match self {
            CompactMode::WALOnly => "wal_only",
            CompactMode::Full => "full",
        }
    }
}

/// Result of a compaction operation
#[derive(Debug, Clone)]
pub struct CompactInfo {
    /// Compaction mode used
    pub mode: CompactMode,

    /// Bytes reclaimed from disk
    pub reclaimed_bytes: u64,

    /// Number of WAL segments removed
    pub wal_segments_removed: usize,

    /// Number of versions removed (Full mode only)
    pub versions_removed: usize,

    /// Snapshot watermark used for compaction
    pub snapshot_watermark: Option<u64>,

    /// Duration of compaction operation
    pub duration_ms: u64,

    /// Timestamp of compaction
    pub timestamp: u64,
}

impl CompactInfo {
    pub fn new(mode: CompactMode) -> Self {
        CompactInfo {
            mode,
            reclaimed_bytes: 0,
            wal_segments_removed: 0,
            versions_removed: 0,
            snapshot_watermark: None,
            duration_ms: 0,
            timestamp: 0,
        }
    }
}

/// Compaction error types
#[derive(Debug, thiserror::Error)]
pub enum CompactionError {
    #[error("No snapshot available for compaction")]
    NoSnapshot,

    #[error("Compaction already in progress")]
    AlreadyInProgress,

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Retention error: {0}")]
    Retention(String),

    #[error("Internal error: {0}")]
    Internal(String),
}
```

### Acceptance Criteria

- [ ] `CompactMode::WALOnly` for safe WAL removal
- [ ] `CompactMode::Full` for WAL + retention enforcement
- [ ] `CompactInfo` with reclaimed_bytes, wal_segments_removed, versions_removed
- [ ] `CompactionError` for error handling
- [ ] Mode names for logging/metrics

---

## Story #526: WAL-Only Compaction

**File**: `crates/storage/src/compaction/wal_only.rs` (NEW)

**Deliverable**: Remove WAL segments covered by snapshot

### Implementation

```rust
/// WAL-only compaction
pub struct WalOnlyCompactor {
    wal_dir: PathBuf,
    manifest: Arc<Mutex<ManifestManager>>,
}

impl WalOnlyCompactor {
    pub fn new(wal_dir: PathBuf, manifest: Arc<Mutex<ManifestManager>>) -> Self {
        WalOnlyCompactor { wal_dir, manifest }
    }

    /// Perform WAL-only compaction
    ///
    /// Removes WAL segments whose highest txn_id is <= snapshot watermark.
    pub fn compact(&self) -> Result<CompactInfo, CompactionError> {
        let start_time = std::time::Instant::now();
        let mut info = CompactInfo::new(CompactMode::WALOnly);

        // Get snapshot watermark
        let manifest = self.manifest.lock().unwrap();
        let watermark = manifest.manifest().snapshot_watermark
            .ok_or(CompactionError::NoSnapshot)?;
        let active_segment = manifest.manifest().active_wal_segment;
        drop(manifest);

        info.snapshot_watermark = Some(watermark);

        // List all WAL segments
        let segments = self.list_segments()?;

        for segment_number in segments {
            // Never remove active segment
            if segment_number >= active_segment {
                continue;
            }

            // Check if segment is fully covered by snapshot
            if self.segment_covered_by_watermark(segment_number, watermark)? {
                let segment_path = WalSegment::segment_path(&self.wal_dir, segment_number);
                let segment_size = std::fs::metadata(&segment_path)?.len();

                // Remove segment
                std::fs::remove_file(&segment_path)?;

                info.reclaimed_bytes += segment_size;
                info.wal_segments_removed += 1;
            }
        }

        info.duration_ms = start_time.elapsed().as_millis() as u64;
        info.timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_micros() as u64;

        Ok(info)
    }

    /// List all WAL segment numbers in order
    fn list_segments(&self) -> Result<Vec<u64>, CompactionError> {
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

    /// Check if all records in a segment are covered by the watermark
    fn segment_covered_by_watermark(
        &self,
        segment_number: u64,
        watermark: u64,
    ) -> Result<bool, CompactionError> {
        let segment_path = WalSegment::segment_path(&self.wal_dir, segment_number);
        let file_data = std::fs::read(&segment_path)?;

        if file_data.len() <= SEGMENT_HEADER_SIZE {
            // Empty segment (only header) - can be removed
            return Ok(true);
        }

        // Find the highest txn_id in the segment
        let mut cursor = SEGMENT_HEADER_SIZE;
        let mut max_txn_id = 0u64;

        while cursor < file_data.len() {
            match WalRecord::from_bytes(&file_data[cursor..]) {
                Ok((record, consumed)) => {
                    max_txn_id = max_txn_id.max(record.txn_id);
                    cursor += consumed;
                }
                Err(WalRecordError::InsufficientData) => break,
                Err(_) => break, // Stop at corrupted record
            }
        }

        // Segment is covered if its highest txn_id is <= watermark
        Ok(max_txn_id <= watermark)
    }
}
```

### Acceptance Criteria

- [ ] Remove segments where max(txn_id) <= snapshot watermark
- [ ] Never remove active segment
- [ ] Track reclaimed_bytes and wal_segments_removed
- [ ] Handle empty segments
- [ ] Error if no snapshot exists

---

## Story #527: Full Compaction (with Retention)

**File**: `crates/storage/src/compaction/full.rs` (NEW)

**Deliverable**: Full compaction with retention policy enforcement

### Design

Full compaction:
1. Perform WAL-only compaction
2. Apply retention policy to identify removable versions
3. Create tombstones for removed entries
4. Update snapshot with compacted state

> **Version Identity Invariant**: Compaction must not rewrite, renumber, or reinterpret version identifiers. This is critical because version numbers are semantic identifiers referenced by users and external systems.

### Implementation

```rust
/// Full compaction (WAL + retention)
pub struct FullCompactor {
    wal_only: WalOnlyCompactor,
    retention_enforcer: Arc<RetentionEnforcer>,
    snapshot_writer: Arc<SnapshotWriter>,
    engine: Arc<Engine>,
    manifest: Arc<Mutex<ManifestManager>>,
}

impl FullCompactor {
    pub fn new(
        wal_dir: PathBuf,
        retention_enforcer: Arc<RetentionEnforcer>,
        snapshot_writer: Arc<SnapshotWriter>,
        engine: Arc<Engine>,
        manifest: Arc<Mutex<ManifestManager>>,
    ) -> Self {
        FullCompactor {
            wal_only: WalOnlyCompactor::new(wal_dir, manifest.clone()),
            retention_enforcer,
            snapshot_writer,
            engine,
            manifest,
        }
    }

    /// Perform full compaction
    pub fn compact(&self) -> Result<CompactInfo, CompactionError> {
        let start_time = std::time::Instant::now();
        let mut info = CompactInfo::new(CompactMode::Full);

        // Step 1: WAL-only compaction
        let wal_result = self.wal_only.compact()?;
        info.reclaimed_bytes += wal_result.reclaimed_bytes;
        info.wal_segments_removed = wal_result.wal_segments_removed;
        info.snapshot_watermark = wal_result.snapshot_watermark;

        // Step 2: Apply retention policy
        let current_time = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_micros() as u64;

        // Collect all runs
        let runs = self.engine.list_runs()?;

        for run_id in runs {
            let removable = self.retention_enforcer.compute_removable_versions(
                run_id.as_bytes(),
                &self.engine.version_index(),
                current_time,
            ).map_err(|e| CompactionError::Retention(e.to_string()))?;

            for (entity_ref, version) in removable {
                // Create tombstone (marks version as removed)
                self.engine.create_tombstone(&entity_ref, version)?;
                info.versions_removed += 1;
            }
        }

        // Step 3: Create compacted snapshot if versions were removed
        if info.versions_removed > 0 {
            // Get new watermark (current txn)
            let watermark_txn = self.engine.current_txn_id();
            let snapshot_id = self.next_snapshot_id();

            // Collect compacted state (excludes tombstoned versions)
            let sections = self.collect_compacted_state()?;

            // Write compacted snapshot
            let snapshot_info = self.snapshot_writer.create_snapshot(
                snapshot_id,
                watermark_txn,
                sections,
            ).map_err(|e| CompactionError::Io(e))?;

            // Update manifest
            let mut manifest = self.manifest.lock().unwrap();
            manifest.set_snapshot_watermark(snapshot_id, watermark_txn)?;
        }

        info.duration_ms = start_time.elapsed().as_millis() as u64;
        info.timestamp = current_time;

        Ok(info)
    }

    /// Collect state for compacted snapshot
    fn collect_compacted_state(&self) -> Result<Vec<SnapshotSection>, CompactionError> {
        let serializer = SnapshotSerializer::new(self.engine.codec().clone());

        Ok(vec![
            SnapshotSection {
                primitive_type: primitive_tags::KV,
                data: serializer.serialize_kv_section(
                    self.engine.kv_entries_without_tombstones()
                ),
            },
            SnapshotSection {
                primitive_type: primitive_tags::EVENT,
                data: serializer.serialize_event_section(
                    self.engine.event_entries_without_tombstones()
                ),
            },
            SnapshotSection {
                primitive_type: primitive_tags::STATE,
                data: serializer.serialize_state_section(
                    self.engine.state_entries_without_tombstones()
                ),
            },
            SnapshotSection {
                primitive_type: primitive_tags::TRACE,
                data: serializer.serialize_trace_section(
                    self.engine.trace_entries_without_tombstones()
                ),
            },
            SnapshotSection {
                primitive_type: primitive_tags::RUN,
                data: serializer.serialize_run_section(
                    self.engine.run_entries_without_tombstones()
                ),
            },
            SnapshotSection {
                primitive_type: primitive_tags::JSON,
                data: serializer.serialize_json_section(
                    self.engine.json_entries_without_tombstones()
                ),
            },
            SnapshotSection {
                primitive_type: primitive_tags::VECTOR,
                data: serializer.serialize_vector_section(
                    self.engine.vector_entries_without_tombstones()
                ),
            },
        ])
    }

    fn next_snapshot_id(&self) -> u64 {
        let manifest = self.manifest.lock().unwrap();
        manifest.manifest().snapshot_id.map(|id| id + 1).unwrap_or(1)
    }
}
```

### Acceptance Criteria

- [ ] Perform WAL-only compaction first
- [ ] Apply retention policy to compute removable versions
- [ ] Create tombstones for removed versions
- [ ] Create new snapshot with compacted state
- [ ] Update MANIFEST with new snapshot
- [ ] Track versions_removed in CompactInfo
- [ ] **Never change version IDs**

---

## Story #528: Tombstone Management

**File**: `crates/storage/src/compaction/tombstone.rs` (NEW)

**Deliverable**: Tombstone tracking for deleted/compacted entries

### Design

Tombstones mark entries as deleted without physically removing them immediately. This allows:
- Concurrent reads to see deletion
- Recovery to understand deletions
- Future garbage collection

### Implementation

```rust
/// Tombstone for a deleted/compacted entry
#[derive(Debug, Clone)]
pub struct Tombstone {
    /// Entity that was deleted
    pub entity_ref: EntityRef,

    /// Version that was deleted
    pub version: u64,

    /// When the tombstone was created
    pub created_at: u64,

    /// Reason for tombstone
    pub reason: TombstoneReason,
}

/// Reason for tombstone creation
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TombstoneReason {
    /// User deleted the entry
    UserDelete,

    /// Retention policy removed the version
    RetentionPolicy,

    /// Compaction removed the version
    Compaction,
}

impl Tombstone {
    pub fn new(
        entity_ref: EntityRef,
        version: u64,
        reason: TombstoneReason,
    ) -> Self {
        Tombstone {
            entity_ref,
            version,
            created_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_micros() as u64,
            reason,
        }
    }

    /// Serialize tombstone to bytes
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::new();

        // Entity ref
        let entity_bytes = entity_ref_to_bytes(&self.entity_ref);
        bytes.extend_from_slice(&(entity_bytes.len() as u32).to_le_bytes());
        bytes.extend_from_slice(&entity_bytes);

        // Version
        bytes.extend_from_slice(&self.version.to_le_bytes());

        // Created at
        bytes.extend_from_slice(&self.created_at.to_le_bytes());

        // Reason
        bytes.push(match self.reason {
            TombstoneReason::UserDelete => 0x01,
            TombstoneReason::RetentionPolicy => 0x02,
            TombstoneReason::Compaction => 0x03,
        });

        bytes
    }

    /// Deserialize tombstone from bytes
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, TombstoneError> {
        let mut cursor = 0;

        // Entity ref
        let entity_len = u32::from_le_bytes(
            bytes[cursor..cursor + 4].try_into().unwrap()
        ) as usize;
        cursor += 4;

        let (entity_ref, _) = entity_ref_from_bytes(&bytes[cursor..cursor + entity_len])
            .map_err(|_| TombstoneError::InvalidEntityRef)?;
        cursor += entity_len;

        // Version
        let version = u64::from_le_bytes(
            bytes[cursor..cursor + 8].try_into().unwrap()
        );
        cursor += 8;

        // Created at
        let created_at = u64::from_le_bytes(
            bytes[cursor..cursor + 8].try_into().unwrap()
        );
        cursor += 8;

        // Reason
        let reason = match bytes[cursor] {
            0x01 => TombstoneReason::UserDelete,
            0x02 => TombstoneReason::RetentionPolicy,
            0x03 => TombstoneReason::Compaction,
            _ => return Err(TombstoneError::InvalidReason),
        };

        Ok(Tombstone {
            entity_ref,
            version,
            created_at,
            reason,
        })
    }
}

/// Tombstone index for tracking deletions
pub struct TombstoneIndex {
    /// Tombstones by entity ref
    tombstones: HashMap<EntityRef, Vec<Tombstone>>,
}

impl TombstoneIndex {
    pub fn new() -> Self {
        TombstoneIndex {
            tombstones: HashMap::new(),
        }
    }

    /// Add a tombstone
    pub fn add(&mut self, tombstone: Tombstone) {
        self.tombstones
            .entry(tombstone.entity_ref.clone())
            .or_insert_with(Vec::new)
            .push(tombstone);
    }

    /// Check if a version is tombstoned
    pub fn is_tombstoned(&self, entity_ref: &EntityRef, version: u64) -> bool {
        self.tombstones
            .get(entity_ref)
            .map(|ts| ts.iter().any(|t| t.version == version))
            .unwrap_or(false)
    }

    /// Get all tombstones for an entity
    pub fn get(&self, entity_ref: &EntityRef) -> Option<&[Tombstone]> {
        self.tombstones.get(entity_ref).map(|v| v.as_slice())
    }

    /// Remove tombstones older than a given timestamp
    pub fn cleanup_before(&mut self, cutoff: u64) {
        for tombstones in self.tombstones.values_mut() {
            tombstones.retain(|t| t.created_at >= cutoff);
        }
        self.tombstones.retain(|_, v| !v.is_empty());
    }
}

#[derive(Debug, thiserror::Error)]
pub enum TombstoneError {
    #[error("Invalid entity ref")]
    InvalidEntityRef,

    #[error("Invalid reason")]
    InvalidReason,
}
```

### Acceptance Criteria

- [ ] `Tombstone` struct with entity_ref, version, created_at, reason
- [ ] `TombstoneReason` enum (UserDelete, RetentionPolicy, Compaction)
- [ ] Serialization/deserialization
- [ ] `TombstoneIndex` for efficient lookup
- [ ] `is_tombstoned()` check
- [ ] `cleanup_before()` for garbage collection

---

## Story #529: Compaction Correctness Verification

**File**: `crates/storage/tests/compaction_tests.rs` (NEW)

**Deliverable**: Tests verifying compaction invariants

### Implementation

```rust
#[cfg(test)]
mod tests {
    use super::*;

    /// Read equivalence: reads before/after compaction must match
    #[test]
    fn test_compaction_read_equivalence() {
        let dir = tempdir().unwrap();
        let db_dir = dir.path().to_path_buf();

        let mut db = Database::create(&db_dir, DatabaseConfig::default()).unwrap();
        let run_id = db.create_run("test-run").unwrap();

        // Write data
        for i in 0..100 {
            db.kv_put(run_id, &format!("key-{}", i), format!("value-{}", i).as_bytes()).unwrap();
        }

        db.checkpoint().unwrap();

        // Capture reads before compaction
        let mut before_reads = Vec::new();
        for i in 0..100 {
            let value = db.kv_get(run_id, &format!("key-{}", i)).unwrap();
            before_reads.push(value);
        }

        // Compact
        let info = db.compact(CompactMode::WALOnly).unwrap();
        assert!(info.wal_segments_removed >= 0);

        // Reads after compaction must match
        for i in 0..100 {
            let value = db.kv_get(run_id, &format!("key-{}", i)).unwrap();
            assert_eq!(before_reads[i], value, "Read mismatch for key-{}", i);
        }
    }

    /// Version IDs must never change during compaction
    #[test]
    fn test_compaction_version_identity() {
        let dir = tempdir().unwrap();
        let db_dir = dir.path().to_path_buf();

        let mut db = Database::create(&db_dir, DatabaseConfig::default()).unwrap();
        let run_id = db.create_run("test-run").unwrap();

        // Write data and capture versions
        let mut versions = Vec::new();
        for i in 0..50 {
            let result = db.kv_put(run_id, &format!("key-{}", i), b"value").unwrap();
            versions.push((format!("key-{}", i), result.version));
        }

        db.checkpoint().unwrap();
        db.compact(CompactMode::WALOnly).unwrap();

        // Verify versions unchanged
        for (key, expected_version) in &versions {
            let value = db.kv_get(run_id, key).unwrap().unwrap();
            assert_eq!(
                value.version, *expected_version,
                "Version changed for {} after compaction", key
            );
        }
    }

    /// History order must be preserved
    #[test]
    fn test_compaction_order_preservation() {
        let dir = tempdir().unwrap();
        let db_dir = dir.path().to_path_buf();

        let mut db = Database::create(&db_dir, DatabaseConfig::default()).unwrap();
        let run_id = db.create_run("test-run").unwrap();

        // Write ordered events
        for i in 0..100 {
            db.event_append(run_id, format!("event-{}", i).as_bytes()).unwrap();
        }

        db.checkpoint().unwrap();
        db.compact(CompactMode::WALOnly).unwrap();

        // Verify order preserved
        let events = db.event_range(run_id, 0..100).unwrap();
        for (i, event) in events.iter().enumerate() {
            let expected = format!("event-{}", i);
            assert_eq!(
                event.value, expected.as_bytes(),
                "Event order changed at position {}", i
            );
        }
    }

    /// Compaction only removes data below watermark
    #[test]
    fn test_compaction_safe_boundaries() {
        let dir = tempdir().unwrap();
        let db_dir = dir.path().to_path_buf();

        let mut db = Database::create(&db_dir, DatabaseConfig::default()).unwrap();
        let run_id = db.create_run("test-run").unwrap();

        // Write data before checkpoint
        db.kv_put(run_id, "before-checkpoint", b"value1").unwrap();

        db.checkpoint().unwrap();

        // Write data after checkpoint
        db.kv_put(run_id, "after-checkpoint", b"value2").unwrap();

        db.compact(CompactMode::WALOnly).unwrap();

        // Both should be present
        assert!(db.kv_get(run_id, "before-checkpoint").unwrap().is_some());
        assert!(db.kv_get(run_id, "after-checkpoint").unwrap().is_some());
    }

    /// Full compaction with retention
    #[test]
    fn test_full_compaction_with_retention() {
        let dir = tempdir().unwrap();
        let db_dir = dir.path().to_path_buf();

        let mut db = Database::create(&db_dir, DatabaseConfig::default()).unwrap();
        let run_id = db.create_run("test-run").unwrap();

        // Set retention policy: keep last 5 versions
        db.set_retention_policy(run_id, RetentionPolicy::keep_last(5)).unwrap();

        // Write 10 versions of same key
        for i in 0..10 {
            db.kv_put(run_id, "versioned-key", format!("value-{}", i).as_bytes()).unwrap();
        }

        db.checkpoint().unwrap();

        // Full compaction
        let info = db.compact(CompactMode::Full).unwrap();
        assert!(info.versions_removed >= 5); // At least 5 old versions removed

        // Current version should still exist
        let current = db.kv_get(run_id, "versioned-key").unwrap().unwrap();
        assert_eq!(current.value, b"value-9");
    }

    /// Concurrent reads during compaction
    #[test]
    fn test_compaction_concurrent_reads() {
        use std::sync::Arc;
        use std::thread;

        let dir = tempdir().unwrap();
        let db_dir = dir.path().to_path_buf();

        let db = Arc::new(Database::create(&db_dir, DatabaseConfig::default()).unwrap());
        let run_id = db.create_run("test-run").unwrap();

        // Write data
        for i in 0..100 {
            db.kv_put(run_id, &format!("key-{}", i), b"value").unwrap();
        }

        db.checkpoint().unwrap();

        // Spawn reader thread
        let reader_db = db.clone();
        let reader = thread::spawn(move || {
            for _ in 0..100 {
                for i in 0..100 {
                    let _ = reader_db.kv_get(run_id, &format!("key-{}", i));
                }
            }
        });

        // Compact while reading
        db.compact(CompactMode::WALOnly).unwrap();

        reader.join().unwrap();

        // All data should still be accessible
        for i in 0..100 {
            assert!(db.kv_get(run_id, &format!("key-{}", i)).unwrap().is_some());
        }
    }

    /// No implicit compaction
    #[test]
    fn test_no_implicit_compaction() {
        let dir = tempdir().unwrap();
        let db_dir = dir.path().to_path_buf();

        let mut db = Database::create(&db_dir, DatabaseConfig::default()).unwrap();
        let run_id = db.create_run("test-run").unwrap();

        // Write lots of data
        for i in 0..1000 {
            db.kv_put(run_id, &format!("key-{}", i), b"value").unwrap();
        }

        db.checkpoint().unwrap();

        // Count WAL segments before explicit compact
        let wal_dir = db_dir.join("WAL");
        let segments_before: Vec<_> = std::fs::read_dir(&wal_dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension() == Some(std::ffi::OsStr::new("seg")))
            .collect();

        // Without explicit compact, segments should remain
        assert!(!segments_before.is_empty());

        // Explicit compact
        db.compact(CompactMode::WALOnly).unwrap();

        // Now segments should be removed
        let segments_after: Vec<_> = std::fs::read_dir(&wal_dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension() == Some(std::ffi::OsStr::new("seg")))
            .collect();

        assert!(segments_after.len() < segments_before.len());
    }
}
```

### Acceptance Criteria

- [ ] Test read equivalence before/after compaction
- [ ] Test version IDs unchanged
- [ ] Test history order preserved
- [ ] Test safe boundaries (only below watermark)
- [ ] Test full compaction with retention
- [ ] Test concurrent reads during compaction
- [ ] Test no implicit/background compaction

---

## Story #530: Compaction API

**File**: `crates/storage/src/compaction/mod.rs`

**Deliverable**: Public compaction API

### Implementation

```rust
impl Database {
    /// Compact the database
    ///
    /// Compaction reclaims disk space by removing WAL segments and old versions.
    ///
    /// # Modes
    ///
    /// - `WALOnly`: Safe mode that only removes WAL segments covered by snapshot
    /// - `Full`: Removes WAL + applies retention policy to remove old versions
    ///
    /// # Important
    ///
    /// Compaction is:
    /// - **Deterministic**: Same input → same output
    /// - **User-triggered**: No background compaction
    /// - **Logically invisible**: Read results unchanged for retained data
    /// - **Version-preserving**: Version IDs never change
    ///
    /// # Example
    /// ```
    /// // Create checkpoint first
    /// db.checkpoint()?;
    ///
    /// // Safe compaction
    /// let info = db.compact(CompactMode::WALOnly)?;
    /// println!("Reclaimed {} bytes", info.reclaimed_bytes);
    ///
    /// // Full compaction with retention
    /// db.set_retention_policy(run_id, RetentionPolicy::keep_last(100))?;
    /// let info = db.compact(CompactMode::Full)?;
    /// println!("Removed {} old versions", info.versions_removed);
    /// ```
    pub fn compact(&self, mode: CompactMode) -> Result<CompactInfo, CompactionError> {
        // Prevent concurrent compaction
        let _lock = self.compaction_lock.try_lock()
            .map_err(|_| CompactionError::AlreadyInProgress)?;

        match mode {
            CompactMode::WALOnly => {
                self.wal_only_compactor.compact()
            }
            CompactMode::Full => {
                self.full_compactor.compact()
            }
        }
    }

    /// Check if compaction is recommended
    ///
    /// Returns true if:
    /// - WAL segments significantly exceed snapshot watermark
    /// - Disk usage is above threshold
    pub fn should_compact(&self) -> Result<bool, StorageError> {
        let manifest = self.manifest.lock().unwrap();
        let snapshot_watermark = manifest.manifest().snapshot_watermark;
        drop(manifest);

        // No snapshot = no compaction possible
        if snapshot_watermark.is_none() {
            return Ok(false);
        }

        // Check WAL size
        let wal_size = self.wal_size()?;
        let snapshot_size = self.snapshot_size()?;

        // Recommend compaction if WAL > snapshot
        Ok(wal_size > snapshot_size)
    }

    /// Get current WAL disk usage
    pub fn wal_size(&self) -> Result<u64, StorageError> {
        let wal_dir = self.db_dir.join("WAL");
        let mut total = 0u64;

        for entry in std::fs::read_dir(&wal_dir)? {
            let entry = entry?;
            if entry.path().extension() == Some(std::ffi::OsStr::new("seg")) {
                total += entry.metadata()?.len();
            }
        }

        Ok(total)
    }

    /// Get current snapshot disk usage
    pub fn snapshot_size(&self) -> Result<u64, StorageError> {
        let manifest = self.manifest.lock().unwrap();
        let snapshot_id = manifest.manifest().snapshot_id;
        drop(manifest);

        if let Some(id) = snapshot_id {
            let path = snapshot_path(&self.db_dir.join("SNAPSHOTS"), id);
            Ok(std::fs::metadata(&path)?.len())
        } else {
            Ok(0)
        }
    }
}
```

### Acceptance Criteria

- [ ] `compact(mode)` returns CompactInfo
- [ ] Prevents concurrent compaction
- [ ] `should_compact()` for heuristic check
- [ ] `wal_size()` and `snapshot_size()` for observability
- [ ] Clear documentation with examples
- [ ] No implicit/background compaction

---

## Files Modified/Created

| File | Action |
|------|--------|
| `crates/storage/src/compaction/mod.rs` | CREATE - Compaction module |
| `crates/storage/src/compaction/wal_only.rs` | CREATE - WAL-only compaction |
| `crates/storage/src/compaction/full.rs` | CREATE - Full compaction |
| `crates/storage/src/compaction/tombstone.rs` | CREATE - Tombstone management |
| `crates/storage/tests/compaction_tests.rs` | CREATE - Compaction tests |
