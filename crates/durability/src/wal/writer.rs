//! WAL writer with durability mode support.
//!
//! The writer handles appending WAL records to segments with proper
//! durability guarantees based on the configured mode.

use crate::codec::StorageCodec;
use crate::format::{WalRecord, WalSegment, SEGMENT_HEADER_SIZE};
use crate::wal::config::WalConfig;
use super::DurabilityMode;
use std::path::{Path, PathBuf};
use std::time::Instant;

/// WAL writer with configurable durability modes.
///
/// The writer manages WAL segments and handles record appending with
/// appropriate fsync behavior based on the durability mode.
///
/// # Durability Modes
///
/// - `None`: No persistence - records are not written to disk
/// - `Strict`: fsync after every record - maximum durability
/// - `Batched`: fsync periodically based on time/count
///
/// # Segment Rotation
///
/// When the current segment exceeds the configured size limit, the writer
/// automatically rotates to a new segment. Closed segments are immutable.
pub struct WalWriter {
    /// Current active segment (None for DurabilityMode::None)
    segment: Option<WalSegment>,

    /// Durability mode
    durability: DurabilityMode,

    /// WAL directory
    wal_dir: PathBuf,

    /// Database UUID
    database_uuid: [u8; 16],

    /// Configuration
    config: WalConfig,

    /// Storage codec for encoding
    codec: Box<dyn StorageCodec>,

    /// Bytes written since last fsync (for Batched mode)
    bytes_since_sync: u64,

    /// Writes since last fsync (for Batched mode)
    writes_since_sync: usize,

    /// Last fsync time (for Batched mode)
    last_sync_time: Instant,

    /// Current segment number
    current_segment_number: u64,
}

impl WalWriter {
    /// Create a new WAL writer.
    ///
    /// If the WAL directory contains existing segments, the writer will
    /// either open the last segment for appending or create a new one.
    pub fn new(
        wal_dir: PathBuf,
        database_uuid: [u8; 16],
        durability: DurabilityMode,
        config: WalConfig,
        codec: Box<dyn StorageCodec>,
    ) -> std::io::Result<Self> {
        // For None mode, don't create any files
        if !durability.requires_wal() {
            return Ok(WalWriter {
                segment: None,
                durability,
                wal_dir,
                database_uuid,
                config,
                codec,
                bytes_since_sync: 0,
                writes_since_sync: 0,
                last_sync_time: Instant::now(),
                current_segment_number: 0,
            });
        }

        // Ensure WAL directory exists
        std::fs::create_dir_all(&wal_dir)?;

        // Find the latest segment
        let latest_segment = Self::find_latest_segment(&wal_dir);

        let (segment, segment_number) = match latest_segment {
            Some(num) => {
                // Try to open existing segment for appending
                match WalSegment::open_append(&wal_dir, num) {
                    Ok(seg) => (seg, num),
                    Err(_) => {
                        // Segment might be corrupted or closed, create new one
                        let new_num = num + 1;
                        let seg = WalSegment::create(&wal_dir, new_num, database_uuid)?;
                        (seg, new_num)
                    }
                }
            }
            None => {
                // No existing segments, create first one
                let seg = WalSegment::create(&wal_dir, 1, database_uuid)?;
                (seg, 1)
            }
        };

        Ok(WalWriter {
            segment: Some(segment),
            durability,
            wal_dir,
            database_uuid,
            config,
            codec,
            bytes_since_sync: 0,
            writes_since_sync: 0,
            last_sync_time: Instant::now(),
            current_segment_number: segment_number,
        })
    }

    /// Append a record to the WAL.
    ///
    /// Respects the configured durability mode:
    /// - `None`: No-op, returns immediately
    /// - `Strict`: Writes and fsyncs before returning
    /// - `Batched`: Writes, fsyncs periodically
    pub fn append(&mut self, record: &WalRecord) -> std::io::Result<()> {
        // None mode: no persistence
        if !self.durability.requires_wal() {
            return Ok(());
        }

        let segment = self
            .segment
            .as_mut()
            .expect("Segment should exist for non-None mode");

        // Serialize record
        let record_bytes = record.to_bytes();

        // Encode through codec
        let encoded = self.codec.encode(&record_bytes);

        // Check if we need to rotate before writing
        if segment.size() + encoded.len() as u64 > self.config.segment_size {
            self.rotate_segment()?;
        }

        // Write to segment
        let segment = self.segment.as_mut().unwrap();
        segment.write(&encoded)?;

        self.bytes_since_sync += encoded.len() as u64;
        self.writes_since_sync += 1;

        // Handle sync based on durability mode
        self.maybe_sync()?;

        Ok(())
    }

