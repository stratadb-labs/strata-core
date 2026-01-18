# M6 Architecture Specification: Retrieval Surfaces

**Version**: 1.1
**Status**: Implementation Ready
**Last Updated**: 2026-01-16

**Revision Notes**:
- v1.1: Added Evolution Warnings (Section 1.5), RetrievalPlan concept reservation (Section 18.0), extensible ScorerContext, JSON flattening warning

---

## Executive Summary

This document specifies the architecture for **Milestone 6 (M6): Retrieval Surfaces** of the in-memory agent database. M6 introduces a retrieval surface that **enables fast experimentation with search and ranking** across all primitives without baking search opinions into the engine.

**THIS DOCUMENT IS AUTHORITATIVE.** All M6 implementation must conform to this specification.

**Related Documents**:
- [M6 Spec](../../M6-spec.md) - Original design document
- [MILESTONES.md](../milestones/MILESTONES.md) - Project milestone tracking

**M6 Philosophy**:
> M6 builds **plumbing**, not **intelligence**.
>
> The retrieval surface must allow algorithm swaps at both primitive and composite levels without engine rewrites. We don't know the right retrieval approach yet. M6 ensures we can experiment.

**M6 Goals** (Surface Lock-In):
- Define stable search interfaces at primitive level
- Define stable composite search orchestration interface
- Establish pluggable scoring and fusion contracts
- Ship a "hello world" algorithm (BM25-lite + RRF) to validate the surface
- Maintain zero overhead when search is not used

**M6 Non-Goals** (Deferred):
- Vector search / embeddings (M9)
- Learning-to-rank / ML rerankers (research milestone)
- Full query DSL (M12)
- Complex analyzers (stemming, synonyms, language detection)
- Full-text highlighting, aggregations, faceting

**Critical Constraint**:
> M6 is a surface milestone, not a relevance milestone. The hello-world algorithm may produce mediocre results. That is acceptable. The interfaces matter more than the ranking quality. We can swap algorithms later.

**Built on M1-M5**:
- M1 provides: Storage (UnifiedStore), WAL, Recovery
- M2 provides: OCC transactions, Snapshot isolation, Conflict detection
- M3 provides: Five primitives (KVStore, EventLog, StateCell, TraceStore, RunIndex)
- M4 provides: Durability modes, performance optimizations, ShardedStore
- M5 provides: JsonStore primitive with path-level mutations
- M6 adds: Retrieval surface with primitive-native search and composite hybrid search

---

## Table of Contents

