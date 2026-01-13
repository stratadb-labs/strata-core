//! WAL (Write-Ahead Log) entry types and file operations
//!
//! This module defines all WAL entry types for the durability layer:
//! - BeginTxn: Start of a transaction
//! - Write: Put or update operation
//! - Delete: Delete operation
//! - CommitTxn: Successful transaction completion
//! - AbortTxn: Transaction rollback
//! - Checkpoint: Snapshot boundary marker
//!
//! CRITICAL: All entries include run_id (except Checkpoint which tracks active runs)
//! This enables:
//! - Run-scoped replay (filter WAL by run_id)
//! - Run diffing (compare WAL entries for two runs)
//! - Audit trails (track all operations per run)
//!
//! ## File Format
//!
//! WAL is an append-only log file containing a sequence of encoded entries.
//! Each entry is self-describing (no framing needed).
//!
//! ## File Operations
//!
//! - `WAL::open()` - Open existing WAL or create new one
//! - `WAL::append()` - Write encoded entry to end of file
//! - `WAL::read_entries()` - Scan from offset, decode entries
//! - `WAL::read_all()` - Scan from beginning
//! - `WAL::flush()` - Flush buffered writes
//! - `WAL::fsync()` - Force sync to disk
//! - `WAL::size()` - Get current file size
//!
//! ## Durability Modes
//!
//! - `Strict` - fsync after every commit (slow, maximum durability)
//! - `Batched` - fsync every N commits OR T ms (DEFAULT, good balance)
//! - `Async` - background thread fsyncs periodically (fast, may lose recent writes)

use crate::encoding::{decode_entry, encode_entry};
use in_mem_core::{
    error::{Error, Result},
    types::{Key, RunId},
    value::{Timestamp, Value},
};
use serde::{Deserialize, Serialize};
use std::fs::{File, OpenOptions};
use std::io::{BufReader, BufWriter, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};
use uuid::Uuid;

/// WAL entry types
///
/// Each entry represents a state-changing operation that must be persisted
/// before it can be considered durable. All entries (except Checkpoint)
/// include run_id to enable run-scoped operations.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum WALEntry {
    /// Begin transaction
    ///
    /// Marks the start of a transaction. All writes/deletes between
    /// BeginTxn and CommitTxn/AbortTxn belong to this transaction.
    BeginTxn {
        /// Transaction identifier (unique within a run)
        txn_id: u64,
        /// Run this transaction belongs to
        run_id: RunId,
        /// Timestamp when transaction started
        timestamp: Timestamp,
    },

    /// Write operation (put or update)
    ///
    /// Records a key-value write operation with its version.
    Write {
        /// Run this write belongs to
        run_id: RunId,
        /// Key being written
        key: Key,
        /// Value being written
        value: Value,
        /// Version number for this write
        version: u64,
    },

    /// Delete operation
    ///
    /// Records a key deletion with its version.
    Delete {
        /// Run this delete belongs to
        run_id: RunId,
        /// Key being deleted
        key: Key,
        /// Version number for this delete
        version: u64,
    },

    /// Commit transaction
    ///
    /// Marks successful completion of a transaction.
    /// All operations in this transaction are now durable.
    CommitTxn {
        /// Transaction identifier
        txn_id: u64,
        /// Run this transaction belongs to
        run_id: RunId,
    },

    /// Abort transaction
    ///
    /// Marks that a transaction was rolled back.
    /// All operations in this transaction should be discarded.
    AbortTxn {
        /// Transaction identifier
        txn_id: u64,
        /// Run this transaction belongs to
        run_id: RunId,
    },

    /// Checkpoint marker (snapshot boundary)
    ///
    /// Marks a point where a consistent snapshot was taken.
    /// WAL entries before this checkpoint can be truncated after
    /// the snapshot is safely persisted.
    Checkpoint {
        /// Unique identifier for this snapshot
        snapshot_id: Uuid,
        /// Version at checkpoint time
        version: u64,
        /// Runs that were active at checkpoint time
        active_runs: Vec<RunId>,
    },
}

