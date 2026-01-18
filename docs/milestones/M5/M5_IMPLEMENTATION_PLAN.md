# M5 Implementation Plan: JSON Primitive

## Overview

This document provides the high-level implementation plan for M5 (JSON Primitive).

**Total Scope**: 7 Epics, 32 Stories

**References**:
- [M5 Architecture Specification](../../architecture/M5_ARCHITECTURE.md)
- [M5 Integration Analysis](./M5_INTEGRATION_ANALYSIS.md)

**Epic Details**:
- [Epic 26: Core Types Foundation](./EPIC_26_CORE_TYPES.md)
- [Epic 27: Path Operations](./EPIC_27_PATH_OPERATIONS.md)
- [Epic 28: JsonStore Core](./EPIC_28_JSONSTORE_CORE.md)
- [Epic 29: WAL Integration](./EPIC_29_WAL_INTEGRATION.md)
- [Epic 30: Transaction Integration](./EPIC_30_TRANSACTION_INTEGRATION.md)
- [Epic 31: Conflict Detection](./EPIC_31_CONFLICT_DETECTION.md)
- [Epic 32: Validation & Non-Regression](./EPIC_32_VALIDATION.md)

---

## Architectural Integration Rules (NON-NEGOTIABLE)

These rules ensure M5 integrates properly with the M1-M4 architecture.

### Rule 1: JSON Lives Inside ShardedStore

Documents stored as: `Key { namespace, TypeTag::Json, doc_id_bytes } -> VersionedValue`

This gives us sharding, versioning, snapshots, WAL, recovery FOR FREE.

**FORBIDDEN**: Separate DashMap storage

### Rule 2: JsonStore Is a Stateless Facade

`pub struct JsonStore { db: Arc<Database> }` - ONLY this.

No internal maps, locks, or state. Exactly like KVStore, EventLog, StateCell, Trace, RunIndex.

### Rule 3: JSON Extends TransactionContext, Not Replaces It

Uses JsonStoreExt trait on TransactionContext. No separate JsonTransactionState.

### Rule 4: Path-Level Semantics Live in Validation, Not Storage

Storage sees: `Key::new_json(namespace, doc_id) -> VersionedValue`
Validation sees: `(Key, JsonPath) -> version`

### Rule 5: WAL Remains Unified

New entry variants (0x20-0x23) added to existing WALEntry enum.

### Rule 6: JSON API Must Feel Like Every Other Primitive

```rust
json.get(&run_id, &doc_id, &path)?
json.transaction(&run_id, |txn| { txn.json_set(...) })?
```

---

## Critical Invariants

1. **Path Semantics Are Positional**: `$.items[0]` refers to position, not stable identity
2. **Mutations Are Path-Based**: WAL records patches, never full documents
3. **Conflict Detection Is Region-Based**: Paths conflict if they overlap
4. **Weak Snapshot Isolation**: Stale reads fail rather than return old data
5. **Non-Regression**: M5 must not degrade M4 primitive performance

---

## Epic Overview

| Epic | Name | Stories | Dependencies |
|------|------|---------|--------------|
| 26 | Core Types Foundation | 5 | M4 complete |
| 27 | Path Operations | 4 | Epic 26 |
| 28 | JsonStore Core | 6 | Epic 27 |
| 29 | WAL Integration | 4 | Epic 28 |
| 30 | Transaction Integration | 5 | Epic 28 |
| 31 | Conflict Detection | 4 | Epic 30 |
| 32 | Validation & Non-Regression | 4 | All others |

---

## Epic 26: Core Types Foundation

**Goal**: Define core JSON types that lock in semantics

| Story | Description | Priority |
|-------|-------------|----------|
| #225 | JsonDocId Type Definition | FOUNDATION |
| #226 | JsonValue Type Definition | FOUNDATION |
| #227 | JsonPath Type Definition | FOUNDATION |
| #228 | JsonPatch Type Definition | HIGH |
| #229 | Document Size Limits | HIGH |

**Acceptance Criteria**:
- [ ] TypeTag::Json = 0x11 added to types.rs
- [ ] Key::new_json() implemented
- [ ] JsonDocId generates unique, hashable identifiers
- [ ] JsonValue represents all JSON types with IndexMap for objects
- [ ] JsonValue implements From<Value> and Into<Value>
- [ ] JsonPath supports parsing, display, and overlap detection
- [ ] JsonPatch defines Set and Delete operations
- [ ] Size limits enforced: 16MB doc, 100 depth, 256 path segments, 1M array elements

---

## Epic 27: Path Operations

**Goal**: Implement path traversal and manipulation

| Story | Description | Priority |
|-------|-------------|----------|
| #230 | Path Traversal (Get) | CRITICAL |
| #231 | Path Mutation (Set) | CRITICAL |
| #232 | Path Deletion | CRITICAL |
| #233 | Intermediate Path Creation | HIGH |

**Acceptance Criteria**:
- [ ] `get_at_path()` navigates objects and arrays correctly
- [ ] `set_at_path()` creates intermediate structures as needed
- [ ] `delete_at_path()` removes values and cleans up empty containers
- [ ] Type mismatches return appropriate errors
- [ ] Root path operations work correctly

---

## Epic 28: JsonStore Core

**Goal**: Implement the JsonStore facade with full API

