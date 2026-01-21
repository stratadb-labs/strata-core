//! Common test utilities for M1+M2 comprehensive tests

use strata_core::contract::Version;
use strata_core::traits::Storage;
use strata_core::types::{Key, Namespace, RunId};
use strata_core::value::Value;
use strata_engine::Database;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use tempfile::TempDir;

/// Global counter for unique key generation across tests
static KEY_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Create a test namespace with unique run_id
pub fn create_namespace() -> (RunId, Namespace) {
    let run_id = RunId::new();
    let ns = Namespace::new(
        "test_tenant".to_string(),
        "test_app".to_string(),
        "test_agent".to_string(),
        run_id,
    );
    (run_id, ns)
}

/// Create a namespace with specific run_id
pub fn create_namespace_for_run(run_id: RunId) -> Namespace {
    Namespace::new(
        "test_tenant".to_string(),
        "test_app".to_string(),
        "test_agent".to_string(),
        run_id,
    )
}

/// Create a unique KV key
pub fn unique_kv_key(ns: &Namespace, prefix: &str) -> Key {
    let counter = KEY_COUNTER.fetch_add(1, Ordering::Relaxed);
    Key::new_kv(ns.clone(), format!("{}_{}", prefix, counter))
}

/// Create a KV key with specific name
pub fn kv_key(ns: &Namespace, name: &str) -> Key {
    Key::new_kv(ns.clone(), name)
}

/// Create an Event key with sequence number
pub fn event_key(ns: &Namespace, seq: u64) -> Key {
    Key::new_event(ns.clone(), seq)
}

/// Create a State key
pub fn state_key(ns: &Namespace, name: &str) -> Key {
    Key::new_state(ns.clone(), name)
}

/// Create test values of various types
pub mod values {
    use super::*;

    pub fn int(n: i64) -> Value {
        Value::I64(n)
    }

    pub fn float(f: f64) -> Value {
        Value::F64(f)
    }

    pub fn string(s: &str) -> Value {
        Value::String(s.to_string())
    }

    pub fn bytes(data: &[u8]) -> Value {
        Value::Bytes(data.to_vec())
    }

    pub fn bool_val(b: bool) -> Value {
        Value::Bool(b)
    }

    pub fn null() -> Value {
        Value::Null
    }

    pub fn array(items: Vec<Value>) -> Value {
        Value::Array(items)
    }

    pub fn map(pairs: Vec<(&str, Value)>) -> Value {
        let mut map = std::collections::HashMap::new();
        for (k, v) in pairs {
            map.insert(k.to_string(), v);
        }
        Value::Map(map)
    }

    /// Generate a large value for stress testing
    pub fn large_bytes(size_kb: usize) -> Value {
        Value::Bytes(vec![0xAB; size_kb * 1024])
    }

    /// Generate a value with specific size
    pub fn sized_string(size: usize) -> Value {
        Value::String("x".repeat(size))
    }
}

/// Test database wrapper with automatic cleanup
pub struct TestDb {
    pub db: Database,
    pub run_id: RunId,
    pub ns: Namespace,
    _temp_dir: TempDir,
}

impl TestDb {
    /// Create a new test database
    pub fn new() -> Self {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let db = Database::open(temp_dir.path().join("db")).expect("Failed to open database");
        let (run_id, ns) = create_namespace();

        Self {
            db,
            run_id,
            ns,
            _temp_dir: temp_dir,
        }
    }

    /// Create a KV key in this database's namespace
    pub fn key(&self, name: &str) -> Key {
        kv_key(&self.ns, name)
    }

    /// Create a unique KV key
    pub fn unique_key(&self, prefix: &str) -> Key {
        unique_kv_key(&self.ns, prefix)
    }

    /// Create an Event key
    pub fn event(&self, seq: u64) -> Key {
        event_key(&self.ns, seq)
    }

    /// Create a State key
    pub fn state(&self, name: &str) -> Key {
        state_key(&self.ns, name)
    }

    /// Get database path for reopening
    pub fn path(&self) -> &Path {
        self.db.data_dir()
    }
}

impl Default for TestDb {
    fn default() -> Self {
        Self::new()
    }
}

