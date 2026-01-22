//! WAL-only compaction
//!
//! Removes WAL segments that are fully covered by a snapshot watermark.
//! This is the safest compaction mode - it only removes data that is
//! guaranteed to be recoverable from the snapshot.
//!
//! # Algorithm
//!
//! 1. Get snapshot watermark (transaction ID) from MANIFEST
//! 2. List all WAL segments
//! 3. For each segment (except the active segment):
//!    - Read all records and find the highest txn_id
//!    - If highest txn_id <= watermark, segment is covered
//!    - Delete covered segments
//! 4. Track reclaimed bytes and segment count
//!
//! # Safety
//!
//! - Never removes the active segment
//! - Only removes segments fully covered by snapshot
//! - Requires a valid snapshot to exist

use crate::format::{ManifestManager, SegmentHeader, WalRecord, WalRecordError, SEGMENT_HEADER_SIZE};
use parking_lot::Mutex;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use super::{CompactInfo, CompactMode, CompactionError};

/// WAL-only compactor
///
/// Removes WAL segments covered by snapshot watermark.
pub struct WalOnlyCompactor {
    wal_dir: PathBuf,
    manifest: Arc<Mutex<ManifestManager>>,
}

impl WalOnlyCompactor {
    /// Create a new WAL-only compactor
    pub fn new(wal_dir: PathBuf, manifest: Arc<Mutex<ManifestManager>>) -> Self {
        WalOnlyCompactor { wal_dir, manifest }
    }

    /// Perform WAL-only compaction
    ///
    /// Removes WAL segments whose highest txn_id <= snapshot watermark.
    /// Returns information about what was compacted.
    ///
    /// # Errors
    ///
    /// - `NoSnapshot`: No snapshot exists to compact against
    /// - `Io`: File system errors during segment access/deletion
    pub fn compact(&self) -> Result<CompactInfo, CompactionError> {
        let start_time = std::time::Instant::now();
        let mut info = CompactInfo::new(CompactMode::WALOnly);

        // Get snapshot watermark from MANIFEST
        let (watermark, active_segment) = {
            let manifest = self.manifest.lock();

            let watermark = manifest
                .manifest()
                .snapshot_watermark
                .ok_or(CompactionError::NoSnapshot)?;

            let active_segment = manifest.manifest().active_wal_segment;

            (watermark, active_segment)
        };

        info.snapshot_watermark = Some(watermark);

        // List all WAL segments
        let segments = self.list_segments()?;

        for segment_number in segments {
            // Never remove active segment
            if segment_number >= active_segment {
                continue;
            }

            // Check if segment is fully covered by snapshot
            match self.segment_covered_by_watermark(segment_number, watermark) {
                Ok(true) => {
                    let segment_path = segment_path(&self.wal_dir, segment_number);

                    match std::fs::metadata(&segment_path) {
                        Ok(metadata) => {
                            let segment_size = metadata.len();

                            if let Err(e) = std::fs::remove_file(&segment_path) {
                                // Log but continue - partial compaction is acceptable
                                eprintln!(
                                    "Warning: failed to remove segment {}: {}",
                                    segment_number, e
                                );
                                continue;
                            }

                            info.reclaimed_bytes += segment_size;
                            info.wal_segments_removed += 1;
                        }
                        Err(e) => {
                            // Segment might have been removed by another process
                            eprintln!(
                                "Warning: failed to stat segment {}: {}",
                                segment_number, e
                            );
                        }
                    }
                }
                Ok(false) => {
                    // Segment has records beyond watermark, keep it
                }
                Err(e) => {
                    // Error reading segment - skip it for safety
                    eprintln!(
                        "Warning: failed to check segment {}: {}",
                        segment_number, e
                    );
                }
            }
        }

        info.duration_ms = start_time.elapsed().as_millis() as u64;
        info.timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_micros() as u64)
            .unwrap_or(0);

