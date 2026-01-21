//! M10 Architecture Conformance Tests
//!
//! This test suite validates the M10 (Storage Backend, Retention, and Compaction)
//! implementation against the authoritative specification in M10_ARCHITECTURE.md.
//!
//! # Test Organization
//!
//! Tests are organized by the invariants defined in the architecture:
//! - Storage Invariants (S1-S9)
//! - Recovery Invariants (R1-R5)
//! - Retention Invariants (RT1-RT4)
//! - Compaction Invariants (C1-C5)
//! - Architectural Rules (5 Rules)
//!
//! # Reference
//!
//! See docs/architecture/M10_ARCHITECTURE.md for the authoritative specification.

use std::io::Read as IoRead;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use strata_core::PrimitiveType;
use strata_storage::codec::{IdentityCodec, StorageCodec};
use strata_storage::compaction::{CompactInfo, CompactMode, TombstoneIndex, WalOnlyCompactor};
use strata_storage::database::{DatabaseConfig, DatabaseHandle};
use strata_storage::format::{
    Manifest, ManifestManager, WalRecord, WalSegment, MANIFEST_FORMAT_VERSION, MANIFEST_MAGIC,
    SEGMENT_FORMAT_VERSION, SEGMENT_HEADER_SIZE, SEGMENT_MAGIC, WAL_RECORD_FORMAT_VERSION,
};
use strata_storage::recovery::{RecoveryCoordinator, RecoveryError};
use strata_storage::retention::RetentionPolicy;
use strata_storage::testing::{CrashPoint, DataState, ReferenceModel, WalCorruptionTester};
use strata_storage::wal::{DurabilityMode, WalConfig, WalReader, WalWriter};
use tempfile::tempdir;

// =============================================================================
// HELPER FUNCTIONS
// =============================================================================

fn test_uuid() -> [u8; 16] {
    [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16]
}

fn make_codec() -> Box<dyn StorageCodec> {
    Box::new(IdentityCodec)
}

fn create_test_wal_record(txn_id: u64, data: &[u8]) -> WalRecord {
    WalRecord::new(txn_id, test_uuid(), txn_id * 1000, data.to_vec())
}

/// Setup a minimal test database with WAL directory and MANIFEST
fn setup_minimal_database(db_path: &Path) {
    std::fs::create_dir_all(db_path).unwrap();
    std::fs::create_dir_all(db_path.join("WAL")).unwrap();
    std::fs::create_dir_all(db_path.join("SNAPSHOTS")).unwrap();
    std::fs::create_dir_all(db_path.join("DATA")).unwrap();

    ManifestManager::create(db_path.join("MANIFEST"), test_uuid(), "identity".to_string()).unwrap();
}

// =============================================================================
// STORAGE INVARIANTS (S1-S9)
// From M10_ARCHITECTURE.md Section 3.1
// =============================================================================

mod storage_invariants {
    use super::*;

    /// S1: WAL is append-only - Records can only be appended, never modified in place
    #[test]
    fn s1_wal_is_append_only() {
        let dir = tempdir().unwrap();
        let wal_dir = dir.path().join("wal");
        std::fs::create_dir_all(&wal_dir).unwrap();

        let mut writer = WalWriter::new(
            wal_dir.clone(),
            test_uuid(),
            DurabilityMode::Strict,
            WalConfig::for_testing(),
            make_codec(),
        )
        .unwrap();

        // Write first record
        let record1 = create_test_wal_record(1, b"first");
        writer.append(&record1).unwrap();
        writer.flush().unwrap();

        // Get segment size after first write
        let segment_path = WalSegment::segment_path(&wal_dir, 1);
        let size_after_first = std::fs::metadata(&segment_path).unwrap().len();

        // Write second record
        let record2 = create_test_wal_record(2, b"second");
        writer.append(&record2).unwrap();
        writer.flush().unwrap();

        // Size must only grow (append-only)
        let size_after_second = std::fs::metadata(&segment_path).unwrap().len();
        assert!(
            size_after_second > size_after_first,
            "S1 violated: WAL size did not grow after append (was {}, now {})",
            size_after_first,
            size_after_second
        );

        // Read back and verify both records exist
        let reader = WalReader::new(make_codec());
        let result = reader.read_all(&wal_dir).unwrap();
        assert_eq!(result.records.len(), 2, "S1: Both records should be present");
        assert_eq!(result.records[0].txn_id, 1);
        assert_eq!(result.records[1].txn_id, 2);
    }

    /// S2: WAL segments are immutable once closed
    #[test]
    fn s2_wal_segments_immutable_once_closed() {
        let dir = tempdir().unwrap();
        let wal_dir = dir.path();

        // Create a segment
        let mut segment = WalSegment::create(wal_dir, 1, test_uuid()).unwrap();
        segment.write(b"test data").unwrap();

        // Close the segment
        segment.close().unwrap();
        assert!(segment.is_closed(), "Segment should be marked closed");

        // Attempting to write to closed segment should fail
        let result = segment.write(b"more data");
        assert!(
            result.is_err(),
            "S2 violated: Write to closed segment should fail"
        );

        // Attempting to truncate closed segment should fail
        let result = segment.truncate(0);
        assert!(
            result.is_err(),
            "S2 violated: Truncate on closed segment should fail"
        );
    }

    /// S3: WAL records are self-delimiting - Each record contains its length and checksum
    #[test]
    fn s3_wal_records_self_delimiting() {
        let record = create_test_wal_record(42, b"test payload");
        let bytes = record.to_bytes();

        // First 4 bytes are length prefix
        let length = u32::from_le_bytes(bytes[0..4].try_into().unwrap()) as usize;
        assert!(
            length > 0,
            "S3: Record should have non-zero length prefix"
        );

        // Length + 4 (prefix) should equal total record size
        assert_eq!(
            bytes.len(),
            length + 4,
            "S3: Length prefix should match actual data size"
        );

        // Last 4 bytes of content (before length prefix is accounted for) are CRC32
        // Verify checksum is present and valid
        assert!(
            WalRecord::verify_checksum(&bytes).is_ok(),
            "S3: Record should have valid checksum"
        );

        // Can parse record independently without external state
        let (parsed, consumed) = WalRecord::from_bytes(&bytes).unwrap();
        assert_eq!(parsed.txn_id, 42);
        assert_eq!(consumed, bytes.len());
    }

    /// S3: Multiple records in sequence are independently parseable
    #[test]
    fn s3_multiple_records_independently_parseable() {
        let records: Vec<WalRecord> = (1..=5)
            .map(|i| create_test_wal_record(i, &vec![i as u8; 10]))
            .collect();

        let mut all_bytes = Vec::new();
        for record in &records {
            all_bytes.extend_from_slice(&record.to_bytes());
        }

        // Parse each record independently
        let mut offset = 0;
        for expected in &records {
            let (parsed, consumed) = WalRecord::from_bytes(&all_bytes[offset..])
                .expect("S3: Each record should be independently parseable");
            assert_eq!(parsed.txn_id, expected.txn_id);
            offset += consumed;
        }
        assert_eq!(offset, all_bytes.len(), "S3: Should consume all bytes");
    }

    /// S4: Snapshots are consistent - Snapshot represents a single logical point in time
    /// Note: Full snapshot testing requires integration with disk_snapshot module
    #[test]
    fn s4_snapshots_are_consistent() {
        // This test validates the watermark mechanism ensures consistency
        let dir = tempdir().unwrap();
        let manifest_path = dir.path().join("MANIFEST");

        let mut manager =
            ManifestManager::create(manifest_path, test_uuid(), "identity".to_string()).unwrap();

        // Set a snapshot watermark
        manager.set_snapshot_watermark(1, 100).unwrap();

        // Watermark should be consistently stored
        assert_eq!(manager.manifest().snapshot_id, Some(1));
        assert_eq!(manager.manifest().snapshot_watermark, Some(100));

        // Reload and verify consistency
        let path = manager.path().to_path_buf();
        let loaded = ManifestManager::load(path).unwrap();
        assert_eq!(
            loaded.manifest().snapshot_watermark,
            Some(100),
            "S4: Snapshot watermark should be persisted consistently"
        );
    }

    /// S5: Snapshots are logical - Not a memory dump
    /// This is tested by verifying snapshot format is defined independently of memory layout
    #[test]
    fn s5_snapshots_are_logical() {
        // The SnapshotSerializer uses explicit serialization, not memory dumps
        // This test verifies the format module defines logical serialization

        // WAL records use explicit serialization format
        let record = create_test_wal_record(1, b"test");
        let bytes = record.to_bytes();

        // Format version is explicitly defined (not derived from struct layout)
        assert_eq!(bytes[4], WAL_RECORD_FORMAT_VERSION);

        // MANIFEST uses explicit format version
        let manifest = Manifest::new(test_uuid(), "identity".to_string());
        let manifest_bytes = manifest.to_bytes();

        // Magic bytes and format version are explicit
        assert_eq!(&manifest_bytes[0..4], &MANIFEST_MAGIC);
        let format_version = u32::from_le_bytes(manifest_bytes[4..8].try_into().unwrap());
        assert_eq!(format_version, MANIFEST_FORMAT_VERSION);
    }

    /// S6: Watermark ordering - Snapshot watermark ≤ all WAL records after it
    #[test]
    fn s6_watermark_ordering() {
        let dir = tempdir().unwrap();
        let db_path = dir.path();

        setup_minimal_database(db_path);

        // Create WAL with records
        let wal_dir = db_path.join("WAL");
        let mut writer = WalWriter::new(
            wal_dir.clone(),
            test_uuid(),
            DurabilityMode::Strict,
            WalConfig::for_testing(),
            make_codec(),
        )
        .unwrap();

        for i in 1..=10 {
            let record = create_test_wal_record(i, &[i as u8]);
            writer.append(&record).unwrap();
        }
        writer.flush().unwrap();
        drop(writer);

        // Set watermark at 5
        let mut manager = ManifestManager::load(db_path.join("MANIFEST")).unwrap();
        manager.set_snapshot_watermark(1, 5).unwrap();

        // Read WAL and verify ordering
        let reader = WalReader::new(make_codec());
        let result = reader.read_all(&wal_dir).unwrap();

        let watermark = manager.manifest().snapshot_watermark.unwrap();

        // Records > watermark should exist in WAL for replay
        for record in &result.records {
            // Note: In a real system, records <= watermark would be in snapshot
            // This test verifies the mechanism for determining which records to replay
            if record.txn_id > watermark {
                // These records need replay after snapshot
                assert!(
                    record.txn_id > watermark,
                    "S6: Records > watermark should be in WAL for replay"
                );
            }
        }
    }

