//! Test utilities for M8 Vector comprehensive tests
//!
//! Provides common helpers for:
//! - Creating test databases with durability
//! - Vector collection setup and manipulation
//! - State capture and comparison
//! - WAL manipulation for crash simulation
//! - Snapshot creation and verification

use in_mem_core::types::RunId;
use in_mem_core::value::Value;
use in_mem_durability::WalEntryType;
use in_mem_engine::Database;
use in_mem_primitives::register_vector_recovery;
use in_mem_primitives::vector::{
    DistanceMetric, StorageDtype, VectorConfig, VectorError, VectorId, VectorMatch, VectorStore,
};
use in_mem_primitives::KVStore;
use serde_json::json;
use std::collections::{BTreeMap, HashMap};
use std::fs::{self, OpenOptions};
use std::hash::{Hash, Hasher};
use std::io::{Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Once};
use tempfile::TempDir;

// Ensure vector recovery is registered exactly once
static INIT_RECOVERY: Once = Once::new();

fn ensure_recovery_registered() {
    INIT_RECOVERY.call_once(|| {
        register_vector_recovery();
    });
}

// Counter for generating unique keys
static HEALTH_CHECK_COUNTER: AtomicU64 = AtomicU64::new(0);

// ============================================================================
// Test Database Wrapper
// ============================================================================

/// Test database wrapper with durability support
pub struct TestDb {
    pub db: Arc<Database>,
    pub dir: TempDir,
    pub run_id: RunId,
}

impl TestDb {
    /// Create a new test database with file-backed WAL
    pub fn new() -> Self {
        ensure_recovery_registered();
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
        ensure_recovery_registered();
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
        ensure_recovery_registered();
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
        self.dir.path().join("wal").join("current.wal")
    }

    /// Get the WAL directory path
    pub fn wal_dir(&self) -> PathBuf {
        self.dir.path().join("wal")
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
        // Flush database before closing
        self.db.flush().expect("Failed to flush database");

        // Create new database with same settings (old one drops when reassigned)
        // KV recovery happens automatically in Database::open_with_mode()
        // Vector recovery happens automatically in VectorStore::new()
        self.db = Arc::new(
            Database::builder()
                .path(self.dir.path())
                .strict()
                .open()
                .expect("Failed to reopen database"),
        );
    }

    /// Get the KV store
    pub fn kv(&self) -> KVStore {
        KVStore::new(self.db.clone())
    }

    /// Get the Vector store
    pub fn vector(&self) -> VectorStore {
        VectorStore::new(self.db.clone())
    }

    /// Create a new run ID for this test
    pub fn new_run(&mut self) -> RunId {
        self.run_id = RunId::new();
        self.run_id
    }
}

