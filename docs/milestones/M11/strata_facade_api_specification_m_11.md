# Strata Facade API Specification (M11)

**Status**: Draft – Targeting Lock-In for MVP  
**Audience**: Application developers, Redis users, SDK authors  
**Scope**: Public embedded SDK surface (Rust, Python, JS, CLI)  
**Goal**: Make Strata instantly usable without understanding runs, versioning, or primitives

---

## 1. Design Goals

The Strata Facade API is the primary human-facing interface. It defines the public contract of Strata.

This API must:

- Feel instantly familiar to Redis users
- Hide runs by default
- Return simple, predictable shapes
- Expose power progressively
- Lock in the canonical value model
- Lock in return types
- Lock in error shapes
- Be stable across SDKs

This API is not a toy. It defines the semantic contract of Strata.

---

## 2. Default Mode vs Advanced Mode

### 2.1 Default Mode (95 percent of users)

Default mode hides substrate complexity.

Properties:
- Single implicit run: `DefaultRun`
- No versioning visible
- No primitive selection required
- No entity refs
- No history by default
- No CAS by default

Everything looks like a normal key-value store with extensions.

### 2.2 Advanced Mode (Escape Hatch)

Advanced mode exposes substrate power.

Properties:
- Explicit run access
- Explicit primitive selection
- Version-aware APIs
- History APIs
- CAS semantics
- Deterministic replay

Default mode must never block access to advanced mode.

---

## 3. Canonical Value Model (Locked)

This is the heart of the public contract.

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

### 3.1 Rules

- This is the only public value model
- All SDKs must map to this model
- JSON is a strict subset of this model
- No implicit type coercions
- No lossy conversions
- Numbers must not auto-promote
- Bytes are not strings

### 3.2 Semantic Properties

| Type   | Comparable | Diffable (future) | Searchable (future) |
|--------|------------|-------------------|----------------------|
| Null   | Yes        | Yes               | Yes                  |
| Bool   | Yes        | Yes               | Yes                  |
| Int    | Yes        | Yes               | Yes                  |
| Float  | Yes        | Yes               | Yes                  |
| String | Yes        | Yes               | Yes                  |
| Bytes  | No         | Yes               | No                   |
| Array  | Structural | Yes               | Partial              |
| Object | Structural | Yes               | Partial              |

---

## 4. Default Facade: Key-Value API

This is the Redis mental anchor.

### 4.1 set

```text
set(key: String, value: Value) -> ()
```

Behavior:
- Overwrites current value
- Creates a new version internally
- Does not expose version

Errors:
- InvalidKey
- ValueTooLarge
- SerializationError
- StorageError

---

### 4.2 get

```text
get(key: String) -> Option<Value>
```

Returns:
- Some(Value) if exists
- None if missing

Notes:
- No silent fallback
- No tombstones exposed

---

### 4.3 getv (escape hatch)

```text
getv(key: String) -> Option<Versioned<Value>>
```

Where:

```rust
struct Versioned<T> {
    value: T,
    version: Version,
    timestamp: u64
}
```

---

### 4.4 mget

```text
mget(keys: Vec<String>) -> Vec<Option<Value>>
```

Rules:
- Order preserved
- Missing keys return None

---

### 4.5 delete

```text
delete(keys: Vec<String>) -> u64
```

Returns:
- Number of keys removed

---

### 4.6 exists

```text
exists(key: String) -> bool
```

---

### 4.7 exists_many

```text
exists_many(keys: Vec<String>) -> u64
```

Returns:
- Count of keys that exist

---

### 4.8 incr (recommended)

```text
incr(key: String, delta: i64 = 1) -> i64
```

Rules:
- Missing key treated as 0
- Value must be Int
- Atomic

Errors:
- WrongType
- Overflow

---

## 5. JSON Facade (JsonStore)

### 5.1 json_set

```text
json_set(key: String, path: String, value: Value) -> ()
```