        Ok(info)
    }

    /// List all WAL segment numbers in sorted order
    fn list_segments(&self) -> Result<Vec<u64>, CompactionError> {
        let mut segments = Vec::new();

        if !self.wal_dir.exists() {
            return Ok(segments);
        }

        for entry in std::fs::read_dir(&self.wal_dir)? {
            let entry = entry?;
            let name = entry.file_name().to_string_lossy().to_string();

            // Match wal-NNNNNN.seg pattern
            if name.starts_with("wal-") && name.ends_with(".seg") && name.len() == 14 {
                if let Ok(num) = name[4..10].parse::<u64>() {
                    segments.push(num);
                }
            }
        }

        segments.sort_unstable();
        Ok(segments)
    }

    /// Check if a segment is fully covered by the snapshot watermark
    ///
    /// A segment is covered if its highest txn_id <= watermark.
    fn segment_covered_by_watermark(
        &self,
        segment_number: u64,
        watermark: u64,
    ) -> Result<bool, CompactionError> {
        let segment_path = segment_path(&self.wal_dir, segment_number);
        let file_data = std::fs::read(&segment_path)?;

        // Validate segment header
        if file_data.len() < SEGMENT_HEADER_SIZE {
            return Err(CompactionError::internal(format!(
                "Segment {} too small for header",
                segment_number
            )));
        }

        let header_bytes: [u8; SEGMENT_HEADER_SIZE] =
            file_data[..SEGMENT_HEADER_SIZE].try_into().map_err(|_| {
                CompactionError::internal(format!(
                    "Failed to read segment {} header",
                    segment_number
                ))
            })?;

        let header = SegmentHeader::from_bytes(&header_bytes).ok_or_else(|| {
            CompactionError::internal(format!("Invalid segment {} header", segment_number))
        })?;

        if !header.is_valid() {
            return Err(CompactionError::internal(format!(
                "Segment {} has invalid magic",
                segment_number
            )));
        }

        // Empty segment (just header) is considered covered
        if file_data.len() <= SEGMENT_HEADER_SIZE {
            return Ok(true);
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
                Err(WalRecordError::InsufficientData) => {
                    // Reached end of valid records (might have partial record at end)
                    break;
                }
                Err(WalRecordError::ChecksumMismatch { .. }) => {
                    // Corrupted record - stop reading
                    // The max_txn_id we have so far is from valid records
                    break;
                }
                Err(_) => {
                    // Other errors - stop reading
                    break;
                }
            }
        }

        // Segment is covered if all records are at or below watermark
        Ok(max_txn_id <= watermark)
    }

    /// Get the WAL directory path
    pub fn wal_dir(&self) -> &Path {
        &self.wal_dir
    }
}

