# M10 Epic Prompt Header

**Copy this header to the top of every M10 epic prompt file (Epics 70-76).**

---

## NAMING CONVENTION - CRITICAL

> **NEVER use "M10" or "Strata" in the actual codebase or comments.**
>
> - "M10" is an internal milestone tracker only - do not use it in code, comments, or user-facing text
> - All existing crates refer to the database as "in-mem" - use this name consistently
> - Do not use "Strata" anywhere in the codebase
> - This applies to: code, comments, docstrings, error messages, log messages, test names
>
> **CORRECT**: `//! Write-ahead log segment file handling`
> **WRONG**: `//! M10 WAL segment for Strata database`

---

## AUTHORITATIVE SPECIFICATIONS - READ THESE FIRST

**The following documents are GOSPEL for ALL M10 implementation:**

1. **`docs/architecture/M10_ARCHITECTURE.md`** - THE AUTHORITATIVE SPECIFICATION
2. **`docs/architecture/PRIMITIVE_CONTRACT.md`** - The seven invariants (M9)
3. `docs/milestones/M10/M10_IMPLEMENTATION_PLAN.md` - Epic/Story breakdown and implementation details
4. `docs/milestones/M10/EPIC_70_WAL_INFRASTRUCTURE.md` through `EPIC_76_CRASH_HARNESS.md` - Story-level specifications

**The architecture spec is LAW.** The implementation plan and epic docs provide execution details but MUST NOT contradict the architecture spec.

This is not a guideline. This is not a suggestion. This is the **LAW**.

### Rules for Every Story in Every Epic of M10:

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

## THE EIGHT ARCHITECTURAL RULES (NON-NEGOTIABLE)

**These rules MUST be followed in EVERY M10 story. Violating any of these is a blocking issue.**

### Rule 1: Storage Is Logically Invisible

> **The storage layer must not change user-visible semantics.**

The seven primitives behave identically before and after M10. Storage is infrastructure, not semantics.

```rust
// CORRECT: Storage integrated, API unchanged
pub fn put(&self, run_id: &RunId, key: &str, value: Value) -> Result<Version> {
    let version = self.engine.put(run_id, key, value)?;
    // Storage happens internally, not visible here
    Ok(version)
}

// WRONG: Storage details leak to users
pub fn put(&self, run_id: &RunId, key: &str, value: Value, wal_path: &Path) -> Result<Version> {
    // NEVER expose storage paths or details to API
}
```

### Rule 2: Durability Mode Determines Commit Semantics

> **Transaction commit semantics depend on durability mode. Storage must respect this.**

```rust
// CORRECT: WAL respects durability mode
pub fn append(&mut self, record: WalRecord, mode: DurabilityMode) -> Result<()> {
    match mode {
        DurabilityMode::Strict => {
            self.file.write_all(&record.serialize()?)?;
            self.file.sync_all()?;  // fsync before returning
        }
        DurabilityMode::Buffered => {
            self.file.write_all(&record.serialize()?)?;
            // fsync on coarse boundary
        }
        DurabilityMode::InMemory => {
            // No WAL writes
        }
    }
    Ok(())
}

// WRONG: Ignore durability mode
pub fn append(&mut self, record: WalRecord) -> Result<()> {
    self.file.write_all(&record.serialize()?)?;
    self.file.sync_all()?;  // Always fsync - WRONG
    Ok(())
}
```

### Rule 3: Recovery Is Deterministic and Idempotent

> **Replaying the same WAL produces identical state. Replaying a record twice produces the same result as once.**

```rust
// CORRECT: Idempotent replay (version from record, not generated)
fn replay_record(&mut self, record: &WalRecord) -> Result<()> {
    for mutation in &record.writeset.mutations {
        match mutation {
            Mutation::Put { entity_ref, value, version } => {
                // Use version FROM the record, don't generate new one
                self.store.apply_versioned(entity_ref, value, *version)?;
            }
        }
    }
    Ok(())
}

// WRONG: Non-idempotent replay (generates new version)
fn replay_record(&mut self, record: &WalRecord) -> Result<()> {
    for mutation in &record.writeset.mutations {
        match mutation {
            Mutation::Put { entity_ref, value, .. } => {
                // Generates new version each replay - WRONG
                self.store.put(entity_ref, value)?;
            }
        }
    }
    Ok(())
}
```

