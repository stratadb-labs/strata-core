# M3 Project Status: Primitives

**Last Updated**: 2026-01-14

## Current Phase: PLANNING COMPLETE - Ready for Implementation

---

## M3 Overview

**Goal**: Implement all five MVP primitives as stateless facades over the M2 transactional engine

**Authoritative Specifications**:
- `docs/architecture/M3_ARCHITECTURE.md` (v1.2)
- `docs/diagrams/m3-architecture.md`
- `docs/milestones/M3_IMPLEMENTATION_PLAN.md`
- `docs/milestones/M3_EPICS.md`

**Primitives to Implement**:
1. **KVStore** - General-purpose key-value storage
2. **EventLog** - Immutable append-only event stream with causal hash chaining
3. **StateCell** - CAS-based versioned cells for coordination
4. **TraceStore** - Structured reasoning traces with indexing
5. **RunIndex** - First-class run lifecycle management

---

## Progress Summary

| Epic | Name | Stories | Status | Blocker |
|------|------|---------|--------|---------|
| 13 ([#159](https://github.com/anibjoshi/in-mem/issues/159)) | Primitives Foundation | #166-#168 | 🔲 Not Started | - |
| 14 ([#160](https://github.com/anibjoshi/in-mem/issues/160)) | KVStore Primitive | #169-#173 | 🔲 Not Started | Epic 13 |
| 15 ([#161](https://github.com/anibjoshi/in-mem/issues/161)) | EventLog Primitive | #174-#179 | 🔲 Not Started | Epic 13 |
| 16 ([#162](https://github.com/anibjoshi/in-mem/issues/162)) | StateCell Primitive | #180-#184 | 🔲 Not Started | Epic 13 |
| 17 ([#163](https://github.com/anibjoshi/in-mem/issues/163)) | TraceStore Primitive | #185-#190 | 🔲 Not Started | Epic 13 |
| 18 ([#164](https://github.com/anibjoshi/in-mem/issues/164)) | RunIndex Primitive | #191-#196 | 🔲 Not Started | Epic 13 |
| 19 ([#165](https://github.com/anibjoshi/in-mem/issues/165)) | Integration & Validation | #197-#201 | 🔲 Not Started | Epics 14-18 |

**Overall Progress**: 0/7 epics complete (0/36 stories)

---

## Epic 13 (GitHub #159): Primitives Foundation 🔲 NOT STARTED

### Stories

| Story | Title | Status | Assignee |
|-------|-------|--------|----------|
| #166 | Primitives Crate Setup & TypeTag Extensions | 🔲 | - |
| #167 | Key Construction Helpers | 🔲 | - |
| #168 | Transaction Extension Trait Infrastructure | 🔲 | - |

### Deliverables
- [ ] `crates/primitives/Cargo.toml`
- [ ] `crates/primitives/src/lib.rs`
- [ ] `crates/core/src/types.rs` (TypeTag additions)
- [ ] Key construction helpers (`Key::new_kv`, `Key::new_event`, etc.)

### Blockers
- None (starts M3)

### Notes
- Story #166 blocks ALL other M3 stories
- TypeTag values: KV=0x01, Event=0x02, State=0x03, Trace=0x04, Run=0x05

---

## Epic 14 (GitHub #160): KVStore Primitive 🔲 NOT STARTED

### Stories

| Story | Title | Status | Assignee |
|-------|-------|--------|----------|
| #169 | KVStore Core Structure | 🔲 | - |
| #170 | KVStore Single-Operation API | 🔲 | - |
| #171 | KVStore Multi-Operation API | 🔲 | - |
| #172 | KVStore List Operations | 🔲 | - |
| #173 | KVStoreExt Transaction Extension | 🔲 | - |

### Deliverables
- [ ] `crates/primitives/src/kv.rs`
- [ ] KVStore struct with get, put, put_with_ttl, delete, list
- [ ] KVTransaction for atomic multi-key operations
- [ ] KVStoreExt trait for cross-primitive transactions

### Blockers
- Epic 13 must be complete

---

## Epic 15 (GitHub #161): EventLog Primitive 🔲 NOT STARTED

### Stories

| Story | Title | Status | Assignee |
|-------|-------|--------|----------|
| #174 | EventLog Core & Event Structure | 🔲 | - |
| #175 | EventLog Append with Hash Chaining | 🔲 | - |
| #176 | EventLog Read Operations | 🔲 | - |
| #177 | EventLog Chain Verification | 🔲 | - |
| #178 | EventLog Query by Type | 🔲 | - |
| #179 | EventLogExt Transaction Extension | 🔲 | - |

### Deliverables
- [ ] `crates/primitives/src/event_log.rs`
- [ ] Event struct with sequence, type, payload, timestamp, hashes
- [ ] Append with automatic sequence assignment and hash chaining
- [ ] verify_chain() for integrity validation
- [ ] EventLogExt trait + append-only invariant enforcement

### Blockers
- Epic 13 must be complete

### Notes
- EventLog is single-writer-ordered per run (CAS on metadata key)
- Hash chaining is causal, not cryptographically secure

---

## Epic 16 (GitHub #162): StateCell Primitive 🔲 NOT STARTED

### Stories

| Story | Title | Status | Assignee |
|-------|-------|--------|----------|
| #180 | StateCell Core & State Structure | 🔲 | - |
| #181 | StateCell Read/Init/Delete Operations | 🔲 | - |
| #182 | StateCell CAS & Set Operations | 🔲 | - |
| #183 | StateCell Transition Closure Pattern | 🔲 | - |
| #184 | StateCellExt Transaction Extension | 🔲 | - |

### Deliverables
- [ ] `crates/primitives/src/state_cell.rs`
- [ ] State struct with value, version, updated_at
- [ ] init, read, cas, set, delete, list, exists operations
- [ ] transition() with automatic retry
- [ ] StateCellExt trait

### Blockers
- Epic 13 must be complete

### Notes
- Purity requirement: transition() closures must be pure (may execute multiple times)
- Named "StateCell" not "StateMachine" - M3 is CAS cells only, not full state machine

---

## Epic 17 (GitHub #163): TraceStore Primitive 🔲 NOT STARTED

### Stories

| Story | Title | Status | Assignee |
|-------|-------|--------|----------|
| #185 | TraceStore Core & TraceType Structures | 🔲 | - |
| #186 | TraceStore Record Operations | 🔲 | - |
| #187 | TraceStore Secondary Indices | 🔲 | - |
| #188 | TraceStore Query Operations | 🔲 | - |
| #189 | TraceStore Tree Reconstruction | 🔲 | - |
| #190 | TraceStoreExt Transaction Extension | 🔲 | - |

### Deliverables
- [ ] `crates/primitives/src/trace.rs`
- [ ] TraceType enum (ToolCall, Decision, Query, Thought, Error, Custom)
- [ ] Trace struct with parent-child relationships
- [ ] Secondary indices (by-type, by-tag, by-parent, by-time)
- [ ] get_tree() for hierarchical reconstruction
- [ ] TraceStoreExt trait

### Blockers
- Epic 13 must be complete

### Notes
- Performance warning: 3-4 index entries per trace (write amplification)
- Designed for debuggability, not high-volume telemetry

---

## Epic 18 (GitHub #164): RunIndex Primitive 🔲 NOT STARTED

### Stories

| Story | Title | Status | Assignee |
|-------|-------|--------|----------|
| #191 | RunIndex Core & RunMetadata Structures | 🔲 | - |
| #192 | RunIndex Create & Get Operations | 🔲 | - |
| #193 | RunIndex Status Update & Transition Validation | 🔲 | - |
| #194 | RunIndex Query Operations & Indices | 🔲 | - |
| #195 | RunIndex Delete & Archive Operations | 🔲 | - |
| #196 | RunIndex Integration with Other Primitives | 🔲 | - |

### Deliverables
- [ ] `crates/primitives/src/run_index.rs`
- [ ] RunStatus enum (Active, Completed, Failed, Cancelled, Paused, Archived)
- [ ] RunMetadata struct
- [ ] Status transition validation (no resurrection, archived is terminal)
- [ ] delete_run() with cascading hard delete
- [ ] archive_run() for soft delete

### Blockers
- Epic 13 must be complete

### Notes
- Status transitions enforced: `is_valid_transition(from, to)`
- Cascading delete removes ALL data for run (KV, Events, States, Traces)

---

## Epic 19 (GitHub #165): Integration & Validation 🔲 NOT STARTED

### Stories

| Story | Title | Status | Assignee |
|-------|-------|--------|----------|
| #197 | Cross-Primitive Transaction Tests | 🔲 | - |
| #198 | Run Isolation Integration Tests | 🔲 | - |
| #199 | Primitive Recovery Tests | 🔲 | - |
| #200 | Primitive Performance Benchmarks | 🔲 | - |
| #201 | M3 Completion Validation | 🔲 | - |

### Deliverables
- [ ] Cross-primitive transaction tests (KV + Event + State + Trace atomic)
- [ ] Run isolation tests for all 5 primitives
- [ ] Recovery tests (primitives survive crash + WAL replay)
- [ ] Performance benchmarks meeting targets
- [ ] `docs/milestones/M3_COMPLETION_REPORT.md`

### Blockers
- Epics 14-18 must be complete

### Performance Targets
| Operation | Target |
|-----------|--------|
| KV put | >10K ops/sec |
| KV get | >20K ops/sec |
| EventLog append | >5K ops/sec |
| StateCell CAS | >5K ops/sec |
| TraceStore record | >2K ops/sec |
| Cross-primitive txn | >1K ops/sec |

---

## Test Summary

| Crate | Tests | Status |
|-------|-------|--------|
| in-mem-primitives | 0 | 🔲 Not Started |
| (existing M1-M2 tests) | 630+ | ✅ Passing |

---

## Key Design Decisions

| Decision | Status | Notes |
|----------|--------|-------|
| Stateless facade pattern | ✅ Approved | Primitives hold Arc<Database> only |
| EventLog single-writer-ordered | ✅ Approved | CAS on metadata key |
| Causal hash chaining | ✅ Approved | Not cryptographic, upgrade path to SHA-256 |
| StateCell purity requirement | ✅ Approved | Closures may execute multiple times |
| TraceStore write amplification | ✅ Approved | Documented performance warning |
| RunIndex status transitions | ✅ Approved | No resurrection, archived is terminal |
| No direct storage mutation | ✅ Approved | All mutations through primitives |

---

## Branch Strategy

```
main                              ← Protected (M2 complete merged)
  └── develop                     ← Working branch for M3
        └── feature/m3-*          ← Epic/story branches
```

---

## GitHub Issues

| Type | Count | Status |
|------|-------|--------|
| Epic Issues (#159-#165) | 7 | ✅ Created |
| Story Issues (#166-#201) | 36 | ✅ Created |

---

## Risks & Blockers

| Risk | Status | Mitigation |
|------|--------|------------|
| TypeTag collisions | 🟢 Low | Reserved 0x10+ for future |
| Hash chain bugs | 🟡 Medium | Comprehensive verify_chain tests |
| Index inconsistency | 🟡 Medium | Atomic index writes |
| TraceStore performance | 🟢 Low | Documented warning |
| Cascading delete misses keys | 🟡 Medium | Integration test with all primitives |

---

## Next Actions

1. [x] Create GitHub epic issues (#159-#165)
2. [x] Create GitHub story issues (#166-#201)
3. [ ] Begin Epic 13: Primitives Foundation
4. [ ] Story #166: Primitives Crate Setup (BLOCKS ALL M3)

---

## Timeline

| Phase | Duration | Status |
|-------|----------|--------|
| Planning | 1 day | ✅ Complete |
| Epic 13 (Foundation) | 1 day | 🔲 Not Started |
| Epics 14-18 (Primitives) | 3 days | 🔲 Not Started |
| Epic 19 (Integration) | 1 day | 🔲 Not Started |
| **Total** | **~6 days** | **Planning Complete** |

---

## Change Log

| Date | Change |
|------|--------|
| 2026-01-14 | Initial M3 project status created |
| 2026-01-14 | M3 Architecture Spec v1.2 complete |
| 2026-01-14 | M3 Implementation Plan complete |
| 2026-01-14 | M3 Epics document complete |
| 2026-01-14 | GitHub issues created (#159-#165 epics, #166-#201 stories) |
| 2026-01-14 | Updated all documents with correct GitHub issue numbers |

---

*Last updated: 2026-01-14*
