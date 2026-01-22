# Epic 33: Core Search Types

**Goal**: Define core search types that lock in the interface

**Dependencies**: M5 complete

---

## Scope

- SearchRequest with query, k, budget, mode, filters
- SearchBudget with time and candidate limits
- SearchResponse with hits, truncated flag, stats
- SearchHit and SearchStats types
- DocRef enum with variants for all 6 primitives
- PrimitiveKind enum

---

## User Stories

| Story | Description | Priority |
|-------|-------------|----------|
| #257 | SearchRequest Type Definition | FOUNDATION |
| #258 | SearchBudget Type Definition | FOUNDATION |
| #259 | SearchResponse Type Definition | FOUNDATION |
| #260 | SearchHit and SearchStats Types | FOUNDATION |
| #261 | DocRef Enum (All Primitives) | FOUNDATION |
| #262 | PrimitiveKind Enum | FOUNDATION |

---

## Story #257: SearchRequest Type Definition

**File**: `crates/core/src/search_types.rs` (NEW)

**Deliverable**: SearchRequest type used by all search APIs

### Implementation

```rust
use crate::types::{RunId, Key};

/// Request for search across primitives
///
/// This is the universal search request type used by both
/// primitive-level search and composite search.
#[derive(Debug, Clone)]
pub struct SearchRequest {
    /// Run to search within
    pub run_id: RunId,

    /// Query string (interpreted by scorer)
    pub query: String,

    /// Maximum results to return
    pub k: usize,

    /// Time and work limits
    pub budget: SearchBudget,

    /// Search mode (extensible for future hybrid modes)
    pub mode: SearchMode,

    /// Optional: limit to specific primitives (for composite search)
    pub primitive_filter: Option<Vec<PrimitiveKind>>,

    /// Optional: time range filter (microseconds since epoch)
    pub time_range: Option<(u64, u64)>,

    /// Optional: tag filter (match any)
    pub tags_any: Vec<String>,
}

impl SearchRequest {
    pub fn new(run_id: RunId, query: impl Into<String>) -> Self {
        SearchRequest {
            run_id,
            query: query.into(),
            k: 10,
            budget: SearchBudget::default(),
            mode: SearchMode::default(),
            primitive_filter: None,
            time_range: None,
            tags_any: vec![],
        }
    }

    pub fn with_k(mut self, k: usize) -> Self {
        self.k = k;
        self
    }

    pub fn with_budget(mut self, budget: SearchBudget) -> Self {
        self.budget = budget;
        self
    }

    pub fn with_mode(mut self, mode: SearchMode) -> Self {
        self.mode = mode;
        self
    }

    pub fn with_primitive_filter(mut self, filter: Vec<PrimitiveKind>) -> Self {
        self.primitive_filter = Some(filter);
        self
    }

    pub fn with_time_range(mut self, start: u64, end: u64) -> Self {
        self.time_range = Some((start, end));
        self
    }

    pub fn with_tags(mut self, tags: Vec<String>) -> Self {
        self.tags_any = tags;
        self
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SearchMode {
    /// Keyword-based search (M6 default)
    #[default]
    Keyword,
    /// Reserved for future vector search
    Vector,
    /// Reserved for future hybrid (keyword + vector)
    Hybrid,
}
```

### Acceptance Criteria

- [ ] SearchRequest has all required fields
- [ ] Builder pattern for construction
- [ ] SearchMode enum with Keyword default
- [ ] Clone, Debug implemented

---

## Story #258: SearchBudget Type Definition

**File**: `crates/core/src/search_types.rs`

**Deliverable**: SearchBudget type for time and candidate limits

### Implementation

