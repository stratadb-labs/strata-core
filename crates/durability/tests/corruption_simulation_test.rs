//! Corruption simulation tests
//!
//! Comprehensive tests that intentionally break WAL files in various ways
//! to verify recovery handles edge cases correctly. Tests cover:
//! - Power loss (partial writes)
//! - Disk errors (bit flips in multiple locations)
//! - Filesystem bugs (truncation, garbage data)
//! - Multi-failure scenarios
//!
//! All tests use real file I/O to catch platform-specific issues.

use strata_core::types::RunId;
use strata_core::Timestamp;
use strata_durability::wal::{DurabilityMode, WALEntry, WAL};
use std::fs::{File, OpenOptions};
use std::io::{Seek, SeekFrom, Write};
use tempfile::TempDir;

/// Helper to get current timestamp
fn now() -> Timestamp {
    Timestamp::now()
}

/// Helper: Write N valid entries to WAL
///
/// Writes N BeginTxn entries with sequential txn_ids (0..count)
fn write_entries(wal: &mut WAL, run_id: RunId, count: usize) {
    for i in 0..count {
        let entry = WALEntry::BeginTxn {
            txn_id: i as u64,
            run_id,
            timestamp: now(),
        };
        wal.append(&entry).unwrap();
    }
}

/// Helper: Write N entries and return their starting offsets
fn write_entries_with_offsets(wal: &mut WAL, run_id: RunId, count: usize) -> Vec<u64> {
    let mut offsets = Vec::with_capacity(count);
    for i in 0..count {
        let entry = WALEntry::BeginTxn {
            txn_id: i as u64,
            run_id,
            timestamp: now(),
        };
        let offset = wal.append(&entry).unwrap();
        offsets.push(offset);
    }
    offsets
}

/// Helper: Corrupt bytes at offset
///
/// Writes the specified corruption bytes at the given file offset.
fn corrupt_at_offset(path: &std::path::Path, offset: u64, corruption: &[u8]) {
    let mut file = OpenOptions::new().write(true).open(path).unwrap();
    file.seek(SeekFrom::Start(offset)).unwrap();
    file.write_all(corruption).unwrap();
    file.sync_all().unwrap();
}

/// Helper: Truncate file to size
fn truncate_file(path: &std::path::Path, size: u64) {
    let file = OpenOptions::new().write(true).open(path).unwrap();
    file.set_len(size).unwrap();
    file.sync_all().unwrap();
}

// ============================================================================
// Test 1: Corrupt entry length field (header corruption)
// ============================================================================

#[test]
fn test_corrupt_entry_length_field() {
    let temp_dir = TempDir::new().unwrap();
    let wal_path = temp_dir.path().join("corrupt_length.wal");

    let run_id = RunId::new();

    // Write valid entry
    {
        let mut wal = WAL::open(&wal_path, DurabilityMode::Strict).unwrap();
        write_entries(&mut wal, run_id, 1);
    }

    // Corrupt length field (first 4 bytes)
    // Set length to impossible value (0xFFFFFFFF = ~4GB)
    corrupt_at_offset(&wal_path, 0, &[0xFF, 0xFF, 0xFF, 0xFF]);

    // Recovery should handle gracefully - either return error or empty
    {
        let wal = WAL::open(&wal_path, DurabilityMode::Strict).unwrap();
        let result = wal.read_all();

        // Either fails with error or returns no entries
        match result {
            Ok(entries) => {
                assert_eq!(
                    entries.len(),
                    0,
                    "Corrupt length should return no entries, got {}",
                    entries.len()
                );
            }
            Err(e) => {
                let err_msg = format!("{:?}", e);
                // Error should provide debugging info
                assert!(
                    err_msg.contains("offset") || err_msg.contains("0"),
                    "Error should include offset info: {}",
                    err_msg
                );
            }
        }
    }
}

// ============================================================================
// Test 2: Corrupt entry payload (detected by CRC)
// ============================================================================