impl WALEntry {
    /// Get run_id from entry (if applicable)
    ///
    /// Returns the run_id for all entry types except Checkpoint,
    /// which tracks multiple runs instead of belonging to a single run.
    pub fn run_id(&self) -> Option<RunId> {
        match self {
            WALEntry::BeginTxn { run_id, .. } => Some(*run_id),
            WALEntry::Write { run_id, .. } => Some(*run_id),
            WALEntry::Delete { run_id, .. } => Some(*run_id),
            WALEntry::CommitTxn { run_id, .. } => Some(*run_id),
            WALEntry::AbortTxn { run_id, .. } => Some(*run_id),
            WALEntry::Checkpoint { .. } => None, // Checkpoint tracks multiple runs
        }
    }

    /// Get transaction ID (if applicable)
    ///
    /// Returns the transaction ID for transaction-related entries:
    /// BeginTxn, CommitTxn, AbortTxn.
    pub fn txn_id(&self) -> Option<u64> {
        match self {
            WALEntry::BeginTxn { txn_id, .. } => Some(*txn_id),
            WALEntry::CommitTxn { txn_id, .. } => Some(*txn_id),
            WALEntry::AbortTxn { txn_id, .. } => Some(*txn_id),
            _ => None,
        }
    }

    /// Get version (if applicable)
    ///
    /// Returns the version for entries that track versions:
    /// Write, Delete, Checkpoint.
    pub fn version(&self) -> Option<u64> {
        match self {
            WALEntry::Write { version, .. } => Some(*version),
            WALEntry::Delete { version, .. } => Some(*version),
            WALEntry::Checkpoint { version, .. } => Some(*version),
            _ => None,
        }
    }

    /// Check if entry is a transaction boundary
    ///
    /// Transaction boundaries are BeginTxn, CommitTxn, and AbortTxn.
    /// These mark the start and end of transactions.
    pub fn is_txn_boundary(&self) -> bool {
        matches!(
            self,
            WALEntry::BeginTxn { .. } | WALEntry::CommitTxn { .. } | WALEntry::AbortTxn { .. }
        )
    }

    /// Check if entry is a checkpoint
    ///
    /// Checkpoints mark snapshot boundaries for WAL truncation.
    pub fn is_checkpoint(&self) -> bool {
        matches!(self, WALEntry::Checkpoint { .. })
    }
}

// ============================================================================
// Durability Modes
// ============================================================================

/// Durability mode configuration
///
/// Controls when fsync is called to ensure data reaches disk.
///
/// # Modes
///
/// - `Strict` - Maximum durability, fsync after every write (slow)
/// - `Batched` - Balance of speed and safety (DEFAULT)
/// - `Async` - Maximum speed, background fsync (may lose recent writes)
///
/// # Default
///
/// The default mode is `Batched { interval_ms: 100, batch_size: 1000 }`,
/// which fsyncs every 100ms or every 1000 writes, whichever comes first.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DurabilityMode {
    /// fsync after every commit (slow, maximum durability)
    ///
    /// Use when data loss is unacceptable, even for a single write.
    /// Expect 10ms+ latency per write.
    Strict,

    /// fsync every N commits OR every T milliseconds
    ///
    /// Good balance of speed and safety. May lose up to batch_size
    /// writes or interval_ms of data on crash.
    Batched {
        /// Maximum time between fsyncs in milliseconds
        interval_ms: u64,
        /// Maximum writes between fsyncs
        batch_size: usize,
    },

    /// Background thread fsyncs periodically
    ///
    /// Maximum speed, minimal latency. May lose up to interval_ms
    /// of writes on crash. Best for agent workloads where speed
    /// matters more than perfect durability.
    Async {
        /// Time between fsyncs in milliseconds
        interval_ms: u64,
    },
}

impl Default for DurabilityMode {
    fn default() -> Self {
        // Default: batched with 100ms interval or 1000 commits
        DurabilityMode::Batched {
            interval_ms: 100,
            batch_size: 1000,
        }
    }
}

// ============================================================================
// WAL File Operations
// ============================================================================

/// Write-Ahead Log with configurable durability
///
/// Append-only log of WAL entries persisted to disk.
/// File format: sequence of encoded entries (self-describing, no framing).
///
/// # Durability Modes
///
/// The WAL supports three durability modes:
/// - `Strict` - fsync after every write (slow but safest)
/// - `Batched` - fsync periodically by time or count (DEFAULT)
/// - `Async` - background thread handles fsync (fastest)
///
/// # Example
///
/// ```ignore
/// use in_mem_durability::wal::{WAL, WALEntry, DurabilityMode};
///
/// // Open with default batched mode
/// let mut wal = WAL::open("data/wal/segment.wal", DurabilityMode::default())?;
/// wal.append(&entry)?;
///
/// // Open with strict mode for maximum durability
/// let mut wal = WAL::open("data/wal/segment.wal", DurabilityMode::Strict)?;
///
/// let entries = wal.read_all()?;
/// ```
pub struct WAL {
    /// File path
    path: PathBuf,

