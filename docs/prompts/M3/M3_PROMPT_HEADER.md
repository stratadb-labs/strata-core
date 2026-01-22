# M3 Epic Prompt Header

**Copy this header to the top of every M3 epic prompt file (Epics 13-19).**

---

## AUTHORITATIVE SPECIFICATIONS - READ THESE FIRST

**`docs/architecture/M3_ARCHITECTURE.md` is the GOSPEL for ALL M3 implementation.**

This is not a guideline. This is not a suggestion. This is the **LAW**.

### Rules for Every Story in Every Epic of M3:

1. **Every story MUST implement behavior EXACTLY as specified in the architecture document**
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

### What the M3 Spec Defines (Read Before Any M3 Work):

| Section | Content | You MUST Follow |
|---------|---------|-----------------|
| Section 3 | Primitives Overview | Stateless facades, run isolation |
| Section 4 | KVStore | get/put/delete/list, TTL metadata |
| Section 5 | EventLog | Append-only, hash chaining, single-writer-ordered |
| Section 6 | StateCell | CAS semantics, transition purity requirement |
| Section 7 | TraceStore | Indices, tree reconstruction, performance warning |
| Section 8 | RunIndex | Status transitions, cascading delete |
| Section 10 | Transaction Integration | Extension traits, cross-primitive atomicity |
| Section 12 | Invariant Enforcement | What primitives must enforce |

### Before Starting ANY Story:

```bash
# 1. Read the full M3 architecture spec
cat docs/architecture/M3_ARCHITECTURE.md

# 2. Read the implementation plan for your story
cat docs/milestones/M3_IMPLEMENTATION_PLAN.md

# 3. Identify which sections apply to your story
# 4. Understand the EXACT behavior required
# 5. Implement EXACTLY that behavior
# 6. Write tests that validate spec compliance
```

**WARNING**: Code review will verify spec compliance. Non-compliant code will be rejected.

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
   /opt/homebrew/bin/gh pr create --base epic-13-primitives-foundation --head epic-13-story-166-crate-setup

   # WRONG: Never PR directly to main
   /opt/homebrew/bin/gh pr create --base main --head epic-13-story-166-crate-setup  # NEVER DO THIS
   ```

2. **Epic branches merge to develop** (after all stories complete)
   ```bash
   git checkout develop
   git merge --no-ff epic-13-primitives-foundation
   ```

3. **develop merges to main** (at milestone boundaries)
   ```bash
   git checkout main
   git merge --no-ff develop -m "M3: Complete"
   ```

4. **main is protected** - requires PR, no direct pushes

### The `complete-story.sh` Script
The script automatically uses the correct base branch:
```bash
./scripts/complete-story.sh 166  # Creates PR to epic-13-primitives-foundation
```

**If you manually create a PR, ALWAYS verify the base branch is the epic branch, not main.**

---

## M3 CORE PRINCIPLES

### Stateless Facade Pattern

**All M3 primitives are stateless facades over the Database engine.**

```rust
/// CORRECT: Stateless facade - only holds Arc<Database>
pub struct KVStore {
    db: Arc<Database>,  // This is ALL the state
}

/// WRONG: Holding additional state
pub struct KVStore {
    db: Arc<Database>,
    cache: HashMap<Key, Value>,  // NEVER DO THIS IN M3
}
```

**Why stateless?**
- Multiple instances of same primitive can coexist safely
- No warm-up or cache invalidation concerns
- Idempotent retry works correctly
- Replay produces same results

### Run Isolation

**Every operation is scoped to a run_id.**

```rust
// Key construction always includes run's namespace
let key = Key::new_kv(Namespace::for_run(run_id), user_key);
```

Different runs CANNOT see each other's data. This is enforced by key prefix isolation.

### Invariant Enforcement

**Primitives enforce invariants, not just wrap storage.**

| Primitive | Enforced Invariants |
|-----------|---------------------|
| KVStore | Key uniqueness per namespace |
| EventLog | Append-only, monotonic sequences, chain integrity |
| StateCell | Version monotonicity (CAS cannot go backward) |
| TraceStore | ID uniqueness, parent must exist for children |
| RunIndex | Status transition validity |

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
- GitHub CLI: `/opt/homebrew/bin/gh`

Do NOT use `gh` without the full path - it may not be in PATH.

---

## Story Workflow

1. **Start story**: `./scripts/start-story.sh <epic> <story> <description>`
2. **Read specs**:
   ```bash
   cat docs/architecture/M3_ARCHITECTURE.md
   cat docs/milestones/M3_IMPLEMENTATION_PLAN.md
   ```
3. **Write tests first** (TDD)
4. **Implement code** to pass tests
5. **Run validation**:
   ```bash
   ~/.cargo/bin/cargo test --all
   ~/.cargo/bin/cargo clippy --all -- -D warnings
   ~/.cargo/bin/cargo fmt --check
   ```
6. **Complete story**: `./scripts/complete-story.sh <story>`

---

## GitHub Issue References

M3 uses the following GitHub issue numbers:

| Epic | GitHub Issue | Stories |
|------|--------------|---------|
| Epic 13: Primitives Foundation | [#159](https://github.com/anibjoshi/in-mem/issues/159) | #166-#168 |
| Epic 14: KVStore Primitive | [#160](https://github.com/anibjoshi/in-mem/issues/160) | #169-#173 |
| Epic 15: EventLog Primitive | [#161](https://github.com/anibjoshi/in-mem/issues/161) | #174-#179 |
| Epic 16: StateCell Primitive | [#162](https://github.com/anibjoshi/in-mem/issues/162) | #180-#184 |
| Epic 17: TraceStore Primitive | [#163](https://github.com/anibjoshi/in-mem/issues/163) | #185-#190 |
| Epic 18: RunIndex Primitive | [#164](https://github.com/anibjoshi/in-mem/issues/164) | #191-#196 |
| Epic 19: Integration & Validation | [#165](https://github.com/anibjoshi/in-mem/issues/165) | #197-#201 |

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

### Validation Phases

| Phase | Focus | Time |
|-------|-------|------|
| 1 | Automated checks (build, test, clippy, fmt) | 5 min |
| 2 | Story completion verification | 10 min |
| 3 | Spec compliance review | 15 min |
| 4 | Code review checklist | 20 min |
| 5 | Best practices verification | 10 min |
| 6 | Epic-specific validation | 10 min |
| 7 | Final sign-off | 5 min |

**Total**: ~75 minutes per epic

### After Validation Passes

```bash
# Merge epic to develop
git checkout develop
git merge --no-ff epic-<N>-<name> -m "Epic <N>: <Name> complete"
git push origin develop

# Close epic issue
/opt/homebrew/bin/gh issue close <epic-issue> --comment "Epic complete. All validation passed."
```

---

*End of M3 Prompt Header - Epic-specific content follows below*
