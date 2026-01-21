Below is the **Facade → Substrate Desugaring Table** in markdown.

This table is the semantic spine of Strata. Every facade call must desugar into a deterministic, explicit substrate operation. This is what prevents “magic layers” and ensures we do not Redis-ify ourselves into semantic ambiguity.

---

# Strata Facade → Substrate Desugaring Table (M11 / Phase 10b)

Status: Draft
Scope: Canonical mapping between Facade API and Substrate API
Audience: Core developers, SDK authors, protocol designers
Purpose: Guarantee semantic alignment, debuggability, and future-proofing

---

## Core Assumptions

Default Facade Mode assumes:

| Concept              | Default                  |
| -------------------- | ------------------------ |
| Implicit run         | `DefaultRun`             |
| Implicit primitive   | KVStore unless specified |
| Implicit version     | Latest committed         |
| Implicit transaction | Auto-commit per call     |
| Implicit namespace   | User space (not system)  |
| History              | Hidden                   |
| CAS                  | Hidden                   |
| EntityRef            | Hidden                   |

All of these become explicit in the Substrate API.

---

## 1. KV Operations

### 1.1 set(key, value)

Facade:

```ts
set(key: String, value: Value) -> ()
```

Desugars to:

```ts
txn {
  run = DefaultRun
  entity = EntityRef::kv(key)
  primitive = KVStore
  op = Put(value)
}
commit()
```

Substrate equivalent:

```ts
begin_txn()
put(run=DefaultRun, primitive=KVStore, key, value)
commit()
```

Notes:

* Creates a new version
* Overwrites latest version
* No version is returned

---

### 1.2 get(key)

Facade:

```ts
get(key: String) -> Option<Value>
```

Desugars to:

```ts
get_latest(run=DefaultRun, primitive=KVStore, key)
```

Substrate equivalent:

```ts
read(
  run=DefaultRun,
  entity=EntityRef::kv(key),
  selector=Latest
)
```

---

### 1.3 getv(key)

Facade:

```ts
getv(key: String) -> Option<Versioned<Value>>
```

Desugars to:

```ts
read_with_version(
  run=DefaultRun,
  primitive=KVStore,
  key,
  selector=Latest
)
```

---

### 1.4 mget(keys)

Facade:

```ts
mget(keys: Vec<String>) -> Vec<Option<Value>>
```

Desugars to:

```ts
batch {
  for key in keys:
    read_latest(run=DefaultRun, primitive=KVStore, key)
}
```

Substrate equivalent:

```ts
batch_read([
  (DefaultRun, KVStore, key1, Latest),
  (DefaultRun, KVStore, key2, Latest),
  ...
])
```

---

### 1.5 delete(keys)

Facade:

```ts
delete(keys: Vec<String>) -> u64
```

Desugars to:

```ts
txn {
  for key in keys:
    delete(run=DefaultRun, primitive=KVStore, key)
}
commit()
```

Returns: count of successful deletes.

---

### 1.6 exists(key)

Facade:

```ts
exists(key: String) -> bool
```

Desugars to:

```ts
read_latest(run=DefaultRun, primitive=KVStore, key) != None
```

---

### 1.7 incr(key, delta)

Facade:

```ts
incr(key: String, delta: i64) -> i64
```

Desugars to:

```ts
txn {
  cur = read_latest(...)
  assert type(cur) == Int
  new = cur + delta
  put(new)
}
commit()
return new
```

Substrate equivalent:

```ts
begin_txn()
v = read(run, key, Latest)
assert_int(v)
put(run, key, v + delta)
commit()
```

---

## 2. JSON Operations

### 2.1 json_set(key, path, value)

Facade:

```ts
json_set(key, path, value) -> ()
```

Desugars to:

```ts
txn {
  doc = read_latest(...)
  new_doc = apply_path_set(doc, path, value)
  put(new_doc)
}
commit()
```

Substrate equivalent:

```ts
begin_txn()
doc = read(...)
doc2 = json_patch(doc, path, value)
put(...)
commit()
```

---

### 2.2 json_get(key, path)

Facade:

```ts
json_get(key, path) -> Option<Value>
```

Desugars to:

```ts
doc = read_latest(...)
return apply_path_get(doc, path)
```

---

### 2.3 json_del(key, path)

