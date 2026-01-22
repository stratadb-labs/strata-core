# M9 Epic Prompt Header

**Copy this header to the top of every M9 epic prompt file (Epics 60-64).**

---

## NAMING CONVENTION - CRITICAL

> **NEVER use "M9" or "Strata" in the actual codebase or comments.**
>
> - "M9" is an internal milestone tracker only - do not use it in code, comments, or user-facing text
> - All existing crates refer to the database as "in-mem" - use this name consistently
> - Do not use "Strata" anywhere in the codebase
> - This applies to: code, comments, docstrings, error messages, log messages, test names
>
> **CORRECT**: `//! Universal entity reference for any in-mem entity`
> **WRONG**: `//! Universal entity reference for any Strata entity`

---

## AUTHORITATIVE SPECIFICATIONS - READ THESE FIRST

**The following documents are GOSPEL for ALL M9 implementation:**

1. **`docs/architecture/M9_ARCHITECTURE.md`** - THE AUTHORITATIVE SPECIFICATION
2. **`docs/architecture/PRIMITIVE_CONTRACT.md`** - The seven invariants
3. `docs/milestones/M9/M9_IMPLEMENTATION_PLAN.md` - Epic/Story breakdown and implementation details
4. `docs/milestones/M9/EPIC_60_CORE_TYPES.md` through `EPIC_64_CONFORMANCE_TESTING.md` - Story-level specifications

**The architecture spec is LAW.** The implementation plan and epic docs provide execution details but MUST NOT contradict the architecture spec.

This is not a guideline. This is not a suggestion. This is the **LAW**.

### Rules for Every Story in Every Epic of M9:

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

## THE FOUR ARCHITECTURAL RULES (NON-NEGOTIABLE)

**These rules MUST be followed in EVERY M9 story. Violating any of these is a blocking issue.**

### Rule 1: Every Read Returns Versioned<T>

> **No read operation may return raw values without version information.**

```rust
// CORRECT: Read returns versioned
pub fn get(&self, run_id: &RunId, key: &str) -> Result<Option<Versioned<Value>>> {
    match self.store.get(&key)? {
        Some(entry) => Ok(Some(Versioned::new(
            Value::from_bytes(&entry.value)?,
            Version::TxnId(entry.version),
            Timestamp::from_micros(entry.timestamp),
        ))),
        None => Ok(None),
    }
}

// WRONG: Read returns raw value
pub fn get(&self, run_id: &RunId, key: &str) -> Result<Option<Value>> {
    // NEVER DO THIS - version info is lost
    self.store.get(&key)
}
```

### Rule 2: Every Write Returns Version

> **Every mutation returns the version it created.**

```rust
// CORRECT: Write returns version
pub fn put(&self, run_id: &RunId, key: &str, value: Value) -> Result<Version> {
    let version = self.store.put(&key, value.to_bytes()?)?;
    Ok(Version::TxnId(version))
}

// WRONG: Write returns nothing
pub fn put(&self, run_id: &RunId, key: &str, value: Value) -> Result<()> {
    // NEVER DO THIS - caller can't know what version was created
    self.store.put(&key, value.to_bytes()?)?;
    Ok(())
}
```

### Rule 3: Transaction Trait Covers All Primitives

> **Every primitive operation is accessible through the `TransactionOps` trait.**

```rust
// CORRECT: All primitives accessible through trait
pub trait TransactionOps {
    // KV
    fn kv_get(&self, key: &str) -> Result<Option<Versioned<Value>>, StrataError>;
    fn kv_put(&mut self, key: &str, value: Value) -> Result<Version, StrataError>;

    // Event
    fn event_append(&mut self, event_type: &str, payload: Value) -> Result<Version, StrataError>;

    // State, Trace, Json, Vector - all included
    // ...
}

// WRONG: Some primitives not in trait
pub trait TransactionOps {
    fn kv_get(&self, key: &str) -> Result<Option<Value>>;
    // Missing: event, state, trace, json, vector - NEVER DO THIS
}
```

### Rule 4: Run Scope Is Always Explicit

> **The run is always known. No ambient run context.**

```rust
// CORRECT: Run is explicit
pub fn get(&self, run_id: &RunId, key: &str) -> Result<Option<Versioned<Value>>> {
    let namespace = self.namespace(run_id);  // Run is explicit
    self.store.get(&namespace, &key)
}

// WRONG: Thread-local run state
thread_local! {
    static CURRENT_RUN: RefCell<Option<RunId>> = RefCell::new(None);
}

pub fn get(&self, key: &str) -> Result<Option<Value>> {
    let run_id = CURRENT_RUN.with(|r| r.borrow().clone()).unwrap();  // NEVER DO THIS
    // ...
}
```

---

## THE SEVEN INVARIANTS (FROM PRIMITIVE_CONTRACT.md)

Every primitive MUST conform to these invariants. M9 tests verify conformance.

