//! Run lifecycle operations.

use super::Strata;
use strata_core::Value;
use crate::{Command, Error, Output, Result};
use crate::types::*;

impl Strata {
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
