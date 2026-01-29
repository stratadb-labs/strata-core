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
/// | Event | 11 | Event log operations |
/// | State | 8 | State cell operations |
/// | Vector | 19 | Vector store operations |
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

    // ==================== JSON (17) ====================
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

    /// Merge a value at a path (RFC 7396 JSON Merge Patch).
    /// Returns: `Output::Version`
    JsonMerge {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        run: Option<RunId>,
        key: String,
        path: String,
        patch: Value,
    },

    /// Get version history for a JSON document.
    /// Returns: `Output::VersionedValues`
    JsonHistory {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        run: Option<RunId>,
        key: String,
        limit: Option<u64>,
        before: Option<u64>,
    },

    /// Check if a JSON document exists.
    /// Returns: `Output::Bool`
    JsonExists {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        run: Option<RunId>,
        key: String,
    },

    /// Get the current version of a JSON document.
    /// Returns: `Output::MaybeUint`
    JsonGetVersion {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        run: Option<RunId>,
        key: String,
    },

    /// Full-text search across JSON documents.
    /// Returns: `Output::JsonSearchHits`
    JsonSearch {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        run: Option<RunId>,
        query: String,
        k: u64,
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

    /// Compare-and-swap: update if version matches.
    /// Returns: `Output::Version`
    JsonCas {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        run: Option<RunId>,
        key: String,
        expected_version: u64,
        path: String,
        value: Value,
    },

    /// Query documents by exact field match.
    /// Returns: `Output::Keys`
    JsonQuery {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        run: Option<RunId>,
        path: String,
        value: Value,
        limit: u64,
    },

    /// Count JSON documents in the store.
    /// Returns: `Output::Uint`
    JsonCount {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        run: Option<RunId>,
    },

    /// Batch get multiple JSON documents.
    /// Returns: `Output::MaybeVersionedValues`
    JsonBatchGet {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        run: Option<RunId>,
        keys: Vec<String>,
    },

    /// Batch create multiple JSON documents atomically.
    /// Returns: `Output::Versions`
    JsonBatchCreate {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        run: Option<RunId>,
        docs: Vec<(String, Value)>,
    },

    /// Atomically push values to an array at path.
    /// Returns: `Output::Uint` (new array length)
    JsonArrayPush {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        run: Option<RunId>,
        key: String,
        path: String,
        values: Vec<Value>,
    },

    /// Atomically increment a numeric value at path.
    /// Returns: `Output::Float`
    JsonIncrement {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        run: Option<RunId>,
        key: String,
        path: String,
        delta: f64,
    },

    /// Atomically pop a value from an array at path.
    /// Returns: `Output::Maybe` (the popped value)
    JsonArrayPop {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        run: Option<RunId>,
        key: String,
        path: String,
    },

    // ==================== Event (11) ====================
    /// Append an event to a stream.
    /// Returns: `Output::Version`
    EventAppend {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        run: Option<RunId>,
        stream: String,
        payload: Value,
    },

    /// Append multiple events atomically.
    /// Returns: `Output::Versions`
    EventAppendBatch {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        run: Option<RunId>,
        events: Vec<(String, Value)>,
    },

    /// Read events from a stream in ascending order.
    /// Returns: `Output::VersionedValues`
    EventRange {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        run: Option<RunId>,
        stream: String,
        start: Option<u64>,
        end: Option<u64>,
        limit: Option<u64>,
    },

    /// Get a specific event by sequence number.
    /// Returns: `Output::MaybeVersioned`
    EventRead {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        run: Option<RunId>,
        stream: String,
        sequence: u64,
    },

    /// Get the count of events in a stream.
    /// Returns: `Output::Uint`
    EventLen {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        run: Option<RunId>,
        stream: String,
    },

    /// Get the latest sequence number in a stream.
    /// Returns: `Output::MaybeUint`
    EventLatestSequence {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        run: Option<RunId>,
        stream: String,
    },

    /// Get stream metadata.
    /// Returns: `Output::StreamInfo`
    EventStreamInfo {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        run: Option<RunId>,
        stream: String,
    },

    /// Read events from a stream in descending order (newest first).
    /// Returns: `Output::VersionedValues`
    EventRevRange {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        run: Option<RunId>,
        stream: String,
        start: Option<u64>,
        end: Option<u64>,
        limit: Option<u64>,
    },

    /// List all streams (event types) in a run.
    /// Returns: `Output::Strings`
    EventStreams {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        run: Option<RunId>,
    },

    /// Get the latest event (head) of a stream.
    /// Returns: `Output::MaybeVersioned`
    EventHead {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        run: Option<RunId>,
        stream: String,
    },

    /// Verify the hash chain integrity of the event log.
    /// Returns: `Output::ChainVerification`
    EventVerifyChain {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        run: Option<RunId>,
    },

    // ==================== State (8) ====================
    // Note: state_transition, state_transition_or_init, state_get_or_init
    // are excluded as they require closures

    /// Set a state cell value.
    /// Returns: `Output::Version`
    StateSet {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        run: Option<RunId>,
        cell: String,
        value: Value,
    },

    /// Get a state cell value.
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

    /// Delete a state cell.
    /// Returns: `Output::Bool`
    StateDelete {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        run: Option<RunId>,
        cell: String,
    },

    /// Check if a state cell exists.
    /// Returns: `Output::Bool`
    StateExists {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        run: Option<RunId>,
        cell: String,
    },

    /// Get version history for a state cell.
    /// Returns: `Output::VersionedValues`
    StateHistory {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        run: Option<RunId>,
        cell: String,
        limit: Option<u64>,
        before: Option<u64>,
    },

    /// Initialize a state cell (only if it doesn't exist).
    /// Returns: `Output::Version`
    StateInit {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        run: Option<RunId>,
        cell: String,
        value: Value,
    },

    /// List all state cell names.
    /// Returns: `Output::Strings`
    StateList {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        run: Option<RunId>,
    },

    // ==================== Vector (19) ====================
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

    /// Get collection information.
    /// Returns: `Output::MaybeCollectionInfo`
    VectorGetCollection {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        run: Option<RunId>,
        collection: String,
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
    /// Returns: `Output::CollectionInfos`
    VectorListCollections {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        run: Option<RunId>,
    },

    /// Check if a collection exists.
    /// Returns: `Output::Bool`
    VectorCollectionExists {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        run: Option<RunId>,
        collection: String,
    },

    /// Get the count of vectors in a collection.
    /// Returns: `Output::Uint`
    VectorCount {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        run: Option<RunId>,
        collection: String,
    },

    /// Batch insert or update vectors.
    /// Returns: `Output::VectorBatchResults`
    VectorUpsertBatch {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        run: Option<RunId>,
        collection: String,
        vectors: Vec<VectorEntry>,
    },

    /// Batch get vectors.
    /// Returns: `Output::MaybeVectorDatas`
    VectorGetBatch {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        run: Option<RunId>,
        collection: String,
        keys: Vec<String>,
    },

    /// Batch delete vectors.
    /// Returns: `Output::Bools`
    VectorDeleteBatch {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        run: Option<RunId>,
        collection: String,
        keys: Vec<String>,
    },

    /// Get version history for a vector.
    /// Returns: `Output::VectorHistoryResult`
    VectorHistory {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        run: Option<RunId>,
        collection: String,
        key: String,
        limit: Option<u64>,
        before_version: Option<u64>,
    },

    /// Get a vector at a specific version.
    /// Returns: `Output::MaybeVectorData`
    VectorGetAt {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        run: Option<RunId>,
        collection: String,
        key: String,
        version: u64,
    },

    /// List all vector keys in a collection.
    /// Returns: `Output::Keys`
    VectorListKeys {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        run: Option<RunId>,
        collection: String,
        limit: Option<u64>,
        cursor: Option<String>,
    },

    /// Scan vectors in a collection.
    /// Returns: `Output::VectorScanResult`
    VectorScan {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        run: Option<RunId>,
        collection: String,
        limit: Option<u64>,
        cursor: Option<String>,
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
            | Command::JsonMerge { run, .. }
            | Command::JsonHistory { run, .. }
            | Command::JsonExists { run, .. }
            | Command::JsonGetVersion { run, .. }
            | Command::JsonSearch { run, .. }
            | Command::JsonList { run, .. }
            | Command::JsonCas { run, .. }
            | Command::JsonQuery { run, .. }
            | Command::JsonCount { run, .. }
            | Command::JsonBatchGet { run, .. }
            | Command::JsonBatchCreate { run, .. }
            | Command::JsonArrayPush { run, .. }
            | Command::JsonIncrement { run, .. }
            | Command::JsonArrayPop { run, .. }
            // Event
            | Command::EventAppend { run, .. }
            | Command::EventAppendBatch { run, .. }
            | Command::EventRange { run, .. }
            | Command::EventRead { run, .. }
            | Command::EventLen { run, .. }
            | Command::EventLatestSequence { run, .. }
            | Command::EventStreamInfo { run, .. }
            | Command::EventRevRange { run, .. }
            | Command::EventStreams { run, .. }
            | Command::EventHead { run, .. }
            | Command::EventVerifyChain { run, .. }
            // State
            | Command::StateSet { run, .. }
            | Command::StateRead { run, .. }
            | Command::StateCas { run, .. }
            | Command::StateDelete { run, .. }
            | Command::StateExists { run, .. }
            | Command::StateHistory { run, .. }
            | Command::StateInit { run, .. }
            | Command::StateList { run, .. }
            // Vector
            | Command::VectorUpsert { run, .. }
            | Command::VectorGet { run, .. }
            | Command::VectorDelete { run, .. }
            | Command::VectorSearch { run, .. }
            | Command::VectorGetCollection { run, .. }
            | Command::VectorCreateCollection { run, .. }
            | Command::VectorDeleteCollection { run, .. }
            | Command::VectorListCollections { run, .. }
            | Command::VectorCollectionExists { run, .. }
            | Command::VectorCount { run, .. }
            | Command::VectorUpsertBatch { run, .. }
            | Command::VectorGetBatch { run, .. }
            | Command::VectorDeleteBatch { run, .. }
            | Command::VectorHistory { run, .. }
            | Command::VectorGetAt { run, .. }
            | Command::VectorListKeys { run, .. }
            | Command::VectorScan { run, .. }
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
