# M10 Implementation Plan: Storage Backend, Retention, and Compaction

## Overview

This document provides the high-level implementation plan for M10 (Storage Backend, Retention, and Compaction).

**Total Scope**: 7 Epics, 40 Stories

**References**:
- [M10 Architecture Specification](../../architecture/M10_ARCHITECTURE.md) - Authoritative spec

**Critical Framing**:
> M10 is an **infrastructure milestone**, not a feature milestone. It adds durability and portability without changing substrate semantics.
>
> Storage is infrastructure, not semantics. The user interacts with the same seven primitives through the same API. They do not know (and should not care) whether state lives in memory or on disk.
>
> **M10 does NOT make Strata a "disk-first" database.** Disk is a persistence layer, not the primary interface. The engine remains the source of truth for semantics.

**Epic Details**:
- [Epic 70: WAL Infrastructure](./EPIC_70_WAL_INFRASTRUCTURE.md)
- [Epic 71: Snapshot System](./EPIC_71_SNAPSHOT_SYSTEM.md)
- [Epic 72: Recovery](./EPIC_72_RECOVERY.md)
- [Epic 73: Retention Policies](./EPIC_73_RETENTION_POLICIES.md)
- [Epic 74: Compaction](./EPIC_74_COMPACTION.md)
- [Epic 75: Database Lifecycle](./EPIC_75_DATABASE_LIFECYCLE.md)
- [Epic 76: Crash Harness](./EPIC_76_CRASH_HARNESS.md)

---

## Architectural Integration Rules (NON-NEGOTIABLE)

These rules ensure M10 integrates properly with the M1-M9 architecture.

### Rule 1: Storage Is Logically Invisible

The storage layer must not change user-visible semantics. Before and after M10, the seven primitives behave identically.

**FORBIDDEN**: Storage-specific methods in the public API, storage details leaking to users.

### Rule 2: Durability Mode Determines Commit Semantics

Transaction commit semantics depend on durability mode. Storage must respect this.

**FORBIDDEN**: Ignoring durability mode, always fsync, or never fsync regardless of mode.

### Rule 3: Recovery Is Deterministic and Idempotent

Replaying the same WAL produces identical state. Replaying a record twice produces the same result as replaying once.

**FORBIDDEN**: Non-deterministic replay, version counters that increment on replay.

### Rule 4: Compaction Is Logically Invisible

Compaction must not change the result of reading any retained version. It reclaims space, nothing more.

**FORBIDDEN**: Compaction changing version IDs, reordering history, affecting read results.

### Rule 5: Retention Policies Are Database Entries

Retention policies are stored as first-class database entries in the system namespace, not in MANIFEST or config files.

**FORBIDDEN**: Storing policies in MANIFEST, unversioned config files, or hard-coded defaults.

### Rule 6: Storage Never Assigns Versions

Versions are assigned by the engine before persistence. Storage persists and replays versions faithfully.

**FORBIDDEN**: Storage generating version numbers, incrementing counters, modifying versions.

### Rule 7: WAL Segments Are Immutable Once Closed

Only the active segment is writable. Closed segments never change.

**FORBIDDEN**: Modifying closed WAL segments, in-place updates, partial rewrites.

### Rule 8: Correctness Over Performance

Any optimization that risks violating invariants is forbidden in M10. Correctness is non-negotiable; performance can be improved later.

**FORBIDDEN**: Batching that loses commits, caching that skips fsync, shortcuts that bypass invariant checks.

**Rationale**: Storage bugs are catastrophic. A slow but correct storage layer can be optimized. A fast but incorrect storage layer destroys user trust and data. M10 establishes correctness; future milestones optimize.

---

## Core Invariants

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

## Epic Overview

| Epic | Name | Stories | Dependencies | Status |
|------|------|---------|--------------|--------|
| 70 | WAL Infrastructure | 7 | M9 complete | Pending |
| 71 | Snapshot System | 6 | Epic 70 | Pending |
| 72 | Recovery | 5 | Epic 70, 71 | Pending |
| 73 | Retention Policies | 5 | Epic 72 | Pending |
| 74 | Compaction | 6 | Epic 71, 73 | Pending |
| 75 | Database Lifecycle | 6 | Epic 70, 71, 72 | Pending |
| 76 | Crash Harness | 5 | Epic 72, 75 | Pending |