| # | Invariant | API Expression | Test Focus |
|---|-----------|----------------|------------|
| 1 | Everything is Addressable | `EntityRef` type | Can create EntityRef for any entity |
| 2 | Everything is Versioned | `Versioned<T>` wrapper | Reads return Versioned, writes return Version |
| 3 | Everything is Transactional | `TransactionOps` trait | All primitives in same transaction |
| 4 | Everything Has a Lifecycle | CRUD method patterns | Create/exist/evolve/destroy |
| 5 | Everything Exists Within a Run | `RunId` parameter | Entities isolated by run |
| 6 | Everything is Introspectable | `exists()` methods | Can check existence |
| 7 | Reads and Writes Have Consistent Semantics | `&self` vs `&mut self` | Reads don't modify, writes produce versions |

---

## PHASED IMPLEMENTATION STRATEGY (NON-NEGOTIABLE)

> **Do not try to convert all 7 primitives in one pass.**

M9 uses a phased approach to reduce risk and maintain momentum:

### Phase 1: Foundation (Epic 60 + Epic 63)
- Implement all core types: `EntityRef`, `Versioned<T>`, `Version`, `Timestamp`, `PrimitiveType`, `RunId`
- Implement `StrataError` enum with all variants
- All types tested independently

**Exit Criteria**: All 6 stories in Epic 60 complete. All 4 stories in Epic 63 complete.

### Phase 2: First Two Primitives (KV + EventLog)
- Apply versioned returns to KV and EventLog only
- Wire TransactionOps for these two
- Write conformance tests (28 tests)

**Exit Criteria**: KV and EventLog fully conform to all 7 invariants. Pattern proven.

### Phase 3: Extend to State + Trace
- Apply the proven pattern to StateCell and TraceStore
- Wire TransactionOps
- Write conformance tests (+28 tests)

**Exit Criteria**: 4 primitives fully conformant.

### Phase 4: Complete Remaining Primitives
- Apply to JsonStore, VectorStore, RunIndex
- Wire TransactionOps
- Write conformance tests (+42 tests)

**Exit Criteria**: All 7 primitives fully conformant.

### Phase 5: Finalize
- RunHandle pattern implementation
- Cross-primitive transaction conformance tests
- Documentation update

**Exit Criteria**: M9 complete. API stable.

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
   gh pr create --base epic-60-core-types --head epic-60-story-469-entity-ref

   # WRONG: Never PR directly to main
   gh pr create --base main --head epic-60-story-469-entity-ref  # NEVER DO THIS
   ```

2. **Epic branches merge to develop** (after all stories complete)
   ```bash
   git checkout develop
   git merge --no-ff epic-60-core-types
   ```

3. **develop merges to main** (at milestone boundaries)
   ```bash
   git checkout main
   git merge --no-ff develop -m "M9: Complete"
   ```

4. **main is protected** - requires PR, no direct pushes

### The `complete-story.sh` Script
The script automatically uses the correct base branch:
```bash
./scripts/complete-story.sh 469  # Creates PR to epic-60-core-types
```

**If you manually create a PR, ALWAYS verify the base branch is the epic branch, not main.**

---

## M9 CORE CONCEPTS

### What M9 Is About

M9 is an **API stabilization milestone**. It defines:

| Aspect | M9 Commits To |
|--------|---------------|
| **EntityRef** | Universal addressing for all primitives |
| **Versioned<T>** | Every read includes version info |
| **Version** | Every write returns the version created |
| **TransactionOps** | Unified trait for all primitives |
| **StrataError** | Unified error type |
| **7 Invariants** | Conformance tests for all primitives |

### What M9 Is NOT

M9 is **not** an optimization or feature milestone. Deferred items:

| Deferred Item | Target |
|---------------|--------|
| Wire protocol | M10 |
| Server implementation | M10 |
| Performance optimization | M11 |
| Python SDK | M12 |
| New primitives | Post-MVP |

### Key Design Decisions

| Decision | Rationale |
|----------|-----------|
| Versioned<T> wrapper | Version info is never lost |
| Version enum variants | Different primitives have different versioning |
| TransactionOps trait | One unified API for all primitives |
| StrataError with EntityRef | Errors have context |
| 49 conformance tests | Every primitive × every invariant |

### Core Types

```rust
/// Universal entity reference
pub enum EntityRef {
    Kv { run_id: RunId, key: String },
    Event { run_id: RunId, sequence: u64 },
    State { run_id: RunId, name: String },
    Trace { run_id: RunId, trace_id: TraceId },
    Run { run_id: RunId },
    Json { run_id: RunId, doc_id: JsonDocId },
    Vector { run_id: RunId, collection: String, vector_id: VectorId },
}

/// Versioned value wrapper
pub struct Versioned<T> {
    pub value: T,
    pub version: Version,
    pub timestamp: Timestamp,
}

/// Version identifier
pub enum Version {
    TxnId(u64),      // For mutable primitives
    Sequence(u64),   // For append-only primitives
    Counter(u64),    // For CAS operations
}

