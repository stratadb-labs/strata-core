# M6 Architecture Diagrams: Retrieval Surfaces

This document contains visual representations of the M6 architecture focused on the retrieval surface that enables fast experimentation with search and ranking across all primitives.

**Architecture Spec Version**: 1.1

---

## Semantic Invariants (Reference)

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                         M6 SEMANTIC INVARIANTS                               │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│  1. NO DATA MOVEMENT                                                        │
│     Composite search runs against each primitive's native storage.          │
│     NO unified search store. NO copying data.                               │
│                                                                             │
│  2. PRIMITIVE SEARCH IS FIRST-CLASS                                         │
│     Each primitive has its own .search() method.                            │
│     Users can search a single primitive directly.                           │
│                                                                             │
│  3. COMPOSITE ORCHESTRATES, NOT REPLACES                                    │
│     db.hybrid().search() calls primitive searches and fuses results.        │
│     It does NOT own indexing, conflict semantics, or storage.               │
│                                                                             │
│  4. SEARCH IS SNAPSHOT-CONSISTENT                                           │
│     All search operations use a SnapshotView.                               │
│     Results are stable for that search invocation.                          │
│                                                                             │
│  5. ZERO OVERHEAD WHEN NOT USED                                             │
│     If no search APIs are invoked: no allocations, no write amplification.  │
│                                                                             │
│  6. ALGORITHMS ARE SWAPPABLE                                                │
│     Scorer and Fuser are traits. BM25-lite + RRF are defaults.              │
│     Future can swap without engine rewrites.                                │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

---

## 1. System Architecture Overview (M6)

```
+-------------------------------------------------------------------------+
|                           Application Layer                              |
|                      (Agent Applications using DB)                       |
+-----------------------------------+-------------------------------------+
                                    |
                                    | High-level typed APIs
                                    v
+-------------------------------------------------------------------------+
|                          Primitives Layer (M3-M5)                        |
|                          (Stateless Facades)                             |
|                                                                          |
|  +-------------+  +-------------+  +--------------+  +-------------+    |
|  |  KV Store   |  |  Event Log  |  |  StateCell   |  |Trace Store  |    |
|  |             |  |             |  |              |  |             |    |
|  | - get()     |  | - append()  |  | - read()     |  | - record()  |    |
|  | - put()     |  | - read()    |  | - init()     |  | - get()     |    |
|  | - delete()  |  | - iter()    |  | - cas()      |  | - query_*() |    |
|  | - list()    |  | - verify()  |  | - set()      |  | - get_tree()|    |
|  | - search()  |  | - search()  |  | - search()   |  | - search()  |    |
|  |   (M6 NEW)  |  |   (M6 NEW)  |  |   (M6 NEW)   |  |   (M6 NEW)  |    |
|  +------+------+  +------+------+  +------+-------+  +------+------+    |
|         |                |                |                |            |
|         +----------------+-------+--------+----------------+            |
|                                  |                                      |
|                                  |                                      |
|  +---------------------------+   |   +-----------------------------+   |
|  |        Run Index          |   |   |      JSON Store (M5)        |   |
|  |                           |   |   |                             |   |
|  | - create_run()            |   |   | - create()     - cas()      |   |
|  | - get_run()               |   |   | - get()        - version()  |   |
|  | - update_status()         |   |   | - set()        - patch()    |   |
|  | - query_runs()            |   |   | - delete()     - exists()   |   |
|  | - search() (M6 NEW)       |   |   | - search() (M6 NEW)         |   |
|  +-------------+-------------+   |   +-------------+---------------+   |
|                |                 |                 |                    |
+----------------+-----------------+-----------------+--------------------+
                                   |
                                   | Database transaction API
                                   v
+-------------------------------------------------------------------------+
|                         Engine Layer (M1-M4)                             |
|                   (Orchestration & Coordination)                         |
|                                                                          |
|  +-------------------------------------------------------------------+  |
|  |                          Database                                  |  |
|  |                                                                    |  |
|  |  M6 NEW: Retrieval Surface                                        |  |
|  |  +-------------------------------------------------------------+  |  |
|  |  |                      HybridSearch                            |  |  |
|  |  |  - select_primitives()                                       |  |  |
|  |  |  - allocate_budgets()                                        |  |  |
|  |  |  - search_primitive()                                        |  |  |
|  |  |  - fuse_results()                                            |  |  |
|  |  +-------------------------------------------------------------+  |  |
|  |                                                                    |  |
|  |  M6 NEW: Optional Indexing (per primitive)                        |  |
|  |  - InvertedIndex (lazy init)                                      |  |
|  |  - enable_search_index(primitive)                                 |  |
|  |  - disable_search_index(primitive)                                |  |
|  |                                                                    |  |
|  +-------------------------------------------------------------------+  |
|                               |                                          |
+----------+-------------------+-------------------+-----------------------+
           |                   |                   |
           v                   v                   v
+------------------+  +-------------------+  +------------------------+
|  Storage (M4)    |  | Durability (M4)   |  | Concurrency (M4)       |
|                  |  |                   |  |                        |
| - ShardedStore   |  | - InMemoryMode    |  | - Transaction Pooling  |
| - DashMap        |  | - BufferedMode    |  | - Read Fast Path       |
| - All TypeTags   |  | - StrictMode      |  | - OCC Validation       |
+------------------+  +-------------------+  +------------------------+
           |                   |                   |
           +-------------------+-------------------+
                               |
                               v
+-------------------------------------------------------------------------+
|                         Core Types Layer (M1 + M6)                       |
|                       (Foundation Definitions)                           |
|                                                                          |
|  M6 NEW Types:                                                           |
|  - SearchRequest   (query, k, budget, mode, filters)                    |
|  - SearchResponse  (hits, truncated, stats)                             |
|  - SearchHit       (doc_ref, score, rank)                               |
|  - DocRef          (back-pointer to source record)                      |
|  - PrimitiveKind   (Kv, Json, Event, State, Trace, Run)                 |
|  - SearchBudget    (time and candidate limits)                          |
+-------------------------------------------------------------------------+
```

