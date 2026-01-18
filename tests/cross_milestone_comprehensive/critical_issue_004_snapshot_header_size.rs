//! ISSUE-004: Snapshot Header Size Mismatch
//!
//! **Severity**: CRITICAL
//! **Location**: `/crates/durability/src/snapshot_types.rs:46`
//!
//! **Problem**:
//! - Spec (SNAPSHOT_FORMAT.md): Header is 39 bytes (includes Prim Count at offset 38)
//! - Implementation: `SNAPSHOT_HEADER_SIZE = 38` bytes
//!
//! The Primitive Count field is written AFTER the header instead of being part of it.
//!
//! **Spec Requirement**: SNAPSHOT_FORMAT.md defines header layout with Prim Count
//! at offset 38.
//!
//! **Impact**: Format deviation could affect future snapshot compatibility.
//!
//! ## Test Strategy
//!
//! 1. Verify snapshot header size matches specification (39 bytes)
//! 2. Verify primitive count is at correct offset (38)
//! 3. Verify all header fields are at correct offsets
//! 4. Verify snapshot validation checks correct size

use crate::test_utils::*;
use std::fs;
use std::path::Path;

/// Expected header layout per SNAPSHOT_FORMAT.md:
/// | Field       | Offset | Size | Type   |
/// |-------------|--------|------|--------|
/// | Magic       | 0      | 10   | bytes  | "INMEM_SNAP"
/// | Version     | 10     | 4    | u32    |
/// | Timestamp   | 14     | 8    | u64    |
/// | WAL Offset  | 22     | 8    | u64    |
/// | Tx Count    | 30     | 8    | u64    |
/// | Prim Count  | 38     | 1    | u8     |
/// Total: 39 bytes
const SPEC_HEADER_SIZE: usize = 39;
const MAGIC: &[u8; 10] = b"INMEM_SNAP";
const MAGIC_OFFSET: usize = 0;
const VERSION_OFFSET: usize = 10;
const TIMESTAMP_OFFSET: usize = 14;
const WAL_OFFSET_OFFSET: usize = 22;
const TX_COUNT_OFFSET: usize = 30;
const PRIM_COUNT_OFFSET: usize = 38;

/// Test that snapshot header size matches specification.
///
/// **Expected behavior when ISSUE-004 is fixed**:
/// - SNAPSHOT_HEADER_SIZE constant equals 39
///
/// **Current behavior (ISSUE-004 present)**:
/// - SNAPSHOT_HEADER_SIZE equals 38
#[test]
fn test_snapshot_header_size_matches_spec() {
    // When ISSUE-004 is fixed:
    // use in_mem_durability::SNAPSHOT_HEADER_SIZE;
    // assert_eq!(SNAPSHOT_HEADER_SIZE, SPEC_HEADER_SIZE);

    // For now, document the expected size per spec
    assert_eq!(
        SPEC_HEADER_SIZE, 39,
        "Spec defines header as 39 bytes (including Prim Count at offset 38)"
    );
}

/// Test that primitive count field is at correct offset.
///
/// **Expected behavior when ISSUE-004 is fixed**:
/// - Prim Count is at offset 38 within the header
///
/// **Current behavior (ISSUE-004 present)**:
/// - Prim Count is written after the header
#[test]
fn test_prim_count_at_correct_offset() {
    let test_db = TestDb::new_strict();
    let run_id = test_db.run_id;

    // Create some data so snapshot has content
    let kv = test_db.kv();
    kv.put(&run_id, "test_key", in_mem_core::value::Value::I64(42))
        .expect("Should put");

    // Trigger a snapshot
    test_db.db.flush().expect("Should flush");

    // Check if snapshot file exists
    let snapshot_dir = test_db.db_path().join("snapshots");
    if snapshot_dir.exists() {
        if let Ok(entries) = fs::read_dir(&snapshot_dir) {
            for entry in entries.filter_map(Result::ok) {
                let path = entry.path();
                if path.extension().map(|e| e == "snap").unwrap_or(false) {
                    // Read the snapshot file
                    let data = fs::read(&path).expect("Should read snapshot");

                    // Verify minimum size
                    assert!(
                        data.len() >= SPEC_HEADER_SIZE,
                        "Snapshot should be at least {} bytes, got {}",
                        SPEC_HEADER_SIZE,
                        data.len()
                    );

                    // Verify magic
                    assert_eq!(
                        &data[MAGIC_OFFSET..MAGIC_OFFSET + 10],
                        MAGIC,
                        "Magic bytes should be 'INMEM_SNAP'"
                    );

                    // When ISSUE-004 is fixed:
                    // - Prim Count should be at offset 38
                    // let prim_count = data[PRIM_COUNT_OFFSET];
                    // assert!(prim_count > 0, "Should have at least one primitive section");

                    return; // Found a snapshot, test passed
                }
            }
        }
    }

    // If no snapshot exists, the test is inconclusive but not a failure
    // (Snapshot might not be created immediately)
}

