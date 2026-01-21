//! Test utilities for M7 comprehensive tests
//!
//! Provides common helpers for:
//! - Creating test databases with durability
//! - State capture and comparison
//! - WAL manipulation for crash simulation
//! - Snapshot creation and verification

use strata_core::types::RunId;
use strata_core::value::Value;
use strata_engine::Database;
use strata_primitives::KVStore;
use std::collections::HashMap;
use std::fs::{self, OpenOptions};
use std::hash::{Hash, Hasher};
use std::io::{Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tempfile::TempDir;

// Counter for generating unique keys
static HEALTH_CHECK_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Test database wrapper with durability support
pub struct TestDb {
    pub db: Arc<Database>,
    pub dir: TempDir,
    pub run_id: RunId,
}

impl TestDb {
    /// Create a new test database with file-backed WAL
    pub fn new() -> Self {
        let dir = tempfile::tempdir().expect("Failed to create temp dir");
        let db = Arc::new(
            Database::builder()
                .path(dir.path())
                .buffered()
                .open()
                .expect("Failed to create test database"),
        );
        let run_id = RunId::new();
        TestDb { db, dir, run_id }
    }

    /// Create a test database with strict durability (fsync on each write)
    pub fn new_strict() -> Self {
        let dir = tempfile::tempdir().expect("Failed to create temp dir");
        let db = Arc::new(
            Database::builder()
                .path(dir.path())
                .strict()
                .open()
                .expect("Failed to create test database"),
        );
        let run_id = RunId::new();
        TestDb { db, dir, run_id }
    }

    /// Create an in-memory test database (no durability)
    pub fn new_in_memory() -> Self {
        let dir = tempfile::tempdir().expect("Failed to create temp dir");
        let db = Arc::new(
            Database::builder()
                .in_memory()
                .open_temp()
                .expect("Failed to create test database"),
        );
        let run_id = RunId::new();
        TestDb { db, dir, run_id }
    }

    /// Get the WAL file path
    pub fn wal_path(&self) -> PathBuf {
        self.dir.path().join("wal.bin")
    }

    /// Get the snapshots directory path
    pub fn snapshot_dir(&self) -> PathBuf {
        self.dir.path().join("snapshots")
    }

    /// Get the database directory path
    pub fn db_path(&self) -> &Path {
        self.dir.path()
    }

    /// Reopen the database (simulates restart)
    pub fn reopen(&mut self) {
        // Create new database (old Arc will be dropped when reassigned)
        let new_db = Arc::new(
            Database::builder()
                .path(self.dir.path())
                .buffered()
                .open()
                .expect("Failed to reopen database"),
        );
        self.db = new_db;
    }

    /// Get the KV store
    pub fn kv(&self) -> KVStore {
        KVStore::new(self.db.clone())
    }
}

impl Default for TestDb {
    fn default() -> Self {
        Self::new()
    }
}

/// Captured state of a database for comparison
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapturedState {
    pub kv_entries: HashMap<String, String>,
    pub hash: u64,
}

impl CapturedState {
    /// Capture the current state of the database
    pub fn capture(db: &Arc<Database>, run_id: &RunId) -> Self {
        let kv = KVStore::new(db.clone());
        let mut kv_entries = HashMap::new();

        // Capture all KV entries for this run
        if let Ok(keys) = kv.list(run_id, None) {
            for key in keys {
                if let Ok(Some(versioned)) = kv.get(run_id, &key) {
                    // Extract just the value, ignoring version/timestamp metadata
                    kv_entries.insert(key.to_string(), format!("{:?}", versioned.value));
                }
            }
        }

        let hash = Self::compute_hash(&kv_entries);
        CapturedState { kv_entries, hash }
    }

    fn compute_hash(entries: &HashMap<String, String>) -> u64 {
        use std::collections::hash_map::DefaultHasher;
        let mut hasher = DefaultHasher::new();

        // Sort keys for deterministic hashing
        let mut sorted_entries: Vec<_> = entries.iter().collect();
        sorted_entries.sort_by_key(|(k, _)| *k);

        for (k, v) in sorted_entries {
            k.hash(&mut hasher);
            v.hash(&mut hasher);
        }

        hasher.finish()
    }
}

/// Hash captured state
pub fn hash_captured_state(state: &CapturedState) -> u64 {
    state.hash
}

/// Corrupt a file at a specific offset
pub fn corrupt_file_at_offset(path: &Path, offset: u64, bytes: &[u8]) {
    let mut file = OpenOptions::new()
        .read(true)
        .write(true)
        .open(path)
        .expect("Failed to open file for corruption");

    file.seek(SeekFrom::Start(offset))
        .expect("Failed to seek in file");
    file.write_all(bytes)
        .expect("Failed to write corruption bytes");
}

/// Corrupt a file by flipping bits at a random location
pub fn corrupt_file_random(path: &Path) {
    let metadata = fs::metadata(path).expect("Failed to get file metadata");
    let size = metadata.len();

    if size > 100 {
        // Corrupt somewhere in the middle
        let offset = size / 2;
        corrupt_file_at_offset(path, offset, &[0xFF, 0xFF, 0xFF, 0xFF]);
    }
}

/// Truncate a file to a specific size
pub fn truncate_file(path: &Path, new_size: u64) {
    let file = OpenOptions::new()
        .write(true)
        .open(path)
        .expect("Failed to open file for truncation");
    file.set_len(new_size).expect("Failed to truncate file");
}

/// Get file size
pub fn file_size(path: &Path) -> u64 {
    fs::metadata(path).map(|m| m.len()).unwrap_or(0)
}

/// Create a partial WAL entry (simulating crash during write)
pub fn create_partial_wal_entry(path: &Path, entry_bytes: &[u8], fraction: f64) {
    let partial_len = ((entry_bytes.len() as f64) * fraction) as usize;
    let partial = &entry_bytes[..partial_len];

    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .expect("Failed to open WAL for partial write");

    file.write_all(partial)
        .expect("Failed to write partial entry");
}

/// Verify that two states are identical
pub fn assert_states_equal(state1: &CapturedState, state2: &CapturedState, msg: &str) {
    assert_eq!(state1.hash, state2.hash, "{}: State hashes differ", msg);
    assert_eq!(
        state1.kv_entries, state2.kv_entries,
        "{}: KV entries differ",
        msg
    );
}

/// Count the number of snapshot files in a directory
pub fn count_snapshots(dir: &Path) -> usize {
    if !dir.exists() {
        return 0;
    }

    fs::read_dir(dir)
        .map(|entries| {
            entries
                .filter_map(Result::ok)
                .filter(|e| {
                    e.path()
                        .extension()
                        .map(|ext| ext == "snap")
                        .unwrap_or(false)
                })
                .count()
        })
        .unwrap_or(0)
}

/// List all snapshot files in a directory
pub fn list_snapshots(dir: &Path) -> Vec<PathBuf> {
    if !dir.exists() {
        return Vec::new();
    }

    fs::read_dir(dir)
        .map(|entries| {
            entries
                .filter_map(Result::ok)
                .filter(|e| {
                    e.path()
                        .extension()
                        .map(|ext| ext == "snap")
                        .unwrap_or(false)
                })
                .map(|e| e.path())
                .collect()
        })
        .unwrap_or_default()
}

/// Test that a database is healthy and can perform basic operations
pub fn assert_db_healthy(db: &Arc<Database>, run_id: &RunId) {
    let kv = KVStore::new(db.clone());

    // Should be able to write
    let counter = HEALTH_CHECK_COUNTER.fetch_add(1, Ordering::SeqCst);
    let test_key = format!("health_check_{}", counter);
    kv.put(run_id, &test_key, Value::String("test".into()))
        .expect("Database should be able to write");

    // Should be able to read back
    let value = kv
        .get(run_id, &test_key)
        .expect("Database should be able to read");
    assert!(value.is_some(), "Database should return written value");
}

/// Create a run and register it with the database
pub fn create_test_run(db: &Arc<Database>) -> RunId {
    use strata_primitives::RunIndex as PrimitiveRunIndex;

    let run_id = RunId::new();
    let run_index = PrimitiveRunIndex::new(db.clone());
    run_index
        .create_run(&run_id.to_string())
        .expect("Failed to create test run");
    run_id
}

/// Write test data to multiple primitives
pub fn populate_test_data(db: &Arc<Database>, run_id: &RunId, count: usize) {
    let kv = KVStore::new(db.clone());

    for i in 0..count {
        kv.put(
            run_id,
            &format!("key_{}", i),
            Value::String(format!("value_{}", i)),
        )
        .expect("Failed to write test data");
    }
}

/// Verify that specific keys exist in the database
pub fn verify_keys_exist(db: &Arc<Database>, run_id: &RunId, keys: &[&str]) {
    let kv = KVStore::new(db.clone());

    for key in keys {
        let value = kv.get(run_id, key).expect("Failed to read key");
        assert!(value.is_some(), "Key {} should exist", key);
    }
}

/// Verify that specific keys do NOT exist in the database
pub fn verify_keys_absent(db: &Arc<Database>, run_id: &RunId, keys: &[&str]) {
    let kv = KVStore::new(db.clone());

    for key in keys {
        let value = kv.get(run_id, key).expect("Failed to read key");
        assert!(value.is_none(), "Key {} should NOT exist", key);
    }
}
