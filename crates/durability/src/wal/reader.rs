//! WAL reader for recovery and replay.
//!
//! The reader handles reading WAL records from segments for recovery.

use crate::codec::StorageCodec;
use crate::format::{WalRecord, WalRecordError, WalSegment};
use std::io::Read;
use std::path::Path;

/// WAL reader for iterating over records in segments.
///
/// The reader can read individual segments or scan all segments in order.
pub struct WalReader {
    /// Storage codec for decoding.
    ///
    /// Currently the codec is stored for future use when codec-aware
    /// decoding is implemented. The identity codec passes through unchanged.
    #[allow(dead_code)]
    codec: Box<dyn StorageCodec>,
}

impl WalReader {
    /// Create a new WAL reader.
    pub fn new(codec: Box<dyn StorageCodec>) -> Self {
        WalReader { codec }
    }

    /// Read all records from a single segment.
    ///
    /// Returns records in order, stopping at the first invalid/incomplete record.
    /// The returned position indicates where valid records end (for truncation).
    pub fn read_segment(
        &self,
        wal_dir: &Path,
        segment_number: u64,
    ) -> Result<(Vec<WalRecord>, u64, ReadStopReason, usize), WalReaderError> {
        let mut segment = WalSegment::open_read(wal_dir, segment_number)
            .map_err(|e: std::io::Error| WalReaderError::IoError(e.to_string()))?;

        self.read_segment_from(&mut segment)
    }

    /// Read records from an already-opened segment.
    pub fn read_segment_from(
        &self,
        segment: &mut WalSegment,
    ) -> Result<(Vec<WalRecord>, u64, ReadStopReason, usize), WalReaderError> {
        let mut records = Vec::new();
        let mut buffer = Vec::new();
        let hdr_size = segment.header_size() as u64;
        let mut valid_end = hdr_size;

        // Seek to start of records (past header)
        segment
            .seek_to(hdr_size)
            .map_err(|e: std::io::Error| WalReaderError::IoError(e.to_string()))?;

        // Read entire segment content after header
        segment
            .file_mut()
            .read_to_end(&mut buffer)
            .map_err(|e: std::io::Error| WalReaderError::IoError(e.to_string()))?;

        let mut offset = 0;
        let mut stop_reason = ReadStopReason::EndOfData;
        let mut skipped_corrupted = 0usize;

        while offset < buffer.len() {
            // Try to decode through codec first
            // For identity codec, this is just the raw bytes
            let remaining = &buffer[offset..];

            // Try to parse a record
            match WalRecord::from_bytes(remaining) {
                Ok((record, consumed)) => {
                    records.push(record);
                    offset += consumed;
                    valid_end = hdr_size + offset as u64;
                }
                Err(WalRecordError::InsufficientData) => {
                    // Partial record at end - this is expected for crash recovery
                    stop_reason = ReadStopReason::PartialRecord;
                    break;
                }
                Err(WalRecordError::ChecksumMismatch { .. }) => {
                    // Try to skip corrupted record using length field
                    let remaining = &buffer[offset..];
                    if remaining.len() >= 4 {
                        let record_len =
                            u32::from_le_bytes(remaining[0..4].try_into().unwrap()) as usize;
                        if record_len > 0
                            && record_len < 64 * 1024 * 1024
                            && remaining.len() >= 4 + record_len
                        {
                            tracing::warn!(
                                offset = offset,
                                "Skipping corrupted WAL record (checksum mismatch)"
                            );
                            offset += 4 + record_len;
                            skipped_corrupted += 1;
                            continue;
                        }
                    }
                    // Can't determine boundary — stop
                    stop_reason = ReadStopReason::ChecksumMismatch { offset };
                    break;
                }
                Err(e) => {
                    // CRC was valid but payload couldn't be parsed.
                    // This indicates codec mismatch or format version incompatibility,
                    // NOT data corruption.
                    stop_reason = ReadStopReason::ParseError {
                        offset,
                        detail: e.to_string(),
                    };
                    break;
                }
            }
        }

        Ok((records, valid_end, stop_reason, skipped_corrupted))
    }

    /// Read all records from all segments in a WAL directory.
    ///
    /// Segments are read in order. Returns all valid records and information
    /// about any truncation needed.
    pub fn read_all(&self, wal_dir: &Path) -> Result<WalReadResult, WalReaderError> {
        let mut segments = self.list_segments(wal_dir)?;
        segments.sort();

        let mut all_records = Vec::new();
        let mut truncate_info = None;
        let mut last_stop_reason = ReadStopReason::EndOfData;
        let mut total_skipped_corrupted = 0usize;

        for (idx, segment_num) in segments.iter().enumerate() {
            let (records, valid_end, stop_reason, skipped) =
                self.read_segment(wal_dir, *segment_num)?;
            all_records.extend(records);
            last_stop_reason = stop_reason;
            total_skipped_corrupted += skipped;

            // Check if this segment needs truncation (only the last one can)
            if idx == segments.len() - 1 {
                let segment = WalSegment::open_read(wal_dir, *segment_num)
                    .map_err(|e: std::io::Error| WalReaderError::IoError(e.to_string()))?;

                if valid_end < segment.size() {
                    truncate_info = Some(TruncateInfo {
                        segment_number: *segment_num,
                        valid_end,
                        original_size: segment.size(),
                    });
                }
            }
        }

        Ok(WalReadResult {
            records: all_records,
            truncate_info,
            stop_reason: last_stop_reason,
            skipped_corrupted: total_skipped_corrupted,
        })
    }

