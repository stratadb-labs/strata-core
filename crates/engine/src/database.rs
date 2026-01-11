//! Database struct and open/close logic
//!
//! This module provides the main Database struct that orchestrates:
//! - Storage initialization
//! - WAL opening
//! - Automatic recovery on startup
//! - Run tracking (begin_run, end_run)

use crate::run::RunTracker;
use in_mem_core::error::{Error, Result};
use in_mem_core::types::{Key, Namespace, RunId};
use in_mem_core::value::{now, RunMetadataEntry, Value};
use in_mem_core::Storage;
use in_mem_durability::replay_wal;
use in_mem_durability::wal::{DurabilityMode, WALEntry, WAL};
use in_mem_storage::UnifiedStore;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use tracing::info;

/// Main database struct
///
/// Orchestrates storage, WAL, and recovery.
/// Create a database by calling `Database::open()`.
///
/// # Thread Safety
///
/// The Database struct is designed to be thread-safe:
/// - `storage` is wrapped in `Arc<UnifiedStore>` - UnifiedStore handles its own
///   internal synchronization
/// - `wal` is wrapped in `Arc<Mutex<WAL>>` - exclusive access via mutex
///
/// The Database can be shared across threads by wrapping it in `Arc<Database>`.
///
/// # Drop Behavior
///
/// When the Database is dropped, the WAL is automatically flushed to ensure
/// all pending writes are persisted to disk. This provides clean shutdown
/// semantics.
pub struct Database {
    /// Data directory path
    data_dir: PathBuf,

    /// Unified storage (thread-safe)
    storage: Arc<UnifiedStore>,

    /// Write-ahead log (protected by mutex for exclusive access)
    wal: Arc<Mutex<WAL>>,

    /// Durability mode for WAL operations
    durability_mode: DurabilityMode,

    /// Run tracker for active runs
    run_tracker: Arc<RunTracker>,

