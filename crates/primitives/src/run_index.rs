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
use strata_durability::run_bundle::{
    filter_wal_for_run, BundleRunInfo, BundleVerifyInfo, ExportOptions, ImportedRunInfo,
    RunBundleError, RunBundleReader, RunBundleResult, RunBundleWriter, RunExportInfo,
};
use strata_durability::wal::WALEntry;
use strata_core::traits::Storage;
use strata_engine::Database;
use serde::{Deserialize, Serialize};
use std::path::Path;
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
    /// - All run-scoped data (KV, Events, States)
    ///
    /// USE WITH CAUTION - this is irreversible!
    pub fn delete_run(&self, run_id: &str) -> Result<()> {
        // First get the run metadata to know what indices to delete
        let run_meta = self
            .get_run(run_id)?
            .ok_or_else(|| Error::InvalidOperation(format!("Run '{}' not found", run_id)))?
            .value;

        // Use run_meta.run_id (the internal UUID) for namespace.
        // All primitives (KV, State, etc.) use the UUID for namespacing via Namespace::for_run().
        let actual_run_id = RunId::from_string(&run_meta.run_id).ok_or_else(|| {
            Error::InvalidOperation(format!("Invalid run UUID: {}", run_meta.run_id))
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
    /// - JSON documents
    /// - Vector entries
    fn delete_run_data_internal(&self, run_id: RunId) -> Result<()> {
        let ns = Namespace::for_run(run_id);

        // Delete data for each type tag (including deprecated Trace for backwards compatibility)
        #[allow(deprecated)]
        for type_tag in [
            TypeTag::KV,
            TypeTag::Event,
            TypeTag::State,
            TypeTag::Trace,
            TypeTag::Json,
            TypeTag::Vector,
        ] {
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

    // ========== RunBundle Export API ==========

    /// Export a terminal run as a portable bundle
    ///
    /// Exports the run to a `.runbundle.tar.zst` archive containing:
    /// - `MANIFEST.json`: Bundle metadata and checksums
    /// - `RUN.json`: Run metadata (state, tags, error)
    /// - `WAL.runlog`: Run-scoped WAL entries
    ///
    /// ## Terminal States
    ///
    /// Only runs in terminal states can be exported:
    /// - Completed
    /// - Failed
    /// - Cancelled
    /// - Archived
    ///
    /// ## Example
    ///
    /// ```ignore
    /// let run_index = RunIndex::new(db.clone());
    ///
    /// // Complete the run first
    /// run_index.complete_run("my-run")?;
    ///
    /// // Export to bundle
    /// let info = run_index.export_run("my-run", Path::new("./my-run.runbundle.tar.zst"))?;
    /// println!("Exported {} WAL entries", info.wal_entry_count);
    /// ```
    ///
    /// ## Errors
    ///
    /// - `RunNotFound`: Run doesn't exist
    /// - `NotTerminal`: Run is not in a terminal state (Active or Paused)
    pub fn export_run(&self, run_id: &str, path: &Path) -> RunBundleResult<RunExportInfo> {
        self.export_run_with_options(run_id, path, &ExportOptions::default())
    }

    /// Export a run with custom options
    ///
    /// Same as [`export_run`](Self::export_run) but with configurable options
    /// like compression level.
    pub fn export_run_with_options(
        &self,
        run_id: &str,
        path: &Path,
        options: &ExportOptions,
    ) -> RunBundleResult<RunExportInfo> {
        // 1. Get run metadata and verify terminal state
        let run_meta = self
            .get_run(run_id)
            .map_err(|e| RunBundleError::Io(std::io::Error::new(std::io::ErrorKind::Other, e.to_string())))?
            .ok_or_else(|| RunBundleError::RunNotFound(run_id.to_string()))?;

        // Check terminal state (Completed, Failed, Cancelled, Archived)
        if !is_exportable_status(&run_meta.value.status) {
            return Err(RunBundleError::NotTerminal(
                run_meta.value.status.as_str().to_string(),
            ));
        }

        // 2. Get WAL entries for this run
        let run_uuid = RunId::from_string(&run_meta.value.run_id).ok_or_else(|| {
            RunBundleError::InvalidBundle(format!("Invalid run UUID: {}", run_meta.value.run_id))
        })?;

        let wal_entries = self.get_wal_entries_for_run(&run_uuid)?;

        // 3. Build bundle components
        let run_info = metadata_to_bundle_run_info(&run_meta.value);

        // 4. Write bundle using RunBundleWriter
        let writer = RunBundleWriter::new(options);
        writer.write(&run_info, &wal_entries, path)
    }

    /// Get all WAL entries for a specific run
    fn get_wal_entries_for_run(&self, run_id: &RunId) -> RunBundleResult<Vec<WALEntry>> {
        // Lock WAL and read all entries
        let wal = self.db.wal();
        let wal_guard = wal.lock();

        let all_entries = wal_guard.read_all().map_err(|e| {
            RunBundleError::Io(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("Failed to read WAL: {}", e),
            ))
        })?;

        // Filter entries for this run
        Ok(filter_wal_for_run(&all_entries, run_id))
    }

    // ========== RunBundle Import API ==========

    /// Import a run from a bundle into this database
    ///
    /// Imports a previously exported run from a `.runbundle.tar.zst` archive.
    /// The bundle's WAL entries are replayed to restore the run's state.
    ///
    /// ## MVP Constraints
    ///
    /// - Fails if a run with the same ID already exists
    /// - Use only with fresh/empty databases for now
    /// - No conflict resolution (post-MVP feature)
    ///
    /// ## Example
    ///
    /// ```ignore
    /// let run_index = RunIndex::new(db.clone());
    ///
    /// // Import from bundle
    /// let info = run_index.import_run(Path::new("./my-run.runbundle.tar.zst"))?;
    /// println!("Imported run {} with {} WAL entries", info.run_id, info.wal_entries_replayed);
    ///
    /// // Verify the run exists
    /// let meta = run_index.get_run(&info.run_id)?;
    /// assert_eq!(meta.value.status, RunStatus::Completed);
    /// ```
    ///
    /// ## Errors
    ///
    /// - `InvalidBundle`: Bundle format is invalid or corrupted
    /// - `ChecksumMismatch`: Bundle checksums don't match
    /// - `RunAlreadyExists`: A run with the same ID already exists
    /// - `WalReplay`: Error replaying WAL entries
    pub fn import_run(&self, path: &Path) -> RunBundleResult<ImportedRunInfo> {
        // 1. Validate bundle and read contents
        let _verify_info = RunBundleReader::validate(path)?;
        let run_info = RunBundleReader::read_run_info(path)?;
        let wal_entries = RunBundleReader::read_wal_entries(path)?;

        // 2. Check run doesn't already exist (MVP: fail on conflict)
        if self.exists(&run_info.name).map_err(|e| {
            RunBundleError::Io(std::io::Error::new(
                std::io::ErrorKind::Other,
                e.to_string(),
            ))
        })? {
            return Err(RunBundleError::RunAlreadyExists(run_info.run_id.clone()));
        }

        // 3. Apply WAL entries to storage and WAL
        let entries_replayed = self.replay_imported_entries(&wal_entries)?;

        // 4. Create run metadata in RunIndex
        self.create_imported_run(&run_info)?;

        Ok(ImportedRunInfo {
            run_id: run_info.run_id,
            wal_entries_replayed: entries_replayed,
        })
    }

    // ========== RunBundle Verification API ==========

    /// Verify a bundle's integrity without importing
    ///
    /// Validates that a `.runbundle.tar.zst` archive is well-formed and intact
    /// without actually importing the run. Use this to check bundles before
    /// importing, or to verify bundles after copying/transferring them.
    ///
    /// ## Validation Checks
    ///
    /// - Archive can be decompressed
    /// - Required files exist (MANIFEST.json, RUN.json, WAL.runlog)
    /// - Format version is supported
    /// - Checksums match manifest
    /// - WAL.runlog header is valid
    ///
    /// ## Example
    ///
    /// ```ignore
    /// let run_index = RunIndex::new(db.clone());
    ///
    /// // Verify before importing
    /// let verify_info = run_index.verify_bundle(Path::new("./my-run.runbundle.tar.zst"))?;
    /// println!("Bundle contains run {} with {} WAL entries",
    ///     verify_info.run_id, verify_info.wal_entry_count);
    ///
    /// if verify_info.checksums_valid {
    ///     // Safe to import
    ///     run_index.import_run(Path::new("./my-run.runbundle.tar.zst"))?;
    /// }
    /// ```
    ///
    /// ## Errors
    ///
    /// - `InvalidBundle`: Bundle format is invalid or corrupted
    /// - `MissingFile`: Required file is missing from archive
    /// - `UnsupportedVersion`: Bundle was created with an incompatible version
    /// - `ChecksumMismatch`: File checksums don't match (returned in result, not as error)
    pub fn verify_bundle(&self, path: &Path) -> RunBundleResult<BundleVerifyInfo> {
        RunBundleReader::validate(path)
    }

    /// Replay imported WAL entries to storage and WAL
    fn replay_imported_entries(&self, entries: &[WALEntry]) -> RunBundleResult<u64> {
        let storage = self.db.storage();
        let wal = self.db.wal();

        let mut entries_applied = 0u64;

        // Group entries by transaction for proper replay
        // For MVP, we apply all committed transactions
        let mut current_txn: Option<(u64, RunId, Vec<WALEntry>)> = None;
        let mut committed_txns: Vec<(u64, RunId, Vec<WALEntry>)> = Vec::new();

        for entry in entries {
            match entry {
                WALEntry::BeginTxn { txn_id, run_id, .. } => {
                    // Start a new transaction
                    current_txn = Some((*txn_id, *run_id, Vec::new()));
                }
                WALEntry::CommitTxn { txn_id, run_id } => {
                    // Commit current transaction
                    if let Some((tid, rid, txn_entries)) = current_txn.take() {
                        if tid == *txn_id && rid == *run_id {
                            committed_txns.push((tid, rid, txn_entries));
                        }
                    }
                }
                WALEntry::AbortTxn { .. } => {
                    // Discard current transaction
                    current_txn = None;
                }
                WALEntry::Checkpoint { .. } => {
                    // Skip checkpoints during import
                }
                _ => {
                    // Add entry to current transaction
                    if let Some((_, _, ref mut txn_entries)) = current_txn {
                        txn_entries.push(entry.clone());
                    }
                }
            }
        }

        // Lock WAL for writing
        let mut wal_guard = wal.lock();

        // Apply each committed transaction
        for (_txn_id, _run_id, txn_entries) in committed_txns {
            for entry in &txn_entries {
                // Apply to storage
                match entry {
                    WALEntry::Write { key, value, version, .. } => {
                        storage
                            .put_with_version(key.clone(), value.clone(), *version, None)
                            .map_err(|e| {
                                RunBundleError::WalReplay(format!("Write failed: {}", e))
                            })?;
                        entries_applied += 1;
                    }
                    WALEntry::Delete { key, version, .. } => {
                        storage.delete_with_version(key, *version).map_err(|e| {
                            RunBundleError::WalReplay(format!("Delete failed: {}", e))
                        })?;
                        entries_applied += 1;
                    }
                    WALEntry::JsonCreate { run_id, doc_id, value_bytes, version, .. } => {
                        // For JSON, create the key and store
                        let key = Key::new_json(
                            Namespace::for_run(*run_id),
                            doc_id,
                        );
                        // Decode msgpack value
                        let json_value: serde_json::Value = rmp_serde::from_slice(value_bytes)
                            .unwrap_or(serde_json::Value::Null);
                        let core_value = json_to_value(&json_value);
                        storage
                            .put_with_version(key, core_value, *version, None)
                            .map_err(|e| {
                                RunBundleError::WalReplay(format!("JsonCreate failed: {}", e))
                            })?;
                        entries_applied += 1;
                    }
                    WALEntry::JsonSet { run_id, doc_id, value_bytes, version, .. } => {
                        let key = Key::new_json(
                            Namespace::for_run(*run_id),
                            doc_id,
                        );
                        let json_value: serde_json::Value = rmp_serde::from_slice(value_bytes)
                            .unwrap_or(serde_json::Value::Null);
                        let core_value = json_to_value(&json_value);
                        storage
                            .put_with_version(key, core_value, *version, None)
                            .map_err(|e| {
                                RunBundleError::WalReplay(format!("JsonSet failed: {}", e))
                            })?;
                        entries_applied += 1;
                    }
                    WALEntry::JsonDelete { run_id, doc_id, version, .. } => {
                        let key = Key::new_json(
                            Namespace::for_run(*run_id),
                            doc_id,
                        );
                        storage.delete_with_version(&key, *version).map_err(|e| {
                            RunBundleError::WalReplay(format!("JsonDelete failed: {}", e))
                        })?;
                        entries_applied += 1;
                    }
                    WALEntry::JsonDestroy { run_id, doc_id } => {
                        let key = Key::new_json(
                            Namespace::for_run(*run_id),
                            doc_id,
                        );
                        // Use version 0 for destroy - it's a deletion without version
                        storage.delete_with_version(&key, 0).map_err(|e| {
                            RunBundleError::WalReplay(format!("JsonDestroy failed: {}", e))
                        })?;
                        entries_applied += 1;
                    }
                    // Vector operations - skip for MVP (complex in-memory state)
                    WALEntry::VectorCollectionCreate { .. } |
                    WALEntry::VectorCollectionDelete { .. } |
                    WALEntry::VectorUpsert { .. } |
                    WALEntry::VectorDelete { .. } => {
                        // Vector operations require rebuilding in-memory indices
                        // Skip for MVP - vectors need special handling
                        entries_applied += 1;
                    }
                    _ => {}
                }

                // Also write to WAL for durability
                wal_guard.append(entry).map_err(|e| {
                    RunBundleError::WalReplay(format!("WAL append failed: {}", e))
                })?;
            }
        }

        // Flush WAL
        wal_guard.flush().map_err(|e| {
            RunBundleError::WalReplay(format!("WAL flush failed: {}", e))
        })?;

        Ok(entries_applied)
    }

    /// Create run metadata from imported bundle info
    ///
    /// This directly creates the RunMetadata with the original run_id from the bundle,
    /// preserving identity across export/import.
    fn create_imported_run(&self, info: &BundleRunInfo) -> RunBundleResult<()> {
        // Convert state to RunStatus
        let status = match info.state.as_str() {
            "completed" => RunStatus::Completed,
            "failed" => RunStatus::Failed,
            "cancelled" => RunStatus::Cancelled,
            "archived" => RunStatus::Archived,
            _ => RunStatus::Completed,
        };

        // Convert serde_json::Value metadata to core::Value
        let metadata = json_to_value(&info.metadata);

        // Create RunMetadata directly with the original run_id from bundle
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as i64;

        let run_meta = RunMetadata {
            name: info.name.clone(),
            run_id: info.run_id.clone(), // Preserve original run_id!
            parent_run: info.parent_run_id.clone(),
            status,
            created_at: now, // Use current time for imported run
            updated_at: now,
            completed_at: Some(now), // Terminal runs have completed_at
            tags: info.tags.clone(),
            metadata,
            error: info.error.clone(),
            version: 1,
        };

        // Store the run metadata directly
        self.db
            .transaction(global_run_id(), |txn| {
                let key = self.key_for(&info.name);

                // Double-check run doesn't exist
                if txn.get(&key)?.is_some() {
                    return Err(Error::InvalidOperation(format!(
                        "Run '{}' already exists",
                        info.name
                    )));
                }

                txn.put(key, to_stored_value(&run_meta))?;

                // Write indices
                Self::write_indices_internal(txn, &run_meta)?;

                Ok(())
            })
            .map_err(|e| RunBundleError::WalReplay(format!("Failed to create run: {}", e)))?;

        Ok(())
    }
}

/// Check if a run status is exportable (terminal)
fn is_exportable_status(status: &RunStatus) -> bool {
    matches!(
        status,
        RunStatus::Completed | RunStatus::Failed | RunStatus::Cancelled | RunStatus::Archived
    )
}

/// Convert RunMetadata to BundleRunInfo for export
fn metadata_to_bundle_run_info(meta: &RunMetadata) -> BundleRunInfo {
    let state_str = match meta.status {
        RunStatus::Completed => "completed",
        RunStatus::Failed => "failed",
        RunStatus::Cancelled => "cancelled",
        RunStatus::Archived => "archived",
        RunStatus::Active => "active",     // Should not happen for export
        RunStatus::Paused => "paused",     // Should not happen for export
    };

    // Convert timestamps from millis to ISO 8601
    let created_at = format_timestamp_iso8601(meta.created_at);
    let closed_at = meta
        .completed_at
        .map(format_timestamp_iso8601)
        .unwrap_or_else(|| created_at.clone());

    // Convert metadata from core::Value to serde_json::Value
    let metadata_json = value_to_json(&meta.metadata);

    BundleRunInfo {
        run_id: meta.run_id.clone(),
        name: meta.name.clone(),
        state: state_str.to_string(),
        created_at,
        closed_at,
        parent_run_id: meta.parent_run.clone(),
        tags: meta.tags.clone(),
        metadata: metadata_json,
        error: meta.error.clone(),
    }
}

/// Format a timestamp (milliseconds since epoch) as ISO 8601
fn format_timestamp_iso8601(millis: i64) -> String {
    let secs = (millis / 1000) as u64;

    // Calculate date components
    let days = secs / 86400;
    let time_secs = secs % 86400;
    let hours = time_secs / 3600;
    let minutes = (time_secs % 3600) / 60;
    let seconds = time_secs % 60;

    // Approximate year/month/day calculation
    let years = 1970 + (days / 365);
    let day_of_year = days % 365;
    let month = (day_of_year / 30).min(11) + 1;
    let day = (day_of_year % 30) + 1;

    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        years, month, day, hours, minutes, seconds
    )
}

/// Convert core::Value to serde_json::Value
fn value_to_json(value: &Value) -> serde_json::Value {
    match value {
        Value::Null => serde_json::Value::Null,
        Value::Bool(b) => serde_json::Value::Bool(*b),
        Value::Int(i) => serde_json::Value::Number((*i).into()),
        Value::Float(f) => serde_json::json!(*f),
        Value::String(s) => serde_json::Value::String(s.clone()),
        Value::Bytes(b) => {
            // Encode bytes as base64
            use base64::Engine;
            let encoded = base64::engine::general_purpose::STANDARD.encode(b);
            serde_json::Value::String(encoded)
        }
        Value::Array(arr) => {
            serde_json::Value::Array(arr.iter().map(value_to_json).collect())
        }
        Value::Object(map) => {
            let obj: serde_json::Map<String, serde_json::Value> = map
                .iter()
                .map(|(k, v)| (k.clone(), value_to_json(v)))
                .collect();
            serde_json::Value::Object(obj)
        }
    }
}

/// Convert serde_json::Value to core::Value
fn json_to_value(json: &serde_json::Value) -> Value {
    match json {
        serde_json::Value::Null => Value::Null,
        serde_json::Value::Bool(b) => Value::Bool(*b),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Value::Int(i)
            } else if let Some(f) = n.as_f64() {
                Value::Float(f)
            } else {
                Value::Null
            }
        }
        serde_json::Value::String(s) => Value::String(s.clone()),
        serde_json::Value::Array(arr) => {
            Value::Array(arr.iter().map(json_to_value).collect())
        }
        serde_json::Value::Object(map) => {
            let obj: std::collections::HashMap<String, Value> = map
                .iter()
                .map(|(k, v)| (k.clone(), json_to_value(v)))
                .collect();
            Value::Object(obj)
        }
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

    /// Helper to create an empty object payload for EventLog tests
    fn empty_event_payload() -> Value {
        Value::Object(std::collections::HashMap::new())
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
        use crate::{EventLog, KVStore, StateCell};

        #[test]
        fn test_run_lifecycle_with_primitives() {
            let (db, _temp) = create_test_db();

            let run_index = RunIndex::new(db.clone());
            let kv = KVStore::new(db.clone());
            let event_log = EventLog::new(db.clone());
            let state_cell = StateCell::new(db.clone());

            // Create run via RunIndex
            let meta = run_index.create_run("integration-test-run").unwrap();
            let run_id = RunId::from_string(&meta.value.run_id).unwrap();

            // Use all primitives with this run
            kv.put(&run_id, "key", Value::Int(42)).unwrap();
            event_log
                .append(&run_id, "test-event", empty_event_payload())
                .unwrap();
            state_cell.init(&run_id, "cell", Value::Bool(true)).unwrap();

            // Verify data exists
            assert!(kv.get(&run_id, "key").unwrap().is_some());
            assert_eq!(event_log.len(&run_id).unwrap(), 1);
            assert!(state_cell.exists(&run_id, "cell").unwrap());

            // Delete run (cascading)
            run_index.delete_run("integration-test-run").unwrap();

            // Verify run metadata is gone
            assert!(run_index.get_run("integration-test-run").unwrap().is_none());

            // Verify all primitive data is gone (cascading delete)
            assert!(kv.get(&run_id, "key").unwrap().is_none());
            assert_eq!(event_log.len(&run_id).unwrap(), 0);
            assert!(!state_cell.exists(&run_id, "cell").unwrap());
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
            kv.put(&run_id, "state", Value::Int(0)).unwrap();

            // EventLog: append multiple events
            event_log.append(&run_id, "start", empty_event_payload()).unwrap();
            event_log
                .append(&run_id, "action", Value::Object(std::collections::HashMap::from([
                    ("action".to_string(), Value::String("do_something".into()))
                ])))
                .unwrap();
            event_log.append(&run_id, "end", empty_event_payload()).unwrap();

            // StateCell: create multiple cells
            state_cell.init(&run_id, "counter", Value::Int(0)).unwrap();
            state_cell
                .init(&run_id, "flag", Value::Bool(false))
                .unwrap();

            // Verify counts before delete
            assert_eq!(event_log.len(&run_id).unwrap(), 3);

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

            // Verify indices are cleaned up
            let prod_runs = run_index.query_by_tag("production").unwrap();
            assert!(!prod_runs.iter().any(|r| r.run_id == meta.value.run_id));
        }
    }

    // ========== RunBundle Export Tests ==========

    mod export_tests {
        use super::*;
        use crate::KVStore;
        use strata_durability::run_bundle::{RunBundleError, RunBundleReader};

        #[test]
        fn test_export_completed_run() {
            let (db, temp) = create_test_db();
            let run_index = RunIndex::new(db.clone());
            let kv = KVStore::new(db.clone());

            // Create and complete a run with some data
            let meta = run_index.create_run("export-test").unwrap();
            let run_id = RunId::from_string(&meta.value.run_id).unwrap();

            // Add some data
            kv.put(&run_id, "key1", Value::String("value1".into())).unwrap();
            kv.put(&run_id, "key2", Value::Int(42)).unwrap();

            // Complete the run
            run_index.complete_run("export-test").unwrap();

            // Export
            let bundle_path = temp.path().join("export-test.runbundle.tar.zst");
            let info = run_index.export_run("export-test", &bundle_path).unwrap();

            assert_eq!(info.run_id, meta.value.run_id);
            assert!(info.bundle_size_bytes > 0);
            assert!(bundle_path.exists());
        }

        #[test]
        fn test_export_failed_run_includes_error() {
            let (db, temp) = create_test_db();
            let run_index = RunIndex::new(db.clone());

            // Create and fail a run
            run_index.create_run("failed-test").unwrap();
            run_index.fail_run("failed-test", "Connection timeout").unwrap();

            // Export
            let bundle_path = temp.path().join("failed-test.runbundle.tar.zst");
            let _info = run_index.export_run("failed-test", &bundle_path).unwrap();

            // Verify error is in the bundle
            let run_info = RunBundleReader::read_run_info(&bundle_path).unwrap();
            assert_eq!(run_info.state, "failed");
            assert_eq!(run_info.error, Some("Connection timeout".to_string()));
        }

        #[test]
        fn test_export_cancelled_run() {
            let (db, temp) = create_test_db();
            let run_index = RunIndex::new(db.clone());

            // Create and cancel a run
            run_index.create_run("cancelled-test").unwrap();
            run_index.cancel_run("cancelled-test").unwrap();

            // Export should succeed
            let bundle_path = temp.path().join("cancelled-test.runbundle.tar.zst");
            let _info = run_index.export_run("cancelled-test", &bundle_path).unwrap();

            let run_info = RunBundleReader::read_run_info(&bundle_path).unwrap();
            assert_eq!(run_info.state, "cancelled");
        }

        #[test]
        fn test_export_archived_run() {
            let (db, temp) = create_test_db();
            let run_index = RunIndex::new(db.clone());

            // Create, complete, then archive a run
            run_index.create_run("archived-test").unwrap();
            run_index.complete_run("archived-test").unwrap();
            run_index.archive_run("archived-test").unwrap();

            // Export should succeed
            let bundle_path = temp.path().join("archived-test.runbundle.tar.zst");
            let _info = run_index.export_run("archived-test", &bundle_path).unwrap();

            let run_info = RunBundleReader::read_run_info(&bundle_path).unwrap();
            assert_eq!(run_info.state, "archived");
        }

        #[test]
        fn test_export_rejects_active_run() {
            let (db, temp) = create_test_db();
            let run_index = RunIndex::new(db.clone());

            // Create a run but don't complete it
            run_index.create_run("active-test").unwrap();

            // Export should fail
            let bundle_path = temp.path().join("active-test.runbundle.tar.zst");
            let result = run_index.export_run("active-test", &bundle_path);

            assert!(result.is_err());
            match result.unwrap_err() {
                RunBundleError::NotTerminal(state) => {
                    assert_eq!(state, "Active");
                }
                e => panic!("Expected NotTerminal error, got: {:?}", e),
            }
        }

        #[test]
        fn test_export_rejects_paused_run() {
            let (db, temp) = create_test_db();
            let run_index = RunIndex::new(db.clone());

            // Create and pause a run
            run_index.create_run("paused-test").unwrap();
            run_index.pause_run("paused-test").unwrap();

            // Export should fail
            let bundle_path = temp.path().join("paused-test.runbundle.tar.zst");
            let result = run_index.export_run("paused-test", &bundle_path);

            assert!(result.is_err());
            match result.unwrap_err() {
                RunBundleError::NotTerminal(state) => {
                    assert_eq!(state, "Paused");
                }
                e => panic!("Expected NotTerminal error, got: {:?}", e),
            }
        }

        #[test]
        fn test_export_rejects_nonexistent_run() {
            let (db, temp) = create_test_db();
            let run_index = RunIndex::new(db.clone());

            let bundle_path = temp.path().join("nonexistent.runbundle.tar.zst");
            let result = run_index.export_run("nonexistent-run", &bundle_path);

            assert!(result.is_err());
            match result.unwrap_err() {
                RunBundleError::RunNotFound(id) => {
                    assert_eq!(id, "nonexistent-run");
                }
                e => panic!("Expected RunNotFound error, got: {:?}", e),
            }
        }

        #[test]
        fn test_export_with_tags() {
            let (db, temp) = create_test_db();
            let run_index = RunIndex::new(db.clone());

            // Create run with tags
            run_index.create_run_with_options(
                "tagged-export",
                None,
                vec!["production".to_string(), "v2.0".to_string()],
                Value::Null,
            ).unwrap();
            run_index.complete_run("tagged-export").unwrap();

            // Export
            let bundle_path = temp.path().join("tagged-export.runbundle.tar.zst");
            run_index.export_run("tagged-export", &bundle_path).unwrap();

            // Verify tags are preserved
            let run_info = RunBundleReader::read_run_info(&bundle_path).unwrap();
            assert!(run_info.tags.contains(&"production".to_string()));
            assert!(run_info.tags.contains(&"v2.0".to_string()));
        }

        #[test]
        fn test_export_bundle_is_verifiable() {
            let (db, temp) = create_test_db();
            let run_index = RunIndex::new(db.clone());
            let kv = KVStore::new(db.clone());

            // Create run with data
            let meta = run_index.create_run("verify-test").unwrap();
            let run_id = RunId::from_string(&meta.value.run_id).unwrap();
            kv.put(&run_id, "test", Value::String("data".into())).unwrap();
            run_index.complete_run("verify-test").unwrap();

            // Export
            let bundle_path = temp.path().join("verify-test.runbundle.tar.zst");
            run_index.export_run("verify-test", &bundle_path).unwrap();

            // Verify bundle integrity
            let verify_info = RunBundleReader::validate(&bundle_path).unwrap();
            assert!(verify_info.checksums_valid);
        }
    }

    // ========== RunBundle Import Tests ==========

    mod import_tests {
        use super::*;
        use crate::KVStore;
        use strata_durability::run_bundle::RunBundleError;

        #[test]
        fn test_import_into_empty_database() {
            // Create source database with data
            let (source_db, source_temp) = create_test_db();
            let source_run_index = RunIndex::new(source_db.clone());
            let source_kv = KVStore::new(source_db.clone());

            let meta = source_run_index.create_run("import-test").unwrap();
            let run_id = RunId::from_string(&meta.value.run_id).unwrap();

            source_kv.put(&run_id, "key1", Value::String("value1".into())).unwrap();
            source_kv.put(&run_id, "key2", Value::Int(42)).unwrap();
            source_run_index.complete_run("import-test").unwrap();

            // Export from source
            let bundle_path = source_temp.path().join("import-test.runbundle.tar.zst");
            source_run_index.export_run("import-test", &bundle_path).unwrap();

            // Create target database (empty)
            let (target_db, _target_temp) = create_test_db();
            let target_run_index = RunIndex::new(target_db.clone());
            let target_kv = KVStore::new(target_db.clone());

            // Import
            let info = target_run_index.import_run(&bundle_path).unwrap();
            assert_eq!(info.run_id, meta.value.run_id);

            // Verify run exists in target
            let imported_meta = target_run_index.get_run("import-test").unwrap().unwrap();
            assert_eq!(imported_meta.value.status, RunStatus::Completed);

            // Verify data exists in target
            let target_run_id = RunId::from_string(&imported_meta.value.run_id).unwrap();
            let val1 = target_kv.get(&target_run_id, "key1").unwrap().unwrap();
            assert_eq!(val1.value, Value::String("value1".into()));

            let val2 = target_kv.get(&target_run_id, "key2").unwrap().unwrap();
            assert_eq!(val2.value, Value::Int(42));
        }

        #[test]
        fn test_import_fails_if_run_exists() {
            // Create source database with data
            let (source_db, source_temp) = create_test_db();
            let source_run_index = RunIndex::new(source_db.clone());

            source_run_index.create_run("duplicate-run").unwrap();
            source_run_index.complete_run("duplicate-run").unwrap();

            let bundle_path = source_temp.path().join("duplicate.runbundle.tar.zst");
            source_run_index.export_run("duplicate-run", &bundle_path).unwrap();

            // Create target database with same run name already existing
            let (target_db, _target_temp) = create_test_db();
            let target_run_index = RunIndex::new(target_db.clone());

            // Create a run with the same name
            target_run_index.create_run("duplicate-run").unwrap();

            // Import should fail
            let result = target_run_index.import_run(&bundle_path);
            assert!(result.is_err());
            match result.unwrap_err() {
                RunBundleError::RunAlreadyExists(_) => {}
                e => panic!("Expected RunAlreadyExists error, got: {:?}", e),
            }
        }

        #[test]
        fn test_import_preserves_failed_run_error() {
            let (source_db, source_temp) = create_test_db();
            let source_run_index = RunIndex::new(source_db.clone());

            source_run_index.create_run("failed-import").unwrap();
            source_run_index.fail_run("failed-import", "Network error").unwrap();

            let bundle_path = source_temp.path().join("failed.runbundle.tar.zst");
            source_run_index.export_run("failed-import", &bundle_path).unwrap();

            // Import into fresh database
            let (target_db, _target_temp) = create_test_db();
            let target_run_index = RunIndex::new(target_db.clone());

            target_run_index.import_run(&bundle_path).unwrap();

            // Verify error is preserved
            let imported = target_run_index.get_run("failed-import").unwrap().unwrap();
            assert_eq!(imported.value.status, RunStatus::Failed);
            assert_eq!(imported.value.error, Some("Network error".to_string()));
        }

        #[test]
        fn test_import_preserves_tags() {
            let (source_db, source_temp) = create_test_db();
            let source_run_index = RunIndex::new(source_db.clone());

            source_run_index.create_run_with_options(
                "tagged-import",
                None,
                vec!["prod".to_string(), "v1.0".to_string()],
                Value::Null,
            ).unwrap();
            source_run_index.complete_run("tagged-import").unwrap();

            let bundle_path = source_temp.path().join("tagged.runbundle.tar.zst");
            source_run_index.export_run("tagged-import", &bundle_path).unwrap();

            // Import
            let (target_db, _target_temp) = create_test_db();
            let target_run_index = RunIndex::new(target_db.clone());

            target_run_index.import_run(&bundle_path).unwrap();

            // Verify tags
            let imported = target_run_index.get_run("tagged-import").unwrap().unwrap();
            assert!(imported.value.tags.contains(&"prod".to_string()));
            assert!(imported.value.tags.contains(&"v1.0".to_string()));
        }

        #[test]
        fn test_import_cancelled_run() {
            let (source_db, source_temp) = create_test_db();
            let source_run_index = RunIndex::new(source_db.clone());

            source_run_index.create_run("cancelled-import").unwrap();
            source_run_index.cancel_run("cancelled-import").unwrap();

            let bundle_path = source_temp.path().join("cancelled.runbundle.tar.zst");
            source_run_index.export_run("cancelled-import", &bundle_path).unwrap();

            // Import
            let (target_db, _target_temp) = create_test_db();
            let target_run_index = RunIndex::new(target_db.clone());

            target_run_index.import_run(&bundle_path).unwrap();

            let imported = target_run_index.get_run("cancelled-import").unwrap().unwrap();
            assert_eq!(imported.value.status, RunStatus::Cancelled);
        }

        #[test]
        fn test_import_archived_run() {
            let (source_db, source_temp) = create_test_db();
            let source_run_index = RunIndex::new(source_db.clone());

            source_run_index.create_run("archived-import").unwrap();
            source_run_index.complete_run("archived-import").unwrap();
            source_run_index.archive_run("archived-import").unwrap();

            let bundle_path = source_temp.path().join("archived.runbundle.tar.zst");
            source_run_index.export_run("archived-import", &bundle_path).unwrap();

            // Import
            let (target_db, _target_temp) = create_test_db();
            let target_run_index = RunIndex::new(target_db.clone());

            target_run_index.import_run(&bundle_path).unwrap();

            let imported = target_run_index.get_run("archived-import").unwrap().unwrap();
            assert_eq!(imported.value.status, RunStatus::Archived);
        }

        #[test]
        fn test_round_trip_preserves_data() {
            // Full round-trip test: create data -> export -> import -> verify
            let (source_db, source_temp) = create_test_db();
            let source_run_index = RunIndex::new(source_db.clone());
            let source_kv = KVStore::new(source_db.clone());

            // Create run with various data types
            let meta = source_run_index.create_run_with_options(
                "round-trip",
                None,
                vec!["test".to_string()],
                Value::Object([("key".to_string(), Value::String("value".into()))].into_iter().collect()),
            ).unwrap();
            let run_id = RunId::from_string(&meta.value.run_id).unwrap();

            // Add KV data
            source_kv.put(&run_id, "string", Value::String("hello".into())).unwrap();
            source_kv.put(&run_id, "int", Value::Int(12345)).unwrap();
            source_kv.put(&run_id, "bool", Value::Bool(true)).unwrap();
            source_kv.put(&run_id, "float", Value::Float(3.14)).unwrap();
            source_kv.put(&run_id, "null", Value::Null).unwrap();

            source_run_index.complete_run("round-trip").unwrap();

            // Export
            let bundle_path = source_temp.path().join("round-trip.runbundle.tar.zst");
            let export_info = source_run_index.export_run("round-trip", &bundle_path).unwrap();

            // Import into fresh database
            let (target_db, _target_temp) = create_test_db();
            let target_run_index = RunIndex::new(target_db.clone());
            let target_kv = KVStore::new(target_db.clone());

            let import_info = target_run_index.import_run(&bundle_path).unwrap();

            // Verify import info
            assert_eq!(import_info.run_id, export_info.run_id);

            // Verify run metadata
            let imported = target_run_index.get_run("round-trip").unwrap().unwrap();
            assert_eq!(imported.value.status, RunStatus::Completed);
            assert!(imported.value.tags.contains(&"test".to_string()));

            // Verify KV data
            let target_run_id = RunId::from_string(&imported.value.run_id).unwrap();

            assert_eq!(
                target_kv.get(&target_run_id, "string").unwrap().unwrap().value,
                Value::String("hello".into())
            );
            assert_eq!(
                target_kv.get(&target_run_id, "int").unwrap().unwrap().value,
                Value::Int(12345)
            );
            assert_eq!(
                target_kv.get(&target_run_id, "bool").unwrap().unwrap().value,
                Value::Bool(true)
            );
            // Float comparison
            if let Value::Float(f) = target_kv.get(&target_run_id, "float").unwrap().unwrap().value {
                assert!((f - 3.14).abs() < 0.001);
            } else {
                panic!("Expected Float");
            }
            assert_eq!(
                target_kv.get(&target_run_id, "null").unwrap().unwrap().value,
                Value::Null
            );
        }

        // ========== Stronger Tests ==========

        #[test]
        fn test_import_corrupted_bundle_fails() {
            let (db, temp) = create_test_db();
            let run_index = RunIndex::new(db.clone());

            // Create a file that looks like a bundle but is corrupted
            let bundle_path = temp.path().join("corrupted.runbundle.tar.zst");
            std::fs::write(&bundle_path, b"not a valid tar.zst file").unwrap();

            let result = run_index.import_run(&bundle_path);
            assert!(result.is_err(), "Import of corrupted bundle should fail");
        }

        #[test]
        fn test_import_nonexistent_bundle_fails() {
            let (db, temp) = create_test_db();
            let run_index = RunIndex::new(db.clone());

            let bundle_path = temp.path().join("nonexistent.runbundle.tar.zst");

            let result = run_index.import_run(&bundle_path);
            assert!(result.is_err(), "Import of nonexistent bundle should fail");
        }

        #[test]
        fn test_export_import_empty_run() {
            // Edge case: run with no data
            let (source_db, source_temp) = create_test_db();
            let source_run_index = RunIndex::new(source_db.clone());

            source_run_index.create_run("empty-run").unwrap();
            source_run_index.complete_run("empty-run").unwrap();

            let bundle_path = source_temp.path().join("empty.runbundle.tar.zst");
            let export_info = source_run_index.export_run("empty-run", &bundle_path).unwrap();

            // Should have 0 WAL entries (run metadata is separate)
            assert_eq!(export_info.wal_entry_count, 0);

            // Import should still work
            let (target_db, _target_temp) = create_test_db();
            let target_run_index = RunIndex::new(target_db.clone());

            let import_info = target_run_index.import_run(&bundle_path).unwrap();
            assert_eq!(import_info.wal_entries_replayed, 0);

            // Run should exist with correct state
            let imported = target_run_index.get_run("empty-run").unwrap().unwrap();
            assert_eq!(imported.value.status, RunStatus::Completed);
        }

        #[test]
        fn test_export_import_binary_data() {
            let (source_db, source_temp) = create_test_db();
            let source_run_index = RunIndex::new(source_db.clone());
            let source_kv = KVStore::new(source_db.clone());

            let meta = source_run_index.create_run("binary-test").unwrap();
            let run_id = RunId::from_string(&meta.value.run_id).unwrap();

            // Store binary data with various byte patterns
            let binary_data = vec![0u8, 1, 2, 255, 254, 128, 0, 0, 127];
            source_kv.put(&run_id, "binary", Value::Bytes(binary_data.clone())).unwrap();

            source_run_index.complete_run("binary-test").unwrap();

            let bundle_path = source_temp.path().join("binary.runbundle.tar.zst");
            source_run_index.export_run("binary-test", &bundle_path).unwrap();

            // Import
            let (target_db, _target_temp) = create_test_db();
            let target_run_index = RunIndex::new(target_db.clone());
            let target_kv = KVStore::new(target_db.clone());

            target_run_index.import_run(&bundle_path).unwrap();

            let imported = target_run_index.get_run("binary-test").unwrap().unwrap();
            let target_run_id = RunId::from_string(&imported.value.run_id).unwrap();

            // Verify binary data is preserved exactly
            let retrieved = target_kv.get(&target_run_id, "binary").unwrap().unwrap();
            assert_eq!(retrieved.value, Value::Bytes(binary_data));
        }

        #[test]
        fn test_export_import_complex_nested_data() {
            let (source_db, source_temp) = create_test_db();
            let source_run_index = RunIndex::new(source_db.clone());
            let source_kv = KVStore::new(source_db.clone());

            let meta = source_run_index.create_run("nested-test").unwrap();
            let run_id = RunId::from_string(&meta.value.run_id).unwrap();

            // Create deeply nested structure
            let nested = Value::Object([
                ("level1".to_string(), Value::Object([
                    ("level2".to_string(), Value::Array(vec![
                        Value::Int(1),
                        Value::String("two".into()),
                        Value::Object([
                            ("level3".to_string(), Value::Bool(true))
                        ].into_iter().collect()),
                    ])),
                ].into_iter().collect())),
            ].into_iter().collect());

            source_kv.put(&run_id, "nested", nested.clone()).unwrap();

            // Also test array at top level
            let array = Value::Array(vec![
                Value::Int(1),
                Value::Int(2),
                Value::Int(3),
            ]);
            source_kv.put(&run_id, "array", array.clone()).unwrap();

            source_run_index.complete_run("nested-test").unwrap();

            let bundle_path = source_temp.path().join("nested.runbundle.tar.zst");
            source_run_index.export_run("nested-test", &bundle_path).unwrap();

            // Import
            let (target_db, _target_temp) = create_test_db();
            let target_run_index = RunIndex::new(target_db.clone());
            let target_kv = KVStore::new(target_db.clone());

            target_run_index.import_run(&bundle_path).unwrap();

            let imported = target_run_index.get_run("nested-test").unwrap().unwrap();
            let target_run_id = RunId::from_string(&imported.value.run_id).unwrap();

            // Verify nested structure is preserved
            assert_eq!(
                target_kv.get(&target_run_id, "nested").unwrap().unwrap().value,
                nested
            );
            assert_eq!(
                target_kv.get(&target_run_id, "array").unwrap().unwrap().value,
                array
            );
        }

        #[test]
        fn test_import_preserves_run_id_identity() {
            // Verify the run_id in the imported run matches the original exactly
            let (source_db, source_temp) = create_test_db();
            let source_run_index = RunIndex::new(source_db.clone());

            let meta = source_run_index.create_run("identity-test").unwrap();
            let original_run_id = meta.value.run_id.clone();

            source_run_index.complete_run("identity-test").unwrap();

            let bundle_path = source_temp.path().join("identity.runbundle.tar.zst");
            source_run_index.export_run("identity-test", &bundle_path).unwrap();

            // Import
            let (target_db, _target_temp) = create_test_db();
            let target_run_index = RunIndex::new(target_db.clone());

            let import_info = target_run_index.import_run(&bundle_path).unwrap();

            // The run_id should be EXACTLY the same
            assert_eq!(import_info.run_id, original_run_id);

            let imported = target_run_index.get_run("identity-test").unwrap().unwrap();
            assert_eq!(imported.value.run_id, original_run_id);
        }

        #[test]
        fn test_export_captures_snapshot_at_export_time() {
            // Data written after export should NOT be in the bundle
            let (db, temp) = create_test_db();
            let run_index = RunIndex::new(db.clone());
            let kv = KVStore::new(db.clone());

            let meta = run_index.create_run("snapshot-test").unwrap();
            let run_id = RunId::from_string(&meta.value.run_id).unwrap();

            // Write initial data
            kv.put(&run_id, "before", Value::String("exported".into())).unwrap();

            run_index.complete_run("snapshot-test").unwrap();

            // Export
            let bundle_path = temp.path().join("snapshot.runbundle.tar.zst");
            run_index.export_run("snapshot-test", &bundle_path).unwrap();

            // Note: We can't write after completing, but we can verify the bundle
            // contains exactly what was there at export time by importing

            let (target_db, _target_temp) = create_test_db();
            let target_run_index = RunIndex::new(target_db.clone());
            let target_kv = KVStore::new(target_db.clone());

            target_run_index.import_run(&bundle_path).unwrap();

            let imported = target_run_index.get_run("snapshot-test").unwrap().unwrap();
            let target_run_id = RunId::from_string(&imported.value.run_id).unwrap();

            // Should have the data that was there at export
            assert_eq!(
                target_kv.get(&target_run_id, "before").unwrap().unwrap().value,
                Value::String("exported".into())
            );
        }

        #[test]
        fn test_export_import_multiple_keys() {
            // Test with many keys to ensure no truncation/loss
            let (source_db, source_temp) = create_test_db();
            let source_run_index = RunIndex::new(source_db.clone());
            let source_kv = KVStore::new(source_db.clone());

            let meta = source_run_index.create_run("many-keys").unwrap();
            let run_id = RunId::from_string(&meta.value.run_id).unwrap();

            // Write 100 keys
            for i in 0..100 {
                source_kv.put(&run_id, &format!("key_{:03}", i), Value::Int(i)).unwrap();
            }

            source_run_index.complete_run("many-keys").unwrap();

            let bundle_path = source_temp.path().join("many-keys.runbundle.tar.zst");
            let export_info = source_run_index.export_run("many-keys", &bundle_path).unwrap();

            // Each put generates 3 WAL entries: BeginTxn, Write, CommitTxn
            // So 100 puts = 300 WAL entries
            assert_eq!(export_info.wal_entry_count, 300);

            // Import
            let (target_db, _target_temp) = create_test_db();
            let target_run_index = RunIndex::new(target_db.clone());
            let target_kv = KVStore::new(target_db.clone());

            let import_info = target_run_index.import_run(&bundle_path).unwrap();
            // entries_replayed counts actual Write operations applied, which is 100
            assert_eq!(import_info.wal_entries_replayed, 100);

            let imported = target_run_index.get_run("many-keys").unwrap().unwrap();
            let target_run_id = RunId::from_string(&imported.value.run_id).unwrap();

            // Verify ALL 100 keys exist with correct values
            for i in 0..100 {
                let key = format!("key_{:03}", i);
                let val = target_kv.get(&target_run_id, &key).unwrap()
                    .unwrap_or_else(|| panic!("Missing key: {}", key));
                assert_eq!(val.value, Value::Int(i), "Wrong value for key: {}", key);
            }
        }

        #[test]
        fn test_export_import_overwrites_and_deletes() {
            // Test that overwrites and deletes are properly captured
            let (source_db, source_temp) = create_test_db();
            let source_run_index = RunIndex::new(source_db.clone());
            let source_kv = KVStore::new(source_db.clone());

            let meta = source_run_index.create_run("overwrite-test").unwrap();
            let run_id = RunId::from_string(&meta.value.run_id).unwrap();

            // Write, overwrite, delete pattern
            source_kv.put(&run_id, "key1", Value::Int(1)).unwrap();
            source_kv.put(&run_id, "key1", Value::Int(2)).unwrap();  // Overwrite
            source_kv.put(&run_id, "key1", Value::Int(3)).unwrap();  // Overwrite again

            source_kv.put(&run_id, "key2", Value::String("delete-me".into())).unwrap();
            source_kv.delete(&run_id, "key2").unwrap();  // Delete

            source_kv.put(&run_id, "key3", Value::Bool(true)).unwrap();

            source_run_index.complete_run("overwrite-test").unwrap();

            let bundle_path = source_temp.path().join("overwrite.runbundle.tar.zst");
            source_run_index.export_run("overwrite-test", &bundle_path).unwrap();

            // Import
            let (target_db, _target_temp) = create_test_db();
            let target_run_index = RunIndex::new(target_db.clone());
            let target_kv = KVStore::new(target_db.clone());

            target_run_index.import_run(&bundle_path).unwrap();

            let imported = target_run_index.get_run("overwrite-test").unwrap().unwrap();
            let target_run_id = RunId::from_string(&imported.value.run_id).unwrap();

            // key1 should have final value (3)
            assert_eq!(
                target_kv.get(&target_run_id, "key1").unwrap().unwrap().value,
                Value::Int(3)
            );

            // key2 should be deleted (not exist)
            assert!(target_kv.get(&target_run_id, "key2").unwrap().is_none());

            // key3 should exist
            assert_eq!(
                target_kv.get(&target_run_id, "key3").unwrap().unwrap().value,
                Value::Bool(true)
            );
        }

        #[test]
        fn test_import_does_not_affect_other_runs() {
            // Ensure importing doesn't corrupt existing data
            let (target_db, target_temp) = create_test_db();
            let target_run_index = RunIndex::new(target_db.clone());
            let target_kv = KVStore::new(target_db.clone());

            // Create existing run with data
            let existing = target_run_index.create_run("existing-run").unwrap();
            let existing_run_id = RunId::from_string(&existing.value.run_id).unwrap();
            target_kv.put(&existing_run_id, "existing", Value::String("keep-me".into())).unwrap();

            // Create a bundle from a different database
            let (source_db, _source_temp) = create_test_db();
            let source_run_index = RunIndex::new(source_db.clone());
            let source_kv = KVStore::new(source_db.clone());

            let meta = source_run_index.create_run("new-run").unwrap();
            let run_id = RunId::from_string(&meta.value.run_id).unwrap();
            source_kv.put(&run_id, "imported", Value::String("new-data".into())).unwrap();
            source_run_index.complete_run("new-run").unwrap();

            let bundle_path = target_temp.path().join("import.runbundle.tar.zst");
            source_run_index.export_run("new-run", &bundle_path).unwrap();

            // Import into target database (which already has data)
            target_run_index.import_run(&bundle_path).unwrap();

            // Verify existing run data is untouched
            let existing_val = target_kv.get(&existing_run_id, "existing").unwrap().unwrap();
            assert_eq!(existing_val.value, Value::String("keep-me".into()));

            // Verify imported run data exists
            let imported = target_run_index.get_run("new-run").unwrap().unwrap();
            let imported_run_id = RunId::from_string(&imported.value.run_id).unwrap();
            let imported_val = target_kv.get(&imported_run_id, "imported").unwrap().unwrap();
            assert_eq!(imported_val.value, Value::String("new-data".into()));
        }

        #[test]
        fn test_export_import_special_characters_in_keys() {
            let (source_db, source_temp) = create_test_db();
            let source_run_index = RunIndex::new(source_db.clone());
            let source_kv = KVStore::new(source_db.clone());

            let meta = source_run_index.create_run("special-chars").unwrap();
            let run_id = RunId::from_string(&meta.value.run_id).unwrap();

            // Keys with special characters
            source_kv.put(&run_id, "key with spaces", Value::Int(1)).unwrap();
            source_kv.put(&run_id, "key/with/slashes", Value::Int(2)).unwrap();
            source_kv.put(&run_id, "key:with:colons", Value::Int(3)).unwrap();
            source_kv.put(&run_id, "key.with.dots", Value::Int(4)).unwrap();
            source_kv.put(&run_id, "key\twith\ttabs", Value::Int(5)).unwrap();
            source_kv.put(&run_id, "日本語キー", Value::Int(6)).unwrap();  // Japanese
            source_kv.put(&run_id, "emoji🔑", Value::Int(7)).unwrap();

            source_run_index.complete_run("special-chars").unwrap();

            let bundle_path = source_temp.path().join("special.runbundle.tar.zst");
            source_run_index.export_run("special-chars", &bundle_path).unwrap();

            // Import
            let (target_db, _target_temp) = create_test_db();
            let target_run_index = RunIndex::new(target_db.clone());
            let target_kv = KVStore::new(target_db.clone());

            target_run_index.import_run(&bundle_path).unwrap();

            let imported = target_run_index.get_run("special-chars").unwrap().unwrap();
            let target_run_id = RunId::from_string(&imported.value.run_id).unwrap();

            // All special keys should work
            assert_eq!(target_kv.get(&target_run_id, "key with spaces").unwrap().unwrap().value, Value::Int(1));
            assert_eq!(target_kv.get(&target_run_id, "key/with/slashes").unwrap().unwrap().value, Value::Int(2));
            assert_eq!(target_kv.get(&target_run_id, "key:with:colons").unwrap().unwrap().value, Value::Int(3));
            assert_eq!(target_kv.get(&target_run_id, "key.with.dots").unwrap().unwrap().value, Value::Int(4));
            assert_eq!(target_kv.get(&target_run_id, "key\twith\ttabs").unwrap().unwrap().value, Value::Int(5));
            assert_eq!(target_kv.get(&target_run_id, "日本語キー").unwrap().unwrap().value, Value::Int(6));
            assert_eq!(target_kv.get(&target_run_id, "emoji🔑").unwrap().unwrap().value, Value::Int(7));
        }
    }

    // ========== RunBundle Verification Tests ==========

    mod verify_tests {
        use super::*;
        use crate::KVStore;
        use strata_durability::run_bundle::RunBundleError;

        #[test]
        fn test_verify_valid_bundle() {
            // Create and export a valid bundle
            let (db, temp_dir) = create_test_db();
            let run_index = RunIndex::new(db.clone());
            let kv = KVStore::new(db.clone());

            let meta = run_index.create_run("verify-test").unwrap();
            let run_id = RunId::from_string(&meta.value.run_id).unwrap();

            kv.put(&run_id, "key1", Value::String("value1".into())).unwrap();
            run_index.complete_run("verify-test").unwrap();

            let bundle_path = temp_dir.path().join("verify-test.runbundle.tar.zst");
            let export_info = run_index.export_run("verify-test", &bundle_path).unwrap();

            // Verify the bundle
            let verify_info = run_index.verify_bundle(&bundle_path).unwrap();

            assert_eq!(verify_info.run_id, meta.value.run_id);
            assert_eq!(verify_info.format_version, 1);
            assert_eq!(verify_info.wal_entry_count, export_info.wal_entry_count);
            assert!(verify_info.checksums_valid);
        }

        #[test]
        fn test_verify_returns_entry_count() {
            let (db, temp_dir) = create_test_db();
            let run_index = RunIndex::new(db.clone());
            let kv = KVStore::new(db.clone());

            let meta = run_index.create_run("count-test").unwrap();
            let run_id = RunId::from_string(&meta.value.run_id).unwrap();

            // Add 5 key-value pairs
            for i in 0..5 {
                kv.put(&run_id, &format!("key{}", i), Value::Int(i)).unwrap();
            }
            run_index.complete_run("count-test").unwrap();

            let bundle_path = temp_dir.path().join("count-test.runbundle.tar.zst");
            run_index.export_run("count-test", &bundle_path).unwrap();

            let verify_info = run_index.verify_bundle(&bundle_path).unwrap();

            // 5 puts = 5 * 3 = 15 WAL entries (BeginTxn, Write, CommitTxn each)
            assert_eq!(verify_info.wal_entry_count, 15);
            assert!(verify_info.checksums_valid);
        }

        #[test]
        fn test_verify_nonexistent_file() {
            let (db, temp_dir) = create_test_db();
            let run_index = RunIndex::new(db);

            let result = run_index.verify_bundle(&temp_dir.path().join("nonexistent.runbundle.tar.zst"));

            assert!(result.is_err());
            let err = result.unwrap_err();
            assert!(matches!(err, RunBundleError::Io(_)));
        }

        #[test]
        fn test_verify_truncated_archive() {
            let (db, temp_dir) = create_test_db();
            let run_index = RunIndex::new(db.clone());
            let kv = KVStore::new(db.clone());

            let meta = run_index.create_run("truncate-test").unwrap();
            let run_id = RunId::from_string(&meta.value.run_id).unwrap();
            kv.put(&run_id, "key1", Value::String("value".into())).unwrap();
            run_index.complete_run("truncate-test").unwrap();

            let bundle_path = temp_dir.path().join("truncate.runbundle.tar.zst");
            run_index.export_run("truncate-test", &bundle_path).unwrap();

            // Read the file and truncate it
            let data = std::fs::read(&bundle_path).unwrap();
            let truncated = &data[..data.len() / 2];
            std::fs::write(&bundle_path, truncated).unwrap();

            // Verification should fail
            let result = run_index.verify_bundle(&bundle_path);
            assert!(result.is_err());
        }

        #[test]
        fn test_verify_corrupted_archive() {
            let (db, temp_dir) = create_test_db();
            let run_index = RunIndex::new(db.clone());
            let kv = KVStore::new(db.clone());

            let meta = run_index.create_run("corrupt-test").unwrap();
            let run_id = RunId::from_string(&meta.value.run_id).unwrap();
            kv.put(&run_id, "key1", Value::String("value".into())).unwrap();
            run_index.complete_run("corrupt-test").unwrap();

            let bundle_path = temp_dir.path().join("corrupt.runbundle.tar.zst");
            run_index.export_run("corrupt-test", &bundle_path).unwrap();

            // Corrupt the file (flip some bytes in the middle)
            let mut data = std::fs::read(&bundle_path).unwrap();
            let mid = data.len() / 2;
            for i in 0..10 {
                if mid + i < data.len() {
                    data[mid + i] ^= 0xFF;
                }
            }
            std::fs::write(&bundle_path, &data).unwrap();

            // Verification should fail
            let result = run_index.verify_bundle(&bundle_path);
            assert!(result.is_err());
        }

        #[test]
        fn test_verify_empty_file() {
            let (db, temp_dir) = create_test_db();
            let run_index = RunIndex::new(db);

            let empty_path = temp_dir.path().join("empty.runbundle.tar.zst");
            std::fs::File::create(&empty_path).unwrap();

            let result = run_index.verify_bundle(&empty_path);
            assert!(result.is_err());
        }

        #[test]
        fn test_verify_random_data() {
            let (db, temp_dir) = create_test_db();
            let run_index = RunIndex::new(db);

            let random_path = temp_dir.path().join("random.runbundle.tar.zst");
            let random_data: Vec<u8> = (0..1000).map(|i| (i % 256) as u8).collect();
            std::fs::write(&random_path, &random_data).unwrap();

            let result = run_index.verify_bundle(&random_path);
            assert!(result.is_err());
        }

        #[test]
        fn test_verify_does_not_import() {
            // Verify should not modify the database
            let (source_db, source_temp) = create_test_db();
            let source_run_index = RunIndex::new(source_db.clone());
            let source_kv = KVStore::new(source_db.clone());

            let meta = source_run_index.create_run("no-import-test").unwrap();
            let run_id = RunId::from_string(&meta.value.run_id).unwrap();
            source_kv.put(&run_id, "key1", Value::String("value".into())).unwrap();
            source_run_index.complete_run("no-import-test").unwrap();

            let bundle_path = source_temp.path().join("no-import.runbundle.tar.zst");
            source_run_index.export_run("no-import-test", &bundle_path).unwrap();

            // Create a fresh database and verify bundle
            let (target_db, _) = create_test_db();
            let target_run_index = RunIndex::new(target_db.clone());
            let target_kv = KVStore::new(target_db.clone());

            // Verify should succeed
            let verify_info = target_run_index.verify_bundle(&bundle_path).unwrap();
            assert!(verify_info.checksums_valid);

            // But run should NOT exist in target database
            assert!(!target_run_index.exists("no-import-test").unwrap());

            // And data should NOT exist
            let target_run_id = RunId::from_string(&verify_info.run_id).unwrap();
            assert!(target_kv.get(&target_run_id, "key1").unwrap().is_none());
        }

        #[test]
        fn test_verify_then_import_workflow() {
            // Common workflow: verify first, then import
            let (source_db, source_temp) = create_test_db();
            let source_run_index = RunIndex::new(source_db.clone());
            let source_kv = KVStore::new(source_db.clone());

            let meta = source_run_index.create_run("workflow-test").unwrap();
            let run_id = RunId::from_string(&meta.value.run_id).unwrap();
            source_kv.put(&run_id, "data", Value::String("important".into())).unwrap();
            source_run_index.complete_run("workflow-test").unwrap();

            let bundle_path = source_temp.path().join("workflow.runbundle.tar.zst");
            source_run_index.export_run("workflow-test", &bundle_path).unwrap();

            // Target database: verify then import
            let (target_db, _) = create_test_db();
            let target_run_index = RunIndex::new(target_db.clone());
            let target_kv = KVStore::new(target_db.clone());

            // Step 1: Verify
            let verify_info = target_run_index.verify_bundle(&bundle_path).unwrap();
            assert!(verify_info.checksums_valid);
            assert_eq!(verify_info.format_version, 1);

            // Step 2: Import (only if verification passed)
            let import_info = target_run_index.import_run(&bundle_path).unwrap();
            assert_eq!(import_info.run_id, verify_info.run_id);

            // Step 3: Use the imported data
            let imported = target_run_index.get_run("workflow-test").unwrap().unwrap();
            let target_run_id = RunId::from_string(&imported.value.run_id).unwrap();
            let val = target_kv.get(&target_run_id, "data").unwrap().unwrap();
            assert_eq!(val.value, Value::String("important".into()));
        }

        #[test]
        fn test_verify_large_bundle() {
            // Test verification with a larger bundle
            let (db, temp_dir) = create_test_db();
            let run_index = RunIndex::new(db.clone());
            let kv = KVStore::new(db.clone());

            let meta = run_index.create_run("large-verify").unwrap();
            let run_id = RunId::from_string(&meta.value.run_id).unwrap();

            // Add 100 entries with larger values
            for i in 0..100 {
                let value = format!("value_{:03}_with_some_longer_content_to_increase_size", i);
                kv.put(&run_id, &format!("key_{:03}", i), Value::String(value)).unwrap();
            }
            run_index.complete_run("large-verify").unwrap();

            let bundle_path = temp_dir.path().join("large.runbundle.tar.zst");
            run_index.export_run("large-verify", &bundle_path).unwrap();

            let verify_info = run_index.verify_bundle(&bundle_path).unwrap();

            assert!(verify_info.checksums_valid);
            assert_eq!(verify_info.wal_entry_count, 300); // 100 * 3
        }

        #[test]
        fn test_verify_multiple_bundles() {
            // Verify that verifying one bundle doesn't affect another
            let (db, temp_dir) = create_test_db();
            let run_index = RunIndex::new(db.clone());
            let kv = KVStore::new(db.clone());

            // Create two runs
            let meta1 = run_index.create_run("run-1").unwrap();
            let run_id1 = RunId::from_string(&meta1.value.run_id).unwrap();
            kv.put(&run_id1, "key", Value::Int(1)).unwrap();
            run_index.complete_run("run-1").unwrap();

            let meta2 = run_index.create_run("run-2").unwrap();
            let run_id2 = RunId::from_string(&meta2.value.run_id).unwrap();
            kv.put(&run_id2, "key", Value::Int(2)).unwrap();
            run_index.complete_run("run-2").unwrap();

            // Export both
            let bundle1_path = temp_dir.path().join("run1.runbundle.tar.zst");
            let bundle2_path = temp_dir.path().join("run2.runbundle.tar.zst");
            run_index.export_run("run-1", &bundle1_path).unwrap();
            run_index.export_run("run-2", &bundle2_path).unwrap();

            // Verify both
            let verify1 = run_index.verify_bundle(&bundle1_path).unwrap();
            let verify2 = run_index.verify_bundle(&bundle2_path).unwrap();

            assert_eq!(verify1.run_id, meta1.value.run_id);
            assert_eq!(verify2.run_id, meta2.value.run_id);
            assert!(verify1.checksums_valid);
            assert!(verify2.checksums_valid);
        }

        #[test]
        fn test_verify_bundle_with_failed_run() {
            let (db, temp_dir) = create_test_db();
            let run_index = RunIndex::new(db.clone());
            let kv = KVStore::new(db.clone());

            let meta = run_index.create_run("failed-verify").unwrap();
            let run_id = RunId::from_string(&meta.value.run_id).unwrap();
            kv.put(&run_id, "partial", Value::String("data".into())).unwrap();
            run_index.fail_run("failed-verify", "Test failure reason").unwrap();

            let bundle_path = temp_dir.path().join("failed.runbundle.tar.zst");
            run_index.export_run("failed-verify", &bundle_path).unwrap();

            let verify_info = run_index.verify_bundle(&bundle_path).unwrap();

            assert!(verify_info.checksums_valid);
            assert_eq!(verify_info.run_id, meta.value.run_id);
        }
    }
}