### Rule 4: Compaction Is Logically Invisible

> **Compaction must not change the result of reading any retained version.**

```rust
// CORRECT: Verify read equivalence before/after compaction
#[test]
fn test_compaction_read_equivalence() {
    let before = db.kv_get(run_id, "key", Version::TxnId(5))?;
    db.compact(CompactMode::Full)?;
    let after = db.kv_get(run_id, "key", Version::TxnId(5))?;
    assert_eq!(before, after);  // Must be identical
}

// WRONG: Compaction changes version IDs
fn compact(&mut self) -> Result<()> {
    // NEVER renumber versions during compaction
}
```

### Rule 5: Retention Policies Are Database Entries

> **Retention policies are stored as first-class database entries in the system namespace.**

```rust
// CORRECT: Policies stored in system namespace
const RETENTION_POLICY_KEY: &str = "_strata/retention_policy";

pub fn set_retention_policy(&self, run_id: &RunId, policy: RetentionPolicy) -> Result<Version> {
    let key = format!("{}/{}", RETENTION_POLICY_KEY, run_id);
    self.kv.put(&SYSTEM_RUN_ID, &key, policy.to_value()?)
}

// WRONG: Policies in MANIFEST or config file
// NEVER store policies outside the database entry system
```

### Rule 6: Storage Never Assigns Versions

> **Versions are assigned by the engine before persistence. Storage persists and replays faithfully.**

```rust
// CORRECT: Storage receives version, stores it
pub fn write_record(&mut self, txn_id: u64, writeset: Writeset) -> Result<()> {
    // txn_id and versions already assigned by engine
    let record = WalRecord { txn_id, writeset, .. };
    self.wal.append(record)
}

// WRONG: Storage generates version
pub fn write_record(&mut self, writeset: Writeset) -> Result<u64> {
    let txn_id = self.next_txn_id();  // NEVER generate versions in storage
}
```

### Rule 7: WAL Segments Are Immutable Once Closed

> **Only the active segment is writable. Closed segments never change.**

```rust
// CORRECT: Append only to active segment
pub fn append(&mut self, record: WalRecord) -> Result<()> {
    if self.should_rotate() {
        self.close_active_segment()?;  // Segment becomes immutable
        self.create_new_segment()?;
    }
    self.active_segment.append(record)
}

// WRONG: Modify closed segments
pub fn update_record(&mut self, segment_id: u64, offset: u64, record: WalRecord) -> Result<()> {
    // NEVER modify closed segments
}
```

### Rule 8: Correctness Over Performance (CRITICAL)

> **Any optimization that risks violating invariants is forbidden in M10.**

Correctness is non-negotiable. Performance can be improved in future milestones.

```rust
// CORRECT: Simple, correct implementation
pub fn commit(&mut self, mode: DurabilityMode) -> Result<Version> {
    let record = self.build_wal_record()?;
    if mode == DurabilityMode::Strict {
        self.wal.append_sync(record)?;  // Sync write
    }
    Ok(version)
}

// WRONG: Optimization that risks data loss
pub fn commit(&mut self, mode: DurabilityMode) -> Result<Version> {
    let record = self.build_wal_record()?;
    // Batch WAL writes to improve throughput
    self.batch_buffer.push(record);  // May lose commits on crash
    if self.batch_buffer.len() > 100 {
        self.wal.append_batch(&self.batch_buffer)?;
    }
    Ok(version)
}
```

**Rationale**: Storage bugs are catastrophic. A slow but correct storage layer can be optimized. A fast but incorrect storage layer destroys user trust and data.

