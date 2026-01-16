# M4 Epic Prompt Header

**Copy this header to the top of every M4 epic prompt file (Epics 20-25).**

---

## AUTHORITATIVE SPECIFICATIONS - READ THESE FIRST

**`docs/architecture/M4_ARCHITECTURE.md` is the GOSPEL for ALL M4 implementation.**

This is not a guideline. This is not a suggestion. This is the **LAW**.

### Rules for Every Story in Every Epic of M4:

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

### What the M4 Spec Defines (Read Before Any M4 Work):

| Section | Content | You MUST Follow |
|---------|---------|-----------------|
| Section 2 | Key Design Decisions | DurabilityMode enum, DashMap + HashMap, pooling |
| Section 3 | Durability Modes | InMemory, Buffered, Strict implementations |
| Section 4 | Sharded Storage | ShardedStore, per-RunId sharding |
| Section 5 | Transaction Pooling | Thread-local pools, reset() method |
| Section 6 | Read Path Optimization | Fast path reads, snapshot-based |
| Section 7 | Red Flag Thresholds | Hard stop criteria, non-negotiable |

### Before Starting ANY Story:

```bash
# 1. Read the full M4 architecture spec
cat docs/architecture/M4_ARCHITECTURE.md

# 2. Read the implementation plan for your story
cat docs/milestones/M4_IMPLEMENTATION_PLAN.md

# 3. Read the epics document
cat docs/milestones/M4_EPICS.md

# 4. Identify which sections apply to your story
# 5. Understand the EXACT behavior required
# 6. Implement EXACTLY that behavior
# 7. Write tests that validate spec compliance
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
   gh pr create --base epic-20-performance-foundation --head epic-20-story-217-baseline

   # WRONG: Never PR directly to main
   gh pr create --base main --head epic-20-story-217-baseline  # NEVER DO THIS
   ```

2. **Epic branches merge to develop** (after all stories complete)
   ```bash
   git checkout develop
   git merge --no-ff epic-20-performance-foundation
   ```

3. **develop merges to main** (at milestone boundaries)
   ```bash
   git checkout main
   git merge --no-ff develop -m "M4: Complete"
   ```

4. **main is protected** - requires PR, no direct pushes

### The `complete-story.sh` Script
The script automatically uses the correct base branch:
```bash
./scripts/complete-story.sh 217  # Creates PR to epic-20-performance-foundation
```

**If you manually create a PR, ALWAYS verify the base branch is the epic branch, not main.**

---

## CRITICAL INVARIANTS (NON-NEGOTIABLE)

**These invariants MUST be maintained throughout ALL M4 implementation. Violating any of these is a blocking issue.**

### 1. Atomicity Scope
> **Transactions are atomic within a single RunId ONLY. Cross-run atomicity is NOT guaranteed.**

This is intentional - per-run isolation enables the sharding that makes M4 fast. Do NOT try to provide cross-run atomicity.

### 2. Snapshot Semantic Invariant
> **Fast-path reads must be observationally equivalent to a snapshot-based transaction.**

This means:
- No dirty reads (uncommitted data)
- No torn reads (partial write sets)
- No stale reads (older than snapshot version)
- No mixing versions (key A at version X, key B at version Y where Y > X)

**"Latest committed at snapshot acquisition"** is the correct definition.

### 3. Thread Lifecycle (Buffered Mode)
> **The background flush thread MUST have proper lifecycle management.**

Required fields:
- `shutdown: AtomicBool` - Shutdown signal
- `flush_thread: Option<JoinHandle<()>>` - Thread handle

Required `Drop` impl:
```rust
impl Drop for BufferedDurability {
    fn drop(&mut self) {
        self.shutdown.store(true, Ordering::Release);
        self.flush_signal.notify_all();
        if let Some(handle) = self.flush_thread.take() {
            let _ = handle.join();
        }
    }
}
```

### 4. Required Dependencies
```toml
[dependencies]
dashmap = "5"
rustc-hash = "1.1"    # NOT fxhash - provides FxHashMap
parking_lot = "0.12"
```

**IMPORTANT**: Use `rustc-hash` crate, NOT `fxhash`. Import as: `use rustc_hash::{FxHashMap, FxBuildHasher};`

---

## M4 CORE PRINCIPLES

### Philosophy: Fastable, Not Fast

**M4 does not aim to be fast. M4 aims to be *fastable*.**

