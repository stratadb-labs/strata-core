//! High-level typed wrapper for the Executor.
//!
//! The [`Strata`] struct provides a convenient Rust API that wraps the
//! [`Executor`] and [`Command`]/[`Output`] enums with typed method calls.
//!
//! All data methods use the default run. Callers needing a specific run
//! should use `executor.execute(Command::... { run: Some(run_id), ... })`
//! directly.
//!
//! # Example
//!
//! ```ignore
//! use strata_executor::Strata;
//! use strata_core::Value;
//!
//! let db = Strata::new(substrate);
//!
//! // No run parameter - always uses default run
//! db.kv_put("key", Value::String("hello".into()))?;
//! let value = db.kv_get("key")?;
//! ```

use std::sync::Arc;

use strata_engine::Database;
use strata_core::Value;

use crate::types::*;
use crate::{Command, Error, Executor, Output, Result};

/// High-level typed wrapper for database operations.
///
/// `Strata` provides a convenient Rust API that wraps the executor's
/// command-based interface with typed method calls. Each method:
///
/// 1. Creates the appropriate [`Command`] with `run: None`
/// 2. Executes it via the [`Executor`] (which resolves to the default run)
/// 3. Extracts and returns the typed result
pub struct Strata {
    executor: Executor,
}

impl Strata {
    /// Create a new Strata instance wrapping the given database.
    pub fn new(db: Arc<Database>) -> Self {
        Self {
            executor: Executor::new(db),
        }
    }

    /// Get the underlying executor.
    pub fn executor(&self) -> &Executor {
        &self.executor
    }

    // =========================================================================
    // Database Operations (4)
    // =========================================================================

    /// Ping the database.
    pub fn ping(&self) -> Result<String> {
        match self.executor.execute(Command::Ping)? {
            Output::Pong { version } => Ok(version),
            _ => Err(Error::Internal {
                reason: "Unexpected output for Ping".into(),
            }),
        }
    }

    /// Get database info.
    pub fn info(&self) -> Result<DatabaseInfo> {
        match self.executor.execute(Command::Info)? {
            Output::DatabaseInfo(info) => Ok(info),
            _ => Err(Error::Internal {
                reason: "Unexpected output for Info".into(),
            }),
        }
    }

    /// Flush the database to disk.
    pub fn flush(&self) -> Result<()> {
        match self.executor.execute(Command::Flush)? {
            Output::Unit => Ok(()),
            _ => Err(Error::Internal {
                reason: "Unexpected output for Flush".into(),
            }),
        }
    }

    /// Compact the database.
    pub fn compact(&self) -> Result<()> {
        match self.executor.execute(Command::Compact)? {
            Output::Unit => Ok(()),
            _ => Err(Error::Internal {
                reason: "Unexpected output for Compact".into(),
            }),
        }
    }

    // =========================================================================
    // KV Operations (15)
    // =========================================================================

    /// Put a value in the KV store.
    pub fn kv_put(&self, key: &str, value: Value) -> Result<u64> {
        match self.executor.execute(Command::KvPut {
            run: None,
            key: key.to_string(),
            value,
        })? {
            Output::Version(v) => Ok(v),
            _ => Err(Error::Internal {
                reason: "Unexpected output for KvPut".into(),
            }),
        }
    }

    /// Get a value from the KV store.
    pub fn kv_get(&self, key: &str) -> Result<Option<VersionedValue>> {
        match self.executor.execute(Command::KvGet {
            run: None,
            key: key.to_string(),
        })? {
            Output::MaybeVersioned(v) => Ok(v),
            _ => Err(Error::Internal {
                reason: "Unexpected output for KvGet".into(),
            }),
        }
    }

    /// Get a value at a specific version.
    pub fn kv_get_at(&self, key: &str, version: u64) -> Result<VersionedValue> {
        match self.executor.execute(Command::KvGetAt {
            run: None,
            key: key.to_string(),
            version,
        })? {
            Output::Versioned(v) => Ok(v),
            _ => Err(Error::Internal {
                reason: "Unexpected output for KvGetAt".into(),
            }),
        }
    }

    /// Delete a key from the KV store.
    pub fn kv_delete(&self, key: &str) -> Result<bool> {
        match self.executor.execute(Command::KvDelete {
            run: None,
            key: key.to_string(),
        })? {
            Output::Bool(deleted) => Ok(deleted),
            _ => Err(Error::Internal {
                reason: "Unexpected output for KvDelete".into(),
            }),
        }
    }

    /// Check if a key exists in the KV store.
    pub fn kv_exists(&self, key: &str) -> Result<bool> {
        match self.executor.execute(Command::KvExists {
            run: None,
            key: key.to_string(),
        })? {
            Output::Bool(exists) => Ok(exists),
            _ => Err(Error::Internal {
                reason: "Unexpected output for KvExists".into(),
            }),
        }
    }

    /// Get version history for a key.
    pub fn kv_history(
        &self,
        key: &str,
        limit: Option<u64>,
        before: Option<u64>,
    ) -> Result<Vec<VersionedValue>> {
        match self.executor.execute(Command::KvHistory {
            run: None,
            key: key.to_string(),
            limit,
            before,
        })? {
            Output::VersionedValues(vals) => Ok(vals),
            _ => Err(Error::Internal {
                reason: "Unexpected output for KvHistory".into(),
            }),
        }
    }

    /// Increment a counter in the KV store.
    pub fn kv_incr(&self, key: &str, delta: i64) -> Result<i64> {
        match self.executor.execute(Command::KvIncr {
            run: None,
            key: key.to_string(),
            delta,
        })? {
            Output::Int(val) => Ok(val),
            _ => Err(Error::Internal {
                reason: "Unexpected output for KvIncr".into(),
            }),
        }
    }

