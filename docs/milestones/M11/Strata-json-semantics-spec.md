# Strata JSON Semantics Specification (M11 / Phase 10b)

**Status**: Draft (Freeze candidate)
**Scope**: JSON facade semantics (`json_set`, `json_get`, `json_del`, `json_merge`) and how JSON maps onto Strata’s canonical `Value` model
**Audience**: SDK authors (Rust/Python/JS), CLI authors, Strata users
**Non-goal**: Search, diff, provenance, ranking, semantic interpretation (reserved for 10c)

---

## 1. Design Goals

This spec defines the user-visible contract for Strata’s JSON behavior:

* Make JSON feel familiar to RedisJSON users and normal app developers
* Be deterministic and consistent across Rust/Python/JS/CLI
* Use Strata’s canonical `Value` model as the single source of truth
* Lock in path semantics and patch semantics for MVP
* Avoid surprising implicit coercions or “helpful” lossy conversions

---

## 2. Canonical Value Model and JSON Subset

Strata’s canonical model (frozen by Phase 10b):

```rust
enum Value {
  Null,
  Bool(bool),
  Int(i64),
  Float(f64),
  String(String),
  Bytes(Vec<u8>),
  Array(Vec<Value>),
  Object(HashMap<String, Value>),
}
```

### 2.1 JSON-Compatible Subset

JSON is a strict subset of `Value`:

| JSON type | Strata `Value`                     |
| --------- | ---------------------------------- |
| null      | `Value::Null`                      |
| boolean   | `Value::Bool`                      |
| number    | `Value::Int` **or** `Value::Float` |
| string    | `Value::String`                    |
| array     | `Value::Array`                     |
| object    | `Value::Object`                    |

**Not JSON-compatible**:

* `Value::Bytes` has **no** JSON representation and must not appear in JSON documents.

### 2.2 Numbers: Int vs Float

Strata does **not** auto-promote numbers.

* If a number is known to be integral and fits in `i64`, it may be represented as `Int(i64)`.
* If it has a fractional component, or exceeds `i64`, it is represented as `Float(f64)`.

**Wire note**: JSON text alone cannot always preserve `Int` vs `Float` intent. For that reason:

* SDKs and binary wire formats must preserve `Int` vs `Float`.
* CLI JSON printing is best-effort and may print `Int` as a JSON number without tagging.

---

## 3. JsonStore Document Constraints

### 3.1 Root Shape

A JsonStore document **MUST** be a JSON object:

* Root is `Value::Object`.
* Attempting to `json_set` the root to a non-object is an error.
* Attempting to store a non-object document is an error.

This is an API-level constraint, not an implementation detail.

### 3.2 Keyspace and Keys

* JsonStore is addressed by a top-level `key: String` (same key rules as KV).
* The JSON document lives “under” that key.
* Paths address locations inside the JSON document.

---

## 4. Path Syntax

Strata uses a **JSON Pointer–like** path syntax with a few Strata-specific rules.

### 4.1 Grammar

A path is a UTF-8 string:

* `""` (empty string) refers to the **root**.
* Otherwise it must begin with `/` and be a sequence of segments separated by `/`.

Examples:

* `""` root
* `/a` field `a`
* `/a/b` field `b` under object `a`
* `/items/0` index `0` under array `items`

### 4.2 Segment Unescaping

Segments use the following escapes:

* `~1` represents `/`
* `~0` represents `~`

Examples:

* `/a~1b` refers to key `a/b`
* `/a~0b` refers to key `a~b`

Any other `~X` sequence is invalid.

### 4.3 Object Segments

If the current node is an object:

* The segment is treated as a field name (after unescaping).
* Missing field behavior depends on the operation (set vs get vs del).

### 4.4 Array Segments

If the current node is an array:

* The segment must be a base-10 non-negative integer (e.g., `0`, `1`, `25`), or the special token `-` (append).
* Leading `+` is invalid.
* Leading zeros are allowed but discouraged (`01` refers to index 1).

