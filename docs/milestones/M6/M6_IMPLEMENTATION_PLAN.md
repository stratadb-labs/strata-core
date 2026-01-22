# M6 Implementation Plan: Retrieval Surfaces

## Overview

This document provides the high-level implementation plan for M6 (Retrieval Surfaces).

**Total Scope**: 7 Epics, 35 Stories

**References**:
- [M6 Architecture Specification](../../architecture/M6_ARCHITECTURE.md) - Authoritative spec (v1.1)
- [M6 Spec](../../../M6-spec.md) - Original design document

**Critical Framing**:
> M6 is called "search" but is actually building **recall infrastructure**.
> The keyword search in M6 is a "hello world" to validate the plumbing.
> Future trajectory: `hybrid.search(query)` → `hybrid.recall(plan)`

**Epic Details**:
- [Epic 33: Core Search Types](./EPIC_33_CORE_TYPES.md)
- [Epic 34: Primitive Search Surface](./EPIC_34_PRIMITIVE_SEARCH.md)
- [Epic 35: Scoring Infrastructure](./EPIC_35_SCORING.md)
- [Epic 36: Composite Search (Hybrid)](./EPIC_36_COMPOSITE_SEARCH.md)
- [Epic 37: Fusion Infrastructure](./EPIC_37_FUSION.md)
- [Epic 38: Optional Indexing](./EPIC_38_INDEXING.md)
- [Epic 39: Validation & Non-Regression](./EPIC_39_VALIDATION.md)

---

## Architectural Integration Rules (NON-NEGOTIABLE)

These rules ensure M6 integrates properly with the M1-M5 architecture.

### Rule 1: No Data Movement

Composite search runs against each primitive's native storage. NO unified search store.

**FORBIDDEN**: Copying primitive data into a search-specific DashMap or index.

### Rule 2: Primitive Search Is First-Class

Each primitive has its own `.search()` method. Users can search a single primitive directly.

### Rule 3: Composite Orchestrates, Not Replaces

`db.hybrid().search()` calls primitive searches and fuses results. It does NOT own storage.

### Rule 4: Snapshot-Consistent Search

All search operations use a SnapshotView. Results are stable for that search invocation.

### Rule 5: Zero Overhead When Not Used

No extra allocations per transaction when search is disabled. Lazy initialization everywhere.

### Rule 6: Algorithm Swappable

Scorer and Fuser are traits. M6 ships BM25-lite and RRF as defaults. Future can swap.

---

## Evolution Warnings

**These M6 design decisions must not ossify** (see Architecture Spec Section 1.5):

| Area | Warning |
|------|---------|
| **SearchRequest** | Must not become a query DSL. Will evolve toward `QueryExpr` variants. |
| **JSON Flattening** | Temporary lossy baseline. Future needs path-aware, field-weighted matching. |
| **ScorerContext** | BM25-shaped for M6. Future scorers need recency, salience, causality signals. |
| **Fusion** | RRF is not endgame. Fusion will become multi-step retrieval planning. |
| **Indexing** | Internal optimization, not conceptual pillar. Must remain swappable. |

**Reserved Concept**: `RetrievalPlan` - M6's `search()` is scaffolding toward `recall(plan)`.

---

## Critical Invariants

1. **SearchRequest Is Universal**: Same type for primitive and composite search
2. **DocRef Back-Pointers**: All hits have DocRef that can be dereferenced
3. **Budget Always Enforced**: Time and candidate limits respected; results truncated, never error
4. **Deterministic Results**: Same snapshot + same request = identical ordered results
5. **Non-Regression**: M6 must not degrade M5 primitive performance

---

## Epic Overview

| Epic | Name | Stories | Dependencies |
|------|------|---------|--------------|
| 33 | Core Search Types | 6 | M5 complete |
| 34 | Primitive Search Surface | 8 | Epic 33 |
| 35 | Scoring Infrastructure | 4 | Epic 33 |
| 36 | Composite Search (Hybrid) | 5 | Epic 34, 35 |
| 37 | Fusion Infrastructure | 4 | Epic 36 |
| 38 | Optional Indexing | 5 | Epic 34 |
| 39 | Validation & Non-Regression | 3 | All others |