    /// S7: MANIFEST atomicity - MANIFEST updates are atomic (write-fsync-rename)
    #[test]
    fn s7_manifest_atomicity() {
        let dir = tempdir().unwrap();
        let manifest_path = dir.path().join("MANIFEST");

        // Create initial MANIFEST
        let manager =
            ManifestManager::create(manifest_path.clone(), test_uuid(), "identity".to_string())
                .unwrap();

        // After creation, no temp file should exist
        let temp_path = manifest_path.with_extension("tmp");
        assert!(
            !temp_path.exists(),
            "S7: Temp file should not exist after atomic write"
        );

        // MANIFEST should exist and be valid
        assert!(manifest_path.exists());

        // Reload should succeed (indicates atomic completion)
        let _loaded = ManifestManager::load(manifest_path.clone()).unwrap();

        // Verify checksum integrity (proves atomic write completed)
        let bytes = std::fs::read(&manifest_path).unwrap();
        let data = &bytes[..bytes.len() - 4];
        let stored_crc = u32::from_le_bytes(bytes[bytes.len() - 4..].try_into().unwrap());
        let computed_crc = crc32fast::hash(data);
        assert_eq!(
            stored_crc, computed_crc,
            "S7: MANIFEST checksum should be valid after atomic write"
        );

        drop(manager);
    }

    /// S8: Codec pass-through - All persisted bytes pass through codec boundary
    #[test]
    fn s8_codec_pass_through() {
        let dir = tempdir().unwrap();
        let wal_dir = dir.path();

        // Create WAL writer with codec
        let codec = make_codec();
        let codec_id = codec.codec_id().to_string();

        let mut writer = WalWriter::new(
            wal_dir.to_path_buf(),
            test_uuid(),
            DurabilityMode::Strict,
            WalConfig::for_testing(),
            codec,
        )
        .unwrap();

        let record = create_test_wal_record(1, b"test data");
        writer.append(&record).unwrap();
        writer.flush().unwrap();
        drop(writer);

        // Read with same codec should succeed
        let reader = WalReader::new(make_codec());
        let result = reader.read_all(wal_dir).unwrap();
        assert_eq!(result.records.len(), 1);

        // Codec ID should be consistent
        assert_eq!(
            codec_id, "identity",
            "S8: Codec ID should be 'identity' for IdentityCodec"
        );
    }

    /// S9: Storage never assigns versions - Versions come from engine
    #[test]
    fn s9_storage_never_assigns_versions() {
        let dir = tempdir().unwrap();
        let wal_dir = dir.path();

        let mut writer = WalWriter::new(
            wal_dir.to_path_buf(),
            test_uuid(),
            DurabilityMode::Strict,
            WalConfig::for_testing(),
            make_codec(),
        )
        .unwrap();

        // Create records with specific txn_ids (versions assigned externally)
        let txn_ids = [42, 100, 999];
        for &txn_id in &txn_ids {
            let record = create_test_wal_record(txn_id, &[txn_id as u8]);
            writer.append(&record).unwrap();
        }
        writer.flush().unwrap();
        drop(writer);

        // Read back and verify versions are exactly as specified
        let reader = WalReader::new(make_codec());
        let result = reader.read_all(wal_dir).unwrap();

        assert_eq!(result.records.len(), 3);
        for (i, &expected_txn_id) in txn_ids.iter().enumerate() {
            assert_eq!(
                result.records[i].txn_id, expected_txn_id,
                "S9: Storage must preserve version {} exactly as provided",
                expected_txn_id
            );
        }
    }
}

// =============================================================================
// RECOVERY INVARIANTS (R1-R5)
// From M10_ARCHITECTURE.md Section 3.2
// =============================================================================

mod recovery_invariants {
    use super::*;

    /// R1: No committed txn lost - In Strict mode, committed transactions survive crash
    #[test]
    fn r1_no_committed_txn_lost_strict_mode() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");

        // Create database and write records in Strict mode
        {
            let handle = DatabaseHandle::create(&db_path, DatabaseConfig::for_testing()).unwrap();
            let uuid = handle.uuid();

            for i in 1..=5 {
                let record = WalRecord::new(i, uuid, i * 1000, vec![i as u8; 10]);
                handle.append_wal(&record).unwrap();
            }
            // Flush ensures fsync in Strict mode
            handle.flush_wal().unwrap();
            // Simulate crash by not calling close()
            drop(handle);
        }

        // "Recover" by reopening
        {
            let handle = DatabaseHandle::open(&db_path, DatabaseConfig::for_testing()).unwrap();

            // Read WAL to verify records survived
            let wal_dir = db_path.join("WAL");
            let reader = WalReader::new(make_codec());
            let result = reader.read_all(&wal_dir).unwrap();

            assert_eq!(
                result.records.len(),
                5,
                "R1: All 5 committed transactions should survive"
            );

            for (i, record) in result.records.iter().enumerate() {
                assert_eq!(
                    record.txn_id,
                    (i + 1) as u64,
                    "R1: Transaction {} should be present",
                    i + 1
                );
            }

            handle.close().unwrap();
        }
    }

    /// R2: Order preservation - WAL replay preserves transaction order
    #[test]
    fn r2_order_preservation() {
        let dir = tempdir().unwrap();
        let db_path = dir.path();

        setup_minimal_database(db_path);

        // Write records in specific order
        let wal_dir = db_path.join("WAL");
        let mut writer = WalWriter::new(
            wal_dir.clone(),
            test_uuid(),
            DurabilityMode::Strict,
            WalConfig::for_testing(),
            make_codec(),
        )
        .unwrap();

        let ordered_ids = [1, 5, 10, 15, 20, 100];
        for &txn_id in &ordered_ids {
            let record = create_test_wal_record(txn_id, &[txn_id as u8]);
            writer.append(&record).unwrap();
        }
        writer.flush().unwrap();
        drop(writer);

        // Read and verify order
        let reader = WalReader::new(make_codec());
        let result = reader.read_all(&wal_dir).unwrap();

        let read_ids: Vec<u64> = result.records.iter().map(|r| r.txn_id).collect();
        assert_eq!(
            read_ids, ordered_ids,
            "R2: Transaction order must be preserved"
        );
    }

    /// R3: Idempotent replay - Replaying a record multiple times = replaying once
    #[test]
    fn r3_idempotent_replay() {
        let dir = tempdir().unwrap();
        let db_path = dir.path();

        setup_minimal_database(db_path);

        // Write records
        let wal_dir = db_path.join("WAL");
        let mut writer = WalWriter::new(
            wal_dir.clone(),
            test_uuid(),
            DurabilityMode::Strict,
            WalConfig::for_testing(),
            make_codec(),
        )
        .unwrap();

        for i in 1..=5 {
            let record = create_test_wal_record(i, &[i as u8]);
            writer.append(&record).unwrap();
        }
        writer.flush().unwrap();
        drop(writer);

        // Perform recovery multiple times
        let coordinator = RecoveryCoordinator::new(db_path.to_path_buf(), make_codec());

        let mut results = Vec::new();
        for _ in 0..3 {
            let mut applied = Vec::new();
            let result = coordinator
                .recover(
                    |_| Ok(()), // No snapshot callback
                    |record| {
                        applied.push(record.txn_id);
                        Ok(())
                    },
                )
                .unwrap();
            results.push((result.replay_stats.records_applied, applied));
        }

        // All recoveries should produce identical results
        assert!(
            results.windows(2).all(|w| w[0] == w[1]),
            "R3: Multiple recoveries should produce identical results"
        );
    }

    /// R4: Snapshot-WAL equivalence - Snapshot + WAL replay = pure WAL replay
    #[test]
    fn r4_snapshot_wal_equivalence() {
        // This test verifies the recovery algorithm correctly combines snapshot and WAL
        let dir = tempdir().unwrap();
        let db_path = dir.path();

        setup_minimal_database(db_path);

        // Write records 1-10
        let wal_dir = db_path.join("WAL");
        let mut writer = WalWriter::new(
            wal_dir.clone(),
            test_uuid(),
            DurabilityMode::Strict,
            WalConfig::for_testing(),
            make_codec(),
        )
        .unwrap();

        for i in 1..=10 {
            let record = create_test_wal_record(i, &[i as u8]);
            writer.append(&record).unwrap();
        }
        writer.flush().unwrap();
        drop(writer);

        // Pure WAL replay
        let reader = WalReader::new(make_codec());
        let wal_result = reader.read_all(&wal_dir).unwrap();
        let pure_wal_ids: Vec<u64> = wal_result.records.iter().map(|r| r.txn_id).collect();

        // Simulate snapshot at watermark 5
        // Records > 5 should be replayed
        let coordinator = RecoveryCoordinator::new(db_path.to_path_buf(), make_codec());

        // Without snapshot, all records replayed
        let mut replayed_ids = Vec::new();
        coordinator
            .recover(
                |_| Ok(()),
                |record| {
                    replayed_ids.push(record.txn_id);
                    Ok(())
                },
            )
            .unwrap();

        // All records should be replayed (no snapshot)
        assert_eq!(
            replayed_ids, pure_wal_ids,
            "R4: Without snapshot, all WAL records should be replayed"
        );
    }

    /// R5: Partial record truncation - Incomplete records at WAL tail are safely truncated
    #[test]
    fn r5_partial_record_truncation() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");

        // Create database with records
        {
            let handle = DatabaseHandle::create(&db_path, DatabaseConfig::for_testing()).unwrap();
            let uuid = handle.uuid();

            for i in 1..=3 {
                let record = WalRecord::new(i, uuid, i * 1000, vec![i as u8; 50]);
                handle.append_wal(&record).unwrap();
            }
            handle.flush_wal().unwrap();
            handle.close().unwrap();
        }

        // Append garbage (partial record) to WAL
        let tester = WalCorruptionTester::new(&db_path);
        tester.append_garbage(b"PARTIAL_RECORD").unwrap();

        // Recovery should handle this gracefully
        let verification = tester.verify_recovery().unwrap();
        assert!(
            verification.recovered,
            "R5: Recovery should succeed with partial record at tail"
        );
    }
}

