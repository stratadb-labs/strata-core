# M7 Implementation Plan: Durability, Snapshots, Replay & Storage Stabilization

## Overview

This document provides the high-level implementation plan for M7 (Durability, Snapshots, Replay & Storage Stabilization).

**Total Scope**: 7 Epics, 38 Stories

**References**:
- [M7 Architecture Specification](../../architecture/M7_ARCHITECTURE.md) - Authoritative spec
- [M7 Scope](../../architecture/M7_SCOPE.md) - Sealed design decisions

**Critical Framing**:
> M7 is not about features. M7 is about truth.
> After crash recovery, the database must correspond to a **prefix of the committed transaction history**.
> No partial transactions may be visible. Either all effects or none.

**Epic Details**:
- [Epic 40: Snapshot Format & Writer](./EPIC_40_SNAPSHOT_FORMAT.md)
- [Epic 41: Crash Recovery](./EPIC_41_CRASH_RECOVERY.md)
- [Epic 42: WAL Enhancement](./EPIC_42_WAL_ENHANCEMENT.md)
- [Epic 43: Run Lifecycle & Replay](./EPIC_43_RUN_REPLAY.md)
- [Epic 44: Cross-Primitive Atomicity](./EPIC_44_CROSS_PRIMITIVE.md)
- [Epic 45: Storage Stabilization](./EPIC_45_STORAGE_STABILIZATION.md)
- [Epic 46: Validation & Benchmarks](./EPIC_46_VALIDATION.md)

---

## Architectural Integration Rules (NON-NEGOTIABLE)

These rules ensure M7 integrates properly with the M1-M6 architecture.

### Rule 1: Recovery Is Deterministic

Same WAL + Snapshot = Same state. Always. No randomness, no non-deterministic behavior.

**FORBIDDEN**: Any use of random values, timestamps, or external state during recovery.

### Rule 2: Recovery Is Prefix-Consistent

After recovery, you see a prefix of the committed transaction history. No partial transactions visible.

**FORBIDDEN**: Applying WAL entries without commit markers.

### Rule 3: Replay Is Side-Effect Free

Replay produces a derived view. It does NOT mutate the canonical store.

**FORBIDDEN**: Writing to canonical storage during replay.

### Rule 4: Snapshots Are Physical, Not Semantic

Snapshots compress WAL effects. They are a cache over history, not the history itself.

**FORBIDDEN**: Storing semantic history (like EventLog) in snapshots.

### Rule 5: Storage APIs Stable After M7

Adding a primitive must NOT require changes to WAL core format, Snapshot core format, Recovery engine, or Replay engine. Only extension points.

**FORBIDDEN**: Hardcoded primitive lists in recovery/snapshot code.

---

## Core Invariants

### Recovery Invariants

| # | Invariant | Test Strategy |
|---|-----------|---------------|
| R1 | Deterministic | Same inputs = same outputs, property test |
| R2 | Idempotent | Recover twice, compare states |
| R3 | Prefix-consistent | Crash simulation tests |
| R4 | Never invents data | Verify no unexpected keys after recovery |
| R5 | Never drops committed | Verify all committed data survives |
| R6 | May drop uncommitted | Verify uncommitted transactions not visible |

### Replay Invariants

| # | Invariant | Test Strategy |
|---|-----------|---------------|
| P1 | Pure function | Same inputs = same view |
| P2 | Side-effect free | Verify canonical store unchanged |
| P3 | Derived view | View is read-only |
| P4 | Does not persist | No WAL/snapshot writes |
| P5 | Deterministic | Same run_id = same view |
| P6 | Idempotent | Replay twice = identical view |

---

## Epic Overview

| Epic | Name | Stories | Dependencies |
|------|------|---------|--------------|
| 40 | Snapshot Format & Writer | 6 | M6 complete |
| 41 | Crash Recovery | 7 | Epic 40 |
| 42 | WAL Enhancement | 5 | M6 complete |
| 43 | Run Lifecycle & Replay | 7 | Epic 41, 42 |
| 44 | Cross-Primitive Atomicity | 4 | Epic 42 |
| 45 | Storage Stabilization | 5 | Epic 41, 42 |
| 46 | Validation & Benchmarks | 4 | All others |

---

## Epic 40: Snapshot Format & Writer

**Goal**: Implement snapshot format and writing with checksums

| Story | Description | Priority |
|-------|-------------|----------|
| #292 | Snapshot Envelope Format | FOUNDATION |
| #293 | SnapshotHeader Type | FOUNDATION |
| #294 | Per-Primitive Serialization | CRITICAL |
| #295 | SnapshotWriter Implementation | CRITICAL |
| #296 | CRC32 Checksum Integration | CRITICAL |
| #297 | Atomic Snapshot Write | HIGH |

**Acceptance Criteria**:
- [ ] Snapshot envelope: magic, version, timestamp, wal_offset, payload, crc32
- [ ] SnapshotHeader with all metadata fields
- [ ] Each primitive has serialize() for snapshots
- [ ] SnapshotWriter writes atomically (temp file + rename)
- [ ] CRC32 checksum validates entire snapshot
- [ ] No derived data (indexes) in snapshot

