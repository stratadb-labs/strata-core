# TraceStore Defects and Gaps

> Consolidated from architecture review, primitive vs substrate analysis, and reasoning trace best practices.
> Source: `crates/api/src/substrate/trace.rs` and `crates/primitives/src/trace.rs`

## Summary

| Category | Count | Priority |
|----------|-------|----------|
| Stubbed APIs | 2 | P0 |
| Hidden Primitive Features | 4 | P0 |
| Missing Table Stakes APIs | 5 | P0-P1 |
| Type System Degradation | 3 | P1 |
| API Design Issues | 3 | P1 |
| World-Class Features | 6 | P1-P2 |
| **Total Issues** | **23** | |

---

## What is TraceStore?

TraceStore is an **observability primitive for agent reasoning traces**, NOT a high-volume telemetry system.

**Design Philosophy:**
```
Designed for: reasoning traces (tens to hundreds per run)
NOT designed for: telemetry (thousands per second)
Each trace creates 3-4 secondary index entries (write amplification)
```

**Purpose:**
- Record structured agent reasoning steps (thoughts, decisions, tool calls)
- Support hierarchical parent-child relationships (trace trees)
- Enable debugging and trace visualization
- Support efficient querying by type, tag, parent, time

**Data Model:**
- Tree structure (forest with multiple roots)
- Run-scoped (all traces isolated by RunId)
- Append-only (no modifications after creation)
- Rich type system with predefined + custom types

---

## Current Substrate API (7 methods, 2 stubbed)

```rust
// Working
fn trace_create(run, type, parent_id?, content, tags) -> (id, Version);
fn trace_get(run, id) -> Option<Versioned<TraceEntry>>;
fn trace_list(run, type?, parent_id?, tag?, limit?, before?) -> Vec<Versioned<TraceEntry>>;
fn trace_children(run, parent_id) -> Vec<Versioned<TraceEntry>>;
fn trace_tree(run, root_id) -> Vec<Versioned<TraceEntry>>;  // Flattened

// STUBBED (not implemented)
fn trace_create_with_id(run, id, ...) -> Version;  // Always fails
fn trace_update_tags(run, id, add, remove) -> Version;  // Always fails
```

---

## Part 1: Stubbed APIs (P0)

### Stub 1: `trace_create_with_id` - Custom ID Assignment

**Priority:** P0

**Current State:**
```rust
fn trace_create_with_id(...) -> StrataResult<Version> {
    Err(ApiError::NotSupported("trace_create_with_id not supported".into()))
}
```

**Why Critical:**
- Cannot create deterministic trace IDs for replay/testing
- Cannot link to external systems with known IDs
- Primitive generates UUIDs, substrate could allow override

**Fix:** Pass ID to primitive instead of letting it generate

---

### Stub 2: `trace_update_tags` - Tag Modification

**Priority:** P0 - API promises something it can't deliver

**Current State:**
```rust
fn trace_update_tags(...) -> StrataResult<Version> {
    Err(ApiError::NotSupported("trace_update_tags not supported".into()))
}
```

**Why Critical:**
- API is defined but always fails
- Users expect it to work based on trait signature
- Facade silently swallows the error (worse!)

**Options:**
1. Implement tag updates in primitive (break append-only)
2. Remove from trait (breaking change)
3. Document clearly as unsupported

---

## Part 2: Hidden Primitive Features (P0)

### Gap 1: `trace_query_by_time` - Time Range Queries

**Priority:** P0

**What Primitive Has:**
```rust
fn query_by_time(&self, run_id: &RunId, start_ms: i64, end_ms: i64) -> Result<Vec<Trace>>;
// Uses hour-bucket index for efficient range queries
```

**What Substrate Exposes:** Nothing

**Why Critical:**
- "Show me what happened in the last 5 minutes" is fundamental
- Debugging requires time-based filtering
- Hour-bucket indices exist but aren't accessible

**Proposed Substrate API:**
```rust
fn trace_query_by_time(&self, run: &ApiRunId, start_time: i64, end_time: i64,
    limit: Option<u64>) -> StrataResult<Vec<Versioned<TraceEntry>>>;
```

---

### Gap 2: `trace_query_by_tag` - Direct Tag Queries

**Priority:** P0

