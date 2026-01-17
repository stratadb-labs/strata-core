//! M7 WAL Writer with Transaction Framing
//!
//! This module implements the M7 WAL writer with transaction support:
//!
//! ## Transaction Lifecycle
//!
//! 1. `begin_transaction()` - Creates a unique TxId
//! 2. `write_tx_entry()` - Writes entries with the TxId
//! 3. `commit_transaction()` - Writes commit marker (durability point)
//! 4. Or `abort_transaction()` - Writes abort marker
//!
//! ## Commit Marker Protocol
//!
//! - Entries without a commit marker are **invisible** after recovery
//! - The commit marker is the **durability point** for the transaction
//! - After commit marker is synced to disk, transaction is durable
//! - Abort markers are optional (uncommitted entries are discarded anyway)
//!
//! ## Durability Modes
//!
//! - `Strict`: fsync after every commit marker
//! - `Batched`: fsync based on count/time (may lose recent commits)
//! - `Async`: Background thread handles fsync
//! - `InMemory`: No persistence

use crate::m7_wal_types::{TxId, WalEntry, WalEntryError};
use crate::wal::DurabilityMode;
use crate::wal_entry_types::WalEntryType;
use std::fs::{File, OpenOptions};
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};
use tracing::{debug, info, trace};
use uuid::Uuid;

/// M7 WAL Writer with transaction framing support
///
/// This writer implements the M7 transaction protocol:
/// - Every data entry includes a TxId
/// - Transactions must have a commit marker to be visible
/// - Commit markers trigger fsync in Strict mode
///
/// # Example
///
/// ```ignore
/// use in_mem_durability::m7_wal_writer::WalWriter;
/// use in_mem_durability::WalEntryType;
///
/// let mut writer = WalWriter::open("test.wal", DurabilityMode::Strict)?;
///
/// // Write a complete transaction
/// let tx_id = writer.write_transaction(vec![
///     (WalEntryType::KvPut, b"key1=value1".to_vec()),
///     (WalEntryType::KvPut, b"key2=value2".to_vec()),
/// ])?;
///
/// // Or write entries manually
/// let tx_id = writer.begin_transaction();
/// writer.write_tx_entry(tx_id, WalEntryType::KvPut, b"key=value".to_vec())?;
/// writer.commit_transaction(tx_id)?;
/// ```
pub struct WalWriter {
    /// File path
    path: PathBuf,

    /// File handle (buffered writer)
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

impl WalWriter {
    /// Open or create a WAL file for writing
    ///
    /// Creates parent directories if they don't exist.
    /// Opens file in append mode.
    ///
    /// # Arguments
    ///
    /// * `path` - Path to WAL file
    /// * `durability_mode` - Durability mode for fsync behavior
    pub fn open<P: AsRef<Path>>(
        path: P,
        durability_mode: DurabilityMode,
    ) -> Result<Self, WalEntryError> {
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
            let path_for_log = path.clone();

            Some(thread::spawn(move || {
                debug!(path = %path_for_log.display(), "Starting async fsync thread");
                while !shutdown.load(Ordering::Relaxed) {
                    thread::sleep(interval);

                    if shutdown.load(Ordering::Relaxed) {
                        break;
                    }

                    if let Ok(mut w) = writer.lock() {
                        let _ = w.flush();
                        let _ = w.get_mut().sync_all();
                        trace!(path = %path_for_log.display(), "Async fsync completed");
                    }
                }
                debug!(path = %path_for_log.display(), "Async fsync thread exiting");
            }))
        } else {
            None
        };

        info!(path = %path.display(), ?durability_mode, "Opened WAL for writing");

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

    /// Begin a new transaction
    ///
    /// Returns a unique TxId to use for subsequent writes.
    /// The transaction is not visible until `commit_transaction()` is called.
    pub fn begin_transaction(&self) -> TxId {
        let tx_id = TxId::new();
        trace!(tx_id = %tx_id, "Beginning transaction");
        tx_id
    }

