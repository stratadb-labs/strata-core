//! RunIndex: First-class run lifecycle management
//!
//! ## Design Principles
//!
//! 1. **Status Transitions**: Valid transitions are enforced (no resurrection).
//! 2. **Terminal State**: Archived is terminal - no transitions allowed.
//! 3. **Cascading Delete**: `delete_run()` removes ALL data for a run.
//!
//! ## Status Transitions
//!
//! Valid transitions:
//! - Active → Completed, Failed, Cancelled, Paused, Archived
//! - Paused → Active, Cancelled, Archived
//! - Completed → Archived
//! - Failed → Archived
//! - Cancelled → Archived
//!
//! Invalid transitions (will error):
//! - Completed → Active (no resurrection)
//! - Failed → Active (no resurrection)
//! - Archived → * (terminal state)
//!
//! ## Key Design
//!
//! - TypeTag: Run (0x05)
//! - Primary key format: `<global_namespace>:<TypeTag::Run>:<run_id>`
//! - Index key format: `<global_namespace>:<TypeTag::Run>:__idx_{type}__{value}__{run_id}`
//!
//! RunIndex uses a global namespace (not run-scoped) since it manages runs themselves.

use strata_concurrency::TransactionContext;
use strata_core::contract::{Timestamp, Version, Versioned};
use strata_core::error::{Error, Result};
use strata_core::types::{Key, Namespace, RunId, TypeTag};
use strata_core::value::Value;
use strata_engine::Database;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

// ========== Global Run ID for RunIndex Operations ==========

/// Get the global RunId used for RunIndex operations
///
/// RunIndex is a global index (not scoped to any particular run),
/// so we use a nil UUID as a sentinel value.
fn global_run_id() -> RunId {
    RunId::from_bytes([0; 16])
}

/// Get the global namespace for RunIndex operations
fn global_namespace() -> Namespace {
    Namespace::for_run(global_run_id())
}

// ========== RunStatus Enum (Story #191) ==========

/// Run lifecycle status
///
/// Each run progresses through these states with enforced transitions.
/// The status determines what operations are valid on the run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RunStatus {
    /// Run is currently executing
    Active,
    /// Run completed successfully
    Completed,
    /// Run failed with error
    Failed,
    /// Run was cancelled
    Cancelled,
    /// Run is paused (can resume)
    Paused,
    /// Run is archived (terminal, soft delete)
    Archived,
}

impl RunStatus {
    /// Check if this is a terminal status
    ///
    /// Only Archived is truly terminal - no transitions allowed from it.
    pub fn is_terminal(&self) -> bool {
        matches!(self, RunStatus::Archived)
    }

    /// Check if this is a "finished" status (completed, failed, or cancelled)
    pub fn is_finished(&self) -> bool {
        matches!(
            self,
            RunStatus::Completed | RunStatus::Failed | RunStatus::Cancelled
        )
    }

    /// Check if transition from current to target is valid
    ///
    /// ## Valid Transitions
    /// - Active: can go anywhere
    /// - Paused: can resume (Active), cancel, or archive
    /// - Completed/Failed/Cancelled: can only archive
    /// - Archived: terminal (no transitions)
    pub fn can_transition_to(&self, target: RunStatus) -> bool {
        match (self, target) {
            // From Active: can go anywhere
            (RunStatus::Active, _) => true,

            // From Paused: can resume, cancel, or archive
            (RunStatus::Paused, RunStatus::Active) => true,
            (RunStatus::Paused, RunStatus::Cancelled) => true,
            (RunStatus::Paused, RunStatus::Archived) => true,

            // From terminal-ish states: can only archive
            (RunStatus::Completed, RunStatus::Archived) => true,
            (RunStatus::Failed, RunStatus::Archived) => true,
            (RunStatus::Cancelled, RunStatus::Archived) => true,

            // Archived is terminal - no transitions allowed
            (RunStatus::Archived, _) => false,

            // All other transitions are invalid (no resurrection)
            _ => false,
        }
    }

    /// Get the string representation for indexing
    pub fn as_str(&self) -> &'static str {
        match self {
            RunStatus::Active => "Active",
            RunStatus::Completed => "Completed",
            RunStatus::Failed => "Failed",
            RunStatus::Cancelled => "Cancelled",
            RunStatus::Paused => "Paused",
            RunStatus::Archived => "Archived",
        }
    }
}

// ========== RunMetadata Struct (Story #191) ==========

/// Metadata about a run
///
/// Contains all information about a run's lifecycle,
/// relationships, and custom metadata.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RunMetadata {
    /// User-provided name/key for the run (used for lookups)
    pub name: String,
    /// Unique run identifier (UUID) for internal use and namespacing
    pub run_id: String,
    /// Parent run name if forked
    pub parent_run: Option<String>,
    /// Current status
    pub status: RunStatus,
    /// Creation timestamp (milliseconds since epoch)
    pub created_at: i64,
    /// Last update timestamp (milliseconds since epoch)
    pub updated_at: i64,
    /// Completion timestamp (if finished)
    pub completed_at: Option<i64>,
    /// User-defined tags
    pub tags: Vec<String>,
    /// Custom metadata
    pub metadata: Value,
    /// Error message if failed
    pub error: Option<String>,
    /// Internal version counter for CAS operations
    #[serde(default = "default_version")]
    pub version: u64,
}

fn default_version() -> u64 {
    1
}

impl RunMetadata {
    /// Create new run metadata with Active status
    ///
    /// Generates a new UUID for the run_id automatically.
    pub fn new(name: &str) -> Self {
        let now = Self::now();
        let run_id = RunId::new();
        Self {
            name: name.to_string(),
            run_id: run_id.to_string(),
            parent_run: None,
            status: RunStatus::Active,
            created_at: now,
            updated_at: now,
            completed_at: None,
            tags: vec![],
            metadata: Value::Null,
            error: None,
            version: 1,
        }
    }

    /// Wrap this metadata in a Versioned container
    pub fn into_versioned(self) -> Versioned<RunMetadata> {
        let version = self.version;
        let timestamp = self.updated_at;
        Versioned::with_timestamp(
            self,
            Version::counter(version),
            Timestamp::from_micros((timestamp * 1000) as u64),
        )
    }

    /// Increment version and update timestamp
    pub fn touch(&mut self) {
        self.version += 1;
        self.updated_at = Self::now();
    }