Facade:

```ts
json_del(key, path) -> u64
```

Desugars to:

```ts
txn {
  doc = read_latest(...)
  (new_doc, deleted_count) = apply_path_delete(doc, path)
  put(new_doc)
}
commit()
return deleted_count
```

---

## 3. Event (Stream) Operations

### 3.1 xadd(stream, payload)

Facade:

```ts
xadd(stream: String, payload: Object) -> Version
```

Desugars to:

```ts
txn {
  append(run=DefaultRun, primitive=EventLog, stream, payload)
}
commit()
return version
```

Substrate equivalent:

```ts
append_event(run, stream, payload)
```

---

### 3.2 xrange(stream, start, end, limit)

Facade:

```ts
xrange(...) -> Vec<Versioned<Value>>
```

Desugars to:

```ts
scan(
  run=DefaultRun,
  primitive=EventLog,
  range=[start, end],
  limit
)
```

---

## 4. Vector Operations

### 4.1 vset(key, vector, metadata)

Facade:

```ts
vset(key, vector, metadata) -> ()
```

Desugars to:

```ts
txn {
  put(run=DefaultRun, primitive=VectorStore, key, {vector, metadata})
}
commit()
```

---

### 4.2 vget(key)

Facade:

```ts
vget(key) -> Option<{vector, metadata}>
```

Desugars to:

```ts
read_latest(run=DefaultRun, primitive=VectorStore, key)
```

---

### 4.3 vdel(key)

Facade:

```ts
vdel(key) -> bool
```

Desugars to:

```ts
txn {
  delete(run=DefaultRun, primitive=VectorStore, key)
}
commit()
```

---

## 5. CAS / StateCell Operations

### 5.1 cas_set(key, expected, new)

Facade:

```ts
cas_set(key, expected, new) -> bool
```

Desugars to:

```ts
txn {
  cur = read_latest(...)
  if cur == expected:
    put(new)
    commit()
    return true
  else:
    rollback()
    return false
}
```

Substrate equivalent:

```ts
compare_and_swap(run, key, expected, new)
```

---

## 6. History & Version Operations

### 6.1 history(key)

Facade:

```ts
history(key) -> Vec<Versioned<Value>>
```

Desugars to:

```ts
scan_versions(run=DefaultRun, primitive=KVStore, key)
```

---

### 6.2 get_at(key, version)

Facade:

```ts
get_at(key, version) -> Value | HistoryTrimmed
```

Desugars to:

```ts
read(run=DefaultRun, primitive=KVStore, key, selector=At(version))
```

---

### 6.3 latest_version(key)

Facade:

```ts
latest_version(key) -> Option<Version>
```

Desugars to:

```ts
read_latest_metadata(...)
```

---

## 7. Run Operations

### 7.1 runs()

Facade:

```ts
runs() -> Vec<RunInfo>
```

Desugars to:

```ts
list_runs()
```

---

### 7.2 use_run(run_id)

Facade:

```ts
use_run(run_id) -> ScopedFacade
```

Desugars to:

```ts
Facade {
  default_run = run_id
}
```

No substrate operation. Pure client-side binding.

---

## 8. Transactions

Facade auto-commit mode:

```ts
set(...)
```

Desugars to:

```ts
begin_txn()
put(...)
commit()
```

Explicit substrate usage:

```ts
txn = begin()
txn.put(...)
txn.put(...)
txn.commit()
```

---

## 9. Error Mapping

| Facade Error        | Substrate Cause                    |
| ------------------- | ---------------------------------- |
| NotFound            | read returned None                 |
| WrongType           | Value model mismatch               |
| InvalidKey          | Key parsing or namespace violation |
| InvalidPath         | JSON path resolution failure       |
| HistoryTrimmed      | Version < retention floor          |
| ConstraintViolation | CAS failure, schema failure        |
| SerializationError  | Value encode/decode failure        |
| StorageError        | WAL / snapshot / IO failure        |
| InternalError       | Invariant violation                |

---

## 10. Why This Table Matters

This table enforces:

• No semantic shortcuts
• No hidden state
• No magic
• No leaky abstractions
• No Redis cosplay

Everything a facade call does is an explicit substrate action.

This is what makes Strata composable, debuggable, and evolvable.


