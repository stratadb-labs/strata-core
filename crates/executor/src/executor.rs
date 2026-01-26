//! The Executor - single entry point to Strata's engine.
//!
//! The Executor is a stateless dispatcher that routes commands to the
//! appropriate primitive operations and converts results to outputs.

use std::sync::Arc;

use strata_api::substrate::SubstrateImpl;

use crate::{Command, Error, Output, Result};

/// The command executor - single entry point to Strata's engine.
///
/// The Executor is **stateless**: it holds references to the database substrate
/// but maintains no state of its own. All state lives in the engine.
///
/// # Thread Safety
///
/// Executor is `Send + Sync` and can be shared across threads.
///
/// # Example
///
/// ```ignore
/// use strata_executor::{Command, Executor, RunId};
/// use strata_core::Value;
///
/// let executor = Executor::new(substrate);
///
/// // Single command execution
/// let result = executor.execute(Command::KvPut {
///     run: RunId::default(),
///     key: "foo".into(),
///     value: Value::Int(42),
/// })?;
///
/// // Batch execution
/// let results = executor.execute_many(vec![
///     Command::KvGet { run: RunId::default(), key: "foo".into() },
///     Command::KvGet { run: RunId::default(), key: "bar".into() },
/// ]);
/// ```
pub struct Executor {
    substrate: Arc<SubstrateImpl>,
}

impl Executor {
    /// Create a new executor wrapping a database substrate.
    pub fn new(substrate: Arc<SubstrateImpl>) -> Self {
        Self { substrate }
    }

