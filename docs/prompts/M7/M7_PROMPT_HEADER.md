# M7 Epic Prompt Header

**Copy this header to the top of every M7 epic prompt file (Epics 40-46).**

---

## AUTHORITATIVE SPECIFICATIONS - READ THESE FIRST

**The following documents are GOSPEL for ALL M7 implementation:**

1. **`docs/architecture/M7_ARCHITECTURE.md`** - THE AUTHORITATIVE SPECIFICATION
2. `docs/milestones/M7/M7_IMPLEMENTATION_PLAN.md` - Epic/Story breakdown and implementation details
3. `docs/milestones/M7/EPIC_40_SNAPSHOT_FORMAT.md` through `EPIC_46_VALIDATION.md` - Story-level specifications
4. `docs/diagrams/m7-architecture.md` - Visual architecture diagrams

**The architecture spec is LAW.** The implementation plan and epic docs provide execution details but MUST NOT contradict the architecture spec.

This is not a guideline. This is not a suggestion. This is the **LAW**.

### Rules for Every Story in Every Epic of M7:

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

## THE FIVE ARCHITECTURAL RULES (NON-NEGOTIABLE)

**These rules MUST be followed in EVERY M7 story. Violating any of these is a blocking issue.**

### Rule 1: Recovery Is Deterministic

> **Same WAL + Snapshot = Same state. Always.**

```rust
// CORRECT: Deterministic recovery
fn recover(snapshot: &Snapshot, wal: &WalReader) -> Database {
    let mut state = snapshot.load()?;
    for entry in wal.entries_from(snapshot.wal_offset)? {
        state.apply(entry)?;  // Deterministic application
    }
    state
}

// WRONG: Non-deterministic recovery
fn recover(snapshot: &Snapshot, wal: &WalReader) -> Database {
    let mut state = snapshot.load()?;
    for entry in wal.entries_from(snapshot.wal_offset)? {
        if rand::random::<bool>() {  // NEVER DO THIS
            state.apply(entry)?;
        }
    }
    state
}
```

### Rule 2: Recovery Is Prefix-Consistent

> **After recovery, you see a prefix of the committed transaction history. No partial transactions visible.**

```rust
// CORRECT: Atomic transaction boundaries
impl WalWriter {
    fn commit_transaction(&self, tx: &Transaction) -> Result<()> {
        // Write all entries for this transaction
        for entry in tx.entries() {
            self.write_entry(entry)?;
        }
        // Write commit marker
        self.write_commit_marker(tx.id())?;
        self.sync()?;
        Ok(())
    }
}

// Recovery only includes transactions with commit markers
impl WalReader {
    fn committed_entries(&self) -> impl Iterator<Item = WalEntry> {
        // Skip entries from transactions without commit markers
    }
}

// WRONG: No transaction boundaries
impl WalWriter {
    fn write(&self, entry: WalEntry) -> Result<()> {
        self.file.write(&entry.serialize())?;  // Individual entries without grouping
    }
}
```

### Rule 3: Replay Is Side-Effect Free

> **Replay produces a derived view. It does NOT mutate the canonical store.**

```rust
// CORRECT: Replay returns read-only view
pub fn replay_run(db: &Database, run_id: RunId) -> Result<ReadOnlyView> {
    let events = db.event_log.get_run_events(run_id)?;
    let view = ReplayEngine::replay(events)?;
    Ok(view)  // Read-only view, not mutable state
}

// WRONG: Replay mutates canonical store
pub fn replay_run(db: &Database, run_id: RunId) -> Result<()> {
    let events = db.event_log.get_run_events(run_id)?;
    for event in events {
        db.apply(event)?;  // NEVER DO THIS - mutates canonical store
    }
    Ok(())
}
```

### Rule 4: Snapshots Are Physical, Not Semantic

> **Snapshots compress WAL effects. They are a cache over history, not the history itself.**

