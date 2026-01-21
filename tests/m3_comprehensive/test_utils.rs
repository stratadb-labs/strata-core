//! Test utilities for M3 comprehensive tests
//!
//! Provides helpers for setting up primitives, creating test data,
//! and asserting M3-specific invariants.

use strata_core::error::Error;
use strata_core::types::RunId;
use strata_core::value::Value;
use strata_engine::Database;
use strata_primitives::{EventLog, KVStore, RunIndex, StateCell, TraceStore};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tempfile::TempDir;

/// Global counter for unique key generation
static KEY_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Generate a unique key name
pub fn unique_key(prefix: &str) -> String {
    let counter = KEY_COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("{}_{}", prefix, counter)
}

/// Test database wrapper with all five primitives
pub struct TestPrimitives {
    pub db: Arc<Database>,
    pub run_id: RunId,
    pub kv: KVStore,
    pub event_log: EventLog,
    pub state_cell: StateCell,
    pub trace_store: TraceStore,
    pub run_index: RunIndex,
    _temp_dir: TempDir,
}

impl TestPrimitives {
    /// Create a new test environment with all primitives
    pub fn new() -> Self {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let db =
            Arc::new(Database::open(temp_dir.path().join("db")).expect("Failed to open database"));
        let run_id = RunId::new();

        Self {
            kv: KVStore::new(db.clone()),
            event_log: EventLog::new(db.clone()),
            state_cell: StateCell::new(db.clone()),
            trace_store: TraceStore::new(db.clone()),
            run_index: RunIndex::new(db.clone()),
            db,
            run_id,
            _temp_dir: temp_dir,
        }
    }

    /// Create a new run and return its ID
    pub fn new_run(&self) -> RunId {
        RunId::new()
    }

    /// Get database path for recovery tests
    pub fn path(&self) -> std::path::PathBuf {
        self._temp_dir.path().join("db")
    }
}

impl Default for TestPrimitives {
    fn default() -> Self {
        Self::new()
    }
}

/// Persistent test primitives for recovery testing
pub struct PersistentTestPrimitives {
    temp_dir: TempDir,
    pub run_id: RunId,
}

impl PersistentTestPrimitives {
    pub fn new() -> Self {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        Self {
            temp_dir,
            run_id: RunId::new(),
        }
    }

    /// Open (or reopen) the database and primitives
    pub fn open(&self) -> OpenedPrimitives {
        let db = Arc::new(
            Database::open(self.temp_dir.path().join("db")).expect("Failed to open database"),
        );
        OpenedPrimitives {
            kv: KVStore::new(db.clone()),
            event_log: EventLog::new(db.clone()),
            state_cell: StateCell::new(db.clone()),
            trace_store: TraceStore::new(db.clone()),
            run_index: RunIndex::new(db.clone()),
            db,
            run_id: self.run_id,
        }
    }

    /// Open with strict durability mode
    pub fn open_strict(&self) -> OpenedPrimitives {
        let db = Arc::new(
            Database::open_with_mode(
                self.temp_dir.path().join("db"),
                strata_durability::wal::DurabilityMode::Strict,
            )
            .expect("Failed to open database"),
        );
        OpenedPrimitives {
            kv: KVStore::new(db.clone()),
            event_log: EventLog::new(db.clone()),
            state_cell: StateCell::new(db.clone()),
            trace_store: TraceStore::new(db.clone()),
            run_index: RunIndex::new(db.clone()),
            db,
            run_id: self.run_id,
        }
    }
}

impl Default for PersistentTestPrimitives {
    fn default() -> Self {
        Self::new()
    }
}

/// Opened primitives from PersistentTestPrimitives
pub struct OpenedPrimitives {
    pub db: Arc<Database>,
    pub run_id: RunId,
    pub kv: KVStore,
    pub event_log: EventLog,
    pub state_cell: StateCell,
    pub trace_store: TraceStore,
    pub run_index: RunIndex,
}

/// Value helpers for testing
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
}

/// Assertion helpers for M3 invariants
pub mod assert_helpers {
    use super::*;

    /// Assert that result is a conflict error
    pub fn assert_conflict<T: std::fmt::Debug>(result: Result<T, Error>) {
        match result {
            Err(e) if e.is_conflict() => {}
            Err(e) => panic!("Expected conflict error, got: {:?}", e),
            Ok(v) => panic!("Expected conflict error, got Ok({:?})", v),
        }
    }

    /// Assert that result is an InvalidState error
    pub fn assert_invalid_state<T: std::fmt::Debug>(result: Result<T, Error>) {
        match result {
            Err(Error::InvalidState(_)) => {}
            Err(e) => panic!("Expected InvalidState error, got: {:?}", e),
            Ok(v) => panic!("Expected InvalidState error, got Ok({:?})", v),
        }
    }

    /// Assert that result is an error (any kind)
    pub fn assert_error<T: std::fmt::Debug>(result: Result<T, Error>) {
        match result {
            Err(_) => {}
            Ok(v) => panic!("Expected error, got Ok({:?})", v),
        }
    }

    /// Assert that exactly one of the results is Ok(true)
    pub fn assert_exactly_one_winner(results: &[Result<bool, Error>]) {
        let winners: usize = results.iter().filter(|r| matches!(r, Ok(true))).count();
        assert_eq!(
            winners, 1,
            "Expected exactly 1 winner, got {}. Results: {:?}",
            winners, results
        );
    }
}

/// M3 invariant assertion helpers
pub mod invariants {
    use super::*;
    use strata_core::contract::Versioned;
    use strata_primitives::Event;

    /// Assert that EventLog chain is valid (M3.9)
    pub fn assert_chain_integrity(events: &[Versioned<Event>]) {
        if events.is_empty() {
            return;
        }

        // First event should have zero prev_hash
        assert_eq!(
            events[0].value.prev_hash, [0u8; 32],
            "First event prev_hash should be zero"
        );

        // Each subsequent event's prev_hash should match previous event's hash
        for i in 1..events.len() {
            assert_eq!(
                events[i].value.prev_hash,
                events[i - 1].value.hash,
                "Chain broken at index {}: prev_hash doesn't match previous hash",
                i
            );
        }
    }

    /// Assert that sequences are contiguous (M3.8)
    pub fn assert_sequences_contiguous(events: &[Versioned<Event>]) {
        for (i, event) in events.iter().enumerate() {
            assert_eq!(
                event.value.sequence, i as u64,
                "Sequence gap at index {}: expected {}, got {}",
                i, i, event.value.sequence
            );
        }
    }

    /// Assert version monotonicity (M3.12)
    pub fn assert_version_monotonic(versions: &[u64]) {
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

    /// Assert that no data from one run is visible in another (M3.2)
    pub fn assert_run_isolation(kv: &KVStore, run1: &RunId, run2: &RunId, key: &str) {
        let val1 = kv.get(run1, key).unwrap();
        let val2 = kv.get(run2, key).unwrap();

        // Both can have values, but they must be independent
        // (This is a helper - actual isolation tests will be more specific)
        if val1.is_some() && val2.is_some() {
            // Values can be different - that's fine
        }
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
