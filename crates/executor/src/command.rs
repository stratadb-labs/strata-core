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
/// | Branch | 5 | Branch lifecycle operations (MVP) |
/// | Transaction | 5 | Transaction control |
/// | Retention | 3 | Retention policy |
/// | Database | 4 | Database-level operations |
///
/// # Branch field
///
/// Data-scoped commands have an optional `branch` field. When omitted (or `None`),
/// the executor resolves it to the default branch before dispatch. JSON
/// with `"branch": "default"` works; new callers can simply omit the field.
///
/// Branch lifecycle commands (BranchGet, BranchDelete, etc.) keep a required
/// `branch: BranchId` since they explicitly operate on a specific branch.
///
/// # Example
///
/// ```text
/// use strata_executor::{Command, BranchId};
/// use strata_core::Value;
///
/// // Explicit branch
/// let cmd = Command::KvPut {
///     branch: Some(BranchId::default()),
///     key: "foo".into(),
///     value: Value::Int(42),
/// };
///
/// // Omit branch (defaults to "default")
/// let cmd = Command::KvPut {
///     branch: None,
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
        /// Target branch (defaults to "default").
        #[serde(default, skip_serializing_if = "Option::is_none")]
        branch: Option<BranchId>,
        /// Target space (defaults to "default").
        #[serde(default, skip_serializing_if = "Option::is_none")]
        space: Option<String>,
        /// Key to write.
        key: String,
        /// Value to store.
        value: Value,
    },

    /// Get a value by key.
    /// Returns: `Output::MaybeValue`
    KvGet {
        /// Target branch (defaults to "default").
        #[serde(default, skip_serializing_if = "Option::is_none")]
        branch: Option<BranchId>,
        /// Target space (defaults to "default").
        #[serde(default, skip_serializing_if = "Option::is_none")]
        space: Option<String>,
        /// Key to look up.
        key: String,
        /// Optional timestamp for time-travel reads (microseconds since epoch).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        as_of: Option<u64>,
    },

    /// Delete a key.
    /// Returns: `Output::Bool` (true if key existed)
    KvDelete {
        /// Target branch (defaults to "default").
        #[serde(default, skip_serializing_if = "Option::is_none")]
        branch: Option<BranchId>,
        /// Target space (defaults to "default").
        #[serde(default, skip_serializing_if = "Option::is_none")]
        space: Option<String>,
        /// Key to delete.
        key: String,
    },

    /// List keys with optional prefix filter.
    /// Returns: `Output::Keys`
    KvList {
        /// Target branch (defaults to "default").
        #[serde(default, skip_serializing_if = "Option::is_none")]
        branch: Option<BranchId>,
        /// Target space (defaults to "default").
        #[serde(default, skip_serializing_if = "Option::is_none")]
        space: Option<String>,
        /// Optional key prefix filter.
        prefix: Option<String>,
        /// Pagination cursor from a previous response.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        cursor: Option<String>,
        /// Maximum number of keys to return.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        limit: Option<u64>,
        /// Optional timestamp for time-travel reads (microseconds since epoch).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        as_of: Option<u64>,
    },

    /// Get full version history for a key.
    /// Returns: `Output::VersionHistory`
    KvGetv {
        /// Target branch (defaults to "default").
        #[serde(default, skip_serializing_if = "Option::is_none")]
        branch: Option<BranchId>,
        /// Target space (defaults to "default").
        #[serde(default, skip_serializing_if = "Option::is_none")]
        space: Option<String>,
        /// Key to retrieve history for.
        key: String,
        /// Optional timestamp for time-travel reads (microseconds since epoch).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        as_of: Option<u64>,
    },

    // ==================== JSON (4 MVP) ====================
    /// Set a value at a path in a JSON document.
    /// Returns: `Output::Version`
    JsonSet {
        /// Target branch (defaults to "default").
        #[serde(default, skip_serializing_if = "Option::is_none")]
        branch: Option<BranchId>,
        /// Target space (defaults to "default").
        #[serde(default, skip_serializing_if = "Option::is_none")]
        space: Option<String>,
        /// Document key.
        key: String,
        /// JSON path within the document.
        path: String,
        /// Value to set at the path.
        value: Value,
    },

    /// Get a value at a path from a JSON document.
    /// Returns: `Output::MaybeVersioned`
    JsonGet {
        /// Target branch (defaults to "default").
        #[serde(default, skip_serializing_if = "Option::is_none")]
        branch: Option<BranchId>,
        /// Target space (defaults to "default").
        #[serde(default, skip_serializing_if = "Option::is_none")]
        space: Option<String>,
        /// Document key.
        key: String,
        /// JSON path to read.
        path: String,
        /// Optional timestamp for time-travel reads (microseconds since epoch).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        as_of: Option<u64>,
    },

    /// Delete a value at a path from a JSON document.
    /// Returns: `Output::Uint` (count of elements removed)
    JsonDelete {
        /// Target branch (defaults to "default").
        #[serde(default, skip_serializing_if = "Option::is_none")]
        branch: Option<BranchId>,
        /// Target space (defaults to "default").
        #[serde(default, skip_serializing_if = "Option::is_none")]
        space: Option<String>,
        /// Document key.
        key: String,
        /// JSON path to delete.
        path: String,
    },

    /// Get full version history for a JSON document.
    /// Returns: `Output::VersionHistory`
    JsonGetv {
        /// Target branch (defaults to "default").
        #[serde(default, skip_serializing_if = "Option::is_none")]
        branch: Option<BranchId>,
        /// Target space (defaults to "default").
        #[serde(default, skip_serializing_if = "Option::is_none")]
        space: Option<String>,
        /// Document key.
        key: String,
        /// Optional timestamp for time-travel reads (microseconds since epoch).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        as_of: Option<u64>,
    },

    /// List JSON documents with cursor-based pagination.
    /// Returns: `Output::JsonListResult`
    JsonList {
        /// Target branch (defaults to "default").
        #[serde(default, skip_serializing_if = "Option::is_none")]
        branch: Option<BranchId>,
        /// Target space (defaults to "default").
        #[serde(default, skip_serializing_if = "Option::is_none")]
        space: Option<String>,
        /// Optional key prefix filter.
        prefix: Option<String>,
        /// Pagination cursor from a previous response.
        cursor: Option<String>,
        /// Maximum number of documents to return.
        limit: u64,
        /// Optional timestamp for time-travel reads (microseconds since epoch).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        as_of: Option<u64>,
    },

    // ==================== Event (4 MVP) ====================
    // MVP: append, read, get_by_type, len
    /// Append an event to the log.
    /// Returns: `Output::Version`
    EventAppend {
        /// Target branch (defaults to "default").
        #[serde(default, skip_serializing_if = "Option::is_none")]
        branch: Option<BranchId>,
        /// Target space (defaults to "default").
        #[serde(default, skip_serializing_if = "Option::is_none")]
        space: Option<String>,
        /// Event type tag (e.g. "user.created").
        event_type: String,
        /// Event payload data.
        payload: Value,
    },

    /// Read a specific event by sequence number.
    /// Returns: `Output::MaybeVersioned`
    EventGet {
        /// Target branch (defaults to "default").
        #[serde(default, skip_serializing_if = "Option::is_none")]
        branch: Option<BranchId>,
        /// Target space (defaults to "default").
        #[serde(default, skip_serializing_if = "Option::is_none")]
        space: Option<String>,
        /// Event sequence number.
        sequence: u64,
        /// Optional timestamp for time-travel reads (microseconds since epoch).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        as_of: Option<u64>,
    },

    /// Read all events of a specific type.
    /// Returns: `Output::VersionedValues`
    EventGetByType {
        /// Target branch (defaults to "default").
        #[serde(default, skip_serializing_if = "Option::is_none")]
        branch: Option<BranchId>,
        /// Target space (defaults to "default").
        #[serde(default, skip_serializing_if = "Option::is_none")]
        space: Option<String>,
        /// Event type to filter by.
        event_type: String,
        /// Maximum number of events to return.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        limit: Option<u64>,
        /// Only return events after this sequence number.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        after_sequence: Option<u64>,
        /// Optional timestamp for time-travel reads (microseconds since epoch).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        as_of: Option<u64>,
    },

    /// Get the total count of events in the log.
    /// Returns: `Output::Uint`
    EventLen {
        /// Target branch (defaults to "default").
        #[serde(default, skip_serializing_if = "Option::is_none")]
        branch: Option<BranchId>,
        /// Target space (defaults to "default").
        #[serde(default, skip_serializing_if = "Option::is_none")]
        space: Option<String>,
    },

    // ==================== State (4 MVP) ====================
    // MVP: set, read, cas, init
    /// Set a state cell value (unconditional write).
    /// Returns: `Output::Version`
    StateSet {
        /// Target branch (defaults to "default").
        #[serde(default, skip_serializing_if = "Option::is_none")]
        branch: Option<BranchId>,
        /// Target space (defaults to "default").
        #[serde(default, skip_serializing_if = "Option::is_none")]
        space: Option<String>,
        /// Cell name.
        cell: String,
        /// Value to store.
        value: Value,
    },

    /// Read a state cell value.
    /// Returns: `Output::MaybeVersioned`
    StateGet {
        /// Target branch (defaults to "default").
        #[serde(default, skip_serializing_if = "Option::is_none")]
        branch: Option<BranchId>,
        /// Target space (defaults to "default").
        #[serde(default, skip_serializing_if = "Option::is_none")]
        space: Option<String>,
        /// Cell name.
        cell: String,
        /// Optional timestamp for time-travel reads (microseconds since epoch).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        as_of: Option<u64>,
    },

    /// Compare-and-swap on a state cell.
    /// Returns: `Output::MaybeVersion`
    StateCas {
        /// Target branch (defaults to "default").
        #[serde(default, skip_serializing_if = "Option::is_none")]
        branch: Option<BranchId>,
        /// Target space (defaults to "default").
        #[serde(default, skip_serializing_if = "Option::is_none")]
        space: Option<String>,
        /// Cell name.
        cell: String,
        /// Expected version counter (`None` means cell must not exist).
        expected_counter: Option<u64>,
        /// New value to swap in.
        value: Value,
    },

    /// Get full version history for a state cell.
    /// Returns: `Output::VersionHistory`
    StateGetv {
        /// Target branch (defaults to "default").
        #[serde(default, skip_serializing_if = "Option::is_none")]
        branch: Option<BranchId>,
        /// Target space (defaults to "default").
        #[serde(default, skip_serializing_if = "Option::is_none")]
        space: Option<String>,
        /// Cell name.
        cell: String,
        /// Optional timestamp for time-travel reads (microseconds since epoch).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        as_of: Option<u64>,
    },

    /// Initialize a state cell (only if it doesn't exist).
    /// Returns: `Output::Version`
    StateInit {
        /// Target branch (defaults to "default").
        #[serde(default, skip_serializing_if = "Option::is_none")]
        branch: Option<BranchId>,
        /// Target space (defaults to "default").
        #[serde(default, skip_serializing_if = "Option::is_none")]
        space: Option<String>,
        /// Cell name.
        cell: String,
        /// Initial value.
        value: Value,
    },

    /// Delete a state cell.
    /// Returns: `Output::Bool` (true if cell existed)
    StateDelete {
        /// Target branch (defaults to "default").
        #[serde(default, skip_serializing_if = "Option::is_none")]
        branch: Option<BranchId>,
        /// Target space (defaults to "default").
        #[serde(default, skip_serializing_if = "Option::is_none")]
        space: Option<String>,
        /// Cell name.
        cell: String,
    },

    /// List state cell names with optional prefix filter.
    /// Returns: `Output::Keys`
    StateList {
        /// Target branch (defaults to "default").
        #[serde(default, skip_serializing_if = "Option::is_none")]
        branch: Option<BranchId>,
        /// Target space (defaults to "default").
        #[serde(default, skip_serializing_if = "Option::is_none")]
        space: Option<String>,
        /// Optional cell name prefix filter.
        prefix: Option<String>,
        /// Optional timestamp for time-travel reads (microseconds since epoch).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        as_of: Option<u64>,
    },

    // ==================== Vector (7 MVP) ====================
    // MVP: upsert, get, delete, search, create_collection, delete_collection, list_collections
    /// Insert or update a vector.
    /// Returns: `Output::Version`
    VectorUpsert {
        /// Target branch (defaults to "default").
        #[serde(default, skip_serializing_if = "Option::is_none")]
        branch: Option<BranchId>,
        /// Target space (defaults to "default").
        #[serde(default, skip_serializing_if = "Option::is_none")]
        space: Option<String>,
        /// Collection name.
        collection: String,
        /// Vector key.
        key: String,
        /// Embedding vector data.
        vector: Vec<f32>,
        /// Optional metadata to associate with the vector.
        metadata: Option<Value>,
    },

    /// Get a vector by key.
    /// Returns: `Output::MaybeVectorData`
    VectorGet {
        /// Target branch (defaults to "default").
        #[serde(default, skip_serializing_if = "Option::is_none")]
        branch: Option<BranchId>,
        /// Target space (defaults to "default").
        #[serde(default, skip_serializing_if = "Option::is_none")]
        space: Option<String>,
        /// Collection name.
        collection: String,
        /// Vector key.
        key: String,
        /// Optional timestamp for time-travel reads (microseconds since epoch).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        as_of: Option<u64>,
    },

    /// Delete a vector.
    /// Returns: `Output::Bool`
    VectorDelete {
        /// Target branch (defaults to "default").
        #[serde(default, skip_serializing_if = "Option::is_none")]
        branch: Option<BranchId>,
        /// Target space (defaults to "default").
        #[serde(default, skip_serializing_if = "Option::is_none")]
        space: Option<String>,
        /// Collection name.
        collection: String,
        /// Vector key.
        key: String,
    },

    /// Search for similar vectors.
    /// Returns: `Output::VectorMatches`
    VectorSearch {
        /// Target branch (defaults to "default").
        #[serde(default, skip_serializing_if = "Option::is_none")]
        branch: Option<BranchId>,
        /// Target space (defaults to "default").
        #[serde(default, skip_serializing_if = "Option::is_none")]
        space: Option<String>,
        /// Collection to search.
        collection: String,
        /// Query embedding vector.
        query: Vec<f32>,
        /// Number of nearest neighbors to return.
        k: u64,
        /// Optional metadata filters.
        filter: Option<Vec<MetadataFilter>>,
        /// Optional distance metric override.
        metric: Option<DistanceMetric>,
        /// Optional timestamp for time-travel reads (microseconds since epoch).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        as_of: Option<u64>,
    },

    /// Create a collection with explicit configuration.
    /// Returns: `Output::Version`
    VectorCreateCollection {
        /// Target branch (defaults to "default").
        #[serde(default, skip_serializing_if = "Option::is_none")]
        branch: Option<BranchId>,
        /// Target space (defaults to "default").
        #[serde(default, skip_serializing_if = "Option::is_none")]
        space: Option<String>,
        /// Collection name.
        collection: String,
        /// Vector dimensionality.
        dimension: u64,
        /// Distance metric for similarity search.
        metric: DistanceMetric,
    },

    /// Delete a collection.
    /// Returns: `Output::Bool`
    VectorDeleteCollection {
        /// Target branch (defaults to "default").
        #[serde(default, skip_serializing_if = "Option::is_none")]
        branch: Option<BranchId>,
        /// Target space (defaults to "default").
        #[serde(default, skip_serializing_if = "Option::is_none")]
        space: Option<String>,
        /// Collection name.
        collection: String,
    },

    /// List all collections in a branch.
    /// Returns: `Output::VectorCollectionList`
    VectorListCollections {
        /// Target branch (defaults to "default").
        #[serde(default, skip_serializing_if = "Option::is_none")]
        branch: Option<BranchId>,
        /// Target space (defaults to "default").
        #[serde(default, skip_serializing_if = "Option::is_none")]
        space: Option<String>,
    },

    /// Get detailed statistics for a single collection.
    /// Returns: `Output::VectorCollectionList` (with single entry)
    VectorCollectionStats {
        /// Target branch (defaults to "default").
        #[serde(default, skip_serializing_if = "Option::is_none")]
        branch: Option<BranchId>,
        /// Target space (defaults to "default").
        #[serde(default, skip_serializing_if = "Option::is_none")]
        space: Option<String>,
        /// Collection name.
        collection: String,
    },

    /// Batch insert or update multiple vectors.
    /// Returns: `Output::Versions`
    VectorBatchUpsert {
        /// Target branch (defaults to "default").
        #[serde(default, skip_serializing_if = "Option::is_none")]
        branch: Option<BranchId>,
        /// Target space (defaults to "default").
        #[serde(default, skip_serializing_if = "Option::is_none")]
        space: Option<String>,
        /// Collection name.
        collection: String,
        /// Vector entries to upsert.
        entries: Vec<BatchVectorEntry>,
    },

    // ==================== Branch (5 MVP) ====================
    /// Create a new branch.
    /// Returns: `Output::BranchWithVersion`
    BranchCreate {
        /// Optional branch name (auto-generated UUID if omitted).
        branch_id: Option<String>,
        /// Optional metadata to attach to the branch.
        metadata: Option<Value>,
    },

    /// Get branch info.
    /// Returns: `Output::MaybeBranchInfo`
    BranchGet {
        /// Branch to look up.
        branch: BranchId,
    },

    /// List all branches.
    /// Returns: `Output::BranchInfoList`
    BranchList {
        /// Optional status filter.
        state: Option<BranchStatus>,
        /// Maximum number of branches to return.
        limit: Option<u64>,
        /// Number of branches to skip.
        offset: Option<u64>,
    },

    /// Check if a branch exists.
    /// Returns: `Output::Bool`
    BranchExists {
        /// Branch to check.
        branch: BranchId,
    },

    /// Delete a branch and all its data (cascading delete).
    /// Returns: `Output::Unit`
    BranchDelete {
        /// Branch to delete.
        branch: BranchId,
    },

    // ==================== Transaction (5) ====================
    /// Begin a new transaction.
    /// Returns: `Output::TxnBegun`
    TxnBegin {
        /// Target branch (defaults to "default").
        #[serde(default, skip_serializing_if = "Option::is_none")]
        branch: Option<BranchId>,
        /// Transaction configuration options.
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
    // Note: Branch-level retention is handled via BranchSetRetention/BranchGetRetention
    // These are database-wide retention operations
    /// Apply retention policy (trigger garbage collection).
    /// Returns: `Output::RetentionResult`
    RetentionApply {
        /// Target branch (defaults to "default").
        #[serde(default, skip_serializing_if = "Option::is_none")]
        branch: Option<BranchId>,
    },

    /// Get retention statistics.
    /// Returns: `Output::RetentionStats`
    RetentionStats {
        /// Target branch (defaults to "default").
        #[serde(default, skip_serializing_if = "Option::is_none")]
        branch: Option<BranchId>,
    },

    /// Preview what would be deleted by retention policy.
    /// Returns: `Output::RetentionPreview`
    RetentionPreview {
        /// Target branch (defaults to "default").
        #[serde(default, skip_serializing_if = "Option::is_none")]
        branch: Option<BranchId>,
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

    /// Get the available time range for a branch.
    /// Returns: `Output::TimeRange`
    TimeRange {
        /// Target branch (defaults to "default").
        #[serde(default, skip_serializing_if = "Option::is_none")]
        branch: Option<BranchId>,
    },

    // ==================== Bundle (3) ====================
    /// Export a branch to a .branchbundle.tar.zst archive.
    /// Returns: `Output::BranchExported`
    BranchExport {
        /// Branch to export.
        branch_id: String,
        /// Output file path.
        path: String,
    },

    /// Import a branch from a .branchbundle.tar.zst archive.
    /// Returns: `Output::BranchImported`
    BranchImport {
        /// Path to the bundle archive.
        path: String,
    },

    /// Validate a .branchbundle.tar.zst archive without importing.
    /// Returns: `Output::BundleValidated`
    BranchBundleValidate {
        /// Path to the bundle archive.
        path: String,
    },

    // ==================== Intelligence (2) ====================
    /// Configure an external model endpoint for query expansion.
    /// Returns: `Output::Unit`
    ConfigureModel {
        /// OpenAI-compatible API endpoint (e.g. "http://localhost:11434/v1").
        endpoint: String,
        /// Model name (e.g. "qwen3:1.7b").
        model: String,
        /// Optional API key for authenticated endpoints.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        api_key: Option<String>,
        /// Request timeout in milliseconds (default: 5000).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        timeout_ms: Option<u64>,
    },

    /// Search across multiple primitives using a structured query.
    /// Returns: `Output::SearchResults`
    Search {
        /// Target branch (defaults to "default").
        #[serde(default, skip_serializing_if = "Option::is_none")]
        branch: Option<BranchId>,
        /// Target space (defaults to "default").
        #[serde(default, skip_serializing_if = "Option::is_none")]
        space: Option<String>,
        /// Structured search query.
        search: SearchQuery,
    },

    // ==================== Space (4) ====================
    /// List spaces in a branch.
    /// Returns: `Output::SpaceList`
    SpaceList {
        /// Target branch (defaults to "default").
        #[serde(default, skip_serializing_if = "Option::is_none")]
        branch: Option<BranchId>,
    },

    /// Create a space explicitly.
    /// Returns: `Output::Unit`
    SpaceCreate {
        /// Target branch (defaults to "default").
        #[serde(default, skip_serializing_if = "Option::is_none")]
        branch: Option<BranchId>,
        /// Space name.
        space: String,
    },

    /// Delete a space (must be empty unless force=true).
    /// Returns: `Output::Unit`
    SpaceDelete {
        /// Target branch (defaults to "default").
        #[serde(default, skip_serializing_if = "Option::is_none")]
        branch: Option<BranchId>,
        /// Space name.
        space: String,
        /// If true, delete even if the space is non-empty.
        #[serde(default)]
        force: bool,
    },

    /// Check if a space exists.
    /// Returns: `Output::Bool`
    SpaceExists {
        /// Target branch (defaults to "default").
        #[serde(default, skip_serializing_if = "Option::is_none")]
        branch: Option<BranchId>,
        /// Space name.
        space: String,
    },
}

impl Command {
    /// Returns `true` if this command performs a write operation.
    ///
    /// Used by the access-mode guard to reject writes when the database
    /// is opened in read-only mode.
    pub fn is_write(&self) -> bool {
        matches!(
            self,
            Command::KvPut { .. }
                | Command::KvDelete { .. }
                | Command::JsonSet { .. }
                | Command::JsonDelete { .. }
                | Command::EventAppend { .. }
                | Command::StateSet { .. }
                | Command::StateCas { .. }
                | Command::StateInit { .. }
                | Command::StateDelete { .. }
                | Command::VectorUpsert { .. }
                | Command::VectorDelete { .. }
                | Command::VectorCreateCollection { .. }
                | Command::VectorDeleteCollection { .. }
                | Command::VectorBatchUpsert { .. }
                | Command::BranchCreate { .. }
                | Command::BranchDelete { .. }
                | Command::SpaceCreate { .. }
                | Command::SpaceDelete { .. }
                | Command::TxnBegin { .. }
                | Command::TxnCommit
                | Command::TxnRollback
                | Command::RetentionApply { .. }
                | Command::Flush
                | Command::Compact
                | Command::BranchExport { .. }
                | Command::BranchImport { .. }
                | Command::ConfigureModel { .. }
        )
    }

    /// Returns the variant name as a static string.
    ///
    /// The exhaustive match ensures the compiler flags any new `Command`
    /// variant that is added without a corresponding name.
    pub fn name(&self) -> &'static str {
        match self {
            Command::KvPut { .. } => "KvPut",
            Command::KvGet { .. } => "KvGet",
            Command::KvDelete { .. } => "KvDelete",
            Command::KvList { .. } => "KvList",
            Command::KvGetv { .. } => "KvGetv",
            Command::JsonSet { .. } => "JsonSet",
            Command::JsonGet { .. } => "JsonGet",
            Command::JsonDelete { .. } => "JsonDelete",
            Command::JsonGetv { .. } => "JsonGetv",
            Command::JsonList { .. } => "JsonList",
            Command::EventAppend { .. } => "EventAppend",
            Command::EventGet { .. } => "EventGet",
            Command::EventGetByType { .. } => "EventGetByType",
            Command::EventLen { .. } => "EventLen",
            Command::StateSet { .. } => "StateSet",
            Command::StateGet { .. } => "StateGet",
            Command::StateCas { .. } => "StateCas",
            Command::StateGetv { .. } => "StateGetv",
            Command::StateInit { .. } => "StateInit",
            Command::StateDelete { .. } => "StateDelete",
            Command::StateList { .. } => "StateList",
            Command::VectorUpsert { .. } => "VectorUpsert",
            Command::VectorGet { .. } => "VectorGet",
            Command::VectorDelete { .. } => "VectorDelete",
            Command::VectorSearch { .. } => "VectorSearch",
            Command::VectorCreateCollection { .. } => "VectorCreateCollection",
            Command::VectorDeleteCollection { .. } => "VectorDeleteCollection",
            Command::VectorListCollections { .. } => "VectorListCollections",
            Command::VectorCollectionStats { .. } => "VectorCollectionStats",
            Command::VectorBatchUpsert { .. } => "VectorBatchUpsert",
            Command::BranchCreate { .. } => "BranchCreate",
            Command::BranchGet { .. } => "BranchGet",
            Command::BranchList { .. } => "BranchList",
            Command::BranchExists { .. } => "BranchExists",
            Command::BranchDelete { .. } => "BranchDelete",
            Command::TxnBegin { .. } => "TxnBegin",
            Command::TxnCommit => "TxnCommit",
            Command::TxnRollback => "TxnRollback",
            Command::TxnInfo => "TxnInfo",
            Command::TxnIsActive => "TxnIsActive",
            Command::RetentionApply { .. } => "RetentionApply",
            Command::RetentionStats { .. } => "RetentionStats",
            Command::RetentionPreview { .. } => "RetentionPreview",
            Command::Ping => "Ping",
            Command::Info => "Info",
            Command::Flush => "Flush",
            Command::Compact => "Compact",
            Command::TimeRange { .. } => "TimeRange",
            Command::BranchExport { .. } => "BranchExport",
            Command::BranchImport { .. } => "BranchImport",
            Command::BranchBundleValidate { .. } => "BranchBundleValidate",
            Command::ConfigureModel { .. } => "ConfigureModel",
            Command::Search { .. } => "Search",
            Command::SpaceList { .. } => "SpaceList",
            Command::SpaceCreate { .. } => "SpaceCreate",
            Command::SpaceDelete { .. } => "SpaceDelete",
            Command::SpaceExists { .. } => "SpaceExists",
        }
    }

    /// Fill in the default branch and space for any data command where they are `None`.
    ///
    /// Called by the executor before dispatch so handlers always receive a
    /// concrete `BranchId` and space name.
    pub fn resolve_defaults(&mut self) {
        macro_rules! resolve_branch {
            ($branch:expr) => {
                if $branch.is_none() {
                    *$branch = Some(BranchId::default());
                }
            };
        }
        macro_rules! resolve_space {
            ($space:expr) => {
                if $space.is_none() {
                    *$space = Some("default".to_string());
                }
            };
        }

        match self {
            // KV
            Command::KvPut { branch, space, .. }
            | Command::KvGet { branch, space, .. }
            | Command::KvDelete { branch, space, .. }
            | Command::KvList { branch, space, .. }
            | Command::KvGetv { branch, space, .. }
            // JSON
            | Command::JsonSet { branch, space, .. }
            | Command::JsonGet { branch, space, .. }
            | Command::JsonGetv { branch, space, .. }
            | Command::JsonDelete { branch, space, .. }
            | Command::JsonList { branch, space, .. }
            // Event (4 MVP)
            | Command::EventAppend { branch, space, .. }
            | Command::EventGet { branch, space, .. }
            | Command::EventGetByType { branch, space, .. }
            | Command::EventLen { branch, space, .. }
            // State
            | Command::StateSet { branch, space, .. }
            | Command::StateGet { branch, space, .. }
            | Command::StateGetv { branch, space, .. }
            | Command::StateCas { branch, space, .. }
            | Command::StateInit { branch, space, .. }
            | Command::StateDelete { branch, space, .. }
            | Command::StateList { branch, space, .. }
            // Vector (7 MVP)
            | Command::VectorUpsert { branch, space, .. }
            | Command::VectorGet { branch, space, .. }
            | Command::VectorDelete { branch, space, .. }
            | Command::VectorSearch { branch, space, .. }
            | Command::VectorCreateCollection { branch, space, .. }
            | Command::VectorDeleteCollection { branch, space, .. }
            | Command::VectorListCollections { branch, space, .. }
            | Command::VectorCollectionStats { branch, space, .. }
            | Command::VectorBatchUpsert { branch, space, .. }
            // Intelligence
            | Command::Search { branch, space, .. } => {
                resolve_branch!(branch);
                resolve_space!(space);
            }

            // Retention, Transaction begin, TimeRange — only have branch, no space
            Command::RetentionApply { branch, .. }
            | Command::RetentionStats { branch, .. }
            | Command::RetentionPreview { branch, .. }
            | Command::TxnBegin { branch, .. }
            | Command::TimeRange { branch, .. } => {
                resolve_branch!(branch);
            }

            // Space commands — only have branch, space is explicit
            Command::SpaceList { branch, .. }
            | Command::SpaceCreate { branch, .. }
            | Command::SpaceDelete { branch, .. }
            | Command::SpaceExists { branch, .. } => {
                resolve_branch!(branch);
            }

            // Branch lifecycle, Transaction, and Database commands have no
            // optional branch to resolve.
            Command::BranchCreate { .. }
            | Command::BranchGet { .. }
            | Command::BranchList { .. }
            | Command::BranchExists { .. }
            | Command::BranchDelete { .. }
            | Command::TxnCommit
            | Command::TxnRollback
            | Command::TxnInfo
            | Command::TxnIsActive
            | Command::Ping
            | Command::Info
            | Command::Flush
            | Command::Compact
            | Command::BranchExport { .. }
            | Command::BranchImport { .. }
            | Command::BranchBundleValidate { .. }
            | Command::ConfigureModel { .. } => {}
        }
    }

    /// Backwards-compatible alias for resolve_defaults
    pub fn resolve_default_branch(&mut self) {
        self.resolve_defaults();
    }
}
