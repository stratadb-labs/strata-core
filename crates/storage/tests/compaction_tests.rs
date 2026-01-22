//! Compaction correctness tests
//!
//! These tests verify that compaction:
//! 1. Correctly identifies and removes covered WAL segments
//! 2. Preserves data integrity after compaction
//! 3. Never removes the active segment
//! 4. Handles edge cases (empty WAL, no snapshot, etc.)

use strata_storage::compaction::{CompactInfo, CompactMode, CompactionError, WalOnlyCompactor};
use strata_storage::database::{DatabaseConfig, DatabaseHandle};
use strata_storage::format::{ManifestManager, WalRecord, WalSegment};
use parking_lot::Mutex;
use std::sync::Arc;
use tempfile::tempdir;

/// Helper function to create a test UUID
fn test_uuid() -> [u8; 16] {
    [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16]
}

/// Helper function to set up a test environment
fn setup_test_env() -> (tempfile::TempDir, std::path::PathBuf, Arc<Mutex<ManifestManager>>) {
    let dir = tempdir().unwrap();
    let wal_dir = dir.path().join("WAL");
    std::fs::create_dir_all(&wal_dir).unwrap();

    let manifest_path = dir.path().join("MANIFEST");
    let manifest =
        ManifestManager::create(manifest_path, test_uuid(), "identity".to_string()).unwrap();

    (dir, wal_dir, Arc::new(Mutex::new(manifest)))
}

/// Helper function to create a segment with specific transaction records
fn create_segment_with_records(
    wal_dir: &std::path::Path,
    segment_number: u64,
    txn_ids: &[u64],
) -> std::io::Result<u64> {
    let mut segment = WalSegment::create(wal_dir, segment_number, test_uuid())?;
    let mut bytes_written = 0u64;

    for &txn_id in txn_ids {
        let record = WalRecord::new(txn_id, test_uuid(), txn_id * 1000, vec![txn_id as u8; 50]);
        let record_bytes = record.to_bytes();
        bytes_written += record_bytes.len() as u64;
        segment.write(&record_bytes)?;
    }

    segment.close()?;
    Ok(bytes_written)
}

/// Helper to count segments in WAL directory
fn count_segments(wal_dir: &std::path::Path) -> usize {
    std::fs::read_dir(wal_dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.file_name()
                .to_str()
                .is_some_and(|n| n.starts_with("wal-") && n.ends_with(".seg"))
        })
        .count()
}

// === Basic Compaction Tests ===

#[test]
fn test_compact_mode_properties() {
    // WALOnly mode should not apply retention
    assert!(!CompactMode::WALOnly.applies_retention());
    assert_eq!(CompactMode::WALOnly.name(), "wal_only");

    // Full mode should apply retention
    assert!(CompactMode::Full.applies_retention());
    assert_eq!(CompactMode::Full.name(), "full");
}

#[test]
fn test_compact_info_tracks_metrics() {
    let mut info = CompactInfo::new(CompactMode::WALOnly);

    assert!(!info.did_compact());

    info.wal_segments_removed = 3;
    info.reclaimed_bytes = 1024;
    info.duration_ms = 50;

    assert!(info.did_compact());
    assert!(info.summary().contains("segments_removed=3"));
    assert!(info.summary().contains("bytes_reclaimed=1024"));
}

#[test]
fn test_compaction_requires_snapshot() {
    let (_dir, wal_dir, manifest) = setup_test_env();

    // Create some segments
    create_segment_with_records(&wal_dir, 1, &[1, 2, 3]).unwrap();

    // No snapshot exists, compaction should fail
    let compactor = WalOnlyCompactor::new(wal_dir, manifest);
    let result = compactor.compact();

    assert!(matches!(result, Err(CompactionError::NoSnapshot)));
}

