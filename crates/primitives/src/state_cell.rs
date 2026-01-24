//! StateCell: CAS-based versioned cells for coordination
//!
//! ## Design Principles
//!
//! 1. **Versioned Updates**: Every update increments the version.
//! 2. **CAS Semantics**: Compare-and-swap ensures safe concurrent updates.
//! 3. **Purity Requirement**: Transition closures MUST be pure functions.
//!
//! ## Naming Note
//!
//! This is "StateCell" not "StateMachine". In M3, this is a simple CAS cell.
//! Full state machine semantics (transitions, guards) may come in M5+.
//!
//! ## Purity Requirement
//!
//! The `transition()` closure may be called multiple times due to OCC retries.
//! Closures MUST be pure functions:
//! - Pure function of inputs (result depends only on &State argument)
//! - No I/O (no file, network, console operations)
//! - No external mutation (don't modify variables outside closure scope)
//! - No irreversible effects (no logging, metrics, API calls)
//! - Idempotent (same input produces same output)
//!
//! ## Key Design
//!
//! - TypeTag: State (0x03)
//! - Key format: `<namespace>:<TypeTag::State>:<cell_name>`

use crate::extensions::StateCellExt;
use strata_concurrency::TransactionContext;
use strata_core::contract::{Version, Versioned};
use strata_core::error::Result;
use strata_core::types::{Key, Namespace, RunId};
use strata_core::value::Value;
use strata_core::Timestamp;
use strata_engine::{Database, RetryConfig};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

// Re-export State from core
pub use strata_core::primitives::State;

/// Serialize a struct to Value::String for storage
fn to_stored_value<T: Serialize>(v: &T) -> Value {
    match serde_json::to_string(v) {
        Ok(s) => Value::String(s),
        Err(_) => Value::Null,
    }
}

/// Deserialize from Value::String storage
fn from_stored_value<T: for<'de> Deserialize<'de>>(
    v: &Value,
) -> std::result::Result<T, serde_json::Error> {
    match v {
        Value::String(s) => serde_json::from_str(s),
        _ => serde_json::from_str("null"), // Will fail with appropriate error
    }
}

/// CAS-based versioned cells for coordination
///
/// ## Design
///
/// Each cell has a value and monotonically increasing version.
/// Updates via CAS ensure safe concurrent access.
///
/// ## Example
///
/// ```rust,ignore
/// use strata_primitives::StateCell;
/// use strata_core::value::Value;
///
/// let sc = StateCell::new(db.clone());
/// let run_id = RunId::new();
///
/// // Initialize a counter
/// sc.init(&run_id, "counter", Value::Int(0))?;
///
/// // Read current state
/// let state = sc.read(&run_id, "counter")?.unwrap();
/// assert_eq!(state.version, 1);
///
/// // CAS update
/// sc.cas(&run_id, "counter", 1, Value::Int(1))?;
///
/// // Transition with automatic retry
/// sc.transition(&run_id, "counter", |state| {
///     let current = state.value.as_i64().unwrap_or(0);
///     Ok((Value::Int(current + 1), current + 1))
/// })?;
/// ```
#[derive(Clone)]
pub struct StateCell {
    db: Arc<Database>,
}

impl StateCell {
    /// Create new StateCell instance
    pub fn new(db: Arc<Database>) -> Self {
        Self { db }
    }

    /// Get the underlying database reference
    pub fn database(&self) -> &Arc<Database> {
        &self.db
    }

    /// Build namespace for run-scoped operations
    fn namespace_for_run(&self, run_id: &RunId) -> Namespace {
        Namespace::for_run(*run_id)
    }

    /// Build key for state cell
    fn key_for(&self, run_id: &RunId, name: &str) -> Key {
        Key::new_state(self.namespace_for_run(run_id), name)
    }

    // ========== Read/Init/Delete Operations (Story #181, #468) ==========

    /// Initialize a cell with a value (only if it doesn't exist)
    ///
    /// Returns `Versioned<u64>` containing the version number with metadata.
    /// The version uses `Version::Counter` type per M9 spec.
    ///
    /// # Story #468: StateCell Versioned Returns
    pub fn init(&self, run_id: &RunId, name: &str, value: Value) -> Result<Versioned<u64>> {
        self.db.transaction(*run_id, |txn| {
            let key = self.key_for(run_id, name);

            // Check if exists
            if txn.get(&key)?.is_some() {
                return Err(strata_core::error::Error::InvalidOperation(format!(
                    "StateCell '{}' already exists",
                    name
                )));
            }

            // Create new state
            let state = State::new(value);
            txn.put(key, to_stored_value(&state))?;
            Ok(Versioned::new(state.version, Version::counter(state.version)))
        })
    }

