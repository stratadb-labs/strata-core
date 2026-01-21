//! Corruption detection simulation tests
//!
//! These tests verify that WAL corruption is detected correctly:
//! - CRC32 detects bit flips
//! - Truncated entries are handled gracefully
//! - Incomplete transactions are detected
//! - Recovery stops at first bad entry

use strata_core::types::{Key, Namespace, RunId};
use strata_core::value::Value;
use strata_core::Timestamp;
use strata_durability::wal::{DurabilityMode, WALEntry, WAL};
use std::fs::OpenOptions;
use std::io::{Read, Seek, SeekFrom, Write};
use tempfile::TempDir;
use uuid::Uuid;

/// Helper to get current timestamp
fn now() -> Timestamp {
    Timestamp::now()
}

#[test]
fn test_crc_detects_bit_flip() {
    let temp_dir = TempDir::new().unwrap();
    let wal_path = temp_dir.path().join("corrupt.wal");

    let run_id = RunId::new();
    let entry = WALEntry::BeginTxn {
        txn_id: 1,
        run_id,
        timestamp: now(),
    };

    // Write entry
    {
        let mut wal = WAL::open(&wal_path, DurabilityMode::Strict).unwrap();
        wal.append(&entry).unwrap();
    }

    // Corrupt file: flip a bit in payload
    {
        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .open(&wal_path)
            .unwrap();

        // Seek to offset 10 (somewhere in entry payload, after header)
        file.seek(SeekFrom::Start(10)).unwrap();

        // Read byte
        let mut buf = [0u8; 1];
        file.read_exact(&mut buf).unwrap();

        // Flip bits
        buf[0] ^= 0xFF;

        // Write back
        file.seek(SeekFrom::Start(10)).unwrap();
        file.write_all(&buf).unwrap();
        file.sync_all().unwrap();
    }

    // Read should detect corruption
    {
        let wal = WAL::open(&wal_path, DurabilityMode::Strict).unwrap();
        let entries = wal.read_all().unwrap();

        // Should return empty (corruption detected, no valid entries)
        // The decoder stops at first corruption
        assert_eq!(
            entries.len(),
            0,
            "Expected no entries due to corruption, got {}",
            entries.len()
        );
    }
}

#[test]
fn test_truncated_entry_handling() {
    let temp_dir = TempDir::new().unwrap();
    let wal_path = temp_dir.path().join("truncated.wal");

    let run_id = RunId::new();

    // Write 3 entries
    {
        let mut wal = WAL::open(&wal_path, DurabilityMode::Strict).unwrap();
        for i in 0..3 {
            let entry = WALEntry::BeginTxn {
                txn_id: i,
                run_id,
                timestamp: now(),
            };
            wal.append(&entry).unwrap();
        }
    }

    let file_size = std::fs::metadata(&wal_path).unwrap().len();

    // Truncate file (remove last 20 bytes - partial entry)
    {
        let file = OpenOptions::new().write(true).open(&wal_path).unwrap();
        file.set_len(file_size - 20).unwrap();
    }

    // Read should gracefully handle truncation
    {
        let wal = WAL::open(&wal_path, DurabilityMode::Strict).unwrap();
        let entries = wal.read_all().unwrap();

        // Should read first 2 entries, stop at truncated 3rd
        assert!(
            entries.len() >= 1 && entries.len() < 3,
            "Expected 1-2 entries, got {}",
            entries.len()
        );

        // Verify we got valid entries
        for entry in &entries {
            assert!(matches!(entry, WALEntry::BeginTxn { .. }));
        }
    }
}

#[test]
fn test_incomplete_transaction_detection() {
    let temp_dir = TempDir::new().unwrap();
    let wal_path = temp_dir.path().join("incomplete.wal");

    let run_id = RunId::new();
    let ns = Namespace::new(
        "tenant".to_string(),
        "app".to_string(),
        "agent".to_string(),
        run_id,
    );

    // Write incomplete transaction: BeginTxn + Writes, but NO CommitTxn
    {
        let mut wal = WAL::open(&wal_path, DurabilityMode::Strict).unwrap();

        wal.append(&WALEntry::BeginTxn {
            txn_id: 1,
            run_id,
            timestamp: now(),
        })
        .unwrap();

        wal.append(&WALEntry::Write {
            run_id,
            key: Key::new_kv(ns.clone(), "key1"),
            value: Value::Bytes(b"value1".to_vec()),
            version: 1,
        })
        .unwrap();

        wal.append(&WALEntry::Write {
            run_id,
            key: Key::new_kv(ns.clone(), "key2"),
            value: Value::Bytes(b"value2".to_vec()),
            version: 2,
        })
        .unwrap();

        // NO CommitTxn - simulates crash
    }

    // Read entries - all should be readable (no CRC corruption)
    {
        let wal = WAL::open(&wal_path, DurabilityMode::Strict).unwrap();
        let entries = wal.read_all().unwrap();

        // All entries are readable (no corruption)
        assert_eq!(
            entries.len(),
            3,
            "Expected 3 entries, got {}",
            entries.len()
        );

        // Verify structure: BeginTxn, Write, Write (no CommitTxn)
        assert!(
            matches!(entries[0], WALEntry::BeginTxn { .. }),
            "Expected BeginTxn"
        );
        assert!(
            matches!(entries[1], WALEntry::Write { .. }),
            "Expected Write"
        );
        assert!(
            matches!(entries[2], WALEntry::Write { .. }),
            "Expected Write"
        );

        // Check that there's no CommitTxn (simulating crash)
        let has_commit = entries
            .iter()
            .any(|e| matches!(e, WALEntry::CommitTxn { .. }));
        assert!(!has_commit, "Should not have CommitTxn (crash simulation)");

        // Note: Recovery logic (in next epic) will discard uncommitted transactions
    }
}

