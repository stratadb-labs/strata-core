# M11 Implementation Plan: Public API & SDK Contract

## Overview

This document provides the high-level implementation plan for M11 (Public API & SDK Contract).

**M11 is split into two parts:**

| Part | Focus | Epics | Stories |
|------|-------|-------|---------|
| **M11a** | Core Contract & API | 80, 81, 82, 83, 84, 87a | ~36 |
| **M11b** | Consumer Surfaces | 85, 86, 87b | ~16 |

**Total Scope**: 8 Epics, ~52 Stories (split across M11a and M11b)

**References**:
- [M11 Architecture Specification](../../architecture/M11_ARCHITECTURE.md) - Authoritative architectural spec
- [M11 Contract Specification](./M11_CONTRACT.md) - Full contract details

**Critical Framing**:
> M11 is a **contract milestone**, not a feature milestone. It freezes the public API surface so all downstream consumers (wire protocol, CLI, SDKs, server) have a stable foundation.
>
> After M11, breaking changes require a major version bump. The contract defines what users observe. Internal implementation details remain flexible.
>
> **M11 does NOT add new capabilities.** It stabilizes, documents, and validates the existing API surface. The engine's seven primitives already exist. M11 ensures they are exposed consistently across all surfaces.

### M11a: Core Contract & API

M11a establishes the **foundation contract** that cannot change:
- Value Model (8 types, equality, limits)
- Wire Encoding (JSON, $bytes, $f64, $absent)
- Error Model (codes, shapes, reasons)
- Facade API (Redis-like surface)
- Substrate API (power-user surface)
- Core Validation (parity tests, round-trip tests, determinism)

**M11a Exit Criteria**: Core contract frozen, Facade↔Substrate parity verified, all core validation tests passing.

### M11b: Consumer Surfaces

M11b builds **user-facing surfaces** on top of the frozen M11a contract:
- CLI (all facade operations)
- SDK Foundation (Rust SDK, Python/JS mappings)
- Full Validation Suite (CLI tests, SDK conformance, regression tests)

**M11b Exit Criteria**: CLI complete, Rust SDK complete, full validation suite passing.

---

**Epic Details**:

**M11a Epics:**
- [Epic 80: Value Model Stabilization](./EPIC_80_VALUE_MODEL.md)
- [Epic 81: Facade API Implementation](./EPIC_81_FACADE_API.md)
- [Epic 82: Substrate API Implementation](./EPIC_82_SUBSTRATE_API.md)
- [Epic 83: Wire Encoding Contract](./EPIC_83_WIRE_ENCODING.md)
- [Epic 84: Error Model Finalization](./EPIC_84_ERROR_MODEL.md)
- Epic 87a: Core Validation (subset of Epic 87)

**M11b Epics:**
- [Epic 85: CLI Implementation](./EPIC_85_CLI.md)
- [Epic 86: SDK Foundation](./EPIC_86_SDK_FOUNDATION.md)
- Epic 87b: Surface Validation (subset of Epic 87)

---

## Architectural Integration Rules (NON-NEGOTIABLE)

These rules ensure M11 produces a stable, consistent contract.

### Rule 1: Facade Desugars to Substrate

Every facade operation MUST map to a deterministic sequence of substrate operations. No hidden semantics.

**FORBIDDEN**: Facade operations with behavior that cannot be expressed in substrate terms.

### Rule 2: No Hidden Errors

The facade MUST surface all substrate errors unchanged. No swallowing, transforming, or hiding errors.

**FORBIDDEN**: Error transformation, silent failures, best-effort fallbacks.

### Rule 3: No Type Coercion

Values MUST NOT be implicitly converted between types. `Int(1)` does not equal `Float(1.0)`.

**FORBIDDEN**: Implicit widening, lossy conversions, type promotion.

### Rule 4: Explicit Run Scoping

Substrate operations MUST require explicit `run_id`. Facade operations MUST target the default run.

**FORBIDDEN**: Substrate operations with implicit run, facade operations with explicit run parameters.

### Rule 5: Wire Encoding Preserves Types

Wire encoding MUST preserve the distinction between Value types. Round-trip must be lossless.

**FORBIDDEN**: Encoding that loses type information, ambiguous representations.

### Rule 6: Errors Are Explicit

All invalid inputs MUST produce explicit errors. No silent failures or best-effort handling.

**FORBIDDEN**: Silent truncation, silent coercion, partial results without indication.

### Rule 7: Contract Stability