#[test]
fn test_compaction_removes_covered_segments() {
    let (_dir, wal_dir, manifest) = setup_test_env();

    // Create segments with sequential transaction IDs
    create_segment_with_records(&wal_dir, 1, &[1, 2, 3]).unwrap();
    create_segment_with_records(&wal_dir, 2, &[4, 5, 6]).unwrap();
    create_segment_with_records(&wal_dir, 3, &[7, 8, 9]).unwrap();
    create_segment_with_records(&wal_dir, 4, &[10, 11, 12]).unwrap();

    assert_eq!(count_segments(&wal_dir), 4);

    // Set snapshot watermark at txn 6 and active segment at 5
    {
        let mut m = manifest.lock();
        m.set_snapshot_watermark(1, 6).unwrap();
        m.manifest_mut().active_wal_segment = 5;
        m.persist().unwrap();
    }

    let compactor = WalOnlyCompactor::new(wal_dir.clone(), manifest);
    let info = compactor.compact().unwrap();

    // Segments 1, 2 should be removed (max txn 3, 6 <= watermark 6)
    // Segments 3, 4 should remain (max txn 9, 12 > watermark 6)
    assert_eq!(info.wal_segments_removed, 2);
    assert!(info.reclaimed_bytes > 0);
    assert_eq!(count_segments(&wal_dir), 2);
}

#[test]
fn test_compaction_never_removes_active_segment() {
    let (_dir, wal_dir, manifest) = setup_test_env();

    create_segment_with_records(&wal_dir, 1, &[1, 2, 3]).unwrap();

    // Set watermark high but segment 1 is active
    {
        let mut m = manifest.lock();
        m.set_snapshot_watermark(1, 100).unwrap();
        m.manifest_mut().active_wal_segment = 1;
        m.persist().unwrap();
    }

    let compactor = WalOnlyCompactor::new(wal_dir.clone(), manifest);
    let info = compactor.compact().unwrap();

    // Active segment should not be removed
    assert_eq!(info.wal_segments_removed, 0);
    assert_eq!(count_segments(&wal_dir), 1);
}