#[test]
fn test_multiple_corruption_points() {
    let temp_dir = TempDir::new().unwrap();
    let wal_path = temp_dir.path().join("multi_corrupt.wal");

    let run_id = RunId::new();

    // Write 10 entries
    {
        let mut wal = WAL::open(&wal_path, DurabilityMode::Strict).unwrap();
        for i in 0..10 {
            let entry = WALEntry::BeginTxn {
                txn_id: i,
                run_id,
                timestamp: now(),
            };
            wal.append(&entry).unwrap();
        }
    }

    let file_size = std::fs::metadata(&wal_path).unwrap().len();

    // Corrupt at offset 50 (in the middle of entries)
    {
        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .open(&wal_path)
            .unwrap();

        // Only corrupt if file is large enough
        if file_size > 60 {
            file.seek(SeekFrom::Start(50)).unwrap();
            let mut buf = [0u8; 4];
            if file.read_exact(&mut buf).is_ok() {
                buf[0] ^= 0xFF;
                buf[1] ^= 0xAA;
                file.seek(SeekFrom::Start(50)).unwrap();
                file.write_all(&buf).unwrap();
                file.sync_all().unwrap();
            }
        }
    }

    // Read should stop at first corruption
    {
        let wal = WAL::open(&wal_path, DurabilityMode::Strict).unwrap();
        let entries = wal.read_all().unwrap();

        // Should have some entries (before corruption) but not all 10
        assert!(
            entries.len() < 10,
            "Expected fewer than 10 entries due to corruption, got {}",
            entries.len()
        );
    }
}

#[test]
fn test_valid_wal_after_drop_fsync() {
    let temp_dir = TempDir::new().unwrap();
    let wal_path = temp_dir.path().join("drop_sync.wal");

    let run_id = RunId::new();
    let ns = Namespace::new(
        "tenant".to_string(),
        "app".to_string(),
        "agent".to_string(),
        run_id,
    );

    // Write transaction with Strict mode (fsync on every write)
    {
        let mut wal = WAL::open(&wal_path, DurabilityMode::Strict).unwrap();

        wal.append(&WALEntry::BeginTxn {
            txn_id: 1,
            run_id,
            timestamp: now(),
        })
        .unwrap();

        wal.append(&WALEntry::Write {
            run_id,
            key: Key::new_kv(ns.clone(), "key"),
            value: Value::Bytes(b"value".to_vec()),
            version: 1,
        })
        .unwrap();

        wal.append(&WALEntry::CommitTxn { txn_id: 1, run_id })
            .unwrap();

        // Drop performs final fsync
    }

    // Recovery: All entries should be readable
    {
        let wal = WAL::open(&wal_path, DurabilityMode::Strict).unwrap();
        let entries = wal.read_all().unwrap();

        assert_eq!(
            entries.len(),
            3,
            "Expected 3 entries, got {}",
            entries.len()
        );
        assert!(matches!(entries[0], WALEntry::BeginTxn { .. }));
        assert!(matches!(entries[1], WALEntry::Write { .. }));
        assert!(matches!(entries[2], WALEntry::CommitTxn { .. }));
    }
}

