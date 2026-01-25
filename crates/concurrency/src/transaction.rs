//! Transaction context for OCC
//!
//! This module implements the core transaction data structure for optimistic
//! concurrency control. TransactionContext tracks all reads, writes, deletes,
//! and CAS operations for a transaction, enabling validation at commit time.
//!
//! See `docs/architecture/M2_TRANSACTION_SEMANTICS.md` for the full specification.

use crate::validation::{validate_transaction, ValidationResult};
use crate::wal_writer::TransactionWALWriter;
use strata_core::error::{Error, Result};
use strata_core::json::{get_at_path, JsonPatch, JsonPath, JsonValue};
use strata_core::traits::{SnapshotView, Storage};
use strata_core::types::{Key, RunId};
use strata_core::value::Value;
use strata_core::StrataError;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::time::{Duration, Instant};

/// Error type for commit failures
///
/// Per spec Core Invariants:
/// - All-or-nothing commit: transaction either commits or aborts entirely
/// - First-committer-wins: conflicts are detected based on read-set
#[derive(Debug, Clone)]
pub enum CommitError {
    /// Transaction aborted due to validation conflicts
    ///
    /// Per spec Section 3: Conflicts detected in read-set or CAS-set
    ValidationFailed(ValidationResult),

    /// Transaction was not in correct state for commit
    ///
    /// Commit requires Active state to transition to Validating
    InvalidState(String),

    /// WAL write failed during commit
    ///
    /// Per spec Section 5: WAL must be written before storage for durability.
    /// If WAL write fails, the transaction cannot be durably committed.
    WALError(String),
}

impl std::fmt::Display for CommitError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CommitError::ValidationFailed(result) => {
                write!(f, "Commit failed: {} conflict(s)", result.conflict_count())
            }
            CommitError::InvalidState(msg) => write!(f, "Invalid state: {}", msg),
            CommitError::WALError(msg) => write!(f, "WAL error: {}", msg),
        }
    }
}

impl std::error::Error for CommitError {}

// Conversion to StrataError
impl From<CommitError> for StrataError {
    fn from(e: CommitError) -> Self {
        match e {
            CommitError::ValidationFailed(result) => StrataError::TransactionAborted {
                reason: format!("Validation failed: {} conflict(s)", result.conflict_count()),
            },
            CommitError::InvalidState(msg) => StrataError::TransactionNotActive { state: msg },
            CommitError::WALError(msg) => StrataError::Storage {
                message: format!("WAL error: {}", msg),
                source: None,
            },
        }
    }
}

/// Result of applying transaction writes to storage
///
/// Per spec Section 6.1: All keys in a transaction get the same commit version.
#[derive(Debug, Clone)]
pub struct ApplyResult {
    /// Version assigned to all writes in this transaction
    pub commit_version: u64,
    /// Number of puts applied
    pub puts_applied: usize,
    /// Number of deletes applied
    pub deletes_applied: usize,
    /// Number of CAS operations applied
    pub cas_applied: usize,
}

impl ApplyResult {
    /// Total number of operations applied
    pub fn total_operations(&self) -> usize {
        self.puts_applied + self.deletes_applied + self.cas_applied
    }
}

/// Summary of pending operations that would be rolled back on abort
///
/// This is useful for debugging, logging, or providing feedback before
/// aborting a transaction. It shows what operations are buffered and
/// would be discarded if the transaction were aborted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PendingOperations {
    /// Number of pending put operations
    pub puts: usize,
    /// Number of pending delete operations
    pub deletes: usize,
    /// Number of pending CAS operations
    pub cas: usize,
}

impl PendingOperations {
    /// Total number of pending operations
    pub fn total(&self) -> usize {
        self.puts + self.deletes + self.cas
    }

    /// Check if there are no pending operations
    pub fn is_empty(&self) -> bool {
        self.total() == 0
    }
}

/// Status of a transaction in its lifecycle
///
/// State transitions:
/// - `Active` → `Validating` (begin commit)
/// - `Validating` → `Committed` (validation passed)
/// - `Validating` → `Aborted` (conflict detected)
/// - `Active` → `Aborted` (user abort or error)
///
/// Terminal states (no transitions allowed):
/// - `Committed`
/// - `Aborted`
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransactionStatus {
    /// Transaction is executing, can read/write
    Active,
    /// Transaction is being validated for conflicts
    Validating,
    /// Transaction committed successfully
    Committed,
    /// Transaction was aborted
    Aborted {
        /// Human-readable reason for abort
        reason: String,
    },
}

/// A compare-and-swap operation to be validated at commit
///
/// CAS operations are buffered until commit time. At commit:
/// 1. Validate that the key's current version equals `expected_version`
/// 2. If valid, write `new_value`
/// 3. If invalid, abort the transaction
///
/// Note: CAS does NOT automatically add to read_set. If you want read-set
/// protection in addition to CAS validation, explicitly read the key first.
#[derive(Debug, Clone)]
pub struct CASOperation {
    /// Key to CAS
    pub key: Key,
    /// Expected version (0 = key must not exist)
    pub expected_version: u64,
    /// New value to write if version matches
    pub new_value: Value,
}

// ============================================================================
// JSON Transaction Types (M5 Epic 30)
// ============================================================================

/// Record of a JSON path read (for conflict detection)
///
/// Tracks which paths within JSON documents were read during a transaction.
/// Used for fine-grained conflict detection at commit time.
#[derive(Debug, Clone)]
pub struct JsonPathRead {
    /// Key of the JSON document
    pub key: Key,
    /// Path that was read
    pub path: JsonPath,
    /// Version of the document when read
    pub version: u64,
}

impl JsonPathRead {
    /// Create a new JSON path read record
    pub fn new(key: Key, path: JsonPath, version: u64) -> Self {
        Self { key, path, version }
    }
}

/// Record of a JSON patch operation (for commit)
///
/// Stores a patch to be applied to a JSON document at commit time.
/// Patches are applied in order to compute the final document state.
#[derive(Debug, Clone)]
pub struct JsonPatchEntry {
    /// Key of the JSON document
    pub key: Key,
    /// Patch to apply
    pub patch: JsonPatch,
    /// Version the document will have after this patch
    pub resulting_version: u64,
}

impl JsonPatchEntry {
    /// Create a new JSON patch entry
    pub fn new(key: Key, patch: JsonPatch, resulting_version: u64) -> Self {
        Self {
            key,
            patch,
            resulting_version,
        }
    }
}

// ============================================================================
// JsonStoreExt Trait (M5 Epic 30)
// ============================================================================

/// Extension trait for JSON operations within transactions (M5 Rule 3)
///
/// This trait enables JSON operations to be performed within a TransactionContext,
/// allowing atomic cross-primitive transactions between JSON and other primitives.
///
/// # Architecture (M5 Architecture Rule 3)
///
/// Per M5 Rule 3: "Add `JsonStoreExt` trait to TransactionContext. NO separate
/// JsonTransaction type." This enables cross-primitive atomic transactions
/// without additional coordination.
///
/// # Usage
///
/// ```ignore
/// db.transaction(run_id, |txn| {
///     // JSON operation
///     let value = txn.json_get(&key, &path)?;
///     txn.json_set(&key, &path, json!({"updated": true}))?;
///
///     // KV operation in same transaction
///     txn.put(other_key, Value::Bytes(b"done".to_vec()))?;
///
///     Ok(())
/// })?;
/// ```
///
/// # Read-Your-Writes
///
/// JSON operations support read-your-writes semantics:
/// - `json_set` writes are visible to subsequent `json_get` calls in the same transaction
/// - Writes are buffered until commit
///
/// # Conflict Detection
///
/// JSON reads and writes are tracked for region-based conflict detection:
/// - All reads track the document version at read time
/// - At commit time, if any read document's version has changed, conflict is detected
pub trait JsonStoreExt {
    /// Get a value at a JSON path within a document
    ///
    /// # Arguments
    /// * `key` - Key of the JSON document
    /// * `path` - JSON path to read from
    ///
    /// # Returns
    /// - `Ok(Some(value))` if path exists
    /// - `Ok(None)` if document exists but path doesn't
    /// - `Err` if document doesn't exist or transaction is invalid
    fn json_get(&mut self, key: &Key, path: &JsonPath) -> Result<Option<JsonValue>>;

    /// Set a value at a JSON path within a document
    ///
    /// # Arguments
    /// * `key` - Key of the JSON document
    /// * `path` - JSON path to write to
    /// * `value` - Value to set
    ///
    /// # Returns
    /// - `Ok(())` on success
    /// - `Err` if document doesn't exist or transaction is invalid
    fn json_set(&mut self, key: &Key, path: &JsonPath, value: JsonValue) -> Result<()>;

    /// Delete a value at a JSON path within a document
    ///
    /// # Arguments
    /// * `key` - Key of the JSON document
    /// * `path` - JSON path to delete
    ///
    /// # Returns
    /// - `Ok(())` on success (even if path didn't exist)
    /// - `Err` if document doesn't exist or transaction is invalid
    fn json_delete(&mut self, key: &Key, path: &JsonPath) -> Result<()>;

    /// Get the entire JSON document
    ///
    /// # Arguments
    /// * `key` - Key of the JSON document
    ///
    /// # Returns
    /// - `Ok(Some(value))` if document exists
    /// - `Ok(None)` if document doesn't exist
    fn json_get_document(&mut self, key: &Key) -> Result<Option<JsonValue>>;

    /// Check if a JSON document exists
    ///
    /// # Arguments
    /// * `key` - Key of the JSON document
    fn json_exists(&mut self, key: &Key) -> Result<bool>;
}

/// Transaction context for OCC with snapshot isolation
///
/// Tracks all reads, writes, deletes, and CAS operations for a transaction.
/// Validation and commit happen at transaction end.
///
/// # Read-Your-Writes Semantics
///
/// When reading a key, the transaction checks in order:
/// 1. **write_set**: Returns uncommitted write from this transaction
/// 2. **delete_set**: Returns None for uncommitted delete
/// 3. **snapshot**: Returns value from snapshot, tracks in read_set
///
/// # Read-Set Tracking
///
/// All reads from the snapshot are tracked in `read_set` with the version read.
/// At commit time, these versions are validated against current storage.
/// If any version changed, the transaction has a read-write conflict.
///
/// # Lifecycle
///
/// 1. **BEGIN**: Create with `with_snapshot()`, status is `Active`
/// 2. **READ/WRITE**: Use `get()`, `put()`, `delete()`, `cas()`
/// 3. **VALIDATE**: Call `mark_validating()`, check for conflicts
/// 4. **COMMIT/ABORT**: Call `mark_committed()` or `mark_aborted()`
pub struct TransactionContext {
    // Identity
    /// Unique transaction ID
    pub txn_id: u64,
    /// Run this transaction belongs to
    pub run_id: RunId,

    // Snapshot isolation
    /// Version at transaction start (snapshot version)
    ///
    /// All reads see data as of this version. Used for conflict detection.
    pub start_version: u64,

    /// Snapshot view for this transaction
    ///
    /// Provides consistent point-in-time view of storage.
    snapshot: Option<Box<dyn SnapshotView>>,

    // Operation tracking
    /// Keys read and their versions (for validation)
    ///
    /// At commit time, we check that each key's current version still matches
    /// the version we read. If not, there's a read-write conflict.
    ///
    /// Version 0 means the key did not exist when read.
    pub read_set: HashMap<Key, u64>,

    /// Keys written with their new values (buffered)
    ///
    /// These writes are not visible to other transactions until commit.
    /// At commit, they are applied atomically to storage.
    pub write_set: HashMap<Key, Value>,

    /// Keys to delete (buffered)
    ///
    /// Deletes are buffered like writes. A deleted key returns None
    /// when read within this transaction (read-your-deletes).
    pub delete_set: HashSet<Key>,

    /// CAS operations to validate and apply
    ///
    /// Each CAS is validated at commit time against the current storage
    /// version, independent of the read_set.
    pub cas_set: Vec<CASOperation>,

    // JSON Operations (M5 - lazy allocation for zero overhead when not using JSON)
    /// JSON path reads for fine-grained conflict detection
    ///
    /// Only allocated when JSON operations are performed.
    json_reads: Option<Vec<JsonPathRead>>,

    /// JSON patches to apply at commit
    ///
    /// Only allocated when JSON operations are performed.
    json_writes: Option<Vec<JsonPatchEntry>>,

    /// Snapshot versions of JSON documents at read time
    ///
    /// Maps document key to the version observed during read.
    /// Only allocated when JSON operations are performed.
    json_snapshot_versions: Option<HashMap<Key, u64>>,

    // State
    /// Current transaction status
    pub status: TransactionStatus,

    // Timing
    /// When this transaction was created
    start_time: Instant,
}

impl TransactionContext {
    /// Create a new transaction context without a snapshot
    ///
    /// This constructor is primarily for testing or for transactions
    /// that don't need to read from storage.
    ///
    /// For normal transactions, use `with_snapshot()`.
    ///
    /// # Arguments
    /// * `txn_id` - Unique transaction identifier
    /// * `run_id` - Run this transaction belongs to
    /// * `start_version` - Snapshot version at transaction start
    ///
    /// # Example
    ///
    /// ```
    /// use strata_concurrency::TransactionContext;
    /// use strata_core::types::RunId;
    ///
    /// let run_id = RunId::new();
    /// let txn = TransactionContext::new(1, run_id, 100);
    /// assert!(txn.is_active());
    /// ```
    pub fn new(txn_id: u64, run_id: RunId, start_version: u64) -> Self {
        TransactionContext {
            txn_id,
            run_id,
            start_version,
            snapshot: None,
            read_set: HashMap::new(),
            write_set: HashMap::new(),
            delete_set: HashSet::new(),
            cas_set: Vec::new(),
            json_reads: None,
            json_writes: None,
            json_snapshot_versions: None,
            status: TransactionStatus::Active,
            start_time: Instant::now(),
        }
    }

    /// Create a new transaction context with a snapshot
    ///
    /// This is the primary constructor for transactions that need to read
    /// from storage. The snapshot provides a consistent point-in-time view.
    ///
    /// # Arguments
    /// * `txn_id` - Unique transaction identifier
    /// * `run_id` - Run this transaction belongs to
    /// * `snapshot` - Snapshot view for this transaction
    ///
    /// # Example
    ///
    /// ```
    /// use strata_concurrency::{TransactionContext, ClonedSnapshotView};
    /// use strata_core::types::RunId;
    /// use std::collections::BTreeMap;
    ///
    /// let run_id = RunId::new();
    /// let snapshot = Box::new(ClonedSnapshotView::empty(100));
    /// let txn = TransactionContext::with_snapshot(1, run_id, snapshot);
    /// assert!(txn.is_active());
    /// assert_eq!(txn.start_version, 100);
    /// ```
    pub fn with_snapshot(txn_id: u64, run_id: RunId, snapshot: Box<dyn SnapshotView>) -> Self {
        let start_version = snapshot.version();
        TransactionContext {
            txn_id,
            run_id,
            start_version,
            snapshot: Some(snapshot),
            read_set: HashMap::new(),
            write_set: HashMap::new(),
            delete_set: HashSet::new(),
            cas_set: Vec::new(),
            json_reads: None,
            json_writes: None,
            json_snapshot_versions: None,
            status: TransactionStatus::Active,
            start_time: Instant::now(),
        }
    }

    // === Read Operations ===

