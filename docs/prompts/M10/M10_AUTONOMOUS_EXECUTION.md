# M10 Storage Backend, Retention & Compaction - Autonomous Execution Prompt

**Usage**: `claude --dangerously-skip-permissions -p "$(cat docs/prompts/M10/M10_AUTONOMOUS_EXECUTION.md)"`

---

## Task

Execute M10 Epics 70-76 with phased implementation and epic-end validation after each epic.

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

## M10 Philosophy

> M10 is an **infrastructure milestone**, not a feature milestone. It adds durability and portability without changing substrate semantics.
>
> Storage is infrastructure, not semantics. The user interacts with the same seven primitives through the same API. They do not know (and should not care) whether state lives in memory or on disk.
>
> **Correctness is non-negotiable. Performance can be improved later.** This is Rule 8.

## The Eight Architectural Rules

These rules are NON-NEGOTIABLE. Violating any is a blocking issue.

1. **Storage Is Logically Invisible**: Storage must not change user-visible semantics
2. **Durability Mode Determines Commit Semantics**: Storage must respect durability modes (InMemory, Buffered, Strict)
3. **Recovery Is Deterministic and Idempotent**: Replaying same WAL = identical state. Replay twice = same as once.
4. **Compaction Is Logically Invisible**: Reading retained versions must be unchanged after compaction
5. **Retention Policies Are Database Entries**: Policies stored in `_strata/` system namespace, not MANIFEST
6. **Storage Never Assigns Versions**: Engine assigns versions before persistence. Storage persists faithfully.
7. **WAL Segments Are Immutable Once Closed**: Only active segment is writable
8. **Correctness Over Performance**: Any optimization risking invariants is FORBIDDEN

## Core Invariants

### Storage (S1-S9)
- WAL append-only, segments immutable once closed, records self-delimiting
- Snapshots are consistent point-in-time, logical (not memory dumps)
- MANIFEST atomic updates, codec pass-through
- Storage NEVER assigns versions

### Recovery (R1-R5)
- No committed txn lost (Strict mode), order preserved
- Idempotent replay, snapshot-WAL equivalence
- Partial record truncation handled

### Retention (RT1-RT4)
- Version ordering preserved, no silent fallback
- Explicit HistoryTrimmed error, policy is versioned

### Compaction (C1-C5)
- Read equivalence, no semantic change, no reordering
- Safe boundaries (below watermark only), version identity preserved

## Execution Pattern

For each epic in the recommended order:

1. **Read specs first**:
   - `docs/architecture/M10_ARCHITECTURE.md` (AUTHORITATIVE)
   - `docs/milestones/M10/M10_IMPLEMENTATION_PLAN.md`
   - `docs/milestones/M10/EPIC_{N}_*.md`

2. **Start epic branch**: `./scripts/start-story.sh {epic} {first-story} {desc}`

3. **Implement all stories** per epic specification

4. **Run epic-end validation**: `docs/prompts/EPIC_END_VALIDATION.md` Phase 6f (M10-specific)

5. **Merge to develop**:
   ```bash
   git checkout develop
   git merge --no-ff epic-{N}-* -m "Epic {N}: {Name} complete"
   git push origin develop
   ```

6. **Proceed to next epic**

## Recommended Execution Order

M10 uses a **phased approach**. Each phase produces a testable, usable increment.

### Phase 1: WAL Foundation (Epic 70)

**Epic**: 70 (WAL Infrastructure) - CRITICAL FOUNDATION

1. **Epic 70: WAL Infrastructure** - Start here
   - Start: `./scripts/start-story.sh 70 499 wal-segment-format`
   - Stories #499-#505: WAL segment format, record structure, append with durability modes, rotation, writeset, config, codec seam

2. **Run epic-end validation**

**Exit Criteria**: Commits written to WAL. WAL readable. Durability modes respected.

### Phase 2: Snapshot + Recovery (Epics 71, 72)

**Epics**: 71 (Snapshot System) + 72 (Recovery)