/// Generate segment file path
///
/// Format: `wal-NNNNNN.seg` where NNNNNN is zero-padded segment number.
fn segment_path(dir: &Path, segment_number: u64) -> PathBuf {
    dir.join(format!("wal-{:06}.seg", segment_number))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::format::WalSegment;
    use tempfile::tempdir;

    fn test_uuid() -> [u8; 16] {
        [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16]
    }

    fn setup_test_env() -> (tempfile::TempDir, PathBuf, Arc<Mutex<ManifestManager>>) {
        let dir = tempdir().unwrap();
        let wal_dir = dir.path().join("WAL");
        std::fs::create_dir_all(&wal_dir).unwrap();

        let manifest_path = dir.path().join("MANIFEST");
        let manifest =
            ManifestManager::create(manifest_path, test_uuid(), "identity".to_string()).unwrap();

        (dir, wal_dir, Arc::new(Mutex::new(manifest)))
    }

    fn create_segment_with_records(
        wal_dir: &Path,
        segment_number: u64,
        txn_ids: &[u64],
    ) -> std::io::Result<()> {
        let mut segment = WalSegment::create(wal_dir, segment_number, test_uuid())?;

        for &txn_id in txn_ids {
            let record = WalRecord::new(txn_id, test_uuid(), txn_id * 1000, vec![txn_id as u8; 10]);
            segment.write(&record.to_bytes())?;
        }

        segment.close()?;
        Ok(())
    }

    #[test]
    fn test_segment_path_format() {
        let path = segment_path(Path::new("/tmp/wal"), 1);
        assert_eq!(path.to_str().unwrap(), "/tmp/wal/wal-000001.seg");

        let path = segment_path(Path::new("/tmp/wal"), 999999);
        assert_eq!(path.to_str().unwrap(), "/tmp/wal/wal-999999.seg");
    }

    #[test]
    fn test_list_segments_empty() {
        let (_dir, wal_dir, manifest) = setup_test_env();

        let compactor = WalOnlyCompactor::new(wal_dir, manifest);
        let segments = compactor.list_segments().unwrap();

        assert!(segments.is_empty());
    }

    #[test]
    fn test_list_segments() {
        let (_dir, wal_dir, manifest) = setup_test_env();

        // Create some segments
        create_segment_with_records(&wal_dir, 1, &[1, 2, 3]).unwrap();
        create_segment_with_records(&wal_dir, 3, &[4, 5, 6]).unwrap();
        create_segment_with_records(&wal_dir, 5, &[7, 8, 9]).unwrap();

        let compactor = WalOnlyCompactor::new(wal_dir, manifest);
        let segments = compactor.list_segments().unwrap();

        assert_eq!(segments, vec![1, 3, 5]);
    }

    #[test]
    fn test_compact_no_snapshot() {
        let (_dir, wal_dir, manifest) = setup_test_env();

        let compactor = WalOnlyCompactor::new(wal_dir, manifest);
        let result = compactor.compact();

        assert!(matches!(result, Err(CompactionError::NoSnapshot)));
    }

    #[test]
    fn test_compact_removes_covered_segments() {
        let (_dir, wal_dir, manifest) = setup_test_env();

        // Create segments with increasing txn_ids
        create_segment_with_records(&wal_dir, 1, &[1, 2, 3]).unwrap();
        create_segment_with_records(&wal_dir, 2, &[4, 5, 6]).unwrap();
        create_segment_with_records(&wal_dir, 3, &[7, 8, 9]).unwrap();

        // Set snapshot watermark at txn 6 and active segment at 4
        {
            let mut m = manifest.lock();
            m.set_snapshot_watermark(1, 6).unwrap();
            m.manifest_mut().active_wal_segment = 4;
            m.persist().unwrap();
        }

        let compactor = WalOnlyCompactor::new(wal_dir.clone(), manifest);
        let info = compactor.compact().unwrap();

        // Segments 1 and 2 should be removed (max txn 3 and 6 <= watermark 6)
        // Segment 3 should remain (max txn 9 > watermark 6)
        assert_eq!(info.wal_segments_removed, 2);
        assert!(info.reclaimed_bytes > 0);
        assert_eq!(info.snapshot_watermark, Some(6));

        // Verify files
        assert!(!segment_path(&wal_dir, 1).exists());
        assert!(!segment_path(&wal_dir, 2).exists());
        assert!(segment_path(&wal_dir, 3).exists());
    }

    #[test]
    fn test_compact_never_removes_active_segment() {
        let (_dir, wal_dir, manifest) = setup_test_env();

        // Create a segment
        create_segment_with_records(&wal_dir, 1, &[1, 2, 3]).unwrap();

        // Set watermark high but active segment is 1
        {
            let mut m = manifest.lock();
            m.set_snapshot_watermark(1, 100).unwrap();
            m.manifest_mut().active_wal_segment = 1; // Segment 1 is active
            m.persist().unwrap();
        }

        let compactor = WalOnlyCompactor::new(wal_dir.clone(), manifest);
        let info = compactor.compact().unwrap();

        // Should not remove active segment
        assert_eq!(info.wal_segments_removed, 0);
        assert!(segment_path(&wal_dir, 1).exists());
    }

    #[test]
    fn test_compact_empty_wal() {
        let (_dir, wal_dir, manifest) = setup_test_env();

        // Set snapshot watermark but no segments
        {
            let mut m = manifest.lock();
            m.set_snapshot_watermark(1, 100).unwrap();
            m.persist().unwrap();
        }

        let compactor = WalOnlyCompactor::new(wal_dir, manifest);
        let info = compactor.compact().unwrap();

        assert_eq!(info.wal_segments_removed, 0);
        assert_eq!(info.reclaimed_bytes, 0);
    }

    #[test]
    fn test_compact_empty_segment() {
        let (_dir, wal_dir, manifest) = setup_test_env();

        // Create an empty segment (just header)
        let segment = WalSegment::create(&wal_dir, 1, test_uuid()).unwrap();
        drop(segment);

        // Create another with records
        create_segment_with_records(&wal_dir, 2, &[1, 2, 3]).unwrap();

        // Set watermark and active segment
        {
            let mut m = manifest.lock();
            m.set_snapshot_watermark(1, 5).unwrap();
            m.manifest_mut().active_wal_segment = 10;
            m.persist().unwrap();
        }

        let compactor = WalOnlyCompactor::new(wal_dir.clone(), manifest);
        let info = compactor.compact().unwrap();

        // Both segments should be removed (empty segment is always covered)
        assert_eq!(info.wal_segments_removed, 2);
    }

    #[test]
    fn test_segment_covered_by_watermark() {
        let (_dir, wal_dir, manifest) = setup_test_env();

        create_segment_with_records(&wal_dir, 1, &[1, 2, 3]).unwrap();

        let compactor = WalOnlyCompactor::new(wal_dir, manifest);

        // Watermark 3 should cover segment with max txn 3
        assert!(compactor.segment_covered_by_watermark(1, 3).unwrap());

        // Watermark 2 should not cover segment with max txn 3
        assert!(!compactor.segment_covered_by_watermark(1, 2).unwrap());

        // Watermark 10 should cover segment
        assert!(compactor.segment_covered_by_watermark(1, 10).unwrap());
    }

    #[test]
    fn test_compact_info_metrics() {
        let (_dir, wal_dir, manifest) = setup_test_env();

        create_segment_with_records(&wal_dir, 1, &[1, 2, 3]).unwrap();
        create_segment_with_records(&wal_dir, 2, &[4, 5, 6]).unwrap();

        {
            let mut m = manifest.lock();
            m.set_snapshot_watermark(1, 10).unwrap();
            m.manifest_mut().active_wal_segment = 10;
            m.persist().unwrap();
        }

        let compactor = WalOnlyCompactor::new(wal_dir, manifest);
        let info = compactor.compact().unwrap();

        assert_eq!(info.mode, CompactMode::WALOnly);
        assert_eq!(info.wal_segments_removed, 2);
        assert!(info.reclaimed_bytes > 0);
        assert!(info.duration_ms < 10000); // Should complete in reasonable time
        assert!(info.timestamp > 0);
    }
}
