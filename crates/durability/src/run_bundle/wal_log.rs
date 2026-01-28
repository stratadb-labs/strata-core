//! WAL.runlog reader and writer
//!
//! This module handles the binary format for run-scoped WAL entries
//! within a RunBundle archive.
//!
//! ## Format
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────────┐
//! │ Header (16 bytes)                                               │
//! ├─────────────────────────────────────────────────────────────────┤
//! │ Magic: "STRATA_WAL" (10 bytes)                                  │
//! │ Version: u16 (2 bytes, LE)                                      │
//! │ Entry Count: u32 (4 bytes, LE)                                  │
//! ├─────────────────────────────────────────────────────────────────┤
//! │ Entries (variable)                                              │
//! ├─────────────────────────────────────────────────────────────────┤
//! │ For each entry:                                                 │
//! │   Length: u32 (4 bytes, LE)                                     │
//! │   Data: [u8; length] (bincode-serialized WALEntry)              │
//! │   CRC32: u32 (4 bytes, LE)                                      │
//! └─────────────────────────────────────────────────────────────────┘
//! ```

use crate::run_bundle::error::{RunBundleError, RunBundleResult};
use crate::run_bundle::types::{xxh3_hex, WAL_RUNLOG_MAGIC, WAL_RUNLOG_VERSION};
use crate::wal::WALEntry;
use strata_core::types::RunId;
use std::io::{Read, Write};

/// Header size in bytes: magic (10) + version (2) + count (4)
const HEADER_SIZE: usize = 16;

// =============================================================================
// WAL Filtering
// =============================================================================

/// Filter WAL entries to only include those belonging to a specific run
///
/// This filters out:
/// - Checkpoint entries (they don't belong to any single run)
/// - Entries belonging to other runs
pub fn filter_wal_for_run(entries: &[WALEntry], run_id: &RunId) -> Vec<WALEntry> {
    entries
        .iter()
        .filter(|entry| entry.run_id() == Some(*run_id))
        .cloned()
        .collect()
}

// =============================================================================
// Writer
// =============================================================================

/// Information about written WAL log
#[derive(Debug, Clone)]
pub struct WalLogInfo {
    /// Number of entries written
    pub entry_count: u64,
    /// Total bytes written (including header and per-entry overhead)
    pub bytes_written: u64,
    /// xxh3 checksum of the entire file content
    pub checksum: String,
}

/// Writer for WAL.runlog format
pub struct WalLogWriter;

impl WalLogWriter {
    /// Write WAL entries to a writer in .runlog format
    ///
    /// Returns information about the written data including checksum.
    pub fn write<W: Write>(entries: &[WALEntry], mut writer: W) -> RunBundleResult<WalLogInfo> {
        let mut buffer = Vec::new();

        // Write header
        buffer.extend_from_slice(WAL_RUNLOG_MAGIC);
        buffer.extend_from_slice(&WAL_RUNLOG_VERSION.to_le_bytes());
        buffer.extend_from_slice(&(entries.len() as u32).to_le_bytes());

        // Write entries
        for entry in entries {
            // Serialize entry with bincode
            let entry_data = bincode::serialize(entry)
                .map_err(|e| RunBundleError::serialization(format!("bincode encode: {}", e)))?;

            // Calculate CRC32 of entry data
            let crc = crc32fast::hash(&entry_data);

            // Write: length + data + crc
            buffer.extend_from_slice(&(entry_data.len() as u32).to_le_bytes());
            buffer.extend_from_slice(&entry_data);
            buffer.extend_from_slice(&crc.to_le_bytes());
        }

        // Calculate checksum of entire buffer
        let checksum = xxh3_hex(&buffer);

        // Write to output
        let bytes_written = buffer.len() as u64;
        writer
            .write_all(&buffer)
            .map_err(RunBundleError::from)?;

        Ok(WalLogInfo {
            entry_count: entries.len() as u64,
            bytes_written,
            checksum,
        })
    }

    /// Write WAL entries to a Vec<u8>
    ///
    /// Convenience method for testing and in-memory operations.
    pub fn write_to_vec(entries: &[WALEntry]) -> RunBundleResult<(Vec<u8>, WalLogInfo)> {
        let mut buffer = Vec::new();
        let info = Self::write(entries, &mut buffer)?;
        Ok((buffer, info))
    }
}

