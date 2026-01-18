# M6 Epic Prompt Header

**Copy this header to the top of every M6 epic prompt file (Epics 33-39).**

---

## AUTHORITATIVE SPECIFICATIONS - READ THESE FIRST

**The following documents are GOSPEL for ALL M6 implementation:**

1. **`docs/architecture/M6_ARCHITECTURE.md`** - THE AUTHORITATIVE SPECIFICATION
2. `docs/milestones/M6/M6_IMPLEMENTATION_PLAN.md` - Epic/Story breakdown and implementation details
3. `docs/milestones/M6/EPIC_33_SEARCH_TYPES.md` through `EPIC_39_VALIDATION.md` - Story-level specifications
4. `docs/diagrams/m6-architecture.md` - Visual architecture diagrams

**The architecture spec is LAW.** The implementation plan and epic docs provide execution details but MUST NOT contradict the architecture spec.

This is not a guideline. This is not a suggestion. This is the **LAW**.

### Rules for Every Story in Every Epic of M6:

1. **Every story MUST implement behavior EXACTLY as specified in the Epic documents**
   - No "improvements" that deviate from the spec
   - No "simplifications" that change behavior
   - No "optimizations" that break guarantees

2. **If your code contradicts the spec, YOUR CODE IS WRONG**
   - The spec defines correct behavior
   - Fix the code, not the spec

3. **If your tests contradict the spec, YOUR TESTS ARE WRONG**
   - Tests must validate spec-compliant behavior
   - Never adjust tests to make broken code pass

4. **If the spec seems wrong or unclear:**
   - STOP implementation immediately
   - Raise the issue for discussion
   - Do NOT proceed with assumptions
   - Do NOT implement your own interpretation

5. **No breaking the spec for ANY reason:**
   - Not for "performance"
   - Not for "simplicity"
   - Not for "it's just an edge case"
   - Not for "we can fix it later"

---

## THE SIX ARCHITECTURAL RULES (NON-NEGOTIABLE)

**These rules MUST be followed in EVERY M6 story. Violating any of these is a blocking issue.**

### Rule 1: No Data Movement

> **Search indexes MAY NOT copy data out of storage. Use references (DocRef) only.**

```rust
// CORRECT: Store reference to original data
pub struct PostingEntry {
    pub doc_ref: DocRef,  // Reference only
    pub tf: u32,
    pub doc_len: u32,
}

// WRONG: Copying document content into index
pub struct PostingEntry {
    pub doc_ref: DocRef,
    pub cached_content: String,  // NEVER DO THIS
}
```

### Rule 2: Primitive Search is First-Class

> **Every primitive exposes `.search(&SearchRequest) -> SearchResponse`. No exceptions.**

```rust
// CORRECT: Each primitive has native search
impl KVStore {
    pub fn search(&self, req: &SearchRequest) -> Result<SearchResponse> { ... }
}
impl JsonStore {
    pub fn search(&self, req: &SearchRequest) -> Result<SearchResponse> { ... }
}
impl EventLog {
    pub fn search(&self, req: &SearchRequest) -> Result<SearchResponse> { ... }
}

// WRONG: Centralized search that doesn't understand primitives
fn search_all(query: &str) -> Vec<String> { ... }  // NEVER DO THIS
```

### Rule 3: Composite Orchestrates, Doesn't Replace

> **`db.hybrid().search()` delegates to primitive searches then fuses. Never bypasses primitives.**

```rust
// CORRECT: Composite delegates to primitives
impl HybridSearch {
    pub fn search(&self, req: &SearchRequest) -> Result<SearchResponse> {
        let kv_results = self.db.kv.search(req)?;
        let json_results = self.db.json.search(req)?;
        // ... other primitives ...
        self.fuser.fuse(results, req.k)
    }
}

// WRONG: Composite accessing storage directly
impl HybridSearch {
    pub fn search(&self, req: &SearchRequest) -> Result<SearchResponse> {
        // Scanning storage directly - NEVER DO THIS
        for (key, value) in self.db.storage().scan_all() { ... }
    }
}
```

### Rule 4: Snapshot-Consistent Search

> **A search takes ONE snapshot; all primitive searches use that same snapshot.**

