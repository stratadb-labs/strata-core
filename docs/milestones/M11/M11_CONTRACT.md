# M11: Public API & SDK Contract Specification

**Status**: Freeze Candidate
**Version**: 1.0
**Last Updated**: 2026-01-21
**Goal**: Freeze the public contract so all downstream surfaces (wire, CLI, Rust SDK, server) use consistent semantics

---

## Table of Contents

1. [What M11 Is](#1-what-m11-is)
2. [Contract Stability Guarantees](#2-contract-stability-guarantees)
3. [Determinism Guarantee](#3-determinism-guarantee)
4. [The Two-Layer API Model](#4-the-two-layer-api-model)
5. [Canonical Value Model](#5-canonical-value-model)
6. [Key Constraints](#6-key-constraints)
7. [Versioned\<T\> Specification](#7-versionedt-specification)
8. [Error Model](#8-error-model)
9. [Undefined Behavior](#9-undefined-behavior)
10. [Facade API Operations](#10-facade-api-operations)
11. [Substrate API Operations](#11-substrate-api-operations)
12. [Facade → Substrate Desugaring](#12-facade--substrate-desugaring)
13. [Wire Encoding Contract](#13-wire-encoding-contract)
14. [CLI Contract](#14-cli-contract)
15. [SDK Mapping](#15-sdk-mapping)
16. [Run Semantics](#16-run-semantics)
17. [Transaction Semantics](#17-transaction-semantics)
18. [History & Retention](#18-history--retention)
19. [What M11 Does NOT Freeze](#19-what-m11-does-not-freeze)
20. [Success Criteria](#20-success-criteria)

---

## 1. What M11 Is

M11 freezes the **public contract** of Strata. After M11, every downstream surface—wire protocol, CLI, Rust SDK, server—must conform to this contract. Breaking changes require a major version bump.

The contract has two layers:
- **Facade API**: Redis-like surface for 95% of users who want familiar patterns
- **Substrate API**: Power-user surface exposing runs, versions, transactions, primitives

**Key architectural invariant**: Every facade call **desugars to exactly one substrate call**. No magic, no hidden semantics.

---

## 2. Contract Stability Guarantees

This section defines what "frozen" means and what constitutes a breaking change.

### 2.1 What Is Frozen

After M11, the following are **stable** and require a major version bump to change:

- Operation names (facade and substrate)
- Parameter names and shapes
- Return shapes
- Error codes
- Error payload shapes (`details` field structure)
- Value model (types and semantics)
- Wire encodings (JSON wrappers, envelope structure)
- Version types (`txn`, `sequence`, `counter`)
- Timestamp units (microseconds)
- Default behaviors (auto-commit, default run targeting)
- CLI command names and output formats
- SDK method names and signatures

### 2.2 What Is NOT Frozen

The following may change without a major version bump:

- Performance characteristics
- Indexing strategies
- Storage layout and file formats
- WAL format
- Internal algorithms
- Optimization heuristics
- Compaction behavior
- Memory usage patterns
- Concurrency implementation details

**Rule**: If it affects what users observe through the API, it is frozen. If it only affects how fast or efficiently those observations are produced, it is not frozen.

---

## 3. Determinism Guarantee

Strata provides a **determinism guarantee** that is fundamental to its design.

### 3.1 Core Guarantee

Given the same sequence of substrate operations, Strata **must** produce the same state.

### 3.2 Implications

- **Replay is deterministic**: Replaying a WAL produces identical state
- **Timestamps do not affect logical state**: Timestamps are metadata, not inputs to state transitions
- **Wire encoding does not affect logical semantics**: JSON and MessagePack produce identical logical results
- **Compaction is invisible**: Compacted state is indistinguishable from uncompacted state (except for trimmed history)

### 3.3 What This Enables

- Debugging via replay
- State verification via hash comparison
- Distributed consistency verification
- Test reproducibility

---

## 4. The Two-Layer API Model

### 4.1 Facade API (Default Mode)

The facade hides Strata's complexity behind Redis-familiar patterns.

**What's hidden by default:**
- Runs (everything targets the implicit default run, named `"default"`)
- Versions (writes don't return versions, reads return values not `Versioned<T>`)
- Primitive selection (users say `set`, not `kv_put`)
- Transactions (each call auto-commits)
- History (no version access unless explicitly requested)

**What users see:**
```
set(key, value) → ()
get(key) → Option<Value>
delete(keys) → count
exists(key) → bool
```

This is the Redis mental model: key → value, nothing else.

### 4.2 Substrate API (Advanced Mode)

The substrate exposes everything explicitly.

**What's visible:**
- Explicit `run_id` on every operation
- Explicit primitive selection (`kv_put`, `json_set`, `event_append`)
- `Versioned<T>` returns on all reads
- Transaction control (`begin`, `commit`, `rollback`)
- Full history access (`history`, `get_at`)
- Retention policy access
- Run lifecycle (`create_run`, `close_run`)
- Trace operations

**The desugaring rule:**
```
Facade: set("key", value)
   ↓ desugars to
Substrate: begin_txn(); kv_put("default", "key", value); commit()
```

**Escape hatches from facade to substrate:**
- `getv(key)` → returns `Versioned<Value>` instead of `Value`
- `use_run(run_id)` → scope operations to a specific run
- `db.substrate().kv_put(...)` → direct substrate access

### 4.3 Facade Invariants

The Facade API is a **lossless projection** of the Substrate API. It adds no semantics, only defaults.

**Invariants (Frozen):**

1. **Mechanically desugarable**: Every facade operation must map to a deterministic sequence of substrate operations. No facade operation may have behavior that cannot be expressed in substrate terms.

2. **No new semantics**: The facade must never introduce semantic behavior that does not exist in the substrate. It only provides defaults and convenience.

3. **No hidden errors**: The facade must never swallow, transform, or hide errors from the substrate. All substrate errors must surface to facade callers.

4. **No ordering changes**: The facade must not reorder operations or change consistency guarantees relative to the substrate.

5. **No implicit magic**: If something happens, it must be traceable to an explicit substrate operation.

These invariants ensure that the facade is a true convenience layer, not a separate system with its own rules.

---

## 5. Canonical Value Model

Strata has exactly one public value model:

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

### 5.1 Core Rules

- This is the only public value model
- JSON is a strict subset (no Bytes in JSON documents without encoding)
- No implicit type coercions
- No lossy conversions
- Numbers don't auto-promote (Int stays Int, Float stays Float)
- Bytes are not strings (distinct wire representations)

### 5.2 Value Comparison Semantics

**Structural equality only.** There is no total ordering defined over `Value`.

**What is defined:**
- Equality comparison (for CAS, deduplication, testing)
- Structural equality is recursive for Array and Object

**What is NOT defined:**
- No `<`, `>`, `<=`, `>=` ordering over Value
- No implicit coercions during comparison
- No numeric widening (`Int(1)` does not equal `Float(1.0)`)
- No locale-dependent string comparisons (byte-wise UTF-8 comparison only)

**Equality rules by type:**
- `Null`: Equal to `Null` only
- `Bool`: `true == true`, `false == false`
- `Int`: Exact integer equality
- `Float`: IEEE-754 equality (`NaN != NaN`, `-0.0 == 0.0`)
- `String`: Byte-wise UTF-8 equality
- `Bytes`: Byte-wise equality
- `Array`: Element-wise recursive equality, length must match
- `Object`: Same key set + recursive value equality (key order irrelevant)

**Future ordering APIs**: Any future comparison or ordering APIs must be explicit and opt-in. They will not be implied by the Value model.

### 5.4 Float Specification

Float is IEEE-754 binary64 (f64).

**Allowed values:**
- All finite floats
- `NaN` (all payload variants)
- `+Inf`, `-Inf`
- `-0.0` (negative zero)

**Equality semantics (for CAS):**
- Uses IEEE-754 equality: `NaN != NaN`
- `-0.0 == 0.0` under IEEE equality (but `-0.0` is preserved in storage and wire)

### 5.5 Size Limits

Configurable at DB open; enforced by engine and wire decoding.

| Limit | Default | Description |
|-------|---------|-------------|
| `max_key_bytes` | 1024 | Maximum key length in UTF-8 bytes |
| `max_string_bytes` | 16 MiB | Maximum string value size |
| `max_bytes_len` | 16 MiB | Maximum bytes value size |
| `max_value_bytes_encoded` | 32 MiB | Maximum encoded value size |
| `max_array_len` | 1,000,000 | Maximum array elements |
| `max_object_entries` | 1,000,000 | Maximum object entries |
| `max_nesting_depth` | 128 | Maximum nesting depth |
| `max_vector_dim` | 8192 | Maximum vector dimensions |

**Violations** return `ConstraintViolation` with reason codes:
- `value_too_large`
- `nesting_too_deep`
- `key_too_long`
- `vector_dim_exceeded`

---

## 6. Key Constraints

### 6.1 Key Format

- Keys are Unicode strings (UTF-8)
- Must be valid UTF-8
- Must not contain `\u0000` (NUL)
- Length: 1 to `max_key_bytes` UTF-8 bytes
- Allowed characters: any Unicode scalar value except NUL

### 6.2 Reserved Prefixes

- `_strata/` is reserved for system namespace
- Keys starting with `_strata/` are **illegal** in facade APIs
- Substrate APIs may access system namespace only via explicit system operations

### 6.3 Errors

- `InvalidKey` for: reserved prefix, contains NUL, empty, too long, invalid UTF-8

---

## 7. Versioned\<T\> Specification

Every read can return versioned form:

```rust
struct Versioned<T> {
    value: T,
    version: Version,
    timestamp: u64  // microseconds since Unix epoch
}
```

### 7.1 Version Tagged Union

```rust
enum Version {
    Txn(u64),      // For KV, JSON, Vector, Run (transaction-based)
    Sequence(u64), // For Events (append-only sequence numbers)
    Counter(u64)   // For StateCell (per-entity CAS counter)
}
```

**Wire encoding:**
```json
{
  "type": "txn" | "sequence" | "counter",
  "value": 123
}
```

**Why tagged union:** Event sequence `5` and transaction ID `5` are not comparable. Treating them as both "monotonic" erases meaning and invites bugs. The wire must preserve this distinction.

### 7.2 Timestamp

- All timestamps are Unix epoch **microseconds** encoded as `u64`
- Monotonic within a run
- Always attached to `Versioned<T>`

---

## 8. Error Model

### 8.1 Canonical Error Codes

| Code | Meaning | Category |
|------|---------|----------|
| `NotFound` | Entity or key not found | |
| `WrongType` | Wrong primitive or value type | Structural |
| `InvalidKey` | Key syntax invalid | Structural |
| `InvalidPath` | JSON path invalid | Structural |
| `HistoryTrimmed` | Requested version no longer retained | |
| `ConstraintViolation` | Schema/shape/invariant violation | Structural |
| `Conflict` | CAS failure, transaction conflict, version mismatch | Temporal |
| `SerializationError` | Value encode/decode failure | |
| `StorageError` | Disk, WAL, or IO failure | |
| `InternalError` | Bug or invariant violation | |

### 8.2 Conflict vs ConstraintViolation

- **Conflict**: Temporal failures — CAS fails, transaction conflict, version mismatch, concurrent modification
- **ConstraintViolation**: Structural failures — type mismatch, invalid value shape, schema violation, root-not-object, size limits exceeded

### 8.3 Wire Error Shape

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

### 8.4 ConstraintViolation Reason Codes

The `details` field includes a `reason` for `ConstraintViolation`:
- `value_too_large`
- `nesting_too_deep`
- `key_too_long`
- `vector_dim_exceeded`
- `vector_dim_mismatch`
- `root_not_object`
- `reserved_prefix`

---

## 9. Undefined Behavior

This section explicitly defines behaviors that are **errors**, not silent failures or best-effort operations. Strata does not have undefined behavior in the C/C++ sense. All invalid inputs produce explicit, documented errors.

### 9.1 Principle

**No silent failures. No best-effort fallbacks.**

When an operation cannot be performed correctly, Strata returns an error. It never:
- Silently truncates data
- Silently coerces types
- Returns partial results without indication
- Guesses user intent

### 9.2 Error-Producing Conditions

The following conditions **always** produce explicit errors:

| Condition | Error Code |
|-----------|------------|
| Invalid UTF-8 in key | `InvalidKey` |
| NUL byte in key | `InvalidKey` |
| Key exceeds max length | `InvalidKey` |
| Key uses reserved prefix (`_strata/`) | `InvalidKey` |
| Empty key | `InvalidKey` |
| Value exceeds size limits | `ConstraintViolation` |
| Nesting exceeds max depth | `ConstraintViolation` |
| JSON path syntax error | `InvalidPath` |
| JSON path targets non-existent intermediate | `InvalidPath` |
| JSON root set to non-Object | `ConstraintViolation` |
| Vector dimension mismatch | `ConstraintViolation` |
| Vector dimension exceeds max | `ConstraintViolation` |
| Comparing incompatible version types | `WrongType` |
| Operating on closed run | `ConstraintViolation` |
| Using stale/committed transaction handle | `Conflict` |
| `use_run` on non-existent run | `NotFound` |
| CAS on wrong primitive type | `WrongType` |
| `incr` on non-Int value | `WrongType` |

### 9.3 Guarantees

- All errors are surfaced through the error model (Section 8)
- All errors include a stable code, human message, and structured details where applicable
- No operation has "undefined" behavior that varies by implementation

---

## 10. Facade API Operations

### 10.1 KV Operations

| Operation | Signature | Return | Notes |
|-----------|-----------|--------|-------|
| `set` | `(key, value)` | `()` | Overwrites, creates new version internally |
| `get` | `(key)` | `Option<Value>` | Returns latest value or None |
| `getv` | `(key)` | `Option<Versioned<Value>>` | Escape hatch for version info |
| `mget` | `(keys[])` | `Vec<Option<Value>>` | Order preserved, None for missing |
| `mset` | `(entries: Vec<(String, Value)>)` | `()` | Atomic multi-set |
| `delete` | `(keys[])` | `u64` | Count of keys that **existed** |
| `exists` | `(key)` | `bool` | Human-friendly boolean |
| `exists_many` | `(keys[])` | `u64` | Count of keys that exist |
| `incr` | `(key, delta=1)` | `i64` | **Atomic** increment, missing = 0, must be Int |

**incr atomicity**: `incr` is engine-level atomic, not a facade transaction macro. Two concurrent increments produce the correct result, not lost updates.

**mset atomicity**: If any entry fails validation, the entire operation fails and no changes are applied.

### 10.2 JSON Operations

| Operation | Signature | Return | Notes |
|-----------|-----------|--------|-------|
| `json_set` | `(key, path, value)` | `()` | Root must be Object |
| `json_get` | `(key, path)` | `Option<Value>` | Returns value at path |
| `json_getv` | `(key, path)` | `Option<Versioned<Value>>` | Returns document-level version |
| `json_del` | `(key, path)` | `u64` | Count of elements removed |
| `json_merge` | `(key, path, value)` | `()` | RFC 7396 JSON Merge Patch semantics |

**Path syntax (JSONPath-style):**
- Root: `$` (means entire document)
- Object field: `$.a.b`
- Array index: `$.items[0]`
- Array append: `$.items[-]` (for `json_set` only)

**Path rules:**
- `$` is valid and means "entire document"
- Setting root replaces the whole document
- Root must always be Object
- Deleting root is forbidden (use `delete` to remove the key)
- Negative indices like `[-1]` are **not supported** (return `InvalidPath`)

**json_merge semantics (RFC 7396):**
- `null` deletes a field
- Objects merge recursively
- Arrays replace (not merge)
- Scalars replace

**json_getv version scope:** Returns the version and timestamp of the **document**, not the subpath. Paths do not have independent version identities.

**Not supported (deferred):** Filters `[?()]`, unions, wildcards, recursive descent.

### 10.3 Event Operations

| Operation | Signature | Return | Notes |
|-----------|-----------|--------|-------|
| `xadd` | `(stream, payload: Object)` | `Version` | Returns event ID as Version |
| `xrange` | `(stream, start?, end?, limit?)` | `Vec<Versioned<Value>>` | |

**xrange default behavior:** No bounds = all events. This can be expensive for large streams.

**Payload rules:**
- Empty object `{}` is allowed
- Bytes are allowed in payloads (encoded via `$bytes` wrapper on JSON wire)

### 10.4 Vector Operations

| Operation | Signature | Return | Notes |
|-----------|-----------|--------|-------|
| `vset` | `(key, vector: f32[], metadata: Object)` | `()` | |
| `vget` | `(key)` | `Option<Versioned<{vector, metadata}>>` | **Returns Versioned** |
| `vdel` | `(key)` | `bool` | |

**Dimension rules:**
- Vector dimensions: 1 to `max_vector_dim` (default 8192)
- If key exists with different dimension: return `ConstraintViolation` with reason `vector_dim_mismatch`
- Dimension changes are not allowed; delete and re-create if needed

### 10.5 State Operations (CAS)

| Operation | Signature | Return | Notes |
|-----------|-----------|--------|-------|
| `cas_set` | `(key, expected: Option<Value>, new: Value)` | `bool` | Compare-and-swap |
| `cas_get` | `(key)` | `Option<Value>` | |

**CAS semantics:**
- `expected = None` means "only set if key is missing" (create-if-not-exists)
- `expected = Some(Value::Null)` means "only set if current value is null"
- Comparison uses structural equality (see below)

**Value equality for CAS:**
- `Null`, `Bool`, `Int`, `String`, `Bytes`: exact equality
- `Float`: IEEE-754 equality (`NaN != NaN`, `-0.0 == 0.0`)
- `Array`: element-wise recursive equality
- `Object`: key-set equality + recursive value equality (order irrelevant)

### 10.6 History Operations (Advanced)

| Operation | Signature | Return | Notes |
|-----------|-----------|--------|-------|
| `history` | `(key, limit?: u64, before?: Version)` | `Vec<Versioned<Value>>` | All retained versions |
| `get_at` | `(key, version)` | `Value \| HistoryTrimmed` | |
| `latest_version` | `(key)` | `Option<Version>` | |

**History ordering:** Newest first (descending by version).

**Pagination:** Use `limit` and `before` for pagination. `before` excludes that version.

**Scope:** Facade history operations apply to **KV keys only**. Other primitives use their specific APIs (`xrange` for events, etc.).

### 10.7 Run Operations (Advanced)

| Operation | Signature | Return | Notes |
|-----------|-----------|--------|-------|
| `runs` | `()` | `Vec<RunInfo>` | List all runs |
| `use_run` | `(run_id)` | `ScopedFacade` | Scope operations to run |

**Run lifecycle:** `create_run` and `close_run` are **substrate-only**. Facade hides run lifecycle.

**use_run behavior:** If `run_id` does not exist, returns `NotFound`. No lazy creation.

### 10.8 Capability Discovery

| Operation | Signature | Return | Notes |
|-----------|-----------|--------|-------|
| `capabilities` | `()` | `Capabilities` | Returns system capabilities |

**Capabilities object:**
```json
{
  "version": "1.0.0",
  "operations": ["kv.set", "kv.get", ...],
  "limits": {
    "max_key_bytes": 1024,
    "max_string_bytes": 16777216,
    "max_bytes_len": 16777216,
    "max_value_bytes_encoded": 33554432,
    "max_array_len": 1000000,
    "max_object_entries": 1000000,
    "max_nesting_depth": 128,
    "max_vector_dim": 8192
  },
  "encodings": ["json"],
  "features": ["history", "retention", "cas"]
}
```

This allows clients to discover what the server supports, enabling graceful degradation and forward compatibility.

---

## 11. Substrate API Operations

### 11.1 Design Philosophy

The Substrate API is the canonical semantic contract. It must:
- Expose all primitives explicitly
- Expose all versioning
- Expose all run scoping
- Expose all transactional semantics
- Be deterministic and replayable
- Be minimal, not friendly
- Be unambiguous and stable

### 11.2 Core Types

```rust
struct RunId(String)  // UUID format: lowercase, hyphenated

struct RunInfo {
    run_id: RunId,
    created_at: u64,  // microseconds
    metadata: Value,
    state: RunState   // "active" | "closed"
}

enum RunState {
    Active,
    Closed
}
```

### 11.3 KVStore

```rust
kv_put(run, key, value) -> Version
kv_get(run, key) -> Option<Versioned<Value>>
kv_get_at(run, key, version) -> Versioned<Value> | HistoryTrimmed
kv_delete(run, key) -> bool
kv_exists(run, key) -> bool
kv_history(run, key, limit?, before?) -> Vec<Versioned<Value>>
kv_incr(run, key, delta) -> i64  // atomic
kv_cas_version(run, key, expected_version, new_value) -> bool
kv_cas_value(run, key, expected_value, new_value) -> bool
```

### 11.4 JsonStore

```rust
json_set(run, key, path, value) -> Version
json_get(run, key, path) -> Option<Versioned<Value>>
json_delete(run, key, path) -> u64
json_merge(run, key, path, value) -> Version
json_history(run, key, limit?, before?) -> Vec<Versioned<Value>>
```

### 11.5 EventLog

```rust
event_append(run, stream, payload: Value::Object) -> Version
event_range(run, stream, start?, end?, limit?) -> Vec<Versioned<Value>>
```

### 11.6 StateCell

```rust
state_get(run, key) -> Option<Versioned<Value>>
state_set(run, key, value) -> Version
state_cas(run, key, expected, new) -> bool
```

### 11.7 VectorStore

```rust
vector_set(run, key, vector: Vec<f32>, metadata: Value::Object) -> Version
vector_get(run, key) -> Option<Versioned<{vector, metadata}>>
vector_delete(run, key) -> bool
vector_history(run, key, limit?, before?) -> Vec<Versioned<Value>>
```

### 11.8 TraceStore

```rust
trace_record(run, trace_type: String, payload: Value) -> Version
trace_get(run, id) -> Option<Versioned<Value>>
trace_range(run, start?, end?, limit?) -> Vec<Versioned<Value>>
```

Trace operations are **substrate-only** for M11.

### 11.9 RunIndex

```rust
run_create(metadata: Value) -> RunId
run_get(run: RunId) -> Option<RunInfo>
run_list() -> Vec<RunInfo>
run_close(run: RunId) -> ()
```

**No run deletion in M11.** Garbage collection of runs is deferred.

### 11.10 Retention

```rust
retention_get(run_id) -> Option<Versioned<RetentionPolicy>>
retention_set(run_id, policy) -> Version

enum RetentionPolicy {
    KeepAll,
    KeepLast(u64),
    KeepFor(Duration),
    Composite(Vec<RetentionPolicy>)
}
```

### 11.11 Transactions

```rust
begin(run_id: RunId) -> Txn
commit(txn: Txn) -> ()
rollback(txn: Txn) -> ()
```

---

## 12. Facade → Substrate Desugaring

Every facade operation desugars to exactly one substrate operation pattern.

### 12.1 KV Operations

| Facade | Substrate |
|--------|-----------|
| `set(key, value)` | `begin_txn(); kv_put(default, key, value); commit()` |
| `get(key)` | `kv_get(default, key).map(\|v\| v.value)` |
| `getv(key)` | `kv_get(default, key)` |
| `mget(keys)` | `batch { kv_get(default, k) for k in keys }` |
| `mset(entries)` | `begin_txn(); for (k,v) in entries: kv_put(default, k, v); commit()` |
| `delete(keys)` | `begin_txn(); for k in keys: kv_delete(default, k); commit()` — returns count existed |
| `exists(key)` | `kv_get(default, key).is_some()` |
| `exists_many(keys)` | `keys.filter(\|k\| kv_get(default, k).is_some()).count()` |
| `incr(key, delta)` | `kv_incr(default, key, delta)` — **atomic engine operation** |

### 12.2 JSON Operations

| Facade | Substrate |
|--------|-----------|
| `json_set(key, path, value)` | `begin_txn(); json_set(default, key, path, value); commit()` |
| `json_get(key, path)` | `json_get(default, key, path).map(\|v\| v.value)` |
| `json_getv(key, path)` | `json_get(default, key, path)` |
| `json_del(key, path)` | `begin_txn(); json_delete(default, key, path); commit()` |
| `json_merge(key, path, value)` | `begin_txn(); json_merge(default, key, path, value); commit()` |

### 12.3 Event Operations

| Facade | Substrate |
|--------|-----------|
| `xadd(stream, payload)` | `event_append(default, stream, payload)` |
| `xrange(stream, start, end, limit)` | `event_range(default, stream, start, end, limit)` |

### 12.4 Vector Operations

| Facade | Substrate |
|--------|-----------|
| `vset(key, vector, metadata)` | `begin_txn(); vector_set(default, key, vector, metadata); commit()` |
| `vget(key)` | `vector_get(default, key)` |
| `vdel(key)` | `begin_txn(); vector_delete(default, key); commit()` |

### 12.5 State Operations

| Facade | Substrate |
|--------|-----------|
| `cas_set(key, expected, new)` | `state_cas(default, key, expected, new)` |
| `cas_get(key)` | `state_get(default, key).map(\|v\| v.value)` |

### 12.6 History Operations

| Facade | Substrate |
|--------|-----------|
| `history(key, limit, before)` | `kv_history(default, key, limit, before)` |
| `get_at(key, version)` | `kv_get_at(default, key, version)` |
| `latest_version(key)` | `kv_get(default, key).map(\|v\| v.version)` |

### 12.7 Run Operations

| Facade | Substrate |
|--------|-----------|
| `runs()` | `run_list()` |
| `use_run(run_id)` | Returns a facade with `default = run_id` (client-side binding) |

---

## 13. Wire Encoding Contract

### 13.1 Request Envelope

```json
{
  "id": "client-generated-request-id",
  "op": "kv.set",
  "params": { "key": "x", "value": 123 }
}
```

### 13.2 Response Envelope (Success)

```json
{
  "id": "client-generated-request-id",
  "ok": true,
  "result": <operation-specific>
}
```

### 13.3 Response Envelope (Error)

```json
{
  "id": "client-generated-request-id",
  "ok": false,
  "error": {
    "code": "NotFound",
    "message": "Key not found",
    "details": null
  }
}
```

### 13.4 Operation Names

**Facade operations:**
- `kv.set`, `kv.get`, `kv.getv`, `kv.mget`, `kv.mset`, `kv.delete`, `kv.exists`, `kv.exists_many`, `kv.incr`
- `json.set`, `json.get`, `json.getv`, `json.del`, `json.merge`
- `event.add`, `event.range`
- `vector.set`, `vector.get`, `vector.del`
- `state.cas_set`, `state.get`
- `history.list`, `history.get_at`, `history.latest_version`
- `run.list`, `run.use`
- `system.capabilities`

**Substrate operations:**
- `substrate.kv.put`, `substrate.kv.get`, `substrate.kv.get_at`, etc.
- `substrate.json.set`, `substrate.json.get`, etc.
- `substrate.event.append`, `substrate.event.range`
- `substrate.vector.set`, `substrate.vector.get`, etc.
- `substrate.state.set`, `substrate.state.get`, `substrate.state.cas`
- `substrate.trace.record`, `substrate.trace.get`, `substrate.trace.range`
- `substrate.run.create`, `substrate.run.get`, `substrate.run.list`, `substrate.run.close`
- `substrate.retention.get`, `substrate.retention.set`
- `txn.begin`, `txn.commit`, `txn.rollback`

### 13.5 JSON Wire Encoding

**JSON wire encoding is mandatory for M11.**

#### Value Type Mapping

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

#### Float Special Values (`$f64` wrapper)

Finite floats use plain JSON numbers. Non-finite and negative zero use wrappers:

```json
{"$f64": "NaN"}
{"$f64": "+Inf"}
{"$f64": "-Inf"}
{"$f64": "-0.0"}
```

Regular `0.0` (positive zero) uses plain JSON: `0.0`

#### Bytes Encoding

```json
{"$bytes": "SGVsbG8gV29ybGQ="}
```

Always base64-encoded. Never raw strings.

#### Absent Value Encoding (for CAS)

To distinguish "key missing" from "value is null":

```json
{"$absent": true}
```

Used in `cas_set` when `expected` is `None`:

```json
{
  "op": "state.cas_set",
  "params": {
    "key": "foo",
    "expected": {"$absent": true},
    "new": 123
  }
}
```

#### Versioned Value Encoding

```json
{
  "value": <Value>,
  "version": { "type": "txn", "value": 123 },
  "timestamp": 1700000000000000
}
```

#### Version Encoding

```json
{ "type": "txn" | "sequence" | "counter", "value": 123 }
```

### 13.6 MessagePack Wire Encoding (Optional)

MessagePack is optional for M11 but defined for future use.

| Strata Type | MessagePack |
|-------------|-------------|
| `Null` | nil |
| `Bool` | bool |
| `Int(i64)` | int64 |
| `Float(f64)` | float64 |
| `String` | str |
| `Bytes` | bin |
| `Array` | array |
| `Object` | map<string, value> |

**Constraints:**
- All integers MUST be encoded as int64
- All floats MUST be encoded as float64 (preserves NaN/Inf/-0.0)
- Bytes MUST use MessagePack `bin` type
- No extension types

### 13.7 Return Shape Encoding

| Operation | Wire Shape |
|-----------|------------|
| `set`, `mset`, `json_set`, `json_merge`, `vset` | `null` |
| `get`, `json_get`, `cas_get` | `Value` or `null` |
| `getv`, `json_getv`, `vget` | `Versioned<Value>` or `null` |
| `mget` | `Array<Value or null>` |
| `delete`, `exists_many`, `json_del` | `int64` |
| `exists`, `vdel`, `cas_set` | `bool` |
| `incr` | `int64` |
| `xadd` | `Version` |
| `xrange`, `history` | `Array<Versioned<Value>>` |

---

## 14. CLI Contract

### 14.1 Command Interface

Redis-like command interface:

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
strata json.del doc $.temp      # prints: (integer) 1
strata json.merge doc $ '{"age": 30}'

# Events
strata xadd stream '{"type":"login"}'  # prints: {"type":"sequence","value":1}
strata xrange stream

# Vectors
strata vset doc1 "[0.1, 0.2, 0.3]" '{"tag":"test"}'
strata vget doc1
strata vdel doc1               # prints: (integer) 1

# State (CAS)
strata cas.set mykey null 123  # prints: (integer) 1 (created)
strata cas.get mykey           # prints: 123
strata cas.set mykey 123 456   # prints: (integer) 1 (updated)
strata cas.set mykey 999 0     # prints: (integer) 0 (mismatch)

# History
strata history mykey
strata history mykey --limit 10
```

### 14.2 Output Conventions

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

### 14.3 Argument Parsing Rules

| Input | Parsed as |
|-------|-----------|
| `123` | `Int(123)` |
| `-456` | `Int(-456)` |
| `1.23` | `Float(1.23)` |
| `"hello"` | `String("hello")` — quotes stripped |
| `hello` | `String("hello")` — bare word |
| `true` / `false` | `Bool` |
| `null` | `Null` |
| `{...}` | Object (must be valid JSON) |
| `[...]` | Array (must be valid JSON) |
| `b64:SGVsbG8=` | `Bytes` (base64 decoded) |

### 14.4 Bytes Input

Use `b64:` prefix for bytes:

```bash
strata set mykey b64:SGVsbG8gV29ybGQ=
```

### 14.5 Run Scoping

Per-command `--run` option:

```bash
strata --run=default set x 123      # explicit default
strata set x 123                     # implicit default (same as above)
strata --run=my-run-id set x 123    # custom run
strata --run=my-run-id get x
```

Default is `"default"` if omitted. No stateful run context in CLI.

### 14.6 CLI Scope

**CLI is facade-only for M11.** Substrate operations are not exposed via CLI.

---

## 15. SDK Mapping

### 15.1 Python Mapping

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

# Versioned wrapper
class Versioned(Generic[T]):
    value: T
    version: Version
    timestamp: int  # microseconds

# Error handling
class StrataError(Exception):
    code: str
    message: str
    details: dict | None

# Usage
db = Strata.open()
db.set("x", 123)
db.get("x")           # -> 123
db.getv("x")          # -> Versioned(value=123, version=..., timestamp=...)
db.exists("x")        # -> True
db.delete(["x"])      # -> 1
db.mget(["a", "b"])   # -> [None, 5]

# JSON
db.json_set("doc", "$.a.b", 5)
db.json_get("doc", "$.a.b")  # -> 5
db.json_getv("doc", "$.a")   # -> Versioned(...)

# Advanced
runs = db.runs()
scoped = db.use_run("my-run-id")
scoped.set("x", 456)
```

### 15.2 JavaScript Mapping

```javascript
// Value mapping
Null    -> null
Bool    -> boolean
Int     -> number | BigInt (for values outside safe integer range)
Float   -> number
String  -> string
Bytes   -> Uint8Array
Array   -> Array<any>
Object  -> Record<string, any>
```

**Note:** JavaScript cannot safely represent all `i64` values. The SDK must use `BigInt` for integers outside the safe range (`Number.MIN_SAFE_INTEGER` to `Number.MAX_SAFE_INTEGER`).

### 15.3 Rust Mapping

```rust
// Direct mapping to Value enum
use strata::{Value, Versioned, Version, StrataError};

let db = Strata::open()?;
db.set("x", Value::Int(123))?;
let v: Option<Value> = db.get("x")?;
let vv: Option<Versioned<Value>> = db.getv("x")?;
```

### 15.4 SDK Requirements

All SDKs MUST:
- Preserve numeric widths (`i64`, `f64`)
- Preserve `Bytes` vs `String` distinction
- Preserve `None`/missing vs `Value::Null` distinction
- Preserve `Versioned` wrapper shape
- Surface structured errors with code, message, details
- Use the same operation names as the facade API

---

## 16. Run Semantics

### 16.1 Default Run

The default run **always exists implicitly** and has the canonical name `"default"`.

- Run ID: `"default"` (literal string, not a UUID)
- Created lazily on first write or on open
- There is no moment where it does not exist
- Facade operations always target this run
- The default run **cannot be closed**

**External name**: The string `"default"` is the external, user-visible name. CLI, logs, docs, and mental models all use this literal:

```bash
strata --run=default set x 1
```

### 16.2 Run Hiding Rule

- **Facade APIs**: NEVER carry `run_id`. Implicitly scoped to default run.
- **Substrate APIs**: ALWAYS require explicit `run_id`.

The "default run" is an implementation detail. Users don't need to know it exists unless they opt into advanced mode.

### 16.3 Run Lifecycle

- `run_create(metadata)` — Substrate only
- `run_close(run_id)` — Substrate only, marks run as closed
- `run_list()` — Available in both facade and substrate
- `use_run(run_id)` — Facade escape hatch, errors if run doesn't exist

### 16.4 Run Deletion

**No run deletion in M11.** This is deferred for future garbage collection features.

### 16.5 RunId Format

UUIDs in lowercase hyphenated format:
```
f47ac10b-58cc-4372-a567-0e02b2c3d479
```

---

## 17. Transaction Semantics

### 17.1 Isolation Level

Transactions provide **snapshot isolation**.

- Reads see a consistent snapshot as of transaction start
- Writes use OCC (optimistic concurrency control) validation at commit
- No guarantee of serializability
- Write-write conflicts return `Conflict` error

### 17.2 Transaction Scope

Transactions are **scoped to a single run**.

- A transaction has a `run_id`
- All operations in that transaction must use that run
- Cross-run transactions are not supported

### 17.3 Auto-Commit

In facade mode, each operation auto-commits:

```
set("x", 1)  →  begin(); kv_put(...); commit()
```

### 17.4 Explicit Transactions (Substrate)

```rust
let txn = db.begin(run_id)?;
txn.kv_put("a", 1)?;
txn.kv_put("b", 2)?;
txn.commit()?;  // or txn.rollback()
```

### 17.5 Transaction Properties

- **Atomic**: All or nothing
- **Isolated**: Snapshot isolation
- **Deterministic**: Same operations produce same state
- **Idempotent on replay**: WAL replay produces identical state

---

## 18. History & Retention

### 18.1 History Access

History is available for versioned primitives:

| Primitive | History API |
|-----------|-------------|
| KV | `kv_history(run, key)` / facade `history(key)` |
| JSON | `json_history(run, key)` |
| Vector | `vector_history(run, key)` |
| Events | `event_range(run, stream)` (events are append-only) |
| State | `state_history(run, key)` (optional) |

**Facade `history()` is KV-only.**

### 18.2 History Ordering

History is returned **newest first** (descending by version).

### 18.3 History Pagination

```rust
history(key, limit: Option<u64>, before: Option<Version>)
```

- `limit`: Maximum number of versions to return
- `before`: Return versions older than this (exclusive)

### 18.4 Retention Policy

```rust
enum RetentionPolicy {
    KeepAll,              // Default
    KeepLast(u64),        // Keep N most recent versions
    KeepFor(Duration),    // Keep versions within time window
    Composite(Vec<RetentionPolicy>)  // Union of policies
}
```

### 18.5 Retention Scope

- Retention is configured **per-run**
- Default policy is `KeepAll`
- Per-key retention is NOT supported in M11

### 18.6 HistoryTrimmed Error

When requesting a version that has been removed by retention:

```json
{
  "code": "HistoryTrimmed",
  "message": "Requested version no longer retained",
  "details": {
    "requested": { "type": "txn", "value": 100 },
    "earliest_retained": { "type": "txn", "value": 150 }
  }
}
```

---

## 19. What M11 Does NOT Freeze

Explicitly deferred to future milestones:

- Diff semantics
- Search ranking algorithms
- Provenance model
- Reasoning structures
- TTL/EXPIRE semantics
- Consumer groups for events
- Vector search query DSL
- JSONPath filters/wildcards/recursive descent
- Python SDK (M12+)
- MessagePack wire format (optional for M11, not required)
- Run deletion/garbage collection
- Per-key retention policies
- Serializable isolation

---

## 20. Success Criteria

M11 is complete when:

1. ✅ Facade API implemented with all operations (including `mset`, `json_getv`, `capabilities`)
2. ✅ Substrate API implemented with explicit run/version/primitive access
3. ✅ Value model frozen and consistent across Rust types and wire
4. ✅ Float edge cases handled (`NaN`, `±Inf`, `-0.0` with `$f64` wrapper)
5. ✅ Bytes encoding consistent (`$bytes` wrapper)
6. ✅ Size limits enforced with `ConstraintViolation` errors
7. ✅ Key validation enforced (UTF-8, no NUL, reserved prefix blocked)
8. ✅ `Versioned<T>` shape frozen with microsecond timestamps and tagged union versions
9. ✅ Error model frozen with all codes and structured payloads
10. ✅ Wire encoding frozen (JSON required, `$f64`/`$bytes`/`$absent` wrappers)
11. ✅ CLI implemented with Redis-like ergonomics and frozen parsing rules
12. ✅ All documentation finalized in this single contract document
13. ✅ Facade→Substrate desugaring documented for every operation
14. ✅ CAS semantics defined (value-compare with `Option<Value>`)
15. ✅ History pagination supported
16. ✅ Transaction isolation level documented (snapshot isolation)
17. ✅ Keyspace partitioning defined (per-primitive)
18. ✅ Validation tests confirm Redis mental model works
19. ✅ Contract stability guarantees documented
20. ✅ Determinism guarantee documented
21. ✅ Facade invariants documented
22. ✅ Value comparison semantics documented (equality only, no ordering)
23. ✅ Undefined behavior explicitly defined as errors
24. ✅ Default run named `"default"` (literal string)
25. ✅ Capability discovery operation available

---

## Appendix A: Primitive Keyspace Partitioning

Keyspaces are **partitioned by primitive**. The same key can exist independently in KV, JSON, Vector, etc.

| Primitive | Namespace | Facade delete |
|-----------|-----------|---------------|
| KV | `kv:{key}` | `delete(keys)` |
| JSON | `json:{key}` | `json_del(key, "$")` then `delete` |
| Vector | `vector:{key}` | `vdel(key)` |
| State | `state:{key}` | No facade delete |
| Events | `event:{stream}` | No facade delete |

Facade `delete(keys)` targets **KV only**.

---

## Appendix B: Quick Reference Card

### Facade API

```
# KV
set(key, value) → ()
get(key) → Option<Value>
getv(key) → Option<Versioned<Value>>
mget(keys) → Vec<Option<Value>>
mset(entries) → ()
delete(keys) → u64
exists(key) → bool
exists_many(keys) → u64
incr(key, delta=1) → i64

# JSON
json_set(key, path, value) → ()
json_get(key, path) → Option<Value>
json_getv(key, path) → Option<Versioned<Value>>
json_del(key, path) → u64
json_merge(key, path, value) → ()

# Events
xadd(stream, payload) → Version
xrange(stream, start?, end?, limit?) → Vec<Versioned<Value>>

# Vectors
vset(key, vector, metadata) → ()
vget(key) → Option<Versioned<{vector, metadata}>>
vdel(key) → bool

# State (CAS)
cas_set(key, expected?, new) → bool
cas_get(key) → Option<Value>

# History
history(key, limit?, before?) → Vec<Versioned<Value>>
get_at(key, version) → Value | HistoryTrimmed
latest_version(key) → Option<Version>

# Runs
runs() → Vec<RunInfo>
use_run(run_id) → ScopedFacade

# System
capabilities() → Capabilities
```

### Wire Special Encodings

```json
{"$bytes": "<base64>"}     // Bytes
{"$f64": "NaN"}            // Float NaN
{"$f64": "+Inf"}           // Float +Infinity
{"$f64": "-Inf"}           // Float -Infinity
{"$f64": "-0.0"}           // Float negative zero
{"$absent": true}          // None/missing (for CAS)
```

### Version Types

```json
{"type": "txn", "value": 123}       // KV, JSON, Vector, Run
{"type": "sequence", "value": 456}  // Events
{"type": "counter", "value": 789}   // StateCell
```

---

**This is the contract. Everything downstream conforms to it.**