    /// Get a value from the transaction
    ///
    /// Implements read-your-writes semantics:
    /// 1. Check write_set (uncommitted writes from this txn) - NO read_set entry
    /// 2. Check delete_set (uncommitted deletes from this txn) - NO read_set entry
    /// 3. Read from snapshot - tracks in read_set
    ///
    /// # Read-Set Tracking
    ///
    /// Only reads from the snapshot are tracked in read_set:
    /// - If key exists in snapshot: tracks `(key, version)`
    /// - If key doesn't exist in snapshot: tracks `(key, 0)`
    ///
    /// # Errors
    /// Returns `Error::InvalidState` if transaction is not active.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let value = txn.get(&key)?;
    /// if let Some(v) = value {
    ///     // Process value
    /// }
    /// ```
    pub fn get(&mut self, key: &Key) -> Result<Option<Value>> {
        self.ensure_active()?;

        // 1. Check write_set first (read-your-writes)
        // No read_set entry - we're reading our own uncommitted write
        if let Some(value) = self.write_set.get(key) {
            return Ok(Some(value.clone()));
        }

        // 2. Check delete_set (return None if deleted in this txn)
        // No read_set entry - we're reading our own uncommitted delete
        if self.delete_set.contains(key) {
            return Ok(None);
        }

        // 3. Read from snapshot
        self.read_from_snapshot(key)
    }

    /// Read from snapshot and track in read_set
    ///
    /// This is the core read path that tracks reads for conflict detection.
    fn read_from_snapshot(&mut self, key: &Key) -> Result<Option<Value>> {
        let snapshot = self.snapshot.as_ref().ok_or_else(|| {
            Error::InvalidState("Transaction has no snapshot for reads".to_string())
        })?;

        let versioned = snapshot.get(key)?;

        // Track in read_set for conflict detection
        if let Some(ref vv) = versioned {
            // Key exists - track its version (as u64 for comparison)
            self.read_set.insert(key.clone(), vv.version.as_u64());
            Ok(Some(vv.value.clone()))
        } else {
            // Key doesn't exist - track with version 0
            // This is important: if someone creates this key before we commit,
            // we have a conflict (we assumed it didn't exist)
            self.read_set.insert(key.clone(), 0);
            Ok(None)
        }
    }

    /// Check if a key exists in the transaction's view
    ///
    /// This is a convenience method that calls `get()` and checks for Some.
    /// Note: This DOES track the read in read_set if the key is read from snapshot.
    ///
    /// # Errors
    /// Returns `Error::InvalidState` if transaction is not active.
    pub fn exists(&mut self, key: &Key) -> Result<bool> {
        Ok(self.get(key)?.is_some())
    }

    /// Scan keys with a prefix
    ///
    /// Returns all keys matching the prefix, implementing read-your-writes:
    /// - Includes uncommitted writes from this transaction matching prefix
    /// - Excludes uncommitted deletes from this transaction
    /// - Tracks all scanned keys from snapshot in read_set
    ///
    /// Results are sorted by key order.
    ///
    /// # Errors
    /// Returns `Error::InvalidState` if transaction is not active or has no snapshot.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let prefix = Key::new_kv(namespace, "user:");
    /// let users = txn.scan_prefix(&prefix)?;
    /// for (key, value) in users {
    ///     // Process each user
    /// }
    /// ```
    pub fn scan_prefix(&mut self, prefix: &Key) -> Result<Vec<(Key, Value)>> {
        self.ensure_active()?;

        let snapshot = self.snapshot.as_ref().ok_or_else(|| {
            Error::InvalidState("Transaction has no snapshot for reads".to_string())
        })?;

        // Get all matching keys from snapshot
        let snapshot_results = snapshot.scan_prefix(prefix)?;

        // Build result set with read-your-writes using BTreeMap for sorted output
        let mut results: BTreeMap<Key, Value> = BTreeMap::new();

        // Add snapshot results (excluding deleted keys, tracking in read_set)
        for (key, vv) in snapshot_results {
            if !self.delete_set.contains(&key) {
                // Track in read_set (as u64 for comparison)
                self.read_set.insert(key.clone(), vv.version.as_u64());
                results.insert(key, vv.value);
            }
            // Note: Deleted keys are NOT tracked in read_set from scan
            // because we're not "reading" them - they're excluded from results
        }

        // Add/overwrite with write_set entries matching prefix
        for (key, value) in &self.write_set {
            if key.starts_with(prefix) {
                // Write_set entries are NOT tracked in read_set
                // (they're our own uncommitted writes)
                results.insert(key.clone(), value.clone());
            }
        }

        Ok(results.into_iter().collect())
    }

    /// Get the version that was read for a key (from read_set)
    ///
    /// Returns None if the key hasn't been read from snapshot.
    /// Returns Some(0) if the key was read but didn't exist.
    pub fn get_read_version(&self, key: &Key) -> Option<u64> {
        self.read_set.get(key).copied()
    }

    // === Write Operations ===

    /// Buffer a write operation
    ///
    /// The write is NOT applied to storage until commit.
    /// Other transactions will NOT see this write (OCC isolation).
    ///
    /// # Semantics
    /// - If the key was previously deleted in this txn, remove from delete_set
    /// - Add/overwrite in write_set (latest value wins)
    /// - Writes are "blind" - no read_set entry unless you explicitly read first
    ///
    /// # Errors
    /// Returns `Error::InvalidState` if transaction is not active.
    ///
    /// # Example
    ///
    /// ```ignore
    /// txn.put(key, Value::Bytes(b"value".to_vec()))?;
    /// // Value is NOT visible to other transactions yet
    /// // Will be visible after successful commit
    /// ```
    pub fn put(&mut self, key: Key, value: Value) -> Result<()> {
        self.ensure_active()?;

        // Remove from delete_set if previously deleted in this txn
        self.delete_set.remove(&key);

        // Add to write_set (overwrites any previous write to same key)
        self.write_set.insert(key, value);
        Ok(())
    }

    /// Buffer a delete operation
    ///
    /// The delete is NOT applied to storage until commit.
    /// Other transactions will NOT see this delete (OCC isolation).
    ///
    /// # Semantics
    /// - If the key was previously written in this txn, remove from write_set
    /// - Add to delete_set
    /// - At commit, creates a tombstone in storage
    ///
    /// # Errors
    /// Returns `Error::InvalidState` if transaction is not active.
    ///
    /// # Example
    ///
    /// ```ignore
    /// txn.delete(key)?;
    /// // Key is NOT deleted from storage yet
    /// // Will be deleted after successful commit
    /// // Reading this key within this txn returns None (read-your-deletes)
    /// ```
    pub fn delete(&mut self, key: Key) -> Result<()> {
        self.ensure_active()?;

        // Remove from write_set if previously written in this txn
        self.write_set.remove(&key);

        // Add to delete_set
        self.delete_set.insert(key);
        Ok(())
    }

    /// Buffer a compare-and-swap operation
    ///
    /// CAS operations are validated at COMMIT time, not call time.
    /// This allows multiple CAS operations to be batched in a single transaction.
    ///
    /// # Semantics
    /// - `expected_version = 0` means "key must not exist"
    /// - `expected_version = N` means "key must be at version N"
    /// - CAS does NOT automatically add to read_set
    /// - If you need read-set protection, explicitly read the key first
    /// - Multiple CAS operations on different keys are allowed
    ///
    /// # Errors
    /// Returns `Error::InvalidState` if transaction is not active.
    ///
    /// # Example
    ///
    /// ```ignore
    /// // Create key only if it doesn't exist (expected_version = 0)
    /// txn.cas(key, 0, Value::Bytes(b"initial".to_vec()))?;
    ///
    /// // Update key only if at version 5
    /// txn.cas(other_key, 5, Value::Bytes(b"updated".to_vec()))?;
    /// ```
    pub fn cas(&mut self, key: Key, expected_version: u64, new_value: Value) -> Result<()> {
        self.ensure_active()?;

        self.cas_set.push(CASOperation {
            key,
            expected_version,
            new_value,
        });
        Ok(())
    }

    // === JSON Operations (M5 Epic 30) ===

    /// Check if this transaction has any JSON operations
    ///
    /// Returns true if any JSON reads, writes, or snapshot versions are recorded.
    /// Useful for determining if JSON-specific validation is needed.
    pub fn has_json_ops(&self) -> bool {
        self.json_reads.is_some()
            || self.json_writes.is_some()
            || self.json_snapshot_versions.is_some()
    }

    /// Get JSON path reads (immutable)
    ///
    /// Returns an empty slice if no JSON reads have been recorded.
    pub fn json_reads(&self) -> &[JsonPathRead] {
        self.json_reads.as_deref().unwrap_or(&[])
    }

    /// Get JSON patch writes (immutable)
    ///
    /// Returns an empty slice if no JSON writes have been recorded.
    pub fn json_writes(&self) -> &[JsonPatchEntry] {
        self.json_writes.as_deref().unwrap_or(&[])
    }

    /// Get JSON snapshot versions (immutable)
    ///
    /// Returns None if no JSON snapshot versions have been recorded.
    pub fn json_snapshot_versions(&self) -> Option<&HashMap<Key, u64>> {
        self.json_snapshot_versions.as_ref()
    }

    /// Ensure json_reads is initialized and return mutable reference
    ///
    /// Lazily allocates the Vec on first use.
    pub fn ensure_json_reads(&mut self) -> &mut Vec<JsonPathRead> {
        self.json_reads.get_or_insert_with(Vec::new)
    }

    /// Ensure json_writes is initialized and return mutable reference
    ///
    /// Lazily allocates the Vec on first use.
    pub fn ensure_json_writes(&mut self) -> &mut Vec<JsonPatchEntry> {
        self.json_writes.get_or_insert_with(Vec::new)
    }

    /// Ensure json_snapshot_versions is initialized and return mutable reference
    ///
    /// Lazily allocates the HashMap on first use.
    pub fn ensure_json_snapshot_versions(&mut self) -> &mut HashMap<Key, u64> {
        self.json_snapshot_versions.get_or_insert_with(HashMap::new)
    }

    /// Record a JSON path read for conflict detection
    ///
    /// This should be called when reading a specific path from a JSON document.
    /// The read will be validated at commit time to detect conflicts.
    pub fn record_json_read(&mut self, key: Key, path: JsonPath, version: u64) {
        self.ensure_json_reads()
            .push(JsonPathRead::new(key, path, version));
    }

    /// Record a JSON patch for commit
    ///
    /// This should be called when modifying a JSON document via patch.
    /// The patch will be applied at commit time.
    pub fn record_json_write(&mut self, key: Key, patch: JsonPatch, resulting_version: u64) {
        self.ensure_json_writes()
            .push(JsonPatchEntry::new(key, patch, resulting_version));
    }

    /// Record JSON document snapshot version
    ///
    /// Tracks the version of a JSON document when it was first read.
    /// Used for document-level conflict detection.
    pub fn record_json_snapshot_version(&mut self, key: Key, version: u64) {
        self.ensure_json_snapshot_versions().insert(key, version);
    }

    /// Clear all buffered operations
    ///
    /// This is useful for retry scenarios where you want to restart
    /// a transaction's operations without creating a new transaction.
    ///
    /// Clears: read_set, write_set, delete_set, cas_set, and all JSON operation sets
    ///
    /// Note: Does not change transaction state or snapshot.
    ///
    /// # Errors
    /// Returns `Error::InvalidState` if transaction is not active.
    pub fn clear_operations(&mut self) -> Result<()> {
        self.ensure_active()?;

        self.read_set.clear();
        self.write_set.clear();
        self.delete_set.clear();
        self.cas_set.clear();
        // Clear JSON sets (set to None to deallocate)
        self.json_reads = None;
        self.json_writes = None;
        self.json_snapshot_versions = None;
        Ok(())
    }

    // === State Management ===

    /// Check if transaction is in Active state
    ///
    /// Only active transactions can accept new read/write operations.
    pub fn is_active(&self) -> bool {
        matches!(self.status, TransactionStatus::Active)
    }

    /// Check if transaction is committed
    pub fn is_committed(&self) -> bool {
        matches!(self.status, TransactionStatus::Committed)
    }

    /// Check if transaction is aborted
    pub fn is_aborted(&self) -> bool {
        matches!(self.status, TransactionStatus::Aborted { .. })
    }

    /// Check if transaction can be rolled back
    ///
    /// A transaction can be rolled back if it's in Active or Validating state.
    /// Once committed or aborted, rollback is not possible.
    pub fn can_rollback(&self) -> bool {
        matches!(
            self.status,
            TransactionStatus::Active | TransactionStatus::Validating
        )
    }

    // === Timeout Support ===

    /// Check if this transaction has exceeded the given timeout
    ///
    /// Returns true if the elapsed time since transaction creation
    /// exceeds the specified timeout duration.
    ///
    /// # Arguments
    /// * `timeout` - Maximum allowed duration for this transaction
    ///
    /// # Example
    /// ```
    /// use strata_concurrency::TransactionContext;
    /// use strata_core::types::RunId;
    /// use std::time::Duration;
    ///
    /// let run_id = RunId::new();
    /// let txn = TransactionContext::new(1, run_id, 100);
    ///
    /// // Should not be expired immediately
    /// assert!(!txn.is_expired(Duration::from_secs(1)));
    /// ```
    pub fn is_expired(&self, timeout: Duration) -> bool {
        self.start_time.elapsed() > timeout
    }

    /// Get the elapsed time since transaction started
    ///
    /// Returns the duration since this transaction was created.
    ///
    /// # Example
    /// ```
    /// use strata_concurrency::TransactionContext;
    /// use strata_core::types::RunId;
    /// use std::time::Duration;
    ///
    /// let run_id = RunId::new();
    /// let txn = TransactionContext::new(1, run_id, 100);
    ///
    /// // Elapsed should be very small initially
    /// assert!(txn.elapsed() < Duration::from_secs(1));
    /// ```
    pub fn elapsed(&self) -> Duration {
        self.start_time.elapsed()
    }

    /// Check if transaction can accept operations
    ///
    /// # Errors
    /// Returns `Error::InvalidState` if transaction is not in `Active` state.
    pub fn ensure_active(&self) -> Result<()> {
        if self.is_active() {
            Ok(())
        } else {
            Err(Error::InvalidState(format!(
                "Transaction {} is not active: {:?}",
                self.txn_id, self.status
            )))
        }
    }

    /// Transition to Validating state
    ///
    /// This is the first step of the commit process. After marking validating,
    /// the transaction should be validated against current storage state.
    ///
    /// # Errors
    /// Returns `Error::InvalidState` if not in `Active` state.
    ///
    /// # State Transition
    /// `Active` → `Validating`
    pub fn mark_validating(&mut self) -> Result<()> {
        self.ensure_active()?;
        self.status = TransactionStatus::Validating;
        Ok(())
    }

    /// Transition to Committed state
    ///
    /// Called after successful validation. The transaction's writes have been
    /// applied to storage.
    ///
    /// # Errors
    /// Returns `Error::InvalidState` if not in `Validating` state.
    ///
    /// # State Transition
    /// `Validating` → `Committed`
    pub fn mark_committed(&mut self) -> Result<()> {
        match &self.status {
            TransactionStatus::Validating => {
                self.status = TransactionStatus::Committed;
                Ok(())
            }
            _ => Err(Error::InvalidState(format!(
                "Cannot commit transaction {} from state {:?}",
                self.txn_id, self.status
            ))),
        }
    }

    /// Abort the transaction and clean up
    ///
    /// Per spec:
    /// - Aborted transactions write nothing to storage
    /// - Aborted transactions write nothing to WAL
    /// - All buffered operations are discarded
    ///
    /// Can be called from `Active` (user abort) or `Validating` (conflict detected).
    ///
    /// # Arguments
    /// * `reason` - Human-readable reason for abort
    ///
    /// # Errors
    /// Returns `Error::InvalidState` if already `Committed` or `Aborted`.
    ///
    /// # State Transitions
    /// - `Active` → `Aborted`
    /// - `Validating` → `Aborted`
    pub fn mark_aborted(&mut self, reason: String) -> Result<()> {
        match &self.status {
            TransactionStatus::Committed => Err(Error::InvalidState(format!(
                "Cannot abort committed transaction {}",
                self.txn_id
            ))),
            TransactionStatus::Aborted { .. } => Err(Error::InvalidState(format!(
                "Transaction {} already aborted",
                self.txn_id
            ))),
            _ => {
                self.status = TransactionStatus::Aborted { reason };

                // Clear all buffered operations per spec
                // Aborted transactions write nothing
                self.write_set.clear();
                self.delete_set.clear();
                self.cas_set.clear();

                // Note: read_set is kept for debugging/diagnostics

                Ok(())
            }
        }
    }