    /// Read records from a segment, filtering by transaction ID.
    ///
    /// Only returns records with txn_id > watermark (for recovery after snapshot).
    pub fn read_segment_after_watermark(
        &self,
        wal_dir: &Path,
        segment_number: u64,
        watermark: u64,
    ) -> Result<Vec<WalRecord>, WalReaderError> {
        let (records, _, _, _) = self.read_segment(wal_dir, segment_number)?;

        Ok(records
            .into_iter()
            .filter(|r| r.txn_id > watermark)
            .collect())
    }

    /// Read all records after a watermark from all segments.
    pub fn read_all_after_watermark(
        &self,
        wal_dir: &Path,
        watermark: u64,
    ) -> Result<Vec<WalRecord>, WalReaderError> {
        let result = self.read_all(wal_dir)?;

        Ok(result
            .records
            .into_iter()
            .filter(|r| r.txn_id > watermark)
            .collect())
    }

    /// List all segment numbers in the WAL directory.
    pub fn list_segments(&self, wal_dir: &Path) -> Result<Vec<u64>, WalReaderError> {
        let mut segments = Vec::new();

        let entries =
            std::fs::read_dir(wal_dir).map_err(|e| WalReaderError::IoError(e.to_string()))?;

        for entry in entries {
            let entry = entry.map_err(|e| WalReaderError::IoError(e.to_string()))?;
            let name = entry.file_name().to_string_lossy().to_string();

            // Expected format: "wal-NNNNNN.seg" where NNNNNN is a 6-digit sequence number
            // Minimum length: "wal-" (4) + 6 digits + ".seg" (4) = 14 chars
            if name.starts_with("wal-") && name.ends_with(".seg") && name.len() >= 14 {
                // Extract the 6-digit sequence number between "wal-" and ".seg"
                if let Ok(num) = name[4..10].parse::<u64>() {
                    segments.push(num);
                }
            }
        }

        segments.sort();
        Ok(segments)
    }

    /// Get the highest transaction ID in a segment.
    pub fn max_txn_id_in_segment(
        &self,
        wal_dir: &Path,
        segment_number: u64,
    ) -> Result<Option<u64>, WalReaderError> {
        let (records, _, _, _) = self.read_segment(wal_dir, segment_number)?;
        Ok(records.iter().map(|r| r.txn_id).max())
    }

    /// Get the highest transaction ID across all segments.
    pub fn max_txn_id(&self, wal_dir: &Path) -> Result<Option<u64>, WalReaderError> {
        let result = self.read_all(wal_dir)?;
        Ok(result.records.iter().map(|r| r.txn_id).max())
    }
}

/// Reason why record reading stopped before reaching end of segment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReadStopReason {
    /// Successfully read all records to end of data
    EndOfData,
    /// Partial record at end of segment (expected after crash)
    PartialRecord,
    /// CRC checksum mismatch - data is corrupted
    ChecksumMismatch {
        /// Byte offset within the segment where the mismatch was detected
        offset: usize,
    },
    /// CRC was valid but payload could not be parsed.
    /// This indicates a codec mismatch, unsupported format version,
    /// or a bug in the record format (not data corruption).
    ParseError {
        /// Byte offset within the segment where parsing failed
        offset: usize,
        /// Human-readable error description
        detail: String,
    },
}

/// Result of reading all WAL segments.
#[derive(Debug)]
pub struct WalReadResult {
    /// All valid records in order
    pub records: Vec<WalRecord>,

    /// Information about truncation needed (if any)
    pub truncate_info: Option<TruncateInfo>,

    /// Why reading stopped (for diagnostics)
    pub stop_reason: ReadStopReason,

    /// Number of corrupted records that were skipped during reading
    pub skipped_corrupted: usize,
}

/// Information about a segment that needs truncation.
#[derive(Debug, Clone)]
pub struct TruncateInfo {
    /// Segment number
    pub segment_number: u64,

    /// Position where valid data ends
    pub valid_end: u64,

    /// Original file size
    pub original_size: u64,
}

impl TruncateInfo {
    /// Get the number of bytes that need to be truncated.
    pub fn bytes_to_truncate(&self) -> u64 {
        self.original_size - self.valid_end
    }
}

