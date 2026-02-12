//! High-level typed wrapper for the Executor.
//!
//! The [`Strata`] struct provides a convenient Rust API that wraps the
//! [`Executor`] and [`Command`]/[`Output`] enums with typed method calls.
//!
//! ## Branch Context
//!
//! Strata maintains a "current branch" context, similar to how git maintains
//! a current branch. All data operations operate on the current branch.
//!
//! - Use `create_branch(name)` to create a new blank branch
//! - Use `set_branch(name)` to switch to an existing branch
//! - Use `current_branch()` to get the current branch name
//! - Use `list_branches()` to see all available branches
//! - Use `fork_branch(dest)` to copy the current branch to a new branch
//!
//! By default, Strata starts on the "default" branch.
//!
//! # Example
//!
//! ```text
//! use strata_executor::{Strata, Value};
//!
//! let mut db = Strata::open("/path/to/data")?;
//!
//! // Work on the default branch
//! db.kv_put("key", Value::String("hello".into()))?;
//!
//! // Create and switch to a different branch
//! db.create_branch("experiment-1")?;
//! db.set_branch("experiment-1")?;
//! db.kv_put("key", Value::String("different".into()))?;
//!
//! // Switch back to default
//! db.set_branch("default")?;
//! assert_eq!(db.kv_get("key")?, Some(Value::String("hello".into())));
//! ```

mod branch;
mod branches;
mod db;
mod event;
mod json;
mod kv;
mod state;
mod vector;

pub use branches::Branches;
pub use strata_engine::branch_ops::{
    BranchDiffEntry, BranchDiffResult, ConflictEntry, DiffSummary, ForkInfo, MergeInfo,
    MergeStrategy, SpaceDiff,
};

use std::path::Path;
use std::sync::Arc;

use strata_engine::{Database, ModelConfig};
use strata_security::{AccessMode, OpenOptions};

use std::sync::Once;

use crate::types::BranchId;
use crate::{Command, Error, Executor, Output, Result, Session};

/// Ensure vector recovery is registered before opening any database.
static VECTOR_RECOVERY_INIT: Once = Once::new();

fn ensure_vector_recovery() {
    VECTOR_RECOVERY_INIT.call_once(|| {
        strata_engine::register_vector_recovery();
    });
}

/// High-level typed wrapper for database operations.
///
/// `Strata` provides a convenient Rust API that wraps the executor's
/// command-based interface with typed method calls. It maintains a
/// "current branch" context that all data operations use.
///
/// ## Branch Context (git-like mental model)
///
/// - **Database** = repository (the whole storage)
/// - **Strata** = working directory (stateful view into the repo)
/// - **Branch** = branch (isolated namespace for data)
///
/// Use `create_branch()` to create new branches and `set_branch()` to switch between them.
pub struct Strata {
    executor: Executor,
    current_branch: BranchId,
    current_space: String,
    access_mode: AccessMode,
}

impl Strata {
    /// Open a database at the given path.
    ///
    /// This is the primary way to create a Strata instance. The database
    /// will be created if it doesn't exist. Opens in read-write mode.
    ///
    /// # Example
    ///
    /// ```text
    /// use strata_executor::{Strata, Value};
    ///
    /// let mut db = Strata::open("/var/data/myapp")?;
    /// db.kv_put("key", Value::String("hello".into()))?;
    /// ```
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self> {
        Self::open_with(path, OpenOptions::default())
    }

