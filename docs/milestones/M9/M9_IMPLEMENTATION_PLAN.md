# M9 Implementation Plan: API Stabilization & Universal Protocol

## Overview

This document provides the high-level implementation plan for M9 (API Stabilization & Universal Protocol).

**Total Scope**: 5 Epics, 28 Stories

**References**:
- [M9 Architecture Specification](../../architecture/M9_ARCHITECTURE.md) - Authoritative spec
- [PRIMITIVE_CONTRACT.md](../../architecture/PRIMITIVE_CONTRACT.md) - The seven invariants
- [CORE_API_SHAPE.md](../../architecture/CORE_API_SHAPE.md) - API patterns

**Critical Framing**:
> M9 is not about features. M9 is about **contracts**.
>
> Before building the server (M10), before adding Python clients (M12), the interface must be stable. M9 separates invariants from conveniences and substrate from product.
>
> "What is the universal way a user interacts with anything in Strata?" This milestone answers that question.

**Epic Details**:
- [Epic 60: Core Types](./EPIC_60_CORE_TYPES.md)
- [Epic 61: Versioned Returns](./EPIC_61_VERSIONED_RETURNS.md)
- [Epic 62: Transaction Unification](./EPIC_62_TRANSACTION_UNIFICATION.md)
- [Epic 63: Error Standardization](./EPIC_63_ERROR_STANDARDIZATION.md)
- [Epic 64: Conformance Testing](./EPIC_64_CONFORMANCE_TESTING.md)

---

## Architectural Integration Rules (NON-NEGOTIABLE)

These rules ensure M9 produces a stable, consistent API.

### Rule 1: Every Read Returns Versioned<T>

No read operation may return raw values without version information.

**FORBIDDEN**: Any read returning `Option<T>` instead of `Option<Versioned<T>>`.

### Rule 2: Every Write Returns Version

Every mutation returns the version it created.

**FORBIDDEN**: Any write returning `()` or `Result<()>`.

### Rule 3: Transaction Trait Covers All Primitives

Every primitive operation is accessible through the `TransactionOps` trait.

**FORBIDDEN**: Any primitive operation that cannot be called within a transaction.

### Rule 4: Run Scope Is Always Explicit

The run is always known. No ambient run context.

**FORBIDDEN**: Thread-local run state, implicit run scoping.

### Rule 5: No Primitive-Specific Special Cases

All primitives follow the same API patterns.

**FORBIDDEN**: Special handling for one primitive that breaks the universal pattern.

---

## Core Invariants

### The Seven Invariants (from PRIMITIVE_CONTRACT.md)

| # | Invariant | API Expression |
|---|-----------|----------------|
| 1 | Everything is Addressable | `EntityRef` type |
| 2 | Everything is Versioned | `Versioned<T>` wrapper |
| 3 | Everything is Transactional | `TransactionOps` trait |
| 4 | Everything Has a Lifecycle | CRUD method patterns |
| 5 | Everything Exists Within a Run | `RunId` parameter |
| 6 | Everything is Introspectable | `exists()` methods |
| 7 | Reads and Writes Have Consistent Semantics | `&self` vs `&mut self` |

### Conformance Testing Requirements

| # | Test Category | Count |
|---|---------------|-------|
| C1 | Invariant 1 tests (addressable) | 7 (one per primitive) |
| C2 | Invariant 2 tests (versioned) | 14 (read + write per primitive) |
| C3 | Invariant 3 tests (transactional) | 7 |
| C4 | Invariant 4 tests (lifecycle) | 7 |
| C5 | Invariant 5 tests (run-scoped) | 7 |
| C6 | Invariant 6 tests (introspectable) | 7 |
| C7 | Invariant 7 tests (read/write) | 7 |

**Total conformance tests**: 56 (some overlap, target ~49 unique)

---

## Epic Overview

