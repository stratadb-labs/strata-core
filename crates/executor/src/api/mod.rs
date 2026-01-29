//! High-level typed wrapper for the Executor.
//!
//! The [`Strata`] struct provides a convenient Rust API that wraps the
//! [`Executor`] and [`Command`]/[`Output`] enums with typed method calls.
//!
//! ## Run Context
//!
//! Strata maintains a "current run" context, similar to how git maintains
//! a current branch. All data operations operate on the current run.
//!
//! - Use `create_run(name)` to create a new blank run
//! - Use `set_run(name)` to switch to an existing run
//! - Use `current_run()` to get the current run name
//! - Use `list_runs()` to see all available runs
//! - Use `fork_run(dest)` to copy the current run to a new run (future)
//!
//! By default, Strata starts on the "default" run.
//!
//! # Example
//!
//! ```ignore
//! use strata_executor::{Strata, Value};
//!
//! let mut db = Strata::open("/path/to/data")?;
//!
//! // Work on the default run
//! db.kv_put("key", Value::String("hello".into()))?;
//!
//! // Create and switch to a different run
//! db.create_run("experiment-1")?;
//! db.set_run("experiment-1")?;
//! db.kv_put("key", Value::String("different".into()))?;
//!
//! // Switch back to default
//! db.set_run("default")?;
//! assert_eq!(db.kv_get("key")?, Some(Value::String("hello".into())));
//! ```

mod db;
mod event;
mod json;
mod kv;
mod run;
mod runs;
mod state;
mod vector;

pub use runs::{Runs, RunDiff};

use std::path::Path;
use std::sync::Arc;

use strata_engine::Database;

use crate::types::RunId;
use crate::{Command, Error, Executor, Output, Result, Session};

/// High-level typed wrapper for database operations.
///
/// `Strata` provides a convenient Rust API that wraps the executor's
/// command-based interface with typed method calls. It maintains a
/// "current run" context that all data operations use.
///
/// ## Run Context (git-like mental model)
///
/// - **Database** = repository (the whole storage)
/// - **Strata** = working directory (stateful view into the repo)
/// - **Run** = branch (isolated namespace for data)
///
/// Use `create_run()` to create new runs and `set_run()` to switch between them.
pub struct Strata {
    executor: Executor,
    current_run: RunId,
}

impl Strata {
    /// Open a database at the given path.
    ///
    /// This is the primary way to create a Strata instance. The database
    /// will be created if it doesn't exist.
    ///
    /// # Example
    ///
    /// ```ignore
    /// use strata_executor::{Strata, Value};
    ///
    /// let mut db = Strata::open("/var/data/myapp")?;
    /// db.kv_put("key", Value::String("hello".into()))?;
    /// ```
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self> {
        let db = Database::open(path).map_err(|e| Error::Internal {
            reason: format!("Failed to open database: {}", e),
        })?;
        let executor = Executor::new(db);

        // Ensure the default run exists
        Self::ensure_default_run(&executor)?;

        Ok(Self {
            executor,
            current_run: RunId::default(),
        })
    }

    /// Open a temporary in-memory database.
    ///
    /// Useful for testing. Data is not persisted.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let mut db = Strata::open_temp()?;
    /// db.kv_put("key", Value::Int(42))?;
    /// ```
    pub fn open_temp() -> Result<Self> {
        let db = Database::builder()
            .no_durability()
            .open_temp()
            .map_err(|e| Error::Internal {
                reason: format!("Failed to open temp database: {}", e),
            })?;
        let executor = Executor::new(db);

        // Ensure the default run exists
        Self::ensure_default_run(&executor)?;

        Ok(Self {
            executor,
            current_run: RunId::default(),
        })
    }

    /// Create a new Strata instance from an existing database.
    ///
    /// Use this when you need more control over database configuration.
    /// For most cases, prefer [`Strata::open()`].
    pub fn from_database(db: Arc<Database>) -> Result<Self> {
        let executor = Executor::new(db);

        // Ensure the default run exists
        Self::ensure_default_run(&executor)?;

        Ok(Self {
            executor,
            current_run: RunId::default(),
        })
    }

    /// Ensures the "default" run exists in the database.
    fn ensure_default_run(executor: &Executor) -> Result<()> {
        // Check if default run exists
        match executor.execute(Command::RunExists {
            run: RunId::default(),
        })? {
            Output::Bool(exists) => {
                if !exists {
                    // Create the default run
                    executor.execute(Command::RunCreate {
                        run_id: Some("default".to_string()),
                        metadata: None,
                    })?;
                }
                Ok(())
            }
            _ => Err(Error::Internal {
                reason: "Unexpected output for RunExists".into(),
            }),
        }
    }

