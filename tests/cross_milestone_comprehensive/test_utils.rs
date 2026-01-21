//! Test utilities for cross-milestone comprehensive tests
//!
//! Provides common helpers for testing all 7 primitives together.

use strata_core::json::{JsonPath, JsonValue};
use strata_core::types::{JsonDocId, RunId};
use strata_core::value::Value;
use strata_engine::Database;
use strata_primitives::{
    register_vector_recovery, DistanceMetric, EventLog, JsonStore, KVStore, RunIndex, StateCell,
    StorageDtype, TraceStore, VectorConfig, VectorStore,
};
use std::path::Path;
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

// Re-export types for tests
pub use strata_primitives::TraceType;

// Counter for generating unique keys
static COUNTER: AtomicU64 = AtomicU64::new(0);

// ============================================================================
// Test Database Wrapper with All Primitives
// ============================================================================

/// Test database wrapper with access to all 7 primitives
pub struct TestDb {
    pub db: Arc<Database>,
    pub dir: TempDir,
    pub run_id: RunId,
}

impl TestDb {
    /// Create a new test database with buffered durability (default for tests)
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

    /// Create a test database with strict durability
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

    /// Create an in-memory test database
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

    /// Get all 7 primitives
    pub fn all_primitives(&self) -> AllPrimitives {
        AllPrimitives {
            kv: KVStore::new(self.db.clone()),
            json: JsonStore::new(self.db.clone()),
            event: EventLog::new(self.db.clone()),
            state: StateCell::new(self.db.clone()),
            trace: TraceStore::new(self.db.clone()),
            run: RunIndex::new(self.db.clone()),
            vector: VectorStore::new(self.db.clone()),
        }
    }

    /// Get the KV store
    pub fn kv(&self) -> KVStore {
        KVStore::new(self.db.clone())
    }

    /// Get the JSON store
    pub fn json(&self) -> JsonStore {
        JsonStore::new(self.db.clone())
    }

    /// Get the Event log
    pub fn event(&self) -> EventLog {
        EventLog::new(self.db.clone())
    }

    /// Get the State cell
    pub fn state(&self) -> StateCell {
        StateCell::new(self.db.clone())
    }

    /// Get the Trace store
    pub fn trace(&self) -> TraceStore {
        TraceStore::new(self.db.clone())
    }

    /// Get the Run index
    pub fn run_index(&self) -> RunIndex {
        RunIndex::new(self.db.clone())
    }

    /// Get the Vector store
    pub fn vector(&self) -> VectorStore {
        VectorStore::new(self.db.clone())
    }

    /// Get the database path
    pub fn db_path(&self) -> &Path {
        self.dir.path()
    }