M4 removes architectural blockers that prevent Redis-class performance:
- Durability modes eliminate fsync penalty
- Sharded storage eliminates lock contention
- Transaction pooling eliminates allocation overhead
- Read fast path eliminates transaction overhead

### Three Durability Modes

```rust
/// InMemory: No persistence, fastest (<3µs)
/// Buffered: WAL + async fsync, balanced (<30µs)
/// Strict: WAL + sync fsync, safest (~2ms)
pub enum DurabilityMode {
    InMemory,
    Buffered { flush_interval_ms: u64, max_pending_writes: usize },
    Strict,
}
```

**Default**: Strict (backwards compatible with M3)

### Red Flag Thresholds (Hard Stops)

| Metric | Threshold | Action if Exceeded |
|--------|-----------|-------------------|
| Snapshot acquisition | > 2µs | Redesign snapshot mechanism |
| A1/A0 ratio | > 20× | Remove abstraction layers |
| B/A1 ratio | > 8× | Inline facade logic |
| Disjoint scaling (4T) | < 2.5× | Redesign sharding |
| p99 latency | > 20× mean | Fix tail latency source |
| Hot-path allocations | > 0 | Eliminate allocations |

**If ANY red flag is triggered: STOP and REDESIGN. No exceptions.**

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
   cat docs/architecture/M4_ARCHITECTURE.md
   cat docs/milestones/M4_IMPLEMENTATION_PLAN.md
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

M4 uses the following GitHub issue numbers:

| Epic | GitHub Issue | Stories |
|------|--------------|---------|
| Epic 20: Performance Foundation | [#211](https://github.com/anibjoshi/in-mem/issues/211) | #217-#220 |
| Epic 21: Durability Modes | [#212](https://github.com/anibjoshi/in-mem/issues/212) | #221-#226 |
| Epic 22: Sharded Storage | [#213](https://github.com/anibjoshi/in-mem/issues/213) | #227-#231 |
| Epic 23: Transaction Pooling | [#214](https://github.com/anibjoshi/in-mem/issues/214) | #232-#235 |
| Epic 24: Read Path Optimization | [#215](https://github.com/anibjoshi/in-mem/issues/215) | #236-#239 |
| Epic 25: Validation & Red Flags | [#216](https://github.com/anibjoshi/in-mem/issues/216) | #240-#244 |

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

### M4-Specific Validation

```bash
# Run M4 benchmarks
~/.cargo/bin/cargo bench --bench m4_performance

# Run red flag tests
~/.cargo/bin/cargo test --test m4_red_flags

# Verify facade tax
~/.cargo/bin/cargo bench --bench m4_facade_tax
```

### Validation Phases

| Phase | Focus | Time |
|-------|-------|------|
| 1 | Automated checks (build, test, clippy, fmt) | 5 min |
| 2 | Story completion verification | 10 min |
| 3 | Spec compliance review | 15 min |
| 4 | Performance validation (benchmarks) | 15 min |
| 5 | Red flag verification | 10 min |
| 6 | Code review checklist | 20 min |
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

| Metric | Target | Red Flag |
|--------|--------|----------|
| `engine/put_direct` (InMemory) | < 3µs | > 10µs |
| `kvstore/put` (InMemory) | < 8µs | > 20µs |
| `kvstore/get` | < 5µs | > 10µs |
| Throughput (1-thread InMemory) | 250K ops/sec | < 100K ops/sec |
| Throughput (4-thread disjoint) | 800K ops/sec | < 400K ops/sec |
| Snapshot acquisition | < 500ns | > 2µs |
| A1/A0 | < 10× | > 20× |
| B/A1 | < 5× | > 8× |

---

## Parallelization Strategy

### Phase 1: Foundation (Day 1)
- **Claude 1**: Story #217 (baseline tag) - BLOCKS ALL

### Phase 2: Core Optimizations (Days 2-4)
After Epic 20 complete:
- **Claude 1**: Epic 21 (Durability)
- **Claude 2**: Epic 22 (Sharded Storage)
- **Claude 3**: Epic 23 (Transaction Pooling)

### Phase 3: Read Optimization (Day 5)
After Epic 22 complete:
- **Claude 1-3**: Epic 24 stories in parallel

### Phase 4: Validation (Day 6-7)
After all optimizations:
- **All**: Epic 25 validation stories

---

*End of M4 Prompt Header - Epic-specific content follows below*