---

## Epic 41: Crash Recovery

**Goal**: Implement crash recovery from snapshot + WAL

| Story | Description | Priority |
|-------|-------------|----------|
| #298 | SnapshotReader with Validation | CRITICAL |
| #299 | Snapshot Discovery (Find Latest Valid) | CRITICAL |
| #300 | Recovery Sequence Implementation | CRITICAL |
| #301 | WAL Replay from Offset | CRITICAL |
| #302 | Corrupt Entry Handling | HIGH |
| #303 | Fallback to Older Snapshot | HIGH |
| #304 | RecoveryResult and RecoveryOptions | HIGH |

**Acceptance Criteria**:
- [ ] SnapshotReader validates checksum before loading
- [ ] Discovery finds latest valid snapshot, falls back to older if corrupt
- [ ] Recovery: load snapshot + replay WAL from offset
- [ ] Corrupt WAL entries skipped with warning (up to limit)
- [ ] RecoveryResult reports transactions recovered, corrupt entries skipped
- [ ] RecoveryOptions configures max corrupt entries, verify checksums

---

## Epic 42: WAL Enhancement

**Goal**: Enhance WAL with checksums and transaction framing

| Story | Description | Priority |
|-------|-------------|----------|
| #305 | WAL Entry Envelope with CRC32 | CRITICAL |
| #306 | Transaction Framing (Commit Markers) | CRITICAL |
| #307 | WAL Entry Type Registry | FOUNDATION |
| #308 | WAL Truncation After Snapshot | HIGH |
| #309 | WAL Corruption Detection | HIGH |

**Acceptance Criteria**:
- [ ] Every WAL entry: length, type, version, payload, crc32
- [ ] Transaction entries have tx_id, commit marker required
- [ ] Entry types 0x00-0x0F reserved for core, 0x10+ for primitives
- [ ] WAL truncation after successful snapshot (atomic)
- [ ] Corrupt entry detected by CRC mismatch, skipped gracefully

---

## Epic 43: Run Lifecycle & Replay

**Goal**: Implement run lifecycle and deterministic replay

| Story | Description | Priority |
|-------|-------------|----------|
| #310 | RunStatus Enum and RunMetadata Type | FOUNDATION |
| #311 | begin_run() Implementation | CRITICAL |
| #312 | end_run() Implementation | CRITICAL |
| #313 | RunIndex Event Offset Tracking | CRITICAL |
| #314 | replay_run() -> ReadOnlyView | CRITICAL |
| #315 | diff_runs() Key-Level Comparison | HIGH |
| #316 | Orphaned Run Detection | HIGH |

