//! Branch lifecycle operations (MVP).
//!
//! MVP branch operations: create, get, list, exists, delete.
//! Advanced features (status transitions, tags, metadata, parent-child,
//! retention) are deferred to post-MVP.

use super::Strata;
use crate::{Command, Error, Output, Result, Value};
use crate::types::*;

impl Strata {
    // =========================================================================
    // Branch Operations (5 MVP)
    // =========================================================================

    /// Create a new branch.
    ///
    /// # Arguments
    /// - `branch_id`: Optional user-provided name. If None, a UUID is generated.
    /// - `metadata`: Optional metadata (ignored in MVP).
    ///
    /// # Returns
    /// Tuple of (BranchInfo, version).
    pub fn branch_create(
        &self,
        branch_id: Option<String>,
        metadata: Option<Value>,
    ) -> Result<(BranchInfo, u64)> {
        match self.executor.execute(Command::BranchCreate { branch_id, metadata })? {
            Output::BranchWithVersion { info, version } => Ok((info, version)),
            _ => Err(Error::Internal {
                reason: "Unexpected output for BranchCreate".into(),
            }),
        }
    }

    /// Get branch info.
    ///
    /// # Returns
    /// `Some(VersionedBranchInfo)` if the branch exists, `None` otherwise.
    pub fn branch_get(&self, run: &str) -> Result<Option<VersionedBranchInfo>> {
        match self.executor.execute(Command::BranchGet {
            run: BranchId::from(run),
        })? {
            Output::BranchInfoVersioned(info) => Ok(Some(info)),
            Output::Maybe(None) => Ok(None),
            _ => Err(Error::Internal {
                reason: "Unexpected output for BranchGet".into(),
            }),
        }
    }

    /// List all branches.
    ///
    /// # Arguments
    /// - `state`: Optional status filter (ignored in MVP).
    /// - `limit`: Optional maximum number of branches to return.
    /// - `offset`: Optional offset (ignored in MVP).
    pub fn branch_list(
        &self,
        state: Option<BranchStatus>,
        limit: Option<u64>,
        offset: Option<u64>,
    ) -> Result<Vec<VersionedBranchInfo>> {
        match self.executor.execute(Command::BranchList {
            state,
            limit,
            offset,
        })? {
            Output::BranchInfoList(runs) => Ok(runs),
            _ => Err(Error::Internal {
                reason: "Unexpected output for BranchList".into(),
            }),
        }
    }

    /// Check if a branch exists.
    pub fn branch_exists(&self, run: &str) -> Result<bool> {
        match self.executor.execute(Command::BranchExists {
            run: BranchId::from(run),
        })? {
            Output::Bool(exists) => Ok(exists),
            _ => Err(Error::Internal {
                reason: "Unexpected output for BranchExists".into(),
            }),
        }
    }

    /// Delete a branch and all its data (cascading delete).
    ///
    /// This deletes:
    /// - The branch metadata
    /// - All branch-scoped data (KV, Events, States, JSON, Vectors)
    ///
    /// USE WITH CAUTION - this is irreversible!
    pub fn branch_delete(&self, run: &str) -> Result<()> {
        match self.executor.execute(Command::BranchDelete {
            run: BranchId::from(run),
        })? {
            Output::Unit => Ok(()),
            _ => Err(Error::Internal {
                reason: "Unexpected output for BranchDelete".into(),
            }),
        }
    }
}
