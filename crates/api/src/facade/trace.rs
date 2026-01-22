//! Trace Facade - Simplified tracing operations
//!
//! This module provides operations for structured logging of reasoning traces.
//!
//! ## Desugaring
//!
//! | Facade | Substrate |
//! |--------|-----------|
//! | `trace(type, content)` | `trace_create(default_run, type, None, content, [])` |
//! | `trace_child(parent, type, content)` | `trace_create(default_run, type, parent, content, [])` |

use strata_core::{StrataResult, Value};

/// Trace type for categorization
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TraceKind {
    /// Internal reasoning
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

impl TraceKind {
    /// Convert to substrate trace type string
    pub fn as_str(&self) -> &str {
        match self {
            TraceKind::Thought => "thought",
            TraceKind::Action => "action",
            TraceKind::Observation => "observation",
            TraceKind::Tool => "tool",
            TraceKind::Message => "message",
            TraceKind::Custom(s) => s,
        }
    }
}

/// A trace entry
#[derive(Debug, Clone)]
pub struct Trace {
    /// Unique trace ID
    pub id: String,
    /// Trace type
    pub kind: TraceKind,
    /// Parent trace ID (if any)
    pub parent_id: Option<String>,
    /// Trace content
    pub content: Value,
    /// Tags
    pub tags: Vec<String>,
    /// Creation timestamp
    pub timestamp: u64,
}

/// Options for creating traces
#[derive(Debug, Clone, Default)]
pub struct TraceOptions {
    /// Parent trace ID
    pub parent_id: Option<String>,
    /// Tags
    pub tags: Vec<String>,
    /// Explicit ID (if not provided, one is generated)
    pub id: Option<String>,
}

impl TraceOptions {
    /// Create default options
    pub fn new() -> Self {
        Self::default()
    }

    /// Set parent trace
    pub fn with_parent(mut self, parent_id: impl Into<String>) -> Self {
        self.parent_id = Some(parent_id.into());
        self
    }

    /// Add a tag
    pub fn with_tag(mut self, tag: impl Into<String>) -> Self {
        self.tags.push(tag.into());
        self
    }

    /// Set explicit ID
    pub fn with_id(mut self, id: impl Into<String>) -> Self {
        self.id = Some(id.into());
        self
    }
}

/// Trace Facade - simplified tracing operations
///
/// Provides structured logging for reasoning traces with hierarchical support.
pub trait TraceFacade {
    /// Create a new trace
    ///
    /// Returns the trace ID.
    ///
    /// ## Example
    /// ```ignore
    /// let id = facade.trace(TraceKind::Thought, json!({
    ///     "text": "I should check the user's permissions first"
    /// }))?;
    /// ```
    fn trace(&self, kind: TraceKind, content: Value) -> StrataResult<String>;

    /// Create a trace with options
    ///
    /// Allows setting parent, tags, and explicit ID.
    fn trace_with_options(
        &self,
        kind: TraceKind,
        content: Value,
        options: TraceOptions,
    ) -> StrataResult<String>;

    /// Create a child trace
    ///
    /// Convenience method for creating a trace with a parent.
    fn trace_child(
        &self,
        parent_id: &str,
        kind: TraceKind,
        content: Value,
    ) -> StrataResult<String>;

    /// Get a trace by ID
    fn trace_get(&self, id: &str) -> StrataResult<Option<Trace>>;

    /// List traces
    ///
    /// Returns traces matching optional filters, newest first.
    ///
    /// ## Parameters
    /// - `kind`: Filter by trace type
    /// - `limit`: Maximum number of results
    fn trace_list(
        &self,
        kind: Option<TraceKind>,
        limit: Option<u64>,
    ) -> StrataResult<Vec<Trace>>;

    /// List root traces (no parent)
    fn trace_roots(&self, limit: Option<u64>) -> StrataResult<Vec<Trace>>;

    /// Get children of a trace
    fn trace_children(&self, parent_id: &str) -> StrataResult<Vec<Trace>>;

    /// Add tags to a trace
    fn trace_tag(&self, id: &str, tags: Vec<String>) -> StrataResult<()>;

    /// Remove tags from a trace
    fn trace_untag(&self, id: &str, tags: Vec<String>) -> StrataResult<()>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_trait_is_object_safe() {
        fn _assert_object_safe(_: &dyn TraceFacade) {}
    }

    #[test]
    fn test_trace_kind() {
        assert_eq!(TraceKind::Thought.as_str(), "thought");
        assert_eq!(TraceKind::Action.as_str(), "action");
        assert_eq!(TraceKind::Custom("my_type".to_string()).as_str(), "my_type");
    }

    #[test]
    fn test_trace_options() {
        let opts = TraceOptions::new()
            .with_parent("parent-123")
            .with_tag("important")
            .with_id("custom-id");

        assert_eq!(opts.parent_id, Some("parent-123".to_string()));
        assert_eq!(opts.tags, vec!["important".to_string()]);
        assert_eq!(opts.id, Some("custom-id".to_string()));
    }
}
