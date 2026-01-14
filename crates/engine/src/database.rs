//! Database struct and open/close logic
//!
//! This module provides the main Database struct that orchestrates:
//! - Storage initialization
//! - WAL opening
//! - Automatic recovery on startup
//! - Transaction API (M2)
//!
//! ## Transaction API
//!
//! The Database provides two ways to execute transactions:
//!
//! 1. **Closure API** (recommended): `db.transaction(run_id, |txn| { ... })`
//!    - Automatic commit on success, abort on error
//!    - Returns the closure's return value
//!
//! 2. **Manual API**: `begin_transaction()` + `commit_transaction()`
//!    - For cases requiring external control over commit timing
//!
//! Per spec Section 4: Implicit transactions wrap M1-style operations.

use crate::coordinator::{TransactionCoordinator, TransactionMetrics};
use in_mem_concurrency::{
    validate_transaction, RecoveryCoordinator, TransactionContext, TransactionWALWriter,
};
use in_mem_core::error::{Error, Result};
use in_mem_core::traits::Storage;
use in_mem_core::types::{Key, RunId};
use in_mem_core::value::Value;
use in_mem_core::VersionedValue;
use in_mem_durability::wal::{DurabilityMode, WAL};
use in_mem_storage::UnifiedStore;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tracing::info;

/// Configuration for transaction retry behavior
///
/// Per spec Section 4.3: Implicit transactions include automatic retry on conflict.
/// This configuration controls the retry behavior for transactions.
///
/// # Example
/// ```ignore
/// let config = RetryConfig {
///     max_retries: 5,
///     base_delay_ms: 10,
///     max_delay_ms: 200,
/// };
/// db.transaction_with_retry(run_id, config, |txn| { ... })?;
/// ```
#[derive(Debug, Clone)]
pub struct RetryConfig {
    /// Maximum number of retry attempts (0 = no retries)
    pub max_retries: usize,
    /// Base delay between retries in milliseconds (exponential backoff)
    pub base_delay_ms: u64,
    /// Maximum delay between retries in milliseconds
    pub max_delay_ms: u64,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_retries: 3,
            base_delay_ms: 10,
            max_delay_ms: 100,
        }
    }
}

impl RetryConfig {
    /// Create a new RetryConfig with default values
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a RetryConfig with no retries
    pub fn no_retry() -> Self {
        Self {
            max_retries: 0,
            ..Default::default()
        }
    }

    /// Set maximum number of retries
    pub fn with_max_retries(mut self, max_retries: usize) -> Self {
        self.max_retries = max_retries;
        self
    }

    /// Set base delay for exponential backoff
    pub fn with_base_delay_ms(mut self, base_delay_ms: u64) -> Self {
        self.base_delay_ms = base_delay_ms;
        self
    }

    /// Set maximum delay between retries
    pub fn with_max_delay_ms(mut self, max_delay_ms: u64) -> Self {
        self.max_delay_ms = max_delay_ms;
        self
    }

    /// Calculate delay for a given attempt (exponential backoff)
    fn calculate_delay(&self, attempt: usize) -> Duration {
        // Cap the shift to prevent overflow (1 << 63 is the max for u64)
        let shift = attempt.min(63);
        let multiplier = 1u64 << shift;
        let delay_ms = self.base_delay_ms.saturating_mul(multiplier);
        Duration::from_millis(delay_ms.min(self.max_delay_ms))
    }
}

/// Main database struct with transaction support
///
/// Orchestrates storage, WAL, recovery, and transactions.
/// Create a database by calling `Database::open()`.
///
/// # Transaction Support (M2)
///
/// The Database provides transaction APIs per spec Section 4:
/// - `transaction()`: Execute a closure within a transaction
/// - `begin_transaction()`: Start a manual transaction
/// - `commit_transaction()`: Commit a manual transaction
///
/// # Example
///
/// ```ignore
/// use in_mem_engine::Database;
/// use in_mem_core::types::RunId;
///
/// let db = Database::open("/path/to/data")?;
/// let run_id = RunId::new();
///
/// // Closure API (recommended)
/// let result = db.transaction(run_id, |txn| {
///     txn.put(key, value)?;
///     Ok(())
/// })?;
/// ```
pub struct Database {
    /// Data directory path
    data_dir: PathBuf,

    /// Unified storage (thread-safe)
    storage: Arc<UnifiedStore>,

    /// Write-ahead log (protected by mutex for exclusive access)
    wal: Arc<Mutex<WAL>>,

    /// Transaction coordinator for lifecycle management, version allocation, and metrics
    ///
    /// Per spec Section 6.1: Single monotonic counter for the entire database.
    coordinator: TransactionCoordinator,

    /// Commit lock to serialize validation + WAL write + storage apply
    ///
    /// This ensures atomicity of the commit sequence and prevents CAS race conditions.
    /// Per spec Section 3.3: First-committer-wins requires atomic validate-and-commit.
    commit_lock: Mutex<()>,
}

impl Database {
    /// Open database at given path with automatic recovery
    ///
    /// This is the main entry point for database initialization.
    /// Uses the default durability mode (Batched).
    ///
    /// # Flow
    ///
    /// 1. Create/open data directory
    /// 2. Open WAL file at `<path>/wal/current.wal`
    /// 3. Create empty storage
    /// 4. Replay WAL to restore state
    /// 5. Return ready database
    ///
    /// # Arguments
    ///
    /// * `path` - Directory path for the database
    ///
    /// # Returns
    ///
    /// * `Ok(Database)` - Ready-to-use database instance
    /// * `Err` - If directory creation, WAL opening, or recovery fails
    ///
    /// # Example
    ///
    /// ```ignore
    /// use in_mem_engine::Database;
    ///
    /// let db = Database::open("/path/to/data")?;
    /// let storage = db.storage();
    /// ```
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self> {
        Self::open_with_mode(path, DurabilityMode::default())
    }

    /// Open database with specific durability mode
    ///
    /// Allows selecting between Strict, Batched, or Async durability modes.
    ///
    /// # Arguments
    ///
    /// * `path` - Directory path for the database
    /// * `durability_mode` - Durability mode for WAL operations
    ///
    /// # Returns
    ///
    /// * `Ok(Database)` - Ready-to-use database instance
    /// * `Err` - If directory creation, WAL opening, or recovery fails
    ///
    /// # Recovery
    ///
    /// Per spec Section 5: Uses RecoveryCoordinator to replay WAL and
    /// initialize TransactionManager with the recovered version.
    pub fn open_with_mode<P: AsRef<Path>>(
        path: P,
        durability_mode: DurabilityMode,
    ) -> Result<Self> {
        let data_dir = path.as_ref().to_path_buf();

        // Create data directory
        std::fs::create_dir_all(&data_dir).map_err(Error::IoError)?;

        // Create WAL directory
        let wal_dir = data_dir.join("wal");
        std::fs::create_dir_all(&wal_dir).map_err(Error::IoError)?;

        let wal_path = wal_dir.join("current.wal");

        // Use RecoveryCoordinator for proper transaction-aware recovery
        // This replays the WAL and initializes version tracking
        let recovery = RecoveryCoordinator::new(wal_path.clone());
        let result = recovery.recover()?;

        info!(
            txns_replayed = result.stats.txns_replayed,
            writes_applied = result.stats.writes_applied,
            deletes_applied = result.stats.deletes_applied,
            incomplete_txns = result.stats.incomplete_txns,
            final_version = result.stats.final_version,
            "Recovery complete"
        );

        // Re-open WAL for appending (recovery opened read-only)
        let wal = WAL::open(&wal_path, durability_mode)?;

        // Create coordinator from recovery result (preserves version continuity)
        let coordinator = TransactionCoordinator::from_recovery(&result);

        Ok(Self {
            data_dir,
            storage: Arc::new(result.storage),
            wal: Arc::new(Mutex::new(wal)),
            coordinator,
            commit_lock: Mutex::new(()),
        })
    }

