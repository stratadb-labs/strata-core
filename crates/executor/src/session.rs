//! Stateful session for transaction support.
//!
//! The [`Session`] wraps an [`Executor`] and manages an optional open
//! transaction, providing read-your-writes semantics across multiple
//! commands within a transaction boundary.
//!
//! # Usage
//!
//! ```ignore
//! use strata_executor::Session;
//!
//! let mut session = Session::new(db.clone());
//!
//! // Begin a transaction
//! session.execute(Command::TxnBegin { branch: None, options: None })?;
//!
//! // Data commands route through the transaction
//! session.execute(Command::KvPut { branch: None, key: "k".into(), value: Value::Int(1) })?;
//! let out = session.execute(Command::KvGet { branch: None, key: "k".into() })?;
//!
//! // Commit
//! session.execute(Command::TxnCommit)?;
//! ```

use std::sync::Arc;

use strata_core::types::{Key, Namespace, TypeTag};
use strata_engine::{Database, Transaction, TransactionContext, TransactionOps};

use crate::bridge::{extract_version, json_to_value, parse_path, to_core_branch_id, to_versioned_value, value_to_json};
use crate::convert::convert_result;
use crate::types::BranchId;
use crate::{Command, Error, Executor, Output, Result};

/// A stateful session that wraps an [`Executor`] and manages an optional
/// open transaction with read-your-writes semantics.
///
/// When no transaction is active, commands delegate to the inner `Executor`.
/// When a transaction is active, data commands (KV, Event, State, JSON)
/// route through the engine's `Transaction<'a>` / `TransactionOps` trait,
/// while non-transactional commands (Branch, Vector, DB) still delegate to
/// the `Executor`.
pub struct Session {
    executor: Executor,
    db: Arc<Database>,
    txn_ctx: Option<TransactionContext>,
    txn_branch_id: Option<strata_core::types::BranchId>,
}

impl Session {
    /// Create a new session.
    pub fn new(db: Arc<Database>) -> Self {
        Self {
            executor: Executor::new(db.clone()),
            db,
            txn_ctx: None,
            txn_branch_id: None,
        }
    }

    /// Returns whether a transaction is currently active.
    pub fn in_transaction(&self) -> bool {
        self.txn_ctx.is_some()
    }

    /// Execute a command, routing through the active transaction when appropriate.
    pub fn execute(&mut self, mut cmd: Command) -> Result<Output> {
        cmd.resolve_default_branch();

        match &cmd {
            // Transaction lifecycle commands
            Command::TxnBegin { .. } => self.handle_begin(&cmd),
            Command::TxnCommit => self.handle_commit(),
            Command::TxnRollback => self.handle_abort(),
            Command::TxnInfo => self.handle_txn_info(),
            Command::TxnIsActive => Ok(Output::Bool(self.in_transaction())),

            // Non-transactional commands always go to executor
            Command::BranchCreate { .. }
            | Command::BranchGet { .. }
            | Command::BranchList { .. }
            | Command::BranchExists { .. }
            | Command::BranchDelete { .. }
            | Command::VectorUpsert { .. }
            | Command::VectorGet { .. }
            | Command::VectorDelete { .. }
            | Command::VectorSearch { .. }
            | Command::VectorCreateCollection { .. }
            | Command::VectorDeleteCollection { .. }
            | Command::VectorListCollections { .. }
            | Command::Ping
            | Command::Info
            | Command::Flush
            | Command::Compact
            | Command::RetentionApply { .. }
            | Command::RetentionStats { .. }
            | Command::RetentionPreview { .. }
            // Version history commands require storage-layer version chains
            // which are not available through the transaction context.
            // These read directly from the committed store.
            | Command::KvGetv { .. }
            | Command::StateReadv { .. }
            | Command::JsonGetv { .. }
            | Command::JsonList { .. }
            | Command::EventReadByType { .. } => self.executor.execute(cmd),

            // Data commands: route through txn if active, else delegate
            _ => {
                if self.txn_ctx.is_some() {
                    self.execute_in_txn(cmd)
                } else {
                    self.executor.execute(cmd)
                }
            }
        }
    }

    /// Get a reference to the underlying executor.
    pub fn executor(&self) -> &Executor {
        &self.executor
    }

    // =========================================================================
    // Transaction lifecycle handlers
    // =========================================================================