---

## Epic 33: Core Search Types

**Goal**: Define core search types that lock in the interface

| Story | Description | Priority |
|-------|-------------|----------|
| #257 | SearchRequest Type Definition | FOUNDATION |
| #258 | SearchBudget Type Definition | FOUNDATION |
| #259 | SearchResponse Type Definition | FOUNDATION |
| #260 | SearchHit and SearchStats Types | FOUNDATION |
| #261 | DocRef Enum (All Primitives) | FOUNDATION |
| #262 | PrimitiveKind Enum | FOUNDATION |

**Acceptance Criteria**:
- [ ] SearchRequest has run_id, query, k, budget, mode, filters
- [ ] SearchBudget has max_wall_time_micros, max_candidates, max_candidates_per_primitive
- [ ] SearchResponse has hits, truncated, stats
- [ ] SearchHit has ref_, score, rank, snippet (optional)
- [ ] DocRef has variants for Kv, Json, Event, State, Trace, Run
- [ ] PrimitiveKind enum with all 6 variants
- [ ] DocRef::primitive_kind() implemented
- [ ] All types are Clone, Debug

---

## Epic 34: Primitive Search Surface

**Goal**: Implement `.search()` on each primitive

| Story | Description | Priority |
|-------|-------------|----------|
| #263 | Searchable Trait Definition | FOUNDATION |
| #264 | KVStore.search() Implementation | CRITICAL |
| #265 | JsonStore.search() Implementation | CRITICAL |
| #266 | EventLog.search() Implementation | CRITICAL |
| #267 | StateCell.search() Implementation | CRITICAL |
| #268 | TraceStore.search() Implementation | CRITICAL |
| #269 | RunIndex.search() Implementation | CRITICAL |
| #270 | Text Extraction Per Primitive | HIGH |

**Acceptance Criteria**:
- [ ] Searchable trait defined with search() method
- [ ] Each primitive implements search(&SearchRequest) -> Result<SearchResponse>
- [ ] Search uses SnapshotView for consistency
- [ ] Budget enforcement (time and candidates) in each primitive
- [ ] Text extraction strategy per primitive
- [ ] DocRef correctly constructed for each primitive
- [ ] All search methods respect run_id filter

---

## Epic 35: Scoring Infrastructure

**Goal**: Implement pluggable scoring with BM25-lite default

| Story | Description | Priority |
|-------|-------------|----------|
| #271 | Scorer Trait Definition | FOUNDATION |
| #272 | ScorerContext Type | FOUNDATION |
| #273 | BM25LiteScorer Implementation | CRITICAL |
| #274 | Tokenizer (Basic) | HIGH |

**Acceptance Criteria**:
- [ ] Scorer trait with score(doc, query, ctx) -> f32
- [ ] ScorerContext has total_docs, doc_freqs, avg_doc_len, now_micros, extensions
- [ ] ScorerContext.extensions is HashMap<String, Value> for future signals
- [ ] BM25LiteScorer with k1=1.2, b=0.75 parameters
- [ ] Basic tokenizer: lowercase, split on non-alphanumeric, min 2 chars
- [ ] Scorer is Send + Sync

---

## Epic 36: Composite Search (Hybrid)

**Goal**: Implement db.hybrid().search() that orchestrates primitive searches

| Story | Description | Priority |
|-------|-------------|----------|
| #275 | HybridSearch Struct Definition | FOUNDATION |
| #276 | Database.hybrid() Accessor | FOUNDATION |
| #277 | Primitive Selection (Filters) | CRITICAL |
| #278 | Budget Allocation Across Primitives | CRITICAL |
| #279 | Search Orchestration (Same Snapshot) | CRITICAL |

**Acceptance Criteria**:
- [ ] HybridSearch holds Arc<Database> only (stateless)
- [ ] db.hybrid() returns HybridSearch
- [ ] primitive_filter selects which primitives to search
- [ ] Budget allocated proportionally across selected primitives
- [ ] All primitive searches use same snapshot

---

## Epic 37: Fusion Infrastructure

