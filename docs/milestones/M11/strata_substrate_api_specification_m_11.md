# Strata Substrate API Specification (M11 / Phase 10b)

**Status**: Draft

**Scope**: Core power-user and systems-facing API

**Audience**: AI engineers, systems developers, SDK implementers, advanced users

**Goal**: Expose Strata’s full semantic power: runs, versions, transactions, history, primitives, determinism. This API is the *truth layer* of Strata. The Facade API is a convenience layer over this.

---

## 1. Design Philosophy

The Substrate API is the canonical semantic contract of Strata.

It must:
- Expose *all* primitives explicitly
- Expose *all* versioning
- Expose *all* run scoping
- Expose *all* transactional semantics
- Be deterministic and replayable
- Be minimal, not friendly
- Be unambiguous
- Be stable

This is not a UX surface. This is a *semantic substrate*.

Every other API must desugar into this one.

---

## 2. Core Concepts

### 2.1 Run

A **Run** is the unit of isolation, history, replay, and reasoning.

Properties:
- Every entity belongs to exactly one run
- Transactions are scoped to a run
- History is scoped to a run
- Retention is scoped to a run

```rust
struct RunId(String) // opaque

struct RunInfo {
    run_id: RunId,
    created_at: Timestamp,
    metadata: Value
}
```

---

### 2.2 Version

A **Version** is a monotonic identity for a mutation.

```rust
enum Version {
    TxnId(u64),
    Sequence(u64),
    Counter(u64)
}
```

Rules:
- Versions are totally ordered within an entity
- Versions are immutable
- Versions are never reassigned
- Versions are assigned by the engine, not storage

---

### 2.3 Timestamp

```rust
struct Timestamp(u64) // microseconds since epoch
```

Rules:
- Always monotonic per run
- Always attached to Versioned<T>

---

### 2.4 Versioned

```rust
struct Versioned<T> {
    value: T,
    version: Version,
    timestamp: Timestamp
}
```

This is the universal return wrapper.

---

### 2.5 EntityRef

Every object is addressed explicitly.

```rust
enum EntityRef {
    KV { run: RunId, key: String },
    Json { run: RunId, key: String },
    EventLog { run: RunId, stream: String },
    StateCell { run: RunId, key: String },
    Vector { run: RunId, key: String },
    Trace { run: RunId, id: String },
    Run { run: RunId }
}
```

---

## 3. Canonical Value Model

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

Rules:
- This is the only public value model
- JSON is a strict subset
- No implicit coercion
- No lossy conversion

---

## 4. Transactions

All primitives participate in transactions.

```rust
begin(run_id: RunId) -> Txn
commit(txn: Txn) -> ()
rollback(txn: Txn) -> ()
```

Rules:
- Atomic
- Isolated
- Deterministic
- Idempotent on replay

---

## 5. Primitive APIs

### 5.1 KVStore

```rust
kv_put(run, key, value) -> Version
kv_get(run, key) -> Option<Versioned<Value>>
kv_get_at(run, key, version) -> Versioned<Value> | HistoryTrimmed
kv_delete(run, key) -> bool
kv_exists(run, key) -> bool
kv_history(run, key) -> Vec<Versioned<Value>>
```

---

### 5.2 JsonStore

```rust
json_set(run, key, path, value) -> Version
json_get(run, key, path) -> Option<Versioned<Value>>
json_delete(run, key, path) -> u64
json_history(run, key) -> Vec<Versioned<Value>>
```

---

### 5.3 EventLog

```rust
event_append(run, stream, payload: Value::Object) -> Version
event_range(run, stream, start, end, limit) -> Vec<Versioned<Value>>
```

---

### 5.4 StateCell

```rust
state_get(run, key) -> Option<Versioned<Value>>
state_set(run, key, value) -> Version
state_cas(run, key, expected, new) -> bool
```

---

### 5.5 VectorStore

```rust
vector_set(run, key, vector: Vec<f32>, metadata: Value::Object) -> Version
vector_get(run, key) -> Option<Versioned<{vector, metadata}>>
vector_delete(run, key) -> bool
```

---

### 5.6 TraceStore

```rust
trace_record(run, trace_type, payload) -> Version
trace_get(run, id) -> Option<Versioned<Value>>
trace_range(run, start, end) -> Vec<Versioned<Value>>
```

---

### 5.7 RunIndex

```rust
run_create(metadata: Value) -> RunId
run_get(run: RunId) -> Option<RunInfo>
run_list() -> Vec<RunInfo>
run_delete(run: RunId) -> ()
```

---

## 6. History Semantics

```rust
get_at(entity, version) -> Versioned<T> | HistoryTrimmed
history(entity) -> Vec<Versioned<T>>
latest_version(entity) -> Option<Version>
```

---

## 7. Error Model

```rust
enum StrataError {
    NotFound,
    WrongType,
    InvalidKey,
    InvalidPath,
    HistoryTrimmed { requested: Version, earliest_retained: Version },
    ConstraintViolation,
    SerializationError,
    StorageError,
    InternalError
}
```

---

## 8. Retention

```rust
enum RetentionPolicy {
    KeepAll,
    KeepLast(u64),
    KeepFor(Duration),
    Composite(Vec<RetentionPolicy>)
}
```

Stored as first-class DB entries.

---

## 9. Determinism Guarantees

- Same operations → same state
- Same WAL → same state
- Replay is idempotent
- Compaction is invisible

---

## 10. Relationship to Facade API

The Facade API is a strict subset.

Every facade call maps to exactly one substrate call.

No semantics are hidden.

---

## 11. Non-Goals

This API does not:
- Optimize UX
- Hide complexity
- Guess intent
- Auto-coerce
- Auto-merge

This is the truth layer.