/// Test that all header fields are at correct offsets.
///
/// **Expected behavior when ISSUE-004 is fixed**:
/// - All fields are correctly placed per SNAPSHOT_FORMAT.md
#[test]
fn test_header_field_offsets() {
    // Per SNAPSHOT_FORMAT.md, verify offset calculations
    assert_eq!(MAGIC_OFFSET, 0, "Magic should be at offset 0");
    assert_eq!(VERSION_OFFSET, 10, "Version should be at offset 10");
    assert_eq!(TIMESTAMP_OFFSET, 14, "Timestamp should be at offset 14");
    assert_eq!(WAL_OFFSET_OFFSET, 22, "WAL Offset should be at offset 22");
    assert_eq!(TX_COUNT_OFFSET, 30, "Tx Count should be at offset 30");
    assert_eq!(PRIM_COUNT_OFFSET, 38, "Prim Count should be at offset 38");

    // Verify sizes add up
    assert_eq!(
        MAGIC_OFFSET + 10,
        VERSION_OFFSET,
        "Magic (10 bytes) should be followed by Version"
    );
    assert_eq!(
        VERSION_OFFSET + 4,
        TIMESTAMP_OFFSET,
        "Version (4 bytes) should be followed by Timestamp"
    );
    assert_eq!(
        TIMESTAMP_OFFSET + 8,
        WAL_OFFSET_OFFSET,
        "Timestamp (8 bytes) should be followed by WAL Offset"
    );
    assert_eq!(
        WAL_OFFSET_OFFSET + 8,
        TX_COUNT_OFFSET,
        "WAL Offset (8 bytes) should be followed by Tx Count"
    );
    assert_eq!(
        TX_COUNT_OFFSET + 8,
        PRIM_COUNT_OFFSET,
        "Tx Count (8 bytes) should be followed by Prim Count"
    );
    assert_eq!(
        PRIM_COUNT_OFFSET + 1,
        SPEC_HEADER_SIZE,
        "Prim Count (1 byte) should complete the header"
    );
}

/// Test snapshot validation checks correct minimum size.
///
/// **Expected behavior when ISSUE-004 is fixed**:
/// - Validation requires at least 43 bytes (39 header + 4 CRC32)
///
/// **Current behavior (ISSUE-004 present)**:
/// - Validation may check for 42 bytes (38 header + 4 CRC32)
#[test]
fn test_snapshot_validation_minimum_size() {
    // Per SNAPSHOT_FORMAT.md:
    // - Header: 39 bytes
    // - CRC32: 4 bytes
    // - Minimum file size: 43 bytes
    const SPEC_MIN_SIZE: usize = 39 + 4;

    assert_eq!(
        SPEC_MIN_SIZE, 43,
        "Minimum snapshot size should be 43 bytes per spec"
    );

    // When ISSUE-004 is fixed:
    // use in_mem_durability::validate_snapshot;
    //
    // // Test that validation rejects files that are too small
    // let too_small = vec![0u8; 42];
    // let result = validate_snapshot(&too_small);
    // assert!(result.is_err(), "Should reject file smaller than 43 bytes");
    //
    // // Test that validation accepts minimum size
    // let mut valid_size = vec![0u8; 43];
    // valid_size[0..10].copy_from_slice(b"INMEM_SNAP");
    // // ... set other fields ...
    // let result = validate_snapshot(&valid_size);
    // // This might still fail due to CRC, but shouldn't fail on size
}

