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
/// | KV | 15 | Key-value operations |
/// | JSON | 17 | JSON document operations |
/// | Event | 11 | Event log operations |
/// | State | 8 | State cell operations |
/// | Vector | 19 | Vector store operations |
/// | Run | 24 | Run lifecycle operations |
/// | Transaction | 5 | Transaction control |
/// | Retention | 3 | Retention policy |
/// | Database | 4 | Database-level operations |
///
/// # Example
///
/// ```ignore
/// use strata_executor::{Command, RunId};
/// use strata_core::Value;
///
/// let cmd = Command::KvPut {
///     run: RunId::default(),
///     key: "foo".into(),
///     value: Value::Int(42),
/// };
///
/// let json = serde_json::to_string(&cmd)?;
/// // {"KvPut":{"run":"default","key":"foo","value":42}}
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum Command {
    // ==================== KV (15) ====================
    /// Put a key-value pair.
    /// Returns: `Output::Version`
    KvPut {
        run: RunId,
        key: String,
        value: Value,
    },

    /// Get a value by key.
    /// Returns: `Output::MaybeVersioned`
    KvGet {
        run: RunId,
        key: String,
    },

    /// Get a value at a specific version.
    /// Returns: `Output::Versioned`
    KvGetAt {
        run: RunId,
        key: String,
        version: u64,
    },

    /// Delete a key.
    /// Returns: `Output::Bool` (true if key existed)
    KvDelete {
        run: RunId,
        key: String,
    },

    /// Check if a key exists.
    /// Returns: `Output::Bool`
    KvExists {
        run: RunId,
        key: String,
    },

    /// Get version history for a key.
    /// Returns: `Output::VersionedValues`
    KvHistory {
        run: RunId,
        key: String,
        limit: Option<u64>,
        before: Option<u64>,
    },

    /// Atomic increment.
    /// Returns: `Output::Int` (new value)
    KvIncr {
        run: RunId,
        key: String,
        delta: i64,
    },

    /// Compare-and-swap by version.
    /// Returns: `Output::Bool` (true if swap succeeded)
    KvCasVersion {
        run: RunId,
        key: String,
        expected_version: Option<u64>,
        new_value: Value,
    },

    /// Compare-and-swap by value.
    /// Returns: `Output::Bool` (true if swap succeeded)
    KvCasValue {
        run: RunId,
        key: String,
        expected_value: Option<Value>,
        new_value: Value,
    },

    /// List keys with optional prefix filter.
    /// Returns: `Output::Keys`
    KvKeys {
        run: RunId,
        prefix: String,
        limit: Option<u64>,
    },

    /// Scan keys with cursor-based pagination.
    /// Returns: `Output::KvScanResult`
    KvScan {
        run: RunId,
        prefix: String,
        limit: u64,
        cursor: Option<String>,
    },

    /// Get multiple values.
    /// Returns: `Output::MaybeVersionedValues`
    KvMget {
        run: RunId,
        keys: Vec<String>,
    },

    /// Put multiple key-value pairs atomically.
    /// Returns: `Output::Version`
    KvMput {
        run: RunId,
        entries: Vec<(String, Value)>,
    },

    /// Delete multiple keys.
    /// Returns: `Output::Uint` (count of keys that existed)
    KvMdelete {
        run: RunId,
        keys: Vec<String>,
    },

    /// Check existence of multiple keys.
    /// Returns: `Output::Uint` (count of keys that exist)
    KvMexists {
        run: RunId,
        keys: Vec<String>,
    },

    // ==================== JSON (17) ====================
    /// Set a value at a path in a JSON document.
    /// Returns: `Output::Version`
    JsonSet {
        run: RunId,
        key: String,
        path: String,
        value: Value,
    },

    /// Get a value at a path from a JSON document.
    /// Returns: `Output::MaybeVersioned`
    JsonGet {
        run: RunId,
        key: String,
        path: String,
    },

    /// Delete a value at a path from a JSON document.
    /// Returns: `Output::Uint` (count of elements removed)
    JsonDelete {
        run: RunId,
        key: String,
        path: String,
    },

    /// Merge a value at a path (RFC 7396 JSON Merge Patch).
    /// Returns: `Output::Version`
    JsonMerge {
        run: RunId,
        key: String,
        path: String,
        patch: Value,
    },

    /// Get version history for a JSON document.
    /// Returns: `Output::VersionedValues`
    JsonHistory {
        run: RunId,
        key: String,
        limit: Option<u64>,
        before: Option<u64>,
    },

    /// Check if a JSON document exists.
    /// Returns: `Output::Bool`
    JsonExists {
        run: RunId,
        key: String,
    },

    /// Get the current version of a JSON document.
    /// Returns: `Output::MaybeUint`
    JsonGetVersion {
        run: RunId,
        key: String,
    },

    /// Full-text search across JSON documents.
    /// Returns: `Output::JsonSearchHits`
    JsonSearch {
        run: RunId,
        query: String,
        k: u64,
    },

    /// List JSON documents with cursor-based pagination.
    /// Returns: `Output::JsonListResult`
    JsonList {
        run: RunId,
        prefix: Option<String>,
        cursor: Option<String>,
        limit: u64,
    },

    /// Compare-and-swap: update if version matches.
    /// Returns: `Output::Version`
    JsonCas {
        run: RunId,
        key: String,
        expected_version: u64,
        path: String,
        value: Value,
    },

    /// Query documents by exact field match.
    /// Returns: `Output::Keys`
    JsonQuery {
        run: RunId,
        path: String,
        value: Value,
        limit: u64,
    },

    /// Count JSON documents in the store.
    /// Returns: `Output::Uint`
    JsonCount {
        run: RunId,
    },

    /// Batch get multiple JSON documents.
    /// Returns: `Output::MaybeVersionedValues`
    JsonBatchGet {
        run: RunId,
        keys: Vec<String>,
    },

    /// Batch create multiple JSON documents atomically.
    /// Returns: `Output::Versions`
    JsonBatchCreate {
        run: RunId,
        docs: Vec<(String, Value)>,
    },

    /// Atomically push values to an array at path.
    /// Returns: `Output::Uint` (new array length)
    JsonArrayPush {
        run: RunId,
        key: String,
        path: String,
        values: Vec<Value>,
    },

    /// Atomically increment a numeric value at path.
    /// Returns: `Output::Float`
    JsonIncrement {
        run: RunId,
        key: String,
        path: String,
        delta: f64,
    },

    /// Atomically pop a value from an array at path.
    /// Returns: `Output::Maybe` (the popped value)
    JsonArrayPop {
        run: RunId,
        key: String,
        path: String,
    },

    // ==================== Event (11) ====================
    /// Append an event to a stream.
    /// Returns: `Output::Version`
    EventAppend {
        run: RunId,
        stream: String,
        payload: Value,
    },

    /// Append multiple events atomically.
    /// Returns: `Output::Versions`
    EventAppendBatch {
        run: RunId,
        events: Vec<(String, Value)>,
    },

    /// Read events from a stream in ascending order.
    /// Returns: `Output::VersionedValues`
    EventRange {
        run: RunId,
        stream: String,
        start: Option<u64>,
        end: Option<u64>,
        limit: Option<u64>,
    },

    /// Get a specific event by sequence number.
    /// Returns: `Output::MaybeVersioned`
    EventGet {
        run: RunId,
        stream: String,
        sequence: u64,
    },

    /// Get the count of events in a stream.
    /// Returns: `Output::Uint`
    EventLen {
        run: RunId,
        stream: String,
    },

    /// Get the latest sequence number in a stream.
    /// Returns: `Output::MaybeUint`
    EventLatestSequence {
        run: RunId,
        stream: String,
    },

    /// Get stream metadata.
    /// Returns: `Output::StreamInfo`
    EventStreamInfo {
        run: RunId,
        stream: String,
    },

    /// Read events from a stream in descending order (newest first).
    /// Returns: `Output::VersionedValues`
    EventRevRange {
        run: RunId,
        stream: String,
        start: Option<u64>,
        end: Option<u64>,
        limit: Option<u64>,
    },

    /// List all streams (event types) in a run.
    /// Returns: `Output::Strings`
    EventStreams {
        run: RunId,
    },

    /// Get the latest event (head) of a stream.
    /// Returns: `Output::MaybeVersioned`
    EventHead {
        run: RunId,
        stream: String,
    },

    /// Verify the hash chain integrity of the event log.
    /// Returns: `Output::ChainVerification`
    EventVerifyChain {
        run: RunId,
    },

    // ==================== State (8) ====================
    // Note: state_transition, state_transition_or_init, state_get_or_init
    // are excluded as they require closures

    /// Set a state cell value.
    /// Returns: `Output::Version`
    StateSet {
        run: RunId,
        cell: String,
        value: Value,
    },

    /// Get a state cell value.
    /// Returns: `Output::MaybeVersioned`
    StateGet {
        run: RunId,
        cell: String,
    },

    /// Compare-and-swap on a state cell.
    /// Returns: `Output::MaybeVersion`
    StateCas {
        run: RunId,
        cell: String,
        expected_counter: Option<u64>,
        value: Value,
    },

    /// Delete a state cell.
    /// Returns: `Output::Bool`
    StateDelete {
        run: RunId,
        cell: String,
    },

    /// Check if a state cell exists.
    /// Returns: `Output::Bool`
    StateExists {
        run: RunId,
        cell: String,
    },

    /// Get version history for a state cell.
    /// Returns: `Output::VersionedValues`
    StateHistory {
        run: RunId,
        cell: String,
        limit: Option<u64>,
        before: Option<u64>,
    },

    /// Initialize a state cell (only if it doesn't exist).
    /// Returns: `Output::Version`
    StateInit {
        run: RunId,
        cell: String,
        value: Value,
    },

    /// List all state cell names.
    /// Returns: `Output::Strings`
    StateList {
        run: RunId,
    },

    // ==================== Vector (19) ====================
    /// Insert or update a vector.
    /// Returns: `Output::Version`
    VectorUpsert {
        run: RunId,
        collection: String,
        key: String,
        vector: Vec<f32>,
        metadata: Option<Value>,
    },

    /// Get a vector by key.
    /// Returns: `Output::MaybeVectorData`
    VectorGet {
        run: RunId,
        collection: String,
        key: String,
    },

    /// Delete a vector.
    /// Returns: `Output::Bool`
    VectorDelete {
        run: RunId,
        collection: String,
        key: String,
    },

    /// Search for similar vectors.
    /// Returns: `Output::VectorMatches`
    VectorSearch {
        run: RunId,
        collection: String,
        query: Vec<f32>,
        k: u64,
        filter: Option<Vec<MetadataFilter>>,
        metric: Option<DistanceMetric>,
    },

    /// Get collection information.
    /// Returns: `Output::MaybeCollectionInfo`
    VectorCollectionInfo {
        run: RunId,
        collection: String,
    },

    /// Create a collection with explicit configuration.
    /// Returns: `Output::Version`
    VectorCreateCollection {
        run: RunId,
        collection: String,
        dimension: u64,
        metric: DistanceMetric,
    },

    /// Delete a collection.
    /// Returns: `Output::Bool`
    VectorDropCollection {
        run: RunId,
        collection: String,
    },

    /// List all collections in a run.
    /// Returns: `Output::CollectionInfos`
    VectorListCollections {
        run: RunId,
    },

    /// Check if a collection exists.
    /// Returns: `Output::Bool`
    VectorCollectionExists {
        run: RunId,
        collection: String,
    },

    /// Get the count of vectors in a collection.
    /// Returns: `Output::Uint`
    VectorCount {
        run: RunId,
        collection: String,
    },

    /// Batch insert or update vectors.
    /// Returns: `Output::VectorBatchResults`
    VectorUpsertBatch {
        run: RunId,
        collection: String,
        vectors: Vec<VectorEntry>,
    },

    /// Batch get vectors.
    /// Returns: `Output::MaybeVectorDatas`
    VectorGetBatch {
        run: RunId,
        collection: String,
        keys: Vec<String>,
    },

    /// Batch delete vectors.
    /// Returns: `Output::Bools`
    VectorDeleteBatch {
        run: RunId,
        collection: String,
        keys: Vec<String>,
    },

    /// Get version history for a vector.
    /// Returns: `Output::VectorHistoryResult`
    VectorHistory {
        run: RunId,
        collection: String,
        key: String,
        limit: Option<u64>,
        before_version: Option<u64>,
    },

    /// Get a vector at a specific version.
    /// Returns: `Output::MaybeVectorData`
    VectorGetAt {
        run: RunId,
        collection: String,
        key: String,
        version: u64,
    },

    /// List all vector keys in a collection.
    /// Returns: `Output::Keys`
    VectorListKeys {
        run: RunId,
        collection: String,
        limit: Option<u64>,
        cursor: Option<String>,
    },

    /// Scan vectors in a collection.
    /// Returns: `Output::VectorScanResult`
    VectorScan {
        run: RunId,
        collection: String,
        limit: Option<u64>,
        cursor: Option<String>,
    },

    // ==================== Run (24) ====================
    /// Create a new run.
    /// Returns: `Output::RunCreated`
    RunCreate {
        run_id: Option<String>,
        metadata: Option<Value>,
    },

    /// Get run info.
    /// Returns: `Output::MaybeRunInfo`
    RunGet {
        run: RunId,
    },

    /// List all runs.
    /// Returns: `Output::RunInfos`
    RunList {
        state: Option<RunStatus>,
        limit: Option<u64>,
        offset: Option<u64>,
    },

    /// Close a run (mark as completed).
    /// Returns: `Output::Version`
    RunClose {
        run: RunId,
    },

    /// Update run metadata.
    /// Returns: `Output::Version`
    RunUpdateMetadata {
        run: RunId,
        metadata: Value,
    },

    /// Check if a run exists.
    /// Returns: `Output::Bool`
    RunExists {
        run: RunId,
    },

    /// Pause a run.
    /// Returns: `Output::Version`
    RunPause {
        run: RunId,
    },

    /// Resume a paused run.
    /// Returns: `Output::Version`
    RunResume {
        run: RunId,
    },

    /// Fail a run with an error message.
    /// Returns: `Output::Version`
    RunFail {
        run: RunId,
        error: String,
    },

    /// Cancel a run.
    /// Returns: `Output::Version`
    RunCancel {
        run: RunId,
    },

    /// Archive a run (soft delete).
    /// Returns: `Output::Version`
    RunArchive {
        run: RunId,
    },

    /// Delete a run and all its data.
    /// Returns: `Output::Unit`
    RunDelete {
        run: RunId,
    },

    /// Query runs by status.
    /// Returns: `Output::RunInfos`
    RunQueryByStatus {
        state: RunStatus,
    },

    /// Query runs by tag.
    /// Returns: `Output::RunInfos`
    RunQueryByTag {
        tag: String,
    },

    /// Count runs.
    /// Returns: `Output::Uint`
    RunCount {
        status: Option<RunStatus>,
    },

    /// Search runs (metadata and index only).
    /// Returns: `Output::RunInfos`
    RunSearch {
        query: String,
        limit: Option<u64>,
    },

    /// Add tags to a run.
    /// Returns: `Output::Version`
    RunAddTags {
        run: RunId,
        tags: Vec<String>,
    },

    /// Remove tags from a run.
    /// Returns: `Output::Version`
    RunRemoveTags {
        run: RunId,
        tags: Vec<String>,
    },

    /// Get tags for a run.
    /// Returns: `Output::Strings`
    RunGetTags {
        run: RunId,
    },

    /// Create a child run.
    /// Returns: `Output::RunCreated`
    RunCreateChild {
        parent: RunId,
        metadata: Option<Value>,
    },

    /// Get child runs.
    /// Returns: `Output::RunInfos`
    RunGetChildren {
        parent: RunId,
    },

    /// Get parent run.
    /// Returns: `Output::MaybeRunId`
    RunGetParent {
        run: RunId,
    },

    /// Set retention policy for a run.
    /// Returns: `Output::Version`
    RunSetRetention {
        run: RunId,
        policy: RetentionPolicyInfo,
    },

    /// Get retention policy for a run.
    /// Returns: `Output::RetentionPolicy`
    RunGetRetention {
        run: RunId,
    },

    // ==================== Transaction (5) ====================
    /// Begin a new transaction.
    /// Returns: `Output::TxnId`
    TxnBegin {
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
        run: RunId,
    },

    /// Get retention statistics.
    /// Returns: `Output::RetentionStats`
    RetentionStats {
        run: RunId,
    },

    /// Preview what would be deleted by retention policy.
    /// Returns: `Output::RetentionPreview`
    RetentionPreview {
        run: RunId,
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
}