**What Primitive Has:**
```rust
fn query_by_tag(&self, run_id: &RunId, tag: &str) -> Result<Vec<Trace>>;
// Uses by-tag index for efficient lookups
```

**What Substrate Has:** `trace_list` accepts `tag` parameter BUT:
- Only used if no `trace_type` filter specified
- Query logic prioritizes type over tag
- Cannot filter by BOTH type AND tag

**Proposed Substrate API:**
```rust
fn trace_query_by_tag(&self, run: &ApiRunId, tag: &str,
    limit: Option<u64>) -> StrataResult<Vec<Versioned<TraceEntry>>>;

// Also fix trace_list to support combined filters
fn trace_list(..., type: Option<TraceType>, tags: &[&str], ...) -> ...;
```

---

### Gap 3: `trace_count` - Trace Statistics

**Priority:** P1

**What Primitive Has:**
```rust
fn count(&self, run_id: &RunId) -> Result<usize>;
```

**What Substrate Exposes:** Nothing

**Why Important:**
- Basic debugging: "How many traces in this run?"
- Monitoring: Track trace counts over time
- Pagination: Know total before paginating

**Proposed Substrate API:**
```rust
fn trace_count(&self, run: &ApiRunId, type_filter: Option<TraceType>)
    -> StrataResult<u64>;
```

---

### Gap 4: `trace_search` - Full-Text Search

**Priority:** P1

**What Primitive Has:**
```rust
fn search(&self, req: &SearchRequest) -> Result<SearchResponse>;
// Full-text search across trace type, tags, metadata
// Respects budget constraints
// Returns scored hits with time-range filtering
```

**What Substrate Exposes:** Nothing

**Why Important:**
- "Find traces mentioning 'timeout'" is common debugging
- Search API exists (M6) but hidden from substrate

**Proposed Substrate API:**
```rust
fn trace_search(&self, run: &ApiRunId, query: &str, limit: Option<u64>)
    -> StrataResult<Vec<Versioned<TraceEntry>>>;
```

---

## Part 3: Type System Degradation (P1)

The primitive has a rich type system that gets flattened at substrate level.

### Issue 1: ToolCall Duration Lost

**Primitive TraceType:**
```rust
ToolCall {
    tool_name: String,
    arguments: Value,
    result: Option<Value>,
    duration_ms: Option<u64>,  // Performance measurement!
}
```

**Substrate Conversion:**
- `TraceType::Tool` maps to primitive with `duration_ms: None`
- No way to record or retrieve duration

**Impact:** Cannot measure tool execution time

---

### Issue 2: Thought Confidence Lost

**Primitive TraceType:**
```rust
Thought {
    content: String,
    confidence: Option<f64>,  // Confidence scoring!
}
```

**Substrate Conversion:**
- `TraceType::Thought` maps to primitive with `confidence: None`
- No way to record or retrieve confidence

**Impact:** Cannot track agent confidence levels

---

### Issue 3: Error Semantics Lost

**Primitive TraceType:**
```rust
Error {
    error_type: String,
    message: String,
    recoverable: bool,  // Important for error handling!
}
```

**Substrate Conversion:**
- No `TraceType::Error` in substrate
- Maps to `TraceType::Custom("Error")` losing structure

**Impact:** Cannot distinguish recoverable vs non-recoverable errors

---

### Issue 4: Decision/Query Types Not Exposed

**Primitive Has:**
```rust
Decision {
    question: String,
    options: Vec<String>,
    chosen: String,
    reasoning: Option<String>,
}

Query {
    query_type: String,
    query: String,
    results_count: Option<u32>,
}
```

**Substrate:** Maps to `Custom("Decision")` and `Custom("Query")` losing structure

**Proposed Fix:** Add these types to substrate enum:
```rust
enum TraceType {
    Thought,
    Action,
    Observation,
    Tool { duration_ms: Option<u64> },  // Add duration
    Message,
    Decision { chosen: String },         // Add Decision
    Query { results_count: Option<u32> }, // Add Query
    Error { recoverable: bool },          // Add Error
    Custom(String),
}
```

---

## Part 4: Missing Table Stakes APIs (P0-P1)

### Gap 5: `trace_delete` - Trace Removal

**Priority:** P1

**Current:** No way to delete traces

