Perfect. This is the last missing piece for freezing 10b properly: how humans *actually* touch Strata.

I will write this as a spec-style document you can drop into the repo.

---

# Strata Python & CLI Mapping Specification (M11 / Phase 10b)

**Status**: Draft
**Scope**: Human-facing interaction surfaces
**Audience**: SDK authors, CLI designers, documentation writers
**Goal**: Ensure Python and CLI surfaces are predictable, idiomatic, and fully aligned with the Facade + Substrate specs

This document answers:

* What does Python see?
* What does the CLI look like?
* How do default mode and advanced mode map?
* How do errors surface?
* How do values appear?
* How do users discover power progressively?

This is a *product* spec, not a systems spec.

---

## 1. Design Principles

### 1.1 Python Principles

Python should feel:

* Natural
* Typed (where possible)
* Explicit, not magical
* Async-ready (later)
* Redis-familiar

Python is not a thin wire wrapper. It is a *human interface*.

---

### 1.2 CLI Principles

CLI should feel:

* Redis-like
* Inspectable
* Scriptable
* Pipe-friendly
* JSON-first

CLI is a debugging surface, not a production API.

---

### 1.3 Progressive Disclosure

Default users should never need to learn:

* Runs
* Versions
* Primitives
* Transactions

Advanced users should have zero barriers to accessing them.

---

## 2. Canonical Value Mapping

These mappings are frozen.

### 2.1 Value Model

| Strata Value | Python           | CLI JSON      |
| ------------ | ---------------- | ------------- |
| Null         | None             | null          |
| Bool         | bool             | true/false    |
| Int(i64)     | int              | number        |
| Float(f64)   | float            | number        |
| String       | str              | string        |
| Bytes        | bytes            | base64 string |
| Array        | list             | array         |
| Object       | dict[str, Value] | object        |

Rules:

* No implicit coercions
* No auto-promotion
* Bytes are not strings
* Null ≠ missing

---

### 2.2 Versioned<T>

Python:

```python
class Versioned(Generic[T]):
    value: T
    version: Version
    timestamp: int
```

CLI:

```json
{
  "value": ...,
  "version": { "type": "...", "value": 123 },
  "timestamp": 1700000000000000
}
```

---

## 3. Default Facade API Mapping

### 3.1 Python

```python
db = Strata.open()

db.set("x", 123)
db.get("x")           # -> 123
db.exists("x")        # -> True
db.delete(["x"])      # -> 1
db.mget(["a", "b"])   # -> [None, 5]
```

---

### 3.2 CLI

```
$ strata set x 123
$ strata get x
123

$ strata exists x
true

$ strata delete x
1

$ strata mget a b
[null, 5]
```

---

### 3.3 JSON Facade

Python:

```python
db.json_set("doc", "$.a.b", 5)
db.json_get("doc", "$.a.b")  # -> 5
db.json_del("doc", "$.a")
```

CLI:

```
$ strata json.set doc $.a.b 5
$ strata json.get doc $.a.b
5
$ strata json.del doc $.a
```

---

### 3.4 Event Facade

Python:

```python
id = db.xadd("stream", {"foo": 1})
events = db.xrange("stream", start=None, end=None, limit=10)
```

CLI:

```
$ strata xadd stream '{"foo":1}'
{ "type": "Sequence", "value": 123 }

$ strata xrange stream
[
  { "value": {...}, "version": {...}, "timestamp": ... }
]
```

---

### 3.5 Vector Facade

Python:

```python
db.vset("doc1", [0.1, 0.2], {"tag": "a"})
entry = db.vget("doc1")
```

CLI:

```
$ strata vset doc1 "[0.1, 0.2]" '{"tag":"a"}'
$ strata vget doc1
```

---

## 4. Advanced Mode Mapping

### 4.1 Runs

Python:

```python
runs = db.runs()
r = db.create_run({"purpose": "test"})
scoped = db.use_run(r.run_id)
```

CLI:

```
$ strata runs
$ strata run.create '{"purpose":"test"}'
$ strata run.use <run_id>
```

---

### 4.2 Versioned APIs

Python:

```python
db.getv("x")                  # -> Versioned[int]
db.history("x")              # -> list[Versioned[int]]
db.get_at("x", version)
```

CLI:

```
$ strata getv x
$ strata history x
$ strata get_at x '{"type":"TxnId","value":12}'
```

---

### 4.3 Transactions

Python:

```python
with db.transaction() as tx:
    tx.set("a", 1)
    tx.set("b", 2)
```

CLI:

```
$ strata txn.begin
$ strata set a 1
$ strata set b 2
$ strata txn.commit
```

---

## 5. Error Mapping

### 5.1 Python

Errors are raised as typed exceptions:

```python
class StrataError(Exception):
    code: str
    message: str
    details: dict | None
```

Example:

```python
try:
    db.get_at("x", version)
except HistoryTrimmed as e:
    print(e.requested, e.earliest_retained)
```

---

### 5.2 CLI

Errors are printed as JSON:

```json
{
  "error": {
    "code": "HistoryTrimmed",
    "message": "Requested version no longer retained",
    "details": {
      "requested": {...},
      "earliest_retained": {...}
    }
  }
}
```

Exit code is nonzero.

---

## 6. Wire Parity Guarantee

Python SDK and CLI must obey:

* Wire encoding spec
* Error shapes
* Return shapes
* Optional semantics

This ensures:

Python = CLI = Rust = JS = Server

No divergence allowed.

---

## 7. Discoverability

### 7.1 Python

```python
help(db)
help(db.json)
help(db.events)
```

---

### 7.2 CLI

```
$ strata help
$ strata help json
$ strata help advanced
```

---

## 8. Freezing Rules

After M11:

The following are frozen:

* Method names
* Return shapes
* Error shapes
* Value mapping
* CLI command names
* Output formats
* Versioned<T> shape

These become part of Strata’s identity.

---

## Next: Freeze Audit

If you want, I will now:

1. Cross-check this against facade spec
2. Cross-check against substrate spec
3. Cross-check against wire spec
4. Generate a 10b Freeze Readiness Checklist
5. Flag any inconsistencies

Say: **Run the freeze audit.**

Once frozen, Strata becomes real.