    /// Get summary of pending operations
    ///
    /// Useful for debugging and logging before abort/commit.
    /// Returns counts of buffered operations that would be applied on commit
    /// or discarded on abort.
    pub fn pending_operations(&self) -> PendingOperations {
        PendingOperations {
            puts: self.write_set.len(),
            deletes: self.delete_set.len(),
            cas: self.cas_set.len(),
        }
    }

    // === Commit Operation ===

    /// Commit the transaction
    ///
    /// Per spec Section 3 and Core Invariants:
    /// 1. Transition to Validating state
    /// 2. Run validation against current storage
    /// 3. If valid: transition to Committed
    /// 4. If invalid: transition to Aborted
    ///
    /// # Arguments
    /// * `store` - Storage to validate against
    ///
    /// # Returns
    /// - Ok(()) if transaction committed successfully
    /// - Err(CommitError::ValidationFailed) if transaction aborted due to conflicts
    /// - Err(CommitError::InvalidState) if not in Active state
    ///
    /// # Note
    /// This method performs validation and state transitions only.
    /// Actual write application is handled separately in .
    /// Full atomic commit with WAL is implemented in .
    ///
    /// # Spec Reference
    /// - Section 3.1: When conflicts occur
    /// - Section 3.3: First-committer-wins rule
    /// - Core Invariants: All-or-nothing commit
    pub fn commit<S: Storage>(&mut self, store: &S) -> std::result::Result<(), CommitError> {
        // Step 1: Transition to Validating
        if !self.is_active() {
            return Err(CommitError::InvalidState(format!(
                "Cannot commit transaction {} from {:?} state - must be Active",
                self.txn_id, self.status
            )));
        }
        self.status = TransactionStatus::Validating;

        // Step 2: Validate against current storage state
        let validation_result = validate_transaction(self, store);

        if !validation_result.is_valid() {
            // Step 3a: Validation failed - abort
            let conflict_count = validation_result.conflict_count();
            self.status = TransactionStatus::Aborted {
                reason: format!("Commit failed: {} conflict(s) detected", conflict_count),
            };
            return Err(CommitError::ValidationFailed(validation_result));
        }

        // Step 3b: Validation passed - mark committed
        self.status = TransactionStatus::Committed;

        Ok(())
    }

    /// Apply all buffered writes to storage
    ///
    /// Per spec Section 6.1:
    /// - Global version incremented ONCE for the whole transaction
    /// - All keys in this transaction get the same commit version
    ///
    /// Per spec Section 6.5:
    /// - Deletes create tombstones with the commit version
    ///
    /// # Arguments
    /// * `store` - Storage to apply writes to
    /// * `commit_version` - Version to assign to all writes
    ///
    /// # Returns
    /// ApplyResult with counts of applied operations
    ///
    /// # Preconditions
    /// - Transaction must be in Committed state (validation passed)
    ///
    /// # Errors
    /// - Error::InvalidState if transaction is not in Committed state
    /// - Error from storage operations if they fail
    pub fn apply_writes<S: Storage>(&self, store: &S, commit_version: u64) -> Result<ApplyResult> {
        if !self.is_committed() {
            return Err(Error::InvalidState(format!(
                "Cannot apply writes: transaction {} is {:?}, must be Committed",
                self.txn_id, self.status
            )));
        }

        let mut result = ApplyResult {
            commit_version,
            puts_applied: 0,
            deletes_applied: 0,
            cas_applied: 0,
        };

        // Apply puts from write_set
        for (key, value) in &self.write_set {
            store.put_with_version(key.clone(), value.clone(), commit_version, None)?;
            result.puts_applied += 1;
        }

        // Apply deletes from delete_set
        for key in &self.delete_set {
            store.delete_with_version(key, commit_version)?;
            result.deletes_applied += 1;
        }

        // Apply CAS operations from cas_set
        // Note: CAS validation already passed in commit(), so we just apply the new values
        for cas_op in &self.cas_set {
            store.put_with_version(
                cas_op.key.clone(),
                cas_op.new_value.clone(),
                commit_version,
                None,
            )?;
            result.cas_applied += 1;
        }

        Ok(result)
    }

    /// Write all transaction operations to WAL
    ///
    /// Per spec Section 5:
    /// - Write/Delete entries for all buffered operations
    /// - Version numbers are preserved exactly
    ///
    /// # Arguments
    /// * `wal_writer` - WAL writer configured for this transaction
    /// * `commit_version` - Version to assign to all writes
    ///
    /// # Preconditions
    /// - Transaction must be in Committed state (validation passed)
    ///
    /// # Errors
    /// - Error::InvalidState if transaction is not in Committed state
    /// - Errors from WAL write operations
    pub fn write_to_wal(
        &self,
        wal_writer: &mut TransactionWALWriter,
        commit_version: u64,
    ) -> Result<()> {
        if !self.is_committed() {
            return Err(Error::InvalidState(format!(
                "Cannot write to WAL: transaction {} is {:?}, must be Committed",
                self.txn_id, self.status
            )));
        }

        // Write puts
        for (key, value) in &self.write_set {
            wal_writer.write_put(key.clone(), value.clone(), commit_version)?;
        }

        // Write deletes
        for key in &self.delete_set {
            wal_writer.write_delete(key.clone(), commit_version)?;
        }

        // Write CAS operations (as puts with the new value)
        for cas_op in &self.cas_set {
            wal_writer.write_put(cas_op.key.clone(), cas_op.new_value.clone(), commit_version)?;
        }

        Ok(())
    }

    // === Introspection ===

    /// Get the number of keys in the read set
    pub fn read_count(&self) -> usize {
        self.read_set.len()
    }

    /// Get the number of keys in the write set
    pub fn write_count(&self) -> usize {
        self.write_set.len()
    }

    /// Get the number of keys in the delete set
    pub fn delete_count(&self) -> usize {
        self.delete_set.len()
    }

    /// Get the number of CAS operations
    pub fn cas_count(&self) -> usize {
        self.cas_set.len()
    }

    /// Check if transaction has any pending operations
    ///
    /// Returns true if there are buffered writes, deletes, or CAS operations
    /// that would need to be applied at commit.
    pub fn has_pending_operations(&self) -> bool {
        !self.write_set.is_empty() || !self.delete_set.is_empty() || !self.cas_set.is_empty()
    }

    /// Check if transaction is read-only
    ///
    /// A read-only transaction has reads but no writes, deletes, or CAS ops.
    /// Read-only transactions always commit successfully (no conflicts possible
    /// since they don't modify anything).
    pub fn is_read_only(&self) -> bool {
        self.write_set.is_empty() && self.delete_set.is_empty() && self.cas_set.is_empty()
    }

    /// Get the abort reason if transaction is aborted
    pub fn abort_reason(&self) -> Option<&str> {
        match &self.status {
            TransactionStatus::Aborted { reason } => Some(reason),
            _ => None,
        }
    }

    // ========================================================================
    // Pooling Support (M4 )
    // ========================================================================

    /// Reset context for reuse (M4 pooling optimization)
    ///
    /// Clears all transaction state without deallocating memory.
    /// HashMap::clear() and Vec::clear() preserve capacity, which is
    /// the key optimization for transaction pooling.
    ///
    /// After reset, the context is ready for a new transaction with:
    /// - New txn_id, run_id, start_version
    /// - New snapshot
    /// - Empty read_set, write_set, delete_set, cas_set (with preserved capacity)
    /// - Active status
    /// - Fresh start_time
    ///
    /// # Arguments
    ///
    /// * `txn_id` - New transaction ID
    /// * `run_id` - New run ID
    /// * `snapshot` - New snapshot view (optional for testing)
    ///
    /// # Example
    ///
    /// ```ignore
    /// let mut ctx = TransactionContext::new(1, run_id, 100);
    /// // ... use the context ...
    ///
    /// // Reset for reuse - capacity is preserved!
    /// ctx.reset(2, new_run_id, Some(new_snapshot));
    /// ```
    pub fn reset(&mut self, txn_id: u64, run_id: RunId, snapshot: Option<Box<dyn SnapshotView>>) {
        // Update identity
        self.txn_id = txn_id;
        self.run_id = run_id;

        // Update snapshot and version
        self.start_version = snapshot.as_ref().map(|s| s.version()).unwrap_or(0);
        self.snapshot = snapshot;

        // Clear collections but preserve capacity - this is the key optimization!
        // HashMap::clear() and HashSet::clear() keep the allocated buckets
        // Vec::clear() keeps the allocated buffer
        self.read_set.clear();
        self.write_set.clear();
        self.delete_set.clear();
        self.cas_set.clear();

        // Clear JSON fields (deallocate, since JSON ops are rare)
        self.json_reads = None;
        self.json_writes = None;
        self.json_snapshot_versions = None;

        // Reset state
        self.status = TransactionStatus::Active;
        self.start_time = Instant::now();
    }

    /// Get current capacity of internal collections (for debugging/testing)
    ///
    /// Returns (read_set_capacity, write_set_capacity, delete_set_capacity, cas_set_capacity).
    /// Used to verify that `reset()` preserves capacity.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let ctx = TransactionContext::new(1, run_id, 100);
    /// let (read_cap, write_cap, delete_cap, cas_cap) = ctx.capacity();
    /// ```
    pub fn capacity(&self) -> (usize, usize, usize, usize) {
        (
            self.read_set.capacity(),
            self.write_set.capacity(),
            self.delete_set.capacity(),
            self.cas_set.capacity(),
        )
    }
}

// ============================================================================
// JsonStoreExt Implementation (M5 Epic 30)
// ============================================================================

impl JsonStoreExt for TransactionContext {
    fn json_get(&mut self, key: &Key, path: &JsonPath) -> Result<Option<JsonValue>> {
        self.ensure_active()?;

        // Check write set first (read-your-writes)
        // Look for the most recent write that affects this path
        if let Some(writes) = &self.json_writes {
            for entry in writes.iter().rev() {
                if entry.key == *key {
                    // Check if the patch affects this path
                    match &entry.patch {
                        JsonPatch::Set {
                            path: set_path,
                            value,
                        } if set_path.is_ancestor_of(path) => {
                            // If set_path equals our path, return the value directly
                            if set_path == path {
                                return Ok(Some(value.clone()));
                            }
                            // Navigate into the written value using the relative path
                            // Build a relative path by skipping the set_path segments
                            let relative_segments: Vec<_> = path
                                .segments()
                                .iter()
                                .skip(set_path.len())
                                .cloned()
                                .collect();
                            let relative_path = JsonPath::from_segments(relative_segments);
                            return Ok(get_at_path(value, &relative_path).cloned());
                        }
                        JsonPatch::Delete { path: del_path } if del_path.is_ancestor_of(path) => {
                            // Path was deleted
                            return Ok(None);
                        }
                        _ => {}
                    }
                }
            }
        }

        // Read from snapshot
        let snapshot = self.snapshot.as_ref().ok_or_else(|| {
            Error::InvalidState("Transaction has no snapshot for reads".to_string())
        })?;

        // Get the document from snapshot
        let versioned = snapshot.get(key)?;
        let Some(vv) = versioned else {
            // Document doesn't exist
            return Ok(None);
        };

        // Track the document version for conflict detection (as u64 for comparison)
        self.record_json_snapshot_version(key.clone(), vv.version.as_u64());
        self.record_json_read(key.clone(), path.clone(), vv.version.as_u64());

        // Deserialize the document
        let doc_bytes = match &vv.value {
            Value::Bytes(b) => b,
            _ => {
                return Err(Error::InvalidOperation(
                    "Expected JSON document to be stored as bytes".to_string(),
                ))
            }
        };

        // Deserialize using MessagePack
        let doc_value: JsonValue = rmp_serde::from_slice(doc_bytes).map_err(|e| {
            Error::InvalidOperation(format!("Failed to deserialize JSON document: {}", e))
        })?;

        // Get value at path
        Ok(get_at_path(&doc_value, path).cloned())
    }

    fn json_set(&mut self, key: &Key, path: &JsonPath, value: JsonValue) -> Result<()> {
        self.ensure_active()?;

        // Ensure we have tracked the snapshot version for this document
        // (for conflict detection at commit time)
        if self
            .json_snapshot_versions()
            .map_or(true, |v| !v.contains_key(key))
        {
            // Try to get the document version from snapshot
            if let Some(snapshot) = &self.snapshot {
                if let Ok(Some(vv)) = snapshot.get(key) {
                    self.record_json_snapshot_version(key.clone(), vv.version.as_u64());
                }
            }
        }

        // Record the write
        let patch = JsonPatch::set_at(path.clone(), value);
        // We don't know the resulting version until commit, use 0 as placeholder
        self.record_json_write(key.clone(), patch, 0);

        Ok(())
    }

    fn json_delete(&mut self, key: &Key, path: &JsonPath) -> Result<()> {
        self.ensure_active()?;

        // Ensure we have tracked the snapshot version for this document
        if self
            .json_snapshot_versions()
            .map_or(true, |v| !v.contains_key(key))
        {
            if let Some(snapshot) = &self.snapshot {
                if let Ok(Some(vv)) = snapshot.get(key) {
                    self.record_json_snapshot_version(key.clone(), vv.version.as_u64());
                }
            }
        }

        // Record the delete
        let patch = JsonPatch::delete_at(path.clone());
        self.record_json_write(key.clone(), patch, 0);

        Ok(())
    }

    fn json_get_document(&mut self, key: &Key) -> Result<Option<JsonValue>> {
        // Get the root path
        let root = JsonPath::root();
        self.json_get(key, &root)
    }

