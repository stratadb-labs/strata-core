//! WAL corruption testing utilities
//!
//! Provides utilities for simulating various types of WAL corruption
//! to test recovery robustness.
//!
//! # Corruption Types
//!
//! - Truncation: Removes bytes from WAL tail (simulates crash during write)
//! - Garbage: Appends invalid bytes (simulates partial write)
//! - Bit rot: Flips random bits (simulates storage degradation)
//! - Partial record: Creates incomplete record at end
//!
//! # Example
//!
//! ```ignore
//! use strata_storage::testing::WalCorruptionTester;
//!
//! let tester = WalCorruptionTester::new("path/to/db");
//! tester.truncate_wal_tail(50)?;
//! // Recovery should handle truncated WAL
//! ```

use std::path::{Path, PathBuf};

/// WAL corruption test utilities
pub struct WalCorruptionTester {
    /// Database directory
    db_dir: PathBuf,
}

impl WalCorruptionTester {
    /// Create a new corruption tester for a database
    pub fn new(db_dir: impl AsRef<Path>) -> Self {
        WalCorruptionTester {
            db_dir: db_dir.as_ref().to_path_buf(),
        }
    }

    /// Get the WAL directory
    pub fn wal_dir(&self) -> PathBuf {
        self.db_dir.join("WAL")
    }

    /// List WAL segment files in order
    pub fn list_segments(&self) -> std::io::Result<Vec<PathBuf>> {
        let wal_dir = self.wal_dir();
        if !wal_dir.exists() {
            return Ok(vec![]);
        }

        let mut segments: Vec<_> = std::fs::read_dir(&wal_dir)?
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().is_some_and(|ext| ext == "seg"))
            .map(|e| e.path())
            .collect();