---

## Epic 70: WAL Infrastructure

**Goal**: Implement append-only, segmented WAL with durability modes

| Story | Description | Priority |
|-------|-------------|----------|
| #498 | WAL Segment File Format | FOUNDATION |
| #499 | WAL Record Structure and Serialization | FOUNDATION |
| #500 | WAL Append with Durability Modes | CRITICAL |
| #501 | WAL Segment Rotation | CRITICAL |
| #502 | Writeset Serialization | CRITICAL |
| #503 | WAL Configuration (Segment Size, etc.) | HIGH |
| #504 | Codec Seam Integration | HIGH |

**Acceptance Criteria**:
- [ ] WAL segment files with format: `wal-NNNNNN.seg`
- [ ] WAL record format: length + format_version + txn_id + run_id + timestamp + writeset + checksum
- [ ] CRC32 checksum for each record
- [ ] Self-delimiting records (can parse independently)
- [ ] `append()` respects durability mode (InMemory, Buffered, Strict)
- [ ] Strict mode: fsync before returning
- [ ] Buffered mode: fsync on coarse boundary
- [ ] InMemory mode: no WAL writes
- [ ] Segment rotation when size exceeds configured limit (default 64MB)
- [ ] Closed segments are immutable (never modified)
- [ ] Writeset serialization with EntityRef and Mutation types
- [ ] All bytes pass through codec boundary (identity codec for MVP)

---

## Epic 71: Snapshot System

**Goal**: Implement point-in-time snapshots with crash-safe creation

| Story | Description | Priority |
|-------|-------------|----------|
| #506 | Snapshot File Format | FOUNDATION |
| #507 | Snapshot Serialization (All Primitives) | CRITICAL |
| #508 | Crash-Safe Snapshot Creation | CRITICAL |
| #509 | Checkpoint API | CRITICAL |
| #510 | Snapshot Metadata and Watermark | HIGH |
| #511 | Snapshot Loading | CRITICAL |

**Acceptance Criteria**:
- [ ] Snapshot files with format: `snap-NNNNNN.chk`
- [ ] Snapshot contains fully materialized logical state at watermark W
- [ ] All 7 primitives serialized correctly (KV, Event, State, Trace, Json, Vector, Run)
- [ ] Crash-safe creation: write temp → fsync → rename → update MANIFEST
- [ ] `checkpoint()` returns `CheckpointInfo { watermark_txn, snapshot_id, timestamp }`
- [ ] Snapshot watermark tracked in MANIFEST
- [ ] Snapshot loading reconstructs equivalent logical state
- [ ] Snapshots are logical (not memory dumps)
- [ ] Codec integration for snapshot data

---

## Epic 72: Recovery

**Goal**: Implement recovery from snapshot + WAL replay

| Story | Description | Priority |
|-------|-------------|----------|
| #513 | MANIFEST Structure and Persistence | FOUNDATION |
| #514 | WAL Replay Implementation | CRITICAL |
| #515 | Snapshot + WAL Recovery Algorithm | CRITICAL |
| #516 | Partial Record Truncation | CRITICAL |
| #517 | Recovery Verification Tests | HIGH |

**Acceptance Criteria**:
- [ ] MANIFEST with format_version, database_uuid, codec_id, active_wal_segment, snapshot_watermark
- [ ] MANIFEST atomic updates (write-fsync-rename)
- [ ] WAL replay: read records, verify checksum, apply mutations
- [ ] Replay is idempotent (same result regardless of replay count)
- [ ] Recovery algorithm: load snapshot → replay WAL records > watermark
- [ ] Partial/corrupt tail records truncated safely
- [ ] No committed transaction lost (Strict mode)
- [ ] Order preservation during replay
- [ ] Snapshot-WAL equivalence verified

---

## Epic 73: Retention Policies