    fn handle_begin(&mut self, cmd: &Command) -> Result<Output> {
        if self.txn_ctx.is_some() {
            return Err(Error::TransactionAlreadyActive);
        }

        let branch = match cmd {
            Command::TxnBegin { branch, .. } => branch.clone().unwrap_or_else(BranchId::default),
            _ => unreachable!(),
        };

        let core_branch_id = to_core_branch_id(&branch)?;
        let ctx = self.db.begin_transaction(core_branch_id);
        self.txn_ctx = Some(ctx);
        self.txn_branch_id = Some(core_branch_id);

        Ok(Output::TxnBegun)
    }

    fn handle_commit(&mut self) -> Result<Output> {
        let mut ctx = self.txn_ctx.take().ok_or(Error::TransactionNotActive)?;
        self.txn_branch_id = None;

        match self.db.commit_transaction(&mut ctx) {
            Ok(version) => {
                self.db.end_transaction(ctx);
                Ok(Output::TxnCommitted { version })
            }
            Err(e) => {
                // Return context to pool even on failure
                self.db.end_transaction(ctx);
                Err(Error::TransactionConflict {
                    reason: e.to_string(),
                })
            }
        }
    }

    fn handle_abort(&mut self) -> Result<Output> {
        let ctx = self.txn_ctx.take().ok_or(Error::TransactionNotActive)?;
        self.txn_branch_id = None;
        self.db.end_transaction(ctx);
        Ok(Output::TxnAborted)
    }

    fn handle_txn_info(&self) -> Result<Output> {
        if let Some(ctx) = &self.txn_ctx {
            Ok(Output::TxnInfo(Some(crate::types::TransactionInfo {
                id: ctx.txn_id.to_string(),
                status: crate::types::TxnStatus::Active,
                started_at: 0,
            })))
        } else {
            Ok(Output::TxnInfo(None))
        }
    }

    // =========================================================================
    // In-transaction command execution
    // =========================================================================

    fn execute_in_txn(&mut self, cmd: Command) -> Result<Output> {
        let branch_id = self.txn_branch_id.expect("txn_branch_id set when txn_ctx is Some");
        let ns = Namespace::for_branch(branch_id);

        // Temporarily take the context to create a Transaction
        let mut ctx = self.txn_ctx.take().unwrap();
        let result = Self::dispatch_in_txn(&self.executor, &mut ctx, ns, cmd);
        self.txn_ctx = Some(ctx);

        result
    }