Frozen elements MUST NOT change without major version bump. This includes operation names, parameter shapes, return shapes, error codes, wire encodings.

**FORBIDDEN**: Changing frozen elements, removing operations, altering semantics.

### Rule 8: Default Run Is Literal "default"

The default run has the canonical name `"default"` (literal string, not UUID). It always exists.

**FORBIDDEN**: UUID for default run, lazy creation visible to users, closeable default run.

---

## Core Invariants

### Facade Invariants

| # | Invariant | Test Strategy |
|---|-----------|---------------|
| FAC-1 | Every facade operation maps to deterministic substrate operations | Desugaring unit tests |
| FAC-2 | Facade adds no semantic behavior beyond defaults | Parity tests facade vs substrate |
| FAC-3 | Facade never swallows substrate errors | Error propagation tests |
| FAC-4 | Facade does not reorder operations | Ordering verification tests |
| FAC-5 | All behavior traces to explicit substrate operation | Audit all code paths |

### Value Model Invariants

| # | Invariant | Test Strategy |
|---|-----------|---------------|
| VAL-1 | Eight types only: Null, Bool, Int, Float, String, Bytes, Array, Object | Type exhaustiveness tests |
| VAL-2 | No implicit type coercions | Cross-type comparison tests |
| VAL-3 | `Int(1)` != `Float(1.0)` | Explicit inequality tests |
| VAL-4 | `Bytes` are not `String` | Type distinction tests |
| VAL-5 | Float uses IEEE-754 equality | NaN, -0.0 equality tests |

### Wire Encoding Invariants

| # | Invariant | Test Strategy |
|---|-----------|---------------|
| WIRE-1 | JSON encoding is mandatory | Encoding availability tests |
| WIRE-2 | Bytes encode as `{"$bytes": "<base64>"}` | Bytes round-trip tests |
| WIRE-3 | Non-finite floats encode as `{"$f64": "..."}` | Float special value tests |
| WIRE-4 | Absent values encode as `{"$absent": true}` | CAS absent value tests |
| WIRE-5 | Round-trip preserves exact type and value | Full round-trip suite |

### Error Invariants

| # | Invariant | Test Strategy |
|---|-----------|---------------|
| ERR-1 | All errors surface through structured error model | Error shape validation |
| ERR-2 | All errors include code, message, details | Error completeness tests |
| ERR-3 | No operation has undefined behavior | Exhaustive edge case tests |
| ERR-4 | `Conflict` = temporal; `ConstraintViolation` = structural | Error categorization tests |

### Versioned<T> Invariants

| # | Invariant | Test Strategy |
|---|-----------|---------------|
| VER-1 | Version is tagged union (txn/sequence/counter) | Version type preservation |
| VER-2 | Timestamp is microseconds since Unix epoch | Timestamp format tests |
| VER-3 | Version types are not comparable across tags | Cross-type comparison rejection |
| VER-4 | Versioned<T> shape is frozen | Shape validation tests |

---

## Epic Overview

### M11a Epics (Core Contract & API)

| Epic | Name | Stories | Dependencies | Status |
|------|------|---------|--------------|--------|
| 80 | Value Model Stabilization | 6 | M10 complete | Pending |
| 81 | Facade API Implementation | 8 | Epic 80 | Pending |
| 82 | Substrate API Implementation | 7 | Epic 80 | Pending |
| 83 | Wire Encoding Contract | 6 | Epic 80 | Pending |
| 84 | Error Model Finalization | 5 | Epic 80 | Pending |
| 87a | Core Validation Suite | 4 | Epics 80-84 | Pending |

**M11a Total**: 36 stories

### M11b Epics (Consumer Surfaces)

| Epic | Name | Stories | Dependencies | Status |
|------|------|---------|--------------|--------|
| 85 | CLI Implementation | 8 | M11a complete | Pending |
| 86 | SDK Foundation | 5 | M11a complete | Pending |
| 87b | Surface Validation Suite | 3 | Epics 85-86 | Pending |

**M11b Total**: 16 stories

---

## Epic 80: Value Model Stabilization

**Goal**: Freeze the canonical value model with all eight types

| Story | Description | Priority |
|-------|-------------|----------|
| #550 | Value Enum Finalization | FOUNDATION |
| #551 | Float Edge Case Handling (NaN, Inf, -0.0) | CRITICAL |
| #552 | Value Equality Implementation | CRITICAL |
| #553 | Size Limits Enforcement | CRITICAL |
| #554 | Key Validation Implementation | CRITICAL |
| #555 | Reserved Prefix Blocking | HIGH |

