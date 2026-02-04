//! Branch management power API.
//!
//! Access via `db.branches()` for advanced branch operations including
//! fork, diff, and merge.
//!
//! # Example
//!
//! ```ignore
//! use strata_executor::Strata;
//!
//! let db = Strata::open("/path/to/data")?;
//!
//! // List all branches
//! for branch in db.branches().list()? {
//!     println!("Branch: {}", branch);
//! }
//!
//! // Create a new branch
//! db.branches().create("experiment-1")?;
//!
//! // Fork a branch (copies all data)
//! db.branches().fork("main", "experiment-2")?;
//!
//! // Diff two branches
//! let diff = db.branches().diff("main", "experiment-2")?;
//!
//! // Merge branches
//! use strata_engine::MergeStrategy;
//! db.branches().merge("experiment-2", "main", MergeStrategy::LastWriterWins)?;
//! ```

use crate::types::BranchId;
use crate::{Command, Error, Executor, Output, Result};
use strata_engine::branch_ops::{BranchDiffResult, ForkInfo, MergeInfo, MergeStrategy};

/// Handle for branch management operations.
///
/// Obtained via [`Strata::branches()`]. Provides the "power API" for branch
/// management including listing, creating, deleting, forking, diffing, and merging.
pub struct Branches<'a> {
    executor: &'a Executor,
}

impl<'a> Branches<'a> {
    pub(crate) fn new(executor: &'a Executor) -> Self {
        Self { executor }
    }

    /// List all branch names.
    ///
    /// # Example
    ///
    /// ```ignore
    /// for branch in db.branches().list()? {
    ///     println!("Branch: {}", branch);
    /// }
    /// ```
    pub fn list(&self) -> Result<Vec<String>> {
        match self.executor.execute(Command::BranchList {
            state: None,
            limit: None,
            offset: None,
        })? {
            Output::BranchInfoList(branches) => {
                Ok(branches.into_iter().map(|r| r.info.id.0).collect())
            }
            _ => Err(Error::Internal {
                reason: "Unexpected output for BranchList".into(),
            }),
        }
    }

    /// Check if a branch exists.
    pub fn exists(&self, name: &str) -> Result<bool> {
        match self.executor.execute(Command::BranchExists {
            branch: BranchId::from(name),
        })? {
            Output::Bool(exists) => Ok(exists),
            _ => Err(Error::Internal {
                reason: "Unexpected output for BranchExists".into(),
            }),
        }
    }

    /// Create a new empty branch.
    ///
    /// The branch starts with no data. Use `fork()` to create a branch
    /// with copied data.
    ///
    /// # Errors
    ///
    /// Returns an error if the branch already exists.
    pub fn create(&self, name: &str) -> Result<()> {
        match self.executor.execute(Command::BranchCreate {
            branch_id: Some(name.to_string()),
            metadata: None,
        })? {
            Output::BranchWithVersion { .. } => Ok(()),
            _ => Err(Error::Internal {
                reason: "Unexpected output for BranchCreate".into(),
            }),
        }
    }

    /// Delete a branch and all its data.
    ///
    /// **WARNING**: This is irreversible! All data in the branch will be deleted.
    ///
    /// # Errors
    ///
    /// - Returns an error if trying to delete the "default" branch
    /// - Returns an error if the branch doesn't exist
    pub fn delete(&self, name: &str) -> Result<()> {
        if name == "default" {
            return Err(Error::ConstraintViolation {
                reason: "Cannot delete the default branch".into(),
            });
        }

        match self.executor.execute(Command::BranchDelete {
            branch: BranchId::from(name),
        })? {
            Output::Unit => Ok(()),
            _ => Err(Error::Internal {
                reason: "Unexpected output for BranchDelete".into(),
            }),
        }
    }

    /// Fork a branch, creating a copy with all its data.
    ///
    /// Creates a new branch named `destination` containing a complete copy
    /// of all data (KV, State, Events, JSON, Vectors) from `source`.
    ///
    /// # Arguments
    ///
    /// * `source` - Name of the branch to copy from
    /// * `destination` - Name for the new forked branch
    ///
    /// # Errors
    ///
    /// - Source branch does not exist
    /// - Destination branch already exists
    ///
    /// # Example
    ///
    /// ```ignore
    /// db.branches().fork("main", "experiment")?;
    /// ```
    pub fn fork(&self, source: &str, destination: &str) -> Result<ForkInfo> {
        let db = &self.executor.primitives().db;
        strata_engine::branch_ops::fork_branch(db, source, destination).map_err(|e| {
            Error::Internal {
                reason: e.to_string(),
            }
        })
    }

    /// Compare two branches and return their differences.
    ///
    /// Returns a structured diff showing per-space added, removed, and
    /// modified entries between the two branches.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let diff = db.branches().diff("main", "experiment")?;
    /// println!("Added: {}", diff.summary.total_added);
    /// println!("Removed: {}", diff.summary.total_removed);
    /// println!("Modified: {}", diff.summary.total_modified);
    /// ```
    pub fn diff(&self, branch_a: &str, branch_b: &str) -> Result<BranchDiffResult> {
        let db = &self.executor.primitives().db;
        strata_engine::branch_ops::diff_branches(db, branch_a, branch_b).map_err(|e| {
            Error::Internal {
                reason: e.to_string(),
            }
        })
    }

    /// Merge data from source branch into target branch.
    ///
    /// Applies changes from `source` into `target`:
    /// - Added entries (in source but not target) are written to target
    /// - Modified entries depend on strategy:
    ///   - `LastWriterWins`: source value overwrites target
    ///   - `Strict`: merge fails if any conflicts exist
    /// - Removed entries (in target but not source) are left unchanged
    ///
    /// # Example
    ///
    /// ```ignore
    /// use strata_engine::MergeStrategy;
    ///
    /// // Merge with last-writer-wins conflict resolution
    /// let info = db.branches().merge("feature", "main", MergeStrategy::LastWriterWins)?;
    /// println!("Applied {} keys", info.keys_applied);
    /// ```
    pub fn merge(&self, source: &str, target: &str, strategy: MergeStrategy) -> Result<MergeInfo> {
        let db = &self.executor.primitives().db;
        strata_engine::branch_ops::merge_branches(db, source, target, strategy).map_err(|e| {
            Error::Internal {
                reason: e.to_string(),
            }
        })
    }
}