    /// Compare-and-swap by version.
    pub fn kv_cas_version(
        &self,
        key: &str,
        expected_version: Option<u64>,
        new_value: Value,
    ) -> Result<bool> {
        match self.executor.execute(Command::KvCasVersion {
            run: None,
            key: key.to_string(),
            expected_version,
            new_value,
        })? {
            Output::Bool(ok) => Ok(ok),
            _ => Err(Error::Internal {
                reason: "Unexpected output for KvCasVersion".into(),
            }),
        }
    }

    /// Compare-and-swap by value.
    pub fn kv_cas_value(
        &self,
        key: &str,
        expected_value: Option<Value>,
        new_value: Value,
    ) -> Result<bool> {
        match self.executor.execute(Command::KvCasValue {
            run: None,
            key: key.to_string(),
            expected_value,
            new_value,
        })? {
            Output::Bool(ok) => Ok(ok),
            _ => Err(Error::Internal {
                reason: "Unexpected output for KvCasValue".into(),
            }),
        }
    }

    /// List keys with optional prefix filter.
    pub fn kv_keys(&self, prefix: &str, limit: Option<u64>) -> Result<Vec<String>> {
        match self.executor.execute(Command::KvKeys {
            run: None,
            prefix: prefix.to_string(),
            limit,
        })? {
            Output::Keys(keys) => Ok(keys),
            _ => Err(Error::Internal {
                reason: "Unexpected output for KvKeys".into(),
            }),
        }
    }

    /// Scan keys with cursor-based pagination.
    pub fn kv_scan(
        &self,
        prefix: &str,
        limit: u64,
        cursor: Option<String>,
    ) -> Result<(Vec<(String, VersionedValue)>, Option<String>)> {
        match self.executor.execute(Command::KvScan {
            run: None,
            prefix: prefix.to_string(),
            limit,
            cursor,
        })? {
            Output::KvScanResult { entries, cursor } => Ok((entries, cursor)),
            _ => Err(Error::Internal {
                reason: "Unexpected output for KvScan".into(),
            }),
        }
    }

    /// Get multiple values from the KV store.
    pub fn kv_mget(&self, keys: Vec<String>) -> Result<Vec<Option<VersionedValue>>> {
        match self.executor.execute(Command::KvMget {
            run: None,
            keys,
        })? {
            Output::Values(vals) => Ok(vals),
            _ => Err(Error::Internal {
                reason: "Unexpected output for KvMget".into(),
            }),
        }
    }

    /// Put multiple values in the KV store.
    pub fn kv_mput(&self, entries: Vec<(String, Value)>) -> Result<u64> {
        match self.executor.execute(Command::KvMput {
            run: None,
            entries,
        })? {
            Output::Version(v) => Ok(v),
            _ => Err(Error::Internal {
                reason: "Unexpected output for KvMput".into(),
            }),
        }
    }

    /// Delete multiple keys.
    pub fn kv_mdelete(&self, keys: Vec<String>) -> Result<u64> {
        match self.executor.execute(Command::KvMdelete {
            run: None,
            keys,
        })? {
            Output::Uint(count) => Ok(count),
            _ => Err(Error::Internal {
                reason: "Unexpected output for KvMdelete".into(),
            }),
        }
    }

    /// Check existence of multiple keys.
    pub fn kv_mexists(&self, keys: Vec<String>) -> Result<u64> {
        match self.executor.execute(Command::KvMexists {
            run: None,
            keys,
        })? {
            Output::Uint(count) => Ok(count),
            _ => Err(Error::Internal {
                reason: "Unexpected output for KvMexists".into(),
            }),
        }
    }

    // =========================================================================
    // JSON Operations (17)
    // =========================================================================

    /// Set a JSON value at a path.
    pub fn json_set(&self, key: &str, path: &str, value: Value) -> Result<u64> {
        match self.executor.execute(Command::JsonSet {
            run: None,
            key: key.to_string(),
            path: path.to_string(),
            value,
        })? {
            Output::Version(v) => Ok(v),
            _ => Err(Error::Internal {
                reason: "Unexpected output for JsonSet".into(),
            }),
        }
    }

    /// Get a JSON value at a path.
    pub fn json_get(&self, key: &str, path: &str) -> Result<Option<VersionedValue>> {
        match self.executor.execute(Command::JsonGet {
            run: None,
            key: key.to_string(),
            path: path.to_string(),
        })? {
            Output::MaybeVersioned(v) => Ok(v),
            _ => Err(Error::Internal {
                reason: "Unexpected output for JsonGet".into(),
            }),
        }
    }

    /// Delete a value at a path from a JSON document.
    pub fn json_delete(&self, key: &str, path: &str) -> Result<u64> {
        match self.executor.execute(Command::JsonDelete {
            run: None,
            key: key.to_string(),
            path: path.to_string(),
        })? {
            Output::Uint(count) => Ok(count),
            _ => Err(Error::Internal {
                reason: "Unexpected output for JsonDelete".into(),
            }),
        }
    }

    /// Merge a value at a path (RFC 7396 JSON Merge Patch).
    pub fn json_merge(&self, key: &str, path: &str, patch: Value) -> Result<u64> {
        match self.executor.execute(Command::JsonMerge {
            run: None,
            key: key.to_string(),
            path: path.to_string(),
            patch,
        })? {
            Output::Version(v) => Ok(v),
            _ => Err(Error::Internal {
                reason: "Unexpected output for JsonMerge".into(),
            }),
        }
    }

