# M11 Architecture: Public API & SDK Contract

**Status**: Freeze Candidate
**Author**: Architecture Team
**Created**: 2026-01-21
**Milestone**: 11 - Public API & SDK Contract

---

## Table of Contents

1. [Executive Summary](#1-executive-summary)
2. [Scope Boundaries](#2-scope-boundaries)
3. [Architectural Rules](#3-architectural-rules)
4. [Core Invariants](#4-core-invariants)
5. [Two-Layer API Model](#5-two-layer-api-model)
6. [Canonical Value Model](#6-canonical-value-model)
7. [Versioned\<T\> Contract](#7-versionedt-contract)
8. [Error Model](#8-error-model)
9. [Wire Encoding Contract](#9-wire-encoding-contract)
10. [CLI Contract](#10-cli-contract)
11. [SDK Mapping](#11-sdk-mapping)
12. [Transaction Semantics](#12-transaction-semantics)
13. [Run Semantics](#13-run-semantics)
14. [History & Retention](#14-history--retention)
15. [Testing Strategy](#15-testing-strategy)
16. [Known Limitations](#16-known-limitations)
17. [Future Extension Points](#17-future-extension-points)
18. [Success Criteria Checklist](#18-success-criteria-checklist)
19. [Document History](#19-document-history)

---

## 1. Executive Summary

### 1.1 Philosophy

M11 freezes the **public contract** of Strata. After M11, every downstream surface—wire protocol, CLI, Rust SDK, future Python/JavaScript SDKs, server—must conform to this contract. Breaking changes require a major version bump.

The architecture centers on a **two-layer API model**:
- **Facade API**: Redis-like surface for 95% of users who want familiar patterns
- **Substrate API**: Power-user surface exposing runs, versions, transactions, primitives

The fundamental architectural invariant: **Every facade call desugars to exactly one substrate call pattern**. No magic, no hidden semantics. The facade is a lossless projection of the substrate.

### 1.2 Goals

1. **Contract Stability**: Define what is frozen vs. what can change
2. **Determinism Guarantee**: Same substrate operations produce same state
3. **API Clarity**: Two layers with clear boundaries and escape hatches
4. **Type Safety**: Single canonical value model across all surfaces
5. **Wire Stability**: Frozen JSON encoding with special value wrappers
6. **SDK Consistency**: Identical semantics across Rust, Python, JavaScript

### 1.3 Non-Goals

- Performance optimization (not frozen)
- Storage format specification (internal detail)
- Compaction algorithms (internal detail)
- Search ranking algorithms (deferred)

---

## 2. Scope Boundaries

### 2.1 What This Milestone IS

| Aspect | Scope |
|--------|-------|
| Facade API | All operations frozen (KV, JSON, Event, Vector, State, History) |
| Substrate API | All primitives exposed with explicit run/version/txn access |
| Value Model | Null, Bool, Int, Float, String, Bytes, Array, Object - frozen |
| Versioned\<T\> | Tagged union versions (txn/sequence/counter), microsecond timestamps |
| Error Model | All error codes and payload shapes frozen |
| Wire Encoding | JSON mandatory, `$bytes`/`$f64`/`$absent` wrappers frozen |
| CLI | Redis-like command interface with frozen parsing rules |
| SDK Mappings | Python, JavaScript, Rust value mappings defined |

### 2.2 What This Milestone IS NOT

| Aspect | Reason |
|--------|--------|
| Performance tuning | Internal optimization, not contract |
| Storage layout | Implementation detail |
| WAL format | Implementation detail |
| Search ranking | Deferred to future milestone |
| Diff semantics | Deferred |
| TTL/EXPIRE | Deferred |
| Consumer groups | Deferred |
| Vector search DSL | Deferred |
| MessagePack wire | Optional, not required |
| Run deletion | Deferred to GC milestone |

### 2.3 Contract Stability Rule

**Frozen (major version required to change):**
- Operation names (facade and substrate)
- Parameter names and shapes
- Return shapes
- Error codes and `details` field structure
- Value model types and semantics
- Wire encodings (JSON wrappers, envelope structure)
- Version types (`txn`, `sequence`, `counter`)
- Timestamp units (microseconds)
- Default behaviors (auto-commit, default run targeting)
- CLI command names and output formats
- SDK method names and signatures

**Not Frozen (may change without major version):**
- Performance characteristics
- Storage layout and file formats
- WAL format
- Internal algorithms
- Compaction behavior
- Memory usage patterns
- Concurrency implementation details

**Rule**: If it affects what users observe through the API, it is frozen. If it only affects how fast or efficiently those observations are produced, it is not frozen.

---

## 3. Architectural Rules

These rules are **non-negotiable**. Violating them is a bug.

### 3.1 Rule 1: Facade Desugars to Substrate

Every facade operation MUST map to a deterministic sequence of substrate operations.

```rust
// CORRECT: Facade desugars mechanically
fn set(&self, key: &str, value: Value) -> Result<()> {
    let txn = self.substrate.begin(DEFAULT_RUN)?;
    self.substrate.kv_put(&txn, key, value)?;
    self.substrate.commit(txn)?;
    Ok(())
}

// WRONG: Facade adds behavior not in substrate
fn set(&self, key: &str, value: Value) -> Result<()> {
    if key.starts_with("cache:") {
        self.cache.set(key, value);  // NO! Hidden semantics
    }
    // ...
}
```

**Invariant**: The facade must never introduce semantic behavior that does not exist in the substrate.

### 3.2 Rule 2: No Hidden Errors

The facade MUST surface all substrate errors unchanged.

```rust
// CORRECT: Propagate substrate errors
fn get(&self, key: &str) -> Result<Option<Value>> {
    self.substrate.kv_get(DEFAULT_RUN, key)
        .map(|opt| opt.map(|v| v.value))
}

// WRONG: Swallow errors
fn get(&self, key: &str) -> Option<Value> {
    self.substrate.kv_get(DEFAULT_RUN, key)
        .ok()  // NO! Error is swallowed
        .flatten()
        .map(|v| v.value)
}
```

**Invariant**: All substrate errors must surface to facade callers.

### 3.3 Rule 3: No Type Coercion

Values MUST NOT be implicitly converted between types.

```rust
// CORRECT: Types are distinct
let int_val = Value::Int(1);
let float_val = Value::Float(1.0);
assert!(int_val != float_val);  // Different types

// WRONG: Implicit widening
fn compare(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Int(i), Value::Float(f)) => (*i as f64) == *f,  // NO!
        // ...
    }
}
```

**Invariant**: `Int(1)` does not equal `Float(1.0)`. No numeric widening.

### 3.4 Rule 4: Explicit Run Scoping

Substrate operations MUST require explicit `run_id`. Facade operations MUST target the default run.

```rust
// CORRECT: Substrate requires explicit run
fn kv_put(&self, run_id: &RunId, key: &str, value: Value) -> Result<Version>;

// CORRECT: Facade uses implicit default
fn set(&self, key: &str, value: Value) -> Result<()> {
    // Always targets "default" run
    self.kv_put(&RunId::DEFAULT, key, value)
}

// WRONG: Substrate with implicit run
fn kv_put(&self, key: &str, value: Value) -> Result<Version>;  // NO! Where's the run?
```

**Invariant**: The substrate is explicit. The facade is opinionated.

### 3.5 Rule 5: Wire Encoding Preserves Types

Wire encoding MUST preserve the distinction between Value types.

```rust
// CORRECT: Bytes use wrapper
let bytes = Value::Bytes(vec![72, 101, 108, 108, 111]);
// Wire: {"$bytes": "SGVsbG8="}

// CORRECT: NaN uses wrapper
let nan = Value::Float(f64::NAN);
// Wire: {"$f64": "NaN"}

// WRONG: Bytes as string
let bytes = Value::Bytes(vec![72, 101, 108, 108, 111]);
// Wire: "Hello"  // NO! Type information lost
```

**Invariant**: Round-trip encoding must preserve exact type and value.

### 3.6 Rule 6: Errors Are Explicit

All invalid inputs MUST produce explicit errors, never silent failures.

```rust
// CORRECT: Invalid key returns error
fn validate_key(key: &str) -> Result<()> {
    if key.contains('\0') {
        return Err(StrataError::InvalidKey {
            message: "Key contains NUL byte".into(),
        });
    }
    Ok(())
}

// WRONG: Silent truncation
fn validate_key(key: &str) -> Result<String> {
    Ok(key.replace('\0', ""))  // NO! Silent data modification
}
```

**Invariant**: No silent failures. No best-effort fallbacks.

---

## 4. Core Invariants

### 4.1 Determinism Invariants

| Invariant | Description |
|-----------|-------------|
| DET-1 | Same sequence of substrate operations produces same state |
| DET-2 | Timestamps are metadata, not inputs to state transitions |
| DET-3 | WAL replay produces identical state |
| DET-4 | Compacted state is indistinguishable from uncompacted (except trimmed history) |

### 4.2 Facade Invariants

| Invariant | Description |
|-----------|-------------|
| FAC-1 | Every facade operation maps to deterministic substrate operations |
| FAC-2 | Facade adds no semantic behavior beyond defaults |
| FAC-3 | Facade never swallows, transforms, or hides substrate errors |
| FAC-4 | Facade does not reorder operations or change consistency guarantees |
| FAC-5 | If something happens, it traces to an explicit substrate operation |

### 4.3 Value Model Invariants

| Invariant | Description |
|-----------|-------------|
| VAL-1 | Eight types only: Null, Bool, Int, Float, String, Bytes, Array, Object |
| VAL-2 | No implicit type coercions |
| VAL-3 | `Int(1)` != `Float(1.0)` |
| VAL-4 | `Bytes` are not `String` (distinct wire representations) |
| VAL-5 | Float uses IEEE-754 equality: `NaN != NaN`, `-0.0 == 0.0` |

### 4.4 Wire Encoding Invariants

| Invariant | Description |
|-----------|-------------|
| WIRE-1 | JSON encoding is mandatory |
| WIRE-2 | Bytes encode as `{"$bytes": "<base64>"}` |
| WIRE-3 | Non-finite floats encode as `{"$f64": "NaN\|+Inf\|-Inf\|-0.0"}` |
| WIRE-4 | Absent values encode as `{"$absent": true}` |
| WIRE-5 | Round-trip preserves exact type and value |

### 4.5 Error Invariants

| Invariant | Description |
|-----------|-------------|
| ERR-1 | All errors surface through structured error model |
| ERR-2 | All errors include stable code, message, and details |
| ERR-3 | No operation has undefined behavior |
| ERR-4 | `Conflict` = temporal failures; `ConstraintViolation` = structural failures |

---

## 5. Two-Layer API Model

### 5.1 Facade API

The facade hides Strata's complexity behind Redis-familiar patterns.

**What's hidden:**
- Runs (everything targets implicit `"default"` run)
- Versions (writes don't return versions, reads return values not `Versioned<T>`)
- Primitive selection (users say `set`, not `kv_put`)
- Transactions (each call auto-commits)
- History (no version access unless explicitly requested)

**Surface:**
```rust
set(key, value) -> ()
get(key) -> Option<Value>
getv(key) -> Option<Versioned<Value>>  // Escape hatch
mget(keys) -> Vec<Option<Value>>
mset(entries) -> ()
delete(keys) -> u64
exists(key) -> bool
exists_many(keys) -> u64
incr(key, delta) -> i64
```

### 5.2 Substrate API

The substrate exposes everything explicitly.

**What's visible:**
- Explicit `run_id` on every operation
- Explicit primitive selection (`kv_put`, `json_set`, `event_append`)
- `Versioned<T>` returns on all reads
- Transaction control (`begin`, `commit`, `rollback`)
- Full history access
- Retention policy access
- Run lifecycle management

**Surface:**
```rust
kv_put(run, key, value) -> Version
kv_get(run, key) -> Option<Versioned<Value>>
kv_get_at(run, key, version) -> Versioned<Value> | HistoryTrimmed
kv_delete(run, key) -> bool
kv_history(run, key, limit?, before?) -> Vec<Versioned<Value>>
kv_cas_version(run, key, expected_version, new_value) -> bool
kv_cas_value(run, key, expected_value, new_value) -> bool
```

### 5.3 Escape Hatches

From facade to substrate:
- `getv(key)` returns `Versioned<Value>` instead of `Value`
- `use_run(run_id)` scopes operations to a specific run
- `db.substrate().*` for direct substrate access

### 5.4 Desugaring Table

| Facade | Substrate Pattern |
|--------|-------------------|
| `set(key, value)` | `begin(); kv_put(default, key, value); commit()` |
| `get(key)` | `kv_get(default, key).map(\|v\| v.value)` |
| `getv(key)` | `kv_get(default, key)` |
| `mget(keys)` | `batch { kv_get(default, k) for k in keys }` |
| `mset(entries)` | `begin(); for (k,v): kv_put(default, k, v); commit()` |
| `delete(keys)` | `begin(); for k: kv_delete(default, k); commit()` |
| `exists(key)` | `kv_get(default, key).is_some()` |
| `incr(key, delta)` | `kv_incr(default, key, delta)` (atomic engine op) |

---

## 6. Canonical Value Model

### 6.1 Type Enum

```rust
enum Value {
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    String(String),
    Bytes(Vec<u8>),
    Array(Vec<Value>),
    Object(HashMap<String, Value>)
}
```

This is the **only** public value model.

### 6.2 Type Rules

| Rule | Description |
|------|-------------|
| JSON subset | JSON is strict subset (no Bytes without encoding) |
| No coercion | No implicit type coercions |
| No lossy conversion | Numbers don't auto-promote |
| Type distinction | Bytes are not strings |

### 6.3 Equality Semantics

Structural equality only. No total ordering defined.

| Type | Equality Rule |
|------|---------------|
| `Null` | Equal to `Null` only |
| `Bool` | `true == true`, `false == false` |
| `Int` | Exact integer equality |
| `Float` | IEEE-754: `NaN != NaN`, `-0.0 == 0.0` |
| `String` | Byte-wise UTF-8 equality |
| `Bytes` | Byte-wise equality |
| `Array` | Element-wise recursive, length must match |
| `Object` | Same key set + recursive value equality |

### 6.4 Size Limits

| Limit | Default | Error |
|-------|---------|-------|
| `max_key_bytes` | 1024 | `InvalidKey` |
| `max_string_bytes` | 16 MiB | `ConstraintViolation` |
| `max_bytes_len` | 16 MiB | `ConstraintViolation` |
| `max_value_bytes_encoded` | 32 MiB | `ConstraintViolation` |
| `max_array_len` | 1,000,000 | `ConstraintViolation` |
| `max_object_entries` | 1,000,000 | `ConstraintViolation` |
| `max_nesting_depth` | 128 | `ConstraintViolation` |
| `max_vector_dim` | 8192 | `ConstraintViolation` |

### 6.5 Key Constraints

- Unicode strings (UTF-8)
- Must be valid UTF-8
- Must not contain `\u0000` (NUL)
- Length: 1 to `max_key_bytes` UTF-8 bytes
- `_strata/` prefix is reserved (illegal in facade APIs)

---

## 7. Versioned\<T\> Contract

### 7.1 Structure

```rust
struct Versioned<T> {
    value: T,
    version: Version,
    timestamp: u64  // microseconds since Unix epoch
}
```

### 7.2 Version Tagged Union

```rust
enum Version {
    Txn(u64),      // For KV, JSON, Vector, Run
    Sequence(u64), // For Events (append-only)
    Counter(u64)   // For StateCell (per-entity CAS)
}
```

**Why tagged union**: Event sequence `5` and transaction ID `5` are not comparable. The wire must preserve this distinction.

### 7.3 Wire Encoding

```json
{
  "value": <Value>,
  "version": { "type": "txn", "value": 123 },
  "timestamp": 1700000000000000
}
```

### 7.4 Timestamp Contract

- All timestamps: Unix epoch **microseconds** as `u64`
- Monotonic within a run
- Always attached to `Versioned<T>`

---

## 8. Error Model

### 8.1 Error Codes

| Code | Meaning | Category |
|------|---------|----------|
| `NotFound` | Entity or key not found | |
| `WrongType` | Wrong primitive or value type | Structural |
| `InvalidKey` | Key syntax invalid | Structural |
| `InvalidPath` | JSON path invalid | Structural |
| `HistoryTrimmed` | Requested version no longer retained | |
| `ConstraintViolation` | Schema/shape/invariant violation | Structural |
| `Conflict` | CAS failure, transaction conflict | Temporal |
| `SerializationError` | Value encode/decode failure | |
| `StorageError` | Disk, WAL, or IO failure | |
| `InternalError` | Bug or invariant violation | |

### 8.2 Wire Error Shape

```json
{
  "ok": false,
  "error": {
    "code": "HistoryTrimmed",
    "message": "Requested version no longer retained",
    "details": {
      "requested": { "type": "txn", "value": 100 },
      "earliest_retained": { "type": "txn", "value": 150 }
    }
  }
}
```

### 8.3 ConstraintViolation Reasons

- `value_too_large`
- `nesting_too_deep`
- `key_too_long`
- `vector_dim_exceeded`
- `vector_dim_mismatch`
- `root_not_object`
- `reserved_prefix`

### 8.4 Error-Producing Conditions

| Condition | Error Code |
|-----------|------------|
| Invalid UTF-8 in key | `InvalidKey` |
| NUL byte in key | `InvalidKey` |
| Key exceeds max length | `InvalidKey` |
| Reserved prefix (`_strata/`) | `InvalidKey` |
| Empty key | `InvalidKey` |
| Value exceeds size limits | `ConstraintViolation` |
| Nesting exceeds max depth | `ConstraintViolation` |
| JSON path syntax error | `InvalidPath` |
| JSON root set to non-Object | `ConstraintViolation` |
| Vector dimension mismatch | `ConstraintViolation` |
| CAS on wrong primitive type | `WrongType` |
| `incr` on non-Int value | `WrongType` |

---

## 9. Wire Encoding Contract

### 9.1 Request Envelope

```json
{
  "id": "client-generated-request-id",
  "op": "kv.set",
  "params": { "key": "x", "value": 123 }
}
```

### 9.2 Response Envelope

**Success:**
```json
{
  "id": "client-generated-request-id",
  "ok": true,
  "result": <operation-specific>
}
```

**Error:**
```json
{
  "id": "client-generated-request-id",
  "ok": false,
  "error": { "code": "...", "message": "...", "details": {...} }
}
```

### 9.3 Value Type Mapping

| Strata Value | JSON Representation |
|--------------|---------------------|
| `Null` | `null` |
| `Bool` | `true` / `false` |
| `Int(i64)` | JSON number |
| `Float(f64)` | JSON number (finite) or `$f64` wrapper |
| `String` | JSON string |
| `Bytes` | `{"$bytes": "<base64>"}` |
| `Array` | JSON array |
| `Object` | JSON object |

### 9.4 Special Value Wrappers

**Float special values (`$f64`):**
```json
{"$f64": "NaN"}
{"$f64": "+Inf"}
{"$f64": "-Inf"}
{"$f64": "-0.0"}
```

**Bytes (`$bytes`):**
```json
{"$bytes": "SGVsbG8gV29ybGQ="}
```

**Absent value (`$absent`):**
```json
{"$absent": true}
```

Used for CAS when `expected = None` (key missing, not value null).

### 9.5 Operation Names

**Facade:**
`kv.set`, `kv.get`, `kv.getv`, `kv.mget`, `kv.mset`, `kv.delete`, `kv.exists`, `kv.exists_many`, `kv.incr`,
`json.set`, `json.get`, `json.getv`, `json.del`, `json.merge`,
`event.add`, `event.range`,
`vector.set`, `vector.get`, `vector.del`,
`state.cas_set`, `state.get`,
`history.list`, `history.get_at`, `history.latest_version`,
`run.list`, `run.use`,
`system.capabilities`

**Substrate:**
`substrate.kv.put`, `substrate.kv.get`, `substrate.kv.get_at`, etc.
`substrate.run.create`, `substrate.run.get`, `substrate.run.list`, `substrate.run.close`
`substrate.retention.get`, `substrate.retention.set`
`txn.begin`, `txn.commit`, `txn.rollback`

---

## 10. CLI Contract

### 10.1 Command Interface

```bash
# KV
strata set x 123
strata get x                    # prints: 123
strata get missing              # prints: (nil)
strata mget a b c               # prints: [123, (nil), "hello"]
strata mset a 1 b 2 c 3         # atomic multi-set
strata delete x y               # prints: (integer) 2
strata exists x                 # prints: (integer) 1
strata incr counter             # prints: (integer) 1

# JSON
strata json.set doc $.name "Alice"
strata json.get doc $.name      # prints: "Alice"

# Events
strata xadd stream '{"type":"login"}'  # prints: {"type":"sequence","value":1}

# Vectors
strata vset doc1 "[0.1, 0.2, 0.3]" '{"tag":"test"}'
strata vget doc1
strata vdel doc1               # prints: (integer) 1

# State (CAS)
strata cas.set mykey null 123  # prints: (integer) 1 (created)
strata cas.get mykey           # prints: 123
strata cas.set mykey 123 456   # prints: (integer) 1 (updated)

# History
strata history mykey --limit 10
```

### 10.2 Output Conventions

| Output Type | Format |
|-------------|--------|
| Missing value | `(nil)` |
| Integer/count | `(integer) N` |
| Boolean | `(integer) 0` or `(integer) 1` |
| String | `"text"` |
| Null value | `null` |
| Object/Array | JSON formatted |
| Bytes | `{"$bytes": "<base64>"}` |
| Error | JSON on stderr, non-zero exit code |

### 10.3 Argument Parsing

| Input | Parsed as |
|-------|-----------|
| `123` | `Int(123)` |
| `1.23` | `Float(1.23)` |
| `"hello"` | `String("hello")` (quotes stripped) |
| `hello` | `String("hello")` (bare word) |
| `true` / `false` | `Bool` |
| `null` | `Null` |
| `{...}` | Object (must be valid JSON) |
| `[...]` | Array (must be valid JSON) |
| `b64:SGVsbG8=` | `Bytes` (base64 decoded) |

### 10.4 Run Scoping

```bash
strata --run=default set x 123      # explicit default
strata set x 123                     # implicit default
strata --run=my-run-id set x 123    # custom run
```

**CLI is facade-only.** Substrate operations are not exposed via CLI.

---

## 11. SDK Mapping

### 11.1 Python

```python
# Value mapping
Null    -> None
Bool    -> bool
Int     -> int
Float   -> float
String  -> str
Bytes   -> bytes
Array   -> list
Object  -> dict[str, Any]

# Usage
db = Strata.open()
db.set("x", 123)
db.get("x")           # -> 123
db.getv("x")          # -> Versioned(value=123, version=..., timestamp=...)
```

### 11.2 JavaScript

```javascript
// Value mapping
Null    -> null
Bool    -> boolean
Int     -> number | BigInt (outside safe integer range)
Float   -> number
String  -> string
Bytes   -> Uint8Array
Array   -> Array<any>
Object  -> Record<string, any>
```

### 11.3 Rust

```rust
use strata::{Value, Versioned, Version, StrataError};

let db = Strata::open()?;
db.set("x", Value::Int(123))?;
let v: Option<Value> = db.get("x")?;
let vv: Option<Versioned<Value>> = db.getv("x")?;
```

### 11.4 SDK Requirements

All SDKs MUST:
- Preserve numeric widths (`i64`, `f64`)
- Preserve `Bytes` vs `String` distinction
- Preserve `None`/missing vs `Value::Null` distinction
- Preserve `Versioned` wrapper shape
- Surface structured errors with code, message, details
- Use the same operation names as the facade API

---

## 12. Transaction Semantics

### 12.1 Isolation Level

Transactions provide **snapshot isolation**.

- Reads see consistent snapshot as of transaction start
- Writes use OCC (optimistic concurrency control) validation at commit
- No guarantee of serializability
- Write-write conflicts return `Conflict` error

### 12.2 Transaction Scope

Transactions are **scoped to a single run**.

- A transaction has a `run_id`
- All operations in that transaction must use that run
- Cross-run transactions are not supported

### 12.3 Auto-Commit

In facade mode, each operation auto-commits:

```
set("x", 1)  →  begin(); kv_put(...); commit()
```

### 12.4 Explicit Transactions

```rust
let txn = db.begin(run_id)?;
txn.kv_put("a", 1)?;
txn.kv_put("b", 2)?;
txn.commit()?;  // or txn.rollback()
```

### 12.5 Transaction Properties

| Property | Guarantee |
|----------|-----------|
| Atomic | All or nothing |
| Isolated | Snapshot isolation |
| Deterministic | Same operations produce same state |
| Idempotent | WAL replay produces identical state |

---

## 13. Run Semantics

### 13.1 Default Run

The default run **always exists implicitly** with the canonical name `"default"`.

- Run ID: `"default"` (literal string, not a UUID)
- Created lazily on first write or on open
- There is no moment where it does not exist
- Facade operations always target this run
- The default run **cannot be closed**

### 13.2 Run Hiding Rule

- **Facade APIs**: NEVER carry `run_id`. Implicitly scoped to default run.
- **Substrate APIs**: ALWAYS require explicit `run_id`.

### 13.3 Run Lifecycle

| Operation | Layer | Description |
|-----------|-------|-------------|
| `run_create(metadata)` | Substrate | Creates new run |
| `run_close(run_id)` | Substrate | Marks run as closed |
| `run_list()` | Both | Lists all runs |
| `use_run(run_id)` | Facade | Escape hatch, errors if not exists |

### 13.4 RunId Format

UUIDs in lowercase hyphenated format:
```
f47ac10b-58cc-4372-a567-0e02b2c3d479
```

Exception: `"default"` is a literal string.

---

## 14. History & Retention

### 14.1 History Access

| Primitive | History API | Facade |
|-----------|-------------|--------|
| KV | `kv_history(run, key)` | `history(key)` |
| JSON | `json_history(run, key)` | Substrate only |
| Vector | `vector_history(run, key)` | Substrate only |
| Events | `event_range(run, stream)` | `xrange(stream)` |
| State | `state_history(run, key)` | Substrate only |

**Facade `history()` is KV-only.**

### 14.2 History Ordering

Newest first (descending by version).

### 14.3 Pagination

```rust
history(key, limit: Option<u64>, before: Option<Version>)
```

- `limit`: Maximum versions to return
- `before`: Return versions older than this (exclusive)

### 14.4 Retention Policy

```rust
enum RetentionPolicy {
    KeepAll,              // Default
    KeepLast(u64),        // Keep N most recent
    KeepFor(Duration),    // Keep within time window
    Composite(Vec<RetentionPolicy>)
}
```

- Retention is configured **per-run**
- Default policy is `KeepAll`
- Per-key retention is NOT supported in M11

---

## 15. Testing Strategy

### 15.1 Contract Conformance Tests

| Test Category | Description |
|---------------|-------------|
| Facade-Substrate parity | Every facade op produces same result as desugared substrate |
| Value round-trip | All 8 types survive encode/decode |
| Wire encoding | `$bytes`, `$f64`, `$absent` wrappers work correctly |
| Error surface | All error conditions produce correct codes |
| Type distinction | `Int(1)` != `Float(1.0)`, `Bytes` != `String` |

### 15.2 Edge Case Tests

| Test | Description |
|------|-------------|
| Float specials | `NaN`, `+Inf`, `-Inf`, `-0.0` preserve correctly |
| Empty values | Empty string, empty bytes, empty array, empty object |
| Size limits | Values at and beyond limits |
| Key validation | NUL bytes, reserved prefix, max length |
| CAS semantics | `None` vs `Null` distinction with `$absent` |

### 15.3 Determinism Tests

| Test | Description |
|------|-------------|
| Replay equality | WAL replay produces byte-identical state |
| Operation sequence | Same ops in same order = same result |
| Timestamp independence | Different timestamps, same logical state |

### 15.4 SDK Parity Tests

| Test | Description |
|------|-------------|
| Python mapping | All value types map correctly |
| JavaScript mapping | BigInt for large integers |
| Rust direct | Native enum usage |
| Cross-SDK | Same operations produce same results |

---

## 16. Known Limitations

### 16.1 Not Supported in M11

| Feature | Reason |
|---------|--------|
| Diff semantics | Deferred |
| Search ranking | Deferred |
| TTL/EXPIRE | Deferred |
| Consumer groups | Deferred |
| Vector search DSL | Deferred |
| JSONPath filters/wildcards | Deferred |
| MessagePack wire | Optional, not required |
| Run deletion | Deferred to GC milestone |
| Per-key retention | Deferred |
| Serializable isolation | Snapshot isolation only |

### 16.2 Intentional Constraints

| Constraint | Rationale |
|------------|-----------|
| No total ordering on Value | Comparison semantics are complex; equality is sufficient |
| Single run per transaction | Simplifies consistency model |
| Facade is KV-history only | Other primitives use specific APIs |

---

## 17. Future Extension Points

### 17.1 Planned Extensions

| Extension | Target Milestone |
|-----------|------------------|
| Python SDK implementation | M14 |
| MessagePack wire format | M12+ |
| Vector search DSL | TBD |
| JSONPath advanced features | TBD |
| TTL/EXPIRE semantics | TBD |
| Consumer groups | TBD |

### 17.2 Extension Rules

- New operations MUST follow existing naming patterns
- New value types require major version
- New error codes require documentation update
- Wire wrappers follow `$name` pattern

---

## 18. Success Criteria Checklist

### 18.1 API Implementation

- [ ] Facade API: All operations implemented (KV, JSON, Event, Vector, State, History)
- [ ] Substrate API: All primitives exposed with explicit run/version/txn
- [ ] `mset`, `json_getv`, `capabilities` operations implemented
- [ ] Escape hatches: `getv`, `use_run`, `db.substrate()` working

### 18.2 Value Model

- [ ] All 8 value types implemented
- [ ] No implicit type coercions
- [ ] Float edge cases handled (`NaN`, `±Inf`, `-0.0`)
- [ ] Size limits enforced with `ConstraintViolation`

### 18.3 Wire Encoding

- [ ] JSON encoding mandatory and working
- [ ] `$bytes` wrapper for Bytes
- [ ] `$f64` wrapper for non-finite floats
- [ ] `$absent` wrapper for CAS missing
- [ ] Round-trip preserves exact types

### 18.4 Error Model

- [ ] All error codes defined and used
- [ ] Structured `details` payloads
- [ ] `ConstraintViolation` reason codes

### 18.5 CLI

- [ ] Redis-like command interface
- [ ] Output conventions match spec
- [ ] Argument parsing rules implemented
- [ ] `--run` option working

### 18.6 Contract Documentation

- [ ] Facade→Substrate desugaring documented
- [ ] Contract stability guarantees documented
- [ ] Determinism guarantee documented
- [ ] All invariants documented

---

## 19. Document History

| Date | Version | Changes |
|------|---------|---------|
| 2026-01-21 | 1.0 | Initial architecture document based on M11_CONTRACT.md |

---

**This document is the architectural specification for M11. All implementations must conform to it.**