// =============================================================================
// Reader
// =============================================================================

/// Reader for WAL.runlog format
pub struct WalLogReader;

impl WalLogReader {
    /// Read all WAL entries from a reader
    ///
    /// Validates the header and CRC32 of each entry.
    pub fn read<R: Read>(mut reader: R) -> RunBundleResult<Vec<WALEntry>> {
        // Read header
        let mut header = [0u8; HEADER_SIZE];
        reader
            .read_exact(&mut header)
            .map_err(|e| RunBundleError::invalid_bundle(format!("failed to read header: {}", e)))?;

        // Validate magic
        if &header[0..10] != WAL_RUNLOG_MAGIC {
            return Err(RunBundleError::invalid_bundle(format!(
                "invalid magic: expected {:?}, got {:?}",
                WAL_RUNLOG_MAGIC,
                &header[0..10]
            )));
        }

        // Parse version
        let version = u16::from_le_bytes([header[10], header[11]]);
        if version != WAL_RUNLOG_VERSION {
            return Err(RunBundleError::UnsupportedVersion {
                version: version as u32,
            });
        }

        // Parse entry count
        let entry_count = u32::from_le_bytes([header[12], header[13], header[14], header[15]]);

        // Read entries
        let mut entries = Vec::with_capacity(entry_count as usize);
        for i in 0..entry_count {
            // Read length
            let mut len_bytes = [0u8; 4];
            reader.read_exact(&mut len_bytes).map_err(|e| {
                RunBundleError::InvalidWalEntry {
                    index: i as usize,
                    reason: format!("failed to read length: {}", e),
                }
            })?;
            let len = u32::from_le_bytes(len_bytes) as usize;

            // Sanity check on length
            if len > 100 * 1024 * 1024 {
                // 100MB max per entry
                return Err(RunBundleError::InvalidWalEntry {
                    index: i as usize,
                    reason: format!("entry length {} exceeds maximum", len),
                });
            }

            // Read entry data
            let mut data = vec![0u8; len];
            reader.read_exact(&mut data).map_err(|e| {
                RunBundleError::InvalidWalEntry {
                    index: i as usize,
                    reason: format!("failed to read data: {}", e),
                }
            })?;

            // Read and verify CRC32
            let mut crc_bytes = [0u8; 4];
            reader.read_exact(&mut crc_bytes).map_err(|e| {
                RunBundleError::InvalidWalEntry {
                    index: i as usize,
                    reason: format!("failed to read crc: {}", e),
                }
            })?;
            let expected_crc = u32::from_le_bytes(crc_bytes);
            let actual_crc = crc32fast::hash(&data);

            if expected_crc != actual_crc {
                return Err(RunBundleError::InvalidWalEntry {
                    index: i as usize,
                    reason: format!(
                        "CRC mismatch: expected {:08x}, got {:08x}",
                        expected_crc, actual_crc
                    ),
                });
            }

            // Deserialize entry
            let entry: WALEntry = bincode::deserialize(&data).map_err(|e| {
                RunBundleError::InvalidWalEntry {
                    index: i as usize,
                    reason: format!("bincode decode: {}", e),
                }
            })?;

            entries.push(entry);
        }

        Ok(entries)
    }

    /// Read WAL entries from a byte slice
    ///
    /// Convenience method for testing and in-memory operations.
    pub fn read_from_slice(data: &[u8]) -> RunBundleResult<Vec<WALEntry>> {
        Self::read(std::io::Cursor::new(data))
    }