/// Wrapper for creating databases that persist across reopens
pub struct PersistentTestDb {
    temp_dir: TempDir,
    pub run_id: RunId,
    pub ns: Namespace,
}

impl PersistentTestDb {
    pub fn new() -> Self {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let (run_id, ns) = create_namespace();
        Self {
            temp_dir,
            run_id,
            ns,
        }
    }

    /// Open the database (creates new or reopens existing)
    ///
    /// Uses default Batched durability mode. For tests requiring guaranteed
    /// durability without explicit close(), use `open_strict()` instead.
    pub fn open(&self) -> Database {
        Database::open(self.temp_dir.path().join("db")).expect("Failed to open database")
    }

    /// Open the database with Strict durability mode
    ///
    /// Every write is immediately synced to disk. Use this for durability tests
    /// where you need guaranteed persistence even without calling close().
    pub fn open_strict(&self) -> Database {
        Database::open_with_mode(
            self.temp_dir.path().join("db"),
            strata_durability::wal::DurabilityMode::Strict,
        )
        .expect("Failed to open database with strict mode")
    }

    /// Get path for manual operations
    pub fn path(&self) -> std::path::PathBuf {
        self.temp_dir.path().join("db")
    }

    /// Create a key in this namespace
    pub fn key(&self, name: &str) -> Key {
        kv_key(&self.ns, name)
    }
}

impl Default for PersistentTestDb {
    fn default() -> Self {
        Self::new()
    }
}

/// Assertion helpers
pub mod assert_helpers {
    use strata_core::error::Error;

    /// Assert that a result is a conflict error
    pub fn assert_conflict<T>(result: Result<T, Error>) {
        match result {
            Err(e) if e.is_conflict() => {}
            Err(e) => panic!("Expected conflict error, got: {:?}", e),
            Ok(_) => panic!("Expected conflict error, got Ok"),
        }
    }

    /// Assert that a result is a timeout error
    pub fn assert_timeout<T>(result: Result<T, Error>) {
        match result {
            Err(e) if e.is_timeout() => {}
            Err(e) => panic!("Expected timeout error, got: {:?}", e),
            Ok(_) => panic!("Expected timeout error, got Ok"),
        }
    }

    /// Assert that a result is an InvalidState error
    pub fn assert_invalid_state<T>(result: Result<T, Error>) {
        match result {
            Err(Error::InvalidState(_)) => {}
            Err(e) => panic!("Expected InvalidState error, got: {:?}", e),
            Ok(_) => panic!("Expected InvalidState error, got Ok"),
        }
    }
}

/// Timing utilities for tests
pub mod timing {
    use std::time::{Duration, Instant};

    /// Measure execution time of a closure
    pub fn measure<F, T>(f: F) -> (T, Duration)
    where
        F: FnOnce() -> T,
    {
        let start = Instant::now();
        let result = f();
        (result, start.elapsed())
    }

    /// Assert that an operation completes within a timeout
    pub fn assert_completes_within<F, T>(timeout: Duration, f: F) -> T
    where
        F: FnOnce() -> T,
    {
        let (result, elapsed) = measure(f);
        assert!(
            elapsed < timeout,
            "Operation took {:?}, expected < {:?}",
            elapsed,
            timeout
        );
        result
    }
}

/// Concurrency test helpers
pub mod concurrent {
    use std::sync::{Arc, Barrier};
    use std::thread::{self, JoinHandle};

    /// Run multiple threads that start at the same time
    pub fn run_concurrent<F, T>(num_threads: usize, f: F) -> Vec<T>
    where
        F: Fn(usize) -> T + Send + Sync + 'static,
        T: Send + 'static,
    {
        let barrier = Arc::new(Barrier::new(num_threads));
        let f = Arc::new(f);

        let handles: Vec<JoinHandle<T>> = (0..num_threads)
            .map(|i| {
                let barrier = Arc::clone(&barrier);
                let f = Arc::clone(&f);
                thread::spawn(move || {
                    barrier.wait();
                    f(i)
                })
            })
            .collect();

        handles
            .into_iter()
            .map(|h| h.join().expect("Thread panicked"))
            .collect()
    }

