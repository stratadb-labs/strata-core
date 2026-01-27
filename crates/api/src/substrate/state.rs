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

    /// Initialize a cell (only if it doesn't exist)
    ///
    /// Creates a new cell with the given value. Fails if the cell already exists.
    /// Returns the new version (always 1 for new cells).
    ///
    /// ## Errors
    ///
    /// - `InvalidKey`: Cell name is invalid
    /// - `ConstraintViolation`: Cell already exists, or run is closed
    /// - `NotFound`: Run does not exist
    fn state_init(&self, run: &ApiRunId, cell: &str, value: Value) -> StrataResult<Version>;

    /// List all cell names in a run
    ///
    /// Returns all cell names that exist in the run.
    ///
    /// ## Errors
    ///
    /// - `NotFound`: Run does not exist
    fn state_list(&self, run: &ApiRunId) -> StrataResult<Vec<String>>;

    /// Apply a transition function with automatic retry
    ///
    /// Reads the current value, applies the transition closure, and writes the result.
    /// Automatically retries on conflict (optimistic concurrency).
    ///
    /// ## Purity Requirement
    ///
    /// The closure MAY BE CALLED MULTIPLE TIMES due to OCC retries.
    /// It MUST be a pure function:
    /// - No I/O (file, network, console)
    /// - No external mutation
    /// - No irreversible effects
    /// - Idempotent (same input -> same output)
    ///
    /// ## Return Value
    ///
    /// Returns `(new_value, new_version)` on success.
    ///
    /// ## Errors
    ///
    /// - `InvalidKey`: Cell name is invalid
    /// - `NotFound`: Run or cell does not exist
    /// - `ConstraintViolation`: Run is closed
    fn state_transition<F>(
        &self,
        run: &ApiRunId,
        cell: &str,
        f: F,
    ) -> StrataResult<(Value, Version)>
    where
        F: Fn(&Value) -> StrataResult<Value> + Send + Sync;

    /// Apply transition or initialize if cell doesn't exist
    ///
    /// If the cell doesn't exist, initializes it with `initial`, then applies the transition.
    ///
    /// ## Errors
    ///
    /// - `InvalidKey`: Cell name is invalid
    /// - `NotFound`: Run does not exist
    /// - `ConstraintViolation`: Run is closed
    fn state_transition_or_init<F>(
        &self,
        run: &ApiRunId,
        cell: &str,
        initial: Value,
        f: F,
    ) -> StrataResult<(Value, Version)>
    where
        F: Fn(&Value) -> StrataResult<Value> + Send + Sync;

    /// Get existing value or initialize with a lazy default
    ///
    /// Returns the current cell value if it exists, otherwise initializes
    /// the cell with the value produced by `default` and returns it.
    ///
    /// ## Lazy Default Pattern
    ///
    /// The `default` closure is only called if the cell doesn't exist.
    /// This avoids allocating default values on the hot path when reading
    /// existing cells.
    ///
    /// ```rust,ignore
    /// // Expensive default only computed if cell doesn't exist
    /// let state = substrate.state_get_or_init(&run, "config", || {
    ///     Value::String(compute_expensive_default())
    /// })?;
    /// ```
    ///
    /// ## Semantics
    ///
    /// - If cell exists: returns current value (default closure NOT called)
    /// - If cell doesn't exist: calls `default()`, initializes cell, returns new value
    /// - The returned version is always 1 for newly created cells
    ///
    /// ## Return Value
    ///
    /// Returns `Versioned<Value>` containing the value and its version.
    ///
    /// ## Errors
    ///
    /// - `InvalidKey`: Cell name is invalid
    /// - `NotFound`: Run does not exist
    /// - `ConstraintViolation`: Run is closed
    fn state_get_or_init<F>(
        &self,
        run: &ApiRunId,
        cell: &str,
        default: F,
    ) -> StrataResult<Versioned<Value>>
    where
        F: FnOnce() -> Value;
}

// =============================================================================
// Implementation
// =============================================================================

use super::impl_::{SubstrateImpl, convert_error};

impl StateCell for SubstrateImpl {
    fn state_set(&self, run: &ApiRunId, cell: &str, value: Value) -> StrataResult<Version> {
        let run_id = run.to_run_id();
        let versioned = self.state().set(&run_id, cell, value).map_err(convert_error)?;
        Ok(versioned.version)
    }

    fn state_get(&self, run: &ApiRunId, cell: &str) -> StrataResult<Option<Versioned<Value>>> {
        let run_id = run.to_run_id();
        let state = self.state().read(&run_id, cell).map_err(convert_error)?;
        Ok(state.map(|s| Versioned {
            value: s.value.value,
            version: s.version,
            timestamp: s.timestamp,
        }))
    }

