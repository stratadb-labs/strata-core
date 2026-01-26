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

            // KV commands - will be implemented in Phase 3
            Command::KvPut { .. }
            | Command::KvGet { .. }
            | Command::KvGetAt { .. }
            | Command::KvDelete { .. }
            | Command::KvExists { .. }
            | Command::KvHistory { .. }
            | Command::KvIncr { .. }
            | Command::KvCasVersion { .. }
            | Command::KvCasValue { .. }
            | Command::KvKeys { .. }
            | Command::KvScan { .. }
            | Command::KvMget { .. }
            | Command::KvMput { .. }
            | Command::KvMdelete { .. }
            | Command::KvMexists { .. } => {
                Err(Error::Internal {
                    reason: "KV commands not yet implemented".to_string(),
                })
            }

            // JSON commands - will be implemented in Phase 3
            Command::JsonSet { .. }
            | Command::JsonGet { .. }
            | Command::JsonDelete { .. }
            | Command::JsonMerge { .. }
            | Command::JsonHistory { .. }
            | Command::JsonExists { .. }
            | Command::JsonGetVersion { .. }
            | Command::JsonSearch { .. }
            | Command::JsonList { .. }
            | Command::JsonCas { .. }
            | Command::JsonQuery { .. }
            | Command::JsonCount { .. }
            | Command::JsonBatchGet { .. }
            | Command::JsonBatchCreate { .. }
            | Command::JsonArrayPush { .. }
            | Command::JsonIncrement { .. }
            | Command::JsonArrayPop { .. } => {
                Err(Error::Internal {
                    reason: "JSON commands not yet implemented".to_string(),
                })
            }

            // Event commands - will be implemented in Phase 3
            Command::EventAppend { .. }
            | Command::EventAppendBatch { .. }
            | Command::EventRange { .. }
            | Command::EventGet { .. }
            | Command::EventLen { .. }
            | Command::EventLatestSequence { .. }
            | Command::EventStreamInfo { .. }
            | Command::EventRevRange { .. }
            | Command::EventStreams { .. }
            | Command::EventHead { .. }
            | Command::EventVerifyChain { .. } => {
                Err(Error::Internal {
                    reason: "Event commands not yet implemented".to_string(),
                })
            }

            // State commands - will be implemented in Phase 3
            Command::StateSet { .. }
            | Command::StateGet { .. }
            | Command::StateCas { .. }
            | Command::StateDelete { .. }
            | Command::StateExists { .. }
            | Command::StateHistory { .. }
            | Command::StateInit { .. }
            | Command::StateList { .. } => {
                Err(Error::Internal {
                    reason: "State commands not yet implemented".to_string(),
                })
            }

            // Vector commands - will be implemented in Phase 3
            Command::VectorUpsert { .. }
            | Command::VectorGet { .. }
            | Command::VectorDelete { .. }
            | Command::VectorSearch { .. }
            | Command::VectorCollectionInfo { .. }
            | Command::VectorCreateCollection { .. }
            | Command::VectorDropCollection { .. }
            | Command::VectorListCollections { .. }
            | Command::VectorCollectionExists { .. }
            | Command::VectorCount { .. }
            | Command::VectorUpsertBatch { .. }
            | Command::VectorGetBatch { .. }
            | Command::VectorDeleteBatch { .. }
            | Command::VectorHistory { .. }
            | Command::VectorGetAt { .. }
            | Command::VectorListKeys { .. }
            | Command::VectorScan { .. } => {
                Err(Error::Internal {
                    reason: "Vector commands not yet implemented".to_string(),
                })
            }

            // Run commands - will be implemented in Phase 3
            Command::RunCreate { .. }
            | Command::RunGet { .. }
            | Command::RunList { .. }
            | Command::RunClose { .. }
            | Command::RunUpdateMetadata { .. }
            | Command::RunExists { .. }
            | Command::RunPause { .. }
            | Command::RunResume { .. }
            | Command::RunFail { .. }
            | Command::RunCancel { .. }
            | Command::RunArchive { .. }
            | Command::RunDelete { .. }
            | Command::RunQueryByStatus { .. }
            | Command::RunQueryByTag { .. }
            | Command::RunCount { .. }
            | Command::RunSearch { .. }
            | Command::RunAddTags { .. }
            | Command::RunRemoveTags { .. }
            | Command::RunGetTags { .. }
            | Command::RunCreateChild { .. }
            | Command::RunGetChildren { .. }
            | Command::RunGetParent { .. }
            | Command::RunSetRetention { .. }
            | Command::RunGetRetention { .. } => {
                Err(Error::Internal {
                    reason: "Run commands not yet implemented".to_string(),
                })
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