    /// Read current state (FAST PATH)
    ///
    /// Bypasses full transaction overhead for read-only access.
    /// Uses direct snapshot read which maintains snapshot isolation.
    ///
    /// Returns `Versioned<State>` with `Version::Counter` type per M9 spec.
    ///
    /// # Story #468: StateCell Versioned Returns
    pub fn read(&self, run_id: &RunId, name: &str) -> Result<Option<Versioned<State>>> {
        use strata_core::traits::SnapshotView;

        let snapshot = self.db.storage().create_snapshot();
        let key = self.key_for(run_id, name);

        match snapshot.get(&key)? {
            Some(vv) => {
                let state: State = from_stored_value(&vv.value)
                    .map_err(|e| strata_core::error::Error::SerializationError(e.to_string()))?;
                let version = Version::counter(state.version);
                let timestamp = Timestamp::from_micros(state.updated_at as u64);
                Ok(Some(Versioned::with_timestamp(state, version, timestamp)))
            }
            None => Ok(None),
        }
    }

    /// Read current state (with full transaction)
    ///
    /// Use this when you need transaction semantics.
    ///
    /// # Story #468: StateCell Versioned Returns
    pub fn read_in_transaction(&self, run_id: &RunId, name: &str) -> Result<Option<Versioned<State>>> {
        self.db.transaction(*run_id, |txn| {
            let key = self.key_for(run_id, name);
            match txn.get(&key)? {
                Some(v) => {
                    let state: State = from_stored_value(&v).map_err(|e| {
                        strata_core::error::Error::SerializationError(e.to_string())
                    })?;
                    let version = Version::counter(state.version);
                    let timestamp = Timestamp::from_micros(state.updated_at as u64);
                    Ok(Some(Versioned::with_timestamp(state, version, timestamp)))
                }
                None => Ok(None),
            }
        })
    }

    /// Delete a cell
    ///
    /// Returns true if deleted, false if didn't exist
    pub fn delete(&self, run_id: &RunId, name: &str) -> Result<bool> {
        self.db.transaction(*run_id, |txn| {
            let key = self.key_for(run_id, name);

            // Check if exists first
            if txn.get(&key)?.is_none() {
                return Ok(false);
            }

            txn.delete(key)?;
            Ok(true)
        })
    }

    /// Check if a cell exists (FAST PATH)
    ///
    /// Uses direct snapshot read which maintains snapshot isolation.
    pub fn exists(&self, run_id: &RunId, name: &str) -> Result<bool> {
        use strata_core::traits::SnapshotView;

        let snapshot = self.db.storage().create_snapshot();
        let key = self.key_for(run_id, name);
        Ok(snapshot.get(&key)?.is_some())
    }

    /// List all cell names in run
    pub fn list(&self, run_id: &RunId) -> Result<Vec<String>> {
        self.db.transaction(*run_id, |txn| {
            let ns = self.namespace_for_run(run_id);
            let prefix = Key::new_state(ns, "");
            let results = txn.scan_prefix(&prefix)?;
            Ok(results
                .into_iter()
                .map(|(key, _)| String::from_utf8_lossy(&key.user_key).to_string())
                .collect())
        })
    }

    /// Get version history for a cell
    ///
    /// Returns historical versions newest first.
    /// Each entry contains the value and counter version at that point in time.
    ///
    /// ## Parameters
    ///
    /// - `limit`: Maximum number of versions to return
    /// - `before_counter`: Only return versions with counter < this value
    ///
    /// ## Example
    ///
    /// ```ignore
    /// // Get last 10 versions
    /// let history = sc.history(&run_id, "cell", Some(10), None)?;
    ///
    /// // Paginate: get next 10 after counter 50
    /// let page2 = sc.history(&run_id, "cell", Some(10), Some(50))?;
    /// ```
    pub fn history(
        &self,
        run_id: &RunId,
        name: &str,
        limit: Option<usize>,
        before_counter: Option<u64>,
    ) -> Result<Vec<Versioned<Value>>> {
        use strata_core::traits::Storage;

        let key = self.key_for(run_id, name);

        // Get raw history from storage layer
        // Storage uses transaction versions, but StateCell stores counter inside State struct
        let raw_history = self.db.storage().get_history(&key, limit, None)?;

        // Convert storage entries to StateCell format
        // Each stored value is a serialized State struct containing counter version
        let mut results: Vec<Versioned<Value>> = Vec::new();

        for versioned_value in raw_history {
            // Deserialize the State struct from storage
            let state: State = match from_stored_value(&versioned_value.value) {
                Ok(s) => s,
                Err(_) => continue, // Skip malformed entries
            };

            // Apply before_counter filter (based on cell's internal counter, not txn version)
            if let Some(before) = before_counter {
                if state.version >= before {
                    continue;
                }
            }

            // Build Versioned<Value> with counter-based version
            let versioned = Versioned::with_timestamp(
                state.value,
                Version::counter(state.version),
                Timestamp::from_micros(state.updated_at as u64),
            );

            results.push(versioned);

            // Apply limit after filtering
            if let Some(max) = limit {
                if results.len() >= max {
                    break;
                }
            }
        }

        Ok(results)
    }

