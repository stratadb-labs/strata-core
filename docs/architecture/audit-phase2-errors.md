# Phase 2a: Error Handling Consistency Audit

Date: 2026-02-04
Status: Complete

## Summary

The codebase has a well-designed canonical error type (`StrataError`) with 21 variants mapping to 10 wire codes, comprehensive constructors, and strong test coverage. The primary concern is **incomplete conversion chains** for low-level durability errors and **information loss** in some error conversions.

**MVP Readiness: CONDITIONAL** — 3 blocking issues identified.

---

## 1. Error Type Inventory

### Canonical: `StrataError` (strata-core)

**File**: `crates/core/src/error.rs` (2,462 lines)

21 variants organized by category:

| Category | Variants | Wire Code |
|----------|----------|-----------|
| Not Found (2) | `NotFound`, `BranchNotFound` | `NotFound` |
| Type (1) | `WrongType` | `WrongType` |
| Conflict (6) | `Conflict`, `VersionConflict`, `WriteConflict`, `TransactionAborted`, `TransactionTimeout`, `TransactionNotActive` | `Conflict` |
| Validation (4) | `InvalidOperation`, `InvalidInput`, `DimensionMismatch`, `PathNotFound` | `ConstraintViolation` / `InvalidPath` |
| History (1) | `HistoryTrimmed` | `HistoryTrimmed` |
| Storage (3) | `Storage`, `Serialization`, `Corruption` | `StorageError` / `SerializationError` |
| Resource (2) | `CapacityExceeded`, `BudgetExceeded` | `ConstraintViolation` |
| Internal (1) | `Internal` | `InternalError` |

All 21 variants exhaustively mapped to 10 wire codes via `code()` method (lines 1123-1160).

### Secondary Error Types

| Type | Crate | Variants | From → StrataError |
|------|-------|----------|-------------------|
| `executor::Error` | executor | 30+ | StrataError → Error (reverse direction) |
| `CommitError` | concurrency | 4 | Yes — with source loss |
| `BranchError` | engine/recovery | 5 | Yes — clean |
| `VectorError` | engine/primitives | 16 | Yes — with placeholder BranchId |

### Low-Level Error Types (durability)

| Type | Variants | From → StrataError |
|------|----------|-------------------|
| `SnapshotError` | 8 | **NO** |
| `DatabaseHandleError` | 6 | **NO** |
| `ManifestError` | 5 | **NO** |
| `CodecError` | 3 | **NO** |
| `WalReaderError` | 3 | **NO** |
| `WalRecordError` | 4 | **NO** |
| `WritesetError` | 3 | **NO** |
| `CompactionError` | 5+ | **NO** |
| `CheckpointError` | 5+ | **NO** |
| `WalConfigError` | 3 | **NO** |
| `RetentionPolicyError` | Various | **NO** |
| `BranchBundleError` | 10 | **NO** |

**12+ durability error types lack `From` impls to `StrataError`**.

---

## 2. Conversion Chain Analysis

### Complete Error Flow

```
StrataError (canonical, 10 wire codes)
  ├── ← CommitError (concurrency)      — partial info loss
  ├── ← BranchError (engine/recovery)  — clean
  ├── ← VectorError (engine/vector)    — placeholder BranchId
  ├── ← io::Error                      — clean
  ├── ← bincode::Error                 — clean
  └── ← serde_json::Error              — clean

executor::Error (API boundary)
  └── ← StrataError                    — Version type → u64 conversion

[NO conversion to StrataError]
  ├── SnapshotError
  ├── DatabaseHandleError
  ├── ManifestError, CodecError
  ├── WalReaderError, WalRecordError
  ├── CompactionError, CheckpointError
  └── BranchBundleError
```

### Information Loss Points

#### Critical: VectorError → StrataError

```rust
// crates/engine/src/primitives/vector/error.rs:149-209
let placeholder_branch_id = BranchId::new();  // ← Fake BranchId
EntityRef::vector(placeholder_branch_id, name, "")
```

Creates a placeholder BranchId when converting `VectorError::CollectionNotFound`. This defeats branch-level error tracking.

#### Significant: CommitError → StrataError

```rust
// crates/concurrency/src/transaction.rs:66-83
CommitError::WALError(msg) → StrataError::Storage {
    message: format!("WAL error: {}", msg),
    source: None,  // ← Original error source LOST
}
```

