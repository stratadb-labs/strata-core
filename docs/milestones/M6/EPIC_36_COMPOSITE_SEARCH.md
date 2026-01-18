# Epic 36: Composite Search (Hybrid)

**Goal**: Implement db.hybrid().search() that orchestrates primitive searches

**Dependencies**: Epic 34 (Primitive Search), Epic 35 (Scoring)

---

## Scope

- HybridSearch struct (stateless orchestrator)
- Database.hybrid() accessor
- Primitive selection based on filters
- Budget allocation across primitives
- Search orchestration with single snapshot

---

## User Stories

| Story | Description | Priority |
|-------|-------------|----------|
| #275 | HybridSearch Struct Definition | FOUNDATION |
| #276 | Database.hybrid() Accessor | FOUNDATION |
| #277 | Primitive Selection (Filters) | CRITICAL |
| #278 | Budget Allocation Across Primitives | CRITICAL |
| #279 | Search Orchestration (Same Snapshot) | CRITICAL |

---

## Story #275: HybridSearch Struct Definition

**File**: `crates/search/src/hybrid.rs` (NEW)

**Deliverable**: HybridSearch orchestrator struct

### Implementation

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
/// All search state is ephemeral per-request.
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

    /// Use a custom fuser for result fusion
    pub fn with_fuser(mut self, fuser: Arc<dyn Fuser>) -> Self {
        self.fuser = fuser;
        self
    }

    /// Search across all (or filtered) primitives
    ///
    /// 1. Selects primitives based on filter
    /// 2. Allocates budget across primitives
    /// 3. Executes primitive searches (same snapshot)
    /// 4. Fuses results
    pub fn search(&self, req: &SearchRequest) -> Result<SearchResponse> {
        let start = Instant::now();

        // 1. Select primitives
        let primitives = self.select_primitives(req);

        // 2. Allocate budgets
        let budgets = self.allocate_budgets(req, primitives.len());

        // 3. Take snapshot for consistent search
        let snapshot = self.db.snapshot();

        // 4. Execute searches
        let mut primitive_results = Vec::new();
        let mut total_candidates = 0;
        let mut any_truncated = false;

        for (primitive, budget) in primitives.iter().zip(budgets.iter()) {
            // Check overall time budget
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
            candidates_by_primitive: primitive_results
                .iter()
                .map(|(p, r)| (*p, r.stats.candidates_considered))
                .collect(),
            index_used: false,
        };

        Ok(SearchResponse {
            hits: fused.hits,
            truncated: any_truncated || fused.truncated,
            stats,
        })
    }
}
```

### Acceptance Criteria

- [ ] HybridSearch holds only Arc<Database>
- [ ] Fuser is configurable
- [ ] search() returns SearchResponse

---

## Story #276: Database.hybrid() Accessor

**File**: `crates/engine/src/database.rs`

**Deliverable**: Method to get HybridSearch from Database

### Implementation

```rust
impl Database {
    /// Get the hybrid search interface
    ///
    /// Returns an orchestrator for searching across multiple primitives.
    pub fn hybrid(&self) -> HybridSearch {
        HybridSearch::new(self.clone())
    }
}
```

### Acceptance Criteria

- [ ] db.hybrid() returns HybridSearch
- [ ] Can chain: db.hybrid().search(&req)

---

## Story #277: Primitive Selection (Filters)

**File**: `crates/search/src/hybrid.rs`

**Deliverable**: Logic to select which primitives to search

### Implementation

```rust
impl HybridSearch {
    /// Select which primitives to search based on request filters
    fn select_primitives(&self, req: &SearchRequest) -> Vec<PrimitiveKind> {
        match &req.primitive_filter {
            Some(filter) => filter.clone(),
            None => PrimitiveKind::all().to_vec(),
        }
    }
}
```

### Acceptance Criteria

- [ ] Returns all primitives if no filter
- [ ] Returns filtered list if filter specified
- [ ] Order is deterministic

---

## Story #278: Budget Allocation Across Primitives

**File**: `crates/search/src/hybrid.rs`

**Deliverable**: Logic to allocate budget across primitives

### Implementation

```rust
impl HybridSearch {
    /// Allocate budget across primitives
    ///
    /// Simple proportional allocation: divide evenly.
    /// Future: could weight by primitive "importance" or size.
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

### Acceptance Criteria

- [ ] Time budget divided evenly
- [ ] Candidate budget uses per_primitive limit
- [ ] Returns empty vec for 0 primitives

---

## Story #279: Search Orchestration (Same Snapshot)

**File**: `crates/search/src/hybrid.rs`

**Deliverable**: Execute primitive searches with single snapshot

### Implementation

```rust
impl HybridSearch {
    /// Execute search on a single primitive
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

// Each primitive needs search_with_snapshot variant
impl KVStore {
    pub fn search_with_snapshot(
        &self,
        req: &SearchRequest,
        snapshot: &Snapshot,
    ) -> Result<SearchResponse> {
        // Use provided snapshot instead of taking new one
        // ... same logic as search() but uses external snapshot
    }
}
```

### Acceptance Criteria

- [ ] Single snapshot taken at start
- [ ] All primitives use same snapshot
- [ ] Consistent view across primitives

---

## Architecture Diagram

```
SearchRequest
     │
     ▼
┌─────────────────────────────────────────────────────────┐
│                    HybridSearch                          │
│  ┌──────────────┐  ┌──────────────┐  ┌───────────────┐  │
│  │select_prims()│──│alloc_budget()│──│take snapshot  │  │
│  └──────────────┘  └──────────────┘  └───────┬───────┘  │
│                                               │          │
│  ┌────────────────────────────────────────────┴────────┐│
│  │              Same Snapshot                          ││
│  │  ┌───────┐ ┌───────┐ ┌───────┐ ┌───────┐ ┌───────┐ ││
│  │  │kv.srch││json.srch│event.srch│state.srch│trace.srch││
│  │  └───┬───┘ └───┬───┘ └───┬───┘ └───┬───┘ └───┬───┘ ││
│  └──────┼─────────┼─────────┼─────────┼─────────┼──────┘│
│         └─────────┴─────────┴─────────┴─────────┘       │
│                           │                              │
│                    ┌──────┴──────┐                       │
│                    │   Fuser     │                       │
│                    │  (RRF)      │                       │
│                    └──────┬──────┘                       │
└───────────────────────────┼─────────────────────────────┘
                            │
                            ▼
                     SearchResponse
```

---

## Testing

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hybrid_search_all_primitives() {
        let db = test_db();

        // Add data to multiple primitives
        db.kv.put(&run_id, "key1", "hello world")?;
        db.json.create(&run_id, json!({"msg": "hello there"}))?;
        db.event.append(&run_id, "greeting", json!({"text": "hello"}))?;

        let req = SearchRequest::new(run_id, "hello");
        let response = db.hybrid().search(&req)?;

        // Should have results from multiple primitives
        let kinds: HashSet<_> = response.hits.iter()
            .map(|h| h.doc_ref.primitive_kind())
            .collect();
        assert!(kinds.len() >= 2);
    }