    /// Get version history for a JSON document.
    pub fn json_history(
        &self,
        key: &str,
        limit: Option<u64>,
        before: Option<u64>,
    ) -> Result<Vec<VersionedValue>> {
        match self.executor.execute(Command::JsonHistory {
            run: None,
            key: key.to_string(),
            limit,
            before,
        })? {
            Output::VersionedValues(vals) => Ok(vals),
            _ => Err(Error::Internal {
                reason: "Unexpected output for JsonHistory".into(),
            }),
        }
    }

    /// Check if a JSON document exists.
    pub fn json_exists(&self, key: &str) -> Result<bool> {
        match self.executor.execute(Command::JsonExists {
            run: None,
            key: key.to_string(),
        })? {
            Output::Bool(exists) => Ok(exists),
            _ => Err(Error::Internal {
                reason: "Unexpected output for JsonExists".into(),
            }),
        }
    }

    /// Get the current version of a JSON document.
    pub fn json_get_version(&self, key: &str) -> Result<Option<u64>> {
        match self.executor.execute(Command::JsonGetVersion {
            run: None,
            key: key.to_string(),
        })? {
            Output::MaybeVersion(v) => Ok(v),
            _ => Err(Error::Internal {
                reason: "Unexpected output for JsonGetVersion".into(),
            }),
        }
    }

    /// Full-text search across JSON documents.
    pub fn json_search(&self, query: &str, k: u64) -> Result<Vec<JsonSearchHit>> {
        match self.executor.execute(Command::JsonSearch {
            run: None,
            query: query.to_string(),
            k,
        })? {
            Output::JsonSearchHits(hits) => Ok(hits),
            _ => Err(Error::Internal {
                reason: "Unexpected output for JsonSearch".into(),
            }),
        }
    }

    /// List JSON documents with cursor-based pagination.
    pub fn json_list(
        &self,
        prefix: Option<String>,
        cursor: Option<String>,
        limit: u64,
    ) -> Result<(Vec<String>, Option<String>)> {
        match self.executor.execute(Command::JsonList {
            run: None,
            prefix,
            cursor,
            limit,
        })? {
            Output::JsonListResult { keys, cursor } => Ok((keys, cursor)),
            _ => Err(Error::Internal {
                reason: "Unexpected output for JsonList".into(),
            }),
        }
    }

    /// Compare-and-swap: update if version matches.
    pub fn json_cas(
        &self,
        key: &str,
        expected_version: u64,
        path: &str,
        value: Value,
    ) -> Result<u64> {
        match self.executor.execute(Command::JsonCas {
            run: None,
            key: key.to_string(),
            expected_version,
            path: path.to_string(),
            value,
        })? {
            Output::Version(v) => Ok(v),
            _ => Err(Error::Internal {
                reason: "Unexpected output for JsonCas".into(),
            }),
        }
    }

    /// Query documents by exact field match.
    pub fn json_query(&self, path: &str, value: Value, limit: u64) -> Result<Vec<String>> {
        match self.executor.execute(Command::JsonQuery {
            run: None,
            path: path.to_string(),
            value,
            limit,
        })? {
            Output::Keys(keys) => Ok(keys),
            _ => Err(Error::Internal {
                reason: "Unexpected output for JsonQuery".into(),
            }),
        }
    }

    /// Count JSON documents in the store.
    pub fn json_count(&self) -> Result<u64> {
        match self.executor.execute(Command::JsonCount {
            run: None,
        })? {
            Output::Uint(count) => Ok(count),
            _ => Err(Error::Internal {
                reason: "Unexpected output for JsonCount".into(),
            }),
        }
    }

    /// Batch get multiple JSON documents.
    pub fn json_batch_get(&self, keys: Vec<String>) -> Result<Vec<Option<VersionedValue>>> {
        match self.executor.execute(Command::JsonBatchGet {
            run: None,
            keys,
        })? {
            Output::Values(vals) => Ok(vals),
            _ => Err(Error::Internal {
                reason: "Unexpected output for JsonBatchGet".into(),
            }),
        }
    }

    /// Batch create multiple JSON documents atomically.
    pub fn json_batch_create(&self, docs: Vec<(String, Value)>) -> Result<Vec<u64>> {
        match self.executor.execute(Command::JsonBatchCreate {
            run: None,
            docs,
        })? {
            Output::Versions(versions) => Ok(versions),
            _ => Err(Error::Internal {
                reason: "Unexpected output for JsonBatchCreate".into(),
            }),
        }
    }

    /// Atomically push values to an array at path.
    pub fn json_array_push(&self, key: &str, path: &str, values: Vec<Value>) -> Result<u64> {
        match self.executor.execute(Command::JsonArrayPush {
            run: None,
            key: key.to_string(),
            path: path.to_string(),
            values,
        })? {
            Output::Uint(len) => Ok(len),
            _ => Err(Error::Internal {
                reason: "Unexpected output for JsonArrayPush".into(),
            }),
        }
    }

    /// Atomically increment a numeric value at path.
    pub fn json_increment(&self, key: &str, path: &str, delta: f64) -> Result<f64> {
        match self.executor.execute(Command::JsonIncrement {
            run: None,
            key: key.to_string(),
            path: path.to_string(),
            delta,
        })? {
            Output::Float(val) => Ok(val),
            _ => Err(Error::Internal {
                reason: "Unexpected output for JsonIncrement".into(),
            }),
        }
    }

