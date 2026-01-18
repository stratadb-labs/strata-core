# M8 Epic Prompt Header

**Copy this header to the top of every M8 epic prompt file (Epics 50-55).**

---

## AUTHORITATIVE SPECIFICATIONS - READ THESE FIRST

**The following documents are GOSPEL for ALL M8 implementation:**

1. **`docs/architecture/M8_ARCHITECTURE.md`** - THE AUTHORITATIVE SPECIFICATION
2. `docs/milestones/M8/M8_IMPLEMENTATION_PLAN.md` - Epic/Story breakdown and implementation details
3. `docs/milestones/M8/EPIC_50_CORE_TYPES.md` through `EPIC_55_TRANSACTION_DURABILITY.md` - Story-level specifications
4. `docs/milestones/M8/M8_SCOPE.md` - Scope boundaries and constraints

**The architecture spec is LAW.** The implementation plan and epic docs provide execution details but MUST NOT contradict the architecture spec.

This is not a guideline. This is not a suggestion. This is the **LAW**.

### IMPORTANT: Naming Convention

**Do NOT use "M8" or "m8" in the codebase or comments.** M8 is an internal milestone indicator only. In code, use "Vector" prefix instead:
- Module names: `vector`, `vector_heap`, `vector_store`, `vector_wal`
- Type names: `VectorConfig`, `VectorStore`, `VectorHeap`, `VectorId`
- WAL constants: `WAL_VECTOR_UPSERT`, not `WAL_M8_UPSERT`
- Test names: `test_vector_*`, not `test_m8_*`
- Comments: "Vector primitive" not "M8 primitive"

### Rules for Every Story in Every Epic of M8:

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

## THE SEVEN ARCHITECTURAL RULES (NON-NEGOTIABLE)

**These rules MUST be followed in EVERY M8 story. Violating any of these is a blocking issue.**

### Rule 1: Stateless Facade Pattern

> VectorStore is a stateless facade. ALL persistent state lives in Database.

```rust
// CORRECT: Stateless facade
pub struct VectorStore {
    db: Arc<Database>,
    backends: RwLock<HashMap<CollectionId, Box<dyn VectorIndexBackend>>>,  // Cache only
}

// The backends map is a cache - it can be reconstructed from Database state
// Multiple VectorStore instances pointing to the same Database are safe

// WRONG: Facade with owned state
pub struct VectorStore {
    collections: HashMap<String, Collection>,  // NEVER DO THIS - state owned by facade
}
```

### Rule 2: Collections Per RunId

> Collections are scoped to RunId. Different runs cannot see each other's collections.

```rust
// CORRECT: RunId scoping
let embeddings = vector_store.get(run_id_a, "embeddings", "doc1")?;
// run_id_b cannot access run_id_a's "embeddings" collection

// WRONG: Global collections
let embeddings = vector_store.get("embeddings", "doc1")?;  // Missing run_id
```

### Rule 3: Upsert Semantics

> Insert with existing key overwrites. This is intentional.

```rust
// CORRECT: Upsert behavior
vector_store.insert(run_id, "docs", "key1", embedding_v1, None)?;
vector_store.insert(run_id, "docs", "key1", embedding_v2, None)?;  // Overwrites v1
let result = vector_store.get(run_id, "docs", "key1")?;
assert_eq!(result.embedding, embedding_v2);  // v2 is returned

// WRONG: Error on duplicate
if vector_store.exists(run_id, "docs", "key1")? {
    return Err(VectorError::KeyAlreadyExists);  // NEVER DO THIS
}
```

### Rule 4: Single Backend Instance Per Collection

> Each collection has exactly one backend. No sharding. No replicas.

```rust
// CORRECT: One backend per collection
fn get_or_create_backend(&self, collection_id: &CollectionId) -> &dyn VectorIndexBackend {
    self.backends.entry(collection_id.clone())
        .or_insert_with(|| self.backend_factory.create(config))
}

// WRONG: Multiple backends
fn search(&self, collection: &str, query: &[f32]) -> Vec<VectorMatch> {
    let results: Vec<_> = self.shards  // NEVER DO THIS
        .par_iter()
        .flat_map(|shard| shard.search(query))
        .collect();
}
```

### Rule 5: BTreeMap for Determinism