    /// Validate a WAL.runlog without fully parsing entries
    ///
    /// Checks header and entry CRCs without deserializing entry contents.
    /// Returns the entry count if valid.
    pub fn validate<R: Read>(mut reader: R) -> RunBundleResult<u32> {
        // Read header
        let mut header = [0u8; HEADER_SIZE];
        reader
            .read_exact(&mut header)
            .map_err(|e| RunBundleError::invalid_bundle(format!("failed to read header: {}", e)))?;

        // Validate magic
        if &header[0..10] != WAL_RUNLOG_MAGIC {
            return Err(RunBundleError::invalid_bundle("invalid magic"));
        }

        // Parse version
        let version = u16::from_le_bytes([header[10], header[11]]);
        if version != WAL_RUNLOG_VERSION {
            return Err(RunBundleError::UnsupportedVersion {
                version: version as u32,
            });
        }

        // Parse entry count
        let entry_count = u32::from_le_bytes([header[12], header[13], header[14], header[15]]);

        // Validate each entry's CRC without deserializing
        for i in 0..entry_count {
            // Read length
            let mut len_bytes = [0u8; 4];
            reader.read_exact(&mut len_bytes).map_err(|e| {
                RunBundleError::InvalidWalEntry {
                    index: i as usize,
                    reason: format!("failed to read length: {}", e),
                }
            })?;
            let len = u32::from_le_bytes(len_bytes) as usize;

            // Sanity check
            if len > 100 * 1024 * 1024 {
                return Err(RunBundleError::InvalidWalEntry {
                    index: i as usize,
                    reason: format!("entry length {} exceeds maximum", len),
                });
            }

            // Read entry data
            let mut data = vec![0u8; len];
            reader.read_exact(&mut data).map_err(|e| {
                RunBundleError::InvalidWalEntry {
                    index: i as usize,
                    reason: format!("failed to read data: {}", e),
                }
            })?;

            // Read and verify CRC32
            let mut crc_bytes = [0u8; 4];
            reader.read_exact(&mut crc_bytes).map_err(|e| {
                RunBundleError::InvalidWalEntry {
                    index: i as usize,
                    reason: format!("failed to read crc: {}", e),
                }
            })?;
            let expected_crc = u32::from_le_bytes(crc_bytes);
            let actual_crc = crc32fast::hash(&data);

            if expected_crc != actual_crc {
                return Err(RunBundleError::InvalidWalEntry {
                    index: i as usize,
                    reason: format!(
                        "CRC mismatch: expected {:08x}, got {:08x}",
                        expected_crc, actual_crc
                    ),
                });
            }
        }

        Ok(entry_count)
    }

    /// Get header info without reading entries
    ///
    /// Returns (version, entry_count) if header is valid.
    pub fn read_header<R: Read>(mut reader: R) -> RunBundleResult<(u16, u32)> {
        let mut header = [0u8; HEADER_SIZE];
        reader
            .read_exact(&mut header)
            .map_err(|e| RunBundleError::invalid_bundle(format!("failed to read header: {}", e)))?;

        // Validate magic
        if &header[0..10] != WAL_RUNLOG_MAGIC {
            return Err(RunBundleError::invalid_bundle("invalid magic"));
        }

        let version = u16::from_le_bytes([header[10], header[11]]);
        let entry_count = u32::from_le_bytes([header[12], header[13], header[14], header[15]]);

        Ok((version, entry_count))
    }
}

// =============================================================================
// Streaming Reader (for large files)
// =============================================================================

/// Iterator over WAL entries for streaming reads
pub struct WalLogIterator<R: Read> {
    reader: R,
    remaining: u32,
    index: usize,
}

impl<R: Read> WalLogIterator<R> {
    /// Create a new iterator from a reader
    ///
    /// Reads and validates the header, then returns an iterator over entries.
    pub fn new(mut reader: R) -> RunBundleResult<Self> {
        let (version, entry_count) = WalLogReader::read_header(&mut reader)?;

        if version != WAL_RUNLOG_VERSION {
            return Err(RunBundleError::UnsupportedVersion {
                version: version as u32,
            });
        }

        Ok(Self {
            reader,
            remaining: entry_count,
            index: 0,
        })
    }

    /// Get the total number of entries
    pub fn entry_count(&self) -> u32 {
        self.remaining + self.index as u32
    }
}

impl<R: Read> Iterator for WalLogIterator<R> {
    type Item = RunBundleResult<WALEntry>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.remaining == 0 {
            return None;
        }

        let result = read_single_entry(&mut self.reader, self.index);
        self.remaining -= 1;
        self.index += 1;

        Some(result)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.remaining as usize, Some(self.remaining as usize))
    }
}