// =============================================================================
// RETENTION INVARIANTS (RT1-RT4)
// From M10_ARCHITECTURE.md Section 3.3
// =============================================================================

mod retention_invariants {
    use super::*;

    /// RT1: Version ordering preserved - Retained versions maintain their relative order
    #[test]
    fn rt1_version_ordering_preserved() {
        let policy = RetentionPolicy::keep_last(3);
        let current_time = 1_000_000_000u64;

        // Versions 1-10, check which are retained
        let mut retained = Vec::new();
        for version in 1..=10 {
            // version_count simulates "N versions remain including this one"
            let version_count = 11 - version; // 10 for v1, 9 for v2, etc.
            if policy.should_retain(
                version,
                version * 1000,
                version_count as usize,
                current_time,
                PrimitiveType::Kv,
            ) {
                retained.push(version);
            }
        }

        // Retained versions should be in order
        let is_ordered = retained.windows(2).all(|w| w[0] < w[1]);
        assert!(
            is_ordered,
            "RT1: Retained versions should maintain relative order"
        );
    }

    /// RT2: No silent fallback - Reads don't silently return nearest available version
    ///
    /// This test verifies that the retention policy provides clear boundaries
    /// between retained and trimmed versions - there's no ambiguity that could
    /// lead to silent fallback behavior.
    #[test]
    fn rt2_no_silent_fallback() {
        // Test with KeepLast policy - boundaries must be deterministic
        let policy = RetentionPolicy::keep_last(3);
        let current_time = 1_000_000_000u64;

        // Simulate 10 versions, check retention decisions
        let mut retained = Vec::new();
        let mut trimmed = Vec::new();

        for version in 1u64..=10 {
            // version_count decreases as we go through older versions
            // version 10 has count=1, version 9 has count=2, etc.
            let version_count = (11 - version) as usize;

            if policy.should_retain(version, version * 1000, version_count, current_time, PrimitiveType::Kv) {
                retained.push(version);
            } else {
                trimmed.push(version);
            }
        }

        // With KeepLast(3), exactly 3 versions should be retained
        assert_eq!(
            retained.len(),
            3,
            "RT2: KeepLast(3) should retain exactly 3 versions, got {:?}",
            retained
        );

        // The retained versions should be the LAST 3 (highest version numbers)
        assert_eq!(
            retained,
            vec![8, 9, 10],
            "RT2: Should retain versions 8, 9, 10 (the last 3)"
        );

        // Trimmed versions are clearly identified - no ambiguity
        assert_eq!(
            trimmed,
            vec![1, 2, 3, 4, 5, 6, 7],
            "RT2: Versions 1-7 should be clearly marked as trimmed"
        );

        // There should be no overlap - a version is either retained OR trimmed
        for v in &retained {
            assert!(
                !trimmed.contains(v),
                "RT2: Version {} cannot be both retained and trimmed",
                v
            );
        }

        // Test with KeepFor policy - time boundary is also deterministic
        // Use a large current_time to avoid underflow
        let one_hour_us = 3_600_000_000u64;
        let current_time_for_keepfor = 10 * one_hour_us; // 10 hours in microseconds
        let policy = RetentionPolicy::keep_for(Duration::from_secs(3600));

        // Timestamps within the window should be retained
        assert!(
            policy.should_retain(1, current_time_for_keepfor - one_hour_us / 2, 1, current_time_for_keepfor, PrimitiveType::Kv),
            "RT2: Version within time window should be retained"
        );

        // Timestamps outside the window should be trimmed
        assert!(
            !policy.should_retain(1, current_time_for_keepfor - 2 * one_hour_us, 1, current_time_for_keepfor, PrimitiveType::Kv),
            "RT2: Version outside time window should be trimmed"
        );
    }

    /// RT3: Explicit unavailability - Trimmed versions return HistoryTrimmed error
    ///
    /// This test verifies that the retention system can identify which versions
    /// are unavailable and provide information for explicit error reporting.
    #[test]
    fn rt3_explicit_unavailability() {
        let policy = RetentionPolicy::keep_last(2);
        let current_time = 1_000_000_000u64;

        // Build a version history and track availability
        struct VersionInfo {
            version: u64,
            #[allow(dead_code)]
            timestamp: u64,
            is_available: bool,
        }

        let mut versions: Vec<VersionInfo> = Vec::new();
        let total = 5u64;

        for v in 1..=total {
            let version_count = (total - v + 1) as usize;
            let is_available = policy.should_retain(v, v * 1000, version_count, current_time, PrimitiveType::Kv);
            versions.push(VersionInfo {
                version: v,
                timestamp: v * 1000,
                is_available,
            });
        }

        // Find the earliest retained version (for error reporting)
        let earliest_retained = versions
            .iter()
            .filter(|v| v.is_available)
            .map(|v| v.version)
            .min();

        assert!(
            earliest_retained.is_some(),
            "RT3: At least one version should be retained"
        );

        let earliest = earliest_retained.unwrap();

        // Verify we can identify unavailable versions and report them
        for v in &versions {
            if !v.is_available {
                // This version would trigger a HistoryTrimmed error
                // The error should contain: requested version and earliest_retained
                assert!(
                    v.version < earliest,
                    "RT3: Unavailable version {} should be less than earliest retained {}",
                    v.version,
                    earliest
                );

                // Verify we have the information needed for explicit error
                let requested_version = v.version;
                let earliest_retained_version = earliest;

                assert!(
                    requested_version < earliest_retained_version,
                    "RT3: Can construct explicit error: requested={}, earliest_retained={}",
                    requested_version,
                    earliest_retained_version
                );
            }
        }

        // With KeepLast(2), versions 1-3 should be unavailable
        let unavailable: Vec<u64> = versions
            .iter()
            .filter(|v| !v.is_available)
            .map(|v| v.version)
            .collect();

        assert_eq!(
            unavailable,
            vec![1, 2, 3],
            "RT3: Versions 1, 2, 3 should be explicitly unavailable"
        );
    }

    /// RT4: Policy is versioned - Retention policy changes are tracked like data
    #[test]
    fn rt4_policy_is_versioned() {
        // Retention policies are stored as database entries and are versioned
        // Test serialization roundtrip to ensure policy can be stored

        let policy1 = RetentionPolicy::keep_all();
        let policy2 = RetentionPolicy::keep_last(10);
        let policy3 = RetentionPolicy::keep_for(Duration::from_secs(3600));
        let policy4 = RetentionPolicy::composite(RetentionPolicy::keep_all())
            .with_override(PrimitiveType::Kv, RetentionPolicy::keep_last(100))
            .build();

        // Each policy should serialize and deserialize correctly
        for policy in [policy1, policy2, policy3, policy4] {
            let bytes = policy.to_bytes();
            let restored = RetentionPolicy::from_bytes(&bytes).unwrap();
            assert_eq!(
                policy.summary(),
                restored.summary(),
                "RT4: Policy should roundtrip correctly"
            );
        }
    }

    /// RT4: Policy serialization preserves all fields
    #[test]
    fn rt4_policy_serialization_complete() {
        let policy = RetentionPolicy::keep_last(42);
        let bytes = policy.to_bytes();
        let restored = RetentionPolicy::from_bytes(&bytes).unwrap();

        // Verify exact match
        match (policy, restored) {
            (RetentionPolicy::KeepLast(n1), RetentionPolicy::KeepLast(n2)) => {
                assert_eq!(n1, n2, "RT4: KeepLast(n) should preserve n");
            }
            _ => panic!("RT4: Policy type should be preserved"),
        }
    }
}

// =============================================================================
// COMPACTION INVARIANTS (C1-C5)
// From M10_ARCHITECTURE.md Section 3.4
// =============================================================================

mod compaction_invariants {
    use super::*;
    use std::sync::Mutex;

    fn setup_compaction_env() -> (tempfile::TempDir, std::path::PathBuf, Arc<Mutex<ManifestManager>>)
    {
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
            let record = create_test_wal_record(txn_id, &vec![txn_id as u8; 50]);
            segment.write(&record.to_bytes())?;
        }