1. **Epic 71: Snapshot System** - depends on Epic 70
   - Start: `./scripts/start-story.sh 71 507 snapshot-file-format`
   - Stories #507-#512: Snapshot format, serialization, crash-safe creation, checkpoint API, metadata, loading

2. **Epic 72: Recovery** - depends on Epics 70, 71
   - Start: `./scripts/start-story.sh 72 514 manifest-structure`
   - Stories #514-#518: MANIFEST structure, WAL replay, recovery algorithm, partial truncation, verification tests

3. **Run epic-end validation for both epics**

**Exit Criteria**: Database can checkpoint, crash, and recover to correct state.

### Phase 3: Database Lifecycle (Epic 75)

**Epic**: 75 (Database Lifecycle)

1. **Epic 75: Database Lifecycle** - depends on Epics 70, 71, 72
   - Start: `./scripts/start-story.sh 75 533 directory-structure`
   - Stories #533-#538: Directory structure, open, close, config, export, import

2. **Run epic-end validation**

**Exit Criteria**: Can create, open, close, copy databases. Full round-trip works.

### Phase 4: Crash Harness (Epic 76)

**Epic**: 76 (Crash Harness) - CRITICAL for validation

1. **Epic 76: Crash Harness** - depends on Epics 72, 75
   - Start: `./scripts/start-story.sh 76 540 crash-harness-framework`
   - Stories #540-#544: Framework, random kill tests, tail corruption tests, reference model, scenario matrix

2. **Run epic-end validation**

**Exit Criteria**: Storage correctness validated under systematic crash scenarios.

### Phase 5: Retention + Compaction (Epics 73, 74)

**Epics**: 73 (Retention Policies) + 74 (Compaction)

1. **Epic 73: Retention Policies** - depends on Epic 72
   - Start: `./scripts/start-story.sh 73 520 retention-policy-type`
   - Stories #520-#524: Policy type, system namespace, CRUD API, enforcement, HistoryTrimmed error

2. **Epic 74: Compaction** - depends on Epics 71, 73
   - Start: `./scripts/start-story.sh 74 526 compact-mode-enum`
   - Stories #526-#531: CompactMode, WAL-only, Full, tombstones, correctness verification, API

3. **Run epic-end validation for both epics**

**Exit Criteria**: Retention policies control data lifetime. Compaction reclaims space correctly.

## GitHub Issue Mapping

| Epic | GitHub Issue | Story Issues | Phase |
|------|--------------|--------------|-------|
| Epic 70: WAL Infrastructure | #498 | #499-#505 | 1 |
| Epic 71: Snapshot System | #506 | #507-#512 | 2 |
| Epic 72: Recovery | #513 | #514-#518 | 2 |
| Epic 75: Database Lifecycle | #532 | #533-#538 | 3 |
| Epic 76: Crash Harness | #539 | #540-#544 | 4 |
| Epic 73: Retention Policies | #519 | #520-#524 | 5 |
| Epic 74: Compaction | #525 | #526-#531 | 5 |

## Stop Conditions

- Any architectural rule violation (8 rules)
- Any invariant violation (S1-S9, R1-R5, RT1-RT4, C1-C5)
- Epic-end validation failure
- Test failures that can't be resolved
- Data loss in Strict mode during crash testing
- Performance regression > 10% in existing operations
- Crash harness reveals unrecoverable state

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

### M10-Specific Validation

```bash
# Run storage tests
~/.cargo/bin/cargo test --package in-mem-storage

# Run storage invariant tests
~/.cargo/bin/cargo test storage_invariant_
~/.cargo/bin/cargo test recovery_invariant_
~/.cargo/bin/cargo test retention_invariant_
~/.cargo/bin/cargo test compaction_invariant_

# Run crash harness tests (Phase 4+)
~/.cargo/bin/cargo test --test crash_harness

# Verify Rule 8 (correctness over performance)
~/.cargo/bin/cargo test correctness_

# Verify non-regression (M7/M8/M9 targets maintained)
~/.cargo/bin/cargo bench --bench m8_vector_performance
~/.cargo/bin/cargo test --test m4_red_flags
```

### Eight Rules Quick Check