#[test]
fn test_compaction_handles_empty_wal() {
    let (_dir, wal_dir, manifest) = setup_test_env();

    // No segments created

    // Set snapshot watermark
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
fn test_compaction_handles_empty_segment() {
    let (_dir, wal_dir, manifest) = setup_test_env();

    // Create an empty segment (just header, no records)
    let segment = WalSegment::create(&wal_dir, 1, test_uuid()).unwrap();
    drop(segment);

    // Create a segment with records
    create_segment_with_records(&wal_dir, 2, &[1, 2, 3]).unwrap();

    {
        let mut m = manifest.lock();
        m.set_snapshot_watermark(1, 10).unwrap();
        m.manifest_mut().active_wal_segment = 10;
        m.persist().unwrap();
    }

    let compactor = WalOnlyCompactor::new(wal_dir.clone(), manifest);
    let info = compactor.compact().unwrap();

    // Both should be removed (empty segment is always covered)
    assert_eq!(info.wal_segments_removed, 2);
    assert_eq!(count_segments(&wal_dir), 0);
}

#[test]
fn test_compaction_preserves_uncovered_segments() {
    let (_dir, wal_dir, manifest) = setup_test_env();

    // Create segments where some are above watermark
    create_segment_with_records(&wal_dir, 1, &[1, 2, 3]).unwrap();
    create_segment_with_records(&wal_dir, 2, &[4, 5, 100]).unwrap(); // txn 100 > watermark

    {
        let mut m = manifest.lock();
        m.set_snapshot_watermark(1, 50).unwrap();
        m.manifest_mut().active_wal_segment = 10;
        m.persist().unwrap();
    }

    let compactor = WalOnlyCompactor::new(wal_dir.clone(), manifest);
    let info = compactor.compact().unwrap();

    // Only segment 1 should be removed
    assert_eq!(info.wal_segments_removed, 1);
    assert_eq!(count_segments(&wal_dir), 1);
}

#[test]
fn test_compaction_reports_correct_watermark() {
    let (_dir, wal_dir, manifest) = setup_test_env();

    create_segment_with_records(&wal_dir, 1, &[1, 2, 3]).unwrap();

    let watermark_value = 42u64;
    {
        let mut m = manifest.lock();
        m.set_snapshot_watermark(1, watermark_value).unwrap();
        m.manifest_mut().active_wal_segment = 10;
        m.persist().unwrap();
    }

    let compactor = WalOnlyCompactor::new(wal_dir, manifest);
    let info = compactor.compact().unwrap();

    assert_eq!(info.snapshot_watermark, Some(watermark_value));
}

#[test]
fn test_compaction_records_timestamp() {
    let (_dir, wal_dir, manifest) = setup_test_env();

    {
        let mut m = manifest.lock();
        m.set_snapshot_watermark(1, 100).unwrap();
        m.persist().unwrap();
    }

    let before = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_micros() as u64;

    let compactor = WalOnlyCompactor::new(wal_dir, manifest);
    let info = compactor.compact().unwrap();

    let after = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_micros() as u64;

    assert!(info.timestamp >= before);
    assert!(info.timestamp <= after);
}

// === Database Handle Integration Tests ===

#[test]
fn test_database_handle_with_compaction() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("test.db");

    // Create database and write some records
    {
        let handle = DatabaseHandle::create(&db_path, DatabaseConfig::for_testing()).unwrap();
        let uuid = handle.uuid();

        for i in 0..10 {
            let record = WalRecord::new(i as u64 + 1, uuid, i as u64 * 1000, vec![i as u8; 50]);
            handle.append_wal(&record).unwrap();
        }

        handle.flush_wal().unwrap();
        handle.close().unwrap();
    }

    // Verify database can be reopened after creation
    {
        let handle = DatabaseHandle::open(&db_path, DatabaseConfig::for_testing()).unwrap();
        handle.close().unwrap();
    }
}

// === Tombstone Tests ===

#[test]
fn test_tombstone_basic_operations() {
    use strata_storage::compaction::{Tombstone, TombstoneIndex, TombstoneReason};

    let mut index = TombstoneIndex::new();
    let run_id = test_uuid();

    // Add tombstones
    index.add(Tombstone::new(
        run_id,
        0,
        b"key1".to_vec(),
        1,
        TombstoneReason::UserDelete,
    ));
    index.add(Tombstone::new(
        run_id,
        0,
        b"key2".to_vec(),
        2,
        TombstoneReason::Compaction,
    ));

    // Check tombstone status
    assert!(index.is_tombstoned(&run_id, 0, b"key1", 1));
    assert!(index.is_tombstoned(&run_id, 0, b"key2", 2));
    assert!(!index.is_tombstoned(&run_id, 0, b"key1", 2));
    assert!(!index.is_tombstoned(&run_id, 0, b"key3", 1));
}

#[test]
fn test_tombstone_serialization() {
    use strata_storage::compaction::{Tombstone, TombstoneIndex, TombstoneReason};

    let mut index = TombstoneIndex::new();
    let run_id = test_uuid();

    index.add(Tombstone::with_timestamp(
        run_id,
        0,
        b"key1".to_vec(),
        1,
        TombstoneReason::UserDelete,
        1000,
    ));
    index.add(Tombstone::with_timestamp(
        run_id,
        1,
        b"key2".to_vec(),
        2,
        TombstoneReason::RetentionPolicy,
        2000,
    ));

    // Serialize and deserialize
    let bytes = index.to_bytes();
    let restored = TombstoneIndex::from_bytes(&bytes).unwrap();

    assert_eq!(restored.len(), 2);
    assert!(restored.is_tombstoned(&run_id, 0, b"key1", 1));
    assert!(restored.is_tombstoned(&run_id, 1, b"key2", 2));
}