#[test]
fn test_crc_on_all_entry_types() {
    let temp_dir = TempDir::new().unwrap();
    let wal_path = temp_dir.path().join("all_types.wal");
    let backup_path = temp_dir.path().join("backup.wal");

    let run_id = RunId::new();
    let ns = Namespace::new(
        "tenant".to_string(),
        "app".to_string(),
        "agent".to_string(),
        run_id,
    );

    // Write all entry types
    {
        let mut wal = WAL::open(&wal_path, DurabilityMode::Strict).unwrap();

        wal.append(&WALEntry::BeginTxn {
            txn_id: 1,
            run_id,
            timestamp: now(),
        })
        .unwrap();

        wal.append(&WALEntry::Write {
            run_id,
            key: Key::new_kv(ns.clone(), "key"),
            value: Value::Bytes(b"value".to_vec()),
            version: 1,
        })
        .unwrap();

        wal.append(&WALEntry::Delete {
            run_id,
            key: Key::new_kv(ns.clone(), "key"),
            version: 2,
        })
        .unwrap();

        wal.append(&WALEntry::CommitTxn { txn_id: 1, run_id })
            .unwrap();

        wal.append(&WALEntry::AbortTxn { txn_id: 2, run_id })
            .unwrap();

        wal.append(&WALEntry::Checkpoint {
            snapshot_id: Uuid::new_v4(),
            version: 10,
            active_runs: vec![run_id],
        })
        .unwrap();
    }

    // Backup the file for restoration
    std::fs::copy(&wal_path, &backup_path).unwrap();

    // All entries should decode successfully
    {
        let wal = WAL::open(&wal_path, DurabilityMode::Strict).unwrap();
        let entries = wal.read_all().unwrap();
        assert_eq!(
            entries.len(),
            6,
            "Expected 6 entries, got {}",
            entries.len()
        );
    }

    // Test corruption at various offsets
    let file_size = std::fs::metadata(&wal_path).unwrap().len();
    let test_offsets: Vec<u64> = vec![10, 50, 100, 150, 200]
        .into_iter()
        .filter(|&o| o < file_size - 5)
        .collect();

    for corrupt_offset in test_offsets {
        // Restore from backup
        std::fs::copy(&backup_path, &wal_path).unwrap();

        // Corrupt at this offset
        {
            let mut file = OpenOptions::new()
                .read(true)
                .write(true)
                .open(&wal_path)
                .unwrap();

            file.seek(SeekFrom::Start(corrupt_offset)).unwrap();
            let mut buf = [0u8; 1];
            if file.read_exact(&mut buf).is_ok() {
                buf[0] ^= 0xFF;
                file.seek(SeekFrom::Start(corrupt_offset)).unwrap();
                file.write_all(&buf).unwrap();
                file.sync_all().unwrap();
            }
        }

        // Should detect corruption (fewer entries than original)
        let wal = WAL::open(&wal_path, DurabilityMode::Strict).unwrap();
        let entries = wal.read_all().unwrap();

        // Corruption means we get fewer valid entries
        assert!(
            entries.len() < 6,
            "Expected fewer than 6 entries at corrupt offset {}, got {}",
            corrupt_offset,
            entries.len()
        );
    }
}

#[test]
fn test_zero_length_wal() {
    let temp_dir = TempDir::new().unwrap();
    let wal_path = temp_dir.path().join("empty.wal");

    // Create empty WAL
    {
        let _wal = WAL::open(&wal_path, DurabilityMode::Strict).unwrap();
    }

    // Read should return empty list, not error
    {
        let wal = WAL::open(&wal_path, DurabilityMode::Strict).unwrap();
        let entries = wal.read_all().unwrap();
        assert_eq!(entries.len(), 0);
    }
}

#[test]
fn test_corruption_preserves_earlier_entries() {
    let temp_dir = TempDir::new().unwrap();
    let wal_path = temp_dir.path().join("partial.wal");

    let run_id = RunId::new();

    // Write 5 entries
    let mut offsets = Vec::new();
    {
        let mut wal = WAL::open(&wal_path, DurabilityMode::Strict).unwrap();
        for i in 0..5 {
            let entry = WALEntry::BeginTxn {
                txn_id: i,
                run_id,
                timestamp: now(),
            };
            let offset = wal.append(&entry).unwrap();
            offsets.push(offset);
        }
    }

    // Corrupt the 4th entry (offset 3)
    let corrupt_offset = offsets[3] + 5; // A few bytes into the 4th entry
    {
        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .open(&wal_path)
            .unwrap();

        file.seek(SeekFrom::Start(corrupt_offset)).unwrap();
        let mut buf = [0u8; 4];
        if file.read_exact(&mut buf).is_ok() {
            buf[0] ^= 0xFF;
            buf[1] ^= 0xFF;
            file.seek(SeekFrom::Start(corrupt_offset)).unwrap();
            file.write_all(&buf).unwrap();
            file.sync_all().unwrap();
        }
    }

    // Read should return first 3 entries (before corruption)
    {
        let wal = WAL::open(&wal_path, DurabilityMode::Strict).unwrap();
        let entries = wal.read_all().unwrap();

        // Should have 3 valid entries (entries 0, 1, 2)
        assert_eq!(
            entries.len(),
            3,
            "Expected 3 entries before corruption, got {}",
            entries.len()
        );

        // Verify they're the correct entries
        for (i, entry) in entries.iter().enumerate() {
            if let WALEntry::BeginTxn { txn_id, .. } = entry {
                assert_eq!(*txn_id, i as u64, "Entry {} has wrong txn_id", i);
            }
        }
    }
}