> Use BTreeMap (NOT HashMap) for all ID-to-data mappings.

```rust
// CORRECT: BTreeMap for deterministic iteration
pub struct VectorHeap {
    id_to_offset: BTreeMap<VectorId, usize>,  // Sorted order guaranteed
}

// Iteration is deterministic
for (id, offset) in self.id_to_offset.iter() {
    // Always same order
}

// WRONG: HashMap iteration
pub struct VectorHeap {
    id_to_offset: HashMap<VectorId, usize>,  // NEVER DO THIS - nondeterministic
}
```

### Rule 6: VectorId Is Never Reused

> Once a VectorId is assigned, it is never recycled. Storage slots may be reused, but IDs never are.

```rust
// CORRECT: Monotonic IDs, reusable slots
impl VectorHeap {
    fn insert(&mut self, embedding: &[f32]) -> VectorId {
        let id = VectorId(self.next_id.fetch_add(1, Ordering::SeqCst));  // Monotonic

        let offset = if let Some(slot) = self.free_slots.pop() {
            slot  // Reuse storage slot
        } else {
            self.allocate_new_slot()  // Allocate new slot
        };

        self.id_to_offset.insert(id, offset);
        id
    }

    fn delete(&mut self, id: VectorId) {
        if let Some(offset) = self.id_to_offset.remove(&id) {
            self.free_slots.push(offset);  // Recycle storage slot, NOT the ID
        }
    }
}

// WRONG: Reusing VectorIds
fn delete(&mut self, id: VectorId) {
    self.free_ids.push(id);  // NEVER DO THIS - IDs must never be reused
}
```

### Rule 7: No Backend-Specific Fields in VectorConfig

> VectorConfig contains only primitive-level configuration. Backend-specific tuning belongs in backend-specific config types.

```rust
// CORRECT: Clean VectorConfig
pub struct VectorConfig {
    pub dimension: usize,
    pub metric: DistanceMetric,
    pub storage_dtype: StorageDtype,
}

// Backend-specific config is separate (M9)
pub struct HnswConfig {
    pub m: usize,
    pub ef_construction: usize,
    pub ef_search: usize,
}

// WRONG: Polluted VectorConfig
pub struct VectorConfig {
    pub dimension: usize,
    pub metric: DistanceMetric,
    pub ef_construction: usize,  // NEVER DO THIS - HNSW-specific
    pub m: usize,                // NEVER DO THIS - HNSW-specific
}
```

---

## CORE INVARIANTS

### Storage Invariants (S1-S9)

| # | Invariant | Test Strategy |
|---|-----------|---------------|
| S1 | Dimension immutable | Attempt dimension change, verify error |
| S2 | Metric immutable | Attempt metric change, verify error |
| S3 | VectorId stable | Track IDs across operations, verify no change |
| S4 | VectorId never reused | Insert -> delete -> insert, verify new ID |
| S5 | Heap + KV consistency | Concurrent operations, verify sync |
| S6 | Run isolation | Cross-run access, verify isolation |
| S7 | BTreeMap sole source | No secondary data structures for active vectors |
| S8 | Snapshot-WAL equivalence | Snapshot + WAL replay = pure WAL replay |
| S9 | Heap-KV reconstructibility | Both can be rebuilt from snapshot + WAL |

### Search Invariants (R1-R10)

| # | Invariant | Test Strategy |
|---|-----------|---------------|
| R1 | Dimension match | Query with wrong dimension, verify error |
| R2 | Score normalization | All metrics return "higher is better" |
| R3 | Deterministic order | Same query = same results, property test |
| R4 | Backend tie-break | Score ties use VectorId asc |
| R5 | Facade tie-break | Score ties use key asc |
| R6 | Snapshot consistency | Concurrent writes during search |
| R7 | Coarse-grained budget | Budget at phase boundaries |
| R8 | Single-threaded | No parallel similarity computation |
| R9 | No implicit normalization | Embeddings stored exactly as provided |
| R10 | Search is read-only | No writes, no counters, no caches during search |

### Transaction Invariants (T1-T4)

| # | Invariant | Test Strategy |
|---|-----------|---------------|
| T1 | Atomic visibility | Cross-primitive transaction tests |
| T2 | Conflict detection | Concurrent writes to same key |
| T3 | Rollback safety | Failed transaction cleanup |
| T4 | VectorId monotonicity across crashes | Crash -> recover -> insert: new ID > all previous |