```rust
// CORRECT: Snapshot is byte-level materialized state
pub struct Snapshot {
    version: u32,
    timestamp: u64,
    wal_offset: u64,
    kv_data: Vec<u8>,      // Serialized KV state
    json_data: Vec<u8>,    // Serialized JSON state
    event_data: Vec<u8>,   // Serialized Event state
    // ... other primitives
    checksum: u32,
}

// WRONG: Snapshot stores semantic history
pub struct Snapshot {
    transactions: Vec<Transaction>,  // NEVER DO THIS - that's what WAL is for
    event_history: Vec<Event>,       // NEVER DO THIS - that's what EventLog is for
}
```

### Rule 5: Storage APIs Must Be Stable After M7

> **Adding a primitive must NOT require changes to WAL core format, Snapshot core format, Recovery engine, or Replay engine. Only extension points.**

```rust
// CORRECT: Primitive registry for extension
pub trait PrimitiveStorageExt {
    fn wal_entry_types(&self) -> &[u8];          // Entry types this primitive uses
    fn snapshot_serialize(&self) -> Result<Vec<u8>>;      // For snapshots
    fn snapshot_deserialize(&mut self, data: &[u8]) -> Result<()>;  // From snapshots
    fn apply_wal_entry(&mut self, entry: &WalEntry) -> Result<()>;
}

// New primitive (M8 Vector) implements the trait
impl PrimitiveStorageExt for VectorStore {
    fn wal_entry_types(&self) -> &[u8] { &[0x70, 0x71, 0x72] }
    fn snapshot_serialize(&self) -> Result<Vec<u8>> { /* ... */ }
    fn snapshot_deserialize(&mut self, data: &[u8]) -> Result<()> { /* ... */ }
    fn apply_wal_entry(&mut self, entry: &WalEntry) -> Result<()> { /* ... */ }
}

// WRONG: Hardcoded primitive list in recovery
fn recover() {
    match entry_type {
        0x01 => kv_apply(entry),
        0x02 => json_apply(entry),
        // Can't add Vector without modifying this match
    }
}
```

---

## CORE INVARIANTS

### Recovery Invariants (R1-R6)

| # | Invariant | Meaning |
|---|-----------|---------|
| R1 | Deterministic | Same WAL + Snapshot = Same state |
| R2 | Idempotent | Replaying recovery produces identical state |
| R3 | Prefix-consistent | No partial transactions visible after recovery |
| R4 | Never invents data | Only committed data appears |
| R5 | Never drops committed data | All durable commits survive |
| R6 | May drop uncommitted data | Depending on durability mode |

### Replay Invariants (P1-P6)

| # | Invariant | Meaning |
|---|-----------|---------|
| P1 | Pure function | Over (Snapshot, WAL, EventLog) |
| P2 | Side-effect free | Does not mutate canonical store |
| P3 | Derived view | Not a new source of truth |
| P4 | Does not persist | Unless explicitly materialized |
| P5 | Deterministic | Same inputs = Same view |
| P6 | Idempotent | Running twice produces identical view |

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
   gh pr create --base epic-40-snapshot-format --head epic-40-story-347-snapshot-envelope

   # WRONG: Never PR directly to main
   gh pr create --base main --head epic-40-story-347-snapshot-envelope  # NEVER DO THIS
   ```

2. **Epic branches merge to develop** (after all stories complete)
   ```bash
   git checkout develop
   git merge --no-ff epic-40-snapshot-format
   ```

3. **develop merges to main** (at milestone boundaries)
   ```bash
   git checkout main
   git merge --no-ff develop -m "M7: Complete"
   ```

4. **main is protected** - requires PR, no direct pushes

### The `complete-story.sh` Script
The script automatically uses the correct base branch:
```bash
./scripts/complete-story.sh 347  # Creates PR to epic-40-snapshot-format
```

**If you manually create a PR, ALWAYS verify the base branch is the epic branch, not main.**

---

## M7 CORE CONCEPTS

### What M7 Is About

M7 is a **durability correctness milestone**. It defines:

| Aspect | M7 Commits To |
|--------|---------------|
| **Snapshot format** | Single file, versioned, checksummed |
| **Recovery sequence** | Snapshot load + WAL replay |
| **Replay API** | `replay_run(run_id) -> ReadOnlyView` |
| **Diff API** | `diff_runs(a, b) -> Diff` (key-level) |
| **WAL format** | Self-validating entries with CRC32 |
| **Storage extension** | Clear patterns for adding primitives |

### What M7 Is NOT

M7 is **not** an optimization milestone. Deferred items:

| Deferred Item | Target |
|---------------|--------|
| Vector primitive | M8 |
| Compression | M9 |
| Encryption at rest | M11 |
| Incremental snapshots | Post-MVP |
| Point-in-time recovery | Post-MVP |

### Key Design Decisions

| Decision | Rationale |
|----------|-----------|
| CRC32 on every WAL entry | Detect corruption early |
| Transaction commit markers | Prefix-consistent recovery |
| Snapshot is physical state | Fast recovery, not semantic replay |
| PrimitiveStorageExt trait | Extensibility without core changes |
| ReadOnlyView from replay | No mutation of canonical store |

### Core Types

```rust
/// WAL entry envelope
pub struct WalEntry {
    pub length: u32,        // Total bytes after this
    pub entry_type: u8,     // From registry
    pub version: u8,        // Format version
    pub tx_id: TxId,        // 16 bytes
    pub payload: Vec<u8>,   // Type-specific
    pub checksum: u32,      // CRC32
}

