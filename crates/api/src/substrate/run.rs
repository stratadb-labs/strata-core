//! RunIndex Substrate Operations
//!
//! The RunIndex manages run lifecycle and metadata.
//! It provides operations for creating, listing, and closing runs.
//!
//! ## Run Model
//!
//! - Every entity belongs to exactly one run (Invariant 5)
//! - The "default" run always exists and cannot be closed
//! - Custom runs are created with UUIDs
//! - Closed runs are read-only
//!
//! ## Run Lifecycle
//!
//! ```text
//! [create] --> Active --> [close] --> Closed
//! ```
//!
//! ## Versioning
//!
//! Run info uses transaction-based versioning (`Version::Txn`).

use super::types::{ApiRunId, RetentionPolicy, RunInfo, RunState};
use strata_core::{StrataResult, Value, Version, Versioned};

/// RunIndex substrate operations
///
/// This trait defines the canonical run management operations.
///
/// ## Contract
///
/// - "default" run always exists
/// - "default" run cannot be closed
/// - Run IDs are unique (UUID or "default")
/// - Closed runs are read-only for data primitives
///
/// ## Error Handling
///
/// | Condition | Error |
/// |-----------|-------|
/// | Invalid run ID format | `InvalidKey` |
/// | Run already exists | `ConstraintViolation` |
/// | Run not found | `NotFound` |
/// | Cannot close default run | `ConstraintViolation` |
/// | Run already closed | `ConstraintViolation` |
pub trait RunIndex {
    /// Create a new run
    ///
    /// Creates a new run with optional metadata.
    /// Returns the run info and version.
    ///
    /// ## Parameters
    ///
    /// - `run_id`: Optional specific ID (if None, generates UUID)
    /// - `metadata`: Optional metadata (must be Object or Null)
    ///
    /// ## Return Value
    ///
    /// Returns `(run_info, version)`.
    ///
    /// ## Errors
    ///
    /// - `InvalidKey`: Run ID format is invalid
    /// - `ConstraintViolation`: Run already exists, or metadata not Object/Null
    fn run_create(
        &self,
        run_id: Option<&ApiRunId>,
        metadata: Option<Value>,
    ) -> StrataResult<(RunInfo, Version)>;

    /// Get run info
    ///
    /// Returns information about a run.
    ///
    /// ## Errors
    ///
    /// - `InvalidKey`: Run ID format is invalid
    /// - `NotFound`: Run does not exist
    fn run_get(&self, run: &ApiRunId) -> StrataResult<Option<Versioned<RunInfo>>>;

    /// List all runs
    ///
    /// Returns all runs matching the filters.
    ///
    /// ## Parameters
    ///
    /// - `state`: Filter by state (Active/Closed)
    /// - `limit`: Maximum runs to return
    /// - `offset`: Skip first N runs
    ///
    /// ## Return Value
    ///
    /// Vector of run info, ordered by creation time (newest first).
    fn run_list(
        &self,
        state: Option<RunState>,
        limit: Option<u64>,
        offset: Option<u64>,
    ) -> StrataResult<Vec<Versioned<RunInfo>>>;

    /// Close a run
    ///
    /// Marks a run as closed. Closed runs are read-only.
    /// Returns the new version.
    ///
    /// ## Semantics
    ///
    /// - Cannot close "default" run
    /// - Cannot close already-closed run
    /// - After closing, all write operations fail with `ConstraintViolation`
    ///
    /// ## Errors
    ///
    /// - `InvalidKey`: Run ID format is invalid
    /// - `NotFound`: Run does not exist
    /// - `ConstraintViolation`: Cannot close default run, or already closed
    fn run_close(&self, run: &ApiRunId) -> StrataResult<Version>;

    /// Update run metadata
    ///
    /// Updates the metadata for a run.
    /// Returns the new version.
    ///
    /// ## Errors
    ///
    /// - `InvalidKey`: Run ID format is invalid
    /// - `NotFound`: Run does not exist
    /// - `ConstraintViolation`: Run is closed, or metadata not Object/Null
    fn run_update_metadata(&self, run: &ApiRunId, metadata: Value) -> StrataResult<Version>;

    /// Check if a run exists
    ///
    /// Returns `true` if the run exists (regardless of state).
    ///
    /// ## Errors
    ///
    /// - `InvalidKey`: Run ID format is invalid
    fn run_exists(&self, run: &ApiRunId) -> StrataResult<bool>;

    /// Set retention policy for a run
    ///
    /// Configures the history retention policy for a run.
    /// Returns the new version.
    ///
    /// ## Semantics
    ///
    /// - Policy applies to all primitives in the run
    /// - Existing history beyond policy may be garbage collected
    ///
    /// ## Errors
    ///
    /// - `InvalidKey`: Run ID format is invalid
    /// - `NotFound`: Run does not exist
    fn run_set_retention(&self, run: &ApiRunId, policy: RetentionPolicy) -> StrataResult<Version>;

    /// Get retention policy for a run
    ///
    /// Returns the current retention policy.
    ///
    /// ## Errors
    ///
    /// - `InvalidKey`: Run ID format is invalid
    /// - `NotFound`: Run does not exist
    fn run_get_retention(&self, run: &ApiRunId) -> StrataResult<RetentionPolicy>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_trait_is_object_safe() {
        fn _assert_object_safe(_: &dyn RunIndex) {}
    }
}
