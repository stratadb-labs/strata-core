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
/// ```ignore
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
        #[serde(default, skip_serializing_if = "Option::is_none")]
        branch: Option<BranchId>,
        key: String,
        value: Value,
    },

    /// Get a value by key.
    /// Returns: `Output::MaybeValue`
    KvGet {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        branch: Option<BranchId>,
        key: String,
    },

    /// Delete a key.
    /// Returns: `Output::Bool` (true if key existed)
    KvDelete {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        branch: Option<BranchId>,
        key: String,
    },

    /// List keys with optional prefix filter.
    /// Returns: `Output::Keys`
    KvList {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        branch: Option<BranchId>,
        prefix: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        cursor: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        limit: Option<u64>,
    },

    /// Get full version history for a key.
    /// Returns: `Output::VersionHistory`
    KvGetv {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        branch: Option<BranchId>,
        key: String,
    },

    // ==================== JSON (4 MVP) ====================
    /// Set a value at a path in a JSON document.
    /// Returns: `Output::Version`
    JsonSet {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        branch: Option<BranchId>,
        key: String,
        path: String,
        value: Value,
    },

    /// Get a value at a path from a JSON document.
    /// Returns: `Output::MaybeVersioned`
    JsonGet {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        branch: Option<BranchId>,
        key: String,
        path: String,
    },

    /// Delete a value at a path from a JSON document.
    /// Returns: `Output::Uint` (count of elements removed)
    JsonDelete {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        branch: Option<BranchId>,
        key: String,
        path: String,
    },

    /// Get full version history for a JSON document.
    /// Returns: `Output::VersionHistory`
    JsonGetv {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        branch: Option<BranchId>,
        key: String,
    },

    /// List JSON documents with cursor-based pagination.
    /// Returns: `Output::JsonListResult`
    JsonList {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        branch: Option<BranchId>,
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
        branch: Option<BranchId>,
        event_type: String,
        payload: Value,
    },

    /// Read a specific event by sequence number.
    /// Returns: `Output::MaybeVersioned`
    EventRead {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        branch: Option<BranchId>,
        sequence: u64,
    },

    /// Read all events of a specific type.
    /// Returns: `Output::VersionedValues`
    EventReadByType {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        branch: Option<BranchId>,
        event_type: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        limit: Option<u64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        after_sequence: Option<u64>,
    },

    /// Get the total count of events in the log.
    /// Returns: `Output::Uint`
    EventLen {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        branch: Option<BranchId>,
    },

    // ==================== State (4 MVP) ====================
    // MVP: set, read, cas, init
    /// Set a state cell value (unconditional write).
    /// Returns: `Output::Version`
    StateSet {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        branch: Option<BranchId>,
        cell: String,
        value: Value,
    },

    /// Read a state cell value.
    /// Returns: `Output::MaybeVersioned`
    StateRead {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        branch: Option<BranchId>,
        cell: String,
    },

    /// Compare-and-swap on a state cell.
    /// Returns: `Output::MaybeVersion`
    StateCas {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        branch: Option<BranchId>,
        cell: String,
        expected_counter: Option<u64>,
        value: Value,
    },

    /// Get full version history for a state cell.
    /// Returns: `Output::VersionHistory`
    StateReadv {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        branch: Option<BranchId>,
        cell: String,
    },

    /// Initialize a state cell (only if it doesn't exist).
    /// Returns: `Output::Version`
    StateInit {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        branch: Option<BranchId>,
        cell: String,
        value: Value,
    },

    /// Delete a state cell.
    /// Returns: `Output::Bool` (true if cell existed)
    StateDelete {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        branch: Option<BranchId>,
        cell: String,
    },

    /// List state cell names with optional prefix filter.
    /// Returns: `Output::Keys`
    StateList {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        branch: Option<BranchId>,
        prefix: Option<String>,
    },

    // ==================== Vector (7 MVP) ====================
    // MVP: upsert, get, delete, search, create_collection, delete_collection, list_collections
    /// Insert or update a vector.
    /// Returns: `Output::Version`
    VectorUpsert {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        branch: Option<BranchId>,
        collection: String,
        key: String,
        vector: Vec<f32>,
        metadata: Option<Value>,
    },

    /// Get a vector by key.
    /// Returns: `Output::MaybeVectorData`
    VectorGet {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        branch: Option<BranchId>,
        collection: String,
        key: String,
    },

    /// Delete a vector.
    /// Returns: `Output::Bool`
    VectorDelete {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        branch: Option<BranchId>,
        collection: String,
        key: String,
    },

    /// Search for similar vectors.
    /// Returns: `Output::VectorMatches`
    VectorSearch {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        branch: Option<BranchId>,
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
        branch: Option<BranchId>,
        collection: String,
        dimension: u64,
        metric: DistanceMetric,
    },

    /// Delete a collection.
    /// Returns: `Output::Bool`
    VectorDeleteCollection {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        branch: Option<BranchId>,
        collection: String,
    },

    /// List all collections in a branch.
    /// Returns: `Output::VectorCollectionList`
    VectorListCollections {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        branch: Option<BranchId>,
    },

    /// Get detailed statistics for a single collection.
    /// Returns: `Output::VectorCollectionList` (with single entry)
    VectorCollectionStats {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        branch: Option<BranchId>,
        collection: String,
    },

    /// Batch insert or update multiple vectors.
    /// Returns: `Output::Versions`
    VectorBatchUpsert {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        branch: Option<BranchId>,
        collection: String,
        entries: Vec<BatchVectorEntry>,
    },

    // ==================== Branch (5 MVP) ====================
    /// Create a new branch.
    /// Returns: `Output::BranchWithVersion`
    BranchCreate {
        branch_id: Option<String>,
        metadata: Option<Value>,
    },

    /// Get branch info.
    /// Returns: `Output::MaybeBranchInfo`
    BranchGet { branch: BranchId },

    /// List all branches.
    /// Returns: `Output::BranchInfoList`
    BranchList {
        state: Option<BranchStatus>,
        limit: Option<u64>,
        offset: Option<u64>,
    },

    /// Check if a branch exists.
    /// Returns: `Output::Bool`
    BranchExists { branch: BranchId },

    /// Delete a branch and all its data (cascading delete).
    /// Returns: `Output::Unit`
    BranchDelete { branch: BranchId },

    // ==================== Transaction (5) ====================
    /// Begin a new transaction.
    /// Returns: `Output::TxnBegun`
    TxnBegin {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        branch: Option<BranchId>,
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
        #[serde(default, skip_serializing_if = "Option::is_none")]
        branch: Option<BranchId>,
    },

    /// Get retention statistics.
    /// Returns: `Output::RetentionStats`
    RetentionStats {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        branch: Option<BranchId>,
    },

    /// Preview what would be deleted by retention policy.
    /// Returns: `Output::RetentionPreview`
    RetentionPreview {
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

    // ==================== Bundle (3) ====================
    /// Export a branch to a .branchbundle.tar.zst archive.
    /// Returns: `Output::BranchExported`
    BranchExport { branch_id: String, path: String },

    /// Import a branch from a .branchbundle.tar.zst archive.
    /// Returns: `Output::BranchImported`
    BranchImport { path: String },

    /// Validate a .branchbundle.tar.zst archive without importing.
    /// Returns: `Output::BundleValidated`
    BranchBundleValidate { path: String },

    // ==================== Intelligence (1) ====================
    /// Search across multiple primitives.
    /// Returns: `Output::SearchResults`
    Search {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        branch: Option<BranchId>,
        query: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        k: Option<u64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        primitives: Option<Vec<String>>,
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
                | Command::TxnBegin { .. }
                | Command::TxnCommit
                | Command::TxnRollback
                | Command::RetentionApply { .. }
                | Command::Flush
                | Command::Compact
                | Command::BranchExport { .. }
                | Command::BranchImport { .. }
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
            Command::EventRead { .. } => "EventRead",
            Command::EventReadByType { .. } => "EventReadByType",
            Command::EventLen { .. } => "EventLen",
            Command::StateSet { .. } => "StateSet",
            Command::StateRead { .. } => "StateRead",
            Command::StateCas { .. } => "StateCas",
            Command::StateReadv { .. } => "StateReadv",
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
            Command::BranchExport { .. } => "BranchExport",
            Command::BranchImport { .. } => "BranchImport",
            Command::BranchBundleValidate { .. } => "BranchBundleValidate",
            Command::Search { .. } => "Search",
        }
    }

    /// Fill in the default branch for any data command where branch is `None`.
    ///
    /// Called by the executor before dispatch so handlers always receive a
    /// concrete `BranchId`.
    pub fn resolve_default_branch(&mut self) {
        macro_rules! resolve {
            ($branch:expr) => {
                if $branch.is_none() {
                    *$branch = Some(BranchId::default());
                }
            };
        }

        match self {
            // KV
            Command::KvPut { branch, .. }
            | Command::KvGet { branch, .. }
            | Command::KvDelete { branch, .. }
            | Command::KvList { branch, .. }
            | Command::KvGetv { branch, .. }
            // JSON
            | Command::JsonSet { branch, .. }
            | Command::JsonGet { branch, .. }
            | Command::JsonGetv { branch, .. }
            | Command::JsonDelete { branch, .. }
            | Command::JsonList { branch, .. }
            // Event (4 MVP)
            | Command::EventAppend { branch, .. }
            | Command::EventRead { branch, .. }
            | Command::EventReadByType { branch, .. }
            | Command::EventLen { branch, .. }
            // State
            | Command::StateSet { branch, .. }
            | Command::StateRead { branch, .. }
            | Command::StateReadv { branch, .. }
            | Command::StateCas { branch, .. }
            | Command::StateInit { branch, .. }
            | Command::StateDelete { branch, .. }
            | Command::StateList { branch, .. }
            // Vector (7 MVP)
            | Command::VectorUpsert { branch, .. }
            | Command::VectorGet { branch, .. }
            | Command::VectorDelete { branch, .. }
            | Command::VectorSearch { branch, .. }
            | Command::VectorCreateCollection { branch, .. }
            | Command::VectorDeleteCollection { branch, .. }
            | Command::VectorListCollections { branch, .. }
            | Command::VectorCollectionStats { branch, .. }
            | Command::VectorBatchUpsert { branch, .. }
            // Retention
            | Command::RetentionApply { branch, .. }
            | Command::RetentionStats { branch, .. }
            | Command::RetentionPreview { branch, .. }
            // Transaction begin
            | Command::TxnBegin { branch, .. }
            // Intelligence
            | Command::Search { branch, .. } => {
                resolve!(branch);
            }

            // Branch lifecycle (5 MVP), Transaction, and Database commands have no
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
            | Command::BranchBundleValidate { .. } => {}
        }
    }
}