    fn json_exists(&mut self, key: &Key) -> Result<bool> {
        self.ensure_active()?;

        // Check if document was deleted in this transaction
        // (We track document deletes in json_writes as well)
        // For now, check the snapshot
        let snapshot = self.snapshot.as_ref().ok_or_else(|| {
            Error::InvalidState("Transaction has no snapshot for reads".to_string())
        })?;

        Ok(snapshot.get(key)?.is_some())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::snapshot::ClonedSnapshotView;
    use strata_core::types::{Namespace, TypeTag};
    use strata_core::Version;
    use strata_core::VersionedValue;

    // === Test Helpers ===

    fn create_test_txn() -> TransactionContext {
        let run_id = RunId::new();
        TransactionContext::new(1, run_id, 100)
    }

    fn create_test_namespace() -> Namespace {
        Namespace::new(
            "tenant".to_string(),
            "app".to_string(),
            "agent".to_string(),
            RunId::new(),
        )
    }

    fn create_test_key(ns: &Namespace, user_key: &[u8]) -> Key {
        Key::new(ns.clone(), TypeTag::KV, user_key.to_vec())
    }

    fn create_versioned_value(data: &[u8], version: u64) -> VersionedValue {
        VersionedValue::new(Value::Bytes(data.to_vec()), Version::txn(version))
    }

    fn create_txn_with_test_data() -> (TransactionContext, Namespace, Key, Key, Key) {
        let ns = create_test_namespace();
        let key1 = create_test_key(&ns, b"key1");
        let key2 = create_test_key(&ns, b"key2");
        let key3 = create_test_key(&ns, b"other");

        let mut data = std::collections::BTreeMap::new();
        data.insert(key1.clone(), create_versioned_value(b"value1", 10));
        data.insert(key2.clone(), create_versioned_value(b"value2", 20));
        data.insert(key3.clone(), create_versioned_value(b"other_value", 30));

        let snapshot = Box::new(ClonedSnapshotView::new(100, data));
        let run_id = RunId::new();
        let txn = TransactionContext::with_snapshot(1, run_id, snapshot);

        (txn, ns, key1, key2, key3)
    }

    // === Construction Tests ===

    #[test]
    fn test_new_transaction_is_active() {
        let txn = create_test_txn();
        assert!(txn.is_active());
        assert!(!txn.is_committed());
        assert!(!txn.is_aborted());
        assert_eq!(txn.txn_id, 1);
        assert_eq!(txn.start_version, 100);
    }

    #[test]
    fn test_new_transaction_has_empty_sets() {
        let txn = create_test_txn();
        assert_eq!(txn.read_count(), 0);
        assert_eq!(txn.write_count(), 0);
        assert_eq!(txn.delete_count(), 0);
        assert_eq!(txn.cas_count(), 0);
        assert!(!txn.has_pending_operations());
        assert!(txn.is_read_only());
    }

    #[test]
    fn test_new_transaction_preserves_run_id() {
        let run_id = RunId::new();
        let txn = TransactionContext::new(42, run_id, 500);
        assert_eq!(txn.run_id, run_id);
        assert_eq!(txn.txn_id, 42);
        assert_eq!(txn.start_version, 500);
    }

    #[test]
    fn test_with_snapshot_sets_version() {
        let snapshot = Box::new(ClonedSnapshotView::empty(200));
        let run_id = RunId::new();
        let txn = TransactionContext::with_snapshot(1, run_id, snapshot);

        assert_eq!(txn.start_version, 200);
        assert!(txn.is_active());
    }

    // === Read Operation Tests ===

    #[test]
    fn test_get_from_snapshot() {
        let (mut txn, _, key1, _, _) = create_txn_with_test_data();

        let result = txn.get(&key1).unwrap();
        assert!(result.is_some());

        match result.unwrap() {
            Value::Bytes(data) => assert_eq!(data, b"value1"),
            _ => panic!("Expected Bytes value"),
        }
    }

    #[test]
    fn test_get_nonexistent_key() {
        let (mut txn, ns, _, _, _) = create_txn_with_test_data();
        let nonexistent = create_test_key(&ns, b"nonexistent");

        let result = txn.get(&nonexistent).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_get_tracks_in_read_set() {
        let (mut txn, _, key1, _, _) = create_txn_with_test_data();

        // Initially empty
        assert_eq!(txn.read_count(), 0);

        // Read a key
        let _ = txn.get(&key1).unwrap();

        // Now tracked
        assert_eq!(txn.read_count(), 1);
        assert_eq!(txn.get_read_version(&key1), Some(10)); // version from snapshot
    }

    #[test]
    fn test_get_tracks_nonexistent_with_version_zero() {
        let (mut txn, ns, _, _, _) = create_txn_with_test_data();
        let nonexistent = create_test_key(&ns, b"nonexistent");

        // Read non-existent key
        let result = txn.get(&nonexistent).unwrap();
        assert!(result.is_none());

        // Should be tracked with version 0
        assert_eq!(txn.read_count(), 1);
        assert_eq!(txn.get_read_version(&nonexistent), Some(0));
    }

    #[test]
    fn test_read_your_writes() {
        let (mut txn, ns, _, _, _) = create_txn_with_test_data();
        let new_key = create_test_key(&ns, b"new_key");

        // Write a new value
        txn.write_set
            .insert(new_key.clone(), Value::String("new_value".to_string()));

        // Read it back - should see uncommitted write
        let result = txn.get(&new_key).unwrap();
        assert!(result.is_some());
        assert_eq!(result.unwrap(), Value::String("new_value".to_string()));

        // Should NOT be in read_set (read-your-writes doesn't track)
        assert_eq!(txn.read_count(), 0);
    }

    #[test]
    fn test_read_your_writes_overwrites_snapshot() {
        let (mut txn, _, key1, _, _) = create_txn_with_test_data();

        // Overwrite existing key
        txn.write_set
            .insert(key1.clone(), Value::String("overwritten".to_string()));

        // Read it back - should see uncommitted write, not snapshot
        let result = txn.get(&key1).unwrap();
        assert_eq!(result.unwrap(), Value::String("overwritten".to_string()));

        // Should NOT be in read_set
        assert_eq!(txn.read_count(), 0);
    }

    #[test]
    fn test_read_deleted_key_returns_none() {
        let (mut txn, _, key1, _, _) = create_txn_with_test_data();

        // Delete a key that exists in snapshot
        txn.delete_set.insert(key1.clone());

        // Read it - should return None
        let result = txn.get(&key1).unwrap();
        assert!(result.is_none());

        // Should NOT be in read_set (read-your-deletes doesn't track)
        assert_eq!(txn.read_count(), 0);
    }

    #[test]
    fn test_read_priority_write_over_delete() {
        let (mut txn, _, key1, _, _) = create_txn_with_test_data();

        // Both delete and write same key (edge case)
        // Write should take priority (it's checked first)
        txn.delete_set.insert(key1.clone());
        txn.write_set
            .insert(key1.clone(), Value::String("written".to_string()));

        let result = txn.get(&key1).unwrap();
        assert_eq!(result.unwrap(), Value::String("written".to_string()));
    }

    #[test]
    fn test_exists_returns_true_for_existing() {
        let (mut txn, _, key1, _, _) = create_txn_with_test_data();
        assert!(txn.exists(&key1).unwrap());
    }

    #[test]
    fn test_exists_returns_false_for_nonexistent() {
        let (mut txn, ns, _, _, _) = create_txn_with_test_data();
        let nonexistent = create_test_key(&ns, b"nonexistent");
        assert!(!txn.exists(&nonexistent).unwrap());
    }

    #[test]
    fn test_exists_tracks_in_read_set() {
        let (mut txn, _, key1, _, _) = create_txn_with_test_data();

        let _ = txn.exists(&key1).unwrap();

        // Should be tracked
        assert_eq!(txn.read_count(), 1);
    }

    #[test]
    fn test_get_fails_when_not_active() {
        let (mut txn, _, key1, _, _) = create_txn_with_test_data();
        txn.mark_validating().unwrap();

        let result = txn.get(&key1);
        assert!(result.is_err());
    }

    #[test]
    fn test_get_fails_without_snapshot() {
        let mut txn = create_test_txn(); // No snapshot
        let ns = create_test_namespace();
        let key = create_test_key(&ns, b"key");

        let result = txn.get(&key);
        assert!(result.is_err());
    }

    // === Scan Prefix Tests ===

    #[test]
    fn test_scan_prefix_from_snapshot() {
        let (mut txn, ns, _, _, _) = create_txn_with_test_data();
        let prefix = create_test_key(&ns, b"key");

        let results = txn.scan_prefix(&prefix).unwrap();

        // Should find key1 and key2, not "other"
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn test_scan_prefix_tracks_in_read_set() {
        let (mut txn, ns, _, _, _) = create_txn_with_test_data();
        let prefix = create_test_key(&ns, b"key");

        let _ = txn.scan_prefix(&prefix).unwrap();

        // All scanned keys should be tracked
        assert_eq!(txn.read_count(), 2);
    }

    #[test]
    fn test_scan_prefix_includes_uncommitted_writes() {
        let (mut txn, ns, _, _, _) = create_txn_with_test_data();

        // Add uncommitted write matching prefix
        let new_key = create_test_key(&ns, b"key_new");
        txn.write_set
            .insert(new_key.clone(), Value::String("new".to_string()));

        let prefix = create_test_key(&ns, b"key");
        let results = txn.scan_prefix(&prefix).unwrap();

        // Should include the uncommitted write
        assert_eq!(results.len(), 3); // key1, key2, key_new

        // Find the new key in results
        let found = results
            .iter()
            .any(|(k, v)| k == &new_key && *v == Value::String("new".to_string()));
        assert!(found);
    }

    #[test]
    fn test_scan_prefix_excludes_deleted_keys() {
        let (mut txn, ns, key1, _, _) = create_txn_with_test_data();

        // Delete key1
        txn.delete_set.insert(key1.clone());

        let prefix = create_test_key(&ns, b"key");
        let results = txn.scan_prefix(&prefix).unwrap();

        // Should NOT include deleted key1
        assert_eq!(results.len(), 1); // only key2

        // Verify key1 is not in results
        let found = results.iter().any(|(k, _)| k == &key1);
        assert!(!found);
    }

    #[test]
    fn test_scan_prefix_write_overwrites_snapshot() {
        let (mut txn, ns, key1, _, _) = create_txn_with_test_data();

        // Overwrite key1
        txn.write_set
            .insert(key1.clone(), Value::String("overwritten".to_string()));

        let prefix = create_test_key(&ns, b"key");
        let results = txn.scan_prefix(&prefix).unwrap();

        // Should have 2 keys
        assert_eq!(results.len(), 2);

        // key1 should have overwritten value
        let key1_value = results.iter().find(|(k, _)| k == &key1).map(|(_, v)| v);
        assert_eq!(key1_value, Some(&Value::String("overwritten".to_string())));
    }

    #[test]
    fn test_scan_prefix_empty_results() {
        let (mut txn, ns, _, _, _) = create_txn_with_test_data();
        let prefix = create_test_key(&ns, b"nonexistent");

        let results = txn.scan_prefix(&prefix).unwrap();
        assert!(results.is_empty());
    }

    #[test]
    fn test_scan_prefix_fails_when_not_active() {
        let (mut txn, ns, _, _, _) = create_txn_with_test_data();
        txn.mark_validating().unwrap();

        let prefix = create_test_key(&ns, b"key");
        let result = txn.scan_prefix(&prefix);
        assert!(result.is_err());
    }

    #[test]
    fn test_scan_prefix_fails_without_snapshot() {
        let mut txn = create_test_txn();
        let ns = create_test_namespace();
        let prefix = create_test_key(&ns, b"key");

        let result = txn.scan_prefix(&prefix);
        assert!(result.is_err());
    }

    // === State Transition Tests ===

    #[test]
    fn test_state_transition_active_to_validating() {
        let mut txn = create_test_txn();
        assert!(txn.mark_validating().is_ok());
        assert!(!txn.is_active());
        assert!(matches!(txn.status, TransactionStatus::Validating));
    }

    #[test]
    fn test_state_transition_validating_to_committed() {
        let mut txn = create_test_txn();
        txn.mark_validating().unwrap();
        assert!(txn.mark_committed().is_ok());
        assert!(txn.is_committed());
        assert!(matches!(txn.status, TransactionStatus::Committed));
    }

    #[test]
    fn test_state_transition_active_to_aborted() {
        let mut txn = create_test_txn();
        assert!(txn.mark_aborted("user requested abort".to_string()).is_ok());
        assert!(txn.is_aborted());
        assert!(matches!(txn.status, TransactionStatus::Aborted { .. }));
        assert_eq!(txn.abort_reason(), Some("user requested abort"));
    }

    #[test]
    fn test_state_transition_validating_to_aborted() {
        let mut txn = create_test_txn();
        txn.mark_validating().unwrap();
        assert!(txn.mark_aborted("conflict detected".to_string()).is_ok());
        assert!(txn.is_aborted());
        assert_eq!(txn.abort_reason(), Some("conflict detected"));
    }

    // === Invalid State Transition Tests ===

    #[test]
    fn test_cannot_validating_from_committed() {
        let mut txn = create_test_txn();
        txn.mark_validating().unwrap();
        txn.mark_committed().unwrap();
        let result = txn.mark_validating();
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), Error::InvalidState(_)));
    }