```rust
/// Limits on search execution
///
/// Search operations respect these limits and return truncated
/// results rather than timing out or erroring.
#[derive(Debug, Clone, Copy)]
pub struct SearchBudget {
    /// Hard stop on wall time (microseconds)
    pub max_wall_time_micros: u64,

    /// Maximum total candidates to consider
    pub max_candidates: usize,

    /// Maximum candidates per primitive (for composite search)
    pub max_candidates_per_primitive: usize,
}

impl Default for SearchBudget {
    fn default() -> Self {
        SearchBudget {
            max_wall_time_micros: 100_000,  // 100ms
            max_candidates: 10_000,
            max_candidates_per_primitive: 2_000,
        }
    }
}

impl SearchBudget {
    pub fn new(max_time_micros: u64, max_candidates: usize) -> Self {
        SearchBudget {
            max_wall_time_micros: max_time_micros,
            max_candidates,
            max_candidates_per_primitive: max_candidates / 6,  // Split across 6 primitives
        }
    }

    pub fn with_time(mut self, micros: u64) -> Self {
        self.max_wall_time_micros = micros;
        self
    }

    pub fn with_candidates(mut self, max: usize) -> Self {
        self.max_candidates = max;
        self
    }

    pub fn with_per_primitive(mut self, max: usize) -> Self {
        self.max_candidates_per_primitive = max;
        self
    }
}
```

### Acceptance Criteria

- [ ] Default values are sensible (100ms, 10K candidates)
- [ ] Builder pattern for customization
- [ ] Clone, Copy, Debug implemented

---

## Story #259: SearchResponse Type Definition

**File**: `crates/core/src/search_types.rs`

**Deliverable**: SearchResponse type returned by all search APIs

### Implementation

```rust
/// Search results
///
/// Returned by both primitive-level and composite search.
/// Contains ranked hits plus execution metadata.
#[derive(Debug, Clone)]
pub struct SearchResponse {
    /// Ranked hits (highest score first)
    pub hits: Vec<SearchHit>,

    /// True if budget caused early termination
    pub truncated: bool,

    /// Execution statistics
    pub stats: SearchStats,
}

impl SearchResponse {
    pub fn empty() -> Self {
        SearchResponse {
            hits: vec![],
            truncated: false,
            stats: SearchStats::default(),
        }
    }

    pub fn new(hits: Vec<SearchHit>, truncated: bool, stats: SearchStats) -> Self {
        SearchResponse { hits, truncated, stats }
    }

    pub fn is_empty(&self) -> bool {
        self.hits.is_empty()
    }

    pub fn len(&self) -> usize {
        self.hits.len()
    }
}
```

### Acceptance Criteria

- [ ] Contains hits, truncated flag, stats
- [ ] empty() constructor for no results
- [ ] Clone, Debug implemented

---

## Story #260: SearchHit and SearchStats Types

**File**: `crates/core/src/search_types.rs`

**Deliverable**: SearchHit for individual results, SearchStats for metadata

### Implementation

```rust
use std::collections::HashMap;

/// A single search result
#[derive(Debug, Clone)]
pub struct SearchHit {
    /// Back-pointer to source record
    pub doc_ref: DocRef,

    /// Score from scorer (higher = more relevant)
    pub score: f32,

    /// Rank in result set (1-indexed)
    pub rank: u32,

    /// Optional snippet for display
    pub snippet: Option<String>,
}

impl SearchHit {
    pub fn new(doc_ref: DocRef, score: f32, rank: u32) -> Self {
        SearchHit {
            doc_ref,
            score,
            rank,
            snippet: None,
        }
    }

    pub fn with_snippet(mut self, snippet: String) -> Self {
        self.snippet = Some(snippet);
        self
    }
}

/// Execution statistics for a search
#[derive(Debug, Clone, Default)]
pub struct SearchStats {
    /// Time spent in search (microseconds)
    pub elapsed_micros: u64,

    /// Total candidates considered
    pub candidates_considered: usize,

    /// Candidates per primitive (for composite search)
    pub candidates_by_primitive: HashMap<PrimitiveKind, usize>,

    /// Whether an index was used
    pub index_used: bool,
}

impl SearchStats {
    pub fn new(elapsed_micros: u64, candidates: usize) -> Self {
        SearchStats {
            elapsed_micros,
            candidates_considered: candidates,
            candidates_by_primitive: HashMap::new(),
            index_used: false,
        }
    }
}
```

### Acceptance Criteria

- [ ] SearchHit has doc_ref, score, rank, optional snippet
- [ ] SearchStats has elapsed_micros, candidates, index_used
- [ ] Clone, Debug implemented

---

## Story #261: DocRef Enum (All Primitives)

**File**: `crates/core/src/search_types.rs`

**Deliverable**: DocRef enum with variants for all 6 primitives

### Implementation