**Acceptance Criteria**:
- [ ] `Value` enum with exactly 8 variants: Null, Bool, Int(i64), Float(f64), String, Bytes, Array, Object
- [ ] Float preserves all IEEE-754 values including NaN, +Inf, -Inf, -0.0
- [ ] Structural equality implemented: `NaN != NaN`, `-0.0 == 0.0`
- [ ] No implicit type coercions in equality
- [ ] Size limits configurable and enforced:
  - `max_key_bytes`: 1024 (default)
  - `max_string_bytes`: 16 MiB
  - `max_bytes_len`: 16 MiB
  - `max_value_bytes_encoded`: 32 MiB
  - `max_array_len`: 1,000,000
  - `max_object_entries`: 1,000,000
  - `max_nesting_depth`: 128
  - `max_vector_dim`: 8192
- [ ] Key validation: valid UTF-8, no NUL, 1 to max_key_bytes, no `_strata/` prefix
- [ ] Violations return `ConstraintViolation` with reason codes

---

## Epic 81: Facade API Implementation

**Goal**: Implement the Redis-like facade API targeting default run

| Story | Description | Priority |
|-------|-------------|----------|
| #557 | KV Operations (set, get, getv, mget, mset, delete, exists, exists_many, incr) | CRITICAL |
| #558 | JSON Operations (json_set, json_get, json_getv, json_del, json_merge) | CRITICAL |
| #559 | Event Operations (xadd, xrange) | CRITICAL |
| #560 | Vector Operations (vset, vget, vdel) | CRITICAL |
| #561 | State Operations (cas_set, cas_get) | CRITICAL |
| #562 | History Operations (history, get_at, latest_version) | HIGH |
| #563 | Run Operations (runs, use_run) | HIGH |
| #564 | Capability Discovery (capabilities) | HIGH |

**Acceptance Criteria**:
- [ ] All KV operations implemented with correct signatures:
  - `set(key, value) -> ()`
  - `get(key) -> Option<Value>`
  - `getv(key) -> Option<Versioned<Value>>`
  - `mget(keys) -> Vec<Option<Value>>`
  - `mset(entries) -> ()` (atomic)
  - `delete(keys) -> u64` (count existed)
  - `exists(key) -> bool`
  - `exists_many(keys) -> u64`
  - `incr(key, delta) -> i64` (atomic engine operation)
- [ ] All JSON operations with correct path syntax (JSONPath-style)
- [ ] `json_getv` returns document-level version
- [ ] `xadd` returns `Version` (sequence type)
- [ ] `vget` returns `Option<Versioned<{vector, metadata}>>`
- [ ] CAS operations with structural equality
- [ ] `use_run` returns `NotFound` if run doesn't exist
- [ ] `capabilities()` returns limits, operations, encodings, features
- [ ] All operations target default run implicitly
- [ ] All operations auto-commit

---

## Epic 82: Substrate API Implementation

**Goal**: Implement the explicit substrate API with run/version/txn access

| Story | Description | Priority |
|-------|-------------|----------|
| #566 | KVStore Substrate (kv_put, kv_get, kv_get_at, kv_delete, kv_exists, kv_history, kv_incr, kv_cas_version, kv_cas_value) | CRITICAL |
| #567 | JsonStore Substrate (json_set, json_get, json_delete, json_merge, json_history) | CRITICAL |
| #568 | EventLog Substrate (event_append, event_range) | CRITICAL |
| #569 | StateCell Substrate (state_get, state_set, state_cas) | CRITICAL |
| #570 | VectorStore Substrate (vector_set, vector_get, vector_delete, vector_history) | CRITICAL |
| #571 | TraceStore Substrate (trace_record, trace_get, trace_range) | HIGH |
| #572 | RunIndex Substrate (run_create, run_get, run_list, run_close) | CRITICAL |

**Acceptance Criteria**:
- [ ] All substrate operations require explicit `run_id` parameter
- [ ] All read operations return `Versioned<T>`
- [ ] All write operations return `Version`
- [ ] `kv_incr` is atomic engine operation
- [ ] `kv_cas_version` compares by version
- [ ] `kv_cas_value` compares by structural equality
- [ ] `run_create` returns new `RunId` (UUID format)
- [ ] `run_close` marks run as closed (cannot be deleted in M11)
- [ ] Default run (`"default"`) cannot be closed
- [ ] TraceStore operations are substrate-only
- [ ] Retention operations: `retention_get`, `retention_set`