`-` is only valid for `json_set` (append semantics).

---

## 5. Operations and Semantics

### 5.1 `json_set(key, path, value) -> ()`

Sets `value` at `path` inside the JSON document stored at `key`.

#### 5.1.1 Behavior Summary

* If `key` is missing:

  * If `path == ""`: create a new document with root object = `value` **only if** `value` is `Object`.
  * If `path != ""`: create a new empty root object `{}` and then apply the path set (see creation rules below).
* If `key` exists but is not a JSON document: error `WrongType`.

#### 5.1.2 Intermediate Creation Rules

For `json_set`, Strata may create missing intermediate **objects** as needed, but does not guess arrays.

* If traversing an object and a segment field is missing:

  * Create it as an object `{}` **if** there are more segments remaining and the next segment is not an array index.
  * If the next segment is an array index, error `InvalidPath` (Strata will not auto-create arrays).
* If traversing an array:

  * The array must already exist, except for `-` append at the final segment.

This keeps behavior predictable and avoids accidental shape creation.

#### 5.1.3 Array Set Rules

When the target is an array:

* If final segment is an integer `i`:

  * `0 <= i < len`: overwrite element `i`.
  * `i == len`: error by default (no implicit extend).
  * `i > len`: error.
* If final segment is `-`:

  * Append the value to the array.

#### 5.1.4 Root Set Constraint

If `path == ""`, then:

* `value` must be `Value::Object`, else `InvalidPath` (or `ConstraintViolation`, depending on your error taxonomy).
* Root overwrite is allowed and replaces the entire document.

#### 5.1.5 Errors

* `InvalidKey`
* `WrongType` (key exists but is not JsonStore document)
* `InvalidPath` (bad syntax, impossible traversal, illegal array index, illegal `~` escapes)
* `ConstraintViolation` (attempted non-object root)
* `SerializationError` (SDK encoding issues)
* `StorageError`

---

### 5.2 `json_get(key, path) -> Option<Value>`

Fetches the value at `path`.

#### 5.2.1 Behavior Summary

* If `key` missing: return `None`.
* If `key` exists but not JsonStore: error `WrongType`.
* If `path` resolves to a node: return `Some(Value)`.
* If `path` is valid but the node does not exist: return `None`.
* If `path` is invalid: error `InvalidPath`.

**No silent fallback**: it does not return “closest existing ancestor”.

---

### 5.3 `json_del(key, path) -> u64`

Deletes the node at `path`. Returns a count of deletions.

#### 5.3.1 Behavior Summary

* If `key` missing: return `0`.
* If `key` exists but not JsonStore: error `WrongType`.
* If `path` resolves to an existing node:

  * Delete it and return `1`.
* If node does not exist: return `0`.
* If `path == ""`:

  * Delete the entire document at `key` and return `1` if it existed, else `0`.

#### 5.3.2 Deletion Semantics

* Deleting an object field removes the field.
* Deleting an array element removes it and shifts elements left (standard JSON array delete semantics).

  * This is visible behavior and must be stable.

---

### 5.4 `json_merge(key, path, value) -> ()`

Merges `value` into the existing node at `path` using **JSON Merge Patch** semantics.

This is intentionally simple and widely understood:

* If the target node is an object and `value` is an object:

  * For each field:

    * If patch field value is `null`, delete that field from target (if present).
    * Else set/overwrite that field to patch value.
    * If both target and patch values are objects, recurse.
* If target node is missing:

  * Create it as an object `{}` if `value` is object; otherwise set directly (see below).
* If either side is not an object:

  * Replace target node with `value`.

#### 5.4.1 Root Merge

If `path == ""`:

* Root must remain an object after merge.
* If patch would replace root with non-object, error `ConstraintViolation`.

#### 5.4.2 Errors

Same as `json_set`, plus:

* `ConstraintViolation` if root object constraint would be violated.

---

## 6. Determinism and Ordering

### 6.1 Object Ordering

