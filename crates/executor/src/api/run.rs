//! Run lifecycle operations (MVP).
//!
//! MVP run operations: create, get, list, exists, delete.
//! Advanced features (status transitions, tags, metadata, parent-child,
//! retention) are deferred to post-MVP.

use super::Strata;
use crate::{Command, Error, Output, Result, Value};
use crate::types::*;

impl Strata {
    // =========================================================================
    // Run Operations (5 MVP)
    // =========================================================================

    /// Create a new run.
    ///
    /// # Arguments
    /// - `run_id`: Optional user-provided name. If None, a UUID is generated.
    /// - `metadata`: Optional metadata (ignored in MVP).
    ///
    /// # Returns
    /// Tuple of (RunInfo, version).
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
    ///
    /// # Returns
    /// `Some(VersionedRunInfo)` if the run exists, `None` otherwise.
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

    /// List all runs.
    ///
    /// # Arguments
    /// - `state`: Optional status filter (ignored in MVP).
    /// - `limit`: Optional maximum number of runs to return.
    /// - `offset`: Optional offset (ignored in MVP).
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

    /// Delete a run and all its data (cascading delete).
    ///
    /// This deletes:
    /// - The run metadata
    /// - All run-scoped data (KV, Events, States, JSON, Vectors)
    ///
    /// USE WITH CAUTION - this is irreversible!
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
}