---

## Epic 83: Wire Encoding Contract

**Goal**: Implement frozen JSON wire encoding with special value wrappers

| Story | Description | Priority |
|-------|-------------|----------|
| #574 | Request/Response Envelope Implementation | FOUNDATION |
| #575 | Value Type JSON Mapping | CRITICAL |
| #576 | $bytes Wrapper Implementation | CRITICAL |
| #577 | $f64 Wrapper Implementation (NaN, Inf, -0.0) | CRITICAL |
| #578 | $absent Wrapper Implementation | CRITICAL |
| #579 | Versioned<T> Wire Encoding | CRITICAL |

**Acceptance Criteria**:
- [ ] Request envelope: `{ "id": "...", "op": "...", "params": {...} }`
- [ ] Success response: `{ "id": "...", "ok": true, "result": ... }`
- [ ] Error response: `{ "id": "...", "ok": false, "error": {...} }`
- [ ] Operation names frozen (e.g., `kv.set`, `json.get`, `substrate.kv.put`)
- [ ] Value type mapping:
  - Null → `null`
  - Bool → `true`/`false`
  - Int → JSON number
  - Float (finite) → JSON number
  - Float (non-finite) → `{"$f64": "NaN"|"+Inf"|"-Inf"|"-0.0"}`
  - String → JSON string
  - Bytes → `{"$bytes": "<base64>"}`
  - Array → JSON array
  - Object → JSON object
- [ ] Absent value for CAS: `{"$absent": true}`
- [ ] Version encoding: `{"type": "txn"|"sequence"|"counter", "value": N}`
- [ ] Versioned<T> encoding: `{"value": ..., "version": {...}, "timestamp": N}`
- [ ] All encodings round-trip correctly

---

## Epic 84: Error Model Finalization

**Goal**: Freeze all error codes and structured payloads

| Story | Description | Priority |
|-------|-------------|----------|
| #581 | Error Code Enumeration | FOUNDATION |
| #582 | Error Wire Shape Implementation | CRITICAL |
| #583 | ConstraintViolation Reason Codes | CRITICAL |
| #584 | Error Details Payload Shapes | CRITICAL |
| #585 | Error-Producing Condition Coverage | HIGH |

**Acceptance Criteria**:
- [ ] All error codes implemented:
  - `NotFound`
  - `WrongType`
  - `InvalidKey`
  - `InvalidPath`
  - `HistoryTrimmed`
  - `ConstraintViolation`
  - `Conflict`
  - `SerializationError`
  - `StorageError`
  - `InternalError`
- [ ] Wire error shape: `{ "code": "...", "message": "...", "details": {...} }`
- [ ] ConstraintViolation reason codes:
  - `value_too_large`
  - `nesting_too_deep`
  - `key_too_long`
  - `vector_dim_exceeded`
  - `vector_dim_mismatch`
  - `root_not_object`
  - `reserved_prefix`
- [ ] HistoryTrimmed includes `requested` and `earliest_retained` versions
- [ ] All error-producing conditions documented and tested

---

## Epic 85: CLI Implementation

**Goal**: Implement Redis-like CLI with frozen parsing rules

| Story | Description | Priority |
|-------|-------------|----------|
| #587 | CLI Argument Parser | FOUNDATION |
| #588 | KV Commands (set, get, mget, mset, delete, exists, incr) | CRITICAL |
| #589 | JSON Commands (json.set, json.get, json.del, json.merge) | CRITICAL |
| #590 | Event Commands (xadd, xrange) | HIGH |
| #591 | Vector Commands (vset, vget, vdel) | HIGH |
| #592 | State Commands (cas.set, cas.get) | HIGH |
| #593 | History Commands (history) | HIGH |
| #594 | Output Formatting | CRITICAL |

**Acceptance Criteria**:
- [ ] CLI command interface: `strata <command> [args...]`
- [ ] Argument parsing rules:
  - `123` → Int
  - `1.23` → Float
  - `"hello"` → String (quotes stripped)
  - `hello` → String (bare word)
  - `true`/`false` → Bool
  - `null` → Null
  - `{...}` → Object (valid JSON)
  - `[...]` → Array (valid JSON)
  - `b64:...` → Bytes (base64 decoded)