---

## WAL ENTRY TYPES

```rust
// Vector WAL entries: 0x70-0x7F range
pub const WAL_VECTOR_COLLECTION_CREATE: u8 = 0x70;
pub const WAL_VECTOR_COLLECTION_DELETE: u8 = 0x71;
pub const WAL_VECTOR_UPSERT: u8 = 0x72;
pub const WAL_VECTOR_DELETE: u8 = 0x73;
```

**Naming Rationale**:
- `COLLECTION_CREATE`/`DELETE`: Prefixed to distinguish from vector-level operations
- `UPSERT` (not `INSERT`): Matches our semantic where insert overwrites if exists

---

## SNAPSHOT FORMAT

```
Vector Collection Snapshot:
+----------------------------------------+
| Version byte: 0x01                     |  (1 byte)
+----------------------------------------+
| Collection name (length-prefixed)      |
+----------------------------------------+
| Config (dimension, metric, dtype)      |
+----------------------------------------+
| next_id (8 bytes)                      |  CRITICAL for T4
+----------------------------------------+
| free_slots count (4 bytes)             |
+----------------------------------------+
| free_slots data                        |  CRITICAL for slot reuse
+----------------------------------------+
| vector count (4 bytes)                 |
+----------------------------------------+
| vectors: [key, vector_id, embedding, metadata]... |
+----------------------------------------+
```

**CRITICAL**: `next_id` and `free_slots` MUST be persisted. Without these, recovery breaks VectorId monotonicity (T4).

---

## BRANCHING STRATEGY

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
   gh pr create --base epic-50-core-types --head epic-50-story-394-vector-config

   # WRONG: Never PR directly to main
   gh pr create --base main --head epic-50-story-394-vector-config  # NEVER DO THIS
   ```

2. **Epic branches merge to develop** (after all stories complete)
   ```bash
   git checkout develop
   git merge --no-ff epic-50-core-types
   ```

3. **develop merges to main** (at milestone boundaries)
   ```bash
   git checkout main
   git merge --no-ff develop -m "M8: Vector Primitive complete"
   ```

4. **main is protected** - requires PR, no direct pushes

### The `complete-story.sh` Script
The script automatically uses the correct base branch:
```bash
./scripts/complete-story.sh 394  # Creates PR to epic-50-core-types
```

**If you manually create a PR, ALWAYS verify the base branch is the epic branch, not main.**

---

## M8 CORE CONCEPTS

### What M8 Is About

M8 is a **vector storage foundation milestone**. It defines:

| Aspect | M8 Commits To |
|--------|---------------|
| **Core types** | VectorConfig, VectorEntry, VectorMatch, DistanceMetric |
| **Storage model** | Hybrid: VectorHeap (embeddings) + KV (metadata) |
| **Backend trait** | VectorIndexBackend for swappable implementations |
| **Search** | BruteForce O(n), deterministic ordering |
| **Integration** | M6 SearchRequest/SearchResponse, RRF fusion |
| **Durability** | WAL + Snapshot via M7 infrastructure |

### What M8 Is NOT

M8 is **not** an optimization milestone. Deferred items:

| Deferred Item | Target |
|---------------|--------|
| HNSW indexing | M9 |
| Quantization (F16, Int8) | M9 |
| Parallel search | M9 |
| Vector compression | M9 |
| Advanced filters (ranges, nested) | M9 |

### Key Design Decisions

| Decision | Rationale |
|----------|-----------|
| BruteForce in M8 | Correct baseline, sufficient for <50K vectors |
| BTreeMap not HashMap | Deterministic iteration for reproducible results |
| VectorId never reused | Simplifies debugging, prevents subtle bugs |
| next_id in snapshot | Critical for T4 invariant |
| Stateless facade | Thread-safe, multiple instances safe |

### Core Types

```rust
/// Collection configuration
pub struct VectorConfig {
    pub dimension: usize,
    pub metric: DistanceMetric,
    pub storage_dtype: StorageDtype,
}

/// Distance metric (all normalized to "higher = more similar")
pub enum DistanceMetric {
    Cosine,      // [-1, 1]
    Euclidean,   // (0, 1] via 1/(1+d)
    DotProduct,  // unbounded
}

