//! Command enum defining all Strata operations.
//!
//! Commands are the "instruction set" of Strata. Every operation that can be
//! performed on the database is represented as a variant of this enum.
//!
//! Commands are:
//! - **Self-contained**: All parameters needed for execution are in the variant
//! - **Serializable**: Can be converted to/from JSON for cross-language use
//! - **Typed**: No generic fallback, every operation has explicit types
//! - **Pure data**: No closures or executable code

use serde::{Deserialize, Serialize};
use strata_core::Value;

use crate::types::*;

/// A command is a self-contained, serializable operation.
///
/// This is the "instruction set" of Strata - every operation that can be
/// performed on the database is represented here.
///
/// # Command Categories
///
/// | Category | Count | Description |
/// |----------|-------|-------------|
/// | KV | 4 | Key-value operations |
/// | JSON | 17 | JSON document operations |
/// | Event | 4 | Event log operations (MVP) |
/// | State | 4 | State cell operations (MVP) |
/// | Vector | 7 | Vector store operations (MVP) |
/// | Run | 5 | Run lifecycle operations (MVP) |
/// | Transaction | 5 | Transaction control |
/// | Retention | 3 | Retention policy |
/// | Database | 4 | Database-level operations |
///
/// # Run field
///
/// Data-scoped commands have an optional `run` field. When omitted (or `None`),
/// the executor resolves it to the default run before dispatch. Existing JSON
/// with `"run": "default"` continues to work; new callers can simply omit the
/// field.
///
/// Run lifecycle commands (RunGet, RunComplete, RunDelete, etc.) keep a required
/// `run: RunId` since they explicitly operate on a specific run.
///
/// # Example
///
/// ```ignore
/// use strata_executor::{Command, RunId};
/// use strata_core::Value;
///
/// // Explicit run
/// let cmd = Command::KvPut {
///     run: Some(RunId::default()),
///     key: "foo".into(),
///     value: Value::Int(42),
/// };
///
/// // Omit run (defaults to "default")
/// let cmd = Command::KvPut {
///     run: None,
///     key: "foo".into(),
///     value: Value::Int(42),
/// };
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum Command {
    // ==================== KV (4) ====================
    /// Put a key-value pair.
    /// Returns: `Output::Version`
    KvPut {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        run: Option<RunId>,
        key: String,
        value: Value,
    },

    /// Get a value by key.
    /// Returns: `Output::MaybeValue`
    KvGet {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        run: Option<RunId>,
        key: String,
    },

    /// Delete a key.
    /// Returns: `Output::Bool` (true if key existed)
    KvDelete {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        run: Option<RunId>,
        key: String,
    },

    /// List keys with optional prefix filter.
    /// Returns: `Output::Keys`
    KvList {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        run: Option<RunId>,
        prefix: Option<String>,
    },

    // ==================== JSON (4 MVP) ====================
    /// Set a value at a path in a JSON document.
    /// Returns: `Output::Version`
    JsonSet {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        run: Option<RunId>,
        key: String,
        path: String,
        value: Value,
    },

    /// Get a value at a path from a JSON document.
    /// Returns: `Output::MaybeVersioned`
    JsonGet {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        run: Option<RunId>,
        key: String,
        path: String,
    },

    /// Delete a value at a path from a JSON document.
    /// Returns: `Output::Uint` (count of elements removed)
    JsonDelete {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        run: Option<RunId>,
        key: String,
        path: String,
    },

    /// List JSON documents with cursor-based pagination.
    /// Returns: `Output::JsonListResult`
    JsonList {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        run: Option<RunId>,
        prefix: Option<String>,
        cursor: Option<String>,
        limit: u64,
    },

    // ==================== Event (4 MVP) ====================
    // MVP: append, read, read_by_type, len

    /// Append an event to the log.
    /// Returns: `Output::Version`
    EventAppend {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        run: Option<RunId>,
        event_type: String,
        payload: Value,
    },

    /// Read a specific event by sequence number.
    /// Returns: `Output::MaybeVersioned`
    EventRead {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        run: Option<RunId>,
        sequence: u64,
    },

    /// Read all events of a specific type.
    /// Returns: `Output::VersionedValues`
    EventReadByType {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        run: Option<RunId>,
        event_type: String,
    },

    /// Get the total count of events in the log.
    /// Returns: `Output::Uint`
    EventLen {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        run: Option<RunId>,
    },

    // ==================== State (4 MVP) ====================
    // MVP: set, read, cas, init

    /// Set a state cell value (unconditional write).
    /// Returns: `Output::Version`
    StateSet {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        run: Option<RunId>,
        cell: String,
        value: Value,
    },

    /// Read a state cell value.
    /// Returns: `Output::MaybeVersioned`
    StateRead {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        run: Option<RunId>,
        cell: String,
    },

    /// Compare-and-swap on a state cell.
    /// Returns: `Output::MaybeVersion`
    StateCas {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        run: Option<RunId>,
        cell: String,
        expected_counter: Option<u64>,
        value: Value,
    },

    /// Initialize a state cell (only if it doesn't exist).
    /// Returns: `Output::Version`
    StateInit {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        run: Option<RunId>,
        cell: String,
        value: Value,
    },

    // ==================== Vector (7 MVP) ====================
    // MVP: upsert, get, delete, search, create_collection, delete_collection, list_collections

    /// Insert or update a vector.
    /// Returns: `Output::Version`
    VectorUpsert {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        run: Option<RunId>,
        collection: String,
        key: String,
        vector: Vec<f32>,
        metadata: Option<Value>,
    },

    /// Get a vector by key.
    /// Returns: `Output::MaybeVectorData`
    VectorGet {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        run: Option<RunId>,
        collection: String,
        key: String,
    },

    /// Delete a vector.
    /// Returns: `Output::Bool`
    VectorDelete {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        run: Option<RunId>,
        collection: String,
        key: String,
    },

    /// Search for similar vectors.
    /// Returns: `Output::VectorMatches`
    VectorSearch {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        run: Option<RunId>,
        collection: String,
        query: Vec<f32>,
        k: u64,
        filter: Option<Vec<MetadataFilter>>,
        metric: Option<DistanceMetric>,
    },

    /// Create a collection with explicit configuration.
    /// Returns: `Output::Version`
    VectorCreateCollection {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        run: Option<RunId>,
        collection: String,
        dimension: u64,
        metric: DistanceMetric,
    },

    /// Delete a collection.
    /// Returns: `Output::Bool`
    VectorDeleteCollection {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        run: Option<RunId>,
        collection: String,
    },

    /// List all collections in a run.
    /// Returns: `Output::VectorCollectionList`
    VectorListCollections {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        run: Option<RunId>,
    },

    // ==================== Run (5 MVP) ====================
    /// Create a new run.
    /// Returns: `Output::RunWithVersion`
    RunCreate {
        run_id: Option<String>,
        metadata: Option<Value>,
    },

    /// Get run info.
    /// Returns: `Output::RunInfoVersioned` or `Output::Maybe(None)`
    RunGet {
        run: RunId,
    },

    /// List all runs.
    /// Returns: `Output::RunInfoList`
    RunList {
        state: Option<RunStatus>,
        limit: Option<u64>,
        offset: Option<u64>,
    },

    /// Check if a run exists.
    /// Returns: `Output::Bool`
    RunExists {
        run: RunId,
    },

    /// Delete a run and all its data (cascading delete).
    /// Returns: `Output::Unit`
    RunDelete {
        run: RunId,
    },

    // ==================== Transaction (5) ====================
    /// Begin a new transaction.
    /// Returns: `Output::TxnBegun`
    TxnBegin {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        run: Option<RunId>,
        options: Option<TxnOptions>,
    },

    /// Commit the current transaction.
    /// Returns: `Output::Version`
    TxnCommit,

    /// Rollback the current transaction.
    /// Returns: `Output::Unit`
    TxnRollback,

    /// Get current transaction info.
    /// Returns: `Output::MaybeTxnInfo`
    TxnInfo,

    /// Check if a transaction is active.
    /// Returns: `Output::Bool`
    TxnIsActive,

    // ==================== Retention (3) ====================
    // Note: Run-level retention is handled via RunSetRetention/RunGetRetention
    // These are database-wide retention operations

    /// Apply retention policy (trigger garbage collection).
    /// Returns: `Output::RetentionResult`
    RetentionApply {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        run: Option<RunId>,
    },

    /// Get retention statistics.
    /// Returns: `Output::RetentionStats`
    RetentionStats {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        run: Option<RunId>,
    },

    /// Preview what would be deleted by retention policy.
    /// Returns: `Output::RetentionPreview`
    RetentionPreview {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        run: Option<RunId>,
    },

    // ==================== Database (4) ====================
    /// Ping the database to check connectivity
    Ping,

    /// Get database information
    Info,

    /// Flush pending writes to disk
    Flush,

    /// Trigger compaction
    Compact,

    // ==================== Intelligence (1) ====================

    /// Search across multiple primitives.
    /// Returns: `Output::SearchResults`
    Search {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        run: Option<RunId>,
        query: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        k: Option<u64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        primitives: Option<Vec<String>>,
    },
}