1. [Scope Boundaries](#1-scope-boundaries)
2. [THE SIX ARCHITECTURAL RULES](#2-the-six-architectural-rules-non-negotiable)
3. [Architecture Principles](#3-architecture-principles)
4. [Interface Invariants](#4-interface-invariants)
5. [Core Types](#5-core-types)
6. [Primitive Search Surface](#6-primitive-search-surface)
7. [Composite Search Surface](#7-composite-search-surface)
8. [Text Extraction](#8-text-extraction)
9. [Scoring Model](#9-scoring-model)
10. [Fusion Model](#10-fusion-model)
11. [Indexing Strategy](#11-indexing-strategy)
12. [Snapshot Consistency](#12-snapshot-consistency)
13. [Budget Enforcement](#13-budget-enforcement)
14. [API Design](#14-api-design)
15. [Performance Characteristics](#15-performance-characteristics)
16. [Testing Strategy](#16-testing-strategy)
17. [Known Limitations](#17-known-limitations)
18. [Future Extension Points](#18-future-extension-points)
19. [Appendix](#19-appendix)

---

## 1. Scope Boundaries

### 1.1 What M6 IS

M6 is a **surface lock-in milestone**. It defines:

| Aspect | M6 Commits To |
|--------|---------------|
| **Primitive search API** | `primitive.search(&SearchRequest) -> SearchResponse` |
| **Composite search API** | `db.hybrid.search(&SearchRequest) -> SearchResponse` |
| **Result reference model** | `DocRef` back-pointers to source records |
| **Budget model** | Time and candidate limits |
| **Pluggable contracts** | Scoring and fusion are swappable |

### 1.2 What M6 is NOT

M6 is **not** a relevance milestone. These are explicitly deferred:

| Deferred Item | Why Deferred | Target Milestone |
|---------------|--------------|------------------|
| Vector search | Separate primitive (HNSW, etc.) | M9 |
| ML rerankers | Research project | Future |
| Query DSL | Advanced feature | M12 |
| Analyzers (stemming, etc.) | Complexity, not core surface | Future |
| Highlighting | Convenience, not surface | Future |
| Aggregations/facets | Advanced feature | Future |
| Distributed search | Requires network layer | Post-M10 |

### 1.3 M6 Is Recall, Not Just Search

**Critical framing**: M6 is called "search" but is actually building **recall infrastructure**.

Search is just one recall mode. Humans do not retrieve memory by keyword alone. They retrieve by:
- Association
- Causality
- Recency
- Salience
- Similarity
- Goals
- Tasks
- Failure loops

M6 builds the surface that enables these modes. The keyword search in M6 is a "hello world" to validate the plumbing.

**Future trajectory**: `hybrid.search(query)` will eventually become `hybrid.recall(plan)`.

### 1.4 The Risk We Are Avoiding

Search over heterogeneous primitives is an unsolved problem. Different approaches exist:
- BM25 keyword matching
- Vector embeddings + similarity
- Learned sparse representations
- Cross-encoders
- Hybrid combinations

**We don't know which is right.** If we bake a specific approach into the engine, we're stuck.

M6 builds stable interfaces so we can swap algorithms without rewriting:
- Storage code
- Primitive APIs
- Transaction/snapshot logic
- WAL/recovery

**Rule**: If a feature requires committing to a specific retrieval algorithm, it is out of scope for M6.

### 1.5 Evolution Warnings

**These are explicit warnings about M6 design decisions that must not ossify:**

#### A. SearchRequest Must Not Become a Query DSL

The current `SearchRequest` with `query: String`, `mode`, filters is acceptable for M6. But this struct will become one of the most stable APIs in the system. Be conscious that it must evolve toward:

```rust
// Future direction (NOT M6):
pub enum QueryExpr {
    Keyword(String),
    Vector(Vec<f32>),
    Hybrid { keyword: String, vector: Vec<f32> },
    Programmatic(Box<dyn RetrievalProgram>),
}
```

The `mode` enum is a good start. Do not add fields to `SearchRequest` without considering long-term API stability.

#### B. JSON Flattening Is Temporary

Flattening JSON into a bag of words is a **lossy, temporary, baseline representation**. Future retrieval will need:
- Path-aware matching
- Field-weighted matching
- Schema-aware matching
- Role-based weighting
- Structural similarity

Do not bake "flattened bag of tokens" as the canonical mental model of JSON retrieval.

#### C. ScorerContext Will Expand

The current `ScorerContext` (total_docs, doc_freqs, avg_doc_len) is BM25-shaped. Future scorers will want:
- Recency curves
- Temporal priors
- Access frequency / write frequency
- User salience
- Causal depth
- Trace centrality
- Graph locality

`ScorerContext` must be extensible, not a fixed shape.

#### D. Fusion Will Become Planning

RRF is not the endgame. Agents will want:
- Stepwise recall
- Chain-based recall
- Query expansion
- Multi-hop recall
- Reasoned recall
- State-conditioned recall

The current pipeline: `Select → Search → Fuse → Return`

Eventually becomes: `Plan → Retrieve → Expand → Retrieve → Rerank → Reason → Retrieve → Fuse → Return`

This is why M6 treats retrieval as a research project.

#### E. Indexing Must Not Ossify

The inverted index is an internal optimization, not a conceptual pillar. Future retrieval will invent:
- Time-segmented indexes
- Recency-weighted indexes
- Role-based indexes
- Access-path indexes
- Structure-aware indexes
- Semantic caches

Rule 1 (No Data Movement) protects us here.

---

## 2. THE SIX ARCHITECTURAL RULES (NON-NEGOTIABLE)

**These rules MUST be followed in ALL M6 implementation. Violating any of these is a blocking issue.**

### Rule 1: No Unified Search Abstraction That Forces Data Movement

> **Composite search must NOT require copying primitive state into a new store. Search runs against each primitive's native storage.**

```rust
// CORRECT: Search uses primitive's native storage
impl KVStore {
    pub fn search(&self, req: &SearchRequest) -> Result<SearchResponse> {
        // Searches directly against ShardedStore
        let snapshot = self.db.snapshot();
        // ... search logic using snapshot ...
    }
}

// WRONG: Copying data to a search-specific store
struct SearchEngine {
    unified_index: DashMap<String, SearchDoc>,  // NEVER DO THIS
}
```

**Why**: Data movement creates consistency issues, increases memory, and couples us to a specific indexing strategy.

### Rule 2: Primitive Search Is a First-Class API

> **Each primitive MUST have its own direct `.search()` method.**

```rust
// CORRECT: Each primitive has search
db.kv.search(&req)?;
db.json.search(&req)?;
db.event.search(&req)?;
db.state.search(&req)?;
db.trace.search(&req)?;
db.run_index.search(&req)?;

// WRONG: Only composite search exists
db.search(&req)?;  // Where does this search? Unclear.
```

**Why**: Enables per-primitive algorithm specialization. KV search can use different strategies than Event search.

### Rule 3: Composite Search Orchestrates, It Does Not Replace

> **`db.hybrid.search()` is a planner and fusion layer. It does NOT own indexing, conflict semantics, or storage.**

```rust
// CORRECT: Hybrid search calls primitive searches
impl HybridSearch {
    pub fn search(&self, req: &SearchRequest) -> Result<SearchResponse> {
        let kv_results = self.db.kv.search(req)?;
        let json_results = self.db.json.search(req)?;
        // ... fuse results ...
    }
}

// WRONG: Hybrid search has its own index
impl HybridSearch {
    index: UnifiedIndex,  // NEVER DO THIS
}
```

**Why**: Keeps primitives authoritative. Composite search is composition, not replacement.

### Rule 4: Search Must Be Snapshot-Consistent

> **Search executes against a SnapshotView. Results are stable for that search invocation.**

```rust
// CORRECT: Search uses snapshot
impl KVStore {
    pub fn search(&self, req: &SearchRequest) -> Result<SearchResponse> {
        let snapshot = self.db.snapshot();  // Point-in-time view
        // All search operations use this snapshot
    }
}

// WRONG: Search sees interleaved writes
impl KVStore {
    pub fn search(&self, req: &SearchRequest) -> Result<SearchResponse> {
        // Reading directly from live storage during iteration
        for key in self.db.storage().keys() {  // NEVER DO THIS
            // May see partial writes
        }
    }
}
```

**Why**: Search results must be deterministic and consistent. Interleaved writes would cause torn reads.

### Rule 5: Zero Overhead When Not Used

> **If no search APIs are invoked: no extra allocations, no write amplification, no background work.**

```rust
// CORRECT: Lazy initialization
pub struct KVStore {
    db: Arc<Database>,
    // No search-related fields - they live in Database if needed
}

// Index only created when search is first used or explicitly enabled
impl Database {
    fn ensure_search_index(&self) {
        self.search_index.get_or_init(|| SearchIndex::new());
    }
}

// WRONG: Always allocating search structures
pub struct KVStore {
    db: Arc<Database>,
    search_index: InvertedIndex,  // NEVER DO THIS - always allocated
}
```

**Why**: Most operations don't use search. Non-search paths must pay zero cost.

### Rule 6: The Surface Must Enable Algorithm Swaps Without Engine Rewrites

> **Scoring, fusion, and candidate generation must be behind stable interfaces.**

```rust
// CORRECT: Pluggable scorer
pub trait Scorer: Send + Sync {
    fn score(&self, doc: &SearchDoc, query: &str) -> f32;
}

// M6 ships with BM25Lite
pub struct BM25LiteScorer { ... }
impl Scorer for BM25LiteScorer { ... }

// Future: Can swap to learned scorer
pub struct LearnedScorer { model: Model }
impl Scorer for LearnedScorer { ... }

// WRONG: Hardcoded scoring
fn score(doc: &SearchDoc, query: &str) -> f32 {
    // BM25 calculation directly embedded
    // Can't swap without rewriting
}
```

**Why**: This is the entire point of M6. Algorithm experimentation requires stable interfaces.

---

## 3. Architecture Principles

### 3.1 M6-Specific Principles

1. **Surface Over Relevance**
   - M6 may produce mediocre search results. That is acceptable.
   - Interface stability matters more than ranking quality.
   - Relevance improvements happen by swapping algorithms, not changing interfaces.

2. **Primitive Authority**
   - Each primitive owns its search implementation.
   - Primitives decide how to enumerate candidates.
   - Primitives decide how to extract searchable text.

3. **Composition Over Integration**
   - Hybrid search composes primitive searches.
   - No "god object" that knows about all primitives.
   - Adding a new primitive = adding a new search method + fusion entry.

4. **Lazy Everything**
   - Index creation: only when search is used or explicitly enabled.
   - Index updates: only for primitives that opted in.
   - Memory allocation: only when needed.

5. **Budget-Bounded Execution**
   - All search operations respect time and candidate budgets.
   - Graceful degradation over timeout errors.
   - Truncated results are clearly marked.

6. **Deterministic Results**
   - Same snapshot + same request = identical ordered results.
   - No randomness in scoring or fusion.
   - Reproducible for testing and debugging.

### 3.2 What Search Is NOT

| Misconception | Reality |
|---------------|---------|
| "Search replaces get/list" | Search is for fuzzy matching; get/list for exact access |
| "Search needs ML" | M6 uses simple keyword matching |
| "Search is always fast" | Budget enforcement may truncate results |
| "Search results are authoritative" | DocRef must be dereferenced for actual data |
| "One search algorithm fits all" | Different primitives may need different strategies |

---

## 4. Interface Invariants (Never Change)

This section defines interface invariants that **MUST hold for all future milestones**. Implementations may change, but these contracts must not.

### 4.1 SearchRequest Is Primitive-Agnostic

The same `SearchRequest` type is used for all primitive searches and composite search.

```rust
pub struct SearchRequest {
    pub run_id: RunId,
    pub query: String,
    pub k: usize,
    pub budget: SearchBudget,
    // ... filters ...
}
```

**This invariant must not change.**

### 4.2 SearchResponse Contains DocRef Back-Pointers

Search results MUST contain `DocRef` that can be used to retrieve the actual data.

```rust
pub struct SearchHit {
    pub ref_: DocRef,  // Back-pointer to source
    pub score: f32,
    pub rank: u32,
    // ...
}
```

**This invariant must not change.**

### 4.3 DocRef Is Exhaustive Over Primitives

`DocRef` MUST have a variant for every searchable primitive.

```rust
pub enum DocRef {
    Kv { key: Key },
    Json { key: Key, doc_id: JsonDocId },
    Event { log_key: Key, seq: u64 },
    State { key: Key },
    Trace { key: Key, span_id: u64 },
    Run { run_id: RunId },
}
```

When a new primitive is added, `DocRef` MUST be extended.

**This invariant must not change.**

### 4.4 Primitive Search Returns Same Type as Composite Search

All search methods return `SearchResponse`. No primitive-specific result types.

```rust
impl KVStore {
    pub fn search(&self, req: &SearchRequest) -> Result<SearchResponse>;
}

impl JsonStore {
    pub fn search(&self, req: &SearchRequest) -> Result<SearchResponse>;
}

impl HybridSearch {
    pub fn search(&self, req: &SearchRequest) -> Result<SearchResponse>;
}
```

**This invariant must not change.**

### 4.5 Budget Is Always Enforced

Search MUST respect budget limits. Results may be truncated, never timeout errors.

```rust
pub struct SearchResponse {
    pub hits: Vec<SearchHit>,
    pub truncated: bool,  // True if budget caused early stop
    pub stats: SearchStats,
}
```

**This invariant must not change.**

---

## 5. Core Types

### 5.1 SearchRequest

```rust
/// Request for search across primitives
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

    /// Optional: limit to specific primitives
    pub primitive_filter: Option<Vec<PrimitiveKind>>,

    /// Optional: time range filter (microseconds)
    pub time_range: Option<(u64, u64)>,

    /// Optional: tag filter (match any)
    pub tags_any: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchMode {
    /// Keyword-based search (M6 default)
    Keyword,
    /// Reserved for future vector search
    Vector,
    /// Reserved for future hybrid
    Hybrid,
}

impl Default for SearchMode {
    fn default() -> Self {
        SearchMode::Keyword
    }
}
```

### 5.2 SearchBudget

```rust
/// Limits on search execution
#[derive(Debug, Clone, Copy)]
pub struct SearchBudget {
    /// Hard stop on wall time (microseconds)
    pub max_wall_time_micros: u64,

    /// Maximum total candidates to consider
    pub max_candidates: usize,

    /// Maximum candidates per primitive
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
```

### 5.3 SearchResponse

```rust
/// Search results
#[derive(Debug, Clone)]
pub struct SearchResponse {
    /// Ranked hits
    pub hits: Vec<SearchHit>,

    /// True if budget caused early termination
    pub truncated: bool,

    /// Execution statistics
    pub stats: SearchStats,
}

#[derive(Debug, Clone)]
pub struct SearchHit {
    /// Back-pointer to source record
    pub ref_: DocRef,

    /// Score from scorer (higher = more relevant)
    pub score: f32,

    /// Rank in result set (1-indexed)
    pub rank: u32,

    /// Optional snippet for display
    pub snippet: Option<String>,

    /// Debug info (off by default)
    pub debug: Option<HitDebug>,
}

#[derive(Debug, Clone)]
pub struct SearchStats {
    /// Time spent in search (microseconds)
    pub elapsed_micros: u64,

    /// Candidates considered
    pub candidates_considered: usize,

    /// Candidates per primitive
    pub candidates_by_primitive: HashMap<PrimitiveKind, usize>,

    /// Whether index was used
    pub index_used: bool,
}

#[derive(Debug, Clone)]
pub struct HitDebug {
    /// Raw score before normalization
    pub raw_score: f32,

    /// Score components
    pub components: HashMap<String, f32>,

    /// Which primitive this came from
    pub source_primitive: PrimitiveKind,
}
```

### 5.4 DocRef

```rust
/// Reference back to source record
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
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PrimitiveKind {
    Kv,
    Json,
    Event,
    State,
    Trace,
    Run,
}
```

### 5.5 SearchDoc (Internal)

```rust
/// Ephemeral searchable view of a record (not stored)
#[derive(Debug, Clone)]
pub(crate) struct SearchDoc {
    /// Back-pointer to source
    pub ref_: DocRef,

    /// Source primitive
    pub primitive: PrimitiveKind,

    /// Run this belongs to
    pub run_id: RunId,

    /// Optional title/label
    pub title: Option<String>,

    /// Primary searchable text
    pub body: String,

    /// Tags for filtering
    pub tags: Vec<String>,

    /// Timestamp for recency (microseconds)
    pub ts_micros: Option<u64>,

    /// Size in bytes (for scoring signals)
    pub bytes: Option<u32>,
}
```

---

## 6. Primitive Search Surface

### 6.1 Search Trait

Each primitive implements search:

```rust
/// Trait for searchable primitives
pub trait Searchable {
    /// Search within this primitive
    fn search(&self, req: &SearchRequest) -> Result<SearchResponse>;

    /// Get primitive kind
    fn primitive_kind(&self) -> PrimitiveKind;
}
```

### 6.2 KVStore Search

```rust
impl KVStore {
    /// Search KV entries
    ///
    /// Extracts text from string values and serialized maps/arrays.
    pub fn search(&self, req: &SearchRequest) -> Result<SearchResponse> {
        let start = Instant::now();
        let snapshot = self.db.snapshot();
        let mut candidates = Vec::new();
        let mut stats = SearchStats::default();

        // Enumerate candidates from snapshot
        for (key, value) in snapshot.scan_kv(req.run_id)? {
            if self.check_budget(&start, &req.budget, candidates.len()) {
                stats.truncated = true;
                break;
            }

            let doc = self.extract_search_doc(&key, &value)?;
            if self.matches_filters(&doc, req) {
                candidates.push(doc);
            }
        }

        // Score and rank
        let hits = self.score_and_rank(candidates, req)?;

        Ok(SearchResponse {
            hits,
            truncated: stats.truncated,
            stats,
        })
    }

    fn extract_search_doc(&self, key: &Key, value: &Value) -> Result<SearchDoc> {
        let body = match value {
            Value::String(s) => s.clone(),
            Value::Bytes(b) => String::from_utf8_lossy(b).into_owned(),
            other => serde_json::to_string(other)?,
        };

        Ok(SearchDoc {
            ref_: DocRef::Kv { key: key.clone() },
            primitive: PrimitiveKind::Kv,
            run_id: key.namespace().run_id(),
            title: Some(key.user_key_str().to_string()),
            body,
            tags: vec![],
            ts_micros: None,
            bytes: Some(body.len() as u32),
        })
    }
}
```

### 6.3 JsonStore Search

```rust
impl JsonStore {
    /// Search JSON documents
    ///
    /// Flattens JSON structure into searchable text.
    pub fn search(&self, req: &SearchRequest) -> Result<SearchResponse> {
        let start = Instant::now();
        let snapshot = self.db.snapshot();
        let mut candidates = Vec::new();

        for (key, doc) in snapshot.scan_json(req.run_id)? {
            if self.check_budget(&start, &req.budget, candidates.len()) {
                break;
            }

            let search_doc = self.extract_search_doc(&key, &doc)?;
            if self.matches_filters(&search_doc, req) {
                candidates.push(search_doc);
            }
        }

        let hits = self.score_and_rank(candidates, req)?;
        Ok(SearchResponse { hits, .. })
    }

    fn extract_search_doc(&self, key: &Key, doc: &JsonDoc) -> Result<SearchDoc> {
        // Flatten JSON into searchable text
        let body = self.flatten_json(&doc.value);

        Ok(SearchDoc {
            ref_: DocRef::Json {
                key: key.clone(),
                doc_id: doc.doc_id
            },
            primitive: PrimitiveKind::Json,
            run_id: key.namespace().run_id(),
            title: None,
            body,
            tags: vec![],
            ts_micros: Some(doc.modified_at),
            bytes: Some(doc.data.len() as u32),
        })
    }

    fn flatten_json(&self, value: &JsonValue) -> String {
        // Recursively extract all string values and "key: value" pairs
        let mut parts = Vec::new();
        self.flatten_recursive(value, &mut parts, "");
        parts.join(" ")
    }
}
```

### 6.4 EventLog Search

```rust
impl EventLog {
    /// Search events
    ///
    /// Can prioritize recent events via timestamp.
    pub fn search(&self, req: &SearchRequest) -> Result<SearchResponse> {
        let snapshot = self.db.snapshot();
        let mut candidates = Vec::new();

        // Scan events, optionally in reverse chronological order
        for event in snapshot.scan_events_reverse(req.run_id)? {
            // ... extract and score ...
        }

        // ...
    }
}
```

### 6.5 Other Primitives

StateCell, TraceStore, and RunIndex follow the same pattern:
- Implement `search(&SearchRequest) -> Result<SearchResponse>`
- Extract searchable text appropriate to their data model
- Respect budget limits
- Return `DocRef` back-pointers

---

## 7. Composite Search Surface

### 7.1 HybridSearch API

```rust
impl Database {
    /// Get hybrid search interface
    pub fn hybrid(&self) -> HybridSearch {
        HybridSearch { db: self.clone() }
    }
}

/// Composite search orchestrator
#[derive(Clone)]
pub struct HybridSearch {
    db: Arc<Database>,
}

impl HybridSearch {
    /// Search across all primitives and fuse results
    pub fn search(&self, req: &SearchRequest) -> Result<SearchResponse> {
        let start = Instant::now();

        // Determine which primitives to search
        let primitives = self.select_primitives(req);

        // Allocate per-primitive budgets
        let budgets = self.allocate_budgets(req, &primitives);

        // Execute primitive searches (same snapshot)
        let snapshot = self.db.snapshot();
        let mut primitive_results = Vec::new();

        for (primitive, budget) in primitives.iter().zip(budgets.iter()) {
            let sub_req = req.with_budget(*budget);
            let result = self.search_primitive(*primitive, &sub_req, &snapshot)?;
            primitive_results.push((*primitive, result));
        }

        // Fuse results
        let fused = self.fuse_results(primitive_results, req)?;

        Ok(fused)
    }

    fn select_primitives(&self, req: &SearchRequest) -> Vec<PrimitiveKind> {
        match &req.primitive_filter {
            Some(filter) => filter.clone(),
            None => vec![
                PrimitiveKind::Kv,
                PrimitiveKind::Json,
                PrimitiveKind::Event,
                PrimitiveKind::State,
                PrimitiveKind::Trace,
                PrimitiveKind::Run,
            ],
        }
    }

    fn allocate_budgets(&self, req: &SearchRequest, primitives: &[PrimitiveKind]) -> Vec<SearchBudget> {
        let n = primitives.len();
        let per_primitive = SearchBudget {
            max_wall_time_micros: req.budget.max_wall_time_micros / n as u64,
            max_candidates: req.budget.max_candidates_per_primitive,
            max_candidates_per_primitive: req.budget.max_candidates_per_primitive,
        };
        vec![per_primitive; n]
    }
}
```

### 7.2 Planner Model

Composite search is a tiny query planner:

1. **Select**: Determine which primitives to query based on filters
2. **Budget**: Allocate time and candidate limits per primitive
3. **Execute**: Run primitive searches against same snapshot
4. **Fuse**: Combine results using fusion algorithm
5. **Return**: Top-k results

```
SearchRequest
     │
     ▼
┌─────────────┐
│  Planner    │
│  - select   │
│  - budget   │
└──────┬──────┘
       │
       ├─────────────────┬─────────────────┐
       ▼                 ▼                 ▼
┌──────────────┐ ┌──────────────┐ ┌──────────────┐
│ kv.search()  │ │json.search() │ │event.search()│
└──────┬───────┘ └──────┬───────┘ └──────┬───────┘
       │                │                │
       └────────────────┴────────────────┘
                        │
                        ▼
                 ┌──────────────┐
                 │    Fuser     │
                 │  (RRF/etc)   │
                 └──────┬───────┘
                        │
                        ▼
                 SearchResponse
```

---

## 8. Text Extraction

### 8.1 Per-Primitive Extractors

Each primitive decides how to extract searchable text:

| Primitive | Extraction Strategy |
|-----------|---------------------|
| **KV** | String values directly; JSON stringify for maps/arrays |
| **JSON** | Flatten all scalar strings + "key: value" pairs + paths |
| **Event** | Event type + payload stringified |
| **State** | State name + current value stringified |
| **Trace** | Span name + attributes stringified |
| **Run** | Run ID + status + metadata stringified |

### 8.2 JSON Flattening

> **WARNING: TEMPORARY BASELINE**
>
> Flattening JSON into a bag of words is a **lossy, temporary representation** acceptable for M6.
>
> This approach loses:
> - Path structure ($.user.name vs $.admin.name)
> - Field semantics (title vs description)
> - Type information (string "123" vs number 123)
> - Nesting depth signals
> - Array position meaning
>
> Future retrieval will need:
> - **Path-aware matching**: Match `$.user.email` specifically
> - **Field-weighted matching**: Title matches worth more than body
> - **Schema-aware matching**: Understand field types and roles
> - **Structural similarity**: Documents with similar shape
>
> The TextExtractor trait exists precisely to swap this out. Do NOT treat flattening as canonical.

```rust
impl JsonStore {
    fn flatten_recursive(
        &self,
        value: &JsonValue,
        parts: &mut Vec<String>,
        path: &str,
    ) {
        match value {
            JsonValue::String(s) => {
                if !path.is_empty() {
                    parts.push(format!("{}: {}", path, s));
                }
                parts.push(s.clone());
            }
            JsonValue::Number(n) => {
                parts.push(format!("{}: {}", path, n));
            }
            JsonValue::Bool(b) => {
                parts.push(format!("{}: {}", path, b));
            }
            JsonValue::Array(arr) => {
                for (i, item) in arr.iter().enumerate() {
                    let child_path = format!("{}[{}]", path, i);
                    self.flatten_recursive(item, parts, &child_path);
                }
            }
            JsonValue::Object(map) => {
                for (k, v) in map.iter() {
                    let child_path = if path.is_empty() {
                        k.clone()
                    } else {
                        format!("{}.{}", path, k)
                    };
                    self.flatten_recursive(v, parts, &child_path);
                }
            }
            JsonValue::Null => {}
        }
    }
}
```

### 8.3 Extensibility

Text extraction is per-primitive and can be customized:

```rust
/// Trait for customizing text extraction
pub trait TextExtractor: Send + Sync {
    fn extract(&self, value: &Value) -> String;
}

// Default: JSON stringify
pub struct DefaultExtractor;

// Custom: Application-specific
pub struct MyAppExtractor;
impl TextExtractor for MyAppExtractor {
    fn extract(&self, value: &Value) -> String {
        // Custom logic
    }
}
```

---

## 9. Scoring Model

### 9.1 Scorer Trait

```rust
/// Pluggable scoring interface
pub trait Scorer: Send + Sync {
    /// Score a document against a query
    fn score(&self, doc: &SearchDoc, query: &str, ctx: &ScorerContext) -> f32;

    /// Name for debugging
    fn name(&self) -> &str;
}

/// Scoring context - MUST remain extensible
///
/// WARNING: This struct is BM25-shaped for M6. Future scorers will need:
/// - Recency curves, temporal priors
/// - Access/write frequency signals
/// - User salience, causal depth
/// - Trace centrality, graph locality
///
/// Do NOT treat these fields as the complete context.
/// Future versions will add an `extensions: HashMap<String, Box<dyn Any>>` or similar.
pub struct ScorerContext {
    /// Total documents in corpus (for IDF)
    pub total_docs: usize,

    /// Document frequency per term (for IDF)
    pub doc_freqs: HashMap<String, usize>,

    /// Average document length (for BM25)
    pub avg_doc_len: f32,

    /// Current timestamp for recency calculations
    pub now_micros: u64,

    /// Extension point for future context signals
    /// M6: unused. Reserved for future scorer requirements.
    pub extensions: HashMap<String, serde_json::Value>,
}
```

### 9.2 BM25-Lite Scorer (M6 Default)

```rust
/// Simple BM25-inspired scorer for M6
pub struct BM25LiteScorer {
    /// k1 parameter (term frequency saturation)
    k1: f32,
    /// b parameter (length normalization)
    b: f32,
}

impl Default for BM25LiteScorer {
    fn default() -> Self {
        BM25LiteScorer { k1: 1.2, b: 0.75 }
    }
}

impl Scorer for BM25LiteScorer {
    fn score(&self, doc: &SearchDoc, query: &str, ctx: &ScorerContext) -> f32 {
        let query_terms = tokenize(query);
        let doc_terms = tokenize(&doc.body);
        let doc_len = doc_terms.len() as f32;

        let mut score = 0.0;

        for term in &query_terms {
            // Term frequency in document
            let tf = doc_terms.iter().filter(|t| *t == term).count() as f32;

            // Inverse document frequency
            let df = ctx.doc_freqs.get(term).copied().unwrap_or(0) as f32;
            let idf = ((ctx.total_docs as f32 - df + 0.5) / (df + 0.5) + 1.0).ln();

            // BM25 term score
            let tf_norm = (tf * (self.k1 + 1.0)) /
                (tf + self.k1 * (1.0 - self.b + self.b * doc_len / ctx.avg_doc_len));

            score += idf * tf_norm;
        }

        // Optional recency boost
        if let Some(ts) = doc.ts_micros {
            let age_hours = (now_micros() - ts) as f32 / 3_600_000_000.0;
            let recency_boost = 1.0 / (1.0 + age_hours / 24.0);  // Decay over 24h
            score *= 1.0 + 0.1 * recency_boost;  // Max 10% boost
        }

        score
    }

    fn name(&self) -> &str {
        "bm25-lite"
    }
}

fn tokenize(text: &str) -> Vec<String> {
    text.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|s| s.len() >= 2)
        .map(String::from)
        .collect()
}
```

### 9.3 Scorer Registry

```rust
/// Registry of available scorers
pub struct ScorerRegistry {
    scorers: HashMap<String, Arc<dyn Scorer>>,
    default: String,
}

impl ScorerRegistry {
    pub fn new() -> Self {
        let mut scorers = HashMap::new();
        scorers.insert("bm25-lite".to_string(), Arc::new(BM25LiteScorer::default()) as Arc<dyn Scorer>);

        ScorerRegistry {
            scorers,
            default: "bm25-lite".to_string(),
        }
    }

    pub fn get(&self, name: &str) -> Option<Arc<dyn Scorer>> {
        self.scorers.get(name).cloned()
    }

    pub fn register(&mut self, name: &str, scorer: Arc<dyn Scorer>) {
        self.scorers.insert(name.to_string(), scorer);
    }
}
```

---

## 10. Fusion Model

### 10.1 Fuser Trait

```rust
/// Pluggable fusion interface
pub trait Fuser: Send + Sync {
    /// Fuse results from multiple primitives
    fn fuse(
        &self,
        results: Vec<(PrimitiveKind, SearchResponse)>,
        k: usize,
    ) -> SearchResponse;

    /// Name for debugging
    fn name(&self) -> &str;
}
```

### 10.2 RRF Fuser (M6 Default)

```rust
/// Reciprocal Rank Fusion
pub struct RRFFuser {
    /// RRF constant (default 60)
    k_rrf: u32,
}

impl Default for RRFFuser {
    fn default() -> Self {
        RRFFuser { k_rrf: 60 }
    }
}

impl Fuser for RRFFuser {
    fn fuse(
        &self,
        results: Vec<(PrimitiveKind, SearchResponse)>,
        k: usize,
    ) -> SearchResponse {
        // Collect all hits with their source ranks
        let mut rrf_scores: HashMap<DocRef, f32> = HashMap::new();
        let mut hit_data: HashMap<DocRef, SearchHit> = HashMap::new();

        for (primitive, response) in results {
            for hit in response.hits {
                // RRF score: 1 / (k + rank)
                let rrf_contribution = 1.0 / (self.k_rrf as f32 + hit.rank as f32);
                *rrf_scores.entry(hit.ref_.clone()).or_insert(0.0) += rrf_contribution;

                // Keep the hit data (use first occurrence)
                hit_data.entry(hit.ref_.clone()).or_insert(hit);
            }
        }

        // Sort by RRF score
        let mut scored: Vec<_> = rrf_scores.into_iter().collect();
        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(Ordering::Equal));

        // Build final hits
        let hits: Vec<SearchHit> = scored
            .into_iter()
            .take(k)
            .enumerate()
            .map(|(i, (ref_, rrf_score))| {
                let mut hit = hit_data.remove(&ref_).unwrap();
                hit.score = rrf_score;
                hit.rank = (i + 1) as u32;
                hit
            })
            .collect();

        SearchResponse {
            hits,
            truncated: false,  // Fusion doesn't truncate
            stats: SearchStats::default(),
        }
    }

    fn name(&self) -> &str {
        "rrf"
    }
}
```

### 10.3 Tie-Breaking

RRF with tie-breaking for determinism:

```rust
impl RRFFuser {
    fn fuse_with_tiebreak(
        &self,
        results: Vec<(PrimitiveKind, SearchResponse)>,
        k: usize,
    ) -> SearchResponse {
        // ... RRF calculation ...

        // Final score with small tie-break from original score
        for (ref_, hit) in &hit_data {
            let rrf_score = rrf_scores.get(ref_).unwrap();
            // Add tiny amount of original score for deterministic ordering
            let final_score = rrf_score + 0.0001 * hit.score;
            // ...
        }
    }
}
```

---

## 11. Indexing Strategy

### 11.1 Index Is Optional

M6 indexing is **opt-in per primitive**. Without indexing, search falls back to scan.

```rust
impl Database {
    /// Enable search indexing for a primitive
    pub fn enable_search_index(&self, primitive: PrimitiveKind) -> Result<()> {
        match primitive {
            PrimitiveKind::Kv => self.kv_index.enable(),
            PrimitiveKind::Json => self.json_index.enable(),
            // ...
        }
    }
}
```

### 11.2 Inverted Index Structure

```rust
/// Simple inverted index for M6
pub(crate) struct InvertedIndex {
    /// Token -> posting list
    postings: DashMap<String, PostingList>,

    /// Document frequency (for IDF)
    doc_freqs: DashMap<String, usize>,

    /// Total documents indexed
    total_docs: AtomicUsize,

    /// Enabled flag
    enabled: AtomicBool,

    /// Version watermark (for snapshot consistency)
    version: AtomicU64,
}

struct PostingList {
    /// DocRef -> (term frequency, doc length, timestamp)
    entries: Vec<PostingEntry>,
}

struct PostingEntry {
    doc_ref: DocRef,
    tf: u32,
    doc_len: u32,
    ts_micros: Option<u64>,
}
```

### 11.3 Index Updates

Index updates happen synchronously on commit:

```rust
impl InvertedIndex {
    /// Update index for a write
    pub fn on_commit(&self, writes: &[WriteEntry]) {
        if !self.enabled.load(Ordering::Acquire) {
            return;  // Index not enabled
        }

        for write in writes {
            match &write.operation {
                WriteOp::Put { key, value } => {
                    let doc = self.extract_doc(key, value);
                    self.index_doc(&doc);
                }
                WriteOp::Delete { key } => {
                    self.remove_doc(key);
                }
            }
        }

        // Bump version watermark
        self.version.fetch_add(1, Ordering::Release);
    }

    fn index_doc(&self, doc: &SearchDoc) {
        let tokens = tokenize(&doc.body);
        let doc_len = tokens.len() as u32;

        // Count term frequencies
        let mut tf_map: HashMap<String, u32> = HashMap::new();
        for token in tokens {
            *tf_map.entry(token).or_insert(0) += 1;
        }

        // Update postings
        for (term, tf) in tf_map {
            self.postings
                .entry(term.clone())
                .or_insert_with(|| PostingList { entries: vec![] })
                .entries
                .push(PostingEntry {
                    doc_ref: doc.ref_.clone(),
                    tf,
                    doc_len,
                    ts_micros: doc.ts_micros,
                });

            // Update doc frequency
            self.doc_freqs
                .entry(term)
                .and_modify(|c| *c += 1)
                .or_insert(1);
        }

        self.total_docs.fetch_add(1, Ordering::Relaxed);
    }
}
```

### 11.4 Index-Accelerated Search

```rust
impl KVStore {
    fn search_with_index(
        &self,
        req: &SearchRequest,
        index: &InvertedIndex,
    ) -> Result<SearchResponse> {
        let query_terms = tokenize(&req.query);

        // Collect candidates from postings
        let mut candidates: HashMap<DocRef, f32> = HashMap::new();

        for term in &query_terms {
            if let Some(postings) = index.postings.get(term) {
                let idf = self.compute_idf(term, index);

                for entry in &postings.entries {
                    let tf_norm = entry.tf as f32 / (entry.tf as f32 + 1.0);
                    *candidates.entry(entry.doc_ref.clone()).or_insert(0.0) += idf * tf_norm;
                }
            }
        }

        // Convert to hits
        let hits = self.candidates_to_hits(candidates, req.k)?;

        Ok(SearchResponse { hits, truncated: false, stats: SearchStats { index_used: true, .. } })
    }
}
```

---

## 12. Snapshot Consistency

### 12.1 Search Uses Snapshot

All search operations use the same snapshot:

```rust
impl HybridSearch {
    pub fn search(&self, req: &SearchRequest) -> Result<SearchResponse> {
        // Single snapshot for entire search
        let snapshot = self.db.snapshot();

        // All primitive searches use this snapshot
        let kv_results = self.db.kv.search_with_snapshot(req, &snapshot)?;
        let json_results = self.db.json.search_with_snapshot(req, &snapshot)?;
        // ...
    }
}
```

### 12.2 Index Version Watermark

If using index, search verifies index is up-to-date:

```rust
impl KVStore {
    fn search_with_index(
        &self,
        req: &SearchRequest,
        index: &InvertedIndex,
        snapshot: &Snapshot,
    ) -> Result<SearchResponse> {
        // Check index is consistent with snapshot
        let index_version = index.version.load(Ordering::Acquire);
        let snapshot_version = snapshot.version();

        if index_version < snapshot_version {
            // Index is stale - fall back to scan
            // (or wait for index to catch up)
            return self.search_with_scan(req, snapshot);
        }

        // Index is up-to-date, use it
        // ...
    }
}
```

### 12.3 Determinism

Same snapshot + same request = identical results:

```rust
#[test]
fn test_search_determinism() {
    let db = test_db();
    let snapshot = db.snapshot();

    let req = SearchRequest { query: "test".into(), k: 10, .. };

    let result1 = db.hybrid().search_with_snapshot(&req, &snapshot)?;
    let result2 = db.hybrid().search_with_snapshot(&req, &snapshot)?;

    assert_eq!(result1.hits, result2.hits);
}
```

---

## 13. Budget Enforcement

### 13.1 Time Budget

```rust
fn check_time_budget(start: Instant, budget: &SearchBudget) -> bool {
    start.elapsed().as_micros() as u64 >= budget.max_wall_time_micros
}
```

### 13.2 Candidate Budget

```rust
fn check_candidate_budget(count: usize, budget: &SearchBudget) -> bool {
    count >= budget.max_candidates
}
```

### 13.3 Budget Enforcement in Search

```rust
impl KVStore {
    fn search(&self, req: &SearchRequest) -> Result<SearchResponse> {
        let start = Instant::now();
        let mut candidates = Vec::new();
        let mut truncated = false;

        for (key, value) in self.scan()? {
            // Check time budget
            if check_time_budget(start, &req.budget) {
                truncated = true;
                break;
            }

            // Check candidate budget
            if check_candidate_budget(candidates.len(), &req.budget) {
                truncated = true;
                break;
            }

            // Process candidate
            candidates.push(self.extract_doc(&key, &value)?);
        }

        Ok(SearchResponse {
            hits: self.score_and_rank(candidates, req.k)?,
            truncated,
            stats: SearchStats {
                elapsed_micros: start.elapsed().as_micros() as u64,
                candidates_considered: candidates.len(),
                ..
            },
        })
    }
}
```

### 13.4 Graceful Degradation

Budget enforcement never throws errors, only marks results as truncated:

```rust
// Application code
let result = db.hybrid().search(&req)?;

if result.truncated {
    // Warn user that results may be incomplete
    warn!("Search truncated due to budget");
}

// Results are still usable
for hit in result.hits {
    // ...
}
```

---

## 14. API Design

### 14.1 Primitive Search APIs

```rust
impl KVStore {
    pub fn search(&self, req: &SearchRequest) -> Result<SearchResponse>;
}

impl JsonStore {
    pub fn search(&self, req: &SearchRequest) -> Result<SearchResponse>;
}

impl EventLog {
    pub fn search(&self, req: &SearchRequest) -> Result<SearchResponse>;
}

impl StateCell {
    pub fn search(&self, req: &SearchRequest) -> Result<SearchResponse>;
}

impl TraceStore {
    pub fn search(&self, req: &SearchRequest) -> Result<SearchResponse>;
}

impl RunIndex {
    pub fn search(&self, req: &SearchRequest) -> Result<SearchResponse>;
}
```

### 14.2 Composite Search API

```rust
impl Database {
    pub fn hybrid(&self) -> HybridSearch;
}

impl HybridSearch {
    pub fn search(&self, req: &SearchRequest) -> Result<SearchResponse>;
}
```

### 14.3 Configuration API

```rust
impl Database {
    /// Enable search indexing for faster searches
    pub fn enable_search_index(&self, primitive: PrimitiveKind) -> Result<()>;

    /// Disable search indexing
    pub fn disable_search_index(&self, primitive: PrimitiveKind) -> Result<()>;

    /// Check if indexing is enabled
    pub fn is_search_index_enabled(&self, primitive: PrimitiveKind) -> bool;
}
```

### 14.4 DocRef Dereferencing

```rust
impl Database {
    /// Retrieve the actual data for a search hit
    pub fn deref_hit(&self, hit: &SearchHit) -> Result<Value> {
        match &hit.ref_ {
            DocRef::Kv { key } => self.kv.get_by_key(key),
            DocRef::Json { key, doc_id } => self.json.get_by_key(key, doc_id),
            DocRef::Event { log_key, seq } => self.event.get_by_seq(log_key, *seq),
            DocRef::State { key } => self.state.get_by_key(key),
            DocRef::Trace { key, span_id } => self.trace.get_span(key, *span_id),
            DocRef::Run { run_id } => self.run_index.get_run(*run_id),
        }
    }
}
```

### 14.5 Usage Example

```rust
// Search across all primitives
let req = SearchRequest {
    run_id,
    query: "authentication error".to_string(),
    k: 20,
    budget: SearchBudget::default(),
    mode: SearchMode::Keyword,
    primitive_filter: None,
    time_range: None,
    tags_any: vec![],
};

let response = db.hybrid().search(&req)?;

for hit in response.hits {
    println!("#{} (score={:.3}): {:?}", hit.rank, hit.score, hit.ref_);

    // Get actual data
    let data = db.deref_hit(&hit)?;
    println!("  Data: {:?}", data);
}

if response.truncated {
    println!("(Results truncated due to budget)");
}
```

---

## 15. Performance Characteristics

### 15.1 M6 Performance Expectations

**M6 prioritizes interface stability over speed.**

| Operation | Without Index | With Index |
|-----------|---------------|------------|
| Single primitive search (1K docs) | 5-20 ms | 1-5 ms |
| Single primitive search (10K docs) | 50-200 ms | 5-20 ms |
| Composite search (6 primitives, 1K each) | 30-100 ms | 10-30 ms |

These are **acceptable for M6**. Optimization is future work.

### 15.2 Non-Regression Requirement

**Critical**: M6 must NOT degrade non-search paths.

| Operation | M5 Target | M6 Requirement |
|-----------|-----------|----------------|
| KVStore get | < 5 µs | < 5 µs |
| KVStore put | < 8 µs | < 8 µs |
| JsonStore get | 30-50 µs | 30-50 µs |
| JsonStore set | 100-200 µs | 100-200 µs |

**How achieved**:
- Lazy index initialization
- No overhead when search not used
- Index updates only for opted-in primitives
- Search code paths separate from CRUD paths

### 15.3 Index Overhead

When indexing is enabled:

| Operation | Overhead |
|-----------|----------|
| Put (index update) | +10-50 µs |
| Delete (index update) | +5-20 µs |
| Memory (per 1K docs) | ~100-500 KB |

---

## 16. Testing Strategy

### 16.1 API Contract Tests

```rust
#[test]
fn test_primitive_search_returns_docref() {
    let db = test_db();
    populate_test_data(&db);

    let req = search_request("test");
    let response = db.kv.search(&req)?;

    for hit in response.hits {
        // DocRef must be dereferenceable
        let data = db.deref_hit(&hit)?;
        assert!(data.is_some());
    }
}

#[test]
fn test_composite_search_orchestrates_primitives() {
    let db = test_db();
    populate_all_primitives(&db);

    let req = search_request("test");
    let response = db.hybrid().search(&req)?;

    // Should have results from multiple primitives
    let primitives: HashSet<_> = response.hits
        .iter()
        .map(|h| h.ref_.primitive_kind())
        .collect();

    assert!(primitives.len() > 1);
}
```

### 16.2 Determinism Tests

```rust
#[test]
fn test_search_deterministic() {
    let db = test_db();
    populate_test_data(&db);

    let req = search_request("test");

    let r1 = db.hybrid().search(&req)?;
    let r2 = db.hybrid().search(&req)?;

    assert_eq!(r1.hits.len(), r2.hits.len());
    for (h1, h2) in r1.hits.iter().zip(r2.hits.iter()) {
        assert_eq!(h1.ref_, h2.ref_);
        assert_eq!(h1.rank, h2.rank);
        assert!((h1.score - h2.score).abs() < 0.0001);
    }
}
```

### 16.3 Budget Enforcement Tests

```rust
#[test]
fn test_time_budget_enforced() {
    let db = test_db();
    populate_large_dataset(&db);  // 100K docs

    let req = SearchRequest {
        query: "test".into(),
        budget: SearchBudget {
            max_wall_time_micros: 10_000,  // 10ms
            ..Default::default()
        },
        ..Default::default()
    };

    let start = Instant::now();
    let response = db.hybrid().search(&req)?;
    let elapsed = start.elapsed();

    // Should complete within budget (with some margin)
    assert!(elapsed.as_micros() < 20_000);

    // Should be marked as truncated
    assert!(response.truncated);
}

#[test]
fn test_candidate_budget_enforced() {
    let db = test_db();
    populate_large_dataset(&db);

    let req = SearchRequest {
        query: "common".into(),  // Matches many docs
        budget: SearchBudget {
            max_candidates: 100,
            ..Default::default()
        },
        ..Default::default()
    };

    let response = db.hybrid().search(&req)?;

    assert!(response.stats.candidates_considered <= 100);
    assert!(response.truncated);
}
```

### 16.4 Snapshot Consistency Tests

```rust
#[test]
fn test_search_sees_consistent_snapshot() {
    let db = test_db();
    db.kv.put(run_id, "key1", "value1")?;

    // Start search (takes snapshot)
    let snapshot = db.snapshot();

    // Concurrent write
    db.kv.put(run_id, "key2", "value2")?;

    // Search should NOT see key2
    let req = search_request("value");
    let response = db.kv.search_with_snapshot(&req, &snapshot)?;

    let refs: Vec<_> = response.hits.iter().map(|h| &h.ref_).collect();
    assert!(refs.iter().any(|r| matches!(r, DocRef::Kv { key } if key.contains("key1"))));
    assert!(!refs.iter().any(|r| matches!(r, DocRef::Kv { key } if key.contains("key2"))));
}
```

### 16.5 Non-Regression Tests

```rust
#[test]
fn test_search_does_not_regress_get() {
    let db = test_db();

    // Baseline get performance
    let get_before = benchmark(|| db.kv.get(run_id, "key"));

    // Enable indexing and do searches
    db.enable_search_index(PrimitiveKind::Kv)?;
    for _ in 0..100 {
        db.kv.search(&search_request("test"))?;
    }

    // Get performance should be unchanged
    let get_after = benchmark(|| db.kv.get(run_id, "key"));

    assert!(get_after < get_before * 1.1);  // Within 10%
}
```

### 16.6 Fusion Tests

```rust
#[test]
fn test_rrf_fusion_properties() {
    // RRF should:
    // 1. Combine results from multiple primitives
    // 2. Rank docs appearing in multiple lists higher
    // 3. Be stable (same inputs = same outputs)

    let results = vec![
        (PrimitiveKind::Kv, make_response(vec!["A", "B", "C"])),
        (PrimitiveKind::Json, make_response(vec!["B", "D", "A"])),
    ];

    let fuser = RRFFuser::default();
    let fused = fuser.fuse(results, 10);

    // "A" and "B" appear in both lists, should rank higher
    assert!(fused.hits[0].ref_.id() == "A" || fused.hits[0].ref_.id() == "B");
    assert!(fused.hits[1].ref_.id() == "A" || fused.hits[1].ref_.id() == "B");
}
```

---

## 17. Known Limitations

### 17.1 M6 Limitations (Intentional)

| Limitation | Impact | Mitigation |
|------------|--------|------------|
| **No vector search** | Can't do semantic similarity | M9 adds vectors |
| **Simple tokenization** | No stemming, synonyms | Future analyzers |
| **BM25-lite only** | Mediocre relevance | Swappable scorer |
| **No highlighting** | Can't show match context | Future convenience |
| **Scan fallback** | Slow without index | Enable indexing |
| **Single-threaded search** | Limited parallelism | Future optimization |

### 17.2 What M6 Explicitly Does NOT Provide

- Vector embeddings / semantic search
- Learning-to-rank / ML models
- Query DSL (filters beyond primitive/time/tags)
- Analyzers (stemming, synonyms, language detection)
- Highlighting / snippets with match context
- Aggregations / facets
- Distributed search
- Real-time index updates (index updates are synchronous on commit)

These are all **intentionally deferred**, not forgotten.

---

## 18. Future Extension Points

### 18.0 RESERVED: RetrievalPlan (Conceptual Placeholder)

**This is not implemented in M6. It is a conceptual reservation.**

The current API is:
```rust
db.hybrid().search(&SearchRequest) -> SearchResponse
```

Eventually, this will become:
```rust
db.hybrid().recall(&RetrievalPlan) -> RetrievalResult
```

Where `RetrievalPlan` is a multi-step, stateful retrieval program:

```rust
/// NOT M6 - Conceptual reservation only
///
/// Retrieval will evolve from "run a query" to "execute a plan".
/// Plans can be multi-step, adaptive, and state-aware.
pub trait RetrievalPlan: Send + Sync {
    /// Get the next retrieval action based on current state
    fn next_step(&mut self, state: &RetrievalState) -> Option<RetrievalAction>;

    /// Check if plan is complete
    fn is_complete(&self) -> bool;
}

pub enum RetrievalAction {
    /// Simple keyword search
    KeywordSearch { query: String, k: usize },
    /// Vector similarity search
    VectorSearch { embedding: Vec<f32>, k: usize },
    /// Expand results with related documents
    Expand { doc_refs: Vec<DocRef>, relation: ExpansionType },
    /// Rerank current results
    Rerank { hits: Vec<SearchHit>, strategy: RerankStrategy },
    /// Multi-hop: use current results to generate new queries
    MultiHop { from: Vec<DocRef>, hop_type: HopType },
    /// Terminate and return current results
    Return,
}

pub struct RetrievalState {
    /// Results accumulated so far
    pub hits: Vec<SearchHit>,
    /// Steps executed
    pub steps_taken: usize,
    /// Budget remaining
    pub budget_remaining: SearchBudget,
    /// Traces for debugging
    pub trace: Vec<RetrievalStep>,
}
```

**Why reserve this concept now?**

1. It clarifies that M6's `SearchRequest` is a simple case of a larger pattern
2. It prevents the API from accidentally becoming a dead end
3. It frames fusion as a step toward planning, not the end state
4. It reminds implementers that `hybrid.search()` is scaffolding

**M6 does NOT implement this.** But M6's design must not prevent it.

### 18.1 M9: Vector Search

```rust
// M9 will add:
pub enum SearchMode {
    Keyword,
    Vector,
    Hybrid,  // Keyword + Vector fusion
}

impl VectorStore {
    pub fn search(&self, req: &SearchRequest) -> Result<SearchResponse>;
}

// Vector scorer
pub struct VectorScorer {
    model: EmbeddingModel,
}
```

### 18.2 Future: ML Rerankers

```rust
// Future research milestone:
pub trait Reranker: Send + Sync {
    fn rerank(&self, hits: Vec<SearchHit>, query: &str) -> Vec<SearchHit>;
}

pub struct CrossEncoderReranker {
    model: CrossEncoderModel,
}
```

### 18.3 Future: Query DSL

```rust
// M12 will add:
pub struct QueryDSL {
    pub must: Vec<QueryClause>,
    pub should: Vec<QueryClause>,
    pub must_not: Vec<QueryClause>,
    pub filter: Vec<FilterClause>,
}
```

### 18.4 Extension Hooks

M6 code is designed for extension:

```rust
// Scorer is pluggable
impl Database {
    pub fn set_scorer(&self, primitive: PrimitiveKind, scorer: Arc<dyn Scorer>);
}

// Fuser is pluggable
impl HybridSearch {
    pub fn with_fuser(self, fuser: Arc<dyn Fuser>) -> Self;
}

// Text extractor is pluggable
impl KVStore {
    pub fn set_text_extractor(&self, extractor: Arc<dyn TextExtractor>);
}
```

---

## 19. Appendix

### 19.1 Dependency Changes

**New dependencies for M6**:
- None required (uses existing Rust stdlib)

**Optional dependencies**:
- `unicode-segmentation`: Better tokenization (if needed)

### 19.2 Crate Structure

```
in-mem/
├── crates/
│   ├── core/
│   │   └── src/
│   │       ├── types.rs          # +DocRef, PrimitiveKind
│   │       └── search.rs         # SearchRequest, SearchResponse, etc. (NEW)
│   ├── search/                   # NEW CRATE
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── scorer.rs         # Scorer trait, BM25LiteScorer
│   │       ├── fuser.rs          # Fuser trait, RRFFuser
│   │       ├── index.rs          # InvertedIndex
│   │       └── hybrid.rs         # HybridSearch
│   └── primitives/
│       └── src/
│           ├── kv.rs             # +search()
│           ├── json.rs           # +search()
│           ├── event.rs          # +search()
│           ├── state.rs          # +search()
│           ├── trace.rs          # +search()
│           └── run_index.rs      # +search()
```

### 19.3 Success Criteria Checklist

**Gate 1: Primitive Search APIs**
- [ ] `kv.search(&SearchRequest)` returns `SearchResponse`
- [ ] `json.search(&SearchRequest)` returns `SearchResponse`
- [ ] `event.search(&SearchRequest)` returns `SearchResponse`
- [ ] `state.search(&SearchRequest)` returns `SearchResponse`
- [ ] `trace.search(&SearchRequest)` returns `SearchResponse`
- [ ] `run_index.search(&SearchRequest)` returns `SearchResponse`

**Gate 2: Composite Search**
- [ ] `db.hybrid().search(&SearchRequest)` orchestrates across primitives
- [ ] RRF fusion implemented with k_rrf=60
- [ ] Primitive filters honored
- [ ] Time range filters work
- [ ] Budget enforcement (time and candidates)

**Gate 3: Core Types**
- [ ] `SearchDoc` ephemeral view with DocRef back-pointer
- [ ] `DocRef` variants for all 6 primitives
- [ ] `SearchRequest` with query, k, budget, mode, filters
- [ ] `SearchResponse` with hits, truncated flag, stats

**Gate 4: Indexing (Optional)**
- [ ] Inverted index per primitive (opt-in)
- [ ] BM25-lite scoring
- [ ] Index updates on commit
- [ ] Snapshot-consistent search results

**Gate 5: Non-Regression**
- [ ] Zero overhead when search not used
- [ ] No extra allocations per transaction when search disabled
- [ ] No background indexing unless opted in
- [ ] KV/JSON/Event performance unchanged

**Gate 6: Pluggability**
- [ ] Scorer is trait-based and swappable
- [ ] Fuser is trait-based and swappable
- [ ] Text extractor is customizable per primitive

---

## Conclusion

M6 is a **surface lock-in milestone**.

It defines:
- Primitive search interfaces (`primitive.search()`)
- Composite search orchestration (`db.hybrid().search()`)
- Result reference model (`DocRef`)
- Pluggable scoring and fusion contracts
- Budget enforcement model

It does NOT attempt to achieve great relevance or performance. That is intentional.

**M6 builds plumbing. Future milestones add intelligence.**

The simple BM25-lite + RRF implementation in M6 is a "hello world" to validate the surface. Once interfaces are validated, future work can add:
- Vector search (M9)
- ML rerankers (research)
- Query DSL (M12)
- Advanced analyzers

All without changing the core interfaces defined here.

---

**Document Version**: 1.0
**Status**: Implementation Ready
**Date**: 2026-01-16
