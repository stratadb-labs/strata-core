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

| Epic | Name | Stories | Dependencies |
|------|------|---------|--------------|
| 60 | Core Types | 6 | M8 complete |
| 61 | Versioned Returns | 7 | Epic 60 |
| 62 | Transaction Unification | 6 | Epic 60 |
| 63 | Error Standardization | 4 | Epic 60 |
| 64 | Conformance Testing | 5 | Epic 61, 62, 63 |

---

## Epic 60: Core Types

**Goal**: Define universal types that express the seven invariants

| Story | Description | Priority |
|-------|-------------|----------|
| #460 | EntityRef Enum Implementation | FOUNDATION |
| #461 | Versioned<T> Wrapper Type | FOUNDATION |
| #462 | Version Enum (TxnId, Sequence, Counter) | FOUNDATION |
| #463 | Timestamp Type | FOUNDATION |
| #464 | PrimitiveType Enum | HIGH |
| #465 | RunId Standardization | FOUNDATION |

**Acceptance Criteria**:
- [ ] `EntityRef` enum with variants for all 7 primitives
- [ ] `EntityRef::run_id()` method returns the run for any entity
- [ ] `EntityRef::primitive_type()` method returns `PrimitiveType`
- [ ] `Versioned<T>` with value, version, timestamp fields
- [ ] `Versioned<T>::map()` for transforming inner value
- [ ] `Versioned<T>::into_value()` (deprecated helper for migration)
- [ ] `Version` enum: TxnId(u64), Sequence(u64), Counter(u64)
- [ ] `Version::as_u64()` for numeric comparison
- [ ] `Timestamp` type with `now()` constructor
- [ ] `RunId` newtype with `new()`, `as_str()`, Display impl
- [ ] All types implement Debug, Clone; IDs implement Hash, Eq

---

## Epic 61: Versioned Returns

**Goal**: Wrap all read returns in Versioned<T>, all writes return Version

| Story | Description | Priority |
|-------|-------------|----------|
| #466 | KVStore Versioned Returns | CRITICAL |
| #467 | EventLog Versioned Returns | CRITICAL |
| #468 | StateCell Versioned Returns | CRITICAL |
| #469 | TraceStore Versioned Returns | CRITICAL |
| #470 | JsonStore Versioned Returns | CRITICAL |
| #471 | VectorStore Versioned Returns | CRITICAL |
| #472 | RunIndex Versioned Returns | CRITICAL |

**Acceptance Criteria**:
- [ ] `kv.get()` returns `Option<Versioned<Value>>`
- [ ] `kv.put()` returns `Version`
- [ ] `events.read()` returns `Option<Versioned<Event>>`
- [ ] `events.append()` returns `Version`
- [ ] `state.read()` returns `Option<Versioned<StateValue>>`
- [ ] `state.set()` returns `Version`
- [ ] `traces.read()` returns `Option<Versioned<Trace>>`
- [ ] `traces.record()` returns `Versioned<TraceId>` (includes the new trace_id)
- [ ] `json.get()` returns `Option<Versioned<JsonValue>>`
- [ ] `json.set()` returns `Version`
- [ ] `vector.get()` returns `Option<Versioned<VectorEntry>>`
- [ ] `vector.upsert()` returns `Version`
- [ ] `runs.get()` returns `Option<Versioned<RunMetadata>>`
- [ ] `runs.create()` returns `Version`
- [ ] All existing tests updated to expect versioned returns
- [ ] Migration helpers provided for gradual adoption

---

## Epic 62: Transaction Unification

**Goal**: Unified TransactionOps trait covering all primitives

| Story | Description | Priority |
|-------|-------------|----------|
| #473 | TransactionOps Trait Definition | FOUNDATION |
| #474 | KV Operations in TransactionOps | CRITICAL |
| #475 | Event Operations in TransactionOps | CRITICAL |
| #476 | State/Trace Operations in TransactionOps | CRITICAL |
| #477 | Json/Vector Operations in TransactionOps | CRITICAL |
| #478 | RunHandle Pattern Implementation | HIGH |

**Acceptance Criteria**:
- [ ] `TransactionOps` trait with all primitive operations
- [ ] Reads take `&self`, writes take `&mut self`
- [ ] All methods return `Result<T>` with `StrataError`
- [ ] KV: `kv_get`, `kv_put`, `kv_delete`, `kv_exists`
- [ ] Event: `event_append`, `event_read`, `event_range`
- [ ] State: `state_read`, `state_set`, `state_cas`, `state_delete`, `state_exists`
- [ ] Trace: `trace_record`, `trace_read`
- [ ] Json: `json_create`, `json_get`, `json_get_path`, `json_set`, `json_delete`, `json_exists`
- [ ] Vector: `vector_upsert`, `vector_get`, `vector_delete`, `vector_search`
- [ ] `RunHandle` provides scoped access to primitives
- [ ] `RunHandle::kv()`, `events()`, `state()`, `traces()`, `json()`, `vectors()`
- [ ] `RunHandle::transaction()` for atomic operations
- [ ] Cross-primitive transaction works: KV + Event + State + Trace + Json + Vector

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

## Epic 64: Conformance Testing

**Goal**: Verify all 7 primitives conform to all 7 invariants