**Why Important:**
- Debug traces may contain sensitive data
- Storage cleanup required
- GDPR/compliance may require deletion

**Proposed API:**
```rust
fn trace_delete(&self, run: &ApiRunId, trace_id: &str) -> StrataResult<bool>;
fn trace_delete_subtree(&self, run: &ApiRunId, root_id: &str) -> StrataResult<u64>;
```

---

### Gap 6: `trace_annotate` - Post-Hoc Annotations

**Priority:** P1

**Current:** Append-only, no modifications

**Why Important:**
- Add debugging notes after trace is recorded
- Mark traces as "reviewed" or "interesting"
- Doesn't violate append-only if implemented as linked annotations

**Proposed API:**
```rust
fn trace_annotate(&self, run: &ApiRunId, trace_id: &str, annotation: Value)
    -> StrataResult<Version>;

fn trace_get_annotations(&self, run: &ApiRunId, trace_id: &str)
    -> StrataResult<Vec<Versioned<Value>>>;
```

---

### Gap 7: `trace_batch_create` - Atomic Multi-Trace Recording

**Priority:** P1

**Current:** Must record traces one at a time

**Why Important:**
- Record multiple related traces atomically
- Reduce round trips
- Ensure consistent timestamps

**Proposed API:**
```rust
struct TraceSpec {
    trace_type: TraceType,
    parent_id: Option<String>,
    content: Value,
    tags: Vec<String>,
}

fn trace_batch_create(&self, run: &ApiRunId, traces: Vec<TraceSpec>)
    -> StrataResult<Vec<(String, Version)>>;
```

---

### Gap 8: `trace_list_tags` - Tag Discovery

**Priority:** P1

**Current:** No way to discover what tags exist

**Why Important:**
- Build tag filter UI
- Understand tagging patterns
- Find traces with specific tags

**Proposed API:**
```rust
fn trace_list_tags(&self, run: &ApiRunId) -> StrataResult<Vec<String>>;
```

---

### Gap 9: `trace_info` - Trace Metadata Without Full Content

**Priority:** P1

**Current:** Must read full trace to get metadata

**Why Important:**
- Check existence with metadata (type, timestamp, tags)
- Efficient pagination
- List view before detail view

**Proposed API:**
```rust
struct TraceInfo {
    id: String,
    trace_type: TraceType,
    parent_id: Option<String>,
    tags: Vec<String>,
    created_at: u64,
    has_children: bool,
}

fn trace_info(&self, run: &ApiRunId, trace_id: &str) -> StrataResult<Option<TraceInfo>>;
```

---

## Part 5: API Design Issues (P1)

### Design Issue 1: Version Lost in Query Results

**Current:**
```rust
fn trace_list(...) -> Vec<Versioned<TraceEntry>>;
// Returns Version::Txn(0) for all items - version information lost!
```

**Why Problem:**
- Cannot use version for CAS operations
- Inconsistent with `trace_get` which preserves version

**Fix:** Preserve version through query pipeline

---

### Design Issue 2: Timestamp Units Inconsistent

**Primitive:** `timestamp: i64` (milliseconds)
**Substrate:** `created_at: u64` (microseconds)

**Fix:** Standardize on one unit and document clearly

---

### Design Issue 3: `trace_tree` Returns Flattened List

**Current:**
```rust
fn trace_tree(run, root_id) -> Vec<Versioned<TraceEntry>>;
// Pre-order flattened, loses tree structure
```

**Primitive Has:**
```rust
struct TraceTree {
    trace: Trace,
    children: Vec<TraceTree>,  // Actual tree structure!
}
fn get_tree(run_id, root_id) -> Option<TraceTree>;
```

**Fix:** Return actual tree structure or depth-annotated list:
```rust
struct TraceWithDepth {
    trace: TraceEntry,
    depth: u32,
}
fn trace_tree(run, root_id) -> StrataResult<Vec<TraceWithDepth>>;

// Or return actual tree
fn trace_tree_structured(run, root_id) -> StrataResult<Option<TraceTree>>;
```

---

## Part 6: World-Class Reasoning Trace Features (P1-P2)

### Gap 10: Trace Linking/Correlation

**Priority:** P1

**Problem:** Cannot link traces across runs or to external systems

**Use Cases:**
- Link agent trace to user session
- Correlate traces across multiple runs
- Connect to external observability systems

