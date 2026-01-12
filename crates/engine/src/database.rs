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
use in_mem_core::types::RunId;
use in_mem_durability::wal::{DurabilityMode, WAL};
use in_mem_storage::UnifiedStore;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use tracing::info;

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
        // 1. Validate
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

            // Write all operations
            for (key, value) in &txn.write_set {
                wal_writer.write_put(key.clone(), value.clone(), commit_version)?;
            }
            for key in &txn.delete_set {
                wal_writer.write_delete(key.clone(), commit_version)?;
            }

            // Write CommitTxn (this also flushes)
            wal_writer.write_commit()?;
        }

        // 4. Apply to storage
        for (key, value) in &txn.write_set {
            self.storage
                .put_with_version(key.clone(), value.clone(), commit_version, None)?;
        }
        for key in &txn.delete_set {
            self.storage.delete_with_version(key, commit_version)?;
        }

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
}