---

## 2. Retrieval Surface Model

```
+-------------------------------------------------------------------------+
|                     Retrieval Surface Model (M6)                         |
+-------------------------------------------------------------------------+

M6 Conceptual Framing:
======================

    ┌─────────────────────────────────────────────────────────────────────┐
    │                                                                     │
    │  M6 is called "search" but is actually building RECALL              │
    │  INFRASTRUCTURE.                                                    │
    │                                                                     │
    │  Search is just one recall mode. Humans retrieve memory by:         │
    │  - Association    - Causality     - Recency                         │
    │  - Salience       - Similarity    - Goals                           │
    │  - Tasks          - Failure loops                                   │
    │                                                                     │
    │  M6 builds the surface that enables these modes.                    │
    │  The keyword search in M6 is a "hello world" to validate plumbing.  │
    │                                                                     │
    │  Future trajectory:                                                 │
    │    hybrid.search(query) → hybrid.recall(plan)                       │
    │                                                                     │
    └─────────────────────────────────────────────────────────────────────┘


Two Search Surfaces:
====================

    ┌─────────────────────────────────────────────────────────────────────┐
    │                                                                     │
    │  PRIMITIVE SEARCH                    COMPOSITE SEARCH               │
    │  ────────────────                    ────────────────               │
    │                                                                     │
    │  db.kv.search(&req)                  db.hybrid().search(&req)       │
    │  db.json.search(&req)                                               │
    │  db.event.search(&req)               Orchestrates primitive         │
    │  db.state.search(&req)               searches and fuses results     │
    │  db.trace.search(&req)                                              │
    │  db.run_index.search(&req)                                          │
    │                                                                     │
    │  Direct access to single            Cross-primitive retrieval       │
    │  primitive's data                   with result fusion              │
    │                                                                     │
    └─────────────────────────────────────────────────────────────────────┘


Surface Architecture:
=====================

    SearchRequest
         │
         ▼
    ┌─────────────────────────────────────────────────────────────────┐
    │                        HybridSearch                              │
    │                                                                  │
    │  ┌──────────────┐  ┌──────────────┐  ┌───────────────┐         │
    │  │select_prims()│──│alloc_budget()│──│take snapshot  │         │
    │  └──────────────┘  └──────────────┘  └───────┬───────┘         │
    │                                               │                  │
    │  ┌────────────────────────────────────────────┴────────────────┐│
    │  │                    Same Snapshot                             ││
    │  │  ┌───────┐ ┌───────┐ ┌───────┐ ┌───────┐ ┌───────┐ ┌─────┐ ││
    │  │  │  kv   │ │ json  │ │ event │ │ state │ │ trace │ │ run │ ││
    │  │  │search │ │search │ │search │ │search │ │search │ │search││
    │  │  └───┬───┘ └───┬───┘ └───┬───┘ └───┬───┘ └───┬───┘ └──┬──┘ ││
    │  └──────┼─────────┼─────────┼─────────┼─────────┼────────┼────┘│
    │         └─────────┴─────────┴─────────┴─────────┴────────┘     │
    │                              │                                  │
    │                       ┌──────┴──────┐                          │
    │                       │   Fuser     │                          │
    │                       │   (RRF)     │                          │
    │                       └──────┬──────┘                          │
    └──────────────────────────────┼─────────────────────────────────┘
                                   │
                                   ▼
                            SearchResponse
```

---

## 3. Core Types

```
+-------------------------------------------------------------------------+
|                          Core Types (M6)                                 |
+-------------------------------------------------------------------------+

SearchRequest:
==============

    ┌─────────────────────────────────────────────────────────────────────┐
    │ SearchRequest {                                                      │
    │   run_id: RunId,              // Scope search to this run           │
    │   query: String,              // Query string                       │
    │   k: usize,                   // Max results to return              │
    │   budget: SearchBudget,       // Time and candidate limits          │
    │   mode: SearchMode,           // Keyword (M6), Vector (M9), Hybrid  │
    │   primitive_filter: Option<Vec<PrimitiveKind>>,  // Limit scope     │
    │   time_range: Option<(u64, u64)>,                // Time filter     │
    │   tags_any: Vec<String>,      // Tag filter                         │
    │ }                                                                    │
    └─────────────────────────────────────────────────────────────────────┘

    WARNING: SearchRequest must not become a query DSL.
    Future direction: QueryExpr variants (Keyword, Vector, Hybrid, Programmatic)


SearchBudget:
=============

    ┌─────────────────────────────────────────────────────────────────────┐
    │ SearchBudget {                                                       │
    │   max_wall_time_micros: u64,          // Hard stop (default 100ms)  │
    │   max_candidates: usize,               // Total limit (default 10K) │
    │   max_candidates_per_primitive: usize, // Per-primitive (default 2K)│
    │ }                                                                    │
    └─────────────────────────────────────────────────────────────────────┘


SearchResponse:
===============

    ┌─────────────────────────────────────────────────────────────────────┐
    │ SearchResponse {                                                     │
    │   hits: Vec<SearchHit>,       // Ranked results                     │
    │   truncated: bool,            // True if budget caused early stop   │
    │   stats: SearchStats,         // Execution statistics               │
    │ }                                                                    │
    └─────────────────────────────────────────────────────────────────────┘


SearchHit:
==========

    ┌─────────────────────────────────────────────────────────────────────┐
    │ SearchHit {                                                          │
    │   doc_ref: DocRef,            // Back-pointer to source record      │
    │   score: f32,                 // Relevance score (higher = better)  │
    │   rank: u32,                  // Position in result set (1-indexed) │
    │   snippet: Option<String>,    // Optional preview                   │
    │   debug: Option<HitDebug>,    // Scoring breakdown (debug mode)     │
    │ }                                                                    │
    └─────────────────────────────────────────────────────────────────────┘


DocRef (Back-Pointer):
======================

    ┌─────────────────────────────────────────────────────────────────────┐
    │ enum DocRef {                                                        │
    │   Kv { key: Key },                                                  │
    │   Json { key: Key, doc_id: JsonDocId },                             │
    │   Event { log_key: Key, seq: u64 },                                 │
    │   State { key: Key },                                               │
    │   Trace { key: Key, span_id: u64 },                                 │
    │   Run { run_id: RunId },                                            │
    │ }                                                                    │
    └─────────────────────────────────────────────────────────────────────┘

    DocRef enables dereferencing:

    let hit = response.hits[0];
    let data = db.deref_hit(&hit)?;  // Retrieves actual record


PrimitiveKind:
==============

    ┌─────────────────────────────────────────────────────────────────────┐
    │ enum PrimitiveKind {                                                 │
    │   Kv,      // Key-value store                                       │
    │   Json,    // JSON documents                                        │
    │   Event,   // Event log entries                                     │
    │   State,   // State cells                                           │
    │   Trace,   // Trace spans                                           │
    │   Run,     // Run metadata                                          │
    │ }                                                                    │
    └─────────────────────────────────────────────────────────────────────┘
```