/// Snapshot envelope
pub struct SnapshotEnvelope {
    pub magic: [u8; 10],    // "INMEM_SNAP"
    pub version: u32,       // Format version
    pub timestamp: u64,     // Microseconds
    pub wal_offset: u64,    // WAL position covered
    pub payload: Vec<u8>,   // Primitive sections
    pub checksum: u32,      // CRC32
}

/// Read-only view from replay
pub struct ReadOnlyView {
    pub run_id: RunId,
    kv_state: HashMap<Key, Value>,
    json_state: HashMap<Key, JsonDoc>,
    event_state: Vec<Event>,
    state_state: HashMap<Key, StateValue>,
    trace_state: Vec<Span>,
}
```

### WAL Entry Type Registry

```
Core (0x00-0x0F):
  0x00 - TransactionCommit
  0x01 - TransactionAbort
  0x02 - SnapshotMarker

KV (0x10-0x1F):
  0x10 - KvPut
  0x11 - KvDelete

JSON (0x20-0x2F):
  0x20 - JsonCreate
  0x21 - JsonSet
  0x22 - JsonDelete
  0x23 - JsonPatch

Event (0x30-0x3F):
  0x30 - EventAppend

State (0x40-0x4F):
  0x40 - StateInit
  0x41 - StateSet
  0x42 - StateTransition

Trace (0x50-0x5F):
  0x50 - TraceRecord

Run (0x60-0x6F):
  0x60 - RunCreate
  0x61 - RunUpdate
  0x62 - RunEnd
  0x63 - RunBegin

Reserved for Vector (M8): 0x70-0x7F
Reserved for future: 0x80-0xFF
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
   cat docs/milestones/M7/M7_IMPLEMENTATION_PLAN.md
   cat docs/milestones/M7/EPIC_<N>_<NAME>.md
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

M7 uses the following GitHub issue numbers:

| Epic | GitHub Issue | Stories (GitHub Issues) |
|------|--------------|-------------------------|
| Epic 40: Snapshot Format & Writer | [#338](https://github.com/anibjoshi/in-mem/issues/338) | #347-#352 |
| Epic 41: Crash Recovery | [#339](https://github.com/anibjoshi/in-mem/issues/339) | #353-#359 |
| Epic 42: WAL Enhancement | [#340](https://github.com/anibjoshi/in-mem/issues/340) | #360-#364 |
| Epic 43: Run Lifecycle & Replay | [#341](https://github.com/anibjoshi/in-mem/issues/341) | #365-#371 |
| Epic 44: Cross-Primitive Atomicity | [#342](https://github.com/anibjoshi/in-mem/issues/342) | #372-#375 |
| Epic 45: Storage Stabilization | [#343](https://github.com/anibjoshi/in-mem/issues/343) | #376-#380 |
| Epic 46: Validation & Benchmarks | [#344](https://github.com/anibjoshi/in-mem/issues/344) | #381-#384 |

**Story Number Mapping** (Spec # -> GitHub Issue #):
- Story #292 = GitHub Issue #347
- Story #293 = GitHub Issue #348
- Story #294 = GitHub Issue #349
- ... (offset of +55)
- Story #329 = GitHub Issue #384

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

### M7-Specific Validation

```bash
# Run M7 crash recovery tests
~/.cargo/bin/cargo test --test m7_crash_recovery

# Run M7 replay tests
~/.cargo/bin/cargo test --test m7_replay_determinism

# Run M7 benchmarks
~/.cargo/bin/cargo bench --bench m7_recovery_performance

# Verify non-regression (M4/M5/M6 targets maintained)
~/.cargo/bin/cargo bench --bench m6_search_performance
~/.cargo/bin/cargo bench --bench m5_json_performance
~/.cargo/bin/cargo bench --bench m4_performance
```

### Validation Phases

| Phase | Focus | Time |
|-------|-------|------|
| 1 | Automated checks (build, test, clippy, fmt) | 5 min |
| 2 | Story completion verification | 10 min |
| 3 | Spec compliance review (5 rules) | 15 min |
| 4 | Non-regression verification (M4/M5/M6 targets) | 10 min |
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

### Non-Regression (M4/M5/M6 Targets Must Be Maintained)

| Metric | Target | M7 Requirement |
|--------|--------|----------------|
| KV put (InMemory) | < 3µs | No regression |
| KV get (fast path) | < 5µs | No regression |
| JSON create (1KB) | < 1ms | No regression |
| JSON get at path | < 100µs | No regression |
| Event append | < 10µs | No regression |
| State read | < 5µs | No regression |
| Search (indexed) | < 10ms | No regression |

### M7 Performance Targets

| Operation | Scale | Target | Red Flag |
|-----------|-------|--------|----------|
| Snapshot write (100MB) | 100MB | < 5s | > 10s |
| Snapshot load (100MB) | 100MB | < 3s | > 6s |
| WAL replay (10K entries) | 10K | < 1s | > 3s |
| Full recovery | 100MB + 10K | < 5s | > 10s |
| Replay run (1K events) | 1K | < 100ms | > 500ms |
| Diff runs (1K keys) | 1K | < 200ms | > 500ms |

---

## Evolution Warnings

M7 includes several "evolution warnings" for future extensibility:

1. **Snapshot format versioned**: M7 uses v1, future versions will add compression (M9), encryption (M11)
2. **WAL entry types extensible**: 0x70-0x7F reserved for Vector (M8), 0x80-0xFF for future primitives
3. **Replay is interpretation**: Never mutates canonical store; materialization is separate (future)
4. **Index recovery is rebuild**: Indexes rebuilt from data, not snapshotted

---

## Parallelization Strategy

### Phase 1: Foundation (Days 1-2)
- **Claude 1**: Epic 42 (WAL Enhancement) - CRITICAL FOUNDATION

### Phase 2: Core Durability (Days 3-5)
After Epic 42 complete:
- **Claude 1**: Epic 40 (Snapshot Format)
- **Claude 2**: Epic 44 (Cross-Primitive Atomicity)

### Phase 3: Recovery (Days 6-8)
After Epic 40 complete:
- **Claude 1**: Epic 41 (Crash Recovery)

### Phase 4: Replay & Stabilization (Days 9-11)
After Epic 41 complete:
- **Claude 1**: Epic 43 (Run Lifecycle & Replay)
- **Claude 2**: Epic 45 (Storage Stabilization)

### Phase 5: Validation (Days 12-14)
After all implementation:
- **All**: Epic 46 validation stories

---

*End of M7 Prompt Header - Epic-specific content follows below*