    // ========== CAS & Set Operations (Story #182, #468) ==========

    /// Compare-and-swap: Update only if version matches
    ///
    /// Returns `Versioned<u64>` containing new version on success.
    /// Uses `Version::Counter` type per M9 spec.
    ///
    /// # Story #468: StateCell Versioned Returns
    pub fn cas(
        &self,
        run_id: &RunId,
        name: &str,
        expected_version: u64,
        new_value: Value,
    ) -> Result<Versioned<u64>> {
        self.db.transaction(*run_id, |txn| {
            let key = self.key_for(run_id, name);

            let current: State = match txn.get(&key)? {
                Some(v) => from_stored_value(&v)
                    .map_err(|e| strata_core::error::Error::SerializationError(e.to_string()))?,
                None => {
                    return Err(strata_core::error::Error::InvalidOperation(format!(
                        "StateCell '{}' not found",
                        name
                    )))
                }
            };

            if current.version != expected_version {
                return Err(strata_core::error::Error::VersionMismatch {
                    expected: expected_version,
                    actual: current.version,
                });
            }

            let new_state = State {
                value: new_value,
                version: current.version + 1,
                updated_at: State::now(),
            };

            txn.put(key, to_stored_value(&new_state))?;
            Ok(Versioned::new(new_state.version, Version::counter(new_state.version)))
        })
    }

    /// Unconditional set (force write)
    ///
    /// Always succeeds, overwrites any existing value.
    /// Creates the cell if it doesn't exist.
    ///
    /// # Story #468: StateCell Versioned Returns
    pub fn set(&self, run_id: &RunId, name: &str, value: Value) -> Result<Versioned<u64>> {
        self.db.transaction(*run_id, |txn| {
            let key = self.key_for(run_id, name);

            let new_version = match txn.get(&key)? {
                Some(v) => {
                    let current: State = from_stored_value(&v).map_err(|e| {
                        strata_core::error::Error::SerializationError(e.to_string())
                    })?;
                    current.version + 1
                }
                None => 1,
            };

            let new_state = State {
                value,
                version: new_version,
                updated_at: State::now(),
            };

            txn.put(key, to_stored_value(&new_state))?;
            Ok(Versioned::new(new_state.version, Version::counter(new_state.version)))
        })
    }

    // ========== Transition Closure Pattern (Story #183, #468) ==========

    /// Apply a transition function with automatic retry on conflict
    ///
    /// Returns `(user_result, Versioned<u64>)` tuple. The version is the version
    /// number after the transition commits, useful for tracking without a separate read.
    ///
    /// ## Purity Requirement
    ///
    /// The closure `f` MAY BE CALLED MULTIPLE TIMES due to OCC retries.
    /// It MUST be a pure function:
    /// - No I/O (file, network, console)
    /// - No external mutation
    /// - No irreversible effects (logging, metrics)
    /// - Idempotent (same input -> same output)
    ///
    /// ## Implementation Note
    ///
    /// This method performs read + closure + write in a SINGLE TRANSACTION
    /// to ensure atomic OCC validation. The entire transaction retries on
    /// conflict, not just the CAS operation.
    ///
    /// ## Example
    ///
    /// ```rust,ignore
    /// let (incremented, versioned) = sc.transition(run_id, "counter", |state| {
    ///     let current = state.value.as_i64().unwrap_or(0);
    ///     Ok((Value::Int(current + 1), current + 1))
    /// })?;
    /// ```
    ///
    /// # Story #468: StateCell Versioned Returns
    pub fn transition<F, T>(&self, run_id: &RunId, name: &str, f: F) -> Result<(T, Versioned<u64>)>
    where
        F: Fn(&State) -> Result<(Value, T)>,
    {
        // Use high retry count for contention scenarios
        // With N concurrent threads on single key, worst case needs N retries per op
        // 200 retries with fast backoff handles 100+ concurrent threads reliably
        let retry_config = RetryConfig::default()
            .with_max_retries(200)
            .with_base_delay_ms(1)
            .with_max_delay_ms(50);

        let key = self.key_for(run_id, name);
        let name_owned = name.to_string();

        // Perform read + closure + write in a SINGLE transaction
        // This ensures atomic OCC validation at commit time
        self.db
            .transaction_with_retry(*run_id, retry_config, |txn| {
                // Read current state within the transaction
                let current: State = match txn.get(&key)? {
                    Some(v) => from_stored_value(&v).map_err(|e| {
                        strata_core::error::Error::SerializationError(e.to_string())
                    })?,
                    None => {
                        return Err(strata_core::error::Error::InvalidOperation(format!(
                            "StateCell '{}' not found",
                            name_owned
                        )))
                    }
                };

                // Compute new value (closure may be called multiple times!)
                let (new_value, result) = f(&current)?;

                // Write new state with incremented version
                let new_version = current.version + 1;
                let new_state = State {
                    value: new_value,
                    version: new_version,
                    updated_at: State::now(),
                };

                txn.put(key.clone(), to_stored_value(&new_state))?;
                Ok((result, Versioned::new(new_version, Version::counter(new_version))))
            })
    }