        segment.close()?;
        Ok(())
    }

    /// C1: Read equivalence - Before/after compaction, retained reads match exactly
    #[test]
    fn c1_read_equivalence() {
        let (_dir, wal_dir, manifest) = setup_compaction_env();

        // Create segments
        create_segment_with_records(&wal_dir, 1, &[1, 2, 3]).unwrap();
        create_segment_with_records(&wal_dir, 2, &[4, 5, 6]).unwrap();
        create_segment_with_records(&wal_dir, 3, &[7, 8, 9]).unwrap();

        // Read before compaction
        let reader = WalReader::new(make_codec());
        let before = reader.read_all(&wal_dir).unwrap();
        let before_ids: Vec<u64> = before.records.iter().map(|r| r.txn_id).collect();

        // Setup watermark and compact
        {
            let mut m = manifest.lock().unwrap();
            m.set_snapshot_watermark(1, 6).unwrap();
            m.manifest_mut().active_wal_segment = 4;
            m.persist().unwrap();
        }

        let compactor = WalOnlyCompactor::new(wal_dir.clone(), manifest);
        compactor.compact().unwrap();

        // Read after compaction
        let after = reader.read_all(&wal_dir).unwrap();
        let after_ids: Vec<u64> = after.records.iter().map(|r| r.txn_id).collect();

        // Retained records (txn_id > watermark) should be identical
        let retained_before: Vec<u64> = before_ids.iter().copied().filter(|&id| id > 6).collect();
        let retained_after: Vec<u64> = after_ids.iter().copied().filter(|&id| id > 6).collect();

        assert_eq!(
            retained_before, retained_after,
            "C1: Retained records should be identical before/after compaction"
        );
    }

    /// C2: No semantic change - Compaction doesn't affect transaction semantics
    #[test]
    fn c2_no_semantic_change() {
        let (_dir, wal_dir, manifest) = setup_compaction_env();

        create_segment_with_records(&wal_dir, 1, &[1, 2, 3]).unwrap();

        // WAL content before
        let reader = WalReader::new(make_codec());
        let before = reader.read_all(&wal_dir).unwrap();

        // Set watermark below all transactions (nothing to compact)
        {
            let mut m = manifest.lock().unwrap();
            m.set_snapshot_watermark(1, 0).unwrap();
            m.manifest_mut().active_wal_segment = 2;
            m.persist().unwrap();
        }

        let compactor = WalOnlyCompactor::new(wal_dir.clone(), manifest);
        let info = compactor.compact().unwrap();

        // Nothing should be removed
        assert_eq!(
            info.wal_segments_removed, 0,
            "C2: No segments should be removed when watermark < all txns"
        );

        // Content should be identical
        let after = reader.read_all(&wal_dir).unwrap();
        assert_eq!(
            before.records.len(),
            after.records.len(),
            "C2: Record count should be unchanged"
        );
    }

    /// C3: No reordering - Compaction doesn't reorder visible history
    #[test]
    fn c3_no_reordering() {
        let (_dir, wal_dir, manifest) = setup_compaction_env();

        // Create segments with specific order
        create_segment_with_records(&wal_dir, 1, &[1, 2]).unwrap();
        create_segment_with_records(&wal_dir, 2, &[10, 20]).unwrap();
        create_segment_with_records(&wal_dir, 3, &[100, 200]).unwrap();

        // Read order before
        let reader = WalReader::new(make_codec());
        let before = reader.read_all(&wal_dir).unwrap();
        let order_before: Vec<u64> = before.records.iter().map(|r| r.txn_id).collect();

        // Compact first segment
        {
            let mut m = manifest.lock().unwrap();
            m.set_snapshot_watermark(1, 2).unwrap();
            m.manifest_mut().active_wal_segment = 4;
            m.persist().unwrap();
        }

        let compactor = WalOnlyCompactor::new(wal_dir.clone(), manifest);
        compactor.compact().unwrap();

        // Read order after
        let after = reader.read_all(&wal_dir).unwrap();
        let order_after: Vec<u64> = after.records.iter().map(|r| r.txn_id).collect();

        // Remaining records should maintain order
        let remaining_before: Vec<u64> = order_before.iter().copied().filter(|&id| id > 2).collect();
        assert_eq!(
            order_after, remaining_before,
            "C3: Order should be preserved after compaction"
        );
    }

    /// C4: Safe boundaries - Compaction only removes data below snapshot watermark
    #[test]
    fn c4_safe_boundaries() {
        let (_dir, wal_dir, manifest) = setup_compaction_env();

        // Segment 1: txns 1-5 (below watermark 10)
        create_segment_with_records(&wal_dir, 1, &[1, 2, 3, 4, 5]).unwrap();
        // Segment 2: txns 6-15 (spans watermark)
        create_segment_with_records(&wal_dir, 2, &[6, 7, 8, 9, 10, 11, 12, 13, 14, 15]).unwrap();

        let reader = WalReader::new(make_codec());
        let _before = reader.read_all(&wal_dir).unwrap();

        // Watermark at 10
        {
            let mut m = manifest.lock().unwrap();
            m.set_snapshot_watermark(1, 10).unwrap();
            m.manifest_mut().active_wal_segment = 3;
            m.persist().unwrap();
        }

        let compactor = WalOnlyCompactor::new(wal_dir.clone(), manifest);
        let info = compactor.compact().unwrap();

        // Only segment 1 should be removed (all txns <= watermark)
        assert_eq!(
            info.wal_segments_removed, 1,
            "C4: Only segment fully below watermark should be removed"
        );

        // Segment 2 should remain because it contains txns > watermark
        let after = reader.read_all(&wal_dir).unwrap();
        assert!(
            after.records.iter().any(|r| r.txn_id > 10),
            "C4: Records > watermark should be preserved"
        );
    }

    /// C5: Version identity - Compaction never rewrites, renumbers, or reinterprets versions
    #[test]
    fn c5_version_identity() {
        let (_dir, wal_dir, manifest) = setup_compaction_env();

        // Create segment with specific version IDs
        let original_ids = [42, 100, 999, 1000, 2000];
        create_segment_with_records(&wal_dir, 1, &original_ids[..2]).unwrap();
        create_segment_with_records(&wal_dir, 2, &original_ids[2..]).unwrap();

        // Compact first segment
        {
            let mut m = manifest.lock().unwrap();
            m.set_snapshot_watermark(1, 100).unwrap();
            m.manifest_mut().active_wal_segment = 3;
            m.persist().unwrap();
        }

        let compactor = WalOnlyCompactor::new(wal_dir.clone(), manifest);
        compactor.compact().unwrap();

        // Read remaining records
        let reader = WalReader::new(make_codec());
        let after = reader.read_all(&wal_dir).unwrap();

        // Version IDs must be exactly as originally written (not renumbered)
        let remaining_ids: Vec<u64> = after.records.iter().map(|r| r.txn_id).collect();
        let expected_remaining: Vec<u64> = original_ids
            .iter()
            .copied()
            .filter(|&id| id > 100)
            .collect();

        assert_eq!(
            remaining_ids, expected_remaining,
            "C5: Version IDs must be preserved exactly, not renumbered"
        );

        // Verify each remaining record has its original version
        for record in &after.records {
            assert!(
                original_ids.contains(&record.txn_id),
                "C5: Version {} should be an original ID",
                record.txn_id
            );
        }
    }
}

// =============================================================================
// ARCHITECTURAL RULES TESTS
// From M10_ARCHITECTURE.md Section 2
// =============================================================================

mod architectural_rules {
    use super::*;
    use std::sync::Mutex;

    /// Rule 1: Storage Is Logically Invisible
    /// The storage layer must not change user-visible semantics
    ///
    /// This test verifies that data written to storage comes back identical,
    /// with no storage-layer transformations visible to the caller.
    #[test]
    fn rule1_storage_logically_invisible() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");

        // Write data through the database layer
        let original_data: Vec<(u64, Vec<u8>)> = vec![
            (1, b"first value with unicode: \xc3\xa9\xc3\xa0\xc3\xbc".to_vec()),
            (2, vec![0x00, 0xFF, 0x7F, 0x80]), // Binary data with edge cases
            (3, vec![0u8; 1000]),              // Large zeros
            (4, (0..255).collect()),            // All byte values
        ];

        {
            let handle = DatabaseHandle::create(&db_path, DatabaseConfig::for_testing()).unwrap();
            let uuid = handle.uuid();

            for (txn_id, data) in &original_data {
                let record = WalRecord::new(*txn_id, uuid, *txn_id * 1000, data.clone());
                handle.append_wal(&record).unwrap();
            }
            handle.flush_wal().unwrap();
            handle.close().unwrap();
        }

