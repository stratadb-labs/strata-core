# Strata Wire Encoding Specification (M11 / Phase 10b)

**Status**: Draft

**Audience**: SDK authors, CLI implementers, server implementers

**Scope**: Defines the stable on-the-wire representation of Strata values, errors, return shapes, and versioned objects.

---

## 1. Purpose

This document defines the **wire contract** between Strata and all external consumers:

- Embedded SDKs (Rust, Python, JS)
- CLI
- Future server protocol
- Serialization formats (JSON, MessagePack, CBOR, etc.)

Once frozen, this specification must not change without a major-version break.

This spec does **not** define transport framing (TCP, HTTP, etc.). It defines the **logical message shapes** and **type encodings**.

---

## 2. Canonical Wire Types

Strata exposes exactly one public value model on the wire:

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

All encodings MUST be lossless with respect to this model.

---

## 3. JSON Encoding

### 3.1 Mapping Rules

| Strata Value | JSON Representation |
|-------------|----------------------|
| Null        | null                 |
| Bool        | true / false         |
| Int(i64)    | JSON number          |
| Float(f64)  | JSON number          |
| String      | JSON string          |
| Bytes       | Base64-encoded string with tag |
| Array       | JSON array           |
| Object      | JSON object          |

### 3.2 Bytes Encoding

Bytes MUST be encoded as tagged objects to avoid ambiguity with strings:

```json
{"$type": "bytes", "data": "BASE64..."}
```

### 3.3 Numeric Rules

- Int is always signed 64-bit
- Float is IEEE 754 double (64-bit)
- No implicit numeric coercion
- SDKs MUST NOT auto-convert floats to ints or vice versa

### 3.4 Object Key Rules

- Keys are UTF-8 strings
- Key order is not significant

---

## 4. Binary Encoding (MessagePack Profile)

Strata defines a canonical MessagePack profile.

### 4.1 Value Encoding

| Strata Type | MessagePack |
|-------------|-------------|
| Null        | nil         |
| Bool        | bool        |
| Int(i64)    | int64       |
| Float(f64)  | float64     |
| String      | str         |
| Bytes       | bin         |
| Array       | array       |
| Object      | map<string, value> |

### 4.2 Required Constraints

- All integers MUST be encoded as int64
- All floats MUST be encoded as float64
- Bytes MUST use MessagePack `bin` type
- No extension types

---

## 5. Versioned Wrapper

Advanced APIs return versioned values:

```rust
struct Versioned<T> {
    value: T,
    version: Version,
    timestamp: u64
}
```

### 5.1 JSON Encoding

```json
{
  "value": <Value>,
  "version": 12345,
  "timestamp": 1690000000
}
```

### 5.2 Binary Encoding

Encoded as a map with fixed keys: `value`, `version`, `timestamp`.

---

## 6. Optional vs Null

Strata distinguishes:

- Missing value → `None`
- Present value that is Null → `Some(Value::Null)`

### 6.1 JSON Representation

- Missing → field absent
- Null → `null`

### 6.2 CLI Representation

- Missing → `(nil)`
- Null → `null`

---

## 7. Error Encoding

### 7.1 Canonical Error Shape

```json
{
  "code": "WrongType",
  "message": "Expected Int, found String",
  "details": { ... }
}
```

### 7.2 Stable Error Codes

| Code |
|------|
| NotFound |
| WrongType |
| InvalidKey |
| InvalidPath |
| HistoryTrimmed |
| ConstraintViolation |
| SerializationError |
| StorageError |
| InternalError |

### 7.3 HistoryTrimmed Payload

```json
{
  "code": "HistoryTrimmed",
  "requested": 100,
  "earliest_retained": 150
}
```

---

## 8. Return Shape Encoding

| Operation | Wire Shape |
|----------|------------|
| set      | null       |
| get      | Value or null |
| getv     | Versioned<Value> or null |
| mget     | Array<Value or null> |
| delete   | int64 |
| exists   | bool |
| exists_many | int64 |
| incr     | int64 |

---

## 9. CLI Rendering Rules

| Strata Value | CLI Output |
|--------------|------------|
| Null         | null |
| Bool         | true / false |
| Int          | 123 |
| Float        | 1.23 |
| String       | "text" |
| Bytes        | <bytes:BASE64> |
| Array        | [ ... ] |
| Object       | { ... } |

Versioned values:

```
{
  value: ...,
  version: 123,
  timestamp: 1690000000
}
```

---

## 10. SDK Mapping Requirements

All SDKs MUST:

- Preserve numeric widths
- Preserve bytes vs string distinction
- Preserve missing vs null distinction
- Preserve Versioned wrapper shape
- Surface structured errors

---

## 11. Stability Guarantees

This spec freezes:

- Canonical Value model
- Numeric widths
- Float encoding
- Bytes encoding
- Versioned wrapper
- Error shape
- Return shapes

No future milestone may change these without a major version bump.



# Addendum to Strata Wire Encoding Specification (M11 / Phase 10b)

## 12. Primitive Payload Shapes (Frozen)

This section defines the canonical payload shapes for each primitive. These are part of the public contract and must remain stable for MVP.

These shapes define:

* What SDKs send
* What the server (later) accepts
* What the CLI prints
* What cross-language clients see

These are not implementation details.

---

### 12.1 KVStore

#### Key

```
String
```

#### Value

```
Value
```

#### Versioned Return