    /// Open a database at the given path with explicit options.
    ///
    /// Use this to open a database in read-only mode or with other
    /// configuration options.
    ///
    /// # Example
    ///
    /// ```text
    /// use strata_executor::{Strata, OpenOptions, AccessMode};
    ///
    /// let db = Strata::open_with("/var/data/myapp", OpenOptions::new().access_mode(AccessMode::ReadOnly))?;
    /// ```
    pub fn open_with<P: AsRef<Path>>(path: P, opts: OpenOptions) -> Result<Self> {
        ensure_vector_recovery();

        let data_dir = path.as_ref().to_path_buf();
        std::fs::create_dir_all(&data_dir).map_err(|e| Error::Internal {
            reason: format!("Failed to create data directory: {}", e),
        })?;

        // Read existing config (or defaults)
        let config_path =
            data_dir.join(strata_engine::database::config::CONFIG_FILE_NAME);
        strata_engine::database::config::StrataConfig::write_default_if_missing(&config_path)
            .map_err(|e| Error::Internal {
                reason: format!("Failed to write default config: {}", e),
            })?;
        let mut cfg = strata_engine::database::config::StrataConfig::from_file(&config_path)
            .map_err(|e| Error::Internal {
                reason: format!("Failed to read config: {}", e),
            })?;

        // Merge OpenOptions overrides into config
        if let Some(ref dur) = opts.durability {
            cfg.durability = dur.clone();
        }
        if let Some(enabled) = opts.auto_embed {
            cfg.auto_embed = enabled;
        }
        if let Some(ref endpoint) = opts.model_endpoint {
            let model = cfg.model.get_or_insert_with(|| ModelConfig {
                endpoint: String::new(),
                model: String::new(),
                api_key: None,
                timeout_ms: 5000,
            });
            model.endpoint = endpoint.clone();
        }
        if let Some(ref name) = opts.model_name {
            let model = cfg.model.get_or_insert_with(|| ModelConfig {
                endpoint: String::new(),
                model: String::new(),
                api_key: None,
                timeout_ms: 5000,
            });
            model.model = name.clone();
        }
        if let Some(ref key) = opts.model_api_key {
            if let Some(ref mut model) = cfg.model {
                model.api_key = Some(key.clone());
            }
        }
        if let Some(ms) = opts.model_timeout_ms {
            if let Some(ref mut model) = cfg.model {
                model.timeout_ms = ms;
            }
        }

        let db =
            Database::open_with_config(&data_dir, cfg).map_err(|e| Error::Internal {
                reason: format!("Failed to open database: {}", e),
            })?;

        let access_mode = opts.access_mode;
        let executor = Executor::new_with_mode(db, access_mode);

        match access_mode {
            AccessMode::ReadWrite => Self::ensure_default_branch(&executor)?,
            AccessMode::ReadOnly => Self::verify_default_branch(&executor)?,
        }

        Ok(Self {
            executor,
            current_branch: BranchId::default(),
            current_space: "default".to_string(),
            access_mode,
        })
    }

    /// Create an ephemeral in-memory database.
    ///
    /// Useful for testing. Data is not persisted and no disk files are created.
    ///
    /// # Example
    ///
    /// ```text
    /// let mut db = Strata::cache()?;
    /// db.kv_put("key", Value::Int(42))?;
    /// ```
    pub fn cache() -> Result<Self> {
        ensure_vector_recovery();
        let db = Database::cache().map_err(|e| Error::Internal {
            reason: format!("Failed to open cache database: {}", e),
        })?;
        let executor = Executor::new(db);

        // Ensure the default branch exists
        Self::ensure_default_branch(&executor)?;

        Ok(Self {
            executor,
            current_branch: BranchId::default(),
            current_space: "default".to_string(),
            access_mode: AccessMode::ReadWrite,
        })
    }

    /// Create a new independent handle to the same database.
    ///
    /// Each handle has its own branch context (starting on "default") and can
    /// be moved to a separate thread. This is the standard way to use Strata
    /// from multiple threads.
    ///
    /// # Example
    ///
    /// ```text
    /// let db = Strata::open("/data/myapp")?;
    /// let handle = db.new_handle()?;
    /// std::thread::spawn(move || {
    ///     handle.kv_put("key", Value::Int(1)).unwrap();
    /// });
    /// ```
    pub fn new_handle(&self) -> Result<Self> {
        let db = self.executor.primitives().db.clone();
        Self::from_database_with_mode(db, self.access_mode)
    }