    /// Get reference to the storage layer
    ///
    /// Use this to perform read/write operations on the database.
    pub fn storage(&self) -> &UnifiedStore {
        &self.storage
    }

    /// Get the data directory path
    pub fn data_dir(&self) -> &Path {
        &self.data_dir
    }

    /// Get access to the WAL for appending entries
    ///
    /// Returns an Arc to the Mutex-protected WAL.
    /// Lock the mutex to append entries.
    pub fn wal(&self) -> Arc<Mutex<WAL>> {
        Arc::clone(&self.wal)
    }

    /// Flush WAL to disk
    ///
    /// Forces all buffered WAL entries to be written to disk.
    /// This is automatically done based on durability mode, but can
    /// be called manually to ensure durability at a specific point.
    pub fn flush(&self) -> Result<()> {
        let wal = self.wal.lock().unwrap();
        wal.fsync()
    }

    // ========================================================================
    // Transaction API (M2)
    // ========================================================================

    /// Execute a transaction with the given closure
    ///
    /// Per spec Section 4:
    /// - Creates TransactionContext with snapshot
    /// - Executes closure with transaction
    /// - Validates and commits on success
    /// - Aborts on error
    ///
    /// # Arguments
    /// * `run_id` - RunId for namespace isolation
    /// * `f` - Closure that performs transaction operations
    ///
    /// # Returns
    /// * `Ok(T)` - Closure return value on successful commit
    /// * `Err` - On validation conflict or closure error
    ///
    /// # Example
    /// ```ignore
    /// let result = db.transaction(run_id, |txn| {
    ///     let val = txn.get(&key)?;
    ///     txn.put(key, new_value)?;
    ///     Ok(val)
    /// })?;
    /// ```
    pub fn transaction<F, T>(&self, run_id: RunId, f: F) -> Result<T>
    where
        F: FnOnce(&mut TransactionContext) -> Result<T>,
    {
        let mut txn = self.begin_transaction(run_id);

        // Execute closure
        let result = f(&mut txn);

        match result {
            Ok(value) => {
                // Commit on success
                self.commit_transaction(&mut txn)?;
                Ok(value)
            }
            Err(e) => {
                // Abort on error (just discard, per spec no AbortTxn in WAL for user aborts)
                let _ = txn.mark_aborted(format!("Closure error: {}", e));
                self.coordinator.record_abort();
                Err(e)
            }
        }
    }

    /// Execute a transaction with automatic retry on conflict
    ///
    /// Per spec Section 4.3: Implicit transactions include automatic retry on conflict.
    /// This method provides explicit retry control for transactions that may conflict.
    ///
    /// The closure is called repeatedly until either:
    /// - The transaction commits successfully
    /// - A non-conflict error occurs (not retried)
    /// - Maximum retries are exceeded
    ///
    /// # Arguments
    /// * `run_id` - RunId for namespace isolation
    /// * `config` - Retry configuration (max retries, delays)
    /// * `f` - Closure that performs transaction operations (must be `Fn`, not `FnOnce`)
    ///
    /// # Returns
    /// * `Ok(T)` - Closure return value on successful commit
    /// * `Err` - On non-conflict error or max retries exceeded
    ///
    /// # Example
    /// ```ignore
    /// let config = RetryConfig::default();
    /// let result = db.transaction_with_retry(run_id, config, |txn| {
    ///     let val = txn.get(&key)?;
    ///     txn.put(key.clone(), Value::I64(val.value + 1))?;
    ///     Ok(())
    /// })?;
    /// ```
    pub fn transaction_with_retry<F, T>(
        &self,
        run_id: RunId,
        config: RetryConfig,
        f: F,
    ) -> Result<T>
    where
        F: Fn(&mut TransactionContext) -> Result<T>,
    {
        let mut last_error = None;

        for attempt in 0..=config.max_retries {
            let mut txn = self.begin_transaction(run_id);

            // Execute closure
            match f(&mut txn) {
                Ok(value) => {
                    // Try to commit
                    match self.commit_transaction(&mut txn) {
                        Ok(()) => return Ok(value),
                        Err(e) if e.is_conflict() && attempt < config.max_retries => {
                            // Conflict during commit - will retry
                            last_error = Some(e);
                            std::thread::sleep(config.calculate_delay(attempt));
                            continue;
                        }
                        Err(e) => {
                            // Non-conflict error or max retries reached
                            let _ = txn.mark_aborted(format!("Commit error: {}", e));
                            self.coordinator.record_abort();
                            return Err(e);
                        }
                    }
                }
                Err(e) if e.is_conflict() && attempt < config.max_retries => {
                    // Conflict from closure - will retry
                    let _ = txn.mark_aborted(format!("Closure conflict: {}", e));
                    self.coordinator.record_abort();
                    last_error = Some(e);
                    std::thread::sleep(config.calculate_delay(attempt));
                    continue;
                }
                Err(e) => {
                    // Non-conflict error or max retries reached
                    let _ = txn.mark_aborted(format!("Closure error: {}", e));
                    self.coordinator.record_abort();
                    return Err(e);
                }
            }
        }

        // Max retries exceeded
        Err(last_error
            .unwrap_or_else(|| Error::TransactionConflict("Max retries exceeded".to_string())))
    }

    /// Execute a transaction with timeout
    ///
    /// If the transaction exceeds the timeout, it will be aborted
    /// before commit is attempted.
    ///
    /// # Arguments
    /// * `run_id` - RunId for namespace isolation
    /// * `timeout` - Maximum duration for the transaction
    /// * `f` - Closure that performs transaction operations
    ///
    /// # Returns
    /// * `Ok(T)` - Closure return value on successful commit
    /// * `Err(TransactionTimeout)` - Transaction exceeded timeout
    /// * `Err` - On validation conflict or closure error
    ///
    /// # Example
    /// ```ignore
    /// use std::time::Duration;
    ///
    /// let result = db.transaction_with_timeout(
    ///     run_id,
    ///     Duration::from_secs(5),
    ///     |txn| {
    ///         txn.put(key, value)?;
    ///         Ok(())
    ///     },
    /// )?;
    /// ```
    pub fn transaction_with_timeout<F, T>(
        &self,
        run_id: RunId,
        timeout: Duration,
        f: F,
    ) -> Result<T>
    where
        F: FnOnce(&mut TransactionContext) -> Result<T>,
    {
        let mut txn = self.begin_transaction(run_id);

        // Execute closure
        let result = f(&mut txn);

        match result {
            Ok(value) => {
                // Check timeout before commit
                if txn.is_expired(timeout) {
                    let elapsed = txn.elapsed();
                    let _ = txn.mark_aborted(format!(
                        "Transaction timeout: elapsed {:?}, limit {:?}",
                        elapsed, timeout
                    ));
                    self.coordinator.record_abort();
                    return Err(Error::TransactionTimeout(format!(
                        "Transaction exceeded timeout of {:?} (elapsed: {:?})",
                        timeout, elapsed
                    )));
                }

                // Commit on success
                self.commit_transaction(&mut txn)?;
                Ok(value)
            }
            Err(e) => {
                // Abort on error
                let _ = txn.mark_aborted(format!("Closure error: {}", e));
                self.coordinator.record_abort();
                Err(e)
            }
        }
    }

