//! State cell primitive.
//!
//! The State primitive provides compare-and-swap (CAS) cells for
//! single-writer coordination and distributed locks.

use crate::error::Result;
use crate::types::{run_id_to_api, RunId, Value, Version, Versioned};
use std::sync::Arc;

use strata_api::substrate::{ApiRunId, StateCell};

/// State cell operations.
///
/// Access via `db.state`.
pub struct State {
    #[allow(dead_code)]
    db: Arc<strata_engine::Database>,
    substrate: strata_api::substrate::SubstrateImpl,
}

impl State {
    pub(crate) fn new(db: Arc<strata_engine::Database>) -> Self {
        let substrate = strata_api::substrate::SubstrateImpl::new(db.clone());
        Self { db, substrate }
    }

    // =========================================================================
    // Simple API (default run)
    // =========================================================================

    /// Get a state cell's current value.
    ///
    /// # Example
    ///
    /// ```ignore
    /// if let Some(state) = db.state.get("task-status")? {
    ///     println!("Current state: {:?}", state.value);
    /// }
    /// ```
    pub fn get(&self, key: &str) -> Result<Option<Versioned<Value>>> {
        let run = ApiRunId::default();
        Ok(self.substrate.state_get(&run, key)?)
    }

    /// Set a state cell's value.
    ///
    /// # Example
    ///
    /// ```ignore
    /// db.state.set("task-status", "running")?;
    /// db.state.set("counter", 42)?;
    /// ```
    pub fn set(&self, key: &str, value: impl Into<Value>) -> Result<Version> {
        let run = ApiRunId::default();
        Ok(self.substrate.state_set(&run, key, value.into())?)
    }

    // =========================================================================
    // Run-scoped API
    // =========================================================================

    /// Get a state cell from a specific run.
    pub fn get_in(&self, run: &RunId, key: &str) -> Result<Option<Versioned<Value>>> {
        let api_run = run_id_to_api(run);
        Ok(self.substrate.state_get(&api_run, key)?)
    }

    /// Set a state cell in a specific run.
    pub fn set_in(&self, run: &RunId, key: &str, value: impl Into<Value>) -> Result<Version> {
        let api_run = run_id_to_api(run);
        Ok(self.substrate.state_set(&api_run, key, value.into())?)
    }

    // =========================================================================
    // Full control API
    // =========================================================================

    /// Compare-and-swap by counter.
    ///
    /// Sets the value only if the current counter matches `expected`.
    /// Pass `None` as expected to succeed only if the cell doesn't exist.
    ///
    /// Returns `Some(Version)` if successful, `None` if CAS failed.
    ///
    /// # Example
    ///
    /// ```ignore
    /// // Get current version
    /// let current = db.state.get_in(&run, "lock")?;
    /// let counter = current.map(|v| {
    ///     match v.version {
    ///         Version::Counter(c) => c,
    ///         _ => 0,
    ///     }
    /// });
    ///
    /// // Try to update
    /// if let Some(new_version) = db.state.cas(&run, "lock", counter, "acquired".into())? {
    ///     println!("Acquired lock at version {:?}", new_version);
    /// } else {
    ///     println!("Lock contention");
    /// }
    /// ```
    pub fn cas(
        &self,
        run: &RunId,
        key: &str,
        expected_counter: Option<u64>,
        value: impl Into<Value>,
    ) -> Result<Option<Version>> {
        let api_run = run_id_to_api(run);
        Ok(self.substrate.state_cas(&api_run, key, expected_counter, value.into())?)
    }

    /// Delete a state cell.
    pub fn delete(&self, run: &RunId, key: &str) -> Result<bool> {
        let api_run = run_id_to_api(run);
        Ok(self.substrate.state_delete(&api_run, key)?)
    }

    /// Check if a state cell exists.
    pub fn exists(&self, run: &RunId, key: &str) -> Result<bool> {
        let api_run = run_id_to_api(run);
        Ok(self.substrate.state_exists(&api_run, key)?)
    }

    /// Get version history for a state cell.
    pub fn history(
        &self,
        run: &RunId,
        key: &str,
        limit: Option<u64>,
    ) -> Result<Vec<Versioned<Value>>> {
        let api_run = run_id_to_api(run);
        Ok(self.substrate.state_history(&api_run, key, limit, None)?)
    }
}
