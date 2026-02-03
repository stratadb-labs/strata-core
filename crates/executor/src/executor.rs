//! The Executor - single entry point to Strata's engine.
//!
//! The Executor is a stateless dispatcher that routes commands to the
//! appropriate primitive operations and converts results to outputs.

use std::sync::Arc;

use strata_engine::Database;
use strata_security::AccessMode;

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
/// use strata_executor::{Command, Executor, BranchId};
/// use strata_core::Value;
///
/// let executor = Executor::new(substrate);
///
/// // Branch is optional - omit it to use the default branch
/// let result = executor.execute(Command::KvPut {
///     branch: None,
///     key: "foo".into(),
///     value: Value::Int(42),
/// })?;
///
/// // Or provide an explicit branch
/// let result = executor.execute(Command::KvPut {
///     branch: Some(BranchId::from("my-branch")),
///     key: "foo".into(),
///     value: Value::Int(42),
/// })?;
/// ```
pub struct Executor {
    primitives: Arc<Primitives>,
    access_mode: AccessMode,
}

impl Executor {
    /// Create a new executor from a database instance.
    pub fn new(db: Arc<Database>) -> Self {
        Self {
            primitives: Arc::new(Primitives::new(db)),
            access_mode: AccessMode::ReadWrite,
        }
    }

    /// Create a new executor with an explicit access mode.
    pub fn new_with_mode(db: Arc<Database>, access_mode: AccessMode) -> Self {
        Self {
            primitives: Arc::new(Primitives::new(db)),
            access_mode,
        }
    }

    /// Returns the access mode of this executor.
    pub fn access_mode(&self) -> AccessMode {
        self.access_mode
    }