    /// Reopen the database (simulates restart)
    pub fn reopen(&mut self) {
        self.db.flush().expect("Failed to flush database");
        self.db = Arc::new(
            Database::builder()
                .path(self.dir.path())
                .strict()
                .open()
                .expect("Failed to reopen database"),
        );
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

/// Container for all 7 primitives
pub struct AllPrimitives {
    pub kv: KVStore,
    pub json: JsonStore,
    pub event: EventLog,
    pub state: StateCell,
    pub trace: TraceStore,
    pub run: RunIndex,
    pub vector: VectorStore,
}

// ============================================================================
// Vector Configuration Helpers
// ============================================================================

/// Create a small dimension config for testing (3 dimensions)
pub fn config_small() -> VectorConfig {
    VectorConfig {
        dimension: 3,
        metric: DistanceMetric::Cosine,
        storage_dtype: StorageDtype::F32,
    }
}

/// Create a standard config for testing (384 dimensions)
pub fn config_standard() -> VectorConfig {
    VectorConfig {
        dimension: 384,
        metric: DistanceMetric::Cosine,
        storage_dtype: StorageDtype::F32,
    }
}

// ============================================================================
// Random Data Generation
// ============================================================================

/// Generate a unique key
pub fn unique_key() -> String {
    let count = COUNTER.fetch_add(1, Ordering::SeqCst);
    format!("key_{}", count)
}

/// Generate a seeded random vector
pub fn seeded_vector(dimension: usize, seed: u64) -> Vec<f32> {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let mut hasher = DefaultHasher::new();
    (0..dimension)
        .map(|i| {
            (i as u64 ^ seed).hash(&mut hasher);
            let h = hasher.finish();
            (h as f32 / u64::MAX as f32) * 2.0 - 1.0
        })
        .collect()
}

/// Generate random vector
pub fn random_vector(dimension: usize) -> Vec<f32> {
    let seed = COUNTER.fetch_add(1, Ordering::SeqCst);
    seeded_vector(dimension, seed)
}

// ============================================================================
// JSON Generation
// ============================================================================

/// Generate a test JSON document (as serde_json::Value for compatibility)
pub fn test_json_doc(index: usize) -> serde_json::Value {
    serde_json::json!({
        "id": index,
        "name": format!("document_{}", index),
        "tags": ["test", "generated"],
        "nested": {
            "value": index * 10,
            "active": true
        }
    })
}

/// Generate a test JSON value (as JsonValue for JsonStore)
pub fn test_json_value(index: usize) -> JsonValue {
    JsonValue::from(serde_json::json!({
        "id": index,
        "name": format!("document_{}", index),
        "tags": ["test", "generated"],
        "nested": {
            "value": index * 10,
            "active": true
        }
    }))
}

/// Generate a deeply nested JSON document
pub fn deep_json_doc(depth: usize) -> serde_json::Value {
    let mut doc = serde_json::json!({ "value": depth });
    for i in (0..depth).rev() {
        doc = serde_json::json!({
            "level": i,
            "child": doc
        });
    }
    doc
}

/// Generate a deeply nested JSON value (as JsonValue)
pub fn deep_json_value(depth: usize) -> JsonValue {
    JsonValue::from(deep_json_doc(depth))
}

// ============================================================================
// Assertion Helpers
// ============================================================================

/// Assert that a database is healthy and can perform basic operations
pub fn assert_db_healthy(db: &Arc<Database>, run_id: &RunId) {
    let kv = KVStore::new(db.clone());
    let key = unique_key();
    kv.put(run_id, &key, Value::String("test".into()))
        .expect("Database should be able to write");
    let value = kv.get(run_id, &key).expect("Database should be able to read").map(|v| v.value);
    assert!(value.is_some(), "Database should return written value");
}

/// Assert that all 7 primitives can perform basic operations
pub fn assert_all_primitives_healthy(test_db: &TestDb) {
    let p = test_db.all_primitives();
    let run_id = test_db.run_id;

    // KV
    let key = unique_key();
    p.kv.put(&run_id, &key, Value::String("kv_test".into()))
        .expect("KV should write");
    assert!(p.kv.get(&run_id, &key).expect("KV read").map(|v| v.value).is_some());

    // JSON
    let doc_id = JsonDocId::new();
    p.json
        .create(&run_id, &doc_id, test_json_value(0))
        .expect("JSON should create");
    assert!(p
        .json
        .get(&run_id, &doc_id, &JsonPath::root())
        .expect("JSON read")
        .is_some());

    // Event
    p.event
        .append(&run_id, "test_event", Value::Null)
        .expect("Event should append");

    // State
    let state_key = unique_key();
    p.state
        .init(&run_id, &state_key, Value::String("initial".into()))
        .expect("State should init");

    // Trace
    p.trace
        .record(
            &run_id,
            strata_primitives::TraceType::Thought {
                content: "test content".into(),
                confidence: None,
            },
            vec![],
            Value::Null,
        )
        .expect("Trace should record");

    // Run
    // Run index operations are typically done through transaction extensions

    // Vector
    let collection = unique_key();
    p.vector
        .create_collection(run_id, &collection, config_small())
        .expect("Vector should create collection");
    let vec_key = unique_key();
    p.vector
        .insert(run_id, &collection, &vec_key, &[1.0, 0.0, 0.0], None)
        .expect("Vector should insert");
}

// ============================================================================
// Issue-Specific Test Helpers
// ============================================================================

/// Helper to check if Searchable trait is implemented
/// Returns true if the search method can be called polymorphically
pub fn can_search_as_searchable<T: strata_primitives::Searchable>(_: &T) -> bool {
    true
}

/// Generate a large JSON document of specified size
pub fn large_json_doc(size_bytes: usize) -> JsonValue {
    // Generate a string of approximately the specified size
    let padding = "x".repeat(size_bytes);
    JsonValue::from(serde_json::json!({
        "data": padding
    }))
}

/// Generate a deeply nested JSON document with specified nesting depth
pub fn nested_json_doc(nesting_depth: usize) -> JsonValue {
    let mut doc = serde_json::json!({"value": "leaf"});
    for i in (0..nesting_depth).rev() {
        doc = serde_json::json!({
            "level": i,
            "child": doc
        });
    }
    JsonValue::from(doc)
}

/// Generate a JSON array with specified number of elements
pub fn large_array_json(element_count: usize) -> JsonValue {
    let arr: Vec<serde_json::Value> = (0..element_count)
        .map(|i| serde_json::json!({"index": i}))
        .collect();
    JsonValue::from(serde_json::json!({"array": arr}))
}

/// Generate a JSON path with specified number of segments
pub fn long_json_path(segment_count: usize) -> String {
    (0..segment_count)
        .map(|i| format!("level{}", i))
        .collect::<Vec<_>>()
        .join(".")
}

/// Helper to create a JsonValue from a serde_json::Value
pub fn json_value(v: serde_json::Value) -> JsonValue {
    JsonValue::from(v)
}

/// Helper to create a new unique JsonDocId
pub fn new_doc_id() -> JsonDocId {
    JsonDocId::new()
}