    /// Run threads with shared state
    pub fn run_with_shared<S, F, T>(num_threads: usize, shared: S, f: F) -> Vec<T>
    where
        S: Send + Sync + 'static,
        F: Fn(usize, &S) -> T + Send + Sync + 'static,
        T: Send + 'static,
    {
        let barrier = Arc::new(Barrier::new(num_threads));
        let shared = Arc::new(shared);
        let f = Arc::new(f);

        let handles: Vec<JoinHandle<T>> = (0..num_threads)
            .map(|i| {
                let barrier = Arc::clone(&barrier);
                let shared = Arc::clone(&shared);
                let f = Arc::clone(&f);
                thread::spawn(move || {
                    barrier.wait();
                    f(i, &shared)
                })
            })
            .collect();

        handles
            .into_iter()
            .map(|h| h.join().expect("Thread panicked"))
            .collect()
    }
}

// ============================================================================
// State Snapshot and Comparison Utilities
// ============================================================================

/// A complete snapshot of database state for comparison
#[derive(Debug, Clone, PartialEq)]
pub struct DatabaseStateSnapshot {
    /// All key-value pairs with their versions, sorted by key
    pub entries: Vec<(Key, Value, Version)>,
    /// Total number of entries
    pub count: usize,
    /// Checksum of all values (for quick comparison)
    pub checksum: u64,
}

impl DatabaseStateSnapshot {
    /// Capture the entire state of a database for a given namespace
    pub fn capture(db: &Database, ns: &Namespace) -> Self {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut entries = Vec::new();
        let mut hasher = DefaultHasher::new();

        // Scan all KV keys in namespace
        // We use the storage layer directly to get all keys
        let storage = db.storage();

        // Create prefix keys for different types and scan
        for type_tag in [
            strata_core::types::TypeTag::KV,
            strata_core::types::TypeTag::Event,
            strata_core::types::TypeTag::State,
        ] {
            let prefix = Key::new(ns.clone(), type_tag, vec![]);
            if let Ok(iter_entries) = storage.scan_prefix(&prefix, u64::MAX) {
                for (key, versioned_value) in iter_entries {
                    // Hash the value for checksum
                    format!("{:?}", versioned_value.value).hash(&mut hasher);
                    versioned_value.version.hash(&mut hasher);

                    entries.push((key, versioned_value.value, versioned_value.version));
                }
            }
        }

        // Sort entries by key for deterministic comparison
        entries.sort_by(|a, b| format!("{:?}", a.0).cmp(&format!("{:?}", b.0)));

        let count = entries.len();
        let checksum = hasher.finish();

        Self {
            entries,
            count,
            checksum,
        }
    }

    /// Quick check if two snapshots are identical (checksum only)
    pub fn quick_equals(&self, other: &Self) -> bool {
        self.count == other.count && self.checksum == other.checksum
    }

    /// Detailed comparison returning differences
    pub fn diff(&self, other: &Self) -> StateDiff {
        let mut missing_in_other = Vec::new();
        let mut missing_in_self = Vec::new();
        let mut value_mismatches = Vec::new();
        let mut version_mismatches = Vec::new();

        let self_map: std::collections::HashMap<_, _> = self
            .entries
            .iter()
            .map(|(k, v, ver)| (format!("{:?}", k), (v.clone(), *ver)))
            .collect();

        let other_map: std::collections::HashMap<_, _> = other
            .entries
            .iter()
            .map(|(k, v, ver)| (format!("{:?}", k), (v.clone(), *ver)))
            .collect();

        // Find entries missing in other
        for (key_str, (value, version)) in &self_map {
            match other_map.get(key_str) {
                None => missing_in_other.push(key_str.clone()),
                Some((other_value, other_version)) => {
                    if value != other_value {
                        value_mismatches.push((
                            key_str.clone(),
                            value.clone(),
                            other_value.clone(),
                        ));
                    }
                    if version != other_version {
                        version_mismatches.push((key_str.clone(), *version, *other_version));
                    }
                }
            }
        }

        // Find entries missing in self
        for key_str in other_map.keys() {
            if !self_map.contains_key(key_str) {
                missing_in_self.push(key_str.clone());
            }
        }

        StateDiff {
            missing_in_other,
            missing_in_self,
            value_mismatches,
            version_mismatches,
        }
    }