---

## CORE INVARIANTS

### Storage Invariants

| # | Invariant | Test Strategy |
|---|-----------|---------------|
| S1 | WAL is append-only | Verify file size only grows, checksum stability |
| S2 | WAL segments immutable once closed | Verify closed segment checksum before/after operations |
| S3 | WAL records are self-delimiting | Parse records independently, verify boundaries |
| S4 | Snapshots are consistent | Concurrent writes during snapshot, verify point-in-time |
| S5 | Snapshots are logical | Compare snapshot content to expected logical state |
| S6 | Watermark ordering | Verify all WAL records after snapshot have higher txn_id |
| S7 | MANIFEST atomicity | Crash during MANIFEST update, verify recovery |
| S8 | Codec pass-through | Verify all bytes go through codec encode/decode |
| S9 | Storage never assigns versions | Verify WAL versions match engine-assigned versions exactly |

### Recovery Invariants

| # | Invariant | Test Strategy |
|---|-----------|---------------|
| R1 | No committed txn lost | Commit in Strict mode, crash, recover, verify data |
| R2 | Order preservation | Replay multiple transactions, verify order |
| R3 | Idempotent replay | Replay same record multiple times, verify same state |
| R4 | Snapshot-WAL equivalence | Compare pure WAL replay vs snapshot + WAL replay |
| R5 | Partial record truncation | Write partial record, verify truncation on recovery |

### Retention Invariants

| # | Invariant | Test Strategy |
|---|-----------|---------------|
| RT1 | Version ordering preserved | Apply retention, verify remaining versions in order |
| RT2 | No silent fallback | Request trimmed version, verify explicit error |
| RT3 | Explicit unavailability | Verify HistoryTrimmed error with metadata |
| RT4 | Policy is versioned | Update policy, verify version change |

### Compaction Invariants

| # | Invariant | Test Strategy |
|---|-----------|---------------|
| C1 | Read equivalence | Compare reads before/after compaction |
| C2 | No semantic change | Transaction behavior identical after compaction |
| C3 | No reordering | Verify history order preserved |
| C4 | Safe boundaries | Verify compaction only removes data below watermark |
| C5 | Version identity | Verify version IDs unchanged after compaction |

---

## PHASED IMPLEMENTATION STRATEGY (NON-NEGOTIABLE)

> **Build the foundation first. WAL must work before snapshots. Recovery must work before retention.**

M10 uses a phased approach where each phase produces a testable, usable increment:

### Phase 1: WAL Foundation (Epic 70)

- WAL segment format and record structure
- Append with durability modes
- Segment rotation
- Codec seam (identity codec)

**Exit Criteria**: Commits are written to WAL. WAL can be read back. Durability modes respected.

### Phase 2: Snapshot + Recovery (Epics 71, 72)

- Snapshot serialization for all 7 primitives
- Crash-safe snapshot creation
- MANIFEST structure
- Recovery: snapshot + WAL replay

**Exit Criteria**: Database can checkpoint, crash, and recover to correct state.

### Phase 3: Database Lifecycle (Epic 75)

- Directory structure creation
- Database open (new and existing)
- Database close with proper cleanup
- Export/import convenience APIs

**Exit Criteria**: Can create, open, close, copy databases. Full round-trip works.

### Phase 4: Crash Harness (Epic 76)

- Crash injection framework
- Random kill tests
- Tail corruption tests
- Reference model comparator

**Exit Criteria**: Storage correctness validated under systematic crash scenarios.

### Phase 5: Retention + Compaction (Epics 73, 74)

- RetentionPolicy types and storage
- System namespace for policies
- WAL-only compaction
- Full compaction with retention