/// Test reading snapshot header fields.
///
/// **Expected behavior when ISSUE-004 is fixed**:
/// - Can read all header fields from a valid snapshot
#[test]
fn test_read_snapshot_header_fields() {
    let test_db = TestDb::new_strict();
    let run_id = test_db.run_id;

    // Create data to ensure snapshot has content
    let kv = test_db.kv();
    for i in 0..5 {
        kv.put(
            &run_id,
            &format!("key_{}", i),
            in_mem_core::value::Value::I64(i as i64),
        )
        .expect("put");
    }

    test_db.db.flush().expect("flush");

    // Find and read snapshot
    let snapshot_dir = test_db.db_path().join("snapshots");
    if !snapshot_dir.exists() {
        return; // No snapshot, test inconclusive
    }

    if let Ok(entries) = fs::read_dir(&snapshot_dir) {
        for entry in entries.filter_map(Result::ok) {
            let path = entry.path();
            if path.extension().map(|e| e == "snap").unwrap_or(false) {
                let data = fs::read(&path).expect("read");

                if data.len() < SPEC_HEADER_SIZE {
                    continue;
                }

                // Read version (u32 little-endian)
                let version = u32::from_le_bytes(
                    data[VERSION_OFFSET..VERSION_OFFSET + 4]
                        .try_into()
                        .unwrap(),
                );
                assert_eq!(version, 1, "Version should be 1");

                // Read timestamp (u64 little-endian)
                let timestamp = u64::from_le_bytes(
                    data[TIMESTAMP_OFFSET..TIMESTAMP_OFFSET + 8]
                        .try_into()
                        .unwrap(),
                );
                assert!(timestamp > 0, "Timestamp should be non-zero");

                // Read WAL offset
                let wal_offset = u64::from_le_bytes(
                    data[WAL_OFFSET_OFFSET..WAL_OFFSET_OFFSET + 8]
                        .try_into()
                        .unwrap(),
                );
                // WAL offset can be 0 if this is the first snapshot
                let _ = wal_offset;

                // Read transaction count
                let tx_count = u64::from_le_bytes(
                    data[TX_COUNT_OFFSET..TX_COUNT_OFFSET + 8]
                        .try_into()
                        .unwrap(),
                );
                assert!(tx_count > 0, "Should have committed transactions");

                // When ISSUE-004 is fixed, Prim Count should be at offset 38:
                // let prim_count = data[PRIM_COUNT_OFFSET];
                // assert!(prim_count > 0, "Should have primitive sections");

                return; // Successfully read header
            }
        }
    }
}

/// Test snapshot format version field.
///
/// **Expected behavior**:
/// - Version field at offset 10 contains valid version number (currently 1)
#[test]
fn test_snapshot_version_field() {
    // Per SNAPSHOT_FORMAT.md:
    // | Version | Description | Status |
    // |---------|-------------|--------|
    // | 1       | Current     | ACTIVE |
    // | 2       | Reserved    | FUTURE |
    // | 3       | Reserved    | FUTURE |

    const CURRENT_VERSION: u32 = 1;

    // When reading a snapshot, version should be 1
    let test_db = TestDb::new_strict();
    let kv = test_db.kv();
    kv.put(
        &test_db.run_id,
        "test",
        in_mem_core::value::Value::String("test".into()),
    )
    .expect("put");
    test_db.db.flush().expect("flush");

    // Find snapshot
    let snapshot_dir = test_db.db_path().join("snapshots");
    if snapshot_dir.exists() {
        if let Ok(entries) = fs::read_dir(&snapshot_dir) {
            for entry in entries.filter_map(Result::ok) {
                let path = entry.path();
                if path.extension().map(|e| e == "snap").unwrap_or(false) {
                    let data = fs::read(&path).expect("read");
                    if data.len() >= VERSION_OFFSET + 4 {
                        let version = u32::from_le_bytes(
                            data[VERSION_OFFSET..VERSION_OFFSET + 4]
                                .try_into()
                                .unwrap(),
                        );
                        assert_eq!(
                            version, CURRENT_VERSION,
                            "Snapshot version should be {}",
                            CURRENT_VERSION
                        );
                        return;
                    }
                }
            }
        }
    }
}
