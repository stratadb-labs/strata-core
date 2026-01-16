//! TraceStore: Structured reasoning traces with indexing
//!
//! ## Design Principles
//!
//! 1. **Structured Types**: Rich trace types (ToolCall, Decision, Query, etc.)
//! 2. **Parent-Child Relationships**: Nested traces for hierarchical reasoning
//! 3. **Secondary Indices**: Efficient queries by type, tag, parent, time
//! 4. **Tree Reconstruction**: Build trace hierarchies for visualization
//!
//! ## Performance Warning
//!
//! TraceStore is optimized for DEBUGGABILITY, not ingestion throughput.
//! Each trace creates 3-4 secondary index entries (write amplification).
//!
//! Designed for: reasoning traces (tens to hundreds per run)
//! NOT designed for: telemetry (thousands per second)
//!
//! For high-volume tracing, consider batching or sampling.
//!
//! ## Key Design
//!
//! - TypeTag: Trace (0x04)
//! - Primary key format: `<namespace>:<TypeTag::Trace>:<trace_id>`
//! - Index key format: `<namespace>:<TypeTag::Trace>:__idx_{type}__{value}__{trace_id}`

use crate::extensions::TraceStoreExt;
use in_mem_concurrency::TransactionContext;
use in_mem_core::error::{Error, Result};
use in_mem_core::types::{Key, Namespace, RunId};
use in_mem_core::value::Value;
use in_mem_engine::Database;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

// ========== TraceType Enum (Story #185) ==========

/// Types of reasoning traces
///
/// Each variant captures different aspects of agent reasoning:
/// - ToolCall: External tool invocations
/// - Decision: Choice points with reasoning
/// - Query: Information lookups
/// - Thought: Internal reasoning
/// - Error: Error occurrences
/// - Custom: User-defined types
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum TraceType {
    /// External tool invocation
    ToolCall {
        /// Name of the tool invoked
        tool_name: String,
        /// Arguments passed to the tool
        arguments: Value,
        /// Result returned by the tool (if completed)
        result: Option<Value>,
        /// Duration in milliseconds (if measured)
        duration_ms: Option<u64>,
    },
    /// Decision point with options
    Decision {
        /// The question being decided
        question: String,
        /// Available options
        options: Vec<String>,
        /// The chosen option
        chosen: String,
        /// Reasoning for the choice (optional)
        reasoning: Option<String>,
    },
    /// Information query
    Query {
        /// Type of query (e.g., "database", "api", "search")
        query_type: String,
        /// The actual query
        query: String,
        /// Number of results returned (optional)
        results_count: Option<u32>,
    },
    /// Internal reasoning
    Thought {
        /// The thought content
        content: String,
        /// Confidence level 0.0-1.0 (optional)
        confidence: Option<f64>,
    },
    /// Error occurrence
    Error {
        /// Type of error
        error_type: String,
        /// Error message
        message: String,
        /// Whether the error is recoverable
        recoverable: bool,
    },
    /// User-defined trace type
    Custom {
        /// Custom type name
        name: String,
        /// Custom data
        data: Value,
    },
}

impl TraceType {
    /// Get the type name for indexing
    ///
    /// Returns a stable string identifier for the trace type.
    /// Custom types return their user-defined name.
    pub fn type_name(&self) -> &str {
        match self {
            TraceType::ToolCall { .. } => "ToolCall",
            TraceType::Decision { .. } => "Decision",
            TraceType::Query { .. } => "Query",
            TraceType::Thought { .. } => "Thought",
            TraceType::Error { .. } => "Error",
            TraceType::Custom { name, .. } => name,
        }
    }
}

// ========== Trace Struct (Story #185) ==========

/// A reasoning trace entry
///
/// Each trace represents a single unit of reasoning with:
/// - Unique identifier
/// - Optional parent for nesting
/// - Typed content
/// - Timestamp for ordering
/// - Tags for filtering
/// - Arbitrary metadata
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Trace {
    /// Unique trace ID (format: "trace-{uuid}")
    pub id: String,
    /// Parent trace ID (if nested)
    pub parent_id: Option<String>,
    /// Type of trace with type-specific data
    pub trace_type: TraceType,
    /// Creation timestamp (milliseconds since epoch)
    pub timestamp: i64,
    /// User-defined tags for filtering
    pub tags: Vec<String>,
    /// Additional metadata
    pub metadata: Value,
}

impl Trace {
    /// Generate a new trace ID
    ///
    /// Format: "trace-{uuid}" where uuid is a v4 UUID
    pub fn generate_id() -> String {
        format!("trace-{}", uuid::Uuid::new_v4())
    }

    /// Get current timestamp in milliseconds
    fn now() -> i64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64
    }
}

// ========== TraceTree Struct (Story #189) ==========

/// A trace with its children for tree visualization
///
/// Used by `get_tree()` to reconstruct the hierarchical
/// structure of traces from their parent-child relationships.
#[derive(Debug, Clone)]
pub struct TraceTree {
    /// The trace at this node
    pub trace: Trace,
    /// Child traces (recursively)
    pub children: Vec<TraceTree>,
}

// ========== Serialization Helpers ==========

/// Serialize a struct to Value::String for storage
fn to_stored_value<T: Serialize>(v: &T) -> Value {
    match serde_json::to_string(v) {
        Ok(s) => Value::String(s),
        Err(_) => Value::Null,
    }
}

/// Deserialize from Value::String storage
fn from_stored_value<T: for<'de> Deserialize<'de>>(
    v: &Value,
) -> std::result::Result<T, serde_json::Error> {
    match v {
        Value::String(s) => serde_json::from_str(s),
        _ => serde_json::from_str("null"), // Will fail with appropriate error
    }
}