| Story | Description | Priority |
|-------|-------------|----------|
| #234 | JsonDoc Internal Structure | FOUNDATION |
| #235 | JsonStore Struct Definition (Stateless Facade) | FOUNDATION |
| #236 | Document Create/Delete | CRITICAL |
| #237 | Document Get/Set/Delete at Path | CRITICAL |
| #238 | Document Exists/List | HIGH |
| #239 | Serialization | HIGH |

**Acceptance Criteria**:
- [ ] JsonStore holds ONLY `Arc<Database>` (stateless facade)
- [ ] JsonDoc stores value, version, created_at, updated_at
- [ ] Documents stored via unified Key::new_json() in ShardedStore
- [ ] Fast path reads use SnapshotView
- [ ] CRUD operations work correctly
- [ ] Version increments on every mutation
- [ ] Serialization roundtrips correctly
- [ ] Document size validated on storage

---

## Epic 29: WAL Integration

**Goal**: Integrate JSON operations with write-ahead logging

| Story | Description | Priority |
|-------|-------------|----------|
| #240 | JSON WAL Entry Types (0x20-0x23) | CRITICAL |
| #241 | WAL Write for JSON Operations | CRITICAL |
| #242 | WAL Replay for JSON | CRITICAL |
| #243 | Idempotent Replay Logic | HIGH |

**Acceptance Criteria**:
- [ ] WAL entry types 0x20-0x23 defined and serializable
- [ ] WAL entries use unified Key (not JsonDocId directly)
- [ ] All JSON mutations write WAL entries before storage
- [ ] WAL replay reconstructs document state correctly
- [ ] Replay is idempotent (version check skips already-applied entries)
- [ ] Patches never include full documents

---

## Epic 30: Transaction Integration

**Goal**: Integrate JSON operations with transaction system

| Story | Description | Priority |
|-------|-------------|----------|
| #244 | JSON Path Read/Patch Types | FOUNDATION |
| #245 | Lazy Set Initialization | CRITICAL |
| #246 | JsonStoreExt Trait Implementation | CRITICAL |
| #247 | Snapshot Version Capture | HIGH |
| #248 | Cross-Primitive Transactions | HIGH |

**Acceptance Criteria**:
- [ ] JsonPathRead and JsonPatchEntry types defined
- [ ] TransactionContext extended with `Option<Vec<...>>` fields (lazy)
- [ ] JsonStoreExt trait implemented on TransactionContext
- [ ] Lazy allocation on first JSON operation (zero overhead for non-JSON txns)
- [ ] Snapshot captures document versions at transaction start
- [ ] JSON + KV/Event/State in same transaction works atomically

---

## Epic 31: Conflict Detection

**Goal**: Implement region-based conflict detection for JSON

| Story | Description | Priority |
|-------|-------------|----------|
| #249 | Path Overlap Detection | CRITICAL |
| #250 | Read-Write Conflict Check | CRITICAL |
| #251 | Write-Write Conflict Check | CRITICAL |
| #252 | Conflict Integration with Commit | HIGH |

**Acceptance Criteria**:
- [ ] `overlaps()` correctly identifies ancestor/descendant/equal paths
- [ ] Read at path X conflicts with write at path Y if X.overlaps(Y)
- [ ] Write at path X conflicts with write at path Y if X.overlaps(Y)
- [ ] Conflict detection integrated into existing validation pipeline
- [ ] Version mismatch (stale read) detected and reported

---

## Epic 32: Validation & Non-Regression

**Goal**: Ensure correctness and maintain M4 performance

| Story | Description | Priority |
|-------|-------------|----------|
| #253 | JSON Unit Tests | CRITICAL |
| #254 | JSON Integration Tests | CRITICAL |
| #255 | Non-Regression Benchmark Suite | CRITICAL |
| #256 | Performance Baseline Documentation | HIGH |

**Acceptance Criteria**:
- [ ] Unit tests cover all path operations and edge cases
- [ ] Integration tests verify WAL replay and transactions
- [ ] KV, Event, State, Trace maintain M4 latency targets
- [ ] JSON operations meet performance baselines
- [ ] No memory leaks detected under load
- [ ] Cross-primitive transaction tests pass

---

## Files to Modify/Create

| File | Action | Description |
|------|--------|-------------|
| `crates/core/src/types.rs` | MODIFY | Add TypeTag::Json = 0x11, Key::new_json() |
| `crates/core/src/json_types.rs` | CREATE | JsonDocId, JsonValue, JsonPath, JsonPatch |
| `crates/primitives/src/json_store.rs` | CREATE | Stateless JsonStore facade |
| `crates/primitives/src/extensions.rs` | MODIFY | Add JsonStoreExt trait |
| `crates/concurrency/src/transaction.rs` | MODIFY | Add lazy JSON tracking fields |
| `crates/durability/src/wal.rs` | MODIFY | Add JSON WAL entry variants |

---

## Success Metrics

**Functional**: All 32 stories passing, 100% acceptance criteria met

**Performance**:
- JSON create: < 1ms for 1KB document
- JSON get at path: < 100μs for 1KB document
- JSON set at path: < 1ms for 1KB document
- KV/Event/State/Trace: No regression from M4

**Quality**: Test coverage > 90%, no memory leaks under 24-hour stress test