    #[test]
    fn test_hybrid_search_with_filter() {
        let db = test_db();

        db.kv.put(&run_id, "key1", "test")?;
        db.json.create(&run_id, json!({"data": "test"}))?;

        let req = SearchRequest::new(run_id, "test")
            .with_primitive_filter(vec![PrimitiveKind::Kv]);

        let response = db.hybrid().search(&req)?;

        // Should only have KV results
        for hit in &response.hits {
            assert_eq!(hit.doc_ref.primitive_kind(), PrimitiveKind::Kv);
        }
    }

    #[test]
    fn test_hybrid_search_snapshot_consistency() {
        let db = test_db();
        db.kv.put(&run_id, "key1", "original")?;

        // Take snapshot via search
        let snapshot = db.snapshot();

        // Concurrent write (after snapshot)
        db.kv.put(&run_id, "key2", "new")?;

        let req = SearchRequest::new(run_id, "new");
        let response = db.kv.search_with_snapshot(&req, &snapshot)?;

        // Should NOT see the new write
        assert!(response.is_empty());
    }
}
```

---

## Files Modified/Created

| File | Action |
|------|--------|
| `crates/search/src/hybrid.rs` | CREATE - HybridSearch orchestrator |
| `crates/search/src/lib.rs` | MODIFY - Export hybrid module |
| `crates/engine/src/database.rs` | MODIFY - Add hybrid() method |
| `crates/primitives/src/*.rs` | MODIFY - Add search_with_snapshot() |