impl Command {
    /// Fill in the default run for any data command where run is `None`.
    ///
    /// Called by the executor before dispatch so handlers always receive a
    /// concrete `RunId`.
    pub fn resolve_default_run(&mut self) {
        macro_rules! resolve {
            ($run:expr) => {
                if $run.is_none() {
                    *$run = Some(RunId::default());
                }
            };
        }

        match self {
            // KV (4 MVP)
            Command::KvPut { run, .. }
            | Command::KvGet { run, .. }
            | Command::KvDelete { run, .. }
            | Command::KvList { run, .. }
            // JSON
            | Command::JsonSet { run, .. }
            | Command::JsonGet { run, .. }
            | Command::JsonDelete { run, .. }
            | Command::JsonList { run, .. }
            // Event (4 MVP)
            | Command::EventAppend { run, .. }
            | Command::EventRead { run, .. }
            | Command::EventReadByType { run, .. }
            | Command::EventLen { run, .. }
            // State (4 MVP)
            | Command::StateSet { run, .. }
            | Command::StateRead { run, .. }
            | Command::StateCas { run, .. }
            | Command::StateInit { run, .. }
            // Vector (7 MVP)
            | Command::VectorUpsert { run, .. }
            | Command::VectorGet { run, .. }
            | Command::VectorDelete { run, .. }
            | Command::VectorSearch { run, .. }
            | Command::VectorCreateCollection { run, .. }
            | Command::VectorDeleteCollection { run, .. }
            | Command::VectorListCollections { run, .. }
            // Retention
            | Command::RetentionApply { run, .. }
            | Command::RetentionStats { run, .. }
            | Command::RetentionPreview { run, .. }
            // Transaction begin
            | Command::TxnBegin { run, .. }
            // Intelligence
            | Command::Search { run, .. } => {
                resolve!(run);
            }

            // Run lifecycle (5 MVP), Transaction, and Database commands have no
            // optional run to resolve.
            Command::RunCreate { .. }
            | Command::RunGet { .. }
            | Command::RunList { .. }
            | Command::RunExists { .. }
            | Command::RunDelete { .. }
            | Command::TxnCommit
            | Command::TxnRollback
            | Command::TxnInfo
            | Command::TxnIsActive
            | Command::Ping
            | Command::Info
            | Command::Flush
            | Command::Compact => {}
        }
    }
}