**Proposed API:**
```rust
fn trace_create_with_links(&self, run: &ApiRunId,
    trace_type: TraceType,
    content: Value,
    links: Vec<TraceLink>,  // External correlation IDs
) -> StrataResult<(String, Version)>;

struct TraceLink {
    link_type: String,      // "follows_from", "child_of", "correlates_to"
    target_id: String,      // External or internal ID
    target_run: Option<ApiRunId>,
}
```

---

### Gap 11: Trace Retention/TTL

**Priority:** P1

**Problem:** Traces accumulate forever

**Use Cases:**
- Auto-delete old debug traces
- Comply with data retention policies
- Manage storage costs

**Proposed API:**
```rust
fn trace_set_retention(&self, run: &ApiRunId, max_age_ms: u64) -> StrataResult<()>;
fn trace_cleanup(&self, run: &ApiRunId, before_time: i64) -> StrataResult<u64>;
```

---

### Gap 12: Trace Statistics/Aggregation

**Priority:** P2

**Problem:** No aggregate queries

**Use Cases:**
- "How many tool calls in this run?"
- "Average tool duration?"
- "Error rate?"

**Proposed API:**
```rust
struct TraceStats {
    total_count: u64,
    by_type: HashMap<String, u64>,
    avg_duration_ms: Option<f64>,  // For ToolCall traces
    error_count: u64,
}

fn trace_stats(&self, run: &ApiRunId) -> StrataResult<TraceStats>;
```

---

### Gap 13: Trace Export

**Priority:** P2

**Problem:** No way to export traces for external analysis

**Use Cases:**
- Export to OpenTelemetry/Jaeger
- Offline analysis
- Backup/archive

**Proposed API:**
```rust
enum ExportFormat {
    Json,
    OpenTelemetry,
    Custom(String),
}

fn trace_export(&self, run: &ApiRunId, format: ExportFormat) -> StrataResult<Value>;
```

---

### Gap 14: Trace Replay/Visualization Hints

**Priority:** P2

**Problem:** No structured support for trace replay

**Use Cases:**
- Replay agent reasoning step-by-step
- Visualize decision trees
- Debug agent behavior

**Proposed API:**
```rust
struct ReplayStep {
    trace: TraceEntry,
    delay_since_previous_ms: u64,
    children_count: u32,
}

fn trace_replay_sequence(&self, run: &ApiRunId, root_id: &str)
    -> StrataResult<Vec<ReplayStep>>;
```

---

### Gap 15: Trace Sampling/Filtering at Record Time

**Priority:** P2

**Problem:** All traces recorded, no filtering

**Use Cases:**
- Only record error traces in production
- Sample high-volume trace types
- Reduce storage for verbose agents

**Proposed API:**
```rust
struct TraceSamplingConfig {
    sample_rate: f64,  // 0.0-1.0
    always_include_types: Vec<TraceType>,
    never_include_types: Vec<TraceType>,
}

fn trace_configure_sampling(&self, run: &ApiRunId, config: TraceSamplingConfig)
    -> StrataResult<()>;
```

---

## Priority Matrix

| ID | Issue | Priority | Effort | Category |
|----|-------|----------|--------|----------|
| Stub 1 | Custom ID assignment | P0 | Low | Stubbed API |
| Stub 2 | Tag updates | P0 | Medium | Stubbed API |
| Gap 1 | Time range queries | P0 | Low | Hidden Feature |
| Gap 2 | Direct tag queries | P0 | Low | Hidden Feature |
| Gap 3 | Trace count | P1 | Low | Hidden Feature |
| Gap 4 | Full-text search | P1 | Low | Hidden Feature |
| Issue 1 | ToolCall duration lost | P1 | Medium | Type Degradation |
| Issue 2 | Thought confidence lost | P1 | Medium | Type Degradation |
| Issue 3 | Error semantics lost | P1 | Medium | Type Degradation |
| Issue 4 | Decision/Query types | P1 | Medium | Type Degradation |
| Gap 5 | Trace deletion | P1 | Medium | Missing API |
| Gap 6 | Post-hoc annotations | P1 | Medium | Missing API |
| Gap 7 | Batch create | P1 | Medium | Missing API |
| Gap 8 | Tag discovery | P1 | Low | Missing API |
| Gap 9 | Trace info (metadata only) | P1 | Low | Missing API |
| Design 1 | Version lost in queries | P1 | Medium | Design |
| Design 2 | Timestamp units | P1 | Low | Design |
| Design 3 | Tree structure flattened | P1 | Low | Design |
| Gap 10 | Trace linking | P1 | Medium | World-Class |
| Gap 11 | Retention/TTL | P1 | Medium | World-Class |
| Gap 12 | Statistics/aggregation | P2 | Medium | World-Class |
| Gap 13 | Export | P2 | Medium | World-Class |
| Gap 14 | Replay hints | P2 | Low | World-Class |
| Gap 15 | Sampling/filtering | P2 | Medium | World-Class |

