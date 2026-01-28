//! Compaction Tests
//!
//! Tests for WAL-only compaction and tombstone cleanup.

use strata_storage::compaction::{CompactInfo, TombstoneIndex, TombstoneReason};
use strata_storage::format::wal_record::{SegmentHeader, WalRecord, WalSegment, SEGMENT_MAGIC};
use std::path::Path;
use tempfile::tempdir;

// ============================================================================
// WAL Segment Tests
// ============================================================================

#[test]
fn wal_segment_creation() {
    let dir = tempdir().unwrap();
    let uuid = [1u8; 16];

    let segment = WalSegment::create(dir.path(), 1, uuid).unwrap();
    assert_eq!(segment.segment_number(), 1);
    assert!(!segment.is_closed());
    assert_eq!(segment.database_uuid(), uuid);
}

#[test]
fn wal_segment_write_and_close() {
    let dir = tempdir().unwrap();
    let uuid = [2u8; 16];

    let mut segment = WalSegment::create(dir.path(), 1, uuid).unwrap();
    let initial_size = segment.size();

    segment.write(b"test data").unwrap();
    assert!(segment.size() > initial_size);

    segment.close().unwrap();
    assert!(segment.is_closed());

    // Cannot write to closed segment
    let result = segment.write(b"more data");
    assert!(result.is_err());
}

#[test]
fn wal_segment_open_read() {
    let dir = tempdir().unwrap();
    let uuid = [3u8; 16];

    // Create and close segment
    {
        let mut segment = WalSegment::create(dir.path(), 1, uuid).unwrap();
        segment.write(b"test").unwrap();
        segment.close().unwrap();
    }

    // Open for reading
    let segment = WalSegment::open_read(dir.path(), 1).unwrap();
    assert_eq!(segment.segment_number(), 1);
    assert!(segment.is_closed()); // Read mode = closed
}

#[test]
fn wal_segment_path_format() {
    let dir = Path::new("/data/wal");
    assert_eq!(
        WalSegment::segment_path(dir, 1).to_str().unwrap(),
        "/data/wal/wal-000001.seg"
    );
    assert_eq!(
        WalSegment::segment_path(dir, 999999).to_str().unwrap(),
        "/data/wal/wal-999999.seg"
    );
}

// ============================================================================
// WAL Record Tests
// ============================================================================

#[test]
fn wal_record_roundtrip() {
    let record = WalRecord::new(42, [1u8; 16], 1234567890, vec![1, 2, 3, 4, 5]);

    let bytes = record.to_bytes();
    let (parsed, consumed) = WalRecord::from_bytes(&bytes).unwrap();

    assert_eq!(parsed.txn_id, 42);
    assert_eq!(parsed.run_id, [1u8; 16]);
    assert_eq!(parsed.timestamp, 1234567890);
    assert_eq!(parsed.writeset, vec![1, 2, 3, 4, 5]);
    assert_eq!(consumed, bytes.len());
}

#[test]
fn wal_record_crc_detects_corruption() {
    let record = WalRecord::new(42, [1u8; 16], 123, vec![1, 2, 3]);
    let mut bytes = record.to_bytes();

    // Corrupt payload
    bytes[10] ^= 0xFF;

    let result = WalRecord::from_bytes(&bytes);
    assert!(result.is_err());
}

#[test]
fn wal_record_empty_writeset() {
    let record = WalRecord::new(1, [0u8; 16], 0, Vec::new());
    let bytes = record.to_bytes();
    let (parsed, _) = WalRecord::from_bytes(&bytes).unwrap();
    assert!(parsed.writeset.is_empty());
}

#[test]
fn multiple_records_in_sequence() {
    let records = vec![
        WalRecord::new(1, [1u8; 16], 100, vec![1, 2, 3]),
        WalRecord::new(2, [2u8; 16], 200, vec![4, 5, 6, 7]),
        WalRecord::new(3, [3u8; 16], 300, vec![]),
    ];

    // Serialize all
    let mut all_bytes = Vec::new();
    for record in &records {
        all_bytes.extend_from_slice(&record.to_bytes());
    }

    // Parse back
    let mut offset = 0;
    for expected in &records {
        let (parsed, consumed) = WalRecord::from_bytes(&all_bytes[offset..]).unwrap();
        assert_eq!(parsed.txn_id, expected.txn_id);
        offset += consumed;
    }
    assert_eq!(offset, all_bytes.len());
}

// ============================================================================
// Segment Header Tests
// ============================================================================

#[test]
fn segment_header_roundtrip() {
    let header = SegmentHeader::new(12345, [0xAB; 16]);
    let bytes = header.to_bytes();
    let parsed = SegmentHeader::from_bytes(&bytes).unwrap();

    assert_eq!(parsed.magic, SEGMENT_MAGIC);
    assert_eq!(parsed.segment_number, 12345);
    assert_eq!(parsed.database_uuid, [0xAB; 16]);
    assert!(parsed.is_valid());
}

#[test]
fn segment_header_invalid_magic() {
    let mut header = SegmentHeader::new(1, [0; 16]);
    header.magic = *b"XXXX";
    assert!(!header.is_valid());
}

// ============================================================================
// Tombstone Index Tests
// ============================================================================