    /// Write an entry as part of a transaction
    ///
    /// The entry will include the TxId and will only be visible
    /// after the transaction is committed.
    ///
    /// # Arguments
    ///
    /// * `tx_id` - Transaction ID from `begin_transaction()`
    /// * `entry_type` - Type of WAL entry
    /// * `payload` - Entry-specific data
    pub fn write_tx_entry(
        &mut self,
        tx_id: TxId,
        entry_type: WalEntryType,
        payload: Vec<u8>,
    ) -> Result<u64, WalEntryError> {
        let entry = WalEntry::new(entry_type, tx_id, payload);
        self.write_entry(&entry)
    }

    /// Commit a transaction
    ///
    /// Writes a commit marker and syncs to disk (in Strict mode).
    /// After this call returns successfully, the transaction is durable.
    ///
    /// # Arguments
    ///
    /// * `tx_id` - Transaction ID to commit
    pub fn commit_transaction(&mut self, tx_id: TxId) -> Result<u64, WalEntryError> {
        trace!(tx_id = %tx_id, "Committing transaction");

        let entry = WalEntry::commit_marker(tx_id);
        let offset = self.write_entry(&entry)?;

        // Always sync on commit in Strict mode
        if self.durability_mode == DurabilityMode::Strict {
            self.sync()?;
        }

        debug!(tx_id = %tx_id, offset, "Transaction committed");
        Ok(offset)
    }

    /// Abort a transaction
    ///
    /// Writes an abort marker. This is optional since uncommitted
    /// entries are discarded during recovery anyway.
    ///
    /// # Arguments
    ///
    /// * `tx_id` - Transaction ID to abort
    pub fn abort_transaction(&mut self, tx_id: TxId) -> Result<u64, WalEntryError> {
        debug!(tx_id = %tx_id, "Aborting transaction");

        let entry = WalEntry::abort_marker(tx_id);
        self.write_entry(&entry)
    }

    /// Write a complete transaction atomically
    ///
    /// Convenience method that writes all entries followed by a commit marker.
    /// Returns the TxId of the committed transaction.
    ///
    /// # Arguments
    ///
    /// * `entries` - List of (entry_type, payload) pairs
    pub fn write_transaction(
        &mut self,
        entries: Vec<(WalEntryType, Vec<u8>)>,
    ) -> Result<TxId, WalEntryError> {
        let tx_id = self.begin_transaction();

        for (entry_type, payload) in entries {
            self.write_tx_entry(tx_id, entry_type, payload)?;
        }

        self.commit_transaction(tx_id)?;

        Ok(tx_id)
    }

    /// Write a snapshot marker
    ///
    /// Marks a point where a consistent snapshot was taken.
    /// WAL entries before this marker can be truncated after
    /// the snapshot is safely persisted.
    ///
    /// # Arguments
    ///
    /// * `snapshot_id` - Unique identifier for this snapshot
    /// * `wal_offset` - WAL offset at snapshot time
    pub fn write_snapshot_marker(
        &mut self,
        snapshot_id: Uuid,
        wal_offset: u64,
    ) -> Result<u64, WalEntryError> {
        let entry = WalEntry::snapshot_marker(snapshot_id, wal_offset);
        let offset = self.write_entry(&entry)?;

        // Always sync after snapshot marker
        self.sync()?;

        info!(snapshot_id = %snapshot_id, wal_offset, "Snapshot marker written");
        Ok(offset)
    }

    /// Write a single entry to the WAL
    ///
    /// Returns the offset where the entry was written.
    fn write_entry(&mut self, entry: &WalEntry) -> Result<u64, WalEntryError> {
        let offset = self.current_offset.load(Ordering::SeqCst);

        // Serialize entry
        let encoded = entry.serialize()?;

        // Write to file
        {
            let mut writer = self.writer.lock().unwrap();
            writer.write_all(&encoded)?;
        }

        // Update offset
        self.current_offset
            .fetch_add(encoded.len() as u64, Ordering::SeqCst);

        // Handle durability mode
        self.handle_durability_mode()?;

        trace!(offset, entry_type = ?entry.entry_type, "Entry written");
        Ok(offset)
    }