    /// Begin a new transaction (for manual control)
    ///
    /// Returns a TransactionContext that must be manually committed or aborted.
    /// Prefer `transaction()` closure API for automatic handling.
    ///
    /// # Arguments
    /// * `run_id` - RunId for namespace isolation
    ///
    /// # Returns
    /// * `TransactionContext` - Active transaction ready for operations
    ///
    /// # Example
    /// ```ignore
    /// let mut txn = db.begin_transaction(run_id);
    /// txn.put(key, value)?;
    /// db.commit_transaction(&mut txn)?;
    /// ```
    pub fn begin_transaction(&self, run_id: RunId) -> TransactionContext {
        self.coordinator.start_transaction(run_id, &self.storage)
    }

    /// Commit a transaction
    ///
    /// Per spec commit sequence:
    /// 1. Validate (conflict detection)
    /// 2. Allocate commit version
    /// 3. Write to WAL (BeginTxn, Writes, CommitTxn)
    /// 4. Apply to storage
    ///
    /// # Arguments
    /// * `txn` - Transaction to commit
    ///
    /// # Returns
    /// * `Ok(())` - Transaction committed successfully
    /// * `Err(TransactionConflict)` - Validation failed, transaction aborted
    ///
    /// # Errors
    /// - `TransactionConflict` - Read-write or CAS conflict detected
    /// - `InvalidState` - Transaction not in Active state
    pub fn commit_transaction(&self, txn: &mut TransactionContext) -> Result<()> {
        // Acquire commit lock to serialize validate → WAL → storage sequence
        // This prevents CAS race conditions where multiple transactions pass validation
        // before any of them writes to storage.
        let _commit_guard = self.commit_lock.lock().unwrap();

        // 1. Validate (under commit lock)
        txn.mark_validating()?;
        let validation = validate_transaction(txn, self.storage.as_ref());

        if !validation.is_valid() {
            let _ = txn.mark_aborted(format!("Validation failed: {:?}", validation.conflicts));
            self.coordinator.record_abort();
            return Err(Error::TransactionConflict(format!(
                "Conflicts: {:?}",
                validation.conflicts
            )));
        }

        // 2. Allocate commit version
        let commit_version = self.coordinator.allocate_commit_version();

        // 3. Write to WAL
        {
            let mut wal = self.wal.lock().unwrap();
            let mut wal_writer = TransactionWALWriter::new(&mut wal, txn.txn_id, txn.run_id);

            // Write BeginTxn
            wal_writer.write_begin()?;

            // Write all operations (puts, deletes, and CAS)
            for (key, value) in &txn.write_set {
                wal_writer.write_put(key.clone(), value.clone(), commit_version)?;
            }
            for key in &txn.delete_set {
                wal_writer.write_delete(key.clone(), commit_version)?;
            }
            // CAS operations are written as puts after validation
            for cas_op in &txn.cas_set {
                wal_writer.write_put(
                    cas_op.key.clone(),
                    cas_op.new_value.clone(),
                    commit_version,
                )?;
            }

            // Write CommitTxn (this also flushes)
            wal_writer.write_commit()?;
        }

        // 4. Apply to storage atomically
        // All writes and deletes are applied in a single batch to ensure atomicity.
        // This prevents other threads from seeing partial transaction states.
        let mut all_writes: Vec<_> = txn
            .write_set
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();

        // CAS operations are also writes after validation
        for cas_op in &txn.cas_set {
            all_writes.push((cas_op.key.clone(), cas_op.new_value.clone()));
        }

        let all_deletes: Vec<_> = txn.delete_set.iter().cloned().collect();

        self.storage
            .apply_batch(&all_writes, &all_deletes, commit_version)?;

        // Mark committed
        txn.mark_committed()?;
        self.coordinator.record_commit();

        Ok(())
    }

    /// Get the transaction coordinator (for metrics/testing)
    pub fn coordinator(&self) -> &TransactionCoordinator {
        &self.coordinator
    }

    /// Get transaction metrics
    ///
    /// Returns statistics about transaction lifecycle including:
    /// - Active count
    /// - Total started/committed/aborted
    /// - Commit rate
    pub fn metrics(&self) -> TransactionMetrics {
        self.coordinator.metrics()
    }

    // ========================================================================
    // Implicit Transactions (M1 Compatibility)
    // ========================================================================

    /// Put a key-value pair (M1 compatibility)
    ///
    /// Per spec Section 4.2: Wraps in implicit transaction, commits immediately.
    /// This provides backwards compatibility with M1-style operations.
    ///
    /// # Arguments
    /// * `run_id` - RunId for namespace isolation
    /// * `key` - Key to store
    /// * `value` - Value to store
    ///
    /// # Returns
    /// * `Ok(())` - Value stored successfully
    /// * `Err(TransactionConflict)` - Conflict detected (rare for implicit txns)
    ///
    /// # Example
    /// ```ignore
    /// db.put(run_id, key, Value::I64(42))?;
    /// ```
    pub fn put(&self, run_id: RunId, key: Key, value: Value) -> Result<()> {
        self.transaction(run_id, |txn| {
            txn.put(key.clone(), value.clone())?;
            Ok(())
        })
    }

    /// Get a value by key (M1 compatibility)
    ///
    /// Per spec Section 4.2: Read-only, always succeeds.
    /// This provides backwards compatibility with M1-style operations.
    ///
    /// Unlike writes, reads don't need a full transaction.
    /// We read directly from storage for O(log n) performance.
    ///
    /// # Arguments
    /// * `key` - Key to retrieve
    ///
    /// # Returns
    /// * `Ok(Some(VersionedValue))` - Value found
    /// * `Ok(None)` - Key doesn't exist
    ///
    /// # Example
    /// ```ignore
    /// if let Some(vv) = db.get(&key)? {
    ///     println!("Value: {:?}, Version: {}", vv.value, vv.version);
    /// }
    /// ```
    pub fn get(&self, key: &Key) -> Result<Option<VersionedValue>> {
        // For read-only operations, read directly from storage
        // This is O(log n) via BTreeMap lookup, avoiding O(n) snapshot clone
        self.storage.get(key)
    }