**Goal**: Implement pluggable result fusion with RRF default

| Story | Description | Priority |
|-------|-------------|----------|
| #280 | Fuser Trait Definition | FOUNDATION |
| #281 | RRFFuser Implementation | CRITICAL |
| #282 | Tie-Breaking for Determinism | HIGH |
| #283 | Result Deduplication | HIGH |

**Acceptance Criteria**:
- [ ] Fuser trait with fuse(results, k) -> SearchResponse
- [ ] RRFFuser with k_rrf=60 constant
- [ ] RRF score: sum(1 / (k_rrf + rank)) across lists
- [ ] Tie-breaking for deterministic ordering
- [ ] Same DocRef from multiple primitives is deduplicated

---

## Epic 38: Optional Indexing

**Goal**: Implement opt-in inverted index for faster search

| Story | Description | Priority |
|-------|-------------|----------|
| #284 | InvertedIndex Structure | FOUNDATION |
| #285 | Enable/Disable Index Per Primitive | HIGH |
| #286 | Index Updates on Commit | HIGH |
| #287 | Index-Accelerated Search | HIGH |
| #288 | Index Version Watermark | HIGH |

**Acceptance Criteria**:
- [ ] InvertedIndex with token -> PostingList
- [ ] db.enable_search_index(primitive) API
- [ ] Index updates synchronously on commit
- [ ] Search uses index when enabled and up-to-date
- [ ] Falls back to scan when index stale
- [ ] Zero overhead when index disabled (lazy init)

---

## Epic 39: Validation & Non-Regression

**Goal**: Ensure correctness and maintain M5 performance

| Story | Description | Priority |
|-------|-------------|----------|
| #289 | Search API Contract Tests | CRITICAL |
| #290 | Non-Regression Benchmark Suite | CRITICAL |
| #291 | Determinism and Snapshot Consistency Tests | CRITICAL |

**Acceptance Criteria**:
- [ ] All 6 primitive search APIs tested
- [ ] Composite search tested with all primitive combinations
- [ ] Budget enforcement tested (time and candidates)
- [ ] Determinism: same request = identical results
- [ ] Snapshot consistency: search doesn't see concurrent writes
- [ ] KV, JSON, Event, State, Trace performance unchanged from M5
- [ ] Zero overhead when search not used (verified by benchmark)

---

## Files to Modify/Create

| File | Action | Description |
|------|--------|-------------|
| `crates/core/src/search_types.rs` | CREATE | SearchRequest, SearchResponse, DocRef, etc. |
| `crates/core/src/lib.rs` | MODIFY | Add search_types module |
| `crates/search/Cargo.toml` | CREATE | New search crate |
| `crates/search/src/lib.rs` | CREATE | Search crate root |
| `crates/search/src/scorer.rs` | CREATE | Scorer trait, BM25LiteScorer |
| `crates/search/src/tokenizer.rs` | CREATE | Basic tokenizer |
| `crates/search/src/fuser.rs` | CREATE | Fuser trait, RRFFuser |
| `crates/search/src/hybrid.rs` | CREATE | HybridSearch composite orchestrator |
| `crates/search/src/index.rs` | CREATE | InvertedIndex (optional) |
| `crates/primitives/src/kv.rs` | MODIFY | Add search() |
| `crates/primitives/src/json_store.rs` | MODIFY | Add search() |
| `crates/primitives/src/event.rs` | MODIFY | Add search() |
| `crates/primitives/src/state.rs` | MODIFY | Add search() |
| `crates/primitives/src/trace.rs` | MODIFY | Add search() |
| `crates/primitives/src/run_index.rs` | MODIFY | Add search() |
| `crates/engine/src/database.rs` | MODIFY | Add hybrid(), enable_search_index() |
| `Cargo.toml` (workspace) | MODIFY | Add search crate |

---

## Success Metrics

**Functional**: All 35 stories passing, 100% acceptance criteria met

**Performance**:
- Search without index (1K docs): < 50ms
- Search with index (1K docs): < 10ms
- KV/JSON/Event/State/Trace: No regression from M5
- Zero overhead when search not used

**Quality**: Test coverage > 90% for search crate