    /// Get current timestamp in milliseconds
    fn now() -> i64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64
    }
}

// ========== Serialization Helpers ==========

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

// ========== RunIndex Core (Story #191) ==========

/// Run lifecycle management primitive
///
/// ## Design
///
/// RunIndex provides first-class run lifecycle management.
/// It is a stateless facade over the Database engine, holding only
/// an `Arc<Database>` reference.
///
/// ## Key Features
///
/// - Status transition validation (no resurrection)
/// - Cascading delete (removes ALL run data)
/// - Secondary indices (by-status, by-tag, by-parent)
/// - Soft archive vs hard delete
///
/// ## Example
///
/// ```rust,ignore
/// use strata_primitives::{RunIndex, RunStatus};
///
/// let ri = RunIndex::new(db.clone());
///
/// // Create a run
/// let meta = ri.create_run("my-run")?;
/// assert_eq!(meta.value.status, RunStatus::Active);
///
/// // Complete the run
/// let meta = ri.complete_run("my-run")?;
/// assert_eq!(meta.value.status, RunStatus::Completed);
///
/// // Archive it (soft delete)
/// let meta = ri.archive_run("my-run")?;
/// assert_eq!(meta.value.status, RunStatus::Archived);
/// ```
#[derive(Clone)]
pub struct RunIndex {
    db: Arc<Database>,
}

impl RunIndex {
    /// Create new RunIndex instance
    pub fn new(db: Arc<Database>) -> Self {
        Self { db }
    }

    /// Get the underlying database reference
    pub fn database(&self) -> &Arc<Database> {
        &self.db
    }

    /// Build key for run metadata
    fn key_for(&self, run_id: &str) -> Key {
        Key::new_run_with_id(global_namespace(), run_id)
    }

    // ========== Create & Get Operations (Story #192) ==========

    /// Create a new run
    ///
    /// Creates a run with Active status and no parent.
    pub fn create_run(&self, run_id: &str) -> Result<Versioned<RunMetadata>> {
        self.create_run_with_options(run_id, None, vec![], Value::Null)
    }

    /// Create a new run with options
    ///
    /// ## Arguments
    /// - `run_id`: Unique identifier for the run
    /// - `parent_run`: Optional parent run ID (for forked runs)
    /// - `tags`: User-defined tags for filtering
    /// - `metadata`: Custom metadata
    ///
    /// ## Errors
    /// - `InvalidOperation` if run already exists
    /// - `InvalidOperation` if parent doesn't exist
    pub fn create_run_with_options(
        &self,
        run_id: &str,
        parent_run: Option<String>,
        tags: Vec<String>,
        metadata: Value,
    ) -> Result<Versioned<RunMetadata>> {
        self.db.transaction(global_run_id(), |txn| {
            let key = self.key_for(run_id);

            // Check if run already exists
            if txn.get(&key)?.is_some() {
                return Err(Error::InvalidOperation(format!(
                    "Run '{}' already exists",
                    run_id
                )));
            }

            // Validate parent exists if specified
            if let Some(ref parent_id) = parent_run {
                let parent_key = self.key_for(parent_id);
                if txn.get(&parent_key)?.is_none() {
                    return Err(Error::InvalidOperation(format!(
                        "Parent run '{}' not found",
                        parent_id
                    )));
                }
            }

            let mut run_meta = RunMetadata::new(run_id);
            run_meta.parent_run = parent_run.clone();
            run_meta.tags = tags.clone();
            run_meta.metadata = metadata;

            txn.put(key, to_stored_value(&run_meta))?;

            // Write indices
            Self::write_indices_internal(txn, &run_meta)?;

            Ok(run_meta.into_versioned())
        })
    }

    /// Get run metadata
    ///
    /// ## Returns
    /// - `Some(Versioned<metadata>)` if run exists
    /// - `None` if run doesn't exist
    pub fn get_run(&self, run_id: &str) -> Result<Option<Versioned<RunMetadata>>> {
        self.db.transaction(global_run_id(), |txn| {
            let key = self.key_for(run_id);
            match txn.get(&key)? {
                Some(v) => {
                    let meta: RunMetadata = from_stored_value(&v)
                        .map_err(|e| Error::SerializationError(e.to_string()))?;
                    Ok(Some(meta.into_versioned()))
                }
                None => Ok(None),
            }
        })
    }

    /// Check if a run exists
    pub fn exists(&self, run_id: &str) -> Result<bool> {
        self.db.transaction(global_run_id(), |txn| {
            let key = self.key_for(run_id);
            Ok(txn.get(&key)?.is_some())
        })
    }

    /// List all run IDs
    pub fn list_runs(&self) -> Result<Vec<String>> {
        self.db.transaction(global_run_id(), |txn| {
            let prefix = Key::new_run_with_id(global_namespace(), "");
            let results = txn.scan_prefix(&prefix)?;

            // Filter out index keys (they contain "__idx_")
            Ok(results
                .into_iter()
                .filter_map(|(k, _)| {
                    let key_str = String::from_utf8(k.user_key.clone()).ok()?;
                    if key_str.contains("__idx_") {
                        None
                    } else {
                        Some(key_str)
                    }
                })
                .collect())
        })
    }

    /// Count all runs
    pub fn count(&self) -> Result<usize> {
        Ok(self.list_runs()?.len())
    }

    // ========== Status Update & Transitions (Story #193) ==========

    /// Update run status with transition validation
    ///
    /// ## Errors
    /// - `InvalidOperation` if run doesn't exist
    /// - `InvalidOperation` if transition is invalid
    pub fn update_status(&self, run_id: &str, new_status: RunStatus) -> Result<Versioned<RunMetadata>> {
        self.db.transaction(global_run_id(), |txn| {
            let key = self.key_for(run_id);

            let mut run_meta: RunMetadata = match txn.get(&key)? {
                Some(v) => {
                    from_stored_value(&v).map_err(|e| Error::SerializationError(e.to_string()))?
                }
                None => {
                    return Err(Error::InvalidOperation(format!(
                        "Run '{}' not found",
                        run_id
                    )))
                }
            };

            // Validate transition
            if !run_meta.status.can_transition_to(new_status) {
                return Err(Error::InvalidOperation(format!(
                    "Invalid status transition: {:?} -> {:?}",
                    run_meta.status, new_status
                )));
            }

            let old_status = run_meta.status;
            run_meta.status = new_status;
            run_meta.touch(); // Increment version and update timestamp

            // Set completed_at for finished states
            if new_status.is_finished() {
                run_meta.completed_at = Some(run_meta.updated_at);
            }

            txn.put(key, to_stored_value(&run_meta))?;

            // Update status index
            Self::update_status_index_internal(txn, run_id, old_status, new_status)?;

            Ok(run_meta.into_versioned())
        })
    }

