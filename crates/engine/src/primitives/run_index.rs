//! RunIndex: Run lifecycle management (MVP)
//!
//! ## MVP Scope
//!
//! RunIndex tracks which runs exist. For MVP, runs are simply created or deleted.
//! Advanced lifecycle management (status transitions, tags, metadata, export/import)
//! is deferred to post-MVP.
//!
//! ## MVP Methods
//!
//! - `create_run(name)` - Create a new run
//! - `get_run(name)` - Get run metadata
//! - `exists(name)` - Check if run exists
//! - `list_runs()` - List all run names
//! - `delete_run(name)` - Delete run and ALL its data (cascading)
//!
//! ## Key Design
//!
//! - TypeTag: Run (0x05)
//! - Primary key format: `<global_namespace>:<TypeTag::Run>:<run_id>`
//! - RunIndex uses a global namespace (not run-scoped) since it manages runs themselves.

use crate::database::Database;
use serde::{Deserialize, Serialize};
use strata_core::contract::{Timestamp, Version, Versioned};
use strata_core::error::Result;
use strata_core::types::{Key, Namespace, RunId, TypeTag};
use strata_core::value::Value;
use strata_core::StrataError;
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

// ========== RunStatus Enum ==========

/// Run lifecycle status
///
/// For MVP, all runs are Active. Status transitions are post-MVP.
/// The enum is kept with all variants for serialization compatibility.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RunStatus {
    /// Run is currently active
    Active,
    /// Run completed successfully (post-MVP)
    Completed,
    /// Run failed with error (post-MVP)
    Failed,
    /// Run was cancelled (post-MVP)
    Cancelled,
    /// Run is paused (post-MVP)
    Paused,
    /// Run is archived (post-MVP)
    Archived,
}

impl RunStatus {
    /// Get the string representation
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

impl Default for RunStatus {
    fn default() -> Self {
        RunStatus::Active
    }
}

// ========== RunMetadata Struct ==========

/// Metadata about a run
///
/// For MVP, only `name`, `run_id`, `created_at`, and `version` are actively used.
/// Other fields are kept for serialization compatibility and future use.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RunMetadata {
    /// User-provided name/key for the run (used for lookups)
    pub name: String,
    /// Unique run identifier (UUID) for internal use and namespacing
    pub run_id: String,
    /// Parent run name if forked (post-MVP)
    pub parent_run: Option<String>,
    /// Current status (always Active for MVP)
    pub status: RunStatus,
    /// Creation timestamp (microseconds since epoch)
    pub created_at: u64,
    /// Last update timestamp (microseconds since epoch)
    pub updated_at: u64,
    /// Completion timestamp (post-MVP)
    pub completed_at: Option<u64>,
    /// User-defined tags (post-MVP)
    pub tags: Vec<String>,
    /// Custom metadata (post-MVP)
    pub metadata: Value,
    /// Error message if failed (post-MVP)
    pub error: Option<String>,
    /// Internal version counter
    #[serde(default = "default_version")]
    pub version: u64,
}

fn default_version() -> u64 {
    1
}

impl RunMetadata {
    /// Create new run metadata with Active status
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
            Timestamp::from_micros(timestamp),
        )
    }

    /// Get current timestamp in microseconds
    fn now() -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_micros() as u64
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
        _ => serde_json::from_str("null"),
    }
}

// ========== RunIndex Core ==========