    /// Handle durability mode after write
    fn handle_durability_mode(&mut self) -> Result<(), WalEntryError> {
        match self.durability_mode {
            DurabilityMode::InMemory => {
                // Just flush buffer for consistency
                let mut writer = self.writer.lock().unwrap();
                writer.flush()?;
            }
            DurabilityMode::Strict => {
                // Sync is handled explicitly in commit_transaction
                // For non-commit entries, just flush
                let mut writer = self.writer.lock().unwrap();
                writer.flush()?;
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
                    self.sync()?;
                    self.writes_since_fsync.store(0, Ordering::SeqCst);
                    *self.last_fsync.lock().unwrap() = Instant::now();
                }
            }
            DurabilityMode::Async { .. } => {
                // Background thread handles fsync
                // Just flush buffer
                let mut writer = self.writer.lock().unwrap();
                writer.flush()?;
            }
        }

        Ok(())
    }

    /// Flush buffered writes to OS buffers
    pub fn flush(&mut self) -> Result<(), WalEntryError> {
        let mut writer = self.writer.lock().unwrap();
        writer.flush()?;
        Ok(())
    }

    /// Force sync to disk (flush + fsync)
    ///
    /// Ensures all buffered data is written to disk.
    pub fn sync(&self) -> Result<(), WalEntryError> {
        let mut writer = self.writer.lock().unwrap();
        writer.flush()?;
        writer.get_mut().sync_all()?;
        Ok(())
    }

