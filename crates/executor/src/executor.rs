//! The Executor - single entry point to Strata's engine.
//!
//! The Executor is a stateless dispatcher that routes commands to the
//! appropriate primitive operations and converts results to outputs.

use std::sync::Arc;

use strata_engine::Database;

use crate::bridge::Primitives;
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
/// // Run is optional - omit it to use the default run
/// let result = executor.execute(Command::KvPut {
///     run: None,
///     key: "foo".into(),
///     value: Value::Int(42),
/// })?;
///
/// // Or provide an explicit run
/// let result = executor.execute(Command::KvPut {
///     run: Some(RunId::from("my-run")),
///     key: "foo".into(),
///     value: Value::Int(42),
/// })?;
/// ```
pub struct Executor {
    primitives: Arc<Primitives>,
}

impl Executor {
    /// Create a new executor from a database instance.
    pub fn new(db: Arc<Database>) -> Self {
        Self {
            primitives: Arc::new(Primitives::new(db)),
        }
    }

    /// Execute a single command.
    ///
    /// Resolves any `None` run fields to the default run before dispatch.
    /// Returns the command result or an error.
    pub fn execute(&self, mut cmd: Command) -> Result<Output> {
        cmd.resolve_default_run();

        match cmd {
            // Database commands
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

            // KV commands (MVP: 4 commands)
            Command::KvPut { run, key, value } => {
                let run = run.expect("resolved by resolve_default_run");
                crate::handlers::kv::kv_put(&self.primitives, run, key, value)
            }
            Command::KvGet { run, key } => {
                let run = run.expect("resolved by resolve_default_run");
                crate::handlers::kv::kv_get(&self.primitives, run, key)
            }
            Command::KvDelete { run, key } => {
                let run = run.expect("resolved by resolve_default_run");
                crate::handlers::kv::kv_delete(&self.primitives, run, key)
            }
            Command::KvList { run, prefix } => {
                let run = run.expect("resolved by resolve_default_run");
                crate::handlers::kv::kv_list(&self.primitives, run, prefix)
            }

            // JSON commands (4 MVP)
            Command::JsonSet {
                run,
                key,
                path,
                value,
            } => {
                let run = run.expect("resolved by resolve_default_run");
                crate::handlers::json::json_set(&self.primitives, run, key, path, value)
            }
            Command::JsonGet { run, key, path } => {
                let run = run.expect("resolved by resolve_default_run");
                crate::handlers::json::json_get(&self.primitives, run, key, path)
            }
            Command::JsonDelete { run, key, path } => {
                let run = run.expect("resolved by resolve_default_run");
                crate::handlers::json::json_delete(&self.primitives, run, key, path)
            }
            Command::JsonList {
                run,
                prefix,
                cursor,
                limit,
            } => {
                let run = run.expect("resolved by resolve_default_run");
                crate::handlers::json::json_list(&self.primitives, run, prefix, cursor, limit)
            }

            // Event commands
            Command::EventAppend {
                run,
                stream,
                payload,
            } => {
                let run = run.expect("resolved by resolve_default_run");
                crate::handlers::event::event_append(&self.primitives, run, stream, payload)
            }
            Command::EventAppendBatch { run, events } => {
                let run = run.expect("resolved by resolve_default_run");
                crate::handlers::event::event_append_batch(&self.primitives, run, events)
            }
            Command::EventRange {
                run,
                stream,
                start,
                end,
                limit,
            } => {
                let run = run.expect("resolved by resolve_default_run");
                crate::handlers::event::event_range(&self.primitives, run, stream, start, end, limit)
            }
            Command::EventRead {
                run,
                stream,
                sequence,
            } => {
                let run = run.expect("resolved by resolve_default_run");
                crate::handlers::event::event_read(&self.primitives, run, stream, sequence)
            }
            Command::EventLen { run, stream } => {
                let run = run.expect("resolved by resolve_default_run");
                crate::handlers::event::event_len(&self.primitives, run, stream)
            }
            Command::EventLatestSequence { run, stream } => {
                let run = run.expect("resolved by resolve_default_run");
                crate::handlers::event::event_latest_sequence(&self.primitives, run, stream)
            }
            Command::EventStreamInfo { run, stream } => {
                let run = run.expect("resolved by resolve_default_run");
                crate::handlers::event::event_stream_info(&self.primitives, run, stream)
            }
            Command::EventRevRange {
                run,
                stream,
                start,
                end,
                limit,
            } => {
                let run = run.expect("resolved by resolve_default_run");
                crate::handlers::event::event_rev_range(&self.primitives, run, stream, start, end, limit)
            }
            Command::EventStreams { run } => {
                let run = run.expect("resolved by resolve_default_run");
                crate::handlers::event::event_streams(&self.primitives, run)
            }
            Command::EventHead { run, stream } => {
                let run = run.expect("resolved by resolve_default_run");
                crate::handlers::event::event_head(&self.primitives, run, stream)
            }
            Command::EventVerifyChain { run } => {
                let run = run.expect("resolved by resolve_default_run");
                crate::handlers::event::event_verify_chain(&self.primitives, run)
            }

            // State commands
            Command::StateSet { run, cell, value } => {
                let run = run.expect("resolved by resolve_default_run");
                crate::handlers::state::state_set(&self.primitives, run, cell, value)
            }
            Command::StateRead { run, cell } => {
                let run = run.expect("resolved by resolve_default_run");
                crate::handlers::state::state_read(&self.primitives, run, cell)
            }
            Command::StateCas {
                run,
                cell,
                expected_counter,
                value,
            } => {
                let run = run.expect("resolved by resolve_default_run");
                crate::handlers::state::state_cas(&self.primitives, run, cell, expected_counter, value)
            }
            Command::StateDelete { run, cell } => {
                let run = run.expect("resolved by resolve_default_run");
                crate::handlers::state::state_delete(&self.primitives, run, cell)
            }
            Command::StateExists { run, cell } => {
                let run = run.expect("resolved by resolve_default_run");
                crate::handlers::state::state_exists(&self.primitives, run, cell)
            }
            Command::StateHistory {
                run,
                cell,
                limit,
                before,
            } => {
                let run = run.expect("resolved by resolve_default_run");
                crate::handlers::state::state_history(&self.primitives, run, cell, limit, before)
            }
            Command::StateInit { run, cell, value } => {
                let run = run.expect("resolved by resolve_default_run");
                crate::handlers::state::state_init(&self.primitives, run, cell, value)
            }
            Command::StateList { run } => {
                let run = run.expect("resolved by resolve_default_run");
                crate::handlers::state::state_list(&self.primitives, run)
            }

            // Vector commands
            Command::VectorUpsert {
                run,
                collection,
                key,
                vector,
                metadata,
            } => {
                let run = run.expect("resolved by resolve_default_run");
                crate::handlers::vector::vector_upsert(
                    &self.primitives,
                    run,
                    collection,
                    key,
                    vector,
                    metadata,
                )
            }
            Command::VectorGet {
                run,
                collection,
                key,
            } => {
                let run = run.expect("resolved by resolve_default_run");
                crate::handlers::vector::vector_get(&self.primitives, run, collection, key)
            }
            Command::VectorDelete {
                run,
                collection,
                key,
            } => {
                let run = run.expect("resolved by resolve_default_run");
                crate::handlers::vector::vector_delete(&self.primitives, run, collection, key)
            }
            Command::VectorSearch {
                run,
                collection,
                query,
                k,
                filter,
                metric,
            } => {
                let run = run.expect("resolved by resolve_default_run");
                crate::handlers::vector::vector_search(
                    &self.primitives,
                    run,
                    collection,
                    query,
                    k,
                    filter,
                    metric,
                )
            }
            Command::VectorGetCollection { run, collection } => {
                let run = run.expect("resolved by resolve_default_run");
                crate::handlers::vector::vector_get_collection(&self.primitives, run, collection)
            }
            Command::VectorCreateCollection {
                run,
                collection,
                dimension,
                metric,
            } => {
                let run = run.expect("resolved by resolve_default_run");
                crate::handlers::vector::vector_create_collection(
                    &self.primitives,
                    run,
                    collection,
                    dimension,
                    metric,
                )
            }
            Command::VectorDeleteCollection { run, collection } => {
                let run = run.expect("resolved by resolve_default_run");
                crate::handlers::vector::vector_delete_collection(&self.primitives, run, collection)
            }
            Command::VectorListCollections { run } => {
                let run = run.expect("resolved by resolve_default_run");
                crate::handlers::vector::vector_list_collections(&self.primitives, run)
            }
            Command::VectorCollectionExists { run, collection } => {
                let run = run.expect("resolved by resolve_default_run");
                crate::handlers::vector::vector_collection_exists(&self.primitives, run, collection)
            }
            Command::VectorCount { run, collection } => {
                let run = run.expect("resolved by resolve_default_run");
                crate::handlers::vector::vector_count(&self.primitives, run, collection)
            }
            Command::VectorUpsertBatch {
                run,
                collection,
                vectors,
            } => {
                let run = run.expect("resolved by resolve_default_run");
                crate::handlers::vector::vector_upsert_batch(&self.primitives, run, collection, vectors)
            }
            Command::VectorGetBatch {
                run,
                collection,
                keys,
            } => {
                let run = run.expect("resolved by resolve_default_run");
                crate::handlers::vector::vector_get_batch(&self.primitives, run, collection, keys)
            }
            Command::VectorDeleteBatch {
                run,
                collection,
                keys,
            } => {
                let run = run.expect("resolved by resolve_default_run");
                crate::handlers::vector::vector_delete_batch(&self.primitives, run, collection, keys)
            }
            Command::VectorHistory {
                run,
                collection,
                key,
                limit,
                before_version,
            } => {
                let run = run.expect("resolved by resolve_default_run");
                crate::handlers::vector::vector_history(
                    &self.primitives,
                    run,
                    collection,
                    key,
                    limit,
                    before_version,
                )
            }
            Command::VectorGetAt {
                run,
                collection,
                key,
                version,
            } => {
                let run = run.expect("resolved by resolve_default_run");
                crate::handlers::vector::vector_get_at(&self.primitives, run, collection, key, version)
            }
            Command::VectorListKeys {
                run,
                collection,
                limit,
                cursor,
            } => {
                let run = run.expect("resolved by resolve_default_run");
                crate::handlers::vector::vector_list_keys(&self.primitives, run, collection, limit, cursor)
            }
            Command::VectorScan {
                run,
                collection,
                limit,
                cursor,
            } => {
                let run = run.expect("resolved by resolve_default_run");
                crate::handlers::vector::vector_scan(&self.primitives, run, collection, limit, cursor)
            }

            // Run commands (5 MVP)
            Command::RunCreate { run_id, metadata } => {
                crate::handlers::run::run_create(&self.primitives, run_id, metadata)
            }
            Command::RunGet { run } => crate::handlers::run::run_get(&self.primitives, run),
            Command::RunList {
                state,
                limit,
                offset,
            } => crate::handlers::run::run_list(&self.primitives, state, limit, offset),
            Command::RunExists { run } => crate::handlers::run::run_exists(&self.primitives, run),
            Command::RunDelete { run } => crate::handlers::run::run_delete(&self.primitives, run),

            // Transaction commands - handled by Session, not Executor
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

            // Intelligence commands
            Command::Search { run, query, k, primitives } => {
                let run = run.expect("resolved by resolve_default_run");
                crate::handlers::search::search(&self.primitives, run, query, k, primitives)
            }
        }
    }

    /// Execute multiple commands sequentially.
    ///
    /// Returns all results in the same order as the input commands.
    /// Execution continues even if some commands fail.
    pub fn execute_many(&self, cmds: Vec<Command>) -> Vec<Result<Output>> {
        cmds.into_iter().map(|cmd| self.execute(cmd)).collect()
    }

    /// Get a reference to the underlying primitives.
    pub fn primitives(&self) -> &Arc<Primitives> {
        &self.primitives
    }
}

// Executor is thread-safe
// SAFETY: Executor only contains Arc<Primitives> which is Send + Sync
unsafe impl Send for Executor {}
unsafe impl Sync for Executor {}