// ========== TraceStore Core (Story #185) ==========

/// TraceStore primitive for structured reasoning traces
///
/// ## Design
///
/// TraceStore provides structured storage for agent reasoning traces.
/// It is a stateless facade over the Database engine, holding only
/// an `Arc<Database>` reference.
///
/// ## Performance Warning
///
/// Each trace write creates 3-4 secondary index entries:
/// - by-type index
/// - by-tag index (one per tag)
/// - by-parent index (if has parent)
/// - by-time index (hour bucket)
///
/// This write amplification is acceptable for debugging traces
/// (tens to hundreds per run) but NOT for high-volume telemetry.
///
/// ## Example
///
/// ```rust,ignore
/// use in_mem_primitives::{TraceStore, TraceType};
/// use in_mem_core::value::Value;
///
/// let ts = TraceStore::new(db.clone());
/// let run_id = RunId::new();
///
/// // Record a tool call trace
/// let trace_id = ts.record(
///     &run_id,
///     TraceType::ToolCall {
///         tool_name: "search".into(),
///         arguments: Value::Null,
///         result: None,
///         duration_ms: Some(42),
///     },
///     vec!["important".into()],
///     Value::Null,
/// )?;
///
/// // Query traces by type
/// let tool_calls = ts.query_by_type(&run_id, "ToolCall")?;
/// ```
#[derive(Clone)]
pub struct TraceStore {
    db: Arc<Database>,
}

impl TraceStore {
    /// Create new TraceStore instance
    pub fn new(db: Arc<Database>) -> Self {
        Self { db }
    }

    /// Get the underlying database reference
    pub fn database(&self) -> &Arc<Database> {
        &self.db
    }

    /// Build namespace for run-scoped operations
    fn namespace_for_run(&self, run_id: &RunId) -> Namespace {
        Namespace::for_run(*run_id)
    }

    /// Build key for trace by ID
    fn key_for(&self, run_id: &RunId, trace_id: &str) -> Key {
        Key::new_trace_with_id(self.namespace_for_run(run_id), trace_id)
    }

    // ========== Record Operations (Story #186) ==========

    /// Record a new trace
    ///
    /// Generates unique ID, writes trace and all secondary indices atomically.
    ///
    /// ## Returns
    /// The generated trace ID on success.
    ///
    /// ## Errors
    /// - `SerializationError` if trace cannot be serialized
    pub fn record(
        &self,
        run_id: &RunId,
        trace_type: TraceType,
        tags: Vec<String>,
        metadata: Value,
    ) -> Result<String> {
        self.record_with_options(run_id, None, trace_type, tags, metadata)
    }

    /// Record a child trace
    ///
    /// Parent must exist. Validates parent ID before recording.
    ///
    /// ## Returns
    /// The generated trace ID on success.
    ///
    /// ## Errors
    /// - `NotFound` if parent trace doesn't exist
    /// - `SerializationError` if trace cannot be serialized
    pub fn record_child(
        &self,
        run_id: &RunId,
        parent_id: &str,
        trace_type: TraceType,
        tags: Vec<String>,
        metadata: Value,
    ) -> Result<String> {
        self.record_with_options(
            run_id,
            Some(parent_id.to_string()),
            trace_type,
            tags,
            metadata,
        )
    }

    /// Record trace with full options
    ///
    /// Internal method that handles both root and child traces.
    fn record_with_options(
        &self,
        run_id: &RunId,
        parent_id: Option<String>,
        trace_type: TraceType,
        tags: Vec<String>,
        metadata: Value,
    ) -> Result<String> {
        let trace_id = Trace::generate_id();

        self.db.transaction(*run_id, |txn| {
            let ns = self.namespace_for_run(run_id);

            // Validate parent exists if provided
            if let Some(ref pid) = parent_id {
                let parent_key = Key::new_trace_with_id(ns.clone(), pid);
                if txn.get(&parent_key)?.is_none() {
                    return Err(Error::InvalidOperation(format!(
                        "Parent trace '{}' not found",
                        pid
                    )));
                }
            }

            let trace = Trace {
                id: trace_id.clone(),
                parent_id: parent_id.clone(),
                trace_type,
                timestamp: Trace::now(),
                tags: tags.clone(),
                metadata,
            };

            // Write primary trace
            let trace_key = Key::new_trace_with_id(ns.clone(), &trace_id);
            txn.put(trace_key, to_stored_value(&trace))?;

            // Write secondary indices (Story #187)
            Self::write_indices_internal(txn, &ns, &trace)?;

            Ok(trace_id)
        })
    }

    /// Get a trace by ID (FAST PATH)
    ///
    /// Bypasses full transaction overhead for read-only access.
    /// Uses direct snapshot read which maintains snapshot isolation.
    ///
    /// ## Returns
    /// - `Some(trace)` if found
    /// - `None` if not found
    ///
    /// ## Errors
    /// - `SerializationError` if trace cannot be deserialized
    pub fn get(&self, run_id: &RunId, trace_id: &str) -> Result<Option<Trace>> {
        use in_mem_core::traits::SnapshotView;

        let snapshot = self.db.storage().create_snapshot();
        let key = self.key_for(run_id, trace_id);

        match snapshot.get(&key)? {
            Some(vv) => {
                let trace: Trace = from_stored_value(&vv.value)
                    .map_err(|e| Error::SerializationError(e.to_string()))?;
                Ok(Some(trace))
            }
            None => Ok(None),
        }
    }