```bash
# Rule 1: Storage is logically invisible (no storage-specific public API)
echo "Rule 1: Storage logically invisible"
grep -r "pub fn.*wal_path\|pub fn.*storage_path" crates/primitives/src/ && echo "FAIL: Storage leaks to API" || echo "PASS"

# Rule 2: Durability mode respected
echo "Rule 2: Durability mode determines commit"
grep -r "match.*DurabilityMode" crates/storage/src/ && echo "PASS: Mode checked" || echo "CHECK"

# Rule 3: Recovery is idempotent
echo "Rule 3: Idempotent replay"
grep -r "apply_versioned\|version.*from.*record" crates/storage/src/recovery/ && echo "PASS: Version from record" || echo "CHECK"

# Rule 4: Compaction logically invisible
echo "Rule 4: Compaction invisible"
grep -r "test_compaction_read_equivalence\|before.*after.*compaction" crates/storage/src/ && echo "PASS: Equivalence tested" || echo "CHECK"

# Rule 5: Retention in database entries
echo "Rule 5: Retention as database entries"
grep -r "_strata/retention_policy" crates/storage/src/ && echo "PASS: System namespace" || echo "FAIL"

# Rule 6: Storage never assigns versions
echo "Rule 6: Storage never assigns versions"
grep -r "next_txn_id\|generate_version" crates/storage/src/ && echo "FAIL: Storage generating versions" || echo "PASS"

# Rule 7: Closed segments immutable
echo "Rule 7: Closed segments immutable"
grep -r "fn update_record\|fn modify_segment" crates/storage/src/ && echo "FAIL: Mutable segments" || echo "PASS"

# Rule 8: Correctness over performance
echo "Rule 8: Correctness over performance"
grep -r "batch_buffer.push\|skip_fsync\|optimization_hack" crates/storage/src/ && echo "FAIL: Risky optimization" || echo "PASS"
```

### Invariant Quick Check

```bash
# Storage invariants
echo "Storage Invariants S1-S9"
~/.cargo/bin/cargo test storage_invariant_ -- --nocapture

# Recovery invariants
echo "Recovery Invariants R1-R5"
~/.cargo/bin/cargo test recovery_invariant_ -- --nocapture

# Retention invariants (Phase 5)
echo "Retention Invariants RT1-RT4"
~/.cargo/bin/cargo test retention_invariant_ -- --nocapture

# Compaction invariants (Phase 5)
echo "Compaction Invariants C1-C5"
~/.cargo/bin/cargo test compaction_invariant_ -- --nocapture
```

## Crash Harness Validation (Phase 4)

The crash harness is critical for validating storage correctness:

```bash
# Run crash scenarios
~/.cargo/bin/cargo test --test crash_harness -- --nocapture

# Verify scenarios:
# - Crash during WAL append (various points)
# - Crash during segment rotation
# - Crash during snapshot creation
# - Crash during MANIFEST update
# - Crash during compaction
# - Multiple consecutive crashes

# All must pass: recovered state matches reference or graceful error
```

## Files to Create

### New Crate Structure

```
crates/storage/
├── Cargo.toml
└── src/
    ├── lib.rs
    ├── format/           # On-disk byte formats
    │   ├── mod.rs
    │   ├── wal_record.rs
    │   ├── snapshot.rs
    │   ├── manifest.rs
    │   ├── writeset.rs
    │   └── primitives.rs
    ├── wal/              # WAL operations
    │   ├── mod.rs
    │   ├── segment.rs
    │   ├── writer.rs
    │   ├── reader.rs
    │   └── config.rs
    ├── snapshot/         # Snapshot operations
    │   ├── mod.rs
    │   ├── writer.rs
    │   └── reader.rs
    ├── recovery/         # Recovery operations
    │   ├── mod.rs
    │   ├── manifest.rs
    │   └── replay.rs
    ├── retention/        # Retention policies
    │   ├── mod.rs
    │   ├── policy.rs
    │   └── enforcement.rs
    ├── compaction/       # Compaction
    │   ├── mod.rs
    │   ├── wal_only.rs
    │   └── full.rs
    ├── codec/            # Codec seam
    │   ├── mod.rs
    │   ├── identity.rs
    │   └── trait.rs
    ├── database.rs       # Database lifecycle
    └── error.rs          # Storage errors
```