```rust
// CORRECT: Single snapshot for entire search
impl HybridSearch {
    pub fn search(&self, req: &SearchRequest) -> Result<SearchResponse> {
        let snapshot = self.db.snapshot();  // ONE snapshot
        let kv_results = self.db.kv.search_with_snapshot(req, &snapshot)?;
        let json_results = self.db.json.search_with_snapshot(req, &snapshot)?;
        // All see consistent view
    }
}

// WRONG: Each primitive takes its own snapshot
impl HybridSearch {
    pub fn search(&self, req: &SearchRequest) -> Result<SearchResponse> {
        let kv_results = self.db.kv.search(req)?;  // Own snapshot
        let json_results = self.db.json.search(req)?;  // Different snapshot!
    }
}
```

### Rule 5: Zero Overhead When Disabled

> **If indexing is off, search scans storage. No extra allocations, no background work.**

```rust
// CORRECT: Check enabled flag first
impl InvertedIndex {
    pub fn on_commit(&self, writes: &[WriteEntry]) {
        if !self.is_enabled() {
            return;  // Zero overhead
        }
        // ... indexing logic ...
    }
}

// CORRECT: Fall back to scan when index disabled
impl KVStore {
    pub fn search(&self, req: &SearchRequest) -> Result<SearchResponse> {
        if self.index.is_enabled() {
            self.search_with_index(req)
        } else {
            self.search_with_scan(req)  // Still works!
        }
    }
}
```

### Rule 6: Algorithm Swappable

> **Scorer and Fuser are traits. BM25Lite and RRF are defaults, not hardcoded.**

```rust
// CORRECT: Pluggable scorer trait
pub trait Scorer: Send + Sync {
    fn score(&self, doc: &SearchDoc, query: &str, ctx: &ScorerContext) -> f32;
    fn name(&self) -> &str;
}

// CORRECT: Default can be swapped
impl HybridSearch {
    pub fn with_fuser(mut self, fuser: Arc<dyn Fuser>) -> Self {
        self.fuser = fuser;
        self
    }
}

// WRONG: Hardcoded algorithm
fn score(doc: &SearchDoc, query: &str) -> f32 {
    // BM25 hardcoded - NEVER DO THIS
    bm25_score(doc, query)
}
```

---

## BRANCHING STRATEGY - READ THIS

### Branch Hierarchy
```
main                          <- Protected: only accepts merges from develop
  └── develop                 <- Integration branch for completed epics
       └── epic-N-name        <- Epic branch (base for all story PRs)
            └── epic-N-story-X-desc  <- Story branches
```

### Critical Rules

1. **Story PRs go to EPIC branch, NOT main**
   ```bash
   # CORRECT: PR base is epic branch
   gh pr create --base epic-33-search-types --head epic-33-story-257-search-request

   # WRONG: Never PR directly to main
   gh pr create --base main --head epic-33-story-257-search-request  # NEVER DO THIS
   ```

2. **Epic branches merge to develop** (after all stories complete)
   ```bash
   git checkout develop
   git merge --no-ff epic-33-search-types
   ```

3. **develop merges to main** (at milestone boundaries)
   ```bash
   git checkout main
   git merge --no-ff develop -m "M6: Complete"
   ```

4. **main is protected** - requires PR, no direct pushes

### The `complete-story.sh` Script
The script automatically uses the correct base branch:
```bash
./scripts/complete-story.sh 302  # Creates PR to epic-33-search-types
```

**If you manually create a PR, ALWAYS verify the base branch is the epic branch, not main.**

---

## M6 CORE CONCEPTS

### Retrieval Surfaces Goal

M6 adds **search/retrieval capabilities** to all primitives:
- KVStore, JsonStore, EventLog, StateCell, TraceStore, RunIndex all get `.search()`
- Composite `db.hybrid().search()` orchestrates across primitives
- Results fused via RRF (Reciprocal Rank Fusion)
- Optional inverted indexing for acceleration

### Key Design Decisions

| Decision | Rationale |
|----------|-----------|
| Primitive-native search | Each primitive knows its data best |
| DocRef back-pointers | No data duplication, deref on demand |
| Snapshot-consistent | All primitives see same point-in-time view |
| Pluggable scoring | BM25 now, vector/learned later |
| Optional indexing | Zero overhead when not needed |
| Budget enforcement | Predictable latency via time/candidate limits |