    /// Execute a single command.
    ///
    /// Resolves any `None` branch fields to the default branch before dispatch.
    /// Returns the command result or an error.
    pub fn execute(&self, mut cmd: Command) -> Result<Output> {
        if self.access_mode == AccessMode::ReadOnly && cmd.is_write() {
            return Err(Error::AccessDenied {
                command: cmd.name().to_string(),
            });
        }

        cmd.resolve_default_branch();

        match cmd {
            // Database commands
            Command::Ping => Ok(Output::Pong {
                version: env!("CARGO_PKG_VERSION").to_string(),
            }),
            Command::Info => {
                let branch_count = self
                    .primitives
                    .branch
                    .list_branches()
                    .map(|ids| ids.len() as u64)
                    .unwrap_or(0);
                Ok(Output::DatabaseInfo(crate::types::DatabaseInfo {
                    version: env!("CARGO_PKG_VERSION").to_string(),
                    uptime_secs: 0,
                    branch_count,
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
            Command::KvPut { branch, key, value } => {
                let branch = branch.ok_or(Error::InvalidInput {
                    reason: "Branch must be specified or resolved to default".into(),
                })?;
                crate::handlers::kv::kv_put(&self.primitives, branch, key, value)
            }
            Command::KvGet { branch, key } => {
                let branch = branch.ok_or(Error::InvalidInput {
                    reason: "Branch must be specified or resolved to default".into(),
                })?;
                crate::handlers::kv::kv_get(&self.primitives, branch, key)
            }
            Command::KvDelete { branch, key } => {
                let branch = branch.ok_or(Error::InvalidInput {
                    reason: "Branch must be specified or resolved to default".into(),
                })?;
                crate::handlers::kv::kv_delete(&self.primitives, branch, key)
            }
            Command::KvList {
                branch,
                prefix,
                cursor,
                limit,
            } => {
                let branch = branch.ok_or(Error::InvalidInput {
                    reason: "Branch must be specified or resolved to default".into(),
                })?;
                crate::handlers::kv::kv_list(&self.primitives, branch, prefix, cursor, limit)
            }
            Command::KvGetv { branch, key } => {
                let branch = branch.ok_or(Error::InvalidInput {
                    reason: "Branch must be specified or resolved to default".into(),
                })?;
                crate::handlers::kv::kv_getv(&self.primitives, branch, key)
            }

            // JSON commands
            Command::JsonSet {
                branch,
                key,
                path,
                value,
            } => {
                let branch = branch.ok_or(Error::InvalidInput {
                    reason: "Branch must be specified or resolved to default".into(),
                })?;
                crate::handlers::json::json_set(&self.primitives, branch, key, path, value)
            }
            Command::JsonGet { branch, key, path } => {
                let branch = branch.ok_or(Error::InvalidInput {
                    reason: "Branch must be specified or resolved to default".into(),
                })?;
                crate::handlers::json::json_get(&self.primitives, branch, key, path)
            }
            Command::JsonGetv { branch, key } => {
                let branch = branch.ok_or(Error::InvalidInput {
                    reason: "Branch must be specified or resolved to default".into(),
                })?;
                crate::handlers::json::json_getv(&self.primitives, branch, key)
            }
            Command::JsonDelete { branch, key, path } => {
                let branch = branch.ok_or(Error::InvalidInput {
                    reason: "Branch must be specified or resolved to default".into(),
                })?;
                crate::handlers::json::json_delete(&self.primitives, branch, key, path)
            }
            Command::JsonList {
                branch,
                prefix,
                cursor,
                limit,
            } => {
                let branch = branch.ok_or(Error::InvalidInput {
                    reason: "Branch must be specified or resolved to default".into(),
                })?;
                crate::handlers::json::json_list(&self.primitives, branch, prefix, cursor, limit)
            }

            // Event commands (4 MVP)
            Command::EventAppend {
                branch,
                event_type,
                payload,
            } => {
                let branch = branch.ok_or(Error::InvalidInput {
                    reason: "Branch must be specified or resolved to default".into(),
                })?;
                crate::handlers::event::event_append(&self.primitives, branch, event_type, payload)
            }
            Command::EventRead { branch, sequence } => {
                let branch = branch.ok_or(Error::InvalidInput {
                    reason: "Branch must be specified or resolved to default".into(),
                })?;
                crate::handlers::event::event_read(&self.primitives, branch, sequence)
            }
            Command::EventReadByType {
                branch,
                event_type,
                limit,
                after_sequence,
            } => {
                let branch = branch.ok_or(Error::InvalidInput {
                    reason: "Branch must be specified or resolved to default".into(),
                })?;
                crate::handlers::event::event_read_by_type(
                    &self.primitives,
                    branch,
                    event_type,
                    limit,
                    after_sequence,
                )
            }
            Command::EventLen { branch } => {
                let branch = branch.ok_or(Error::InvalidInput {
                    reason: "Branch must be specified or resolved to default".into(),
                })?;
                crate::handlers::event::event_len(&self.primitives, branch)
            }

            // State commands (4 MVP)
            Command::StateSet {
                branch,
                cell,
                value,
            } => {
                let branch = branch.ok_or(Error::InvalidInput {
                    reason: "Branch must be specified or resolved to default".into(),
                })?;
                crate::handlers::state::state_set(&self.primitives, branch, cell, value)
            }
            Command::StateRead { branch, cell } => {
                let branch = branch.ok_or(Error::InvalidInput {
                    reason: "Branch must be specified or resolved to default".into(),
                })?;
                crate::handlers::state::state_read(&self.primitives, branch, cell)
            }
            Command::StateReadv { branch, cell } => {
                let branch = branch.ok_or(Error::InvalidInput {
                    reason: "Branch must be specified or resolved to default".into(),
                })?;
                crate::handlers::state::state_readv(&self.primitives, branch, cell)
            }
            Command::StateCas {
                branch,
                cell,
                expected_counter,
                value,
            } => {
                let branch = branch.ok_or(Error::InvalidInput {
                    reason: "Branch must be specified or resolved to default".into(),
                })?;
                crate::handlers::state::state_cas(
                    &self.primitives,
                    branch,
                    cell,
                    expected_counter,
                    value,
                )
            }
            Command::StateInit {
                branch,
                cell,
                value,
            } => {
                let branch = branch.ok_or(Error::InvalidInput {
                    reason: "Branch must be specified or resolved to default".into(),
                })?;
                crate::handlers::state::state_init(&self.primitives, branch, cell, value)
            }
            Command::StateDelete { branch, cell } => {
                let branch = branch.ok_or(Error::InvalidInput {
                    reason: "Branch must be specified or resolved to default".into(),
                })?;
                crate::handlers::state::state_delete(&self.primitives, branch, cell)
            }
            Command::StateList { branch, prefix } => {
                let branch = branch.ok_or(Error::InvalidInput {
                    reason: "Branch must be specified or resolved to default".into(),
                })?;
                crate::handlers::state::state_list(&self.primitives, branch, prefix)
            }

            // Vector commands
            Command::VectorUpsert {
                branch,
                collection,
                key,
                vector,
                metadata,
            } => {
                let branch = branch.ok_or(Error::InvalidInput {
                    reason: "Branch must be specified or resolved to default".into(),
                })?;
                crate::handlers::vector::vector_upsert(
                    &self.primitives,
                    branch,
                    collection,
                    key,
                    vector,
                    metadata,
                )
            }
            Command::VectorGet {
                branch,
                collection,
                key,
            } => {
                let branch = branch.ok_or(Error::InvalidInput {
                    reason: "Branch must be specified or resolved to default".into(),
                })?;
                crate::handlers::vector::vector_get(&self.primitives, branch, collection, key)
            }
            Command::VectorDelete {
                branch,
                collection,
                key,
            } => {
                let branch = branch.ok_or(Error::InvalidInput {
                    reason: "Branch must be specified or resolved to default".into(),
                })?;
                crate::handlers::vector::vector_delete(&self.primitives, branch, collection, key)
            }
            Command::VectorSearch {
                branch,
                collection,
                query,
                k,
                filter,
                metric,
            } => {
                let branch = branch.ok_or(Error::InvalidInput {
                    reason: "Branch must be specified or resolved to default".into(),
                })?;
                crate::handlers::vector::vector_search(
                    &self.primitives,
                    branch,
                    collection,
                    query,
                    k,
                    filter,
                    metric,
                )
            }
            Command::VectorCreateCollection {
                branch,
                collection,
                dimension,
                metric,
            } => {
                let branch = branch.ok_or(Error::InvalidInput {
                    reason: "Branch must be specified or resolved to default".into(),
                })?;
                crate::handlers::vector::vector_create_collection(
                    &self.primitives,
                    branch,
                    collection,
                    dimension,
                    metric,
                )
            }
            Command::VectorDeleteCollection { branch, collection } => {
                let branch = branch.ok_or(Error::InvalidInput {
                    reason: "Branch must be specified or resolved to default".into(),
                })?;
                crate::handlers::vector::vector_delete_collection(
                    &self.primitives,
                    branch,
                    collection,
                )
            }
            Command::VectorListCollections { branch } => {
                let branch = branch.ok_or(Error::InvalidInput {
                    reason: "Branch must be specified or resolved to default".into(),
                })?;
                crate::handlers::vector::vector_list_collections(&self.primitives, branch)
            }
            Command::VectorCollectionStats { branch, collection } => {
                let branch = branch.ok_or(Error::InvalidInput {
                    reason: "Branch must be specified or resolved to default".into(),
                })?;
                crate::handlers::vector::vector_collection_stats(
                    &self.primitives,
                    branch,
                    collection,
                )
            }
            Command::VectorBatchUpsert {
                branch,
                collection,
                entries,
            } => {
                let branch = branch.ok_or(Error::InvalidInput {
                    reason: "Branch must be specified or resolved to default".into(),
                })?;
                crate::handlers::vector::vector_batch_upsert(
                    &self.primitives,
                    branch,
                    collection,
                    entries,
                )
            }

            // Branch commands (5 MVP)
            Command::BranchCreate {
                branch_id,
                metadata,
            } => crate::handlers::branch::branch_create(&self.primitives, branch_id, metadata),
            Command::BranchGet { branch } => {
                crate::handlers::branch::branch_get(&self.primitives, branch)
            }
            Command::BranchList {
                state,
                limit,
                offset,
            } => crate::handlers::branch::branch_list(&self.primitives, state, limit, offset),
            Command::BranchExists { branch } => {
                crate::handlers::branch::branch_exists(&self.primitives, branch)
            }
            Command::BranchDelete { branch } => {
                crate::handlers::branch::branch_delete(&self.primitives, branch)
            }

            // Transaction commands - handled by Session, not Executor
            Command::TxnBegin { .. }
            | Command::TxnCommit
            | Command::TxnRollback
            | Command::TxnInfo
            | Command::TxnIsActive => Err(Error::Internal {
                reason: "Transaction commands not yet implemented".to_string(),
            }),

            // Retention commands
            Command::RetentionApply { branch } => {
                let branch = branch.ok_or(Error::InvalidInput {
                    reason: "Branch must be specified or resolved to default".into(),
                })?;
                let branch_id = crate::bridge::to_core_branch_id(&branch)?;
                // Use the current version as the safe GC boundary:
                // all versions older than the current version are prunable
                // since they have been superseded by newer commits.
                let current = self.primitives.db.current_version();
                let _pruned = self.primitives.db.gc_versions_before(branch_id, current);
                Ok(Output::Unit)
            }
            Command::RetentionStats { .. } | Command::RetentionPreview { .. } => {
                Err(Error::Internal {
                    reason: "Retention commands not yet implemented".to_string(),
                })
            }

            // Bundle commands
            Command::BranchExport { branch_id, path } => {
                crate::handlers::branch::branch_export(&self.primitives, branch_id, path)
            }
            Command::BranchImport { path } => {
                crate::handlers::branch::branch_import(&self.primitives, path)
            }
            Command::BranchBundleValidate { path } => {
                crate::handlers::branch::branch_bundle_validate(path)
            }

            // Intelligence commands
            Command::Search {
                branch,
                query,
                k,
                primitives,
            } => {
                let branch = branch.ok_or(Error::InvalidInput {
                    reason: "Branch must be specified or resolved to default".into(),
                })?;
                crate::handlers::search::search(&self.primitives, branch, query, k, primitives)
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