#[test]
fn tombstone_add_and_check() {
    let mut index = TombstoneIndex::new();
    let run_id = [1u8; 16];

    index.add(strata_storage::compaction::Tombstone::new(
        run_id,
        0, // KV
        b"key1".to_vec(),
        1,
        TombstoneReason::UserDelete,
    ));

    assert!(index.is_tombstoned(&run_id, 0, b"key1", 1));
    assert!(!index.is_tombstoned(&run_id, 0, b"key1", 2)); // Different version
    assert!(!index.is_tombstoned(&run_id, 0, b"key2", 1)); // Different key
    assert!(!index.is_tombstoned(&run_id, 1, b"key1", 1)); // Different type
}

#[test]
fn tombstone_cleanup_before_cutoff() {
    let mut index = TombstoneIndex::new();
    let run_id = [1u8; 16];

    // Add tombstones at different times
    index.add(strata_storage::compaction::Tombstone::with_timestamp(
        run_id,
        0,
        b"old1".to_vec(),
        1,
        TombstoneReason::UserDelete,
        100,
    ));
    index.add(strata_storage::compaction::Tombstone::with_timestamp(
        run_id,
        0,
        b"old2".to_vec(),
        1,
        TombstoneReason::UserDelete,
        200,
    ));
    index.add(strata_storage::compaction::Tombstone::with_timestamp(
        run_id,
        0,
        b"new".to_vec(),
        1,
        TombstoneReason::UserDelete,
        300,
    ));

    assert_eq!(index.len(), 3);

    // Cleanup before 250
    let removed = index.cleanup_before(250);
    assert_eq!(removed, 2);
    assert_eq!(index.len(), 1);

    // Only 'new' should remain
    assert!(!index.is_tombstoned(&run_id, 0, b"old1", 1));
    assert!(!index.is_tombstoned(&run_id, 0, b"old2", 1));
    assert!(index.is_tombstoned(&run_id, 0, b"new", 1));
}

#[test]
fn tombstone_by_reason() {
    let mut index = TombstoneIndex::new();
    let run_id = [1u8; 16];

    index.add(strata_storage::compaction::Tombstone::new(
        run_id,
        0,
        b"user_deleted".to_vec(),
        1,
        TombstoneReason::UserDelete,
    ));
    index.add(strata_storage::compaction::Tombstone::new(
        run_id,
        0,
        b"compacted1".to_vec(),
        1,
        TombstoneReason::Compaction,
    ));
    index.add(strata_storage::compaction::Tombstone::new(
        run_id,
        0,
        b"compacted2".to_vec(),
        1,
        TombstoneReason::Compaction,
    ));

    assert_eq!(index.get_by_reason(TombstoneReason::UserDelete).len(), 1);
    assert_eq!(index.get_by_reason(TombstoneReason::Compaction).len(), 2);
    assert_eq!(
        index.get_by_reason(TombstoneReason::RetentionPolicy).len(),
        0
    );
}

#[test]
fn tombstone_by_run() {
    let mut index = TombstoneIndex::new();
    let run1 = [1u8; 16];
    let mut run2 = [1u8; 16];
    run2[0] = 2;

    index.add(strata_storage::compaction::Tombstone::new(
        run1,
        0,
        b"k1".to_vec(),
        1,
        TombstoneReason::UserDelete,
    ));
    index.add(strata_storage::compaction::Tombstone::new(
        run1,
        0,
        b"k2".to_vec(),
        1,
        TombstoneReason::UserDelete,
    ));
    index.add(strata_storage::compaction::Tombstone::new(
        run2,
        0,
        b"k1".to_vec(),
        1,
        TombstoneReason::UserDelete,
    ));

    assert_eq!(index.get_by_run(&run1).len(), 2);
    assert_eq!(index.get_by_run(&run2).len(), 1);
}

#[test]
fn tombstone_serialization_roundtrip() {
    let mut index = TombstoneIndex::new();
    let run_id = [1u8; 16];

    index.add(strata_storage::compaction::Tombstone::with_timestamp(
        run_id,
        0,
        b"key1".to_vec(),
        1,
        TombstoneReason::UserDelete,
        100,
    ));
    index.add(strata_storage::compaction::Tombstone::with_timestamp(
        run_id,
        1,
        b"key2".to_vec(),
        2,
        TombstoneReason::Compaction,
        200,
    ));

    let bytes = index.to_bytes();
    let parsed = TombstoneIndex::from_bytes(&bytes).unwrap();

    assert_eq!(parsed.len(), 2);
    assert!(parsed.is_tombstoned(&run_id, 0, b"key1", 1));
    assert!(parsed.is_tombstoned(&run_id, 1, b"key2", 2));
}

// ============================================================================
// CompactInfo Tests
// ============================================================================

#[test]
fn compact_info_creation() {
    use strata_storage::compaction::CompactMode;

    let mut info = CompactInfo::new(CompactMode::WALOnly);
    info.wal_segments_removed = 5;
    info.reclaimed_bytes = 1024;

    assert_eq!(info.mode, CompactMode::WALOnly);
    assert_eq!(info.wal_segments_removed, 5);
    assert!(info.did_compact());
}

#[test]
fn compact_info_no_compaction() {
    use strata_storage::compaction::CompactMode;

    let info = CompactInfo::new(CompactMode::WALOnly);
    assert!(!info.did_compact());
}

#[test]
fn compact_info_summary() {
    use strata_storage::compaction::CompactMode;

    let mut info = CompactInfo::new(CompactMode::Full);
    info.wal_segments_removed = 5;
    info.versions_removed = 100;
    info.reclaimed_bytes = 2048;

    let summary = info.summary();
    assert!(summary.contains("segments_removed"));
    assert!(summary.contains("versions_removed"));
}