- [ ] Output conventions:
  - Missing value: `(nil)`
  - Integer/count: `(integer) N`
  - Boolean: `(integer) 0` or `(integer) 1`
  - String: `"text"`
  - Null value: `null`
  - Object/Array: JSON formatted
  - Bytes: `{"$bytes": "<base64>"}`
  - Error: JSON on stderr, non-zero exit
- [ ] Run scoping: `--run=<run_id>` option (default is `"default"`)
- [ ] Vector commands working:
  - `strata vset <key> <vector> <metadata>` → success
  - `strata vget <key>` → Versioned output or `(nil)`
  - `strata vdel <key>` → `(integer) 0` or `(integer) 1`
- [ ] State/CAS commands working:
  - `strata cas.set <key> <expected> <new>` → `(integer) 0` or `(integer) 1`
  - `strata cas.get <key>` → value or `(nil)`
- [ ] CLI is facade-only (no substrate operations exposed)

---

## Epic 86: SDK Foundation

**Goal**: Define SDK mappings and implement Rust SDK

| Story | Description | Priority |
|-------|-------------|----------|
| #596 | SDK Value Mapping Specification | FOUNDATION |
| #597 | Rust SDK Implementation | CRITICAL |
| #598 | Python SDK Mapping Definition | HIGH |
| #599 | JavaScript SDK Mapping Definition | HIGH |
| #600 | SDK Conformance Test Harness | CRITICAL |

**Acceptance Criteria**:
- [ ] Python mapping defined:
  - Null → `None`
  - Bool → `bool`
  - Int → `int`
  - Float → `float`
  - String → `str`
  - Bytes → `bytes`
  - Array → `list`
  - Object → `dict[str, Any]`
- [ ] JavaScript mapping defined:
  - Null → `null`
  - Bool → `boolean`
  - Int → `number | BigInt` (outside safe integer range)
  - Float → `number`
  - String → `string`
  - Bytes → `Uint8Array`
  - Array → `Array<any>`
  - Object → `Record<string, any>`
- [ ] Rust SDK uses native `Value` enum
- [ ] SDK requirements enforced:
  - Preserve numeric widths (i64, f64)
  - Preserve Bytes vs String distinction
  - Preserve None/missing vs Null distinction
  - Preserve Versioned wrapper shape
  - Surface structured errors
  - Use same operation names as facade
- [ ] Conformance test harness validates SDK behavior

---

## Epic 87a: Core Validation Suite (M11a)

**Goal**: Validate core contract guarantees before building consumer surfaces

| Story | Description | Priority |
|-------|-------------|----------|
| #602 | Facade-Substrate Parity Tests | CRITICAL |
| #603 | Value Round-Trip Tests | CRITICAL |
| #604 | Wire Encoding Conformance Tests | CRITICAL |
| #605 | Determinism Verification Tests | CRITICAL |

**Acceptance Criteria (M11a Exit Gate)**:
- [ ] Facade-Substrate parity: every facade operation produces same result as desugared substrate
- [ ] Value round-trip: all 8 types survive encode/decode
- [ ] Float edge cases: NaN, +Inf, -Inf, -0.0 all preserved
- [ ] Bytes vs String distinction preserved
- [ ] $absent distinguishes missing from null
- [ ] All error codes produce correct wire shape
- [ ] Same substrate operations produce same state (determinism)
- [ ] Timestamp independence (different timestamps, same logical state)

---

## Epic 87b: Surface Validation Suite (M11b)

**Goal**: Validate consumer surfaces and complete contract validation

| Story | Description | Priority |
|-------|-------------|----------|
| #607 | CLI Conformance Tests | CRITICAL |
| #608 | SDK Conformance Test Harness | CRITICAL |
| #609 | End-to-End Regression Suite | HIGH |

**Acceptance Criteria (M11b Exit Gate)**:
- [ ] CLI argument parsing tests pass
- [ ] CLI output formatting tests pass
- [ ] CLI commands for all primitives working
- [ ] SDK value mapping tests pass
- [ ] SDK error handling tests pass
- [ ] WAL replay determinism verified
- [ ] Contract stability validated (no accidental breaking changes)
- [ ] Golden file regression tests pass

---

## Files to Create/Modify

### New Files