| Epic | Name | Stories | Dependencies | Status |
|------|------|---------|--------------|--------|
| 60 | Core Types | 6 | M8 complete | ✅ COMPLETE |
| 63 | Error Standardization | 4 | Epic 60 | ✅ COMPLETE |
| 61 | Versioned Returns | 7 | Epic 60 | ✅ COMPLETE |
| 62 | Transaction Unification | 6 | Epic 60 | ✅ COMPLETE |
| 64 | Conformance Testing | 5 | Epic 61, 62, 63 | ✅ COMPLETE |

---

## Epic 60: Core Types ✅ COMPLETE

**Goal**: Define universal types that express the seven invariants

**Status**: Completed in branch `milestone-9-phase-1` (commit f8df454)

| Story | Description | Priority | Status |
|-------|-------------|----------|--------|
| #469 | EntityRef Enum Implementation | FOUNDATION | ✅ Done |
| #470 | Versioned<T> Wrapper Type | FOUNDATION | ✅ Done |
| #471 | Version Enum (TxnId, Sequence, Counter) | FOUNDATION | ✅ Done |
| #472 | Timestamp Type | FOUNDATION | ✅ Done |
| #473 | PrimitiveType Enum | HIGH | ✅ Done |
| #474 | RunName Type | FOUNDATION | ✅ Done |

**Acceptance Criteria**:
- [x] `EntityRef` enum with variants for all 7 primitives
- [x] `EntityRef::run_id()` method returns the run for any entity
- [x] `EntityRef::primitive_type()` method returns `PrimitiveType`
- [x] `Versioned<T>` with value, version fields (unified with VersionedValue)
- [x] `Versioned<T>::map()` for transforming inner value
- [x] `Versioned<T>::into_value()` for extracting value
- [x] `Version` enum: TxnId(u64), Sequence(u64), Counter(u64)
- [x] `Version::as_u64()` for numeric comparison
- [x] `Timestamp` type with `now()` constructor (microsecond precision)
- [x] `RunName` type with validation (alphanumeric, max 128 chars)
- [x] All types implement Debug, Clone; IDs implement Hash, Eq

**Implementation Notes**:
- Types located in `crates/core/src/contract/`
- In-place migration: `PrimitiveKind` renamed to `PrimitiveType`
- `DocRef` unified with `EntityRef` (all variants require `run_id`)
- `Timestamp` changed from seconds (i64) to microseconds (u64)
- All existing tests updated (1500+ library tests, 445+ integration tests pass)

---

## Epic 61: Versioned Returns ✅ COMPLETE

**Goal**: Wrap all read returns in Versioned<T>, all writes return Version

**Status**: Completed in branch `milestone-9-phase-3` (commit ba85d89)

| Story | Description | Priority | Status |
|-------|-------------|----------|--------|
| #466 | KVStore Versioned Returns | CRITICAL | ✅ Done |
| #467 | EventLog Versioned Returns | CRITICAL | ✅ Done |
| #468 | StateCell Versioned Returns | CRITICAL | ✅ Done |
| #469 | TraceStore Versioned Returns | CRITICAL | ✅ Done |
| #470 | JsonStore Versioned Returns | CRITICAL | ✅ Done |
| #471 | VectorStore Versioned Returns | CRITICAL | ✅ Done |
| #472 | RunIndex Versioned Returns | CRITICAL | ✅ Done |

**Acceptance Criteria**:
- [x] `kv.get()` returns `Option<Versioned<Value>>`
- [x] `kv.put()` returns `Version`
- [x] `events.read()` returns `Option<Versioned<Event>>`
- [x] `events.append()` returns `Version`
- [x] `state.read()` returns `Option<Versioned<State>>`
- [x] `state.init()/set()` returns `Versioned<u64>`
- [x] `traces.get()` returns `Option<Versioned<Trace>>`
- [x] `traces.record()` returns `Versioned<String>` (trace_id)
- [x] `json.get()` returns `Option<Versioned<JsonValue>>`
- [x] `json.create()/set()` returns `Version`
- [x] `vector.get()` returns `Option<Versioned<VectorEntry>>`
- [x] `vector.insert()` returns `Version`
- [x] `runs.get_run()` returns `Option<Versioned<RunMetadata>>`
- [x] `runs.create_run()` returns `Versioned<RunMetadata>`
- [x] All existing tests updated to expect versioned returns