    /// Apply transition or initialize if cell doesn't exist
    ///
    /// First attempts to initialize the cell with `initial` value,
    /// then applies the transition function.
    ///
    /// Returns `(user_result, Versioned<u64>)` tuple.
    ///
    /// # Story #468: StateCell Versioned Returns
    pub fn transition_or_init<F, T>(
        &self,
        run_id: &RunId,
        name: &str,
        initial: Value,
        f: F,
    ) -> Result<(T, Versioned<u64>)>
    where
        F: Fn(&State) -> Result<(Value, T)>,
    {
        // Try to init first (ignore AlreadyExists error)
        let _ = self.init(run_id, name, initial);

        // Then transition
        self.transition(run_id, name, f)
    }

    // ========== Search API (M6) ==========

    /// Search state cells
    ///
    /// Searches cell names and values. Respects budget constraints.
    ///
    /// # Example
    ///
    /// ```ignore
    /// use strata_core::SearchRequest;
    ///
    /// let response = state.search(&SearchRequest::new(run_id, "counter"))?;
    /// for hit in response.hits {
    ///     println!("Found state {:?} with score {}", hit.doc_ref, hit.score);
    /// }
    /// ```
    pub fn search(
        &self,
        req: &strata_core::SearchRequest,
    ) -> strata_core::error::Result<strata_core::SearchResponse> {
        use crate::searchable::{build_search_response, SearchCandidate};
        use strata_core::search_types::DocRef;
        use strata_core::traits::SnapshotView;
        use std::time::Instant;

        let start = Instant::now();
        let snapshot = self.db.storage().create_snapshot();
        let ns = self.namespace_for_run(&req.run_id);
        let scan_prefix = Key::new_state(ns, "");

        let mut candidates = Vec::new();
        let mut truncated = false;

        // Scan all state cells for this run
        for (key, versioned_value) in snapshot.scan_prefix(&scan_prefix)? {
            // Check budget constraints
            if start.elapsed().as_micros() as u64 >= req.budget.max_wall_time_micros {
                truncated = true;
                break;
            }
            if candidates.len() >= req.budget.max_candidates_per_primitive {
                truncated = true;
                break;
            }

            // Deserialize state
            let state: State = match from_stored_value(&versioned_value.value) {
                Ok(s) => s,
                Err(_) => continue,
            };

            // Time range filter
            if let Some((start_ts, end_ts)) = req.time_range {
                if state.updated_at < start_ts as i64 || state.updated_at > end_ts as i64 {
                    continue;
                }
            }

            // Extract searchable text: cell name + value
            let cell_name = key.user_key_string().unwrap_or_default();
            let text = self.extract_state_text(&cell_name, &state);

            candidates.push(SearchCandidate::new(
                DocRef::State { run_id: req.run_id, name: cell_name },
                text,
                Some(state.updated_at as u64),
            ));
        }

        Ok(build_search_response(
            candidates,
            &req.query,
            req.k,
            truncated,
            start.elapsed().as_micros() as u64,
        ))
    }

    /// Extract searchable text from a state cell
    fn extract_state_text(&self, name: &str, state: &State) -> String {
        let mut parts = vec![name.to_string()];
        if let Ok(s) = serde_json::to_string(&state.value) {
            parts.push(s);
        }
        parts.join(" ")
    }
}

// ========== Searchable Trait Implementation (M6) ==========

impl crate::searchable::Searchable for StateCell {
    fn search(
        &self,
        req: &strata_core::SearchRequest,
    ) -> strata_core::error::Result<strata_core::SearchResponse> {
        self.search(req)
    }

    fn primitive_kind(&self) -> strata_core::PrimitiveType {
        strata_core::PrimitiveType::State
    }
}