impl Default for TestDb {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Vector Configuration Helpers
// ============================================================================

/// Create a VectorConfig for MiniLM (384 dimensions, Cosine)
pub fn config_minilm() -> VectorConfig {
    VectorConfig {
        dimension: 384,
        metric: DistanceMetric::Cosine,
        storage_dtype: StorageDtype::F32,
    }
}

/// Create a VectorConfig for OpenAI Ada (1536 dimensions, Cosine)
pub fn config_openai_ada() -> VectorConfig {
    VectorConfig {
        dimension: 1536,
        metric: DistanceMetric::Cosine,
        storage_dtype: StorageDtype::F32,
    }
}

/// Create a VectorConfig with custom settings
pub fn config_custom(dimension: usize, metric: DistanceMetric) -> VectorConfig {
    VectorConfig {
        dimension,
        metric,
        storage_dtype: StorageDtype::F32,
    }
}

/// Create a small dimension config for testing (3 dimensions)
pub fn config_small() -> VectorConfig {
    VectorConfig {
        dimension: 3,
        metric: DistanceMetric::Cosine,
        storage_dtype: StorageDtype::F32,
    }
}

/// Create a config with Euclidean distance metric (384 dimensions)
pub fn config_euclidean() -> VectorConfig {
    VectorConfig {
        dimension: 384,
        metric: DistanceMetric::Euclidean,
        storage_dtype: StorageDtype::F32,
    }
}

/// Create a config with DotProduct distance metric (384 dimensions)
pub fn config_dotproduct() -> VectorConfig {
    VectorConfig {
        dimension: 384,
        metric: DistanceMetric::DotProduct,
        storage_dtype: StorageDtype::F32,
    }
}

// ============================================================================
// Random Vector Generation
// ============================================================================

/// Generate a random vector of the given dimension
pub fn random_vector(dimension: usize) -> Vec<f32> {
    use std::collections::hash_map::DefaultHasher;
    use std::time::{SystemTime, UNIX_EPOCH};

    // Simple deterministic-ish random based on time and counter
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let seed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos() as u64
        ^ COUNTER.fetch_add(1, Ordering::SeqCst);

    let mut hasher = DefaultHasher::new();
    (0..dimension)
        .map(|i| {
            (i as u64 ^ seed).hash(&mut hasher);
            let h = hasher.finish();
            // Normalize to [-1, 1] range
            ((h as f32 / u64::MAX as f32) * 2.0 - 1.0)
        })
        .collect()
}

/// Generate a seeded random vector (for reproducibility)
pub fn seeded_random_vector(dimension: usize, seed: u64) -> Vec<f32> {
    use std::collections::hash_map::DefaultHasher;

    let mut hasher = DefaultHasher::new();
    (0..dimension)
        .map(|i| {
            (i as u64 ^ seed).hash(&mut hasher);
            let h = hasher.finish();
            ((h as f32 / u64::MAX as f32) * 2.0 - 1.0)
        })
        .collect()
}

/// Generate a zero vector
pub fn zero_vector(dimension: usize) -> Vec<f32> {
    vec![0.0; dimension]
}

/// Generate a unit vector (only first component is 1.0)
pub fn unit_vector(dimension: usize) -> Vec<f32> {
    let mut v = vec![0.0; dimension];
    if dimension > 0 {
        v[0] = 1.0;
    }
    v
}

// ============================================================================
// State Capture for Comparison
// ============================================================================

/// Captured state of a vector collection for comparison
#[derive(Debug, Clone, PartialEq)]
pub struct CapturedVectorState {
    pub vectors: BTreeMap<String, (VectorId, Vec<f32>, Option<serde_json::Value>)>,
    pub count: usize,
    pub max_id: u64,
}

impl CapturedVectorState {
    /// Capture the current state of a vector collection using known keys
    ///
    /// Since there's no list_keys API, callers must provide the keys to capture.
    pub fn capture_keys(vector_store: &VectorStore, run_id: RunId, collection: &str, keys: &[String]) -> Self {
        let mut vectors = BTreeMap::new();
        let mut max_id = 0u64;

        for key in keys {
            if let Ok(Some(entry)) = vector_store.get(run_id, collection, key) {
                let vid = entry.vector_id();
                if vid.as_u64() > max_id {
                    max_id = vid.as_u64();
                }
                vectors.insert(
                    key.to_string(),
                    (vid, entry.embedding.clone(), entry.metadata.clone()),
                );
            }
        }

        let count = vectors.len();

        CapturedVectorState {
            vectors,
            count,
            max_id,
        }
    }