---

## Epic 62: Transaction Unification ✅ COMPLETE

**Goal**: Unified TransactionOps trait covering all primitives

**Status**: Completed in branch `milestone-9-phase-3` (commit ba85d89)

| Story | Description | Priority | Status |
|-------|-------------|----------|--------|
| #473 | TransactionOps Trait Definition | FOUNDATION | ✅ Done |
| #474 | KV Operations in TransactionOps | CRITICAL | ✅ Done |
| #475 | Event Operations in TransactionOps | CRITICAL | ✅ Done |
| #476 | State/Trace Operations in TransactionOps | CRITICAL | ✅ Done |
| #477 | Json/Vector Operations in TransactionOps | CRITICAL | ✅ Done |
| #478 | RunHandle Pattern Implementation | HIGH | ✅ Done |

**Acceptance Criteria**:
- [x] Extension traits with all primitive operations (KVStoreExt, EventLogExt, etc.)
- [x] Implemented on `TransactionContext`
- [x] All methods return `Result<T>`
- [x] KV: `kv_get`, `kv_put`, `kv_delete`, `kv_exists`
- [x] Event: `event_append`, `event_read`
- [x] State: `state_read`, `state_set`, `state_cas`
- [x] Trace: `trace_record`, `trace_record_child`
- [x] Json: `json_create`, `json_get`, `json_set`
- [x] Vector: `vector_insert`, `vector_get`
- [x] `RunHandle` provides scoped access to primitives
- [x] `RunHandle::kv()`, `events()`, `state()`, `traces()`, `json()`, `vectors()`
- [x] `RunHandle::transaction()` for atomic operations
- [x] Cross-primitive transaction works: KV + Event + State + Trace + Json + Vector

**Implementation Notes**:
- Extension traits defined in `crates/primitives/src/extensions.rs`
- `RunHandle` implemented in `crates/primitives/src/run_handle.rs`
- Cross-primitive atomicity verified by conformance tests

---

## Epic 63: Error Standardization

**Goal**: Unified StrataError across all primitives

| Story | Description | Priority |
|-------|-------------|----------|
| #479 | StrataError Enum Definition | FOUNDATION |
| #480 | Error Conversion from Primitive Errors | CRITICAL |
| #481 | EntityRef in Error Messages | HIGH |
| #482 | Error Documentation and Guidelines | HIGH |

**Acceptance Criteria**:
- [ ] `StrataError` enum with all variants:
  - `NotFound { entity_ref: EntityRef }`
  - `VersionConflict { entity_ref, expected, actual }`
  - `TransactionAborted { reason }`
  - `RunNotFound { run_id }`
  - `InvalidOperation { entity_ref, reason }`
  - `DimensionMismatch { expected, got }`
  - `Storage(StorageError)`
  - `Serialization(String)`
- [ ] `impl From<KvError> for StrataError`
- [ ] `impl From<EventError> for StrataError`
- [ ] `impl From<StateError> for StrataError`
- [ ] `impl From<TraceError> for StrataError`
- [ ] `impl From<JsonError> for StrataError`
- [ ] `impl From<VectorError> for StrataError`
- [ ] `impl From<RunError> for StrataError`
- [ ] All error messages include EntityRef when applicable
- [ ] Error handling guidelines documented

---

## Epic 64: Conformance Testing ✅ COMPLETE

**Goal**: Verify all 7 primitives conform to all 7 invariants

**Status**: Completed in branch `milestone-9-phase-3` (commit ba85d89)

