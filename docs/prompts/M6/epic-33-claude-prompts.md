# Epic 33: Core Search Types - Implementation Prompts

**Epic Goal**: Define core search types that lock in the interface

**GitHub Issue**: [#295](https://github.com/anibjoshi/in-mem/issues/295)
**Status**: Ready to begin (after M5 complete)
**Dependencies**: M5 complete

---

## AUTHORITATIVE SPECIFICATIONS - READ THESE FIRST

**`docs/architecture/M6_ARCHITECTURE.md` is THE AUTHORITATIVE SPEC.**

Before starting ANY story in this epic, read:
1. **Architecture Spec (AUTHORITATIVE)**: `docs/architecture/M6_ARCHITECTURE.md`
2. **Epic Spec**: `docs/milestones/M6/EPIC_33_CORE_TYPES.md`
3. **Prompt Header**: `docs/prompts/M6/M6_PROMPT_HEADER.md` for the 6 architectural rules

**The architecture spec is LAW.** Epic docs provide implementation details but MUST NOT contradict the architecture spec.

---

## Epic 33 Overview

### Scope
- SearchRequest with query, k, budget, mode, filters
- SearchBudget with time and candidate limits
- SearchResponse with hits, truncated flag, stats
- SearchHit and SearchStats types
- DocRef enum with variants for all 6 primitives
- PrimitiveKind enum

### Success Criteria
- [ ] SearchRequest builder pattern works
- [ ] SearchBudget has sensible defaults (100ms, 10K candidates)
- [ ] SearchResponse contains hits, truncated flag, stats
- [ ] DocRef has variants for all 6 primitives
- [ ] PrimitiveKind::all() returns all 6 kinds
- [ ] All types are Clone, Debug

### Component Breakdown
- **Story #257 (GitHub #302)**: SearchRequest Type Definition - FOUNDATION
- **Story #258 (GitHub #303)**: SearchBudget Type Definition - FOUNDATION
- **Story #259 (GitHub #304)**: SearchResponse Type Definition - FOUNDATION
- **Story #260 (GitHub #305)**: SearchHit and SearchStats Types - FOUNDATION
- **Story #261 (GitHub #306)**: DocRef Enum (All Primitives) - FOUNDATION
- **Story #262 (GitHub #307)**: PrimitiveKind Enum - FOUNDATION

---

## Dependency Graph

```
Story #302 (SearchRequest) ──┬──> Story #304 (SearchResponse)
                             │
Story #303 (SearchBudget) ───┴──> Story #305 (SearchHit/Stats)
                                        │
Story #307 (PrimitiveKind) ────────────>│
                                        │
Story #306 (DocRef) <───────────────────┘
```

---

## Parallelization Strategy

### Optimal Execution (3 Claudes)

| Phase | Duration | Claude 1 | Claude 2 | Claude 3 |
|-------|----------|----------|----------|----------|
| 1 | 2 hours | #302 SearchRequest | #303 SearchBudget | #307 PrimitiveKind |
| 2 | 2 hours | #304 SearchResponse | #305 SearchHit/Stats | - |
| 3 | 2 hours | #306 DocRef | - | - |

**Total Wall Time**: ~6 hours (vs. ~10 hours sequential)

---

## Story #302: SearchRequest Type Definition

**GitHub Issue**: [#302](https://github.com/anibjoshi/in-mem/issues/302)
**Estimated Time**: 2 hours
**Dependencies**: M5 complete
**Blocks**: Stories #304, #305, #306

### Start Story

```bash
gh issue view 302
./scripts/start-story.sh 33 302 search-request
```

### Implementation Steps

#### Step 1: Create search_types.rs module

Create `crates/core/src/search_types.rs`:

```rust
//! Core search types for M6 Retrieval Surfaces
//!
//! These types define the interface between search APIs and implementations.

use crate::types::RunId;
use std::collections::HashMap;

/// Request for search across primitives
#[derive(Debug, Clone)]
pub struct SearchRequest {
    pub run_id: RunId,
    pub query: String,
    pub k: usize,
    pub budget: SearchBudget,
    pub mode: SearchMode,
    pub primitive_filter: Option<Vec<PrimitiveKind>>,
    pub time_range: Option<(u64, u64)>,
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

    pub fn with_primitive_filter(mut self, filter: Vec<PrimitiveKind>) -> Self {
        self.primitive_filter = Some(filter);
        self
    }

    pub fn with_time_range(mut self, start: u64, end: u64) -> Self {
        self.time_range = Some((start, end));
        self
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SearchMode {
    #[default]
    Keyword,
    Vector,
    Hybrid,
}
```

#### Step 2: Update lib.rs

```rust
pub mod search_types;
pub use search_types::*;
```

### Tests

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
    fn test_search_request_defaults() {
        let run_id = RunId::new();
        let req = SearchRequest::new(run_id, "query");

        assert_eq!(req.k, 10);
        assert_eq!(req.mode, SearchMode::Keyword);
        assert!(req.primitive_filter.is_none());
    }
}
```

### Validation

```bash
~/.cargo/bin/cargo test -p in-mem-core -- search_request
~/.cargo/bin/cargo clippy -p in-mem-core -- -D warnings
```

### Complete Story

```bash
./scripts/complete-story.sh 302
```

---

## Story #303: SearchBudget Type Definition

**GitHub Issue**: [#303](https://github.com/anibjoshi/in-mem/issues/303)
**Estimated Time**: 1 hour
**Dependencies**: None
**Blocks**: Stories #302, #304

### Start Story

```bash
gh issue view 303
./scripts/start-story.sh 33 303 search-budget
```

### Implementation

Add to `crates/core/src/search_types.rs`:

```rust
/// Limits on search execution
#[derive(Debug, Clone, Copy)]
pub struct SearchBudget {
    pub max_wall_time_micros: u64,
    pub max_candidates: usize,
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
    pub fn with_time(mut self, micros: u64) -> Self {
        self.max_wall_time_micros = micros;
        self
    }

    pub fn with_candidates(mut self, max: usize) -> Self {
        self.max_candidates = max;
        self
    }
}
```

### Complete Story

```bash
./scripts/complete-story.sh 303
```

---

## Story #304: SearchResponse Type Definition

**GitHub Issue**: [#304](https://github.com/anibjoshi/in-mem/issues/304)
**Estimated Time**: 1 hour
**Dependencies**: Stories #303, #305

### Start Story

```bash
gh issue view 304
./scripts/start-story.sh 33 304 search-response
```

### Implementation

```rust
/// Search results
#[derive(Debug, Clone)]
pub struct SearchResponse {
    pub hits: Vec<SearchHit>,
    pub truncated: bool,
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
}
```

### Complete Story

```bash
./scripts/complete-story.sh 304
```

---

## Story #305: SearchHit and SearchStats Types

**GitHub Issue**: [#305](https://github.com/anibjoshi/in-mem/issues/305)
**Estimated Time**: 2 hours
**Dependencies**: Story #306

### Start Story

```bash
gh issue view 305
./scripts/start-story.sh 33 305 search-hit-stats
```

### Implementation

```rust
/// A single search result
#[derive(Debug, Clone)]
pub struct SearchHit {
    pub doc_ref: DocRef,
    pub score: f32,
    pub rank: u32,
    pub snippet: Option<String>,
}

impl SearchHit {
    pub fn new(doc_ref: DocRef, score: f32, rank: u32) -> Self {
        SearchHit { doc_ref, score, rank, snippet: None }
    }

    pub fn with_snippet(mut self, snippet: String) -> Self {
        self.snippet = Some(snippet);
        self
    }
}

/// Execution statistics
#[derive(Debug, Clone, Default)]
pub struct SearchStats {
    pub elapsed_micros: u64,
    pub candidates_considered: usize,
    pub candidates_by_primitive: HashMap<PrimitiveKind, usize>,
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

### Complete Story

```bash
./scripts/complete-story.sh 305
```

---

## Story #306: DocRef Enum (All Primitives)

**GitHub Issue**: [#306](https://github.com/anibjoshi/in-mem/issues/306)
**Estimated Time**: 2 hours
**Dependencies**: Story #307

### Start Story

```bash
gh issue view 306
./scripts/start-story.sh 33 306 doc-ref
```

### Implementation

```rust
use crate::types::{Key, RunId};
use crate::json_types::JsonDocId;

/// Reference back to source record
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum DocRef {
    Kv { key: Key },
    Json { key: Key, doc_id: JsonDocId },
    Event { log_key: Key, seq: u64 },
    State { key: Key },
    Trace { key: Key, span_id: u64 },
    Run { run_id: RunId },
}

impl DocRef {
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

### Complete Story

```bash
./scripts/complete-story.sh 306
```

---

## Story #307: PrimitiveKind Enum

**GitHub Issue**: [#307](https://github.com/anibjoshi/in-mem/issues/307)
**Estimated Time**: 1 hour
**Dependencies**: None

### Start Story

```bash
gh issue view 307
./scripts/start-story.sh 33 307 primitive-kind
```

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

### Complete Story

```bash
./scripts/complete-story.sh 307
```

---

## Epic 33 Completion Checklist

### 1. Final Validation

```bash
~/.cargo/bin/cargo test -p in-mem-core -- search
~/.cargo/bin/cargo test --workspace
~/.cargo/bin/cargo clippy --workspace -- -D warnings
~/.cargo/bin/cargo fmt --check
```

### 2. Verify Deliverables

- [ ] SearchRequest builder pattern works
- [ ] SearchBudget has default 100ms, 10K candidates
- [ ] SearchResponse contains hits, truncated, stats
- [ ] SearchHit has doc_ref, score, rank
- [ ] DocRef has all 6 primitive variants
- [ ] PrimitiveKind::all() returns 6 kinds

### 3. Merge to Develop

```bash
git checkout develop
git merge --no-ff epic-33-search-types -m "Epic 33: Core Search Types complete

Delivered:
- SearchRequest with builder pattern
- SearchBudget with time/candidate limits
- SearchResponse with hits, truncated, stats
- SearchHit and SearchStats types
- DocRef enum for all 6 primitives
- PrimitiveKind enum

Stories: #302, #303, #304, #305, #306, #307
"
git push origin develop
gh issue close 295 --comment "Epic 33: Core Search Types - COMPLETE"
```

---

## Summary

Epic 33 establishes the foundational search types that all subsequent M6 epics build upon. These types define the interface contracts for search requests, responses, and results.
