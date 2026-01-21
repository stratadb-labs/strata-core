//! Test utilities for M5 comprehensive tests
//!
//! Provides common helpers for creating databases, documents, and assertions.

pub use strata_core::json::{JsonPath, JsonValue};
pub use strata_core::types::{JsonDocId, RunId};
pub use strata_durability::wal::DurabilityMode;
pub use strata_engine::Database;
pub use strata_primitives::JsonStore;
use std::sync::Arc;

// =============================================================================
// Database Creation
// =============================================================================

/// Create a test database with InMemory durability (fastest)
pub fn create_test_db() -> Arc<Database> {
    Arc::new(
        Database::builder()
            .durability(DurabilityMode::InMemory)
            .open_temp()
            .expect("Failed to create test database"),
    )
}

/// Create a test database with specified durability mode
pub fn create_test_db_with_mode(mode: DurabilityMode) -> Arc<Database> {
    Arc::new(
        Database::builder()
            .durability(mode)
            .open_temp()
            .expect("Failed to create test database"),
    )
}

/// Create a test database with persistent path for recovery tests
pub fn create_persistent_db(path: &std::path::Path) -> Arc<Database> {
    Arc::new(
        Database::open_with_mode(path, DurabilityMode::default())
            .expect("Failed to create persistent database"),
    )
}

/// All durability modes for cross-mode testing
pub fn all_durability_modes() -> Vec<DurabilityMode> {
    vec![
        DurabilityMode::InMemory,
        DurabilityMode::default(), // Batched
        DurabilityMode::Strict,
    ]
}

/// Run a test workload across all durability modes
pub fn test_across_modes<F, T>(test_name: &str, workload: F)
where
    F: Fn(Arc<Database>) -> T,
    T: PartialEq + std::fmt::Debug,
{
    let modes = all_durability_modes();
    let mut results: Vec<(DurabilityMode, T)> = Vec::new();

    for mode in modes {
        let db = create_test_db_with_mode(mode.clone());
        let result = workload(db);
        results.push((mode, result));
    }

    // Assert all results identical to first (InMemory)
    let (first_mode, first_result) = &results[0];
    for (mode, result) in &results[1..] {
        assert_eq!(
            first_result, result,
            "SEMANTIC DRIFT in '{}': {:?} produced {:?}, but {:?} produced {:?}",
            test_name, first_mode, first_result, mode, result
        );
    }
}

// =============================================================================
// Path Helpers
// =============================================================================

/// Parse a path from string (convenience wrapper)
pub fn path(s: &str) -> JsonPath {
    s.parse().expect("Invalid path")
}

/// Create root path
pub fn root() -> JsonPath {
    JsonPath::root()
}

// =============================================================================
// JSON Value Helpers
// =============================================================================

/// Create JSON value from serde_json macro
#[macro_export]
macro_rules! json {
    ($($tt:tt)*) => {
        JsonValue::from(serde_json::json!($($tt)*))
    };
}

/// Create an empty object
pub fn empty_object() -> JsonValue {
    JsonValue::object()
}

/// Create an empty array
pub fn empty_array() -> JsonValue {
    JsonValue::array()
}

// =============================================================================
// Document Helpers
// =============================================================================

/// Create a test document with given value
pub fn create_doc(store: &JsonStore, run_id: &RunId, value: JsonValue) -> JsonDocId {
    let doc_id = JsonDocId::new();
    store
        .create(run_id, &doc_id, value)
        .expect("Failed to create document");
    doc_id
}

/// Create a test document and return both store and doc_id
pub fn setup_doc(value: JsonValue) -> (Arc<Database>, JsonStore, RunId, JsonDocId) {
    let db = create_test_db();
    let store = JsonStore::new(db.clone());
    let run_id = RunId::new();
    let doc_id = create_doc(&store, &run_id, value);
    (db, store, run_id, doc_id)
}

/// Setup with a standard test object
pub fn setup_standard_doc() -> (Arc<Database>, JsonStore, RunId, JsonDocId) {
    let value: JsonValue = serde_json::json!({
        "user": {
            "name": "Alice",
            "age": 30,
            "email": "alice@example.com"
        },
        "items": [
            {"id": 1, "name": "item1"},
            {"id": 2, "name": "item2"},
            {"id": 3, "name": "item3"}
        ],
        "config": {
            "enabled": true,
            "settings": {
                "theme": "dark",
                "language": "en"
            }
        }
    })
    .into();
    setup_doc(value)
}

// =============================================================================
// Assertion Helpers
// =============================================================================

/// Assert that getting a path returns expected value
pub fn assert_get(
    store: &JsonStore,
    run_id: &RunId,
    doc_id: &JsonDocId,
    path: &str,
    expected: JsonValue,
) {
    let actual = store
        .get(run_id, doc_id, &path.parse().unwrap())
        .expect("get failed")
        .expect("path not found");
    assert_eq!(
        actual.value, expected,
        "Value mismatch at path '{}': expected {:?}, got {:?}",
        path, expected, actual.value
    );
}

/// Assert that getting a path returns None (not found)
pub fn assert_get_none(store: &JsonStore, run_id: &RunId, doc_id: &JsonDocId, path: &str) {
    let actual = store
        .get(run_id, doc_id, &path.parse().unwrap())
        .expect("get failed");
    assert!(
        actual.is_none(),
        "Expected None at path '{}', got {:?}",
        path,
        actual
    );
}

