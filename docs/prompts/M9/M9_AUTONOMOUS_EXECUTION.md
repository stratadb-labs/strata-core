# M9 API Stabilization & Universal Protocol - Autonomous Execution Prompt

**Usage**: `claude --dangerously-skip-permissions -p "$(cat docs/prompts/M9/M9_AUTONOMOUS_EXECUTION.md)"`

---

## Task

Execute M9 Epics 60-64 with phased implementation and epic-end validation after each epic.

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

## M9 Philosophy

> M9 is not about features. M9 is about **contracts**.
>
> Before building the server (M10), before adding Python clients (M12), the interface must be stable. M9 separates invariants from conveniences and substrate from product.
>
> "What is the universal way a user interacts with anything in Strata?" This milestone answers that question.

## The Four Architectural Rules

These rules are NON-NEGOTIABLE. Violating any is a blocking issue.

1. **Every Read Returns Versioned<T>**: No read operation may return raw values without version information
2. **Every Write Returns Version**: Every mutation returns the version it created
3. **Transaction Trait Covers All Primitives**: Every primitive operation is accessible through `TransactionOps`
4. **Run Scope Is Always Explicit**: The run is always known. No ambient run context.

## The Seven Invariants

Every primitive must conform to these invariants:

1. **Addressable**: Every entity has a stable identity via `EntityRef`
2. **Versioned**: Every read returns `Versioned<T>`, every write returns `Version`
3. **Transactional**: Every primitive participates in transactions
4. **Lifecycle**: Every primitive follows create/exist/evolve/destroy
5. **Run-scoped**: Every entity belongs to exactly one run
6. **Introspectable**: Every primitive has `exists()` or equivalent
7. **Read/Write**: Reads never modify state, writes always produce versions

## Execution Pattern

For each epic in the recommended order:

1. **Read specs first**:
   - `docs/architecture/M9_ARCHITECTURE.md` (AUTHORITATIVE)
   - `docs/architecture/PRIMITIVE_CONTRACT.md` (7 invariants)
   - `docs/prompts/M9/epic-{N}-claude-prompts.md`
   - `docs/milestones/M9/EPIC_{N}_*.md`

2. **Start epic branch**: `./scripts/start-story.sh {epic} {first-story} {desc}`

3. **Implement all stories** per epic prompt

4. **Run epic-end validation**: `docs/prompts/EPIC_END_VALIDATION.md` Phase 6e

5. **Merge to develop**:
   ```bash
   git checkout develop
   git merge --no-ff epic-{N}-* -m "Epic {N}: {Name} complete"
   git push origin develop
   ```

6. **Proceed to next epic**

## Recommended Execution Order

M9 uses a **phased approach**. Do not convert all 7 primitives in one pass.

### Phase 1: Foundation (Epic 60 + Epic 63)

**Epics**: 60 (Core Types) + 63 (Error Standardization)

1. **Epic 60: Core Types** - CRITICAL FOUNDATION
   - Start: `./scripts/start-story.sh 60 469 entity-ref`
   - Stories #469-#474: EntityRef, Versioned<T>, Version, Timestamp, PrimitiveType, RunId

2. **Epic 63: Error Standardization** - depends on EntityRef
   - Start: `./scripts/start-story.sh 63 488 strata-error`
   - Stories #488-#491: StrataError enum, conversions, EntityRef in errors, documentation

3. **Run epic-end validation for both epics**

**Exit Criteria**: All core types implemented and tested. StrataError complete with EntityRef context.

### Phase 2: First Two Primitives (KV + EventLog)

**Epics**: 61 (partial) + 62 (partial) + 64 (partial)

1. **Epic 61 Phase 2**: KVStore + EventLog versioned returns
   - Stories #475, #476

2. **Epic 62 Phase 2**: TransactionOps trait + KV/Event operations
   - Stories #482, #483, #484

3. **Epic 64 Phase 2**: Conformance tests for KV + EventLog
   - Story #492 (partial): 28 tests (7 invariants × 2 primitives × 2)

**Exit Criteria**: KV and EventLog fully conform to all 7 invariants. Pattern proven.