    /// Complete a run successfully
    pub fn complete_run(&self, run_id: &str) -> Result<Versioned<RunMetadata>> {
        self.update_status(run_id, RunStatus::Completed)
    }

    /// Fail a run with error message
    pub fn fail_run(&self, run_id: &str, error: &str) -> Result<Versioned<RunMetadata>> {
        self.db.transaction(global_run_id(), |txn| {
            let key = self.key_for(run_id);

            let mut run_meta: RunMetadata = match txn.get(&key)? {
                Some(v) => {
                    from_stored_value(&v).map_err(|e| Error::SerializationError(e.to_string()))?
                }
                None => {
                    return Err(Error::InvalidOperation(format!(
                        "Run '{}' not found",
                        run_id
                    )))
                }
            };

            if !run_meta.status.can_transition_to(RunStatus::Failed) {
                return Err(Error::InvalidOperation(format!(
                    "Invalid status transition: {:?} -> Failed",
                    run_meta.status
                )));
            }

            let old_status = run_meta.status;
            run_meta.status = RunStatus::Failed;
            run_meta.error = Some(error.to_string());
            run_meta.touch(); // Increment version and update timestamp
            run_meta.completed_at = Some(run_meta.updated_at);

            txn.put(key, to_stored_value(&run_meta))?;
            Self::update_status_index_internal(txn, run_id, old_status, RunStatus::Failed)?;

            Ok(run_meta.into_versioned())
        })
    }

    /// Pause a run
    pub fn pause_run(&self, run_id: &str) -> Result<Versioned<RunMetadata>> {
        self.update_status(run_id, RunStatus::Paused)
    }

    /// Resume a paused run
    pub fn resume_run(&self, run_id: &str) -> Result<Versioned<RunMetadata>> {
        self.update_status(run_id, RunStatus::Active)
    }

    /// Cancel a run
    pub fn cancel_run(&self, run_id: &str) -> Result<Versioned<RunMetadata>> {
        self.update_status(run_id, RunStatus::Cancelled)
    }

    // ========== Secondary Indices (Story #194) ==========

    /// Write secondary indices for a run
    fn write_indices_internal(txn: &mut TransactionContext, meta: &RunMetadata) -> Result<()> {
        let ns = global_namespace();

        // Index by status - use name for both key suffix and value (for lookups)
        let status_key =
            Key::new_run_index(ns.clone(), "by-status", meta.status.as_str(), &meta.name);
        txn.put(status_key, Value::String(meta.name.clone()))?;

        // Index by each tag
        for tag in &meta.tags {
            let tag_key = Key::new_run_index(ns.clone(), "by-tag", tag, &meta.name);
            txn.put(tag_key, Value::String(meta.name.clone()))?;
        }

        // Index by parent
        if let Some(ref parent) = meta.parent_run {
            let parent_key = Key::new_run_index(ns.clone(), "by-parent", parent, &meta.name);
            txn.put(parent_key, Value::String(meta.name.clone()))?;
        }

        Ok(())
    }

    /// Update status index on transition
    fn update_status_index_internal(
        txn: &mut TransactionContext,
        run_id: &str,
        old_status: RunStatus,
        new_status: RunStatus,
    ) -> Result<()> {
        let ns = global_namespace();

        // Remove old status index
        let old_key = Key::new_run_index(ns.clone(), "by-status", old_status.as_str(), run_id);
        txn.delete(old_key)?;

        // Add new status index
        let new_key = Key::new_run_index(ns, "by-status", new_status.as_str(), run_id);
        txn.put(new_key, Value::String(run_id.to_string()))?;

        Ok(())
    }

    /// Query runs by status
    pub fn query_by_status(&self, status: RunStatus) -> Result<Vec<RunMetadata>> {
        let ids = self.scan_index("by-status", status.as_str())?;
        self.get_many(&ids)
    }

    /// Query runs by tag
    pub fn query_by_tag(&self, tag: &str) -> Result<Vec<RunMetadata>> {
        let ids = self.scan_index("by-tag", tag)?;
        self.get_many(&ids)
    }

    /// Get child runs
    pub fn get_child_runs(&self, parent_id: &str) -> Result<Vec<RunMetadata>> {
        let ids = self.scan_index("by-parent", parent_id)?;
        self.get_many(&ids)
    }

    /// Scan an index and return run IDs
    fn scan_index(&self, index_type: &str, index_value: &str) -> Result<Vec<String>> {
        self.db.transaction(global_run_id(), |txn| {
            let prefix = Key::new_run_index(global_namespace(), index_type, index_value, "");

            let results = txn.scan_prefix(&prefix)?;
            Ok(results
                .into_iter()
                .filter_map(|(_, v)| {
                    if let Value::String(s) = v {
                        Some(s)
                    } else {
                        None
                    }
                })
                .collect())
        })
    }

    /// Get multiple runs by IDs
    fn get_many(&self, ids: &[String]) -> Result<Vec<RunMetadata>> {
        let mut runs = Vec::new();
        for id in ids {
            if let Some(versioned) = self.get_run(id)? {
                runs.push(versioned.value);
            }
        }
        Ok(runs)
    }

    // ========== Delete & Archive Operations (Story #195) ==========

    /// Archive a run (soft delete - status change to Archived)
    ///
    /// The run metadata is preserved but marked as archived.
    /// Archived runs cannot transition to any other status.
    pub fn archive_run(&self, run_id: &str) -> Result<Versioned<RunMetadata>> {
        self.update_status(run_id, RunStatus::Archived)
    }

