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
//! - `None` - no fsync, data lost on crash (fastest, for testing)

use crate::encoding::{decode_entry, encode_entry};
use strata_core::{
    error::Result,
    primitives::json::JsonPath,
    types::{Key, RunId},
    value::Value,
    StrataError, Timestamp,
};
use serde::{Deserialize, Serialize};
use std::fs::{File, OpenOptions};
use tracing::error;
use std::io::{BufReader, BufWriter, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use parking_lot::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;
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

    // ========================================================================
    // JSON Operations - Entry types 0x20-0x23
    // ========================================================================
    /// Create new JSON document (0x20)
    ///
    /// Records creation of a new JSON document with initial value.
    /// The value is stored as msgpack-serialized bytes for WAL compatibility.
    JsonCreate {
        /// Run this operation belongs to
        run_id: RunId,
        /// Document identifier
        doc_id: String,
        /// Initial JSON value (msgpack serialized)
        value_bytes: Vec<u8>,
        /// Version assigned to this document
        version: u64,
        /// Timestamp when created
        timestamp: Timestamp,
    },

    /// Set value at path in JSON document (0x21)
    ///
    /// Records a path-level mutation to a JSON document.
    /// The value is stored as msgpack-serialized bytes for WAL compatibility.
    JsonSet {
        /// Run this operation belongs to
        run_id: RunId,
        /// Document identifier
        doc_id: String,
        /// Path to set value at
        path: JsonPath,
        /// New value at path (msgpack serialized)
        value_bytes: Vec<u8>,
        /// New document version after this operation
        version: u64,
    },

    /// Delete value at path in JSON document (0x22)
    ///
    /// Records deletion of a value at a path within a document.
    JsonDelete {
        /// Run this operation belongs to
        run_id: RunId,
        /// Document identifier
        doc_id: String,
        /// Path to delete
        path: JsonPath,
        /// New document version after this operation
        version: u64,
    },

    /// Destroy entire JSON document (0x23)
    ///
    /// Records complete deletion of a JSON document.
    JsonDestroy {
        /// Run this operation belongs to
        run_id: RunId,
        /// Document identifier
        doc_id: String,
    },

    // ========================================================================
    // Vector Operations - Entry types 0x70-0x73
    // ========================================================================
    /// Create vector collection (0x70)
    ///
    /// Records creation of a new vector collection with configuration.
    VectorCollectionCreate {
        /// Run this collection belongs to
        run_id: RunId,
        /// Collection name
        collection: String,
        /// Collection dimension
        dimension: usize,
        /// Distance metric (0=Cosine, 1=Euclidean, 2=DotProduct)
        metric: u8,
        /// Version assigned
        version: u64,
    },

    /// Delete vector collection (0x71)
    ///
    /// Records deletion of a vector collection and all its vectors.
    VectorCollectionDelete {
        /// Run this collection belongs to
        run_id: RunId,
        /// Collection name
        collection: String,
        /// Version assigned
        version: u64,
    },

    /// Vector upsert (insert or update) (0x72)
    ///
    /// Records insertion or update of a vector with its embedding.
    /// WARNING: TEMPORARY M8 FORMAT - Full embedding stored in WAL.
    /// This bloats WAL size but ensures correctness for M8.
    VectorUpsert {
        /// Run this vector belongs to
        run_id: RunId,
        /// Collection name
        collection: String,
        /// User-provided key
        key: String,
        /// Internal vector ID (for deterministic replay)
        vector_id: u64,
        /// Full embedding data
        embedding: Vec<f32>,
        /// Optional metadata (MessagePack serialized)
        metadata: Option<Vec<u8>>,
        /// Version assigned
        version: u64,
        /// Optional reference to source document (e.g., JSON doc, KV entry)
        ///
        /// Used by internal search infrastructure to link embeddings back to
        /// their source documents for hydration during search result assembly.
        /// Backwards compatible: old WAL entries without this field default to None.
        #[serde(default)]
        source_ref: Option<strata_core::EntityRef>,
    },

    /// Vector delete (0x73)
    ///
    /// Records deletion of a vector from a collection.
    VectorDelete {
        /// Run this vector belongs to
        run_id: RunId,
        /// Collection name
        collection: String,
        /// User-provided key
        key: String,
        /// Internal vector ID
        vector_id: u64,
        /// Version assigned
        version: u64,
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
            // JSON operations
            WALEntry::JsonCreate { run_id, .. } => Some(*run_id),
            WALEntry::JsonSet { run_id, .. } => Some(*run_id),
            WALEntry::JsonDelete { run_id, .. } => Some(*run_id),
            WALEntry::JsonDestroy { run_id, .. } => Some(*run_id),
            // Vector operations
            WALEntry::VectorCollectionCreate { run_id, .. } => Some(*run_id),
            WALEntry::VectorCollectionDelete { run_id, .. } => Some(*run_id),
            WALEntry::VectorUpsert { run_id, .. } => Some(*run_id),
            WALEntry::VectorDelete { run_id, .. } => Some(*run_id),
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
    /// Write, Delete, Checkpoint, JsonCreate, JsonSet, JsonDelete.
    pub fn version(&self) -> Option<u64> {
        match self {
            WALEntry::Write { version, .. } => Some(*version),
            WALEntry::Delete { version, .. } => Some(*version),
            WALEntry::Checkpoint { version, .. } => Some(*version),
            // JSON operations with version
            WALEntry::JsonCreate { version, .. } => Some(*version),
            WALEntry::JsonSet { version, .. } => Some(*version),
            WALEntry::JsonDelete { version, .. } => Some(*version),
            // Vector operations with version
            WALEntry::VectorCollectionCreate { version, .. } => Some(*version),
            WALEntry::VectorCollectionDelete { version, .. } => Some(*version),
            WALEntry::VectorUpsert { version, .. } => Some(*version),
            WALEntry::VectorDelete { version, .. } => Some(*version),
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
/// - `None` - No persistence (fastest mode for dev/testing)
///
/// # Default
///
/// The default mode is `Batched { interval_ms: 100, batch_size: 1000 }`,
/// which fsyncs every 100ms or every 1000 writes, whichever comes first.
///
/// # Performance Targets
///
/// | Mode | Latency Target | Use Case |
/// |------|----------------|----------|
/// | None | <3µs | Tests, caches, ephemeral data |
/// | Batched | <30µs | Production (balanced) |
/// | Strict | ~2ms | Checkpoints, audit logs |
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DurabilityMode {
    /// No durability - all data lost on crash (fastest mode)
    ///
    /// Bypasses WAL entirely. No fsync, no file I/O.
    /// Target latency: <3µs for engine/put_direct.
    /// Use case: Tests, caches, ephemeral data, development.
    ///
    /// # Performance
    ///
    /// This mode enables 250K+ ops/sec by eliminating I/O entirely.
    None,

    /// fsync after every commit (slow, maximum durability)
    ///
    /// Use when data loss is unacceptable, even for a single write.
    /// Expect 10ms+ latency per write.
    Strict,

    /// fsync every N commits OR every T milliseconds
    ///
    /// Good balance of speed and safety. May lose up to batch_size
    /// writes or interval_ms of data on crash.
    /// Target latency: <30µs.
    Batched {
        /// Maximum time between fsyncs in milliseconds
        interval_ms: u64,
        /// Maximum writes between fsyncs
        batch_size: usize,
    },
}

impl DurabilityMode {
    /// Check if this mode requires WAL persistence
    ///
    /// Returns false for None mode, true for all others.
    pub fn requires_wal(&self) -> bool {
        !matches!(self, DurabilityMode::None)
    }

    /// Check if this mode requires immediate fsync on every commit
    ///
    /// Returns true only for Strict mode.
    pub fn requires_immediate_fsync(&self) -> bool {
        matches!(self, DurabilityMode::Strict)
    }

    /// Human-readable description of the mode
    pub fn description(&self) -> &'static str {
        match self {
            DurabilityMode::None => "No durability (fastest, all data lost on crash)",
            DurabilityMode::Strict => "Sync fsync (safest, slowest)",
            DurabilityMode::Batched { .. } => "Batched fsync (balanced speed/safety)",
        }
    }

    /// Create a buffered mode with recommended defaults
    ///
    /// Returns `Batched { interval_ms: 100, batch_size: 1000 }`.
    ///
    /// # Default Values
    ///
    /// - **interval_ms**: 100 - Maximum 100ms between fsyncs
    /// - **batch_size**: 1000 - Maximum 1000 writes before fsync
    ///
    /// # Rationale
    ///
    /// These defaults balance performance and durability:
    /// - 100ms interval keeps data loss window bounded
    /// - 1000 batch size handles burst writes efficiently
    /// - Both thresholds work together - whichever is reached first triggers fsync
    ///
    /// This is the recommended mode for production workloads.
    pub fn buffered_default() -> Self {
        DurabilityMode::Batched {
            interval_ms: 100,
            batch_size: 1000,
        }
    }
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
/// - `None` - no fsync, data lost on crash (fastest, for testing)
///
/// # Example
///
/// ```ignore
/// use strata_durability::wal::{WAL, WALEntry, DurabilityMode};
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

    /// File handle (buffered writer for appends)
    writer: Arc<Mutex<BufWriter<File>>>,

    /// Current file offset (atomic for thread-safe access)
    current_offset: Arc<AtomicU64>,

    /// Durability mode
    durability_mode: DurabilityMode,

    /// Last fsync time (for batched mode)
    last_fsync: Arc<Mutex<Instant>>,

    /// Writes since last fsync (for batched mode)
    writes_since_fsync: Arc<AtomicU64>,
}

/// Result of reading WAL entries with detailed information
///
/// This struct provides more information than just the entries,
/// including whether corruption was detected.
#[derive(Debug)]
pub struct WalReadResult {
    /// Successfully decoded entries
    pub entries: Vec<WALEntry>,
    /// Number of bytes successfully read
    pub bytes_read: u64,
    /// If corruption was detected, information about it
    pub corruption: Option<WalCorruptionInfo>,
}

/// Information about WAL corruption detected during read
#[derive(Debug, Clone)]
pub struct WalCorruptionInfo {
    /// Byte offset where corruption was detected
    pub offset: u64,
    /// Description of the corruption
    pub message: String,
    /// Number of entries successfully read before corruption
    pub entries_before_corruption: usize,
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

        Ok(Self {
            path,
            writer,
            current_offset,
            durability_mode,
            last_fsync,
            writes_since_fsync,
        })
    }

    /// Append entry to WAL with durability mode handling
    ///
    /// Encodes entry and writes to end of file.
    /// Handles fsync based on configured durability mode:
    /// - Strict: fsync after every write
    /// - Batched: fsync after batch_size writes OR interval_ms elapsed
    /// - None: just flush, no fsync (data may be lost on crash)
    ///
    /// # Arguments
    ///
    /// * `entry` - WAL entry to append
    ///
    /// # Returns
    ///
    /// * `Ok(u64)` - Offset where entry was written
    /// * `Err` - If encoding or writing fails
    ///
    /// # Thread Safety
    ///
    /// This method is thread-safe. Multiple threads can call append concurrently.
    /// Internal locking ensures entries are written atomically.
    pub fn append(&self, entry: &WALEntry) -> Result<u64> {
        let offset = self.current_offset.load(Ordering::SeqCst);

        // Encode entry
        let encoded = encode_entry(entry)?;

        // Write to file
        {
            let mut writer = self.writer.lock();
            writer.write_all(&encoded).map_err(|e| {
                StrataError::storage(format!("Failed to write entry at offset {}: {}", offset, e))
            })?;
        }

        // Update offset
        self.current_offset
            .fetch_add(encoded.len() as u64, Ordering::SeqCst);

        // Handle durability mode
        match self.durability_mode {
            DurabilityMode::None => {
                // No fsync for None mode
                // Just flush buffer for consistency - in practice, engine should
                // check requires_wal() and skip WAL entirely for None mode
                let mut writer = self.writer.lock();
                writer
                    .flush()
                    .map_err(|e| StrataError::storage(format!("Failed to flush: {}", e)))?;
            }
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
                    let last = self.last_fsync.lock();
                    let elapsed = last.elapsed().as_millis() as u64;
                    let writes = self.writes_since_fsync.load(Ordering::SeqCst);

                    elapsed >= interval_ms || writes >= batch_size as u64
                };

                if should_fsync {
                    self.fsync()?;
                    self.writes_since_fsync.store(0, Ordering::SeqCst);
                    *self.last_fsync.lock() = Instant::now();
                }
            }
        }

        Ok(offset)
    }

    /// Flush buffered writes to OS buffers
    ///
    /// Note: This flushes to OS buffers, not necessarily to disk.
    /// For true durability, use fsync().
    ///
    /// # Thread Safety
    ///
    /// This method is thread-safe.
    pub fn flush(&self) -> Result<()> {
        let mut writer = self.writer.lock();
        writer
            .flush()
            .map_err(|e| StrataError::storage(format!("Failed to flush WAL: {}", e)))
    }

    /// Force sync to disk (flush + fsync)
    ///
    /// Ensures all buffered data is written to disk.
    /// This is the most durable option but also the slowest.
    pub fn fsync(&self) -> Result<()> {
        let mut writer = self.writer.lock();

        // Flush buffer
        writer
            .flush()
            .map_err(|e| StrataError::storage(format!("Failed to flush: {}", e)))?;

        // Fsync to disk
        writer
            .get_mut()
            .sync_all()
            .map_err(|e| StrataError::storage(format!("Failed to fsync: {}", e)))?;

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
            let mut writer = self.writer.lock();
            if let Err(e) = writer.flush() {
                error!(error = %e, "WAL flush before read failed");
            }
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
                    Err(StrataError::Storage { ref message, .. }) if message.contains("Incomplete entry") => {
                        // Incomplete entry - need more data, keep the remaining bytes
                        // This is expected when entry spans buffer boundaries
                        break;
                    }
                    Err(StrataError::Corruption { message }) => {
                        // CRC mismatch or deserialization failure - actual corruption!
                        // Return entries read so far and stop (conservative approach:
                        // don't skip past corruption, return valid entries before it)
                        error!(
                            offset = file_offset,
                            error = %message,
                            entries_recovered = entries.len(),
                            "WAL corruption detected - returning valid entries before corruption"
                        );
                        return Ok(entries);
                    }
                    Err(e) => {
                        // Other unexpected error - return it
                        return Err(e);
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

    /// Read entries with detailed corruption information
    ///
    /// Unlike `read_entries()` which just returns entries, this method returns
    /// a `WalReadResult` that includes information about any corruption detected.
    /// This is useful for recovery diagnostics.
    ///
    /// # Arguments
    ///
    /// * `start_offset` - Byte offset to start reading from
    ///
    /// # Returns
    ///
    /// * `Ok(WalReadResult)` - Detailed read result with entries and corruption info
    /// * `Err` - If file operations fail (not corruption)
    pub fn read_entries_detailed(&self, start_offset: u64) -> Result<WalReadResult> {
        // Flush any buffered writes before reading
        {
            let mut writer = self.writer.lock();
            if let Err(e) = writer.flush() {
                error!(error = %e, "WAL flush before read failed");
            }
        }

        // Open separate read handle
        let file = File::open(&self.path)?;
        let mut reader = BufReader::new(file);
        reader.seek(SeekFrom::Start(start_offset))?;

        let mut entries = Vec::new();
        let mut file_offset = start_offset;
        let mut buf = Vec::new();
        let mut read_buf = vec![0u8; 64 * 1024];
        let mut corruption: Option<WalCorruptionInfo> = None;

        loop {
            let bytes_read = reader.read(&mut read_buf)?;
            if bytes_read == 0 {
                break;
            }

            buf.extend_from_slice(&read_buf[..bytes_read]);

            let mut offset_in_buf = 0;
            while offset_in_buf < buf.len() {
                match decode_entry(&buf[offset_in_buf..], file_offset) {
                    Ok((entry, bytes_consumed)) => {
                        entries.push(entry);
                        offset_in_buf += bytes_consumed;
                        file_offset += bytes_consumed as u64;
                    }
                    Err(StrataError::Storage { ref message, .. }) if message.contains("Incomplete entry") => {
                        break;
                    }
                    Err(StrataError::Corruption { message }) => {
                        corruption = Some(WalCorruptionInfo {
                            offset: file_offset,
                            message,
                            entries_before_corruption: entries.len(),
                        });
                        // Stop reading on corruption
                        buf.clear();
                        break;
                    }
                    Err(_) => {
                        break;
                    }
                }
            }

            if corruption.is_some() {
                break;
            }

            if offset_in_buf > 0 {
                buf.drain(..offset_in_buf);
            }

            if bytes_read < read_buf.len() && !buf.is_empty() {
                break;
            }
        }

        Ok(WalReadResult {
            entries,
            bytes_read: file_offset - start_offset,
            corruption,
        })
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

    /// Truncate WAL to specified offset
    ///
    /// Removes all data after the given offset. This is used after a checkpoint
    /// to reclaim disk space by removing entries that are no longer needed
    /// (their state is captured in the snapshot).
    ///
    /// # Safety
    ///
    /// This operation is destructive and cannot be undone. Only truncate after:
    /// 1. A snapshot has been successfully written and verified
    /// 2. The snapshot contains all data up to the truncation point
    ///
    /// # Arguments
    ///
    /// * `offset` - Byte offset to truncate to (data after this offset is removed)
    ///
    /// # Returns
    ///
    /// * `Ok(())` - If truncation succeeded
    /// * `Err` - If truncation fails (file system error, invalid offset, etc.)
    ///
    /// # Example
    ///
    /// ```ignore
    /// use strata_durability::wal::{WAL, DurabilityMode};
    ///
    /// let mut wal = WAL::open("data/wal/segment.wal", DurabilityMode::default())?;
    ///
    /// // After creating a snapshot at offset 10000...
    /// wal.truncate_to(10000)?;
    /// assert_eq!(wal.size(), 10000);
    /// ```
    pub fn truncate_to(&mut self, offset: u64) -> Result<()> {
        // Flush any pending writes first
        self.flush()?;

        // Validate offset
        let current_size = self.current_offset.load(Ordering::SeqCst);
        if offset > current_size {
            return Err(StrataError::invalid_input(format!(
                "Cannot truncate to offset {} - WAL size is only {}",
                offset, current_size
            )));
        }

        // Close current writer and truncate the file
        {
            let mut writer = self.writer.lock();
            writer
                .flush()
                .map_err(|e| StrataError::storage(format!("Failed to flush before truncate: {}", e)))?;

            // Get underlying file and truncate
            let file = writer.get_mut();
            file.set_len(offset)
                .map_err(|e| StrataError::storage(format!("Failed to truncate WAL: {}", e)))?;

            // Sync the truncation to disk
            file.sync_all()
                .map_err(|e| StrataError::storage(format!("Failed to sync after truncate: {}", e)))?;
        }

        // Update current offset
        self.current_offset.store(offset, Ordering::SeqCst);

        Ok(())
    }

    /// Find the last checkpoint in the WAL and return its offset
    ///
    /// Scans the WAL for Checkpoint entries and returns the byte offset
    /// immediately after the last checkpoint. This offset can be used for
    /// safe truncation after verifying the corresponding snapshot.
    ///
    /// # Returns
    ///
    /// * `Ok(Some((offset, snapshot_id, version)))` - Offset after checkpoint, snapshot ID, and version
    /// * `Ok(None)` - No checkpoints found in WAL
    /// * `Err` - If reading WAL fails
    pub fn find_last_checkpoint(&self) -> Result<Option<(u64, uuid::Uuid, u64)>> {
        // Flush any buffered writes before reading
        {
            let mut writer = self.writer.lock();
            if let Err(e) = writer.flush() {
                error!(error = %e, "WAL flush before checkpoint scan failed");
            }
        }

        let mut last_checkpoint: Option<(uuid::Uuid, u64)> = None;
        let mut offset: u64 = 0;
        let mut last_checkpoint_end_offset: u64 = 0;

        // Read WAL while tracking byte offsets
        let file = std::fs::File::open(&self.path)?;
        let mut reader = std::io::BufReader::new(file);
        let mut buf = Vec::new();
        let mut read_buf = vec![0u8; 64 * 1024];

        loop {
            let bytes_read = reader.read(&mut read_buf)?;
            if bytes_read == 0 {
                break;
            }

            buf.extend_from_slice(&read_buf[..bytes_read]);

            let mut pos = 0;
            while pos < buf.len() {
                match crate::encoding::decode_entry(&buf[pos..], offset) {
                    Ok((entry, consumed)) => {
                        if let WALEntry::Checkpoint {
                            snapshot_id,
                            version,
                            ..
                        } = entry
                        {
                            last_checkpoint = Some((snapshot_id, version));
                            last_checkpoint_end_offset = offset + consumed as u64;
                        }
                        pos += consumed;
                        offset += consumed as u64;
                    }
                    Err(StrataError::Storage { ref message, .. }) if message.contains("Incomplete entry") => {
                        // Need more data
                        break;
                    }
                    Err(_) => {
                        // Corruption or other error - stop scanning
                        buf.clear();
                        break;
                    }
                }
            }

            if pos > 0 {
                buf.drain(..pos);
            }

            if bytes_read < read_buf.len() {
                break;
            }
        }

        Ok(last_checkpoint.map(|(id, ver)| (last_checkpoint_end_offset, id, ver)))
    }
}

impl Drop for WAL {
    fn drop(&mut self) {
        // Final fsync to ensure all data is durable
        if let Err(e) = self.fsync() {
            error!(
                error = %e,
                path = %self.path.display(),
                "WAL final fsync on drop failed - data may not be durable"
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use strata_core::types::Namespace;
    use std::thread;
    use std::time::Duration;

    /// Helper to get current timestamp
    fn now() -> Timestamp {
        Timestamp::now()
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
    fn test_none_mode() {
        let temp_dir = TempDir::new().unwrap();
        let wal_path = temp_dir.path().join("none.wal");

        let mut wal = WAL::open(&wal_path, DurabilityMode::None).unwrap();
        assert_eq!(wal.durability_mode(), DurabilityMode::None);

        let run_id = RunId::new();
        let entry = WALEntry::BeginTxn {
            txn_id: 1,
            run_id,
            timestamp: now(),
        };

        wal.append(&entry).unwrap();

        // Drop performs final fsync, so entry should be readable
        drop(wal);
        let wal = WAL::open(&wal_path, DurabilityMode::None).unwrap();
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

        // Use None mode - data won't be synced during append
        {
            let mut wal = WAL::open(&wal_path, DurabilityMode::None).unwrap();
            wal.append(&entry).unwrap();
            // Drop should call final fsync
        }

        // Entry should still be readable after drop's final fsync
        let wal = WAL::open(&wal_path, DurabilityMode::default()).unwrap();
        let entries = wal.read_all().unwrap();
        assert_eq!(entries.len(), 1);
    }

    // ========================================================================
    // M4 DurabilityMode Tests
    // ========================================================================

    #[test]
    fn test_durability_mode_requires_wal() {
        // None does not require WAL
        assert!(!DurabilityMode::None.requires_wal());

        // All others require WAL
        assert!(DurabilityMode::Strict.requires_wal());
        assert!(DurabilityMode::default().requires_wal());
        assert!(DurabilityMode::buffered_default().requires_wal());
    }

    #[test]
    fn test_durability_mode_requires_immediate_fsync() {
        // Only Strict requires immediate fsync
        assert!(DurabilityMode::Strict.requires_immediate_fsync());

        // Others do not
        assert!(!DurabilityMode::None.requires_immediate_fsync());
        assert!(!DurabilityMode::default().requires_immediate_fsync());
        assert!(!DurabilityMode::buffered_default().requires_immediate_fsync());
    }

    #[test]
    fn test_durability_mode_description() {
        // All modes have non-empty descriptions
        assert!(!DurabilityMode::None.description().is_empty());
        assert!(!DurabilityMode::Strict.description().is_empty());
        assert!(!DurabilityMode::default().description().is_empty());
    }

    #[test]
    fn test_durability_mode_buffered_default() {
        let mode = DurabilityMode::buffered_default();
        match mode {
            DurabilityMode::Batched {
                interval_ms,
                batch_size,
            } => {
                assert_eq!(interval_ms, 100);
                assert_eq!(batch_size, 1000);
            }
            _ => panic!("Expected Batched mode from buffered_default()"),
        }
    }

    #[test]
    fn test_durability_mode_none_variant() {
        // Ensure None is a valid variant and can be used
        let mode = DurabilityMode::None;
        assert!(!mode.requires_wal());
        assert!(!mode.requires_immediate_fsync());
        assert!(mode.description().contains("durability"));
    }

    // ========================================================================
    // JSON Entry Type Tests
    // ========================================================================

    use strata_core::primitives::json::JsonPath;

    #[test]
    fn test_json_create_entry() {
        let run_id = RunId::new();
        let doc_id = "test-doc";
        // Simulate msgpack-serialized empty object
        let value_bytes = vec![0x80]; // msgpack empty map

        let entry = WALEntry::JsonCreate {
            run_id,
            doc_id: doc_id.to_string(),
            value_bytes,
            version: 1,
            timestamp: now(),
        };

        assert_eq!(entry.run_id(), Some(run_id));
        assert_eq!(entry.version(), Some(1));
        assert!(!entry.is_txn_boundary());
        assert!(!entry.is_checkpoint());
        assert_eq!(entry.txn_id(), None);
    }

    #[test]
    fn test_json_set_entry() {
        let run_id = RunId::new();
        let doc_id = "test-doc";
        let path = "user.name".parse::<JsonPath>().unwrap();
        // Simulate msgpack-serialized string "Alice"
        let value_bytes = b"\xa5Alice".to_vec();

        let entry = WALEntry::JsonSet {
            run_id,
            doc_id: doc_id.to_string(),
            path: path.clone(),
            value_bytes,
            version: 2,
        };

        assert_eq!(entry.run_id(), Some(run_id));
        assert_eq!(entry.version(), Some(2));
        assert!(!entry.is_txn_boundary());
        assert!(!entry.is_checkpoint());

        if let WALEntry::JsonSet {
            path: p, version, ..
        } = entry
        {
            assert_eq!(p, path);
            assert_eq!(version, 2);
        } else {
            panic!("Expected JsonSet variant");
        }
    }

    #[test]
    fn test_json_delete_entry() {
        let run_id = RunId::new();
        let doc_id = "test-doc";
        let path = "temp.field".parse::<JsonPath>().unwrap();

        let entry = WALEntry::JsonDelete {
            run_id,
            doc_id: doc_id.to_string(),
            path,
            version: 3,
        };

        assert_eq!(entry.run_id(), Some(run_id));
        assert_eq!(entry.version(), Some(3));
        assert!(!entry.is_txn_boundary());
        assert!(!entry.is_checkpoint());
    }

    #[test]
    fn test_json_destroy_entry() {
        let run_id = RunId::new();
        let doc_id = "test-doc";

        let entry = WALEntry::JsonDestroy { run_id, doc_id: doc_id.to_string() };

        assert_eq!(entry.run_id(), Some(run_id));
        assert_eq!(entry.version(), None); // JsonDestroy has no version
        assert!(!entry.is_txn_boundary());
        assert!(!entry.is_checkpoint());
    }

    #[test]
    fn test_json_entries_serialize() {
        let run_id = RunId::new();
        let doc_id = "test-doc";

        let entries = vec![
            WALEntry::JsonCreate {
                run_id,
                doc_id: doc_id.to_string(),
                value_bytes: vec![0x80], // msgpack empty map
                version: 1,
                timestamp: now(),
            },
            WALEntry::JsonSet {
                run_id,
                doc_id: doc_id.to_string(),
                path: "name".parse().unwrap(),
                value_bytes: b"\xa4test".to_vec(), // msgpack string "test"
                version: 2,
            },
            WALEntry::JsonDelete {
                run_id,
                doc_id: doc_id.to_string(),
                path: "temp".parse().unwrap(),
                version: 3,
            },
            WALEntry::JsonDestroy { run_id, doc_id: doc_id.to_string() },
        ];

        for entry in entries {
            let encoded = bincode::serialize(&entry).expect("serialization failed");
            let decoded: WALEntry = bincode::deserialize(&encoded).expect("deserialization failed");
            assert_eq!(entry, decoded);
        }
    }

    // ========================================================================
    // WAL Truncation Tests
    // ========================================================================

    #[test]
    fn test_truncate_to() {
        let temp_dir = TempDir::new().unwrap();
        let wal_path = temp_dir.path().join("truncate.wal");

        let mut wal = WAL::open(&wal_path, DurabilityMode::default()).unwrap();

        let run_id = RunId::new();
        let entry1 = WALEntry::BeginTxn {
            txn_id: 1,
            run_id,
            timestamp: now(),
        };
        let entry2 = WALEntry::CommitTxn { txn_id: 1, run_id };

        // Append entries and get offsets
        let offset1 = wal.append(&entry1).unwrap();
        let offset2 = wal.append(&entry2).unwrap();
        wal.flush().unwrap();

        // Verify both entries are readable
        let entries = wal.read_all().unwrap();
        assert_eq!(entries.len(), 2);

        // Truncate to after first entry
        wal.truncate_to(offset2).unwrap();
        assert_eq!(wal.size(), offset2);

        // Read should only return first entry
        let entries = wal.read_all().unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0], entry1);

        // Truncate to beginning
        wal.truncate_to(0).unwrap();
        assert_eq!(wal.size(), 0);

        // Read should return empty
        let entries = wal.read_all().unwrap();
        assert_eq!(entries.len(), 0);
    }

    #[test]
    fn test_truncate_invalid_offset() {
        let temp_dir = TempDir::new().unwrap();
        let wal_path = temp_dir.path().join("truncate_invalid.wal");

        let mut wal = WAL::open(&wal_path, DurabilityMode::default()).unwrap();

        // Try to truncate to offset beyond file size
        let result = wal.truncate_to(1000);
        assert!(result.is_err());
    }

    #[test]
    fn test_find_last_checkpoint() {
        let temp_dir = TempDir::new().unwrap();
        let wal_path = temp_dir.path().join("checkpoint.wal");

        let mut wal = WAL::open(&wal_path, DurabilityMode::default()).unwrap();

        let run_id = RunId::new();

        // Append entries without checkpoint
        wal.append(&WALEntry::BeginTxn {
            txn_id: 1,
            run_id,
            timestamp: now(),
        }).unwrap();

        // No checkpoint yet
        let result = wal.find_last_checkpoint().unwrap();
        assert!(result.is_none());

        // Add checkpoint
        let snapshot_id = uuid::Uuid::new_v4();
        wal.append(&WALEntry::Checkpoint {
            snapshot_id,
            version: 100,
            active_runs: vec![run_id],
        }).unwrap();

        // More entries after checkpoint
        wal.append(&WALEntry::CommitTxn { txn_id: 1, run_id }).unwrap();
        wal.flush().unwrap();

        // Find checkpoint
        let result = wal.find_last_checkpoint().unwrap();
        assert!(result.is_some());

        let (offset, found_id, version) = result.unwrap();
        assert_eq!(found_id, snapshot_id);
        assert_eq!(version, 100);
        assert!(offset > 0);
    }

    // ========================================================================
    // Detailed Read Tests
    // ========================================================================

    #[test]
    fn test_read_entries_detailed() {
        let temp_dir = TempDir::new().unwrap();
        let wal_path = temp_dir.path().join("detailed.wal");

        let mut wal = WAL::open(&wal_path, DurabilityMode::default()).unwrap();

        let run_id = RunId::new();
        let entry1 = WALEntry::BeginTxn {
            txn_id: 1,
            run_id,
            timestamp: now(),
        };
        let entry2 = WALEntry::CommitTxn { txn_id: 1, run_id };

        wal.append(&entry1).unwrap();
        wal.append(&entry2).unwrap();
        wal.flush().unwrap();

        // Read detailed result
        let result = wal.read_entries_detailed(0).unwrap();

        assert_eq!(result.entries.len(), 2);
        assert!(result.bytes_read > 0);
        assert!(result.corruption.is_none());
    }

    #[test]
    fn test_read_entries_detailed_detects_corruption() {
        let temp_dir = TempDir::new().unwrap();
        let wal_path = temp_dir.path().join("corrupt_detailed.wal");

        // Write entries and record entry boundaries
        let mut entry_offsets = Vec::new();
        {
            let mut wal = WAL::open(&wal_path, DurabilityMode::Strict).unwrap();
            let run_id = RunId::new();

            for i in 0..5 {
                let offset = wal.append(&WALEntry::BeginTxn {
                    txn_id: i,
                    run_id,
                    timestamp: now(),
                }).unwrap();
                entry_offsets.push(offset);
            }
            wal.flush().unwrap();
        }

        // Corrupt the file AFTER the second entry (in the third entry)
        // This ensures we can read at least 2 entries before hitting corruption
        let corrupt_offset = entry_offsets[2] + 10; // 10 bytes into third entry
        {
            use std::io::Write;
            let mut file = std::fs::OpenOptions::new()
                .write(true)
                .open(&wal_path)
                .unwrap();
            file.seek(SeekFrom::Start(corrupt_offset)).unwrap();
            file.write_all(&[0xFF; 20]).unwrap(); // Write garbage
            file.sync_all().unwrap();
        }

        // Read with detailed method
        let wal = WAL::open(&wal_path, DurabilityMode::default()).unwrap();
        let result = wal.read_entries_detailed(0).unwrap();

        // Should have read exactly 2 entries before corruption
        assert_eq!(result.entries.len(), 2, "Should read 2 entries before corruption");

        // Corruption info should be present
        assert!(result.corruption.is_some(), "Should detect corruption");
        let corruption = result.corruption.unwrap();

        // Corruption offset should be at the start of the third entry
        assert_eq!(corruption.offset, entry_offsets[2], "Corruption offset should be at third entry");
        assert!(!corruption.message.is_empty(), "Corruption message should not be empty");
        assert_eq!(corruption.entries_before_corruption, 2, "Should report 2 entries before corruption");
    }

    // ========================================================================
    // Vector Entry Type Tests
    // ========================================================================

    #[test]
    fn test_vector_collection_create_entry() {
        let run_id = RunId::new();
        let entry = WALEntry::VectorCollectionCreate {
            run_id,
            collection: "embeddings".to_string(),
            dimension: 384,
            metric: 0, // Cosine
            version: 1,
        };

        assert_eq!(entry.run_id(), Some(run_id));
        assert_eq!(entry.version(), Some(1));
        assert!(!entry.is_txn_boundary());
        assert!(!entry.is_checkpoint());
        assert_eq!(entry.txn_id(), None);
    }

    #[test]
    fn test_vector_collection_delete_entry() {
        let run_id = RunId::new();
        let entry = WALEntry::VectorCollectionDelete {
            run_id,
            collection: "old_embeddings".to_string(),
            version: 42,
        };

        assert_eq!(entry.run_id(), Some(run_id));
        assert_eq!(entry.version(), Some(42));
        assert!(!entry.is_txn_boundary());
    }

    #[test]
    fn test_vector_upsert_entry() {
        let run_id = RunId::new();
        let entry = WALEntry::VectorUpsert {
            run_id,
            collection: "embeddings".to_string(),
            key: "doc_1".to_string(),
            vector_id: 99,
            embedding: vec![0.1, 0.2, 0.3, 0.4],
            metadata: Some(vec![0x80]), // msgpack empty map
            version: 5,
            source_ref: None,
        };

        assert_eq!(entry.run_id(), Some(run_id));
        assert_eq!(entry.version(), Some(5));
        assert!(!entry.is_txn_boundary());
        assert_eq!(entry.txn_id(), None);

        if let WALEntry::VectorUpsert {
            embedding,
            vector_id,
            ..
        } = entry
        {
            assert_eq!(embedding.len(), 4);
            assert_eq!(vector_id, 99);
        } else {
            panic!("Expected VectorUpsert variant");
        }
    }

    #[test]
    fn test_vector_delete_entry() {
        let run_id = RunId::new();
        let entry = WALEntry::VectorDelete {
            run_id,
            collection: "embeddings".to_string(),
            key: "doc_1".to_string(),
            vector_id: 99,
            version: 6,
        };

        assert_eq!(entry.run_id(), Some(run_id));
        assert_eq!(entry.version(), Some(6));
        assert!(!entry.is_txn_boundary());
    }

    #[test]
    fn test_vector_entries_serialize() {
        let run_id = RunId::new();

        let entries = vec![
            WALEntry::VectorCollectionCreate {
                run_id,
                collection: "col".to_string(),
                dimension: 128,
                metric: 1, // Euclidean
                version: 1,
            },
            WALEntry::VectorUpsert {
                run_id,
                collection: "col".to_string(),
                key: "k1".to_string(),
                vector_id: 1,
                embedding: vec![1.0; 128],
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

        for entry in entries {
            let encoded = bincode::serialize(&entry).expect("serialization failed");
            let decoded: WALEntry = bincode::deserialize(&encoded).expect("deserialization failed");
            assert_eq!(entry, decoded);
        }
    }

    #[test]
    fn test_vector_upsert_with_source_ref() {
        let run_id = RunId::new();
        let entity_ref = strata_core::EntityRef::kv(run_id, "source_key");

        let entry = WALEntry::VectorUpsert {
            run_id,
            collection: "embeddings".to_string(),
            key: "vec_1".to_string(),
            vector_id: 1,
            embedding: vec![0.5; 4],
            metadata: None,
            version: 1,
            source_ref: Some(entity_ref.clone()),
        };

        // Serialize and deserialize
        let encoded = bincode::serialize(&entry).expect("serialization failed");
        let decoded: WALEntry = bincode::deserialize(&encoded).expect("deserialization failed");

        if let WALEntry::VectorUpsert { source_ref, .. } = decoded {
            assert_eq!(source_ref, Some(entity_ref));
        } else {
            panic!("Expected VectorUpsert");
        }
    }

    #[test]
    fn test_vector_upsert_source_ref_default_none() {
        // Test backward compatibility: old entries without source_ref default to None
        let run_id = RunId::new();
        let entry = WALEntry::VectorUpsert {
            run_id,
            collection: "col".to_string(),
            key: "k".to_string(),
            vector_id: 1,
            embedding: vec![1.0],
            metadata: None,
            version: 1,
            source_ref: None,
        };

        let encoded = bincode::serialize(&entry).unwrap();
        let decoded: WALEntry = bincode::deserialize(&encoded).unwrap();

        if let WALEntry::VectorUpsert { source_ref, .. } = decoded {
            assert!(source_ref.is_none());
        } else {
            panic!("Expected VectorUpsert");
        }
    }

    // ========================================================================
    // WAL File Adversarial Tests
    // ========================================================================

    #[test]
    fn test_wal_read_empty_file() {
        let temp_dir = TempDir::new().unwrap();
        let wal_path = temp_dir.path().join("empty.wal");

        let wal = WAL::open(&wal_path, DurabilityMode::default()).unwrap();
        let entries = wal.read_all().unwrap();
        assert!(entries.is_empty());
        assert_eq!(wal.size(), 0);
    }

    #[test]
    fn test_wal_read_entries_detailed_empty() {
        let temp_dir = TempDir::new().unwrap();
        let wal_path = temp_dir.path().join("empty_detailed.wal");

        let wal = WAL::open(&wal_path, DurabilityMode::default()).unwrap();
        let result = wal.read_entries_detailed(0).unwrap();

        assert!(result.entries.is_empty());
        assert_eq!(result.bytes_read, 0);
        assert!(result.corruption.is_none());
    }

    #[test]
    fn test_wal_truncate_then_append() {
        let temp_dir = TempDir::new().unwrap();
        let wal_path = temp_dir.path().join("trunc_append.wal");

        let mut wal = WAL::open(&wal_path, DurabilityMode::Strict).unwrap();
        let run_id = RunId::new();

        // Write 3 entries
        for i in 0..3 {
            wal.append(&WALEntry::BeginTxn {
                txn_id: i,
                run_id,
                timestamp: now(),
            })
            .unwrap();
        }
        wal.flush().unwrap();

        let entries_before = wal.read_all().unwrap();
        assert_eq!(entries_before.len(), 3);

        // Truncate to 0
        wal.truncate_to(0).unwrap();
        assert_eq!(wal.size(), 0);

        // Append new entry after truncation
        let new_entry = WALEntry::CommitTxn { txn_id: 99, run_id };
        wal.append(&new_entry).unwrap();
        wal.flush().unwrap();

        let entries_after = wal.read_all().unwrap();
        assert_eq!(entries_after.len(), 1);
        assert_eq!(entries_after[0], new_entry);
    }

    #[test]
    fn test_wal_corruption_stops_read_entries() {
        let temp_dir = TempDir::new().unwrap();
        let wal_path = temp_dir.path().join("corrupt_read.wal");

        let mut entry_offsets = Vec::new();
        {
            let mut wal = WAL::open(&wal_path, DurabilityMode::Strict).unwrap();
            let run_id = RunId::new();
            for i in 0..5 {
                let offset = wal.append(&WALEntry::BeginTxn {
                    txn_id: i,
                    run_id,
                    timestamp: now(),
                })
                .unwrap();
                entry_offsets.push(offset);
            }
            wal.flush().unwrap();
        }

        // Corrupt the 4th entry
        {
            use std::io::Write;
            let mut file = std::fs::OpenOptions::new()
                .write(true)
                .open(&wal_path)
                .unwrap();
            file.seek(SeekFrom::Start(entry_offsets[3] + 5)).unwrap();
            file.write_all(&[0xFF; 30]).unwrap();
            file.sync_all().unwrap();
        }

        // read_entries should return entries before corruption
        let wal = WAL::open(&wal_path, DurabilityMode::default()).unwrap();
        let entries = wal.read_entries(0).unwrap();
        assert_eq!(entries.len(), 3, "Should read 3 entries before corruption");
    }

    #[test]
    fn test_wal_find_last_checkpoint_with_multiple_checkpoints() {
        let temp_dir = TempDir::new().unwrap();
        let wal_path = temp_dir.path().join("multi_checkpoint.wal");

        let mut wal = WAL::open(&wal_path, DurabilityMode::Strict).unwrap();
        let run_id = RunId::new();

        let snap_id_1 = uuid::Uuid::new_v4();
        let snap_id_2 = uuid::Uuid::new_v4();

        // First checkpoint at version 100
        wal.append(&WALEntry::Checkpoint {
            snapshot_id: snap_id_1,
            version: 100,
            active_runs: vec![run_id],
        })
        .unwrap();

        // Some entries between checkpoints
        wal.append(&WALEntry::BeginTxn {
            txn_id: 1,
            run_id,
            timestamp: now(),
        })
        .unwrap();

        // Second checkpoint at version 200 (should be the one found)
        wal.append(&WALEntry::Checkpoint {
            snapshot_id: snap_id_2,
            version: 200,
            active_runs: vec![run_id],
        })
        .unwrap();
        wal.flush().unwrap();

        let result = wal.find_last_checkpoint().unwrap();
        assert!(result.is_some());
        let (_, found_id, version) = result.unwrap();
        assert_eq!(found_id, snap_id_2, "Should find the LAST checkpoint");
        assert_eq!(version, 200);
    }

    #[test]
    fn test_wal_concurrent_append() {
        let temp_dir = TempDir::new().unwrap();
        let wal_path = temp_dir.path().join("concurrent.wal");

        let wal = Arc::new(WAL::open(&wal_path, DurabilityMode::None).unwrap());

        let handles: Vec<_> = (0..4)
            .map(|thread_id| {
                let wal = Arc::clone(&wal);
                thread::spawn(move || {
                    let run_id = RunId::new();
                    for i in 0..50 {
                        let entry = WALEntry::BeginTxn {
                            txn_id: (thread_id * 100 + i) as u64,
                            run_id,
                            timestamp: now(),
                        };
                        wal.append(&entry).unwrap();
                    }
                })
            })
            .collect();

        for h in handles {
            h.join().unwrap();
        }

        // Flush and read - should have 200 entries total (4 threads * 50 entries)
        wal.flush().unwrap();
        let entries = wal.read_all().unwrap();
        assert_eq!(entries.len(), 200);
    }

    #[test]
    fn test_wal_truncate_to_same_offset() {
        let temp_dir = TempDir::new().unwrap();
        let wal_path = temp_dir.path().join("trunc_same.wal");

        let mut wal = WAL::open(&wal_path, DurabilityMode::Strict).unwrap();
        let run_id = RunId::new();

        wal.append(&WALEntry::BeginTxn {
            txn_id: 1,
            run_id,
            timestamp: now(),
        })
        .unwrap();
        wal.flush().unwrap();

        let size = wal.size();

        // Truncate to current size (no-op)
        wal.truncate_to(size).unwrap();
        assert_eq!(wal.size(), size);

        // Data should still be readable
        let entries = wal.read_all().unwrap();
        assert_eq!(entries.len(), 1);
    }

    #[test]
    fn test_wal_json_destroy_has_no_version() {
        let run_id = RunId::new();
        let entry = WALEntry::JsonDestroy {
            run_id,
            doc_id: "doc".to_string(),
        };

        // JsonDestroy is the only variant with run_id that has NO version
        assert_eq!(entry.version(), None);
        assert_eq!(entry.run_id(), Some(run_id));
        assert_eq!(entry.txn_id(), None);
        assert!(!entry.is_txn_boundary());
        assert!(!entry.is_checkpoint());
    }

    #[test]
    fn test_wal_version_returns_none_for_non_versioned_entries() {
        let run_id = RunId::new();

        // These entry types should have no version
        let entries_without_version: Vec<WALEntry> = vec![
            WALEntry::BeginTxn {
                txn_id: 1,
                run_id,
                timestamp: now(),
            },
            WALEntry::CommitTxn { txn_id: 1, run_id },
            WALEntry::AbortTxn { txn_id: 1, run_id },
            WALEntry::JsonDestroy {
                run_id,
                doc_id: "d".to_string(),
            },
        ];

        for entry in &entries_without_version {
            assert_eq!(
                entry.version(),
                None,
                "Entry {:?} should have no version",
                entry
            );
        }
    }

    #[test]
    fn test_wal_txn_id_returns_none_for_non_txn_entries() {
        let run_id = RunId::new();
        let ns = Namespace::new(
            "t".to_string(),
            "a".to_string(),
            "g".to_string(),
            run_id,
        );

        let non_txn_entries: Vec<WALEntry> = vec![
            WALEntry::Write {
                run_id,
                key: Key::new_kv(ns, "k"),
                value: Value::Int(1),
                version: 1,
            },
            WALEntry::Checkpoint {
                snapshot_id: Uuid::new_v4(),
                version: 1,
                active_runs: vec![],
            },
            WALEntry::JsonCreate {
                run_id,
                doc_id: "d".to_string(),
                value_bytes: vec![],
                version: 1,
                timestamp: now(),
            },
            WALEntry::VectorUpsert {
                run_id,
                collection: "c".to_string(),
                key: "k".to_string(),
                vector_id: 1,
                embedding: vec![1.0],
                metadata: None,
                version: 1,
                source_ref: None,
            },
        ];

        for entry in &non_txn_entries {
            assert_eq!(
                entry.txn_id(),
                None,
                "Entry {:?} should have no txn_id",
                entry
            );
        }
    }
}