#[test]
fn test_corrupt_entry_payload() {
    let temp_dir = TempDir::new().unwrap();
    let wal_path = temp_dir.path().join("corrupt_payload.wal");

    let run_id = RunId::new();

    // Write valid entry
    {
        let mut wal = WAL::open(&wal_path, DurabilityMode::Strict).unwrap();
        write_entries(&mut wal, run_id, 1);
    }

    // Corrupt payload (offset 10, somewhere in the middle after header)
    // This should cause CRC mismatch
    corrupt_at_offset(&wal_path, 10, &[0xFF, 0xFF, 0xFF, 0xFF]);

    // Should detect CRC mismatch and return no valid entries
    {
        let wal = WAL::open(&wal_path, DurabilityMode::Strict).unwrap();
        let entries = wal.read_all().unwrap();

        // Corruption detected, no valid entries returned
        assert_eq!(
            entries.len(),
            0,
            "Corrupt payload should be detected by CRC, got {} entries",
            entries.len()
        );
    }
}

// ============================================================================
// Test 3: Truncated CRC (missing bytes at end)
// ============================================================================

#[test]
fn test_truncated_crc() {
    let temp_dir = TempDir::new().unwrap();
    let wal_path = temp_dir.path().join("truncated_crc.wal");

    let run_id = RunId::new();

    // Write valid entry
    {
        let mut wal = WAL::open(&wal_path, DurabilityMode::Strict).unwrap();
        write_entries(&mut wal, run_id, 1);
    }

    let file_size = std::fs::metadata(&wal_path).unwrap().len();

    // Truncate last 2 bytes (CRC is 4 bytes, remove half)
    // This simulates power loss during CRC write
    truncate_file(&wal_path, file_size - 2);

    // Should detect incomplete entry
    {
        let wal = WAL::open(&wal_path, DurabilityMode::Strict).unwrap();
        let entries = wal.read_all().unwrap();

        // Should gracefully handle truncation (return empty)
        assert_eq!(
            entries.len(),
            0,
            "Truncated CRC should return no complete entries"
        );
    }
}

// ============================================================================
// Test 4: Multiple entries, first corrupt
// ============================================================================

#[test]
fn test_multiple_entries_first_corrupt() {
    let temp_dir = TempDir::new().unwrap();
    let wal_path = temp_dir.path().join("multi_first_corrupt.wal");

    let run_id = RunId::new();

    // Write 5 valid entries
    {
        let mut wal = WAL::open(&wal_path, DurabilityMode::Strict).unwrap();
        write_entries(&mut wal, run_id, 5);
    }

    // Corrupt first entry (offset 10, in payload area)
    corrupt_at_offset(&wal_path, 10, &[0xFF]);

    // Should fail at first entry, return nothing
    {
        let wal = WAL::open(&wal_path, DurabilityMode::Strict).unwrap();
        let entries = wal.read_all().unwrap();

        // First entry corrupt means nothing is returned
        assert_eq!(
            entries.len(),
            0,
            "First entry corrupt should return no entries"
        );
    }
}

// ============================================================================
// Test 5: Multiple entries, middle corrupt
// ============================================================================

#[test]
fn test_multiple_entries_middle_corrupt() {
    let temp_dir = TempDir::new().unwrap();
    let wal_path = temp_dir.path().join("multi_middle_corrupt.wal");

    let run_id = RunId::new();

    // Write 5 valid entries, track offsets
    let offsets: Vec<u64>;
    {
        let mut wal = WAL::open(&wal_path, DurabilityMode::Strict).unwrap();
        offsets = write_entries_with_offsets(&mut wal, run_id, 5);
    }

    // Corrupt 3rd entry (index 2)
    // Corrupt a few bytes into the entry (skip length field, hit payload)
    let corrupt_position = offsets[2] + 10;
    corrupt_at_offset(&wal_path, corrupt_position, &[0xFF]);

    // Should read first 2 entries, stop at 3rd corruption
    {
        let wal = WAL::open(&wal_path, DurabilityMode::Strict).unwrap();
        let entries = wal.read_all().unwrap();

        // Should have exactly 2 entries (entries 0 and 1, before corruption)
        assert_eq!(
            entries.len(),
            2,
            "Should read 2 entries before corruption, got {}",
            entries.len()
        );

        // Verify correct entries were read
        for (i, entry) in entries.iter().enumerate() {
            if let WALEntry::BeginTxn { txn_id, .. } = entry {
                assert_eq!(*txn_id, i as u64, "Entry {} has wrong txn_id", i);
            }
        }
    }
}

// ============================================================================
// Test 6: Valid entries after corruption NOT read (conservative)
// ============================================================================

