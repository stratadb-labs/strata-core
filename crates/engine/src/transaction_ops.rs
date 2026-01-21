//! TransactionOps trait - unified primitive operations
//!
//! This trait expresses Invariant 3: Everything is Transactional.
//! Every primitive's operations are accessible through this trait,
//! enabling cross-primitive atomic operations.
//!
//! ## Design Principles
//!
//! 1. **Reads are `&self`**: Read operations never modify state
//! 2. **Writes are `&mut self`**: Write operations require exclusive access
//! 3. **All operations return `Result<T, StrataError>`**: Consistent error handling
//! 4. **All reads return `Versioned<T>`**: Version information is never lost
//! 5. **All writes return `Version`**: Every mutation produces a version
//!
//! ## Usage
//!
//! ```rust,ignore
//! db.transaction(&run_id, |txn| {
//!     // Read from KV
//!     let config = txn.kv_get("config")?;
//!
//!     // Write to Event
//!     let event_version = txn.event_append("config_read", json!({}))?;
//!
//!     // Update State
//!     txn.state_set("last_event", Value::from(event_version.as_u64()))?;
//!
//!     Ok(())
//! })?;
//! ```

use strata_core::{
    Event, JsonDocId, JsonPath, JsonValue, MetadataFilter, RunMetadata, RunStatus, State,
    StrataError, Trace, TraceType, Value, VectorEntry, VectorMatch, Version, Versioned,
};

/// Operations available within a transaction
///
/// This trait expresses Invariant 3: Everything is Transactional.
/// Every primitive's operations are accessible through this trait,
/// enabling cross-primitive atomic operations.
///
/// ## Phase 2 Implementation
///
/// Phase 2 implements KV and EventLog operations. Other primitive
/// operations return `unimplemented!()` and will be wired in later phases:
/// - Phase 3: State + Trace
/// - Phase 4: Json + Vector
/// - Phase 5: Run operations (finalize)
pub trait TransactionOps {
    // =========================================================================
    // KV Operations (Phase 2)
    // =========================================================================

    /// Get a KV entry by key
    fn kv_get(&self, key: &str) -> Result<Option<Versioned<Value>>, StrataError>;

    /// Put a KV entry (upsert semantics)
    fn kv_put(&mut self, key: &str, value: Value) -> Result<Version, StrataError>;

    /// Delete a KV entry
    fn kv_delete(&mut self, key: &str) -> Result<bool, StrataError>;

    /// Check if a KV entry exists
    fn kv_exists(&self, key: &str) -> Result<bool, StrataError>;

    /// List keys matching a prefix
    fn kv_list(&self, prefix: Option<&str>) -> Result<Vec<String>, StrataError>;

    // =========================================================================
    // Event Operations (Phase 2)
    // =========================================================================

    /// Append an event to the log
    fn event_append(&mut self, event_type: &str, payload: Value) -> Result<Version, StrataError>;

    /// Read an event by sequence number
    fn event_read(&self, sequence: u64) -> Result<Option<Versioned<Event>>, StrataError>;

    /// Read a range of events [start, end)
    fn event_range(&self, start: u64, end: u64) -> Result<Vec<Versioned<Event>>, StrataError>;

    /// Get current event count (length of the log)
    fn event_len(&self) -> Result<u64, StrataError>;

    // =========================================================================
    // State Operations (Phase 3)
    // =========================================================================

    /// Read a state cell
    fn state_read(&self, name: &str) -> Result<Option<Versioned<State>>, StrataError>;

    /// Initialize a state cell (fails if exists)
    fn state_init(&mut self, name: &str, value: Value) -> Result<Version, StrataError>;

    /// Compare-and-swap a state cell
    fn state_cas(
        &mut self,
        name: &str,
        expected_version: u64,
        value: Value,
    ) -> Result<Version, StrataError>;

    /// Delete a state cell
    fn state_delete(&mut self, name: &str) -> Result<bool, StrataError>;

    /// Check if a state cell exists
    fn state_exists(&self, name: &str) -> Result<bool, StrataError>;

    // =========================================================================
    // Trace Operations (Phase 3)
    // =========================================================================

    /// Record a trace entry
    fn trace_record(
        &mut self,
        trace_type: TraceType,
        tags: Vec<String>,
        content: Value,
    ) -> Result<Versioned<u64>, StrataError>;

    /// Read a trace by ID
    fn trace_read(&self, trace_id: u64) -> Result<Option<Versioned<Trace>>, StrataError>;

    /// Check if a trace exists
    fn trace_exists(&self, trace_id: u64) -> Result<bool, StrataError>;

    /// Get trace count for this run
    fn trace_count(&self) -> Result<u64, StrataError>;

    // =========================================================================
    // Json Operations (Phase 4)
    // =========================================================================

    /// Create a JSON document
    fn json_create(&mut self, doc_id: &JsonDocId, value: JsonValue) -> Result<Version, StrataError>;

    /// Get an entire JSON document
    fn json_get(&self, doc_id: &JsonDocId) -> Result<Option<Versioned<JsonValue>>, StrataError>;

    /// Get a value at a path within a JSON document
    fn json_get_path(
        &self,
        doc_id: &JsonDocId,
        path: &JsonPath,
    ) -> Result<Option<JsonValue>, StrataError>;

    /// Set a value at a path within a JSON document
    fn json_set(
        &mut self,
        doc_id: &JsonDocId,
        path: &JsonPath,
        value: JsonValue,
    ) -> Result<Version, StrataError>;

    /// Delete a JSON document
    fn json_delete(&mut self, doc_id: &JsonDocId) -> Result<bool, StrataError>;

    /// Check if a JSON document exists
    fn json_exists(&self, doc_id: &JsonDocId) -> Result<bool, StrataError>;

    /// Destroy a JSON document (same as delete, for API consistency)
    fn json_destroy(&mut self, doc_id: &JsonDocId) -> Result<bool, StrataError>;

    // =========================================================================
    // Vector Operations (Phase 4)
    // =========================================================================

    /// Insert a vector into a collection
    fn vector_insert(
        &mut self,
        collection: &str,
        key: &str,
        embedding: &[f32],
        metadata: Option<Value>,
    ) -> Result<Version, StrataError>;

    /// Get a vector by key
    fn vector_get(
        &self,
        collection: &str,
        key: &str,
    ) -> Result<Option<Versioned<VectorEntry>>, StrataError>;

    /// Delete a vector
    fn vector_delete(&mut self, collection: &str, key: &str) -> Result<bool, StrataError>;

    /// Search for similar vectors
    fn vector_search(
        &self,
        collection: &str,
        query: &[f32],
        k: usize,
        filter: Option<MetadataFilter>,
    ) -> Result<Vec<VectorMatch>, StrataError>;

    /// Check if a vector exists
    fn vector_exists(&self, collection: &str, key: &str) -> Result<bool, StrataError>;

    // =========================================================================
    // Run Operations (Phase 5 - Limited, runs are meta-level)
    // =========================================================================

    /// Get run metadata (the current run)
    fn run_metadata(&self) -> Result<Option<Versioned<RunMetadata>>, StrataError>;

    /// Update run status
    fn run_update_status(&mut self, status: RunStatus) -> Result<Version, StrataError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    // Verify trait is object-safe (can be used as dyn TransactionOps)
    fn _assert_object_safe(_: &dyn TransactionOps) {}

    #[test]
    fn test_trait_compiles() {
        // This test verifies the trait compiles with all methods
        // Actual implementation tests are in the Transaction impl
    }
}
