//! Format Validation Tests
//!
//! Tests for WAL and Snapshot file format validation.

use strata_storage::codec::IdentityCodec;
use strata_storage::disk_snapshot::{SnapshotReader, SnapshotSection, SnapshotWriter};
use strata_storage::format::snapshot::{
    primitive_tags, parse_snapshot_id, snapshot_path, SectionHeader, SnapshotHeader,
    SnapshotHeaderError, SNAPSHOT_HEADER_SIZE, SNAPSHOT_MAGIC,
};
use strata_storage::format::wal_record::{
    SegmentHeader, WalRecord, WalRecordError, SEGMENT_FORMAT_VERSION, SEGMENT_HEADER_SIZE,
    SEGMENT_MAGIC, WAL_RECORD_FORMAT_VERSION,
};
use std::path::Path;
use tempfile::tempdir;

// ============================================================================
// WAL Format Tests
// ============================================================================

#[test]
fn wal_segment_has_correct_magic() {
    let header = SegmentHeader::new(1, [0u8; 16]);
    assert_eq!(header.magic, SEGMENT_MAGIC);
    assert_eq!(&header.magic, b"STRA");
}

#[test]
fn wal_segment_has_correct_version() {
    let header = SegmentHeader::new(1, [0u8; 16]);
    assert_eq!(header.format_version, SEGMENT_FORMAT_VERSION);
}

#[test]
fn wal_record_crc_detects_bit_flip() {
    let record = WalRecord::new(42, [1u8; 16], 123, vec![1, 2, 3, 4, 5]);
    let mut bytes = record.to_bytes();

    // Flip a single bit in the payload
    bytes[15] ^= 0x01;

    let result = WalRecord::from_bytes(&bytes);
    assert!(matches!(
        result,
        Err(WalRecordError::ChecksumMismatch { .. })
    ));
}

#[test]
fn wal_record_crc_detects_truncation() {
    let record = WalRecord::new(42, [1u8; 16], 123, vec![1, 2, 3, 4, 5]);
    let bytes = record.to_bytes();

    // Truncate the record
    let truncated = &bytes[..bytes.len() - 10];

    let result = WalRecord::from_bytes(truncated);
    assert!(matches!(result, Err(WalRecordError::InsufficientData)));
}

#[test]
fn wal_record_rejects_zero_length() {
    // Length field says 0
    let bytes = [0u8; 10];
    let result = WalRecord::from_bytes(&bytes);
    assert!(matches!(result, Err(WalRecordError::InvalidFormat)));
}

#[test]
fn wal_record_version_is_current() {
    let record = WalRecord::new(1, [0u8; 16], 0, vec![]);
    let bytes = record.to_bytes();

    // Format version is at offset 4 (after length field)
    assert_eq!(bytes[4], WAL_RECORD_FORMAT_VERSION);
}

#[test]
fn wal_segment_header_size_is_32() {
    assert_eq!(SEGMENT_HEADER_SIZE, 32);

    let header = SegmentHeader::new(1, [0u8; 16]);
    let bytes = header.to_bytes();
    assert_eq!(bytes.len(), 32);
}

// ============================================================================
// Snapshot Format Tests
// ============================================================================

#[test]
fn snapshot_has_correct_magic() {
    let header = SnapshotHeader::new(1, 100, 123456, [0u8; 16], 8);
    assert_eq!(header.magic, SNAPSHOT_MAGIC);
    assert_eq!(&header.magic, b"SNAP");
}

#[test]
fn snapshot_header_roundtrip() {
    let original = SnapshotHeader::new(42, 1000, 9999999, [0xAB; 16], 12);
    let bytes = original.to_bytes();
    let parsed = SnapshotHeader::from_bytes(&bytes).unwrap();

    assert_eq!(parsed.magic, SNAPSHOT_MAGIC);
    assert_eq!(parsed.snapshot_id, 42);
    assert_eq!(parsed.watermark_txn, 1000);
    assert_eq!(parsed.created_at, 9999999);
    assert_eq!(parsed.database_uuid, [0xAB; 16]);
    assert_eq!(parsed.codec_id_len, 12);
}

#[test]
fn snapshot_header_size_is_64() {
    assert_eq!(SNAPSHOT_HEADER_SIZE, 64);

    let header = SnapshotHeader::new(1, 100, 123, [0u8; 16], 8);
    let bytes = header.to_bytes();
    assert_eq!(bytes.len(), 64);
}

#[test]
fn snapshot_validates_magic() {
    let mut header = SnapshotHeader::new(1, 100, 123, [0u8; 16], 8);
    header.magic = *b"FAKE";

    let result = header.validate();
    assert!(matches!(
        result,
        Err(SnapshotHeaderError::InvalidMagic { .. })
    ));
}

#[test]
fn snapshot_validates_version() {
    let mut header = SnapshotHeader::new(1, 100, 123, [0u8; 16], 8);
    header.format_version = 999; // Future version

    let result = header.validate();
    assert!(matches!(
        result,
        Err(SnapshotHeaderError::UnsupportedVersion { .. })
    ));
}

#[test]
fn snapshot_crc_detects_corruption() {
    let temp_dir = tempdir().unwrap();
    let uuid = [1u8; 16];

    // Create valid snapshot
    let writer =
        SnapshotWriter::new(temp_dir.path().to_path_buf(), Box::new(IdentityCodec), uuid).unwrap();

    let sections = vec![SnapshotSection::new(primitive_tags::KV, vec![1, 2, 3, 4])];
    let info = writer.create_snapshot(1, 100, sections).unwrap();

    // Corrupt the file
    let mut data = std::fs::read(&info.path).unwrap();
    data[70] ^= 0xFF; // Corrupt somewhere in the middle
    std::fs::write(&info.path, &data).unwrap();

    // Should fail CRC check
    let reader = SnapshotReader::new(Box::new(IdentityCodec));
    let result = reader.load(&info.path);
    assert!(result.is_err());
}