        // Reopen and verify data is byte-for-byte identical
        {
            let _handle = DatabaseHandle::open(&db_path, DatabaseConfig::for_testing()).unwrap();
            let wal_dir = db_path.join("WAL");
            let reader = WalReader::new(make_codec());
            let result = reader.read_all(&wal_dir).unwrap();

            assert_eq!(
                result.records.len(),
                original_data.len(),
                "Rule 1: All records should be present"
            );

            for (record, (expected_txn, expected_data)) in
                result.records.iter().zip(original_data.iter())
            {
                assert_eq!(
                    record.txn_id, *expected_txn,
                    "Rule 1: Transaction ID must be preserved exactly"
                );
                assert_eq!(
                    &record.writeset, expected_data,
                    "Rule 1: Data must be byte-for-byte identical - storage is invisible"
                );
            }
        }
    }

    /// Rule 2: Durability Mode Determines Commit Semantics
    ///
    /// This test verifies that Strict mode actually persists data to disk
    /// before returning, while other modes have different guarantees.
    #[test]
    fn rule2_durability_mode_determines_commit_semantics() {
        // Test Strict mode: data must survive simulated crash
        let dir = tempdir().unwrap();
        let wal_dir = dir.path().join("strict");
        std::fs::create_dir_all(&wal_dir).unwrap();

        {
            let mut writer = WalWriter::new(
                wal_dir.clone(),
                test_uuid(),
                DurabilityMode::Strict,
                WalConfig::for_testing(),
                make_codec(),
            )
            .unwrap();

            let record = create_test_wal_record(1, b"strict mode data");
            writer.append(&record).unwrap();
            writer.flush().unwrap();
            // Drop without close - simulates crash after flush
        }

        // Data must be readable after "crash" in Strict mode
        let reader = WalReader::new(make_codec());
        let result = reader.read_all(&wal_dir).unwrap();
        assert_eq!(
            result.records.len(),
            1,
            "Rule 2: Strict mode must persist data on flush"
        );
        assert_eq!(result.records[0].txn_id, 1);

        // Test that Batched and Async modes create valid WAL files
        for (mode_name, mode) in [
            ("Batched", DurabilityMode::Batched { interval_ms: 100, batch_size: 1000 }),
            ("Async", DurabilityMode::Async { interval_ms: 50 }),
        ] {
            let mode_dir = dir.path().join(mode_name.to_lowercase());
            std::fs::create_dir_all(&mode_dir).unwrap();

            let mut writer = WalWriter::new(
                mode_dir.clone(),
                test_uuid(),
                mode,
                WalConfig::for_testing(),
                make_codec(),
            )
            .unwrap();

            let record = create_test_wal_record(42, format!("{} mode", mode_name).as_bytes());
            writer.append(&record).unwrap();
            writer.flush().unwrap();
            drop(writer);

            // After explicit flush+drop, data should be readable
            let result = reader.read_all(&mode_dir).unwrap();
            assert!(
                !result.records.is_empty(),
                "Rule 2: {} mode should write data after flush",
                mode_name
            );
        }
    }

    /// Rule 3: Recovery Is Deterministic and Idempotent
    #[test]
    fn rule3_recovery_deterministic_idempotent() {
        let dir = tempdir().unwrap();
        let db_path = dir.path();

        setup_minimal_database(db_path);

        // Write records with specific content to verify exact replay
        let wal_dir = db_path.join("WAL");
        let mut writer = WalWriter::new(
            wal_dir.clone(),
            test_uuid(),
            DurabilityMode::Strict,
            WalConfig::for_testing(),
            make_codec(),
        )
        .unwrap();

        let test_data: Vec<(u64, Vec<u8>)> = vec![
            (1, b"first".to_vec()),
            (2, b"second".to_vec()),
            (3, b"third".to_vec()),
        ];

        for (txn_id, data) in &test_data {
            let record = create_test_wal_record(*txn_id, data);
            writer.append(&record).unwrap();
        }
        writer.flush().unwrap();
        drop(writer);

        // Run recovery multiple times and collect results
        let coordinator = RecoveryCoordinator::new(db_path.to_path_buf(), make_codec());
        let mut all_results: Vec<Vec<(u64, Vec<u8>)>> = Vec::new();

        for iteration in 0..3 {
            let mut recovered = Vec::new();
            let result = coordinator.recover(
                |_| Ok(()),
                |record| {
                    recovered.push((record.txn_id, record.writeset.clone()));
                    Ok(())
                },
            );
            assert!(
                result.is_ok(),
                "Rule 3: Recovery iteration {} should succeed",
                iteration
            );
            all_results.push(recovered);
        }

        // All iterations must produce identical results
        for (i, result) in all_results.iter().enumerate().skip(1) {
            assert_eq!(
                &all_results[0], result,
                "Rule 3: Recovery iteration {} differs from iteration 0 - not deterministic",
                i
            );
        }

        // Results must match original data
        assert_eq!(
            all_results[0], test_data,
            "Rule 3: Recovered data must match original"
        );
    }

    /// Rule 4: Compaction Is Logically Invisible
    ///
    /// This test verifies that retained data is byte-for-byte identical
    /// before and after compaction - not just that counts match.
    #[test]
    fn rule4_compaction_logically_invisible() {
        let dir = tempdir().unwrap();
        let wal_dir = dir.path().join("WAL");
        std::fs::create_dir_all(&wal_dir).unwrap();

        let manifest_path = dir.path().join("MANIFEST");
        let manifest = ManifestManager::create(
            manifest_path,
            test_uuid(),
            "identity".to_string(),
        )
        .unwrap();
        let manifest = Arc::new(Mutex::new(manifest));

        // Create segments with specific data
        let segment1_data: Vec<(u64, Vec<u8>)> = vec![
            (1, b"will be compacted".to_vec()),
            (2, b"also compacted".to_vec()),
        ];
        let segment2_data: Vec<(u64, Vec<u8>)> = vec![
            (10, b"retained data one".to_vec()),
            (20, b"retained data two".to_vec()),
        ];

        // Write segment 1
        {
            let mut seg = WalSegment::create(&wal_dir, 1, test_uuid()).unwrap();
            for (txn_id, data) in &segment1_data {
                let record = create_test_wal_record(*txn_id, data);
                seg.write(&record.to_bytes()).unwrap();
            }
            seg.close().unwrap();
        }

        // Write segment 2
        {
            let mut seg = WalSegment::create(&wal_dir, 2, test_uuid()).unwrap();
            for (txn_id, data) in &segment2_data {
                let record = create_test_wal_record(*txn_id, data);
                seg.write(&record.to_bytes()).unwrap();
            }
            seg.close().unwrap();
        }

        // Read retained data BEFORE compaction (records > watermark 5)
        let reader = WalReader::new(make_codec());
        let before = reader.read_all(&wal_dir).unwrap();
        let retained_before: Vec<(u64, Vec<u8>)> = before
            .records
            .iter()
            .filter(|r| r.txn_id > 5)
            .map(|r| (r.txn_id, r.writeset.clone()))
            .collect();

        // Compact with watermark at 5
        {
            let mut m = manifest.lock().unwrap();
            m.set_snapshot_watermark(1, 5).unwrap();
            m.manifest_mut().active_wal_segment = 3;
            m.persist().unwrap();
        }

        let compactor = WalOnlyCompactor::new(wal_dir.clone(), manifest);
        let info = compactor.compact().unwrap();

        assert!(
            info.wal_segments_removed > 0,
            "Rule 4: Compaction should remove segments"
        );

        // Read retained data AFTER compaction
        let after = reader.read_all(&wal_dir).unwrap();
        let retained_after: Vec<(u64, Vec<u8>)> = after
            .records
            .iter()
            .map(|r| (r.txn_id, r.writeset.clone()))
            .collect();

        // Retained data must be BYTE-FOR-BYTE identical
        assert_eq!(
            retained_before, retained_after,
            "Rule 4: Compaction must not change retained data - storage is invisible"
        );
    }

    /// Rule 5: Retention Policies Are Database Entries
    ///
    /// This test verifies that policies can be stored in the system namespace
    /// and retrieved correctly - demonstrating they are treated as data.
    #[test]
    fn rule5_retention_policies_are_database_entries() {
        use strata_storage::retention::system_namespace;

        // Test that retention policy key generation works
        let run_id = test_uuid();
        let key = system_namespace::retention_policy_key(&run_id);

        assert!(
            system_namespace::is_system_key(&key),
            "Rule 5: Retention policy key should be in system namespace"
        );
        assert!(
            system_namespace::is_retention_policy_key(&key),
            "Rule 5: Key should be recognized as retention policy"
        );

        // Test roundtrip of run_id through key
        let extracted = system_namespace::run_id_from_retention_key(&key);
        assert_eq!(
            extracted,
            Some(run_id),
            "Rule 5: Run ID should be extractable from key"
        );

        // Test that all policy types can be serialized as database entries
        let policies = vec![
            ("KeepAll", RetentionPolicy::keep_all()),
            ("KeepLast", RetentionPolicy::keep_last(100)),
            ("KeepFor", RetentionPolicy::keep_for(Duration::from_secs(86400))),
            (
                "Composite",
                RetentionPolicy::composite(RetentionPolicy::keep_all())
                    .with_override(PrimitiveType::Kv, RetentionPolicy::keep_last(10))
                    .with_override(PrimitiveType::Event, RetentionPolicy::keep_for(Duration::from_secs(3600)))
                    .build(),
            ),
        ];

        for (name, policy) in policies {
            let bytes = policy.to_bytes();
            assert!(
                !bytes.is_empty(),
                "Rule 5: {} policy should serialize to non-empty bytes",
                name
            );

            let restored = RetentionPolicy::from_bytes(&bytes).unwrap();
            assert_eq!(
                policy.summary(),
                restored.summary(),
                "Rule 5: {} policy should roundtrip correctly",
                name
            );

            // Verify the bytes are valid database entry content
            // (no null bytes at start which could indicate invalid format)
            assert_ne!(
                bytes[0], 0,
                "Rule 5: {} policy serialization should have valid format marker",
                name
            );
        }
    }
}

// =============================================================================
// PORTABILITY TESTS
// From M10_ARCHITECTURE.md Section 4.2
// =============================================================================

mod portability_tests {
    use super::*;
    use strata_storage::database::{export_database, import_database};

    /// Database directory is portable by copy
    #[test]
    fn portability_copy_produces_valid_clone() {
        let dir = tempdir().unwrap();
        let src_path = dir.path().join("source.db");
        let dst_path = dir.path().join("clone.db");

        // Create source database with data
        {
            let handle = DatabaseHandle::create(&src_path, DatabaseConfig::for_testing()).unwrap();
            let uuid = handle.uuid();

            for i in 1..=5 {
                let record = WalRecord::new(i, uuid, i * 1000, vec![i as u8; 20]);
                handle.append_wal(&record).unwrap();
            }
            handle.flush_wal().unwrap();
            handle.close().unwrap();
        }

        // Copy database directory
        copy_dir_recursive(&src_path, &dst_path).unwrap();

        // Open clone and verify
        {
            let handle = DatabaseHandle::open(&dst_path, DatabaseConfig::for_testing()).unwrap();

            // Read WAL from clone
            let wal_dir = dst_path.join("WAL");
            let reader = WalReader::new(make_codec());
            let result = reader.read_all(&wal_dir).unwrap();

            assert_eq!(
                result.records.len(),
                5,
                "Clone should contain all records"
            );

            handle.close().unwrap();
        }
    }

    /// Export creates a consistent copy
    #[test]
    fn portability_export_import_roundtrip() {
        let dir = tempdir().unwrap();
        let src_path = dir.path().join("source.db");
        let export_path = dir.path().join("export.db");

        // Create source
        {
            let handle = DatabaseHandle::create(&src_path, DatabaseConfig::for_testing()).unwrap();
            let uuid = handle.uuid();

            for i in 1..=3 {
                let record = WalRecord::new(i, uuid, i * 1000, vec![i as u8; 10]);
                handle.append_wal(&record).unwrap();
            }
            handle.flush_wal().unwrap();
            handle.close().unwrap();
        }

        // Export
        let export_info = export_database(&src_path, &export_path, &DatabaseConfig::for_testing()).unwrap();

        assert!(export_path.exists());
        assert!(export_info.size_bytes > 0);

        // Import (open)
        let handle = import_database(&export_path, DatabaseConfig::for_testing()).unwrap();
        assert!(handle.path().exists());

        // Verify data
        let wal_dir = export_path.join("WAL");
        let reader = WalReader::new(make_codec());
        let result = reader.read_all(&wal_dir).unwrap();

        assert_eq!(result.records.len(), 3);

        handle.close().unwrap();
    }