| Story | Description | Priority | Status |
|-------|-------------|----------|--------|
| #483 | Invariant 1-2 Conformance Tests (Addressable, Versioned) | CRITICAL | ✅ Done |
| #484 | Invariant 3-4 Conformance Tests (Transactional, Lifecycle) | CRITICAL | ✅ Done |
| #485 | Invariant 5-6 Conformance Tests (Run-scoped, Introspectable) | CRITICAL | ✅ Done |
| #486 | Invariant 7 Conformance Tests (Read/Write) | CRITICAL | ✅ Done |
| #487 | Cross-Primitive Transaction Conformance | CRITICAL | ✅ Done |

**Acceptance Criteria**:
- [x] 7 tests for Invariant 1: Each primitive has stable identity via EntityRef
- [x] 14 tests for Invariant 2: Each primitive read returns Versioned<T>, write returns Version
- [x] 7 tests for Invariant 3: Each primitive participates in transactions
- [x] 7 tests for Invariant 4: Each primitive follows create/exist/evolve/destroy lifecycle
- [x] 7 tests for Invariant 5: Each primitive is scoped to RunId
- [x] 7 tests for Invariant 6: Each primitive has exists() or equivalent
- [x] 6 tests for Invariant 7: Reads never modify, writes always produce versions
- [x] Cross-primitive atomic transaction test
- [x] Cross-primitive rollback test (failure rolls back all)
- [x] **62 conformance tests passing** (exceeds target of 49)

**Implementation Notes**:
- Conformance tests in `crates/primitives/tests/versioned_conformance_tests.rs`
- Tests organized by invariant module (invariant_1_addressable, invariant_2_versioned, etc.)
- Additional version monotonicity tests (3 tests)
- All tests pass in < 0.1s

---

## Files to Modify/Create

### Phase 1 Files (✅ Complete)

| File | Action | Description | Status |
|------|--------|-------------|--------|
| `crates/core/src/contract/mod.rs` | CREATE | Contract module | ✅ Done |
| `crates/core/src/contract/entity_ref.rs` | CREATE | EntityRef enum (DocRef unified) | ✅ Done |
| `crates/core/src/contract/versioned.rs` | CREATE | Versioned<T> wrapper | ✅ Done |
| `crates/core/src/contract/version.rs` | CREATE | Version enum | ✅ Done |
| `crates/core/src/contract/timestamp.rs` | CREATE | Timestamp type (microseconds) | ✅ Done |
| `crates/core/src/contract/run_name.rs` | CREATE | RunName type | ✅ Done |
| `crates/core/src/contract/primitive_type.rs` | CREATE | PrimitiveType enum | ✅ Done |
| `crates/core/src/lib.rs` | MODIFY | Export new types | ✅ Done |

### Remaining Files (Phases 2-5) ✅ COMPLETE

| File | Action | Description | Status |
|------|--------|-------------|--------|
| `crates/primitives/src/kv.rs` | MODIFY | Versioned returns | ✅ Done |
| `crates/primitives/src/event_log.rs` | MODIFY | Versioned returns | ✅ Done |
| `crates/primitives/src/state_cell.rs` | MODIFY | Versioned returns | ✅ Done |
| `crates/primitives/src/trace.rs` | MODIFY | Versioned returns | ✅ Done |
| `crates/primitives/src/json_store.rs` | MODIFY | Versioned returns | ✅ Done |
| `crates/primitives/src/vector/store.rs` | MODIFY | Versioned returns | ✅ Done |
| `crates/primitives/src/run_index.rs` | MODIFY | Versioned returns | ✅ Done |
| `crates/primitives/src/extensions.rs` | MODIFY | Extension traits | ✅ Done |
| `crates/engine/src/transaction/context.rs` | MODIFY | Implement extensions | ✅ Done |
| `crates/primitives/src/run_handle.rs` | CREATE | RunHandle pattern | ✅ Done |
| `crates/primitives/tests/versioned_conformance_tests.rs` | CREATE | 62 conformance tests | ✅ Done |

---

## Dependency Order