---

## 4. Primitive Search Flow

```
+-------------------------------------------------------------------------+
|                      Primitive Search Flow (M6)                          |
+-------------------------------------------------------------------------+

Single Primitive Search:
========================

    Application              Primitive            Snapshot         Scorer
        │                        │                    │                │
        │ kv.search(&req)        │                    │                │
        ├───────────────────────►│                    │                │
        │                        │                    │                │
        │                        │ take snapshot      │                │
        │                        ├───────────────────►│                │
        │                        │                    │                │
        │                        │ scan candidates    │                │
        │                        │◄───────────────────┤                │
        │                        │                    │                │
        │                        │ for each candidate:│                │
        │                        │   check budget     │                │
        │                        │   extract text     │                │
        │                        │   score(doc,query) │                │
        │                        ├────────────────────────────────────►│
        │                        │                    │                │
        │                        │◄────────────────────────────────────┤
        │                        │   score: f32      │                │
        │                        │                    │                │
        │                        │ sort by score      │                │
        │                        │ take top-k         │                │
        │                        │ assign ranks       │                │
        │                        │                    │                │
        │◄───────────────────────┤                    │                │
        │  SearchResponse        │                    │                │


Text Extraction Per Primitive:
==============================

    +-------------------+------------------------------------------------+
    |    Primitive      |  Text Extraction Strategy                      |
    +-------------------+------------------------------------------------+
    | KV                | String values directly; JSON stringify maps    |
    | JSON              | Flatten all scalars + "key: value" pairs       |
    | Event             | Event type + payload stringified               |
    | State             | State name + current value stringified         |
    | Trace             | Span name + attributes stringified             |
    | Run               | Run ID + status + metadata stringified         |
    +-------------------+------------------------------------------------+

    WARNING: JSON flattening is TEMPORARY.

    ┌─────────────────────────────────────────────────────────────────────┐
    │                                                                     │
    │  Flattening JSON into a bag of words is a LOSSY baseline.           │
    │                                                                     │
    │  This approach loses:                                               │
    │  - Path structure ($.user.name vs $.admin.name)                     │
    │  - Field semantics (title vs description)                           │
    │  - Type information (string "123" vs number 123)                    │
    │                                                                     │
    │  Future retrieval will need:                                        │
    │  - Path-aware matching                                              │
    │  - Field-weighted matching                                          │
    │  - Schema-aware matching                                            │
    │                                                                     │
    └─────────────────────────────────────────────────────────────────────┘
```

---

## 5. Composite Search Flow

```
+-------------------------------------------------------------------------+
|                     Composite Search Flow (M6)                           |
+-------------------------------------------------------------------------+

HybridSearch Orchestration:
===========================

    Application          HybridSearch         Primitives           Fuser
        │                     │                    │                  │
        │ hybrid().search(&req)                    │                  │
        ├────────────────────►│                    │                  │
        │                     │                    │                  │
        │                     │ 1. Select primitives (based on filter)│
        │                     │    [Kv, Json, Event, State, Trace, Run]
        │                     │                    │                  │
        │                     │ 2. Allocate budgets│                  │
        │                     │    time / N        │                  │
        │                     │    candidates / primitive             │
        │                     │                    │                  │
        │                     │ 3. Take SINGLE snapshot               │
        │                     │                    │                  │
        │                     │ 4. Execute searches (same snapshot)   │
        │                     ├───────────────────►│                  │
        │                     │   kv.search()      │                  │
        │                     │   json.search()    │                  │
        │                     │   event.search()   │                  │
        │                     │   state.search()   │                  │
        │                     │   trace.search()   │                  │
        │                     │   run.search()     │                  │
        │                     │◄───────────────────┤                  │
        │                     │   6 SearchResponses│                  │
        │                     │                    │                  │
        │                     │ 5. Fuse results    │                  │
        │                     ├───────────────────────────────────────►│
        │                     │   Vec<(PrimitiveKind, SearchResponse)>│
        │                     │                    │                  │
        │                     │◄───────────────────────────────────────┤
        │                     │   Fused SearchResponse                │
        │                     │                    │                  │
        │◄────────────────────┤                    │                  │
        │  SearchResponse     │                    │                  │


Budget Allocation:
==================

    Total Budget: 100ms, 10K candidates
    6 Primitives Selected

    ┌─────────────────────────────────────────────────────────────────────┐
    │                                                                     │
    │  Per-Primitive Budget:                                              │
    │                                                                     │
    │  ┌───────┐ ┌───────┐ ┌───────┐ ┌───────┐ ┌───────┐ ┌───────┐      │
    │  │  KV   │ │ JSON  │ │ EVENT │ │ STATE │ │ TRACE │ │  RUN  │      │
    │  │ 16ms  │ │ 16ms  │ │ 16ms  │ │ 16ms  │ │ 16ms  │ │ 16ms  │      │
    │  │ 2K    │ │ 2K    │ │ 2K    │ │ 2K    │ │ 2K    │ │ 2K    │      │
    │  └───────┘ └───────┘ └───────┘ └───────┘ └───────┘ └───────┘      │
    │                                                                     │
    │  Time: 100ms / 6 = ~16ms per primitive                             │
    │  Candidates: max_candidates_per_primitive (2K default)             │
    │                                                                     │
    └─────────────────────────────────────────────────────────────────────┘


CRITICAL: Single Snapshot
=========================

    ┌─────────────────────────────────────────────────────────────────────┐
    │                                                                     │
    │  All primitive searches use the SAME snapshot.                      │
    │                                                                     │
    │  This ensures:                                                      │
    │  - Consistent view across primitives                                │
    │  - No torn reads                                                    │
    │  - Deterministic results                                            │
    │                                                                     │
    │  Concurrent writes during search are NOT visible.                   │
    │                                                                     │
    └─────────────────────────────────────────────────────────────────────┘
```

