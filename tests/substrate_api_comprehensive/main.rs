//! Substrate API Comprehensive Test Suite
//!
//! This test suite provides comprehensive coverage of the Substrate API layer,
//! testing correctness, durability, concurrency, and transaction semantics.
//!
//! ## Primitives Covered
//!
//! - `kv`: KVStore - key-value storage with versioning
//! - `eventlog`: EventLog - append-only event streams
//!
//! ## Running Tests
//!
//! ```bash
//! # Run all substrate API comprehensive tests
//! cargo test --test substrate_api_comprehensive
//!
//! # Run KV tests only
//! cargo test --test substrate_api_comprehensive kv::
//!
//! # Run EventLog tests only
//! cargo test --test substrate_api_comprehensive eventlog::
//!
//! # Run specific module
//! cargo test --test substrate_api_comprehensive kv::durability
//!
//! # Run with output
//! cargo test --test substrate_api_comprehensive -- --nocapture
//! ```

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use strata_api::substrate::{ApiRunId, EventLog, JsonStore, KVStore, KVStoreBatch, RunIndex, StateCell, SubstrateImpl, VectorStore};
use strata_core::{Value, Version};
use strata_engine::Database;
use tempfile::TempDir;

/// Helper to create Value::Object from a list of key-value pairs
///
/// Usage: `obj([("key", Value::Int(1)), ("other", Value::String("hello".into()))])`
pub fn obj<I>(pairs: I) -> Value
where
    I: IntoIterator<Item = (&'static str, Value)>,
{
    Value::Object(pairs.into_iter().map(|(k, v)| (k.to_string(), v)).collect())
}

/// Helper to create Value::Object from owned strings
pub fn obj_owned<I>(pairs: I) -> Value
where
    I: IntoIterator<Item = (String, Value)>,
{
    Value::Object(pairs.into_iter().collect())
}

// Test data loader
pub mod test_data;

// Test modules by primitive
pub mod eventlog;
pub mod jsonstore;
pub mod kv;
pub mod statecell;
pub mod vectorstore;

// =============================================================================
// SHARED TEST UTILITIES
// =============================================================================

/// Create an in-memory test database (fastest, no persistence)
pub fn create_inmemory_db() -> Arc<Database> {
    Arc::new(
        Database::builder()
            .in_memory()
            .open_temp()
            .expect("Failed to create in-memory database"),
    )
}

/// Create a buffered test database (balanced speed/durability)
pub fn create_buffered_db() -> Arc<Database> {
    Arc::new(
        Database::builder()
            .buffered()
            .open_temp()
            .expect("Failed to create buffered database"),
    )
}

/// Create a strict test database (fsync on every write)
pub fn create_strict_db() -> Arc<Database> {
    Arc::new(
        Database::builder()
            .strict()
            .open_temp()
            .expect("Failed to create strict database"),
    )
}

/// Create a SubstrateImpl from a database
pub fn create_substrate(db: Arc<Database>) -> SubstrateImpl {
    SubstrateImpl::new(db)
}

/// Quick setup: create in-memory db + substrate
pub fn quick_setup() -> (Arc<Database>, SubstrateImpl) {
    let db = create_inmemory_db();
    let substrate = create_substrate(db.clone());
    (db, substrate)
}

/// Test database wrapper with durability support and reopen capability
pub struct TestDb {
    pub dir: TempDir,
    pub db: Option<Arc<Database>>,
    pub mode: &'static str,
}

impl TestDb {
    /// Create a new test database with buffered durability (file-backed)
    pub fn new_buffered() -> Self {
        let dir = TempDir::new().expect("Failed to create temp directory");
        let db = Arc::new(
            Database::builder()
                .path(dir.path())
                .buffered()
                .open()
                .expect("Failed to create buffered database"),
        );
        TestDb {
            dir,
            db: Some(db),
            mode: "buffered",
        }
    }

    /// Create a new test database with strict durability (file-backed)
    pub fn new_strict() -> Self {
        let dir = TempDir::new().expect("Failed to create temp directory");
        let db = Arc::new(
            Database::builder()
                .path(dir.path())
                .strict()
                .open()
                .expect("Failed to create strict database"),
        );
        TestDb {
            dir,
            db: Some(db),
            mode: "strict",
        }
    }

    /// Create a new in-memory test database (no persistence)
    pub fn new_in_memory() -> Self {
        let dir = TempDir::new().expect("Failed to create temp directory");
        let db = Arc::new(
            Database::builder()
                .in_memory()
                .open_temp()
                .expect("Failed to create in-memory database"),
        );
        TestDb {
            dir,
            db: Some(db),
            mode: "in_memory",
        }
    }

    /// Get the database Arc
    pub fn db(&self) -> Arc<Database> {
        self.db.as_ref().unwrap().clone()
    }

    /// Get the substrate implementation
    pub fn substrate(&self) -> SubstrateImpl {
        SubstrateImpl::new(self.db())
    }

    /// Get the WAL file path
    pub fn wal_path(&self) -> PathBuf {
        self.dir.path().join("wal.bin")
    }

    /// Get the database directory path
    pub fn db_path(&self) -> &Path {
        self.dir.path()
    }

    /// Simulate crash by closing and reopening the database
    /// Returns true if reopen succeeded
    pub fn reopen(&mut self) -> bool {
        // Drop the current database
        self.db = None;

        // Reopen with same mode
        let result = match self.mode {
            "buffered" => Database::builder()
                .path(self.dir.path())
                .buffered()
                .open(),
            "strict" => Database::builder()
                .path(self.dir.path())
                .strict()
                .open(),
            "in_memory" => {
                // In-memory databases don't persist - create fresh
                Database::builder().in_memory().open_temp()
            }
            _ => unreachable!(),
        };

        match result {
            Ok(db) => {
                self.db = Some(Arc::new(db));
                true
            }
            Err(_) => false,
        }
    }
}

/// Run a test across all three durability modes
pub fn test_across_modes<F, T>(test_name: &str, workload: F)
where
    F: Fn(Arc<Database>) -> T,
    T: PartialEq + std::fmt::Debug,
{
    let modes = [
        ("in_memory", create_inmemory_db()),
        ("buffered", create_buffered_db()),
        ("strict", create_strict_db()),
    ];

    let mut results: Vec<(&str, T)> = Vec::new();

    for (mode_name, db) in modes {
        let result = workload(db);
        results.push((mode_name, result));
    }

    // Assert all results identical to first (in_memory)
    let (first_mode, first_result) = &results[0];
    for (mode, result) in &results[1..] {
        assert_eq!(
            first_result, result,
            "SEMANTIC DRIFT in '{}': {:?} produced {:?}, but {:?} produced {:?}",
            test_name, first_mode, first_result, mode, result
        );
    }
}

/// Run a test across all three durability modes using SubstrateImpl
pub fn test_across_substrate_modes<F>(test_fn: F)
where
    F: Fn(&SubstrateImpl),
{
    // Test in-memory mode
    let db = create_inmemory_db();
    let substrate = SubstrateImpl::new(db);
    test_fn(&substrate);

    // Test buffered mode
    let db = create_buffered_db();
    let substrate = SubstrateImpl::new(db);
    test_fn(&substrate);

    // Test strict mode
    let db = create_strict_db();
    let substrate = SubstrateImpl::new(db);
    test_fn(&substrate);
}

/// Create a persistent database at the given path
pub fn create_persistent_db(path: &std::path::Path) -> SubstrateImpl {
    let db = Arc::new(
        Database::builder()
            .path(path)
            .buffered()
            .open()
            .expect("Failed to create persistent database"),
    );
    SubstrateImpl::new(db)
}

/// Standard test values covering all 8 types
pub fn standard_test_values() -> Vec<(&'static str, Value)> {
    vec![
        ("null", Value::Null),
        ("bool_true", Value::Bool(true)),
        ("bool_false", Value::Bool(false)),
        ("int_pos", Value::Int(42)),
        ("int_neg", Value::Int(-42)),
        ("int_max", Value::Int(i64::MAX)),
        ("int_min", Value::Int(i64::MIN)),
        ("float_pos", Value::Float(3.14159)),
        ("float_neg", Value::Float(-2.71828)),
        ("string", Value::String("hello world".into())),
        ("string_unicode", Value::String("世界 🌍 مرحبا".into())),
        ("string_empty", Value::String("".into())),
        ("bytes", Value::Bytes(vec![0x00, 0x01, 0xFF, 0xFE])),
        ("bytes_empty", Value::Bytes(vec![])),
        (
            "array",
            Value::Array(vec![Value::Int(1), Value::String("two".into())]),
        ),
        ("object", {
            let mut m = HashMap::new();
            m.insert("nested".to_string(), Value::Int(123));
            Value::Object(m)
        }),
    ]
}

/// Compare two values, handling special float cases (NaN, Infinity)
pub fn values_equal(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Float(fa), Value::Float(fb)) => {
            if fa.is_nan() && fb.is_nan() {
                true
            } else if fa.is_infinite() && fb.is_infinite() {
                fa.signum() == fb.signum()
            } else {
                fa == fb
            }
        }
        _ => a == b,
    }
}