    /// Atomically pop a value from an array at path.
    pub fn json_array_pop(&self, key: &str, path: &str) -> Result<Option<Value>> {
        match self.executor.execute(Command::JsonArrayPop {
            run: None,
            key: key.to_string(),
            path: path.to_string(),
        })? {
            Output::Maybe(val) => Ok(val),
            _ => Err(Error::Internal {
                reason: "Unexpected output for JsonArrayPop".into(),
            }),
        }
    }

    // =========================================================================
    // Event Operations (11)
    // =========================================================================

    /// Append an event to a stream.
    pub fn event_append(&self, stream: &str, payload: Value) -> Result<u64> {
        match self.executor.execute(Command::EventAppend {
            run: None,
            stream: stream.to_string(),
            payload,
        })? {
            Output::Version(v) => Ok(v),
            _ => Err(Error::Internal {
                reason: "Unexpected output for EventAppend".into(),
            }),
        }
    }

    /// Append multiple events atomically.
    pub fn event_append_batch(&self, events: Vec<(String, Value)>) -> Result<Vec<u64>> {
        match self.executor.execute(Command::EventAppendBatch {
            run: None,
            events,
        })? {
            Output::Versions(versions) => Ok(versions),
            _ => Err(Error::Internal {
                reason: "Unexpected output for EventAppendBatch".into(),
            }),
        }
    }

    /// Get events from a stream in a range.
    pub fn event_range(
        &self,
        stream: &str,
        start: Option<u64>,
        end: Option<u64>,
        limit: Option<u64>,
    ) -> Result<Vec<VersionedValue>> {
        match self.executor.execute(Command::EventRange {
            run: None,
            stream: stream.to_string(),
            start,
            end,
            limit,
        })? {
            Output::VersionedValues(events) => Ok(events),
            _ => Err(Error::Internal {
                reason: "Unexpected output for EventRange".into(),
            }),
        }
    }

    /// Get a specific event by sequence number.
    pub fn event_read(&self, stream: &str, sequence: u64) -> Result<Option<VersionedValue>> {
        match self.executor.execute(Command::EventRead {
            run: None,
            stream: stream.to_string(),
            sequence,
        })? {
            Output::MaybeVersioned(v) => Ok(v),
            _ => Err(Error::Internal {
                reason: "Unexpected output for EventRead".into(),
            }),
        }
    }

    /// Get the count of events in a stream.
    pub fn event_len(&self, stream: &str) -> Result<u64> {
        match self.executor.execute(Command::EventLen {
            run: None,
            stream: stream.to_string(),
        })? {
            Output::Uint(len) => Ok(len),
            _ => Err(Error::Internal {
                reason: "Unexpected output for EventLen".into(),
            }),
        }
    }

    /// Get the latest sequence number in a stream.
    pub fn event_latest_sequence(&self, stream: &str) -> Result<Option<u64>> {
        match self.executor.execute(Command::EventLatestSequence {
            run: None,
            stream: stream.to_string(),
        })? {
            Output::MaybeVersion(seq) => Ok(seq),
            _ => Err(Error::Internal {
                reason: "Unexpected output for EventLatestSequence".into(),
            }),
        }
    }

    /// Get stream metadata.
    pub fn event_stream_info(&self, stream: &str) -> Result<StreamInfo> {
        match self.executor.execute(Command::EventStreamInfo {
            run: None,
            stream: stream.to_string(),
        })? {
            Output::StreamInfo(info) => Ok(info),
            _ => Err(Error::Internal {
                reason: "Unexpected output for EventStreamInfo".into(),
            }),
        }
    }

    /// Read events from a stream in descending order (newest first).
    pub fn event_rev_range(
        &self,
        stream: &str,
        start: Option<u64>,
        end: Option<u64>,
        limit: Option<u64>,
    ) -> Result<Vec<VersionedValue>> {
        match self.executor.execute(Command::EventRevRange {
            run: None,
            stream: stream.to_string(),
            start,
            end,
            limit,
        })? {
            Output::VersionedValues(events) => Ok(events),
            _ => Err(Error::Internal {
                reason: "Unexpected output for EventRevRange".into(),
            }),
        }
    }

    /// List all event streams.
    pub fn event_streams(&self) -> Result<Vec<String>> {
        match self.executor.execute(Command::EventStreams {
            run: None,
        })? {
            Output::Strings(streams) => Ok(streams),
            _ => Err(Error::Internal {
                reason: "Unexpected output for EventStreams".into(),
            }),
        }
    }

    /// Get the latest event (head) of a stream.
    pub fn event_head(&self, stream: &str) -> Result<Option<VersionedValue>> {
        match self.executor.execute(Command::EventHead {
            run: None,
            stream: stream.to_string(),
        })? {
            Output::MaybeVersioned(v) => Ok(v),
            _ => Err(Error::Internal {
                reason: "Unexpected output for EventHead".into(),
            }),
        }
    }

    /// Verify the hash chain integrity of the event log.
    pub fn event_verify_chain(&self) -> Result<ChainVerificationResult> {
        match self.executor.execute(Command::EventVerifyChain {
            run: None,
        })? {
            Output::ChainVerification(result) => Ok(result),
            _ => Err(Error::Internal {
                reason: "Unexpected output for EventVerifyChain".into(),
            }),
        }
    }

    // =========================================================================
    // State Operations (8)
    // =========================================================================

    /// Set a state cell value.
    pub fn state_set(&self, cell: &str, value: Value) -> Result<u64> {
        match self.executor.execute(Command::StateSet {
            run: None,
            cell: cell.to_string(),
            value,
        })? {
            Output::Version(v) => Ok(v),
            _ => Err(Error::Internal {
                reason: "Unexpected output for StateSet".into(),
            }),
        }
    }