---

## 6. Scoring Model

```
+-------------------------------------------------------------------------+
|                         Scoring Model (M6)                               |
+-------------------------------------------------------------------------+

Scorer Trait:
=============

    ┌─────────────────────────────────────────────────────────────────────┐
    │ trait Scorer: Send + Sync {                                          │
    │   fn score(&self, doc: &SearchDoc, query: &str, ctx: &ScorerContext) │
    │       -> f32;                                                        │
    │   fn name(&self) -> &str;                                            │
    │ }                                                                    │
    └─────────────────────────────────────────────────────────────────────┘


ScorerContext:
==============

    ┌─────────────────────────────────────────────────────────────────────┐
    │ struct ScorerContext {                                               │
    │   total_docs: usize,                  // For IDF calculation        │
    │   doc_freqs: HashMap<String, usize>,  // Term document frequencies  │
    │   avg_doc_len: f32,                   // For length normalization   │
    │   now_micros: u64,                    // For recency calculations   │
    │   extensions: HashMap<String, Value>, // Future signals (RESERVED)  │
    │ }                                                                    │
    └─────────────────────────────────────────────────────────────────────┘

    WARNING: ScorerContext is BM25-shaped for M6.

    ┌─────────────────────────────────────────────────────────────────────┐
    │                                                                     │
    │  Future scorers will need:                                          │
    │  - Recency curves, temporal priors                                  │
    │  - Access/write frequency signals                                   │
    │  - User salience, causal depth                                      │
    │  - Trace centrality, graph locality                                 │
    │                                                                     │
    │  Use `extensions` field for forward compatibility.                  │
    │                                                                     │
    └─────────────────────────────────────────────────────────────────────┘


BM25-Lite Scorer (M6 Default):
==============================

    BM25 Formula:

    score = Σ IDF(term) × (tf × (k1 + 1)) / (tf + k1 × (1 - b + b × dl/avgdl))

    Where:
    - IDF(term) = ln((N - df + 0.5) / (df + 0.5) + 1)
    - tf = term frequency in document
    - k1 = 1.2 (term frequency saturation)
    - b = 0.75 (length normalization)
    - dl = document length
    - avgdl = average document length


    Optional Boosts:
    ----------------

    +-------------------+--------------------------------------------------+
    |     Boost         |  Effect                                          |
    +-------------------+--------------------------------------------------+
    | Recency           | score *= 1 + 0.1 × (1 / (1 + age_hours/24))     |
    | Title Match       | score *= 1.2 if query term in title             |
    +-------------------+--------------------------------------------------+


Tokenizer (Basic):
==================

    ┌─────────────────────────────────────────────────────────────────────┐
    │                                                                     │
    │  fn tokenize(text: &str) -> Vec<String>                             │
    │                                                                     │
    │  1. Lowercase                                                       │
    │  2. Split on non-alphanumeric                                       │
    │  3. Filter tokens < 2 characters                                    │
    │                                                                     │
    │  "Hello, World!" → ["hello", "world"]                              │
    │  "I am a test"   → ["am", "test"]  (filters "I", "a")              │
    │                                                                     │
    │  Future: stemming, stopwords, synonyms (deferred)                   │
    │                                                                     │
    └─────────────────────────────────────────────────────────────────────┘
```

---

## 7. Fusion Model