| File | Description |
|------|-------------|
| **Facade Module** | |
| `crates/api/src/facade/mod.rs` | Facade module entry point |
| `crates/api/src/facade/kv.rs` | KV facade operations |
| `crates/api/src/facade/json.rs` | JSON facade operations |
| `crates/api/src/facade/event.rs` | Event facade operations |
| `crates/api/src/facade/vector.rs` | Vector facade operations |
| `crates/api/src/facade/state.rs` | State (CAS) facade operations |
| `crates/api/src/facade/history.rs` | History facade operations |
| `crates/api/src/facade/run.rs` | Run facade operations |
| `crates/api/src/facade/capabilities.rs` | Capability discovery |
| **Substrate Module** | |
| `crates/api/src/substrate/mod.rs` | Substrate module entry point |
| `crates/api/src/substrate/kv.rs` | KVStore substrate |
| `crates/api/src/substrate/json.rs` | JsonStore substrate |
| `crates/api/src/substrate/event.rs` | EventLog substrate |
| `crates/api/src/substrate/state.rs` | StateCell substrate |
| `crates/api/src/substrate/vector.rs` | VectorStore substrate |
| `crates/api/src/substrate/trace.rs` | TraceStore substrate |
| `crates/api/src/substrate/run.rs` | RunIndex substrate |
| `crates/api/src/substrate/retention.rs` | Retention substrate |
| **Wire Module** | |
| `crates/wire/src/lib.rs` | Wire crate entry point |
| `crates/wire/src/json/mod.rs` | JSON encoding module |
| `crates/wire/src/json/value.rs` | Value JSON encoding |
| `crates/wire/src/json/wrappers.rs` | $bytes, $f64, $absent wrappers |
| `crates/wire/src/json/envelope.rs` | Request/response envelopes |
| `crates/wire/src/json/version.rs` | Version encoding |
| **CLI Module** | |
| `crates/cli/src/main.rs` | CLI entry point |
| `crates/cli/src/parser.rs` | Argument parser |
| `crates/cli/src/commands/mod.rs` | Command module |
| `crates/cli/src/commands/kv.rs` | KV commands |
| `crates/cli/src/commands/json.rs` | JSON commands |
| `crates/cli/src/commands/event.rs` | Event commands |
| `crates/cli/src/commands/vector.rs` | Vector commands |
| `crates/cli/src/commands/state.rs` | State/CAS commands |
| `crates/cli/src/commands/history.rs` | History commands |
| `crates/cli/src/output.rs` | Output formatting |
| **Error Module** | |
| `crates/core/src/error/codes.rs` | Error code enum |
| `crates/core/src/error/constraint.rs` | ConstraintViolation reasons |
| `crates/core/src/error/wire.rs` | Wire error shape |

### Modified Files

| File | Changes |
|------|---------|
| `crates/core/src/value.rs` | Finalize Value enum, add equality |
| `crates/core/src/version.rs` | Add tagged union Version type |
| `crates/core/src/error.rs` | Add all error codes |
| `crates/engine/src/lib.rs` | Wire facade/substrate layers |
| `Cargo.toml` | Add wire, cli crates |

---

## Dependency Order

```
            Epic 80 (Value Model Stabilization)
                        ↓
            ┌───────────┼───────────┐
            ↓           ↓           ↓
        Epic 81     Epic 82     Epic 83
        (Facade)    (Substrate) (Wire)
            ↓           ↓           ↓
            └───────────┼───────────┘
                        ↓
                Epic 84 (Error Model)
                        ↓
                Epic 87a (Core Validation)
                        ↓
    ════════════════════════════════════════
                M11a COMPLETE
    ════════════════════════════════════════
                        ↓
                ┌───────┴───────┐
                ↓               ↓
            Epic 85         Epic 86
            (CLI)           (SDK)
                ↓               ↓
                └───────┬───────┘
                        ↓
            Epic 87b (Surface Validation)
                        ↓
    ════════════════════════════════════════
                M11b COMPLETE
    ════════════════════════════════════════
```

**M11a Recommended Implementation Order**:
1. Epic 80: Value Model Stabilization (foundation for everything)
2. Epic 83: Wire Encoding Contract (needed early for testing)
3. Epic 84: Error Model Finalization (needed by API layers)
4. Epic 81: Facade API Implementation (user-facing layer)
5. Epic 82: Substrate API Implementation (power-user layer)
6. Epic 87a: Core Validation Suite (validates core contract)

**M11b Recommended Implementation Order** (after M11a complete):
7. Epic 85: CLI Implementation (uses facade + wire)
8. Epic 86: SDK Foundation (uses facade + wire + error)
9. Epic 87b: Surface Validation Suite (validates consumer surfaces)

---

## Phased Implementation Strategy