* JSON objects are treated as unordered maps.
* The wire format may preserve insertion order in some encodings, but **clients must not rely on ordering**.
* CLI output may display keys in stable sorted order for readability (recommended), but this is presentation only.

### 6.2 Equality

* Two JSON objects are equal if they have the same set of keys and equal values recursively.
* Array equality is positional.
* Numeric equality:

  * `Int(1)` is not equal to `Float(1.0)` at the `Value` level.

---

## 7. CLI Semantics

### 7.1 Printing

The CLI prints JSON-compatible values as JSON text:

* `Null/Bool/String/Array/Object` map directly.
* `Int/Float` print as JSON numbers.
* `Bytes` cannot appear in JsonStore; if encountered, CLI must print a safe placeholder and return a non-zero exit code (indicates contract violation).

### 7.2 Missing vs Null

* Missing path returns “(nil)” or equivalent in CLI.
* Present `null` returns `null`.

This distinction must be preserved.

---

## 8. Python / JS SDK Semantics

### 8.1 Python Mapping

Recommended mapping:

* JSON-compatible `Value` maps naturally to Python types:

  * `Null -> None`
  * `Bool -> bool`
  * `Int -> int`
  * `Float -> float`
  * `String -> str`
  * `Array -> list`
  * `Object -> dict[str, Any]`

Important:

* Python SDK must preserve `Int` vs `Float` at the wire/value layer even though both are “numbers” in Python.
* If a Python user passes `1`, it becomes `Int(1)` unless the user explicitly passes a float.

### 8.2 JS Mapping

* `Null -> null`
* `Bool -> boolean`
* `Int -> number` (with constraints)
* `Float -> number`
* `String -> string`
* `Array -> Array<any>`
* `Object -> Record<string, any>`

Important:

* JS cannot safely represent all `i64`. The JS SDK must:

  * Use BigInt for `Int` outside safe range, or
  * Represent large ints as tagged objects, depending on your broader Value rules.
* This is part of the “wire types” contract and must be consistent with the wire encoding spec.

(If you already chose a global rule for i64-in-JS in the wire spec, this JSON semantics spec should reference that rule.)

---

## 9. Error Contract for JSON

JSON operations use the canonical error model. Minimum required:

* `WrongType`
* `InvalidPath`
* `ConstraintViolation`
* `SerializationError`
* `StorageError`

Recommended structured payload for `InvalidPath`:

```json
{
  "code": "InvalidPath",
  "message": "Invalid JSON path",
  "path": "/a/~2b",
  "reason": "Invalid escape sequence"
}
```

Payload fields should be stable once frozen.

---

## 10. Examples

### 10.1 Create and Set

* `json_set("user:1", "", {"name":"Ada","age":37})` creates document.
* `json_set("user:1", "/prefs/theme", "dark")` creates missing objects `prefs`.

Result:

```json
{"name":"Ada","age":37,"prefs":{"theme":"dark"}}
```

### 10.2 Array Append

Given:

```json
{"items":[1,2]}
```

* `json_set("k", "/items/-", 3)` -> `[1,2,3]`

### 10.3 Delete

* `json_del("user:1", "/prefs/theme")` returns `1`
* `json_del("user:1", "/prefs/missing")` returns `0`

### 10.4 Merge Patch

Given:

```json
{"a":1,"b":{"x":1,"y":2}}
```

Patch:

```json
{"b":{"y":null,"z":3}}
```

After `json_merge("k", "", patch)`:

```json
{"a":1,"b":{"x":1,"z":3}}
```

---

## 11. Explicit Non-Goals (for 10b)

This spec intentionally does **not** define:

* JSONPath queries, filters, or wildcards
* Schema validation
* Diff semantics
* Patch formats beyond merge patch
* Search semantics

---

If you want, I can also draft a **1-page “RedisJSON compatibility notes”** section that maps RedisJSON commands (JSON.SET/GET/DEL/MERGE) to these Strata semantics and calls out any deliberate incompatibilities (like “no implicit array creation”).