```
Epic 60 (Core Types)
    ↓
Epic 61 (Versioned Returns) ←── Epic 60
    ↓
Epic 62 (Transaction Unification) ←── Epic 60
    ↓
Epic 63 (Error Standardization) ←── Epic 60
    ↓
Epic 64 (Conformance Testing) ←── Epic 61, 62, 63
```

**Recommended Implementation Order**:
1. Epic 60: Core Types (foundation for everything)
2. Epic 63: Error Standardization (needed for return types)
3. Epic 61: Versioned Returns (most impactful change)
4. Epic 62: Transaction Unification (builds on versioned returns)
5. Epic 64: Conformance Testing (validates everything)

---

## Phased Implementation Strategy

> **Guiding Principle**: Do not try to convert all 7 primitives in one pass.
>
> This is a lot of mechanical change: signature updates everywhere, test rewrites, plumbing work.
> Protect momentum by proving the pattern works before generalizing.

### Phase 1: Foundation (Epic 60 Complete) ✅ DONE

Implement all core types fully:
- `EntityRef`, `Versioned<T>`, `Version`, `Timestamp`, `PrimitiveType`, `RunName`
- All types tested independently

**Exit Criteria**: All 6 stories in Epic 60 complete. ~~All 4 stories in Epic 63 complete.~~

**Completed**: 2026-01-19
- Branch: `milestone-9-phase-1`
- Commit: `f8df454`
- Files: 67 changed, +3333/-1119 lines
- Tests: All library tests pass (1500+), all integration tests pass (445+)

**Note**: Epic 63 (Error Standardization) deferred to Phase 2 - core types implemented first following in-place migration strategy.

### Phase 2: Error Standardization + First Two Primitives (KV + EventLog) ✅ DONE

**Status**: Completed in branch `milestone-9-phase-3` (commit f6ecb68)

**Epic 63: Error Standardization** (prerequisite for versioned returns):
- ✅ #479: StrataError Enum Definition
- ✅ #480: Error Conversion from Primitive Errors
- ✅ #481: EntityRef in Error Messages
- ✅ #482: Error Documentation and Guidelines

Apply versioned returns to **KV** and **EventLog** only:
- ✅ #466: KVStore Versioned Returns
- ✅ #467: EventLog Versioned Returns

Wire TransactionOps for these two:
- ✅ #473: TransactionOps Trait Definition
- ✅ #474: KV Operations in TransactionOps
- ✅ #475: Event Operations in TransactionOps

### Phase 3: Extend to State + Trace ✅ DONE

**Status**: Completed in branch `milestone-9-phase-3` (commit ba85d89)

Apply the proven pattern:
- ✅ #468: StateCell Versioned Returns
- ✅ #469: TraceStore Versioned Returns
- ✅ #476: State/Trace Operations in TransactionOps

### Phase 4: Complete Remaining Primitives ✅ DONE

**Status**: Completed in branch `milestone-9-phase-3` (commit ba85d89)

Apply to Json, Vector, RunIndex:
- ✅ #470: JsonStore Versioned Returns
- ✅ #471: VectorStore Versioned Returns
- ✅ #472: RunIndex Versioned Returns
- ✅ #477: Json/Vector Operations in TransactionOps

### Phase 5: Finalize ✅ DONE

**Status**: Completed in branch `milestone-9-phase-3` (commit ba85d89)

- ✅ #478: RunHandle Pattern Implementation
- ✅ #487: Cross-Primitive Transaction Conformance
- ✅ Final conformance test sweep (62 tests - exceeds target of 49)

**Exit Criteria Met**: M9 complete. API stable.

### Phase Summary