    fn dispatch_in_txn(
        executor: &Executor,
        ctx: &mut TransactionContext,
        ns: Namespace,
        cmd: Command,
    ) -> Result<Output> {
        // Read commands use ctx.get() / ctx.scan_prefix() directly so they
        // fall through to the snapshot when the key isn't in the write-set.
        // Write commands create a Transaction which handles event sequencing
        // and other write-specific logic.
        match cmd {
            // === KV reads — via ctx for snapshot fallback ===
            Command::KvGet { key, .. } => {
                let full_key = Key::new_kv(ns, &key);
                let result = ctx.get(&full_key).map_err(Error::from)?;
                Ok(Output::Maybe(result))
            }
            Command::KvList { prefix, .. } => {
                let prefix_key = match prefix {
                    Some(ref p) => Key::new_kv(ns.clone(), p),
                    None => Key::new(ns.clone(), TypeTag::KV, vec![]),
                };
                let entries = ctx.scan_prefix(&prefix_key).map_err(Error::from)?;
                let keys: Vec<String> = entries
                    .into_iter()
                    .filter_map(|(k, _)| k.user_key_string())
                    .collect();
                Ok(Output::Keys(keys))
            }

            // === State reads — via ctx for snapshot fallback ===
            Command::StateRead { cell, .. } => {
                let full_key = Key::new_state(ns, &cell);
                let result = ctx.get(&full_key).map_err(Error::from)?;
                match result {
                    Some(strata_core::value::Value::String(s)) => {
                        let state: strata_core::State =
                            serde_json::from_str(&s).map_err(|e| Error::Internal { reason: e.to_string() })?;
                        Ok(Output::Maybe(Some(state.value)))
                    }
                    Some(other) => Ok(Output::Maybe(Some(other))),
                    None => Ok(Output::Maybe(None)),
                }
            }

            // === JSON reads — via ctx for snapshot fallback ===
            Command::JsonGet { key, path, .. } => {
                let full_key = Key::new_json(ns.clone(), &key);
                if path == "$" || path.is_empty() {
                    let result = ctx.get(&full_key).map_err(Error::from)?;
                    match result {
                        Some(strata_core::value::Value::String(s)) => {
                            let jv: strata_core::JsonValue =
                                serde_json::from_str(&s).map_err(|e| Error::Internal { reason: e.to_string() })?;
                            let val = convert_result(json_to_value(jv))?;
                            Ok(Output::Maybe(Some(val)))
                        }
                        Some(other) => Ok(Output::Maybe(Some(other))),
                        None => Ok(Output::Maybe(None)),
                    }
                } else {
                    // Path-based get still needs Transaction for JSON patch logic
                    let txn = Transaction::new(ctx, ns);
                    let json_path = convert_result(parse_path(&path))?;
                    let result =
                        txn.json_get_path(&key, &json_path).map_err(Error::from)?;
                    match result {
                        Some(jv) => {
                            let val = convert_result(json_to_value(jv))?;
                            Ok(Output::Maybe(Some(val)))
                        }
                        None => Ok(Output::Maybe(None)),
                    }
                }
            }

            // === Write commands — use Transaction ===
            Command::KvPut { key, value, .. } => {
                let mut txn = Transaction::new(ctx, ns);
                let version = txn.kv_put(&key, value).map_err(Error::from)?;
                Ok(Output::Version(extract_version(&version)))
            }
            Command::KvDelete { key, .. } => {
                let full_key = Key::new_kv(ns, &key);
                let existed = ctx.exists(&full_key).map_err(Error::from)?;
                ctx.delete(full_key).map_err(Error::from)?;
                Ok(Output::Bool(existed))
            }

            // === Event operations — use Transaction for hash chaining ===
            Command::EventAppend {
                event_type, payload, ..
            } => {
                let mut txn = Transaction::new(ctx, ns);
                let version = txn.event_append(&event_type, payload).map_err(Error::from)?;
                Ok(Output::Version(extract_version(&version)))
            }
            Command::EventRead { sequence, .. } => {
                let txn = Transaction::new(ctx, ns);
                let result = txn.event_read(sequence).map_err(Error::from)?;
                Ok(Output::MaybeVersioned(result.map(|v| {
                    to_versioned_value(strata_core::Versioned::new(
                        v.value.payload.clone(),
                        v.version,
                    ))
                })))
            }
            Command::EventLen { .. } => {
                let txn = Transaction::new(ctx, ns);
                let len = txn.event_len().map_err(Error::from)?;
                Ok(Output::Uint(len))
            }

            // === State writes — use Transaction ===
            Command::StateInit { cell, value, .. } => {
                let mut txn = Transaction::new(ctx, ns);
                let version = txn.state_init(&cell, value).map_err(Error::from)?;
                Ok(Output::Version(extract_version(&version)))
            }
            Command::StateCas {
                cell,
                expected_counter,
                value,
                ..
            } => {
                let mut txn = Transaction::new(ctx, ns);
                let expected = match expected_counter {
                    Some(v) => strata_core::Version::Counter(v),
                    None => strata_core::Version::Counter(0),
                };
                let version = txn.state_cas(&cell, expected, value).map_err(Error::from)?;
                Ok(Output::MaybeVersion(Some(extract_version(&version))))
            }

            // === JSON writes — use Transaction ===
            Command::JsonSet {
                key, path, value, ..
            } => {
                let mut txn = Transaction::new(ctx, ns);
                let json_path = convert_result(parse_path(&path))?;
                let json_value = convert_result(value_to_json(value))?;
                let version =
                    txn.json_set(&key, &json_path, json_value).map_err(Error::from)?;
                Ok(Output::Version(extract_version(&version)))
            }
            Command::JsonDelete { key, .. } => {
                let mut txn = Transaction::new(ctx, ns);
                let deleted = txn.json_delete(&key).map_err(Error::from)?;
                Ok(Output::Uint(if deleted { 1 } else { 0 }))
            }

            // Commands not directly mapped to TransactionOps — delegate to executor.
            // This includes batch operations, history, CAS, scan, incr, etc.
            other => executor.execute(other),
        }
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        if let Some(ctx) = self.txn_ctx.take() {
            self.db.end_transaction(ctx);
        }
    }
}