**Acceptance Criteria**:
- [ ] RunStatus: Active, Completed, Orphaned, NotFound
- [ ] begin_run() writes WAL entry, creates run metadata
- [ ] end_run() writes WAL entry, marks run completed
- [ ] RunIndex tracks event offsets for O(run size) replay
- [ ] replay_run() returns ReadOnlyView (doesn't mutate canonical store)
- [ ] diff_runs() compares two runs at key level
- [ ] Orphaned runs (no end marker) detected and reported

---

## Epic 44: Cross-Primitive Atomicity

**Goal**: Ensure transactions spanning primitives are atomic

| Story | Description | Priority |
|-------|-------------|----------|
| #317 | Transaction Grouping in WAL | CRITICAL |
| #318 | Atomic Commit (All or Nothing) | CRITICAL |
| #319 | Recovery Respects Transaction Boundaries | CRITICAL |
| #320 | Cross-Primitive Transaction Tests | HIGH |

**Acceptance Criteria**:
- [ ] All entries in a transaction share tx_id
- [ ] Commit marker required for transaction to be visible
- [ ] Recovery only applies entries with commit markers
- [ ] Transactions spanning KV + JSON + Event + State recover atomically
- [ ] Orphaned transactions (no commit) are not visible after recovery

---

## Epic 45: Storage Stabilization

**Goal**: Freeze storage APIs for future primitives

| Story | Description | Priority |
|-------|-------------|----------|
| #321 | PrimitiveStorageExt Trait | FOUNDATION |
| #322 | Primitive Registry Implementation | CRITICAL |
| #323 | Extension Point Documentation | HIGH |
| #324 | WAL Entry Type Allocation | HIGH |
| #325 | Snapshot Section Format | HIGH |

**Acceptance Criteria**:
- [ ] PrimitiveStorageExt trait for new primitives
- [ ] Primitive registry for dynamic primitive handling
- [ ] Clear documentation of extension points
- [ ] WAL entry type ranges allocated (0x70-0x7F for Vector)
- [ ] Snapshot section format documented for new primitives
- [ ] Adding Vector primitive (M8) requires NO changes to recovery engine

---

## Epic 46: Validation & Benchmarks

**Goal**: Ensure correctness and document performance baselines

| Story | Description | Priority |
|-------|-------------|----------|
| #326 | Crash Simulation Test Suite | CRITICAL |
| #327 | Recovery Invariant Tests | CRITICAL |
| #328 | Replay Determinism Tests | CRITICAL |
| #329 | Performance Baseline Documentation | HIGH |

**Acceptance Criteria**:
- [ ] Crash during normal operation test
- [ ] Crash during snapshot write test
- [ ] Crash during WAL truncation test
- [ ] Corrupt WAL entries test
- [ ] Corrupt snapshot test (fallback to older)
- [ ] All recovery invariants (R1-R6) validated
- [ ] All replay invariants (P1-P6) validated
- [ ] Performance baselines documented: snapshot write, snapshot load, WAL replay, full recovery

---

## Snapshot Triggers

| Trigger | Default | Configurable |
|---------|---------|--------------|
| WAL size | 100 MB | Yes |
| Time interval | 30 minutes | Yes |
| Manual | `db.snapshot()` | N/A |
| Shutdown | Optional (default: yes) | Yes |
| Retention | 2 snapshots | Yes |

---

## Files to Modify/Create

| File | Action | Description |
|------|--------|-------------|
| `crates/durability/src/snapshot.rs` | CREATE | Snapshot format, reader, writer |
| `crates/durability/src/snapshot_types.rs` | CREATE | SnapshotHeader, SnapshotEnvelope |
| `crates/durability/src/recovery.rs` | CREATE | Recovery engine |
| `crates/durability/src/wal.rs` | MODIFY | Add CRC32, transaction framing |
| `crates/durability/src/wal_types.rs` | MODIFY | Entry envelope with checksum |
| `crates/core/src/run_types.rs` | CREATE | RunStatus, RunMetadata |
| `crates/primitives/src/run_index.rs` | MODIFY | Event offset tracking, replay support |
| `crates/engine/src/database.rs` | MODIFY | Add snapshot(), begin_run(), end_run(), replay_run(), diff_runs() |
| `crates/engine/src/replay.rs` | CREATE | ReplayEngine, ReadOnlyView |
| `crates/storage/src/primitive_ext.rs` | CREATE | PrimitiveStorageExt trait |
| `Cargo.toml` | MODIFY | Add crc32fast dependency |

---

## Dependency Order

```
Epic 40 (Snapshot Format)
    ↓
Epic 41 (Crash Recovery) ←── Epic 42 (WAL Enhancement)
    ↓                              ↓
Epic 43 (Run Replay) ←───────── Epic 44 (Cross-Primitive)
    ↓
Epic 45 (Storage Stabilization)
    ↓
Epic 46 (Validation & Benchmarks)
```

**Recommended Implementation Order**:
1. Epic 42: WAL Enhancement (CRC32, transaction framing)
2. Epic 40: Snapshot Format & Writer
3. Epic 41: Crash Recovery
4. Epic 44: Cross-Primitive Atomicity
5. Epic 43: Run Lifecycle & Replay
6. Epic 45: Storage Stabilization
7. Epic 46: Validation & Benchmarks

---

## Success Metrics

**Functional**: All 38 stories passing, 100% acceptance criteria met

**Correctness**:
- All recovery invariants (R1-R6) validated
- All replay invariants (P1-P6) validated
- Crash simulation tests pass for all scenarios
- Cross-primitive atomicity verified

**Performance**:
- Snapshot write (100MB): < 5 seconds
- Snapshot load (100MB): < 3 seconds
- WAL replay (10K entries): < 1 second
- Full recovery (100MB + 10K WAL): < 5 seconds
- Replay run (1K events): < 100ms
- Diff runs (1K keys): < 200ms

**Quality**: Test coverage > 90% for new code

---

## Risk Mitigation

| Risk | Likelihood | Impact | Mitigation |
|------|------------|--------|------------|
| Data loss bugs | Medium | Critical | Extensive crash simulation, property tests |
| Replay non-determinism | Medium | High | Fuzzing, property-based tests |
| Recovery performance | Low | Medium | Benchmark early, optimize later |
| Cross-primitive atomicity bugs | Medium | High | Comprehensive transaction tests |
| Scope creep | Low | Medium | Strict "not in scope" adherence |

---

## Not In Scope (Explicitly Deferred)

1. **Compression** - M9 (optimization)
2. **Encryption at rest** - M11 (security)
3. **Incremental snapshots** - Post-MVP
4. **Point-in-time recovery (PITR)** - Post-MVP
5. **Timestamp-based replay** - Future (clock semantics complexity)
6. **Path-level diff** - Future (key-level is sufficient for M7)
7. **Online backup** - Post-MVP
8. **Parallel snapshot write** - Post-MVP

---

## Post-M7 Expectations

After M7 completion:
1. Database survives crashes and recovers correctly
2. Recovery time is bounded by snapshot frequency
3. Runs can be replayed deterministically
4. Adding Vector primitive (M8) requires NO changes to recovery engine
5. Storage APIs are frozen and documented
6. Performance baselines are documented for future comparison