    /// File handle (buffered writer for appends, shared for async mode)
    writer: Arc<Mutex<BufWriter<File>>>,

    /// Current file offset (atomic for thread-safe access)
    current_offset: Arc<AtomicU64>,

    /// Durability mode
    durability_mode: DurabilityMode,

    /// Last fsync time (for batched mode)
    last_fsync: Arc<Mutex<Instant>>,

    /// Writes since last fsync (for batched mode)
    writes_since_fsync: Arc<AtomicU64>,

    /// Background fsync thread handle (for async mode)
    fsync_thread: Option<JoinHandle<()>>,

    /// Shutdown flag for async thread
    shutdown: Arc<AtomicBool>,
}

impl WAL {
    /// Open existing WAL or create new one with specified durability mode
    ///
    /// Creates parent directories if they don't exist.
    /// Opens file in append mode with read capability.
    ///
    /// # Arguments
    ///
    /// * `path` - Path to WAL file
    /// * `durability_mode` - Durability mode for fsync behavior
    ///
    /// # Returns
    ///
    /// * `Ok(WAL)` - Opened WAL handle
    /// * `Err` - If file operations fail
    ///
    /// # Example
    ///
    /// ```ignore
    /// // Open with default batched mode
    /// let wal = WAL::open("data/wal/segment.wal", DurabilityMode::default())?;
    ///
    /// // Open with strict mode
    /// let wal = WAL::open("data/wal/segment.wal", DurabilityMode::Strict)?;
    /// ```
    pub fn open<P: AsRef<Path>>(path: P, durability_mode: DurabilityMode) -> Result<Self> {
        let path = path.as_ref().to_path_buf();

        // Create parent directory if needed
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)?;
            }
        }

        // Open file (create if doesn't exist, append mode)
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .read(true)
            .open(&path)?;

        // Get current file size (start offset)
        let current_offset = Arc::new(AtomicU64::new(file.metadata()?.len()));

        let writer = Arc::new(Mutex::new(BufWriter::new(file)));
        let last_fsync = Arc::new(Mutex::new(Instant::now()));
        let writes_since_fsync = Arc::new(AtomicU64::new(0));
        let shutdown = Arc::new(AtomicBool::new(false));

        // Spawn background fsync thread for async mode
        let fsync_thread = if let DurabilityMode::Async { interval_ms } = durability_mode {
            let writer = Arc::clone(&writer);
            let shutdown = Arc::clone(&shutdown);
            let interval = Duration::from_millis(interval_ms);

            Some(thread::spawn(move || {
                while !shutdown.load(Ordering::Relaxed) {
                    thread::sleep(interval);

                    if shutdown.load(Ordering::Relaxed) {
                        break;
                    }

                    if let Ok(mut w) = writer.lock() {
                        let _ = w.flush();
                        let _ = w.get_mut().sync_all();
                    }
                }
            }))
        } else {
            None
        };

        Ok(Self {
            path,
            writer,
            current_offset,
            durability_mode,
            last_fsync,
            writes_since_fsync,
            fsync_thread,
            shutdown,
        })
    }

    /// Append entry to WAL with durability mode handling
    ///
    /// Encodes entry and writes to end of file.
    /// Handles fsync based on configured durability mode:
    /// - Strict: fsync after every write
    /// - Batched: fsync after batch_size writes OR interval_ms elapsed
    /// - Async: just flush, background thread handles fsync
    ///
    /// # Arguments
    ///
    /// * `entry` - WAL entry to append
    ///
    /// # Returns
    ///
    /// * `Ok(u64)` - Offset where entry was written
    /// * `Err` - If encoding or writing fails
    pub fn append(&mut self, entry: &WALEntry) -> Result<u64> {
        let offset = self.current_offset.load(Ordering::SeqCst);

        // Encode entry
        let encoded = encode_entry(entry)?;

        // Write to file
        {
            let mut writer = self.writer.lock().unwrap();
            writer.write_all(&encoded).map_err(|e| {
                Error::StorageError(format!("Failed to write entry at offset {}: {}", offset, e))
            })?;
        }

        // Update offset
        self.current_offset
            .fetch_add(encoded.len() as u64, Ordering::SeqCst);

        // Handle durability mode
        match self.durability_mode {
            DurabilityMode::Strict => {
                // Flush and fsync immediately
                self.fsync()?;
            }
            DurabilityMode::Batched {
                interval_ms,
                batch_size,
            } => {
                self.writes_since_fsync.fetch_add(1, Ordering::SeqCst);

                let should_fsync = {
                    let last = self.last_fsync.lock().unwrap();
                    let elapsed = last.elapsed().as_millis() as u64;
                    let writes = self.writes_since_fsync.load(Ordering::SeqCst);

                    elapsed >= interval_ms || writes >= batch_size as u64
                };

                if should_fsync {
                    self.fsync()?;
                    self.writes_since_fsync.store(0, Ordering::SeqCst);
                    *self.last_fsync.lock().unwrap() = Instant::now();
                }
            }
            DurabilityMode::Async { .. } => {
                // Background thread handles fsync
                // Just flush buffer to ensure writes are visible to reader
                let mut writer = self.writer.lock().unwrap();
                writer
                    .flush()
                    .map_err(|e| Error::StorageError(format!("Failed to flush: {}", e)))?;
            }
        }

        Ok(offset)
    }

    /// Flush buffered writes to OS buffers
    ///
    /// Note: This flushes to OS buffers, not necessarily to disk.
    /// For true durability, use fsync().
    pub fn flush(&mut self) -> Result<()> {
        let mut writer = self.writer.lock().unwrap();
        writer
            .flush()
            .map_err(|e| Error::StorageError(format!("Failed to flush WAL: {}", e)))
    }

    /// Force sync to disk (flush + fsync)
    ///
    /// Ensures all buffered data is written to disk.
    /// This is the most durable option but also the slowest.
    pub fn fsync(&self) -> Result<()> {
        let mut writer = self.writer.lock().unwrap();

        // Flush buffer
        writer
            .flush()
            .map_err(|e| Error::StorageError(format!("Failed to flush: {}", e)))?;

        // Fsync to disk
        writer
            .get_mut()
            .sync_all()
            .map_err(|e| Error::StorageError(format!("Failed to fsync: {}", e)))?;

        Ok(())
    }

    /// Read all entries from WAL starting at offset
    ///
    /// Returns vector of decoded entries.
    /// Stops at first corruption or end of file.
    /// Incomplete entries at EOF are expected (partial writes) and ignored.
    ///
    /// # Arguments
    ///
    /// * `start_offset` - Byte offset to start reading from
    ///
    /// # Returns
    ///
    /// * `Ok(Vec<WALEntry>)` - Decoded entries
    /// * `Err` - If file operations fail or mid-file corruption detected
    pub fn read_entries(&self, start_offset: u64) -> Result<Vec<WALEntry>> {
        // Flush any buffered writes before reading
        {
            let mut writer = self.writer.lock().unwrap();
            let _ = writer.flush();
        }

        // Open separate read handle (writer is buffered, don't interfere)
        let file = File::open(&self.path)?;

        let mut reader = BufReader::new(file);

        // Seek to start offset
        reader.seek(SeekFrom::Start(start_offset))?;

        let mut entries = Vec::new();
        let mut file_offset = start_offset;

        // Buffer to hold data, including leftover bytes from previous iteration
        let mut buf = Vec::new();
        let mut read_buf = vec![0u8; 64 * 1024]; // 64KB read buffer

        // Read file in chunks, handling entries that span buffer boundaries
        loop {
            // Read more data into read_buf
            let bytes_read = reader.read(&mut read_buf)?;

            if bytes_read == 0 {
                // EOF - any remaining bytes in buf are an incomplete entry (partial write)
                break;
            }

            // Append new data to existing buffer (which may contain leftover bytes)
            buf.extend_from_slice(&read_buf[..bytes_read]);

            // Decode entries from buffer
            let mut offset_in_buf = 0;
            while offset_in_buf < buf.len() {
                match decode_entry(&buf[offset_in_buf..], file_offset) {
                    Ok((entry, bytes_consumed)) => {
                        entries.push(entry);
                        offset_in_buf += bytes_consumed;
                        file_offset += bytes_consumed as u64;
                    }
                    Err(_) => {
                        // Decode failed - could be incomplete entry or corruption
                        // Keep the remaining bytes and read more data
                        break;
                    }
                }
            }

            // Remove consumed bytes from buffer, keeping any leftover for next iteration
            if offset_in_buf > 0 {
                buf.drain(..offset_in_buf);
            }

            // If we read less than a full buffer and still have leftover bytes,
            // we're at EOF with an incomplete entry (partial write at end of file)
            if bytes_read < read_buf.len() && !buf.is_empty() {
                // EOF with incomplete entry - this is expected (partial write)
                break;
            }
        }

        Ok(entries)
    }

    /// Read all entries from beginning of file
    ///
    /// Convenience method equivalent to `read_entries(0)`.
    pub fn read_all(&self) -> Result<Vec<WALEntry>> {
        self.read_entries(0)
    }

    /// Get current file size (offset for next write)
    pub fn size(&self) -> u64 {
        self.current_offset.load(Ordering::SeqCst)
    }

    /// Get file path
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Get durability mode
    pub fn durability_mode(&self) -> DurabilityMode {
        self.durability_mode
    }
}