    /// Capture state by probing a range of expected keys (key_0, key_1, ...)
    pub fn capture(vector_store: &VectorStore, run_id: RunId, collection: &str) -> Self {
        // Try to get count first
        let count = vector_store.count(run_id, collection).unwrap_or(0);

        let mut vectors = BTreeMap::new();
        let mut max_id = 0u64;

        // Probe keys in common patterns - this is a heuristic for tests
        // that use predictable key naming
        for i in 0..1000 {
            let key = format!("key_{}", i);
            if let Ok(Some(entry)) = vector_store.get(run_id, collection, &key) {
                let vid = entry.vector_id();
                if vid.as_u64() > max_id {
                    max_id = vid.as_u64();
                }
                vectors.insert(
                    key,
                    (vid, entry.embedding.clone(), entry.metadata.clone()),
                );
            }
            // Also try key_{:02} format
            let key2 = format!("key_{:02}", i);
            if let Ok(Some(entry)) = vector_store.get(run_id, collection, &key2) {
                let vid = entry.vector_id();
                if vid.as_u64() > max_id {
                    max_id = vid.as_u64();
                }
                vectors.insert(
                    key2,
                    (vid, entry.embedding.clone(), entry.metadata.clone()),
                );
            }
        }

        CapturedVectorState {
            vectors,
            count,
            max_id,
        }
    }
}

/// Captured state of the entire database (KV + Vector)
#[derive(Debug, Clone, PartialEq)]
pub struct CapturedDbState {
    pub kv_entries: HashMap<String, String>,
    pub vector_collections: HashMap<String, CapturedVectorState>,
}

impl CapturedDbState {
    /// Capture the current state of the database
    pub fn capture(db: &Arc<Database>, run_id: RunId, collections: &[&str]) -> Self {
        let kv = KVStore::new(db.clone());
        let vector = VectorStore::new(db.clone());

        let mut kv_entries = HashMap::new();
        if let Ok(keys) = kv.list(&run_id, None) {
            for key in keys {
                if let Ok(Some(value)) = kv.get(&run_id, &key) {
                    kv_entries.insert(key.to_string(), format!("{:?}", value));
                }
            }
        }

        let mut vector_collections = HashMap::new();
        for collection in collections {
            let state = CapturedVectorState::capture(&vector, run_id, collection);
            vector_collections.insert(collection.to_string(), state);
        }

        CapturedDbState {
            kv_entries,
            vector_collections,
        }
    }
}

// ============================================================================
// Distance Calculation Helpers
// ============================================================================

/// Calculate cosine similarity between two vectors
pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    assert_eq!(a.len(), b.len(), "Vectors must have same dimension");

    let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();

    if norm_a == 0.0 || norm_b == 0.0 {
        0.0
    } else {
        dot / (norm_a * norm_b)
    }
}

/// Calculate Euclidean distance between two vectors
pub fn euclidean_distance(a: &[f32], b: &[f32]) -> f32 {
    assert_eq!(a.len(), b.len(), "Vectors must have same dimension");

    a.iter()
        .zip(b.iter())
        .map(|(x, y)| (x - y).powi(2))
        .sum::<f32>()
        .sqrt()
}

/// Convert Euclidean distance to similarity score (higher is better)
pub fn euclidean_similarity(a: &[f32], b: &[f32]) -> f32 {
    let dist = euclidean_distance(a, b);
    1.0 / (1.0 + dist)
}

/// Calculate dot product between two vectors
pub fn dot_product(a: &[f32], b: &[f32]) -> f32 {
    assert_eq!(a.len(), b.len(), "Vectors must have same dimension");
    a.iter().zip(b.iter()).map(|(x, y)| x * y).sum()
}

// ============================================================================
// WAL Manipulation Helpers
// ============================================================================