```
+-------------------------------------------------------------------------+
|                          Fusion Model (M6)                               |
+-------------------------------------------------------------------------+

Fuser Trait:
============

    ┌─────────────────────────────────────────────────────────────────────┐
    │ trait Fuser: Send + Sync {                                           │
    │   fn fuse(                                                           │
    │       &self,                                                         │
    │       results: Vec<(PrimitiveKind, SearchResponse)>,                │
    │       k: usize,                                                      │
    │   ) -> SearchResponse;                                               │
    │   fn name(&self) -> &str;                                            │
    │ }                                                                    │
    └─────────────────────────────────────────────────────────────────────┘


RRF Fuser (M6 Default):
=======================

    Reciprocal Rank Fusion (RRF)

    RRF Score = Σ 1 / (k_rrf + rank)  across all lists containing the doc

    Where k_rrf = 60 (default smoothing constant)


    Example:
    --------

    List A: [doc1@rank1, doc2@rank2, doc3@rank3]
    List B: [doc2@rank1, doc4@rank2, doc1@rank3]
    k_rrf = 60

    RRF Scores:
    - doc1: 1/(60+1) + 1/(60+3) = 0.0164 + 0.0159 = 0.0323
    - doc2: 1/(60+2) + 1/(60+1) = 0.0161 + 0.0164 = 0.0325  ← highest
    - doc3: 1/(60+3) = 0.0159
    - doc4: 1/(60+2) = 0.0161

    Final ranking: [doc2, doc1, doc4, doc3]


    ┌─────────────────────────────────────────────────────────────────────┐
    │                                                                     │
    │  Key RRF Properties:                                                │
    │                                                                     │
    │  - Documents in multiple lists get higher scores                    │
    │  - High rank in any list contributes significantly                  │
    │  - k_rrf=60 balances single-list dominance vs. multi-list presence │
    │                                                                     │
    └─────────────────────────────────────────────────────────────────────┘


Deduplication:
==============

    ┌─────────────────────────────────────────────────────────────────────┐
    │                                                                     │
    │  Same DocRef from multiple primitives → merged into one hit         │
    │                                                                     │
    │  - RRF scores are SUMMED                                            │
    │  - First occurrence's metadata (snippet, etc.) is kept              │
    │  - Output contains unique DocRefs only                              │
    │                                                                     │
    └─────────────────────────────────────────────────────────────────────┘


Deterministic Tie-Breaking:
===========================

    When RRF scores are equal:

    1. Primary:   RRF score (descending)
    2. Secondary: Original score from first occurrence
    3. Tertiary:  Stable DocRef hash

    Same inputs → identical output order (required for testing)


Evolution Warning:
==================

    ┌─────────────────────────────────────────────────────────────────────┐
    │                                                                     │
    │  RRF is NOT the endgame.                                            │
    │                                                                     │
    │  Current pipeline:                                                  │
    │    Select → Search → Fuse → Return                                  │
    │                                                                     │
    │  Future pipeline:                                                   │
    │    Plan → Retrieve → Expand → Retrieve → Rerank →                  │
    │    Reason → Retrieve → Fuse → Return                               │
    │                                                                     │
    │  Fusion will become multi-step retrieval PLANNING.                  │
    │                                                                     │
    └─────────────────────────────────────────────────────────────────────┘
```

---

## 8. Optional Indexing

```
+-------------------------------------------------------------------------+
|                       Optional Indexing (M6)                             |
+-------------------------------------------------------------------------+

Index Is OPT-IN:
================

    ┌─────────────────────────────────────────────────────────────────────┐
    │                                                                     │
    │  By default: NO indexing. Search uses scan.                         │
    │                                                                     │
    │  Enable per primitive:                                              │
    │    db.enable_search_index(PrimitiveKind::Kv)?;                      │
    │                                                                     │
    │  Disable:                                                           │
    │    db.disable_search_index(PrimitiveKind::Kv)?;                     │
    │                                                                     │
    │  Check status:                                                      │
    │    db.is_search_index_enabled(PrimitiveKind::Kv)                    │
    │                                                                     │
    └─────────────────────────────────────────────────────────────────────┘


InvertedIndex Structure:
========================

    ┌─────────────────────────────────────────────────────────────────────┐
    │ struct InvertedIndex {                                               │
    │   postings: DashMap<String, PostingList>,  // Token → documents     │
    │   doc_freqs: DashMap<String, usize>,       // For IDF calculation   │
    │   total_docs: AtomicUsize,                 // Corpus size           │
    │   enabled: AtomicBool,                     // On/off flag           │
    │   version: AtomicU64,                      // Watermark             │
    │ }                                                                    │
    │                                                                      │
    │ struct PostingList {                                                 │
    │   entries: Vec<PostingEntry>,                                        │
    │ }                                                                    │
    │                                                                      │
    │ struct PostingEntry {                                                │
    │   doc_ref: DocRef,      // Back-pointer                             │
    │   tf: u32,              // Term frequency                           │
    │   doc_len: u32,         // Document length                          │
    │   ts_micros: Option<u64>, // Timestamp                              │
    │ }                                                                    │
    └─────────────────────────────────────────────────────────────────────┘


Index Updates (Synchronous):
============================

    ┌─────────────────────────────────────────────────────────────────────┐
    │                                                                     │
    │  Index updates happen SYNCHRONOUSLY on commit.                      │
    │                                                                     │
    │  fn on_commit(&self, writes: &[WriteEntry]) {                       │
    │      if !self.is_enabled() {                                        │
    │          return;  // NOOP - zero overhead                           │
    │      }                                                               │
    │                                                                     │
    │      for write in writes {                                          │
    │          match write.operation {                                    │
    │              Put { .. } => self.index_document(..),                 │
    │              Delete { .. } => self.remove_document(..),             │
    │          }                                                           │
    │      }                                                               │
    │                                                                     │
    │      self.version.fetch_add(1, Ordering::Release);  // Bump        │
    │  }                                                                   │
    │                                                                     │
    └─────────────────────────────────────────────────────────────────────┘


Search Decision Flow:
=====================

    ┌─────────────────────────────────────────────────────────────────────┐
    │                                                                     │
    │                    Is index enabled?                                │
    │                          │                                          │
    │              ┌───────────┴───────────┐                             │
    │              │                       │                              │
    │             YES                      NO                             │
    │              │                       │                              │
    │              ▼                       │                              │
    │     Is index up-to-date?            │                              │
    │     (version >= snapshot)            │                              │
    │              │                       │                              │
    │      ┌───────┴───────┐              │                              │
    │      │               │              │                              │
    │     YES              NO             │                              │
    │      │               │              │                              │
    │      ▼               ▼              ▼                              │
    │  ┌─────────┐   ┌─────────┐   ┌─────────┐                          │
    │  │  INDEX  │   │  SCAN   │   │  SCAN   │                          │
    │  │ SEARCH  │   │ SEARCH  │   │ SEARCH  │                          │
    │  │  (fast) │   │(fallback)│  │(default)│                          │
    │  └─────────┘   └─────────┘   └─────────┘                          │
    │                                                                     │
    └─────────────────────────────────────────────────────────────────────┘


Zero Overhead When Disabled:
============================

    ┌─────────────────────────────────────────────────────────────────────┐
    │                                                                     │
    │  CRITICAL: If indexing is disabled:                                 │
    │                                                                     │
    │  - No allocations for index structures                              │
    │  - on_commit() returns immediately (O(1) check)                     │
    │  - No write amplification                                           │
    │  - No background work                                               │
    │                                                                     │
    │  Non-search paths pay ZERO cost for search capability existing.     │
    │                                                                     │
    └─────────────────────────────────────────────────────────────────────┘


Evolution Warning:
==================

    ┌─────────────────────────────────────────────────────────────────────┐
    │                                                                     │
    │  The inverted index is an INTERNAL OPTIMIZATION.                    │
    │  It is NOT a conceptual pillar.                                     │
    │                                                                     │
    │  Future retrieval may invent:                                       │
    │  - Time-segmented indexes                                           │
    │  - Recency-weighted indexes                                         │
    │  - Role-based indexes                                               │
    │  - Access-path indexes                                              │
    │  - Structure-aware indexes                                          │
    │  - Semantic caches                                                  │
    │                                                                     │
    │  Indexing must remain SWAPPABLE.                                    │
    │                                                                     │
    └─────────────────────────────────────────────────────────────────────┘
```