**Goal**: Implement user-configurable retention policies as database entries

| Story | Description | Priority |
|-------|-------------|----------|
| #519 | RetentionPolicy Type Definition | FOUNDATION |
| #520 | System Namespace for Policies | CRITICAL |
| #521 | Retention Policy CRUD API | CRITICAL |
| #522 | Retention Policy Enforcement | CRITICAL |
| #523 | HistoryTrimmed Error Type | HIGH |

**Acceptance Criteria**:
- [ ] `RetentionPolicy` enum: KeepAll, KeepLast(N), KeepFor(Duration), Composite
- [ ] Policies stored in system namespace (`_strata/retention_policy`)
- [ ] Policies are versioned and transactional
- [ ] `set_retention_policy(run_id, policy)` returns `Version`
- [ ] `get_retention_policy(run_id)` returns `Option<Versioned<RetentionPolicy>>`
- [ ] Bootstrap default: `KeepAll` (no data loss by default)
- [ ] `HistoryTrimmed` error with `requested` and `earliest_retained` versions
- [ ] Retention enforcement during compaction
- [ ] Version ordering preserved for retained versions
- [ ] No silent fallback to nearest version

---

## Epic 74: Compaction

**Goal**: Implement deterministic, user-triggered compaction

| Story | Description | Priority |
|-------|-------------|----------|
| #525 | CompactMode Enum and CompactInfo | FOUNDATION |
| #526 | WAL-Only Compaction | CRITICAL |
| #527 | Full Compaction (with Retention) | CRITICAL |
| #528 | Tombstone Management | HIGH |
| #529 | Compaction Correctness Verification | HIGH |
| #530 | Compaction API | CRITICAL |

**Acceptance Criteria**:
- [ ] `CompactMode::WALOnly` - remove WAL segments covered by snapshot
- [ ] `CompactMode::Full` - WAL + retention enforcement
- [ ] `compact(mode)` returns `CompactInfo { reclaimed_bytes, wal_segments_removed, versions_removed }`
- [ ] Compaction is logically invisible (read equivalence)
- [ ] Version IDs never changed during compaction
- [ ] History order preserved
- [ ] Only removes data below snapshot watermark
- [ ] No implicit/background compaction
- [ ] Tombstones for deleted entries (internal implementation detail)

---

## Epic 75: Database Lifecycle

**Goal**: Implement database open/close and portability features

| Story | Description | Priority |
|-------|-------------|----------|
| #532 | Database Directory Structure | FOUNDATION |
| #533 | Database Open (New and Existing) | CRITICAL |
| #534 | Database Close | CRITICAL |
| #535 | DatabaseConfig Type | HIGH |
| #536 | Export (Convenience Wrapper) | HIGH |
| #537 | Import (Open Exported Artifact) | HIGH |

**Acceptance Criteria**:
- [ ] Directory structure: `strata.db/` with MANIFEST, WAL/, SNAPSHOTS/, DATA/
- [ ] `Database::open(path, config)` - open existing or create new
- [ ] `Database::open_default(config)` - use platform default path
- [ ] Create new: initialize directory, create MANIFEST
- [ ] Open existing: validate MANIFEST, run recovery
- [ ] Codec mismatch detected on open
- [ ] `Database::close()` - flush WAL, update MANIFEST, fsync
- [ ] `DatabaseConfig` with durability_mode, wal_segment_size, codec
- [ ] `export(path)` - checkpoint + copy directory
- [ ] `import(path)` = `Database::open(path, config)`
- [ ] Portability: copy closed database = valid clone

---

## Epic 76: Crash Harness

**Goal**: Implement a crash testing harness for validating storage correctness under failure conditions

| Story | Description | Priority |
|-------|-------------|----------|
| #539 | Crash Harness Framework | CRITICAL |
| #540 | Random Process Kill Tests | CRITICAL |
| #541 | WAL Tail Corruption Tests | CRITICAL |
| #542 | Reference Model Comparator | HIGH |
| #543 | Crash Scenario Matrix | HIGH |