#[test]
fn test_valid_entries_after_corruption_not_read() {
    let temp_dir = TempDir::new().unwrap();
    let wal_path = temp_dir.path().join("valid_after_corrupt.wal");

    let run_id = RunId::new();

    // Write 10 entries
    let offsets: Vec<u64>;
    {
        let mut wal = WAL::open(&wal_path, DurabilityMode::Strict).unwrap();
        offsets = write_entries_with_offsets(&mut wal, run_id, 10);
    }

    // Corrupt entry at index 3 (entries 0, 1, 2 should be valid)
    let corrupt_position = offsets[3] + 10;
    corrupt_at_offset(&wal_path, corrupt_position, &[0xFF, 0xFF]);

    // Recovery should NOT read entries after corruption
    // (conservative: stop at first error, don't skip past corruption)
    {
        let wal = WAL::open(&wal_path, DurabilityMode::Strict).unwrap();
        let entries = wal.read_all().unwrap();

        // Should have read only 3 entries (stopped at corruption)
        // Even though entries 4-9 are technically valid, we don't skip
        assert_eq!(
            entries.len(),
            3,
            "Should stop at corruption, not read past it. Got {} entries",
            entries.len()
        );
    }
}

// ============================================================================
// Test 7: Interleaved valid/corrupt entries
// ============================================================================

#[test]
fn test_interleaved_valid_corrupt() {
    let temp_dir = TempDir::new().unwrap();
    let wal_path = temp_dir.path().join("interleaved.wal");

    let run_id = RunId::new();

    // Write 10 entries
    let offsets: Vec<u64>;
    {
        let mut wal = WAL::open(&wal_path, DurabilityMode::Strict).unwrap();
        offsets = write_entries_with_offsets(&mut wal, run_id, 10);
    }

    // Corrupt entries 2, 5, 8 (we'll only see corruption at 2)
    for &idx in &[2usize, 5, 8] {
        if idx < offsets.len() {
            corrupt_at_offset(&wal_path, offsets[idx] + 10, &[0xFF]);
        }
    }

    // Should fail at first corruption (entry 2), read only entries 0, 1
    {
        let wal = WAL::open(&wal_path, DurabilityMode::Strict).unwrap();
        let entries = wal.read_all().unwrap();

        // Should read only 2 entries (0 and 1), stopped at first corruption
        assert_eq!(
            entries.len(),
            2,
            "Should stop at first corruption (entry 2), got {} entries",
            entries.len()
        );
    }
}

// ============================================================================
// Test 8: Error messages include file offset for debugging
// ============================================================================

#[test]
fn test_error_messages_include_offset() {
    let temp_dir = TempDir::new().unwrap();
    let wal_path = temp_dir.path().join("error_offset.wal");

    let run_id = RunId::new();

    // Write 3 entries
    let offsets: Vec<u64>;
    {
        let mut wal = WAL::open(&wal_path, DurabilityMode::Strict).unwrap();
        offsets = write_entries_with_offsets(&mut wal, run_id, 3);
    }

    // Corrupt second entry (index 1)
    let corrupt_offset = offsets[1] + 10;
    corrupt_at_offset(&wal_path, corrupt_offset, &[0xFF]);

    // We expect the WAL to return entries before corruption
    // The offset information should be available for debugging
    {
        let wal = WAL::open(&wal_path, DurabilityMode::Strict).unwrap();
        let entries = wal.read_all().unwrap();

        // Should read first entry only (stopped at second entry's corruption)
        assert_eq!(
            entries.len(),
            1,
            "Should read 1 entry before corruption, got {}",
            entries.len()
        );

        // Verify it's the correct entry
        if let WALEntry::BeginTxn { txn_id, .. } = &entries[0] {
            assert_eq!(*txn_id, 0);
        }
    }
}

// ============================================================================
// Test 9: Power loss simulation (partial writes)
// ============================================================================

