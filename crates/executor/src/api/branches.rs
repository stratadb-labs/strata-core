//! Branch management power API.
//!
//! Access via `db.branches()` for advanced branch operations like fork and diff.
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
//! // Future: fork a branch (copies all data)
//! // db.branches().fork("main", "experiment-2")?;
//! ```

use crate::{Command, Error, Executor, Output, Result};
use crate::types::BranchId;

/// Handle for branch management operations.
///
/// Obtained via [`Strata::branches()`]. Provides the "power API" for branch
/// management including listing, creating, deleting, and (future) forking.
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
            Output::BranchInfoList(runs) => Ok(runs.into_iter().map(|r| r.info.id.0).collect()),
            _ => Err(Error::Internal {
                reason: "Unexpected output for BranchList".into(),
            }),
        }
    }

    /// Check if a branch exists.
    pub fn exists(&self, name: &str) -> Result<bool> {
        match self.executor.execute(Command::BranchExists {
            run: BranchId::from(name),
        })? {
            Output::Bool(exists) => Ok(exists),
            _ => Err(Error::Internal {
                reason: "Unexpected output for BranchExists".into(),
            }),
        }
    }

    /// Create a new empty branch.
    ///
    /// The branch starts with no data. Use `fork()` (when available) to
    /// create a branch with copied data.
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
            run: BranchId::from(name),
        })? {
            Output::Unit => Ok(()),
            _ => Err(Error::Internal {
                reason: "Unexpected output for BranchDelete".into(),
            }),
        }
    }

    /// Fork the current branch, creating a copy with all its data.
    ///
    /// **NOT YET IMPLEMENTED** - This is a stub for future functionality.
    ///
    /// When implemented, this will:
    /// 1. Create a new branch with the destination name
    /// 2. Copy all data (KV, State, Events, JSON, Vectors) from current branch to destination
    /// 3. Stay on the current branch (use `set_branch()` to switch after)
    ///
    /// # Arguments
    ///
    /// * `destination` - Name for the new forked branch
    ///
    /// # Example (future)
    ///
    /// ```ignore
    /// // Fork current branch to "experiment"
    /// db.branches().fork("experiment")?;
    ///
    /// // Switch to the fork
    /// db.set_branch("experiment")?;
    /// // ... make changes without affecting original ...
    /// ```
    pub fn fork(&self, _destination: &str) -> Result<()> {
        Err(Error::NotImplemented {
            feature: "fork_branch".into(),
            reason: "Branch forking is planned for a future release. For now, create a new branch and manually copy data.".into(),
        })
    }

    /// Compare two branches and return their differences.
    ///
    /// **NOT YET IMPLEMENTED** - This is a stub for future functionality.
    ///
    /// When implemented, this will compare all data between two branches and
    /// return a structured diff showing:
    /// - Keys that exist only in branch1
    /// - Keys that exist only in branch2
    /// - Keys that exist in both but have different values
    ///
    /// # Example (future)
    ///
    /// ```ignore
    /// let diff = db.branches().diff("main", "experiment")?;
    /// println!("Added: {:?}", diff.added);
    /// println!("Removed: {:?}", diff.removed);
    /// println!("Changed: {:?}", diff.changed);
    /// ```
    pub fn diff(&self, _branch1: &str, _branch2: &str) -> Result<BranchDiff> {
        Err(Error::NotImplemented {
            feature: "diff_branches".into(),
            reason: "Branch diffing is planned for a future release.".into(),
        })
    }
}

/// Result of comparing two branches (future).
///
/// This is a placeholder for the diff result structure.
#[derive(Debug, Clone, Default)]
pub struct BranchDiff {
    /// Keys that exist only in the first branch
    pub only_in_first: Vec<String>,
    /// Keys that exist only in the second branch
    pub only_in_second: Vec<String>,
    /// Keys that exist in both but have different values
    pub different: Vec<String>,
}