    /// Get the underlying executor.
    pub fn executor(&self) -> &Executor {
        &self.executor
    }

    /// Get a handle for run management operations.
    ///
    /// The returned [`Runs`] handle provides the "power API" for run
    /// management, including listing, creating, deleting, and (future)
    /// forking runs.
    ///
    /// # Example
    ///
    /// ```ignore
    /// // List all runs
    /// for run in db.runs().list()? {
    ///     println!("Run: {}", run);
    /// }
    ///
    /// // Create a new run
    /// db.runs().create("experiment")?;
    ///
    /// // Future: fork the current run to a new destination
    /// // db.runs().fork("experiment-copy")?;
    /// ```
    pub fn runs(&self) -> Runs<'_> {
        Runs::new(&self.executor)
    }

    /// Create a new [`Session`] for interactive transaction support.
    ///
    /// The returned session wraps a fresh executor and can manage an
    /// optional open transaction across multiple `execute()` calls.
    pub fn session(db: Arc<Database>) -> Session {
        Session::new(db)
    }

    // =========================================================================
    // Run Context
    // =========================================================================

    /// Get the current run name.
    ///
    /// Returns the name of the run that all data operations will use.
    pub fn current_run(&self) -> &str {
        self.current_run.as_str()
    }

    /// Switch to an existing run.
    ///
    /// All subsequent data operations will use this run.
    ///
    /// # Errors
    ///
    /// Returns an error if the run doesn't exist. Use `create_run()` first
    /// to create a new run.
    ///
    /// # Example
    ///
    /// ```ignore
    /// // Switch to an existing run
    /// db.set_run("my-experiment")?;
    /// db.kv_put("key", "value")?;  // Data goes to my-experiment
    ///
    /// // Switch back to default
    /// db.set_run("default")?;
    /// ```
    pub fn set_run(&mut self, run_name: &str) -> Result<()> {
        // Check if run exists
        if !self.runs().exists(run_name)? {
            return Err(Error::RunNotFound {
                run: run_name.to_string(),
            });
        }

        self.current_run = RunId::from(run_name);
        Ok(())
    }

    /// Create a new blank run.
    ///
    /// The new run starts with no data. Stays on the current run after creation.
    /// Use `set_run()` to switch to the new run.
    ///
    /// # Errors
    ///
    /// Returns an error if the run already exists.
    ///
    /// # Example
    ///
    /// ```ignore
    /// // Create a new run
    /// db.create_run("experiment")?;
    ///
    /// // Optionally switch to it
    /// db.set_run("experiment")?;
    /// ```
    pub fn create_run(&self, run_name: &str) -> Result<()> {
        self.runs().create(run_name)
    }

    /// Fork the current run with all its data into a new run.
    ///
    /// **NOT YET IMPLEMENTED** - Returns `NotImplemented` error.
    ///
    /// When implemented, this will copy all data (KV, State, Events, JSON,
    /// Vectors) from the current run to the new run. Stays on the current
    /// run after forking. Use `set_run()` to switch to the fork.
    ///
    /// # Example (future)
    ///
    /// ```ignore
    /// // Fork current run to "experiment"
    /// db.fork_run("experiment")?;
    ///
    /// // Switch to the fork
    /// db.set_run("experiment")?;
    /// // ... make changes without affecting original ...
    /// ```
    pub fn fork_run(&self, destination: &str) -> Result<()> {
        self.runs().fork(destination)
    }

    /// List all available runs.
    ///
    /// Returns a list of run names.
    pub fn list_runs(&self) -> Result<Vec<String>> {
        self.runs().list()
    }

    /// Delete a run and all its data.
    ///
    /// **WARNING**: This is irreversible! All data in the run will be deleted.
    ///
    /// # Errors
    ///
    /// - Returns an error if trying to delete the current run
    /// - Returns an error if trying to delete the "default" run
    pub fn delete_run(&self, run_name: &str) -> Result<()> {
        // Cannot delete current run
        if run_name == self.current_run.as_str() {
            return Err(Error::ConstraintViolation {
                reason: "Cannot delete the current run. Switch to a different run first.".into(),
            });
        }

        self.runs().delete(run_name)
    }

    /// Get the RunId for use in commands.
    ///
    /// This is used internally by the data operation methods.
    pub(crate) fn run_id(&self) -> Option<RunId> {
        Some(self.current_run.clone())
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Value;
    use crate::types::*;

    fn create_strata() -> Strata {
        Strata::open_temp().unwrap()
    }

    #[test]
    fn test_ping() {
        let db = create_strata();
        let version = db.ping().unwrap();
        assert!(!version.is_empty());
    }

    #[test]
    fn test_info() {
        let db = create_strata();
        let info = db.info().unwrap();
        assert!(!info.version.is_empty());
    }

    #[test]
    fn test_kv_put_get() {
        let db = create_strata();

        // Simplified API: just pass &str directly
        let version = db.kv_put("key1", "hello").unwrap();
        assert!(version > 0);

        let value = db.kv_get("key1").unwrap();
        assert!(value.is_some());
        assert_eq!(value.unwrap(), Value::String("hello".into()));
    }

    #[test]
    fn test_kv_delete() {
        let db = create_strata();

        // Simplified API: just pass i64 directly
        db.kv_put("key1", 42i64).unwrap();
        assert!(db.kv_get("key1").unwrap().is_some());

        let existed = db.kv_delete("key1").unwrap();
        assert!(existed);
        assert!(db.kv_get("key1").unwrap().is_none());
    }

    #[test]
    fn test_kv_list() {
        let db = create_strata();

        db.kv_put("user:1", 1i64).unwrap();
        db.kv_put("user:2", 2i64).unwrap();
        db.kv_put("task:1", 3i64).unwrap();

        let user_keys = db.kv_list(Some("user:")).unwrap();
        assert_eq!(user_keys.len(), 2);

        let all_keys = db.kv_list(None).unwrap();
        assert_eq!(all_keys.len(), 3);
    }

    #[test]
    fn test_state_set_get() {
        let db = create_strata();

        // Simplified API: just pass &str directly
        db.state_set("cell", "state").unwrap();
        let value = db.state_read("cell").unwrap();
        assert!(value.is_some());
        assert_eq!(value.unwrap().value, Value::String("state".into()));
    }

    #[test]
    fn test_event_append_range() {
        let db = create_strata();

        // Event payloads must be Objects
        db.event_append("stream", Value::Object(
            [("value".to_string(), Value::Int(1))].into_iter().collect()
        )).unwrap();
        db.event_append("stream", Value::Object(
            [("value".to_string(), Value::Int(2))].into_iter().collect()
        )).unwrap();

        let events = db.event_range("stream", None, None, None).unwrap();
        assert_eq!(events.len(), 2);
    }

    #[test]
    fn test_vector_operations() {
        let db = create_strata();

        db.vector_create_collection("vecs", 4u64, DistanceMetric::Cosine).unwrap();
        db.vector_upsert("vecs", "v1", vec![1.0, 0.0, 0.0, 0.0], None).unwrap();
        db.vector_upsert("vecs", "v2", vec![0.0, 1.0, 0.0, 0.0], None).unwrap();

        let matches = db.vector_search("vecs", vec![1.0, 0.0, 0.0, 0.0], 10u64).unwrap();
        assert_eq!(matches.len(), 2);
        assert_eq!(matches[0].key, "v1");
    }

    #[test]
    fn test_run_create_list() {
        let db = create_strata();

        let (info, _version) = db.run_create(
            Some("550e8400-e29b-41d4-a716-446655440099".to_string()),
            None,
        ).unwrap();
        assert_eq!(info.id.as_str(), "550e8400-e29b-41d4-a716-446655440099");

        let runs = db.run_list(None, None, None).unwrap();
        assert!(!runs.is_empty());
    }

    // =========================================================================
    // Run Context Tests
    // =========================================================================

    #[test]
    fn test_current_run_default() {
        let db = create_strata();
        assert_eq!(db.current_run(), "default");
    }

    #[test]
    fn test_create_run() {
        let db = create_strata();

        // Create a new run (stays on current run)
        db.create_run("experiment-1").unwrap();

        // Still on default run
        assert_eq!(db.current_run(), "default");

        // But the run exists
        assert!(db.run_exists("experiment-1").unwrap());
    }

    #[test]
    fn test_set_run_to_existing() {
        let mut db = create_strata();

        // Create a run first
        db.create_run("my-run").unwrap();

        // Switch to it
        db.set_run("my-run").unwrap();
        assert_eq!(db.current_run(), "my-run");
    }

    #[test]
    fn test_set_run_nonexistent_fails() {
        let mut db = create_strata();

        // Try to switch to a run that doesn't exist
        let result = db.set_run("nonexistent");
        assert!(result.is_err());
    }

    #[test]
    fn test_list_runs() {
        let db = create_strata();

        // Create a few runs
        db.create_run("run-a").unwrap();
        db.create_run("run-b").unwrap();
        db.create_run("run-c").unwrap();

        let runs = db.list_runs().unwrap();
        assert!(runs.contains(&"run-a".to_string()));
        assert!(runs.contains(&"run-b".to_string()));
        assert!(runs.contains(&"run-c".to_string()));
    }

    #[test]
    fn test_delete_run() {
        let db = create_strata();

        // Create a run
        db.create_run("to-delete").unwrap();

        // Delete the run
        db.delete_run("to-delete").unwrap();

        // Verify it's gone
        assert!(!db.run_exists("to-delete").unwrap());
    }

    #[test]
    fn test_delete_current_run_fails() {
        let mut db = create_strata();

        db.create_run("current-run").unwrap();
        db.set_run("current-run").unwrap();

        // Trying to delete the current run should fail
        let result = db.delete_run("current-run");
        assert!(result.is_err());
    }

    #[test]
    fn test_delete_default_run_fails() {
        let db = create_strata();

        // Trying to delete the default run should fail
        let result = db.delete_run("default");
        assert!(result.is_err());
    }

    #[test]
    fn test_run_context_data_isolation() {
        let mut db = create_strata();

        // Put data in default run (simplified API)
        db.kv_put("key", "default-value").unwrap();

        // Create and switch to another run
        db.create_run("experiment").unwrap();
        db.set_run("experiment").unwrap();

        // The key should not exist in this run
        assert!(db.kv_get("key").unwrap().is_none());

        // Put different data
        db.kv_put("key", "experiment-value").unwrap();

        // Switch back to default
        db.set_run("default").unwrap();

        // Original value should still be there
        let value = db.kv_get("key").unwrap();
        assert_eq!(value, Some(Value::String("default-value".into())));
    }

    #[test]
    fn test_run_context_isolation_all_primitives() {
        let mut db = create_strata();

        // Put data in default run (simplified API)
        db.kv_put("kv-key", 1i64).unwrap();
        db.state_set("state-cell", 10i64).unwrap();
        db.event_append("stream", Value::Object(
            [("x".to_string(), Value::Int(100))].into_iter().collect()
        )).unwrap();

        // Create and switch to another run
        db.create_run("isolated").unwrap();
        db.set_run("isolated").unwrap();

        // None of the data should exist in this run
        assert!(db.kv_get("kv-key").unwrap().is_none());
        assert!(db.state_read("state-cell").unwrap().is_none());
        assert_eq!(db.event_len("stream").unwrap(), 0);
    }

    // =========================================================================
    // db.runs() Power API Tests
    // =========================================================================

    #[test]
    fn test_runs_list() {
        let db = create_strata();

        // Create some runs
        db.runs().create("run-a").unwrap();
        db.runs().create("run-b").unwrap();

        let runs = db.runs().list().unwrap();
        assert!(runs.contains(&"run-a".to_string()));
        assert!(runs.contains(&"run-b".to_string()));
    }

    #[test]
    fn test_runs_exists() {
        let db = create_strata();

        assert!(!db.runs().exists("nonexistent").unwrap());

        db.runs().create("my-run").unwrap();
        assert!(db.runs().exists("my-run").unwrap());
    }

    #[test]
    fn test_runs_create() {
        let db = create_strata();

        db.runs().create("new-run").unwrap();
        assert!(db.runs().exists("new-run").unwrap());
    }

    #[test]
    fn test_runs_create_duplicate_fails() {
        let db = create_strata();

        db.runs().create("my-run").unwrap();
        let result = db.runs().create("my-run");
        assert!(result.is_err());
    }

    #[test]
    fn test_runs_delete() {
        let db = create_strata();

        db.runs().create("to-delete").unwrap();
        assert!(db.runs().exists("to-delete").unwrap());

        db.runs().delete("to-delete").unwrap();
        assert!(!db.runs().exists("to-delete").unwrap());
    }

    #[test]
    fn test_runs_delete_default_fails() {
        let db = create_strata();

        let result = db.runs().delete("default");
        assert!(result.is_err());
    }

    #[test]
    fn test_runs_fork_not_implemented() {
        let db = create_strata();

        // fork() forks the current run to a new destination
        let result = db.runs().fork("destination");
        assert!(result.is_err());

        // Check it's specifically a NotImplemented error
        match result {
            Err(crate::Error::NotImplemented { feature, .. }) => {
                assert_eq!(feature, "fork_run");
            }
            _ => panic!("Expected NotImplemented error"),
        }
    }

    #[test]
    fn test_runs_diff_not_implemented() {
        let db = create_strata();

        let result = db.runs().diff("run1", "run2");
        assert!(result.is_err());

        match result {
            Err(crate::Error::NotImplemented { feature, .. }) => {
                assert_eq!(feature, "diff_runs");
            }
            _ => panic!("Expected NotImplemented error"),
        }
    }
}
