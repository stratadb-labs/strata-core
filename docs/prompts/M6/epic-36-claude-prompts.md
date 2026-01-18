# Epic 36: Composite Search (Hybrid) - Implementation Prompts

**Epic Goal**: Implement db.hybrid().search() that orchestrates primitive searches

**GitHub Issue**: [#298](https://github.com/anibjoshi/in-mem/issues/298)
**Status**: Ready after Epics 34, 35
**Dependencies**: Epic 34 (Primitive Search), Epic 35 (Scoring)

---

## AUTHORITATIVE SPECIFICATIONS - READ THESE FIRST

**`docs/architecture/M6_ARCHITECTURE.md` is THE AUTHORITATIVE SPEC.**

Before starting ANY story in this epic, read:
1. **Architecture Spec (AUTHORITATIVE)**: `docs/architecture/M6_ARCHITECTURE.md`
2. **Epic Spec**: `docs/milestones/M6/EPIC_36_COMPOSITE_SEARCH.md`
3. **Prompt Header**: `docs/prompts/M6/M6_PROMPT_HEADER.md` for the 6 architectural rules

**CRITICAL RULES**:
- Rule 3: Composite Orchestrates, Doesn't Replace
- Rule 4: Snapshot-Consistent Search

---

## Epic 36 Overview

### Scope
- HybridSearch struct (stateless orchestrator)
- Database.hybrid() accessor
- Primitive selection based on filters
- Budget allocation across primitives
- Search orchestration with single snapshot

### Success Criteria
- [ ] HybridSearch is stateless (only Arc<Database>)
- [ ] db.hybrid() returns HybridSearch
- [ ] Single snapshot used for all primitive searches
- [ ] Budget allocated evenly across primitives
- [ ] Primitive filter works correctly

### Component Breakdown
- **Story #275 (GitHub #320)**: HybridSearch Struct Definition - FOUNDATION
- **Story #276 (GitHub #321)**: Database.hybrid() Accessor - FOUNDATION
- **Story #277 (GitHub #322)**: Primitive Selection (Filters) - CRITICAL
- **Story #278 (GitHub #323)**: Budget Allocation Across Primitives - CRITICAL
- **Story #279 (GitHub #324)**: Search Orchestration (Same Snapshot) - CRITICAL

---

## Dependency Graph

```
Story #320 (HybridSearch) ──> Story #321 (db.hybrid())
                                     │
Story #322 (Primitive Filter) ───────┼──> Story #324 (Orchestration)
                                     │
Story #323 (Budget Allocation) ──────┘
```

---

## Parallelization Strategy

### Optimal Execution (2 Claudes)

| Phase | Duration | Claude 1 | Claude 2 |
|-------|----------|----------|----------|
| 1 | 2 hours | #320 HybridSearch | - |
| 2 | 2 hours | #321 db.hybrid() | #322 Primitive Filter |
| 3 | 2 hours | #323 Budget Allocation | - |
| 4 | 3 hours | #324 Orchestration | - |

**Total Wall Time**: ~9 hours (vs. ~12 hours sequential)

---

## Story #320: HybridSearch Struct Definition

**GitHub Issue**: [#320](https://github.com/anibjoshi/in-mem/issues/320)
**Estimated Time**: 2 hours
**Dependencies**: Epics 34, 35 complete
**Blocks**: Story #321

### Start Story

```bash
gh issue view 320
./scripts/start-story.sh 36 320 hybrid-search
```

### Implementation

Create `crates/search/src/hybrid.rs`:

```rust
use std::sync::Arc;
use crate::engine::Database;
use crate::search_types::{SearchRequest, SearchResponse, PrimitiveKind};
use crate::fuser::{Fuser, RRFFuser};

/// Composite search orchestrator
///
/// HybridSearch coordinates searches across multiple primitives
/// and fuses results into a single ranked list.
///
/// CRITICAL: HybridSearch is STATELESS. It holds only Arc<Database>.
#[derive(Clone)]
pub struct HybridSearch {
    db: Arc<Database>,
    fuser: Arc<dyn Fuser>,
}

impl HybridSearch {
    pub(crate) fn new(db: Arc<Database>) -> Self {
        HybridSearch {
            db,
            fuser: Arc::new(RRFFuser::default()),
        }
    }

    pub fn with_fuser(mut self, fuser: Arc<dyn Fuser>) -> Self {
        self.fuser = fuser;
        self
    }

    pub fn search(&self, req: &SearchRequest) -> Result<SearchResponse> {
        // Implementation in Story #324
        unimplemented!()
    }
}
```

### Complete Story

```bash
./scripts/complete-story.sh 320
```

---

## Story #321: Database.hybrid() Accessor

**GitHub Issue**: [#321](https://github.com/anibjoshi/in-mem/issues/321)
**Estimated Time**: 1 hour
**Dependencies**: Story #320

### Start Story

```bash
gh issue view 321
./scripts/start-story.sh 36 321 db-hybrid
```

### Implementation

Add to `crates/engine/src/database.rs`:

```rust
impl Database {
    /// Get the hybrid search interface
    pub fn hybrid(&self) -> HybridSearch {
        HybridSearch::new(self.clone())
    }
}
```

### Tests

```rust
#[test]
fn test_db_hybrid_accessor() {
    let db = test_db();
    let hybrid = db.hybrid();
    // Should compile and not panic
}

#[test]
fn test_hybrid_chain() {
    let db = test_db();
    db.kv.put(&run_id, "key", "value")?;

    let req = SearchRequest::new(run_id, "value");
    let response = db.hybrid().search(&req)?;
    // Should work
}
```

### Complete Story

```bash
./scripts/complete-story.sh 321
```

---

## Story #322: Primitive Selection (Filters)

**GitHub Issue**: [#322](https://github.com/anibjoshi/in-mem/issues/322)
**Estimated Time**: 2 hours
**Dependencies**: Story #320

### Start Story

```bash
gh issue view 322
./scripts/start-story.sh 36 322 primitive-filter
```

### Implementation

Add to `crates/search/src/hybrid.rs`:

```rust
impl HybridSearch {
    fn select_primitives(&self, req: &SearchRequest) -> Vec<PrimitiveKind> {
        match &req.primitive_filter {
            Some(filter) => filter.clone(),
            None => PrimitiveKind::all().to_vec(),
        }
    }
}
```

### Tests

```rust
#[test]
fn test_primitive_filter() {
    let db = test_db();
    db.kv.put(&run_id, "key1", "test")?;
    db.json.create(&run_id, json!({"x": "test"}))?;

    let req = SearchRequest::new(run_id, "test")
        .with_primitive_filter(vec![PrimitiveKind::Kv]);

    let response = db.hybrid().search(&req)?;

    for hit in &response.hits {
        assert_eq!(hit.doc_ref.primitive_kind(), PrimitiveKind::Kv);
    }
}
```

### Complete Story

```bash
./scripts/complete-story.sh 322
```

---

## Story #323: Budget Allocation Across Primitives

**GitHub Issue**: [#323](https://github.com/anibjoshi/in-mem/issues/323)
**Estimated Time**: 2 hours
**Dependencies**: Story #320

### Start Story

```bash
gh issue view 323
./scripts/start-story.sh 36 323 budget-allocation
```

### Implementation

Add to `crates/search/src/hybrid.rs`:

```rust
impl HybridSearch {
    fn allocate_budgets(&self, req: &SearchRequest, num_primitives: usize) -> Vec<SearchBudget> {
        if num_primitives == 0 {
            return vec![];
        }

        let per_primitive_time = req.budget.max_wall_time_micros / num_primitives as u64;
        let per_primitive_candidates = req.budget.max_candidates_per_primitive;

        vec![
            SearchBudget {
                max_wall_time_micros: per_primitive_time,
                max_candidates: per_primitive_candidates,
                max_candidates_per_primitive: per_primitive_candidates,
            };
            num_primitives
        ]
    }
}
```

### Complete Story

```bash
./scripts/complete-story.sh 323
```

---

## Story #324: Search Orchestration (Same Snapshot)

**GitHub Issue**: [#324](https://github.com/anibjoshi/in-mem/issues/324)
**Estimated Time**: 3 hours
**Dependencies**: Stories #320-#323

### Start Story

```bash
gh issue view 324
./scripts/start-story.sh 36 324 orchestration
```

### Implementation

Complete the `search()` method in `crates/search/src/hybrid.rs`:

```rust
impl HybridSearch {
    pub fn search(&self, req: &SearchRequest) -> Result<SearchResponse> {
        let start = Instant::now();

        // 1. Select primitives
        let primitives = self.select_primitives(req);

        // 2. Allocate budgets
        let budgets = self.allocate_budgets(req, primitives.len());

        // 3. Take SINGLE snapshot for consistent search
        let snapshot = self.db.snapshot();

        // 4. Execute searches
        let mut primitive_results = Vec::new();
        let mut total_candidates = 0;
        let mut any_truncated = false;

        for (primitive, budget) in primitives.iter().zip(budgets.iter()) {
            if start.elapsed().as_micros() as u64 >= req.budget.max_wall_time_micros {
                any_truncated = true;
                break;
            }

            let sub_req = req.clone().with_budget(*budget);
            let result = self.search_primitive(*primitive, &sub_req, &snapshot)?;

            total_candidates += result.stats.candidates_considered;
            if result.truncated {
                any_truncated = true;
            }

            primitive_results.push((*primitive, result));
        }

        // 5. Fuse results
        let fused = self.fuser.fuse(primitive_results, req.k);

        // 6. Build stats
        let stats = SearchStats {
            elapsed_micros: start.elapsed().as_micros() as u64,
            candidates_considered: total_candidates,
            ..Default::default()
        };

        Ok(SearchResponse {
            hits: fused.hits,
            truncated: any_truncated || fused.truncated,
            stats,
        })
    }

    fn search_primitive(
        &self,
        primitive: PrimitiveKind,
        req: &SearchRequest,
        snapshot: &Snapshot,
    ) -> Result<SearchResponse> {
        match primitive {
            PrimitiveKind::Kv => self.db.kv.search_with_snapshot(req, snapshot),
            PrimitiveKind::Json => self.db.json.search_with_snapshot(req, snapshot),
            PrimitiveKind::Event => self.db.event.search_with_snapshot(req, snapshot),
            PrimitiveKind::State => self.db.state.search_with_snapshot(req, snapshot),
            PrimitiveKind::Trace => self.db.trace.search_with_snapshot(req, snapshot),
            PrimitiveKind::Run => self.db.run_index.search_with_snapshot(req, snapshot),
        }
    }
}
```

### Tests

```rust
#[test]
fn test_hybrid_search_all_primitives() {
    let db = test_db();
    db.kv.put(&run_id, "key1", "hello world")?;
    db.json.create(&run_id, json!({"msg": "hello there"}))?;
    db.event.append(&run_id, "greeting", json!({"text": "hello"}))?;

    let req = SearchRequest::new(run_id, "hello");
    let response = db.hybrid().search(&req)?;

    let kinds: HashSet<_> = response.hits.iter()
        .map(|h| h.doc_ref.primitive_kind())
        .collect();
    assert!(kinds.len() >= 2);
}

#[test]
fn test_hybrid_search_snapshot_consistency() {
    let db = test_db();
    db.kv.put(&run_id, "key1", "original")?;

    let snapshot = db.snapshot();
    db.kv.put(&run_id, "key2", "new")?;

    let req = SearchRequest::new(run_id, "new");
    let response = db.kv.search_with_snapshot(&req, &snapshot)?;

    // Should NOT see the new write
    assert!(response.is_empty());
}
```

### Complete Story

```bash
./scripts/complete-story.sh 324
```

---

## Epic 36 Completion Checklist

### 1. Final Validation

```bash
~/.cargo/bin/cargo test -p search -- hybrid
~/.cargo/bin/cargo test --workspace
~/.cargo/bin/cargo clippy --workspace -- -D warnings
~/.cargo/bin/cargo fmt --check
```

### 2. Verify Deliverables

- [ ] HybridSearch is stateless
- [ ] db.hybrid() works
- [ ] Single snapshot for all primitives
- [ ] Budget allocation is even
- [ ] Primitive filter works

### 3. Merge to Develop

```bash
git checkout develop
git merge --no-ff epic-36-composite-search -m "Epic 36: Composite Search complete

Delivered:
- HybridSearch orchestrator (stateless)
- Database.hybrid() accessor
- Primitive selection with filters
- Budget allocation across primitives
- Snapshot-consistent orchestration

Stories: #320-#324
"
git push origin develop
gh issue close 298 --comment "Epic 36: Composite Search - COMPLETE"
```