/// Get WAL file size
pub fn wal_size(path: &Path) -> u64 {
    fs::metadata(path).map(|m| m.len()).unwrap_or(0)
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

/// Truncate a file to a specific size
pub fn truncate_file(path: &Path, new_size: u64) {
    let file = OpenOptions::new()
        .write(true)
        .open(path)
        .expect("Failed to open file for truncation");
    file.set_len(new_size).expect("Failed to truncate file");
}

/// Delete all snapshot files in a directory
pub fn delete_snapshots(dir: &Path) {
    if !dir.exists() {
        return;
    }

    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.filter_map(Result::ok) {
            let path = entry.path();
            if path.extension().map(|ext| ext == "snap").unwrap_or(false) {
                let _ = fs::remove_file(path);
            }
        }
    }
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

// ============================================================================
// Assertion Helpers
// ============================================================================

/// Assert that two vector states are identical
pub fn assert_vector_states_equal(state1: &CapturedVectorState, state2: &CapturedVectorState, msg: &str) {
    assert_eq!(state1.count, state2.count, "{}: Vector counts differ", msg);
    assert_eq!(
        state1.vectors.len(),
        state2.vectors.len(),
        "{}: Vector map sizes differ",
        msg
    );

    for (key, (id1, emb1, meta1)) in &state1.vectors {
        let (id2, emb2, meta2) = state2
            .vectors
            .get(key)
            .expect(&format!("{}: Missing key {} in second state", msg, key));

        assert_eq!(id1, id2, "{}: VectorId differs for key {}", msg, key);
        assert_eq!(emb1.len(), emb2.len(), "{}: Embedding length differs for key {}", msg, key);

        for (i, (v1, v2)) in emb1.iter().zip(emb2.iter()).enumerate() {
            assert!(
                (v1 - v2).abs() < 1e-6,
                "{}: Embedding value differs at index {} for key {}",
                msg,
                i,
                key
            );
        }

        assert_eq!(meta1, meta2, "{}: Metadata differs for key {}", msg, key);
    }
}

/// Assert that a database is healthy and can perform basic operations
pub fn assert_db_healthy(db: &Arc<Database>, run_id: &RunId) {
    let kv = KVStore::new(db.clone());

    let counter = HEALTH_CHECK_COUNTER.fetch_add(1, Ordering::SeqCst);
    let test_key = format!("health_check_{}", counter);
    kv.put(run_id, &test_key, Value::String("test".into()))
        .expect("Database should be able to write");

    let value = kv
        .get(run_id, &test_key)
        .expect("Database should be able to read");
    assert!(value.is_some(), "Database should return written value");
}

/// Assert that a vector collection is healthy
pub fn assert_vector_collection_healthy(
    vector_store: &VectorStore,
    run_id: RunId,
    collection: &str,
    dimension: usize,
) {
    let counter = HEALTH_CHECK_COUNTER.fetch_add(1, Ordering::SeqCst);
    let test_key = format!("health_check_{}", counter);
    let test_embedding = random_vector(dimension);

    // Should be able to insert
    vector_store
        .insert(run_id, collection, &test_key, &test_embedding, None)
        .expect("Vector store should be able to insert");

    // Should be able to get
    let entry = vector_store
        .get(run_id, collection, &test_key)
        .expect("Vector store should be able to get");
    assert!(entry.is_some(), "Vector store should return inserted entry");

    // Should be able to search
    let results = vector_store
        .search(run_id, collection, &test_embedding, 1, None)
        .expect("Vector store should be able to search");
    assert!(!results.is_empty(), "Search should return results");
}

// ============================================================================
// Test Data Generation
// ============================================================================

/// Populate a vector collection with test data
pub fn populate_vector_collection(
    vector_store: &VectorStore,
    run_id: RunId,
    collection: &str,
    count: usize,
    dimension: usize,
) -> Vec<(String, Vec<f32>)> {
    let mut entries = Vec::new();

    for i in 0..count {
        let key = format!("key_{}", i);
        let embedding = seeded_random_vector(dimension, i as u64);
        vector_store
            .insert(run_id, collection, &key, &embedding, None)
            .expect("Failed to insert vector");
        entries.push((key, embedding));
    }

    entries
}

/// Populate a vector collection with metadata
pub fn populate_vector_collection_with_metadata(
    vector_store: &VectorStore,
    run_id: RunId,
    collection: &str,
    count: usize,
    dimension: usize,
) -> Vec<(String, Vec<f32>, serde_json::Value)> {
    let mut entries = Vec::new();

    for i in 0..count {
        let key = format!("key_{}", i);
        let embedding = seeded_random_vector(dimension, i as u64);
        let metadata = json!({
            "index": i,
            "category": format!("cat_{}", i % 5),
            "value": i as f64 * 0.1
        });
        vector_store
            .insert(run_id, collection, &key, &embedding, Some(metadata.clone()))
            .expect("Failed to insert vector");
        entries.push((key, embedding, metadata));
    }

    entries
}

// ============================================================================
// WAL Entry Type Verification
// ============================================================================

/// Verify WAL entry type codes
pub fn verify_wal_entry_types() {
    assert_eq!(WalEntryType::VectorCollectionCreate as u8, 0x70);
    assert_eq!(WalEntryType::VectorCollectionDelete as u8, 0x71);
    assert_eq!(WalEntryType::VectorUpsert as u8, 0x72);
    assert_eq!(WalEntryType::VectorDelete as u8, 0x73);
}