> **Guiding Principle**: Stabilize the data model first. Wire encoding must work before APIs. APIs must work before CLI/SDK. Each phase produces a testable, validated increment. M11a (Phases 1-4) must be fully validated before starting M11b (Phases 5-6).

### M11a Phases

#### Phase 1: Data Model Foundation

Stabilize value model and wire encoding:
- Value enum finalization
- Float edge case handling
- Size limits enforcement
- Key validation
- JSON wire encoding with special wrappers

**Exit Criteria**: All 8 value types encode/decode correctly. Round-trip tests pass.

#### Phase 2: Error Model

Freeze all error codes and payloads:
- Error code enumeration
- Wire error shape
- ConstraintViolation reasons
- Details payload shapes

**Exit Criteria**: All error conditions produce correct structured errors.

#### Phase 3: API Layers

Implement facade and substrate APIs:
- Facade API with all operations
- Substrate API with explicit parameters
- Facade→Substrate desugaring verified
- Auto-commit semantics

**Exit Criteria**: Both API layers complete. Parity tests pass.

#### Phase 4: Core Validation (M11a Exit Gate)

Comprehensive validation of core contract:
- Value model tests (100% coverage)
- Wire encoding round-trip tests
- Facade-Substrate parity tests
- Error model verification
- Determinism tests for core APIs

**Exit Criteria**: All M11a contract guarantees validated. Zero defects in core contract.

---

### M11b Phases

**Prerequisite**: M11a must be fully complete and validated before starting M11b.

#### Phase 5: CLI + SDK

Implement consumer surfaces:
- CLI with Redis-like ergonomics
- Rust SDK implementation
- Python/JavaScript mapping definitions
- Output formatting

**Exit Criteria**: CLI works for all facade operations. SDK conformance harness passes.

#### Phase 6: Surface Validation (M11b Exit Gate)

Comprehensive validation of consumer surfaces:
- CLI argument parsing tests
- CLI output formatting tests
- CLI command integration tests
- SDK conformance tests
- SDK type mapping verification

**Exit Criteria**: All M11b contract guarantees validated. No regressions in M11a.

---

### Phase Summary

| Phase | Milestone | Epics | Key Deliverable | Status |
|-------|-----------|-------|-----------------|--------|
| 1 | M11a | 80, 83 | Data model + wire encoding | Pending |
| 2 | M11a | 84 | Error model | Pending |
| 3 | M11a | 81, 82 | API layers | Pending |
| 4 | M11a | 87a | Core validation | Pending |
| 5 | M11b | 85, 86 | CLI + SDK | Pending |
| 6 | M11b | 87b | Surface validation | Pending |

---

## Testing Strategy

### Unit Tests

- Value type construction and properties
- Float edge cases (NaN, Inf, -0.0)
- Value equality semantics
- Size limit enforcement
- Key validation logic
- Wire encoding for each value type
- Special wrapper encoding ($bytes, $f64, $absent)
- Error code mapping
- CLI argument parsing

### Integration Tests

- Facade operation → substrate desugaring
- Full request/response cycle
- Multi-operation transactions
- Run scoping with `use_run`
- History pagination
- CAS operations with $absent

### Contract Tests

- Every facade operation produces same result as desugared substrate
- All 8 value types round-trip through wire encoding
- Float edge cases preserve exact representation
- Bytes vs String distinction maintained
- $absent distinguishes missing from null
- All error codes produce correct wire shape
- Version tagged union preserved

### Determinism Tests

- Same substrate operations produce same state
- WAL replay produces identical state
- Timestamp independence verified
- Compaction invisibility maintained

### SDK Parity Tests

- Same operations produce same results across SDKs
- Value mapping consistent
- Error handling consistent
- Versioned wrapper shape consistent

### CLI Tests

- Argument parsing for all input types
- Output formatting for all return types
- Error output on stderr with non-zero exit
- Run scoping with --run option

---

## Success Metrics

**Functional**: All 48 stories passing, 100% acceptance criteria met

**Contract Stability**:
- All frozen elements documented
- No breaking changes in frozen elements
- Contract versioned and dated

**API Completeness**:
- All facade operations implemented
- All substrate operations implemented
- All escape hatches working (getv, use_run, db.substrate())

**Wire Conformance**:
- JSON encoding mandatory and working
- All special wrappers implemented
- Round-trip tests pass for all types

**Error Model**:
- All error codes implemented
- All ConstraintViolation reasons implemented
- Structured details for all relevant errors

**CLI**:
- All facade commands working
- Argument parsing correct
- Output formatting correct