    fn state_cas(
        &self,
        run: &ApiRunId,
        cell: &str,
        expected_counter: Option<u64>,
        value: Value,
    ) -> StrataResult<Option<Version>> {
        let run_id = run.to_run_id();

        match expected_counter {
            None => {
                // Create only if doesn't exist
                match self.state().init(&run_id, cell, value) {
                    Ok(versioned) => Ok(Some(versioned.version)),
                    Err(_) => Ok(None), // Already exists
                }
            }
            Some(expected) => {
                // CAS with expected version
                match self.state().cas(&run_id, cell, Version::counter(expected), value) {
                    Ok(versioned) => Ok(Some(versioned.version)),
                    Err(_) => Ok(None), // Version mismatch
                }
            }
        }
    }

    fn state_delete(&self, run: &ApiRunId, cell: &str) -> StrataResult<bool> {
        let run_id = run.to_run_id();
        self.state().delete(&run_id, cell).map_err(convert_error)
    }

    fn state_exists(&self, run: &ApiRunId, cell: &str) -> StrataResult<bool> {
        let run_id = run.to_run_id();
        self.state().exists(&run_id, cell).map_err(convert_error)
    }

    fn state_history(
        &self,
        run: &ApiRunId,
        cell: &str,
        limit: Option<u64>,
        before: Option<Version>,
    ) -> StrataResult<Vec<Versioned<Value>>> {
        let run_id = run.to_run_id();

        // Extract counter from before (StateCell uses Counter versions)
        let before_counter = match before {
            Some(Version::Counter(c)) => Some(c),
            Some(_) => return Err(strata_core::StrataError::invalid_input(
                "StateCell operations use Counter versions",
            )),
            None => None,
        };

        // Use primitive's history method
        self.state()
            .history(&run_id, cell, limit.map(|l| l as usize), before_counter)
            .map_err(convert_error)
    }

    fn state_init(&self, run: &ApiRunId, cell: &str, value: Value) -> StrataResult<Version> {
        let run_id = run.to_run_id();
        let versioned = self.state().init(&run_id, cell, value).map_err(convert_error)?;
        Ok(versioned.version)
    }

    fn state_list(&self, run: &ApiRunId) -> StrataResult<Vec<String>> {
        let run_id = run.to_run_id();
        self.state().list(&run_id).map_err(convert_error)
    }

    fn state_transition<F>(
        &self,
        run: &ApiRunId,
        cell: &str,
        f: F,
    ) -> StrataResult<(Value, Version)>
    where
        F: Fn(&Value) -> StrataResult<Value> + Send + Sync,
    {
        let run_id = run.to_run_id();

        // Wrap the substrate closure to match the primitive's signature
        let primitive_closure = |state: &strata_engine::State| {
            let new_value = f(&state.value)
                .map_err(|e| strata_core::StrataError::invalid_input(e.to_string()))?;
            Ok((new_value.clone(), new_value))
        };

        let (new_value, versioned) = self.state()
            .transition(&run_id, cell, primitive_closure)
            .map_err(convert_error)?;

        Ok((new_value, versioned.version))
    }

    fn state_transition_or_init<F>(
        &self,
        run: &ApiRunId,
        cell: &str,
        initial: Value,
        f: F,
    ) -> StrataResult<(Value, Version)>
    where
        F: Fn(&Value) -> StrataResult<Value> + Send + Sync,
    {
        let run_id = run.to_run_id();

        // Wrap the substrate closure to match the primitive's signature
        let primitive_closure = |state: &strata_engine::State| {
            let new_value = f(&state.value)
                .map_err(|e| strata_core::StrataError::invalid_input(e.to_string()))?;
            Ok((new_value.clone(), new_value))
        };

        let (new_value, versioned) = self.state()
            .transition_or_init(&run_id, cell, initial, primitive_closure)
            .map_err(convert_error)?;

        Ok((new_value, versioned.version))
    }

    fn state_get_or_init<F>(
        &self,
        run: &ApiRunId,
        cell: &str,
        default: F,
    ) -> StrataResult<Versioned<Value>>
    where
        F: FnOnce() -> Value,
    {
        let run_id = run.to_run_id();

        // Fast path: check if cell exists
        let existing = self.state().read(&run_id, cell).map_err(convert_error)?;

        if let Some(state) = existing {
            // Cell exists - return it without calling default
            return Ok(Versioned {
                value: state.value.value,
                version: state.version,
                timestamp: state.timestamp,
            });
        }

        // Cell doesn't exist - call default and initialize
        let default_value = default();
        let versioned = self.state().init(&run_id, cell, default_value.clone())
            .map_err(convert_error)?;

        // Return the newly created value
        Ok(Versioned {
            value: default_value,
            version: versioned.version,
            timestamp: strata_core::Timestamp::now(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Note: StateCell is NOT dyn-compatible due to generic methods
    // state_transition<F> and state_transition_or_init<F>
    // This is by design for type-safe closures.
}