/// Assert document version equals expected
pub fn assert_version(store: &JsonStore, run_id: &RunId, doc_id: &JsonDocId, expected: u64) {
    let actual = store
        .get_version(run_id, doc_id)
        .expect("get_version failed")
        .expect("doc not found");
    assert_eq!(
        actual, expected,
        "Version mismatch: expected {}, got {}",
        expected, actual
    );
}

/// Assert document exists
pub fn assert_exists(store: &JsonStore, run_id: &RunId, doc_id: &JsonDocId) {
    assert!(
        store.exists(run_id, doc_id).expect("exists failed"),
        "Document should exist"
    );
}

/// Assert document does not exist
pub fn assert_not_exists(store: &JsonStore, run_id: &RunId, doc_id: &JsonDocId) {
    assert!(
        !store.exists(run_id, doc_id).expect("exists failed"),
        "Document should not exist"
    );
}

// =============================================================================
// Concurrency Helpers
// =============================================================================

use std::sync::Barrier;
use std::thread;

/// Run two closures concurrently and wait for both to complete
pub fn run_concurrent<F1, F2, R1, R2>(f1: F1, f2: F2) -> (R1, R2)
where
    F1: FnOnce() -> R1 + Send + 'static,
    F2: FnOnce() -> R2 + Send + 'static,
    R1: Send + 'static,
    R2: Send + 'static,
{
    let barrier = Arc::new(Barrier::new(2));

    let b1 = barrier.clone();
    let h1 = thread::spawn(move || {
        b1.wait();
        f1()
    });

    let b2 = barrier.clone();
    let h2 = thread::spawn(move || {
        b2.wait();
        f2()
    });

    let r1 = h1.join().expect("Thread 1 panicked");
    let r2 = h2.join().expect("Thread 2 panicked");

    (r1, r2)
}

/// Run multiple closures concurrently
pub fn run_concurrent_n<F, R>(count: usize, f: F) -> Vec<R>
where
    F: Fn(usize) -> R + Send + Sync + 'static,
    R: Send + 'static,
{
    let barrier = Arc::new(Barrier::new(count));
    let f = Arc::new(f);

    let handles: Vec<_> = (0..count)
        .map(|i| {
            let b = barrier.clone();
            let f = f.clone();
            thread::spawn(move || {
                b.wait();
                f(i)
            })
        })
        .collect();

    handles
        .into_iter()
        .map(|h| h.join().expect("Thread panicked"))
        .collect()
}

// =============================================================================
// Timeout Helper
// =============================================================================

use std::time::Duration;

/// Run a function with a timeout
pub fn with_timeout<F, T>(timeout: Duration, f: F) -> Option<T>
where
    F: FnOnce() -> T + Send + 'static,
    T: Send + 'static,
{
    use std::sync::mpsc;

    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let result = f();
        let _ = tx.send(result);
    });

    rx.recv_timeout(timeout).ok()
}

// =============================================================================
// Random Generation (for fuzzing)
// =============================================================================

/// Generate a random JSON tree with controlled depth
pub fn random_json_tree(depth: usize, seed: u64) -> JsonValue {
    use std::hash::{Hash, Hasher};

    fn hash_seed(seed: u64, salt: u64) -> u64 {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        seed.hash(&mut hasher);
        salt.hash(&mut hasher);
        hasher.finish()
    }

    fn generate(depth: usize, seed: u64) -> JsonValue {
        if depth == 0 {
            // Leaf node - random primitive
            match seed % 4 {
                0 => JsonValue::from(seed as i64),
                1 => JsonValue::from(format!("str_{}", seed)),
                2 => JsonValue::from(seed % 2 == 0),
                _ => JsonValue::null(),
            }
        } else {
            // Container node
            if seed % 2 == 0 {
                // Object
                let mut map = serde_json::Map::new();
                let count = (seed % 5) as usize + 1;
                for i in 0..count {
                    let key = format!("key_{}", i);
                    let child = generate(depth - 1, hash_seed(seed, i as u64));
                    map.insert(key, child.into_inner());
                }
                JsonValue::from(serde_json::Value::Object(map))
            } else {
                // Array
                let count = (seed % 5) as usize + 1;
                let arr: Vec<serde_json::Value> = (0..count)
                    .map(|i| generate(depth - 1, hash_seed(seed, i as u64)).into_inner())
                    .collect();
                JsonValue::from(serde_json::Value::Array(arr))
            }
        }
    }

    generate(depth, seed)
}

/// Collect all valid paths in a JSON tree
pub fn collect_paths(value: &JsonValue) -> Vec<JsonPath> {
    fn collect(value: &serde_json::Value, current: JsonPath, paths: &mut Vec<JsonPath>) {
        paths.push(current.clone());

        match value {
            serde_json::Value::Object(obj) => {
                for (key, child) in obj {
                    let child_path = current.clone().key(key);
                    collect(child, child_path, paths);
                }
            }
            serde_json::Value::Array(arr) => {
                for (idx, child) in arr.iter().enumerate() {
                    let child_path = current.clone().index(idx);
                    collect(child, child_path, paths);
                }
            }
            _ => {}
        }
    }

    let mut paths = Vec::new();
    collect(value.as_inner(), JsonPath::root(), &mut paths);
    paths
}
