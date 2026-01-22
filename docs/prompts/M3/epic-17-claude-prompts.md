# Epic 17: TraceStore Primitive - Implementation Prompts

**Epic Goal**: Structured reasoning traces with indexing.

**GitHub Issue**: [#163](https://github.com/anibjoshi/in-mem/issues/163)
**Status**: Ready to begin (after Epic 13)
**Dependencies**: Epic 13 (Primitives Foundation) complete

---

## AUTHORITATIVE SPECIFICATIONS - READ THESE FIRST

**`docs/architecture/M3_ARCHITECTURE.md` is the GOSPEL for ALL M3 implementation.**

Before starting ANY story in this epic, read:
- Section 7: TraceStore Primitive
- Section 7.5: Secondary Indices
- Section 11.4: Index Consistency Contract

See `docs/prompts/M3_PROMPT_HEADER.md` for complete guidelines.

---

## Epic 17 Overview

### Performance Warning

**TraceStore is optimized for DEBUGGABILITY, not throughput.**

Each trace write creates 3-4 secondary index entries (write amplification):
- by-type index
- by-tag index (per tag)
- by-parent index
- by-time index

**Designed for**: Reasoning traces (tens to hundreds per run)
**NOT designed for**: Telemetry (thousands per second)

### Scope
- TraceStore struct as stateless facade
- Trace and TraceType structures
- Record operations with ID generation
- Parent-child relationships for nested traces
- Secondary indices: by-type, by-tag, by-parent, by-time
- Query operations using indices
- Tree reconstruction

### Success Criteria
- [ ] TraceStore struct implemented with `Arc<Database>` reference
- [ ] TraceType enum with all variants
- [ ] Trace struct with id, parent_id, trace_type, timestamp, tags, metadata
- [ ] `record()` stores trace with auto-generated ID
- [ ] `record_child()` stores trace with parent reference
- [ ] Secondary indices written atomically with trace
- [ ] `query_by_type()`, `query_by_tag()`, `query_by_time()` use indices
- [ ] `get_tree()` recursively builds TraceTree
- [ ] Parent existence validated for child traces
- [ ] All unit tests pass (>95% coverage)

### Component Breakdown
- **Story #185**: TraceStore Core & TraceType Structures - BLOCKS others
- **Story #186**: TraceStore Record Operations
- **Story #187**: TraceStore Secondary Indices
- **Story #188**: TraceStore Query Operations
- **Story #189**: TraceStore Tree Reconstruction
- **Story #190**: TraceStoreExt Transaction Extension

---

## Dependency Graph

```
Phase 1 (Sequential):
  Story #185 (TraceStore Core)
    └─> BLOCKS #186

Phase 2 (Parallel - 2 Claudes):
  Story #186 (Record Operations)
  Story #187 (Secondary Indices)
    └─> Both depend on #185

Phase 3 (Parallel - 2 Claudes after #187):
  Story #188 (Query Operations)
  Story #189 (Tree Reconstruction)
    └─> Both depend on #187

Phase 4 (Sequential):
  Story #190 (TraceStoreExt)
    └─> Depends on all previous stories
```

---

## Story #185: TraceStore Core & TraceType Structures

**GitHub Issue**: [#185](https://github.com/anibjoshi/in-mem/issues/185)
**Estimated Time**: 4 hours
**Dependencies**: Epic 13 complete
**Blocks**: Story #186

### Start Story

```bash
/opt/homebrew/bin/gh issue view 185
./scripts/start-story.sh 17 185 tracestore-core
```

### Implementation

Create `crates/primitives/src/trace.rs`:

```rust
//! TraceStore: Structured reasoning traces with indexing
//!
//! ## Performance Warning
//!
//! TraceStore is optimized for DEBUGGABILITY, not ingestion throughput.
//! Each trace creates 3-4 secondary index entries (write amplification).
//!
//! Designed for: reasoning traces (tens to hundreds per run)
//! NOT for: telemetry (thousands per second)

use std::sync::Arc;
use serde::{Serialize, Deserialize};
use in_mem_engine::Database;
use in_mem_core::{Key, Namespace, RunId, Value, Result};

/// Types of reasoning traces
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum TraceType {
    /// External tool invocation
    ToolCall {
        tool_name: String,
        arguments: Value,
        result: Option<Value>,
        duration_ms: Option<u64>,
    },
    /// Decision point with options
    Decision {
        question: String,
        options: Vec<String>,
        chosen: String,
        reasoning: Option<String>,
    },
    /// Information query
    Query {
        query_type: String,
        query: String,
        results_count: Option<u32>,
    },
    /// Internal reasoning
    Thought {
        content: String,
        confidence: Option<f64>,
    },
    /// Error occurrence
    Error {
        error_type: String,
        message: String,
        recoverable: bool,
    },
    /// User-defined trace type
    Custom {
        name: String,
        data: Value,
    },
}

impl TraceType {
    /// Get the type name for indexing
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

/// A reasoning trace entry
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Trace {
    /// Unique trace ID
    pub id: String,
    /// Parent trace ID (if nested)
    pub parent_id: Option<String>,
    /// Type of trace
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
    pub fn generate_id() -> String {
        format!("trace-{}", uuid::Uuid::new_v4())
    }
}

/// TraceStore primitive for structured reasoning traces
///
/// PERFORMANCE WARNING: Write amplification due to secondary indices.
/// Use for debugging, not high-volume telemetry.
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
        Namespace::for_run(run_id)
    }

    /// Build key for trace
    fn key_for(&self, run_id: &RunId, trace_id: &str) -> Key {
        Key::new_trace(self.namespace_for_run(run_id), trace_id)
    }
}
```

### Tests

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_trace_type_names() {
        assert_eq!(TraceType::ToolCall {
            tool_name: "test".into(),
            arguments: Value::Null,
            result: None,
            duration_ms: None,
        }.type_name(), "ToolCall");

        assert_eq!(TraceType::Decision {
            question: "q".into(),
            options: vec![],
            chosen: "a".into(),
            reasoning: None,
        }.type_name(), "Decision");

        assert_eq!(TraceType::Custom {
            name: "MyType".into(),
            data: Value::Null,
        }.type_name(), "MyType");
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
}
```

### Complete Story

```bash
./scripts/complete-story.sh 185
```

---

## Story #186: TraceStore Record Operations

**GitHub Issue**: [#186](https://github.com/anibjoshi/in-mem/issues/186)
**Estimated Time**: 4 hours
**Dependencies**: Story #185

### Implementation

```rust
impl TraceStore {
    /// Record a new trace
    ///
    /// Generates unique ID, validates parent if provided, writes trace
    /// and all secondary indices atomically.
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
    pub fn record_child(
        &self,
        run_id: &RunId,
        parent_id: &str,
        trace_type: TraceType,
        tags: Vec<String>,
        metadata: Value,
    ) -> Result<String> {
        self.record_with_options(run_id, Some(parent_id.to_string()), trace_type, tags, metadata)
    }

    /// Record trace with full options
    pub fn record_with_options(
        &self,
        run_id: &RunId,
        parent_id: Option<String>,
        trace_type: TraceType,
        tags: Vec<String>,
        metadata: Value,
    ) -> Result<String> {
        let trace_id = Trace::generate_id();

        self.db.transaction(run_id, |txn| {
            let ns = self.namespace_for_run(run_id);

            // Validate parent exists if provided
            if let Some(ref pid) = parent_id {
                let parent_key = Key::new_trace(ns.clone(), pid);
                if txn.get(&parent_key)?.is_none() {
                    return Err(Error::NotFound(format!("Parent trace '{}' not found", pid)));
                }
            }

            let timestamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis() as i64;

            let trace = Trace {
                id: trace_id.clone(),
                parent_id: parent_id.clone(),
                trace_type: trace_type.clone(),
                timestamp,
                tags: tags.clone(),
                metadata,
            };

            // Write primary trace
            let trace_key = Key::new_trace(ns.clone(), &trace_id);
            txn.put(trace_key, Value::from_json(serde_json::to_value(&trace)?)?)?;

            // Write indices (Story #187)
            self.write_indices(txn, &ns, &trace)?;

            Ok(trace_id)
        })
    }

    /// Get a trace by ID
    pub fn get(&self, run_id: &RunId, trace_id: &str) -> Result<Option<Trace>> {
        self.db.transaction(run_id, |txn| {
            let key = self.key_for(run_id, trace_id);
            match txn.get(&key)? {
                Some(v) => Ok(Some(serde_json::from_value(v.into_json()?)?)),
                None => Ok(None),
            }
        })
    }
}
```

### Complete Story

```bash
./scripts/complete-story.sh 186
```

---

## Story #187: TraceStore Secondary Indices

**GitHub Issue**: [#187](https://github.com/anibjoshi/in-mem/issues/187)
**Estimated Time**: 5 hours
**Dependencies**: Story #185
**Blocks**: Stories #188, #189

### Implementation

```rust
impl TraceStore {
    /// Write all secondary indices for a trace
    ///
    /// Called atomically within the same transaction as the primary write.
    fn write_indices(
        &self,
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
            let tag_index_key = Key::new_trace_index(
                ns.clone(),
                "by-tag",
                tag,
                &trace.id,
            );
            txn.put(tag_index_key, Value::String(trace.id.clone()))?;
        }

        // Index by parent (if has parent)
        if let Some(ref parent_id) = trace.parent_id {
            let parent_index_key = Key::new_trace_index(
                ns.clone(),
                "by-parent",
                parent_id,
                &trace.id,
            );
            txn.put(parent_index_key, Value::String(trace.id.clone()))?;
        }

        // Index by time (hour bucket for range queries)
        let hour_bucket = trace.timestamp / (3600 * 1000);  // Hour since epoch
        let time_index_key = Key::new_trace_index(
            ns.clone(),
            "by-time",
            &hour_bucket.to_string(),
            &trace.id,
        );
        txn.put(time_index_key, Value::String(trace.id.clone()))?;

        Ok(())
    }

    /// Scan an index and return trace IDs
    fn scan_index(
        &self,
        run_id: &RunId,
        index_type: &str,
        index_value: &str,
    ) -> Result<Vec<String>> {
        self.db.transaction(run_id, |txn| {
            let ns = self.namespace_for_run(run_id);
            let prefix = Key::new_trace_index(ns, index_type, index_value, "");

            let results = txn.scan_prefix(&prefix)?;
            Ok(results
                .into_iter()
                .filter_map(|(_, v)| v.as_string().map(|s| s.to_string()))
                .collect())
        })
    }
}
```

### Complete Story

```bash
./scripts/complete-story.sh 187
```

---

## Story #188: TraceStore Query Operations

**GitHub Issue**: [#188](https://github.com/anibjoshi/in-mem/issues/188)
**Estimated Time**: 4 hours
**Dependencies**: Story #187

### Implementation

```rust
impl TraceStore {
    /// Query traces by type
    pub fn query_by_type(&self, run_id: &RunId, type_name: &str) -> Result<Vec<Trace>> {
        let ids = self.scan_index(run_id, "by-type", type_name)?;
        self.get_many(run_id, &ids)
    }

    /// Query traces by tag
    pub fn query_by_tag(&self, run_id: &RunId, tag: &str) -> Result<Vec<Trace>> {
        let ids = self.scan_index(run_id, "by-tag", tag)?;
        self.get_many(run_id, &ids)
    }

    /// Query traces in a time range
    pub fn query_by_time(
        &self,
        run_id: &RunId,
        start_ms: i64,
        end_ms: i64,
    ) -> Result<Vec<Trace>> {
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
    pub fn get_children(&self, run_id: &RunId, parent_id: &str) -> Result<Vec<Trace>> {
        let ids = self.scan_index(run_id, "by-parent", parent_id)?;
        self.get_many(run_id, &ids)
    }

    /// Get multiple traces by IDs
    fn get_many(&self, run_id: &RunId, ids: &[String]) -> Result<Vec<Trace>> {
        let mut traces = Vec::new();
        for id in ids {
            if let Some(trace) = self.get(run_id, id)? {
                traces.push(trace);
            }
        }
        Ok(traces)
    }
}
```

### Complete Story

```bash
./scripts/complete-story.sh 188
```

---

## Story #189: TraceStore Tree Reconstruction

**GitHub Issue**: [#189](https://github.com/anibjoshi/in-mem/issues/189)
**Estimated Time**: 4 hours
**Dependencies**: Story #187

### Implementation

```rust
/// A trace with its children
#[derive(Debug, Clone)]
pub struct TraceTree {
    pub trace: Trace,
    pub children: Vec<TraceTree>,
}

impl TraceStore {
    /// Build a trace tree from a root trace
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
    pub fn get_roots(&self, run_id: &RunId) -> Result<Vec<Trace>> {
        self.db.transaction(run_id, |txn| {
            let ns = self.namespace_for_run(run_id);
            let prefix = Key::new_trace(ns, "");

            let results = txn.scan_prefix(&prefix)?;
            let mut roots = Vec::new();

            for (_, v) in results {
                let trace: Trace = serde_json::from_value(v.into_json()?)?;
                if trace.parent_id.is_none() {
                    roots.push(trace);
                }
            }

            Ok(roots)
        })
    }
}
```

### Complete Story

```bash
./scripts/complete-story.sh 189
```

---

## Story #190: TraceStoreExt Transaction Extension

**GitHub Issue**: [#190](https://github.com/anibjoshi/in-mem/issues/190)
**Estimated Time**: 3 hours
**Dependencies**: Stories #185-#189

### Implementation

```rust
use crate::extensions::TraceStoreExt;

impl TraceStoreExt for TransactionContext {
    fn trace_record(&mut self, trace_type: &str, metadata: Value) -> Result<String> {
        let trace_id = Trace::generate_id();
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;

        let trace = Trace {
            id: trace_id.clone(),
            parent_id: None,
            trace_type: TraceType::Custom {
                name: trace_type.to_string(),
                data: metadata.clone(),
            },
            timestamp,
            tags: vec![],
            metadata,
        };

        let key = Key::new_trace(self.namespace().clone(), &trace_id);
        self.put(key, Value::from_json(serde_json::to_value(&trace)?)?)?;

        // Write type index
        let type_index = Key::new_trace_index(
            self.namespace().clone(),
            "by-type",
            trace_type,
            &trace_id,
        );
        self.put(type_index, Value::String(trace_id.clone()))?;

        Ok(trace_id)
    }

    fn trace_record_child(
        &mut self,
        parent_id: &str,
        trace_type: &str,
        metadata: Value,
    ) -> Result<String> {
        // Validate parent exists
        let parent_key = Key::new_trace(self.namespace().clone(), parent_id);
        if self.get(&parent_key)?.is_none() {
            return Err(Error::NotFound(format!("Parent trace '{}' not found", parent_id)));
        }

        let trace_id = Trace::generate_id();
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;

        let trace = Trace {
            id: trace_id.clone(),
            parent_id: Some(parent_id.to_string()),
            trace_type: TraceType::Custom {
                name: trace_type.to_string(),
                data: metadata.clone(),
            },
            timestamp,
            tags: vec![],
            metadata,
        };

        let key = Key::new_trace(self.namespace().clone(), &trace_id);
        self.put(key, Value::from_json(serde_json::to_value(&trace)?)?)?;

        // Write indices
        let type_index = Key::new_trace_index(
            self.namespace().clone(),
            "by-type",
            trace_type,
            &trace_id,
        );
        self.put(type_index, Value::String(trace_id.clone()))?;

        let parent_index = Key::new_trace_index(
            self.namespace().clone(),
            "by-parent",
            parent_id,
            &trace_id,
        );
        self.put(parent_index, Value::String(trace_id.clone()))?;

        Ok(trace_id)
    }
}
```

Update `crates/primitives/src/lib.rs`:

```rust
pub mod trace;
pub use trace::{TraceStore, Trace, TraceType, TraceTree};
```

### Complete Story

```bash
./scripts/complete-story.sh 190
```

---

## Epic 17 Completion Checklist

### Verify Deliverables

- [ ] TraceStore struct is stateless
- [ ] TraceType enum has all variants
- [ ] Trace struct has all required fields
- [ ] record() and record_child() work
- [ ] Secondary indices written atomically
- [ ] Query operations use indices correctly
- [ ] Tree reconstruction works
- [ ] Parent validation enforced
- [ ] All tests pass

### Merge and Close

```bash
git checkout develop
git merge --no-ff epic-17-tracestore-primitive -m "Epic 17: TraceStore Primitive

Complete:
- TraceStore stateless facade
- TraceType enum (ToolCall, Decision, Query, Thought, Error, Custom)
- Trace structure with parent-child relationships
- Secondary indices (by-type, by-tag, by-parent, by-time)
- Query operations using indices
- Tree reconstruction
- TraceStoreExt transaction extension

PERFORMANCE WARNING: Write amplification (3-4 index entries per trace).
Designed for debuggability, not high-volume telemetry.

Stories: #185, #186, #187, #188, #189, #190
"

/opt/homebrew/bin/gh issue close 163 --comment "Epic 17: TraceStore Primitive - COMPLETE"
```

---

## Summary

Epic 17 implements the TraceStore primitive - structured reasoning traces with indexing. Key design decisions:
- Write amplification is acceptable for debuggability
- Parent existence validated for child traces
- Indices enable efficient queries without full scans