    #[test]
    fn test_cannot_validating_from_aborted() {
        let mut txn = create_test_txn();
        txn.mark_aborted("test".to_string()).unwrap();
        let result = txn.mark_validating();
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), Error::InvalidState(_)));
    }

    #[test]
    fn test_cannot_commit_from_active() {
        let mut txn = create_test_txn();
        let result = txn.mark_committed();
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), Error::InvalidState(_)));
    }

    #[test]
    fn test_cannot_commit_from_aborted() {
        let mut txn = create_test_txn();
        txn.mark_aborted("test".to_string()).unwrap();
        let result = txn.mark_committed();
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), Error::InvalidState(_)));
    }

    #[test]
    fn test_cannot_commit_from_committed() {
        let mut txn = create_test_txn();
        txn.mark_validating().unwrap();
        txn.mark_committed().unwrap();
        let result = txn.mark_committed();
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), Error::InvalidState(_)));
    }

    #[test]
    fn test_cannot_abort_committed_transaction() {
        let mut txn = create_test_txn();
        txn.mark_validating().unwrap();
        txn.mark_committed().unwrap();
        let result = txn.mark_aborted("too late".to_string());
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), Error::InvalidState(_)));
    }

    #[test]
    fn test_cannot_abort_already_aborted() {
        let mut txn = create_test_txn();
        txn.mark_aborted("first abort".to_string()).unwrap();
        let result = txn.mark_aborted("second abort".to_string());
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), Error::InvalidState(_)));
    }

    // === ensure_active Tests ===

    #[test]
    fn test_ensure_active_succeeds_when_active() {
        let txn = create_test_txn();
        assert!(txn.ensure_active().is_ok());
    }

    #[test]
    fn test_ensure_active_fails_when_validating() {
        let mut txn = create_test_txn();
        txn.mark_validating().unwrap();
        let result = txn.ensure_active();
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), Error::InvalidState(_)));
    }

    #[test]
    fn test_ensure_active_fails_when_committed() {
        let mut txn = create_test_txn();
        txn.mark_validating().unwrap();
        txn.mark_committed().unwrap();
        let result = txn.ensure_active();
        assert!(result.is_err());
    }

    #[test]
    fn test_ensure_active_fails_when_aborted() {
        let mut txn = create_test_txn();
        txn.mark_aborted("test".to_string()).unwrap();
        let result = txn.ensure_active();
        assert!(result.is_err());
    }

    // === Abort Reason Tests ===

    #[test]
    fn test_abort_reason_none_when_not_aborted() {
        let txn = create_test_txn();
        assert!(txn.abort_reason().is_none());
    }

    #[test]
    fn test_abort_reason_none_when_committed() {
        let mut txn = create_test_txn();
        txn.mark_validating().unwrap();
        txn.mark_committed().unwrap();
        assert!(txn.abort_reason().is_none());
    }

    #[test]
    fn test_abort_reason_preserves_message() {
        let mut txn = create_test_txn();
        txn.mark_aborted("read-write conflict on key X".to_string())
            .unwrap();
        assert_eq!(txn.abort_reason(), Some("read-write conflict on key X"));
    }

    // === CASOperation Tests ===

    #[test]
    fn test_cas_operation_creation() {
        let run_id = RunId::new();
        let namespace = Namespace::new("t".into(), "a".into(), "g".into(), run_id);
        let key = Key::new(namespace, TypeTag::KV, b"test".to_vec());
        let value = Value::Int(42);

        let cas_op = CASOperation {
            key: key.clone(),
            expected_version: 5,
            new_value: value.clone(),
        };

        assert_eq!(cas_op.key, key);
        assert_eq!(cas_op.expected_version, 5);
        assert_eq!(cas_op.new_value, value);
    }

    #[test]
    fn test_cas_operation_version_zero_means_not_exist() {
        let run_id = RunId::new();
        let namespace = Namespace::new("t".into(), "a".into(), "g".into(), run_id);
        let key = Key::new(namespace, TypeTag::KV, b"new_key".to_vec());

        let cas_op = CASOperation {
            key,
            expected_version: 0,
            new_value: Value::String("initial".to_string()),
        };

        assert_eq!(cas_op.expected_version, 0);
    }

    // === TransactionStatus Tests ===

    #[test]
    fn test_transaction_status_equality() {
        assert_eq!(TransactionStatus::Active, TransactionStatus::Active);
        assert_eq!(TransactionStatus::Validating, TransactionStatus::Validating);
        assert_eq!(TransactionStatus::Committed, TransactionStatus::Committed);

        let aborted1 = TransactionStatus::Aborted {
            reason: "test".to_string(),
        };
        let aborted2 = TransactionStatus::Aborted {
            reason: "test".to_string(),
        };
        let aborted3 = TransactionStatus::Aborted {
            reason: "other".to_string(),
        };

        assert_eq!(aborted1, aborted2);
        assert_ne!(aborted1, aborted3);
        assert_ne!(TransactionStatus::Active, TransactionStatus::Validating);
    }

    #[test]
    fn test_transaction_status_debug() {
        let active = TransactionStatus::Active;
        let debug_str = format!("{:?}", active);
        assert!(debug_str.contains("Active"));

        let aborted = TransactionStatus::Aborted {
            reason: "conflict".to_string(),
        };
        let debug_str = format!("{:?}", aborted);
        assert!(debug_str.contains("Aborted"));
        assert!(debug_str.contains("conflict"));
    }

    #[test]
    fn test_transaction_status_clone() {
        let original = TransactionStatus::Aborted {
            reason: "test".to_string(),
        };
        let cloned = original.clone();
        assert_eq!(original, cloned);
    }

    // === Write Operation Tests ===

    fn create_txn_with_empty_snapshot() -> TransactionContext {
        let snapshot = Box::new(ClonedSnapshotView::empty(100));
        let run_id = RunId::new();
        TransactionContext::with_snapshot(1, run_id, snapshot)
    }

    #[test]
    fn test_put_adds_to_write_set() {
        let ns = create_test_namespace();
        let mut txn = create_txn_with_empty_snapshot();
        let key = create_test_key(&ns, b"key1");
        let value = Value::Bytes(b"value1".to_vec());

        txn.put(key.clone(), value).unwrap();

        assert_eq!(txn.write_count(), 1);
        assert!(txn.write_set.contains_key(&key));
    }

    #[test]
    fn test_put_overwrites_in_write_set() {
        let ns = create_test_namespace();
        let mut txn = create_txn_with_empty_snapshot();
        let key = create_test_key(&ns, b"key1");

        txn.put(key.clone(), Value::Bytes(b"v1".to_vec())).unwrap();
        txn.put(key.clone(), Value::Bytes(b"v2".to_vec())).unwrap();

        assert_eq!(txn.write_count(), 1);
        let stored = txn.write_set.get(&key).unwrap();
        match stored {
            Value::Bytes(data) => assert_eq!(data, b"v2"),
            _ => panic!("Expected Bytes"),
        }
    }

    #[test]
    fn test_put_removes_from_delete_set() {
        let ns = create_test_namespace();
        let mut txn = create_txn_with_empty_snapshot();
        let key = create_test_key(&ns, b"key1");

        txn.delete(key.clone()).unwrap();
        assert!(txn.delete_set.contains(&key));
        assert_eq!(txn.delete_count(), 1);

        txn.put(key.clone(), Value::Bytes(b"v1".to_vec())).unwrap();

        assert!(!txn.delete_set.contains(&key));
        assert!(txn.write_set.contains_key(&key));
        assert_eq!(txn.delete_count(), 0);
        assert_eq!(txn.write_count(), 1);
    }

    #[test]
    fn test_put_is_read_your_writes() {
        let ns = create_test_namespace();
        let mut txn = create_txn_with_empty_snapshot();
        let key = create_test_key(&ns, b"key1");

        txn.put(key.clone(), Value::String("hello".to_string()))
            .unwrap();

        // Should be able to read our own write
        let result = txn.get(&key).unwrap();
        assert_eq!(result, Some(Value::String("hello".to_string())));

        // Should NOT be in read_set (read-your-writes)
        assert_eq!(txn.read_count(), 0);
    }

    #[test]
    fn test_put_fails_when_not_active() {
        let ns = create_test_namespace();
        let mut txn = create_txn_with_empty_snapshot();
        let key = create_test_key(&ns, b"key1");

        txn.mark_validating().unwrap();

        let result = txn.put(key, Value::Bytes(b"value".to_vec()));
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), Error::InvalidState(_)));
    }

    #[test]
    fn test_delete_adds_to_delete_set() {
        let ns = create_test_namespace();
        let mut txn = create_txn_with_empty_snapshot();
        let key = create_test_key(&ns, b"key1");

        txn.delete(key.clone()).unwrap();

        assert_eq!(txn.delete_count(), 1);
        assert!(txn.delete_set.contains(&key));
    }

    #[test]
    fn test_delete_removes_from_write_set() {
        let ns = create_test_namespace();
        let mut txn = create_txn_with_empty_snapshot();
        let key = create_test_key(&ns, b"key1");

        txn.put(key.clone(), Value::Bytes(b"v1".to_vec())).unwrap();
        assert!(txn.write_set.contains_key(&key));
        assert_eq!(txn.write_count(), 1);

        txn.delete(key.clone()).unwrap();

        assert!(!txn.write_set.contains_key(&key));
        assert!(txn.delete_set.contains(&key));
        assert_eq!(txn.write_count(), 0);
        assert_eq!(txn.delete_count(), 1);
    }

    #[test]
    fn test_delete_is_read_your_deletes() {
        let (mut txn, _, key1, _, _) = create_txn_with_test_data();

        // Key exists in snapshot
        assert!(txn.get(&key1).unwrap().is_some());

        // Delete it
        txn.delete(key1.clone()).unwrap();

        // Now it should return None (read-your-deletes)
        assert!(txn.get(&key1).unwrap().is_none());
    }

    #[test]
    fn test_delete_fails_when_not_active() {
        let ns = create_test_namespace();
        let mut txn = create_txn_with_empty_snapshot();
        let key = create_test_key(&ns, b"key1");

        txn.mark_validating().unwrap();

        let result = txn.delete(key);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), Error::InvalidState(_)));
    }

    #[test]
    fn test_cas_adds_to_cas_set() {
        let ns = create_test_namespace();
        let mut txn = create_txn_with_empty_snapshot();
        let key = create_test_key(&ns, b"key1");
        let value = Value::Bytes(b"new_value".to_vec());

        txn.cas(key.clone(), 50, value.clone()).unwrap();

        assert_eq!(txn.cas_count(), 1);
        let cas_op = &txn.cas_set[0];
        assert_eq!(cas_op.key, key);
        assert_eq!(cas_op.expected_version, 50);
        assert_eq!(cas_op.new_value, value);
    }

    #[test]
    fn test_cas_version_zero_means_not_exist() {
        let ns = create_test_namespace();
        let mut txn = create_txn_with_empty_snapshot();
        let key = create_test_key(&ns, b"new_key");

        // CAS with expected_version = 0 means key must not exist
        txn.cas(key.clone(), 0, Value::String("initial".to_string()))
            .unwrap();

        assert_eq!(txn.cas_count(), 1);
        assert_eq!(txn.cas_set[0].expected_version, 0);
    }

    #[test]
    fn test_multiple_cas_operations() {
        let ns = create_test_namespace();
        let mut txn = create_txn_with_empty_snapshot();

        txn.cas(
            create_test_key(&ns, b"k1"),
            10,
            Value::Bytes(b"v1".to_vec()),
        )
        .unwrap();
        txn.cas(
            create_test_key(&ns, b"k2"),
            20,
            Value::Bytes(b"v2".to_vec()),
        )
        .unwrap();
        txn.cas(create_test_key(&ns, b"k3"), 0, Value::Bytes(b"v3".to_vec()))
            .unwrap();

        assert_eq!(txn.cas_count(), 3);

        // Verify each CAS operation
        assert_eq!(txn.cas_set[0].expected_version, 10);
        assert_eq!(txn.cas_set[1].expected_version, 20);
        assert_eq!(txn.cas_set[2].expected_version, 0);
    }

    #[test]
    fn test_cas_does_not_add_to_read_set() {
        let ns = create_test_namespace();
        let mut txn = create_txn_with_empty_snapshot();
        let key = create_test_key(&ns, b"key1");

        txn.cas(key.clone(), 5, Value::String("value".to_string()))
            .unwrap();

        // CAS does NOT add to read_set
        assert_eq!(txn.read_count(), 0);
        assert!(txn.get_read_version(&key).is_none());
    }

    #[test]
    fn test_cas_fails_when_not_active() {
        let ns = create_test_namespace();
        let mut txn = create_txn_with_empty_snapshot();
        let key = create_test_key(&ns, b"key1");

        txn.mark_validating().unwrap();

        let result = txn.cas(key, 0, Value::Bytes(b"value".to_vec()));
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), Error::InvalidState(_)));
    }

    #[test]
    fn test_has_pending_operations_with_writes() {
        let ns = create_test_namespace();
        let mut txn = create_txn_with_empty_snapshot();
        assert!(!txn.has_pending_operations());

        txn.put(create_test_key(&ns, b"k"), Value::Bytes(b"v".to_vec()))
            .unwrap();
        assert!(txn.has_pending_operations());
    }

    #[test]
    fn test_has_pending_operations_with_deletes() {
        let ns = create_test_namespace();
        let mut txn = create_txn_with_empty_snapshot();
        assert!(!txn.has_pending_operations());

        txn.delete(create_test_key(&ns, b"k")).unwrap();
        assert!(txn.has_pending_operations());
    }

    #[test]
    fn test_has_pending_operations_with_cas() {
        let ns = create_test_namespace();
        let mut txn = create_txn_with_empty_snapshot();
        assert!(!txn.has_pending_operations());

        txn.cas(create_test_key(&ns, b"k"), 0, Value::Bytes(b"v".to_vec()))
            .unwrap();
        assert!(txn.has_pending_operations());
    }

    #[test]
    fn test_is_read_only_false_with_writes() {
        let ns = create_test_namespace();
        let mut txn = create_txn_with_empty_snapshot();
        assert!(txn.is_read_only());

        txn.put(create_test_key(&ns, b"k"), Value::Bytes(b"v".to_vec()))
            .unwrap();
        assert!(!txn.is_read_only());
    }

    #[test]
    fn test_is_read_only_true_with_only_reads() {
        let (mut txn, _, key1, _, _) = create_txn_with_test_data();

        // Only reads
        let _ = txn.get(&key1).unwrap();

        assert!(txn.is_read_only());
        assert!(!txn.has_pending_operations());
    }

    #[test]
    fn test_clear_operations() {
        let ns = create_test_namespace();
        let mut txn = create_txn_with_empty_snapshot();

        txn.put(create_test_key(&ns, b"k1"), Value::Bytes(b"v1".to_vec()))
            .unwrap();
        txn.delete(create_test_key(&ns, b"k2")).unwrap();
        txn.cas(create_test_key(&ns, b"k3"), 0, Value::Bytes(b"v3".to_vec()))
            .unwrap();

        assert!(txn.has_pending_operations());
        assert_eq!(txn.write_count(), 1);
        assert_eq!(txn.delete_count(), 1);
        assert_eq!(txn.cas_count(), 1);

        txn.clear_operations().unwrap();

        assert!(!txn.has_pending_operations());
        assert_eq!(txn.write_count(), 0);
        assert_eq!(txn.delete_count(), 0);
        assert_eq!(txn.cas_count(), 0);
        assert_eq!(txn.read_count(), 0);
    }

    #[test]
    fn test_clear_operations_clears_read_set() {
        let (mut txn, _, key1, _, _) = create_txn_with_test_data();

        // Read a key
        let _ = txn.get(&key1).unwrap();
        assert_eq!(txn.read_count(), 1);

        txn.clear_operations().unwrap();

        assert_eq!(txn.read_count(), 0);
    }

    #[test]
    fn test_clear_operations_fails_when_not_active() {
        let mut txn = create_txn_with_empty_snapshot();
        txn.mark_validating().unwrap();

        let result = txn.clear_operations();
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), Error::InvalidState(_)));
    }

    #[test]
    fn test_put_then_delete_then_put() {
        let ns = create_test_namespace();
        let mut txn = create_txn_with_empty_snapshot();
        let key = create_test_key(&ns, b"key1");

        // Put, then delete, then put again
        txn.put(key.clone(), Value::String("first".to_string()))
            .unwrap();
        assert_eq!(txn.write_count(), 1);
        assert_eq!(txn.delete_count(), 0);

        txn.delete(key.clone()).unwrap();
        assert_eq!(txn.write_count(), 0);
        assert_eq!(txn.delete_count(), 1);

        txn.put(key.clone(), Value::String("second".to_string()))
            .unwrap();
        assert_eq!(txn.write_count(), 1);
        assert_eq!(txn.delete_count(), 0);

        // Should read the final value
        let result = txn.get(&key).unwrap();
        assert_eq!(result, Some(Value::String("second".to_string())));
    }

    #[test]
    fn test_delete_then_put_then_delete() {
        let ns = create_test_namespace();
        let mut txn = create_txn_with_empty_snapshot();
        let key = create_test_key(&ns, b"key1");

        // Delete, then put, then delete again
        txn.delete(key.clone()).unwrap();
        assert_eq!(txn.delete_count(), 1);

        txn.put(key.clone(), Value::String("value".to_string()))
            .unwrap();
        assert_eq!(txn.delete_count(), 0);
        assert_eq!(txn.write_count(), 1);

        txn.delete(key.clone()).unwrap();
        assert_eq!(txn.delete_count(), 1);
        assert_eq!(txn.write_count(), 0);

        // Should read None
        let result = txn.get(&key).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_multiple_keys_independent() {
        let ns = create_test_namespace();
        let mut txn = create_txn_with_empty_snapshot();
        let key1 = create_test_key(&ns, b"key1");
        let key2 = create_test_key(&ns, b"key2");
        let key3 = create_test_key(&ns, b"key3");

        txn.put(key1.clone(), Value::String("v1".to_string()))
            .unwrap();
        txn.delete(key2.clone()).unwrap();
        txn.cas(key3.clone(), 0, Value::String("v3".to_string()))
            .unwrap();

        assert_eq!(txn.write_count(), 1);
        assert_eq!(txn.delete_count(), 1);
        assert_eq!(txn.cas_count(), 1);

        // Each key's state is independent
        assert!(txn.write_set.contains_key(&key1));
        assert!(txn.delete_set.contains(&key2));
        assert_eq!(txn.cas_set[0].key, key3);
    }

    // === Commit Tests ===

    mod commit_tests {
        use super::*;
        use strata_storage::UnifiedStore;

        fn create_test_store() -> UnifiedStore {
            UnifiedStore::new()
        }

        fn create_txn_with_store(store: &UnifiedStore) -> TransactionContext {
            let snapshot = store.create_snapshot();
            let run_id = RunId::new();
            TransactionContext::with_snapshot(1, run_id, Box::new(snapshot))
        }

        #[test]
        fn test_commit_empty_transaction() {
            let store = create_test_store();
            let mut txn = create_txn_with_store(&store);

            let result = txn.commit(&store);

            assert!(result.is_ok());
            assert!(txn.is_committed());
        }

        #[test]
        fn test_commit_read_only_transaction_no_conflict() {
            let store = create_test_store();
            let ns = create_test_namespace();
            let key = create_test_key(&ns, b"key");

            store
                .put(key.clone(), Value::Int(100), None)
                .expect("put failed");

            let mut txn = create_txn_with_store(&store);
            let _ = txn.get(&key).expect("get failed"); // Read adds to read_set

            // No concurrent modification - should commit
            let result = txn.commit(&store);
            assert!(result.is_ok());
            assert!(txn.is_committed());
        }

        #[test]
        fn test_commit_read_only_with_concurrent_modification() {
            let store = create_test_store();
            let ns = create_test_namespace();
            let key = create_test_key(&ns, b"key");

            store
                .put(key.clone(), Value::Int(100), None)
                .expect("put failed");

            let mut txn = create_txn_with_store(&store);
            let _ = txn.get(&key).expect("get failed"); // Read adds to read_set

            // Concurrent modification
            store
                .put(key.clone(), Value::Int(200), None)
                .expect("put failed");

            // Per spec Section 3.2 Scenario 3: Read-only transactions ALWAYS commit.
            // They only see their snapshot view and have no writes to validate.
            let result = txn.commit(&store);
            assert!(result.is_ok());
            assert!(txn.is_committed());
        }

        #[test]
        fn test_commit_with_blind_write() {
            let store = create_test_store();
            let ns = create_test_namespace();
            let key = create_test_key(&ns, b"key");

            store
                .put(key.clone(), Value::Int(100), None)
                .expect("put failed");

            let mut txn = create_txn_with_store(&store);
            // Blind write - no read first
            txn.put(key.clone(), Value::Int(200)).expect("put failed");

            // Concurrent modification
            store
                .put(key.clone(), Value::Int(300), None)
                .expect("put failed");

            // Per spec Section 3.2 Scenario 1: Blind writes do NOT conflict
            let result = txn.commit(&store);
            assert!(result.is_ok());
            assert!(txn.is_committed());
        }

        #[test]
        fn test_commit_with_read_write_conflict() {
            let store = create_test_store();
            let ns = create_test_namespace();
            let key = create_test_key(&ns, b"key");

            store
                .put(key.clone(), Value::Int(100), None)
                .expect("put failed");

            let mut txn = create_txn_with_store(&store);
            let _ = txn.get(&key).expect("get failed"); // Read adds to read_set
            txn.put(key.clone(), Value::Int(200)).expect("put failed");

            // Concurrent modification
            store
                .put(key.clone(), Value::Int(300), None)
                .expect("put failed");

            // Per spec Section 3.1 Condition 1: Read-write conflict
            let result = txn.commit(&store);
            assert!(result.is_err());
            assert!(txn.is_aborted());

            if let Err(CommitError::ValidationFailed(validation)) = result {
                assert!(!validation.is_valid());
                assert_eq!(validation.conflict_count(), 1);
            } else {
                panic!("Expected ValidationFailed error");
            }
        }

        #[test]
        fn test_commit_with_cas_conflict() {
            let store = create_test_store();
            let ns = create_test_namespace();
            let key = create_test_key(&ns, b"counter");

            store
                .put(key.clone(), Value::Int(0), None)
                .expect("put failed");
            let v1 = store.get(&key).expect("get failed").unwrap().version.as_u64();

            let mut txn = create_txn_with_store(&store);
            txn.cas(key.clone(), v1, Value::Int(1)).expect("cas failed");

            // Concurrent modification
            store
                .put(key.clone(), Value::Int(100), None)
                .expect("put failed");

            // Per spec Section 3.1 Condition 3: CAS conflict
            let result = txn.commit(&store);
            assert!(result.is_err());
            assert!(txn.is_aborted());
        }

        #[test]
        fn test_commit_first_committer_wins() {
            let store = create_test_store();
            let ns = create_test_namespace();
            let key = create_test_key(&ns, b"shared");

            store
                .put(key.clone(), Value::String("initial".into()), None)
                .expect("put failed");

            // T1 and T2 both read and write the same key
            let mut txn1 = create_txn_with_store(&store);
            let _ = txn1.get(&key).expect("get failed");
            txn1.put(key.clone(), Value::String("from_t1".into()))
                .expect("put failed");

            let mut txn2 = create_txn_with_store(&store);
            let _ = txn2.get(&key).expect("get failed");
            txn2.put(key.clone(), Value::String("from_t2".into()))
                .expect("put failed");

            // T1 commits first - should succeed
            let result1 = txn1.commit(&store);
            assert!(result1.is_ok());
            assert!(txn1.is_committed());

            // Simulate T1's write being applied (will be proper in )
            store
                .put(key.clone(), Value::String("from_t1".into()), None)
                .expect("put failed");

            // T2 tries to commit - should fail (read-set version changed)
            let result2 = txn2.commit(&store);
            assert!(result2.is_err());
            assert!(txn2.is_aborted());
        }

        #[test]
        fn test_cannot_commit_twice() {
            let store = create_test_store();
            let mut txn = create_txn_with_store(&store);

            let result1 = txn.commit(&store);
            assert!(result1.is_ok());

            let result2 = txn.commit(&store);
            assert!(result2.is_err());

            if let Err(CommitError::InvalidState(msg)) = result2 {
                assert!(msg.contains("Committed"));
            } else {
                panic!("Expected InvalidState error");
            }
        }

        #[test]
        fn test_cannot_commit_aborted_transaction() {
            let store = create_test_store();
            let mut txn = create_txn_with_store(&store);

            txn.mark_aborted("Manual abort".to_string())
                .expect("abort failed");

            let result = txn.commit(&store);
            assert!(result.is_err());

            if let Err(CommitError::InvalidState(msg)) = result {
                assert!(msg.contains("Aborted"));
            } else {
                panic!("Expected InvalidState error");
            }
        }

        #[test]
        fn test_commit_transitions_to_validating_then_committed() {
            let store = create_test_store();
            let mut txn = create_txn_with_store(&store);

            assert!(txn.is_active());

            let result = txn.commit(&store);

            assert!(result.is_ok());
            // Should end up in Committed state (validating is transient)
            assert!(txn.is_committed());
        }

        #[test]
        fn test_commit_with_cas_success() {
            let store = create_test_store();
            let ns = create_test_namespace();
            let key = create_test_key(&ns, b"counter");

            store
                .put(key.clone(), Value::Int(0), None)
                .expect("put failed");
            let v1 = store.get(&key).expect("get failed").unwrap().version.as_u64();

            let mut txn = create_txn_with_store(&store);
            txn.cas(key.clone(), v1, Value::Int(1)).expect("cas failed");

            // No concurrent modification - CAS should succeed
            let result = txn.commit(&store);
            assert!(result.is_ok());
            assert!(txn.is_committed());
        }

        #[test]
        fn test_commit_with_multiple_operations() {
            let store = create_test_store();
            let ns = create_test_namespace();
            let key1 = create_test_key(&ns, b"key1");
            let key2 = create_test_key(&ns, b"key2");
            let key3 = create_test_key(&ns, b"key3");

            // Setup
            store
                .put(key1.clone(), Value::Int(1), None)
                .expect("put failed");
            store
                .put(key2.clone(), Value::Int(2), None)
                .expect("put failed");

            let mut txn = create_txn_with_store(&store);

            // Mix of operations
            let _ = txn.get(&key1).expect("get failed"); // Read
            txn.put(key1.clone(), Value::Int(10)).expect("put failed"); // Write after read
            txn.put(key3.clone(), Value::Int(30)).expect("put failed"); // Blind write (new key)
            txn.delete(key2.clone()).expect("delete failed"); // Delete

            // No concurrent modifications - should commit
            let result = txn.commit(&store);
            assert!(result.is_ok());
            assert!(txn.is_committed());
        }

        #[test]
        fn test_commit_error_display() {
            let store = create_test_store();
            let ns = create_test_namespace();
            let key = create_test_key(&ns, b"key");

            store
                .put(key.clone(), Value::Int(100), None)
                .expect("put failed");

            let mut txn = create_txn_with_store(&store);
            let _ = txn.get(&key).expect("get failed");
            // Add a write to make this NOT a read-only transaction
            txn.put(key.clone(), Value::Int(150)).expect("put failed");

            // Concurrent modification
            store
                .put(key.clone(), Value::Int(200), None)
                .expect("put failed");

            let result = txn.commit(&store);
            if let Err(e) = result {
                let display = format!("{}", e);
                assert!(display.contains("conflict"));
            } else {
                panic!("Expected error");
            }
        }

        #[test]
        fn test_commit_invalid_state_display() {
            let err = CommitError::InvalidState("test reason".to_string());
            let display = format!("{}", err);
            assert!(display.contains("Invalid state"));
            assert!(display.contains("test reason"));
        }
    }

    // === Apply Writes Tests ===

    mod apply_writes_tests {
        use super::*;
        use strata_storage::UnifiedStore;

        fn create_test_store() -> UnifiedStore {
            UnifiedStore::new()
        }

        fn create_txn_with_store(store: &UnifiedStore) -> TransactionContext {
            let snapshot = store.create_snapshot();
            let run_id = RunId::new();
            TransactionContext::with_snapshot(1, run_id, Box::new(snapshot))
        }

        #[test]
        fn test_apply_writes_empty_transaction() {
            let store = create_test_store();
            let mut txn = create_txn_with_store(&store);
            txn.commit(&store).expect("commit failed");

            let result = txn.apply_writes(&store, 100).expect("apply_writes failed");

            assert_eq!(result.commit_version, 100);
            assert_eq!(result.puts_applied, 0);
            assert_eq!(result.deletes_applied, 0);
            assert_eq!(result.cas_applied, 0);
            assert_eq!(result.total_operations(), 0);
        }

        #[test]
        fn test_apply_writes_single_put() {
            let store = create_test_store();
            let ns = create_test_namespace();
            let key = create_test_key(&ns, b"key");

            let mut txn = create_txn_with_store(&store);
            txn.put(key.clone(), Value::Int(42)).expect("put failed");
            txn.commit(&store).expect("commit failed");

            let result = txn.apply_writes(&store, 100).expect("apply_writes failed");

            assert_eq!(result.puts_applied, 1);
            assert_eq!(result.commit_version, 100);

            // Verify key was written with correct version
            let stored = store.get(&key).expect("get failed").unwrap();
            assert_eq!(stored.version.as_u64(), 100);
            assert_eq!(stored.value, Value::Int(42));
        }

        #[test]
        fn test_apply_writes_multiple_puts_same_version() {
            let store = create_test_store();
            let ns = create_test_namespace();
            let key1 = create_test_key(&ns, b"key1");
            let key2 = create_test_key(&ns, b"key2");
            let key3 = create_test_key(&ns, b"key3");

            let mut txn = create_txn_with_store(&store);
            txn.put(key1.clone(), Value::Int(1)).expect("put failed");
            txn.put(key2.clone(), Value::Int(2)).expect("put failed");
            txn.put(key3.clone(), Value::Int(3)).expect("put failed");
            txn.commit(&store).expect("commit failed");

            let result = txn.apply_writes(&store, 50).expect("apply_writes failed");

            assert_eq!(result.puts_applied, 3);

            // All keys should have same commit version
            assert_eq!(store.get(&key1).unwrap().unwrap().version.as_u64(), 50);
            assert_eq!(store.get(&key2).unwrap().unwrap().version.as_u64(), 50);
            assert_eq!(store.get(&key3).unwrap().unwrap().version.as_u64(), 50);
        }

        #[test]
        fn test_apply_writes_with_delete() {
            let store = create_test_store();
            let ns = create_test_namespace();
            let key = create_test_key(&ns, b"key");

            // Pre-existing key
            store
                .put(key.clone(), Value::Int(100), None)
                .expect("put failed");

            let mut txn = create_txn_with_store(&store);
            txn.delete(key.clone()).expect("delete failed");
            txn.commit(&store).expect("commit failed");

            let result = txn.apply_writes(&store, 50).expect("apply_writes failed");

            assert_eq!(result.deletes_applied, 1);
            assert!(store.get(&key).expect("get failed").is_none());
        }

        #[test]
        fn test_apply_writes_with_cas() {
            let store = create_test_store();
            let ns = create_test_namespace();
            let key = create_test_key(&ns, b"counter");

            store
                .put(key.clone(), Value::Int(0), None)
                .expect("put failed");
            let v1 = store.get(&key).expect("get failed").unwrap().version.as_u64();

            let mut txn = create_txn_with_store(&store);
            txn.cas(key.clone(), v1, Value::Int(1)).expect("cas failed");
            txn.commit(&store).expect("commit failed");

            let result = txn.apply_writes(&store, 50).expect("apply_writes failed");

            assert_eq!(result.cas_applied, 1);

            let stored = store.get(&key).expect("get failed").unwrap();
            assert_eq!(stored.version.as_u64(), 50);
            assert_eq!(stored.value, Value::Int(1));
        }

        #[test]
        fn test_apply_writes_fails_if_not_committed() {
            let store = create_test_store();
            let txn = create_txn_with_store(&store);

            // Transaction is still Active
            let result = txn.apply_writes(&store, 100);

            assert!(result.is_err());
        }

        #[test]
        fn test_apply_writes_fails_if_aborted() {
            let store = create_test_store();
            let mut txn = create_txn_with_store(&store);

            txn.mark_aborted("test abort".to_string())
                .expect("abort failed");

            let result = txn.apply_writes(&store, 100);

            assert!(result.is_err());
        }

        #[test]
        fn test_apply_writes_updates_global_version() {
            let store = create_test_store();
            let ns = create_test_namespace();
            let key = create_test_key(&ns, b"key");

            let initial_version = store.current_version();

            let mut txn = create_txn_with_store(&store);
            txn.put(key.clone(), Value::Int(42)).expect("put failed");
            txn.commit(&store).expect("commit failed");

            txn.apply_writes(&store, 100).expect("apply_writes failed");

            assert!(store.current_version() >= 100);
            assert!(store.current_version() > initial_version);
        }

        #[test]
        fn test_apply_writes_mixed_operations() {
            let store = create_test_store();
            let ns = create_test_namespace();
            let key1 = create_test_key(&ns, b"put_key");
            let key2 = create_test_key(&ns, b"delete_key");
            let key3 = create_test_key(&ns, b"cas_key");

            // Setup: pre-existing keys
            store
                .put(key2.clone(), Value::Int(200), None)
                .expect("put failed");
            store
                .put(key3.clone(), Value::Int(300), None)
                .expect("put failed");
            let v3 = store.get(&key3).expect("get failed").unwrap().version.as_u64();

            let mut txn = create_txn_with_store(&store);
            txn.put(key1.clone(), Value::Int(1)).expect("put failed");
            txn.delete(key2.clone()).expect("delete failed");
            txn.cas(key3.clone(), v3, Value::Int(301))
                .expect("cas failed");
            txn.commit(&store).expect("commit failed");

            let result = txn.apply_writes(&store, 50).expect("apply_writes failed");

            assert_eq!(result.puts_applied, 1);
            assert_eq!(result.deletes_applied, 1);
            assert_eq!(result.cas_applied, 1);
            assert_eq!(result.total_operations(), 3);

            // Verify results
            assert_eq!(store.get(&key1).unwrap().unwrap().value, Value::Int(1));
            assert!(store.get(&key2).unwrap().is_none());
            assert_eq!(store.get(&key3).unwrap().unwrap().value, Value::Int(301));
        }

        #[test]
        fn test_apply_result_total_operations() {
            let result = ApplyResult {
                commit_version: 100,
                puts_applied: 5,
                deletes_applied: 3,
                cas_applied: 2,
            };

            assert_eq!(result.total_operations(), 10);
        }
    }

    // === Rollback Tests ===

    mod rollback_tests {
        use super::*;
        use strata_storage::UnifiedStore;

        fn create_test_store() -> UnifiedStore {
            UnifiedStore::new()
        }

        fn create_txn_with_snapshot(store: &UnifiedStore) -> TransactionContext {
            let run_id = RunId::new();
            TransactionContext::with_snapshot(
                store.current_version(),
                run_id,
                Box::new(store.create_snapshot()),
            )
        }

        fn create_test_namespace() -> Namespace {
            let run_id = RunId::new();
            Namespace::new("t".into(), "a".into(), "g".into(), run_id)
        }

        fn create_key(ns: &Namespace, name: &str) -> Key {
            Key::new(ns.clone(), TypeTag::KV, name.as_bytes().to_vec())
        }

        #[test]
        fn test_abort_clears_write_set() {
            let store = create_test_store();
            let ns = create_test_namespace();
            let key = create_key(&ns, "key");

            let mut txn = create_txn_with_snapshot(&store);
            txn.put(key.clone(), Value::Int(42)).unwrap();

            assert_eq!(txn.write_count(), 1);

            txn.mark_aborted("Test abort".to_string()).unwrap();

            assert_eq!(txn.write_count(), 0);
            assert!(matches!(txn.status, TransactionStatus::Aborted { .. }));
        }

        #[test]
        fn test_abort_clears_delete_set() {
            let store = create_test_store();
            let ns = create_test_namespace();
            let key = create_key(&ns, "key");

            store.put(key.clone(), Value::Int(100), None).unwrap();

            let mut txn = create_txn_with_snapshot(&store);
            txn.delete(key.clone()).unwrap();

            assert_eq!(txn.delete_count(), 1);

            txn.mark_aborted("Test abort".to_string()).unwrap();

            assert_eq!(txn.delete_count(), 0);
        }

        #[test]
        fn test_abort_clears_cas_set() {
            let store = create_test_store();
            let ns = create_test_namespace();
            let key = create_key(&ns, "counter");

            store.put(key.clone(), Value::Int(0), None).unwrap();
            let version = store.get(&key).unwrap().unwrap().version.as_u64();

            let mut txn = create_txn_with_snapshot(&store);
            txn.cas(key.clone(), version, Value::Int(1)).unwrap();

            assert_eq!(txn.cas_count(), 1);

            txn.mark_aborted("Test abort".to_string()).unwrap();

            assert_eq!(txn.cas_count(), 0);
        }

        #[test]
        fn test_abort_preserves_read_set() {
            let store = create_test_store();
            let ns = create_test_namespace();
            let key = create_key(&ns, "key");

            store.put(key.clone(), Value::Int(100), None).unwrap();

            let mut txn = create_txn_with_snapshot(&store);
            let _ = txn.get(&key).unwrap();

            assert!(!txn.read_set.is_empty());

            txn.mark_aborted("Test abort".to_string()).unwrap();

            // Read set is preserved for debugging
            assert!(!txn.read_set.is_empty());
        }

        #[test]
        fn test_can_rollback_from_active() {
            let store = create_test_store();
            let txn = create_txn_with_snapshot(&store);

            assert!(txn.can_rollback());
        }

        #[test]
        fn test_can_rollback_from_validating() {
            let store = create_test_store();
            let mut txn = create_txn_with_snapshot(&store);
            txn.status = TransactionStatus::Validating;

            assert!(txn.can_rollback());
        }

        #[test]
        fn test_cannot_rollback_committed() {
            let store = create_test_store();
            let mut txn = create_txn_with_snapshot(&store);
            txn.status = TransactionStatus::Committed;

            assert!(!txn.can_rollback());
        }

        #[test]
        fn test_cannot_rollback_aborted() {
            let store = create_test_store();
            let mut txn = create_txn_with_snapshot(&store);
            txn.mark_aborted("already aborted".to_string()).unwrap();

            assert!(!txn.can_rollback());
        }

        #[test]
        fn test_pending_operations_empty() {
            let store = create_test_store();
            let txn = create_txn_with_snapshot(&store);

            let pending = txn.pending_operations();
            assert_eq!(pending.puts, 0);
            assert_eq!(pending.deletes, 0);
            assert_eq!(pending.cas, 0);
            assert_eq!(pending.total(), 0);
            assert!(pending.is_empty());
        }

        #[test]
        fn test_pending_operations_with_writes() {
            let store = create_test_store();
            let ns = create_test_namespace();
            let key = create_key(&ns, "key");

            let mut txn = create_txn_with_snapshot(&store);
            txn.put(key.clone(), Value::Int(1)).unwrap();

            let pending = txn.pending_operations();
            assert_eq!(pending.puts, 1);
            assert_eq!(pending.deletes, 0);
            assert_eq!(pending.cas, 0);
            assert_eq!(pending.total(), 1);
            assert!(!pending.is_empty());
        }

        #[test]
        fn test_pending_operations_with_mixed_ops() {
            let store = create_test_store();
            let ns = create_test_namespace();
            let key1 = create_key(&ns, "key1");
            let key2 = create_key(&ns, "key2");
            let key3 = create_key(&ns, "key3");

            store.put(key2.clone(), Value::Int(100), None).unwrap();
            store.put(key3.clone(), Value::Int(0), None).unwrap();
            let v3 = store.get(&key3).unwrap().unwrap().version.as_u64();

            let mut txn = create_txn_with_snapshot(&store);
            txn.put(key1.clone(), Value::Int(1)).unwrap();
            txn.delete(key2.clone()).unwrap();
            txn.cas(key3.clone(), v3, Value::Int(1)).unwrap();

            let pending = txn.pending_operations();
            assert_eq!(pending.puts, 1);
            assert_eq!(pending.deletes, 1);
            assert_eq!(pending.cas, 1);
            assert_eq!(pending.total(), 3);
            assert!(!pending.is_empty());
        }

        #[test]
        fn test_pending_operations_cleared_after_abort() {
            let store = create_test_store();
            let ns = create_test_namespace();
            let key = create_key(&ns, "key");

            let mut txn = create_txn_with_snapshot(&store);
            txn.put(key.clone(), Value::Int(1)).unwrap();

            assert_eq!(txn.pending_operations().total(), 1);

            txn.mark_aborted("abort".to_string()).unwrap();

            let pending = txn.pending_operations();
            assert_eq!(pending.total(), 0);
            assert!(pending.is_empty());
        }

        #[test]
        fn test_pending_operations_debug() {
            let pending = PendingOperations {
                puts: 3,
                deletes: 2,
                cas: 1,
            };

            let debug_str = format!("{:?}", pending);
            assert!(debug_str.contains("3"));
            assert!(debug_str.contains("2"));
            assert!(debug_str.contains("1"));
        }

        #[test]
        fn test_pending_operations_equality() {
            let p1 = PendingOperations {
                puts: 1,
                deletes: 2,
                cas: 3,
            };
            let p2 = PendingOperations {
                puts: 1,
                deletes: 2,
                cas: 3,
            };
            let p3 = PendingOperations {
                puts: 0,
                deletes: 2,
                cas: 3,
            };

            assert_eq!(p1, p2);
            assert_ne!(p1, p3);
        }

        #[test]
        fn test_pending_operations_clone() {
            let p1 = PendingOperations {
                puts: 1,
                deletes: 2,
                cas: 3,
            };
            let p2 = p1;

            assert_eq!(p1, p2);
        }
    }

    // === Write to WAL Tests ===

    mod write_to_wal_tests {
        use super::*;
        use crate::wal_writer::TransactionWALWriter;
        use strata_durability::wal::{DurabilityMode, WALEntry, WAL};
        use strata_storage::UnifiedStore;
        use tempfile::TempDir;

        fn create_test_store() -> UnifiedStore {
            UnifiedStore::new()
        }

        fn create_test_wal() -> (WAL, TempDir) {
            let temp_dir = TempDir::new().unwrap();
            let wal_path = temp_dir.path().join("test.wal");
            let wal = WAL::open(&wal_path, DurabilityMode::default()).unwrap();
            (wal, temp_dir)
        }

        fn create_txn_with_store(store: &UnifiedStore) -> TransactionContext {
            let snapshot = store.create_snapshot();
            let run_id = RunId::new();
            TransactionContext::with_snapshot(1, run_id, Box::new(snapshot))
        }

        #[test]
        fn test_write_to_wal_empty_transaction() {
            let store = create_test_store();
            let (mut wal, _temp) = create_test_wal();

            let mut txn = create_txn_with_store(&store);
            txn.commit(&store).expect("commit failed");

            let mut writer = TransactionWALWriter::new(&mut wal, 1, txn.run_id);
            writer.write_begin().unwrap();
            txn.write_to_wal(&mut writer, 100).unwrap();
            writer.write_commit().unwrap();

            let entries = wal.read_all().unwrap();
            assert_eq!(entries.len(), 2); // BeginTxn + CommitTxn
        }

        #[test]
        fn test_write_to_wal_with_puts() {
            let store = create_test_store();
            let ns = create_test_namespace();
            let key1 = create_test_key(&ns, b"key1");
            let key2 = create_test_key(&ns, b"key2");
            let (mut wal, _temp) = create_test_wal();

            let mut txn = create_txn_with_store(&store);
            txn.put(key1.clone(), Value::Int(1)).expect("put failed");
            txn.put(key2.clone(), Value::Int(2)).expect("put failed");
            txn.commit(&store).expect("commit failed");

            let mut writer = TransactionWALWriter::new(&mut wal, 1, txn.run_id);
            writer.write_begin().unwrap();
            txn.write_to_wal(&mut writer, 100).unwrap();
            writer.write_commit().unwrap();

            let entries = wal.read_all().unwrap();
            assert_eq!(entries.len(), 4); // BeginTxn + 2 Write + CommitTxn

            // Verify write entries have correct version
            let write_count = entries
                .iter()
                .filter(|e| matches!(e, WALEntry::Write { version: 100, .. }))
                .count();
            assert_eq!(write_count, 2);
        }

        #[test]
        fn test_write_to_wal_with_delete() {
            let store = create_test_store();
            let ns = create_test_namespace();
            let key = create_test_key(&ns, b"key");
            let (mut wal, _temp) = create_test_wal();

            store
                .put(key.clone(), Value::Int(100), None)
                .expect("put failed");

            let mut txn = create_txn_with_store(&store);
            txn.delete(key.clone()).expect("delete failed");
            txn.commit(&store).expect("commit failed");

            let mut writer = TransactionWALWriter::new(&mut wal, 1, txn.run_id);
            writer.write_begin().unwrap();
            txn.write_to_wal(&mut writer, 100).unwrap();
            writer.write_commit().unwrap();

            let entries = wal.read_all().unwrap();
            assert_eq!(entries.len(), 3); // BeginTxn + Delete + CommitTxn

            // Verify delete entry
            let delete_entry = entries
                .iter()
                .find(|e| matches!(e, WALEntry::Delete { .. }))
                .expect("No delete entry found");
            if let WALEntry::Delete { version, .. } = delete_entry {
                assert_eq!(*version, 100);
            }
        }

        #[test]
        fn test_write_to_wal_with_cas() {
            let store = create_test_store();
            let ns = create_test_namespace();
            let key = create_test_key(&ns, b"counter");
            let (mut wal, _temp) = create_test_wal();

            store
                .put(key.clone(), Value::Int(0), None)
                .expect("put failed");
            let v1 = store.get(&key).expect("get failed").unwrap().version.as_u64();

            let mut txn = create_txn_with_store(&store);
            txn.cas(key.clone(), v1, Value::Int(1)).expect("cas failed");
            txn.commit(&store).expect("commit failed");

            let mut writer = TransactionWALWriter::new(&mut wal, 1, txn.run_id);
            writer.write_begin().unwrap();
            txn.write_to_wal(&mut writer, 100).unwrap();
            writer.write_commit().unwrap();

            let entries = wal.read_all().unwrap();
            // CAS is written as a Write entry
            assert_eq!(entries.len(), 3); // BeginTxn + Write (CAS) + CommitTxn
        }

        #[test]
        fn test_write_to_wal_fails_if_not_committed() {
            let store = create_test_store();
            let (mut wal, _temp) = create_test_wal();

            let txn = create_txn_with_store(&store);

            let mut writer = TransactionWALWriter::new(&mut wal, 1, txn.run_id);
            let result = txn.write_to_wal(&mut writer, 100);

            assert!(result.is_err());
        }

        #[test]
        fn test_write_to_wal_entries_include_run_id() {
            let store = create_test_store();
            let ns = create_test_namespace();
            let key = create_test_key(&ns, b"key");
            let (mut wal, _temp) = create_test_wal();

            let mut txn = create_txn_with_store(&store);
            let run_id = txn.run_id;
            txn.put(key.clone(), Value::Int(42)).expect("put failed");
            txn.commit(&store).expect("commit failed");

            let mut writer = TransactionWALWriter::new(&mut wal, 1, run_id);
            writer.write_begin().unwrap();
            txn.write_to_wal(&mut writer, 100).unwrap();
            writer.write_commit().unwrap();

            let entries = wal.read_all().unwrap();

            // All entries should have the same run_id
            for entry in &entries {
                assert_eq!(entry.run_id(), Some(run_id));
            }
        }

        #[test]
        fn test_write_to_wal_mixed_operations() {
            let store = create_test_store();
            let ns = create_test_namespace();
            let key1 = create_test_key(&ns, b"put_key");
            let key2 = create_test_key(&ns, b"delete_key");
            let key3 = create_test_key(&ns, b"cas_key");
            let (mut wal, _temp) = create_test_wal();

            // Setup: pre-existing keys
            store
                .put(key2.clone(), Value::Int(200), None)
                .expect("put failed");
            store
                .put(key3.clone(), Value::Int(300), None)
                .expect("put failed");
            let v3 = store.get(&key3).expect("get failed").unwrap().version.as_u64();

            let mut txn = create_txn_with_store(&store);
            txn.put(key1.clone(), Value::Int(1)).expect("put failed");
            txn.delete(key2.clone()).expect("delete failed");
            txn.cas(key3.clone(), v3, Value::Int(301))
                .expect("cas failed");
            txn.commit(&store).expect("commit failed");

            let mut writer = TransactionWALWriter::new(&mut wal, 1, txn.run_id);
            writer.write_begin().unwrap();
            txn.write_to_wal(&mut writer, 50).unwrap();
            writer.write_commit().unwrap();

            let entries = wal.read_all().unwrap();
            // BeginTxn + Write (put) + Delete + Write (CAS) + CommitTxn = 5
            assert_eq!(entries.len(), 5);

            // Verify all write/delete entries have version 50
            for entry in &entries {
                if let Some(version) = entry.version() {
                    assert_eq!(version, 50);
                }
            }
        }
    }

    // === Reset and Pooling Tests ===

    mod reset_tests {
        use super::*;

        #[test]
        fn test_reset_preserves_capacity() {
            // Create a transaction and populate it with data to grow internal collections
            let ns = create_test_namespace();
            let run_id = ns.run_id;
            let mut txn = TransactionContext::new(1, run_id, 100);

            // Add many entries to force capacity growth
            for i in 0..100 {
                let key = create_test_key(&ns, format!("key{}", i).as_bytes());
                txn.read_set.insert(key.clone(), i as u64);
                txn.write_set
                    .insert(key.clone(), Value::Bytes(vec![i as u8]));
                if i % 2 == 0 {
                    txn.delete_set.insert(key.clone());
                }
            }
            for i in 0..50 {
                let key = create_test_key(&ns, format!("cas{}", i).as_bytes());
                txn.cas_set.push(CASOperation {
                    key,
                    expected_version: i as u64,
                    new_value: Value::Bytes(vec![i as u8]),
                });
            }

            // Record capacity after growth
            let (read_cap, write_cap, delete_cap, cas_cap) = txn.capacity();
            assert!(read_cap >= 100, "read_set should have grown");
            assert!(write_cap >= 100, "write_set should have grown");
            assert!(delete_cap >= 50, "delete_set should have grown");
            assert!(cas_cap >= 50, "cas_set should have grown");

            // Reset the transaction
            let new_run_id = RunId::new();
            txn.reset(2, new_run_id, None);

            // Verify data is cleared
            assert!(txn.read_set.is_empty(), "read_set should be empty");
            assert!(txn.write_set.is_empty(), "write_set should be empty");
            assert!(txn.delete_set.is_empty(), "delete_set should be empty");
            assert!(txn.cas_set.is_empty(), "cas_set should be empty");

            // Verify capacity is preserved
            let (new_read_cap, new_write_cap, new_delete_cap, new_cas_cap) = txn.capacity();
            assert_eq!(
                new_read_cap, read_cap,
                "read_set capacity should be preserved"
            );
            assert_eq!(
                new_write_cap, write_cap,
                "write_set capacity should be preserved"
            );
            assert_eq!(
                new_delete_cap, delete_cap,
                "delete_set capacity should be preserved"
            );
            assert_eq!(new_cas_cap, cas_cap, "cas_set capacity should be preserved");

            // Verify state is reset correctly
            assert_eq!(txn.txn_id, 2);
            assert_eq!(txn.run_id, new_run_id);
            assert_eq!(txn.start_version, 0); // No snapshot
            assert!(txn.is_active());
        }

        #[test]
        fn test_reset_with_snapshot() {
            let ns = create_test_namespace();
            let run_id = ns.run_id;
            let mut txn = TransactionContext::new(1, run_id, 100);

            // Populate to grow collections
            for i in 0..10 {
                let key = create_test_key(&ns, format!("key{}", i).as_bytes());
                txn.write_set
                    .insert(key, Value::Bytes(format!("value{}", i).into()));
            }

            // Create a snapshot for reset
            let snapshot_data = std::collections::BTreeMap::new();
            let snapshot = Box::new(ClonedSnapshotView::new(500, snapshot_data));

            // Reset with the new snapshot
            let new_run_id = RunId::new();
            txn.reset(42, new_run_id, Some(snapshot));

            // Verify snapshot version is used
            assert_eq!(txn.txn_id, 42);
            assert_eq!(txn.run_id, new_run_id);
            assert_eq!(txn.start_version, 500);
            assert!(txn.is_active());
            assert!(txn.write_set.is_empty());
        }

        #[test]
        fn test_reset_clears_aborted_state() {
            let ns = create_test_namespace();
            let run_id = ns.run_id;
            let mut txn = TransactionContext::new(1, run_id, 100);

            // Abort the transaction
            txn.mark_aborted("Test abort".to_string()).unwrap();
            assert!(!txn.is_active());

            // Reset should restore to Active state
            let new_run_id = RunId::new();
            txn.reset(2, new_run_id, None);

            assert!(txn.is_active());
            assert_eq!(txn.status, TransactionStatus::Active);
        }

        #[test]
        fn test_capacity_returns_correct_values() {
            let ns = create_test_namespace();
            let run_id = ns.run_id;
            let mut txn = TransactionContext::new(1, run_id, 100);

            // Initially capacity might be 0 or small
            let (r0, w0, d0, c0) = txn.capacity();

            // Add entries
            for i in 0..20 {
                let key = create_test_key(&ns, format!("k{}", i).as_bytes());
                txn.read_set.insert(key.clone(), i as u64);
            }
            for i in 0..30 {
                let key = create_test_key(&ns, format!("w{}", i).as_bytes());
                txn.write_set.insert(key, Value::Bytes(vec![i as u8]));
            }
            for i in 0..10 {
                let key = create_test_key(&ns, format!("d{}", i).as_bytes());
                txn.delete_set.insert(key);
            }
            for i in 0..5 {
                let key = create_test_key(&ns, format!("c{}", i).as_bytes());
                txn.cas_set.push(CASOperation {
                    key,
                    expected_version: i as u64,
                    new_value: Value::Bytes(vec![]),
                });
            }

            // Capacity should have grown
            let (r1, w1, d1, c1) = txn.capacity();
            assert!(r1 >= 20 || r1 > r0, "read_set capacity should grow");
            assert!(w1 >= 30 || w1 > w0, "write_set capacity should grow");
            assert!(d1 >= 10 || d1 > d0, "delete_set capacity should grow");
            assert!(c1 >= 5 || c1 > c0, "cas_set capacity should grow");
        }
    }

    // ========================================================================
    // JSON Transaction Types Tests
    // ========================================================================

    mod json_types_tests {
        use super::*;
        use strata_core::json::JsonPath;
        use strata_core::types::{JsonDocId, Namespace};

        fn create_json_key(run_id: RunId, doc_id: &JsonDocId) -> Key {
            Key::new_json(Namespace::for_run(run_id), doc_id)
        }

        #[test]
        fn test_json_path_read_creation() {
            let run_id = RunId::new();
            let doc_id = JsonDocId::new();
            let key = create_json_key(run_id, &doc_id);
            let path = "foo.bar".parse::<JsonPath>().unwrap();

            let read = JsonPathRead::new(key.clone(), path.clone(), 5);

            assert_eq!(read.key, key);
            assert_eq!(read.path, path);
            assert_eq!(read.version, 5);
        }

        #[test]
        fn test_json_path_read_root() {
            let run_id = RunId::new();
            let doc_id = JsonDocId::new();
            let key = create_json_key(run_id, &doc_id);
            let path = JsonPath::root();

            let read = JsonPathRead::new(key, path.clone(), 0);

            assert_eq!(read.path, path);
            assert_eq!(read.version, 0); // Version 0 = document doesn't exist
        }

        #[test]
        fn test_json_path_read_nested_path() {
            let run_id = RunId::new();
            let doc_id = JsonDocId::new();
            let key = create_json_key(run_id, &doc_id);
            let path = "users[0].profile.name".parse::<JsonPath>().unwrap();

            let read = JsonPathRead::new(key, path.clone(), 42);

            assert_eq!(read.path, path);
            assert_eq!(read.version, 42);
        }

        #[test]
        fn test_json_patch_entry_creation() {
            use strata_core::json::JsonValue;

            let run_id = RunId::new();
            let doc_id = JsonDocId::new();
            let key = create_json_key(run_id, &doc_id);
            let path = "count".parse::<JsonPath>().unwrap();
            let patch = JsonPatch::set_at(path, JsonValue::from(42i64));

            let entry = JsonPatchEntry::new(key.clone(), patch, 10);

            assert_eq!(entry.key, key);
            assert_eq!(entry.resulting_version, 10);
        }

        #[test]
        fn test_json_patch_entry_delete() {
            let run_id = RunId::new();
            let doc_id = JsonDocId::new();
            let key = create_json_key(run_id, &doc_id);
            let path = "temp_field".parse::<JsonPath>().unwrap();
            let patch = JsonPatch::delete_at(path);

            let entry = JsonPatchEntry::new(key.clone(), patch, 5);

            assert_eq!(entry.key, key);
            assert_eq!(entry.resulting_version, 5);
        }

        #[test]
        fn test_json_path_read_clone() {
            let run_id = RunId::new();
            let doc_id = JsonDocId::new();
            let key = create_json_key(run_id, &doc_id);
            let path = "data".parse::<JsonPath>().unwrap();

            let read = JsonPathRead::new(key, path, 100);
            let cloned = read.clone();

            assert_eq!(read.key, cloned.key);
            assert_eq!(read.path, cloned.path);
            assert_eq!(read.version, cloned.version);
        }

        #[test]
        fn test_json_patch_entry_clone() {
            use strata_core::json::JsonValue;

            let run_id = RunId::new();
            let doc_id = JsonDocId::new();
            let key = create_json_key(run_id, &doc_id);
            let path = "value".parse::<JsonPath>().unwrap();
            let patch = JsonPatch::set_at(path, JsonValue::from("test"));

            let entry = JsonPatchEntry::new(key, patch, 1);
            let cloned = entry.clone();

            assert_eq!(entry.key, cloned.key);
            assert_eq!(entry.resulting_version, cloned.resulting_version);
        }

        // === Lazy JSON Fields Tests ===

        #[test]
        fn test_json_fields_lazy_allocation() {
            // New transaction should have no JSON fields allocated
            let txn = create_test_txn();
            assert!(!txn.has_json_ops());
            assert!(txn.json_reads().is_empty());
            assert!(txn.json_writes().is_empty());
            assert!(txn.json_snapshot_versions().is_none());
        }

        #[test]
        fn test_json_reads_lazy_init() {
            let mut txn = create_test_txn();
            let run_id = RunId::new();
            let doc_id = JsonDocId::new();
            let key = create_json_key(run_id, &doc_id);
            let path = "data.field".parse::<JsonPath>().unwrap();

            // Initially not allocated
            assert!(!txn.has_json_ops());

            // Record a JSON read
            txn.record_json_read(key.clone(), path.clone(), 1);

            // Now has JSON ops
            assert!(txn.has_json_ops());
            assert_eq!(txn.json_reads().len(), 1);
            assert_eq!(txn.json_reads()[0].key, key);
            assert_eq!(txn.json_reads()[0].version, 1);
        }

        #[test]
        fn test_json_writes_lazy_init() {
            use strata_core::json::JsonValue;

            let mut txn = create_test_txn();
            let run_id = RunId::new();
            let doc_id = JsonDocId::new();
            let key = create_json_key(run_id, &doc_id);
            let path = "data.value".parse::<JsonPath>().unwrap();
            let patch = JsonPatch::set_at(path, JsonValue::from(42));

            // Initially not allocated
            assert!(!txn.has_json_ops());

            // Record a JSON write
            txn.record_json_write(key.clone(), patch, 2);

            // Now has JSON ops
            assert!(txn.has_json_ops());
            assert_eq!(txn.json_writes().len(), 1);
            assert_eq!(txn.json_writes()[0].key, key);
            assert_eq!(txn.json_writes()[0].resulting_version, 2);
        }

        #[test]
        fn test_json_snapshot_versions_lazy_init() {
            let mut txn = create_test_txn();
            let run_id = RunId::new();
            let doc_id = JsonDocId::new();
            let key = create_json_key(run_id, &doc_id);

            // Initially not allocated
            assert!(txn.json_snapshot_versions().is_none());

            // Record a snapshot version
            txn.record_json_snapshot_version(key.clone(), 100);

            // Now allocated
            assert!(txn.has_json_ops());
            let versions = txn.json_snapshot_versions().unwrap();
            assert_eq!(versions.len(), 1);
            assert_eq!(*versions.get(&key).unwrap(), 100);
        }

        #[test]
        fn test_multiple_json_reads() {
            let mut txn = create_test_txn();
            let run_id = RunId::new();

            for i in 0..3 {
                let doc_id = JsonDocId::new();
                let key = create_json_key(run_id, &doc_id);
                let path_str = format!("field{}", i);
                let path = path_str.parse::<JsonPath>().unwrap();
                txn.record_json_read(key, path, i as u64);
            }

            assert_eq!(txn.json_reads().len(), 3);
        }

        #[test]
        fn test_clear_operations_clears_json() {
            use strata_core::json::JsonValue;

            let mut txn = create_test_txn();
            let run_id = RunId::new();
            let doc_id = JsonDocId::new();
            let key = create_json_key(run_id, &doc_id);
            let path = "test".parse::<JsonPath>().unwrap();

            // Add various JSON operations
            txn.record_json_read(key.clone(), path.clone(), 1);
            txn.record_json_write(
                key.clone(),
                JsonPatch::set_at(path.clone(), JsonValue::from("x")),
                2,
            );
            txn.record_json_snapshot_version(key, 1);

            assert!(txn.has_json_ops());

            // Clear operations
            txn.clear_operations().unwrap();

            // JSON fields should be deallocated (None)
            assert!(!txn.has_json_ops());
            assert!(txn.json_reads().is_empty());
            assert!(txn.json_writes().is_empty());
            assert!(txn.json_snapshot_versions().is_none());
        }

        #[test]
        fn test_ensure_methods_return_mutable_ref() {
            let mut txn = create_test_txn();

            // Ensure methods create empty collections
            let reads = txn.ensure_json_reads();
            assert!(reads.is_empty());

            let writes = txn.ensure_json_writes();
            assert!(writes.is_empty());

            let versions = txn.ensure_json_snapshot_versions();
            assert!(versions.is_empty());

            // Now all are allocated
            assert!(txn.has_json_ops());
        }

        // === JsonStoreExt Read-Your-Writes Tests ===

        #[test]
        fn test_json_set_records_write() {
            use strata_core::json::JsonValue;

            let mut txn = create_test_txn();
            let run_id = RunId::new();
            let doc_id = JsonDocId::new();
            let key = create_json_key(run_id, &doc_id);
            let path = "data.field".parse::<JsonPath>().unwrap();

            // Perform json_set
            txn.json_set(&key, &path, JsonValue::from(42)).unwrap();

            // Should have recorded the write
            assert!(txn.has_json_ops());
            assert_eq!(txn.json_writes().len(), 1);
            assert_eq!(txn.json_writes()[0].key, key);
        }

        #[test]
        fn test_json_delete_records_write() {
            let mut txn = create_test_txn();
            let run_id = RunId::new();
            let doc_id = JsonDocId::new();
            let key = create_json_key(run_id, &doc_id);
            let path = "data.field".parse::<JsonPath>().unwrap();

            // Perform json_delete
            txn.json_delete(&key, &path).unwrap();

            // Should have recorded the write (delete is a write)
            assert!(txn.has_json_ops());
            assert_eq!(txn.json_writes().len(), 1);
        }

        #[test]
        fn test_json_read_your_writes_direct_path() {
            use strata_core::json::JsonValue;

            let mut txn = create_test_txn();
            let run_id = RunId::new();
            let doc_id = JsonDocId::new();
            let key = create_json_key(run_id, &doc_id);
            let path = "data".parse::<JsonPath>().unwrap();

            // Set a value
            txn.json_set(&key, &path, JsonValue::from("hello")).unwrap();

            // Read it back (should get from write set, not snapshot)
            let result = txn.json_get(&key, &path).unwrap();
            assert_eq!(result, Some(JsonValue::from("hello")));
        }

        #[test]
        fn test_json_read_your_writes_nested_path() {
            use strata_core::json::JsonValue;

            let mut txn = create_test_txn();
            let run_id = RunId::new();
            let doc_id = JsonDocId::new();
            let key = create_json_key(run_id, &doc_id);
            let parent_path = "data".parse::<JsonPath>().unwrap();
            let child_path: JsonPath = "data.name".parse().unwrap();

            // Set an object at parent path
            // Use a simple object structure
            let mut obj = serde_json::Map::new();
            obj.insert(
                "name".to_string(),
                serde_json::Value::String("Alice".to_string()),
            );
            obj.insert("age".to_string(), serde_json::Value::Number(30.into()));
            let json_obj = JsonValue::from(serde_json::Value::Object(obj));
            txn.json_set(&key, &parent_path, json_obj).unwrap();

            // Read child path (should navigate into the written object)
            let result = txn.json_get(&key, &child_path).unwrap();
            assert_eq!(result, Some(JsonValue::from("Alice")));
        }

        #[test]
        fn test_json_read_your_deletes() {
            use strata_core::json::JsonValue;

            let mut txn = create_test_txn();
            let run_id = RunId::new();
            let doc_id = JsonDocId::new();
            let key = create_json_key(run_id, &doc_id);
            let path = "data".parse::<JsonPath>().unwrap();

            // First set a value
            txn.json_set(&key, &path, JsonValue::from("hello")).unwrap();

            // Then delete it
            txn.json_delete(&key, &path).unwrap();

            // Reading should return None (deleted)
            let result = txn.json_get(&key, &path).unwrap();
            assert_eq!(result, None);
        }

        #[test]
        fn test_json_inactive_txn_errors() {
            use strata_core::json::JsonValue;

            let mut txn = create_test_txn();
            let run_id = RunId::new();
            let doc_id = JsonDocId::new();
            let key = create_json_key(run_id, &doc_id);
            let path = "data".parse::<JsonPath>().unwrap();

            // Abort the transaction
            let _ = txn.mark_aborted("test".to_string());

            // Operations should fail
            assert!(txn.json_set(&key, &path, JsonValue::from(1)).is_err());
            assert!(txn.json_get(&key, &path).is_err());
            assert!(txn.json_delete(&key, &path).is_err());
        }
    }
}