**Exit Criteria**: Retention policies control data lifetime. Compaction reclaims space correctly.

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
   gh pr create --base epic-70-wal-infrastructure --head epic-70-story-499-wal-segment-format

   # WRONG: Never PR directly to main
   gh pr create --base main --head epic-70-story-499-wal-segment-format  # NEVER DO THIS
   ```

2. **Epic branches merge to develop** (after all stories complete)
   ```bash
   git checkout develop
   git merge --no-ff epic-70-wal-infrastructure
   ```

3. **develop merges to main** (at milestone boundaries)
   ```bash
   git checkout main
   git merge --no-ff develop -m "M10: Complete"
   ```

4. **main is protected** - requires PR, no direct pushes

### The `complete-story.sh` Script
The script automatically uses the correct base branch:
```bash
./scripts/complete-story.sh 499  # Creates PR to epic-70-wal-infrastructure
```

**If you manually create a PR, ALWAYS verify the base branch is the epic branch, not main.**

---

## M10 CORE CONCEPTS

### What M10 Is About

M10 is an **infrastructure milestone**. It adds:

| Aspect | M10 Commits To |
|--------|----------------|
| **WAL** | Append-only, segmented, durability modes |
| **Snapshot** | Point-in-time materialized logical state |
| **Recovery** | Snapshot + WAL replay, deterministic/idempotent |
| **Retention** | User-configurable policies (KeepAll, KeepLast, KeepFor) |
| **Compaction** | WAL-only and Full modes, logically invisible |
| **Database Lifecycle** | open/close, export/import, portability |
| **Crash Harness** | Systematic crash testing framework |

### What M10 Is NOT

M10 is **not** a "disk-first" database conversion. The engine remains the source of truth for semantics.

| Deferred Item | Target |
|---------------|--------|
| Encryption implementation | Post-MVP (codec seam ready) |
| Background compaction | Post-MVP |
| Incremental snapshots | Post-MVP |
| Multi-node replication | Post-MVP |
| Automatic checkpointing | Post-MVP |
| WAL compression | Post-MVP |

### Key Design Decisions

| Decision | Rationale |
|----------|-----------|
| format/ module | Separates serialization from operational logic |
| Identity codec first | Correctness before complexity |
| User-triggered compaction | Predictability over automation |
| System namespace for policies | Policies are versioned database entries |
| Crash harness as epic | Storage correctness requires systematic testing |

### Core Types

```rust
/// WAL record structure
pub struct WalRecord {
    pub length: u32,
    pub format_version: u8,
    pub txn_id: u64,
    pub run_id: RunId,
    pub commit_timestamp: u64,
    pub writeset: Writeset,
    pub checksum: u32,  // CRC32
}

/// Writeset contains mutations
pub struct Writeset {
    pub mutations: Vec<Mutation>,
}

pub enum Mutation {
    Put { entity_ref: EntityRef, value: Vec<u8>, version: Version },
    Delete { entity_ref: EntityRef },
    Append { entity_ref: EntityRef, value: Vec<u8>, version: Version },
}

/// Retention policy types
pub enum RetentionPolicy {
    KeepAll,
    KeepLast(u64),
    KeepFor(Duration),
    Composite(Vec<RetentionPolicy>),
}

/// Compaction modes
pub enum CompactMode {
    WALOnly,  // Remove WAL segments covered by snapshot
    Full,     // WAL + retention enforcement
}

/// Compaction result
pub struct CompactInfo {
    pub reclaimed_bytes: u64,
    pub wal_segments_removed: u32,
    pub versions_removed: u64,
}

