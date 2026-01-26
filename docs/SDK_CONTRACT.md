# Strata SDK Contract

> **Version**: 1.0
> **Status**: Authoritative

This document defines the contract for implementing Strata SDKs. All SDKs—regardless of language—MUST conform to this specification.

---

## Table of Contents

1. [Architecture Overview](#1-architecture-overview)
2. [Database Lifecycle](#2-database-lifecycle)
3. [Executor Commands](#3-executor-commands)
4. [Simple API](#4-simple-api)
5. [Error Handling](#5-error-handling)
6. [Client Patterns](#6-client-patterns)
7. [Conformance Requirements](#7-conformance-requirements)

---

## 1. Architecture Overview

Strata SDKs expose two API tiers:

```
┌────────────────────────────────────────────────────────┐
│                      User Code                         │
│                  db.set("key", "value")                │
└────────────────────────────────────────────────────────┘
                           │
                           ▼
┌────────────────────────────────────────────────────────┐
│                     Simple API                         │
│  • Implicit default run                                │
│  • Simplified return types                             │
│  • Desugars to Executor commands                       │
└────────────────────────────────────────────────────────┘
                           │
                           ▼
┌────────────────────────────────────────────────────────┐
│                   Executor API                         │
│  • Explicit run on every command                       │
│  • Full return types (versions, timestamps)            │
│  • Canonical command set                               │
└────────────────────────────────────────────────────────┘
                           │
                           ▼
┌────────────────────────────────────────────────────────┐
│                      Database                          │
└────────────────────────────────────────────────────────┘
```

**Simple API**: Redis-like ergonomics. Users call `set("key", "value")` without thinking about runs or versions.

**Executor API**: Full power. Every command explicitly specifies the run and receives complete output including versions.

Both tiers MUST be implemented. The Simple API is the primary user interface; the Executor API is available for advanced use cases.

### Core Principle: No Magic

Every Simple API call maps to exactly ONE Executor command. No hidden logic, no implicit batching, no automatic retries. The mapping is documented and deterministic.

---

## 2. Database Lifecycle

### 2.1 Opening a Database

SDKs MUST provide these factory methods:

| Method | Description |
|--------|-------------|
| `open(path)` | Open or create database at path with default durability |
| `open_temp()` | Create temporary database, deleted on close |
| `ephemeral()` | Create in-memory database with no disk I/O |
| `builder()` | Create builder for custom configuration |

### 2.2 Builder Pattern

The builder allows custom configuration:

```
builder()
    .path(path)                              // Set database path
    .no_durability()                         // No fsync (fastest, data lost on crash)
    .strict()                                // fsync every write (slowest, safest)
    .buffered()                              // Batched fsync (balanced, recommended)
    .buffered_with(interval_ms, batch_size)  // Custom batched settings
    .open()                                  // Open at configured path
    .open_temp()                             // Open at temporary path
```

### 2.3 Durability Modes

| Mode | Description | Latency | Throughput | Data Loss on Crash |
|------|-------------|---------|------------|-------------------|
| None | No fsync | <3µs | 250K+ ops/sec | All uncommitted data |
| Strict | fsync every write | ~2ms | ~500 ops/sec | Zero |
| Batched | fsync periodically | <30µs | 50K+ ops/sec | Up to interval/batch |

**Batched defaults**: 100ms interval, 1000 write batch size. Fsync triggers on whichever threshold is reached first.

**Recommended**: Use `buffered()` for production, `no_durability()` for tests.

### 2.4 Storage Modes

| Method | Disk Files | WAL | Data Survives Restart |
|--------|------------|-----|----------------------|
| `open(path)` | Yes | Yes | Yes |
| `open_temp()` | Yes (temp dir) | Yes | No (deleted on close) |
| `ephemeral()` | None | None | No |

### 2.5 Closing

SDKs MUST ensure proper cleanup:
- Flush pending writes
- Release file locks
- Delete temp files (for `open_temp()`)

Implement language-idiomatic patterns (context managers, RAII, try-finally).

---

## 3. Executor Commands

The Executor is the canonical interface. It accepts commands and returns outputs.

```
execute(command) → output | error
```

### 3.1 Command Summary

| Category | Commands | Description |
|----------|----------|-------------|
| KV | 15 | Key-value storage with versioning |
| JSON | 17 | Document storage with path operations |
| Event | 11 | Append-only event streams |
| State | 8 | CAS-based coordination cells |
| Vector | 19 | Similarity search |
| Run | 24 | Run lifecycle management |
| Transaction | 5 | Transaction control |
| Database | 4 | Database operations |
| **Total** | **103** | |

### 3.2 KV Commands

| Command | Parameters | Output |
|---------|------------|--------|
| KvPut | run, key, value | Version |
| KvGet | run, key | MaybeVersioned |
| KvGetAt | run, key, version | Versioned |
| KvDelete | run, key | Bool |
| KvExists | run, key | Bool |
| KvHistory | run, key, limit?, before? | VersionedList |
| KvIncr | run, key, delta | Int |
| KvCasVersion | run, key, expected_version?, new_value | MaybeVersion |
| KvCasValue | run, key, expected_value?, new_value | MaybeVersion |
| KvKeys | run, prefix?, limit?, cursor? | Keys |
| KvScan | run, prefix?, limit?, cursor? | ScanResult |
| KvMget | run, keys[] | MaybeVersionedList |
| KvMput | run, entries[] | Version |
| KvMdelete | run, keys[] | Count |
| KvMexists | run, keys[] | Count |

### 3.3 JSON Commands

| Command | Parameters | Output |
|---------|------------|--------|
| JsonSet | run, key, path, value | Version |
| JsonGet | run, key, path | MaybeVersioned |
| JsonDelete | run, key, path | Count |
| JsonMerge | run, key, path, patch | Version |
| JsonHistory | run, key, limit?, before? | VersionedList |
| JsonExists | run, key | Bool |
| JsonGetVersion | run, key | MaybeVersion |
| JsonSearch | run, query, k | SearchHits |
| JsonList | run, prefix?, cursor?, limit | ListResult |
| JsonCas | run, key, expected_version, path, value | Version |
| JsonQuery | run, path, value, limit | Keys |
| JsonCount | run | Count |
| JsonBatchGet | run, keys[] | MaybeVersionedList |
| JsonBatchCreate | run, docs[] | Versions |
| JsonArrayPush | run, key, path, values[] | Count |
| JsonIncrement | run, key, path, delta | Float |
| JsonArrayPop | run, key, path | MaybeValue |

### 3.4 Event Commands

| Command | Parameters | Output |
|---------|------------|--------|
| EventAppend | run, stream, payload | Sequence |
| EventAppendBatch | run, events[] | Sequences |
| EventRange | run, stream, start?, end?, limit? | VersionedList |
| EventGet | run, stream, sequence | MaybeVersioned |
| EventLen | run, stream | Count |
| EventLatestSequence | run, stream | MaybeSequence |
| EventStreamInfo | run, stream | StreamInfo |
| EventRevRange | run, stream, start?, end?, limit? | VersionedList |
| EventStreams | run | Strings |
| EventHead | run, stream | MaybeVersioned |
| EventVerifyChain | run | ChainVerification |

### 3.5 State Commands

| Command | Parameters | Output |
|---------|------------|--------|
| StateSet | run, cell, value | Version |
| StateGet | run, cell | MaybeVersioned |
| StateCas | run, cell, expected_counter?, value | MaybeVersion |
| StateDelete | run, cell | Bool |
| StateExists | run, cell | Bool |
| StateHistory | run, cell, limit?, before? | VersionedList |
| StateInit | run, cell, value | Version |
| StateList | run | Strings |

### 3.6 Vector Commands

| Command | Parameters | Output |
|---------|------------|--------|
| VectorUpsert | run, collection, key, vector, metadata? | Version |
| VectorGet | run, collection, key | MaybeVectorData |
| VectorDelete | run, collection, key | Bool |
| VectorSearch | run, collection, query, k, filter?, metric? | Matches |
| VectorSearchWithBudget | run, collection, query, k, filter?, budget | MatchesWithFlag |
| VectorCollectionInfo | run, collection | MaybeCollectionInfo |
| VectorCreateCollection | run, collection, dimension, metric | Version |
| VectorDropCollection | run, collection | Bool |
| VectorListCollections | run | CollectionList |
| VectorCollectionExists | run, collection | Bool |
| VectorCount | run, collection | Count |
| VectorUpsertBatch | run, collection, vectors[] | BatchResult |
| VectorGetBatch | run, collection, keys[] | VectorDataList |
| VectorDeleteBatch | run, collection, keys[] | Bools |
| VectorHistory | run, collection, key, limit?, before? | VectorDataList |
| VectorGetAt | run, collection, key, version | MaybeVectorData |
| VectorListKeys | run, collection, limit?, cursor? | Keys |
| VectorScan | run, collection, limit?, cursor? | VectorScanResult |
| VectorUpsertWithSource | run, collection, key, vector, metadata?, source? | Version |

### 3.7 Run Commands

| Command | Parameters | Output |
|---------|------------|--------|
| RunCreate | run_id?, metadata? | RunWithVersion |
| RunGet | run | MaybeRunInfo |
| RunList | status?, limit?, offset? | RunInfoList |
| RunClose | run | Version |
| RunUpdateMetadata | run, metadata | Version |
| RunExists | run | Bool |
| RunPause | run | Version |
| RunResume | run | Version |
| RunFail | run, error | Version |
| RunCancel | run | Version |
| RunArchive | run | Version |
| RunDelete | run | Unit |
| RunQueryByStatus | status | RunInfoList |
| RunQueryByTag | tag | RunInfoList |
| RunCount | status? | Count |
| RunSearch | query, limit? | RunInfoList |
| RunAddTags | run, tags[] | Version |
| RunRemoveTags | run, tags[] | Version |
| RunGetTags | run | Strings |
| RunCreateChild | parent, metadata? | RunWithVersion |
| RunGetChildren | parent | RunInfoList |
| RunGetParent | run | MaybeRunId |
| RunSetRetention | run, policy | Version |
| RunGetRetention | run | RetentionPolicy |

### 3.8 Transaction Commands

| Command | Parameters | Output |
|---------|------------|--------|
| TxnBegin | options? | TxnId |
| TxnCommit | | Version |
| TxnRollback | | Unit |
| TxnInfo | | MaybeTxnInfo |
| TxnIsActive | | Bool |

### 3.9 Database Commands

| Command | Parameters | Output |
|---------|------------|--------|
| Ping | | Pong |
| Info | | DatabaseInfo |
| Flush | | Unit |
| Compact | | Unit |

### 3.10 Output Types

| Type | Structure |
|------|-----------|
| Unit | (none) |
| Bool | boolean |
| Int | signed 64-bit integer |
| Count | unsigned 64-bit integer |
| Float | 64-bit float |
| Version | unsigned 64-bit integer |
| MaybeVersion | optional Version |
| Versioned | { value, version, timestamp } |
| MaybeVersioned | optional Versioned |
| VersionedList | array of Versioned |
| MaybeVersionedList | array of optional Versioned |
| Keys | array of strings |
| Strings | array of strings |
| ScanResult | { entries: [(key, Versioned)], cursor? } |

---

## 4. Simple API

The Simple API provides ergonomic access using implicit defaults.

### 4.1 Desugaring Rules

Every Simple API call desugars to one Executor command:

1. **Run**: Use `"default"` run
2. **Execute**: Call corresponding Executor command
3. **Translate**: Convert output to simplified type

### 4.2 KV Methods

| Simple Method | Executor Command | Output Translation |
|---------------|------------------|-------------------|
| `set(key, value)` | KvPut(default, key, value) | Version → void |
| `get(key)` | KvGet(default, key) | MaybeVersioned → optional Value |
| `getv(key)` | KvGet(default, key) | MaybeVersioned → MaybeVersioned |
| `del(key)` | KvDelete(default, key) | Bool → bool |
| `exists(key)` | KvExists(default, key) | Bool → bool |
| `incr(key)` | KvIncr(default, key, 1) | Int → int |
| `incrby(key, delta)` | KvIncr(default, key, delta) | Int → int |
| `setnx(key, value)` | KvCasVersion(default, key, null, value) | MaybeVersion → bool |
| `mget(keys)` | KvMget(default, keys) | MaybeVersionedList → optional Value[] |
| `mset(entries)` | KvMput(default, entries) | Version → void |

### 4.3 JSON Methods

| Simple Method | Executor Command | Output Translation |
|---------------|------------------|-------------------|
| `json_set(key, path, value)` | JsonSet(default, key, path, value) | Version → void |
| `json_get(key, path)` | JsonGet(default, key, path) | MaybeVersioned → optional Value |
| `json_del(key, path)` | JsonDelete(default, key, path) | Count → int |
| `json_merge(key, path, patch)` | JsonMerge(default, key, path, patch) | Version → void |
| `json_push(key, path, values)` | JsonArrayPush(default, key, path, values) | Count → int |
| `json_incr(key, path, delta)` | JsonIncrement(default, key, path, delta) | Float → float |

### 4.4 Event Methods

| Simple Method | Executor Command | Output Translation |
|---------------|------------------|-------------------|
| `xadd(stream, payload)` | EventAppend(default, stream, payload) | Sequence → int |
| `xrange(stream, start, end)` | EventRange(default, stream, start, end, null) | VersionedList → Event[] |
| `xlen(stream)` | EventLen(default, stream) | Count → int |
| `xlast(stream)` | EventLatestSequence(default, stream) | MaybeSequence → optional int |

### 4.5 State Methods

| Simple Method | Executor Command | Output Translation |
|---------------|------------------|-------------------|
| `state_set(cell, value)` | StateSet(default, cell, value) | Version → void |
| `state_get(cell)` | StateGet(default, cell) | MaybeVersioned → optional Value |
| `state_getv(cell)` | StateGet(default, cell) | MaybeVersioned → MaybeVersioned |
| `state_del(cell)` | StateDelete(default, cell) | Bool → bool |
| `state_cas(cell, expected, value)` | StateCas(default, cell, expected, value) | MaybeVersion → optional int |

### 4.6 Vector Methods

| Simple Method | Executor Command | Output Translation |
|---------------|------------------|-------------------|
| `vadd(coll, key, vec, meta?)` | VectorUpsert(default, coll, key, vec, meta) | Version → void |
| `vget(coll, key)` | VectorGet(default, coll, key) | MaybeVectorData → optional VectorData |
| `vdel(coll, key)` | VectorDelete(default, coll, key) | Bool → bool |
| `vsearch(coll, query, k)` | VectorSearch(default, coll, query, k, null, null) | Matches → Match[] |
| `vcreate(coll, dim, metric)` | VectorCreateCollection(default, coll, dim, metric) | Version → void |
| `vdrop(coll)` | VectorDropCollection(default, coll) | Bool → bool |

### 4.7 Run Scoping

To operate on a non-default run:

```
scoped = client.use_run("my-run")
scoped.set("key", "value")  // Targets "my-run" instead of "default"
```

`use_run(run_id)` returns a new client instance with a different default run.

### 4.8 Escape Hatches

Methods ending in `v` return full versioned data:
- `get(key)` → optional Value
- `getv(key)` → optional { value, version, timestamp }

For full control, access the executor directly:
```
client.executor.execute(command)
```

---

## 5. Error Handling

### 5.1 Error Types

| Error | Fields | When |
|-------|--------|------|
| NotFound | entity, key | Resource doesn't exist |
| AlreadyExists | entity, key | Resource already exists |
| WrongType | expected, actual | Type mismatch |
| InvalidInput | reason | Bad parameter |
| VersionConflict | expected, actual | CAS failed |
| Conflict | reason | Generic conflict |
| RunClosed | run | Operation on closed run |
| DimensionMismatch | expected, actual | Vector dimension wrong |
| ConstraintViolation | reason | Constraint violated |
| TransactionError | reason | Transaction issue |
| IoError | reason | I/O failure |
| InternalError | reason | Internal failure |

### 5.2 Error Propagation

1. **Never swallow errors** - All Executor errors reach the user
2. **Preserve details** - Error fields must be accessible
3. **Use language idioms** - Exceptions, Result types, error returns

---

## 6. Client Patterns

SDKs MUST ship these helper functions.

### 6.1 State Transition (Optimistic Concurrency)

```
function state_transition(client, cell, transform_fn, max_retries=10):
    for attempt in 0..max_retries:
        current = client.state_getv(cell)
        if current is null:
            raise NotFound

        new_value = transform_fn(current.value)
        result = client.state_cas(cell, current.version, new_value)

        if result is not null:
            return (new_value, result)

        sleep(exponential_backoff(attempt))

    raise ConflictError("max retries exceeded")
```

### 6.2 Get Or Initialize

```
function get_or_init(client, key, default_fn):
    value = client.get(key)
    if value is not null:
        return value

    default = default_fn()
    if client.setnx(key, default):
        return default

    return client.get(key)
```

---

## 7. Conformance Requirements

### 7.1 Required Implementations

- [ ] Database lifecycle (open, open_temp, ephemeral, builder, close)
- [ ] All durability modes (None, Strict, Batched)
- [ ] All 103 Executor commands
- [ ] All Simple API methods with correct desugaring
- [ ] All error types with proper propagation
- [ ] Run scoping via use_run()
- [ ] Escape hatches (getv, state_getv, executor access)
- [ ] Client patterns (state_transition, get_or_init)

### 7.2 Conformance Tests

SDKs MUST pass tests verifying:

1. **Desugaring**: Each Simple method generates correct Executor command
2. **Output Translation**: Outputs are correctly simplified
3. **Error Propagation**: All errors reach user with details intact
4. **Run Scoping**: use_run() correctly targets specified run
5. **Patterns**: Helper functions work correctly

### 7.3 Documentation Requirements

SDK documentation MUST:
- Show Simple API as primary interface
- Document all desugaring rules
- Explain escape hatches for advanced use
- Include durability mode guidance

---

## Appendix: Type Definitions

### Value

Supported value types:
- Null
- Bool
- Int (signed 64-bit)
- Float (64-bit)
- String
- Bytes
- Array
- Object

### Versioned

```
{
    value: Value,
    version: uint64,
    timestamp: uint64 (nanoseconds since epoch)
}
```

### DistanceMetric

- Cosine
- Euclidean
- DotProduct

### RunStatus

- Active
- Paused
- Closed
- Failed
- Cancelled
- Archived

### RetentionPolicy

- KeepAll
- KeepLast { count }
- KeepSince { timestamp }
- KeepVersions { count }
