# M5 Epic Prompt Header

**Copy this header to the top of every M5 epic prompt file (Epics 26-32).**

---

## AUTHORITATIVE SPECIFICATIONS - READ THESE FIRST

**The following documents are GOSPEL for ALL M5 implementation:**

1. **`docs/architecture/M5_ARCHITECTURE.md`** - THE AUTHORITATIVE SPECIFICATION
2. `docs/milestones/M5/M5_IMPLEMENTATION_PLAN.md` - Epic/Story breakdown and implementation details
3. `docs/milestones/M5/EPIC_26_CORE_TYPES.md` through `EPIC_32_VALIDATION.md` - Story-level specifications

**The architecture spec is LAW.** The implementation plan and epic docs provide execution details but MUST NOT contradict the architecture spec.

This is not a guideline. This is not a suggestion. This is the **LAW**.

### Rules for Every Story in Every Epic of M5:

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

**These rules MUST be followed in EVERY M5 story. Violating any of these is a blocking issue.**

### Rule 1: JSON Lives Inside ShardedStore

> **Documents stored via `Key::new_json()` in existing ShardedStore. NO separate DashMap.**

```rust
// CORRECT: Use existing storage with JSON type tag
let key = Key::new_json(Namespace::for_run(run_id), &doc_id);
self.db.storage().put(key, serialized_doc)?;

// WRONG: Separate DashMap for JSON
struct JsonStore {
    documents: DashMap<JsonDocId, JsonDoc>,  // NEVER DO THIS
}
```

### Rule 2: JsonStore is Stateless Facade

> **JsonStore holds ONLY `Arc<Database>`. No internal state, no maps, no locks.**

```rust
// CORRECT: Stateless facade
#[derive(Clone)]
pub struct JsonStore {
    db: Arc<Database>,  // ONLY state
}

// WRONG: Holding additional state
pub struct JsonStore {
    db: Arc<Database>,
    cache: DashMap<Key, JsonDoc>,  // NEVER DO THIS
}
```

### Rule 3: JSON Extends TransactionContext

> **Add `JsonStoreExt` trait to TransactionContext. NO separate JsonTransaction type.**

```rust
// CORRECT: Extension trait on existing TransactionContext
pub trait JsonStoreExt {
    fn json_get(&self, key: &Key, path: &JsonPath) -> Result<Option<JsonValue>>;
    fn json_set(&mut self, key: &Key, path: &JsonPath, value: JsonValue) -> Result<()>;
}

impl JsonStoreExt for TransactionContext {
    // Implementation...
}

// WRONG: Separate transaction type
pub struct JsonTransaction {
    inner: TransactionContext,
}
```

### Rule 4: Path-Level Semantics in Validation, Not Storage

> **Storage sees whole documents. Path logic lives in JsonStoreExt methods.**

```rust
// CORRECT: Storage stores whole documents
storage.put(key, serialize_whole_doc(&doc))?;

// Path operations happen at the API layer
fn json_set(key, path, value) {
    let mut doc = self.load_doc(key)?;
    set_at_path(&mut doc.value, path, value)?;  // Path logic here
    self.store_doc(key, doc)?;
}

// WRONG: Storage understanding paths
storage.put_at_path(key, path, value)?;  // NEVER DO THIS
```

### Rule 5: WAL Remains Unified

> **Add JSON entry types (0x20-0x23) to existing WALEntry enum. NO separate JSON WAL.**

```rust
// CORRECT: Extend existing WALEntry enum
pub enum WALEntry {
    // Existing entries...
    Put { key: Key, value: Value },
    Delete { key: Key },

    // NEW: JSON entries (0x20-0x23)
    JsonCreate { key: Key, doc: JsonDoc },           // 0x20
    JsonSet { key: Key, path: JsonPath, value: JsonValue, version: u64 }, // 0x21
    JsonDelete { key: Key, path: JsonPath, version: u64 },  // 0x22
    JsonDestroy { key: Key },                        // 0x23
}

// WRONG: Separate WAL
struct JsonWAL {
    entries: Vec<JsonWALEntry>,  // NEVER DO THIS
}
```

### Rule 6: JSON API Feels Like Other Primitives

> **Same patterns as KVStore, EventLog, etc.**

```rust
// CORRECT: Follows existing primitive patterns
let json = JsonStore::new(db.clone());
json.create(&run_id, &doc_id, initial_value)?;
json.set(&run_id, &doc_id, &path, new_value)?;
let value = json.get(&run_id, &doc_id, &path)?;

// Also works in transactions
db.transaction(run_id, |txn| {
    txn.json_set(&key, &path, value)?;
    txn.kv_put("related", related_value)?;  // Cross-primitive atomic
    Ok(())
})?;
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
   gh pr create --base epic-26-core-types --head epic-26-story-225-json-value

   # WRONG: Never PR directly to main
   gh pr create --base main --head epic-26-story-225-json-value  # NEVER DO THIS
   ```

2. **Epic branches merge to develop** (after all stories complete)
   ```bash
   git checkout develop
   git merge --no-ff epic-26-core-types
   ```

3. **develop merges to main** (at milestone boundaries)
   ```bash
   git checkout main
   git merge --no-ff develop -m "M5: Complete"
   ```

4. **main is protected** - requires PR, no direct pushes

### The `complete-story.sh` Script
The script automatically uses the correct base branch:
```bash
./scripts/complete-story.sh 263  # Creates PR to epic-26-core-types
```