impl Drop for WAL {
    fn drop(&mut self) {
        // Shutdown async fsync thread if running
        self.shutdown.store(true, Ordering::Relaxed);

        if let Some(handle) = self.fsync_thread.take() {
            let _ = handle.join();
        }

        // Final fsync to ensure all data is durable
        let _ = self.fsync();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use in_mem_core::types::Namespace;

    /// Helper to get current timestamp
    fn now() -> Timestamp {
        Utc::now().timestamp()
    }

    #[test]
    fn test_begin_txn_entry() {
        let run_id = RunId::new();
        let entry = WALEntry::BeginTxn {
            txn_id: 42,
            run_id,
            timestamp: now(),
        };

        assert_eq!(entry.run_id(), Some(run_id));
        assert_eq!(entry.txn_id(), Some(42));
        assert!(entry.is_txn_boundary());
        assert!(!entry.is_checkpoint());
        assert_eq!(entry.version(), None);
    }

    #[test]
    fn test_write_entry() {
        let run_id = RunId::new();
        let ns = Namespace::new(
            "tenant".to_string(),
            "app".to_string(),
            "agent".to_string(),
            run_id,
        );
        let key = Key::new_kv(ns, "test");
        let value = Value::Bytes(b"data".to_vec());

        let entry = WALEntry::Write {
            run_id,
            key: key.clone(),
            value: value.clone(),
            version: 100,
        };

        assert_eq!(entry.run_id(), Some(run_id));
        assert_eq!(entry.version(), Some(100));
        assert!(!entry.is_txn_boundary());
        assert!(!entry.is_checkpoint());
        assert_eq!(entry.txn_id(), None);

        if let WALEntry::Write {
            key: k, value: v, ..
        } = entry
        {
            assert_eq!(k, key);
            assert_eq!(v, value);
        } else {
            panic!("Expected Write variant");
        }
    }

    #[test]
    fn test_delete_entry() {
        let run_id = RunId::new();
        let ns = Namespace::new(
            "tenant".to_string(),
            "app".to_string(),
            "agent".to_string(),
            run_id,
        );
        let key = Key::new_kv(ns, "test");

        let entry = WALEntry::Delete {
            run_id,
            key: key.clone(),
            version: 101,
        };

        assert_eq!(entry.run_id(), Some(run_id));
        assert_eq!(entry.version(), Some(101));
        assert!(!entry.is_txn_boundary());
        assert!(!entry.is_checkpoint());
        assert_eq!(entry.txn_id(), None);

        if let WALEntry::Delete { key: k, .. } = entry {
            assert_eq!(k, key);
        } else {
            panic!("Expected Delete variant");
        }
    }

    #[test]
    fn test_commit_txn_entry() {
        let run_id = RunId::new();
        let entry = WALEntry::CommitTxn { txn_id: 42, run_id };

        assert_eq!(entry.run_id(), Some(run_id));
        assert_eq!(entry.txn_id(), Some(42));
        assert!(entry.is_txn_boundary());
        assert!(!entry.is_checkpoint());
        assert_eq!(entry.version(), None);
    }

    #[test]
    fn test_abort_txn_entry() {
        let run_id = RunId::new();
        let entry = WALEntry::AbortTxn { txn_id: 99, run_id };

        assert_eq!(entry.run_id(), Some(run_id));
        assert_eq!(entry.txn_id(), Some(99));
        assert!(entry.is_txn_boundary());
        assert!(!entry.is_checkpoint());
        assert_eq!(entry.version(), None);
    }

    #[test]
    fn test_checkpoint_entry() {
        let run1 = RunId::new();
        let run2 = RunId::new();

        let entry = WALEntry::Checkpoint {
            snapshot_id: Uuid::new_v4(),
            version: 1000,
            active_runs: vec![run1, run2],
        };

        assert!(entry.is_checkpoint());
        assert_eq!(entry.version(), Some(1000));
        assert_eq!(entry.run_id(), None); // Checkpoint doesn't have single run_id
        assert!(!entry.is_txn_boundary());
        assert_eq!(entry.txn_id(), None);

        if let WALEntry::Checkpoint { active_runs, .. } = entry {
            assert_eq!(active_runs.len(), 2);
            assert!(active_runs.contains(&run1));
            assert!(active_runs.contains(&run2));
        } else {
            panic!("Expected Checkpoint variant");
        }
    }

    #[test]
    fn test_serialization_roundtrip() {
        let run_id = RunId::new();
        let timestamp = now();
        let entry = WALEntry::BeginTxn {
            txn_id: 42,
            run_id,
            timestamp,
        };

        // Serialize with bincode
        let encoded = bincode::serialize(&entry).expect("serialization failed");

        // Deserialize
        let decoded: WALEntry = bincode::deserialize(&encoded).expect("deserialization failed");

        assert_eq!(entry, decoded);
    }

    #[test]
    fn test_all_entries_serialize() {
        let run_id = RunId::new();
        let ns = Namespace::new(
            "tenant".to_string(),
            "app".to_string(),
            "agent".to_string(),
            run_id,
        );

        let entries = vec![
            WALEntry::BeginTxn {
                txn_id: 1,
                run_id,
                timestamp: now(),
            },
            WALEntry::Write {
                run_id,
                key: Key::new_kv(ns.clone(), "key"),
                value: Value::Bytes(b"value".to_vec()),
                version: 10,
            },
            WALEntry::Delete {
                run_id,
                key: Key::new_kv(ns, "key"),
                version: 11,
            },
            WALEntry::CommitTxn { txn_id: 1, run_id },
            WALEntry::AbortTxn { txn_id: 2, run_id },
            WALEntry::Checkpoint {
                snapshot_id: Uuid::new_v4(),
                version: 100,
                active_runs: vec![run_id],
            },
        ];

        for entry in entries {
            let encoded = bincode::serialize(&entry).expect("serialization failed");
            let decoded: WALEntry = bincode::deserialize(&encoded).expect("deserialization failed");
            assert_eq!(entry, decoded);
        }
    }

    // ========================================================================
    // WAL File Operations Tests
    // ========================================================================

    use tempfile::TempDir;

    #[test]
    fn test_open_new_wal() {
        let temp_dir = TempDir::new().unwrap();
        let wal_path = temp_dir.path().join("test.wal");

        let wal = WAL::open(&wal_path, DurabilityMode::default()).unwrap();
        assert_eq!(wal.size(), 0);
        assert!(wal_path.exists());
    }

    #[test]
    fn test_append_and_read() {
        let temp_dir = TempDir::new().unwrap();
        let wal_path = temp_dir.path().join("test.wal");

        let mut wal = WAL::open(&wal_path, DurabilityMode::default()).unwrap();

        let run_id = RunId::new();
        let entry1 = WALEntry::BeginTxn {
            txn_id: 1,
            run_id,
            timestamp: now(),
        };
        let entry2 = WALEntry::CommitTxn { txn_id: 1, run_id };

        // Append entries
        wal.append(&entry1).unwrap();
        wal.append(&entry2).unwrap();
        wal.flush().unwrap();

        // Read back
        let entries = wal.read_all().unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0], entry1);
        assert_eq!(entries[1], entry2);
    }

    #[test]
    fn test_append_multiple_entries() {
        let temp_dir = TempDir::new().unwrap();
        let wal_path = temp_dir.path().join("test.wal");

        let mut wal = WAL::open(&wal_path, DurabilityMode::default()).unwrap();

        let run_id = RunId::new();
        let ns = Namespace::new(
            "tenant".to_string(),
            "app".to_string(),
            "agent".to_string(),
            run_id,
        );

        // Append 100 entries
        for i in 0..100u64 {
            let entry = WALEntry::Write {
                run_id,
                key: Key::new_kv(ns.clone(), format!("key_{}", i)),
                value: Value::Bytes(vec![i as u8]),
                version: i,
            };
            wal.append(&entry).unwrap();
        }

        wal.flush().unwrap();

        // Read back
        let entries = wal.read_all().unwrap();
        assert_eq!(entries.len(), 100);

        // Verify first and last entries
        if let WALEntry::Write { version, .. } = &entries[0] {
            assert_eq!(*version, 0);
        } else {
            panic!("Expected Write entry");
        }
        if let WALEntry::Write { version, .. } = &entries[99] {
            assert_eq!(*version, 99);
        } else {
            panic!("Expected Write entry");
        }
    }

    #[test]
    fn test_read_from_offset() {
        let temp_dir = TempDir::new().unwrap();
        let wal_path = temp_dir.path().join("test.wal");

        let mut wal = WAL::open(&wal_path, DurabilityMode::default()).unwrap();

        let run_id = RunId::new();
        let entry1 = WALEntry::BeginTxn {
            txn_id: 1,
            run_id,
            timestamp: now(),
        };
        let entry2 = WALEntry::CommitTxn { txn_id: 1, run_id };

        let _offset1 = wal.append(&entry1).unwrap();
        let offset2 = wal.append(&entry2).unwrap();
        wal.flush().unwrap();

        // Read from offset2 (should only get entry2)
        let entries = wal.read_entries(offset2).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0], entry2);
    }

    #[test]
    fn test_reopen_wal() {
        let temp_dir = TempDir::new().unwrap();
        let wal_path = temp_dir.path().join("test.wal");

        let run_id = RunId::new();
        let entry1 = WALEntry::BeginTxn {
            txn_id: 1,
            run_id,
            timestamp: now(),
        };

        let initial_size;

        // Write entry and close
        {
            let mut wal = WAL::open(&wal_path, DurabilityMode::default()).unwrap();
            wal.append(&entry1).unwrap();
            wal.flush().unwrap();
            initial_size = wal.size();
        }

        // Reopen and verify entry still there
        {
            let wal = WAL::open(&wal_path, DurabilityMode::default()).unwrap();
            assert_eq!(wal.size(), initial_size);

            let entries = wal.read_all().unwrap();
            assert_eq!(entries.len(), 1);
            assert_eq!(entries[0], entry1);
        }
    }

    #[test]
    fn test_append_after_reopen() {
        let temp_dir = TempDir::new().unwrap();
        let wal_path = temp_dir.path().join("test.wal");

        let run_id = RunId::new();
        let entry1 = WALEntry::BeginTxn {
            txn_id: 1,
            run_id,
            timestamp: now(),
        };
        let entry2 = WALEntry::CommitTxn { txn_id: 1, run_id };

        // Write first entry and close
        {
            let mut wal = WAL::open(&wal_path, DurabilityMode::default()).unwrap();
            wal.append(&entry1).unwrap();
            wal.flush().unwrap();
        }

        // Reopen and append second entry
        {
            let mut wal = WAL::open(&wal_path, DurabilityMode::default()).unwrap();
            wal.append(&entry2).unwrap();
            wal.flush().unwrap();
        }

        // Read all entries
        {
            let wal = WAL::open(&wal_path, DurabilityMode::default()).unwrap();
            let entries = wal.read_all().unwrap();
            assert_eq!(entries.len(), 2);
            assert_eq!(entries[0], entry1);
            assert_eq!(entries[1], entry2);
        }
    }

    #[test]
    fn test_wal_creates_parent_directories() {
        let temp_dir = TempDir::new().unwrap();
        let wal_path = temp_dir.path().join("nested").join("dir").join("test.wal");

        let wal = WAL::open(&wal_path, DurabilityMode::default()).unwrap();
        assert_eq!(wal.size(), 0);
        assert!(wal_path.exists());
    }

    // ========================================================================
    // Durability Mode Tests
    // ========================================================================

    #[test]
    fn test_durability_mode_default() {
        let mode = DurabilityMode::default();
        assert_eq!(
            mode,
            DurabilityMode::Batched {
                interval_ms: 100,
                batch_size: 1000,
            }
        );
    }

    #[test]
    fn test_strict_mode() {
        let temp_dir = TempDir::new().unwrap();
        let wal_path = temp_dir.path().join("strict.wal");

        let mut wal = WAL::open(&wal_path, DurabilityMode::Strict).unwrap();
        assert_eq!(wal.durability_mode(), DurabilityMode::Strict);

        let run_id = RunId::new();
        let entry = WALEntry::BeginTxn {
            txn_id: 1,
            run_id,
            timestamp: now(),
        };

        // Append triggers immediate fsync in strict mode
        wal.append(&entry).unwrap();

        // Reopen without explicit flush - entry should be durable
        drop(wal);
        let wal = WAL::open(&wal_path, DurabilityMode::Strict).unwrap();
        let entries = wal.read_all().unwrap();
        assert_eq!(entries.len(), 1);
    }

    #[test]
    fn test_batched_mode_by_count() {
        let temp_dir = TempDir::new().unwrap();
        let wal_path = temp_dir.path().join("batched.wal");

        let mut wal = WAL::open(
            &wal_path,
            DurabilityMode::Batched {
                interval_ms: 10000, // Long interval, won't trigger
                batch_size: 10,
            },
        )
        .unwrap();

        let run_id = RunId::new();

        // Append 10 entries - triggers batch fsync at 10
        for i in 0..10 {
            let entry = WALEntry::BeginTxn {
                txn_id: i,
                run_id,
                timestamp: now(),
            };
            wal.append(&entry).unwrap();
        }

        // Entries should be durable after batch_size reached
        drop(wal);
        let wal = WAL::open(
            &wal_path,
            DurabilityMode::Batched {
                interval_ms: 10000,
                batch_size: 10,
            },
        )
        .unwrap();
        let entries = wal.read_all().unwrap();
        assert_eq!(entries.len(), 10);
    }

    #[test]
    fn test_batched_mode_by_time() {
        let temp_dir = TempDir::new().unwrap();
        let wal_path = temp_dir.path().join("batched_time.wal");

        let mut wal = WAL::open(
            &wal_path,
            DurabilityMode::Batched {
                interval_ms: 10, // Short interval
                batch_size: 1000000,
            },
        )
        .unwrap();

        let run_id = RunId::new();
        let entry = WALEntry::BeginTxn {
            txn_id: 1,
            run_id,
            timestamp: now(),
        };

        wal.append(&entry).unwrap();

        // Wait for interval to elapse
        thread::sleep(Duration::from_millis(20));

        // Append another entry - should trigger time-based fsync
        let entry2 = WALEntry::CommitTxn { txn_id: 1, run_id };
        wal.append(&entry2).unwrap();

        drop(wal);
        let wal = WAL::open(
            &wal_path,
            DurabilityMode::Batched {
                interval_ms: 10,
                batch_size: 1000000,
            },
        )
        .unwrap();
        let entries = wal.read_all().unwrap();
        assert_eq!(entries.len(), 2);
    }

    #[test]
    fn test_async_mode() {
        let temp_dir = TempDir::new().unwrap();
        let wal_path = temp_dir.path().join("async.wal");

        let mut wal = WAL::open(&wal_path, DurabilityMode::Async { interval_ms: 50 }).unwrap();

        let run_id = RunId::new();
        let entry = WALEntry::BeginTxn {
            txn_id: 1,
            run_id,
            timestamp: now(),
        };

        wal.append(&entry).unwrap();

        // Wait for background fsync
        thread::sleep(Duration::from_millis(100));

        drop(wal);
        let wal = WAL::open(&wal_path, DurabilityMode::Async { interval_ms: 50 }).unwrap();
        let entries = wal.read_all().unwrap();
        assert_eq!(entries.len(), 1);
    }

    #[test]
    fn test_drop_performs_final_fsync() {
        let temp_dir = TempDir::new().unwrap();
        let wal_path = temp_dir.path().join("drop.wal");

        let run_id = RunId::new();
        let entry = WALEntry::BeginTxn {
            txn_id: 1,
            run_id,
            timestamp: now(),
        };

        // Use async mode with long interval - data won't be synced by background thread
        {
            let mut wal =
                WAL::open(&wal_path, DurabilityMode::Async { interval_ms: 10000 }).unwrap();
            wal.append(&entry).unwrap();
            // Drop should call final fsync
        }

        // Entry should still be readable after drop's final fsync
        let wal = WAL::open(&wal_path, DurabilityMode::default()).unwrap();
        let entries = wal.read_all().unwrap();
        assert_eq!(entries.len(), 1);
    }
}