        segments.sort();
        Ok(segments)
    }

    /// Get the latest WAL segment
    pub fn latest_segment(&self) -> std::io::Result<Option<PathBuf>> {
        Ok(self.list_segments()?.into_iter().last())
    }

    /// Truncate WAL tail by removing bytes
    ///
    /// Simulates crash during write where only partial data is written.
    pub fn truncate_wal_tail(&self, bytes_to_remove: usize) -> std::io::Result<TruncationResult> {
        let Some(segment_path) = self.latest_segment()? else {
            return Ok(TruncationResult {
                segment: None,
                original_size: 0,
                new_size: 0,
                bytes_removed: 0,
            });
        };

        let original_size = std::fs::metadata(&segment_path)?.len();

        if original_size <= bytes_to_remove as u64 {
            return Ok(TruncationResult {
                segment: Some(segment_path),
                original_size,
                new_size: original_size,
                bytes_removed: 0,
            });
        }

        let new_size = original_size - bytes_to_remove as u64;
        let file = std::fs::OpenOptions::new()
            .write(true)
            .open(&segment_path)?;
        file.set_len(new_size)?;

        Ok(TruncationResult {
            segment: Some(segment_path),
            original_size,
            new_size,
            bytes_removed: bytes_to_remove,
        })
    }

    /// Append garbage bytes to WAL tail
    ///
    /// Simulates partial/corrupt write at end of WAL.
    pub fn append_garbage(&self, garbage: &[u8]) -> std::io::Result<GarbageResult> {
        let Some(segment_path) = self.latest_segment()? else {
            return Ok(GarbageResult {
                segment: None,
                original_size: 0,
                new_size: 0,
                bytes_appended: 0,
            });
        };

        let original_size = std::fs::metadata(&segment_path)?.len();

        let mut file = std::fs::OpenOptions::new()
            .append(true)
            .open(&segment_path)?;

        std::io::Write::write_all(&mut file, garbage)?;

        let new_size = original_size + garbage.len() as u64;

        Ok(GarbageResult {
            segment: Some(segment_path),
            original_size,
            new_size,
            bytes_appended: garbage.len(),
        })
    }

    /// Append random garbage of given length
    pub fn append_random_garbage(&self, length: usize) -> std::io::Result<GarbageResult> {
        use std::time::{SystemTime, UNIX_EPOCH};

        // Simple pseudo-random generation
        let seed = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos() as u64;

        let garbage: Vec<u8> = (0..length)
            .map(|i| ((seed.wrapping_mul(i as u64 + 1)) & 0xFF) as u8)
            .collect();

        self.append_garbage(&garbage)
    }

    /// Create a partial WAL record at the tail
    ///
    /// Simulates crash in the middle of writing a record.
    pub fn create_partial_record(&self) -> std::io::Result<GarbageResult> {
        // Create bytes that look like the start of a record but are incomplete
        let partial = vec![
            0x10, 0x00, 0x00, 0x00, // Length prefix suggesting 16 bytes
            0x01, // Format version
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, // Partial txn_id
                  // Missing: rest of header, writeset, CRC
        ];

        self.append_garbage(&partial)
    }

    /// Corrupt random bytes in WAL segments
    ///
    /// Simulates bit rot or storage degradation.
    pub fn corrupt_random_bytes(&self, count: usize) -> std::io::Result<CorruptionResult> {
        use std::time::{SystemTime, UNIX_EPOCH};

        let segments = self.list_segments()?;
        if segments.is_empty() {
            return Ok(CorruptionResult {
                segments_affected: 0,
                bytes_corrupted: 0,
            });
        }

        let mut total_corrupted = 0;
        let seed = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos() as u64;

        for (seg_idx, segment_path) in segments.iter().enumerate() {
            let mut data = std::fs::read(segment_path)?;

            // Skip segment header (32 bytes) to avoid breaking format parsing
            let header_size = 32;
            if data.len() <= header_size {
                continue;
            }

            let data_len = data.len() - header_size;
            let corruptions_in_segment = count / segments.len() + 1;

            for i in 0..corruptions_in_segment {
                let pos_seed = seed.wrapping_mul((seg_idx * 1000 + i) as u64 + 1);
                let pos = header_size + (pos_seed as usize % data_len);
                let xor_value = (pos_seed >> 8) as u8;

                if pos < data.len() {
                    data[pos] ^= xor_value.max(1); // Ensure at least one bit flips
                    total_corrupted += 1;
                }
            }

            std::fs::write(segment_path, data)?;
        }

        Ok(CorruptionResult {
            segments_affected: segments.len(),
            bytes_corrupted: total_corrupted,
        })
    }

    /// Verify that recovery handles corruption gracefully
    ///
    /// Opens the database using recovery and checks if it recovers
    /// without panicking or returning unexpected errors.
    pub fn verify_recovery(&self) -> std::io::Result<RecoveryVerification> {
        use crate::codec::IdentityCodec;
        use crate::recovery::RecoveryCoordinator;

        let codec = Box::new(IdentityCodec);
        let coordinator = RecoveryCoordinator::new(self.db_dir.clone(), codec);

        if !coordinator.needs_recovery() {
            return Ok(RecoveryVerification {
                recovered: false,
                error: Some("No MANIFEST found".to_string()),
                wal_records_recovered: 0,
            });
        }

        let mut record_count = 0;

        let result = coordinator.recover(
            |_snapshot| Ok(()),
            |_record| {
                record_count += 1;
                Ok(())
            },
        );

        match result {
            Ok(_result) => Ok(RecoveryVerification {
                recovered: true,
                error: None,
                wal_records_recovered: record_count,
            }),
            Err(e) => Ok(RecoveryVerification {
                recovered: false,
                error: Some(e.to_string()),
                wal_records_recovered: record_count,
            }),
        }
    }
}

/// Result of WAL truncation
#[derive(Debug)]
pub struct TruncationResult {
    /// Segment that was truncated
    pub segment: Option<PathBuf>,
    /// Original file size
    pub original_size: u64,
    /// New file size after truncation
    pub new_size: u64,
    /// Bytes removed
    pub bytes_removed: usize,
}

/// Result of appending garbage
#[derive(Debug)]
pub struct GarbageResult {
    /// Segment that was modified
    pub segment: Option<PathBuf>,
    /// Original file size
    pub original_size: u64,
    /// New file size after append
    pub new_size: u64,
    /// Bytes appended
    pub bytes_appended: usize,
}

/// Result of byte corruption
#[derive(Debug)]
pub struct CorruptionResult {
    /// Number of segments affected
    pub segments_affected: usize,
    /// Total bytes corrupted
    pub bytes_corrupted: usize,
}

/// Result of recovery verification
#[derive(Debug)]
pub struct RecoveryVerification {
    /// Whether recovery succeeded
    pub recovered: bool,
    /// Error message if recovery failed
    pub error: Option<String>,
    /// Number of WAL records recovered
    pub wal_records_recovered: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::{DatabaseConfig, DatabaseHandle};
    use crate::format::WalRecord;
    use tempfile::tempdir;