The `source` field exists on `StrataError::Storage` but is set to `None`, losing the error chain.

#### Significant: StrataError → executor::Error

```rust
// crates/executor/src/convert.rs:42-53
StrataError::VersionConflict { expected, actual, .. } → Error::VersionConflict {
    expected: version_to_u64(&expected),   // ← Type info lost
    actual: version_to_u64(&actual),       // ← Txn/Sequence/Counter distinction lost
}
```

Version enum variant information reduced to u64 + string name.

---

## 3. Wire Encoding Coverage

All 21 `StrataError` variants map to wire codes — **complete coverage**.

| Wire Code | Mapped Variants |
|-----------|----------------|
| `NotFound` | NotFound, BranchNotFound |
| `WrongType` | WrongType |
| `Conflict` | Conflict, VersionConflict, WriteConflict, TransactionAborted, TransactionTimeout, TransactionNotActive |
| `ConstraintViolation` | InvalidOperation, InvalidInput, DimensionMismatch, CapacityExceeded, BudgetExceeded |
| `InvalidPath` | PathNotFound |
| `HistoryTrimmed` | HistoryTrimmed |
| `StorageError` | Storage, Corruption |
| `SerializationError` | Serialization |
| `InternalError` | Internal |
| `InvalidKey` | **Defined but never mapped** — unused wire code |

**Gap**: `InvalidKey` wire code exists but no `StrataError` variant maps to it.

---

## 4. Error Construction Quality

**Strengths**:
- 21 typed constructor methods (e.g., `StrataError::not_found(EntityRef)`)
- All constructors documented with doc-test examples
- Structured `ErrorDetails` with type-safe key-value pairs
- Classification methods: `is_retryable()`, `is_serious()`, `is_conflict()`, `is_validation_error()`
- Entity reference tracking via `entity_ref()` and `branch_id()`

**Issues**:
- `ConstraintReason` enum (18 variants) is defined but never used — `InvalidOperation` takes a `String` reason instead
- Generic `Conflict(String)` variant has `Option<EntityRef>` but is not always populated

---

## 5. Error Testing Coverage

| Crate | Error Tests | Assessment |
|-------|------------|------------|
| strata-core | 100+ tests | Excellent — all constructors, wire codes, details, classification |
| strata-executor | 5 tests | Gap — only 5 of 30+ conversion variants tested |
| strata-engine (vector) | 4 tests | Minimal — VectorError→StrataError conversion not explicitly tested |
| strata-concurrency | 0 tests | Gap — CommitError→StrataError conversion untested |
| strata-durability | 3 tests | Minimal — only BranchBundleError tested |

---

## 6. Inconsistencies

1. **thiserror vs manual**: Most types use `#[derive(thiserror::Error)]` but `CommitError` uses manual `Display` impl
2. **`#[from]` usage**: `DatabaseHandleError` uses `#[from]` for nested types; `BranchBundleError` does not
3. **DimensionMismatch dual mapping**: Both `StrataError::DimensionMismatch` and `VectorError::DimensionMismatch` exist with different conversion paths

---

## 7. Recommendations

### MVP Blocking

1. **Add `From` impls for durability errors** — Ensure all error types that can reach the API boundary have a conversion path to `StrataError`. At minimum: `SnapshotError`, `DatabaseHandleError`, `ManifestError`, `CodecError`, `WalReaderError`.

2. **Fix VectorError placeholder BranchId** — Add `branch_id: Option<BranchId>` context to `VectorError` variants that need it, or pass branch context through the conversion call sites.

3. **Preserve error source in WAL conversion** — Change `CommitError::WALError` conversion to set `source: Some(Box::new(original_error))` instead of `None`.

### Post-MVP

4. **Test executor Error conversions** — Add tests for all 30+ variants, not just 5.
5. **Use `ConstraintReason` enum** — Replace `String` reason in `InvalidOperation` with the typed enum.
6. **Standardize on `thiserror` derive** for `CommitError`.
7. **Remove or use `InvalidKey` wire code** — it's defined but never mapped.

---

## Methodology

Read all error type definitions, `From` implementations, wire code mapping (`code()` method), error construction patterns, and test suites across all 8 crates. Traced conversion chains from low-level errors through to API boundary.