**If you manually create a PR, ALWAYS verify the base branch is the epic branch, not main.**

---

## M5 CORE CONCEPTS

### JSON Primitive Goals

M5 adds JSON document support as a **fifth primitive type** alongside:
- KVStore (key-value)
- EventLog (append-only events)
- StateCell (versioned state)
- TraceStore (hierarchical traces)

### Key Design Decisions

| Decision | Rationale |
|----------|-----------|
| Store in ShardedStore | Leverage existing sharding, snapshots, and durability |
| Stateless JsonStore | Same pattern as other primitives, no cache invalidation |
| Path-level operations | Fine-grained reads/writes without loading entire document |
| Region-based conflicts | Only overlapping paths conflict, not entire document |
| Lazy transaction state | Zero overhead when not using JSON in a transaction |

### TypeTag and Key Construction

```rust
// New TypeTag variant
pub enum TypeTag {
    KV = 0x01,
    Event = 0x02,
    State = 0x03,
    Trace = 0x04,
    Json = 0x11,  // NEW
}

// Key construction
impl Key {
    pub fn new_json(ns: Namespace, doc_id: &JsonDocId) -> Self {
        Self::new(TypeTag::Json, ns, doc_id.as_bytes())
    }
}
```

### Fast Path Reads (SnapshotView)

```rust
impl JsonStore {
    /// Fast path read - no transaction overhead
    pub fn get(&self, run_id: &RunId, doc_id: &JsonDocId, path: &JsonPath) -> Result<Option<JsonValue>> {
        let snapshot = self.db.storage().create_snapshot();
        let key = self.key_for(run_id, doc_id);

        match snapshot.get(&key) {
            Some(vv) => {
                let doc = self.deserialize_doc(&vv.value)?;
                Ok(get_at_path(&doc.value, path).cloned())
            }
            None => Ok(None),
        }
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
   cat docs/milestones/M5/M5_IMPLEMENTATION_PLAN.md
   cat docs/milestones/M5/EPIC_<N>_<NAME>.md
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

M5 uses the following GitHub issue numbers:

| Epic | GitHub Issue | Stories |
|------|--------------|---------|
| Epic 26: Core Types | [#256](https://github.com/anibjoshi/in-mem/issues/256) | #263-#267 |
| Epic 27: Path Operations | [#257](https://github.com/anibjoshi/in-mem/issues/257) | #268-#271 |
| Epic 28: JsonStore Core | [#258](https://github.com/anibjoshi/in-mem/issues/258) | #272-#277 |
| Epic 29: WAL Integration | [#259](https://github.com/anibjoshi/in-mem/issues/259) | #278-#281 |
| Epic 30: Transaction Integration | [#260](https://github.com/anibjoshi/in-mem/issues/260) | #282-#286 |
| Epic 31: Conflict Detection | [#261](https://github.com/anibjoshi/in-mem/issues/261) | #287-#290 |
| Epic 32: Validation & Non-Regression | [#262](https://github.com/anibjoshi/in-mem/issues/262) | #291-#294 |

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

### M5-Specific Validation

```bash
# Run M5 benchmarks
~/.cargo/bin/cargo bench --bench m5_performance

# Run non-regression tests (verify M4 targets maintained)
~/.cargo/bin/cargo bench --bench m4_performance

# Verify JSON operations meet targets
~/.cargo/bin/cargo test --test m5_json_integration
```

### Validation Phases

| Phase | Focus | Time |
|-------|-------|------|
| 1 | Automated checks (build, test, clippy, fmt) | 5 min |
| 2 | Story completion verification | 10 min |
| 3 | Spec compliance review (6 rules) | 15 min |
| 4 | Non-regression verification (M4 targets) | 10 min |
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

### Non-Regression (M4 Targets Must Be Maintained)

| Metric | M4 Target | M5 Requirement |
|--------|-----------|----------------|
| KV put (InMemory) | < 3µs | No regression |
| KV put (Buffered) | < 30µs | No regression |
| KV get (fast path) | < 5µs | No regression |
| Event append | < 10µs | No regression |
| State read | < 5µs | No regression |
| Trace append | < 15µs | No regression |

### JSON Targets

| Operation | Document Size | Target |
|-----------|---------------|--------|
| JSON create | 1KB | < 1ms |
| JSON get at path | 1KB | < 100µs |
| JSON set at path | 1KB | < 1ms |
| JSON delete at path | 1KB | < 500µs |

---

## Parallelization Strategy

### Phase 1: Foundation (Days 1-2)
- **Claude 1**: Epic 26 (Core Types) - BLOCKS ALL

### Phase 2: Core Implementation (Days 3-5)
After Epic 26 complete:
- **Claude 1**: Epic 27 (Path Operations)
- **Claude 2**: Epic 28 (JsonStore Core) - Wait for Epic 27 Story #230
- **Claude 3**: Epic 29 (WAL Integration) - Wait for Epic 26

### Phase 3: Transaction & Conflict (Days 6-8)
After Epic 28 complete:
- **Claude 1**: Epic 30 (Transaction Integration)
- **Claude 2**: Epic 31 (Conflict Detection) - Wait for Epic 30

### Phase 4: Validation (Days 9-10)
After all implementation:
- **All**: Epic 32 validation stories

---

*End of M5 Prompt Header - Epic-specific content follows below*
