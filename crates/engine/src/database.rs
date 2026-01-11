//! Database struct and open/close logic
//!
//! This module provides the main Database struct that orchestrates:
//! - Storage initialization
//! - WAL opening
//! - Automatic recovery on startup

use in_mem_core::error::{Error, Result};
use in_mem_durability::replay_wal;
use in_mem_durability::wal::{DurabilityMode, WAL};
use in_mem_storage::UnifiedStore;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use tracing::info;

/// Main database struct
///
/// Orchestrates storage, WAL, and recovery.
/// Create a database by calling `Database::open()`.
pub struct Database {
    /// Data directory path
    data_dir: PathBuf,

    /// Unified storage (thread-safe)
    storage: Arc<UnifiedStore>,

    /// Write-ahead log (protected by mutex for exclusive access)
    wal: Arc<Mutex<WAL>>,
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

        Ok(Self {
            data_dir,
            storage,
            wal: Arc::new(Mutex::new(wal)),
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
}
