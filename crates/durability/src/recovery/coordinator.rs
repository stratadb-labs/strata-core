//! Recovery coordinator
//!
//! Recovery algorithm:
//! 1. Load MANIFEST
//! 2. If snapshot exists: load snapshot -> replay WAL > watermark
//! 3. If no snapshot: replay all WAL
//! 4. Truncate partial records at WAL tail
//!
//! # Recovery Properties
//!
//! - **Deterministic**: Same inputs -> same state
//! - **Idempotent**: Multiple recoveries -> same result
//! - **Atomic**: Either fully recovers or fails cleanly
//!
//! # Example
//!
//! ```ignore
//! let recovery = RecoveryCoordinator::new(db_dir, codec);
//! let result = recovery.recover()?;
//! println!("Recovered {} records from WAL", result.replay_stats.records_applied);
//! ```

use std::path::{Path, PathBuf};

use crate::codec::{CodecError, StorageCodec};
use crate::disk_snapshot::{SnapshotReadError, SnapshotReader};
use crate::format::manifest::{Manifest, ManifestError, ManifestManager};
use crate::format::{snapshot_path, WalRecord};
use crate::wal::WalReaderError;

use super::replayer::{ReplayStats, WalReplayError, WalReplayer};

/// Recovery coordinator
///
/// Coordinates recovery from MANIFEST, snapshot, and WAL.
pub struct RecoveryCoordinator {
    db_dir: PathBuf,
    codec: Box<dyn StorageCodec>,
}

impl RecoveryCoordinator {
    /// Create a new recovery coordinator
    pub fn new(db_dir: PathBuf, codec: Box<dyn StorageCodec>) -> Self {
        RecoveryCoordinator { db_dir, codec }
    }

    /// Get the database directory
    pub fn db_dir(&self) -> &Path {
        &self.db_dir
    }

    /// Get the MANIFEST path
    pub fn manifest_path(&self) -> PathBuf {
        self.db_dir.join("MANIFEST")
    }

    /// Get the WAL directory
    pub fn wal_dir(&self) -> PathBuf {
        self.db_dir.join("WAL")
    }

    /// Get the snapshots directory
    pub fn snapshots_dir(&self) -> PathBuf {
        self.db_dir.join("SNAPSHOTS")
    }

    /// Check if recovery is needed (MANIFEST exists)
    pub fn needs_recovery(&self) -> bool {
        self.manifest_path().exists()
    }

    /// Load the MANIFEST
    pub fn load_manifest(&self) -> Result<ManifestManager, RecoveryError> {
        ManifestManager::load(self.manifest_path()).map_err(RecoveryError::from)
    }

    /// Perform recovery, returning information needed to reconstruct state
    ///
    /// This method:
    /// 1. Loads the MANIFEST
    /// 2. Validates the codec
    /// 3. Determines the recovery path (snapshot + WAL or WAL only)
    /// 4. Returns recovery info for the caller to apply
    ///
    /// Note: This does NOT apply the records - the caller must do that
    /// using the returned RecoveryPlan.
    pub fn plan_recovery(&self) -> Result<RecoveryPlan, RecoveryError> {
        // Step 1: Load MANIFEST
        let manifest_manager = self.load_manifest()?;
        let manifest = manifest_manager.manifest().clone();

        // Step 2: Validate codec
        if manifest.codec_id != self.codec.codec_id() {
            return Err(RecoveryError::CodecMismatch {
                expected: manifest.codec_id.clone(),
                actual: self.codec.codec_id().to_string(),
            });
        }

        // Step 3: Determine recovery path
        let (snapshot_path, watermark) = if let Some(snapshot_id) = manifest.snapshot_id {
            let path = snapshot_path(&self.snapshots_dir(), snapshot_id);
            let watermark = manifest.snapshot_watermark;
            (Some(path), watermark)
        } else {
            (None, None)
        };

        Ok(RecoveryPlan {
            manifest,
            snapshot_path,
            wal_dir: self.wal_dir(),
            watermark,
        })
    }