```json
{
  "value": Value,
  "version": Version,
  "timestamp": u64
}
```

---

### 12.2 JsonStore

#### Document Root

```
Value::Object
```

#### Path

```
String (JSON Pointer–like)
```

#### Value

```
Value (JSON subset only)
```

#### Versioned Return

```json
{
  "value": Value,
  "version": Version,
  "timestamp": u64
}
```

---

### 12.3 EventLog

#### Event Type

```
String
```

#### Payload

```
Value
```

#### Append Return

```
Version
```

#### Versioned Event Return

```json
{
  "value": {
    "type": String,
    "payload": Value
  },
  "version": Version,
  "timestamp": u64
}
```

---

### 12.4 TraceStore

#### Trace Type

```
String
```

#### Metadata

```
Value
```

#### Versioned Trace Return

```json
{
  "value": {
    "type": String,
    "metadata": Value
  },
  "version": Version,
  "timestamp": u64
}
```

---

### 12.5 VectorStore

#### Vector

```
Array<Float32>
```

#### Metadata

```
Value::Object
```

#### Stored Entry

```json
{
  "vector": [f32, f32, ...],
  "metadata": Value
}
```

#### Versioned Return

```json
{
  "value": {
    "vector": [f32],
    "metadata": Value
  },
  "version": Version,
  "timestamp": u64
}
```

---

### 12.6 StateCell

#### Value

```
Value
```

#### Versioned Return

```json
{
  "value": Value,
  "version": Version,
  "timestamp": u64
}
```

---

### 12.7 RunIndex

#### Run Metadata

```
Value::Object
```

#### RunInfo

```json
{
  "run_id": RunId,
  "metadata": Value,
  "created_at": u64,
  "state": "active" | "closed"
}
```

---

### 12.8 Version Type

```json
{
  "type": "TxnId" | "Sequence" | "Counter",
  "value": u64
}
```

This prevents cross-type comparison errors and preserves primitive-specific semantics.

---

### 12.9 RunId

```json
String
```

Canonical format: UUID string (lowercase, hyphenated)

Example:

```
"f47ac10b-58cc-4372-a567-0e02b2c3d479"
```

---

## 13. Request/Response Envelope (Lightweight)

This section defines the minimal, stable, language-agnostic envelope used by all wire protocols (embedded RPC, TCP server later, CLI piping, etc).

This envelope is intentionally thin. It does not contain semantics.

---

### 13.1 Request Envelope

```json
{
  "id": String,
  "op": String,
  "params": Object
}
```

#### Fields

| Field  | Type   | Description                   |
| ------ | ------ | ----------------------------- |
| id     | String | Client-generated request ID   |
| op     | String | Operation name                |
| params | Object | Operation-specific parameters |

---

### 13.2 Response Envelope (Success)

```json
{
  "id": String,
  "ok": true,
  "result": Any
}
```

---

### 13.3 Response Envelope (Error)

```json
{
  "id": String,
  "ok": false,
  "error": {
    "code": String,
    "message": String,
    "details": Object | null
  }
}
```

---

### 13.4 Error Codes (Canonical)

These codes are frozen by 10b.

| Code                | Meaning                          |
| ------------------- | -------------------------------- |
| NotFound            | Entity or key not found          |
| WrongType           | Wrong primitive or type          |
| InvalidKey          | Key syntax invalid               |
| InvalidPath         | JSON path invalid                |
| HistoryTrimmed      | Requested version is unavailable |
| ConstraintViolation | API-level invariant violation    |
| SerializationError  | Invalid Value encoding           |
| StorageError        | Disk or WAL failure              |
| InternalError       | Bug or invariant violation       |

---

### 13.5 Operation Naming Convention

Facade operations:

| Operation  | op              |
| ---------- | --------------- |
| set        | "kv.set"        |
| get        | "kv.get"        |
| getv       | "kv.getv"       |
| mget       | "kv.mget"       |
| delete     | "kv.delete"     |
| exists     | "kv.exists"     |
| incr       | "kv.incr"       |
| json_set   | "json.set"      |
| json_get   | "json.get"      |
| json_del   | "json.del"      |
| json_merge | "json.merge"    |
| xadd       | "event.add"     |
| xrange     | "event.range"   |
| vset       | "vector.set"    |
| vget       | "vector.get"    |
| vdel       | "vector.del"    |
| cas_set    | "state.cas_set" |
| cas_get    | "state.get"     |

Substrate operations:

| Operation     | op                        |
| ------------- | ------------------------- |
| kv_put        | "substrate.kv.put"        |
| kv_get        | "substrate.kv.get"        |
| json_set      | "substrate.json.set"      |
| event_append  | "substrate.event.append"  |
| trace_record  | "substrate.trace.record"  |
| vector_upsert | "substrate.vector.upsert" |
| state_set     | "substrate.state.set"     |
| run_create    | "substrate.run.create"    |
| txn_begin     | "txn.begin"               |
| txn_commit    | "txn.commit"              |
| txn_rollback  | "txn.rollback"            |

---

### 13.6 Versioned Return Convention

Any version-aware API returns:

```json
{
  "value": T,
  "version": Version,
  "timestamp": u64
}
```

This shape is frozen.

---

### 13.7 Optional Return Convention

* Missing → `null`
* Present → actual value
* No sentinel objects
* No magic markers

---

### 13.8 Batch Return Convention

Batch APIs return arrays:

```json
[Value | null, Value | null, ...]
```