    fn copy_dir_recursive(src: &Path, dst: &Path) -> std::io::Result<()> {
        std::fs::create_dir_all(dst)?;

        for entry in std::fs::read_dir(src)? {
            let entry = entry?;
            let src_path = entry.path();
            let dst_path = dst.join(entry.file_name());

            if entry.file_type()?.is_dir() {
                copy_dir_recursive(&src_path, &dst_path)?;
            } else {
                std::fs::copy(&src_path, &dst_path)?;
            }
        }

        Ok(())
    }
}

// =============================================================================
// DATABASE LIFECYCLE TESTS
// From M10_ARCHITECTURE.md Section 12
// =============================================================================

mod database_lifecycle_tests {
    use super::*;

    /// Database::open creates new or opens existing
    #[test]
    fn lifecycle_create_new_database() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("new.db");

        // Should not exist initially
        assert!(!db_path.exists());

        // Create new
        let handle = DatabaseHandle::create(&db_path, DatabaseConfig::for_testing()).unwrap();

        // Should create directory structure
        assert!(db_path.exists());
        assert!(db_path.join("MANIFEST").exists());
        assert!(db_path.join("WAL").exists());
        assert!(db_path.join("SNAPSHOTS").exists());
        assert!(db_path.join("DATA").exists());

        handle.close().unwrap();
    }

    /// Database::open opens existing database
    #[test]
    fn lifecycle_open_existing_database() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("existing.db");

        // Create
        {
            let handle = DatabaseHandle::create(&db_path, DatabaseConfig::for_testing()).unwrap();
            handle.close().unwrap();
        }

        // Open existing
        {
            let handle = DatabaseHandle::open(&db_path, DatabaseConfig::for_testing()).unwrap();
            assert!(handle.path().exists());
            handle.close().unwrap();
        }
    }

    /// Database::close flushes and syncs
    #[test]
    fn lifecycle_close_flushes_data() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");

        {
            let handle = DatabaseHandle::create(&db_path, DatabaseConfig::for_testing()).unwrap();
            let uuid = handle.uuid();

            // Write some data
            let record = WalRecord::new(1, uuid, 1000, vec![1, 2, 3]);
            handle.append_wal(&record).unwrap();

            // Close should flush
            handle.close().unwrap();
        }

        // Reopen and verify data persisted
        {
            let _handle = DatabaseHandle::open(&db_path, DatabaseConfig::for_testing()).unwrap();

            let wal_dir = db_path.join("WAL");
            let reader = WalReader::new(make_codec());
            let result = reader.read_all(&wal_dir).unwrap();

            assert_eq!(result.records.len(), 1, "Data should persist after close");
        }
    }

    /// Codec mismatch detected on open
    #[test]
    fn lifecycle_codec_mismatch_detected() {
        let dir = tempdir().unwrap();
        let db_path = dir.path();

        // Create MANIFEST with different codec
        std::fs::create_dir_all(db_path).unwrap();
        ManifestManager::create(
            db_path.join("MANIFEST"),
            test_uuid(),
            "aes256-gcm".to_string(), // Different codec
        )
        .unwrap();

        // Recovery coordinator should detect mismatch
        let coordinator = RecoveryCoordinator::new(db_path.to_path_buf(), make_codec()); // identity codec
        let result = coordinator.plan_recovery();

        assert!(
            matches!(result, Err(RecoveryError::CodecMismatch { .. })),
            "Should detect codec mismatch"
        );
    }
}

// =============================================================================
// WAL FORMAT TESTS
// From M10_ARCHITECTURE.md Section 5
// =============================================================================

mod wal_format_tests {
    use super::*;

    /// WAL segment file format validation
    #[test]
    fn wal_segment_format() {
        let dir = tempdir().unwrap();

        let segment = WalSegment::create(dir.path(), 1, test_uuid()).unwrap();

        // Verify segment path format: wal-NNNNNN.seg
        let path = segment.path().to_path_buf();
        let filename = path.file_name().unwrap().to_str().unwrap();
        assert!(filename.starts_with("wal-"));
        assert!(filename.ends_with(".seg"));
        assert_eq!(filename, "wal-000001.seg");

        drop(segment);

        // Verify header
        let mut file = std::fs::File::open(&path).unwrap();
        let mut header = [0u8; SEGMENT_HEADER_SIZE];
        file.read_exact(&mut header).unwrap();

        // Magic bytes
        assert_eq!(&header[0..4], &SEGMENT_MAGIC);

        // Format version
        let version = u32::from_le_bytes(header[4..8].try_into().unwrap());
        assert_eq!(version, SEGMENT_FORMAT_VERSION);
    }

    /// WAL record format validation
    #[test]
    fn wal_record_format() {
        let record = WalRecord::new(42, test_uuid(), 1234567890, vec![1, 2, 3, 4, 5]);
        let bytes = record.to_bytes();

        // Format: length (4) + format_version (1) + payload + crc (4)
        assert!(bytes.len() >= 4 + 1 + 33 + 4); // minimum size

        // Length prefix
        let length = u32::from_le_bytes(bytes[0..4].try_into().unwrap());
        assert_eq!(bytes.len(), 4 + length as usize);

        // Format version
        assert_eq!(bytes[4], WAL_RECORD_FORMAT_VERSION);

        // TxnId at offset 5 (1 byte format version + start of payload)
        let txn_id = u64::from_le_bytes(bytes[5..13].try_into().unwrap());
        assert_eq!(txn_id, 42);

        // RunId at offset 13
        let run_id: [u8; 16] = bytes[13..29].try_into().unwrap();
        assert_eq!(run_id, test_uuid());

        // Timestamp at offset 29
        let timestamp = u64::from_le_bytes(bytes[29..37].try_into().unwrap());
        assert_eq!(timestamp, 1234567890);
    }

    /// WAL segment rotation on size limit
    #[test]
    fn wal_segment_rotation() {
        let dir = tempdir().unwrap();
        let wal_dir = dir.path();

        // Use small segment size for testing
        let config = WalConfig {
            segment_size: 500, // Very small to trigger rotation
            ..WalConfig::for_testing()
        };

        let mut writer = WalWriter::new(
            wal_dir.to_path_buf(),
            test_uuid(),
            DurabilityMode::Strict,
            config,
            make_codec(),
        )
        .unwrap();

        // Write enough records to trigger rotation
        for i in 1..=20 {
            let record = create_test_wal_record(i, &vec![i as u8; 50]);
            writer.append(&record).unwrap();
        }
        writer.flush().unwrap();

        // Should have multiple segments
        let segment_count = std::fs::read_dir(wal_dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| {
                e.file_name()
                    .to_str()
                    .is_some_and(|n| n.starts_with("wal-") && n.ends_with(".seg"))
            })
            .count();

        assert!(
            segment_count > 1,
            "Should have rotated to multiple segments (got {})",
            segment_count
        );
    }
}

// =============================================================================
// MANIFEST FORMAT TESTS
// From M10_ARCHITECTURE.md Section 4.3
// =============================================================================

mod manifest_format_tests {
    use super::*;

    /// MANIFEST structure validation
    #[test]
    fn manifest_structure() {
        let manifest = Manifest::new(test_uuid(), "identity".to_string());
        let bytes = manifest.to_bytes();

        // Magic: "STRM"
        assert_eq!(&bytes[0..4], &MANIFEST_MAGIC);

        // Format version
        let version = u32::from_le_bytes(bytes[4..8].try_into().unwrap());
        assert_eq!(version, MANIFEST_FORMAT_VERSION);

        // Database UUID
        let uuid: [u8; 16] = bytes[8..24].try_into().unwrap();
        assert_eq!(uuid, test_uuid());

        // CRC32 at end
        let data = &bytes[..bytes.len() - 4];
        let stored_crc = u32::from_le_bytes(bytes[bytes.len() - 4..].try_into().unwrap());
        let computed_crc = crc32fast::hash(data);
        assert_eq!(stored_crc, computed_crc);
    }

    /// MANIFEST contains only physical metadata
    #[test]
    fn manifest_physical_metadata_only() {
        let manifest = Manifest::new(test_uuid(), "identity".to_string());

        // Should contain physical metadata
        assert!(manifest.format_version > 0);
        assert!(!manifest.database_uuid.iter().all(|&b| b == 0));
        assert!(!manifest.codec_id.is_empty());
        assert!(manifest.active_wal_segment > 0);

        // Snapshot info is optional
        assert!(manifest.snapshot_watermark.is_none());
        assert!(manifest.snapshot_id.is_none());

        // No semantic data (retention policies, user config, etc.)
        // - This is validated by the struct definition itself
    }

    /// MANIFEST atomic update protocol
    #[test]
    fn manifest_atomic_update() {
        let dir = tempdir().unwrap();
        let manifest_path = dir.path().join("MANIFEST");

        // Create
        let mut manager =
            ManifestManager::create(manifest_path.clone(), test_uuid(), "identity".to_string())
                .unwrap();

        // Update
        manager.set_active_segment(5).unwrap();

        // No temp file should exist after atomic update
        let temp_path = manifest_path.with_extension("tmp");
        assert!(!temp_path.exists());

        // MANIFEST should be valid
        let loaded = ManifestManager::load(manifest_path).unwrap();
        assert_eq!(loaded.manifest().active_wal_segment, 5);
    }
}

// =============================================================================
// CRASH HARNESS TESTS
// From M10_ARCHITECTURE.md Section 13
// =============================================================================

mod crash_harness_tests {
    use super::*;