### Phase 3: Extend to State + Trace

1. **Epic 61 Phase 3**: StateCell + TraceStore versioned returns
   - Stories #477, #478

2. **Epic 62 Phase 3**: State/Trace in TransactionOps
   - Story #485

3. **Epic 64 Phase 3**: Conformance tests for State + Trace
   - Story #493 (partial): +28 tests

**Exit Criteria**: 4 primitives fully conformant.

### Phase 4: Complete Remaining Primitives

1. **Epic 61 Phase 4**: JsonStore + VectorStore + RunIndex versioned returns
   - Stories #479, #480, #481

2. **Epic 62 Phase 4**: Json/Vector in TransactionOps
   - Story #486

3. **Epic 64 Phase 4**: Conformance tests for remaining primitives
   - Story #494 (partial): +42 tests

**Exit Criteria**: All 7 primitives fully conformant.

### Phase 5: Finalize

1. **Epic 62 Phase 5**: RunHandle pattern
   - Story #487

2. **Epic 64 Phase 5**: Complete conformance testing
   - Stories #495, #496: Invariant 7 tests + Cross-primitive tests

3. **Final validation**: All 49 conformance tests passing

**Exit Criteria**: M9 complete. API stable. Ready for M10 (server).

## GitHub Issue Mapping

| Epic | GitHub Issue | Story Issues | Phase |
|------|--------------|--------------|-------|
| Epic 60: Core Types | #464 | #469-#474 | 1 |
| Epic 63: Error Standardization | #467 | #488-#491 | 1 |
| Epic 61: Versioned Returns | #465 | #475-#481 | 2-4 |
| Epic 62: Transaction Unification | #466 | #482-#487 | 2-5 |
| Epic 64: Conformance Testing | #468 | #492-#496 | 2-5 |

## Stop Conditions

- Any architectural rule violation (4 rules)
- Any invariant violation (7 invariants)
- Epic-end validation failure
- Test failures that can't be resolved
- API change that breaks M7/M8 functionality
- Performance regression > 5%

## Validation Between Phases

After each phase, run the validation from `docs/prompts/EPIC_END_VALIDATION.md`:

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
# Run M9 conformance tests (as they are implemented)
~/.cargo/bin/cargo test --test conformance

# Run M9 invariant tests
~/.cargo/bin/cargo test invariant_

# Verify primitives conform (add as each phase completes)
~/.cargo/bin/cargo test conformance::kv      # Phase 2+
~/.cargo/bin/cargo test conformance::event   # Phase 2+
~/.cargo/bin/cargo test conformance::state   # Phase 3+
~/.cargo/bin/cargo test conformance::trace   # Phase 3+
~/.cargo/bin/cargo test conformance::json    # Phase 4+
~/.cargo/bin/cargo test conformance::vector  # Phase 4+
~/.cargo/bin/cargo test conformance::run     # Phase 4+

# Verify non-regression (M7/M8 targets maintained)
~/.cargo/bin/cargo bench --bench m8_vector_performance
~/.cargo/bin/cargo bench --bench m7_recovery_performance
```

### Four Rules Quick Check

```bash
# Rule 1: Every read returns Versioned<T>
grep -r "fn get.*Result<Option<Versioned" crates/primitives/src/ && echo "PASS" || echo "CHECK"

# Rule 2: Every write returns Version
grep -r "fn put.*Result<Version" crates/primitives/src/ && echo "PASS" || echo "CHECK"

# Rule 3: TransactionOps covers all primitives
grep -r "fn kv_\|fn event_\|fn state_\|fn trace_\|fn json_\|fn vector_" crates/concurrency/src/transaction_ops.rs

# Rule 4: Run scope is explicit
grep -r "run_id: &RunId" crates/primitives/src/ | wc -l
# Should be present in all primitive methods
```

## Start

Begin with Phase 1: Epic 60 (Core Types) + Epic 63 (Error Standardization).

Read the specs, implement EntityRef first (foundation for everything else), then proceed through the types.

Remember: **Do not try to convert all 7 primitives in one pass.** Prove the pattern works with KV + EventLog in Phase 2 before generalizing.