    /// Delete a key (M1 compatibility)
    ///
    /// Per spec Section 4.2: Wraps in implicit transaction, commits immediately.
    /// This provides backwards compatibility with M1-style operations.
    ///
    /// # Arguments
    /// * `run_id` - RunId for namespace isolation
    /// * `key` - Key to delete
    ///
    /// # Returns
    /// * `Ok(())` - Key deleted (or didn't exist)
    /// * `Err(TransactionConflict)` - Conflict detected (rare for implicit txns)
    ///
    /// # Example
    /// ```ignore
    /// db.delete(run_id, key)?;
    /// ```
    pub fn delete(&self, run_id: RunId, key: Key) -> Result<()> {
        self.transaction(run_id, |txn| {
            txn.delete(key.clone())?;
            Ok(())
        })
    }

    /// Compare-and-swap (M1 compatibility with explicit version)
    ///
    /// Per spec Section 3.4: CAS validates expected_version before write.
    /// The operation succeeds only if the current version matches expected_version.
    ///
    /// # Arguments
    /// * `run_id` - RunId for namespace isolation
    /// * `key` - Key to update
    /// * `expected_version` - Version the key must have (0 = key must not exist)
    /// * `new_value` - New value to write if version matches
    ///
    /// # Returns
    /// * `Ok(())` - CAS succeeded
    /// * `Err(TransactionConflict)` - Version mismatch or conflict
    ///
    /// # Example
    /// ```ignore
    /// // Get current version
    /// let vv = db.get(&key)?.unwrap();
    /// // Atomic update only if version matches
    /// db.cas(run_id, key, vv.version, Value::I64(new_val))?;
    /// ```
    pub fn cas(
        &self,
        run_id: RunId,
        key: Key,
        expected_version: u64,
        new_value: Value,
    ) -> Result<()> {
        self.transaction(run_id, |txn| {
            txn.cas(key.clone(), expected_version, new_value.clone())?;
            Ok(())
        })
    }

    /// Gracefully close the database
    ///
    /// Ensures all WAL entries are flushed to disk before returning.
    /// This should be called before dropping the database for guaranteed durability.
    pub fn close(&self) -> Result<()> {
        let wal = self.wal.lock().unwrap();
        wal.fsync()
    }
}

impl Drop for Database {
    fn drop(&mut self) {
        // Attempt to sync WAL on drop. This is a best-effort fallback;
        // users should call close() explicitly for guaranteed durability.
        match self.wal.lock() {
            Ok(wal) => {
                if let Err(e) = wal.fsync() {
                    // Log error but don't panic - we're in drop
                    eprintln!("WARNING: Failed to sync WAL on database drop: {}", e);
                    tracing::error!("Failed to sync WAL on database drop: {}", e);
                }
            }
            Err(e) => {
                eprintln!("WARNING: Failed to acquire WAL lock on database drop: {}", e);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use in_mem_core::types::{Key, Namespace, RunId};
    use in_mem_core::value::Value;
    use in_mem_core::Storage;
    use in_mem_durability::wal::WALEntry;
    use tempfile::TempDir;

    fn now() -> i64 {
        Utc::now().timestamp()
    }

    #[test]
    fn test_open_empty_database() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("db");

        let db = Database::open(&db_path).unwrap();

        // Should have empty storage
        assert_eq!(db.storage().current_version(), 0);

        // Data directory should exist
        assert!(db_path.exists());
        assert!(db_path.join("wal").exists());
        assert!(db_path.join("wal/current.wal").exists());
    }

    #[test]
    fn test_open_with_existing_wal() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("db");

        let run_id = RunId::new();
        let ns = Namespace::new(
            "tenant".to_string(),
            "app".to_string(),
            "agent".to_string(),
            run_id,
        );

        // Write data to WAL manually before opening database
        {
            std::fs::create_dir_all(db_path.join("wal")).unwrap();
            let mut wal =
                WAL::open(db_path.join("wal/current.wal"), DurabilityMode::Strict).unwrap();

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

            wal.append(&WALEntry::CommitTxn { txn_id: 1, run_id })
                .unwrap();
        }

        // Open database (should replay WAL)
        let db = Database::open(&db_path).unwrap();

        // Storage should have data from WAL
        let key1 = Key::new_kv(ns, "key1");
        let val = db.storage().get(&key1).unwrap().unwrap();

        if let Value::Bytes(bytes) = val.value {
            assert_eq!(bytes, b"value1");
        } else {
            panic!("Wrong value type");
        }
    }

    #[test]
    fn test_open_close_reopen() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("db");

        let run_id = RunId::new();
        let ns = Namespace::new(
            "tenant".to_string(),
            "app".to_string(),
            "agent".to_string(),
            run_id,
        );

        // Open database, write via WAL, close
        {
            let db = Database::open(&db_path).unwrap();

            // Write to WAL
            let wal = db.wal();
            let mut wal_guard = wal.lock().unwrap();

            wal_guard
                .append(&WALEntry::BeginTxn {
                    txn_id: 1,
                    run_id,
                    timestamp: now(),
                })
                .unwrap();

            wal_guard
                .append(&WALEntry::Write {
                    run_id,
                    key: Key::new_kv(ns.clone(), "persistent"),
                    value: Value::Bytes(b"data".to_vec()),
                    version: 1,
                })
                .unwrap();

            wal_guard
                .append(&WALEntry::CommitTxn { txn_id: 1, run_id })
                .unwrap();

            drop(wal_guard); // Release lock
            db.flush().unwrap(); // Ensure written to disk
        }

        // Reopen database
        {
            let db = Database::open(&db_path).unwrap();

            // Data should be restored from WAL
            let key = Key::new_kv(ns, "persistent");
            let val = db.storage().get(&key).unwrap().unwrap();

            if let Value::Bytes(bytes) = val.value {
                assert_eq!(bytes, b"data");
            } else {
                panic!("Wrong value type");
            }
        }
    }

    #[test]
    fn test_recovery_discards_incomplete() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("db");

        let run_id = RunId::new();
        let ns = Namespace::new(
            "tenant".to_string(),
            "app".to_string(),
            "agent".to_string(),
            run_id,
        );

        // Write incomplete transaction to WAL (simulates crash)
        {
            std::fs::create_dir_all(db_path.join("wal")).unwrap();
            let mut wal =
                WAL::open(db_path.join("wal/current.wal"), DurabilityMode::Strict).unwrap();

            wal.append(&WALEntry::BeginTxn {
                txn_id: 1,
                run_id,
                timestamp: now(),
            })
            .unwrap();

            wal.append(&WALEntry::Write {
                run_id,
                key: Key::new_kv(ns.clone(), "incomplete"),
                value: Value::Bytes(b"never_committed".to_vec()),
                version: 1,
            })
            .unwrap();

            // NO CommitTxn - simulates crash
        }

        // Open database (recovery should discard incomplete transaction)
        let db = Database::open(&db_path).unwrap();

