# Epic 74: Compaction - Implementation Prompts

**Epic Goal**: Implement deterministic, user-triggered compaction

**GitHub Issue**: [#525](https://github.com/anibjoshi/in-mem/issues/525)
**Status**: Ready to begin
**Dependencies**: Epic 71 (Snapshot System), Epic 73 (Retention Policies)
**Phase**: 4 (Retention & Compaction)

---

## NAMING CONVENTION - CRITICAL

> **NEVER use "M10" or "Strata" in the actual codebase or comments.**
>
> - "M10" is an internal milestone tracker only - do not use it in code, comments, or user-facing text
> - All existing crates refer to the database as "in-mem" - use this name consistently
> - Do not use "Strata" anywhere in the codebase
> - This applies to: code, comments, docstrings, error messages, log messages, test names
>
> **CORRECT**: `//! Database compaction for disk space reclamation`
> **WRONG**: `//! M10 Compaction for Strata database`

---

## AUTHORITATIVE SPECIFICATIONS - READ THESE FIRST

**`docs/architecture/M10_ARCHITECTURE.md` is THE AUTHORITATIVE SPEC.**

Before starting ANY story in this epic, read:
1. **Architecture Spec (AUTHORITATIVE)**: `docs/architecture/M10_ARCHITECTURE.md`
2. **Implementation Plan**: `docs/milestones/M10/M10_IMPLEMENTATION_PLAN.md`
3. **Epic Spec**: `docs/milestones/M10/EPIC_74_COMPACTION.md`
4. **Prompt Header**: `docs/prompts/M10/M10_PROMPT_HEADER.md` for the 8 architectural rules

**The architecture spec is LAW.** Epic docs provide implementation details but MUST NOT contradict the architecture spec.

---

## Epic 74 Overview

### Scope
- CompactMode enum (WALOnly, Full)
- WAL-only compaction (remove segments covered by snapshot)
- Full compaction (WAL + retention enforcement)
- Tombstone management
- Compaction correctness verification

### Key Rules for Epic 74

1. **Compaction is user-triggered** - No background compaction
2. **Compaction is deterministic** - Same input → same output
3. **Compaction is logically invisible** - Read results unchanged for retained data
4. **Version IDs never change** - Critical semantic invariant

### Success Criteria
- [ ] `CompactMode` enum with WALOnly, Full
- [ ] WAL-only compaction removes covered segments
- [ ] Full compaction applies retention policy
- [ ] Tombstone management for deleted entries
- [ ] Version identity invariant preserved
- [ ] Compaction correctness tests
- [ ] All tests passing

### Component Breakdown
- **Story #525**: CompactMode Enum and CompactInfo - FOUNDATION
- **Story #526**: WAL-Only Compaction - CRITICAL
- **Story #527**: Full Compaction (with Retention) - CRITICAL
- **Story #528**: Tombstone Management - HIGH
- **Story #529**: Compaction Correctness Verification - HIGH
- **Story #530**: Compaction API - CRITICAL

---

## File Organization

### Directory Structure

```bash
mkdir -p crates/storage/src/compaction
```

**Target structure**:
```
crates/storage/src/
├── lib.rs
├── format/
│   └── ...
├── wal/
│   └── ...
├── snapshot/
│   └── ...
├── recovery/
│   └── ...
├── retention/
│   └── ...
├── compaction/               # NEW
│   ├── mod.rs
│   ├── wal_only.rs           # WAL-only compaction
│   ├── full.rs               # Full compaction
│   └── tombstone.rs          # Tombstone management
└── codec/
    └── ...
```

---

## Dependency Graph

```
Story #525 (CompactMode) ──────> Story #526 (WAL-Only)
                                       │
                              └──> Story #527 (Full)
                                       │
Story #528 (Tombstone) ────────────────┘
                                       │
                              └──> Story #529 (Verification)
                                       │
Story #530 (API) ──────────────────────┘
```

**Recommended Order**: #525 (CompactMode) → #526 (WAL-Only) → #528 (Tombstone) → #527 (Full) → #530 (API) → #529 (Verification)

---

## Story #525: CompactMode Enum and CompactInfo

**GitHub Issue**: [#525](https://github.com/anibjoshi/in-mem/issues/525)
**Estimated Time**: 2 hours
**Dependencies**: None
**Blocks**: Stories #526, #527

### Start Story

```bash
gh issue view 525
./scripts/start-story.sh 74 525 compact-mode
```

### Implementation

Create `crates/storage/src/compaction/mod.rs`:

```rust
//! Database compaction
//!
//! Compaction reclaims disk space by removing WAL segments and old versions.
//! Compaction is user-triggered and deterministic.

pub mod wal_only;
pub mod full;
pub mod tombstone;

/// Compaction mode
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompactMode {
    /// Remove WAL segments covered by snapshot
    ///
    /// Safest mode. Only removes WAL segments whose transactions
    /// are fully captured in a snapshot. All version history preserved.
    WALOnly,

    /// Full compaction: WAL + retention policy enforcement
    ///
    /// Removes WAL segments AND applies retention policy to remove
    /// old versions. Version IDs never change.
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

### Complete Story

```bash
./scripts/complete-story.sh 525
```

---

## Story #526: WAL-Only Compaction

**GitHub Issue**: [#526](https://github.com/anibjoshi/in-mem/issues/526)
**Estimated Time**: 3 hours
**Dependencies**: Story #525
**Blocks**: Story #527

### Start Story

```bash
gh issue view 526
./scripts/start-story.sh 74 526 wal-only-compaction
```

### Implementation

Create `crates/storage/src/compaction/wal_only.rs`:

```rust
//! WAL-only compaction
//!
//! Removes WAL segments covered by snapshot watermark.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use crate::format::manifest::ManifestManager;
use crate::format::wal_record::{WalRecord, WalRecordError, SEGMENT_HEADER_SIZE};
use crate::wal::segment::WalSegment;
use super::{CompactMode, CompactInfo, CompactionError};

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
    /// Removes WAL segments whose highest txn_id <= snapshot watermark.
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

    fn segment_covered_by_watermark(
        &self,
        segment_number: u64,
        watermark: u64,
    ) -> Result<bool, CompactionError> {
        let segment_path = WalSegment::segment_path(&self.wal_dir, segment_number);
        let file_data = std::fs::read(&segment_path)?;

        if file_data.len() <= SEGMENT_HEADER_SIZE {
            return Ok(true); // Empty segment
        }

        // Find highest txn_id in segment
        let mut cursor = SEGMENT_HEADER_SIZE;
        let mut max_txn_id = 0u64;

        while cursor < file_data.len() {
            match WalRecord::from_bytes(&file_data[cursor..]) {
                Ok((record, consumed)) => {
                    max_txn_id = max_txn_id.max(record.txn_id);
                    cursor += consumed;
                }
                Err(WalRecordError::InsufficientData) => break,
                Err(_) => break,
            }
        }

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

### Complete Story

```bash
./scripts/complete-story.sh 526
```

---

## Story #527: Full Compaction (with Retention)

**GitHub Issue**: [#527](https://github.com/anibjoshi/in-mem/issues/527)
**Estimated Time**: 4 hours
**Dependencies**: Stories #526, #528
**Blocks**: Story #529

### Start Story

```bash
gh issue view 527
./scripts/start-story.sh 74 527 full-compaction
```

### Design

Full compaction:
1. Perform WAL-only compaction
2. Apply retention policy to identify removable versions
3. Create tombstones for removed entries
4. Update snapshot with compacted state

> **Version Identity Invariant**: Compaction must not rewrite, renumber, or reinterpret version identifiers. Version numbers are semantic identifiers referenced by users and external systems.

### Implementation

Create `crates/storage/src/compaction/full.rs`:

```rust
//! Full compaction with retention enforcement

use std::sync::Arc;
use std::path::PathBuf;
use crate::retention::enforcement::RetentionEnforcer;
use crate::snapshot::writer::SnapshotWriter;
use super::wal_only::WalOnlyCompactor;
use super::{CompactMode, CompactInfo, CompactionError};

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

        let runs = self.engine.list_runs()?;

        for run_id in runs {
            let removable = self.retention_enforcer.compute_removable_versions(
                run_id.as_bytes(),
                &self.engine.version_index(),
                current_time,
            ).map_err(|e| CompactionError::Retention(e.to_string()))?;

            for (entity_ref, version) in removable {
                self.engine.create_tombstone(&entity_ref, version)?;
                info.versions_removed += 1;
            }
        }

        // Step 3: Create compacted snapshot if versions were removed
        if info.versions_removed > 0 {
            let watermark_txn = self.engine.current_txn_id();
            let snapshot_id = self.next_snapshot_id();

            let sections = self.collect_compacted_state()?;

            let snapshot_info = self.snapshot_writer.create_snapshot(
                snapshot_id,
                watermark_txn,
                sections,
            ).map_err(|e| CompactionError::Io(e))?;

            let mut manifest = self.manifest.lock().unwrap();
            manifest.set_snapshot_watermark(snapshot_id, watermark_txn)?;
        }

        info.duration_ms = start_time.elapsed().as_millis() as u64;
        info.timestamp = current_time;

        Ok(info)
    }

    fn collect_compacted_state(&self) -> Result<Vec<SnapshotSection>, CompactionError> {
        // Collect state excluding tombstoned versions
        // ... implementation
        todo!()
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

### Complete Story

```bash
./scripts/complete-story.sh 527
```

---

## Story #528: Tombstone Management

**GitHub Issue**: [#528](https://github.com/anibjoshi/in-mem/issues/528)
**Estimated Time**: 3 hours
**Dependencies**: Story #525
**Blocks**: Story #527

### Start Story

```bash
gh issue view 528
./scripts/start-story.sh 74 528 tombstone-management
```

### Implementation

Create `crates/storage/src/compaction/tombstone.rs`:

```rust
//! Tombstone tracking for deleted/compacted entries

use std::collections::HashMap;
use crate::core::EntityRef;

/// Tombstone for a deleted/compacted entry
#[derive(Debug, Clone)]
pub struct Tombstone {
    pub entity_ref: EntityRef,
    pub version: u64,
    pub created_at: u64,
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
    pub fn new(entity_ref: EntityRef, version: u64, reason: TombstoneReason) -> Self {
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

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::new();

        let entity_bytes = entity_ref_to_bytes(&self.entity_ref);
        bytes.extend_from_slice(&(entity_bytes.len() as u32).to_le_bytes());
        bytes.extend_from_slice(&entity_bytes);

        bytes.extend_from_slice(&self.version.to_le_bytes());
        bytes.extend_from_slice(&self.created_at.to_le_bytes());

        bytes.push(match self.reason {
            TombstoneReason::UserDelete => 0x01,
            TombstoneReason::RetentionPolicy => 0x02,
            TombstoneReason::Compaction => 0x03,
        });

        bytes
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, TombstoneError> {
        // ... deserialization
        todo!()
    }
}

/// Tombstone index for tracking deletions
pub struct TombstoneIndex {
    tombstones: HashMap<EntityRef, Vec<Tombstone>>,
}

impl TombstoneIndex {
    pub fn new() -> Self {
        TombstoneIndex {
            tombstones: HashMap::new(),
        }
    }

    pub fn add(&mut self, tombstone: Tombstone) {
        self.tombstones
            .entry(tombstone.entity_ref.clone())
            .or_insert_with(Vec::new)
            .push(tombstone);
    }

    pub fn is_tombstoned(&self, entity_ref: &EntityRef, version: u64) -> bool {
        self.tombstones
            .get(entity_ref)
            .map(|ts| ts.iter().any(|t| t.version == version))
            .unwrap_or(false)
    }

    pub fn get(&self, entity_ref: &EntityRef) -> Option<&[Tombstone]> {
        self.tombstones.get(entity_ref).map(|v| v.as_slice())
    }

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

### Complete Story

```bash
./scripts/complete-story.sh 528
```

---

## Story #529: Compaction Correctness Verification

**GitHub Issue**: [#529](https://github.com/anibjoshi/in-mem/issues/529)
**Estimated Time**: 4 hours
**Dependencies**: Story #527
**Blocks**: None

### Start Story

```bash
gh issue view 529
./scripts/start-story.sh 74 529 compaction-verification
```

### Implementation

Create `crates/storage/tests/compaction_tests.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    /// Read equivalence: reads before/after compaction must match
    #[test]
    fn test_compaction_read_equivalence() {
        let dir = tempdir().unwrap();
        let db_dir = dir.path().to_path_buf();

        let mut db = Database::create(&db_dir, DatabaseConfig::default()).unwrap();
        let run_id = db.create_run("test-run").unwrap();

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
        db.compact(CompactMode::WALOnly).unwrap();

        // Reads after compaction must match
        for i in 0..100 {
            let value = db.kv_get(run_id, &format!("key-{}", i)).unwrap();
            assert_eq!(before_reads[i], value);
        }
    }

    /// Version IDs must never change during compaction
    #[test]
    fn test_compaction_version_identity() {
        let dir = tempdir().unwrap();
        let db_dir = dir.path().to_path_buf();

        let mut db = Database::create(&db_dir, DatabaseConfig::default()).unwrap();
        let run_id = db.create_run("test-run").unwrap();

        // Capture versions
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
            assert_eq!(value.version, *expected_version);
        }
    }

    /// History order must be preserved
    #[test]
    fn test_compaction_order_preservation() {
        let dir = tempdir().unwrap();
        let db_dir = dir.path().to_path_buf();

        let mut db = Database::create(&db_dir, DatabaseConfig::default()).unwrap();
        let run_id = db.create_run("test-run").unwrap();

        for i in 0..100 {
            db.event_append(run_id, format!("event-{}", i).as_bytes()).unwrap();
        }

        db.checkpoint().unwrap();
        db.compact(CompactMode::WALOnly).unwrap();

        let events = db.event_range(run_id, 0..100).unwrap();
        for (i, event) in events.iter().enumerate() {
            let expected = format!("event-{}", i);
            assert_eq!(event.value, expected.as_bytes());
        }
    }

    /// No implicit compaction
    #[test]
    fn test_no_implicit_compaction() {
        let dir = tempdir().unwrap();
        let db_dir = dir.path().to_path_buf();

        let mut db = Database::create(&db_dir, DatabaseConfig::default()).unwrap();
        let run_id = db.create_run("test-run").unwrap();

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

        assert!(!segments_before.is_empty());

        // Explicit compact
        db.compact(CompactMode::WALOnly).unwrap();

        let segments_after: Vec<_> = std::fs::read_dir(&wal_dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension() == Some(std::ffi::OsStr::new("seg")))
            .collect();

        assert!(segments_after.len() < segments_before.len());
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
        assert!(info.versions_removed >= 5);

        // Current version should still exist
        let current = db.kv_get(run_id, "versioned-key").unwrap().unwrap();
        assert_eq!(current.value, b"value-9");
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

### Complete Story

```bash
./scripts/complete-story.sh 529
```

---

## Story #530: Compaction API

**GitHub Issue**: [#530](https://github.com/anibjoshi/in-mem/issues/530)
**Estimated Time**: 2 hours
**Dependencies**: Stories #526, #527
**Blocks**: Story #529

### Start Story

```bash
gh issue view 530
./scripts/start-story.sh 74 530 compaction-api
```

### Implementation

Add to `crates/storage/src/database.rs`:

```rust
impl Database {
    /// Compact the database
    ///
    /// Reclaims disk space by removing WAL segments and old versions.
    ///
    /// # Modes
    /// - `WALOnly`: Safe mode, removes WAL segments covered by snapshot
    /// - `Full`: Removes WAL + applies retention policy
    ///
    /// # Important
    /// Compaction is:
    /// - **Deterministic**: Same input → same output
    /// - **User-triggered**: No background compaction
    /// - **Logically invisible**: Read results unchanged
    /// - **Version-preserving**: Version IDs never change
    pub fn compact(&self, mode: CompactMode) -> Result<CompactInfo, CompactionError> {
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
    pub fn should_compact(&self) -> Result<bool, StorageError> {
        let manifest = self.manifest.lock().unwrap();
        let snapshot_watermark = manifest.manifest().snapshot_watermark;
        drop(manifest);

        if snapshot_watermark.is_none() {
            return Ok(false);
        }

        let wal_size = self.wal_size()?;
        let snapshot_size = self.snapshot_size()?;

        Ok(wal_size > snapshot_size)
    }

    /// Get current WAL disk usage
    pub fn wal_size(&self) -> Result<u64, StorageError> {
        let wal_dir = self.paths.wal_dir.clone();
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
            let path = snapshot_path(&self.paths.snapshots_dir, id);
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

### Complete Story

```bash
./scripts/complete-story.sh 530
```

---

## Epic 74 Completion Checklist

### 1. Final Validation

```bash
~/.cargo/bin/cargo build --workspace
~/.cargo/bin/cargo test --workspace
~/.cargo/bin/cargo clippy --workspace -- -D warnings
~/.cargo/bin/cargo fmt --check
```

### 2. Verify Deliverables

- [ ] `CompactMode` enum with WALOnly, Full
- [ ] `WalOnlyCompactor` removes covered segments
- [ ] `FullCompactor` applies retention
- [ ] `TombstoneIndex` tracks deletions
- [ ] `compact()` API with CompactInfo
- [ ] Correctness verification tests

### 3. Run Epic-End Validation

See `docs/prompts/EPIC_END_VALIDATION.md`

### 4. Merge to Develop

```bash
git checkout develop
git merge --no-ff epic-74-compaction -m "Epic 74: Compaction complete

Delivered:
- CompactMode enum (WALOnly, Full)
- WAL-only compaction
- Full compaction with retention enforcement
- Tombstone management
- Compaction correctness verification

Stories: #525, #526, #527, #528, #529, #530
"
git push origin develop
gh issue close 525 --comment "Epic 74: Compaction - COMPLETE"
```

---

## Summary

Epic 74 establishes the compaction system:

- **CompactMode** defines compaction semantics
- **WAL-Only Compaction** safely removes covered segments
- **Full Compaction** applies retention policies
- **Tombstone Management** tracks deletions
- **Correctness Tests** verify invariants

This completes the storage lifecycle management foundation.