/// MANIFEST structure
pub struct Manifest {
    pub format_version: u32,
    pub database_uuid: Uuid,
    pub codec_id: String,
    pub active_wal_segment: u64,
    pub snapshot_watermark: Option<u64>,
    pub snapshot_id: Option<u64>,
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
   cat docs/milestones/M10/M10_IMPLEMENTATION_PLAN.md
   cat docs/milestones/M10/EPIC_<N>_*.md
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

M10 uses the following GitHub issue numbers:

| Epic | GitHub Issue | Stories (GitHub Issues) |
|------|--------------|-------------------------|
| Epic 70: WAL Infrastructure | [#498](https://github.com/anibjoshi/in-mem/issues/498) | #499-#505 |
| Epic 71: Snapshot System | [#506](https://github.com/anibjoshi/in-mem/issues/506) | #507-#512 |
| Epic 72: Recovery | [#513](https://github.com/anibjoshi/in-mem/issues/513) | #514-#518 |
| Epic 73: Retention Policies | [#519](https://github.com/anibjoshi/in-mem/issues/519) | #520-#524 |
| Epic 74: Compaction | [#525](https://github.com/anibjoshi/in-mem/issues/525) | #526-#531 |
| Epic 75: Database Lifecycle | [#532](https://github.com/anibjoshi/in-mem/issues/532) | #533-#538 |
| Epic 76: Crash Harness | [#539](https://github.com/anibjoshi/in-mem/issues/539) | #540-#544 |

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

### M10-Specific Validation

```bash
# Run storage tests
~/.cargo/bin/cargo test --package in-mem-storage

# Run invariant tests
~/.cargo/bin/cargo test storage_invariant_
~/.cargo/bin/cargo test recovery_invariant_
~/.cargo/bin/cargo test retention_invariant_
~/.cargo/bin/cargo test compaction_invariant_

# Run crash harness tests
~/.cargo/bin/cargo test --test crash_harness

# Verify correctness (Rule 8)
~/.cargo/bin/cargo test correctness_

# Verify non-regression (M7/M8/M9 targets maintained)
~/.cargo/bin/cargo bench --bench m8_vector_performance
~/.cargo/bin/cargo bench --bench m9_api_performance
```

### Validation Phases

| Phase | Focus | Time |
|-------|-------|------|
| 1 | Automated checks (build, test, clippy, fmt) | 5 min |
| 2 | Story completion verification | 10 min |
| 3 | Spec compliance review (8 rules, invariants) | 15 min |
| 4 | Non-regression verification (M7/M8/M9 targets) | 10 min |
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

### Non-Regression (M7/M8/M9 Targets Must Be Maintained)

| Metric | Target | M10 Requirement |
|--------|--------|-----------------|
| KV put (InMemory) | < 3µs | No regression |
| KV get (fast path) | < 5µs | No regression |
| Vector upsert | < 100µs | No regression |
| Vector search (k=10) | < 10ms | No regression |
| Snapshot write (100MB) | < 5s | New target |
| Recovery (100MB + 10K WAL) | < 5s | New target |

### M10 Expectations

M10 adds storage overhead but must not regress existing operations:

| Operation | Expectation |
|-----------|-------------|
| InMemory mode | No storage overhead (no WAL writes) |
| Buffered mode | < 30µs additional latency |
| Strict mode | ~2ms additional latency (fsync) |
| Snapshot creation | < 1s for typical workloads |
| Recovery | < 5s for 100MB + 10K WAL records |

---

## Directory Structure

```
strata.db/
├── MANIFEST                    # Physical metadata
├── WAL/
│   ├── wal-000001.seg         # WAL segment files
│   ├── wal-000002.seg
│   └── ...
├── SNAPSHOTS/
│   ├── snap-000010.chk        # Snapshot checkpoint files
│   └── ...
└── DATA/                       # Optional: materialized data segments
    └── ...
```

---

## The format/ Module

> **Design Note**: The `format/` module centralizes all on-disk byte formats. This separation keeps serialization logic (how bytes are laid out) separate from operational logic (how WAL/snapshots/MANIFEST are managed).

```
crates/storage/src/format/
├── mod.rs              # Format module entry point
├── wal_record.rs       # WAL record binary format
├── snapshot.rs         # Snapshot binary format
├── manifest.rs         # MANIFEST binary format
├── writeset.rs         # Writeset binary format
└── primitives.rs       # Primitive serialization formats
```

This pattern:
- Prevents business logic from creeping into serialization code
- Makes format evolution easier to manage
- Enables format versioning and backward compatibility

---

*End of M10 Prompt Header - Epic-specific content follows below*