**Acceptance Criteria**:
- [ ] Crash harness that can kill process at random points during operation
- [ ] Configurable crash injection points (mid-write, post-write, pre-fsync, post-fsync)
- [ ] WAL tail corruption simulation (truncate, garbage bytes, partial records)
- [ ] MANIFEST corruption simulation (missing, truncated, invalid)
- [ ] Reference model that tracks expected state from successful operations
- [ ] Comparator that validates recovered state matches reference model
- [ ] Crash scenario matrix covering:
  - Crash during WAL append (various points)
  - Crash during segment rotation
  - Crash during snapshot creation
  - Crash during MANIFEST update
  - Crash during compaction
  - Multiple consecutive crashes
- [ ] All scenarios pass: recovered state matches reference or graceful error
- [ ] Property-based testing integration (randomized operation sequences)
- [ ] CI integration with crash harness tests

**Rationale**: Storage bugs are catastrophic and often only manifest under specific failure conditions. A systematic crash harness is how serious storage engines are validated. This is not optional for a durable storage layer.

---

## Files to Create/Modify

> **Design Note**: The `format/` module centralizes all on-disk byte formats. This separation keeps serialization logic (how bytes are laid out) separate from operational logic (how WAL/snapshots/MANIFEST are managed). This pattern prevents business logic from creeping into serialization code and makes format evolution easier to manage.

### New Files

| File | Description |
|------|-------------|
| `crates/storage/src/lib.rs` | Storage crate entry point |
| **Format Module** | **On-disk byte formats (separates serialization from logic)** |
| `crates/storage/src/format/mod.rs` | Format module entry point |
| `crates/storage/src/format/wal_record.rs` | WAL record binary format |
| `crates/storage/src/format/snapshot.rs` | Snapshot binary format |
| `crates/storage/src/format/manifest.rs` | MANIFEST binary format |
| `crates/storage/src/format/writeset.rs` | Writeset binary format |
| `crates/storage/src/format/primitives.rs` | Primitive serialization formats |
| **WAL Module** | **WAL operations (uses format/)** |
| `crates/storage/src/wal/mod.rs` | WAL module |
| `crates/storage/src/wal/segment.rs` | WAL segment file handling |
| `crates/storage/src/wal/writer.rs` | WAL append and rotation |
| `crates/storage/src/wal/reader.rs` | WAL reading for replay |
| `crates/storage/src/wal/config.rs` | WAL configuration |
| **Snapshot Module** | **Snapshot operations (uses format/)** |
| `crates/storage/src/snapshot/mod.rs` | Snapshot module |
| `crates/storage/src/snapshot/writer.rs` | Snapshot creation logic |
| `crates/storage/src/snapshot/reader.rs` | Snapshot loading logic |
| **Recovery Module** | **Recovery operations (uses format/)** |
| `crates/storage/src/recovery/mod.rs` | Recovery module |
| `crates/storage/src/recovery/manifest.rs` | MANIFEST handling |
| `crates/storage/src/recovery/replay.rs` | WAL replay implementation |
| **Retention Module** | |
| `crates/storage/src/retention/mod.rs` | Retention module |
| `crates/storage/src/retention/policy.rs` | RetentionPolicy types |
| `crates/storage/src/retention/enforcement.rs` | Retention enforcement |
| **Compaction Module** | |
| `crates/storage/src/compaction/mod.rs` | Compaction module |
| `crates/storage/src/compaction/wal_only.rs` | WAL-only compaction |
| `crates/storage/src/compaction/full.rs` | Full compaction |
| **Codec Module** | |
| `crates/storage/src/codec/mod.rs` | Codec module |
| `crates/storage/src/codec/identity.rs` | Identity codec |
| `crates/storage/src/codec/trait.rs` | StorageCodec trait |
| **Top-level Files** | |
| `crates/storage/src/database.rs` | Database lifecycle |
| `crates/storage/src/error.rs` | Storage errors |

### Modified Files