**SDK**:
- Rust SDK complete
- Python/JavaScript mappings defined
- Conformance harness passing

**Quality**: Test coverage > 90% for new code

---

## Risk Mitigation

| Risk | Likelihood | Impact | Mitigation |
|------|------------|--------|------------|
| Breaking change introduced | Medium | High | Contract validation suite, careful review |
| Float edge cases mishandled | Medium | High | Comprehensive float tests, IEEE-754 compliance |
| Wire encoding ambiguity | Low | High | Explicit wrapper semantics, round-trip tests |
| Facade-Substrate divergence | Medium | Medium | Parity tests, mechanical desugaring |
| CLI parsing inconsistency | Low | Medium | Frozen parsing rules, extensive tests |
| SDK mapping errors | Medium | Medium | Conformance harness, type preservation tests |

---

## Not In Scope (Explicitly Deferred)

1. **Python SDK implementation** - M14 (mappings defined only)
2. **JavaScript SDK implementation** - Post-MVP (mappings defined only)
3. **MessagePack wire encoding** - Optional, not required
4. **Search/vector query DSL** - Post-MVP
5. **JSONPath advanced features** - Filters, wildcards, recursive descent
6. **TTL/EXPIRE semantics** - Post-MVP
7. **Consumer groups for events** - Post-MVP
8. **Diff semantics** - Post-MVP
9. **Run deletion** - Deferred to GC milestone
10. **Per-key retention** - Post-MVP
11. **Serializable isolation** - Snapshot isolation only

---

## Post-M11 Expectations

After M11 completion:
1. Public API contract is frozen and documented
2. All facade operations have consistent behavior
3. All substrate operations expose full power-user control
4. Wire encoding is stable (JSON with $bytes, $f64, $absent)
5. CLI provides Redis-like ergonomics
6. Rust SDK is complete and conformant
7. Python/JavaScript mappings are defined for future implementation
8. Breaking changes require major version bump
9. Contract validation suite catches regressions
10. All downstream consumers (server, SDKs) have stable foundation

---

## Facade→Substrate Desugaring Reference

For quick reference, the complete desugaring table:

### KV Operations

| Facade | Substrate |
|--------|-----------|
| `set(key, value)` | `begin(); kv_put(default, key, value); commit()` |
| `get(key)` | `kv_get(default, key).map(\|v\| v.value)` |
| `getv(key)` | `kv_get(default, key)` |
| `mget(keys)` | `batch { kv_get(default, k) for k in keys }` |
| `mset(entries)` | `begin(); for (k,v): kv_put(default, k, v); commit()` |
| `delete(keys)` | `begin(); for k: kv_delete(default, k); commit()` |
| `exists(key)` | `kv_get(default, key).is_some()` |
| `exists_many(keys)` | `keys.filter(\|k\| kv_get(default, k).is_some()).count()` |
| `incr(key, delta)` | `kv_incr(default, key, delta)` |

### JSON Operations

| Facade | Substrate |
|--------|-----------|
| `json_set(key, path, value)` | `begin(); json_set(default, key, path, value); commit()` |
| `json_get(key, path)` | `json_get(default, key, path).map(\|v\| v.value)` |
| `json_getv(key, path)` | `json_get(default, key, path)` |
| `json_del(key, path)` | `begin(); json_delete(default, key, path); commit()` |
| `json_merge(key, path, value)` | `begin(); json_merge(default, key, path, value); commit()` |

### Other Operations

| Facade | Substrate |
|--------|-----------|
| `xadd(stream, payload)` | `event_append(default, stream, payload)` |
| `xrange(stream, start, end, limit)` | `event_range(default, stream, start, end, limit)` |
| `vset(key, vector, metadata)` | `begin(); vector_set(default, key, vector, metadata); commit()` |
| `vget(key)` | `vector_get(default, key)` |
| `vdel(key)` | `begin(); vector_delete(default, key); commit()` |
| `cas_set(key, expected, new)` | `state_cas(default, key, expected, new)` |
| `cas_get(key)` | `state_get(default, key).map(\|v\| v.value)` |
| `history(key, limit, before)` | `kv_history(default, key, limit, before)` |
| `get_at(key, version)` | `kv_get_at(default, key, version)` |
| `runs()` | `run_list()` |
| `use_run(run_id)` | Returns facade with `default = run_id` |

---

## Document History

| Version | Date | Changes |
|---------|------|---------|
| 1.0 | 2026-01-21 | Initial M11 implementation plan |