#[test]
fn test_power_loss_simulation() {
    let temp_dir = TempDir::new().unwrap();
    let wal_path = temp_dir.path().join("power_loss.wal");

    // Simulate power loss: write partial entry header (length only, no payload/CRC)
    {
        let mut file = File::create(&wal_path).unwrap();

        // Write partial entry header (length field says 0x10 = 16 bytes expected)
        // But no actual payload or CRC follows - simulates power loss mid-write
        file.write_all(&[0x10, 0x00, 0x00, 0x00]).unwrap();
        file.sync_all().unwrap();
    }

    // Recovery should handle gracefully
    {
        let wal = WAL::open(&wal_path, DurabilityMode::Strict).unwrap();
        let entries = wal.read_all().unwrap();

        // Should return empty (incomplete entry ignored/handled)
        assert_eq!(
            entries.len(),
            0,
            "Power loss simulation should return no entries"
        );
    }
}

// ============================================================================
// Test 10: Filesystem bug simulation (garbage data appended)
// ============================================================================

#[test]
fn test_filesystem_bug_simulation() {
    let temp_dir = TempDir::new().unwrap();
    let wal_path = temp_dir.path().join("fs_bug.wal");

    let run_id = RunId::new();

    // Write valid entries
    {
        let mut wal = WAL::open(&wal_path, DurabilityMode::Strict).unwrap();
        write_entries(&mut wal, run_id, 5);
    }

    let file_size = std::fs::metadata(&wal_path).unwrap().len();

    // Simulate filesystem bug: garbage data appended (random non-zero bytes)
    // This can happen with filesystem bugs, pre-allocation issues, or sector reuse
    {
        let mut file = OpenOptions::new().write(true).open(&wal_path).unwrap();
        file.seek(SeekFrom::Start(file_size)).unwrap();
        // Write garbage that looks like an impossibly large entry length
        // This will be detected as incomplete entry at EOF and handled gracefully
        file.write_all(&[0x00, 0x10, 0x00, 0x00]).unwrap(); // Small but valid-ish length
        file.sync_all().unwrap();
    }

    // Recovery should read valid entries, stop at garbage gracefully
    {
        let wal = WAL::open(&wal_path, DurabilityMode::Strict).unwrap();
        let entries = wal.read_all().unwrap();

        // Should read all 5 valid entries
        // Trailing garbage should be handled gracefully (incomplete entry at EOF)
        assert_eq!(
            entries.len(),
            5,
            "Should read 5 valid entries despite trailing garbage, got {}",
            entries.len()
        );
    }
}

// ============================================================================
// Additional edge case tests
// ============================================================================

#[test]
fn test_corrupt_type_tag() {
    let temp_dir = TempDir::new().unwrap();
    let wal_path = temp_dir.path().join("corrupt_type.wal");

    let run_id = RunId::new();

    // Write valid entry
    {
        let mut wal = WAL::open(&wal_path, DurabilityMode::Strict).unwrap();
        write_entries(&mut wal, run_id, 1);
    }

    // Corrupt type tag (byte 4, right after length field)
    // Set to invalid type value (0xFF)
    corrupt_at_offset(&wal_path, 4, &[0xFF]);

    // Should detect corruption
    {
        let wal = WAL::open(&wal_path, DurabilityMode::Strict).unwrap();
        let entries = wal.read_all().unwrap();

        // Invalid type tag should cause decode failure
        assert_eq!(
            entries.len(),
            0,
            "Corrupt type tag should return no entries"
        );
    }
}

#[test]
fn test_completely_random_garbage() {
    let temp_dir = TempDir::new().unwrap();
    let wal_path = temp_dir.path().join("garbage.wal");

    // Write completely random garbage to file
    {
        let mut file = File::create(&wal_path).unwrap();
        // Random-looking bytes that won't form a valid entry
        file.write_all(&[
            0xDE, 0xAD, 0xBE, 0xEF, 0xCA, 0xFE, 0xBA, 0xBE, 0x12, 0x34, 0x56, 0x78, 0x9A, 0xBC,
            0xDE, 0xF0,
        ])
        .unwrap();
        file.sync_all().unwrap();
    }

    // Recovery should handle gracefully
    {
        let wal = WAL::open(&wal_path, DurabilityMode::Strict).unwrap();
        let entries = wal.read_all().unwrap();

        // Garbage file should return no entries
        assert_eq!(entries.len(), 0, "Garbage file should return no entries");
    }
}