/// Run lifecycle management primitive (MVP)
///
/// ## Design
///
/// RunIndex provides run lifecycle management. It is a stateless facade
/// over the Database engine, holding only an `Arc<Database>` reference.
///
/// ## MVP Methods
///
/// - `create_run()` - Create a new run
/// - `get_run()` - Get run metadata
/// - `exists()` - Check if run exists
/// - `list_runs()` - List all runs
/// - `delete_run()` - Delete run and all its data
///
/// ## Example
///
/// ```rust,ignore
/// let ri = RunIndex::new(db.clone());
///
/// // Create a run
/// let meta = ri.create_run("my-run")?;
/// assert_eq!(meta.value.status, RunStatus::Active);
///
/// // Check existence
/// assert!(ri.exists("my-run")?);
///
/// // Delete it
/// ri.delete_run("my-run")?;
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

    /// Build key for run metadata
    fn key_for(&self, run_id: &str) -> Key {
        Key::new_run_with_id(global_namespace(), run_id)
    }

    // ========== MVP Methods ==========

    /// Create a new run
    ///
    /// Creates a run with Active status.
    ///
    /// ## Errors
    /// - `InvalidInput` if run already exists
    pub fn create_run(&self, run_id: &str) -> Result<Versioned<RunMetadata>> {
        self.db.transaction(global_run_id(), |txn| {
            let key = self.key_for(run_id);

            // Check if run already exists
            if txn.get(&key)?.is_some() {
                return Err(StrataError::invalid_input(format!(
                    "Run '{}' already exists",
                    run_id
                )));
            }

            let run_meta = RunMetadata::new(run_id);
            txn.put(key, to_stored_value(&run_meta))?;

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
                        .map_err(|e| StrataError::serialization(e.to_string()))?;
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

            Ok(results
                .into_iter()
                .filter_map(|(k, _)| {
                    let key_str = String::from_utf8(k.user_key.clone()).ok()?;
                    // Filter out any index keys (legacy data)
                    if key_str.contains("__idx_") {
                        None
                    } else {
                        Some(key_str)
                    }
                })
                .collect())
        })
    }

    /// Delete a run and ALL its data (cascading delete)
    ///
    /// This deletes:
    /// - The run metadata
    /// - All run-scoped data (KV, Events, States, JSON, Vectors)
    ///
    /// USE WITH CAUTION - this is irreversible!
    pub fn delete_run(&self, run_id: &str) -> Result<()> {
        // First get the run metadata
        let run_meta = self
            .get_run(run_id)?
            .ok_or_else(|| StrataError::invalid_input(format!("Run '{}' not found", run_id)))?
            .value;

        // Determine the RunId for namespace deletion
        let actual_run_id = RunId::from_string(&run_meta.name)
            .or_else(|| RunId::from_string(&run_meta.run_id))
            .ok_or_else(|| {
                StrataError::invalid_input(format!(
                    "Invalid run identifiers: name='{}', run_id='{}'",
                    run_meta.name, run_meta.run_id
                ))
            })?;

        // Delete all run-scoped data (cascading delete)
        self.delete_run_data_internal(actual_run_id)?;

        // Delete the run metadata
        self.db.transaction(global_run_id(), |txn| {
            let meta_key = self.key_for(run_id);
            txn.delete(meta_key)?;
            Ok(())
        })
    }

    /// Internal helper to delete all run-scoped data
    fn delete_run_data_internal(&self, run_id: RunId) -> Result<()> {
        let ns = Namespace::for_run(run_id);

        // Delete data for each type tag
        #[allow(deprecated)]
        for type_tag in [
            TypeTag::KV,
            TypeTag::Event,
            TypeTag::State,
            TypeTag::Trace, // Deprecated but kept for backwards compatibility
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
}

// ========== Searchable Trait Implementation ==========
//
// Search is handled by the intelligence layer.
// This implementation returns empty results.

impl crate::primitives::searchable::Searchable for RunIndex {
    fn search(
        &self,
        _req: &crate::SearchRequest,
    ) -> strata_core::error::Result<crate::SearchResponse> {
        // Search moved to intelligence layer - return empty results
        Ok(crate::SearchResponse::empty())
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

    fn setup() -> (TempDir, Arc<Database>, RunIndex) {
        let temp_dir = TempDir::new().unwrap();
        let db = Database::open(temp_dir.path()).unwrap();
        let ri = RunIndex::new(db.clone());
        (temp_dir, db, ri)
    }

    #[test]
    fn test_create_run() {
        let (_temp, _db, ri) = setup();

        let result = ri.create_run("test-run").unwrap();
        assert_eq!(result.value.name, "test-run");
        assert_eq!(result.value.status, RunStatus::Active);
    }

    #[test]
    fn test_create_run_duplicate_fails() {
        let (_temp, _db, ri) = setup();

        ri.create_run("test-run").unwrap();
        let result = ri.create_run("test-run");
        assert!(result.is_err());
    }

    #[test]
    fn test_get_run() {
        let (_temp, _db, ri) = setup();

        ri.create_run("test-run").unwrap();

        let result = ri.get_run("test-run").unwrap();
        assert!(result.is_some());
        assert_eq!(result.unwrap().value.name, "test-run");
    }

    #[test]
    fn test_get_run_not_found() {
        let (_temp, _db, ri) = setup();

        let result = ri.get_run("nonexistent").unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_exists() {
        let (_temp, _db, ri) = setup();

        assert!(!ri.exists("test-run").unwrap());

        ri.create_run("test-run").unwrap();
        assert!(ri.exists("test-run").unwrap());
    }

    #[test]
    fn test_list_runs() {
        let (_temp, _db, ri) = setup();

        ri.create_run("run-a").unwrap();
        ri.create_run("run-b").unwrap();
        ri.create_run("run-c").unwrap();

        let runs = ri.list_runs().unwrap();
        assert_eq!(runs.len(), 3);
        assert!(runs.contains(&"run-a".to_string()));
        assert!(runs.contains(&"run-b".to_string()));
        assert!(runs.contains(&"run-c".to_string()));
    }

    #[test]
    fn test_delete_run() {
        let (_temp, _db, ri) = setup();

        ri.create_run("test-run").unwrap();
        assert!(ri.exists("test-run").unwrap());

        ri.delete_run("test-run").unwrap();
        assert!(!ri.exists("test-run").unwrap());
    }

    #[test]
    fn test_delete_run_not_found() {
        let (_temp, _db, ri) = setup();

        let result = ri.delete_run("nonexistent");
        assert!(result.is_err());
    }

    #[test]
    fn test_run_status_default() {
        assert_eq!(RunStatus::default(), RunStatus::Active);
    }

    #[test]
    fn test_run_status_as_str() {
        assert_eq!(RunStatus::Active.as_str(), "Active");
        assert_eq!(RunStatus::Completed.as_str(), "Completed");
        assert_eq!(RunStatus::Failed.as_str(), "Failed");
    }
}