    /// Get a trace by ID (with full transaction)
    ///
    /// Use this when you need transaction semantics.
    pub fn get_in_transaction(&self, run_id: &RunId, trace_id: &str) -> Result<Option<Trace>> {
        self.db.transaction(*run_id, |txn| {
            let key = self.key_for(run_id, trace_id);
            match txn.get(&key)? {
                Some(v) => {
                    let trace: Trace = from_stored_value(&v)
                        .map_err(|e| Error::SerializationError(e.to_string()))?;
                    Ok(Some(trace))
                }
                None => Ok(None),
            }
        })
    }

    /// Check if a trace exists (FAST PATH)
    ///
    /// Uses direct snapshot read which maintains snapshot isolation.
    pub fn exists(&self, run_id: &RunId, trace_id: &str) -> Result<bool> {
        use in_mem_core::traits::SnapshotView;

        let snapshot = self.db.storage().create_snapshot();
        let key = self.key_for(run_id, trace_id);
        Ok(snapshot.get(&key)?.is_some())
    }

    // ========== Secondary Indices (Story #187) ==========

    /// Write all secondary indices for a trace
    ///
    /// Called atomically within the same transaction as the primary write.
    /// Creates indices for:
    /// - by-type: trace type name
    /// - by-tag: each tag (multiple entries)
    /// - by-parent: parent ID (if has parent)
    /// - by-time: hour bucket for range queries
    fn write_indices_internal(
        txn: &mut TransactionContext,
        ns: &Namespace,
        trace: &Trace,
    ) -> Result<()> {
        // Index by type
        let type_index_key = Key::new_trace_index(
            ns.clone(),
            "by-type",
            trace.trace_type.type_name(),
            &trace.id,
        );
        txn.put(type_index_key, Value::String(trace.id.clone()))?;

        // Index by each tag
        for tag in &trace.tags {
            let tag_index_key = Key::new_trace_index(ns.clone(), "by-tag", tag, &trace.id);
            txn.put(tag_index_key, Value::String(trace.id.clone()))?;
        }

        // Index by parent (if has parent)
        if let Some(ref parent_id) = trace.parent_id {
            let parent_index_key =
                Key::new_trace_index(ns.clone(), "by-parent", parent_id, &trace.id);
            txn.put(parent_index_key, Value::String(trace.id.clone()))?;
        }

        // Index by time (hour bucket for range queries)
        let hour_bucket = trace.timestamp / (3600 * 1000); // Hour since epoch
        let time_index_key =
            Key::new_trace_index(ns.clone(), "by-time", &hour_bucket.to_string(), &trace.id);
        txn.put(time_index_key, Value::String(trace.id.clone()))?;

        Ok(())
    }

    /// Scan an index and return trace IDs
    ///
    /// Used internally by query methods to find traces matching criteria.
    fn scan_index(
        &self,
        run_id: &RunId,
        index_type: &str,
        index_value: &str,
    ) -> Result<Vec<String>> {
        self.db.transaction(*run_id, |txn| {
            let ns = self.namespace_for_run(run_id);
            // Create prefix key - empty trace_id gives us the prefix to scan
            let prefix = Key::new_trace_index(ns, index_type, index_value, "");

            let results = txn.scan_prefix(&prefix)?;
            Ok(results
                .into_iter()
                .filter_map(|(_, v)| {
                    if let Value::String(s) = v {
                        Some(s)
                    } else {
                        None
                    }
                })
                .collect())
        })
    }

    // ========== Query Operations (Story #188) ==========

    /// Query traces by type
    ///
    /// Returns all traces matching the given type name.
    /// Uses the by-type index for efficient lookup.
    ///
    /// ## Example
    /// ```rust,ignore
    /// let tool_calls = ts.query_by_type(&run_id, "ToolCall")?;
    /// let decisions = ts.query_by_type(&run_id, "Decision")?;
    /// ```
    pub fn query_by_type(&self, run_id: &RunId, type_name: &str) -> Result<Vec<Trace>> {
        let ids = self.scan_index(run_id, "by-type", type_name)?;
        self.get_many(run_id, &ids)
    }

    /// Query traces by tag
    ///
    /// Returns all traces with the given tag.
    /// Uses the by-tag index for efficient lookup.
    ///
    /// ## Example
    /// ```rust,ignore
    /// let important = ts.query_by_tag(&run_id, "important")?;
    /// ```
    pub fn query_by_tag(&self, run_id: &RunId, tag: &str) -> Result<Vec<Trace>> {
        let ids = self.scan_index(run_id, "by-tag", tag)?;
        self.get_many(run_id, &ids)
    }

    /// Query traces in a time range
    ///
    /// Returns all traces with timestamps in [start_ms, end_ms).
    /// Uses the by-time index with hour buckets for efficient lookup.
    ///
    /// Note: Results are filtered to exact time range after index lookup.
    ///
    /// ## Arguments
    /// - `start_ms`: Start timestamp (inclusive, milliseconds since epoch)
    /// - `end_ms`: End timestamp (exclusive, milliseconds since epoch)
    pub fn query_by_time(&self, run_id: &RunId, start_ms: i64, end_ms: i64) -> Result<Vec<Trace>> {
        let start_hour = start_ms / (3600 * 1000);
        let end_hour = end_ms / (3600 * 1000);

        let mut all_ids = Vec::new();
        for hour in start_hour..=end_hour {
            let ids = self.scan_index(run_id, "by-time", &hour.to_string())?;
            all_ids.extend(ids);
        }

        // Filter to exact time range
        let traces = self.get_many(run_id, &all_ids)?;
        Ok(traces
            .into_iter()
            .filter(|t| t.timestamp >= start_ms && t.timestamp < end_ms)
            .collect())
    }