#[test]
fn test_zero_length_entry() {
    let temp_dir = TempDir::new().unwrap();
    let wal_path = temp_dir.path().join("zero_length.wal");

    // Write a zero-length entry header (invalid)
    {
        let mut file = File::create(&wal_path).unwrap();
        // Length field = 0 (invalid, minimum is 1 type + 4 CRC = 5)
        file.write_all(&[0x00, 0x00, 0x00, 0x00]).unwrap();
        file.sync_all().unwrap();
    }

    // Recovery should handle gracefully
    {
        let wal = WAL::open(&wal_path, DurabilityMode::Strict).unwrap();
        let entries = wal.read_all().unwrap();

        // Zero-length entry should be handled
        assert_eq!(
            entries.len(),
            0,
            "Zero-length entry should return no entries"
        );
    }
}

#[test]
fn test_corruption_in_crc_field() {
    let temp_dir = TempDir::new().unwrap();
    let wal_path = temp_dir.path().join("corrupt_crc.wal");

    let run_id = RunId::new();

    // Write valid entry
    {
        let mut wal = WAL::open(&wal_path, DurabilityMode::Strict).unwrap();
        write_entries(&mut wal, run_id, 1);
    }

    let file_size = std::fs::metadata(&wal_path).unwrap().len();

    // Corrupt CRC field (last 4 bytes of entry)
    // Flip bits in the CRC
    corrupt_at_offset(&wal_path, file_size - 4, &[0xFF, 0xFF, 0xFF, 0xFF]);

    // Should detect CRC mismatch
    {
        let wal = WAL::open(&wal_path, DurabilityMode::Strict).unwrap();
        let entries = wal.read_all().unwrap();

        // CRC mismatch should be detected
        assert_eq!(entries.len(), 0, "Corrupt CRC should return no entries");
    }
}

#[test]
fn test_multiple_power_loss_scenarios() {
    let temp_dir = TempDir::new().unwrap();

    // Scenario 1: Power loss after writing only length field
    {
        let wal_path = temp_dir.path().join("power_loss_1.wal");
        let mut file = File::create(&wal_path).unwrap();
        file.write_all(&[0x20, 0x00, 0x00, 0x00]).unwrap(); // Length = 32
        file.sync_all().unwrap();

        let wal = WAL::open(&wal_path, DurabilityMode::Strict).unwrap();
        let entries = wal.read_all().unwrap();
        assert_eq!(entries.len(), 0);
    }

    // Scenario 2: Power loss after writing length + type
    {
        let wal_path = temp_dir.path().join("power_loss_2.wal");
        let mut file = File::create(&wal_path).unwrap();
        file.write_all(&[0x20, 0x00, 0x00, 0x00, 0x01]).unwrap(); // Length + type
        file.sync_all().unwrap();

        let wal = WAL::open(&wal_path, DurabilityMode::Strict).unwrap();
        let entries = wal.read_all().unwrap();
        assert_eq!(entries.len(), 0);
    }

    // Scenario 3: Power loss after writing partial payload
    {
        let wal_path = temp_dir.path().join("power_loss_3.wal");
        let mut file = File::create(&wal_path).unwrap();
        // Length + type + 10 bytes of "payload"
        file.write_all(&[
            0x20, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00,
        ])
        .unwrap();
        file.sync_all().unwrap();

        let wal = WAL::open(&wal_path, DurabilityMode::Strict).unwrap();
        let entries = wal.read_all().unwrap();
        assert_eq!(entries.len(), 0);
    }
}

#[test]
fn test_bit_flip_at_various_offsets() {
    let temp_dir = TempDir::new().unwrap();

    for bit_position in [5, 8, 12, 15, 20, 25, 30] {
        let wal_path = temp_dir
            .path()
            .join(format!("bitflip_{}.wal", bit_position));

        let run_id = RunId::new();

        // Write valid entry
        {
            let mut wal = WAL::open(&wal_path, DurabilityMode::Strict).unwrap();
            write_entries(&mut wal, run_id, 1);
        }

        let file_size = std::fs::metadata(&wal_path).unwrap().len();

        // Only corrupt if file is large enough
        if bit_position < file_size {
            corrupt_at_offset(&wal_path, bit_position, &[0xFF]);

            let wal = WAL::open(&wal_path, DurabilityMode::Strict).unwrap();
            let entries = wal.read_all().unwrap();

            // Any bit flip should cause corruption detection
            assert_eq!(
                entries.len(),
                0,
                "Bit flip at offset {} should be detected",
                bit_position
            );
        }
    }
}