    /// Get a state cell value.
    pub fn state_read(&self, cell: &str) -> Result<Option<VersionedValue>> {
        match self.executor.execute(Command::StateRead {
            run: None,
            cell: cell.to_string(),
        })? {
            Output::MaybeVersioned(v) => Ok(v),
            _ => Err(Error::Internal {
                reason: "Unexpected output for StateRead".into(),
            }),
        }
    }

    /// Compare-and-swap on a state cell.
    pub fn state_cas(
        &self,
        cell: &str,
        expected_counter: Option<u64>,
        value: Value,
    ) -> Result<Option<u64>> {
        match self.executor.execute(Command::StateCas {
            run: None,
            cell: cell.to_string(),
            expected_counter,
            value,
        })? {
            Output::MaybeVersion(v) => Ok(v),
            _ => Err(Error::Internal {
                reason: "Unexpected output for StateCas".into(),
            }),
        }
    }

    /// Delete a state cell.
    pub fn state_delete(&self, cell: &str) -> Result<bool> {
        match self.executor.execute(Command::StateDelete {
            run: None,
            cell: cell.to_string(),
        })? {
            Output::Bool(deleted) => Ok(deleted),
            _ => Err(Error::Internal {
                reason: "Unexpected output for StateDelete".into(),
            }),
        }
    }

    /// Check if a state cell exists.
    pub fn state_exists(&self, cell: &str) -> Result<bool> {
        match self.executor.execute(Command::StateExists {
            run: None,
            cell: cell.to_string(),
        })? {
            Output::Bool(exists) => Ok(exists),
            _ => Err(Error::Internal {
                reason: "Unexpected output for StateExists".into(),
            }),
        }
    }

    /// Get version history for a state cell.
    pub fn state_history(
        &self,
        cell: &str,
        limit: Option<u64>,
        before: Option<u64>,
    ) -> Result<Vec<VersionedValue>> {
        match self.executor.execute(Command::StateHistory {
            run: None,
            cell: cell.to_string(),
            limit,
            before,
        })? {
            Output::VersionedValues(vals) => Ok(vals),
            _ => Err(Error::Internal {
                reason: "Unexpected output for StateHistory".into(),
            }),
        }
    }

    /// Initialize a state cell (only if it doesn't exist).
    pub fn state_init(&self, cell: &str, value: Value) -> Result<u64> {
        match self.executor.execute(Command::StateInit {
            run: None,
            cell: cell.to_string(),
            value,
        })? {
            Output::Version(v) => Ok(v),
            _ => Err(Error::Internal {
                reason: "Unexpected output for StateInit".into(),
            }),
        }
    }

    /// List all state cell names.
    pub fn state_list(&self) -> Result<Vec<String>> {
        match self.executor.execute(Command::StateList {
            run: None,
        })? {
            Output::Strings(names) => Ok(names),
            _ => Err(Error::Internal {
                reason: "Unexpected output for StateList".into(),
            }),
        }
    }

    // =========================================================================
    // Vector Operations (17)
    // =========================================================================

    /// Create a vector collection.
    pub fn vector_create_collection(
        &self,
        collection: &str,
        dimension: u64,
        metric: DistanceMetric,
    ) -> Result<u64> {
        match self.executor.execute(Command::VectorCreateCollection {
            run: None,
            collection: collection.to_string(),
            dimension,
            metric,
        })? {
            Output::Version(v) => Ok(v),
            _ => Err(Error::Internal {
                reason: "Unexpected output for VectorCreateCollection".into(),
            }),
        }
    }

    /// Upsert a vector.
    pub fn vector_upsert(
        &self,
        collection: &str,
        key: &str,
        vector: Vec<f32>,
        metadata: Option<Value>,
    ) -> Result<u64> {
        match self.executor.execute(Command::VectorUpsert {
            run: None,
            collection: collection.to_string(),
            key: key.to_string(),
            vector,
            metadata,
        })? {
            Output::Version(v) => Ok(v),
            _ => Err(Error::Internal {
                reason: "Unexpected output for VectorUpsert".into(),
            }),
        }
    }

    /// Get a vector by key.
    pub fn vector_get(&self, collection: &str, key: &str) -> Result<Option<VersionedVectorData>> {
        match self.executor.execute(Command::VectorGet {
            run: None,
            collection: collection.to_string(),
            key: key.to_string(),
        })? {
            Output::VectorData(data) => Ok(data),
            _ => Err(Error::Internal {
                reason: "Unexpected output for VectorGet".into(),
            }),
        }
    }

    /// Delete a vector.
    pub fn vector_delete(&self, collection: &str, key: &str) -> Result<bool> {
        match self.executor.execute(Command::VectorDelete {
            run: None,
            collection: collection.to_string(),
            key: key.to_string(),
        })? {
            Output::Bool(deleted) => Ok(deleted),
            _ => Err(Error::Internal {
                reason: "Unexpected output for VectorDelete".into(),
            }),
        }
    }

    /// Search for similar vectors.
    pub fn vector_search(
        &self,
        collection: &str,
        query: Vec<f32>,
        k: u64,
    ) -> Result<Vec<VectorMatch>> {
        match self.executor.execute(Command::VectorSearch {
            run: None,
            collection: collection.to_string(),
            query,
            k,
            filter: None,
            metric: None,
        })? {
            Output::VectorMatches(matches) => Ok(matches),
            _ => Err(Error::Internal {
                reason: "Unexpected output for VectorSearch".into(),
            }),
        }
    }