```rust
use crate::types::{Key, RunId};
use crate::json_types::JsonDocId;

/// Reference back to source record
///
/// Every search hit contains a DocRef that can be used to
/// retrieve the actual data from the appropriate primitive.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum DocRef {
    /// KV store entry
    Kv { key: Key },

    /// JSON document
    Json { key: Key, doc_id: JsonDocId },

    /// Event log entry
    Event { log_key: Key, seq: u64 },

    /// State cell
    State { key: Key },

    /// Trace span
    Trace { key: Key, span_id: u64 },

    /// Run metadata
    Run { run_id: RunId },
}

impl DocRef {
    /// Get the primitive kind for this reference
    pub fn primitive_kind(&self) -> PrimitiveKind {
        match self {
            DocRef::Kv { .. } => PrimitiveKind::Kv,
            DocRef::Json { .. } => PrimitiveKind::Json,
            DocRef::Event { .. } => PrimitiveKind::Event,
            DocRef::State { .. } => PrimitiveKind::State,
            DocRef::Trace { .. } => PrimitiveKind::Trace,
            DocRef::Run { .. } => PrimitiveKind::Run,
        }
    }

    /// Get the run_id this reference belongs to
    pub fn run_id(&self) -> RunId {
        match self {
            DocRef::Kv { key } => key.namespace().run_id(),
            DocRef::Json { key, .. } => key.namespace().run_id(),
            DocRef::Event { log_key, .. } => log_key.namespace().run_id(),
            DocRef::State { key } => key.namespace().run_id(),
            DocRef::Trace { key, .. } => key.namespace().run_id(),
            DocRef::Run { run_id } => *run_id,
        }
    }
}
```

### Acceptance Criteria

- [ ] All 6 primitive variants present
- [ ] primitive_kind() returns correct kind
- [ ] run_id() extracts run from any variant
- [ ] Clone, Debug, PartialEq, Eq, Hash implemented

---

## Story #262: PrimitiveKind Enum

**File**: `crates/core/src/search_types.rs`

**Deliverable**: PrimitiveKind enum for identifying primitives

### Implementation

```rust
/// Enumeration of all searchable primitives
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PrimitiveKind {
    Kv,
    Json,
    Event,
    State,
    Trace,
    Run,
}

impl PrimitiveKind {
    /// All primitive kinds
    pub fn all() -> &'static [PrimitiveKind] {
        &[
            PrimitiveKind::Kv,
            PrimitiveKind::Json,
            PrimitiveKind::Event,
            PrimitiveKind::State,
            PrimitiveKind::Trace,
            PrimitiveKind::Run,
        ]
    }
}

impl std::fmt::Display for PrimitiveKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PrimitiveKind::Kv => write!(f, "kv"),
            PrimitiveKind::Json => write!(f, "json"),
            PrimitiveKind::Event => write!(f, "event"),
            PrimitiveKind::State => write!(f, "state"),
            PrimitiveKind::Trace => write!(f, "trace"),
            PrimitiveKind::Run => write!(f, "run"),
        }
    }
}
```

### Acceptance Criteria

- [ ] All 6 variants present
- [ ] all() returns all variants
- [ ] Display for debugging
- [ ] Clone, Copy, Debug, PartialEq, Eq, Hash implemented

---

## Testing

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_search_request_builder() {
        let run_id = RunId::new();
        let req = SearchRequest::new(run_id, "test query")
            .with_k(20)
            .with_budget(SearchBudget::default().with_time(50_000));

        assert_eq!(req.query, "test query");
        assert_eq!(req.k, 20);
        assert_eq!(req.budget.max_wall_time_micros, 50_000);
    }

    #[test]
    fn test_doc_ref_primitive_kind() {
        let run_id = RunId::new();

        let kv_ref = DocRef::Kv { key: Key::new_kv(run_id, "test") };
        assert_eq!(kv_ref.primitive_kind(), PrimitiveKind::Kv);

        let run_ref = DocRef::Run { run_id };
        assert_eq!(run_ref.primitive_kind(), PrimitiveKind::Run);
    }

    #[test]
    fn test_primitive_kind_all() {
        assert_eq!(PrimitiveKind::all().len(), 6);
    }
}
```

---

## Files Modified/Created

| File | Action |
|------|--------|
| `crates/core/src/search_types.rs` | CREATE - All search types |
| `crates/core/src/lib.rs` | MODIFY - Export search_types module |
