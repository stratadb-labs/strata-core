# Phase 1b: Unsafe Code Audit

Date: 2026-02-04
Status: Complete

## Summary

**4 unsafe instances** across 2 files. All are **SOUND** with **LOW** risk.

**MVP Readiness: PASS** — no soundness issues found.

---

## Complete Inventory

### Instance 1: Immutable pointer cast — `json.rs:1028`

**File**: `crates/core/src/primitives/json.rs:1028`
**Code**: `unsafe { &*(current as *const serde_json::Value as *const JsonValue) }`

| Aspect | Assessment |
|--------|------------|
| **Purpose** | Cast `&serde_json::Value` to `&JsonValue` at the end of JSON path traversal |
| **Safety basis** | `JsonValue` is `#[repr(transparent)]` wrapping `serde_json::Value` — identical memory layout guaranteed |
| **Aliasing** | Immutable reference only; no aliasing concerns |
| **Lifetime** | Tied to input lifetime `<'a>` — correct |
| **SAFETY comment** | Present and detailed (3 invariants documented) |
| **Test coverage** | 15+ tests (`test_get_at_path_*` covering root, nested, arrays, missing, type mismatches) |
| **Alternative** | Would require cloning or `Box` allocation to avoid the cast |
| **Risk** | **LOW** — idiomatic `repr(transparent)` newtype pattern |

### Instance 2: Mutable pointer cast — `json.rs:1088`

**File**: `crates/core/src/primitives/json.rs:1088`
**Code**: `unsafe { &mut *(current as *mut serde_json::Value as *mut JsonValue) }`

| Aspect | Assessment |
|--------|------------|
| **Purpose** | Cast `&mut serde_json::Value` to `&mut JsonValue` at the end of mutable JSON path traversal |
| **Safety basis** | Same `#[repr(transparent)]` guarantee as Instance 1 |
| **Aliasing** | Mutable reference from `&mut JsonValue` input — borrow checker ensures exclusivity before the cast |
| **Lifetime** | Tied to input lifetime `<'a>` — correct |
| **SAFETY comment** | Present and detailed (same 3 invariants) |
| **Test coverage** | 5 tests (`test_get_at_path_mut_*` covering modification, root, missing) |
| **Alternative** | Same as Instance 1 |
| **Risk** | **LOW** — same pattern with borrow checker exclusivity pre-verified |

### Instance 3: `unsafe impl Send for Executor` — `executor.rs:712`

**File**: `crates/executor/src/executor.rs:712`

**Executor struct fields**:
```
primitives: Arc<Primitives>    — Arc is Send+Sync for any T
access_mode: AccessMode        — Copy enum (ReadWrite | ReadOnly)
```

**Primitives struct fields** (all `Arc<Database>` wrappers):
```
db: Arc<Database>, kv: PrimitiveKVStore, json: PrimitiveJsonStore,
event: PrimitiveEventLog, state: PrimitiveStateCell,
branch: PrimitiveBranchIndex, vector: PrimitiveVectorStore,
space: PrimitiveSpaceIndex
```

| Aspect | Assessment |
|--------|------------|
| **Purpose** | Allow `Executor` to be sent across thread boundaries |
| **Safety basis** | All fields are either `Arc<T>` (always Send) or `Copy` enums (always Send) |
| **Why manual** | Compiler couldn't auto-derive due to type complexity; all fields actually satisfy `Send` |
| **SAFETY comment** | Present but incomplete — mentions `Arc<Primitives>` but not `AccessMode` |
| **Risk** | **LOW** — conservative Arc-based design |

### Instance 4: `unsafe impl Sync for Executor` — `executor.rs:713`

**File**: `crates/executor/src/executor.rs:713`

| Aspect | Assessment |
|--------|------------|
| **Purpose** | Allow shared references `&Executor` across threads |
| **Safety basis** | Same as Instance 3 — `Arc<Primitives>` is Sync, `AccessMode` is a Copy enum |
| **SAFETY comment** | Same line as Instance 3 |
| **Risk** | **LOW** |

---

## Risk Matrix

| # | Location | Type | Sound | Comment | Risk |
|---|----------|------|-------|---------|------|
| 1 | `core/primitives/json.rs:1028` | ptr cast (immut) | SOUND | Detailed | LOW |
| 2 | `core/primitives/json.rs:1088` | ptr cast (mut) | SOUND | Detailed | LOW |
| 3 | `executor/executor.rs:712` | `impl Send` | SOUND | Basic | LOW |
| 4 | `executor/executor.rs:713` | `impl Sync` | SOUND | Basic | LOW |

---

## Recommendations

### Minor improvements (not blocking)

1. **Expand Send/Sync SAFETY comment** to explicitly mention both fields:
   ```rust
   // SAFETY: Executor is Send+Sync because:
   // - Arc<Primitives> is Send+Sync (all Primitives fields are Arc<Database>)
   // - AccessMode is a Copy enum (trivially Send+Sync)
   ```

2. **No other action needed** — all unsafe blocks are sound, well-documented, and well-tested.

---

## Methodology

Searched all production code for `unsafe` keyword. Read 20+ lines of surrounding context for each instance. Verified struct field types for Send/Sync analysis. Checked `#[repr(transparent)]` annotation on `JsonValue`.