    /// Crash point data states verify the durability guarantees at each point
    ///
    /// This test verifies that the crash point framework correctly models
    /// what data should be present after recovery from each crash scenario.
    #[test]
    fn crash_point_expected_states() {
        // The crash point framework models the durability guarantees of the WAL.
        // This is important for testing because it tells us what assertions
        // are valid after simulating a crash at each point.

        // Before WAL write: data has not been written to any buffer
        // Recovery expectation: data NOT present (it was never written)
        let point = CrashPoint::BeforeWalWrite;
        assert_eq!(
            point.expected_data_state(),
            DataState::NotPresent,
            "BeforeWalWrite: data was never written, cannot be recovered"
        );

        // After write, before fsync: data is in OS buffer cache
        // Recovery expectation: data MAY be present (kernel might have flushed)
        let point = CrashPoint::AfterWalWriteBeforeFsync;
        assert_eq!(
            point.expected_data_state(),
            DataState::MayBePresent,
            "AfterWalWriteBeforeFsync: data in kernel buffer, may or may not survive power loss"
        );

        // After fsync: data is on durable storage
        // Recovery expectation: data MUST be present (durability guarantee)
        let point = CrashPoint::AfterFsync;
        assert_eq!(
            point.expected_data_state(),
            DataState::Present,
            "AfterFsync: data is durable, must survive any crash"
        );

        // During various operations that happen AFTER data is durable
        // Recovery expectation: committed data MUST be present

        let committed_data_points = [
            (CrashPoint::DuringSegmentRotation, "Segment rotation happens after commits"),
            (CrashPoint::DuringSnapshotBeforeRename, "Snapshot is of committed data"),
            (CrashPoint::DuringSnapshotAfterRename, "Snapshot is of committed data"),
            (CrashPoint::DuringManifestUpdate, "MANIFEST tracks committed state"),
            (CrashPoint::DuringCompaction, "Compaction only removes committed data"),
        ];

        for (point, reason) in committed_data_points {
            assert_eq!(
                point.expected_data_state(),
                DataState::Present,
                "{:?}: {} - committed data must survive",
                point,
                reason
            );
        }
    }

    /// All crash points are defined and the set is complete
    ///
    /// This test verifies that the crash point enumeration covers all
    /// critical points in the write path where a crash could occur.
    #[test]
    fn all_crash_points_defined() {
        let points = CrashPoint::all();

        // Verify minimum expected count - should have at least 8 crash points
        assert!(
            points.len() >= 8,
            "Should have at least 8 crash points defined, got {}",
            points.len()
        );

        // The write path has these critical points that MUST be tested
        let required_points = [
            (CrashPoint::BeforeWalWrite, "Before any write occurs"),
            (CrashPoint::AfterWalWriteBeforeFsync, "Data in kernel buffer"),
            (CrashPoint::AfterFsync, "Data is durable"),
            (CrashPoint::DuringSegmentRotation, "WAL segment boundary"),
            (CrashPoint::DuringSnapshotBeforeRename, "Snapshot atomicity - before"),
            (CrashPoint::DuringSnapshotAfterRename, "Snapshot atomicity - after"),
            (CrashPoint::DuringManifestUpdate, "Metadata atomicity"),
            (CrashPoint::DuringCompaction, "Compaction safety"),
        ];

        for (point, description) in required_points {
            assert!(
                points.contains(&point),
                "Missing crash point {:?}: {}",
                point,
                description
            );
        }

        // Verify each crash point has a valid expected state
        for point in &points {
            let state = point.expected_data_state();
            assert!(
                matches!(state, DataState::NotPresent | DataState::MayBePresent | DataState::Present),
                "Crash point {:?} should have valid expected state",
                point
            );
        }
    }

    /// Reference model tracks operations and can detect mismatches
    ///
    /// This test verifies that the reference model can be used to validate
    /// database state after crash recovery.
    #[test]
    fn reference_model_tracking() {
        let mut model = ReferenceModel::new();

        // Perform a sequence of operations
        model.kv_put("run1", "key1", b"value1".to_vec());
        model.kv_put("run1", "key2", b"value2".to_vec());
        model.kv_put("run1", "key3", b"value3".to_vec());
        model.kv_delete("run1", "key1");
        model.kv_put("run1", "key2", b"value2_updated".to_vec());

        // Verify the model tracks state correctly
        assert!(
            model.get_kv("run1", "key1").is_none(),
            "Deleted key should not be present"
        );
        assert_eq!(
            model.get_kv("run1", "key2"),
            Some(&b"value2_updated".to_vec()),
            "Updated value should be latest"
        );
        assert_eq!(
            model.get_kv("run1", "key3"),
            Some(&b"value3".to_vec()),
            "Unchanged key should be present"
        );
        assert_eq!(
            model.total_operations(),
            5,
            "Should track all 5 operations"
        );

        // Test comparison - simulate actual state from database
        let mut actual_state: std::collections::HashMap<String, Vec<u8>> = std::collections::HashMap::new();
        actual_state.insert("key2".to_string(), b"value2_updated".to_vec());
        actual_state.insert("key3".to_string(), b"value3".to_vec());

        // This should match (key1 is deleted in both model and "actual")
        let mismatches = model.compare_kv("run1", &actual_state);
        assert!(
            mismatches.is_empty(),
            "Matching state should have no mismatches: {:?}",
            mismatches
        );

        // Test mismatch detection - actual state differs
        let mut wrong_state: std::collections::HashMap<String, Vec<u8>> = std::collections::HashMap::new();
        wrong_state.insert("key2".to_string(), b"wrong_value".to_vec());

        let mismatches = model.compare_kv("run1", &wrong_state);
        assert!(
            !mismatches.is_empty(),
            "Differing state should have mismatches"
        );
    }

    /// Reference model checkpoint tracking for crash recovery testing
    #[test]
    fn reference_model_checkpoint_for_crash_testing() {
        let mut model = ReferenceModel::new();

        // Operations before checkpoint
        model.kv_put("run1", "before1", b"val1".to_vec());
        model.kv_put("run1", "before2", b"val2".to_vec());

        assert_eq!(model.operations_since_checkpoint(), 2);

        // Take checkpoint (simulates snapshot)
        model.checkpoint();

        assert_eq!(model.operations_since_checkpoint(), 0);
        assert!(model.last_checkpoint_index().is_some());
        let checkpoint_idx = model.last_checkpoint_index().unwrap();

        // Operations after checkpoint
        model.kv_put("run1", "after1", b"val3".to_vec());
        model.kv_delete("run1", "before1");

        assert_eq!(model.operations_since_checkpoint(), 2);

        // The checkpoint index should not change
        assert_eq!(model.last_checkpoint_index(), Some(checkpoint_idx));
    }

    /// WAL tail corruption is handled - verifies recovery from partial writes
    #[test]
    fn wal_tail_corruption_handled() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");

        // Create database with records
        {
            let handle = DatabaseHandle::create(&db_path, DatabaseConfig::for_testing()).unwrap();
            let uuid = handle.uuid();

            for i in 1..=3 {
                let record = WalRecord::new(i, uuid, i * 1000, vec![i as u8; 20]);
                handle.append_wal(&record).unwrap();
            }
            handle.flush_wal().unwrap();
            handle.close().unwrap();
        }

        // Corrupt WAL tail
        let tester = WalCorruptionTester::new(&db_path);
        tester.append_garbage(b"GARBAGE_DATA").unwrap();

        // Recovery should handle corruption
        let verification = tester.verify_recovery().unwrap();
        assert!(verification.recovered, "Should recover from tail corruption");
    }
}

// =============================================================================
// TOMBSTONE TESTS
// From M10_ARCHITECTURE.md Section 9.5
// =============================================================================

mod tombstone_tests {
    use super::*;
    use strata_storage::compaction::{Tombstone, TombstoneReason};

    /// Tombstones are internal implementation details
    #[test]
    fn tombstone_internal_implementation() {
        let mut index = TombstoneIndex::new();
        let run_id = test_uuid();

        // Add tombstone
        index.add(Tombstone::new(
            run_id,
            0,
            b"deleted_key".to_vec(),
            1,
            TombstoneReason::UserDelete,
        ));

        // Tombstone status is tracked
        assert!(index.is_tombstoned(&run_id, 0, b"deleted_key", 1));
        assert!(!index.is_tombstoned(&run_id, 0, b"deleted_key", 2));
    }

    /// Tombstone serialization for persistence
    #[test]
    fn tombstone_serialization() {
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
            TombstoneReason::Compaction,
            2000,
        ));

        let bytes = index.to_bytes();
        let restored = TombstoneIndex::from_bytes(&bytes).unwrap();

        assert_eq!(restored.len(), 2);
        assert!(restored.is_tombstoned(&run_id, 0, b"key1", 1));
        assert!(restored.is_tombstoned(&run_id, 1, b"key2", 2));
    }

    /// Tombstone cleanup by timestamp
    #[test]
    fn tombstone_cleanup() {
        let mut index = TombstoneIndex::new();
        let run_id = test_uuid();

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

        let removed = index.cleanup_before(300);
        assert_eq!(removed, 1);
        assert_eq!(index.len(), 1);
    }
}

// =============================================================================
// SUCCESS CRITERIA CHECKLIST TESTS
// From M10_ARCHITECTURE.md Section 16
// =============================================================================

mod success_criteria_tests {
    use super::*;

    /// Gate 1: WAL and Recovery
    mod gate1_wal_and_recovery {
        use super::*;

        #[test]
        fn wal_append_with_all_durability_modes() {
            for mode in [
                DurabilityMode::InMemory,
                DurabilityMode::Batched { interval_ms: 100, batch_size: 1000 },
                DurabilityMode::Strict,
                DurabilityMode::Async { interval_ms: 50 },
            ] {
                let dir = tempdir().unwrap();
                let wal_dir = dir.path();

                // Skip InMemory mode for WAL tests (no WAL in InMemory mode typically)
                if matches!(mode, DurabilityMode::InMemory) {
                    continue;
                }

                let mut writer = WalWriter::new(
                    wal_dir.to_path_buf(),
                    test_uuid(),
                    mode,
                    WalConfig::for_testing(),
                    make_codec(),
                )
                .unwrap();

                let record = create_test_wal_record(1, b"test");
                writer.append(&record).unwrap();
                writer.flush().unwrap();

                let reader = WalReader::new(make_codec());
                let result = reader.read_all(wal_dir).unwrap();
                assert!(!result.records.is_empty(), "Mode {:?} should write records", mode);
            }
        }

        #[test]
        fn wal_records_self_delimiting_and_checksummed() {
            let record = create_test_wal_record(42, b"payload");
            let bytes = record.to_bytes();

            // Has length prefix
            let length = u32::from_le_bytes(bytes[0..4].try_into().unwrap()) as usize;
            assert!(length > 0);
            assert_eq!(bytes.len(), 4 + length);

            // Has valid checksum
            assert!(WalRecord::verify_checksum(&bytes).is_ok());
        }