| Story | Description | Priority |
|-------|-------------|----------|
| #483 | Invariant 1-2 Conformance Tests (Addressable, Versioned) | CRITICAL |
| #484 | Invariant 3-4 Conformance Tests (Transactional, Lifecycle) | CRITICAL |
| #485 | Invariant 5-6 Conformance Tests (Run-scoped, Introspectable) | CRITICAL |
| #486 | Invariant 7 Conformance Tests (Read/Write) | CRITICAL |
| #487 | Cross-Primitive Transaction Conformance | CRITICAL |

**Acceptance Criteria**:
- [ ] 7 tests for Invariant 1: Each primitive has stable identity via EntityRef
- [ ] 14 tests for Invariant 2: Each primitive read returns Versioned<T>, write returns Version
- [ ] 7 tests for Invariant 3: Each primitive participates in transactions
- [ ] 7 tests for Invariant 4: Each primitive follows create/exist/evolve/destroy lifecycle
- [ ] 7 tests for Invariant 5: Each primitive is scoped to RunId
- [ ] 7 tests for Invariant 6: Each primitive has exists() or equivalent
- [ ] 7 tests for Invariant 7: Reads never modify, writes always produce versions
- [ ] Cross-primitive atomic transaction test (all 7 primitives)
- [ ] Cross-primitive rollback test (failure rolls back all)
- [ ] All 49 conformance tests passing
- [ ] Test coverage > 90% for new code

---

## Files to Modify/Create

| File | Action | Description |
|------|--------|-------------|
| `crates/core/src/entity_ref.rs` | CREATE | EntityRef enum |
| `crates/core/src/versioned.rs` | CREATE | Versioned<T> wrapper |
| `crates/core/src/version.rs` | CREATE | Version enum |
| `crates/core/src/timestamp.rs` | CREATE | Timestamp type |
| `crates/core/src/run_id.rs` | MODIFY | Standardize RunId |
| `crates/core/src/primitive_type.rs` | CREATE | PrimitiveType enum |
| `crates/core/src/lib.rs` | MODIFY | Export new types |
| `crates/core/src/error.rs` | MODIFY | Add StrataError |
| `crates/primitives/src/kv_store.rs` | MODIFY | Versioned returns |
| `crates/primitives/src/event_log.rs` | MODIFY | Versioned returns |
| `crates/primitives/src/state_cell.rs` | MODIFY | Versioned returns |
| `crates/primitives/src/trace_store.rs` | MODIFY | Versioned returns |
| `crates/primitives/src/json_store.rs` | MODIFY | Versioned returns |
| `crates/primitives/src/vector/store.rs` | MODIFY | Versioned returns |
| `crates/primitives/src/run_index.rs` | MODIFY | Versioned returns |
| `crates/engine/src/transaction.rs` | MODIFY | TransactionOps trait |
| `crates/engine/src/run_handle.rs` | CREATE | RunHandle pattern |
| `crates/engine/src/database.rs` | MODIFY | Wire new patterns |
| `tests/conformance/` | CREATE | Conformance test suite |

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

### Phase 1: Foundation (Epic 60 Complete)

Implement all core types fully:
- `EntityRef`, `Versioned<T>`, `Version`, `Timestamp`, `PrimitiveType`, `RunId`
- `StrataError` enum with all variants
- All types tested independently

**Exit Criteria**: All 6 stories in Epic 60 complete. All 4 stories in Epic 63 complete.

### Phase 2: First Two Primitives (KV + EventLog)

Apply versioned returns to **KV** and **EventLog** only:
- #466: KVStore Versioned Returns
- #467: EventLog Versioned Returns

Wire TransactionOps for these two:
- #473: TransactionOps Trait Definition
- #474: KV Operations in TransactionOps
- #475: Event Operations in TransactionOps

Write conformance tests for KV and EventLog (14 tests each = 28 tests).

**Exit Criteria**: KV and EventLog fully conform to all 7 invariants. Pattern proven.

### Phase 3: Extend to State + Trace

Apply the proven pattern:
- #468: StateCell Versioned Returns
- #469: TraceStore Versioned Returns
- #476: State/Trace Operations in TransactionOps

Write conformance tests (14 tests each = 28 tests).

**Exit Criteria**: 4 primitives fully conformant.

### Phase 4: Complete Remaining Primitives

Apply to Json, Vector, RunIndex:
- #470: JsonStore Versioned Returns
- #471: VectorStore Versioned Returns
- #472: RunIndex Versioned Returns
- #477: Json/Vector Operations in TransactionOps

Write conformance tests (14 tests each = 42 tests).

**Exit Criteria**: All 7 primitives fully conformant.

### Phase 5: Finalize

- #478: RunHandle Pattern Implementation
- #487: Cross-Primitive Transaction Conformance
- Final conformance test sweep (49 tests)
- Documentation update

**Exit Criteria**: M9 complete. API stable.

### Phase Summary

| Phase | Primitives | Epics/Stories | Conformance Tests |
|-------|------------|---------------|-------------------|
| 1 | (types only) | Epic 60, 63 | Unit tests only |
| 2 | KV, EventLog | 61 (partial), 62 (partial) | 28 tests |
| 3 | + State, Trace | 61 (partial), 62 (partial) | + 28 tests |
| 4 | + Json, Vector, Run | 61 (complete), 62 (partial) | + 42 tests |
| 5 | (finalize) | 62 (complete), 64 | + cross-primitive |

**Benefits of Phased Approach**:
1. **Early validation**: Prove pattern works with 2 primitives before committing to 7
2. **Maintained momentum**: Smaller batches, frequent completions
3. **Reduced risk**: Catch design issues early
4. **Easier debugging**: When something breaks, fewer variables
5. **Visible progress**: Each phase is a milestone

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