---

## Recommended Fix Order

### Phase 1: Expose Existing Primitives (Low Effort)
1. Expose time range queries (Gap 1) - primitive has it
2. Expose direct tag queries (Gap 2) - primitive has it
3. Expose trace count (Gap 3) - primitive has it
4. Expose full-text search (Gap 4) - primitive has it
5. Add trace info/metadata API (Gap 9)
6. Add tag discovery (Gap 8)
7. Fix timestamp units (Design 2)

### Phase 2: Fix Type System (Medium Effort)
8. Add duration to Tool type (Issue 1)
9. Add confidence to Thought type (Issue 2)
10. Add Error type with recoverable flag (Issue 3)
11. Add Decision/Query types (Issue 4)
12. Fix tree structure (Design 3)
13. Fix version preservation (Design 1)

### Phase 3: Missing APIs (Medium Effort)
14. Implement trace deletion (Gap 5)
15. Implement annotations (Gap 6)
16. Implement batch create (Gap 7)
17. Decide on stubbed APIs (Stub 1, 2)

### Phase 4: World-Class Features (High Effort)
18. Trace linking/correlation (Gap 10)
19. Retention/TTL (Gap 11)
20. Statistics (Gap 12)
21. Export (Gap 13)
22. Replay hints (Gap 14)
23. Sampling (Gap 15)

---

## Comparison with Industry Standards

| Feature | Strata TraceStore | OpenTelemetry | Jaeger | LangSmith |
|---------|-------------------|---------------|--------|-----------|
| Record traces | ✅ | ✅ | ✅ | ✅ |
| Hierarchical | ✅ | ✅ | ✅ | ✅ |
| Query by type | ✅ | ✅ | ✅ | ✅ |
| Query by tag | ❌ (hidden) | ✅ | ✅ | ✅ |
| Query by time | ❌ (hidden) | ✅ | ✅ | ✅ |
| Full-text search | ❌ (hidden) | ❌ | ✅ | ✅ |
| Duration tracking | ❌ (lost) | ✅ | ✅ | ✅ |
| **Tree visualization** | ✅ | ✅ | ✅ | ✅ |
| **Confidence scores** | ❌ (lost) | ❌ | ❌ | ✅ |
| Linking/correlation | ❌ | ✅ | ✅ | ✅ |
| Retention/TTL | ❌ | ✅ | ✅ | ✅ |
| Export | ❌ | ✅ | ✅ | ✅ |
| Sampling | ❌ | ✅ | ✅ | ❌ |
| Statistics | ❌ | ✅ | ✅ | ✅ |

**Strata's Unique Strengths:**
- Rich type system with Decision, Query, Error types (but hidden!)
- Confidence scoring for Thought traces (but hidden!)
- Agent-specific types (vs generic spans)

**Strata's Gaps:**
- Time/tag queries exist but hidden
- Type richness lost at substrate level
- No retention, export, linking

---

## Key Finding

**TraceStore has a sophisticated rich type system at the primitive level that gets completely flattened at substrate.**

The primitive supports:
- `ToolCall` with `duration_ms` and `result`
- `Thought` with `confidence`
- `Decision` with `options`, `chosen`, `reasoning`
- `Query` with `results_count`
- `Error` with `recoverable`

But substrate collapses everything to:
- `Thought`, `Action`, `Observation`, `Tool`, `Message`, `Custom`

Losing:
- Duration measurements
- Confidence scores
- Error recoverability
- Decision reasoning

**Recommendation:** Either expose the rich types at substrate level, or provide typed accessors for the structured data within `content`.