// ========== StateCellExt Implementation (Story #184) ==========

impl StateCellExt for TransactionContext {
    fn state_read(&mut self, name: &str) -> Result<Option<Value>> {
        let ns = Namespace::for_run(self.run_id);
        let key = Key::new_state(ns, name);

        match self.get(&key)? {
            Some(v) => {
                let state: State = from_stored_value(&v)
                    .map_err(|e| strata_core::error::Error::SerializationError(e.to_string()))?;
                Ok(Some(state.value))
            }
            None => Ok(None),
        }
    }

    fn state_cas(&mut self, name: &str, expected_version: u64, new_value: Value) -> Result<u64> {
        let ns = Namespace::for_run(self.run_id);
        let key = Key::new_state(ns, name);

        let current: State = match self.get(&key)? {
            Some(v) => from_stored_value(&v)
                .map_err(|e| strata_core::error::Error::SerializationError(e.to_string()))?,
            None => {
                return Err(strata_core::error::Error::InvalidOperation(format!(
                    "StateCell '{}' not found",
                    name
                )))
            }
        };

        if current.version != expected_version {
            return Err(strata_core::error::Error::VersionMismatch {
                expected: expected_version,
                actual: current.version,
            });
        }

        let new_state = State {
            value: new_value,
            version: current.version + 1,
            updated_at: State::now(),
        };

        self.put(key, to_stored_value(&new_state))?;
        Ok(new_state.version)
    }

    fn state_set(&mut self, name: &str, value: Value) -> Result<u64> {
        let ns = Namespace::for_run(self.run_id);
        let key = Key::new_state(ns, name);

        let new_version = match self.get(&key)? {
            Some(v) => {
                let current: State = from_stored_value(&v)
                    .map_err(|e| strata_core::error::Error::SerializationError(e.to_string()))?;
                current.version + 1
            }
            None => 1,
        };

        let new_state = State {
            value,
            version: new_version,
            updated_at: State::now(),
        };

        self.put(key, to_stored_value(&new_state))?;
        Ok(new_state.version)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn setup() -> (TempDir, Arc<Database>, StateCell) {
        let temp_dir = TempDir::new().unwrap();
        let db = Arc::new(Database::open(temp_dir.path()).unwrap());
        let sc = StateCell::new(db.clone());
        (temp_dir, db, sc)
    }

    // ========== Story #180: Core & State Structure Tests ==========

    #[test]
    fn test_state_creation() {
        let state = State::new(Value::Int(42));
        assert_eq!(state.version, 1);
        assert!(state.updated_at > 0);
        assert_eq!(state.value, Value::Int(42));
    }

    #[test]
    fn test_state_serialization() {
        let state = State::new(Value::String("test".into()));
        let json = serde_json::to_string(&state).unwrap();
        let restored: State = serde_json::from_str(&json).unwrap();
        assert_eq!(state.value, restored.value);
        assert_eq!(state.version, restored.version);
    }

    #[test]
    fn test_statecell_creation() {
        let (_temp, _db, sc) = setup();
        // Just verify we can get the database reference
        let _db_ref = sc.database();
    }

    #[test]
    fn test_statecell_is_clone() {
        let (_temp, _db, sc) = setup();
        let _sc2 = sc.clone();
    }

    #[test]
    fn test_statecell_is_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<StateCell>();
    }

    // ========== Story #181, #468: Read/Init/Delete Tests ==========

    #[test]
    fn test_init_and_read() {
        let (_temp, _db, sc) = setup();
        let run_id = RunId::new();

        let versioned = sc.init(&run_id, "counter", Value::Int(0)).unwrap();
        assert_eq!(versioned.value, 1);
        assert!(versioned.version.is_counter());

        let state = sc.read(&run_id, "counter").unwrap().unwrap();
        assert_eq!(state.value.value, Value::Int(0));
        assert_eq!(state.value.version, 1);
        assert!(state.version.is_counter());
    }

    #[test]
    fn test_init_already_exists() {
        let (_temp, _db, sc) = setup();
        let run_id = RunId::new();

        sc.init(&run_id, "cell", Value::Null).unwrap();
        let result = sc.init(&run_id, "cell", Value::Null);
        assert!(result.is_err());
    }