Rules:
- Root must be Object
- Path semantics locked in here

---

### 5.2 json_get

```text
json_get(key: String, path: String) -> Option<Value>
```

---

### 5.3 json_del

```text
json_del(key: String, path: String) -> u64
```

Returns:
- Number of elements removed

---

## 6. Event Facade (Redis Streams Mental Model)

### 6.1 xadd

```text
xadd(stream: String, payload: Object) -> Version
```

Returns:
- Version as stable event ID

---

### 6.2 xrange

```text
xrange(
    stream: String,
    start: Option<Version>,
    end: Option<Version>,
    limit: Option<u64>
) -> Vec<Versioned<Value>>
```

---

## 7. Vector Facade

### 7.1 vset

```text
vset(key: String, vector: Vec<f32>, metadata: Object) -> ()
```

---

### 7.2 vget

```text
vget(key: String) -> Option<{ vector: Vec<f32>, metadata: Value }>
```

---

### 7.3 vdel

```text
vdel(key: String) -> bool
```

---

## 8. StateCell Facade (CAS)

Hidden by default.

### 8.1 cas_set

```text
cas_set(key: String, expected: Value, new: Value) -> bool
```

---

### 8.2 cas_get

```text
cas_get(key: String) -> Option<Value>
```

---

## 9. History & Version APIs (Advanced)

### 9.1 history

```text
history(key: String) -> Vec<Versioned<Value>>
```

---

### 9.2 get_at

```text
get_at(key: String, version: Version) -> Value | HistoryTrimmed
```

---

### 9.3 latest_version

```text
latest_version(key: String) -> Option<Version>
```

---

## 10. Run APIs (Hidden by Default)

### 10.1 runs

```text
runs() -> Vec<RunInfo>
```

---

### 10.2 use_run

```text
use_run(run_id: RunId) -> ScopedFacade
```

---

## 11. Error Model (Locked)

All SDKs must expose the same categories.

```rust
enum StrataError {
    NotFound,
    WrongType,
    InvalidKey,
    InvalidPath,
    HistoryTrimmed {
        requested: Version,
        earliest_retained: Version
    },
    ConstraintViolation,
    SerializationError,
    StorageError,
    InternalError
}
```

Rules:
- No stringly-typed errors
- Errors must have stable codes
- Payload fields must be stable
- CLI prints friendly text
- SDKs expose structured errors

---

## 12. Return Shape Conventions

| Operation | Return |
|----------|--------|
| set      | ()     |
| get      | Option<Value> |
| getv     | Option<Versioned<Value>> |
| mget     | Vec<Option<Value>> |
| delete   | u64 |
| exists   | bool |
| exists_many | u64 |
| incr     | i64 |
| json_get | Option<Value> |
| xadd     | Version |
| xrange   | Vec<Versioned<Value>> |
| vget     | Option<{vector, metadata}> |

---

## 13. Wire Compatibility Implications

This facade freezes:

- The canonical Value model
- Numeric widths (i64, f64)
- Optional vs Null semantics
- Versioned<T> shape
- Error shapes

These must be preserved across:

- Rust SDK
- Python SDK
- JS SDK
- CLI
- Server protocol (later)

---

## 14. Facade to Substrate Mapping (Required)

Every facade operation must desugar to exactly one substrate operation.

Examples:

```text
set(k, v) → kv.put(DefaultRun, k, v)
get(k) → kv.get(DefaultRun, k)
xadd(s, v) → event.append(DefaultRun, s, v)
```

This mapping must be explicit and documented for every method.

---

## 15. Stability Guarantees

Once M11 ships:

- Value model is frozen
- Error model is frozen
- Return shapes are frozen
- Facade method names are frozen
- Behavior is frozen

Future work may add:
- New primitives
- New methods
- New advanced APIs

But must not break this contract.

---

## 16. Out of Scope for M11

- Diff semantics
- Search ranking
- Provenance shape
- Reasoning structures

Lock in shape, not intelligence.