#[test]
fn test_tombstone_cleanup() {
    use strata_storage::compaction::{Tombstone, TombstoneIndex, TombstoneReason};

    let mut index = TombstoneIndex::new();
    let run_id = test_uuid();

    // Add tombstones with different timestamps
    index.add(Tombstone::with_timestamp(
        run_id,
        0,
        b"old".to_vec(),
        1,
        TombstoneReason::UserDelete,
        100,
    ));
    index.add(Tombstone::with_timestamp(
        run_id,
        0,
        b"new".to_vec(),
        2,
        TombstoneReason::UserDelete,
        500,
    ));

    assert_eq!(index.len(), 2);

    // Cleanup old tombstones
    let removed = index.cleanup_before(300);
    assert_eq!(removed, 1);
    assert_eq!(index.len(), 1);

    assert!(!index.is_tombstoned(&run_id, 0, b"old", 1));
    assert!(index.is_tombstoned(&run_id, 0, b"new", 2));
}

#[test]
fn test_tombstone_by_reason() {
    use strata_storage::compaction::{Tombstone, TombstoneIndex, TombstoneReason};

    let mut index = TombstoneIndex::new();
    let run_id = test_uuid();

    index.add(Tombstone::new(
        run_id,
        0,
        b"k1".to_vec(),
        1,
        TombstoneReason::UserDelete,
    ));
    index.add(Tombstone::new(
        run_id,
        0,
        b"k2".to_vec(),
        2,
        TombstoneReason::Compaction,
    ));
    index.add(Tombstone::new(
        run_id,
        0,
        b"k3".to_vec(),
        3,
        TombstoneReason::Compaction,
    ));

    assert_eq!(index.get_by_reason(TombstoneReason::UserDelete).len(), 1);
    assert_eq!(index.get_by_reason(TombstoneReason::Compaction).len(), 2);
    assert_eq!(
        index.get_by_reason(TombstoneReason::RetentionPolicy).len(),
        0
    );
}

// === Edge Cases ===

#[test]
fn test_compaction_with_single_segment() {
    let (_dir, wal_dir, manifest) = setup_test_env();

    create_segment_with_records(&wal_dir, 1, &[1, 2, 3]).unwrap();

    {
        let mut m = manifest.lock();
        m.set_snapshot_watermark(1, 10).unwrap();
        m.manifest_mut().active_wal_segment = 2; // Segment 1 is not active
        m.persist().unwrap();
    }

    let compactor = WalOnlyCompactor::new(wal_dir.clone(), manifest);
    let info = compactor.compact().unwrap();

    assert_eq!(info.wal_segments_removed, 1);
    assert_eq!(count_segments(&wal_dir), 0);
}

#[test]
fn test_compaction_with_boundary_watermark() {
    let (_dir, wal_dir, manifest) = setup_test_env();

    // Segment with max txn_id = 5
    create_segment_with_records(&wal_dir, 1, &[1, 2, 3, 4, 5]).unwrap();

    // Watermark exactly at max txn_id
    {
        let mut m = manifest.lock();
        m.set_snapshot_watermark(1, 5).unwrap();
        m.manifest_mut().active_wal_segment = 2;
        m.persist().unwrap();
    }

    let compactor = WalOnlyCompactor::new(wal_dir.clone(), manifest);
    let info = compactor.compact().unwrap();

    // Segment should be removed (max txn 5 <= watermark 5)
    assert_eq!(info.wal_segments_removed, 1);
}

#[test]
fn test_compaction_with_non_sequential_segments() {
    let (_dir, wal_dir, manifest) = setup_test_env();

    // Create non-sequential segment numbers
    create_segment_with_records(&wal_dir, 1, &[1]).unwrap();
    create_segment_with_records(&wal_dir, 5, &[2]).unwrap();
    create_segment_with_records(&wal_dir, 10, &[3]).unwrap();

    {
        let mut m = manifest.lock();
        m.set_snapshot_watermark(1, 2).unwrap();
        m.manifest_mut().active_wal_segment = 11;
        m.persist().unwrap();
    }

    let compactor = WalOnlyCompactor::new(wal_dir.clone(), manifest);
    let info = compactor.compact().unwrap();

    // Segments 1 and 5 should be removed
    assert_eq!(info.wal_segments_removed, 2);
    assert_eq!(count_segments(&wal_dir), 1);
}