    /// Search for similar vectors with filter and metric options.
    pub fn vector_search_filtered(
        &self,
        collection: &str,
        query: Vec<f32>,
        k: u64,
        filter: Option<Vec<MetadataFilter>>,
        metric: Option<DistanceMetric>,
    ) -> Result<Vec<VectorMatch>> {
        match self.executor.execute(Command::VectorSearch {
            run: None,
            collection: collection.to_string(),
            query,
            k,
            filter,
            metric,
        })? {
            Output::VectorMatches(matches) => Ok(matches),
            _ => Err(Error::Internal {
                reason: "Unexpected output for VectorSearch".into(),
            }),
        }
    }

    /// Get collection information.
    pub fn vector_get_collection(&self, collection: &str) -> Result<Option<CollectionInfo>> {
        match self.executor.execute(Command::VectorGetCollection {
            run: None,
            collection: collection.to_string(),
        })? {
            Output::VectorGetCollection(info) => Ok(info),
            _ => Err(Error::Internal {
                reason: "Unexpected output for VectorGetCollection".into(),
            }),
        }
    }

    /// Delete a collection.
    pub fn vector_delete_collection(&self, collection: &str) -> Result<bool> {
        match self.executor.execute(Command::VectorDeleteCollection {
            run: None,
            collection: collection.to_string(),
        })? {
            Output::Bool(dropped) => Ok(dropped),
            _ => Err(Error::Internal {
                reason: "Unexpected output for VectorDeleteCollection".into(),
            }),
        }
    }

    /// List all collections.
    pub fn vector_list_collections(&self) -> Result<Vec<CollectionInfo>> {
        match self.executor.execute(Command::VectorListCollections {
            run: None,
        })? {
            Output::VectorCollectionList(infos) => Ok(infos),
            _ => Err(Error::Internal {
                reason: "Unexpected output for VectorListCollections".into(),
            }),
        }
    }

    /// Check if a collection exists.
    pub fn vector_collection_exists(&self, collection: &str) -> Result<bool> {
        match self.executor.execute(Command::VectorCollectionExists {
            run: None,
            collection: collection.to_string(),
        })? {
            Output::Bool(exists) => Ok(exists),
            _ => Err(Error::Internal {
                reason: "Unexpected output for VectorCollectionExists".into(),
            }),
        }
    }

    /// Get the count of vectors in a collection.
    pub fn vector_count(&self, collection: &str) -> Result<u64> {
        match self.executor.execute(Command::VectorCount {
            run: None,
            collection: collection.to_string(),
        })? {
            Output::Uint(count) => Ok(count),
            _ => Err(Error::Internal {
                reason: "Unexpected output for VectorCount".into(),
            }),
        }
    }

    /// Batch insert or update vectors.
    pub fn vector_upsert_batch(
        &self,
        collection: &str,
        vectors: Vec<VectorEntry>,
    ) -> Result<Vec<VectorBatchEntry>> {
        match self.executor.execute(Command::VectorUpsertBatch {
            run: None,
            collection: collection.to_string(),
            vectors,
        })? {
            Output::VectorBatchResult(results) => Ok(results),
            _ => Err(Error::Internal {
                reason: "Unexpected output for VectorUpsertBatch".into(),
            }),
        }
    }

    /// Batch get vectors.
    pub fn vector_get_batch(
        &self,
        collection: &str,
        keys: Vec<String>,
    ) -> Result<Vec<Option<VersionedVectorData>>> {
        match self.executor.execute(Command::VectorGetBatch {
            run: None,
            collection: collection.to_string(),
            keys,
        })? {
            Output::VectorDataList(data) => Ok(data),
            _ => Err(Error::Internal {
                reason: "Unexpected output for VectorGetBatch".into(),
            }),
        }
    }

    /// Batch delete vectors.
    pub fn vector_delete_batch(&self, collection: &str, keys: Vec<String>) -> Result<Vec<bool>> {
        match self.executor.execute(Command::VectorDeleteBatch {
            run: None,
            collection: collection.to_string(),
            keys,
        })? {
            Output::Bools(results) => Ok(results),
            _ => Err(Error::Internal {
                reason: "Unexpected output for VectorDeleteBatch".into(),
            }),
        }
    }

    /// Get version history for a vector.
    pub fn vector_history(
        &self,
        collection: &str,
        key: &str,
        limit: Option<u64>,
        before_version: Option<u64>,
    ) -> Result<Vec<VersionedVectorData>> {
        match self.executor.execute(Command::VectorHistory {
            run: None,
            collection: collection.to_string(),
            key: key.to_string(),
            limit,
            before_version,
        })? {
            Output::VectorDataHistory(history) => Ok(history),
            _ => Err(Error::Internal {
                reason: "Unexpected output for VectorHistory".into(),
            }),
        }
    }

    /// Get a vector at a specific version.
    pub fn vector_get_at(
        &self,
        collection: &str,
        key: &str,
        version: u64,
    ) -> Result<Option<VersionedVectorData>> {
        match self.executor.execute(Command::VectorGetAt {
            run: None,
            collection: collection.to_string(),
            key: key.to_string(),
            version,
        })? {
            Output::VectorData(data) => Ok(data),
            _ => Err(Error::Internal {
                reason: "Unexpected output for VectorGetAt".into(),
            }),
        }
    }

    /// List all vector keys in a collection.
    pub fn vector_list_keys(
        &self,
        collection: &str,
        limit: Option<u64>,
        cursor: Option<String>,
    ) -> Result<Vec<String>> {
        match self.executor.execute(Command::VectorListKeys {
            run: None,
            collection: collection.to_string(),
            limit,
            cursor,
        })? {
            Output::Keys(keys) => Ok(keys),
            _ => Err(Error::Internal {
                reason: "Unexpected output for VectorListKeys".into(),
            }),
        }
    }