---

## 9. Budget Enforcement

```
+-------------------------------------------------------------------------+
|                      Budget Enforcement (M6)                             |
+-------------------------------------------------------------------------+

Budget Checks:
==============

    fn check_budget(start: Instant, candidates: usize, budget: &SearchBudget)
        -> BudgetStatus
    {
        if start.elapsed().as_micros() >= budget.max_wall_time_micros {
            return BudgetStatus::TimeExhausted;
        }
        if candidates >= budget.max_candidates {
            return BudgetStatus::CandidatesExhausted;
        }
        BudgetStatus::Ok
    }


Search Loop with Budget:
========================

    fn search(&self, req: &SearchRequest) -> Result<SearchResponse> {
        let start = Instant::now();
        let mut candidates = Vec::new();
        let mut truncated = false;

        for (key, value) in self.scan()? {

            // Check budget BEFORE processing each candidate
            match check_budget(start, candidates.len(), &req.budget) {
                BudgetStatus::Ok => {}
                _ => {
                    truncated = true;
                    break;  // Stop early, don't error
                }
            }

            let doc = self.extract_doc(&key, &value)?;
            candidates.push(doc);
        }

        let hits = self.score_and_rank(candidates, req.k)?;

        Ok(SearchResponse {
            hits,
            truncated,  // Clearly indicate truncation
            stats: SearchStats {
                elapsed_micros: start.elapsed().as_micros() as u64,
                candidates_considered: candidates.len(),
                ..
            },
        })
    }


Graceful Degradation:
=====================

    ┌─────────────────────────────────────────────────────────────────────┐
    │                                                                     │
    │  Budget enforcement NEVER throws errors.                            │
    │                                                                     │
    │  Instead:                                                           │
    │  - Search stops early                                               │
    │  - Results are returned (partial)                                   │
    │  - truncated = true in response                                     │
    │  - stats show what was considered                                   │
    │                                                                     │
    │  Application decides how to handle truncation.                      │
    │                                                                     │
    └─────────────────────────────────────────────────────────────────────┘


Application Pattern:
====================

    let result = db.hybrid().search(&req)?;

    if result.truncated {
        warn!("Search truncated due to budget - results may be incomplete");
        // Options:
        // 1. Show results with warning
        // 2. Retry with larger budget
        // 3. Narrow search scope
    }

    for hit in result.hits {
        // Results are still usable
    }
```

---

## 10. Snapshot Consistency

```
+-------------------------------------------------------------------------+
|                     Snapshot Consistency (M6)                            |
+-------------------------------------------------------------------------+

Single Snapshot for Composite Search:
=====================================

    ┌─────────────────────────────────────────────────────────────────────┐
    │                                                                     │
    │  Composite search takes ONE snapshot at the start.                  │
    │  ALL primitive searches use this SAME snapshot.                     │
    │                                                                     │
    │  This ensures:                                                      │
    │  - Consistent view across all primitives                            │
    │  - Writes during search are invisible                               │
    │  - Results are reproducible                                         │
    │                                                                     │
    └─────────────────────────────────────────────────────────────────────┘


Timeline Example:
=================

    T1: Search starts, takes snapshot (version = 100)
    T2: KV search begins (using snapshot v100)
    T3: Concurrent write: kv.put("new_key", "value")  → version = 101
    T4: JSON search begins (using snapshot v100)
    T5: Search completes

    Result: "new_key" is NOT visible in search results
            (snapshot v100 doesn't include v101 write)


Index Version Watermark:
========================

    ┌─────────────────────────────────────────────────────────────────────┐
    │                                                                     │
    │  Index has a version watermark that tracks freshness.               │
    │                                                                     │
    │  fn is_consistent_with(&self, snapshot: &Snapshot) -> bool {        │
    │      self.version.load(Ordering::Acquire) >= snapshot.version()     │
    │  }                                                                   │
    │                                                                     │
    │  If index is stale (version < snapshot version):                    │
    │  → Fall back to scan (correct but slower)                          │
    │                                                                     │
    └─────────────────────────────────────────────────────────────────────┘


Determinism Guarantee:
======================

    ┌─────────────────────────────────────────────────────────────────────┐
    │                                                                     │
    │  Same snapshot + same request = IDENTICAL ordered results           │
    │                                                                     │
    │  Required for:                                                      │
    │  - Testing                                                          │
    │  - Debugging                                                        │
    │  - Caching                                                          │
    │  - Reproducibility                                                  │
    │                                                                     │
    │  Achieved by:                                                       │
    │  - Deterministic scoring                                            │
    │  - Deterministic tie-breaking                                       │
    │  - Stable iteration order                                           │
    │                                                                     │
    └─────────────────────────────────────────────────────────────────────┘
```