    /// Execute recovery with a callback for each WAL record
    ///
    /// This method:
    /// 1. Plans recovery
    /// 2. If snapshot exists, loads it and calls the snapshot callback
    /// 3. Replays WAL records after watermark, calling the record callback
    /// 4. Truncates partial records
    ///
    /// Returns recovery result with statistics.
    pub fn recover<S, R>(
        &self,
        mut on_snapshot: S,
        mut on_record: R,
    ) -> Result<RecoveryResult, RecoveryError>
    where
        S: FnMut(RecoverySnapshot) -> Result<(), RecoveryError>,
        R: FnMut(&WalRecord) -> Result<(), RecoveryError>,
    {
        let plan = self.plan_recovery()?;

        // Load snapshot if exists
        if let Some(snapshot_path) = &plan.snapshot_path {
            let snapshot_reader = SnapshotReader::new(clone_codec(self.codec.as_ref())?);
            let loaded = snapshot_reader.load(snapshot_path)?;

            on_snapshot(RecoverySnapshot {
                snapshot_id: loaded.header.snapshot_id,
                watermark_txn: loaded.header.watermark_txn,
                sections: loaded.sections,
            })?;
        }

        // Replay WAL
        let replayer = WalReplayer::new(plan.wal_dir.clone(), clone_codec(self.codec.as_ref())?);
        let replay_stats = replayer.replay_after(plan.watermark, |record| {
            on_record(record).map_err(|e| WalReplayError::Apply(e.to_string()))
        })?;

        // Truncate partial records
        let truncated = self.truncate_partial_records(&plan.wal_dir)?;

        Ok(RecoveryResult {
            manifest: plan.manifest,
            snapshot_watermark: plan.watermark,
            replay_stats,
            bytes_truncated: truncated,
        })
    }

    /// Truncate partial WAL records at the tail of the active segment
    ///
    /// This is safe because:
    /// - Partial records mean the transaction wasn't committed
    /// - In Always mode, committed transactions are fsynced
    /// - In Standard mode, some data loss is expected on crash
    pub fn truncate_partial_records(&self, wal_dir: &Path) -> Result<u64, RecoveryError> {
        let reader = crate::wal::WalReader::new(clone_codec(self.codec.as_ref())?);

        // Get all segments
        let segments = reader.list_segments(wal_dir)?;

        // Only truncate the last (active) segment
        let last_segment = match segments.last() {
            Some(&segment) => segment,
            None => return Ok(0),
        };
        let result = reader.read_all(wal_dir)?;

        if let Some(truncate_info) = result.truncate_info {
            if truncate_info.segment_number == last_segment {
                let segment_path =
                    crate::format::WalSegment::segment_path(wal_dir, truncate_info.segment_number);

                // Truncate the file
                let file = std::fs::OpenOptions::new()
                    .write(true)
                    .open(&segment_path)?;
                file.set_len(truncate_info.valid_end)?;
                file.sync_all()?;

                return Ok(truncate_info.bytes_to_truncate());
            }
        }

        Ok(0)
    }
}

/// Recovery plan - information needed to reconstruct state
#[derive(Debug, Clone)]
pub struct RecoveryPlan {
    /// MANIFEST data
    pub manifest: Manifest,
    /// Path to snapshot file (if exists)
    pub snapshot_path: Option<PathBuf>,
    /// WAL directory
    pub wal_dir: PathBuf,
    /// Watermark for WAL replay (skip records <= this)
    pub watermark: Option<u64>,
}

/// Snapshot data for recovery
pub struct RecoverySnapshot {
    /// Snapshot ID
    pub snapshot_id: u64,
    /// Watermark transaction ID
    pub watermark_txn: u64,
    /// Snapshot sections (primitive data)
    pub sections: Vec<crate::disk_snapshot::LoadedSection>,
}

/// Result of recovery
#[derive(Debug)]
pub struct RecoveryResult {
    /// MANIFEST data
    pub manifest: Manifest,
    /// Snapshot watermark (if snapshot was loaded)
    pub snapshot_watermark: Option<u64>,
    /// WAL replay statistics
    pub replay_stats: ReplayStats,
    /// Bytes truncated from partial records
    pub bytes_truncated: u64,
}