    /// Handle fsync based on durability mode.
    fn maybe_sync(&mut self) -> std::io::Result<()> {
        match self.durability {
            DurabilityMode::Strict => {
                // Always sync immediately
                if let Some(ref mut segment) = self.segment {
                    segment.sync()?;
                }
                self.reset_sync_counters();
            }
            DurabilityMode::Batched {
                interval_ms,
                batch_size,
            } => {
                let should_sync = self.writes_since_sync >= batch_size
                    || self.last_sync_time.elapsed().as_millis() as u64 >= interval_ms
                    || self.bytes_since_sync >= self.config.buffered_sync_bytes;

                if should_sync {
                    if let Some(ref mut segment) = self.segment {
                        segment.sync()?;
                    }
                    self.reset_sync_counters();
                }
            }
            DurabilityMode::None => {
                // No sync needed
            }
        }

        Ok(())
    }

    /// Reset sync tracking counters.
    fn reset_sync_counters(&mut self) {
        self.bytes_since_sync = 0;
        self.writes_since_sync = 0;
        self.last_sync_time = Instant::now();
    }

    /// Rotate to a new segment.
    ///
    /// Closes the current segment (making it immutable) and creates a new one.
    fn rotate_segment(&mut self) -> std::io::Result<()> {
        // Close current segment
        if let Some(ref mut segment) = self.segment {
            segment.close()?;
        }

        // Create new segment
        self.current_segment_number += 1;
        let new_segment = WalSegment::create(
            &self.wal_dir,
            self.current_segment_number,
            self.database_uuid,
        )?;

        self.segment = Some(new_segment);
        self.reset_sync_counters();

        Ok(())
    }

    /// Force flush any buffered data to disk.
    ///
    /// This ensures all written records are persisted, regardless of
    /// durability mode settings.
    pub fn flush(&mut self) -> std::io::Result<()> {
        if let Some(ref mut segment) = self.segment {
            segment.sync()?;
        }
        self.reset_sync_counters();
        Ok(())
    }

    /// Get the current segment number.
    pub fn current_segment(&self) -> u64 {
        self.current_segment_number
    }

    /// Get the current segment size in bytes.
    pub fn current_segment_size(&self) -> u64 {
        self.segment
            .as_ref()
            .map(|s: &WalSegment| s.size())
            .unwrap_or(SEGMENT_HEADER_SIZE as u64)
    }

    /// Get the WAL directory path.
    pub fn wal_dir(&self) -> &Path {
        &self.wal_dir
    }

    /// Find the latest segment number in the WAL directory.
    fn find_latest_segment(dir: &Path) -> Option<u64> {
        std::fs::read_dir(dir)
            .ok()?
            .filter_map(|e| e.ok())
            .filter_map(|e| {
                let name = e.file_name().to_string_lossy().to_string();
                if name.starts_with("wal-") && name.ends_with(".seg") {
                    // Extract segment number from "wal-NNNNNN.seg"
                    let num_str = &name[4..10];
                    num_str.parse::<u64>().ok()
                } else {
                    None
                }
            })
            .max()
    }

    /// List all segment numbers in order.
    pub fn list_segments(&self) -> std::io::Result<Vec<u64>> {
        let mut segments = Vec::new();

        for entry in std::fs::read_dir(&self.wal_dir)? {
            let entry = entry?;
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with("wal-") && name.ends_with(".seg") {
                if let Ok(num) = name[4..10].parse::<u64>() {
                    segments.push(num);
                }
            }
        }

        segments.sort();
        Ok(segments)
    }