    #[test]
    fn test_list_segments_empty() {
        let dir = tempdir().unwrap();
        let tester = WalCorruptionTester::new(dir.path());

        let segments = tester.list_segments().unwrap();
        assert!(segments.is_empty());
    }

    #[test]
    fn test_truncate_wal_tail() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");

        // Create database with some WAL data
        {
            let handle = DatabaseHandle::create(&db_path, DatabaseConfig::for_testing()).unwrap();
            for i in 0..10 {
                let record = WalRecord::new(i + 1, handle.uuid(), i * 1000, vec![i as u8; 100]);
                handle.append_wal(&record).unwrap();
            }
            handle.flush_wal().unwrap();
            handle.close().unwrap();
        }

        let tester = WalCorruptionTester::new(&db_path);
        let result = tester.truncate_wal_tail(50).unwrap();

        assert!(result.segment.is_some());
        assert_eq!(result.bytes_removed, 50);
        assert_eq!(result.new_size, result.original_size - 50);
    }

    #[test]
    fn test_append_garbage() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");

        // Create database
        {
            let handle = DatabaseHandle::create(&db_path, DatabaseConfig::for_testing()).unwrap();
            let record = WalRecord::new(1, handle.uuid(), 1000, vec![1, 2, 3]);
            handle.append_wal(&record).unwrap();
            handle.flush_wal().unwrap();
            handle.close().unwrap();
        }

        let tester = WalCorruptionTester::new(&db_path);
        let result = tester.append_garbage(b"GARBAGE").unwrap();

        assert!(result.segment.is_some());
        assert_eq!(result.bytes_appended, 7);
        assert_eq!(result.new_size, result.original_size + 7);
    }

    #[test]
    fn test_create_partial_record() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");

        // Create database
        {
            let handle = DatabaseHandle::create(&db_path, DatabaseConfig::for_testing()).unwrap();
            let record = WalRecord::new(1, handle.uuid(), 1000, vec![1, 2, 3]);
            handle.append_wal(&record).unwrap();
            handle.flush_wal().unwrap();
            handle.close().unwrap();
        }

        let tester = WalCorruptionTester::new(&db_path);
        let result = tester.create_partial_record().unwrap();

        assert!(result.segment.is_some());
        assert!(result.bytes_appended > 0);
    }

    #[test]
    fn test_recovery_after_truncation() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");

        // Create database with data
        {
            let handle = DatabaseHandle::create(&db_path, DatabaseConfig::for_testing()).unwrap();
            for i in 0..10 {
                let record = WalRecord::new(i + 1, handle.uuid(), i * 1000, vec![i as u8; 50]);
                handle.append_wal(&record).unwrap();
            }
            handle.flush_wal().unwrap();
            handle.close().unwrap();
        }

        // Truncate WAL
        let tester = WalCorruptionTester::new(&db_path);
        tester.truncate_wal_tail(100).unwrap();

        // Verify recovery works
        let verification = tester.verify_recovery().unwrap();
        assert!(verification.recovered, "Should recover from truncation");
    }

    #[test]
    fn test_recovery_after_garbage() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");

        // Create database with data
        {
            let handle = DatabaseHandle::create(&db_path, DatabaseConfig::for_testing()).unwrap();
            let record = WalRecord::new(1, handle.uuid(), 1000, vec![1, 2, 3]);
            handle.append_wal(&record).unwrap();
            handle.flush_wal().unwrap();
            handle.close().unwrap();
        }

        // Append garbage
        let tester = WalCorruptionTester::new(&db_path);
        tester.append_garbage(b"INVALID_GARBAGE_DATA").unwrap();

        // Verify recovery works (should truncate garbage)
        let verification = tester.verify_recovery().unwrap();
        assert!(
            verification.recovered,
            "Should recover with garbage truncated"
        );
    }

    #[test]
    fn test_recovery_after_partial_record() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");

        // Create database
        {
            let handle = DatabaseHandle::create(&db_path, DatabaseConfig::for_testing()).unwrap();
            let record = WalRecord::new(1, handle.uuid(), 1000, vec![1, 2, 3]);
            handle.append_wal(&record).unwrap();
            handle.flush_wal().unwrap();
            handle.close().unwrap();
        }

        // Create partial record
        let tester = WalCorruptionTester::new(&db_path);
        tester.create_partial_record().unwrap();

        // Verify recovery works
        let verification = tester.verify_recovery().unwrap();
        assert!(verification.recovered, "Should recover from partial record");
    }
}