| File | Changes |
|------|---------|
| `crates/engine/src/database.rs` | Wire storage backend |
| `crates/engine/src/transaction/commit.rs` | Write to WAL on commit |
| `crates/core/src/error.rs` | Add HistoryTrimmed error |
| `Cargo.toml` | Add storage crate |

---

## Dependency Order

```
Epic 70 (WAL Infrastructure)
    ↓
Epic 71 (Snapshot System) ←── Epic 70
    ↓
Epic 72 (Recovery) ←── Epic 70, 71
    ↓
Epic 75 (Database Lifecycle) ←── Epic 70, 71, 72
    ↓
Epic 76 (Crash Harness) ←── Epic 72, 75
    ↓
Epic 73 (Retention Policies) ←── Epic 72
    ↓
Epic 74 (Compaction) ←── Epic 71, 73
```

**Recommended Implementation Order**:
1. Epic 70: WAL Infrastructure (foundation for everything)
2. Epic 71: Snapshot System (depends on WAL for watermark tracking)
3. Epic 72: Recovery (brings WAL + Snapshot together)
4. Epic 75: Database Lifecycle (uses recovery, enables testing)
5. Epic 76: Crash Harness (validates correctness before adding complexity)
6. Epic 73: Retention Policies (requires working database)
7. Epic 74: Compaction (requires retention policies)

---

## Phased Implementation Strategy

> **Guiding Principle**: Build the foundation first. WAL must work before snapshots. Recovery must work before retention. Each phase produces a testable, usable increment.

### Phase 1: WAL Foundation

Implement WAL infrastructure and basic commit durability:
- WAL segment format and record structure
- Append with durability modes
- Segment rotation
- Codec seam (identity codec)

**Exit Criteria**: Commits are written to WAL. WAL can be read back. Durability modes respected.

### Phase 2: Snapshot + Recovery

Implement snapshots and recovery algorithm:
- Snapshot serialization for all primitives
- Crash-safe snapshot creation
- MANIFEST structure
- Recovery: snapshot + WAL replay

**Exit Criteria**: Database can checkpoint, crash, and recover to correct state.

### Phase 3: Database Lifecycle

Implement open/close and portability:
- Directory structure creation
- Database open (new and existing)
- Database close with proper cleanup
- Export/import convenience APIs

**Exit Criteria**: Can create, open, close, copy databases. Full round-trip works.

### Phase 4: Crash Harness

Implement crash testing harness before adding more complexity:
- Crash injection framework
- Random kill tests
- Tail corruption tests
- Reference model comparator

**Exit Criteria**: Storage correctness validated under systematic crash scenarios.

### Phase 5: Retention + Compaction

Implement retention policies and compaction:
- RetentionPolicy types and storage
- System namespace for policies
- WAL-only compaction
- Full compaction with retention

**Exit Criteria**: Retention policies control data lifetime. Compaction reclaims space correctly.

### Phase Summary

| Phase | Epics | Key Deliverable | Status |
|-------|-------|-----------------|--------|
| 1 | 70 | WAL durability | Pending |
| 2 | 71, 72 | Crash recovery | Pending |
| 3 | 75 | Database lifecycle | Pending |
| 4 | 76 | Crash validation | Pending |
| 5 | 73, 74 | Retention + Compaction | Pending |

---

## Testing Strategy

### Unit Tests

- WAL record serialization/deserialization
- Checksum calculation and verification
- Segment rotation logic
- Snapshot serialization for each primitive
- MANIFEST read/write
- RetentionPolicy serialization
- Codec encode/decode

### Integration Tests

- Write transactions, verify in WAL
- Checkpoint, verify snapshot contents
- Full recovery cycle (commit → crash → recover)
- Multiple checkpoints, verify correct snapshot used
- Database open/close lifecycle
- Export → import round-trip

### Recovery Tests

- Commit in Strict mode, crash, verify data survives
- Commit in Buffered mode, crash at various points
- Partial WAL record, verify truncation
- Corrupt checksum, verify detection
- Crash during snapshot creation, verify recovery
- Crash during MANIFEST update, verify recovery
- Multiple crashes in sequence

### Compaction Tests