    #[test]
    fn test_read_nonexistent() {
        let (_temp, _db, sc) = setup();
        let run_id = RunId::new();

        let result = sc.read(&run_id, "nonexistent").unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_delete() {
        let (_temp, _db, sc) = setup();
        let run_id = RunId::new();

        sc.init(&run_id, "temp", Value::Null).unwrap();
        assert!(sc.exists(&run_id, "temp").unwrap());

        let deleted = sc.delete(&run_id, "temp").unwrap();
        assert!(deleted);
        assert!(!sc.exists(&run_id, "temp").unwrap());
    }

    #[test]
    fn test_delete_nonexistent() {
        let (_temp, _db, sc) = setup();
        let run_id = RunId::new();

        let deleted = sc.delete(&run_id, "nonexistent").unwrap();
        assert!(!deleted);
    }

    #[test]
    fn test_exists() {
        let (_temp, _db, sc) = setup();
        let run_id = RunId::new();

        assert!(!sc.exists(&run_id, "cell").unwrap());
        sc.init(&run_id, "cell", Value::Null).unwrap();
        assert!(sc.exists(&run_id, "cell").unwrap());
    }

    #[test]
    fn test_list() {
        let (_temp, _db, sc) = setup();
        let run_id = RunId::new();

        sc.init(&run_id, "alpha", Value::Null).unwrap();
        sc.init(&run_id, "beta", Value::Null).unwrap();
        sc.init(&run_id, "gamma", Value::Null).unwrap();

        let names = sc.list(&run_id).unwrap();
        assert_eq!(names.len(), 3);
        assert!(names.contains(&"alpha".to_string()));
        assert!(names.contains(&"beta".to_string()));
        assert!(names.contains(&"gamma".to_string()));
    }

    #[test]
    fn test_run_isolation() {
        let (_temp, _db, sc) = setup();
        let run1 = RunId::new();
        let run2 = RunId::new();

        sc.init(&run1, "shared", Value::Int(1)).unwrap();
        sc.init(&run2, "shared", Value::Int(2)).unwrap();

        let state1 = sc.read(&run1, "shared").unwrap().unwrap();
        let state2 = sc.read(&run2, "shared").unwrap().unwrap();

        assert_eq!(state1.value.value, Value::Int(1));
        assert_eq!(state2.value.value, Value::Int(2));
    }

    // ========== Story #182, #468: CAS & Set Tests ==========

    #[test]
    fn test_cas_success() {
        let (_temp, _db, sc) = setup();
        let run_id = RunId::new();

        sc.init(&run_id, "counter", Value::Int(0)).unwrap();

        // CAS with correct version
        let new_versioned = sc.cas(&run_id, "counter", 1, Value::Int(1)).unwrap();
        assert_eq!(new_versioned.value, 2);
        assert!(new_versioned.version.is_counter());

        let state = sc.read(&run_id, "counter").unwrap().unwrap();
        assert_eq!(state.value.value, Value::Int(1));
        assert_eq!(state.value.version, 2);
    }

    #[test]
    fn test_cas_conflict() {
        let (_temp, _db, sc) = setup();
        let run_id = RunId::new();

        sc.init(&run_id, "counter", Value::Int(0)).unwrap();

        // CAS with wrong version
        let result = sc.cas(&run_id, "counter", 999, Value::Int(1));
        assert!(matches!(
            result,
            Err(strata_core::error::Error::VersionMismatch { .. })
        ));
    }

    #[test]
    fn test_cas_not_found() {
        let (_temp, _db, sc) = setup();
        let run_id = RunId::new();

        let result = sc.cas(&run_id, "nonexistent", 1, Value::Int(1));
        assert!(result.is_err());
    }

    #[test]
    fn test_set_creates_if_not_exists() {
        let (_temp, _db, sc) = setup();
        let run_id = RunId::new();

        let versioned = sc.set(&run_id, "new-cell", Value::Int(42)).unwrap();
        assert_eq!(versioned.value, 1);

        let state = sc.read(&run_id, "new-cell").unwrap().unwrap();
        assert_eq!(state.value.value, Value::Int(42));
    }

    #[test]
    fn test_set_overwrites() {
        let (_temp, _db, sc) = setup();
        let run_id = RunId::new();

        sc.init(&run_id, "cell", Value::Int(1)).unwrap();
        let versioned = sc.set(&run_id, "cell", Value::Int(100)).unwrap();
        assert_eq!(versioned.value, 2);

        let state = sc.read(&run_id, "cell").unwrap().unwrap();
        assert_eq!(state.value.value, Value::Int(100));
    }

    #[test]
    fn test_version_monotonicity() {
        let (_temp, _db, sc) = setup();
        let run_id = RunId::new();

        sc.init(&run_id, "cell", Value::Int(0)).unwrap();

        for i in 1..=10 {
            let v = sc.set(&run_id, "cell", Value::Int(i)).unwrap();
            assert_eq!(v.value, (i + 1) as u64);
        }

        let state = sc.read(&run_id, "cell").unwrap().unwrap();
        assert_eq!(state.value.version, 11);
    }

    // ========== Story #183, #468: Transition Tests ==========

    #[test]
    fn test_transition_increment() {
        let (_temp, _db, sc) = setup();
        let run_id = RunId::new();

        sc.init(&run_id, "counter", Value::Int(0)).unwrap();

        let (result, new_versioned) = sc
            .transition(&run_id, "counter", |state| {
                let current = match &state.value {
                    Value::Int(n) => *n,
                    _ => 0,
                };
                Ok((Value::Int(current + 1), current + 1))
            })
            .unwrap();

        assert_eq!(result, 1);
        assert_eq!(new_versioned.value, 2);
        assert!(new_versioned.version.is_counter());

        let state = sc.read(&run_id, "counter").unwrap().unwrap();
        assert_eq!(state.value.value, Value::Int(1));
        assert_eq!(state.value.version, 2);
    }

    #[test]
    fn test_transition_not_found() {
        let (_temp, _db, sc) = setup();
        let run_id = RunId::new();

        let result = sc.transition(&run_id, "nonexistent", |_state| Ok((Value::Null, ())));
        assert!(result.is_err());
    }

    #[test]
    fn test_transition_or_init_creates() {
        let (_temp, _db, sc) = setup();
        let run_id = RunId::new();

        // Cell doesn't exist, should init then transition
        let (result, _versioned) = sc
            .transition_or_init(&run_id, "new-counter", Value::Int(0), |state| {
                let current = match &state.value {
                    Value::Int(n) => *n,
                    _ => 0,
                };
                Ok((Value::Int(current + 10), current + 10))
            })
            .unwrap();

        assert_eq!(result, 10);

        let state = sc.read(&run_id, "new-counter").unwrap().unwrap();
        assert_eq!(state.value.value, Value::Int(10));
    }

    #[test]
    fn test_transition_or_init_existing() {
        let (_temp, _db, sc) = setup();
        let run_id = RunId::new();

        // Init first
        sc.init(&run_id, "counter", Value::Int(5)).unwrap();

        // transition_or_init should use existing value
        let (result, _versioned) = sc
            .transition_or_init(&run_id, "counter", Value::Int(0), |state| {
                let current = match &state.value {
                    Value::Int(n) => *n,
                    _ => 0,
                };
                Ok((Value::Int(current + 1), current + 1))
            })
            .unwrap();

        assert_eq!(result, 6);
    }

    #[test]
    fn test_multiple_transitions() {
        let (_temp, _db, sc) = setup();
        let run_id = RunId::new();

        sc.init(&run_id, "counter", Value::Int(0)).unwrap();

        for expected in 1..=5 {
            let (result, _versioned) = sc
                .transition(&run_id, "counter", |state| {
                    let current = match &state.value {
                        Value::Int(n) => *n,
                        _ => 0,
                    };
                    Ok((Value::Int(current + 1), current + 1))
                })
                .unwrap();
            assert_eq!(result, expected);
        }

        let state = sc.read(&run_id, "counter").unwrap().unwrap();
        assert_eq!(state.value.value, Value::Int(5));
        assert_eq!(state.value.version, 6);
    }

    // ========== Story #184: StateCellExt Tests ==========

    #[test]
    fn test_statecell_ext_read() {
        let (_temp, db, sc) = setup();
        let run_id = RunId::new();

        sc.init(&run_id, "cell", Value::String("hello".into()))
            .unwrap();

        let result = db
            .transaction(run_id, |txn| {
                let value = txn.state_read("cell")?;
                Ok(value)
            })
            .unwrap();

        assert_eq!(result, Some(Value::String("hello".into())));
    }

    #[test]
    fn test_statecell_ext_read_not_found() {
        let (_temp, db, _sc) = setup();
        let run_id = RunId::new();

        let result = db
            .transaction(run_id, |txn| {
                let value = txn.state_read("nonexistent")?;
                Ok(value)
            })
            .unwrap();

        assert!(result.is_none());
    }

    #[test]
    fn test_statecell_ext_cas() {
        let (_temp, db, sc) = setup();
        let run_id = RunId::new();

        sc.init(&run_id, "cell", Value::Int(1)).unwrap();

        let new_version = db
            .transaction(run_id, |txn| txn.state_cas("cell", 1, Value::Int(2)))
            .unwrap();

        assert_eq!(new_version, 2);

        let state = sc.read(&run_id, "cell").unwrap().unwrap();
        assert_eq!(state.value.value, Value::Int(2));
    }

    #[test]
    fn test_statecell_ext_set() {
        let (_temp, db, sc) = setup();
        let run_id = RunId::new();

        let version = db
            .transaction(run_id, |txn| txn.state_set("new-cell", Value::Int(42)))
            .unwrap();

        assert_eq!(version, 1);

        let state = sc.read(&run_id, "new-cell").unwrap().unwrap();
        assert_eq!(state.value.value, Value::Int(42));
    }

    #[test]
    fn test_cross_primitive_transaction() {
        use crate::extensions::KVStoreExt;

        let (_temp, db, sc) = setup();
        let run_id = RunId::new();

        sc.init(&run_id, "counter", Value::Int(0)).unwrap();

        // Combine KV and StateCell in single transaction
        db.transaction(run_id, |txn| {
            txn.kv_put("key", Value::String("value".into()))?;
            txn.state_set("counter", Value::Int(1))?;
            Ok(())
        })
        .unwrap();

        // Verify both were written
        let state = sc.read(&run_id, "counter").unwrap().unwrap();
        assert_eq!(state.value.value, Value::Int(1));
    }

    // ========== Fast Path Tests (Story #238, #468) ==========

    #[test]
    fn test_fast_read_returns_correct_value() {
        let (_temp, _db, sc) = setup();
        let run_id = RunId::new();

        sc.init(&run_id, "cell", Value::Int(42)).unwrap();

        let state = sc.read(&run_id, "cell").unwrap().unwrap();
        assert_eq!(state.value.value, Value::Int(42));
        assert_eq!(state.value.version, 1);
        assert!(state.version.is_counter());
    }

    #[test]
    fn test_fast_read_returns_none_for_missing() {
        let (_temp, _db, sc) = setup();
        let run_id = RunId::new();

        let state = sc.read(&run_id, "nonexistent").unwrap();
        assert!(state.is_none());
    }

    #[test]
    fn test_fast_read_equals_transaction_read() {
        let (_temp, _db, sc) = setup();
        let run_id = RunId::new();

        sc.init(&run_id, "cell", Value::String("test".into()))
            .unwrap();

        let fast = sc.read(&run_id, "cell").unwrap();
        let txn = sc.read_in_transaction(&run_id, "cell").unwrap();

        // Compare the inner values (timestamp may differ slightly)
        assert_eq!(fast.as_ref().map(|v| &v.value), txn.as_ref().map(|v| &v.value));
        assert_eq!(fast.as_ref().map(|v| v.version.clone()), txn.as_ref().map(|v| v.version.clone()));
    }

    #[test]
    fn test_fast_exists_uses_fast_path() {
        let (_temp, _db, sc) = setup();
        let run_id = RunId::new();

        assert!(!sc.exists(&run_id, "cell").unwrap());

        sc.init(&run_id, "cell", Value::Null).unwrap();

        assert!(sc.exists(&run_id, "cell").unwrap());
    }

    #[test]
    fn test_fast_read_run_isolation() {
        let (_temp, _db, sc) = setup();
        let run1 = RunId::new();
        let run2 = RunId::new();

        sc.init(&run1, "shared", Value::Int(1)).unwrap();
        sc.init(&run2, "shared", Value::Int(2)).unwrap();

        let state1 = sc.read(&run1, "shared").unwrap().unwrap();
        let state2 = sc.read(&run2, "shared").unwrap().unwrap();

        assert_eq!(state1.value.value, Value::Int(1));
        assert_eq!(state2.value.value, Value::Int(2));
    }

    // ========== Story #468: Versioned Returns Tests ==========

    #[test]
    fn test_versioned_init_has_counter_version() {
        let (_temp, _db, sc) = setup();
        let run_id = RunId::new();

        let versioned = sc.init(&run_id, "cell", Value::Int(0)).unwrap();
        assert_eq!(versioned.value, 1);
        assert!(versioned.version.is_counter());
        assert_eq!(versioned.version, Version::counter(1));
    }

    #[test]
    fn test_versioned_read_has_counter_version() {
        let (_temp, _db, sc) = setup();
        let run_id = RunId::new();

        sc.init(&run_id, "cell", Value::Int(42)).unwrap();
        let versioned = sc.read(&run_id, "cell").unwrap().unwrap();

        assert!(versioned.version.is_counter());
        assert_eq!(versioned.version, Version::counter(1));
        assert!(versioned.timestamp.as_micros() > 0);
    }

    #[test]
    fn test_versioned_cas_has_counter_version() {
        let (_temp, _db, sc) = setup();
        let run_id = RunId::new();

        sc.init(&run_id, "cell", Value::Int(0)).unwrap();
        let versioned = sc.cas(&run_id, "cell", 1, Value::Int(1)).unwrap();

        assert!(versioned.version.is_counter());
        assert_eq!(versioned.version, Version::counter(2));
    }
}