#[test]
fn test_compaction_idempotent() {
    let (_dir, wal_dir, manifest) = setup_test_env();

    create_segment_with_records(&wal_dir, 1, &[1, 2, 3]).unwrap();
    create_segment_with_records(&wal_dir, 2, &[4, 5, 6]).unwrap();

    {
        let mut m = manifest.lock();
        m.set_snapshot_watermark(1, 3).unwrap();
        m.manifest_mut().active_wal_segment = 3;
        m.persist().unwrap();
    }

    let compactor = WalOnlyCompactor::new(wal_dir.clone(), manifest);

    // First compaction
    let info1 = compactor.compact().unwrap();
    assert_eq!(info1.wal_segments_removed, 1);

    // Second compaction should be idempotent
    let info2 = compactor.compact().unwrap();
    assert_eq!(info2.wal_segments_removed, 0);
}

// === Concurrent Compaction Tests ===

#[test]
fn test_multiple_compactor_instances() {
    let (_dir, wal_dir, manifest) = setup_test_env();

    create_segment_with_records(&wal_dir, 1, &[1, 2, 3]).unwrap();

    {
        let mut m = manifest.lock();
        m.set_snapshot_watermark(1, 10).unwrap();
        m.manifest_mut().active_wal_segment = 2;
        m.persist().unwrap();
    }

    // Create two compactors sharing the same manifest
    let compactor1 = WalOnlyCompactor::new(wal_dir.clone(), manifest.clone());
    let compactor2 = WalOnlyCompactor::new(wal_dir.clone(), manifest);

    // First compaction succeeds
    let info1 = compactor1.compact().unwrap();
    assert_eq!(info1.wal_segments_removed, 1);

    // Second compaction has nothing to do (segment already removed)
    let info2 = compactor2.compact().unwrap();
    assert_eq!(info2.wal_segments_removed, 0);
}

// === Stress Tests ===

#[test]
fn test_compaction_many_segments() {
    let (_dir, wal_dir, manifest) = setup_test_env();

    // Create 50 segments
    for i in 1..=50 {
        create_segment_with_records(&wal_dir, i, &[i]).unwrap();
    }

    assert_eq!(count_segments(&wal_dir), 50);

    {
        let mut m = manifest.lock();
        m.set_snapshot_watermark(1, 25).unwrap();
        m.manifest_mut().active_wal_segment = 51;
        m.persist().unwrap();
    }

    let compactor = WalOnlyCompactor::new(wal_dir.clone(), manifest);
    let info = compactor.compact().unwrap();

    // Segments 1-25 should be removed
    assert_eq!(info.wal_segments_removed, 25);
    assert_eq!(count_segments(&wal_dir), 25);
}

#[test]
fn test_compaction_large_segments() {
    let (_dir, wal_dir, manifest) = setup_test_env();

    // Create a segment with many records
    let txn_ids: Vec<u64> = (1..=100).collect();
    create_segment_with_records(&wal_dir, 1, &txn_ids).unwrap();

    {
        let mut m = manifest.lock();
        m.set_snapshot_watermark(1, 100).unwrap();
        m.manifest_mut().active_wal_segment = 2;
        m.persist().unwrap();
    }

    let compactor = WalOnlyCompactor::new(wal_dir.clone(), manifest);
    let info = compactor.compact().unwrap();

    assert_eq!(info.wal_segments_removed, 1);
    assert!(info.reclaimed_bytes > 5000); // Should have reclaimed substantial bytes
}