    /// Create a new Strata instance from an existing database.
    ///
    /// Use this when you need more control over database configuration.
    /// For most cases, prefer [`Strata::open()`].
    pub fn from_database(db: Arc<Database>) -> Result<Self> {
        Self::from_database_with_mode(db, AccessMode::ReadWrite)
    }

    /// Create a new Strata instance from an existing database with a
    /// specific access mode.
    fn from_database_with_mode(db: Arc<Database>, access_mode: AccessMode) -> Result<Self> {
        ensure_vector_recovery();
        let executor = Executor::new_with_mode(db, access_mode);

        match access_mode {
            AccessMode::ReadWrite => Self::ensure_default_branch(&executor)?,
            AccessMode::ReadOnly => Self::verify_default_branch(&executor)?,
        }

        Ok(Self {
            executor,
            current_branch: BranchId::default(),
            current_space: "default".to_string(),
            access_mode,
        })
    }

    /// Ensures the "default" branch exists in the database, creating it if
    /// missing.
    fn ensure_default_branch(executor: &Executor) -> Result<()> {
        // Check if default branch exists
        match executor.execute(Command::BranchExists {
            branch: BranchId::default(),
        })? {
            Output::Bool(exists) => {
                if !exists {
                    // Create the default branch
                    executor.execute(Command::BranchCreate {
                        branch_id: Some("default".to_string()),
                        metadata: None,
                    })?;
                }
                Ok(())
            }
            _ => Err(Error::Internal {
                reason: "Unexpected output for BranchExists".into(),
            }),
        }
    }

    /// Verifies the "default" branch exists without attempting to create it.
    ///
    /// Used by read-only open to avoid issuing writes.
    fn verify_default_branch(executor: &Executor) -> Result<()> {
        // BranchExists is a read command, so the read-only guard won't fire.
        match executor.execute(Command::BranchExists {
            branch: BranchId::default(),
        })? {
            Output::Bool(true) => Ok(()),
            Output::Bool(false) => Err(Error::BranchNotFound {
                branch: "default".to_string(),
            }),
            _ => Err(Error::Internal {
                reason: "Unexpected output for BranchExists".into(),
            }),
        }
    }

    /// Get the underlying executor.
    pub fn executor(&self) -> &Executor {
        &self.executor
    }

    /// Returns the access mode of this database handle.
    pub fn access_mode(&self) -> AccessMode {
        self.access_mode
    }

    /// Get WAL durability counters for diagnostics.
    ///
    /// Returns `None` for cache (in-memory) databases.
    pub fn durability_counters(&self) -> Option<strata_engine::WalCounters> {
        self.executor.primitives().db.durability_counters()
    }