---

## 11. Performance Characteristics

```
+-------------------------------------------------------------------------+
|                   Performance Characteristics (M6)                       |
+-------------------------------------------------------------------------+

M6 Performance Philosophy:
==========================

    ┌─────────────────────────────────────────────────────────────────────┐
    │                                                                     │
    │  M6 prioritizes INTERFACE STABILITY over SPEED.                     │
    │                                                                     │
    │  Search may be slow. That is acceptable.                            │
    │  The interfaces matter more than ranking quality or speed.          │
    │  We can optimize algorithms later without changing interfaces.      │
    │                                                                     │
    └─────────────────────────────────────────────────────────────────────┘


Expected Latencies:
===================

    +-------------------------------------+---------------+---------------+
    |            Operation                | Without Index | With Index    |
    +-------------------------------------+---------------+---------------+
    | Single primitive search (1K docs)   |    5-20 ms    |    1-5 ms     |
    | Single primitive search (10K docs)  |   50-200 ms   |    5-20 ms    |
    | Composite search (6 prims, 1K each) |   30-100 ms   |   10-30 ms    |
    +-------------------------------------+---------------+---------------+


Non-Regression Requirement:
===========================

    ┌─────────────────────────────────────────────────────────────────────┐
    │                                                                     │
    │  CRITICAL: M6 must NOT degrade non-search paths.                    │
    │                                                                     │
    │  +-------------------+------------------+------------------+        │
    │  |    Operation      |   M5 Target      |  M6 Requirement  |        │
    │  +-------------------+------------------+------------------+        │
    │  | KVStore get       |      < 5 µs      |      < 5 µs      |        │
    │  | KVStore put       |      < 8 µs      |      < 8 µs      |        │
    │  | JsonStore get     |    30-50 µs      |    30-50 µs      |        │
    │  | JsonStore set     |   100-200 µs     |   100-200 µs     |        │
    │  | EventLog append   |     < 10 µs      |     < 10 µs      |        │
    │  +-------------------+------------------+------------------+        │
    │                                                                     │
    └─────────────────────────────────────────────────────────────────────┘


How Non-Regression Is Achieved:
===============================

    1. LAZY INDEX INITIALIZATION
       - Index structures not allocated until search is used
       - Non-search code paths unchanged

    2. EARLY BAILOUT IN HOOKS
       - on_commit() checks enabled flag first (O(1))
       - If disabled, returns immediately

    3. SEPARATE CODE PATHS
       - Search code is additive
       - CRUD operations unchanged

    4. NO SHARED HOT PATHS
       - Search uses its own data structures
       - No contention with normal operations


Index Overhead (When Enabled):
==============================

    +----------------------------+------------------+
    |        Operation           |    Overhead      |
    +----------------------------+------------------+
    | Put (with index update)    |    +10-50 µs     |
    | Delete (with index update) |    +5-20 µs      |
    | Memory (per 1K docs)       |   ~100-500 KB    |
    +----------------------------+------------------+
```

---

## 12. API Summary

```
+-------------------------------------------------------------------------+
|                          API Summary (M6)                                |
+-------------------------------------------------------------------------+

Primitive Search APIs:
======================

    // Direct primitive search
    db.kv.search(&req)?;
    db.json.search(&req)?;
    db.event.search(&req)?;
    db.state.search(&req)?;
    db.trace.search(&req)?;
    db.run_index.search(&req)?;


Composite Search API:
=====================

    // Cross-primitive search with fusion
    db.hybrid().search(&req)?;


Index Configuration API:
========================

    // Enable/disable indexing per primitive
    db.enable_search_index(PrimitiveKind::Kv)?;
    db.disable_search_index(PrimitiveKind::Kv)?;
    db.is_search_index_enabled(PrimitiveKind::Kv);

    // Rebuild index from existing data
    db.rebuild_search_index(PrimitiveKind::Kv)?;


DocRef Dereferencing:
=====================

    // Retrieve actual data from search hit
    let data = db.deref_hit(&hit)?;


Usage Example:
==============

    // Build search request
    let req = SearchRequest {
        run_id,
        query: "authentication error".to_string(),
        k: 20,
        budget: SearchBudget::default(),
        mode: SearchMode::Keyword,
        primitive_filter: None,  // Search all primitives
        time_range: None,
        tags_any: vec![],
    };

    // Execute search
    let response = db.hybrid().search(&req)?;

    // Process results
    for hit in response.hits {
        println!("#{} (score={:.3}): {:?}", hit.rank, hit.score, hit.doc_ref);

        // Get actual data
        let data = db.deref_hit(&hit)?;
        println!("  Data: {:?}", data);
    }

    if response.truncated {
        println!("(Results truncated due to budget)");
    }
```

---

## 13. Future Extension Points