        // Incomplete transaction should NOT be in storage
        let key = Key::new_kv(ns, "incomplete");
        assert!(db.storage().get(&key).unwrap().is_none());
    }

    #[test]
    fn test_corrupted_wal_handled_gracefully() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("db");

        // Create corrupted WAL
        {
            std::fs::create_dir_all(db_path.join("wal")).unwrap();
            let wal_path = db_path.join("wal/current.wal");

            // Write garbage data - WAL decoder will stop at first invalid entry
            std::fs::write(&wal_path, b"CORRUPTED_DATA_NOT_VALID_WAL").unwrap();
        }

        // Open should succeed with empty storage (corrupted entries are skipped)
        let result = Database::open(&db_path);
        assert!(result.is_ok());

        let db = result.unwrap();
        // Storage should be empty since no valid entries could be decoded
        assert_eq!(db.storage().current_version(), 0);
    }

    #[test]
    fn test_open_with_different_durability_modes() {
        let temp_dir = TempDir::new().unwrap();

        // Strict mode
        {
            let db =
                Database::open_with_mode(temp_dir.path().join("strict"), DurabilityMode::Strict)
                    .unwrap();
            assert!(db.data_dir().exists());
        }

        // Batched mode
        {
            let db = Database::open_with_mode(
                temp_dir.path().join("batched"),
                DurabilityMode::Batched {
                    interval_ms: 100,
                    batch_size: 1000,
                },
            )
            .unwrap();
            assert!(db.data_dir().exists());
        }

        // Async mode
        {
            let db = Database::open_with_mode(
                temp_dir.path().join("async"),
                DurabilityMode::Async { interval_ms: 50 },
            )
            .unwrap();
            assert!(db.data_dir().exists());
        }
    }

    #[test]
    fn test_data_dir_accessor() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("db");

        let db = Database::open(&db_path).unwrap();

        assert_eq!(db.data_dir(), db_path);
    }

    #[test]
    fn test_flush() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("db");

        let db = Database::open(&db_path).unwrap();

        // Flush should succeed
        assert!(db.flush().is_ok());
    }

    // ========================================================================
    // Transaction API Tests (M2 Story #98)
    // ========================================================================

    fn create_test_namespace(run_id: RunId) -> Namespace {
        Namespace::new(
            "tenant".to_string(),
            "app".to_string(),
            "agent".to_string(),
            run_id,
        )
    }

    #[test]
    fn test_transaction_closure_api() {
        let temp_dir = TempDir::new().unwrap();
        let db = Database::open(temp_dir.path().join("db")).unwrap();

        let run_id = RunId::new();
        let ns = create_test_namespace(run_id);
        let key = Key::new_kv(ns, "test_key");

        // Execute transaction
        let result = db.transaction(run_id, |txn| {
            txn.put(key.clone(), Value::I64(42))?;
            Ok(())
        });

        assert!(result.is_ok());

        // Verify data was committed
        let stored = db.storage().get(&key).unwrap().unwrap();
        assert_eq!(stored.value, Value::I64(42));
    }

    #[test]
    fn test_transaction_returns_closure_value() {
        let temp_dir = TempDir::new().unwrap();
        let db = Database::open(temp_dir.path().join("db")).unwrap();

        let run_id = RunId::new();
        let ns = create_test_namespace(run_id);
        let key = Key::new_kv(ns, "test_key");

        // Pre-populate using transaction
        db.transaction(run_id, |txn| {
            txn.put(key.clone(), Value::I64(100))?;
            Ok(())
        })
        .unwrap();

        // Transaction returns a value
        let result: Result<i64> = db.transaction(run_id, |txn| {
            let val = txn.get(&key)?.unwrap();
            if let Value::I64(n) = val {
                Ok(n)
            } else {
                Err(Error::InvalidState("wrong type".to_string()))
            }
        });

        assert_eq!(result.unwrap(), 100);
    }

    #[test]
    fn test_transaction_read_your_writes() {
        let temp_dir = TempDir::new().unwrap();
        let db = Database::open(temp_dir.path().join("db")).unwrap();

        let run_id = RunId::new();
        let ns = create_test_namespace(run_id);
        let key = Key::new_kv(ns, "ryw_key");

        // Per spec Section 2.1: "Its own uncommitted writes - always visible"
        let result: Result<Value> = db.transaction(run_id, |txn| {
            txn.put(key.clone(), Value::String("written".to_string()))?;

            // Should see our own write
            let val = txn.get(&key)?.unwrap();
            Ok(val)
        });

        assert_eq!(result.unwrap(), Value::String("written".to_string()));
    }

    #[test]
    fn test_transaction_aborts_on_closure_error() {
        let temp_dir = TempDir::new().unwrap();
        let db = Database::open(temp_dir.path().join("db")).unwrap();

        let run_id = RunId::new();
        let ns = create_test_namespace(run_id);
        let key = Key::new_kv(ns, "abort_key");

        // Transaction that errors
        let result: Result<()> = db.transaction(run_id, |txn| {
            txn.put(key.clone(), Value::I64(999))?;
            Err(Error::InvalidState("intentional error".to_string()))
        });

        assert!(result.is_err());

        // Data should NOT be committed
        assert!(db.storage().get(&key).unwrap().is_none());
    }

    #[test]
    fn test_begin_and_commit_manual() {
        let temp_dir = TempDir::new().unwrap();
        let db = Database::open(temp_dir.path().join("db")).unwrap();

        let run_id = RunId::new();
        let ns = create_test_namespace(run_id);
        let key = Key::new_kv(ns, "manual_key");

        // Manual transaction control
        let mut txn = db.begin_transaction(run_id);
        txn.put(key.clone(), Value::I64(123)).unwrap();

        // Commit manually
        db.commit_transaction(&mut txn).unwrap();

        // Verify committed
        let stored = db.storage().get(&key).unwrap().unwrap();
        assert_eq!(stored.value, Value::I64(123));
    }

    #[test]
    fn test_transaction_wal_logging() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("db");
        let run_id = RunId::new();
        let ns = create_test_namespace(run_id);
        let key = Key::new_kv(ns, "wal_key");

        // Execute transaction
        {
            let db = Database::open(&db_path).unwrap();
            db.transaction(run_id, |txn| {
                txn.put(key.clone(), Value::I64(42))?;
                Ok(())
            })
            .unwrap();
        }

        // Reopen database (triggers recovery from WAL)
        {
            let db = Database::open(&db_path).unwrap();
            let stored = db.storage().get(&key).unwrap().unwrap();
            assert_eq!(stored.value, Value::I64(42));
        }
    }

    #[test]
    fn test_transaction_version_allocation() {
        let temp_dir = TempDir::new().unwrap();
        let db = Database::open(temp_dir.path().join("db")).unwrap();

        let run_id = RunId::new();
        let ns = create_test_namespace(run_id);

        // First transaction
        db.transaction(run_id, |txn| {
            txn.put(Key::new_kv(ns.clone(), "key1"), Value::I64(1))?;
            Ok(())
        })
        .unwrap();

        let v1 = db.storage().current_version();
        assert!(v1 > 0);

        // Second transaction
        db.transaction(run_id, |txn| {
            txn.put(Key::new_kv(ns.clone(), "key2"), Value::I64(2))?;
            Ok(())
        })
        .unwrap();

        let v2 = db.storage().current_version();
        assert!(v2 > v1); // Versions must be monotonic
    }

    #[test]
    fn test_coordinator_accessor() {
        let temp_dir = TempDir::new().unwrap();
        let db = Database::open(temp_dir.path().join("db")).unwrap();

        // Coordinator should be accessible
        let _coordinator = db.coordinator();
        // Initial version should be 0 for empty database
        assert_eq!(db.coordinator().current_version(), 0);
    }

    #[test]
    fn test_transaction_metrics() {
        let temp_dir = TempDir::new().unwrap();
        let db = Database::open(temp_dir.path().join("db")).unwrap();

        let run_id = RunId::new();
        let ns = create_test_namespace(run_id);

        // Initial metrics should be zero
        let initial_metrics = db.metrics();
        assert_eq!(initial_metrics.total_started, 0);
        assert_eq!(initial_metrics.total_committed, 0);
        assert_eq!(initial_metrics.total_aborted, 0);

        // Commit a transaction
        db.transaction(run_id, |txn| {
            txn.put(Key::new_kv(ns.clone(), "key1"), Value::I64(1))?;
            Ok(())
        })
        .unwrap();

        let after_commit = db.metrics();
        assert_eq!(after_commit.total_started, 1);
        assert_eq!(after_commit.total_committed, 1);
        assert_eq!(after_commit.total_aborted, 0);
        assert_eq!(after_commit.commit_rate, 1.0);

        // Abort a transaction
        let _: Result<()> = db.transaction(run_id, |txn| {
            txn.put(Key::new_kv(ns.clone(), "key2"), Value::I64(2))?;
            Err(Error::InvalidState("intentional abort".to_string()))
        });

        let after_abort = db.metrics();
        assert_eq!(after_abort.total_started, 2);
        assert_eq!(after_abort.total_committed, 1);
        assert_eq!(after_abort.total_aborted, 1);
        assert_eq!(after_abort.commit_rate, 0.5);
    }

    #[test]
    fn test_transaction_multi_key() {
        let temp_dir = TempDir::new().unwrap();
        let db = Database::open(temp_dir.path().join("db")).unwrap();

        let run_id = RunId::new();
        let ns = create_test_namespace(run_id);

        // Transaction with multiple keys
        db.transaction(run_id, |txn| {
            txn.put(Key::new_kv(ns.clone(), "a"), Value::I64(1))?;
            txn.put(Key::new_kv(ns.clone(), "b"), Value::I64(2))?;
            txn.put(Key::new_kv(ns.clone(), "c"), Value::I64(3))?;
            Ok(())
        })
        .unwrap();

        // All keys should have the same version (per spec Section 6.1)
        let v_a = db
            .storage()
            .get(&Key::new_kv(ns.clone(), "a"))
            .unwrap()
            .unwrap()
            .version;
        let v_b = db
            .storage()
            .get(&Key::new_kv(ns.clone(), "b"))
            .unwrap()
            .unwrap()
            .version;
        let v_c = db
            .storage()
            .get(&Key::new_kv(ns.clone(), "c"))
            .unwrap()
            .unwrap()
            .version;

        assert_eq!(v_a, v_b);
        assert_eq!(v_b, v_c);
    }

    #[test]
    fn test_transaction_with_delete() {
        let temp_dir = TempDir::new().unwrap();
        let db = Database::open(temp_dir.path().join("db")).unwrap();

        let run_id = RunId::new();
        let ns = create_test_namespace(run_id);
        let key = Key::new_kv(ns, "to_delete");

        // Create key
        db.transaction(run_id, |txn| {
            txn.put(key.clone(), Value::I64(100))?;
            Ok(())
        })
        .unwrap();

        assert!(db.storage().get(&key).unwrap().is_some());

        // Delete key
        db.transaction(run_id, |txn| {
            txn.delete(key.clone())?;
            Ok(())
        })
        .unwrap();

        // Key should be gone
        assert!(db.storage().get(&key).unwrap().is_none());
    }

    // ========================================================================
    // Implicit Transaction Tests (M1 Compatibility - Story #100)
    // ========================================================================

    #[test]
    fn test_implicit_put() {
        let temp_dir = TempDir::new().unwrap();
        let db = Database::open(temp_dir.path().join("db")).unwrap();

        let run_id = RunId::new();
        let ns = create_test_namespace(run_id);
        let key = Key::new_kv(ns, "implicit_put");

        // M1-style put
        db.put(run_id, key.clone(), Value::I64(42)).unwrap();

        // Verify stored
        let stored = db.storage().get(&key).unwrap().unwrap();
        assert_eq!(stored.value, Value::I64(42));
    }

    #[test]
    fn test_implicit_get() {
        let temp_dir = TempDir::new().unwrap();
        let db = Database::open(temp_dir.path().join("db")).unwrap();

        let run_id = RunId::new();
        let ns = create_test_namespace(run_id);
        let key = Key::new_kv(ns, "implicit_get");

        // Pre-populate using put
        db.put(run_id, key.clone(), Value::I64(100)).unwrap();

        // M1-style get
        let result = db.get(&key).unwrap();
        assert!(result.is_some());
        assert_eq!(result.unwrap().value, Value::I64(100));
    }

    #[test]
    fn test_implicit_get_nonexistent() {
        let temp_dir = TempDir::new().unwrap();
        let db = Database::open(temp_dir.path().join("db")).unwrap();

        let run_id = RunId::new();
        let ns = create_test_namespace(run_id);
        let key = Key::new_kv(ns, "nonexistent");

        // M1-style get for nonexistent key
        let result = db.get(&key).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_implicit_delete() {
        let temp_dir = TempDir::new().unwrap();
        let db = Database::open(temp_dir.path().join("db")).unwrap();

        let run_id = RunId::new();
        let ns = create_test_namespace(run_id);
        let key = Key::new_kv(ns, "to_delete_implicit");

        // Pre-populate
        db.put(run_id, key.clone(), Value::I64(1)).unwrap();
        assert!(db.get(&key).unwrap().is_some());

        // M1-style delete
        db.delete(run_id, key.clone()).unwrap();

        // Verify deleted
        assert!(db.get(&key).unwrap().is_none());
    }

    #[test]
    fn test_implicit_cas_success() {
        let temp_dir = TempDir::new().unwrap();
        let db = Database::open(temp_dir.path().join("db")).unwrap();

        let run_id = RunId::new();
        let ns = create_test_namespace(run_id);
        let key = Key::new_kv(ns, "cas_key");

        // Initial put
        db.put(run_id, key.clone(), Value::I64(1)).unwrap();
        let current = db.get(&key).unwrap().unwrap();

        // CAS with correct version
        db.cas(run_id, key.clone(), current.version, Value::I64(2))
            .unwrap();

        // Verify updated
        let updated = db.get(&key).unwrap().unwrap();
        assert_eq!(updated.value, Value::I64(2));
    }

    #[test]
    fn test_implicit_cas_failure() {
        let temp_dir = TempDir::new().unwrap();
        let db = Database::open(temp_dir.path().join("db")).unwrap();

        let run_id = RunId::new();
        let ns = create_test_namespace(run_id);
        let key = Key::new_kv(ns, "cas_fail");

        // Initial put
        db.put(run_id, key.clone(), Value::I64(1)).unwrap();

        // CAS with wrong version
        let result = db.cas(run_id, key.clone(), 999, Value::I64(2));
        assert!(result.is_err());

        // Value should be unchanged
        let stored = db.get(&key).unwrap().unwrap();
        assert_eq!(stored.value, Value::I64(1));
    }

    #[test]
    fn test_implicit_operations_durable() {
        // Verify implicit operations are written to WAL and survive restart
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("db");
        let run_id = RunId::new();
        let ns = create_test_namespace(run_id);

        let key1 = Key::new_kv(ns.clone(), "durable1");
        let key2 = Key::new_kv(ns.clone(), "durable2");

        // Write and close
        {
            let db = Database::open(&db_path).unwrap();
            db.put(run_id, key1.clone(), Value::I64(100)).unwrap();
            db.put(run_id, key2.clone(), Value::String("test".to_string()))
                .unwrap();
        }

        // Reopen and verify
        {
            let db = Database::open(&db_path).unwrap();
            let v1 = db.get(&key1).unwrap().unwrap();
            let v2 = db.get(&key2).unwrap().unwrap();

            assert_eq!(v1.value, Value::I64(100));
            assert_eq!(v2.value, Value::String("test".to_string()));
        }
    }

    #[test]
    fn test_implicit_cas_create_new_key() {
        let temp_dir = TempDir::new().unwrap();
        let db = Database::open(temp_dir.path().join("db")).unwrap();

        let run_id = RunId::new();
        let ns = create_test_namespace(run_id);
        let key = Key::new_kv(ns, "new_cas_key");

        // CAS with version 0 should create new key (key doesn't exist)
        db.cas(run_id, key.clone(), 0, Value::I64(42)).unwrap();

        // Verify created
        let stored = db.get(&key).unwrap().unwrap();
        assert_eq!(stored.value, Value::I64(42));
    }

    #[test]
    fn test_implicit_mixed_operations() {
        let temp_dir = TempDir::new().unwrap();
        let db = Database::open(temp_dir.path().join("db")).unwrap();

        let run_id = RunId::new();
        let ns = create_test_namespace(run_id);

        // Put, Get, CAS, Get, Delete, Get
        let key = Key::new_kv(ns, "mixed_ops");

        // Put
        db.put(run_id, key.clone(), Value::I64(1)).unwrap();
        let v1 = db.get(&key).unwrap().unwrap();
        assert_eq!(v1.value, Value::I64(1));

        // CAS to increment
        db.cas(run_id, key.clone(), v1.version, Value::I64(2))
            .unwrap();
        let v2 = db.get(&key).unwrap().unwrap();
        assert_eq!(v2.value, Value::I64(2));

        // Delete
        db.delete(run_id, key.clone()).unwrap();
        assert!(db.get(&key).unwrap().is_none());
    }

    // ========================================================================
    // Retry Tests (Story #101)
    // ========================================================================

    #[test]
    fn test_retry_config_default() {
        let config = RetryConfig::default();
        assert_eq!(config.max_retries, 3);
        assert_eq!(config.base_delay_ms, 10);
        assert_eq!(config.max_delay_ms, 100);
    }

    #[test]
    fn test_retry_config_builder() {
        let config = RetryConfig::new()
            .with_max_retries(5)
            .with_base_delay_ms(20)
            .with_max_delay_ms(200);

        assert_eq!(config.max_retries, 5);
        assert_eq!(config.base_delay_ms, 20);
        assert_eq!(config.max_delay_ms, 200);
    }

    #[test]
    fn test_retry_config_no_retry() {
        let config = RetryConfig::no_retry();
        assert_eq!(config.max_retries, 0);
    }

    #[test]
    fn test_retry_config_delay_calculation() {
        let config = RetryConfig {
            max_retries: 5,
            base_delay_ms: 10,
            max_delay_ms: 100,
        };

        // Exponential backoff: 10, 20, 40, 80, 100 (capped)
        assert_eq!(config.calculate_delay(0).as_millis(), 10);
        assert_eq!(config.calculate_delay(1).as_millis(), 20);
        assert_eq!(config.calculate_delay(2).as_millis(), 40);
        assert_eq!(config.calculate_delay(3).as_millis(), 80);
        assert_eq!(config.calculate_delay(4).as_millis(), 100); // Capped at max
        assert_eq!(config.calculate_delay(5).as_millis(), 100); // Still capped
    }

    #[test]
    fn test_transaction_with_retry_success() {
        let temp_dir = TempDir::new().unwrap();
        let db = Database::open(temp_dir.path().join("db")).unwrap();

        let run_id = RunId::new();
        let ns = create_test_namespace(run_id);
        let key = Key::new_kv(ns, "retry_success");

        // Transaction with retry that succeeds on first try
        let result = db.transaction_with_retry(run_id, RetryConfig::default(), |txn| {
            txn.put(key.clone(), Value::I64(42))?;
            Ok(42)
        });

        assert_eq!(result.unwrap(), 42);

        // Verify stored
        let stored = db.get(&key).unwrap().unwrap();
        assert_eq!(stored.value, Value::I64(42));
    }

    #[test]
    fn test_transaction_with_retry_non_conflict_error_not_retried() {
        use std::sync::atomic::{AtomicU64, Ordering};

        let temp_dir = TempDir::new().unwrap();
        let db = Database::open(temp_dir.path().join("db")).unwrap();

        let run_id = RunId::new();
        let attempts = AtomicU64::new(0);

        let result: Result<()> =
            db.transaction_with_retry(run_id, RetryConfig::default(), |_txn| {
                attempts.fetch_add(1, Ordering::Relaxed);
                Err(Error::InvalidState("not a conflict".to_string()))
            });

        // Should only try once (non-conflict errors don't retry)
        assert_eq!(attempts.load(Ordering::Relaxed), 1);
        assert!(result.is_err());
    }

    #[test]
    fn test_transaction_with_retry_conflict_is_retried() {
        use std::sync::atomic::{AtomicU64, Ordering};

        let temp_dir = TempDir::new().unwrap();
        let db = Database::open(temp_dir.path().join("db")).unwrap();

        let run_id = RunId::new();
        let ns = create_test_namespace(run_id);
        let key = Key::new_kv(ns, "retry_conflict");
        let attempts = AtomicU64::new(0);

        let config = RetryConfig {
            max_retries: 2,
            base_delay_ms: 1, // Short delay for tests
            max_delay_ms: 10,
        };

        // Conflict on first 2 attempts, succeed on third
        let result: Result<()> = db.transaction_with_retry(run_id, config, |txn| {
            let count = attempts.fetch_add(1, Ordering::Relaxed);
            if count < 2 {
                Err(Error::TransactionConflict("simulated conflict".to_string()))
            } else {
                txn.put(key.clone(), Value::I64(count as i64))?;
                Ok(())
            }
        });

        assert!(result.is_ok());
        assert_eq!(attempts.load(Ordering::Relaxed), 3); // Tried 3 times
    }

    #[test]
    fn test_transaction_with_retry_max_retries_exceeded() {
        use std::sync::atomic::{AtomicU64, Ordering};

        let temp_dir = TempDir::new().unwrap();
        let db = Database::open(temp_dir.path().join("db")).unwrap();

        let run_id = RunId::new();
        let attempts = AtomicU64::new(0);

        let config = RetryConfig {
            max_retries: 2,
            base_delay_ms: 1,
            max_delay_ms: 10,
        };

        // Always return conflict
        let result: Result<()> = db.transaction_with_retry(run_id, config, |_txn| {
            attempts.fetch_add(1, Ordering::Relaxed);
            Err(Error::TransactionConflict("always conflict".to_string()))
        });

        // Should try 3 times (initial + 2 retries)
        assert_eq!(attempts.load(Ordering::Relaxed), 3);
        assert!(result.is_err());
        assert!(result.unwrap_err().is_conflict());
    }

    #[test]
    fn test_transaction_with_retry_no_retry_config() {
        use std::sync::atomic::{AtomicU64, Ordering};

        let temp_dir = TempDir::new().unwrap();
        let db = Database::open(temp_dir.path().join("db")).unwrap();

        let run_id = RunId::new();
        let attempts = AtomicU64::new(0);

        // No retries configured
        let result: Result<()> =
            db.transaction_with_retry(run_id, RetryConfig::no_retry(), |_txn| {
                attempts.fetch_add(1, Ordering::Relaxed);
                Err(Error::TransactionConflict("conflict".to_string()))
            });

        // Should try exactly once
        assert_eq!(attempts.load(Ordering::Relaxed), 1);
        assert!(result.is_err());
    }

    #[test]
    fn test_transaction_with_retry_returns_value() {
        let temp_dir = TempDir::new().unwrap();
        let db = Database::open(temp_dir.path().join("db")).unwrap();

        let run_id = RunId::new();
        let ns = create_test_namespace(run_id);
        let key = Key::new_kv(ns, "return_value");

        // Pre-populate
        db.put(run_id, key.clone(), Value::I64(100)).unwrap();

        // Transaction returns a value
        // Note: txn.get() returns Option<Value>, not Option<VersionedValue>
        let result: Result<i64> =
            db.transaction_with_retry(run_id, RetryConfig::default(), |txn| {
                let val = txn.get(&key)?.unwrap();
                if let Value::I64(n) = val {
                    Ok(n)
                } else {
                    Err(Error::InvalidState("wrong type".to_string()))
                }
            });

        assert_eq!(result.unwrap(), 100);
    }

    #[test]
    fn test_transaction_with_retry_read_modify_write() {
        use std::sync::atomic::{AtomicU64, Ordering};

        let temp_dir = TempDir::new().unwrap();
        let db = Database::open(temp_dir.path().join("db")).unwrap();

        let run_id = RunId::new();
        let ns = create_test_namespace(run_id);
        let key = Key::new_kv(ns, "counter");
        let attempts = AtomicU64::new(0);

        // Initialize counter
        db.put(run_id, key.clone(), Value::I64(0)).unwrap();

        let config = RetryConfig {
            max_retries: 5,
            base_delay_ms: 1,
            max_delay_ms: 10,
        };

        // Simulate conflict on first attempt, then succeed
        // Note: txn.get() returns Option<Value>, not Option<VersionedValue>
        let result = db.transaction_with_retry(run_id, config, |txn| {
            let count = attempts.fetch_add(1, Ordering::Relaxed);

            // Read
            let val = txn.get(&key)?.unwrap();
            let n = match val {
                Value::I64(n) => n,
                _ => return Err(Error::InvalidState("wrong type".to_string())),
            };

            // Simulate conflict on first attempt
            if count == 0 {
                return Err(Error::TransactionConflict("simulated conflict".to_string()));
            }

            // Write incremented value
            txn.put(key.clone(), Value::I64(n + 1))?;
            Ok(n + 1)
        });

        assert_eq!(result.unwrap(), 1);
        assert_eq!(attempts.load(Ordering::Relaxed), 2); // Tried twice

        // Verify final value
        let stored = db.get(&key).unwrap().unwrap();
        assert_eq!(stored.value, Value::I64(1));
    }

    // ========================================================================
    // Timeout Tests (Story #102)
    // ========================================================================

    #[test]
    fn test_transaction_is_expired() {
        use std::thread;

        let temp_dir = TempDir::new().unwrap();
        let db = Database::open(temp_dir.path().join("db")).unwrap();

        let run_id = RunId::new();
        let txn = db.begin_transaction(run_id);

        // Should not be expired immediately
        assert!(!txn.is_expired(Duration::from_secs(1)));

        // Sleep briefly
        thread::sleep(Duration::from_millis(50));

        // Should be expired with very short timeout
        assert!(txn.is_expired(Duration::from_millis(10)));

        // Should not be expired with longer timeout
        assert!(!txn.is_expired(Duration::from_secs(10)));
    }

    #[test]
    fn test_transaction_with_timeout_success() {
        let temp_dir = TempDir::new().unwrap();
        let db = Database::open(temp_dir.path().join("db")).unwrap();

        let run_id = RunId::new();
        let ns = create_test_namespace(run_id);
        let key = Key::new_kv(ns, "timeout_success");

        // Transaction completes within timeout
        let result = db.transaction_with_timeout(run_id, Duration::from_secs(5), |txn| {
            txn.put(key.clone(), Value::I64(42))?;
            Ok(42)
        });

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 42);

        // Verify stored
        let stored = db.get(&key).unwrap().unwrap();
        assert_eq!(stored.value, Value::I64(42));
    }

    #[test]
    fn test_transaction_with_timeout_expired() {
        use std::thread;

        let temp_dir = TempDir::new().unwrap();
        let db = Database::open(temp_dir.path().join("db")).unwrap();

        let run_id = RunId::new();
        let ns = create_test_namespace(run_id);
        let key = Key::new_kv(ns, "timeout_expired");

        // Transaction exceeds timeout
        let result: Result<()> = db.transaction_with_timeout(
            run_id,
            Duration::from_millis(10), // Very short timeout
            |txn| {
                txn.put(key.clone(), Value::I64(999))?;
                // Sleep to exceed timeout
                thread::sleep(Duration::from_millis(50));
                Ok(())
            },
        );

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.is_timeout());

        // Data should NOT be committed
        assert!(db.get(&key).unwrap().is_none());
    }

    #[test]
    fn test_transaction_with_timeout_normal_not_affected() {
        let temp_dir = TempDir::new().unwrap();
        let db = Database::open(temp_dir.path().join("db")).unwrap();

        let run_id = RunId::new();
        let ns = create_test_namespace(run_id);

        // Run many quick transactions with timeout
        for i in 0..100 {
            let key = Key::new_kv(ns.clone(), &format!("key_{}", i));
            let result = db.transaction_with_timeout(run_id, Duration::from_secs(5), |txn| {
                txn.put(key.clone(), Value::I64(i as i64))?;
                Ok(())
            });
            assert!(result.is_ok());
        }

        // All should be stored
        for i in 0..100 {
            let key = Key::new_kv(ns.clone(), &format!("key_{}", i));
            let val = db.get(&key).unwrap().unwrap();
            assert_eq!(val.value, Value::I64(i as i64));
        }
    }

    #[test]
    fn test_transaction_elapsed() {
        use std::thread;

        let temp_dir = TempDir::new().unwrap();
        let db = Database::open(temp_dir.path().join("db")).unwrap();

        let run_id = RunId::new();
        let txn = db.begin_transaction(run_id);

        // Elapsed should be very small initially
        let initial = txn.elapsed();
        assert!(initial < Duration::from_millis(100));

        // After sleep, elapsed should increase
        thread::sleep(Duration::from_millis(50));
        let after = txn.elapsed();
        assert!(after >= Duration::from_millis(50));
        assert!(after > initial);
    }
}