/// Unified error type
pub enum StrataError {
    NotFound { entity_ref: EntityRef },
    VersionConflict { entity_ref: EntityRef, expected: Version, actual: Version },
    WriteConflict { entity_ref: EntityRef },
    TransactionAborted { reason: String },
    RunNotFound { run_id: RunId },
    InvalidOperation { entity_ref: EntityRef, reason: String },
    DimensionMismatch { expected: usize, got: usize },
    CollectionNotFound { run_id: RunId, collection: String },
    Storage { message: String, source: Option<Box<dyn Error + Send + Sync>> },
    Serialization { message: String },
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
   cat docs/milestones/M9/M9_IMPLEMENTATION_PLAN.md
   cat docs/milestones/M9/EPIC_<N>_<NAME>.md
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

M9 uses the following GitHub issue numbers:

| Epic | GitHub Issue | Stories (GitHub Issues) |
|------|--------------|-------------------------|
| Epic 60: Core Types | [#464](https://github.com/anibjoshi/in-mem/issues/464) | #469-#474 |
| Epic 61: Versioned Returns | [#465](https://github.com/anibjoshi/in-mem/issues/465) | #475-#481 |
| Epic 62: Transaction Unification | [#466](https://github.com/anibjoshi/in-mem/issues/466) | #482-#487 |
| Epic 63: Error Standardization | [#467](https://github.com/anibjoshi/in-mem/issues/467) | #488-#491 |
| Epic 64: Conformance Testing | [#468](https://github.com/anibjoshi/in-mem/issues/468) | #492-#496 |

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

### M9-Specific Validation

```bash
# Run M9 conformance tests
~/.cargo/bin/cargo test --test conformance

# Run M9 invariant tests
~/.cargo/bin/cargo test invariant_

# Verify all 7 primitives conform
~/.cargo/bin/cargo test conformance::kv
~/.cargo/bin/cargo test conformance::event
~/.cargo/bin/cargo test conformance::state
~/.cargo/bin/cargo test conformance::trace
~/.cargo/bin/cargo test conformance::json
~/.cargo/bin/cargo test conformance::vector
~/.cargo/bin/cargo test conformance::run

# Verify non-regression (M7/M8 targets maintained)
~/.cargo/bin/cargo bench --bench m8_vector_performance
~/.cargo/bin/cargo bench --bench m7_recovery_performance
```

### Validation Phases

| Phase | Focus | Time |
|-------|-------|------|
| 1 | Automated checks (build, test, clippy, fmt) | 5 min |
| 2 | Story completion verification | 10 min |
| 3 | Spec compliance review (4 rules, 7 invariants) | 15 min |
| 4 | Non-regression verification (M7/M8 targets) | 10 min |
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

### Non-Regression (M7/M8 Targets Must Be Maintained)

| Metric | Target | M9 Requirement |
|--------|--------|----------------|
| KV put (InMemory) | < 3µs | No regression |
| KV get (fast path) | < 5µs | No regression |
| Vector upsert | < 100µs | No regression |
| Vector search (k=10) | < 10ms | No regression |
| Snapshot write (100MB) | < 5s | No regression |
| Recovery (100MB + 10K WAL) | < 5s | No regression |

### M9 Expectations

M9 changes are primarily API changes. Performance impact should be minimal:

| Operation | Expectation |
|-----------|-------------|
| Versioned<T> wrapper | < 1% overhead (stack allocation) |
| Version return | < 1% overhead (already computed) |
| StrataError | No runtime cost (error path only) |
| TransactionOps | No overhead (trait dispatch inlined) |

---

## Evolution Warnings

M9 includes several "evolution warnings" for future extensibility:

1. **API frozen after M9**: M10 (server) and M12 (Python SDK) depend on stable API
2. **EntityRef extensible**: New primitives add variants (but old code compiles)
3. **Version enum extensible**: New version types can be added
4. **StrataError extensible**: New error variants can be added
5. **TransactionOps stable**: Methods can be added, not removed

---

## Parallelization Strategy

### Phase 1: Foundation (Epic 60 + 63)
- **Claude 1**: Epic 60 (Core Types) - FOUNDATION
- **Claude 2**: Epic 63 (Error Standardization) - can start after EntityRef done

### Phase 2: First Primitives (Days 2-3)
After Epic 60 + 63 complete:
- **Claude 1**: Epic 61 Phase 2 (KV + EventLog versioned returns)
- **Claude 2**: Epic 62 Phase 2 (TransactionOps + KV + Event)

### Phase 3: Extend (Days 4-5)
- **Claude 1**: Epic 61 Phase 3-4 (remaining primitives)
- **Claude 2**: Epic 62 Phase 3-4 (remaining primitives)

### Phase 4: Finalize (Days 6-7)
- **All**: Epic 64 (Conformance Testing)
- **All**: Epic 62 Phase 5 (RunHandle)

---

*End of M9 Prompt Header - Epic-specific content follows below*