#[test]
fn snapshot_section_header_size_is_9() {
    assert_eq!(SectionHeader::SIZE, 9);
}

#[test]
fn snapshot_section_roundtrip() {
    let header = SectionHeader::new(primitive_tags::KV, 1024);
    let bytes = header.to_bytes();
    let parsed = SectionHeader::from_bytes(&bytes);

    assert_eq!(parsed.primitive_type, primitive_tags::KV);
    assert_eq!(parsed.data_len, 1024);
}

// ============================================================================
// Snapshot Path Format
// ============================================================================

#[test]
fn snapshot_path_format() {
    let dir = Path::new("/data/snapshots");

    assert_eq!(
        snapshot_path(dir, 1).to_str().unwrap(),
        "/data/snapshots/snap-000001.chk"
    );
    assert_eq!(
        snapshot_path(dir, 999999).to_str().unwrap(),
        "/data/snapshots/snap-999999.chk"
    );
}

#[test]
fn parse_snapshot_id_valid() {
    assert_eq!(parse_snapshot_id("snap-000001.chk"), Some(1));
    assert_eq!(parse_snapshot_id("snap-000100.chk"), Some(100));
    assert_eq!(parse_snapshot_id("snap-999999.chk"), Some(999999));
}

#[test]
fn parse_snapshot_id_invalid() {
    assert_eq!(parse_snapshot_id("snapshot-000001.chk"), None);
    assert_eq!(parse_snapshot_id("snap-000001.bak"), None);
    assert_eq!(parse_snapshot_id("wal-000001.seg"), None);
    assert_eq!(parse_snapshot_id("snap-.chk"), None);
    assert_eq!(parse_snapshot_id("random.file"), None);
}

// ============================================================================
// Primitive Tags
// ============================================================================

#[test]
fn primitive_tags_are_valid() {
    assert_eq!(primitive_tags::KV, 0x01);
    assert_eq!(primitive_tags::EVENT, 0x02);
    assert_eq!(primitive_tags::STATE, 0x03);
    assert_eq!(primitive_tags::RUN, 0x05);
    assert_eq!(primitive_tags::JSON, 0x06);
    assert_eq!(primitive_tags::VECTOR, 0x07);
}

#[test]
fn primitive_tag_names() {
    assert_eq!(primitive_tags::tag_name(primitive_tags::KV), "KV");
    assert_eq!(primitive_tags::tag_name(primitive_tags::EVENT), "Event");
    assert_eq!(primitive_tags::tag_name(primitive_tags::STATE), "State");
    assert_eq!(primitive_tags::tag_name(primitive_tags::RUN), "Run");
    assert_eq!(primitive_tags::tag_name(primitive_tags::JSON), "Json");
    assert_eq!(primitive_tags::tag_name(primitive_tags::VECTOR), "Vector");
    assert_eq!(primitive_tags::tag_name(0xFF), "Unknown");
}

#[test]
fn all_tags_constant() {
    assert_eq!(
        primitive_tags::ALL_TAGS,
        [
            primitive_tags::KV,
            primitive_tags::EVENT,
            primitive_tags::STATE,
            primitive_tags::RUN,
            primitive_tags::JSON,
            primitive_tags::VECTOR,
        ]
    );
}

// ============================================================================
// Snapshot Read/Write Integration
// ============================================================================

#[test]
fn snapshot_write_read_roundtrip() {
    let temp_dir = tempdir().unwrap();
    let uuid = [0xAB; 16];

    let writer =
        SnapshotWriter::new(temp_dir.path().to_path_buf(), Box::new(IdentityCodec), uuid).unwrap();

    let sections = vec![
        SnapshotSection::new(primitive_tags::KV, b"kv_data".to_vec()),
        SnapshotSection::new(primitive_tags::EVENT, b"event_data".to_vec()),
        SnapshotSection::new(primitive_tags::VECTOR, b"vector_data".to_vec()),
    ];

    let info = writer.create_snapshot(42, 1000, sections).unwrap();

    let reader = SnapshotReader::new(Box::new(IdentityCodec));
    let loaded = reader.load(&info.path).unwrap();

    assert_eq!(loaded.snapshot_id(), 42);
    assert_eq!(loaded.watermark_txn(), 1000);
    assert_eq!(loaded.database_uuid(), uuid);
    assert_eq!(loaded.sections.len(), 3);

    let kv = loaded.find_section(primitive_tags::KV).unwrap();
    assert_eq!(kv.data, b"kv_data");

    let event = loaded.find_section(primitive_tags::EVENT).unwrap();
    assert_eq!(event.data, b"event_data");

    let vector = loaded.find_section(primitive_tags::VECTOR).unwrap();
    assert_eq!(vector.data, b"vector_data");
}

#[test]
fn snapshot_with_empty_section() {
    let temp_dir = tempdir().unwrap();
    let uuid = [0u8; 16];

    let writer =
        SnapshotWriter::new(temp_dir.path().to_path_buf(), Box::new(IdentityCodec), uuid).unwrap();

    let sections = vec![SnapshotSection::new(primitive_tags::KV, vec![])];

    let info = writer.create_snapshot(1, 1, sections).unwrap();

    let reader = SnapshotReader::new(Box::new(IdentityCodec));
    let loaded = reader.load(&info.path).unwrap();

    let kv = loaded.find_section(primitive_tags::KV).unwrap();
    assert!(kv.data.is_empty());
}