/// Recovery errors
#[derive(Debug, thiserror::Error)]
pub enum RecoveryError {
    /// MANIFEST error
    #[error("MANIFEST error: {0}")]
    Manifest(#[from] ManifestError),

    /// Snapshot error
    #[error("Snapshot error: {0}")]
    Snapshot(#[from] SnapshotReadError),

    /// WAL replay error
    #[error("WAL replay error: {0}")]
    Replay(#[from] WalReplayError),

    /// WAL reader error
    #[error("WAL reader error: {0}")]
    WalReader(#[from] WalReaderError),

    /// Codec mismatch
    #[error("Codec mismatch: expected {expected}, got {actual}")]
    CodecMismatch {
        /// Expected codec ID from MANIFEST
        expected: String,
        /// Actual codec ID provided
        actual: String,
    },

    /// IO error
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// Codec error
    #[error("Codec error: {0}")]
    Codec(#[from] CodecError),

    /// Apply error (from callbacks)
    #[error("Apply error: {0}")]
    Apply(String),
}

impl RecoveryError {
    /// Create an apply error
    pub fn apply(msg: impl Into<String>) -> Self {
        RecoveryError::Apply(msg.into())
    }
}

/// Helper to clone a boxed codec
fn clone_codec(codec: &dyn StorageCodec) -> Result<Box<dyn StorageCodec>, CodecError> {
    crate::codec::get_codec(codec.codec_id())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codec::IdentityCodec;
    use crate::disk_snapshot::{SnapshotSection, SnapshotWriter};
    use crate::format::{primitive_tags, WalRecord};
    use crate::wal::{DurabilityMode, WalConfig, WalWriter};
    use tempfile::tempdir;

    fn make_codec() -> Box<dyn StorageCodec> {
        Box::new(IdentityCodec)
    }

    fn test_uuid() -> [u8; 16] {
        [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16]
    }

    fn setup_manifest(db_dir: &Path) {
        ManifestManager::create(db_dir.join("MANIFEST"), test_uuid(), "identity".to_string())
            .unwrap();
    }

    fn setup_wal(db_dir: &Path, records: &[WalRecord]) {
        let wal_dir = db_dir.join("WAL");
        let mut writer = WalWriter::new(
            wal_dir,
            test_uuid(),
            DurabilityMode::Always,
            WalConfig::for_testing(),
            make_codec(),
        )
        .unwrap();

        for record in records {
            writer.append(record).unwrap();
        }
        writer.flush().unwrap();
    }

    fn setup_snapshot(db_dir: &Path, snapshot_id: u64, watermark: u64) {
        let snap_dir = db_dir.join("SNAPSHOTS");
        std::fs::create_dir_all(&snap_dir).unwrap();

        let writer = SnapshotWriter::new(snap_dir, make_codec(), test_uuid()).unwrap();

        // Create simple snapshot with KV section
        let sections = vec![SnapshotSection::new(primitive_tags::KV, vec![0u8; 4])];
        writer
            .create_snapshot(snapshot_id, watermark, sections)
            .unwrap();
    }

    #[test]
    fn test_needs_recovery() {
        let dir = tempdir().unwrap();
        let db_dir = dir.path().to_path_buf();

        let coordinator = RecoveryCoordinator::new(db_dir.clone(), make_codec());
        assert!(!coordinator.needs_recovery());

        setup_manifest(&db_dir);
        assert!(coordinator.needs_recovery());
    }

    #[test]
    fn test_load_manifest() {
        let dir = tempdir().unwrap();
        let db_dir = dir.path().to_path_buf();

        setup_manifest(&db_dir);

        let coordinator = RecoveryCoordinator::new(db_dir, make_codec());
        let manager = coordinator.load_manifest().unwrap();

        assert_eq!(manager.manifest().codec_id, "identity");
        assert_eq!(manager.manifest().database_uuid, test_uuid());
    }

    #[test]
    fn test_plan_recovery_wal_only() {
        let dir = tempdir().unwrap();
        let db_dir = dir.path().to_path_buf();

        setup_manifest(&db_dir);

        let coordinator = RecoveryCoordinator::new(db_dir, make_codec());
        let plan = coordinator.plan_recovery().unwrap();

        assert!(plan.snapshot_path.is_none());
        assert!(plan.watermark.is_none());
    }

    #[test]
    fn test_plan_recovery_with_snapshot() {
        let dir = tempdir().unwrap();
        let db_dir = dir.path().to_path_buf();

        // Create manifest with snapshot info
        let mut manager =
            ManifestManager::create(db_dir.join("MANIFEST"), test_uuid(), "identity".to_string())
                .unwrap();
        manager.set_snapshot_watermark(1, 100).unwrap();

        setup_snapshot(&db_dir, 1, 100);

        let coordinator = RecoveryCoordinator::new(db_dir, make_codec());
        let plan = coordinator.plan_recovery().unwrap();

        assert!(plan.snapshot_path.is_some());
        assert_eq!(plan.watermark, Some(100));
    }

    #[test]
    fn test_codec_mismatch() {
        let dir = tempdir().unwrap();
        let db_dir = dir.path().to_path_buf();

        // Create manifest with different codec
        ManifestManager::create(
            db_dir.join("MANIFEST"),
            test_uuid(),
            "aes256".to_string(), // Different codec
        )
        .unwrap();

        let coordinator = RecoveryCoordinator::new(db_dir, make_codec()); // identity codec
        let result = coordinator.plan_recovery();

        assert!(matches!(result, Err(RecoveryError::CodecMismatch { .. })));
    }

    #[test]
    fn test_recover_wal_only() {
        let dir = tempdir().unwrap();
        let db_dir = dir.path().to_path_buf();

        setup_manifest(&db_dir);

        let records: Vec<_> = (1..=5)
            .map(|i| WalRecord::new(i, test_uuid(), i * 1000, vec![i as u8]))
            .collect();
        setup_wal(&db_dir, &records);

        let coordinator = RecoveryCoordinator::new(db_dir, make_codec());
        let mut applied = Vec::new();

        let result = coordinator
            .recover(
                |_snapshot| {
                    panic!("Should not have snapshot");
                },
                |record| {
                    applied.push(record.txn_id);
                    Ok(())
                },
            )
            .unwrap();

        assert_eq!(result.replay_stats.records_applied, 5);
        assert_eq!(applied, vec![1, 2, 3, 4, 5]);
        assert!(result.snapshot_watermark.is_none());
    }

    #[test]
    fn test_recover_with_snapshot() {
        let dir = tempdir().unwrap();
        let db_dir = dir.path().to_path_buf();

        // Setup manifest with snapshot
        let mut manager =
            ManifestManager::create(db_dir.join("MANIFEST"), test_uuid(), "identity".to_string())
                .unwrap();
        manager.set_snapshot_watermark(1, 50).unwrap();

        // Create snapshot
        setup_snapshot(&db_dir, 1, 50);

        // Create WAL with records before and after watermark
        let records: Vec<_> = (1..=100)
            .map(|i| WalRecord::new(i, test_uuid(), i * 1000, vec![i as u8]))
            .collect();
        setup_wal(&db_dir, &records);

        let coordinator = RecoveryCoordinator::new(db_dir, make_codec());
        let mut snapshot_loaded = false;
        let mut applied = Vec::new();

        let result = coordinator
            .recover(
                |snapshot| {
                    snapshot_loaded = true;
                    assert_eq!(snapshot.snapshot_id, 1);
                    assert_eq!(snapshot.watermark_txn, 50);
                    Ok(())
                },
                |record| {
                    applied.push(record.txn_id);
                    Ok(())
                },
            )
            .unwrap();

        assert!(snapshot_loaded);
        assert_eq!(result.snapshot_watermark, Some(50));
        assert_eq!(result.replay_stats.records_skipped, 50);
        assert_eq!(result.replay_stats.records_applied, 50);
        assert!(applied.iter().all(|&id| id > 50));
    }

    #[test]
    fn test_truncate_partial_records() {
        let dir = tempdir().unwrap();
        let db_dir = dir.path().to_path_buf();

        setup_manifest(&db_dir);

        let records: Vec<_> = (1..=3)
            .map(|i| WalRecord::new(i, test_uuid(), 0, vec![i as u8]))
            .collect();
        setup_wal(&db_dir, &records);

        // Append garbage to WAL
        let wal_dir = db_dir.join("WAL");
        let segment_path = crate::format::WalSegment::segment_path(&wal_dir, 1);
        use std::io::Write;
        let mut file = std::fs::OpenOptions::new()
            .append(true)
            .open(&segment_path)
            .unwrap();
        file.write_all(&[0xFF; 50]).unwrap();
        drop(file);

        let coordinator = RecoveryCoordinator::new(db_dir, make_codec());
        let truncated = coordinator.truncate_partial_records(&wal_dir).unwrap();

        assert_eq!(truncated, 50);

        // Verify file was actually truncated
        let result = crate::wal::WalReader::new(make_codec())
            .read_all(&wal_dir)
            .unwrap();
        assert!(result.truncate_info.is_none()); // No more truncation needed
        assert_eq!(result.records.len(), 3);
    }

    #[test]
    fn test_recovery_idempotent() {
        let dir = tempdir().unwrap();
        let db_dir = dir.path().to_path_buf();

        setup_manifest(&db_dir);

        let records: Vec<_> = (1..=5)
            .map(|i| WalRecord::new(i, test_uuid(), 0, vec![i as u8]))
            .collect();
        setup_wal(&db_dir, &records);

        // Recover multiple times
        let mut results = Vec::new();
        for _ in 0..3 {
            let coordinator = RecoveryCoordinator::new(db_dir.clone(), make_codec());
            let mut applied = Vec::new();

            let result = coordinator
                .recover(
                    |_| Ok(()),
                    |record| {
                        applied.push(record.txn_id);
                        Ok(())
                    },
                )
                .unwrap();

            results.push((result.replay_stats.records_applied, applied));
        }

        // All recoveries should produce same results
        assert!(results.windows(2).all(|w| w[0] == w[1]));
    }

    #[test]
    fn test_recover_corrupted_snapshot_crc_mismatch() {
        let dir = tempdir().unwrap();
        let db_dir = dir.path().to_path_buf();

        // Setup manifest with snapshot reference
        let mut manager =
            ManifestManager::create(db_dir.join("MANIFEST"), test_uuid(), "identity".to_string())
                .unwrap();
        manager.set_snapshot_watermark(1, 50).unwrap();

        // Create valid snapshot first
        setup_snapshot(&db_dir, 1, 50);

        // Now corrupt the snapshot by modifying section data (not header/codec)
        let snap_path = crate::format::snapshot_path(&db_dir.join("SNAPSHOTS"), 1);
        let mut data = std::fs::read(&snap_path).unwrap();
        // Corrupt data in the section area (after header and codec ID)
        if data.len() > 82 {
            data[82] ^= 0xFF;
        }
        std::fs::write(&snap_path, &data).unwrap();

        // Setup WAL with some records
        let records: Vec<_> = (51..=60)
            .map(|i| WalRecord::new(i, test_uuid(), i * 1000, vec![i as u8]))
            .collect();
        setup_wal(&db_dir, &records);

        // Try to recover - should fail with Snapshot error
        let coordinator = RecoveryCoordinator::new(db_dir, make_codec());
        let result = coordinator.recover(|_| Ok(()), |_| Ok(()));

        assert!(
            matches!(result, Err(RecoveryError::Snapshot(_))),
            "Expected Snapshot error for corrupted snapshot, got: {:?}",
            result
        );

        // Verify the underlying error is CRC mismatch
        if let Err(RecoveryError::Snapshot(snapshot_err)) = result {
            let msg = snapshot_err.to_string();
            assert!(
                msg.contains("CRC") || msg.contains("crc"),
                "Expected CRC error message, got: {}",
                msg
            );
        }
    }

    #[test]
    fn test_recover_missing_snapshot_file() {
        let dir = tempdir().unwrap();
        let db_dir = dir.path().to_path_buf();

        // Setup manifest with snapshot reference (but don't create the snapshot)
        let mut manager =
            ManifestManager::create(db_dir.join("MANIFEST"), test_uuid(), "identity".to_string())
                .unwrap();
        manager.set_snapshot_watermark(1, 50).unwrap();

        // Create SNAPSHOTS directory but don't put the snapshot file there
        std::fs::create_dir_all(db_dir.join("SNAPSHOTS")).unwrap();

        // Setup WAL
        let records: Vec<_> = (51..=60)
            .map(|i| WalRecord::new(i, test_uuid(), i * 1000, vec![i as u8]))
            .collect();
        setup_wal(&db_dir, &records);

        // Try to recover - should fail with Snapshot error (file not found)
        let coordinator = RecoveryCoordinator::new(db_dir, make_codec());
        let result = coordinator.recover(|_| Ok(()), |_| Ok(()));

        assert!(
            matches!(result, Err(RecoveryError::Snapshot(_))),
            "Expected Snapshot error for missing snapshot file, got: {:?}",
            result
        );
    }

    #[test]
    fn test_recover_corrupted_snapshot_invalid_magic() {
        let dir = tempdir().unwrap();
        let db_dir = dir.path().to_path_buf();

        // Setup manifest with snapshot reference
        let mut manager =
            ManifestManager::create(db_dir.join("MANIFEST"), test_uuid(), "identity".to_string())
                .unwrap();
        manager.set_snapshot_watermark(1, 50).unwrap();

        // Create valid snapshot first
        setup_snapshot(&db_dir, 1, 50);

        // Corrupt the magic bytes at the beginning
        let snap_path = crate::format::snapshot_path(&db_dir.join("SNAPSHOTS"), 1);
        let mut data = std::fs::read(&snap_path).unwrap();
        data[0..4].copy_from_slice(b"BADM"); // Invalid magic
        std::fs::write(&snap_path, &data).unwrap();

        // Setup WAL
        let records: Vec<_> = (51..=60)
            .map(|i| WalRecord::new(i, test_uuid(), i * 1000, vec![i as u8]))
            .collect();
        setup_wal(&db_dir, &records);

        // Try to recover - should fail with Snapshot error
        let coordinator = RecoveryCoordinator::new(db_dir, make_codec());
        let result = coordinator.recover(|_| Ok(()), |_| Ok(()));

        assert!(
            matches!(result, Err(RecoveryError::Snapshot(_))),
            "Expected Snapshot error for invalid magic, got: {:?}",
            result
        );

        // Verify the underlying error mentions magic
        if let Err(RecoveryError::Snapshot(snapshot_err)) = result {
            let msg = snapshot_err.to_string();
            assert!(
                msg.contains("magic") || msg.contains("Magic"),
                "Expected magic-related error message, got: {}",
                msg
            );
        }
    }

    #[test]
    fn test_recover_callback_error_propagated() {
        let dir = tempdir().unwrap();
        let db_dir = dir.path().to_path_buf();

        setup_manifest(&db_dir);

        let records: Vec<_> = (1..=5)
            .map(|i| WalRecord::new(i, test_uuid(), i * 1000, vec![i as u8]))
            .collect();
        setup_wal(&db_dir, &records);

        // Recovery with callback that returns error
        let coordinator = RecoveryCoordinator::new(db_dir, make_codec());
        let result = coordinator.recover(
            |_| Ok(()),
            |record| {
                if record.txn_id == 3 {
                    Err(RecoveryError::apply("simulated failure at txn 3"))
                } else {
                    Ok(())
                }
            },
        );

        assert!(
            matches!(result, Err(RecoveryError::Replay(_))),
            "Expected Replay error when callback fails, got: {:?}",
            result
        );
    }
}
