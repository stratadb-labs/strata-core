//! The Executor - single entry point to Strata's engine.
//!
//! The Executor is a stateless dispatcher that routes commands to the
//! appropriate primitive operations and converts results to outputs.

use std::sync::Arc;
use std::time::Instant;

use strata_engine::Database;
use strata_security::AccessMode;
use tracing::{debug, warn};

use crate::bridge::{to_core_branch_id, Primitives};
use crate::convert::convert_result;
use crate::types::BranchId;
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
/// ```text
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

    /// Auto-register a space on first write to a non-default space.
    ///
    /// This is idempotent: calling it on an already-registered space just
    /// performs a single `txn.get()` check. The "default" space is skipped
    /// since it always exists implicitly.
    fn ensure_space_registered(&self, branch: &BranchId, space: &str) -> Result<()> {
        if space == "default" {
            return Ok(());
        }
        let core_branch = to_core_branch_id(branch)?;
        convert_result(self.primitives.space.register(core_branch, space))?;
        Ok(())
    }

    /// Execute a single command.
    ///
    /// Resolves any `None` branch fields to the default branch before dispatch.
    /// Returns the command result or an error.
    pub fn execute(&self, mut cmd: Command) -> Result<Output> {
        if self.access_mode == AccessMode::ReadOnly && cmd.is_write() {
            warn!(target: "strata::command", command = %cmd.name(), "Write rejected in read-only mode");
            return Err(Error::AccessDenied {
                command: cmd.name().to_string(),
            });
        }

        cmd.resolve_defaults();

        let cmd_name = cmd.name();
        let start = Instant::now();

        let result = match cmd {
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
                crate::handlers::embed_hook::flush_embed_buffer(&self.primitives);
                convert_result(self.primitives.db.flush())?;
                Ok(Output::Unit)
            }
            Command::Compact => {
                convert_result(self.primitives.db.compact())?;
                Ok(Output::Unit)
            }
            Command::TimeRange { branch } => {
                let branch = branch.ok_or(Error::InvalidInput {
                    reason: "Branch must be specified or resolved to default".into(),
                })?;
                crate::handlers::vector::time_range(&self.primitives, branch)
            }

            // KV commands (MVP: 4 commands)
            Command::KvPut {
                branch,
                space,
                key,
                value,
            } => {
                let branch = branch.ok_or(Error::InvalidInput {
                    reason: "Branch must be specified or resolved to default".into(),
                })?;
                let space = space.unwrap_or_else(|| "default".to_string());
                self.ensure_space_registered(&branch, &space)?;
                crate::handlers::kv::kv_put(&self.primitives, branch, space, key, value)
            }
            Command::KvGet {
                branch,
                space,
                key,
                as_of,
            } => {
                let branch = branch.ok_or(Error::InvalidInput {
                    reason: "Branch must be specified or resolved to default".into(),
                })?;
                let space = space.unwrap_or_else(|| "default".to_string());
                if let Some(ts) = as_of {
                    crate::handlers::kv::kv_get_at(&self.primitives, branch, space, key, ts)
                } else {
                    crate::handlers::kv::kv_get(&self.primitives, branch, space, key)
                }
            }
            Command::KvDelete { branch, space, key } => {
                let branch = branch.ok_or(Error::InvalidInput {
                    reason: "Branch must be specified or resolved to default".into(),
                })?;
                let space = space.unwrap_or_else(|| "default".to_string());
                self.ensure_space_registered(&branch, &space)?;
                crate::handlers::kv::kv_delete(&self.primitives, branch, space, key)
            }
            Command::KvList {
                branch,
                space,
                prefix,
                cursor,
                limit,
                as_of,
            } => {
                let branch = branch.ok_or(Error::InvalidInput {
                    reason: "Branch must be specified or resolved to default".into(),
                })?;
                let space = space.unwrap_or_else(|| "default".to_string());
                if let Some(ts) = as_of {
                    crate::handlers::kv::kv_list_at(&self.primitives, branch, space, prefix, ts)
                } else {
                    crate::handlers::kv::kv_list(
                        &self.primitives,
                        branch,
                        space,
                        prefix,
                        cursor,
                        limit,
                    )
                }
            }
            // Note: as_of is intentionally ignored for getv — version history
            // always returns all versions, not a point-in-time snapshot.
            Command::KvGetv {
                branch,
                space,
                key,
                as_of: _,
            } => {
                let branch = branch.ok_or(Error::InvalidInput {
                    reason: "Branch must be specified or resolved to default".into(),
                })?;
                let space = space.unwrap_or_else(|| "default".to_string());
                crate::handlers::kv::kv_getv(&self.primitives, branch, space, key)
            }

            // JSON commands
            Command::JsonSet {
                branch,
                space,
                key,
                path,
                value,
            } => {
                let branch = branch.ok_or(Error::InvalidInput {
                    reason: "Branch must be specified or resolved to default".into(),
                })?;
                let space = space.unwrap_or_else(|| "default".to_string());
                self.ensure_space_registered(&branch, &space)?;
                crate::handlers::json::json_set(&self.primitives, branch, space, key, path, value)
            }
            Command::JsonGet {
                branch,
                space,
                key,
                path,
                as_of,
            } => {
                let branch = branch.ok_or(Error::InvalidInput {
                    reason: "Branch must be specified or resolved to default".into(),
                })?;
                let space = space.unwrap_or_else(|| "default".to_string());
                if let Some(ts) = as_of {
                    crate::handlers::json::json_get_at(
                        &self.primitives,
                        branch,
                        space,
                        key,
                        path,
                        ts,
                    )
                } else {
                    crate::handlers::json::json_get(&self.primitives, branch, space, key, path)
                }
            }
            // Note: as_of is intentionally ignored for getv — version history
            // always returns all versions, not a point-in-time snapshot.
            Command::JsonGetv {
                branch,
                space,
                key,
                as_of: _,
            } => {
                let branch = branch.ok_or(Error::InvalidInput {
                    reason: "Branch must be specified or resolved to default".into(),
                })?;
                let space = space.unwrap_or_else(|| "default".to_string());
                crate::handlers::json::json_getv(&self.primitives, branch, space, key)
            }
            Command::JsonDelete {
                branch,
                space,
                key,
                path,
            } => {
                let branch = branch.ok_or(Error::InvalidInput {
                    reason: "Branch must be specified or resolved to default".into(),
                })?;
                let space = space.unwrap_or_else(|| "default".to_string());
                self.ensure_space_registered(&branch, &space)?;
                crate::handlers::json::json_delete(&self.primitives, branch, space, key, path)
            }
            Command::JsonList {
                branch,
                space,
                prefix,
                cursor,
                limit,
                as_of,
            } => {
                let branch = branch.ok_or(Error::InvalidInput {
                    reason: "Branch must be specified or resolved to default".into(),
                })?;
                let space = space.unwrap_or_else(|| "default".to_string());
                if let Some(ts) = as_of {
                    crate::handlers::json::json_list_at(&self.primitives, branch, space, prefix, ts)
                } else {
                    crate::handlers::json::json_list(
                        &self.primitives,
                        branch,
                        space,
                        prefix,
                        cursor,
                        limit,
                    )
                }
            }

            // Event commands (4 MVP)
            Command::EventAppend {
                branch,
                space,
                event_type,
                payload,
            } => {
                let branch = branch.ok_or(Error::InvalidInput {
                    reason: "Branch must be specified or resolved to default".into(),
                })?;
                let space = space.unwrap_or_else(|| "default".to_string());
                self.ensure_space_registered(&branch, &space)?;
                crate::handlers::event::event_append(
                    &self.primitives,
                    branch,
                    space,
                    event_type,
                    payload,
                )
            }
            Command::EventGet {
                branch,
                space,
                sequence,
                as_of,
            } => {
                let branch = branch.ok_or(Error::InvalidInput {
                    reason: "Branch must be specified or resolved to default".into(),
                })?;
                let space = space.unwrap_or_else(|| "default".to_string());
                if let Some(ts) = as_of {
                    crate::handlers::event::event_get_at(
                        &self.primitives,
                        branch,
                        space,
                        sequence,
                        ts,
                    )
                } else {
                    crate::handlers::event::event_get(&self.primitives, branch, space, sequence)
                }
            }
            Command::EventGetByType {
                branch,
                space,
                event_type,
                limit,
                after_sequence,
                as_of,
            } => {
                let branch = branch.ok_or(Error::InvalidInput {
                    reason: "Branch must be specified or resolved to default".into(),
                })?;
                let space = space.unwrap_or_else(|| "default".to_string());
                if let Some(ts) = as_of {
                    crate::handlers::event::event_get_by_type_at(
                        &self.primitives,
                        branch,
                        space,
                        event_type,
                        ts,
                    )
                } else {
                    crate::handlers::event::event_get_by_type(
                        &self.primitives,
                        branch,
                        space,
                        event_type,
                        limit,
                        after_sequence,
                    )
                }
            }
            Command::EventLen { branch, space } => {
                let branch = branch.ok_or(Error::InvalidInput {
                    reason: "Branch must be specified or resolved to default".into(),
                })?;
                let space = space.unwrap_or_else(|| "default".to_string());
                crate::handlers::event::event_len(&self.primitives, branch, space)
            }

            // State commands (4 MVP)
            Command::StateSet {
                branch,
                space,
                cell,
                value,
            } => {
                let branch = branch.ok_or(Error::InvalidInput {
                    reason: "Branch must be specified or resolved to default".into(),
                })?;
                let space = space.unwrap_or_else(|| "default".to_string());
                self.ensure_space_registered(&branch, &space)?;
                crate::handlers::state::state_set(&self.primitives, branch, space, cell, value)
            }
            Command::StateGet {
                branch,
                space,
                cell,
                as_of,
            } => {
                let branch = branch.ok_or(Error::InvalidInput {
                    reason: "Branch must be specified or resolved to default".into(),
                })?;
                let space = space.unwrap_or_else(|| "default".to_string());
                if let Some(ts) = as_of {
                    crate::handlers::state::state_get_at(&self.primitives, branch, space, cell, ts)
                } else {
                    crate::handlers::state::state_get(&self.primitives, branch, space, cell)
                }
            }
            // Note: as_of is intentionally ignored for getv — version history
            // always returns all versions, not a point-in-time snapshot.
            Command::StateGetv {
                branch,
                space,
                cell,
                as_of: _,
            } => {
                let branch = branch.ok_or(Error::InvalidInput {
                    reason: "Branch must be specified or resolved to default".into(),
                })?;
                let space = space.unwrap_or_else(|| "default".to_string());
                crate::handlers::state::state_getv(&self.primitives, branch, space, cell)
            }
            Command::StateCas {
                branch,
                space,
                cell,
                expected_counter,
                value,
            } => {
                let branch = branch.ok_or(Error::InvalidInput {
                    reason: "Branch must be specified or resolved to default".into(),
                })?;
                let space = space.unwrap_or_else(|| "default".to_string());
                self.ensure_space_registered(&branch, &space)?;
                crate::handlers::state::state_cas(
                    &self.primitives,
                    branch,
                    space,
                    cell,
                    expected_counter,
                    value,
                )
            }
            Command::StateInit {
                branch,
                space,
                cell,
                value,
            } => {
                let branch = branch.ok_or(Error::InvalidInput {
                    reason: "Branch must be specified or resolved to default".into(),
                })?;
                let space = space.unwrap_or_else(|| "default".to_string());
                self.ensure_space_registered(&branch, &space)?;
                crate::handlers::state::state_init(&self.primitives, branch, space, cell, value)
            }
            Command::StateDelete {
                branch,
                space,
                cell,
            } => {
                let branch = branch.ok_or(Error::InvalidInput {
                    reason: "Branch must be specified or resolved to default".into(),
                })?;
                let space = space.unwrap_or_else(|| "default".to_string());
                self.ensure_space_registered(&branch, &space)?;
                crate::handlers::state::state_delete(&self.primitives, branch, space, cell)
            }
            Command::StateList {
                branch,
                space,
                prefix,
                as_of,
            } => {
                let branch = branch.ok_or(Error::InvalidInput {
                    reason: "Branch must be specified or resolved to default".into(),
                })?;
                let space = space.unwrap_or_else(|| "default".to_string());
                if let Some(ts) = as_of {
                    crate::handlers::state::state_list_at(
                        &self.primitives,
                        branch,
                        space,
                        prefix,
                        ts,
                    )
                } else {
                    crate::handlers::state::state_list(&self.primitives, branch, space, prefix)
                }
            }

            // Vector commands
            Command::VectorUpsert {
                branch,
                space,
                collection,
                key,
                vector,
                metadata,
            } => {
                let branch = branch.ok_or(Error::InvalidInput {
                    reason: "Branch must be specified or resolved to default".into(),
                })?;
                let space = space.unwrap_or_else(|| "default".to_string());
                self.ensure_space_registered(&branch, &space)?;
                crate::handlers::vector::vector_upsert(
                    &self.primitives,
                    branch,
                    space,
                    collection,
                    key,
                    vector,
                    metadata,
                )
            }
            Command::VectorGet {
                branch,
                space,
                collection,
                key,
                as_of,
            } => {
                let branch = branch.ok_or(Error::InvalidInput {
                    reason: "Branch must be specified or resolved to default".into(),
                })?;
                let space = space.unwrap_or_else(|| "default".to_string());
                if let Some(ts) = as_of {
                    crate::handlers::vector::vector_get_at(
                        &self.primitives,
                        branch,
                        space,
                        collection,
                        key,
                        ts,
                    )
                } else {
                    crate::handlers::vector::vector_get(
                        &self.primitives,
                        branch,
                        space,
                        collection,
                        key,
                    )
                }
            }
            Command::VectorDelete {
                branch,
                space,
                collection,
                key,
            } => {
                let branch = branch.ok_or(Error::InvalidInput {
                    reason: "Branch must be specified or resolved to default".into(),
                })?;
                let space = space.unwrap_or_else(|| "default".to_string());
                self.ensure_space_registered(&branch, &space)?;
                crate::handlers::vector::vector_delete(
                    &self.primitives,
                    branch,
                    space,
                    collection,
                    key,
                )
            }
            Command::VectorSearch {
                branch,
                space,
                collection,
                query,
                k,
                filter,
                metric,
                as_of,
            } => {
                let branch = branch.ok_or(Error::InvalidInput {
                    reason: "Branch must be specified or resolved to default".into(),
                })?;
                let space = space.unwrap_or_else(|| "default".to_string());
                if let Some(ts) = as_of {
                    crate::handlers::vector::vector_search_at(
                        &self.primitives,
                        branch,
                        space,
                        collection,
                        query,
                        k,
                        filter,
                        metric,
                        ts,
                    )
                } else {
                    crate::handlers::vector::vector_search(
                        &self.primitives,
                        branch,
                        space,
                        collection,
                        query,
                        k,
                        filter,
                        metric,
                    )
                }
            }
            Command::VectorCreateCollection {
                branch,
                space,
                collection,
                dimension,
                metric,
            } => {
                let branch = branch.ok_or(Error::InvalidInput {
                    reason: "Branch must be specified or resolved to default".into(),
                })?;
                let space = space.unwrap_or_else(|| "default".to_string());
                self.ensure_space_registered(&branch, &space)?;
                crate::handlers::vector::vector_create_collection(
                    &self.primitives,
                    branch,
                    space,
                    collection,
                    dimension,
                    metric,
                )
            }
            Command::VectorDeleteCollection {
                branch,
                space,
                collection,
            } => {
                let branch = branch.ok_or(Error::InvalidInput {
                    reason: "Branch must be specified or resolved to default".into(),
                })?;
                let space = space.unwrap_or_else(|| "default".to_string());
                self.ensure_space_registered(&branch, &space)?;
                crate::handlers::vector::vector_delete_collection(
                    &self.primitives,
                    branch,
                    space,
                    collection,
                )
            }
            Command::VectorListCollections { branch, space } => {
                let branch = branch.ok_or(Error::InvalidInput {
                    reason: "Branch must be specified or resolved to default".into(),
                })?;
                let space = space.unwrap_or_else(|| "default".to_string());
                crate::handlers::vector::vector_list_collections(&self.primitives, branch, space)
            }
            Command::VectorCollectionStats {
                branch,
                space,
                collection,
            } => {
                let branch = branch.ok_or(Error::InvalidInput {
                    reason: "Branch must be specified or resolved to default".into(),
                })?;
                let space = space.unwrap_or_else(|| "default".to_string());
                crate::handlers::vector::vector_collection_stats(
                    &self.primitives,
                    branch,
                    space,
                    collection,
                )
            }
            Command::VectorBatchUpsert {
                branch,
                space,
                collection,
                entries,
            } => {
                let branch = branch.ok_or(Error::InvalidInput {
                    reason: "Branch must be specified or resolved to default".into(),
                })?;
                let space = space.unwrap_or_else(|| "default".to_string());
                self.ensure_space_registered(&branch, &space)?;
                crate::handlers::vector::vector_batch_upsert(
                    &self.primitives,
                    branch,
                    space,
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
            Command::ConfigureModel {
                endpoint,
                model,
                api_key,
                timeout_ms,
            } => crate::handlers::configure_model::configure_model(
                &self.primitives,
                endpoint,
                model,
                api_key,
                timeout_ms,
            ),
            Command::Search {
                branch,
                space,
                search,
            } => {
                let branch = branch.ok_or(Error::InvalidInput {
                    reason: "Branch must be specified or resolved to default".into(),
                })?;
                let space = space.unwrap_or_else(|| "default".to_string());
                crate::handlers::search::search(&self.primitives, branch, space, search)
            }

            // Space commands
            Command::SpaceList { branch } => {
                let branch = branch.ok_or(Error::InvalidInput {
                    reason: "Branch must be specified or resolved to default".into(),
                })?;
                crate::handlers::space::space_list(&self.primitives, branch)
            }
            Command::SpaceCreate { branch, space } => {
                let branch = branch.ok_or(Error::InvalidInput {
                    reason: "Branch must be specified or resolved to default".into(),
                })?;
                crate::handlers::space::space_create(&self.primitives, branch, space)
            }
            Command::SpaceDelete {
                branch,
                space,
                force,
            } => {
                let branch = branch.ok_or(Error::InvalidInput {
                    reason: "Branch must be specified or resolved to default".into(),
                })?;
                crate::handlers::space::space_delete(&self.primitives, branch, space, force)
            }
            Command::SpaceExists { branch, space } => {
                let branch = branch.ok_or(Error::InvalidInput {
                    reason: "Branch must be specified or resolved to default".into(),
                })?;
                crate::handlers::space::space_exists(&self.primitives, branch, space)
            }
        };

        match &result {
            Ok(_) => {
                debug!(target: "strata::command", command = %cmd_name, duration_us = start.elapsed().as_micros() as u64, "Command executed");
            }
            Err(e) => {
                warn!(target: "strata::command", command = %cmd_name, duration_us = start.elapsed().as_micros() as u64, error = %e, "Command failed");
            }
        }

        result
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

impl Drop for Executor {
    fn drop(&mut self) {
        // Drain any pending embeddings so they aren't silently lost when the
        // executor is dropped without an explicit flush.
        crate::handlers::embed_hook::flush_embed_buffer(&self.primitives);
    }
}

// Static assertion: Executor must remain Send+Sync.
// If a future refactor adds a non-Send/Sync field, this will fail at compile time.
const _: () = {
    fn _assert_send<T: Send>() {}
    fn _assert_sync<T: Sync>() {}
    fn _check() {
        _assert_send::<Executor>();
        _assert_sync::<Executor>();
    }
};
