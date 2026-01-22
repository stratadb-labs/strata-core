//! TraceStore Substrate Operations
//!
//! The TraceStore provides structured logging for reasoning traces.
//! It supports hierarchical traces with parent-child relationships.
//!
//! ## Trace Model
//!
//! - Traces form a forest (multiple roots) or a tree (single root)
//! - Each trace has a unique ID
//! - Traces can have a parent (forming a hierarchy)
//! - Traces can have type and tags for categorization
//!
//! ## Trace Types
//!
//! - `Thought`: Internal reasoning
//! - `Action`: Executed action
//! - `Observation`: External observation
//! - `Tool`: Tool invocation
//! - `Message`: User/assistant message
//!
//! ## Versioning
//!
//! Traces use transaction-based versioning (`Version::Txn`).

use super::types::ApiRunId;
use strata_core::{StrataResult, Value, Version, Versioned};
use serde::{Deserialize, Serialize};

/// Trace type for categorization
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TraceType {
    /// Internal reasoning
    #[default]
    Thought,
    /// Executed action
    Action,
    /// External observation
    Observation,
    /// Tool invocation
    Tool,
    /// User or assistant message
    Message,
    /// Custom type
    Custom(String),
}

/// A trace entry
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TraceEntry {
    /// Unique trace ID
    pub id: String,
    /// Trace type
    pub trace_type: TraceType,
    /// Parent trace ID (if any)
    pub parent_id: Option<String>,
    /// Trace content/payload
    pub content: Value,
    /// Tags for filtering
    pub tags: Vec<String>,
    /// Creation timestamp (microseconds since epoch)
    pub created_at: u64,
}

/// TraceStore substrate operations
///
/// This trait defines the canonical trace store operations.
/// All operations require explicit run_id and return versioned results.
///
/// ## Contract
///
/// - Trace IDs are unique within a run
/// - Parent references must point to existing traces
/// - Content must be `Value::Object`
///
/// ## Error Handling
///
/// | Condition | Error |
/// |-----------|-------|
/// | Invalid trace ID | `InvalidKey` |
/// | Invalid parent reference | `NotFound` |
/// | Content not Object | `ConstraintViolation` |
/// | Run not found | `NotFound` |
/// | Run is closed | `ConstraintViolation` |
pub trait TraceStore {
    /// Create a new trace
    ///
    /// Adds a new trace entry and returns its version.
    ///
    /// ## Parameters
    ///
    /// - `trace_type`: Type of trace for categorization
    /// - `parent_id`: Optional parent trace ID
    /// - `content`: Trace content (must be Object)
    /// - `tags`: Optional tags for filtering
    ///
    /// ## Return Value
    ///
    /// Returns `(trace_id, version)` where `trace_id` is a generated UUID.
    ///
    /// ## Errors
    ///
    /// - `ConstraintViolation`: Content is not Object, or run is closed
    /// - `NotFound`: Run or parent trace does not exist
    fn trace_create(
        &self,
        run: &ApiRunId,
        trace_type: TraceType,
        parent_id: Option<&str>,
        content: Value,
        tags: Vec<String>,
    ) -> StrataResult<(String, Version)>;

    /// Create a trace with explicit ID
    ///
    /// Like `trace_create`, but with a caller-provided ID.
    ///
    /// ## Errors
    ///
    /// - `InvalidKey`: Trace ID is invalid
    /// - `ConstraintViolation`: ID already exists, content not Object, or run closed
    /// - `NotFound`: Run or parent trace does not exist
    fn trace_create_with_id(
        &self,
        run: &ApiRunId,
        id: &str,
        trace_type: TraceType,
        parent_id: Option<&str>,
        content: Value,
        tags: Vec<String>,
    ) -> StrataResult<Version>;

    /// Get a trace by ID
    ///
    /// Returns the trace entry.
    ///
    /// ## Errors
    ///
    /// - `InvalidKey`: Trace ID is invalid
    /// - `NotFound`: Run or trace does not exist
    fn trace_get(&self, run: &ApiRunId, id: &str) -> StrataResult<Option<Versioned<TraceEntry>>>;

    /// List traces with optional filters
    ///
    /// Returns traces matching the filters, newest first.
    ///
    /// ## Parameters
    ///
    /// - `trace_type`: Filter by type
    /// - `parent_id`: Filter by parent (`Some(None)` = roots only, `None` = no filter)
    /// - `tag`: Filter by tag (trace must have this tag)
    /// - `limit`: Maximum traces to return
    /// - `before`: Return traces older than this (exclusive)
    ///
    /// ## Errors
    ///
    /// - `NotFound`: Run does not exist
    fn trace_list(
        &self,
        run: &ApiRunId,
        trace_type: Option<TraceType>,
        parent_id: Option<Option<&str>>,
        tag: Option<&str>,
        limit: Option<u64>,
        before: Option<Version>,
    ) -> StrataResult<Vec<Versioned<TraceEntry>>>;

    /// Get child traces
    ///
    /// Returns all traces with the given parent ID.
    ///
    /// ## Errors
    ///
    /// - `InvalidKey`: Parent ID is invalid
    /// - `NotFound`: Run or parent trace does not exist
    fn trace_children(
        &self,
        run: &ApiRunId,
        parent_id: &str,
    ) -> StrataResult<Vec<Versioned<TraceEntry>>>;

    /// Get the trace tree rooted at the given trace
    ///
    /// Returns the trace and all its descendants.
    /// Order is pre-order (parent before children).
    ///
    /// ## Errors
    ///
    /// - `InvalidKey`: Trace ID is invalid
    /// - `NotFound`: Run or trace does not exist
    fn trace_tree(&self, run: &ApiRunId, root_id: &str) -> StrataResult<Vec<Versioned<TraceEntry>>>;

    /// Update trace tags
    ///
    /// Adds or removes tags from a trace.
    /// Returns the new version.
    ///
    /// ## Parameters
    ///
    /// - `add_tags`: Tags to add
    /// - `remove_tags`: Tags to remove (if present)
    ///
    /// ## Errors
    ///
    /// - `InvalidKey`: Trace ID is invalid
    /// - `NotFound`: Run or trace does not exist
    /// - `ConstraintViolation`: Run is closed
    fn trace_update_tags(
        &self,
        run: &ApiRunId,
        id: &str,
        add_tags: Vec<String>,
        remove_tags: Vec<String>,
    ) -> StrataResult<Version>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_trait_is_object_safe() {
        fn _assert_object_safe(_: &dyn TraceStore) {}
    }

    #[test]
    fn test_trace_type_default() {
        assert_eq!(TraceType::default(), TraceType::Thought);
    }

    #[test]
    fn test_trace_type_serialization() {
        let types = vec![
            TraceType::Thought,
            TraceType::Action,
            TraceType::Observation,
            TraceType::Tool,
            TraceType::Message,
            TraceType::Custom("my_type".to_string()),
        ];

        for t in types {
            let json = serde_json::to_string(&t).unwrap();
            let restored: TraceType = serde_json::from_str(&json).unwrap();
            assert_eq!(t, restored);
        }
    }
}