    /// Close the writer, ensuring all data is flushed.
    pub fn close(mut self) -> std::io::Result<()> {
        self.flush()?;
        if let Some(ref mut segment) = self.segment {
            segment.close()?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codec::IdentityCodec;
    use tempfile::tempdir;

    fn make_writer(dir: &Path, durability: DurabilityMode) -> WalWriter {
        WalWriter::new(
            dir.to_path_buf(),
            [1u8; 16],
            durability,
            WalConfig::for_testing(),
            Box::new(IdentityCodec),
        )
        .unwrap()
    }

    fn make_record(txn_id: u64) -> WalRecord {
        WalRecord::new(txn_id, [1u8; 16], 12345, vec![1, 2, 3])
    }

    #[test]
    fn test_inmemory_mode_no_files() {
        let dir = tempdir().unwrap();

        let mut writer = make_writer(dir.path(), DurabilityMode::None);
        writer.append(&make_record(1)).unwrap();
        writer.append(&make_record(2)).unwrap();

        // No files should be created
        assert!(std::fs::read_dir(dir.path()).unwrap().next().is_none());
    }

    #[test]
    fn test_strict_mode_creates_segment() {
        let dir = tempdir().unwrap();
        let wal_dir = dir.path().join("wal");

        let mut writer = make_writer(&wal_dir, DurabilityMode::Strict);
        writer.append(&make_record(1)).unwrap();

        // Segment should exist
        assert!(WalSegment::segment_path(&wal_dir, 1).exists());
    }

    #[test]
    fn test_segment_rotation() {
        let dir = tempdir().unwrap();
        let wal_dir = dir.path().join("wal");

        // Use very small segment size to force rotation
        let config = WalConfig::new()
            .with_segment_size(100) // Very small
            .with_buffered_sync_bytes(50);

        let mut writer = WalWriter::new(
            wal_dir.clone(),
            [1u8; 16],
            DurabilityMode::Strict,
            config,
            Box::new(IdentityCodec),
        )
        .unwrap();

        // Write enough records to trigger rotation
        for i in 0..10 {
            writer
                .append(&WalRecord::new(i, [1u8; 16], 0, vec![0; 50]))
                .unwrap();
        }

        // Should have multiple segments
        let segments = writer.list_segments().unwrap();
        assert!(
            segments.len() > 1,
            "Should have rotated to multiple segments"
        );
    }

    #[test]
    fn test_flush() {
        let dir = tempdir().unwrap();
        let wal_dir = dir.path().join("wal");

        let mut writer = make_writer(
            &wal_dir,
            DurabilityMode::Batched {
                interval_ms: 10000,
                batch_size: 10000,
            },
        );

        writer.append(&make_record(1)).unwrap();
        writer.flush().unwrap();

        // File should be synced
        assert!(WalSegment::segment_path(&wal_dir, 1).exists());
    }

    #[test]
    fn test_close() {
        let dir = tempdir().unwrap();
        let wal_dir = dir.path().join("wal");

        let mut writer = make_writer(&wal_dir, DurabilityMode::Strict);
        writer.append(&make_record(1)).unwrap();
        writer.close().unwrap();

        // Should be able to reopen
        let writer2 = make_writer(&wal_dir, DurabilityMode::Strict);
        assert!(writer2.current_segment() >= 1);
    }

    #[test]
    fn test_resume_existing_segment() {
        let dir = tempdir().unwrap();
        let wal_dir = dir.path().join("wal");

        // Write some data
        {
            let mut writer = make_writer(&wal_dir, DurabilityMode::Strict);
            writer.append(&make_record(1)).unwrap();
            writer.flush().unwrap();
            // Don't close, just drop
        }

        // Reopen and continue
        {
            let mut writer = make_writer(&wal_dir, DurabilityMode::Strict);
            writer.append(&make_record(2)).unwrap();
            writer.flush().unwrap();
        }

        // Should have appended to existing or created new
        let writer = make_writer(&wal_dir, DurabilityMode::Strict);
        assert!(writer.current_segment() >= 1);
    }

    #[test]
    fn test_batched_mode_sync_threshold() {
        let dir = tempdir().unwrap();
        let wal_dir = dir.path().join("wal");

        let config = WalConfig::new()
            .with_segment_size(1024 * 1024)
            .with_buffered_sync_bytes(100); // Small threshold

        let mut writer = WalWriter::new(
            wal_dir.clone(),
            [1u8; 16],
            DurabilityMode::Batched {
                interval_ms: 10000,
                batch_size: 100,
            },
            config,
            Box::new(IdentityCodec),
        )
        .unwrap();

        // Write records to trigger sync
        for i in 0..20 {
            writer.append(&make_record(i)).unwrap();
        }

        // Segment should have data
        assert!(writer.current_segment_size() > SEGMENT_HEADER_SIZE as u64);
    }
}
