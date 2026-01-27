# Executor ↔ Engine API Report

Comprehensive mapping between the executor's public command interface and the
engine's primitive methods. This document serves as the source of truth for what
is exposed, what is covered, and what gaps exist.

**Generated**: 2026-01-27
**Branch**: `refactor/codebase-cleanup`

---

## Table of Contents

1. [Architecture Overview](#1-architecture-overview)
2. [Executor Public Interface](#2-executor-public-interface)
   - 2.1 [Command Enum (106 variants)](#21-command-enum-106-variants)
   - 2.2 [Output Enum (40 variants)](#22-output-enum-40-variants)
   - 2.3 [Error Enum (22 variants)](#23-error-enum-22-variants)
3. [Engine Primitive APIs](#3-engine-primitive-apis)
   - 3.1 [Database](#31-database)
   - 3.2 [KVStore](#32-kvstore)
   - 3.3 [JsonStore](#33-jsonstore)
   - 3.4 [EventLog](#34-eventlog)
   - 3.5 [StateCell](#35-statecell)
   - 3.6 [VectorStore](#36-vectorstore)
   - 3.7 [RunIndex](#37-runindex)
   - 3.8 [Transaction Extension Traits](#38-transaction-extension-traits)
4. [Bidirectional Mapping](#4-bidirectional-mapping)
   - 4.1 [Executor → Engine (Forward)](#41-executor--engine-forward)
   - 4.2 [Engine → Executor (Reverse)](#42-engine--executor-reverse)
5. [Gap Analysis](#5-gap-analysis)
6. [Validation Rules](#6-validation-rules)
7. [Naming Consistency Audit](#7-naming-consistency-audit)

---

## 1. Architecture Overview

```
Rust SDK     Python SDK     CLI     MCP Server
     │            │          │           │
     └────────────┴──────────┴───────────┘
                       │
          ┌────────────┴────────────┐
          │     Command (enum)      │  ← Typed, serializable
          │     106 variants        │
          └────────────┬────────────┘
                       │
          ┌────────────┴────────────┐
          │     bridge::Primitives  │  ← Holds 6 engine primitives
          │     + validation        │
          │     + type conversion   │
          └────────────┬────────────┘
                       │
          ┌────────────┴────────────┐
          │     Engine Primitives   │  ← ~170 public methods
          │  KV, JSON, Event,       │
          │  State, Vector, Run     │
          └─────────────────────────┘
```

The executor is a stateless command dispatcher. Each `Command` variant carries
all parameters needed for execution. The `bridge::Primitives` struct holds
`Arc<Database>` plus the six engine primitive structs and provides validation,
RunId conversion, and type conversion between executor types and engine types.

**Run resolution**: Data-scoped commands (KV, JSON, Event, State, Vector, Retention)
carry `run: Option<RunId>`. When `None`, the executor resolves it to the default
run (`RunId::default()` = nil UUID) via `Command::resolve_default_run()` at the
top of `Executor::execute()`. Run lifecycle commands (RunGet, RunClose, etc.)
still require `run: RunId` since they explicitly target a specific run.

---

## 2. Executor Public Interface

### 2.1 Command Enum (106 variants)

#### KV Commands (15)

| # | Command | Fields | Returns |
|---|---------|--------|---------|
| 1 | `KvPut` | `run: Option<RunId>, key: String, value: Value` | `Version(u64)` |
| 2 | `KvGet` | `run: Option<RunId>, key: String` | `MaybeVersioned(Option<VersionedValue>)` |
| 3 | `KvGetAt` | `run: Option<RunId>, key: String, version: u64` | `Versioned(VersionedValue)` |
| 4 | `KvDelete` | `run: Option<RunId>, key: String` | `Bool(bool)` |
| 5 | `KvExists` | `run: Option<RunId>, key: String` | `Bool(bool)` |
| 6 | `KvHistory` | `run: Option<RunId>, key: String, limit: Option<u64>, before: Option<u64>` | `VersionedValues(Vec<VersionedValue>)` |
| 7 | `KvIncr` | `run: Option<RunId>, key: String, delta: i64` | `Int(i64)` |
| 8 | `KvCasVersion` | `run: Option<RunId>, key: String, expected_version: Option<u64>, new_value: Value` | `Bool(bool)` |
| 9 | `KvCasValue` | `run: Option<RunId>, key: String, expected_value: Option<Value>, new_value: Value` | `Bool(bool)` |
| 10 | `KvKeys` | `run: Option<RunId>, prefix: String, limit: Option<u64>` | `Keys(Vec<String>)` |
| 11 | `KvScan` | `run: Option<RunId>, prefix: String, limit: u64, cursor: Option<String>` | `KvScanResult { entries, cursor }` |
| 12 | `KvMget` | `run: Option<RunId>, keys: Vec<String>` | `Values(Vec<Option<VersionedValue>>)` |
| 13 | `KvMput` | `run: Option<RunId>, entries: Vec<(String, Value)>` | `Version(u64)` |
| 14 | `KvMdelete` | `run: Option<RunId>, keys: Vec<String>` | `Uint(u64)` |
| 15 | `KvMexists` | `run: Option<RunId>, keys: Vec<String>` | `Uint(u64)` |

> **Note**: `KvMput` is currently **disabled** pending engine `transaction_with_version` API.

#### JSON Commands (17)

| # | Command | Fields | Returns |
|---|---------|--------|---------|
| 16 | `JsonSet` | `run: Option<RunId>, key: String, path: String, value: Value` | `Version(u64)` |
| 17 | `JsonGet` | `run: Option<RunId>, key: String, path: String` | `MaybeVersioned(Option<VersionedValue>)` |
| 18 | `JsonDelete` | `run: Option<RunId>, key: String, path: String` | `Uint(u64)` |
| 19 | `JsonMerge` | `run: Option<RunId>, key: String, path: String, patch: Value` | `Version(u64)` |
| 20 | `JsonHistory` | `run: Option<RunId>, key: String, limit: Option<u64>, before: Option<u64>` | `VersionedValues(Vec<VersionedValue>)` |
| 21 | `JsonExists` | `run: Option<RunId>, key: String` | `Bool(bool)` |
| 22 | `JsonGetVersion` | `run: Option<RunId>, key: String` | `MaybeVersion(Option<u64>)` |
| 23 | `JsonSearch` | `run: Option<RunId>, query: String, k: u64` | `JsonSearchHits(Vec<JsonSearchHit>)` |
| 24 | `JsonList` | `run: Option<RunId>, prefix: Option<String>, cursor: Option<String>, limit: u64` | `JsonListResult { keys, cursor }` |
| 25 | `JsonCas` | `run: Option<RunId>, key: String, expected_version: u64, path: String, value: Value` | `Version(u64)` |
| 26 | `JsonQuery` | `run: Option<RunId>, path: String, value: Value, limit: u64` | `Keys(Vec<String>)` |
| 27 | `JsonCount` | `run: Option<RunId>` | `Uint(u64)` |
| 28 | `JsonBatchGet` | `run: Option<RunId>, keys: Vec<String>` | `Values(Vec<Option<VersionedValue>>)` |
| 29 | `JsonBatchCreate` | `run: Option<RunId>, docs: Vec<(String, Value)>` | `Versions(Vec<u64>)` |
| 30 | `JsonArrayPush` | `run: Option<RunId>, key: String, path: String, values: Vec<Value>` | `Uint(u64)` |
| 31 | `JsonIncrement` | `run: Option<RunId>, key: String, path: String, delta: f64` | `Float(f64)` |
| 32 | `JsonArrayPop` | `run: Option<RunId>, key: String, path: String` | `Maybe(Option<Value>)` |

#### Event Commands (11)

| # | Command | Fields | Returns |
|---|---------|--------|---------|
| 33 | `EventAppend` | `run: Option<RunId>, stream: String, payload: Value` | `Version(u64)` |
| 34 | `EventAppendBatch` | `run: Option<RunId>, events: Vec<(String, Value)>` | `Versions(Vec<u64>)` |
| 35 | `EventRange` | `run: Option<RunId>, stream: String, start: Option<u64>, end: Option<u64>, limit: Option<u64>` | `VersionedValues(Vec<VersionedValue>)` |
| 36 | `EventRead` | `run: Option<RunId>, stream: String, sequence: u64` | `MaybeVersioned(Option<VersionedValue>)` |
| 37 | `EventLen` | `run: Option<RunId>, stream: String` | `Uint(u64)` |
| 38 | `EventLatestSequence` | `run: Option<RunId>, stream: String` | `MaybeVersion(Option<u64>)` |
| 39 | `EventStreamInfo` | `run: Option<RunId>, stream: String` | `StreamInfo(StreamInfo)` |
| 40 | `EventRevRange` | `run: Option<RunId>, stream: String, start: Option<u64>, end: Option<u64>, limit: Option<u64>` | `VersionedValues(Vec<VersionedValue>)` |
| 41 | `EventStreams` | `run: Option<RunId>` | `Strings(Vec<String>)` |
| 42 | `EventHead` | `run: Option<RunId>, stream: String` | `MaybeVersioned(Option<VersionedValue>)` |
| 43 | `EventVerifyChain` | `run: Option<RunId>` | `ChainVerification(ChainVerificationResult)` |

#### State Commands (8)

| # | Command | Fields | Returns |
|---|---------|--------|---------|
| 44 | `StateSet` | `run: Option<RunId>, cell: String, value: Value` | `Version(u64)` |
| 45 | `StateRead` | `run: Option<RunId>, cell: String` | `MaybeVersioned(Option<VersionedValue>)` |
| 46 | `StateCas` | `run: Option<RunId>, cell: String, expected_counter: Option<u64>, value: Value` | `MaybeVersion(Option<u64>)` |
| 47 | `StateDelete` | `run: Option<RunId>, cell: String` | `Bool(bool)` |
| 48 | `StateExists` | `run: Option<RunId>, cell: String` | `Bool(bool)` |
| 49 | `StateHistory` | `run: Option<RunId>, cell: String, limit: Option<u64>, before: Option<u64>` | `VersionedValues(Vec<VersionedValue>)` |
| 50 | `StateInit` | `run: Option<RunId>, cell: String, value: Value` | `Version(u64)` |
| 51 | `StateList` | `run: Option<RunId>` | `Strings(Vec<String>)` |

#### Vector Commands (19)

| # | Command | Fields | Returns |
|---|---------|--------|---------|
| 52 | `VectorUpsert` | `run: Option<RunId>, collection: String, key: String, vector: Vec<f32>, metadata: Option<Value>` | `Version(u64)` |
| 53 | `VectorGet` | `run: Option<RunId>, collection: String, key: String` | `VectorData(Option<VersionedVectorData>)` |
| 54 | `VectorDelete` | `run: Option<RunId>, collection: String, key: String` | `Bool(bool)` |
| 55 | `VectorSearch` | `run: Option<RunId>, collection: String, query: Vec<f32>, k: u64, filter: Option<Vec<MetadataFilter>>, metric: Option<DistanceMetric>` | `VectorMatches(Vec<VectorMatch>)` |
| 56 | `VectorGetCollection` | `run: Option<RunId>, collection: String` | `VectorCollectionInfo(Option<CollectionInfo>)` |
| 57 | `VectorCreateCollection` | `run: Option<RunId>, collection: String, dimension: u64, metric: DistanceMetric` | `Version(u64)` |
| 58 | `VectorDeleteCollection` | `run: Option<RunId>, collection: String` | `Bool(bool)` |
| 59 | `VectorListCollections` | `run: Option<RunId>` | `VectorCollectionList(Vec<CollectionInfo>)` |
| 60 | `VectorCollectionExists` | `run: Option<RunId>, collection: String` | `Bool(bool)` |
| 61 | `VectorCount` | `run: Option<RunId>, collection: String` | `Uint(u64)` |
| 62 | `VectorUpsertBatch` | `run: Option<RunId>, collection: String, vectors: Vec<VectorEntry>` | `VectorBatchResult(Vec<VectorBatchEntry>)` |
| 63 | `VectorGetBatch` | `run: Option<RunId>, collection: String, keys: Vec<String>` | `VectorDataList(Vec<Option<VersionedVectorData>>)` |
| 64 | `VectorDeleteBatch` | `run: Option<RunId>, collection: String, keys: Vec<String>` | `Bools(Vec<bool>)` |
| 65 | `VectorHistory` | `run: Option<RunId>, collection: String, key: String, limit: Option<u64>, before_version: Option<u64>` | `VectorDataHistory(Vec<VersionedVectorData>)` |
| 66 | `VectorGetAt` | `run: Option<RunId>, collection: String, key: String, version: u64` | `VectorData(Option<VersionedVectorData>)` |
| 67 | `VectorListKeys` | `run: Option<RunId>, collection: String, limit: Option<u64>, cursor: Option<String>` | `Keys(Vec<String>)` |
| 68 | `VectorScan` | `run: Option<RunId>, collection: String, limit: Option<u64>, cursor: Option<String>` | `VectorKeyValues(Vec<(String, VectorData)>)` |

#### Run Commands (24)

| # | Command | Fields | Returns |
|---|---------|--------|---------|
| 69 | `RunCreate` | `run_id: Option<String>, metadata: Option<Value>` | `RunWithVersion { info, version }` |
| 70 | `RunGet` | `run: RunId` | `RunInfoVersioned(VersionedRunInfo)` |
| 71 | `RunList` | `state: Option<RunStatus>, limit: Option<u64>, offset: Option<u64>` | `RunInfoList(Vec<VersionedRunInfo>)` |
| 72 | `RunComplete` | `run: RunId` | `Version(u64)` |
| 73 | `RunUpdateMetadata` | `run: RunId, metadata: Value` | `Version(u64)` |
| 74 | `RunExists` | `run: RunId` | `Bool(bool)` |
| 75 | `RunPause` | `run: RunId` | `Version(u64)` |
| 76 | `RunResume` | `run: RunId` | `Version(u64)` |
| 77 | `RunFail` | `run: RunId, error: String` | `Version(u64)` |
| 78 | `RunCancel` | `run: RunId` | `Version(u64)` |
| 79 | `RunArchive` | `run: RunId` | `Version(u64)` |
| 80 | `RunDelete` | `run: RunId` | `Unit` |
| 81 | `RunQueryByStatus` | `state: RunStatus` | `RunInfoList(Vec<VersionedRunInfo>)` |
| 82 | `RunQueryByTag` | `tag: String` | `RunInfoList(Vec<VersionedRunInfo>)` |
| 83 | `RunCount` | `status: Option<RunStatus>` | `Uint(u64)` |
| 84 | `RunSearch` | `query: String, limit: Option<u64>` | `RunInfoList(Vec<VersionedRunInfo>)` |
| 85 | `RunAddTags` | `run: RunId, tags: Vec<String>` | `Version(u64)` |
| 86 | `RunRemoveTags` | `run: RunId, tags: Vec<String>` | `Version(u64)` |
| 87 | `RunGetTags` | `run: RunId` | `Strings(Vec<String>)` |
| 88 | `RunCreateChild` | `parent: RunId, metadata: Option<Value>` | `RunWithVersion { info, version }` |
| 89 | `RunGetChildren` | `parent: RunId` | `RunInfoList(Vec<VersionedRunInfo>)` |
| 90 | `RunGetParent` | `run: RunId` | `MaybeRunId(Option<RunId>)` |
| 91 | `RunSetRetention` | `run: RunId, policy: RetentionPolicyInfo` | `Version(u64)` |
| 92 | `RunGetRetention` | `run: RunId` | `RetentionPolicy(RetentionPolicyInfo)` |

#### Transaction Commands (5) — NOT IMPLEMENTED

| # | Command | Fields | Returns |
|---|---------|--------|---------|
| 93 | `TxnBegin` | `options: Option<TxnOptions>` | `TxnId(String)` |
| 94 | `TxnCommit` | _(none)_ | `Version(u64)` |
| 95 | `TxnRollback` | _(none)_ | `Unit` |
| 96 | `TxnInfo` | _(none)_ | `TxnInfo(Option<TransactionInfo>)` |
| 97 | `TxnIsActive` | _(none)_ | `Bool(bool)` |

> Deferred: executor is stateless by design; transactions require session state.

#### Retention Commands (3) — NOT IMPLEMENTED

| # | Command | Fields | Returns |
|---|---------|--------|---------|
| 98 | `RetentionApply` | `run: Option<RunId>` | — |
| 99 | `RetentionStats` | `run: Option<RunId>` | — |
| 100 | `RetentionPreview` | `run: Option<RunId>` | — |

> Deferred: requires GC infrastructure not yet built.

#### Database Commands (4)

| # | Command | Fields | Returns | Status |
|---|---------|--------|---------|--------|
| 101 | `Ping` | _(none)_ | `Pong { version }` | Implemented |
| 102 | `Info` | _(none)_ | `DatabaseInfo(DatabaseInfo)` | Stub |
| 103 | `Flush` | _(none)_ | `Unit` | Stub (no-op) |
| 104 | `Compact` | _(none)_ | `Unit` | Stub (no-op) |

#### Implementation Summary

```
Total command variants:  106
  Fully implemented:      93
  Disabled (KvMput):       1
  Stub (Info/Flush/Compact): 3  (commands exist, implementation incomplete)
  Not implemented:         9  (5 Transaction + 3 Retention + KvMput)
```

---

### 2.2 Output Enum (40 variants)

#### Primitive Results
| Variant | Type | Used By |
|---------|------|---------|
| `Unit` | — | RunDelete, Flush, Compact, TxnRollback |
| `Value(Value)` | single value | _(reserved)_ |
| `Versioned(VersionedValue)` | value + version | KvGetAt |
| `Maybe(Option<Value>)` | optional value | JsonArrayPop |
| `MaybeVersioned(Option<VersionedValue>)` | optional value + version | KvGet, JsonGet, StateGet, EventGet, EventHead |
| `MaybeVersion(Option<u64>)` | optional version | StateCas, JsonGetVersion, EventLatestSequence |
| `Version(u64)` | version number | KvPut, JsonSet, StateSet, all Run mutations |
| `Bool(bool)` | boolean | KvDelete, KvExists, KvCas*, StateDelete, StateExists, VectorDelete, RunExists |
| `Int(i64)` | signed integer | KvIncr |
| `Uint(u64)` | unsigned integer | JsonCount, JsonDelete, JsonArrayPush, EventLen, VectorCount, RunCount, KvMdelete, KvMexists |
| `Float(f64)` | float | JsonIncrement |

#### Collection Results
| Variant | Type | Used By |
|---------|------|---------|
| `Values(Vec<Option<VersionedValue>>)` | batch get | KvMget, JsonBatchGet |
| `VersionedValues(Vec<VersionedValue>)` | history/range | KvHistory, JsonHistory, StateHistory, EventRange, EventRevRange |
| `Versions(Vec<u64>)` | batch versions | EventAppendBatch, JsonBatchCreate |
| `Keys(Vec<String>)` | key lists | KvKeys, JsonQuery, VectorListKeys |
| `Strings(Vec<String>)` | string lists | EventStreams, StateList, RunGetTags |
| `Bools(Vec<bool>)` | batch booleans | VectorDeleteBatch |

#### Paginated Results
| Variant | Type | Used By |
|---------|------|---------|
| `KvScanResult` | `{ entries, cursor }` | KvScan |
| `JsonListResult` | `{ keys, cursor }` | JsonList |

#### Search Results
| Variant | Type | Used By |
|---------|------|---------|
| `JsonSearchHits(Vec<JsonSearchHit>)` | search results | JsonSearch |
| `VectorMatches(Vec<VectorMatch>)` | similarity results | VectorSearch |
| `VectorMatchesWithExhausted` | `{ matches, exhausted }` | _(reserved for budget search)_ |

#### Vector-Specific
| Variant | Type | Used By |
|---------|------|---------|
| `VectorData(Option<VersionedVectorData>)` | single vector | VectorGet, VectorGetAt |
| `VectorDataList(Vec<Option<VersionedVectorData>>)` | batch vectors | VectorGetBatch |
| `VectorDataHistory(Vec<VersionedVectorData>)` | vector history | VectorHistory |
| `VectorKeyValues(Vec<(String, VectorData)>)` | scan results | VectorScan |
| `VectorBatchResult(Vec<VectorBatchEntry>)` | batch upsert | VectorUpsertBatch |
| `VectorCollectionInfo(Option<CollectionInfo>)` | collection meta | VectorCollectionInfo |
| `VectorCollectionList(Vec<CollectionInfo>)` | all collections | VectorListCollections |

#### Event-Specific
| Variant | Type | Used By |
|---------|------|---------|
| `StreamInfo(StreamInfo)` | stream metadata | EventStreamInfo |
| `ChainVerification(ChainVerificationResult)` | integrity check | EventVerifyChain |

#### Run-Specific
| Variant | Type | Used By |
|---------|------|---------|
| `RunInfo(RunInfo)` | unversioned info | _(reserved)_ |
| `RunInfoVersioned(VersionedRunInfo)` | versioned info | RunGet |
| `RunInfoList(Vec<VersionedRunInfo>)` | run list | RunList, RunQueryByStatus, RunQueryByTag, RunSearch, RunGetChildren |
| `RunWithVersion { info, version }` | creation result | RunCreate, RunCreateChild |
| `MaybeRunId(Option<RunId>)` | parent lookup | RunGetParent |

#### Transaction/Retention/Database
| Variant | Type | Used By |
|---------|------|---------|
| `TxnId(String)` | transaction ID | TxnBegin |
| `TxnInfo(Option<TransactionInfo>)` | transaction state | TxnInfo |
| `RetentionVersion(Option<RetentionVersionInfo>)` | retention version | _(reserved)_ |
| `RetentionPolicy(RetentionPolicyInfo)` | retention policy | RunGetRetention |
| `DatabaseInfo(DatabaseInfo)` | database metadata | Info |
| `Pong { version }` | connectivity | Ping |

---

### 2.3 Error Enum (22 variants)

| Category | Variant | Fields |
|----------|---------|--------|
| **Not Found** | `KeyNotFound` | `key: String` |
| | `RunNotFound` | `run: String` |
| | `CollectionNotFound` | `collection: String` |
| | `StreamNotFound` | `stream: String` |
| | `CellNotFound` | `cell: String` |
| | `DocumentNotFound` | `key: String` |
| **Type** | `WrongType` | `expected: String, actual: String` |
| **Validation** | `InvalidKey` | `reason: String` |
| | `InvalidPath` | `reason: String` |
| | `InvalidInput` | `reason: String` |
| **Concurrency** | `VersionConflict` | `expected: u64, actual: u64` |
| | `TransitionFailed` | `expected: String, actual: String` |
| | `Conflict` | `reason: String` |
| **State** | `RunClosed` | `run: String` |
| | `RunExists` | `run: String` |
| | `CollectionExists` | `collection: String` |
| **Constraint** | `DimensionMismatch` | `expected: usize, actual: usize` |
| | `ConstraintViolation` | `reason: String` |
| | `HistoryTrimmed` | `requested: u64, earliest: u64` |
| | `Overflow` | `reason: String` |
| **Transaction** | `TransactionNotActive` | _(none)_ |
| | `TransactionAlreadyActive` | _(none)_ |
| **System** | `Io` | `reason: String` |
| | `Serialization` | `reason: String` |
| | `Internal` | `reason: String` |

---

## 3. Engine Primitive APIs

### 3.1 Database

```rust
// Lifecycle
Database::builder() -> DatabaseBuilder
Database::open(path) -> Result<Self>
Database::open_with_mode(path, DurabilityMode) -> Result<Self>
Database::ephemeral() -> Result<Self>
Database::shutdown(&self) -> Result<()>
Database::is_open(&self) -> bool
Database::is_ephemeral(&self) -> bool

// Storage access
Database::storage(&self) -> &Arc<ShardedStore>
Database::extension<T>(&self) -> Arc<T>
Database::flush(&self) -> Result<()>

// Transaction API
Database::transaction<F, T>(&self, run_id: RunId, f: F) -> Result<T>
  where F: FnOnce(&mut TransactionContext) -> Result<T>
```

```rust
// DatabaseBuilder
DatabaseBuilder::new() -> Self
DatabaseBuilder::path<P>(self, P) -> Self
DatabaseBuilder::no_durability(self) -> Self
DatabaseBuilder::buffered(self) -> Self
DatabaseBuilder::strict(self) -> Self
DatabaseBuilder::get_durability(&self) -> DurabilityMode
DatabaseBuilder::get_path(&self) -> Option<&PathBuf>
DatabaseBuilder::open(self) -> Result<Database>
DatabaseBuilder::open_temp(self) -> Result<Database>
```

### 3.2 KVStore

```rust
// Constructor
KVStore::new(db: Arc<Database>) -> Self
KVStore::database(&self) -> &Arc<Database>

// Single-key reads
fn get(&self, run_id: &RunId, key: &str) -> Result<Option<Versioned<Value>>>
fn get_at(&self, run_id: &RunId, key: &str, max_version: u64) -> Result<Option<Versioned<Value>>>
fn exists(&self, run_id: &RunId, key: &str) -> Result<bool>
fn contains(&self, run_id: &RunId, key: &str) -> Result<bool>            // alias for exists
fn history(&self, run_id: &RunId, key: &str, limit: Option<usize>, before_version: Option<u64>) -> Result<Vec<Versioned<Value>>>

// Single-key writes
fn put(&self, run_id: &RunId, key: &str, value: Value) -> Result<Version>
fn put_with_ttl(&self, run_id: &RunId, key: &str, value: Value, ttl: Duration) -> Result<Version>
fn delete(&self, run_id: &RunId, key: &str) -> Result<bool>

// Batch reads
fn get_many(&self, run_id: &RunId, keys: &[&str]) -> Result<Vec<Option<Versioned<Value>>>>
fn get_many_map(&self, run_id: &RunId, keys: &[&str]) -> Result<HashMap<String, Versioned<Value>>>

// List/scan
fn list(&self, run_id: &RunId, prefix: Option<&str>) -> Result<Vec<String>>
fn list_with_values(&self, run_id: &RunId, prefix: Option<&str>) -> Result<Vec<(String, Versioned<Value>)>>
fn keys(&self, run_id: &RunId, prefix: Option<&str>, limit: Option<usize>) -> Result<Vec<String>>
fn scan(&self, run_id: &RunId, prefix: &str, limit: usize, cursor: Option<&str>) -> Result<ScanResult>

// Search
fn search(&self, req: &SearchRequest) -> Result<SearchResponse>

// Explicit transactions
fn transaction<F, T>(&self, run_id: &RunId, f: F) -> Result<T>
  where F: FnOnce(&mut KVTransaction) -> Result<T>

// Deprecated
fn get_value(&self, run_id: &RunId, key: &str) -> Result<Option<Value>>   // use get()
fn get_in_transaction(&self, run_id: &RunId, key: &str) -> Result<Option<Versioned<Value>>>
fn put_no_version(&self, run_id: &RunId, key: &str, value: Value) -> Result<()>  // use put()
```

**KVTransaction** (used inside `KVStore::transaction`):
```rust
fn get(&mut self, key: &str) -> Result<Option<Value>>
fn put(&mut self, key: &str, value: Value) -> Result<()>
fn delete(&mut self, key: &str) -> Result<bool>
fn list(&mut self, prefix: Option<&str>) -> Result<Vec<String>>
```

### 3.3 JsonStore

```rust
// Constructor
JsonStore::new(db: Arc<Database>) -> Self
JsonStore::database(&self) -> &Arc<Database>

// Serialization helpers
JsonStore::serialize_doc(doc: &JsonDoc) -> Result<Value>
JsonStore::deserialize_doc(value: &Value) -> Result<JsonDoc>

// Document CRUD
fn create(&self, run_id: &RunId, doc_id: &str, value: JsonValue) -> Result<Version>
fn set(&self, run_id: &RunId, doc_id: &str, path: &JsonPath, value: JsonValue) -> Result<Version>
fn get(&self, run_id: &RunId, doc_id: &str, path: &JsonPath) -> Result<Option<Versioned<JsonValue>>>
fn get_doc(&self, run_id: &RunId, doc_id: &str) -> Result<Option<Versioned<JsonDoc>>>
fn get_version(&self, run_id: &RunId, doc_id: &str) -> Result<Option<u64>>
fn exists(&self, run_id: &RunId, doc_id: &str) -> Result<bool>
fn delete_at_path(&self, run_id: &RunId, doc_id: &str, path: &JsonPath) -> Result<Version>
fn destroy(&self, run_id: &RunId, doc_id: &str) -> Result<bool>
fn merge(&self, run_id: &RunId, doc_id: &str, path: &JsonPath, patch: JsonValue) -> Result<Version>
fn cas(&self, run_id: &RunId, doc_id: &str, expected_version: u64, path: &JsonPath, value: JsonValue) -> Result<Version>
fn history(&self, run_id: &RunId, doc_id: &str, limit: Option<usize>, before_version: Option<u64>) -> Result<Vec<Versioned<JsonDoc>>>

// List/count
fn list(&self, run_id: &RunId, prefix: Option<&str>, cursor: Option<&str>, limit: usize) -> Result<JsonListResult>
fn count(&self, run_id: &RunId) -> Result<u64>

// Batch
fn batch_get<S: AsRef<str>>(&self, run_id: &RunId, doc_ids: &[S]) -> Result<Vec<Option<Versioned<JsonDoc>>>>
fn batch_create<S: AsRef<str> + Clone>(&self, run_id: &RunId, docs: Vec<(S, JsonValue)>) -> Result<Vec<Version>>

// Array/numeric operations
fn array_push(&self, run_id: &RunId, doc_id: &str, path: &JsonPath, values: Vec<JsonValue>) -> Result<(Version, usize)>
fn array_pop(&self, run_id: &RunId, doc_id: &str, path: &JsonPath) -> Result<(Version, Option<JsonValue>)>
fn increment(&self, run_id: &RunId, doc_id: &str, path: &JsonPath, delta: f64) -> Result<(Version, f64)>

// Query/search
fn query(&self, run_id: &RunId, path: &JsonPath, value: &JsonValue, limit: usize) -> Result<Vec<String>>
fn search(&self, req: &SearchRequest) -> Result<SearchResponse>
```

### 3.4 EventLog

```rust
// Constructor
EventLog::new(db: Arc<Database>) -> Self
EventLog::database(&self) -> &Arc<Database>

// Append
fn append(&self, run_id: &RunId, event_type: &str, payload: Value) -> Result<Version>
fn append_batch(&self, run_id: &RunId, events: &[(&str, Value)]) -> Result<Vec<Version>>

// Single-event reads
fn read(&self, run_id: &RunId, sequence: u64) -> Result<Option<Versioned<Event>>>
fn read_in_transaction(&self, run_id: &RunId, sequence: u64) -> Result<Option<Versioned<Event>>>

// Range reads
fn read_range(&self, run_id: &RunId, start: u64, end: u64) -> Result<Vec<Versioned<Event>>>
fn read_range_reverse(&self, run_id: &RunId, start: u64, end: u64) -> Result<Vec<Versioned<Event>>>

// Global metadata
fn head(&self, run_id: &RunId) -> Result<Option<Versioned<Event>>>
fn len(&self, run_id: &RunId) -> Result<u64>
fn is_empty(&self, run_id: &RunId) -> Result<bool>

// Chain verification
fn verify_chain(&self, run_id: &RunId) -> Result<ChainVerification>

// Per-stream (by event_type)
fn len_by_type(&self, run_id: &RunId, event_type: &str) -> Result<u64>
fn latest_sequence_by_type(&self, run_id: &RunId, event_type: &str) -> Result<Option<u64>>
fn stream_info(&self, run_id: &RunId, event_type: &str) -> Result<Option<StreamMeta>>
fn head_by_type(&self, run_id: &RunId, event_type: &str) -> Result<Option<Versioned<Event>>>
fn stream_names(&self, run_id: &RunId) -> Result<Vec<String>>

// Query
fn read_by_type(&self, run_id: &RunId, event_type: &str) -> Result<Vec<Versioned<Event>>>
fn event_types(&self, run_id: &RunId) -> Result<Vec<String>>

// Search
fn search(&self, req: &SearchRequest) -> Result<SearchResponse>
```

### 3.5 StateCell

```rust
// Constructor
StateCell::new(db: Arc<Database>) -> Self
StateCell::database(&self) -> &Arc<Database>

// CRUD
fn init(&self, run_id: &RunId, name: &str, value: Value) -> Result<Versioned<Version>>
fn set(&self, run_id: &RunId, name: &str, value: Value) -> Result<Versioned<Version>>
fn read(&self, run_id: &RunId, name: &str) -> Result<Option<Versioned<State>>>
fn read_in_transaction(&self, run_id: &RunId, name: &str) -> Result<Option<Versioned<State>>>
fn delete(&self, run_id: &RunId, name: &str) -> Result<bool>
fn exists(&self, run_id: &RunId, name: &str) -> Result<bool>
fn list(&self, run_id: &RunId) -> Result<Vec<String>>
fn history(&self, run_id: &RunId, name: &str, limit: Option<usize>, before_counter: Option<u64>) -> Result<Vec<Versioned<Value>>>

// CAS
fn cas(&self, run_id: &RunId, name: &str, expected_version: Version, new_value: Value) -> Result<Versioned<Version>>

// Closure-based transitions (not serializable)
fn transition<F, T>(&self, run_id: &RunId, name: &str, f: F) -> Result<(T, Versioned<Version>)>
  where F: Fn(&State) -> Result<(Value, T)>
fn transition_or_init<F, T>(&self, run_id: &RunId, name: &str, initial: Value, f: F) -> Result<(T, Versioned<Version>)>
  where F: Fn(&State) -> Result<(Value, T)>

// Search
fn search(&self, req: &SearchRequest) -> Result<SearchResponse>
```

### 3.6 VectorStore

```rust
// Constructor
VectorStore::new(db: Arc<Database>) -> Self
VectorStore::database(&self) -> &Arc<Database>

// Recovery
fn recover(&self) -> VectorResult<RecoveryStats>

// Collection management
fn create_collection(&self, run_id: RunId, name: &str, config: VectorConfig) -> VectorResult<Versioned<CollectionInfo>>
fn delete_collection(&self, run_id: RunId, name: &str) -> VectorResult<()>
fn list_collections(&self, run_id: RunId) -> VectorResult<Vec<CollectionInfo>>
fn get_collection(&self, run_id: RunId, name: &str) -> VectorResult<Option<Versioned<CollectionInfo>>>
fn collection_exists(&self, run_id: RunId, name: &str) -> VectorResult<bool>

// Single-vector CRUD
fn insert(&self, run_id: RunId, collection: &str, key: &str, embedding: &[f32]) -> VectorResult<Versioned<VectorRecord>>
fn insert_with_source(&self, run_id: RunId, collection: &str, key: &str, embedding: &[f32], source_ref: Option<EntityRef>) -> VectorResult<Versioned<VectorRecord>>
fn get(&self, run_id: RunId, collection: &str, key: &str) -> VectorResult<Option<Versioned<VectorRecord>>>
fn get_at(&self, run_id: RunId, collection: &str, key: &str, max_version: u64) -> VectorResult<Option<Versioned<VectorRecord>>>
fn delete(&self, run_id: RunId, collection: &str, key: &str) -> VectorResult<bool>
fn count(&self, run_id: RunId, collection: &str) -> VectorResult<usize>
fn history(&self, run_id: RunId, collection: &str, key: &str, limit: Option<usize>, before_version: Option<u64>) -> VectorResult<Vec<Versioned<VectorRecord>>>

// List/scan
fn list_keys(&self, run_id: RunId, collection: &str, limit: Option<usize>, cursor: Option<&str>) -> VectorResult<Vec<String>>
fn scan(&self, run_id: RunId, collection: &str, limit: usize, cursor: Option<&str>) -> VectorResult<ScanResult<VectorRecord>>

// Search
fn search(&self, run_id: RunId, collection: &str, query: &[f32], k: usize, filter: Option<MetadataFilter>) -> VectorResult<Vec<VectorMatch>>
fn search_simple(&self, run_id: RunId, collection: &str, query: &[f32], k: usize) -> VectorResult<Vec<VectorMatch>>
fn search_with_sources(&self, run_id: RunId, collection: &str, query: &[f32], k: usize, filter: Option<MetadataFilter>) -> VectorResult<Vec<(VectorMatch, Option<EntityRef>)>>
fn search_response(&self, run_id: RunId, collection: &str, query: &[f32], k: usize, budget: SearchBudget) -> VectorResult<SearchResponse>
fn search_with_budget(&self, run_id: RunId, collection: &str, query: &[f32], k: usize, filter: Option<MetadataFilter>, budget: SearchBudget) -> VectorResult<SearchResponse>

// Batch operations
fn insert_batch(&self, run_id: RunId, collection: &str, vectors: Vec<(String, Vec<f32>)>) -> VectorResult<Vec<Versioned<VectorRecord>>>
fn insert_batch_with_source(&self, run_id: RunId, collection: &str, vectors: Vec<(String, Vec<f32>, Option<EntityRef>)>) -> VectorResult<Vec<Versioned<VectorRecord>>>
fn get_batch(&self, run_id: RunId, collection: &str, keys: &[&str]) -> VectorResult<Vec<Option<Versioned<VectorRecord>>>>
fn delete_batch(&self, run_id: RunId, collection: &str, keys: &[&str]) -> VectorResult<usize>

// Metadata access
fn get_key_and_metadata(&self, run_id: RunId, collection: &str, key: &str) -> VectorResult<Option<(String, Option<JsonValue>)>>
fn get_key_metadata_source(&self, run_id: RunId, collection: &str, key: &str) -> VectorResult<Option<(String, Option<JsonValue>, Option<EntityRef>)>>

// Internal/WAL replay (not user-facing)
fn ensure_collection_loaded(&self, run_id: RunId, name: &str) -> VectorResult<()>
fn replay_create_collection(&self, run_id: RunId, name: &str, config: VectorConfig) -> VectorResult<()>
fn replay_delete_collection(&self, run_id: RunId, name: &str) -> VectorResult<()>
fn replay_upsert(&self, ...) -> VectorResult<()>
fn replay_delete(&self, ...) -> VectorResult<()>
fn backends(&self) -> Arc<VectorBackendState>
fn db(&self) -> &Database
```

### 3.7 RunIndex

```rust
// Constructor
RunIndex::new(db: Arc<Database>) -> Self
RunIndex::database(&self) -> &Arc<Database>

// Create/get
fn create_run(&self, run_id: &str) -> Result<Versioned<RunMetadata>>
fn create_run_with_options(&self, run_id: &str, parent_run: Option<String>, tags: Vec<String>, metadata: Value) -> Result<Versioned<RunMetadata>>
fn get_run(&self, run_id: &str) -> Result<Option<Versioned<RunMetadata>>>
fn exists(&self, run_id: &str) -> Result<bool>
fn list_runs(&self) -> Result<Vec<String>>
fn count(&self) -> Result<usize>

// Status transitions
fn update_status(&self, run_id: &str, new_status: RunStatus) -> Result<Versioned<RunMetadata>>
fn complete_run(&self, run_id: &str) -> Result<Versioned<RunMetadata>>
fn fail_run(&self, run_id: &str, error: &str) -> Result<Versioned<RunMetadata>>
fn pause_run(&self, run_id: &str) -> Result<Versioned<RunMetadata>>
fn resume_run(&self, run_id: &str) -> Result<Versioned<RunMetadata>>
fn cancel_run(&self, run_id: &str) -> Result<Versioned<RunMetadata>>
fn archive_run(&self, run_id: &str) -> Result<Versioned<RunMetadata>>
fn delete_run(&self, run_id: &str) -> Result<()>

// Queries
fn query_by_status(&self, status: RunStatus) -> Result<Vec<RunMetadata>>
fn query_by_tag(&self, tag: &str) -> Result<Vec<RunMetadata>>
fn get_child_runs(&self, parent_id: &str) -> Result<Vec<RunMetadata>>

// Metadata updates
fn add_tags(&self, run_id: &str, new_tags: Vec<String>) -> Result<Versioned<RunMetadata>>
fn remove_tags(&self, run_id: &str, tags_to_remove: Vec<String>) -> Result<Versioned<RunMetadata>>
fn update_metadata(&self, run_id: &str, metadata: Value) -> Result<Versioned<RunMetadata>>

// Search
fn search(&self, req: &SearchRequest) -> Result<SearchResponse>

// Import/export
fn export_run(&self, run_id: &str, path: &Path) -> RunBundleResult<RunExportInfo>
fn export_run_with_options(&self, run_id: &str, path: &Path, options: &ExportOptions) -> RunBundleResult<RunExportInfo>
fn import_run(&self, path: &Path) -> RunBundleResult<ImportedRunInfo>
fn verify_bundle(&self, path: &Path) -> RunBundleResult<BundleVerifyInfo>
```

### 3.8 Transaction Extension Traits

These traits are implemented on `TransactionContext` (obtained from `Database::transaction()`):

```rust
// KVStoreExt
fn kv_get(&mut self, key: &str) -> Result<Option<Value>>
fn kv_put(&mut self, key: &str, value: Value) -> Result<()>
fn kv_delete(&mut self, key: &str) -> Result<()>

// EventLogExt
fn event_append(&mut self, event_type: &str, payload: Value) -> Result<u64>
fn event_read(&mut self, sequence: u64) -> Result<Option<Value>>

// StateCellExt
fn state_read(&mut self, name: &str) -> Result<Option<Value>>
fn state_cas(&mut self, name: &str, expected_version: Version, new_value: Value) -> Result<Version>
fn state_set(&mut self, name: &str, value: Value) -> Result<Version>

// JsonStoreExt
fn json_get(&mut self, doc_id: &str, path: &JsonPath) -> Result<Option<JsonValue>>
fn json_set(&mut self, doc_id: &str, path: &JsonPath, value: JsonValue) -> Result<Version>
fn json_create(&mut self, doc_id: &str, value: JsonValue) -> Result<Version>

// VectorStoreExt
fn vector_get(&mut self, collection: &str, key: &str) -> Result<Option<Vec<f32>>>
fn vector_insert(&mut self, collection: &str, key: &str, embedding: &[f32]) -> Result<Version>
```

---

## 4. Bidirectional Mapping

### 4.1 Executor → Engine (Forward)

How each executor command maps to engine method calls.

#### KV

| Executor Command | Engine Call | Notes |
|-----------------|------------|-------|
| `KvPut` | `KVStore::put()` | |
| `KvGet` | `KVStore::get()` | |
| `KvGetAt` | `KVStore::get_at()` | |
| `KvDelete` | `KVStore::delete()` | |
| `KvExists` | `KVStore::exists()` | |
| `KvHistory` | `KVStore::history()` | |
| `KvIncr` | `Database::transaction()` + `KVStoreExt::{kv_get, kv_put}` | Atomic read-modify-write |
| `KvCasVersion` | `Database::transaction()` + `KVStoreExt::{kv_get, kv_put}` | Version-based CAS |
| `KvCasValue` | `Database::transaction()` + `KVStoreExt::{kv_get, kv_put}` | Value-based CAS |
| `KvKeys` | `KVStore::keys()` | |
| `KvScan` | `KVStore::scan()` | |
| `KvMget` | `KVStore::get_many()` | |
| `KvMput` | — | **DISABLED** |
| `KvMdelete` | Loop over `KVStore::delete()` | Not atomic |
| `KvMexists` | Loop over `KVStore::exists()` | Not atomic |

#### JSON

| Executor Command | Engine Call | Notes |
|-----------------|------------|-------|
| `JsonSet` | `JsonStore::set()` (auto-creates via `create()` if needed) | |
| `JsonGet` | `JsonStore::get()` | |
| `JsonDelete` | `JsonStore::delete_at_path()` | |
| `JsonMerge` | `JsonStore::merge()` | |
| `JsonHistory` | `JsonStore::history()` | |
| `JsonExists` | `JsonStore::exists()` | |
| `JsonGetVersion` | `JsonStore::get_version()` | |
| `JsonSearch` | `JsonStore::search()` | |
| `JsonList` | `JsonStore::list()` | |
| `JsonCas` | `JsonStore::cas()` | |
| `JsonQuery` | `JsonStore::query()` | |
| `JsonCount` | `JsonStore::count()` | |
| `JsonBatchGet` | `JsonStore::batch_get()` | |
| `JsonBatchCreate` | `JsonStore::batch_create()` | |
| `JsonArrayPush` | `JsonStore::array_push()` | Returns new length only (drops version) |
| `JsonIncrement` | `JsonStore::increment()` | Returns new value only (drops version) |
| `JsonArrayPop` | `JsonStore::array_pop()` | Returns popped value only (drops version) |

#### Event

| Executor Command | Engine Call | Notes |
|-----------------|------------|-------|
| `EventAppend` | `EventLog::append()` | |
| `EventAppendBatch` | `EventLog::append_batch()` | |
| `EventRange` | `EventLog::read_by_type()` or `read_range()` | Filtered to stream |
| `EventRead` | `EventLog::read()` | |
| `EventLen` | `EventLog::len_by_type()` | Per-stream, not global |
| `EventLatestSequence` | `EventLog::latest_sequence_by_type()` | |
| `EventStreamInfo` | `EventLog::stream_info()` | |
| `EventRevRange` | `EventLog::read_range_reverse()` | |
| `EventStreams` | `EventLog::stream_names()` | |
| `EventHead` | `EventLog::head_by_type()` | Per-stream head |
| `EventVerifyChain` | `EventLog::verify_chain()` | |

#### State

| Executor Command | Engine Call | Notes |
|-----------------|------------|-------|
| `StateSet` | `StateCell::set()` | |
| `StateRead` | `StateCell::read()` | |
| `StateCas` | `StateCell::cas()` or `StateCell::init()` | `None` expected → init |
| `StateDelete` | `StateCell::delete()` | |
| `StateExists` | `StateCell::exists()` | |
| `StateHistory` | `StateCell::history()` | |
| `StateInit` | `StateCell::init()` | |
| `StateList` | `StateCell::list()` | |

#### Vector

| Executor Command | Engine Call | Notes |
|-----------------|------------|-------|
| `VectorUpsert` | `VectorStore::insert()` | Auto-creates collection if needed |
| `VectorGet` | `VectorStore::get()` | |
| `VectorDelete` | `VectorStore::delete()` | |
| `VectorSearch` | `VectorStore::search()` | |
| `VectorGetCollection` | `VectorStore::get_collection()` | |
| `VectorCreateCollection` | `VectorStore::create_collection()` | |
| `VectorDeleteCollection` | `VectorStore::delete_collection()` | |
| `VectorListCollections` | `VectorStore::list_collections()` | |
| `VectorCollectionExists` | `VectorStore::collection_exists()` | |
| `VectorCount` | `VectorStore::count()` | |
| `VectorUpsertBatch` | `VectorStore::insert_batch()` | |
| `VectorGetBatch` | `VectorStore::get_batch()` | |
| `VectorDeleteBatch` | `VectorStore::delete_batch()` | |
| `VectorHistory` | `VectorStore::history()` | |
| `VectorGetAt` | `VectorStore::get_at()` | |
| `VectorListKeys` | `VectorStore::list_keys()` | |
| `VectorScan` | `VectorStore::scan()` | |

#### Run

| Executor Command | Engine Call | Notes |
|-----------------|------------|-------|
| `RunCreate` | `RunIndex::create_run_with_options()` | |
| `RunGet` | `RunIndex::get_run()` | |
| `RunList` | `RunIndex::list_runs()` + `get_run()` per ID | |
| `RunComplete` | `RunIndex::complete_run()` | |
| `RunUpdateMetadata` | `RunIndex::update_metadata()` | |
| `RunExists` | `RunIndex::exists()` | |
| `RunPause` | `RunIndex::pause_run()` | |
| `RunResume` | `RunIndex::resume_run()` | |
| `RunFail` | `RunIndex::fail_run()` | |
| `RunCancel` | `RunIndex::cancel_run()` | |
| `RunArchive` | `RunIndex::archive_run()` | |
| `RunDelete` | `RunIndex::delete_run()` | |
| `RunQueryByStatus` | `RunIndex::query_by_status()` | |
| `RunQueryByTag` | `RunIndex::query_by_tag()` | |
| `RunCount` | `RunIndex::count()` or filter by status | |
| `RunSearch` | `RunIndex::search()` | |
| `RunAddTags` | `RunIndex::add_tags()` | |
| `RunRemoveTags` | `RunIndex::remove_tags()` | |
| `RunGetTags` | `RunIndex::get_run()` → extract tags | |
| `RunCreateChild` | `RunIndex::create_run_with_options()` | With parent_run set |
| `RunGetChildren` | `RunIndex::get_child_runs()` | |
| `RunGetParent` | `RunIndex::get_run()` → extract parent | |
| `RunSetRetention` | `RunIndex::update_metadata()` | Stored in metadata |
| `RunGetRetention` | `RunIndex::get_run()` → extract from metadata | |

#### Database

| Executor Command | Engine Call | Notes |
|-----------------|------------|-------|
| `Ping` | _(synthetic)_ | Returns crate version |
| `Info` | _(stub)_ | TODO |
| `Flush` | _(stub)_ | Should call `Database::flush()` |
| `Compact` | _(stub)_ | No engine method exists |

---

### 4.2 Engine → Executor (Reverse)

Every engine method and whether it has a corresponding executor command.

#### Database

| Engine Method | Executor Command | Status |
|--------------|-----------------|--------|
| `Database::builder()` | — | Lifecycle, not a command |
| `Database::open()` | — | Lifecycle |
| `Database::open_with_mode()` | — | Lifecycle |
| `Database::ephemeral()` | — | Lifecycle |
| `Database::flush()` | `Flush` | **Stub** (no-op) |
| `Database::shutdown()` | — | **Not exposed** |
| `Database::is_open()` | — | **Not exposed** |
| `Database::is_ephemeral()` | — | Not needed |
| `Database::storage()` | — | Internal |
| `Database::extension()` | — | Internal |
| `Database::transaction()` | `TxnBegin/Commit/Rollback` | **Not implemented** |

#### KVStore

| Engine Method | Executor Command | Status |
|--------------|-----------------|--------|
| `get()` | `KvGet` | Covered |
| `get_at()` | `KvGetAt` | Covered |
| `exists()` | `KvExists` | Covered |
| `contains()` | — | Alias of `exists`, not needed |
| `history()` | `KvHistory` | Covered |
| `put()` | `KvPut` | Covered |
| `put_with_ttl()` | — | **Not exposed** |
| `delete()` | `KvDelete` | Covered |
| `get_many()` | `KvMget` | Covered |
| `get_many_map()` | — | **Not exposed** (HashMap variant) |
| `list()` | — | Superseded by `keys()` |
| `list_with_values()` | — | **Not exposed** |
| `keys()` | `KvKeys` | Covered |
| `scan()` | `KvScan` | Covered |
| `search()` | — | **Not exposed** |
| `transaction()` | _(used internally by KvIncr, KvCas*)_ | Internal |
| `get_value()` | — | Deprecated |
| `get_in_transaction()` | — | Internal |
| `put_no_version()` | — | Deprecated |

#### JsonStore

| Engine Method | Executor Command | Status |
|--------------|-----------------|--------|
| `create()` | _(used internally by JsonSet)_ | Internal |
| `set()` | `JsonSet` | Covered |
| `get()` | `JsonGet` | Covered |
| `get_doc()` | — | **Not exposed** (full JsonDoc) |
| `get_version()` | `JsonGetVersion` | Covered |
| `exists()` | `JsonExists` | Covered |
| `delete_at_path()` | `JsonDelete` | Covered |
| `destroy()` | — | **Not exposed** (delete entire document) |
| `merge()` | `JsonMerge` | Covered |
| `cas()` | `JsonCas` | Covered |
| `history()` | `JsonHistory` | Covered |
| `list()` | `JsonList` | Covered |
| `count()` | `JsonCount` | Covered |
| `batch_get()` | `JsonBatchGet` | Covered |
| `batch_create()` | `JsonBatchCreate` | Covered |
| `array_push()` | `JsonArrayPush` | Covered |
| `array_pop()` | `JsonArrayPop` | Covered |
| `increment()` | `JsonIncrement` | Covered |
| `query()` | `JsonQuery` | Covered |
| `search()` | `JsonSearch` | Covered |

#### EventLog

| Engine Method | Executor Command | Status |
|--------------|-----------------|--------|
| `append()` | `EventAppend` | Covered |
| `append_batch()` | `EventAppendBatch` | Covered |
| `read()` | `EventRead` | Covered |
| `read_in_transaction()` | — | Internal |
| `read_range()` | `EventRange` | Covered |
| `read_range_reverse()` | `EventRevRange` | Covered |
| `head()` | — | **Not exposed** (global head, not per-stream) |
| `len()` | — | **Not exposed** (global len, not per-stream) |
| `is_empty()` | — | **Not exposed** |
| `verify_chain()` | `EventVerifyChain` | Covered |
| `len_by_type()` | `EventLen` | Covered |
| `latest_sequence_by_type()` | `EventLatestSequence` | Covered |
| `stream_info()` | `EventStreamInfo` | Covered |
| `head_by_type()` | `EventHead` | Covered |
| `stream_names()` | `EventStreams` | Covered |
| `read_by_type()` | _(used internally by EventRange)_ | Internal |
| `event_types()` | — | Alias for `stream_names`, not needed |
| `search()` | — | **Not exposed** |

#### StateCell

| Engine Method | Executor Command | Status |
|--------------|-----------------|--------|
| `init()` | `StateInit` | Covered |
| `set()` | `StateSet` | Covered |
| `read()` | `StateRead` | Covered |
| `read_in_transaction()` | — | Internal |
| `delete()` | `StateDelete` | Covered |
| `exists()` | `StateExists` | Covered |
| `list()` | `StateList` | Covered |
| `history()` | `StateHistory` | Covered |
| `cas()` | `StateCas` | Covered |
| `transition()` | — | **Cannot expose** (requires closure) |
| `transition_or_init()` | — | **Cannot expose** (requires closure) |
| `search()` | — | **Not exposed** |

#### VectorStore

| Engine Method | Executor Command | Status |
|--------------|-----------------|--------|
| `create_collection()` | `VectorCreateCollection` | Covered |
| `delete_collection()` | `VectorDeleteCollection` | Covered |
| `list_collections()` | `VectorListCollections` | Covered |
| `get_collection()` | `VectorGetCollection` | Covered |
| `collection_exists()` | `VectorCollectionExists` | Covered |
| `insert()` | `VectorUpsert` | Covered |
| `insert_with_source()` | — | **Not exposed** |
| `get()` | `VectorGet` | Covered |
| `get_at()` | `VectorGetAt` | Covered |
| `delete()` | `VectorDelete` | Covered |
| `count()` | `VectorCount` | Covered |
| `history()` | `VectorHistory` | Covered |
| `list_keys()` | `VectorListKeys` | Covered |
| `scan()` | `VectorScan` | Covered |
| `search()` | `VectorSearch` | Covered |
| `search_simple()` | — | Convenience, covered by `search()` |
| `search_with_sources()` | — | **Not exposed** |
| `search_response()` | — | Internal format |
| `search_with_budget()` | — | **Not exposed** |
| `insert_batch()` | `VectorUpsertBatch` | Covered |
| `insert_batch_with_source()` | — | **Not exposed** |
| `get_batch()` | `VectorGetBatch` | Covered |
| `delete_batch()` | `VectorDeleteBatch` | Covered |
| `get_key_and_metadata()` | — | **Not exposed** |
| `get_key_metadata_source()` | — | **Not exposed** |
| `recover()` | — | Internal (WAL recovery) |
| `replay_*()` | — | Internal (WAL replay, 5 methods) |
| `ensure_collection_loaded()` | — | Internal |
| `backends()` | — | Internal |
| `db()` | — | Internal |

#### RunIndex

| Engine Method | Executor Command | Status |
|--------------|-----------------|--------|
| `create_run()` | _(used internally)_ | Internal |
| `create_run_with_options()` | `RunCreate` | Covered |
| `get_run()` | `RunGet` | Covered |
| `exists()` | `RunExists` | Covered |
| `list_runs()` | `RunList` | Covered |
| `count()` | `RunCount` | Covered |
| `update_status()` | — | Covered by specific commands |
| `complete_run()` | `RunComplete` | Covered |
| `fail_run()` | `RunFail` | Covered |
| `pause_run()` | `RunPause` | Covered |
| `resume_run()` | `RunResume` | Covered |
| `cancel_run()` | `RunCancel` | Covered |
| `archive_run()` | `RunArchive` | Covered |
| `delete_run()` | `RunDelete` | Covered |
| `query_by_status()` | `RunQueryByStatus` | Covered |
| `query_by_tag()` | `RunQueryByTag` | Covered |
| `get_child_runs()` | `RunGetChildren` | Covered |
| `add_tags()` | `RunAddTags` | Covered |
| `remove_tags()` | `RunRemoveTags` | Covered |
| `update_metadata()` | `RunUpdateMetadata` | Covered |
| `search()` | `RunSearch` | Covered |
| `export_run()` | — | **Not exposed** |
| `export_run_with_options()` | — | **Not exposed** |
| `import_run()` | — | **Not exposed** |
| `verify_bundle()` | — | **Not exposed** |

---

## 5. Gap Analysis

### Engine methods not exposed through executor

| Priority | Engine Method | Primitive | Impact |
|----------|--------------|-----------|--------|
| **High** | `JsonStore::destroy()` | JSON | Cannot delete an entire JSON document |
| **High** | `Database::flush()` | Database | `Flush` command exists but is a no-op |
| **Medium** | `KVStore::put_with_ttl()` | KV | No TTL support for keys |
| **Medium** | `KVStore::search()` | KV | No full-text search across KV entries |
| **Medium** | `EventLog::search()` | Event | No full-text search across events |
| **Medium** | `StateCell::search()` | State | No full-text search across state cells |
| **Medium** | `VectorStore::search_with_budget()` | Vector | No budget-constrained search |
| **Medium** | `RunIndex::export_run()` | Run | Cannot export runs to bundle |
| **Medium** | `RunIndex::import_run()` | Run | Cannot import runs from bundle |
| **Medium** | `RunIndex::verify_bundle()` | Run | Cannot verify bundle integrity |
| **Low** | `EventLog::head()` | Event | Global head (all streams), use `EventHead` per-stream |
| **Low** | `EventLog::len()` | Event | Global event count, use `EventLen` per-stream |
| **Low** | `EventLog::is_empty()` | Event | Check via `EventLen` |
| **Low** | `KVStore::list_with_values()` | KV | Can be composed from `KvScan` |
| **Low** | `KVStore::get_many_map()` | KV | Convenience, use `KvMget` |
| **Low** | `VectorStore::insert_with_source()` | Vector | Source tracking (EntityRef) |
| **Low** | `VectorStore::insert_batch_with_source()` | Vector | Source tracking (EntityRef) |
| **Low** | `VectorStore::search_with_sources()` | Vector | Source tracking (EntityRef) |
| **Low** | `VectorStore::get_key_and_metadata()` | Vector | Metadata-only access |
| **Low** | `VectorStore::get_key_metadata_source()` | Vector | Source tracking (EntityRef) |
| N/A | `StateCell::transition()` | State | **Cannot expose** — requires closure |
| N/A | `StateCell::transition_or_init()` | State | **Cannot expose** — requires closure |

### Executor commands with no engine backing

| Command | Status | Notes |
|---------|--------|-------|
| `KvMput` | Disabled | Needs `transaction_with_version` |
| `TxnBegin` | Not implemented | Needs session state architecture |
| `TxnCommit` | Not implemented | Same |
| `TxnRollback` | Not implemented | Same |
| `TxnInfo` | Not implemented | Same |
| `TxnIsActive` | Not implemented | Same |
| `RetentionApply` | Not implemented | Needs GC infrastructure |
| `RetentionStats` | Not implemented | Same |
| `RetentionPreview` | Not implemented | Same |
| `Ping` | Synthetic | No engine method, returns crate version |
| `Info` | Stub | Partial implementation |
| `Flush` | Stub | Should call `Database::flush()` |
| `Compact` | Stub | No engine method exists |

---

## 6. Validation Rules

The executor's bridge layer applies these validation rules before calling engine
methods. The engine does not perform these checks itself.

### Key Validation (KV and JSON keys)
- Must be non-empty
- Maximum 1024 bytes
- No NUL bytes (`\0`)
- Cannot start with `_strata/` (reserved internal prefix)

### Event Stream Names
- Must be non-empty

### Event Payloads
- Must be `Value::Object` (not scalar or array)

### Vector Collection Names
- Cannot start with `_` (reserved for internal collections)

### Run IDs
- Must be `"default"` (maps to nil UUID `[0u8; 16]`) or a valid UUID string
- UUID strings are parsed via `uuid::Uuid::parse_str()`

### Type Conversions (bridge layer)
- `Value` ↔ `serde_json::Value` for JSON and vector metadata
- `executor::DistanceMetric` ↔ `engine::DistanceMetric`
- `executor::MetadataFilter` → `engine::MetadataFilter` (struct with `equals: HashMap<String, JsonScalar>`)
- `executor::RunStatus` ↔ `engine::RunStatus`
- `Versioned<T>` → `VersionedValue` (flattens version + value)

---

## 7. Naming Consistency Audit

The executor's Command variants should mirror the engine's method names as closely as
possible. This section documents the naming alignment between executor and engine.

### Resolved Mismatches (renamed)

These 5 commands were renamed to match their engine counterparts:

| Executor Command | Engine Method | Previous Name |
|-----------------|--------------|---------------|
| `EventRead` | `EventLog::read()` | `EventGet` |
| `StateRead` | `StateCell::read()` | `StateGet` |
| `VectorDeleteCollection` | `VectorStore::delete_collection()` | `VectorDropCollection` |
| `VectorGetCollection` | `VectorStore::get_collection()` | `VectorCollectionInfo` |
| `RunComplete` | `RunIndex::complete_run()` | `RunClose` |

### Intentional Divergences

| Executor Command | Engine Method | Reason |
|-----------------|--------------|--------|
| `VectorUpsert` | `VectorStore::insert()` | Engine's `insert()` performs upsert semantics. Executor name is more honest about the behavior. |
| `VectorUpsertBatch` | `VectorStore::insert_batch()` | Consistent with `VectorUpsert`. |

### Confirmed Matches (no action needed)

All other executor commands already match their engine counterparts:

- **KV**: `KvPut`→`put()`, `KvGet`→`get()`, `KvGetAt`→`get_at()`, `KvDelete`→`delete()`,
  `KvExists`→`exists()`, `KvHistory`→`history()`, `KvKeys`→`keys()`, `KvScan`→`scan()`,
  `KvMget`→`get_many()`
- **KV (composite)**: `KvIncr`, `KvCasVersion`, `KvCasValue`, `KvMdelete`, `KvMexists` —
  no single engine method; these use transactions or loops. Names are fine.
- **JSON**: All 17 commands match their engine method names exactly.
- **Event**: `EventAppend`→`append()`, `EventAppendBatch`→`append_batch()`,
  `EventRange`→`read_by_type()`/`read_range()`, `EventLen`→`len_by_type()`,
  `EventLatestSequence`→`latest_sequence_by_type()`, `EventStreamInfo`→`stream_info()`,
  `EventRevRange`→`read_range_reverse()`, `EventStreams`→`stream_names()`,
  `EventHead`→`head_by_type()`, `EventVerifyChain`→`verify_chain()`
- **State**: `StateSet`→`set()`, `StateCas`→`cas()`, `StateDelete`→`delete()`,
  `StateExists`→`exists()`, `StateHistory`→`history()`, `StateInit`→`init()`,
  `StateList`→`list()`
- **Vector**: `VectorGet`→`get()`, `VectorDelete`→`delete()`, `VectorSearch`→`search()`,
  `VectorCreateCollection`→`create_collection()`, `VectorListCollections`→`list_collections()`,
  `VectorCollectionExists`→`collection_exists()`, `VectorCount`→`count()`,
  `VectorGetBatch`→`get_batch()`, `VectorDeleteBatch`→`delete_batch()`,
  `VectorHistory`→`history()`, `VectorGetAt`→`get_at()`, `VectorListKeys`→`list_keys()`,
  `VectorScan`→`scan()`
- **Run**: All 24 commands match their engine counterparts (except `RunComplete`, listed above).

### Completed Renames

All 5 naming mismatches have been resolved. The executor now mirrors engine method names.