    /// Get children of a trace
    ///
    /// Returns all traces that have the given trace as their parent.
    /// Uses the by-parent index for efficient lookup.
    pub fn get_children(&self, run_id: &RunId, parent_id: &str) -> Result<Vec<Trace>> {
        let ids = self.scan_index(run_id, "by-parent", parent_id)?;
        self.get_many(run_id, &ids)
    }

    /// Get multiple traces by IDs
    ///
    /// Internal helper for query methods.
    fn get_many(&self, run_id: &RunId, ids: &[String]) -> Result<Vec<Trace>> {
        let mut traces = Vec::new();
        for id in ids {
            if let Some(trace) = self.get(run_id, id)? {
                traces.push(trace);
            }
        }
        Ok(traces)
    }

    // ========== Tree Reconstruction (Story #189) ==========

    /// Build a trace tree from a root trace
    ///
    /// Recursively fetches all descendants of the given trace
    /// and builds a tree structure for visualization.
    ///
    /// ## Returns
    /// - `Some(tree)` if root trace exists
    /// - `None` if root trace not found
    pub fn get_tree(&self, run_id: &RunId, root_id: &str) -> Result<Option<TraceTree>> {
        let root = match self.get(run_id, root_id)? {
            Some(t) => t,
            None => return Ok(None),
        };

        Ok(Some(self.build_tree(run_id, root)?))
    }

    /// Recursively build trace tree
    fn build_tree(&self, run_id: &RunId, trace: Trace) -> Result<TraceTree> {
        let children = self.get_children(run_id, &trace.id)?;

        let child_trees: Vec<TraceTree> = children
            .into_iter()
            .map(|c| self.build_tree(run_id, c))
            .collect::<Result<Vec<_>>>()?;

        Ok(TraceTree {
            trace,
            children: child_trees,
        })
    }

    /// Get all root traces (traces without parents)
    ///
    /// Returns all traces that don't have a parent_id set.
    /// Useful for finding entry points into trace hierarchies.
    pub fn get_roots(&self, run_id: &RunId) -> Result<Vec<Trace>> {
        self.db.transaction(*run_id, |txn| {
            let ns = self.namespace_for_run(run_id);
            // Scan all traces (empty string prefix matches all trace keys)
            let prefix = Key::new_trace_with_id(ns, "trace-");

            let results = txn.scan_prefix(&prefix)?;
            let mut roots = Vec::new();

            for (_, v) in results {
                let trace: Trace =
                    from_stored_value(&v).map_err(|e| Error::SerializationError(e.to_string()))?;
                if trace.parent_id.is_none() {
                    roots.push(trace);
                }
            }

            Ok(roots)
        })
    }

    /// List all traces for a run
    ///
    /// Returns all traces without filtering. Use query methods
    /// for filtered access.
    pub fn list(&self, run_id: &RunId) -> Result<Vec<Trace>> {
        self.db.transaction(*run_id, |txn| {
            let ns = self.namespace_for_run(run_id);
            let prefix = Key::new_trace_with_id(ns, "trace-");

            let results = txn.scan_prefix(&prefix)?;
            let mut traces = Vec::new();

            for (_, v) in results {
                let trace: Trace =
                    from_stored_value(&v).map_err(|e| Error::SerializationError(e.to_string()))?;
                traces.push(trace);
            }

            Ok(traces)
        })
    }

    /// Count traces for a run
    pub fn count(&self, run_id: &RunId) -> Result<usize> {
        Ok(self.list(run_id)?.len())
    }
}

// ========== TraceStoreExt Implementation (Story #190) ==========

impl TraceStoreExt for TransactionContext {
    fn trace_record(&mut self, trace_type: &str, metadata: Value) -> Result<String> {
        let trace_id = Trace::generate_id();
        let ns = Namespace::for_run(self.run_id);

        let trace = Trace {
            id: trace_id.clone(),
            parent_id: None,
            trace_type: TraceType::Custom {
                name: trace_type.to_string(),
                data: metadata.clone(),
            },
            timestamp: Trace::now(),
            tags: vec![],
            metadata,
        };

        // Write primary trace
        let key = Key::new_trace_with_id(ns.clone(), &trace_id);
        self.put(key, to_stored_value(&trace))?;

        // Write type index
        let type_index = Key::new_trace_index(ns.clone(), "by-type", trace_type, &trace_id);
        self.put(type_index, Value::String(trace_id.clone()))?;

        // Write time index
        let hour_bucket = trace.timestamp / (3600 * 1000);
        let time_index = Key::new_trace_index(ns, "by-time", &hour_bucket.to_string(), &trace_id);
        self.put(time_index, Value::String(trace_id.clone()))?;

        Ok(trace_id)
    }