    /// Execute a single command.
    ///
    /// Returns the command result or an error.
    pub fn execute(&self, cmd: Command) -> Result<Output> {
        match cmd {
            // Database commands (implemented as examples)
            Command::Ping => Ok(Output::Pong {
                version: env!("CARGO_PKG_VERSION").to_string(),
            }),
            Command::Info => {
                // TODO: Implement properly
                Ok(Output::DatabaseInfo(crate::types::DatabaseInfo {
                    version: env!("CARGO_PKG_VERSION").to_string(),
                    uptime_secs: 0,
                    run_count: 0,
                    total_keys: 0,
                }))
            }
            Command::Flush => {
                // TODO: Call substrate flush
                Ok(Output::Unit)
            }
            Command::Compact => {
                // TODO: Call substrate compact
                Ok(Output::Unit)
            }

            // KV commands
            Command::KvPut { run, key, value } => {
                crate::handlers::kv::kv_put(&self.substrate, run, key, value)
            }
            Command::KvGet { run, key } => {
                crate::handlers::kv::kv_get(&self.substrate, run, key)
            }
            Command::KvGetAt { run, key, version } => {
                crate::handlers::kv::kv_get_at(&self.substrate, run, key, version)
            }
            Command::KvDelete { run, key } => {
                crate::handlers::kv::kv_delete(&self.substrate, run, key)
            }
            Command::KvExists { run, key } => {
                crate::handlers::kv::kv_exists(&self.substrate, run, key)
            }
            Command::KvHistory {
                run,
                key,
                limit,
                before,
            } => crate::handlers::kv::kv_history(&self.substrate, run, key, limit, before),
            Command::KvIncr { run, key, delta } => {
                crate::handlers::kv::kv_incr(&self.substrate, run, key, delta)
            }
            Command::KvCasVersion {
                run,
                key,
                expected_version,
                new_value,
            } => crate::handlers::kv::kv_cas_version(
                &self.substrate,
                run,
                key,
                expected_version,
                new_value,
            ),
            Command::KvCasValue {
                run,
                key,
                expected_value,
                new_value,
            } => crate::handlers::kv::kv_cas_value(
                &self.substrate,
                run,
                key,
                expected_value,
                new_value,
            ),
            Command::KvKeys { run, prefix, limit } => {
                crate::handlers::kv::kv_keys(&self.substrate, run, prefix, limit)
            }
            Command::KvScan {
                run,
                prefix,
                limit,
                cursor,
            } => crate::handlers::kv::kv_scan(&self.substrate, run, prefix, limit, cursor),
            Command::KvMget { run, keys } => {
                crate::handlers::kv::kv_mget(&self.substrate, run, keys)
            }
            Command::KvMput { run, entries } => {
                crate::handlers::kv::kv_mput(&self.substrate, run, entries)
            }
            Command::KvMdelete { run, keys } => {
                crate::handlers::kv::kv_mdelete(&self.substrate, run, keys)
            }
            Command::KvMexists { run, keys } => {
                crate::handlers::kv::kv_mexists(&self.substrate, run, keys)
            }

            // JSON commands
            Command::JsonSet {
                run,
                key,
                path,
                value,
            } => crate::handlers::json::json_set(&self.substrate, run, key, path, value),
            Command::JsonGet { run, key, path } => {
                crate::handlers::json::json_get(&self.substrate, run, key, path)
            }
            Command::JsonDelete { run, key, path } => {
                crate::handlers::json::json_delete(&self.substrate, run, key, path)
            }
            Command::JsonMerge {
                run,
                key,
                path,
                patch,
            } => crate::handlers::json::json_merge(&self.substrate, run, key, path, patch),
            Command::JsonHistory {
                run,
                key,
                limit,
                before,
            } => crate::handlers::json::json_history(&self.substrate, run, key, limit, before),
            Command::JsonExists { run, key } => {
                crate::handlers::json::json_exists(&self.substrate, run, key)
            }
            Command::JsonGetVersion { run, key } => {
                crate::handlers::json::json_get_version(&self.substrate, run, key)
            }
            Command::JsonSearch { run, query, k } => {
                crate::handlers::json::json_search(&self.substrate, run, query, k)
            }
            Command::JsonList {
                run,
                prefix,
                cursor,
                limit,
            } => crate::handlers::json::json_list(&self.substrate, run, prefix, cursor, limit),
            Command::JsonCas {
                run,
                key,
                expected_version,
                path,
                value,
            } => crate::handlers::json::json_cas(
                &self.substrate,
                run,
                key,
                expected_version,
                path,
                value,
            ),
            Command::JsonQuery {
                run,
                path,
                value,
                limit,
            } => crate::handlers::json::json_query(&self.substrate, run, path, value, limit),
            Command::JsonCount { run } => {
                crate::handlers::json::json_count(&self.substrate, run)
            }
            Command::JsonBatchGet { run, keys } => {
                crate::handlers::json::json_batch_get(&self.substrate, run, keys)
            }
            Command::JsonBatchCreate { run, docs } => {
                crate::handlers::json::json_batch_create(&self.substrate, run, docs)
            }
            Command::JsonArrayPush {
                run,
                key,
                path,
                values,
            } => crate::handlers::json::json_array_push(&self.substrate, run, key, path, values),
            Command::JsonIncrement {
                run,
                key,
                path,
                delta,
            } => crate::handlers::json::json_increment(&self.substrate, run, key, path, delta),
            Command::JsonArrayPop { run, key, path } => {
                crate::handlers::json::json_array_pop(&self.substrate, run, key, path)
            }

            // Event commands
            Command::EventAppend {
                run,
                stream,
                payload,
            } => crate::handlers::event::event_append(&self.substrate, run, stream, payload),
            Command::EventAppendBatch { run, events } => {
                crate::handlers::event::event_append_batch(&self.substrate, run, events)
            }
            Command::EventRange {
                run,
                stream,
                start,
                end,
                limit,
            } => crate::handlers::event::event_range(&self.substrate, run, stream, start, end, limit),
            Command::EventGet {
                run,
                stream,
                sequence,
            } => crate::handlers::event::event_get(&self.substrate, run, stream, sequence),
            Command::EventLen { run, stream } => {
                crate::handlers::event::event_len(&self.substrate, run, stream)
            }
            Command::EventLatestSequence { run, stream } => {
                crate::handlers::event::event_latest_sequence(&self.substrate, run, stream)
            }
            Command::EventStreamInfo { run, stream } => {
                crate::handlers::event::event_stream_info(&self.substrate, run, stream)
            }
            Command::EventRevRange {
                run,
                stream,
                start,
                end,
                limit,
            } => crate::handlers::event::event_rev_range(&self.substrate, run, stream, start, end, limit),
            Command::EventStreams { run } => {
                crate::handlers::event::event_streams(&self.substrate, run)
            }
            Command::EventHead { run, stream } => {
                crate::handlers::event::event_head(&self.substrate, run, stream)
            }
            Command::EventVerifyChain { run } => {
                crate::handlers::event::event_verify_chain(&self.substrate, run)
            }

            // State commands
            Command::StateSet { run, cell, value } => {
                crate::handlers::state::state_set(&self.substrate, run, cell, value)
            }
            Command::StateGet { run, cell } => {
                crate::handlers::state::state_get(&self.substrate, run, cell)
            }
            Command::StateCas {
                run,
                cell,
                expected_counter,
                value,
            } => crate::handlers::state::state_cas(&self.substrate, run, cell, expected_counter, value),
            Command::StateDelete { run, cell } => {
                crate::handlers::state::state_delete(&self.substrate, run, cell)
            }
            Command::StateExists { run, cell } => {
                crate::handlers::state::state_exists(&self.substrate, run, cell)
            }
            Command::StateHistory {
                run,
                cell,
                limit,
                before,
            } => crate::handlers::state::state_history(&self.substrate, run, cell, limit, before),
            Command::StateInit { run, cell, value } => {
                crate::handlers::state::state_init(&self.substrate, run, cell, value)
            }
            Command::StateList { run } => {
                crate::handlers::state::state_list(&self.substrate, run)
            }

            // Vector commands
            Command::VectorUpsert {
                run,
                collection,
                key,
                vector,
                metadata,
            } => crate::handlers::vector::vector_upsert(
                &self.substrate,
                run,
                collection,
                key,
                vector,
                metadata,
            ),
            Command::VectorGet {
                run,
                collection,
                key,
            } => crate::handlers::vector::vector_get(&self.substrate, run, collection, key),
            Command::VectorDelete {
                run,
                collection,
                key,
            } => crate::handlers::vector::vector_delete(&self.substrate, run, collection, key),
            Command::VectorSearch {
                run,
                collection,
                query,
                k,
                filter,
                metric,
            } => crate::handlers::vector::vector_search(
                &self.substrate,
                run,
                collection,
                query,
                k,
                filter,
                metric,
            ),
            Command::VectorCollectionInfo { run, collection } => {
                crate::handlers::vector::vector_collection_info(&self.substrate, run, collection)
            }
            Command::VectorCreateCollection {
                run,
                collection,
                dimension,
                metric,
            } => crate::handlers::vector::vector_create_collection(
                &self.substrate,
                run,
                collection,
                dimension,
                metric,
            ),
            Command::VectorDropCollection { run, collection } => {
                crate::handlers::vector::vector_drop_collection(&self.substrate, run, collection)
            }
            Command::VectorListCollections { run } => {
                crate::handlers::vector::vector_list_collections(&self.substrate, run)
            }
            Command::VectorCollectionExists { run, collection } => {
                crate::handlers::vector::vector_collection_exists(&self.substrate, run, collection)
            }
            Command::VectorCount { run, collection } => {
                crate::handlers::vector::vector_count(&self.substrate, run, collection)
            }
            Command::VectorUpsertBatch {
                run,
                collection,
                vectors,
            } => crate::handlers::vector::vector_upsert_batch(&self.substrate, run, collection, vectors),
            Command::VectorGetBatch {
                run,
                collection,
                keys,
            } => crate::handlers::vector::vector_get_batch(&self.substrate, run, collection, keys),
            Command::VectorDeleteBatch {
                run,
                collection,
                keys,
            } => crate::handlers::vector::vector_delete_batch(&self.substrate, run, collection, keys),
            Command::VectorHistory {
                run,
                collection,
                key,
                limit,
                before_version,
            } => crate::handlers::vector::vector_history(
                &self.substrate,
                run,
                collection,
                key,
                limit,
                before_version,
            ),
            Command::VectorGetAt {
                run,
                collection,
                key,
                version,
            } => crate::handlers::vector::vector_get_at(&self.substrate, run, collection, key, version),
            Command::VectorListKeys {
                run,
                collection,
                limit,
                cursor,
            } => crate::handlers::vector::vector_list_keys(&self.substrate, run, collection, limit, cursor),
            Command::VectorScan {
                run,
                collection,
                limit,
                cursor,
            } => crate::handlers::vector::vector_scan(&self.substrate, run, collection, limit, cursor),

            // Run commands
            Command::RunCreate { run_id, metadata } => {
                crate::handlers::run::run_create(&self.substrate, run_id, metadata)
            }
            Command::RunGet { run } => crate::handlers::run::run_get(&self.substrate, run),
            Command::RunList {
                state,
                limit,
                offset,
            } => crate::handlers::run::run_list(&self.substrate, state, limit, offset),
            Command::RunClose { run } => crate::handlers::run::run_close(&self.substrate, run),
            Command::RunUpdateMetadata { run, metadata } => {
                crate::handlers::run::run_update_metadata(&self.substrate, run, metadata)
            }
            Command::RunExists { run } => crate::handlers::run::run_exists(&self.substrate, run),
            Command::RunPause { run } => crate::handlers::run::run_pause(&self.substrate, run),
            Command::RunResume { run } => crate::handlers::run::run_resume(&self.substrate, run),
            Command::RunFail { run, error } => {
                crate::handlers::run::run_fail(&self.substrate, run, error)
            }
            Command::RunCancel { run } => crate::handlers::run::run_cancel(&self.substrate, run),
            Command::RunArchive { run } => crate::handlers::run::run_archive(&self.substrate, run),
            Command::RunDelete { run } => crate::handlers::run::run_delete(&self.substrate, run),
            Command::RunQueryByStatus { state } => {
                crate::handlers::run::run_query_by_status(&self.substrate, state)
            }
            Command::RunQueryByTag { tag } => {
                crate::handlers::run::run_query_by_tag(&self.substrate, tag)
            }
            Command::RunCount { status } => {
                crate::handlers::run::run_count(&self.substrate, status)
            }
            Command::RunSearch { query, limit } => {
                crate::handlers::run::run_search(&self.substrate, query, limit)
            }
            Command::RunAddTags { run, tags } => {
                crate::handlers::run::run_add_tags(&self.substrate, run, tags)
            }
            Command::RunRemoveTags { run, tags } => {
                crate::handlers::run::run_remove_tags(&self.substrate, run, tags)
            }
            Command::RunGetTags { run } => crate::handlers::run::run_get_tags(&self.substrate, run),
            Command::RunCreateChild { parent, metadata } => {
                crate::handlers::run::run_create_child(&self.substrate, parent, metadata)
            }
            Command::RunGetChildren { parent } => {
                crate::handlers::run::run_get_children(&self.substrate, parent)
            }
            Command::RunGetParent { run } => {
                crate::handlers::run::run_get_parent(&self.substrate, run)
            }
            Command::RunSetRetention { run, policy } => {
                crate::handlers::run::run_set_retention(&self.substrate, run, policy)
            }
            Command::RunGetRetention { run } => {
                crate::handlers::run::run_get_retention(&self.substrate, run)
            }

            // Transaction commands - will be implemented in Phase 3
            Command::TxnBegin { .. }
            | Command::TxnCommit
            | Command::TxnRollback
            | Command::TxnInfo
            | Command::TxnIsActive => {
                Err(Error::Internal {
                    reason: "Transaction commands not yet implemented".to_string(),
                })
            }

            // Retention commands - will be implemented in Phase 3
            Command::RetentionApply { .. }
            | Command::RetentionStats { .. }
            | Command::RetentionPreview { .. } => {
                Err(Error::Internal {
                    reason: "Retention commands not yet implemented".to_string(),
                })
            }
        }
    }

    /// Execute multiple commands sequentially.
    ///
    /// Returns all results in the same order as the input commands.
    /// Execution continues even if some commands fail.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let results = executor.execute_many(vec![cmd1, cmd2, cmd3]);
    /// // results[0] corresponds to cmd1, etc.
    /// ```
    pub fn execute_many(&self, cmds: Vec<Command>) -> Vec<Result<Output>> {
        cmds.into_iter().map(|cmd| self.execute(cmd)).collect()
    }

    /// Get a reference to the underlying substrate.
    ///
    /// This is an escape hatch for advanced use cases.
    pub fn substrate(&self) -> &Arc<SubstrateImpl> {
        &self.substrate
    }
}

// Executor is thread-safe
// SAFETY: Executor only contains Arc<SubstrateImpl> which is Send + Sync
unsafe impl Send for Executor {}
unsafe impl Sync for Executor {}
