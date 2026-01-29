//! Run management power API.
//!
//! Access via `db.runs()` for advanced run operations like fork and diff.
//!
//! # Example
//!
//! ```ignore
//! use strata_executor::Strata;
//!
//! let db = Strata::open("/path/to/data")?;
//!
//! // List all runs
//! for run in db.runs().list()? {
//!     println!("Run: {}", run);
//! }
//!
//! // Create a new run
//! db.runs().create("experiment-1")?;
//!
//! // Future: fork a run (copies all data)
//! // db.runs().fork("main", "experiment-2")?;
//! ```

use crate::{Command, Error, Executor, Output, Result};
use crate::types::RunId;

/// Handle for run management operations.
///
/// Obtained via [`Strata::runs()`]. Provides the "power API" for run
/// management including listing, creating, deleting, and (future) forking.
pub struct Runs<'a> {
    executor: &'a Executor,
}

impl<'a> Runs<'a> {
    pub(crate) fn new(executor: &'a Executor) -> Self {
        Self { executor }
    }

    /// List all run names.
    ///
    /// # Example
    ///
    /// ```ignore
    /// for run in db.runs().list()? {
    ///     println!("Run: {}", run);
    /// }
    /// ```
    pub fn list(&self) -> Result<Vec<String>> {
        match self.executor.execute(Command::RunList {
            state: None,
            limit: None,
            offset: None,
        })? {
            Output::RunInfoList(runs) => Ok(runs.into_iter().map(|r| r.info.id.0).collect()),
            _ => Err(Error::Internal {
                reason: "Unexpected output for RunList".into(),
            }),
        }
    }

    /// Check if a run exists.
    pub fn exists(&self, name: &str) -> Result<bool> {
        match self.executor.execute(Command::RunExists {
            run: RunId::from(name),
        })? {
            Output::Bool(exists) => Ok(exists),
            _ => Err(Error::Internal {
                reason: "Unexpected output for RunExists".into(),
            }),
        }
    }

    /// Create a new empty run.
    ///
    /// The run starts with no data. Use `fork()` (when available) to
    /// create a run with copied data.
    ///
    /// # Errors
    ///
    /// Returns an error if the run already exists.
    pub fn create(&self, name: &str) -> Result<()> {
        match self.executor.execute(Command::RunCreate {
            run_id: Some(name.to_string()),
            metadata: None,
        })? {
            Output::RunWithVersion { .. } => Ok(()),
            _ => Err(Error::Internal {
                reason: "Unexpected output for RunCreate".into(),
            }),
        }
    }

    /// Delete a run and all its data.
    ///
    /// **WARNING**: This is irreversible! All data in the run will be deleted.
    ///
    /// # Errors
    ///
    /// - Returns an error if trying to delete the "default" run
    /// - Returns an error if the run doesn't exist
    pub fn delete(&self, name: &str) -> Result<()> {
        if name == "default" {
            return Err(Error::ConstraintViolation {
                reason: "Cannot delete the default run".into(),
            });
        }

        match self.executor.execute(Command::RunDelete {
            run: RunId::from(name),
        })? {
            Output::Unit => Ok(()),
            _ => Err(Error::Internal {
                reason: "Unexpected output for RunDelete".into(),
            }),
        }
    }

    /// Fork the current run, creating a copy with all its data.
    ///
    /// **NOT YET IMPLEMENTED** - This is a stub for future functionality.
    ///
    /// When implemented, this will:
    /// 1. Create a new run with the destination name
    /// 2. Copy all data (KV, State, Events, JSON, Vectors) from current run to destination
    /// 3. Stay on the current run (use `set_run()` to switch after)
    ///
    /// # Arguments
    ///
    /// * `destination` - Name for the new forked run
    ///
    /// # Example (future)
    ///
    /// ```ignore
    /// // Fork current run to "experiment"
    /// db.runs().fork("experiment")?;
    ///
    /// // Switch to the fork
    /// db.set_run("experiment")?;
    /// // ... make changes without affecting original ...
    /// ```
    pub fn fork(&self, _destination: &str) -> Result<()> {
        Err(Error::NotImplemented {
            feature: "fork_run".into(),
            reason: "Run forking is planned for a future release. For now, create a new run and manually copy data.".into(),
        })
    }

    /// Compare two runs and return their differences.
    ///
    /// **NOT YET IMPLEMENTED** - This is a stub for future functionality.
    ///
    /// When implemented, this will compare all data between two runs and
    /// return a structured diff showing:
    /// - Keys that exist only in run1
    /// - Keys that exist only in run2
    /// - Keys that exist in both but have different values
    ///
    /// # Example (future)
    ///
    /// ```ignore
    /// let diff = db.runs().diff("main", "experiment")?;
    /// println!("Added: {:?}", diff.added);
    /// println!("Removed: {:?}", diff.removed);
    /// println!("Changed: {:?}", diff.changed);
    /// ```
    pub fn diff(&self, _run1: &str, _run2: &str) -> Result<RunDiff> {
        Err(Error::NotImplemented {
            feature: "diff_runs".into(),
            reason: "Run diffing is planned for a future release.".into(),
        })
    }
}

/// Result of comparing two runs (future).
///
/// This is a placeholder for the diff result structure.
#[derive(Debug, Clone, Default)]
pub struct RunDiff {
    /// Keys that exist only in the first run
    pub only_in_first: Vec<String>,
    /// Keys that exist only in the second run
    pub only_in_second: Vec<String>,
    /// Keys that exist in both but have different values
    pub different: Vec<String>,
}
