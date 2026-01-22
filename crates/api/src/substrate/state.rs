//! StateCell Substrate Operations
//!
//! StateCell provides compare-and-swap (CAS) cells for single-writer coordination.
//! Each cell has a value and a counter that increments on every write.
//!
//! ## Counter Model
//!
//! - Every StateCell has a counter starting at 0
//! - Every write increments the counter by 1
//! - CAS operations compare against the counter (version)
//!
//! ## Versioning
//!
//! StateCells use counter-based versioning (`Version::Counter`).
//! The counter represents the number of writes to the cell.
//!
//! ## Use Cases
//!
//! - Leader election
//! - Distributed locks
//! - Single-writer coordination

use super::types::ApiRunId;
use strata_core::{StrataResult, Value, Version, Versioned};

/// StateCell substrate operations
///
/// This trait defines the canonical state cell operations.
/// All operations require explicit run_id and return versioned results.
///
/// ## Contract
///
/// - Cells use counter-based versioning
/// - Every write increments the counter
/// - CAS is the primary write mechanism for coordination
///
/// ## Error Handling
///
/// | Condition | Error |
/// |-----------|-------|
/// | Invalid cell name | `InvalidKey` |
/// | Run not found | `NotFound` |
/// | Run is closed | `ConstraintViolation` |
/// | CAS failure | Returns `false`, not error |
pub trait StateCell {
    /// Set the cell value
    ///
    /// Unconditionally sets the value and increments the counter.
    /// Returns the new version (counter value).
    ///
    /// ## Semantics
    ///
    /// - Creates cell if it doesn't exist (counter starts at 1)
    /// - Always increments counter (even if value unchanged)
    /// - Use `cas` for coordinated updates
    ///
    /// ## Errors
    ///
    /// - `InvalidKey`: Cell name is invalid
    /// - `NotFound`: Run does not exist
    /// - `ConstraintViolation`: Run is closed
    fn state_set(&self, run: &ApiRunId, cell: &str, value: Value) -> StrataResult<Version>;

    /// Get the cell value
    ///
    /// Returns the current value and counter.
    ///
    /// ## Return Value
    ///
    /// - `Some(Versioned<Value>)`: Cell exists
    /// - `None`: Cell does not exist
    ///
    /// ## Errors
    ///
    /// - `InvalidKey`: Cell name is invalid
    /// - `NotFound`: Run does not exist
    fn state_get(&self, run: &ApiRunId, cell: &str) -> StrataResult<Option<Versioned<Value>>>;

    /// Compare-and-swap
    ///
    /// Sets the value only if the current counter matches `expected_counter`.
    ///
    /// ## Semantics
    ///
    /// - If `expected_counter` is `None`, succeeds only if cell doesn't exist
    /// - If `expected_counter` is `Some(n)`, succeeds only if counter == n
    /// - On success, increments counter and returns new version
    /// - On failure, returns `None` (no error)
    ///
    /// ## Return Value
    ///
    /// - `Some(Version)`: CAS succeeded, new version returned
    /// - `None`: CAS failed due to counter mismatch
    ///
    /// ## Errors
    ///
    /// - `InvalidKey`: Cell name is invalid
    /// - `NotFound`: Run does not exist
    /// - `ConstraintViolation`: Run is closed
    fn state_cas(
        &self,
        run: &ApiRunId,
        cell: &str,
        expected_counter: Option<u64>,
        value: Value,
    ) -> StrataResult<Option<Version>>;

    /// Delete the cell
    ///
    /// Removes the cell entirely.
    /// Returns `true` if the cell existed.
    ///
    /// ## Errors
    ///
    /// - `InvalidKey`: Cell name is invalid
    /// - `NotFound`: Run does not exist
    /// - `ConstraintViolation`: Run is closed
    fn state_delete(&self, run: &ApiRunId, cell: &str) -> StrataResult<bool>;

    /// Check if cell exists
    ///
    /// Returns `true` if the cell exists.
    ///
    /// ## Errors
    ///
    /// - `InvalidKey`: Cell name is invalid
    /// - `NotFound`: Run does not exist
    fn state_exists(&self, run: &ApiRunId, cell: &str) -> StrataResult<bool>;

    /// Get version history for a cell
    ///
    /// Returns historical versions, newest first.
    ///
    /// ## Parameters
    ///
    /// - `limit`: Maximum versions to return
    /// - `before`: Return versions older than this (exclusive)
    ///
    /// ## Errors
    ///
    /// - `InvalidKey`: Cell name is invalid
    /// - `NotFound`: Run does not exist
    fn state_history(
        &self,
        run: &ApiRunId,
        cell: &str,
        limit: Option<u64>,
        before: Option<Version>,
    ) -> StrataResult<Vec<Versioned<Value>>>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_trait_is_object_safe() {
        fn _assert_object_safe(_: &dyn StateCell) {}
    }
}