/// Search result
pub struct VectorMatch {
    pub key: String,
    pub score: f32,  // Higher = more similar
    pub metadata: Option<JsonValue>,
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
   cat docs/milestones/M8/M8_IMPLEMENTATION_PLAN.md
   cat docs/milestones/M8/EPIC_<N>_<NAME>.md
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

M8 uses the following GitHub issue numbers:

| Epic | GitHub Issue | Stories (GitHub Issues) |
|------|--------------|-------------------------|
| Epic 50: Core Types & Configuration | [#388](https://github.com/anibjoshi/in-mem/issues/388) | #394-#398 |
| Epic 51: Vector Heap & Storage | [#389](https://github.com/anibjoshi/in-mem/issues/389) | #399-#404 |
| Epic 52: Index Backend Abstraction | [#390](https://github.com/anibjoshi/in-mem/issues/390) | #405-#409 |
| Epic 53: Collection Management | [#391](https://github.com/anibjoshi/in-mem/issues/391) | #410-#414 |
| Epic 54: Search Integration | [#392](https://github.com/anibjoshi/in-mem/issues/392) | #415-#420 |
| Epic 55: Transaction & Durability | [#393](https://github.com/anibjoshi/in-mem/issues/393) | #421-#425 |

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

### M8-Specific Validation

```bash
# Run M8 vector tests
~/.cargo/bin/cargo test --test vector_storage_tests

# Run M8 search tests
~/.cargo/bin/cargo test --test vector_search_tests

# Run M8 durability tests
~/.cargo/bin/cargo test --test vector_durability_tests

# Verify non-regression (M7 targets maintained)
~/.cargo/bin/cargo test --test m7_recovery_tests
~/.cargo/bin/cargo bench --bench m7_recovery_performance
```

### Validation Phases

| Phase | Focus | Time |
|-------|-------|------|
| 1 | Automated checks (build, test, clippy, fmt) | 5 min |
| 2 | Story completion verification | 10 min |
| 3 | Spec compliance review (7 rules) | 15 min |
| 4 | Non-regression verification (M7 targets) | 10 min |
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

### Non-Regression (M7 Targets Must Be Maintained)

| Metric | Target | M8 Requirement |
|--------|--------|----------------|
| Snapshot write (100MB) | < 5s | No regression |
| Snapshot load (100MB) | < 3s | No regression |
| WAL replay (10K entries) | < 1s | No regression |
| Full recovery | < 5s | No regression |

### M8 Performance Targets (Baselines, Not Optimized)

| Operation | Scale | Target | Red Flag |
|-----------|-------|--------|----------|
| Insert (384-1536 dims) | 1 | < 10ms | > 50ms |
| Search 1K vectors | 1K | < 5ms | > 25ms |
| Search 10K vectors | 10K | < 50ms | > 200ms |
| Search 50K vectors | 50K | < 200ms | > 1s |
| Hybrid search | - | within M6 budget | - |

---

## Evolution Warnings

M8 includes several "evolution warnings" for future extensibility:

1. **BruteForce is temporary**: M9 adds HNSW for O(log n) search
2. **F32 only in M8**: M9 adds F16, Int8 quantization
3. **Equality filters only**: M9 adds range filters, nested paths
4. **Single-threaded search**: M9 may add parallelism (carefully)
5. **VectorIndexBackend trait**: Designed for both BruteForce and HNSW

---

## Parallelization Strategy

### Phase 1: Foundation (Days 1-2)
- **Claude 1**: Epic 50 (Core Types) - FOUNDATION

### Phase 2: Storage (Days 3-5)
After Epic 50 complete:
- **Claude 1**: Epic 51 (Vector Heap)
- **Claude 2**: Epic 52 (Index Backend)

### Phase 3: Collection & Search (Days 6-8)
After Epic 51 complete:
- **Claude 1**: Epic 53 (Collection Management)
After Epic 52 complete:
- **Claude 2**: Epic 54 (Search Integration)

### Phase 4: Durability (Days 9-11)
After Epics 51, 52 complete:
- **Claude 1**: Epic 55 (Transaction & Durability)

### Phase 5: Validation (Days 12-14)
After all implementation:
- **All**: Full validation, integration tests

---

*End of M8 Prompt Header - Epic-specific content follows below*