| Phase | Primitives | Epics/Stories | Conformance Tests | Status |
|-------|------------|---------------|-------------------|--------|
| 1 | (types only) | Epic 60 | Unit tests only | ✅ DONE |
| 2 | KV, EventLog | 61 (partial), 62 (partial), 63 | 28 tests | ✅ DONE |
| 3 | + State, Trace | 61 (partial), 62 (partial) | + 28 tests | ✅ DONE |
| 4 | + Json, Vector, Run | 61 (complete), 62 (partial) | + 42 tests | ✅ DONE |
| 5 | (finalize) | 62 (complete), 64 | + cross-primitive | ✅ DONE |

**Final Status**: All 5 phases complete. 62 conformance tests passing. M9 API Stabilization complete.

**Benefits Realized**:
1. **Early validation**: Pattern proven with KV + EventLog before generalizing
2. **Maintained momentum**: Each phase completed incrementally
3. **Reduced risk**: Design issues caught and resolved early
4. **Easier debugging**: Smaller scope per phase simplified troubleshooting
5. **Visible progress**: Clear milestone achievements throughout

---

## Migration Strategy

### Phase 1: Add Types (Non-Breaking)

Add new types without changing existing APIs:
- `EntityRef`, `Versioned<T>`, `Version`, `StrataError`
- Existing code continues to work

### Phase 2: Add Versioned APIs (Parallel)

Add new methods alongside existing:
- `kv.get_versioned()` alongside `kv.get()`
- Mark old methods `#[deprecated]`

### Phase 3: Switch Callers

Update all internal code to use new methods:
- Update all tests
- Update all examples

### Phase 4: Remove Deprecated

Remove old API surface:
- Remove `#[deprecated]` methods
- Finalize documentation

---

## Success Metrics

**Functional**: All 28 stories passing, 100% acceptance criteria met

**Correctness**:
- All 49 conformance tests passing (7 primitives × 7 invariants)
- Cross-primitive transaction tests passing
- No primitive-specific special cases

**API Quality**:
- All reads return `Versioned<T>`
- All writes return `Version`
- All primitives in `TransactionOps` trait
- `StrataError` used everywhere

**Documentation**:
- PRIMITIVE_CONTRACT.md finalized
- CORE_API_SHAPE.md finalized
- Migration guide complete

**Quality**: Test coverage > 90% for new code

---

## Risk Mitigation

| Risk | Likelihood | Impact | Mitigation |
|------|------------|--------|------------|
| Breaking existing code | High | Medium | Phased migration, deprecated markers |
| Over-specification | Medium | Medium | Keep invariants minimal |
| Test maintenance burden | Medium | Low | Clear test organization |
| Performance regression | Low | Low | Benchmark before/after |
| Scope creep | Medium | Medium | Strict "stabilization only" rule |

---

## Not In Scope (Explicitly Deferred)

1. **Wire protocol** - M10 (transport layer)
2. **Server implementation** - M10
3. **Performance optimization** - M11
4. **Python SDK** - M12
5. **New primitives** - Post-MVP
6. **Advanced introspection (diff, history, explain)** - Post-MVP (Magic APIs)
7. **EntityRef sub-addressing** - Post-MVP
8. **Causal version tracking** - Post-MVP
9. **Cross-run references** - Post-MVP

---

## Post-M9 Expectations

After M9 completion:
1. API is stable and documented
2. All 7 primitives conform to all 7 invariants
3. Server (M10) can build on stable foundations
4. Python SDK (M12) has clear API to target
5. Future primitives have clear conformance requirements
6. Users have one mental model for all primitives
7. Version information is never optional or lost

---

## Testing Strategy

### Unit Tests
- EntityRef construction and methods
- Versioned<T> map and transformation
- Version comparison and serialization
- Error conversion from all primitive errors

### Integration Tests
- Versioned returns from all primitives
- TransactionOps with all primitive operations
- RunHandle pattern usage
- Error propagation across layers

### Conformance Tests
- 49 invariant conformance tests (organized by primitive and invariant)
- Cross-primitive transaction atomicity
- Cross-primitive rollback safety

### Migration Tests
- Deprecated APIs still work during migration
- New APIs produce same values as old
- Gradual migration path is viable