    /// Delete a run (HARD DELETE - removes ALL data for this run)
    ///
    /// This is a cascading delete that removes:
    /// - Run metadata
    /// - All secondary indices
    /// - All run-scoped data (KV, Events, States, Traces)
    ///
    /// USE WITH CAUTION - this is irreversible!
    pub fn delete_run(&self, run_id: &str) -> Result<()> {
        // First get the run metadata to know what indices to delete
        let run_meta = self
            .get_run(run_id)?
            .ok_or_else(|| Error::InvalidOperation(format!("Run '{}' not found", run_id)))?
            .value;

        // Parse run_id string to get actual RunId for namespace
        let actual_run_id = RunId::from_string(&run_meta.run_id).ok_or_else(|| {
            Error::InvalidOperation(format!("Invalid run_id format: {}", run_meta.run_id))
        })?;

        // First, delete all run-scoped data (cascading delete)
        self.delete_run_data_internal(actual_run_id)?;

        // Then delete the RunIndex metadata and indices
        self.db.transaction(global_run_id(), |txn| {
            // Delete run metadata
            let meta_key = self.key_for(run_id);
            txn.delete(meta_key)?;

            // Delete status index
            let status_key = Key::new_run_index(
                global_namespace(),
                "by-status",
                run_meta.status.as_str(),
                run_id,
            );
            txn.delete(status_key)?;

            // Delete tag indices
            for tag in &run_meta.tags {
                let tag_key = Key::new_run_index(global_namespace(), "by-tag", tag, run_id);
                txn.delete(tag_key)?;
            }

            // Delete parent index
            if let Some(ref parent) = run_meta.parent_run {
                let parent_key =
                    Key::new_run_index(global_namespace(), "by-parent", parent, run_id);
                txn.delete(parent_key)?;
            }

            Ok(())
        })
    }

    /// Internal helper to delete all run-scoped data
    ///
    /// Deletes all data for the given run from all type tags:
    /// - KV entries
    /// - Events
    /// - State cells
    /// - Traces
    fn delete_run_data_internal(&self, run_id: RunId) -> Result<()> {
        let ns = Namespace::for_run(run_id);

        // Delete data for each type tag
        for type_tag in [TypeTag::KV, TypeTag::Event, TypeTag::State, TypeTag::Trace] {
            self.db.transaction(run_id, |txn| {
                let prefix = Key::new(ns.clone(), type_tag, vec![]);
                let entries = txn.scan_prefix(&prefix)?;

                for (key, _) in entries {
                    txn.delete(key)?;
                }

                Ok(())
            })?;
        }

        Ok(())
    }

    /// Add tags to a run
    pub fn add_tags(&self, run_id: &str, new_tags: Vec<String>) -> Result<Versioned<RunMetadata>> {
        self.db.transaction(global_run_id(), |txn| {
            let key = self.key_for(run_id);

            let mut meta: RunMetadata = match txn.get(&key)? {
                Some(v) => {
                    from_stored_value(&v).map_err(|e| Error::SerializationError(e.to_string()))?
                }
                None => {
                    return Err(Error::InvalidOperation(format!(
                        "Run '{}' not found",
                        run_id
                    )))
                }
            };

            // Add new tags
            let ns = global_namespace();
            for tag in &new_tags {
                if !meta.tags.contains(tag) {
                    meta.tags.push(tag.clone());

                    // Add tag index
                    let tag_key = Key::new_run_index(ns.clone(), "by-tag", tag, run_id);
                    txn.put(tag_key, Value::String(run_id.to_string()))?;
                }
            }

            meta.touch();
            txn.put(key, to_stored_value(&meta))?;

            Ok(meta.into_versioned())
        })
    }

    /// Remove tags from a run
    pub fn remove_tags(&self, run_id: &str, tags_to_remove: Vec<String>) -> Result<Versioned<RunMetadata>> {
        self.db.transaction(global_run_id(), |txn| {
            let key = self.key_for(run_id);

            let mut meta: RunMetadata = match txn.get(&key)? {
                Some(v) => {
                    from_stored_value(&v).map_err(|e| Error::SerializationError(e.to_string()))?
                }
                None => {
                    return Err(Error::InvalidOperation(format!(
                        "Run '{}' not found",
                        run_id
                    )))
                }
            };

            // Remove tags
            let ns = global_namespace();
            for tag in &tags_to_remove {
                if let Some(pos) = meta.tags.iter().position(|t| t == tag) {
                    meta.tags.remove(pos);

                    // Remove tag index
                    let tag_key = Key::new_run_index(ns.clone(), "by-tag", tag, run_id);
                    txn.delete(tag_key)?;
                }
            }

            meta.touch();
            txn.put(key, to_stored_value(&meta))?;

            Ok(meta.into_versioned())
        })
    }

    /// Update custom metadata
    pub fn update_metadata(&self, run_id: &str, metadata: Value) -> Result<Versioned<RunMetadata>> {
        self.db.transaction(global_run_id(), |txn| {
            let key = self.key_for(run_id);

            let mut meta: RunMetadata = match txn.get(&key)? {
                Some(v) => {
                    from_stored_value(&v).map_err(|e| Error::SerializationError(e.to_string()))?
                }
                None => {
                    return Err(Error::InvalidOperation(format!(
                        "Run '{}' not found",
                        run_id
                    )))
                }
            };

            meta.metadata = metadata;
            meta.touch();
            txn.put(key, to_stored_value(&meta))?;

            Ok(meta.into_versioned())
        })
    }

    // ========== Search API (M6) ==========

    /// Search runs
    ///
    /// Searches run ID, status, and metadata. Respects budget constraints.
    ///
    /// # Example
    ///
    /// ```ignore
    /// use strata_core::SearchRequest;
    ///
    /// let response = run_index.search(&SearchRequest::new(run_id, "active"))?;
    /// for hit in response.hits {
    ///     println!("Found run {:?} with score {}", hit.doc_ref, hit.score);
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
        let prefix = Key::new_run_with_id(global_namespace(), "");

        let mut candidates = Vec::new();
        let mut truncated = false;