    /// Scan vectors in a collection.
    pub fn vector_scan(
        &self,
        collection: &str,
        limit: Option<u64>,
        cursor: Option<String>,
    ) -> Result<Vec<(String, VectorData)>> {
        match self.executor.execute(Command::VectorScan {
            run: None,
            collection: collection.to_string(),
            limit,
            cursor,
        })? {
            Output::VectorKeyValues(entries) => Ok(entries),
            _ => Err(Error::Internal {
                reason: "Unexpected output for VectorScan".into(),
            }),
        }
    }

    // =========================================================================
    // Run Operations (24)
    // =========================================================================

    /// Create a new run.
    pub fn run_create(
        &self,
        run_id: Option<String>,
        metadata: Option<Value>,
    ) -> Result<(RunInfo, u64)> {
        match self.executor.execute(Command::RunCreate { run_id, metadata })? {
            Output::RunWithVersion { info, version } => Ok((info, version)),
            _ => Err(Error::Internal {
                reason: "Unexpected output for RunCreate".into(),
            }),
        }
    }

    /// Get run info.
    pub fn run_get(&self, run: &str) -> Result<Option<VersionedRunInfo>> {
        match self.executor.execute(Command::RunGet {
            run: RunId::from(run),
        })? {
            Output::RunInfoVersioned(info) => Ok(Some(info)),
            Output::Maybe(None) => Ok(None),
            _ => Err(Error::Internal {
                reason: "Unexpected output for RunGet".into(),
            }),
        }
    }

    /// List runs.
    pub fn run_list(
        &self,
        state: Option<RunStatus>,
        limit: Option<u64>,
        offset: Option<u64>,
    ) -> Result<Vec<VersionedRunInfo>> {
        match self.executor.execute(Command::RunList {
            state,
            limit,
            offset,
        })? {
            Output::RunInfoList(runs) => Ok(runs),
            _ => Err(Error::Internal {
                reason: "Unexpected output for RunList".into(),
            }),
        }
    }

    /// Close a run.
    pub fn run_complete(&self, run: &str) -> Result<u64> {
        match self.executor.execute(Command::RunComplete {
            run: RunId::from(run),
        })? {
            Output::Version(v) => Ok(v),
            _ => Err(Error::Internal {
                reason: "Unexpected output for RunComplete".into(),
            }),
        }
    }

    /// Update run metadata.
    pub fn run_update_metadata(&self, run: &str, metadata: Value) -> Result<u64> {
        match self.executor.execute(Command::RunUpdateMetadata {
            run: RunId::from(run),
            metadata,
        })? {
            Output::Version(v) => Ok(v),
            _ => Err(Error::Internal {
                reason: "Unexpected output for RunUpdateMetadata".into(),
            }),
        }
    }

    /// Check if a run exists.
    pub fn run_exists(&self, run: &str) -> Result<bool> {
        match self.executor.execute(Command::RunExists {
            run: RunId::from(run),
        })? {
            Output::Bool(exists) => Ok(exists),
            _ => Err(Error::Internal {
                reason: "Unexpected output for RunExists".into(),
            }),
        }
    }

    /// Pause a run.
    pub fn run_pause(&self, run: &str) -> Result<u64> {
        match self.executor.execute(Command::RunPause {
            run: RunId::from(run),
        })? {
            Output::Version(v) => Ok(v),
            _ => Err(Error::Internal {
                reason: "Unexpected output for RunPause".into(),
            }),
        }
    }

    /// Resume a paused run.
    pub fn run_resume(&self, run: &str) -> Result<u64> {
        match self.executor.execute(Command::RunResume {
            run: RunId::from(run),
        })? {
            Output::Version(v) => Ok(v),
            _ => Err(Error::Internal {
                reason: "Unexpected output for RunResume".into(),
            }),
        }
    }

    /// Fail a run with an error message.
    pub fn run_fail(&self, run: &str, error: &str) -> Result<u64> {
        match self.executor.execute(Command::RunFail {
            run: RunId::from(run),
            error: error.to_string(),
        })? {
            Output::Version(v) => Ok(v),
            _ => Err(Error::Internal {
                reason: "Unexpected output for RunFail".into(),
            }),
        }
    }

    /// Cancel a run.
    pub fn run_cancel(&self, run: &str) -> Result<u64> {
        match self.executor.execute(Command::RunCancel {
            run: RunId::from(run),
        })? {
            Output::Version(v) => Ok(v),
            _ => Err(Error::Internal {
                reason: "Unexpected output for RunCancel".into(),
            }),
        }
    }

    /// Archive a run (soft delete).
    pub fn run_archive(&self, run: &str) -> Result<u64> {
        match self.executor.execute(Command::RunArchive {
            run: RunId::from(run),
        })? {
            Output::Version(v) => Ok(v),
            _ => Err(Error::Internal {
                reason: "Unexpected output for RunArchive".into(),
            }),
        }
    }

    /// Delete a run.
    pub fn run_delete(&self, run: &str) -> Result<()> {
        match self.executor.execute(Command::RunDelete {
            run: RunId::from(run),
        })? {
            Output::Unit => Ok(()),
            _ => Err(Error::Internal {
                reason: "Unexpected output for RunDelete".into(),
            }),
        }
    }

    /// Query runs by status.
    pub fn run_query_by_status(&self, state: RunStatus) -> Result<Vec<VersionedRunInfo>> {
        match self.executor.execute(Command::RunQueryByStatus { state })? {
            Output::RunInfoList(runs) => Ok(runs),
            _ => Err(Error::Internal {
                reason: "Unexpected output for RunQueryByStatus".into(),
            }),
        }
    }