```
+-------------------------------------------------------------------------+
|                    Future Extension Points (M6)                          |
+-------------------------------------------------------------------------+

RESERVED: RetrievalPlan (NOT M6):
=================================

    ┌─────────────────────────────────────────────────────────────────────┐
    │                                                                     │
    │  Current API:                                                       │
    │    db.hybrid().search(&SearchRequest) -> SearchResponse             │
    │                                                                     │
    │  Future API:                                                        │
    │    db.hybrid().recall(&RetrievalPlan) -> RetrievalResult           │
    │                                                                     │
    │  RetrievalPlan is a multi-step, stateful retrieval program.         │
    │                                                                     │
    └─────────────────────────────────────────────────────────────────────┘

    trait RetrievalPlan: Send + Sync {
        fn next_step(&mut self, state: &RetrievalState) -> Option<RetrievalAction>;
        fn is_complete(&self) -> bool;
    }

    enum RetrievalAction {
        KeywordSearch { query: String, k: usize },
        VectorSearch { embedding: Vec<f32>, k: usize },
        Expand { doc_refs: Vec<DocRef>, relation: ExpansionType },
        Rerank { hits: Vec<SearchHit>, strategy: RerankStrategy },
        MultiHop { from: Vec<DocRef>, hop_type: HopType },
        Return,
    }


M9: Vector Search:
==================

    enum SearchMode {
        Keyword,    // M6
        Vector,     // M9
        Hybrid,     // M9 (Keyword + Vector fusion)
    }

    struct VectorStore {
        fn search(&self, req: &SearchRequest) -> Result<SearchResponse>;
    }


Future: ML Rerankers:
=====================

    trait Reranker: Send + Sync {
        fn rerank(&self, hits: Vec<SearchHit>, query: &str) -> Vec<SearchHit>;
    }

    struct CrossEncoderReranker {
        model: CrossEncoderModel,
    }


M12: Query DSL:
===============

    struct QueryDSL {
        must: Vec<QueryClause>,
        should: Vec<QueryClause>,
        must_not: Vec<QueryClause>,
        filter: Vec<FilterClause>,
    }


Extension Hooks:
================

    // Scorer is pluggable
    db.set_scorer(PrimitiveKind::Kv, Arc::new(MyScorer))?;

    // Fuser is pluggable
    db.hybrid().with_fuser(Arc::new(MyFuser)).search(&req)?;

    // Text extractor is pluggable
    db.kv.set_text_extractor(Arc::new(MyExtractor))?;
```

---

## 14. M6 Philosophy

```
+-------------------------------------------------------------------------+
|                           M6 Philosophy                                  |
+-------------------------------------------------------------------------+

    ┌─────────────────────────────────────────────────────────────────────┐
    │                                                                     │
    │     M6 builds PLUMBING, not INTELLIGENCE.                           │
    │                                                                     │
    │     The retrieval surface must allow algorithm swaps at both        │
    │     primitive and composite levels without engine rewrites.         │
    │                                                                     │
    │     We don't know the right retrieval approach yet.                 │
    │     M6 ensures we can experiment.                                   │
    │                                                                     │
    └─────────────────────────────────────────────────────────────────────┘


What M6 Locks In:
=================

    ┌─────────────────────────────────────────────────────────────────────┐
    │                                                                     │
    │  ✓ Primitive search interface: primitive.search(&SearchRequest)     │
    │  ✓ Composite search interface: db.hybrid().search(&SearchRequest)  │
    │  ✓ Result reference model: DocRef back-pointers                    │
    │  ✓ Budget enforcement model: time and candidates                   │
    │  ✓ Pluggable contracts: Scorer and Fuser traits                    │
    │                                                                     │
    └─────────────────────────────────────────────────────────────────────┘


What M6 Explicitly Defers:
==========================

    ┌─────────────────────────────────────────────────────────────────────┐
    │                                                                     │
    │  → M9:  Vector search / embeddings                                  │
    │  → M12: Full query DSL                                              │
    │  → Future: ML rerankers, learning-to-rank                          │
    │  → Future: Complex analyzers (stemming, synonyms)                  │
    │  → Future: Highlighting, aggregations, faceting                    │
    │  → Future: Distributed search                                       │
    │                                                                     │
    └─────────────────────────────────────────────────────────────────────┘


The "Hello World" Algorithm:
============================

    ┌─────────────────────────────────────────────────────────────────────┐
    │                                                                     │
    │  BM25-lite + RRF is a "hello world" to validate the surface.        │
    │                                                                     │
    │  It may produce mediocre results. That is acceptable.               │
    │  The interfaces matter more than the ranking quality.               │
    │                                                                     │
    │  Once interfaces are validated, future work can add:                │
    │  - Better scorers                                                   │
    │  - Better fusers                                                    │
    │  - Vector search                                                    │
    │  - ML rerankers                                                     │
    │                                                                     │
    │  All without changing the core interfaces defined here.             │
    │                                                                     │
    └─────────────────────────────────────────────────────────────────────┘


M6 Success Criteria:
====================

    ┌─────────────────────────────────────────────────────────────────────┐
    │                                                                     │
    │  ✓ All 6 primitives have .search() method                          │
    │  ✓ db.hybrid().search() orchestrates across primitives             │
    │  ✓ DocRef can be dereferenced to get actual data                   │
    │  ✓ Budget enforcement works (time and candidates)                  │
    │  ✓ Scorer and Fuser are swappable                                  │
    │  ✓ Zero overhead when search not used                              │
    │  ✓ No regression in M5 primitive performance                       │
    │                                                                     │
    └─────────────────────────────────────────────────────────────────────┘
```

---

These diagrams illustrate the key architectural components and flows for M6's Retrieval Surfaces milestone. M6 builds upon M5's JSON primitive while adding a retrieval layer that enables experimentation with search algorithms.

**Key Design Points Reflected in These Diagrams**:
- Retrieval surface is "plumbing for experimentation" not final search solution
- Two search surfaces: primitive (direct) and composite (orchestrated)
- No data movement - search runs against native primitive storage
- Single snapshot for composite search ensures consistency
- Pluggable Scorer and Fuser traits enable algorithm swaps
- Optional indexing per primitive with zero overhead when disabled
- Budget enforcement for graceful degradation
- DocRef back-pointers enable result dereferencing
- BM25-lite + RRF are "hello world" algorithms to validate interfaces

**M6 Philosophy**: M6 builds plumbing, not intelligence. The retrieval surface must allow algorithm swaps without engine rewrites. We don't know the right retrieval approach yet. M6 ensures we can experiment.