- WAL-only compaction, verify segments removed
- Full compaction with retention, verify versions removed
- Read equivalence before/after compaction
- Version IDs unchanged after compaction
- Compaction with concurrent reads

### Retention Tests

- KeepAll preserves everything
- KeepLast(N) keeps only N versions
- KeepFor(Duration) keeps versions within window
- Trimmed version returns HistoryTrimmed error
- Policy changes are versioned

### Portability Tests

- Close database, copy directory, open copy
- Export, import, verify identical state
- Cross-platform portability (byte order, etc.)

---

## Success Metrics

**Functional**: All 40 stories passing, 100% acceptance criteria met

**Correctness**:
- All storage invariants (S1-S9) validated
- All recovery invariants (R1-R5) validated
- All retention invariants (RT1-RT4) validated
- All compaction invariants (C1-C5) validated
- Crash recovery verified with multiple scenarios
- Determinism verified (same operations = same state)

**Durability**:
- No committed transaction lost (Strict mode)
- Recovery time < 1s for typical workloads
- WAL replay correct for all transaction types

**Portability**:
- Copy closed database = valid clone
- Export/import round-trip works
- No platform-specific issues

**Quality**: Test coverage > 90% for new code

---

## Risk Mitigation

| Risk | Likelihood | Impact | Mitigation |
|------|------------|--------|------------|
| WAL corruption | Low | High | CRC32 checksums, recovery truncation |
| Snapshot inconsistency | Medium | High | Point-in-time capture, comprehensive tests |
| Recovery bugs | Medium | High | Property-based testing, crash simulation |
| Performance regression | Medium | Medium | Benchmark before/after, async fsync |
| Complexity creep | Medium | Medium | Strict scope, no background operations |
| Codec bugs | Low | Medium | Start with identity codec, extensive tests |

---

## Not In Scope (Explicitly Deferred)

1. **Encryption implementation** - Post-MVP (codec seam ready)
2. **Background compaction** - Post-MVP
3. **Incremental snapshots** - Post-MVP
4. **Multi-node replication** - Post-MVP
5. **Tiered storage (S3, etc.)** - Post-MVP
6. **Online defragmentation** - Post-MVP
7. **Automatic checkpointing** - Post-MVP (users trigger manually)
8. **WAL compression** - Post-MVP
9. **Parallel WAL replay** - Post-MVP

---

## Post-M10 Expectations

After M10 completion:
1. Strata persists all committed transactions to disk
2. Strata recovers correctly after crash (Strict mode: no data loss)
3. Database can grow beyond RAM (storage is authoritative)
4. Users can configure retention policies (KeepAll, KeepLast, KeepFor)
5. Users can trigger compaction to reclaim disk space
6. Database is portable by copy (`strata.db/` directory)
7. Codec seam ready for future encryption-at-rest
8. Same 7 primitives, same API, now durable

---

## WAL Entry Types

```rust
// WAL record contains:
// - format_version: u8
// - txn_id: u64
// - run_id: RunId (UUID)
// - commit_timestamp: u64
// - writeset: Writeset
// - checksum: u32 (CRC32)

// Writeset contains mutations:
pub enum Mutation {
    Put { entity_ref: EntityRef, value: Vec<u8>, version: Version },
    Delete { entity_ref: EntityRef },
    Append { entity_ref: EntityRef, value: Vec<u8>, version: Version },
}
```

---

## MANIFEST Format

```rust
pub struct Manifest {
    pub format_version: u32,      // For forward compatibility
    pub database_uuid: Uuid,       // Unique database identifier
    pub codec_id: String,          // Codec for encryption
    pub active_wal_segment: u64,   // Current writable segment
    pub snapshot_watermark: Option<u64>,  // Latest snapshot txn_id
    pub snapshot_id: Option<u64>,  // Latest snapshot identifier
}
```

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

## Document History

| Version | Date | Changes |
|---------|------|---------|
| 1.0 | 2026-01-20 | Initial M10 implementation plan |
| 1.1 | 2026-01-20 | Added Rule 8 (Correctness > Performance), Epic 76 (Crash Harness), format/ module |