/// Read a single entry from a reader
fn read_single_entry<R: Read>(reader: &mut R, index: usize) -> RunBundleResult<WALEntry> {
    // Read length
    let mut len_bytes = [0u8; 4];
    reader.read_exact(&mut len_bytes).map_err(|e| {
        RunBundleError::InvalidWalEntry {
            index,
            reason: format!("failed to read length: {}", e),
        }
    })?;
    let len = u32::from_le_bytes(len_bytes) as usize;

    // Sanity check
    if len > 100 * 1024 * 1024 {
        return Err(RunBundleError::InvalidWalEntry {
            index,
            reason: format!("entry length {} exceeds maximum", len),
        });
    }

    // Read data
    let mut data = vec![0u8; len];
    reader.read_exact(&mut data).map_err(|e| {
        RunBundleError::InvalidWalEntry {
            index,
            reason: format!("failed to read data: {}", e),
        }
    })?;

    // Read and verify CRC
    let mut crc_bytes = [0u8; 4];
    reader.read_exact(&mut crc_bytes).map_err(|e| {
        RunBundleError::InvalidWalEntry {
            index,
            reason: format!("failed to read crc: {}", e),
        }
    })?;
    let expected_crc = u32::from_le_bytes(crc_bytes);
    let actual_crc = crc32fast::hash(&data);

    if expected_crc != actual_crc {
        return Err(RunBundleError::InvalidWalEntry {
            index,
            reason: format!(
                "CRC mismatch: expected {:08x}, got {:08x}",
                expected_crc, actual_crc
            ),
        });
    }

    // Deserialize
    bincode::deserialize(&data).map_err(|e| RunBundleError::InvalidWalEntry {
        index,
        reason: format!("bincode decode: {}", e),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use strata_core::types::{Key, Namespace, TypeTag};
    use strata_core::value::Value;
    use strata_core::Timestamp;

    fn make_test_run_id() -> RunId {
        RunId::new()
    }

    fn make_test_entries(run_id: RunId) -> Vec<WALEntry> {
        let ns = Namespace::for_run(run_id);
        vec![
            WALEntry::BeginTxn {
                txn_id: 1,
                run_id,
                timestamp: Timestamp::now(),
            },
            WALEntry::Write {
                run_id,
                key: Key::new(ns.clone(), TypeTag::KV, b"key1".to_vec()),
                value: Value::String("value1".to_string()),
                version: 1,
            },
            WALEntry::Write {
                run_id,
                key: Key::new(ns.clone(), TypeTag::KV, b"key2".to_vec()),
                value: Value::Int(42),
                version: 2,
            },
            WALEntry::CommitTxn { txn_id: 1, run_id },
        ]
    }

    #[test]
    fn test_write_read_roundtrip() {
        let run_id = make_test_run_id();
        let entries = make_test_entries(run_id);

        // Write
        let (data, info) = WalLogWriter::write_to_vec(&entries).unwrap();

        assert_eq!(info.entry_count, 4);
        assert!(info.bytes_written > HEADER_SIZE as u64);
        assert!(!info.checksum.is_empty());

        // Read
        let read_entries = WalLogReader::read_from_slice(&data).unwrap();

        assert_eq!(read_entries.len(), entries.len());
        for (original, read) in entries.iter().zip(read_entries.iter()) {
            assert_eq!(original, read);
        }
    }

    #[test]
    fn test_empty_entries() {
        let entries: Vec<WALEntry> = vec![];

        let (data, info) = WalLogWriter::write_to_vec(&entries).unwrap();

        assert_eq!(info.entry_count, 0);
        assert_eq!(info.bytes_written, HEADER_SIZE as u64);

        let read_entries = WalLogReader::read_from_slice(&data).unwrap();
        assert!(read_entries.is_empty());
    }

    #[test]
    fn test_validate_only() {
        let run_id = make_test_run_id();
        let entries = make_test_entries(run_id);

        let (data, _) = WalLogWriter::write_to_vec(&entries).unwrap();

        // Validate without full parse
        let count = WalLogReader::validate(std::io::Cursor::new(&data)).unwrap();
        assert_eq!(count, 4);
    }

    #[test]
    fn test_read_header() {
        let run_id = make_test_run_id();
        let entries = make_test_entries(run_id);

        let (data, _) = WalLogWriter::write_to_vec(&entries).unwrap();

        let (version, count) = WalLogReader::read_header(std::io::Cursor::new(&data)).unwrap();
        assert_eq!(version, WAL_RUNLOG_VERSION);
        assert_eq!(count, 4);
    }

    #[test]
    fn test_invalid_magic() {
        let mut data = vec![0u8; HEADER_SIZE];
        data[0..10].copy_from_slice(b"WRONG_MAGI");

        let result = WalLogReader::read_from_slice(&data);
        assert!(matches!(result, Err(RunBundleError::InvalidBundle(_))));
    }

    #[test]
    fn test_corrupted_entry_crc() {
        let run_id = make_test_run_id();
        let entries = make_test_entries(run_id);

        let (mut data, _) = WalLogWriter::write_to_vec(&entries).unwrap();

        // Corrupt a byte in the first entry's data
        if data.len() > HEADER_SIZE + 10 {
            data[HEADER_SIZE + 10] ^= 0xFF;
        }

        let result = WalLogReader::read_from_slice(&data);
        assert!(matches!(
            result,
            Err(RunBundleError::InvalidWalEntry { index: 0, .. })
        ));
    }

    #[test]
    fn test_streaming_iterator() {
        let run_id = make_test_run_id();
        let entries = make_test_entries(run_id);

        let (data, _) = WalLogWriter::write_to_vec(&entries).unwrap();

        let iter = WalLogIterator::new(std::io::Cursor::new(&data)).unwrap();
        assert_eq!(iter.entry_count(), 4);

        let read_entries: Vec<WALEntry> = iter.map(|r| r.unwrap()).collect();
        assert_eq!(read_entries.len(), entries.len());
    }

    #[test]
    fn test_filter_wal_for_run() {
        let run1 = make_test_run_id();
        let run2 = make_test_run_id();

        let entries = vec![
            WALEntry::BeginTxn {
                txn_id: 1,
                run_id: run1,
                timestamp: Timestamp::now(),
            },
            WALEntry::BeginTxn {
                txn_id: 2,
                run_id: run2,
                timestamp: Timestamp::now(),
            },
            WALEntry::CommitTxn {
                txn_id: 1,
                run_id: run1,
            },
            WALEntry::CommitTxn {
                txn_id: 2,
                run_id: run2,
            },
        ];

        let run1_entries = filter_wal_for_run(&entries, &run1);
        assert_eq!(run1_entries.len(), 2);
        assert!(run1_entries.iter().all(|e| e.run_id() == Some(run1)));

        let run2_entries = filter_wal_for_run(&entries, &run2);
        assert_eq!(run2_entries.len(), 2);
        assert!(run2_entries.iter().all(|e| e.run_id() == Some(run2)));
    }

    #[test]
    fn test_filter_excludes_checkpoint() {
        let run_id = make_test_run_id();

        let entries = vec![
            WALEntry::BeginTxn {
                txn_id: 1,
                run_id,
                timestamp: Timestamp::now(),
            },
            WALEntry::Checkpoint {
                snapshot_id: uuid::Uuid::new_v4(),
                version: 100,
                active_runs: vec![run_id],
            },
            WALEntry::CommitTxn { txn_id: 1, run_id },
        ];

        let filtered = filter_wal_for_run(&entries, &run_id);
        assert_eq!(filtered.len(), 2);
        assert!(filtered.iter().all(|e| !e.is_checkpoint()));
    }

    #[test]
    fn test_checksum_consistency() {
        let run_id = make_test_run_id();
        let entries = make_test_entries(run_id);

        // Write twice, checksums should match
        let (data1, info1) = WalLogWriter::write_to_vec(&entries).unwrap();
        let (data2, info2) = WalLogWriter::write_to_vec(&entries).unwrap();

        assert_eq!(info1.checksum, info2.checksum);
        assert_eq!(data1, data2);
    }

    #[test]
    fn test_large_entry() {
        let run_id = make_test_run_id();
        let ns = Namespace::for_run(run_id);

        // Create entry with large value
        let large_value = "x".repeat(1024 * 1024); // 1MB string
        let entries = vec![WALEntry::Write {
            run_id,
            key: Key::new(ns, TypeTag::KV, b"large_key".to_vec()),
            value: Value::String(large_value),
            version: 1,
        }];

        let (data, info) = WalLogWriter::write_to_vec(&entries).unwrap();
        assert!(info.bytes_written > 1024 * 1024);

        let read_entries = WalLogReader::read_from_slice(&data).unwrap();
        assert_eq!(read_entries.len(), 1);
        assert_eq!(entries[0], read_entries[0]);
    }

    // ========================================================================
    // Adversarial WAL.runlog Tests
    // ========================================================================

    #[test]
    fn test_truncated_header() {
        // Only 5 bytes (less than HEADER_SIZE)
        let data = vec![0u8; 5];
        let result = WalLogReader::read_from_slice(&data);
        assert!(result.is_err());
    }

    #[test]
    fn test_wrong_version() {
        let mut data = vec![0u8; HEADER_SIZE];
        data[0..10].copy_from_slice(WAL_RUNLOG_MAGIC);
        // Set version to 999
        data[10..12].copy_from_slice(&999u16.to_le_bytes());
        data[12..16].copy_from_slice(&0u32.to_le_bytes()); // 0 entries

        let result = WalLogReader::read_from_slice(&data);
        assert!(result.is_err());
    }

    #[test]
    fn test_entry_count_mismatch_more_declared() {
        let run_id = make_test_run_id();
        let entries = make_test_entries(run_id);

        let (mut data, _) = WalLogWriter::write_to_vec(&entries).unwrap();

        // Bump entry count to 999 (more than actual)
        data[12..16].copy_from_slice(&999u32.to_le_bytes());

        // Should fail when trying to read more entries than exist
        let result = WalLogReader::read_from_slice(&data);
        assert!(result.is_err());
    }

    #[test]
    fn test_filter_nonexistent_run() {
        let run_id = make_test_run_id();
        let other_run = make_test_run_id();
        let entries = make_test_entries(run_id);

        let filtered = filter_wal_for_run(&entries, &other_run);
        assert!(filtered.is_empty());
    }

    #[test]
    fn test_filter_empty_entries() {
        let run_id = make_test_run_id();
        let entries: Vec<WALEntry> = vec![];
        let filtered = filter_wal_for_run(&entries, &run_id);
        assert!(filtered.is_empty());
    }

    #[test]
    fn test_iterator_stops_on_corruption() {
        let run_id = make_test_run_id();
        let entries = make_test_entries(run_id);

        let (mut data, _) = WalLogWriter::write_to_vec(&entries).unwrap();

        // Corrupt a data byte in the second entry area
        let corrupt_pos = HEADER_SIZE + 30;
        if corrupt_pos < data.len() {
            data[corrupt_pos] ^= 0xFF;
        }

        let iter = WalLogIterator::new(std::io::Cursor::new(&data)).unwrap();
        let results: Vec<_> = iter.collect();

        // At least one entry should fail
        assert!(
            results.iter().any(|r| r.is_err()),
            "Iterator should encounter corruption"
        );
    }

    #[test]
    fn test_vector_entries_in_runlog() {
        let run_id = make_test_run_id();

        let entries = vec![
            WALEntry::VectorCollectionCreate {
                run_id,
                collection: "col".to_string(),
                dimension: 128,
                metric: 0,
                version: 1,
            },
            WALEntry::VectorUpsert {
                run_id,
                collection: "col".to_string(),
                key: "k1".to_string(),
                vector_id: 1,
                embedding: vec![0.1; 128],
                metadata: None,
                version: 2,
                source_ref: None,
            },
            WALEntry::VectorDelete {
                run_id,
                collection: "col".to_string(),
                key: "k1".to_string(),
                vector_id: 1,
                version: 3,
            },
            WALEntry::VectorCollectionDelete {
                run_id,
                collection: "col".to_string(),
                version: 4,
            },
        ];

        let (data, info) = WalLogWriter::write_to_vec(&entries).unwrap();
        assert_eq!(info.entry_count, 4);

        let read_entries = WalLogReader::read_from_slice(&data).unwrap();
        assert_eq!(read_entries.len(), 4);

        for (original, read) in entries.iter().zip(read_entries.iter()) {
            assert_eq!(original, read);
        }
    }

    #[test]
    fn test_checksum_changes_with_different_data() {
        let run_id = make_test_run_id();
        let ns = Namespace::for_run(run_id);

        let entries1 = vec![WALEntry::Write {
            run_id,
            key: Key::new(ns.clone(), TypeTag::KV, b"key1".to_vec()),
            value: Value::Int(1),
            version: 1,
        }];

        let entries2 = vec![WALEntry::Write {
            run_id,
            key: Key::new(ns, TypeTag::KV, b"key1".to_vec()),
            value: Value::Int(2), // Different value
            version: 1,
        }];

        let (_, info1) = WalLogWriter::write_to_vec(&entries1).unwrap();
        let (_, info2) = WalLogWriter::write_to_vec(&entries2).unwrap();

        assert_ne!(
            info1.checksum, info2.checksum,
            "Different data should produce different checksums"
        );
    }
}