    /// Transaction ID counter for internal operations
    next_txn_id: AtomicU64,
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
    pub fn open_with_mode<P: AsRef<Path>>(
        path: P,
        durability_mode: DurabilityMode,
    ) -> Result<Self> {
        let data_dir = path.as_ref().to_path_buf();

        // Create data directory
        std::fs::create_dir_all(&data_dir).map_err(Error::IoError)?;

        // Create WAL directory and open WAL
        let wal_dir = data_dir.join("wal");
        std::fs::create_dir_all(&wal_dir).map_err(Error::IoError)?;

        let wal_path = wal_dir.join("current.wal");
        let wal = WAL::open(&wal_path, durability_mode)?;

        // Create empty storage
        let storage = Arc::new(UnifiedStore::new());

        // Replay WAL to restore state
        let stats = replay_wal(&wal, storage.as_ref())?;

        info!(
            txns_applied = stats.txns_applied,
            writes_applied = stats.writes_applied,
            deletes_applied = stats.deletes_applied,
            incomplete_txns = stats.incomplete_txns,
            orphaned_entries = stats.orphaned_entries,
            final_version = stats.final_version,
            "Recovery complete"
        );

        // Initialize txn_id counter from 1 - each run_id + txn_id combination is unique
        let next_txn_id = AtomicU64::new(1);

        Ok(Self {
            data_dir,
            storage,
            wal: Arc::new(Mutex::new(wal)),
            durability_mode,
            run_tracker: Arc::new(RunTracker::new()),
            next_txn_id,
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

    /// Get the durability mode
    ///
    /// Returns the durability mode that was configured when the database was opened.
    pub fn durability_mode(&self) -> DurabilityMode {
        self.durability_mode
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

    // ========================================
    // Run Tracking Methods
    // ========================================

    /// Begin a new run
    ///
    /// Creates run metadata and marks the run as active.
    /// The metadata is stored in storage and WAL for durability.
    ///
    /// # Arguments
    ///
    /// * `run_id` - Unique identifier for this run
    /// * `tags` - Optional tags for categorization
    ///
    /// # Returns
    ///
    /// Ok(()) on success
    pub fn begin_run(&self, run_id: RunId, tags: Vec<(String, String)>) -> Result<()> {
        let first_version = self.storage.current_version();
        let metadata = RunMetadataEntry {
            run_id,
            parent_run_id: None,
            status: "running".to_string(),
            created_at: now(),
            completed_at: None,
            first_version,
            last_version: 0,
            tags,
        };

        // Store metadata in storage via WAL transaction for durability
        let ns = Namespace::new(
            "system".to_string(),
            "in-mem".to_string(),
            "run-tracker".to_string(),
            run_id,
        );
        let key = Key::new_run_metadata(ns, run_id);

        // Write to WAL as transaction
        let txn_id = self.next_txn_id.fetch_add(1, Ordering::SeqCst);
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;

        {
            let mut wal = self.wal.lock().unwrap();
            wal.append(&WALEntry::BeginTxn {
                txn_id,
                run_id,
                timestamp,
            })?;

            // Get version for this write
            let version = self.storage.current_version() + 1;

            wal.append(&WALEntry::Write {
                run_id,
                key: key.clone(),
                value: Value::RunMetadata(metadata.clone()),
                version,
            })?;

            wal.append(&WALEntry::CommitTxn { txn_id, run_id })?;
        }

        // Apply to storage
        self.storage
            .put(key, Value::RunMetadata(metadata.clone()), None)?;

        // Track as active
        self.run_tracker.begin_run(metadata)?;

        Ok(())
    }

    /// Begin a forked run (with parent)
    ///
    /// Creates run metadata with a parent reference for forked runs.
    /// The metadata is stored in storage and WAL for durability.
    ///
    /// # Arguments
    ///
    /// * `run_id` - Unique identifier for this run
    /// * `parent_run_id` - The parent run this was forked from
    /// * `tags` - Optional tags for categorization
    ///
    /// # Returns
    ///
    /// Ok(()) on success
    pub fn begin_forked_run(
        &self,
        run_id: RunId,
        parent_run_id: RunId,
        tags: Vec<(String, String)>,
    ) -> Result<()> {
        let first_version = self.storage.current_version();
        let metadata = RunMetadataEntry {
            run_id,
            parent_run_id: Some(parent_run_id),
            status: "running".to_string(),
            created_at: now(),
            completed_at: None,
            first_version,
            last_version: 0,
            tags,
        };

        // Store metadata in storage via WAL transaction for durability
        let ns = Namespace::new(
            "system".to_string(),
            "in-mem".to_string(),
            "run-tracker".to_string(),
            run_id,
        );
        let key = Key::new_run_metadata(ns, run_id);

        // Write to WAL as transaction
        let txn_id = self.next_txn_id.fetch_add(1, Ordering::SeqCst);
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;

        {
            let mut wal = self.wal.lock().unwrap();
            wal.append(&WALEntry::BeginTxn {
                txn_id,
                run_id,
                timestamp,
            })?;

            let version = self.storage.current_version() + 1;

            wal.append(&WALEntry::Write {
                run_id,
                key: key.clone(),
                value: Value::RunMetadata(metadata.clone()),
                version,
            })?;

            wal.append(&WALEntry::CommitTxn { txn_id, run_id })?;
        }

        // Apply to storage
        self.storage
            .put(key, Value::RunMetadata(metadata.clone()), None)?;

        // Track as active
        self.run_tracker.begin_run(metadata)?;

        Ok(())
    }

    /// End a run
    ///
    /// Updates run metadata with completion time and final version,
    /// then removes from active tracking. The update is persisted via WAL.
    ///
    /// # Arguments
    ///
    /// * `run_id` - The ID of the run to end
    ///
    /// # Returns
    ///
    /// Ok(()) on success (even if run was not active)
    pub fn end_run(&self, run_id: RunId) -> Result<()> {
        // Get metadata from active runs
        if let Some(mut metadata) = self.run_tracker.get_active(run_id) {
            metadata.completed_at = Some(now());
            metadata.last_version = self.storage.current_version();
            metadata.status = "completed".to_string();

            // Update in storage via WAL transaction
            let ns = Namespace::new(
                "system".to_string(),
                "in-mem".to_string(),
                "run-tracker".to_string(),
                run_id,
            );
            let key = Key::new_run_metadata(ns, run_id);

            // Write to WAL as transaction
            let txn_id = self.next_txn_id.fetch_add(1, Ordering::SeqCst);
            let timestamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs() as i64;

            {
                let mut wal = self.wal.lock().unwrap();
                wal.append(&WALEntry::BeginTxn {
                    txn_id,
                    run_id,
                    timestamp,
                })?;

                let version = self.storage.current_version() + 1;

                wal.append(&WALEntry::Write {
                    run_id,
                    key: key.clone(),
                    value: Value::RunMetadata(metadata.clone()),
                    version,
                })?;

                wal.append(&WALEntry::CommitTxn { txn_id, run_id })?;
            }

            // Apply to storage
            self.storage.put(key, Value::RunMetadata(metadata), None)?;

            // Remove from active
            self.run_tracker.end_run(run_id)?;
        }

        Ok(())
    }

    /// Get run metadata
    ///
    /// Returns metadata for a run, checking active runs first,
    /// then falling back to storage for completed runs.
    ///
    /// # Arguments
    ///
    /// * `run_id` - The ID of the run to look up
    ///
    /// # Returns
    ///
    /// Some(metadata) if found, None otherwise
    pub fn get_run(&self, run_id: RunId) -> Result<Option<RunMetadataEntry>> {
        // Check active runs first
        if let Some(metadata) = self.run_tracker.get_active(run_id) {
            return Ok(Some(metadata));
        }

        // Check storage for completed runs
        let ns = Namespace::new(
            "system".to_string(),
            "in-mem".to_string(),
            "run-tracker".to_string(),
            run_id,
        );
        let key = Key::new_run_metadata(ns, run_id);

        if let Some(versioned) = self.storage.get(&key)? {
            if let Value::RunMetadata(metadata) = versioned.value {
                return Ok(Some(metadata));
            }
        }

        Ok(None)
    }

    /// List active run IDs
    ///
    /// Returns all currently active run IDs.
    pub fn list_active_runs(&self) -> Vec<RunId> {
        self.run_tracker.list_active()
    }

    /// Check if a run is active
    ///
    /// # Arguments
    ///
    /// * `run_id` - The ID of the run to check
    ///
    /// # Returns
    ///
    /// true if the run is currently active
    pub fn is_run_active(&self, run_id: RunId) -> bool {
        self.run_tracker.is_active(run_id)
    }

    /// Get the count of active runs
    pub fn active_run_count(&self) -> usize {
        self.run_tracker.active_count()
    }
}

impl Drop for Database {
    /// Ensures WAL is flushed on clean shutdown
    ///
    /// This provides durability guarantees by flushing any pending
    /// WAL entries to disk when the database is dropped.
    fn drop(&mut self) {
        if let Err(e) = self.flush() {
            eprintln!("Warning: Failed to flush WAL during shutdown: {:?}", e);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use in_mem_durability::wal::WALEntry;
    use tempfile::TempDir;

    fn timestamp() -> i64 {
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
                timestamp: timestamp(),
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
                    timestamp: timestamp(),
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
                timestamp: timestamp(),
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

    #[test]
    fn test_database_thread_safe() {
        use std::sync::Arc;
        use std::thread;

        let temp_dir = TempDir::new().unwrap();
        let db = Arc::new(Database::open(temp_dir.path()).unwrap());

        let mut handles = vec![];

        // Spawn 10 threads accessing storage and flushing
        for i in 0..10 {
            let db = Arc::clone(&db);

            let handle = thread::spawn(move || {
                // Access storage from multiple threads
                let storage = db.storage();
                let _version = storage.current_version();

                // Access data_dir
                let _data_dir = db.data_dir();

                // Flush from multiple threads (some will contend on mutex)
                if i % 2 == 0 {
                    let _ = db.flush();
                }
            });

            handles.push(handle);
        }

        // All threads should complete without deadlock or panic
        for handle in handles {
            handle.join().unwrap();
        }
    }

    #[test]
    fn test_database_drop_flushes() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("db");

        let run_id = RunId::new();
        let ns = Namespace::new(
            "tenant".to_string(),
            "app".to_string(),
            "agent".to_string(),
            run_id,
        );

        // Open database, write to WAL, drop (should flush)
        {
            let db = Database::open(&db_path).unwrap();

            // Write to WAL
            let wal = db.wal();
            let mut wal_guard = wal.lock().unwrap();

            wal_guard
                .append(&WALEntry::BeginTxn {
                    txn_id: 1,
                    run_id,
                    timestamp: timestamp(),
                })
                .unwrap();

            wal_guard
                .append(&WALEntry::Write {
                    run_id,
                    key: Key::new_kv(ns.clone(), "drop_test"),
                    value: Value::Bytes(b"value".to_vec()),
                    version: 1,
                })
                .unwrap();

            wal_guard
                .append(&WALEntry::CommitTxn { txn_id: 1, run_id })
                .unwrap();

            drop(wal_guard); // Release lock before drop
                             // Database is dropped here - should flush WAL automatically
        }

        // Reopen database - data should be recovered
        let db = Database::open(&db_path).unwrap();
        let key = Key::new_kv(ns, "drop_test");
        let val = db.storage().get(&key).unwrap().unwrap();

        if let Value::Bytes(bytes) = val.value {
            assert_eq!(bytes, b"value");
        } else {
            panic!("Wrong value type");
        }
    }

    #[test]
    fn test_durability_mode_accessor() {
        let temp_dir = TempDir::new().unwrap();

        // Default mode (should be Batched)
        let db = Database::open(temp_dir.path().join("default")).unwrap();
        assert!(matches!(
            db.durability_mode(),
            DurabilityMode::Batched { .. }
        ));

        // Strict mode
        let db = Database::open_with_mode(temp_dir.path().join("strict"), DurabilityMode::Strict)
            .unwrap();
        assert!(matches!(db.durability_mode(), DurabilityMode::Strict));

        // Async mode
        let db = Database::open_with_mode(
            temp_dir.path().join("async"),
            DurabilityMode::Async { interval_ms: 100 },
        )
        .unwrap();
        assert!(matches!(
            db.durability_mode(),
            DurabilityMode::Async { interval_ms: 100 }
        ));
    }

    #[test]
    fn test_database_reopen_same_path() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("db");

        // Open and close multiple times
        for _ in 0..3 {
            let db = Database::open(&db_path).unwrap();
            assert_eq!(db.data_dir(), db_path);
            // Database dropped here
        }
    }

    // ========================================
    // Run Tracking Tests
    // ========================================

    #[test]
    fn test_run_lifecycle() {
        let temp_dir = TempDir::new().unwrap();
        let db = Database::open(temp_dir.path()).unwrap();

        let run_id = RunId::new();

        // Begin run
        db.begin_run(run_id, vec![("env".to_string(), "test".to_string())])
            .unwrap();
        assert!(db.is_run_active(run_id));
        assert_eq!(db.active_run_count(), 1);

        // Get run metadata
        let metadata = db.get_run(run_id).unwrap().unwrap();
        assert_eq!(metadata.run_id, run_id);
        assert_eq!(metadata.status, "running");
        assert!(metadata.completed_at.is_none());
        assert_eq!(metadata.tags.len(), 1);
        assert_eq!(metadata.tags[0], ("env".to_string(), "test".to_string()));

        // End run
        db.end_run(run_id).unwrap();
        assert!(!db.is_run_active(run_id));
        assert_eq!(db.active_run_count(), 0);

        // Metadata still retrievable from storage
        let metadata = db.get_run(run_id).unwrap().unwrap();
        assert_eq!(metadata.status, "completed");
        assert!(metadata.completed_at.is_some());
    }

    #[test]
    fn test_multiple_active_runs() {
        let temp_dir = TempDir::new().unwrap();
        let db = Database::open(temp_dir.path()).unwrap();

        let run1 = RunId::new();
        let run2 = RunId::new();
        let run3 = RunId::new();

        db.begin_run(run1, vec![]).unwrap();
        db.begin_run(run2, vec![]).unwrap();
        db.begin_run(run3, vec![]).unwrap();

        assert_eq!(db.active_run_count(), 3);
        let active = db.list_active_runs();
        assert!(active.contains(&run1));
        assert!(active.contains(&run2));
        assert!(active.contains(&run3));

        db.end_run(run2).unwrap();

        assert_eq!(db.active_run_count(), 2);
        let active = db.list_active_runs();
        assert!(active.contains(&run1));
        assert!(!active.contains(&run2));
        assert!(active.contains(&run3));
    }

    #[test]
    fn test_run_metadata_persistence() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("db");

        let run_id = RunId::new();

        // Create and end run
        {
            let db = Database::open(&db_path).unwrap();
            db.begin_run(run_id, vec![("key".to_string(), "value".to_string())])
                .unwrap();
            db.end_run(run_id).unwrap();
            db.flush().unwrap();
        }

        // Reopen and verify metadata persisted
        {
            let db = Database::open(&db_path).unwrap();

            // Run should not be active (in-memory tracker is fresh)
            assert!(!db.is_run_active(run_id));

            // But metadata should still be retrievable from storage
            let metadata = db.get_run(run_id).unwrap().unwrap();
            assert_eq!(metadata.run_id, run_id);
            assert_eq!(metadata.status, "completed");
            assert_eq!(metadata.tags.len(), 1);
            assert_eq!(metadata.tags[0], ("key".to_string(), "value".to_string()));
        }
    }

    #[test]
    fn test_forked_run() {
        let temp_dir = TempDir::new().unwrap();
        let db = Database::open(temp_dir.path()).unwrap();

        let parent_id = RunId::new();
        let child_id = RunId::new();

        // Start parent run
        db.begin_run(parent_id, vec![]).unwrap();

        // Fork child run
        db.begin_forked_run(
            child_id,
            parent_id,
            vec![("forked".to_string(), "true".to_string())],
        )
        .unwrap();

        // Verify child has parent reference
        let child = db.get_run(child_id).unwrap().unwrap();
        assert_eq!(child.parent_run_id, Some(parent_id));
        assert_eq!(child.tags[0], ("forked".to_string(), "true".to_string()));

        // Both should be active
        assert!(db.is_run_active(parent_id));
        assert!(db.is_run_active(child_id));
        assert_eq!(db.active_run_count(), 2);
    }

    #[test]
    fn test_get_run_not_found() {
        let temp_dir = TempDir::new().unwrap();
        let db = Database::open(temp_dir.path()).unwrap();

        let run_id = RunId::new();
        let result = db.get_run(run_id).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_end_run_not_active() {
        let temp_dir = TempDir::new().unwrap();
        let db = Database::open(temp_dir.path()).unwrap();

        let run_id = RunId::new();

        // End run that was never started - should succeed (no-op)
        let result = db.end_run(run_id);
        assert!(result.is_ok());
    }

    #[test]
    fn test_run_version_tracking() {
        let temp_dir = TempDir::new().unwrap();
        let db = Database::open(temp_dir.path()).unwrap();

        let run_id = RunId::new();

        // Get initial version
        let initial_version = db.storage().current_version();

        // Begin run
        db.begin_run(run_id, vec![]).unwrap();

        let metadata = db.get_run(run_id).unwrap().unwrap();
        assert_eq!(metadata.first_version, initial_version);

        // Make some writes to bump version
        let ns = Namespace::new(
            "tenant".to_string(),
            "app".to_string(),
            "agent".to_string(),
            run_id,
        );
        db.storage()
            .put(Key::new_kv(ns.clone(), "key1"), Value::I64(1), None)
            .unwrap();
        db.storage()
            .put(Key::new_kv(ns, "key2"), Value::I64(2), None)
            .unwrap();

        // End run
        db.end_run(run_id).unwrap();

        // Verify last_version is updated
        let metadata = db.get_run(run_id).unwrap().unwrap();
        assert!(metadata.last_version >= metadata.first_version);
    }

    #[test]
    fn test_concurrent_run_tracking() {
        use std::sync::Arc;
        use std::thread;

        let temp_dir = TempDir::new().unwrap();
        let db = Arc::new(Database::open(temp_dir.path()).unwrap());

        let mut handles = vec![];

        // Spawn threads that create and end runs
        for _ in 0..10 {
            let db = Arc::clone(&db);
            let handle = thread::spawn(move || {
                let run_id = RunId::new();
                db.begin_run(run_id, vec![]).unwrap();
                assert!(db.is_run_active(run_id));

                // Do some work
                std::thread::sleep(std::time::Duration::from_millis(1));

                db.end_run(run_id).unwrap();
                assert!(!db.is_run_active(run_id));
            });
            handles.push(handle);
        }

        for handle in handles {
            handle.join().unwrap();
        }

        // All runs should be ended
        assert_eq!(db.active_run_count(), 0);
    }
}