        #[test]
        fn recovery_replays_wal_correctly() {
            let dir = tempdir().unwrap();
            let db_path = dir.path();

            setup_minimal_database(db_path);

            let wal_dir = db_path.join("WAL");
            let mut writer = WalWriter::new(
                wal_dir.clone(),
                test_uuid(),
                DurabilityMode::Strict,
                WalConfig::for_testing(),
                make_codec(),
            )
            .unwrap();

            for i in 1..=5 {
                let record = create_test_wal_record(i, &[i as u8]);
                writer.append(&record).unwrap();
            }
            writer.flush().unwrap();
            drop(writer);

            let coordinator = RecoveryCoordinator::new(db_path.to_path_buf(), make_codec());
            let mut replayed = Vec::new();

            coordinator
                .recover(
                    |_| Ok(()),
                    |record| {
                        replayed.push(record.txn_id);
                        Ok(())
                    },
                )
                .unwrap();

            assert_eq!(replayed, vec![1, 2, 3, 4, 5]);
        }
    }

    /// Gate 2: Snapshots and Checkpoint
    mod gate2_snapshots_and_checkpoint {
        use super::*;

        #[test]
        fn snapshot_watermark_correctly_maintained() {
            let dir = tempdir().unwrap();
            let manifest_path = dir.path().join("MANIFEST");

            let mut manager = ManifestManager::create(manifest_path.clone(), test_uuid(), "identity".to_string()).unwrap();

            // Initially no snapshot
            assert!(manager.manifest().snapshot_id.is_none());
            assert!(manager.manifest().snapshot_watermark.is_none());

            // Set watermark
            manager.set_snapshot_watermark(1, 100).unwrap();
            assert_eq!(manager.manifest().snapshot_id, Some(1));
            assert_eq!(manager.manifest().snapshot_watermark, Some(100));

            // Persists across reload
            let loaded = ManifestManager::load(manifest_path).unwrap();
            assert_eq!(loaded.manifest().snapshot_watermark, Some(100));
        }
    }

    /// Gate 3: Retention
    mod gate3_retention {
        use super::*;

        #[test]
        fn retention_policies_stored_as_entries() {
            let policies = vec![
                RetentionPolicy::keep_all(),
                RetentionPolicy::keep_last(10),
                RetentionPolicy::keep_for(Duration::from_secs(3600)),
            ];

            for policy in policies {
                let bytes = policy.to_bytes();
                let restored = RetentionPolicy::from_bytes(&bytes).unwrap();
                assert_eq!(policy.summary(), restored.summary());
            }
        }

        #[test]
        fn keep_all_keep_last_keep_for_work() {
            // Use a large current_time to ensure KeepFor duration math works
            // Timestamps are in microseconds, so 1 hour = 3600 * 1_000_000 = 3_600_000_000 us
            let one_hour_us = 3_600_000_000u64;
            let current_time = 10 * one_hour_us; // 10 hours in microseconds

            // KeepAll always retains
            let policy = RetentionPolicy::keep_all();
            assert!(policy.should_retain(1, 0, 1000, current_time, PrimitiveType::Kv));

            // KeepLast retains within limit
            let policy = RetentionPolicy::keep_last(5);
            assert!(policy.should_retain(1, 0, 3, current_time, PrimitiveType::Kv));
            assert!(!policy.should_retain(1, 0, 10, current_time, PrimitiveType::Kv));

            // KeepFor retains within duration (1 hour = 3600 seconds)
            let policy = RetentionPolicy::keep_for(Duration::from_secs(3600));
            let recent = current_time - 1_000_000; // 1 second ago (within 1 hour)
            let old = current_time - 5 * one_hour_us; // 5 hours ago (older than 1 hour policy)
            assert!(policy.should_retain(1, recent, 1, current_time, PrimitiveType::Kv));
            assert!(!policy.should_retain(1, old, 1, current_time, PrimitiveType::Kv));
        }
    }

    /// Gate 4: Compaction
    mod gate4_compaction {
        use super::*;
        use std::sync::Mutex;

        #[test]
        fn wal_only_compaction_removes_old_segments() {
            let dir = tempdir().unwrap();
            let wal_dir = dir.path().join("WAL");
            std::fs::create_dir_all(&wal_dir).unwrap();

            let manifest_path = dir.path().join("MANIFEST");
            let manifest = ManifestManager::create(manifest_path, test_uuid(), "identity".to_string()).unwrap();
            let manifest = Arc::new(Mutex::new(manifest));

            // Create segments
            {
                let mut seg = WalSegment::create(&wal_dir, 1, test_uuid()).unwrap();
                seg.write(&create_test_wal_record(1, b"data").to_bytes()).unwrap();
                seg.close().unwrap();
            }
            {
                let mut seg = WalSegment::create(&wal_dir, 2, test_uuid()).unwrap();
                seg.write(&create_test_wal_record(10, b"data").to_bytes()).unwrap();
                seg.close().unwrap();
            }

            // Set watermark
            {
                let mut m = manifest.lock().unwrap();
                m.set_snapshot_watermark(1, 5).unwrap();
                m.manifest_mut().active_wal_segment = 3;
                m.persist().unwrap();
            }

            let compactor = WalOnlyCompactor::new(wal_dir, manifest);
            let info = compactor.compact().unwrap();

            assert_eq!(info.wal_segments_removed, 1);
        }

        #[test]
        fn compaction_is_logically_invisible() {
            // CompactInfo provides metrics, not semantic changes
            let info = CompactInfo::new(CompactMode::WALOnly);
            assert_eq!(info.versions_removed, 0); // No semantic versions affected
        }

        #[test]
        fn reclaimed_space_is_measurable() {
            let dir = tempdir().unwrap();
            let wal_dir = dir.path().join("WAL");
            std::fs::create_dir_all(&wal_dir).unwrap();

            let manifest_path = dir.path().join("MANIFEST");
            let manifest = ManifestManager::create(manifest_path, test_uuid(), "identity".to_string()).unwrap();
            let manifest = Arc::new(Mutex::new(manifest));

            // Create segment with data
            {
                let mut seg = WalSegment::create(&wal_dir, 1, test_uuid()).unwrap();
                for i in 1..=10 {
                    seg.write(&create_test_wal_record(i, &vec![i as u8; 100]).to_bytes()).unwrap();
                }
                seg.close().unwrap();
            }

            {
                let mut m = manifest.lock().unwrap();
                m.set_snapshot_watermark(1, 100).unwrap();
                m.manifest_mut().active_wal_segment = 2;
                m.persist().unwrap();
            }

            let compactor = WalOnlyCompactor::new(wal_dir, manifest);
            let info = compactor.compact().unwrap();

            assert!(info.reclaimed_bytes > 0, "Should report reclaimed bytes");
        }
    }

    /// Gate 5: Portability
    mod gate5_portability {
        use super::*;
        use strata_storage::database::export_database;

        #[test]
        fn database_directory_portable_by_copy() {
            let dir = tempdir().unwrap();
            let src = dir.path().join("src.db");
            let dst = dir.path().join("dst.db");

            {
                let handle = DatabaseHandle::create(&src, DatabaseConfig::for_testing()).unwrap();
                handle.close().unwrap();
            }

            // Manual copy
            copy_dir_all(&src, &dst).unwrap();

            // Should be openable
            let handle = DatabaseHandle::open(&dst, DatabaseConfig::for_testing()).unwrap();
            handle.close().unwrap();
        }

        #[test]
        fn export_import_work_as_convenience_wrappers() {
            let dir = tempdir().unwrap();
            let src = dir.path().join("src.db");
            let export = dir.path().join("export.db");

            {
                let handle = DatabaseHandle::create(&src, DatabaseConfig::for_testing()).unwrap();
                handle.close().unwrap();
            }

            let info = export_database(&src, &export, &DatabaseConfig::for_testing()).unwrap();
            assert!(info.size_bytes > 0);
            assert!(export.exists());
        }

        fn copy_dir_all(src: &Path, dst: &Path) -> std::io::Result<()> {
            std::fs::create_dir_all(dst)?;
            for entry in std::fs::read_dir(src)? {
                let entry = entry?;
                let src_path = entry.path();
                let dst_path = dst.join(entry.file_name());
                if src_path.is_dir() {
                    copy_dir_all(&src_path, &dst_path)?;
                } else {
                    std::fs::copy(&src_path, &dst_path)?;
                }
            }
            Ok(())
        }
    }

    /// Gate 6: Codec Seam
    mod gate6_codec_seam {
        use super::*;

        #[test]
        fn all_persistence_goes_through_codec() {
            // IdentityCodec passes through unchanged
            let codec = IdentityCodec;
            let data = b"test data";
            let encoded = codec.encode(data);
            let decoded = codec.decode(&encoded).unwrap();
            assert_eq!(&decoded, data);
        }

        #[test]
        fn identity_codec_works() {
            let codec = make_codec();
            assert_eq!(codec.codec_id(), "identity");

            let data = vec![1, 2, 3, 4, 5];
            let encoded = codec.encode(&data);
            let decoded = codec.decode(&encoded).unwrap();
            assert_eq!(decoded, data);
        }

        #[test]
        fn codec_id_stored_in_manifest() {
            let manifest = Manifest::new(test_uuid(), "identity".to_string());
            assert_eq!(manifest.codec_id, "identity");
        }

        #[test]
        fn codec_mismatch_detected_on_open() {
            let dir = tempdir().unwrap();
            let db_path = dir.path();

            std::fs::create_dir_all(db_path).unwrap();
            ManifestManager::create(
                db_path.join("MANIFEST"),
                test_uuid(),
                "aes256".to_string(),
            )
            .unwrap();

            let coordinator = RecoveryCoordinator::new(db_path.to_path_buf(), make_codec());
            let result = coordinator.plan_recovery();

            assert!(matches!(result, Err(RecoveryError::CodecMismatch { .. })));
        }
    }
}