    /// Query runs by tag.
    pub fn run_query_by_tag(&self, tag: &str) -> Result<Vec<VersionedRunInfo>> {
        match self.executor.execute(Command::RunQueryByTag {
            tag: tag.to_string(),
        })? {
            Output::RunInfoList(runs) => Ok(runs),
            _ => Err(Error::Internal {
                reason: "Unexpected output for RunQueryByTag".into(),
            }),
        }
    }

    /// Count runs.
    pub fn run_count(&self, status: Option<RunStatus>) -> Result<u64> {
        match self.executor.execute(Command::RunCount { status })? {
            Output::Uint(count) => Ok(count),
            _ => Err(Error::Internal {
                reason: "Unexpected output for RunCount".into(),
            }),
        }
    }

    /// Search runs.
    pub fn run_search(&self, query: &str, limit: Option<u64>) -> Result<Vec<VersionedRunInfo>> {
        match self.executor.execute(Command::RunSearch {
            query: query.to_string(),
            limit,
        })? {
            Output::RunInfoList(runs) => Ok(runs),
            _ => Err(Error::Internal {
                reason: "Unexpected output for RunSearch".into(),
            }),
        }
    }

    /// Add tags to a run.
    pub fn run_add_tags(&self, run: &str, tags: Vec<String>) -> Result<u64> {
        match self.executor.execute(Command::RunAddTags {
            run: RunId::from(run),
            tags,
        })? {
            Output::Version(v) => Ok(v),
            _ => Err(Error::Internal {
                reason: "Unexpected output for RunAddTags".into(),
            }),
        }
    }

    /// Remove tags from a run.
    pub fn run_remove_tags(&self, run: &str, tags: Vec<String>) -> Result<u64> {
        match self.executor.execute(Command::RunRemoveTags {
            run: RunId::from(run),
            tags,
        })? {
            Output::Version(v) => Ok(v),
            _ => Err(Error::Internal {
                reason: "Unexpected output for RunRemoveTags".into(),
            }),
        }
    }

    /// Get tags for a run.
    pub fn run_get_tags(&self, run: &str) -> Result<Vec<String>> {
        match self.executor.execute(Command::RunGetTags {
            run: RunId::from(run),
        })? {
            Output::Strings(tags) => Ok(tags),
            _ => Err(Error::Internal {
                reason: "Unexpected output for RunGetTags".into(),
            }),
        }
    }

    /// Create a child run.
    pub fn run_create_child(
        &self,
        parent: &str,
        metadata: Option<Value>,
    ) -> Result<(RunInfo, u64)> {
        match self.executor.execute(Command::RunCreateChild {
            parent: RunId::from(parent),
            metadata,
        })? {
            Output::RunWithVersion { info, version } => Ok((info, version)),
            _ => Err(Error::Internal {
                reason: "Unexpected output for RunCreateChild".into(),
            }),
        }
    }

    /// Get child runs.
    pub fn run_get_children(&self, parent: &str) -> Result<Vec<VersionedRunInfo>> {
        match self.executor.execute(Command::RunGetChildren {
            parent: RunId::from(parent),
        })? {
            Output::RunInfoList(runs) => Ok(runs),
            _ => Err(Error::Internal {
                reason: "Unexpected output for RunGetChildren".into(),
            }),
        }
    }

    /// Get parent run.
    pub fn run_get_parent(&self, run: &str) -> Result<Option<RunId>> {
        match self.executor.execute(Command::RunGetParent {
            run: RunId::from(run),
        })? {
            Output::MaybeRunId(id) => Ok(id),
            _ => Err(Error::Internal {
                reason: "Unexpected output for RunGetParent".into(),
            }),
        }
    }

    /// Set retention policy for a run.
    pub fn run_set_retention(&self, run: &str, policy: RetentionPolicyInfo) -> Result<u64> {
        match self.executor.execute(Command::RunSetRetention {
            run: RunId::from(run),
            policy,
        })? {
            Output::Version(v) => Ok(v),
            _ => Err(Error::Internal {
                reason: "Unexpected output for RunSetRetention".into(),
            }),
        }
    }

    /// Get retention policy for a run.
    pub fn run_get_retention(&self, run: &str) -> Result<RetentionPolicyInfo> {
        match self.executor.execute(Command::RunGetRetention {
            run: RunId::from(run),
        })? {
            Output::RetentionPolicy(policy) => Ok(policy),
            _ => Err(Error::Internal {
                reason: "Unexpected output for RunGetRetention".into(),
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
    use strata_engine::Database;

    fn create_strata() -> Strata {
        let db = Arc::new(Database::builder().no_durability().open_temp().unwrap());
        Strata::new(db)
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

        let version = db.kv_put("key1", Value::String("hello".into())).unwrap();
        assert!(version > 0);

        let value = db.kv_get("key1").unwrap();
        assert!(value.is_some());
        assert_eq!(value.unwrap().value, Value::String("hello".into()));
    }

    #[test]
    fn test_kv_exists_delete() {
        let db = create_strata();

        db.kv_put("key1", Value::Int(42)).unwrap();
        assert!(db.kv_exists("key1").unwrap());

        db.kv_delete("key1").unwrap();
        assert!(!db.kv_exists("key1").unwrap());
    }

    #[test]
    fn test_kv_incr() {
        let db = create_strata();

        db.kv_put("counter", Value::Int(10)).unwrap();
        let val = db.kv_incr("counter", 5).unwrap();
        assert_eq!(val, 15);
    }

    #[test]
    fn test_state_set_get() {
        let db = create_strata();

        db.state_set("cell", Value::String("state".into())).unwrap();
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
}