## Common Patterns

### WAL Record Format

```rust
// Record structure
pub struct WalRecord {
    pub length: u32,          // Record length (for seeking)
    pub format_version: u8,   // For forward compatibility
    pub txn_id: u64,          // Transaction ID (assigned by engine)
    pub run_id: RunId,        // Run scope
    pub commit_timestamp: u64,// Commit time
    pub writeset: Writeset,   // Mutations
    pub checksum: u32,        // CRC32
}
```

### Crash-Safe Write Pattern

```rust
// Write temp → fsync → rename → update MANIFEST
fn crash_safe_snapshot(&self, snapshot: &Snapshot) -> Result<()> {
    let temp_path = self.snapshot_dir.join(format!("snap-{}.tmp", snapshot.id));
    let final_path = self.snapshot_dir.join(format!("snap-{:06}.chk", snapshot.id));

    // 1. Write to temp file
    let mut file = File::create(&temp_path)?;
    file.write_all(&snapshot.serialize()?)?;

    // 2. fsync temp file
    file.sync_all()?;

    // 3. Atomic rename
    std::fs::rename(&temp_path, &final_path)?;

    // 4. fsync directory
    let dir = File::open(&self.snapshot_dir)?;
    dir.sync_all()?;

    // 5. Update MANIFEST (also crash-safe)
    self.update_manifest(|m| m.snapshot_id = Some(snapshot.id))?;

    Ok(())
}
```

### Idempotent Replay Pattern

```rust
// Replay uses version FROM record, never generates
fn replay_record(&mut self, record: &WalRecord) -> Result<()> {
    for mutation in &record.writeset.mutations {
        match mutation {
            Mutation::Put { entity_ref, value, version } => {
                // CRITICAL: Use version from record, not self.next_version()
                self.store.apply_versioned(entity_ref, value.clone(), *version)?;
            }
            Mutation::Delete { entity_ref } => {
                self.store.mark_deleted(entity_ref)?;
            }
            Mutation::Append { entity_ref, value, version } => {
                self.store.apply_append(entity_ref, value.clone(), *version)?;
            }
        }
    }
    Ok(())
}
```

## Troubleshooting

### "Data lost after crash in Strict mode"

This is a BLOCKING BUG. In Strict mode, no committed transaction may be lost.

1. Verify `sync_all()` is called before returning from commit
2. Verify MANIFEST is updated atomically
3. Check crash harness for the specific failure point

### "Compaction changed version IDs"

This violates Rule 4 (Compaction logically invisible).

1. Never renumber versions during compaction
2. Only remove data below watermark
3. Test with `test_compaction_read_equivalence`

### "Recovery produces different state"

This violates Rule 3 (Recovery is deterministic and idempotent).

1. Verify replay uses version FROM WAL record
2. Verify no auto-increment counters in storage
3. Test with `test_snapshot_wal_equivalence`

### "Retention silently returns nearest version"

This violates RT2 (No silent fallback).

1. Return explicit `HistoryTrimmed` error
2. Include `requested` and `earliest_retained` in error
3. Test with `test_trimmed_version_error`

## Start

Begin with Phase 1: Epic 70 (WAL Infrastructure).

Read the specs:
1. `docs/architecture/M10_ARCHITECTURE.md`
2. `docs/milestones/M10/M10_IMPLEMENTATION_PLAN.md`
3. `docs/milestones/M10/EPIC_70_WAL_INFRASTRUCTURE.md`

Then start with Story #499 (WAL Segment File Format):
```bash
./scripts/start-story.sh 70 499 wal-segment-format
```

Remember:
- **Rule 8: Correctness Over Performance** - Do not optimize until correctness is proven
- **Crash Harness (Phase 4)** validates correctness before adding retention/compaction complexity
- **format/ module** keeps serialization separate from operational logic
- All epic-end validation uses `docs/prompts/EPIC_END_VALIDATION.md`

---

*End of M10 Autonomous Execution Prompt*