/// WAL reader errors.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum WalReaderError {
    /// I/O error
    #[error("I/O error: {0}")]
    IoError(String),

    /// Segment not found
    #[error("Segment not found: {0}")]
    SegmentNotFound(u64),

    /// Record parsing error
    #[error("Record parsing error: {0}")]
    ParseError(String),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codec::IdentityCodec;
    use crate::wal::config::WalConfig;
    use crate::wal::writer::WalWriter;
    use crate::wal::DurabilityMode;
    use tempfile::tempdir;

    fn make_codec() -> Box<dyn StorageCodec> {
        Box::new(IdentityCodec)
    }

    fn write_records(wal_dir: &Path, records: &[WalRecord]) {
        let mut writer = WalWriter::new(
            wal_dir.to_path_buf(),
            [1u8; 16],
            DurabilityMode::Always,
            WalConfig::for_testing(),
            make_codec(),
        )
        .unwrap();

        for record in records {
            writer.append(record).unwrap();
        }

        writer.flush().unwrap();
    }

    #[test]
    fn test_read_empty_segment() {
        let dir = tempdir().unwrap();
        let wal_dir = dir.path().join("wal");

        // Create empty segment
        std::fs::create_dir_all(&wal_dir).unwrap();
        WalSegment::create(&wal_dir, 1, [1u8; 16]).unwrap();

        let reader = WalReader::new(make_codec());
        let (records, _, _, _) = reader.read_segment(&wal_dir, 1).unwrap();

        assert!(records.is_empty());
    }

    #[test]
    fn test_read_single_record() {
        let dir = tempdir().unwrap();
        let wal_dir = dir.path().join("wal");

        let record = WalRecord::new(1, [1u8; 16], 12345, vec![1, 2, 3]);
        write_records(&wal_dir, &[record.clone()]);

        let reader = WalReader::new(make_codec());
        let (records, _, _, _) = reader.read_segment(&wal_dir, 1).unwrap();

        assert_eq!(records.len(), 1);
        assert_eq!(records[0].txn_id, 1);
        assert_eq!(records[0].writeset, vec![1, 2, 3]);
    }

    #[test]
    fn test_read_multiple_records() {
        let dir = tempdir().unwrap();
        let wal_dir = dir.path().join("wal");

        let records: Vec<_> = (1..=5)
            .map(|i| WalRecord::new(i, [1u8; 16], i * 1000, vec![i as u8; 10]))
            .collect();

        write_records(&wal_dir, &records);

        let reader = WalReader::new(make_codec());
        let result = reader.read_all(&wal_dir).unwrap();

        assert_eq!(result.records.len(), 5);
        for (i, record) in result.records.iter().enumerate() {
            assert_eq!(record.txn_id, (i + 1) as u64);
        }
    }

    #[test]
    fn test_read_after_watermark() {
        let dir = tempdir().unwrap();
        let wal_dir = dir.path().join("wal");

        let records: Vec<_> = (1..=10)
            .map(|i| WalRecord::new(i, [1u8; 16], i * 1000, vec![]))
            .collect();

        write_records(&wal_dir, &records);

        let reader = WalReader::new(make_codec());
        let filtered = reader.read_all_after_watermark(&wal_dir, 5).unwrap();

        assert_eq!(filtered.len(), 5);
        assert!(filtered.iter().all(|r| r.txn_id > 5));
    }

    #[test]
    fn test_list_segments() {
        let dir = tempdir().unwrap();
        let wal_dir = dir.path().join("wal");
        std::fs::create_dir_all(&wal_dir).unwrap();

        // Create multiple segments
        WalSegment::create(&wal_dir, 1, [1u8; 16]).unwrap();
        WalSegment::create(&wal_dir, 2, [1u8; 16]).unwrap();
        WalSegment::create(&wal_dir, 3, [1u8; 16]).unwrap();

        let reader = WalReader::new(make_codec());
        let segments = reader.list_segments(&wal_dir).unwrap();

        assert_eq!(segments, vec![1, 2, 3]);
    }

    #[test]
    fn test_max_txn_id() {
        let dir = tempdir().unwrap();
        let wal_dir = dir.path().join("wal");

        let records: Vec<_> = (1..=10)
            .map(|i| WalRecord::new(i, [1u8; 16], 0, vec![]))
            .collect();

        write_records(&wal_dir, &records);

        let reader = WalReader::new(make_codec());
        let max = reader.max_txn_id(&wal_dir).unwrap();

        assert_eq!(max, Some(10));
    }

    #[test]
    fn test_partial_record_detection() {
        let dir = tempdir().unwrap();
        let wal_dir = dir.path().join("wal");

        // Write valid records
        let records: Vec<_> = (1..=3)
            .map(|i| WalRecord::new(i, [1u8; 16], 0, vec![i as u8]))
            .collect();
        write_records(&wal_dir, &records);

        // Append garbage to simulate crash mid-write
        let segment_path = WalSegment::segment_path(&wal_dir, 1);
        use std::io::Write;
        let mut file = std::fs::OpenOptions::new()
            .append(true)
            .open(&segment_path)
            .unwrap();
        file.write_all(&[0xFF; 10]).unwrap(); // Garbage bytes

        let reader = WalReader::new(make_codec());
        let result = reader.read_all(&wal_dir).unwrap();

        // Should still read valid records
        assert_eq!(result.records.len(), 3);

        // Should report truncation needed
        assert!(result.truncate_info.is_some());
        let truncate = result.truncate_info.unwrap();
        assert_eq!(truncate.bytes_to_truncate(), 10);
    }
}