    /// Get current file position (offset for next write)
    pub fn position(&self) -> u64 {
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

impl Drop for WalWriter {
    fn drop(&mut self) {
        // Shutdown async fsync thread
        self.shutdown.store(true, Ordering::Relaxed);

        if let Some(handle) = self.fsync_thread.take() {
            let _ = handle.join();
        }

        // Final sync
        let _ = self.sync();
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_begin_transaction() {
        let temp_dir = TempDir::new().unwrap();
        let wal_path = temp_dir.path().join("test.wal");

        let writer = WalWriter::open(&wal_path, DurabilityMode::default()).unwrap();

        let tx1 = writer.begin_transaction();
        let tx2 = writer.begin_transaction();

        // Each transaction should have a unique ID
        assert_ne!(tx1, tx2);
        assert!(!tx1.is_nil());
        assert!(!tx2.is_nil());
    }

    #[test]
    fn test_write_and_commit_transaction() {
        let temp_dir = TempDir::new().unwrap();
        let wal_path = temp_dir.path().join("test.wal");

        let mut writer = WalWriter::open(&wal_path, DurabilityMode::Strict).unwrap();

        let tx_id = writer.begin_transaction();
        writer
            .write_tx_entry(tx_id, WalEntryType::KvPut, b"key1=value1".to_vec())
            .unwrap();
        writer
            .write_tx_entry(tx_id, WalEntryType::KvPut, b"key2=value2".to_vec())
            .unwrap();
        writer.commit_transaction(tx_id).unwrap();

        // File should have data
        assert!(writer.position() > 0);
    }

    #[test]
    fn test_write_transaction_convenience() {
        let temp_dir = TempDir::new().unwrap();
        let wal_path = temp_dir.path().join("test.wal");

        let mut writer = WalWriter::open(&wal_path, DurabilityMode::Strict).unwrap();

        let tx_id = writer
            .write_transaction(vec![
                (WalEntryType::KvPut, b"key1=value1".to_vec()),
                (WalEntryType::KvPut, b"key2=value2".to_vec()),
            ])
            .unwrap();

        assert!(!tx_id.is_nil());
        assert!(writer.position() > 0);
    }

    #[test]
    fn test_abort_transaction() {
        let temp_dir = TempDir::new().unwrap();
        let wal_path = temp_dir.path().join("test.wal");

        let mut writer = WalWriter::open(&wal_path, DurabilityMode::Strict).unwrap();

        let tx_id = writer.begin_transaction();
        writer
            .write_tx_entry(tx_id, WalEntryType::KvPut, b"key=value".to_vec())
            .unwrap();
        writer.abort_transaction(tx_id).unwrap();

        assert!(writer.position() > 0);
    }

    #[test]
    fn test_snapshot_marker() {
        let temp_dir = TempDir::new().unwrap();
        let wal_path = temp_dir.path().join("test.wal");

        let mut writer = WalWriter::open(&wal_path, DurabilityMode::Strict).unwrap();

        let snapshot_id = Uuid::new_v4();
        let wal_offset = writer.position();
        writer
            .write_snapshot_marker(snapshot_id, wal_offset)
            .unwrap();

        assert!(writer.position() > 0);
    }

    #[test]
    fn test_multiple_transactions() {
        let temp_dir = TempDir::new().unwrap();
        let wal_path = temp_dir.path().join("test.wal");

        let mut writer = WalWriter::open(&wal_path, DurabilityMode::Strict).unwrap();

        // Write 10 transactions
        for i in 0..10 {
            let tx_id = writer
                .write_transaction(vec![(
                    WalEntryType::KvPut,
                    format!("key{}=value{}", i, i).into_bytes(),
                )])
                .unwrap();
            assert!(!tx_id.is_nil());
        }

        assert!(writer.position() > 0);
    }

    #[test]
    fn test_batched_mode() {
        let temp_dir = TempDir::new().unwrap();
        let wal_path = temp_dir.path().join("test.wal");

        let mut writer = WalWriter::open(
            &wal_path,
            DurabilityMode::Batched {
                interval_ms: 10000, // Long interval
                batch_size: 5,      // Trigger after 5 writes
            },
        )
        .unwrap();

        // Write 5 transactions (should trigger batch fsync)
        for i in 0..5 {
            writer
                .write_transaction(vec![(
                    WalEntryType::KvPut,
                    format!("key{}=value{}", i, i).into_bytes(),
                )])
                .unwrap();
        }

        // File should have data
        assert!(writer.position() > 0);
    }

    #[test]
    fn test_durability_mode_getter() {
        let temp_dir = TempDir::new().unwrap();
        let wal_path = temp_dir.path().join("test.wal");

        let writer = WalWriter::open(&wal_path, DurabilityMode::Strict).unwrap();
        assert_eq!(writer.durability_mode(), DurabilityMode::Strict);
    }

    #[test]
    fn test_path_getter() {
        let temp_dir = TempDir::new().unwrap();
        let wal_path = temp_dir.path().join("test.wal");

        let writer = WalWriter::open(&wal_path, DurabilityMode::Strict).unwrap();
        assert_eq!(writer.path(), wal_path);
    }

    #[test]
    fn test_creates_parent_directories() {
        let temp_dir = TempDir::new().unwrap();
        let wal_path = temp_dir.path().join("nested").join("dir").join("test.wal");

        let writer = WalWriter::open(&wal_path, DurabilityMode::Strict).unwrap();
        assert!(wal_path.exists());
        assert_eq!(writer.position(), 0);
    }

    #[test]
    fn test_interleaved_transactions() {
        let temp_dir = TempDir::new().unwrap();
        let wal_path = temp_dir.path().join("test.wal");

        let mut writer = WalWriter::open(&wal_path, DurabilityMode::Strict).unwrap();

        // Start two transactions
        let tx1 = writer.begin_transaction();
        let tx2 = writer.begin_transaction();

        // Interleave writes
        writer
            .write_tx_entry(tx1, WalEntryType::KvPut, b"tx1_key1=value1".to_vec())
            .unwrap();
        writer
            .write_tx_entry(tx2, WalEntryType::KvPut, b"tx2_key1=value1".to_vec())
            .unwrap();
        writer
            .write_tx_entry(tx1, WalEntryType::KvPut, b"tx1_key2=value2".to_vec())
            .unwrap();
        writer
            .write_tx_entry(tx2, WalEntryType::KvPut, b"tx2_key2=value2".to_vec())
            .unwrap();

        // Commit tx1, abort tx2
        writer.commit_transaction(tx1).unwrap();
        writer.abort_transaction(tx2).unwrap();

        assert!(writer.position() > 0);
    }
}