### Core Types

```rust
/// Request for search operation
pub struct SearchRequest {
    pub run_id: RunId,
    pub query: String,
    pub k: usize,                              // Top-k results
    pub budget: SearchBudget,
    pub primitive_filter: Option<Vec<PrimitiveKind>>,
}

/// Reference to a document in any primitive
pub enum DocRef {
    Kv { run_id: RunId, key: String },
    Json { run_id: RunId, doc_id: JsonDocId },
    Event { run_id: RunId, seq: u64 },
    State { run_id: RunId, cell_id: String },
    Trace { run_id: RunId, span_id: SpanId },
    Run { run_id: RunId },
}

/// A single search hit
pub struct SearchHit {
    pub doc_ref: DocRef,
    pub score: f32,
    pub rank: u32,
    pub snippet: Option<String>,
}
```

### Primitive Search Pattern

```rust
impl KVStore {
    /// Search KV pairs matching query
    pub fn search(&self, req: &SearchRequest) -> Result<SearchResponse> {
        let snapshot = self.db.storage().create_snapshot();
        self.search_with_snapshot(req, &snapshot)
    }

    fn search_with_snapshot(&self, req: &SearchRequest, snapshot: &Snapshot) -> Result<SearchResponse> {
        // 1. Enumerate candidates (scan or index)
        // 2. Score with Scorer
        // 3. Rank and return top-k
    }
}
```

### Composite Search Pattern

```rust
impl HybridSearch {
    pub fn search(&self, req: &SearchRequest) -> Result<SearchResponse> {
        let snapshot = self.db.snapshot();  // Single snapshot

        let results: Vec<(PrimitiveKind, SearchResponse)> = self.primitives
            .iter()
            .filter(|p| req.includes_primitive(*p))
            .map(|p| (p, self.search_primitive(p, req, &snapshot)))
            .collect();

        self.fuser.fuse(results, req.k)
    }
}
```

---

## TDD METHODOLOGY

**CRITICAL TESTING RULE** (applies to EVERY story):

- **NEVER adjust tests to make them pass**
- If a test fails, the CODE must be fixed, not the test
- Tests define correct behavior - failed tests reveal bugs in implementation
- Only adjust a test if the test itself is incorrect (wrong assertion logic)
- Tests MUST validate spec-compliant behavior

---

## Tool Paths

**ALWAYS use fully qualified paths:**
- Cargo: `~/.cargo/bin/cargo`
- GitHub CLI: `gh` (should be in PATH)

---

## Story Workflow

1. **Start story**: `./scripts/start-story.sh <epic> <story> <description>`
2. **Read specs**:
   ```bash
   cat docs/milestones/M6/M6_IMPLEMENTATION_PLAN.md
   cat docs/milestones/M6/EPIC_<N>_<NAME>.md
   ```
3. **Write tests first** (TDD)
4. **Implement code** to pass tests
5. **Run validation**:
   ```bash
   ~/.cargo/bin/cargo test --workspace
   ~/.cargo/bin/cargo clippy --workspace -- -D warnings
   ~/.cargo/bin/cargo fmt --check
   ```
6. **Complete story**: `./scripts/complete-story.sh <story>`

---

## GitHub Issue References

M6 uses the following GitHub issue numbers:

| Epic | GitHub Issue | Stories |
|------|--------------|---------|
| Epic 33: Core Search Types | [#295](https://github.com/anibjoshi/in-mem/issues/295) | #302-#307 |
| Epic 34: Primitive Search Surface | [#296](https://github.com/anibjoshi/in-mem/issues/296) | #308-#315 |
| Epic 35: Scoring Infrastructure | [#297](https://github.com/anibjoshi/in-mem/issues/297) | #316-#319 |
| Epic 36: Composite Search (Hybrid) | [#298](https://github.com/anibjoshi/in-mem/issues/298) | #320-#324 |
| Epic 37: Fusion Infrastructure | [#299](https://github.com/anibjoshi/in-mem/issues/299) | #325-#328 |
| Epic 38: Optional Indexing | [#300](https://github.com/anibjoshi/in-mem/issues/300) | #329-#333 |
| Epic 39: Validation & Non-Regression | [#301](https://github.com/anibjoshi/in-mem/issues/301) | #334-#336 |

---

## EPIC END VALIDATION

**At the end of every epic, run the full validation process.**

See: `docs/prompts/EPIC_END_VALIDATION.md`

### Quick Validation Commands

```bash
# Phase 1: Automated checks (must all pass)
~/.cargo/bin/cargo build --workspace && \
~/.cargo/bin/cargo test --workspace && \
~/.cargo/bin/cargo clippy --workspace -- -D warnings && \
~/.cargo/bin/cargo fmt --check && \
echo "Phase 1: PASS"
```

### M6-Specific Validation

```bash
# Run M6 benchmarks
~/.cargo/bin/cargo bench --bench m6_search_performance

# Run non-regression tests (verify M5 targets maintained)
~/.cargo/bin/cargo bench --bench m5_json_performance
~/.cargo/bin/cargo bench --bench m4_performance

# Verify search operations meet targets
~/.cargo/bin/cargo test --test m6_search_integration
```

### Validation Phases

| Phase | Focus | Time |
|-------|-------|------|
| 1 | Automated checks (build, test, clippy, fmt) | 5 min |
| 2 | Story completion verification | 10 min |
| 3 | Spec compliance review (6 rules) | 15 min |
| 4 | Non-regression verification (M4/M5 targets) | 10 min |
| 5 | Code review checklist | 20 min |
| 6 | Epic-specific validation | 15 min |
| 7 | Final sign-off | 5 min |

**Total**: ~80 minutes per epic

### After Validation Passes

```bash
# Merge epic to develop
git checkout develop
git merge --no-ff epic-<N>-<name> -m "Epic <N>: <Name> complete"
git push origin develop

# Close epic issue
gh issue close <epic-issue> --comment "Epic complete. All validation passed."
```

---

## Performance Targets

### Non-Regression (M4/M5 Targets Must Be Maintained)

| Metric | Target | M6 Requirement |
|--------|--------|----------------|
| KV put (InMemory) | < 3µs | No regression |
| KV get (fast path) | < 5µs | No regression |
| JSON create (1KB) | < 1ms | No regression |
| JSON get at path | < 100µs | No regression |
| Event append | < 10µs | No regression |
| State read | < 5µs | No regression |

### Search Targets

| Operation | Scale | Target | Red Flag |
|-----------|-------|--------|----------|
| KV search (1K docs, no index) | 1,000 | < 50ms | > 200ms |
| KV search (1K docs, indexed) | 1,000 | < 10ms | > 50ms |
| Hybrid search (1K total) | 1,000 | < 100ms | > 500ms |
| Index update (per commit) | 1 doc | < 100µs | > 1ms |

---

## Evolution Warnings

M6 includes several "evolution warnings" for future extensibility:

1. **SearchRequest**: `extensions: HashMap<String, Value>` reserved for future signals
2. **ScorerContext**: `extensions: HashMap<String, Value>` reserved for future scorer requirements
3. **JSON Flattening**: Current approach is naive; vector search will need smarter strategies
4. **Fusion**: RRF is default; learned fusion reserved for future
5. **Indexing**: Synchronous updates; async background indexing reserved for future

---

## Parallelization Strategy

### Phase 1: Foundation (Days 1-2)
- **Claude 1**: Epic 33 (Core Search Types) - BLOCKS ALL

### Phase 2: Primitive Search (Days 3-5)
After Epic 33 complete:
- **Claude 1**: Epic 34 (Primitive Search) - Stories #263-#267
- **Claude 2**: Epic 35 (Scoring Infrastructure)

### Phase 3: Composite & Fusion (Days 6-8)
After Epic 34 complete:
- **Claude 1**: Epic 36 (Composite Search)
- **Claude 2**: Epic 37 (Fusion Infrastructure)

### Phase 4: Indexing (Days 9-10)
After Epic 36 complete:
- **Claude 1**: Epic 38 (Optional Indexing)

### Phase 5: Validation (Days 11-12)
After all implementation:
- **All**: Epic 39 validation stories

---

*End of M6 Prompt Header - Epic-specific content follows below*