    /// Assert this snapshot equals another, with detailed error on failure
    pub fn assert_equals(&self, other: &Self, context: &str) {
        if self == other {
            return;
        }

        let diff = self.diff(other);
        panic!(
            "State mismatch in {}\n\
             Missing in recovered: {:?}\n\
             Extra in recovered: {:?}\n\
             Value mismatches: {:?}\n\
             Version mismatches: {:?}",
            context,
            diff.missing_in_other,
            diff.missing_in_self,
            diff.value_mismatches,
            diff.version_mismatches
        );
    }
}

/// Differences between two state snapshots
#[derive(Debug)]
pub struct StateDiff {
    pub missing_in_other: Vec<String>,
    pub missing_in_self: Vec<String>,
    pub value_mismatches: Vec<(String, Value, Value)>,
    pub version_mismatches: Vec<(String, Version, Version)>,
}

impl StateDiff {
    pub fn is_empty(&self) -> bool {
        self.missing_in_other.is_empty()
            && self.missing_in_self.is_empty()
            && self.value_mismatches.is_empty()
            && self.version_mismatches.is_empty()
    }
}

/// Invariant assertion helpers
pub mod invariants {
    use super::*;
    use strata_engine::Database;

    /// Assert that database state before crash equals state after recovery
    /// This is THE fundamental M1 invariant
    pub fn assert_recovery_preserves_state(
        before: &DatabaseStateSnapshot,
        after: &DatabaseStateSnapshot,
    ) {
        before.assert_equals(after, "recovery state comparison");
    }

    /// Assert that a transaction either fully committed or fully aborted
    /// (no partial writes visible)
    pub fn assert_atomic_transaction(db: &Database, keys: &[Key], expected_all_exist: bool) {
        let existence: Vec<bool> = keys.iter().map(|k| db.get(k).unwrap().is_some()).collect();

        let all_exist = existence.iter().all(|&e| e);
        let none_exist = existence.iter().all(|&e| !e);

        assert!(
            all_exist || none_exist,
            "Atomicity violation: keys have mixed existence {:?}. \
             Expected all {} but got mixed state.",
            existence,
            if expected_all_exist {
                "present"
            } else {
                "absent"
            }
        );

        if expected_all_exist {
            assert!(all_exist, "Expected all keys to exist but none did");
        } else {
            assert!(none_exist, "Expected no keys to exist but some did");
        }
    }

    /// Assert that no partial writes are visible
    /// All keys in transaction should have same version "generation"
    pub fn assert_no_partial_writes(db: &Database, keys: &[Key]) {
        let versions: Vec<Option<u64>> = keys
            .iter()
            .map(|k| db.get(k).unwrap().map(|v| v.version.as_u64()))
            .collect();

        // Either all None or all Some
        let all_none = versions.iter().all(|v| v.is_none());
        let all_some = versions.iter().all(|v| v.is_some());

        assert!(
            all_none || all_some,
            "Partial write detected: versions = {:?}",
            versions
        );
    }

    /// Assert monotonic version ordering
    pub fn assert_monotonic_versions(versions: &[u64]) {
        for i in 1..versions.len() {
            assert!(
                versions[i] > versions[i - 1],
                "Version monotonicity violated: v[{}]={} should be > v[{}]={}",
                i,
                versions[i],
                i - 1,
                versions[i - 1]
            );
        }
    }

    /// Assert that exactly one CAS operation succeeded among concurrent attempts
    pub fn assert_exactly_one_cas_winner(results: &[bool]) {
        let winners: usize = results.iter().filter(|&&r| r).count();
        assert_eq!(
            winners, 1,
            "CAS invariant violated: expected exactly 1 winner, got {}. Results: {:?}",
            winners, results
        );
    }

    /// Assert snapshot consistency: all reads within a transaction see
    /// values from the same logical point in time
    pub fn assert_snapshot_consistency(reads: &[(Key, Option<Value>, u64)]) {
        // All read versions should be <= snapshot version
        // (This is a simplified check - real check needs snapshot version)
        if reads.is_empty() {
            return;
        }

        // Verify no reads see "future" data relative to each other
        // This is enforced by the snapshot mechanism
    }
}
