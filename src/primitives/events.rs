//! Event stream primitive.
//!
//! The Events primitive provides append-only event streams with
//! ordered reads and range queries.

use crate::error::Result;
use crate::types::{run_id_to_api, RunId, Value, Version, Versioned};
use std::sync::Arc;

use strata_api::substrate::{ApiRunId, EventLog};

/// Event stream operations.
///
/// Access via `db.events`.
pub struct Events {
    #[allow(dead_code)]
    db: Arc<strata_engine::Database>,
    substrate: strata_api::substrate::SubstrateImpl,
}

impl Events {
    pub(crate) fn new(db: Arc<strata_engine::Database>) -> Self {
        let substrate = strata_api::substrate::SubstrateImpl::new(db.clone());
        Self { db, substrate }
    }

    // =========================================================================
    // Simple API (default run)
    // =========================================================================

    /// Append an event to a stream.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let mut event = HashMap::new();
    /// event.insert("action".to_string(), Value::from("login"));
    /// db.events.append("activity", event)?;
    /// ```
    pub fn append(&self, stream: &str, payload: impl Into<Value>) -> Result<Version> {
        let run = ApiRunId::default();
        Ok(self.substrate.event_append(&run, stream, payload.into())?)
    }

    /// Read events from a stream.
    ///
    /// # Arguments
    ///
    /// * `stream` - Stream name
    /// * `limit` - Maximum events to return
    ///
    /// # Example
    ///
    /// ```ignore
    /// let events = db.events.read("activity", 100)?;
    /// for event in events {
    ///     println!("{:?}: {:?}", event.version, event.value);
    /// }
    /// ```
    pub fn read(&self, stream: &str, limit: usize) -> Result<Vec<Versioned<Value>>> {
        let run = ApiRunId::default();
        Ok(self.substrate.event_range(&run, stream, None, None, Some(limit as u64))?)
    }

    // =========================================================================
    // Run-scoped API
    // =========================================================================

    /// Append an event to a stream in a specific run.
    pub fn append_in(&self, run: &RunId, stream: &str, payload: impl Into<Value>) -> Result<Version> {
        let api_run = run_id_to_api(run);
        Ok(self.substrate.event_append(&api_run, stream, payload.into())?)
    }

    /// Read events from a stream in a specific run.
    pub fn read_in(
        &self,
        run: &RunId,
        stream: &str,
        limit: usize,
    ) -> Result<Vec<Versioned<Value>>> {
        let api_run = run_id_to_api(run);
        Ok(self.substrate.event_range(&api_run, stream, None, None, Some(limit as u64))?)
    }

    // =========================================================================
    // Range queries
    // =========================================================================

    /// Read events in a sequence range.
    ///
    /// # Arguments
    ///
    /// * `start` - Starting sequence (inclusive)
    /// * `end` - Ending sequence (inclusive)
    pub fn range(
        &self,
        run: &RunId,
        stream: &str,
        start: u64,
        end: u64,
    ) -> Result<Vec<Versioned<Value>>> {
        let api_run = run_id_to_api(run);
        Ok(self.substrate.event_range(&api_run, stream, Some(start), Some(end), None)?)
    }

    /// Get the latest event for a stream.
    pub fn head(&self, run: &RunId, stream: &str) -> Result<Option<Versioned<Value>>> {
        let api_run = run_id_to_api(run);
        Ok(self.substrate.event_head(&api_run, stream)?)
    }

    /// Get the count of events in a stream.
    pub fn count(&self, run: &RunId, stream: &str) -> Result<u64> {
        let api_run = run_id_to_api(run);
        Ok(self.substrate.event_stream_info(&api_run, stream)?.count)
    }

    /// List all stream names in a run.
    pub fn streams(&self, run: &RunId) -> Result<Vec<String>> {
        let api_run = run_id_to_api(run);
        Ok(self.substrate.event_streams(&api_run)?)
    }
}