    fn trace_record_child(
        &mut self,
        parent_id: &str,
        trace_type: &str,
        metadata: Value,
    ) -> Result<String> {
        let ns = Namespace::for_run(self.run_id);

        // Validate parent exists
        let parent_key = Key::new_trace_with_id(ns.clone(), parent_id);
        if self.get(&parent_key)?.is_none() {
            return Err(Error::InvalidOperation(format!(
                "Parent trace '{}' not found",
                parent_id
            )));
        }

        let trace_id = Trace::generate_id();

        let trace = Trace {
            id: trace_id.clone(),
            parent_id: Some(parent_id.to_string()),
            trace_type: TraceType::Custom {
                name: trace_type.to_string(),
                data: metadata.clone(),
            },
            timestamp: Trace::now(),
            tags: vec![],
            metadata,
        };

        // Write primary trace
        let key = Key::new_trace_with_id(ns.clone(), &trace_id);
        self.put(key, to_stored_value(&trace))?;

        // Write type index
        let type_index = Key::new_trace_index(ns.clone(), "by-type", trace_type, &trace_id);
        self.put(type_index, Value::String(trace_id.clone()))?;

        // Write parent index
        let parent_index = Key::new_trace_index(ns.clone(), "by-parent", parent_id, &trace_id);
        self.put(parent_index, Value::String(trace_id.clone()))?;

        // Write time index
        let hour_bucket = trace.timestamp / (3600 * 1000);
        let time_index = Key::new_trace_index(ns, "by-time", &hour_bucket.to_string(), &trace_id);
        self.put(time_index, Value::String(trace_id.clone()))?;

        Ok(trace_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn create_test_db() -> (Arc<Database>, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let db = Database::open(temp_dir.path()).unwrap();
        (Arc::new(db), temp_dir)
    }

    // ========== Story #185 Tests: TraceType ==========

    #[test]
    fn test_trace_type_tool_call() {
        let tt = TraceType::ToolCall {
            tool_name: "search".into(),
            arguments: Value::String("query".into()),
            result: Some(Value::I64(42)),
            duration_ms: Some(100),
        };
        assert_eq!(tt.type_name(), "ToolCall");
    }

    #[test]
    fn test_trace_type_decision() {
        let tt = TraceType::Decision {
            question: "Which approach?".into(),
            options: vec!["A".into(), "B".into()],
            chosen: "A".into(),
            reasoning: Some("Because it's faster".into()),
        };
        assert_eq!(tt.type_name(), "Decision");
    }

    #[test]
    fn test_trace_type_query() {
        let tt = TraceType::Query {
            query_type: "database".into(),
            query: "SELECT * FROM users".into(),
            results_count: Some(10),
        };
        assert_eq!(tt.type_name(), "Query");
    }

    #[test]
    fn test_trace_type_thought() {
        let tt = TraceType::Thought {
            content: "I should try approach A".into(),
            confidence: Some(0.85),
        };
        assert_eq!(tt.type_name(), "Thought");
    }

    #[test]
    fn test_trace_type_error() {
        let tt = TraceType::Error {
            error_type: "NetworkError".into(),
            message: "Connection timeout".into(),
            recoverable: true,
        };
        assert_eq!(tt.type_name(), "Error");
    }

    #[test]
    fn test_trace_type_custom() {
        let tt = TraceType::Custom {
            name: "MyCustomType".into(),
            data: Value::Bool(true),
        };
        assert_eq!(tt.type_name(), "MyCustomType");
    }

    #[test]
    fn test_trace_type_serialization() {
        let tt = TraceType::ToolCall {
            tool_name: "test".into(),
            arguments: Value::Null,
            result: None,
            duration_ms: None,
        };

        let json = serde_json::to_string(&tt).unwrap();
        let restored: TraceType = serde_json::from_str(&json).unwrap();
        assert_eq!(tt, restored);
    }

    // ========== Story #185 Tests: Trace ==========

    #[test]
    fn test_trace_generate_id() {
        let id1 = Trace::generate_id();
        let id2 = Trace::generate_id();

        assert!(id1.starts_with("trace-"));
        assert!(id2.starts_with("trace-"));
        assert_ne!(id1, id2); // Should be unique
    }

    #[test]
    fn test_trace_serialization() {
        let trace = Trace {
            id: "trace-123".into(),
            parent_id: None,
            trace_type: TraceType::Thought {
                content: "Thinking...".into(),
                confidence: Some(0.9),
            },
            timestamp: 1234567890,
            tags: vec!["important".into()],
            metadata: Value::Null,
        };

        let json = serde_json::to_string(&trace).unwrap();
        let restored: Trace = serde_json::from_str(&json).unwrap();
        assert_eq!(trace, restored);
    }

    #[test]
    fn test_trace_with_parent() {
        let trace = Trace {
            id: "trace-child".into(),
            parent_id: Some("trace-parent".into()),
            trace_type: TraceType::Thought {
                content: "Child thought".into(),
                confidence: None,
            },
            timestamp: 1234567890,
            tags: vec![],
            metadata: Value::Null,
        };

        assert_eq!(trace.parent_id, Some("trace-parent".into()));
    }

    // ========== Story #185 Tests: TraceStore Core ==========

    #[test]
    fn test_tracestore_new() {
        let (db, _temp) = create_test_db();
        let ts = TraceStore::new(db.clone());
        assert!(Arc::ptr_eq(ts.database(), &db));
    }

    // ========== Story #186 Tests: Record Operations ==========

    #[test]
    fn test_record_and_get() {
        let (db, _temp) = create_test_db();
        let ts = TraceStore::new(db);
        let run_id = RunId::new();

        let trace_id = ts
            .record(
                &run_id,
                TraceType::ToolCall {
                    tool_name: "search".into(),
                    arguments: Value::String("query".into()),
                    result: None,
                    duration_ms: None,
                },
                vec!["test".into()],
                Value::Null,
            )
            .unwrap();

        assert!(trace_id.starts_with("trace-"));

        let trace = ts.get(&run_id, &trace_id).unwrap().unwrap();
        assert_eq!(trace.id, trace_id);
        assert_eq!(trace.tags, vec!["test".to_string()]);
    }

    #[test]
    fn test_record_child() {
        let (db, _temp) = create_test_db();
        let ts = TraceStore::new(db);
        let run_id = RunId::new();

        // Create parent
        let parent_id = ts
            .record(
                &run_id,
                TraceType::Decision {
                    question: "What to do?".into(),
                    options: vec!["A".into(), "B".into()],
                    chosen: "A".into(),
                    reasoning: None,
                },
                vec![],
                Value::Null,
            )
            .unwrap();

        // Create child
        let child_id = ts
            .record_child(
                &run_id,
                &parent_id,
                TraceType::ToolCall {
                    tool_name: "execute_a".into(),
                    arguments: Value::Null,
                    result: None,
                    duration_ms: None,
                },
                vec![],
                Value::Null,
            )
            .unwrap();

        let child = ts.get(&run_id, &child_id).unwrap().unwrap();
        assert_eq!(child.parent_id, Some(parent_id));
    }

    #[test]
    fn test_record_child_parent_not_found() {
        let (db, _temp) = create_test_db();
        let ts = TraceStore::new(db);
        let run_id = RunId::new();

        let result = ts.record_child(
            &run_id,
            "nonexistent-parent",
            TraceType::Thought {
                content: "test".into(),
                confidence: None,
            },
            vec![],
            Value::Null,
        );

        assert!(result.is_err());
        match result {
            Err(Error::InvalidOperation(msg)) => {
                assert!(msg.contains("nonexistent-parent"));
            }
            _ => panic!("Expected InvalidOperation error"),
        }
    }

    #[test]
    fn test_exists() {
        let (db, _temp) = create_test_db();
        let ts = TraceStore::new(db);
        let run_id = RunId::new();

        let trace_id = ts
            .record(
                &run_id,
                TraceType::Thought {
                    content: "test".into(),
                    confidence: None,
                },
                vec![],
                Value::Null,
            )
            .unwrap();

        assert!(ts.exists(&run_id, &trace_id).unwrap());
        assert!(!ts.exists(&run_id, "nonexistent").unwrap());
    }

    // ========== Story #187 Tests: Secondary Indices ==========

    #[test]
    fn test_index_by_type() {
        let (db, _temp) = create_test_db();
        let ts = TraceStore::new(db);
        let run_id = RunId::new();

        // Create traces of different types
        ts.record(
            &run_id,
            TraceType::ToolCall {
                tool_name: "t1".into(),
                arguments: Value::Null,
                result: None,
                duration_ms: None,
            },
            vec![],
            Value::Null,
        )
        .unwrap();

        ts.record(
            &run_id,
            TraceType::ToolCall {
                tool_name: "t2".into(),
                arguments: Value::Null,
                result: None,
                duration_ms: None,
            },
            vec![],
            Value::Null,
        )
        .unwrap();

        ts.record(
            &run_id,
            TraceType::Decision {
                question: "q".into(),
                options: vec![],
                chosen: "a".into(),
                reasoning: None,
            },
            vec![],
            Value::Null,
        )
        .unwrap();

        let tool_calls = ts.query_by_type(&run_id, "ToolCall").unwrap();
        assert_eq!(tool_calls.len(), 2);

        let decisions = ts.query_by_type(&run_id, "Decision").unwrap();
        assert_eq!(decisions.len(), 1);
    }

    #[test]
    fn test_index_by_tag() {
        let (db, _temp) = create_test_db();
        let ts = TraceStore::new(db);
        let run_id = RunId::new();

        ts.record(
            &run_id,
            TraceType::Thought {
                content: "t1".into(),
                confidence: None,
            },
            vec!["important".into(), "review".into()],
            Value::Null,
        )
        .unwrap();

        ts.record(
            &run_id,
            TraceType::Thought {
                content: "t2".into(),
                confidence: None,
            },
            vec!["important".into()],
            Value::Null,
        )
        .unwrap();

        ts.record(
            &run_id,
            TraceType::Thought {
                content: "t3".into(),
                confidence: None,
            },
            vec!["other".into()],
            Value::Null,
        )
        .unwrap();

        let important = ts.query_by_tag(&run_id, "important").unwrap();
        assert_eq!(important.len(), 2);

        let review = ts.query_by_tag(&run_id, "review").unwrap();
        assert_eq!(review.len(), 1);
    }

    // ========== Story #188 Tests: Query Operations ==========

    #[test]
    fn test_query_by_time() {
        let (db, _temp) = create_test_db();
        let ts = TraceStore::new(db);
        let run_id = RunId::new();

        // Record a trace (will use current time)
        ts.record(
            &run_id,
            TraceType::Thought {
                content: "test".into(),
                confidence: None,
            },
            vec![],
            Value::Null,
        )
        .unwrap();

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;

        // Query for traces in the last hour
        let traces = ts
            .query_by_time(&run_id, now - 3600 * 1000, now + 3600 * 1000)
            .unwrap();
        assert_eq!(traces.len(), 1);

        // Query for traces in the past (should be empty)
        let old_traces = ts.query_by_time(&run_id, 0, now - 3600 * 1000).unwrap();
        assert_eq!(old_traces.len(), 0);
    }

    #[test]
    fn test_get_children() {
        let (db, _temp) = create_test_db();
        let ts = TraceStore::new(db);
        let run_id = RunId::new();

        // Create parent
        let parent_id = ts
            .record(
                &run_id,
                TraceType::Decision {
                    question: "Root".into(),
                    options: vec![],
                    chosen: "A".into(),
                    reasoning: None,
                },
                vec![],
                Value::Null,
            )
            .unwrap();

        // Create children
        ts.record_child(
            &run_id,
            &parent_id,
            TraceType::Thought {
                content: "Child 1".into(),
                confidence: None,
            },
            vec![],
            Value::Null,
        )
        .unwrap();

        ts.record_child(
            &run_id,
            &parent_id,
            TraceType::Thought {
                content: "Child 2".into(),
                confidence: None,
            },
            vec![],
            Value::Null,
        )
        .unwrap();

        let children = ts.get_children(&run_id, &parent_id).unwrap();
        assert_eq!(children.len(), 2);
    }

    // ========== Story #189 Tests: Tree Reconstruction ==========

    #[test]
    fn test_get_tree() {
        let (db, _temp) = create_test_db();
        let ts = TraceStore::new(db);
        let run_id = RunId::new();

        // Create a tree structure
        let root_id = ts
            .record(
                &run_id,
                TraceType::Decision {
                    question: "Root".into(),
                    options: vec![],
                    chosen: "A".into(),
                    reasoning: None,
                },
                vec![],
                Value::Null,
            )
            .unwrap();

        let child1_id = ts
            .record_child(
                &run_id,
                &root_id,
                TraceType::Thought {
                    content: "Child 1".into(),
                    confidence: None,
                },
                vec![],
                Value::Null,
            )
            .unwrap();

        ts.record_child(
            &run_id,
            &child1_id,
            TraceType::ToolCall {
                tool_name: "grandchild".into(),
                arguments: Value::Null,
                result: None,
                duration_ms: None,
            },
            vec![],
            Value::Null,
        )
        .unwrap();

        // Get tree
        let tree = ts.get_tree(&run_id, &root_id).unwrap().unwrap();
        assert_eq!(tree.trace.id, root_id);
        assert_eq!(tree.children.len(), 1);
        assert_eq!(tree.children[0].children.len(), 1);
    }

    #[test]
    fn test_get_tree_not_found() {
        let (db, _temp) = create_test_db();
        let ts = TraceStore::new(db);
        let run_id = RunId::new();

        let tree = ts.get_tree(&run_id, "nonexistent").unwrap();
        assert!(tree.is_none());
    }

    #[test]
    fn test_get_roots() {
        let (db, _temp) = create_test_db();
        let ts = TraceStore::new(db);
        let run_id = RunId::new();

        // Create root traces
        let root1_id = ts
            .record(
                &run_id,
                TraceType::Decision {
                    question: "Root 1".into(),
                    options: vec![],
                    chosen: "A".into(),
                    reasoning: None,
                },
                vec![],
                Value::Null,
            )
            .unwrap();

        ts.record(
            &run_id,
            TraceType::Decision {
                question: "Root 2".into(),
                options: vec![],
                chosen: "B".into(),
                reasoning: None,
            },
            vec![],
            Value::Null,
        )
        .unwrap();

        // Create child (not a root)
        ts.record_child(
            &run_id,
            &root1_id,
            TraceType::Thought {
                content: "Child".into(),
                confidence: None,
            },
            vec![],
            Value::Null,
        )
        .unwrap();

        let roots = ts.get_roots(&run_id).unwrap();
        assert_eq!(roots.len(), 2);
        assert!(roots.iter().all(|r| r.parent_id.is_none()));
    }

    #[test]
    fn test_list_and_count() {
        let (db, _temp) = create_test_db();
        let ts = TraceStore::new(db);
        let run_id = RunId::new();

        assert_eq!(ts.count(&run_id).unwrap(), 0);

        ts.record(
            &run_id,
            TraceType::Thought {
                content: "t1".into(),
                confidence: None,
            },
            vec![],
            Value::Null,
        )
        .unwrap();

        ts.record(
            &run_id,
            TraceType::Thought {
                content: "t2".into(),
                confidence: None,
            },
            vec![],
            Value::Null,
        )
        .unwrap();

        assert_eq!(ts.count(&run_id).unwrap(), 2);
        assert_eq!(ts.list(&run_id).unwrap().len(), 2);
    }

    // ========== Story #190 Tests: TraceStoreExt ==========

    #[test]
    fn test_trace_ext_record() {
        let (db, _temp) = create_test_db();
        let run_id = RunId::new();

        let trace_id = db
            .transaction(run_id, |txn| {
                txn.trace_record("MyOperation", Value::String("test data".into()))
            })
            .unwrap();

        assert!(trace_id.starts_with("trace-"));

        // Verify trace exists
        let ts = TraceStore::new(db);
        let trace = ts.get(&run_id, &trace_id).unwrap().unwrap();
        assert!(
            matches!(trace.trace_type, TraceType::Custom { name, .. } if name == "MyOperation")
        );
    }

    #[test]
    fn test_trace_ext_record_child() {
        let (db, _temp) = create_test_db();
        let run_id = RunId::new();

        // Create parent
        let parent_id = db
            .transaction(run_id, |txn| txn.trace_record("Parent", Value::Null))
            .unwrap();

        // Create child
        let child_id = db
            .transaction(run_id, |txn| {
                txn.trace_record_child(&parent_id, "Child", Value::Null)
            })
            .unwrap();

        // Verify relationship
        let ts = TraceStore::new(db);
        let child = ts.get(&run_id, &child_id).unwrap().unwrap();
        assert_eq!(child.parent_id, Some(parent_id));
    }

    #[test]
    fn test_trace_ext_record_child_parent_not_found() {
        let (db, _temp) = create_test_db();
        let run_id = RunId::new();

        let result = db.transaction(run_id, |txn| {
            txn.trace_record_child("nonexistent", "Child", Value::Null)
        });

        assert!(result.is_err());
    }

    // ========== Run Isolation Tests ==========

    #[test]
    fn test_run_isolation() {
        let (db, _temp) = create_test_db();
        let ts = TraceStore::new(db);
        let run1 = RunId::new();
        let run2 = RunId::new();

        // Create trace in run1
        let trace_id = ts
            .record(
                &run1,
                TraceType::Thought {
                    content: "run1 trace".into(),
                    confidence: None,
                },
                vec![],
                Value::Null,
            )
            .unwrap();

        // Should exist in run1
        assert!(ts.get(&run1, &trace_id).unwrap().is_some());

        // Should NOT exist in run2
        assert!(ts.get(&run2, &trace_id).unwrap().is_none());
    }

    // ========== Edge Cases ==========

    #[test]
    fn test_multiple_tags() {
        let (db, _temp) = create_test_db();
        let ts = TraceStore::new(db);
        let run_id = RunId::new();

        ts.record(
            &run_id,
            TraceType::Thought {
                content: "multi-tag".into(),
                confidence: None,
            },
            vec!["tag1".into(), "tag2".into(), "tag3".into()],
            Value::Null,
        )
        .unwrap();

        // Should be findable by all tags
        assert_eq!(ts.query_by_tag(&run_id, "tag1").unwrap().len(), 1);
        assert_eq!(ts.query_by_tag(&run_id, "tag2").unwrap().len(), 1);
        assert_eq!(ts.query_by_tag(&run_id, "tag3").unwrap().len(), 1);
    }

    #[test]
    fn test_empty_tags() {
        let (db, _temp) = create_test_db();
        let ts = TraceStore::new(db);
        let run_id = RunId::new();

        ts.record(
            &run_id,
            TraceType::Thought {
                content: "no tags".into(),
                confidence: None,
            },
            vec![],
            Value::Null,
        )
        .unwrap();

        // Should still be queryable by type
        assert_eq!(ts.query_by_type(&run_id, "Thought").unwrap().len(), 1);
    }

    #[test]
    fn test_custom_type_indexing() {
        let (db, _temp) = create_test_db();
        let ts = TraceStore::new(db);
        let run_id = RunId::new();

        ts.record(
            &run_id,
            TraceType::Custom {
                name: "MyCustomType".into(),
                data: Value::Bool(true),
            },
            vec![],
            Value::Null,
        )
        .unwrap();

        // Should be queryable by custom type name
        let results = ts.query_by_type(&run_id, "MyCustomType").unwrap();
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn test_deep_tree() {
        let (db, _temp) = create_test_db();
        let ts = TraceStore::new(db);
        let run_id = RunId::new();

        // Create a chain of 5 traces
        let root_id = ts
            .record(
                &run_id,
                TraceType::Thought {
                    content: "level 0".into(),
                    confidence: None,
                },
                vec![],
                Value::Null,
            )
            .unwrap();

        let mut parent_id = root_id.clone();
        for i in 1..5 {
            parent_id = ts
                .record_child(
                    &run_id,
                    &parent_id,
                    TraceType::Thought {
                        content: format!("level {}", i),
                        confidence: None,
                    },
                    vec![],
                    Value::Null,
                )
                .unwrap();
        }

        // Get tree and verify depth
        let tree = ts.get_tree(&run_id, &root_id).unwrap().unwrap();

        fn count_depth(tree: &TraceTree) -> usize {
            if tree.children.is_empty() {
                1
            } else {
                1 + tree.children.iter().map(count_depth).max().unwrap_or(0)
            }
        }

        assert_eq!(count_depth(&tree), 5);
    }

    // ========== Fast Path Tests (Story #238) ==========

    #[test]
    fn test_fast_get_returns_correct_value() {
        let (db, _temp) = create_test_db();
        let ts = TraceStore::new(db);
        let run_id = RunId::new();

        let trace_id = ts
            .record(
                &run_id,
                TraceType::Thought {
                    content: "test content".into(),
                    confidence: Some(0.9),
                },
                vec!["important".into()],
                Value::Null,
            )
            .unwrap();

        let trace = ts.get(&run_id, &trace_id).unwrap().unwrap();
        assert_eq!(trace.id, trace_id);
        assert_eq!(trace.tags, vec!["important".to_string()]);
    }

    #[test]
    fn test_fast_get_returns_none_for_missing() {
        let (db, _temp) = create_test_db();
        let ts = TraceStore::new(db);
        let run_id = RunId::new();

        let trace = ts.get(&run_id, "nonexistent").unwrap();
        assert!(trace.is_none());
    }

    #[test]
    fn test_fast_get_equals_transaction_get() {
        let (db, _temp) = create_test_db();
        let ts = TraceStore::new(db);
        let run_id = RunId::new();

        let trace_id = ts
            .record(
                &run_id,
                TraceType::ToolCall {
                    tool_name: "search".into(),
                    arguments: Value::String("query".into()),
                    result: None,
                    duration_ms: None,
                },
                vec![],
                Value::Null,
            )
            .unwrap();

        let fast = ts.get(&run_id, &trace_id).unwrap();
        let txn = ts.get_in_transaction(&run_id, &trace_id).unwrap();

        assert_eq!(fast, txn);
    }

    #[test]
    fn test_fast_exists_uses_fast_path() {
        let (db, _temp) = create_test_db();
        let ts = TraceStore::new(db);
        let run_id = RunId::new();

        assert!(!ts.exists(&run_id, "nonexistent").unwrap());

        let trace_id = ts
            .record(
                &run_id,
                TraceType::Thought {
                    content: "test".into(),
                    confidence: None,
                },
                vec![],
                Value::Null,
            )
            .unwrap();

        assert!(ts.exists(&run_id, &trace_id).unwrap());
    }

    #[test]
    fn test_fast_get_run_isolation() {
        let (db, _temp) = create_test_db();
        let ts = TraceStore::new(db);
        let run1 = RunId::new();
        let run2 = RunId::new();

        let trace_id = ts
            .record(
                &run1,
                TraceType::Thought {
                    content: "run1 trace".into(),
                    confidence: None,
                },
                vec![],
                Value::Null,
            )
            .unwrap();

        // Should exist in run1
        assert!(ts.get(&run1, &trace_id).unwrap().is_some());

        // Should NOT exist in run2
        assert!(ts.get(&run2, &trace_id).unwrap().is_none());
    }
}