        // Scan all runs
        for (key, versioned_value) in snapshot.scan_prefix(&prefix)? {
            // Filter out index keys
            let key_str = String::from_utf8(key.user_key.clone()).unwrap_or_default();
            if key_str.contains("__idx_") {
                continue;
            }

            // Check budget constraints
            if start.elapsed().as_micros() as u64 >= req.budget.max_wall_time_micros {
                truncated = true;
                break;
            }
            if candidates.len() >= req.budget.max_candidates_per_primitive {
                truncated = true;
                break;
            }

            // Deserialize run metadata
            let meta: RunMetadata = match from_stored_value(&versioned_value.value) {
                Ok(m) => m,
                Err(_) => continue,
            };

            // Time range filter
            if let Some((start_ts, end_ts)) = req.time_range {
                if meta.created_at < start_ts as i64 || meta.created_at > end_ts as i64 {
                    continue;
                }
            }

            // Extract searchable text
            let text = self.extract_run_text(&meta);

            candidates.push(SearchCandidate::new(
                DocRef::Run {
                    run_id: strata_core::types::RunId::from_string(&meta.run_id)
                        .unwrap_or_default(),
                },
                text,
                Some(meta.created_at as u64),
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

    /// Extract searchable text from a run
    fn extract_run_text(&self, meta: &RunMetadata) -> String {
        let status_str = match meta.status {
            RunStatus::Active => "active",
            RunStatus::Completed => "completed",
            RunStatus::Failed => "failed",
            RunStatus::Cancelled => "cancelled",
            RunStatus::Paused => "paused",
            RunStatus::Archived => "archived",
        };
        let mut parts = vec![meta.run_id.clone(), status_str.to_string()];
        parts.extend(meta.tags.clone());
        if let Ok(s) = serde_json::to_string(&meta.metadata) {
            parts.push(s);
        }
        parts.join(" ")
    }
}

// ========== Searchable Trait Implementation (M6) ==========

impl crate::searchable::Searchable for RunIndex {
    fn search(
        &self,
        req: &strata_core::SearchRequest,
    ) -> strata_core::error::Result<strata_core::SearchResponse> {
        self.search(req)
    }

    fn primitive_kind(&self) -> strata_core::PrimitiveType {
        strata_core::PrimitiveType::Run
    }
}

// ========== Tests ==========

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn create_test_db() -> (Arc<Database>, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let db = Database::open(temp_dir.path()).unwrap();
        (Arc::new(db), temp_dir)
    }

    // ========== Story #191 Tests: RunStatus ==========

    #[test]
    fn test_status_active_can_go_anywhere() {
        assert!(RunStatus::Active.can_transition_to(RunStatus::Completed));
        assert!(RunStatus::Active.can_transition_to(RunStatus::Failed));
        assert!(RunStatus::Active.can_transition_to(RunStatus::Cancelled));
        assert!(RunStatus::Active.can_transition_to(RunStatus::Paused));
        assert!(RunStatus::Active.can_transition_to(RunStatus::Archived));
    }

    #[test]
    fn test_status_paused_transitions() {
        assert!(RunStatus::Paused.can_transition_to(RunStatus::Active));
        assert!(RunStatus::Paused.can_transition_to(RunStatus::Cancelled));
        assert!(RunStatus::Paused.can_transition_to(RunStatus::Archived));
        assert!(!RunStatus::Paused.can_transition_to(RunStatus::Completed));
        assert!(!RunStatus::Paused.can_transition_to(RunStatus::Failed));
    }

    #[test]
    fn test_status_no_resurrection() {
        // Cannot go from finished states back to Active
        assert!(!RunStatus::Completed.can_transition_to(RunStatus::Active));
        assert!(!RunStatus::Failed.can_transition_to(RunStatus::Active));
        assert!(!RunStatus::Cancelled.can_transition_to(RunStatus::Active));
    }

    #[test]
    fn test_status_can_archive_from_finished() {
        assert!(RunStatus::Completed.can_transition_to(RunStatus::Archived));
        assert!(RunStatus::Failed.can_transition_to(RunStatus::Archived));
        assert!(RunStatus::Cancelled.can_transition_to(RunStatus::Archived));
    }

    #[test]
    fn test_status_archived_is_terminal() {
        assert!(!RunStatus::Archived.can_transition_to(RunStatus::Active));
        assert!(!RunStatus::Archived.can_transition_to(RunStatus::Completed));
        assert!(!RunStatus::Archived.can_transition_to(RunStatus::Failed));
        assert!(!RunStatus::Archived.can_transition_to(RunStatus::Cancelled));
        assert!(!RunStatus::Archived.can_transition_to(RunStatus::Paused));
        assert!(!RunStatus::Archived.can_transition_to(RunStatus::Archived));
    }

    #[test]
    fn test_status_is_terminal() {
        assert!(RunStatus::Archived.is_terminal());
        assert!(!RunStatus::Active.is_terminal());
        assert!(!RunStatus::Completed.is_terminal());
        assert!(!RunStatus::Failed.is_terminal());
    }

    #[test]
    fn test_status_is_finished() {
        assert!(RunStatus::Completed.is_finished());
        assert!(RunStatus::Failed.is_finished());
        assert!(RunStatus::Cancelled.is_finished());
        assert!(!RunStatus::Active.is_finished());
        assert!(!RunStatus::Paused.is_finished());
        assert!(!RunStatus::Archived.is_finished());
    }

    #[test]
    fn test_status_serialization() {
        let status = RunStatus::Active;
        let json = serde_json::to_string(&status).unwrap();
        let restored: RunStatus = serde_json::from_str(&json).unwrap();
        assert_eq!(status, restored);
    }

    // ========== Story #191 Tests: RunMetadata ==========

    #[test]
    fn test_run_metadata_creation() {
        let meta = RunMetadata::new("run-123");
        assert_eq!(meta.name, "run-123");
        // run_id should be a valid UUID
        assert!(RunId::from_string(&meta.run_id).is_some());
        assert_eq!(meta.status, RunStatus::Active);
        assert!(meta.created_at > 0);
        assert!(meta.parent_run.is_none());
        assert!(meta.completed_at.is_none());
        assert!(meta.tags.is_empty());
        assert!(meta.error.is_none());
    }

    #[test]
    fn test_run_metadata_serialization() {
        let meta = RunMetadata::new("run-test");
        let json = serde_json::to_string(&meta).unwrap();
        let restored: RunMetadata = serde_json::from_str(&json).unwrap();
        assert_eq!(meta, restored);
    }

    // ========== Story #191 Tests: RunIndex Core ==========

    #[test]
    fn test_runindex_new() {
        let (db, _temp) = create_test_db();
        let ri = RunIndex::new(db.clone());
        assert!(Arc::ptr_eq(ri.database(), &db));
    }

    // ========== Story #192 Tests: Create & Get ==========

    #[test]
    fn test_create_run() {
        let (db, _temp) = create_test_db();
        let ri = RunIndex::new(db);

        let meta = ri.create_run("my-run").unwrap();
        assert_eq!(meta.value.name, "my-run");
        // run_id is a generated UUID
        assert!(RunId::from_string(&meta.value.run_id).is_some());
        assert_eq!(meta.value.status, RunStatus::Active);
    }

    #[test]
    fn test_create_run_already_exists() {
        let (db, _temp) = create_test_db();
        let ri = RunIndex::new(db);

        ri.create_run("my-run").unwrap();
        let result = ri.create_run("my-run");

        assert!(result.is_err());
        match result {
            Err(Error::InvalidOperation(msg)) => {
                assert!(msg.contains("already exists"));
            }
            _ => panic!("Expected InvalidOperation error"),
        }
    }

    #[test]
    fn test_create_run_with_parent() {
        let (db, _temp) = create_test_db();
        let ri = RunIndex::new(db);

        ri.create_run("parent-run").unwrap();
        let meta = ri
            .create_run_with_options(
                "child-run",
                Some("parent-run".to_string()),
                vec![],
                Value::Null,
            )
            .unwrap();

        assert_eq!(meta.value.parent_run, Some("parent-run".to_string()));
    }

    #[test]
    fn test_create_run_parent_not_found() {
        let (db, _temp) = create_test_db();
        let ri = RunIndex::new(db);

        let result = ri.create_run_with_options(
            "child-run",
            Some("nonexistent".to_string()),
            vec![],
            Value::Null,
        );

        assert!(result.is_err());
    }

    #[test]
    fn test_create_run_with_tags() {
        let (db, _temp) = create_test_db();
        let ri = RunIndex::new(db);

        let meta = ri
            .create_run_with_options(
                "tagged-run",
                None,
                vec!["experiment".to_string(), "v1".to_string()],
                Value::Null,
            )
            .unwrap();

        assert_eq!(meta.value.tags, vec!["experiment", "v1"]);
    }

    #[test]
    fn test_get_run() {
        let (db, _temp) = create_test_db();
        let ri = RunIndex::new(db);

        let created = ri.create_run("my-run").unwrap();
        let meta = ri.get_run("my-run").unwrap().unwrap();

        assert_eq!(meta.value.name, "my-run");
        assert_eq!(meta.value.run_id, created.value.run_id);
    }

    #[test]
    fn test_get_run_not_found() {
        let (db, _temp) = create_test_db();
        let ri = RunIndex::new(db);

        let meta = ri.get_run("nonexistent").unwrap();
        assert!(meta.is_none());
    }

    #[test]
    fn test_exists() {
        let (db, _temp) = create_test_db();
        let ri = RunIndex::new(db);

        assert!(!ri.exists("my-run").unwrap());
        ri.create_run("my-run").unwrap();
        assert!(ri.exists("my-run").unwrap());
    }

    #[test]
    fn test_list_runs() {
        let (db, _temp) = create_test_db();
        let ri = RunIndex::new(db);

        ri.create_run("run-1").unwrap();
        ri.create_run("run-2").unwrap();
        ri.create_run("run-3").unwrap();

        let runs = ri.list_runs().unwrap();
        assert_eq!(runs.len(), 3);
        assert!(runs.contains(&"run-1".to_string()));
        assert!(runs.contains(&"run-2".to_string()));
        assert!(runs.contains(&"run-3".to_string()));
    }

    #[test]
    fn test_count() {
        let (db, _temp) = create_test_db();
        let ri = RunIndex::new(db);

        assert_eq!(ri.count().unwrap(), 0);
        ri.create_run("run-1").unwrap();
        assert_eq!(ri.count().unwrap(), 1);
        ri.create_run("run-2").unwrap();
        assert_eq!(ri.count().unwrap(), 2);
    }

    // ========== Story #193 Tests: Status Transitions ==========

    #[test]
    fn test_complete_run() {
        let (db, _temp) = create_test_db();
        let ri = RunIndex::new(db);

        ri.create_run("my-run").unwrap();
        let meta = ri.complete_run("my-run").unwrap();

        assert_eq!(meta.value.status, RunStatus::Completed);
        assert!(meta.value.completed_at.is_some());
    }

    #[test]
    fn test_fail_run() {
        let (db, _temp) = create_test_db();
        let ri = RunIndex::new(db);

        ri.create_run("my-run").unwrap();
        let meta = ri.fail_run("my-run", "Something went wrong").unwrap();

        assert_eq!(meta.value.status, RunStatus::Failed);
        assert_eq!(meta.value.error, Some("Something went wrong".to_string()));
        assert!(meta.value.completed_at.is_some());
    }

    #[test]
    fn test_pause_and_resume() {
        let (db, _temp) = create_test_db();
        let ri = RunIndex::new(db);

        ri.create_run("my-run").unwrap();

        let meta = ri.pause_run("my-run").unwrap();
        assert_eq!(meta.value.status, RunStatus::Paused);

        let meta = ri.resume_run("my-run").unwrap();
        assert_eq!(meta.value.status, RunStatus::Active);
    }

    #[test]
    fn test_cancel_run() {
        let (db, _temp) = create_test_db();
        let ri = RunIndex::new(db);

        ri.create_run("my-run").unwrap();
        let meta = ri.cancel_run("my-run").unwrap();

        assert_eq!(meta.value.status, RunStatus::Cancelled);
        assert!(meta.value.completed_at.is_some());
    }

    #[test]
    fn test_invalid_resurrection() {
        let (db, _temp) = create_test_db();
        let ri = RunIndex::new(db);

        ri.create_run("my-run").unwrap();
        ri.complete_run("my-run").unwrap();

        // Cannot go from Completed to Active
        let result = ri.update_status("my-run", RunStatus::Active);
        assert!(result.is_err());
    }

    #[test]
    fn test_archived_is_terminal() {
        let (db, _temp) = create_test_db();
        let ri = RunIndex::new(db);

        ri.create_run("my-run").unwrap();
        ri.archive_run("my-run").unwrap();

        // Cannot transition from Archived
        let result = ri.update_status("my-run", RunStatus::Active);
        assert!(result.is_err());
    }

    // ========== Story #194 Tests: Query Operations ==========

    #[test]
    fn test_query_by_status() {
        let (db, _temp) = create_test_db();
        let ri = RunIndex::new(db);

        ri.create_run("active-1").unwrap();
        ri.create_run("active-2").unwrap();
        ri.create_run("completed-1").unwrap();
        ri.complete_run("completed-1").unwrap();

        let active = ri.query_by_status(RunStatus::Active).unwrap();
        assert_eq!(active.len(), 2);

        let completed = ri.query_by_status(RunStatus::Completed).unwrap();
        assert_eq!(completed.len(), 1);
    }

    #[test]
    fn test_query_by_tag() {
        let (db, _temp) = create_test_db();
        let ri = RunIndex::new(db);

        ri.create_run_with_options("run-1", None, vec!["experiment".to_string()], Value::Null)
            .unwrap();
        ri.create_run_with_options(
            "run-2",
            None,
            vec!["experiment".to_string(), "v2".to_string()],
            Value::Null,
        )
        .unwrap();
        ri.create_run_with_options("run-3", None, vec!["production".to_string()], Value::Null)
            .unwrap();

        let experiments = ri.query_by_tag("experiment").unwrap();
        assert_eq!(experiments.len(), 2);

        let v2 = ri.query_by_tag("v2").unwrap();
        assert_eq!(v2.len(), 1);
    }

    #[test]
    fn test_get_child_runs() {
        let (db, _temp) = create_test_db();
        let ri = RunIndex::new(db);

        ri.create_run("parent").unwrap();
        ri.create_run_with_options("child-1", Some("parent".to_string()), vec![], Value::Null)
            .unwrap();
        ri.create_run_with_options("child-2", Some("parent".to_string()), vec![], Value::Null)
            .unwrap();

        let children = ri.get_child_runs("parent").unwrap();
        assert_eq!(children.len(), 2);
    }

    // ========== Story #195 Tests: Delete & Archive ==========

    #[test]
    fn test_archive_run() {
        let (db, _temp) = create_test_db();
        let ri = RunIndex::new(db);

        ri.create_run("my-run").unwrap();
        ri.complete_run("my-run").unwrap();
        let meta = ri.archive_run("my-run").unwrap();

        assert_eq!(meta.value.status, RunStatus::Archived);
    }

    #[test]
    fn test_delete_run() {
        let (db, _temp) = create_test_db();
        let ri = RunIndex::new(db);

        ri.create_run("my-run").unwrap();
        assert!(ri.exists("my-run").unwrap());

        ri.delete_run("my-run").unwrap();
        assert!(!ri.exists("my-run").unwrap());
    }

    #[test]
    fn test_delete_run_not_found() {
        let (db, _temp) = create_test_db();
        let ri = RunIndex::new(db);

        let result = ri.delete_run("nonexistent");
        assert!(result.is_err());
    }

    #[test]
    fn test_add_tags() {
        let (db, _temp) = create_test_db();
        let ri = RunIndex::new(db);

        ri.create_run("my-run").unwrap();
        let meta = ri
            .add_tags("my-run", vec!["tag1".to_string(), "tag2".to_string()])
            .unwrap();

        assert!(meta.value.tags.contains(&"tag1".to_string()));
        assert!(meta.value.tags.contains(&"tag2".to_string()));

        // Tags should be queryable
        let results = ri.query_by_tag("tag1").unwrap();
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn test_remove_tags() {
        let (db, _temp) = create_test_db();
        let ri = RunIndex::new(db);

        ri.create_run_with_options(
            "my-run",
            None,
            vec!["tag1".to_string(), "tag2".to_string()],
            Value::Null,
        )
        .unwrap();

        let meta = ri.remove_tags("my-run", vec!["tag1".to_string()]).unwrap();

        assert!(!meta.value.tags.contains(&"tag1".to_string()));
        assert!(meta.value.tags.contains(&"tag2".to_string()));

        // Tag1 should no longer return results
        let results = ri.query_by_tag("tag1").unwrap();
        assert_eq!(results.len(), 0);
    }

    #[test]
    fn test_update_metadata() {
        let (db, _temp) = create_test_db();
        let ri = RunIndex::new(db);

        ri.create_run("my-run").unwrap();
        let meta = ri
            .update_metadata("my-run", Value::String("custom data".to_string()))
            .unwrap();

        assert_eq!(meta.value.metadata, Value::String("custom data".to_string()));
    }

    // ========== Edge Cases ==========

    #[test]
    fn test_status_index_updates_on_transition() {
        let (db, _temp) = create_test_db();
        let ri = RunIndex::new(db);

        ri.create_run("my-run").unwrap();

        // Should be in Active index
        let active = ri.query_by_status(RunStatus::Active).unwrap();
        assert_eq!(active.len(), 1);

        // Complete the run
        ri.complete_run("my-run").unwrap();

        // Should no longer be in Active, now in Completed
        let active = ri.query_by_status(RunStatus::Active).unwrap();
        assert_eq!(active.len(), 0);

        let completed = ri.query_by_status(RunStatus::Completed).unwrap();
        assert_eq!(completed.len(), 1);
    }

    #[test]
    fn test_full_lifecycle() {
        let (db, _temp) = create_test_db();
        let ri = RunIndex::new(db);

        // Create
        let meta = ri.create_run("lifecycle-run").unwrap();
        assert_eq!(meta.value.status, RunStatus::Active);

        // Pause
        let meta = ri.pause_run("lifecycle-run").unwrap();
        assert_eq!(meta.value.status, RunStatus::Paused);

        // Resume
        let meta = ri.resume_run("lifecycle-run").unwrap();
        assert_eq!(meta.value.status, RunStatus::Active);

        // Complete
        let meta = ri.complete_run("lifecycle-run").unwrap();
        assert_eq!(meta.value.status, RunStatus::Completed);
        assert!(meta.value.completed_at.is_some());

        // Archive
        let meta = ri.archive_run("lifecycle-run").unwrap();
        assert_eq!(meta.value.status, RunStatus::Archived);

        // Cannot transition from Archived
        assert!(ri
            .update_status("lifecycle-run", RunStatus::Active)
            .is_err());
    }

    // ========== Story #196 Tests: Integration ==========

    mod integration_tests {
        use super::*;
        use crate::{EventLog, KVStore, StateCell, TraceStore, TraceType};

        #[test]
        fn test_run_lifecycle_with_primitives() {
            let (db, _temp) = create_test_db();

            let run_index = RunIndex::new(db.clone());
            let kv = KVStore::new(db.clone());
            let event_log = EventLog::new(db.clone());
            let state_cell = StateCell::new(db.clone());
            let trace_store = TraceStore::new(db.clone());

            // Create run via RunIndex
            let meta = run_index.create_run("integration-test-run").unwrap();
            let run_id = RunId::from_string(&meta.value.run_id).unwrap();

            // Use all primitives with this run
            kv.put(&run_id, "key", Value::I64(42)).unwrap();
            event_log
                .append(&run_id, "test-event", Value::Null)
                .unwrap();
            state_cell.init(&run_id, "cell", Value::Bool(true)).unwrap();
            trace_store
                .record(
                    &run_id,
                    TraceType::Thought {
                        content: "test thought".into(),
                        confidence: None,
                    },
                    vec![],
                    Value::Null,
                )
                .unwrap();

            // Verify data exists
            assert!(kv.get(&run_id, "key").unwrap().is_some());
            assert_eq!(event_log.len(&run_id).unwrap(), 1);
            assert!(state_cell.exists(&run_id, "cell").unwrap());
            assert_eq!(trace_store.count(&run_id).unwrap(), 1);

            // Delete run (cascading)
            run_index.delete_run("integration-test-run").unwrap();

            // Verify run metadata is gone
            assert!(run_index.get_run("integration-test-run").unwrap().is_none());

            // Verify all primitive data is gone (cascading delete)
            assert!(kv.get(&run_id, "key").unwrap().is_none());
            assert_eq!(event_log.len(&run_id).unwrap(), 0);
            assert!(!state_cell.exists(&run_id, "cell").unwrap());
            assert_eq!(trace_store.count(&run_id).unwrap(), 0);
        }

        #[test]
        fn test_multiple_runs_isolation() {
            let (db, _temp) = create_test_db();

            let run_index = RunIndex::new(db.clone());
            let kv = KVStore::new(db.clone());

            // Create two runs
            let meta1 = run_index.create_run("run-1").unwrap();
            let meta2 = run_index.create_run("run-2").unwrap();
            let run_id1 = RunId::from_string(&meta1.value.run_id).unwrap();
            let run_id2 = RunId::from_string(&meta2.value.run_id).unwrap();

            // Store data in both runs
            kv.put(&run_id1, "key", Value::String("run1-value".into()))
                .unwrap();
            kv.put(&run_id2, "key", Value::String("run2-value".into()))
                .unwrap();

            // Verify isolation
            // M9: get() returns Versioned<Value>
            assert_eq!(
                kv.get(&run_id1, "key").unwrap().map(|v| v.value),
                Some(Value::String("run1-value".into()))
            );
            assert_eq!(
                kv.get(&run_id2, "key").unwrap().map(|v| v.value),
                Some(Value::String("run2-value".into()))
            );

            // Delete run-1, run-2 should be unaffected
            run_index.delete_run("run-1").unwrap();

            assert!(kv.get(&run_id1, "key").unwrap().is_none());
            assert_eq!(
                kv.get(&run_id2, "key").unwrap().map(|v| v.value),
                Some(Value::String("run2-value".into()))
            );
        }

        #[test]
        fn test_run_index_with_all_primitive_types() {
            let (db, _temp) = create_test_db();

            let run_index = RunIndex::new(db.clone());
            let kv = KVStore::new(db.clone());
            let event_log = EventLog::new(db.clone());
            let state_cell = StateCell::new(db.clone());
            let trace_store = TraceStore::new(db.clone());

            // Create run with tags
            let meta = run_index
                .create_run_with_options(
                    "full-integration",
                    None,
                    vec!["production".into(), "agent-v1".into()],
                    Value::Null,
                )
                .unwrap();
            let run_id = RunId::from_string(&meta.value.run_id).unwrap();

            // KV: store multiple entries
            kv.put(&run_id, "config", Value::String("{}".into()))
                .unwrap();
            kv.put(&run_id, "state", Value::I64(0)).unwrap();

            // EventLog: append multiple events
            event_log.append(&run_id, "start", Value::Null).unwrap();
            event_log
                .append(&run_id, "action", Value::String("do_something".into()))
                .unwrap();
            event_log.append(&run_id, "end", Value::Null).unwrap();

            // StateCell: create multiple cells
            state_cell.init(&run_id, "counter", Value::I64(0)).unwrap();
            state_cell
                .init(&run_id, "flag", Value::Bool(false))
                .unwrap();

            // TraceStore: record multiple traces
            trace_store
                .record(
                    &run_id,
                    TraceType::ToolCall {
                        tool_name: "search".into(),
                        arguments: Value::Null,
                        result: None,
                        duration_ms: Some(100),
                    },
                    vec!["tool".into()],
                    Value::Null,
                )
                .unwrap();
            trace_store
                .record(
                    &run_id,
                    TraceType::Decision {
                        question: "what to do?".into(),
                        options: vec!["a".into(), "b".into()],
                        chosen: "a".into(),
                        reasoning: None,
                    },
                    vec![],
                    Value::Null,
                )
                .unwrap();

            // Verify counts before delete
            assert_eq!(event_log.len(&run_id).unwrap(), 3);
            assert_eq!(trace_store.count(&run_id).unwrap(), 2);

            // Query run by tag
            let prod_runs = run_index.query_by_tag("production").unwrap();
            assert!(prod_runs.iter().any(|r| r.run_id == meta.value.run_id));

            // Complete and archive the run
            run_index.complete_run("full-integration").unwrap();
            run_index.archive_run("full-integration").unwrap();

            // Verify run is archived
            let archived = run_index.query_by_status(RunStatus::Archived).unwrap();
            assert!(archived.iter().any(|r| r.run_id == meta.value.run_id));

            // Delete the run
            run_index.delete_run("full-integration").unwrap();

            // Verify everything is gone
            assert!(run_index.get_run("full-integration").unwrap().is_none());
            assert!(kv.get(&run_id, "config").unwrap().is_none());
            assert!(kv.get(&run_id, "state").unwrap().is_none());
            assert_eq!(event_log.len(&run_id).unwrap(), 0);
            assert!(!state_cell.exists(&run_id, "counter").unwrap());
            assert!(!state_cell.exists(&run_id, "flag").unwrap());
            assert_eq!(trace_store.count(&run_id).unwrap(), 0);

            // Verify indices are cleaned up
            let prod_runs = run_index.query_by_tag("production").unwrap();
            assert!(!prod_runs.iter().any(|r| r.run_id == meta.value.run_id));
        }
    }
}