    /// Get a handle for branch management operations.
    ///
    /// The returned [`Branches`] handle provides the "power API" for branch
    /// management, including listing, creating, deleting, forking, diffing,
    /// and merging branches.
    ///
    /// # Example
    ///
    /// ```text
    /// // List all branches
    /// for branch in db.branches().list()? {
    ///     println!("Branch: {}", branch);
    /// }
    ///
    /// // Create a new branch
    /// db.branches().create("experiment")?;
    ///
    /// // Fork a branch
    /// db.branches().fork("default", "experiment-copy")?;
    /// ```
    pub fn branches(&self) -> Branches<'_> {
        Branches::new(&self.executor)
    }

    /// Create a new [`Session`] for interactive transaction support.
    ///
    /// The returned session wraps a fresh executor and can manage an
    /// optional open transaction across multiple `execute()` calls.
    /// The session inherits the access mode of this handle.
    pub fn session(&self) -> Session {
        Session::new_with_mode(self.executor.primitives().db.clone(), self.access_mode)
    }

    // =========================================================================
    // Branch Context
    // =========================================================================

    /// Get the current branch name.
    ///
    /// Returns the name of the branch that all data operations will use.
    pub fn current_branch(&self) -> &str {
        self.current_branch.as_str()
    }

    /// Switch to an existing branch.
    ///
    /// All subsequent data operations will use this branch.
    ///
    /// # Errors
    ///
    /// Returns an error if the branch doesn't exist. Use `create_branch()` first
    /// to create a new branch.
    ///
    /// # Example
    ///
    /// ```text
    /// // Switch to an existing branch
    /// db.set_branch("my-experiment")?;
    /// db.kv_put("key", "value")?;  // Data goes to my-experiment
    ///
    /// // Switch back to default
    /// db.set_branch("default")?;
    /// ```
    pub fn set_branch(&mut self, branch_name: &str) -> Result<()> {
        // Check if branch exists
        if !self.branches().exists(branch_name)? {
            return Err(Error::BranchNotFound {
                branch: branch_name.to_string(),
            });
        }

        self.current_branch = BranchId::from(branch_name);
        Ok(())
    }

    /// Create a new blank branch.
    ///
    /// The new branch starts with no data. Stays on the current branch after creation.
    /// Use `set_branch()` to switch to the new branch.
    ///
    /// # Errors
    ///
    /// Returns an error if the branch already exists.
    ///
    /// # Example
    ///
    /// ```text
    /// // Create a new branch
    /// db.create_branch("experiment")?;
    ///
    /// // Optionally switch to it
    /// db.set_branch("experiment")?;
    /// ```
    pub fn create_branch(&self, branch_name: &str) -> Result<()> {
        self.branches().create(branch_name)
    }

    /// Fork the current branch with all its data into a new branch.
    ///
    /// Copies all data (KV, State, Events, JSON, Vectors) from the current
    /// branch to the new branch. Stays on the current branch after forking.
    /// Use `set_branch()` to switch to the fork.
    ///
    /// # Example
    ///
    /// ```text
    /// // Fork current branch to "experiment"
    /// db.fork_branch("experiment")?;
    ///
    /// // Switch to the fork
    /// db.set_branch("experiment")?;
    /// // ... make changes without affecting original ...
    /// ```
    pub fn fork_branch(&self, destination: &str) -> Result<ForkInfo> {
        self.branches().fork(self.current_branch(), destination)
    }

    /// Compare two branches and return their differences.
    ///
    /// Returns a structured diff showing per-space added, removed, and
    /// modified entries between the two branches.
    pub fn diff_branches(&self, branch_a: &str, branch_b: &str) -> Result<BranchDiffResult> {
        self.branches().diff(branch_a, branch_b)
    }

    /// Merge data from source branch into target branch.
    ///
    /// See [`Branches::merge`] for details on merge strategies.
    pub fn merge_branches(
        &self,
        source: &str,
        target: &str,
        strategy: MergeStrategy,
    ) -> Result<MergeInfo> {
        self.branches().merge(source, target, strategy)
    }

    /// List all available branches.
    ///
    /// Returns a list of branch names.
    pub fn list_branches(&self) -> Result<Vec<String>> {
        self.branches().list()
    }

    /// Delete a branch and all its data.
    ///
    /// **WARNING**: This is irreversible! All data in the branch will be deleted.
    ///
    /// # Errors
    ///
    /// - Returns an error if trying to delete the current branch
    /// - Returns an error if trying to delete the "default" branch
    pub fn delete_branch(&self, branch_name: &str) -> Result<()> {
        // Cannot delete the current branch
        if branch_name == self.current_branch.as_str() {
            return Err(Error::ConstraintViolation {
                reason: "Cannot delete the current branch. Switch to a different branch first."
                    .into(),
            });
        }

        self.branches().delete(branch_name)
    }

    /// Get the BranchId for use in commands.
    ///
    /// This is used internally by the data operation methods.
    pub(crate) fn branch_id(&self) -> Option<BranchId> {
        Some(self.current_branch.clone())
    }

    /// Get the space for use in commands.
    pub(crate) fn space_id(&self) -> Option<String> {
        Some(self.current_space.clone())
    }

    // =========================================================================
    // Space Context
    // =========================================================================

    /// Get the current space name.
    pub fn current_space(&self) -> &str {
        &self.current_space
    }

    /// Switch to a different space.
    ///
    /// All subsequent data operations will use this space.
    /// The "default" space always exists. Other spaces are created on first use.
    pub fn set_space(&mut self, space: &str) -> Result<()> {
        strata_core::validate_space_name(space).map_err(|reason| Error::InvalidInput { reason })?;
        self.current_space = space.to_string();
        Ok(())
    }

    /// List all spaces in the current branch.
    pub fn list_spaces(&self) -> Result<Vec<String>> {
        match self.executor.execute(Command::SpaceList {
            branch: self.branch_id(),
        })? {
            Output::SpaceList(spaces) => Ok(spaces),
            _ => Err(Error::Internal {
                reason: "Unexpected output for SpaceList".into(),
            }),
        }
    }

    /// Delete a space from the current branch.
    ///
    /// # Errors
    /// - Returns error if trying to delete the "default" space
    /// - Returns error if space is non-empty (unless force is true)
    pub fn delete_space(&self, space: &str) -> Result<()> {
        if space == "default" {
            return Err(Error::ConstraintViolation {
                reason: "Cannot delete the default space".into(),
            });
        }
        match self.executor.execute(Command::SpaceDelete {
            branch: self.branch_id(),
            space: space.to_string(),
            force: false,
        })? {
            Output::Unit => Ok(()),
            _ => Err(Error::Internal {
                reason: "Unexpected output for SpaceDelete".into(),
            }),
        }
    }

    /// Delete a space forcefully (even if non-empty).
    pub fn delete_space_force(&self, space: &str) -> Result<()> {
        if space == "default" {
            return Err(Error::ConstraintViolation {
                reason: "Cannot delete the default space".into(),
            });
        }
        match self.executor.execute(Command::SpaceDelete {
            branch: self.branch_id(),
            space: space.to_string(),
            force: true,
        })? {
            Output::Unit => Ok(()),
            _ => Err(Error::Internal {
                reason: "Unexpected output for SpaceDelete".into(),
            }),
        }
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::*;
    use crate::Value;

    fn create_strata() -> Strata {
        Strata::cache().unwrap()
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
        let value = db.state_get("cell").unwrap();
        assert!(value.is_some());
        assert_eq!(value.unwrap(), Value::String("state".into()));
    }

    #[test]
    fn test_event_append_range() {
        let db = create_strata();

        // Event payloads must be Objects
        db.event_append(
            "stream",
            Value::Object([("value".to_string(), Value::Int(1))].into_iter().collect()),
        )
        .unwrap();
        db.event_append(
            "stream",
            Value::Object([("value".to_string(), Value::Int(2))].into_iter().collect()),
        )
        .unwrap();

        let events = db.event_get_by_type("stream").unwrap();
        assert_eq!(events.len(), 2);
    }

    #[test]
    fn test_vector_operations() {
        let db = create_strata();

        db.vector_create_collection("vecs", 4u64, DistanceMetric::Cosine)
            .unwrap();
        db.vector_upsert("vecs", "v1", vec![1.0, 0.0, 0.0, 0.0], None)
            .unwrap();
        db.vector_upsert("vecs", "v2", vec![0.0, 1.0, 0.0, 0.0], None)
            .unwrap();

        let matches = db
            .vector_search("vecs", vec![1.0, 0.0, 0.0, 0.0], 10u64)
            .unwrap();
        assert_eq!(matches.len(), 2);
        assert_eq!(matches[0].key, "v1");
    }

    #[test]
    fn test_branch_create_list() {
        let db = create_strata();

        let (info, _version) = db
            .branch_create(
                Some("550e8400-e29b-41d4-a716-446655440099".to_string()),
                None,
            )
            .unwrap();
        assert_eq!(info.id.as_str(), "550e8400-e29b-41d4-a716-446655440099");

        let branches = db.branch_list(None, None, None).unwrap();
        assert!(!branches.is_empty());
    }

    // =========================================================================
    // Branch Context Tests
    // =========================================================================

    #[test]
    fn test_current_branch_default() {
        let db = create_strata();
        assert_eq!(db.current_branch(), "default");
    }

    #[test]
    fn test_create_branch() {
        let db = create_strata();

        // Create a new branch (stays on current branch)
        db.create_branch("experiment-1").unwrap();

        // Still on default branch
        assert_eq!(db.current_branch(), "default");

        // But the branch exists
        assert!(db.branch_exists("experiment-1").unwrap());
    }

    #[test]
    fn test_set_branch_to_existing() {
        let mut db = create_strata();

        // Create a branch first
        db.create_branch("my-branch").unwrap();

        // Switch to it
        db.set_branch("my-branch").unwrap();
        assert_eq!(db.current_branch(), "my-branch");
    }

    #[test]
    fn test_set_branch_nonexistent_fails() {
        let mut db = create_strata();

        // Try to switch to a branch that doesn't exist
        let result = db.set_branch("nonexistent");
        assert!(result.is_err());
    }

    #[test]
    fn test_list_branches() {
        let db = create_strata();

        // Create a few branches
        db.create_branch("branch-a").unwrap();
        db.create_branch("branch-b").unwrap();
        db.create_branch("branch-c").unwrap();

        let branches = db.list_branches().unwrap();
        assert!(branches.contains(&"branch-a".to_string()));
        assert!(branches.contains(&"branch-b".to_string()));
        assert!(branches.contains(&"branch-c".to_string()));
    }

    #[test]
    fn test_delete_branch() {
        let db = create_strata();

        // Create a branch
        db.create_branch("to-delete").unwrap();

        // Delete the branch
        db.delete_branch("to-delete").unwrap();

        // Verify it's gone
        assert!(!db.branch_exists("to-delete").unwrap());
    }

    #[test]
    fn test_delete_current_branch_fails() {
        let mut db = create_strata();

        db.create_branch("current-branch").unwrap();
        db.set_branch("current-branch").unwrap();

        // Trying to delete the current branch should fail
        let result = db.delete_branch("current-branch");
        assert!(result.is_err());
    }

    #[test]
    fn test_delete_default_branch_fails() {
        let db = create_strata();

        // Trying to delete the default branch should fail
        let result = db.delete_branch("default");
        assert!(result.is_err());
    }

    #[test]
    fn test_branch_context_data_isolation() {
        let mut db = create_strata();

        // Put data in default branch (simplified API)
        db.kv_put("key", "default-value").unwrap();

        // Create and switch to another branch
        db.create_branch("experiment").unwrap();
        db.set_branch("experiment").unwrap();

        // The key should not exist in this branch
        assert!(db.kv_get("key").unwrap().is_none());

        // Put different data
        db.kv_put("key", "experiment-value").unwrap();

        // Switch back to default
        db.set_branch("default").unwrap();

        // Original value should still be there
        let value = db.kv_get("key").unwrap();
        assert_eq!(value, Some(Value::String("default-value".into())));
    }

    #[test]
    fn test_branch_context_isolation_all_primitives() {
        let mut db = create_strata();

        // Put data in default branch (simplified API)
        db.kv_put("kv-key", 1i64).unwrap();
        db.state_set("state-cell", 10i64).unwrap();
        db.event_append(
            "stream",
            Value::Object([("x".to_string(), Value::Int(100))].into_iter().collect()),
        )
        .unwrap();

        // Create and switch to another branch
        db.create_branch("isolated").unwrap();
        db.set_branch("isolated").unwrap();

        // None of the data should exist in this branch
        assert!(db.kv_get("kv-key").unwrap().is_none());
        assert!(db.state_get("state-cell").unwrap().is_none());
        assert_eq!(db.event_len().unwrap(), 0);
    }

    // =========================================================================
    // db.branches() Power API Tests
    // =========================================================================

    #[test]
    fn test_branches_list() {
        let db = create_strata();

        // Create some branches
        db.branches().create("branch-a").unwrap();
        db.branches().create("branch-b").unwrap();

        let branches = db.branches().list().unwrap();
        assert!(branches.contains(&"branch-a".to_string()));
        assert!(branches.contains(&"branch-b".to_string()));
    }

    #[test]
    fn test_branches_exists() {
        let db = create_strata();

        assert!(!db.branches().exists("nonexistent").unwrap());

        db.branches().create("my-branch").unwrap();
        assert!(db.branches().exists("my-branch").unwrap());
    }

    #[test]
    fn test_branches_create() {
        let db = create_strata();

        db.branches().create("new-branch").unwrap();
        assert!(db.branches().exists("new-branch").unwrap());
    }

    #[test]
    fn test_branches_create_duplicate_fails() {
        let db = create_strata();

        db.branches().create("my-branch").unwrap();
        let result = db.branches().create("my-branch");
        assert!(result.is_err());
    }

    #[test]
    fn test_branches_delete() {
        let db = create_strata();

        db.branches().create("to-delete").unwrap();
        assert!(db.branches().exists("to-delete").unwrap());

        db.branches().delete("to-delete").unwrap();
        assert!(!db.branches().exists("to-delete").unwrap());
    }

    #[test]
    fn test_branches_delete_default_fails() {
        let db = create_strata();

        let result = db.branches().delete("default");
        assert!(result.is_err());
    }

    #[test]
    fn test_branches_fork() {
        let db = create_strata();

        // Write some data to default branch
        db.kv_put("key1", "value1").unwrap();
        db.kv_put("key2", 42i64).unwrap();

        // Fork default branch to "forked"
        let info = db.fork_branch("forked").unwrap();
        assert_eq!(info.source, "default");
        assert_eq!(info.destination, "forked");
        assert!(info.keys_copied >= 2);
    }

    #[test]
    fn test_branches_diff() {
        let mut db = create_strata();

        // Write data to default branch
        db.kv_put("shared", "value-a").unwrap();
        db.kv_put("only-default", 1i64).unwrap();

        // Create another branch with different data
        db.create_branch("other").unwrap();
        db.set_branch("other").unwrap();
        db.kv_put("shared", "value-b").unwrap();
        db.kv_put("only-other", 2i64).unwrap();

        let diff = db.diff_branches("default", "other").unwrap();
        assert_eq!(diff.branch_a, "default");
        assert_eq!(diff.branch_b, "other");
        // "shared" should be modified, "only-default" removed, "only-other" added
        assert!(diff.summary.total_modified >= 1);
        assert!(diff.summary.total_removed >= 1);
        assert!(diff.summary.total_added >= 1);
    }

    #[test]
    fn test_branches_merge() {
        let mut db = create_strata();

        // Write data to default
        db.kv_put("base-key", "base-value").unwrap();

        // Create and populate source branch
        db.create_branch("source").unwrap();
        db.set_branch("source").unwrap();
        db.kv_put("new-key", "new-value").unwrap();

        // Merge source into default
        db.set_branch("default").unwrap();
        let info = db
            .merge_branches("source", "default", MergeStrategy::LastWriterWins)
            .unwrap();
        assert!(info.keys_applied >= 1);

        // Verify merged data
        assert_eq!(
            db.kv_get("new-key").unwrap(),
            Some(Value::String("new-value".into()))
        );
        // Original data should still be there
        assert_eq!(
            db.kv_get("base-key").unwrap(),
            Some(Value::String("base-value".into()))
        );
    }
}
